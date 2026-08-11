use crate::apu::APU;
use crate::cartridge_save::{CartridgeSave, SaveLoadReport};
use crate::cpu::CPU;
use crate::mmu::{JoypadButton, MMU};
use crate::ppu::PPU;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

pub const DMG_CPU_FREQUENCY: u64 = 4_194_304;
pub const DMG_FRAME_CYCLES: u64 = 70_224;
pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

const MOONEYE_PASS: [u8; 6] = [3, 5, 8, 13, 21, 34];
const MOONEYE_FAIL: [u8; 6] = [0x42; 6];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestOutcome {
    Passed,
    Failed,
    Timeout,
}

#[derive(Debug)]
pub struct RunReport {
    pub outcome: TestOutcome,
    pub cycles: u64,
    pub frames: u64,
    pub serial: Vec<u8>,
}

pub struct GameBoy {
    pub cpu: CPU,
    pub mmu: MMU,
    pub ppu: PPU,
    pub apu: APU,
    _audio_receiver: mpsc::Receiver<(f32, f32)>,
    apu_enabled: bool,
    frames: u64,
    cycles: u64,
    rendered_this_frame: bool,
    save: Option<CartridgeSave>,
}

impl GameBoy {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<(f32, f32)>();
        Self {
            cpu: CPU::new(),
            mmu: MMU::new(),
            ppu: PPU::new(),
            apu: APU::new(sender),
            _audio_receiver: receiver,
            apu_enabled: true,
            frames: 0,
            cycles: 0,
            rendered_this_frame: false,
            save: None,
        }
    }

    pub fn load_boot_rom(&mut self, boot_rom: &[u8]) {
        self.mmu.load_boot_rom(&boot_rom.to_vec());
    }

    pub fn load_rom(&mut self, rom: &[u8]) {
        self.mmu.load_rom(&rom.to_vec());
        self.save = None;
    }

    pub fn load_rom_from_path(&mut self, path: &Path) -> io::Result<SaveLoadReport> {
        let rom = load_file(path)?;
        self.mmu.load_rom(&rom);
        let mut save = CartridgeSave::for_rom_path(path);
        let report = save.load_after_rom(&mut self.mmu);
        self.save = Some(save);
        Ok(report)
    }

    pub fn flush_save_if_dirty(&mut self) -> io::Result<bool> {
        match &self.save {
            Some(save) => save.flush_if_dirty(&mut self.mmu),
            None => Ok(false),
        }
    }

    pub fn set_force_dmg(&mut self, force_dmg: bool) {
        self.mmu.set_force_dmg(force_dmg);
    }

    pub fn step(&mut self) -> u8 {
        if self.ppu.reached_oam() && self.rendered_this_frame {
            self.rendered_this_frame = false;
        }

        let pc = self.cpu.program_counter();
        let bank = self.mmu.mapped_rom_bank(pc);
        let cycles = self.cpu.cycle(&mut self.mmu);
        assert!(
            cycles > 0,
            "CPU returned zero cycles at bank {:#04x}, PC {:#06x}",
            bank,
            pc
        );
        let peripheral_cycles = self.mmu.peripheral_cycles(cycles);
        self.mmu.tick_rtc(u64::from(peripheral_cycles));
        self.ppu.cycle(peripheral_cycles, &mut self.mmu);
        if self.apu_enabled {
            self.apu.cycle(peripheral_cycles, &mut self.mmu);
            while self._audio_receiver.try_recv().is_ok() {}
        }

        if self.ppu.reached_vblank() && !self.rendered_this_frame {
            self.frames += 1;
            self.rendered_this_frame = true;
        }

        self.cycles += cycles as u64;
        cycles
    }

    pub fn run_until(&mut self, max_cycles: u64) -> RunReport {
        let start_cycles = self.cycles;
        while self.cycles - start_cycles < max_cycles {
            self.step();

            let serial = self.mmu.serial_output();
            if serial.ends_with(&MOONEYE_PASS) {
                return self.report(TestOutcome::Passed);
            }
            if serial.ends_with(&MOONEYE_FAIL) || serial.windows(6).any(|w| w == MOONEYE_FAIL) {
                return self.report(TestOutcome::Failed);
            }
            if serial_contains_ascii(serial, "Passed") {
                return self.report(TestOutcome::Passed);
            }
            if serial_contains_ascii(serial, "Failed") {
                return self.report(TestOutcome::Failed);
            }
            if let Some(outcome) = self.blargg_memory_outcome() {
                return self.report(outcome);
            }
        }

        self.report(TestOutcome::Timeout)
    }

    pub fn run_frames(&mut self, frames: u64) {
        let target = self.frames + frames;
        while self.frames < target {
            self.step();
        }
    }

    pub fn run_for_cycles(&mut self, max_cycles: u64) {
        let target = self.cycles + max_cycles;
        while self.cycles < target {
            self.step();
        }
    }

    pub fn set_button(&mut self, button: JoypadButton, pressed: bool) {
        self.mmu.set_joypad_button(button, pressed);
    }

    pub fn set_apu_enabled(&mut self, enabled: bool) {
        self.apu_enabled = enabled;
    }

    pub fn framebuffer_rgba(&mut self) -> Vec<u8> {
        self.ppu.get_rgba_buffer().to_vec()
    }

    pub fn blargg_memory_text(&self) -> Option<String> {
        if [
            self.mmu.get_byte(0xA001),
            self.mmu.get_byte(0xA002),
            self.mmu.get_byte(0xA003),
        ] != [0xDE, 0xB0, 0x61]
        {
            return None;
        }
        let bytes = (0xA004..0xC000)
            .map(|addr| self.mmu.get_byte(addr))
            .take_while(|byte| *byte != 0)
            .collect::<Vec<_>>();
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn blargg_memory_outcome(&self) -> Option<TestOutcome> {
        if self.blargg_memory_text()?.is_empty() {
            return None;
        }
        match self.mmu.get_byte(0xA000) {
            0x80 => None,
            0 => Some(TestOutcome::Passed),
            _ => Some(TestOutcome::Failed),
        }
    }

    fn report(&self, outcome: TestOutcome) -> RunReport {
        RunReport {
            outcome,
            cycles: self.cycles,
            frames: self.frames,
            serial: self.mmu.serial_output().to_vec(),
        }
    }
}

pub fn load_file(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

pub fn default_boot_rom_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/dmg_boot.bin")
}

pub fn packed_framebuffer_to_rgba(screen: &[u8; 5760]) -> Vec<u8> {
    let mut rgba = vec![0; SCREEN_WIDTH * SCREEN_HEIGHT * 4];
    for (i, four_pixels) in screen.iter().copied().enumerate() {
        for j in (0..4).rev() {
            let pixel = (four_pixels & (0b11 << (j * 2))) >> (j * 2);
            let pixel_idx = 16 * i + 4 * (3 - j);
            let (r, g, b) = match pixel {
                0b00 => (0xC4, 0xCF, 0xA1),
                0b01 => (0x8B, 0xAC, 0x0F),
                0b10 => (0x30, 0x62, 0x30),
                0b11 => (0x0F, 0x38, 0x0F),
                _ => unreachable!(),
            };
            rgba[pixel_idx] = r;
            rgba[pixel_idx + 1] = g;
            rgba[pixel_idx + 2] = b;
            rgba[pixel_idx + 3] = 255;
        }
    }
    rgba
}

pub fn write_ppm(path: &Path, rgba: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(file, "P6\n{} {}\n255", SCREEN_WIDTH, SCREEN_HEIGHT)?;
    for pixel in rgba.chunks_exact(4) {
        file.write_all(&pixel[..3])?;
    }
    Ok(())
}

fn serial_contains_ascii(serial: &[u8], needle: &str) -> bool {
    std::str::from_utf8(serial)
        .map(|text| text.contains(needle))
        .unwrap_or(false)
}
