use crate::mmu::MMU;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RTC_MAGIC: &str = "DEITYGB_MBC3_RTC_V1";
const RTC_SIDECAR_EXTENSION: &str = "rtc";

#[derive(Debug, Clone)]
pub struct CartridgeSave {
    save_path: PathBuf,
    rtc_path: PathBuf,
    enabled: bool,
}

#[derive(Debug, Default, Clone)]
pub struct SaveLoadReport {
    pub enabled: bool,
    pub save_path: Option<PathBuf>,
    pub rtc_path: Option<PathBuf>,
    pub messages: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
pub struct SaveFlushReport {
    pub cartridge_ram_written: bool,
    pub rtc_written: bool,
}

impl SaveFlushReport {
    pub fn wrote_anything(self) -> bool {
        self.cartridge_ram_written || self.rtc_written
    }
}

impl CartridgeSave {
    pub fn for_rom_path(rom_path: &Path) -> Self {
        let save_path = rom_path.with_extension("sav");
        let rtc_path = rom_path.with_extension(RTC_SIDECAR_EXTENSION);
        Self {
            save_path,
            rtc_path,
            enabled: false,
        }
    }

    pub fn save_path(&self) -> &Path {
        &self.save_path
    }

    pub fn rtc_path(&self) -> &Path {
        &self.rtc_path
    }

    pub fn load_after_rom(&mut self, mmu: &mut MMU) -> SaveLoadReport {
        self.load_after_rom_at(mmu, unix_now())
    }

    pub fn load_after_rom_at(&mut self, mmu: &mut MMU, now_unix: u64) -> SaveLoadReport {
        self.enabled = mmu.cartridge_has_battery();
        let mut report = SaveLoadReport {
            enabled: self.enabled,
            save_path: self.enabled.then_some(self.save_path.clone()),
            rtc_path: (self.enabled && mmu.cartridge_has_persistable_rtc())
                .then_some(self.rtc_path.clone()),
            messages: Vec::new(),
        };

        if !self.enabled {
            report
                .messages
                .push("cartridge header has no battery; persistence disabled".to_string());
            return report;
        }

        if mmu.cartridge_has_persistable_ram() {
            match fs::read(&self.save_path) {
                Ok(data) => {
                    let expected = mmu.cartridge_ram().len();
                    let copied = mmu.load_cartridge_ram(&data);
                    if data.len() == expected {
                        report.messages.push(format!(
                            "loaded {} bytes of cartridge RAM from {}",
                            copied,
                            self.save_path.display()
                        ));
                    } else if data.len() < expected {
                        report.messages.push(format!(
                            "loaded truncated save {} ({} of {} bytes); missing bytes remain empty",
                            self.save_path.display(),
                            data.len(),
                            expected
                        ));
                    } else {
                        report.messages.push(format!(
                            "loaded first {} bytes from oversized save {} ({} bytes)",
                            copied,
                            self.save_path.display(),
                            data.len()
                        ));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    report.messages.push(format!(
                        "no cartridge RAM save found at {}; starting with empty RAM",
                        self.save_path.display()
                    ));
                }
                Err(error) => {
                    report.messages.push(format!(
                        "could not read {}; starting with empty RAM: {}",
                        self.save_path.display(),
                        error
                    ));
                }
            }
        }

        if mmu.cartridge_has_persistable_rtc() {
            match fs::read_to_string(&self.rtc_path) {
                Ok(text) => match parse_rtc_sidecar(&text) {
                    Some(state) => {
                        if mmu.load_mbc3_rtc_state(state.rtc, state.cycles) {
                            if state.rtc[4] & 0x40 == 0 {
                                mmu.advance_rtc_seconds(
                                    now_unix.saturating_sub(state.timestamp_unix),
                                );
                            }
                            report.messages.push(format!(
                                "loaded MBC3 RTC sidecar from {}",
                                self.rtc_path.display()
                            ));
                        }
                    }
                    None => report.messages.push(format!(
                        "ignored malformed MBC3 RTC sidecar {}; clock starts from reset",
                        self.rtc_path.display()
                    )),
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    report.messages.push(format!(
                        "no MBC3 RTC sidecar found at {}; clock starts from reset",
                        self.rtc_path.display()
                    ));
                }
                Err(error) => report.messages.push(format!(
                    "could not read MBC3 RTC sidecar {}; clock starts from reset: {}",
                    self.rtc_path.display(),
                    error
                )),
            }
        }

        mmu.clear_save_dirty();
        report
    }

    pub fn flush_if_dirty(&self, mmu: &mut MMU) -> io::Result<bool> {
        self.flush_if_dirty_at(mmu, unix_now())
    }

    pub fn flush_if_dirty_at(&self, mmu: &mut MMU, now_unix: u64) -> io::Result<bool> {
        self.flush_report_if_dirty_at(mmu, now_unix)
            .map(SaveFlushReport::wrote_anything)
    }

    pub fn flush_report_if_dirty(&self, mmu: &mut MMU) -> io::Result<SaveFlushReport> {
        self.flush_report_if_dirty_at(mmu, unix_now())
    }

    pub fn flush_report_if_dirty_at(
        &self,
        mmu: &mut MMU,
        now_unix: u64,
    ) -> io::Result<SaveFlushReport> {
        if !self.enabled {
            return Ok(SaveFlushReport::default());
        }

        let mut report = SaveFlushReport::default();
        if mmu.cartridge_has_persistable_ram() && mmu.cartridge_ram_dirty() {
            write_atomic(&self.save_path, mmu.cartridge_ram())?;
            report.cartridge_ram_written = true;
        }

        if mmu.cartridge_has_persistable_rtc() && mmu.rtc_dirty() {
            if let Some((rtc, cycles)) = mmu.mbc3_rtc_state() {
                write_atomic(
                    &self.rtc_path,
                    format_rtc_sidecar(rtc, cycles, now_unix).as_bytes(),
                )?;
                report.rtc_written = true;
            }
        }

        if report.wrote_anything() {
            mmu.clear_save_dirty();
        }
        Ok(report)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RtcSidecar {
    rtc: [u8; 5],
    cycles: u64,
    timestamp_unix: u64,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn format_rtc_sidecar(rtc: [u8; 5], cycles: u64, timestamp_unix: u64) -> String {
    format!(
        "{RTC_MAGIC}\ntimestamp_unix={timestamp_unix}\ncycles={cycles}\nrtc={:02X},{:02X},{:02X},{:02X},{:02X}\n",
        rtc[0], rtc[1], rtc[2], rtc[3], rtc[4]
    )
}

fn parse_rtc_sidecar(text: &str) -> Option<RtcSidecar> {
    let mut lines = text.lines();
    if lines.next()? != RTC_MAGIC {
        return None;
    }
    let mut timestamp_unix = None;
    let mut cycles = None;
    let mut rtc = None;
    for line in lines {
        let (key, value) = line.split_once('=')?;
        match key {
            "timestamp_unix" => timestamp_unix = value.parse().ok(),
            "cycles" => cycles = value.parse().ok(),
            "rtc" => {
                let bytes = value
                    .split(',')
                    .map(|part| u8::from_str_radix(part, 16).ok())
                    .collect::<Option<Vec<_>>>()?;
                rtc =
                    (bytes.len() == 5).then(|| [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4]]);
            }
            _ => return None,
        }
    }
    Some(RtcSidecar {
        rtc: rtc?,
        cycles: cycles?,
        timestamp_unix: timestamp_unix?,
    })
}

fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut temp_path = path.to_path_buf();
    let temp_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map_or_else(|| "tmp".to_string(), |extension| format!("{extension}.tmp"));
    temp_path.set_extension(temp_extension);

    {
        let mut file = File::create(&temp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
    }
    fs::rename(temp_path, path)
}
