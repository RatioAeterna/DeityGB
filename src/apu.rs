use crate::mmu::MMU;
use std::sync::mpsc;

const CPU_HZ: u32 = 4_194_304;
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 1, 1, 0],
];

#[derive(Clone, Default)]
struct Envelope {
    volume: u8,
    timer: u8,
    period: u8,
    increase: bool,
    running: bool,
}

impl Envelope {
    fn trigger(&mut self, register: u8) {
        self.volume = register >> 4;
        self.period = register & 7;
        self.timer = if self.period == 0 { 8 } else { self.period };
        self.increase = register & 8 != 0;
        self.running = true;
    }

    fn clock(&mut self) {
        if !self.running {
            return;
        }
        self.timer -= 1;
        if self.timer != 0 {
            return;
        }
        self.timer = if self.period == 0 { 8 } else { self.period };
        let next = if self.increase {
            self.volume.checked_add(1).filter(|volume| *volume <= 15)
        } else {
            self.volume.checked_sub(1)
        };
        if let Some(volume) = next {
            self.volume = volume;
        } else {
            self.running = false;
        }
    }
}

#[derive(Clone, Default)]
struct PulseChannel {
    enabled: bool,
    length: u16,
    timer: i32,
    duty_step: u8,
    envelope: Envelope,
}

impl PulseChannel {
    fn clock_timer(&mut self, cycles: i32, frequency: u16) {
        self.timer -= cycles;
        let period = i32::from((2048 - frequency) * 4);
        while self.timer <= 0 {
            self.timer += period;
            self.duty_step = (self.duty_step + 1) & 7;
        }
    }

    fn output(&self, duty: u8) -> u8 {
        if self.enabled && DUTY_TABLE[duty as usize][self.duty_step as usize] != 0 {
            self.envelope.volume
        } else {
            0
        }
    }
}

#[derive(Clone, Default)]
struct WaveChannel {
    enabled: bool,
    length: u16,
    timer: i32,
    position: u8,
    sample: u8,
    fetched: bool,
}

#[derive(Clone)]
struct NoiseChannel {
    enabled: bool,
    length: u16,
    timer: i32,
    lfsr: u16,
    envelope: Envelope,
}

impl Default for NoiseChannel {
    fn default() -> Self {
        Self {
            enabled: false,
            length: 0,
            timer: 0,
            lfsr: 0x7fff,
            envelope: Envelope::default(),
        }
    }
}

#[derive(Clone, Default)]
struct Sweep {
    shadow_frequency: u16,
    timer: u8,
    enabled: bool,
    negate_used: bool,
}

#[derive(Clone)]
pub struct APU {
    pub audio_sender: mpsc::Sender<(f32, f32)>,
    pulse1: PulseChannel,
    pulse2: PulseChannel,
    wave: WaveChannel,
    noise: NoiseChannel,
    sweep: Sweep,
    frame_step: u8,
    sample_clock: u32,
    sample_rate: u32,
    powered: bool,
    left_capacitor: f32,
    right_capacitor: f32,
}

impl APU {
    pub fn new(sender: mpsc::Sender<(f32, f32)>) -> Self {
        Self::with_sample_rate(sender, 48_000)
    }

    pub fn with_sample_rate(sender: mpsc::Sender<(f32, f32)>, sample_rate: u32) -> Self {
        Self {
            audio_sender: sender,
            pulse1: PulseChannel::default(),
            pulse2: PulseChannel::default(),
            wave: WaveChannel::default(),
            noise: NoiseChannel::default(),
            sweep: Sweep::default(),
            frame_step: 0,
            sample_clock: 0,
            sample_rate,
            powered: true,
            left_capacitor: 0.0,
            right_capacitor: 0.0,
        }
    }

    fn frequency(mmu: &MMU, low: usize, high: usize) -> u16 {
        u16::from(mmu.get_raw_byte(low)) | (u16::from(mmu.get_raw_byte(high) & 7) << 8)
    }

    fn set_frequency(mmu: &mut MMU, low: usize, high: usize, frequency: u16) {
        mmu.set_raw_byte(low, frequency as u8);
        let old_high = mmu.get_raw_byte(high);
        mmu.set_raw_byte(high, (old_high & !7) | ((frequency >> 8) as u8 & 7));
    }

    fn set_status(mmu: &mut MMU, channel: u8, enabled: bool) {
        let mask = 1 << channel;
        let status = mmu.get_raw_byte(0xff26);
        mmu.set_raw_byte(
            0xff26,
            if enabled {
                status | mask
            } else {
                status & !mask
            },
        );
    }

    fn disable_all(&mut self, mmu: &mut MMU) {
        self.pulse1.enabled = false;
        self.pulse2.enabled = false;
        self.wave.enabled = false;
        self.noise.enabled = false;
        let power = mmu.get_raw_byte(0xff26) & 0x80;
        mmu.set_raw_byte(0xff26, power);
    }

    fn handle_power(&mut self, mmu: &mut MMU) {
        let powered = mmu.get_raw_byte(0xff26) & 0x80 != 0;
        if self.powered && !powered {
            self.disable_all(mmu);
            for addr in 0xff10..=0xff25 {
                mmu.set_raw_byte(addr, 0);
            }
            self.frame_step = 0;
            self.left_capacitor = 0.0;
            self.right_capacitor = 0.0;
            mmu.nr10_written = false;
            mmu.nr11_written = false;
            mmu.nr12_written = false;
            mmu.nr14_written = false;
            mmu.nr21_written = false;
            mmu.nr22_written = false;
            mmu.nr24_written = false;
            mmu.nr30_written = false;
            mmu.nr31_written = false;
            mmu.nr34_written = false;
            mmu.nr41_written = false;
            mmu.nr42_written = false;
            mmu.nr44_written = false;
        } else if !self.powered && powered {
            self.frame_step = 0;
            if mmu.cgb_mode() {
                self.pulse1.length = 0;
                self.pulse2.length = 0;
                self.wave.length = 0;
                self.noise.length = 0;
            }
        }
        self.powered = powered;
    }

    fn extra_length_clock(&self) -> bool {
        self.frame_step & 1 != 0
    }

    fn clock_length(length: &mut u16, enabled: &mut bool, length_enabled: bool) {
        if length_enabled && *length != 0 {
            *length -= 1;
            if *length == 0 {
                *enabled = false;
            }
        }
    }

    fn handle_length_enable(
        extra_clock: bool,
        old: u8,
        new: u8,
        length: &mut u16,
        enabled: &mut bool,
    ) {
        if old & 0x40 == 0 && new & 0x40 != 0 && extra_clock && *length != 0 {
            *length -= 1;
            if *length == 0 {
                *enabled = false;
            }
        }
    }

    fn trigger_pulse(
        channel: &mut PulseChannel,
        mmu: &MMU,
        envelope_addr: usize,
        high: u8,
        extra_clock: bool,
    ) {
        if channel.length == 0 {
            channel.length = 64;
            if high & 0x40 != 0 && extra_clock {
                channel.length -= 1;
            }
        }
        let envelope = mmu.get_raw_byte(envelope_addr);
        channel.enabled = envelope & 0xf8 != 0;
        channel.envelope.trigger(envelope);
        channel.timer =
            i32::from((2048 - Self::frequency(mmu, envelope_addr + 1, envelope_addr + 2)) * 4);
    }

    fn sweep_calculation(&mut self, mmu: &mut MMU, update: bool) -> bool {
        let nr10 = mmu.get_raw_byte(0xff10);
        let shift = nr10 & 7;
        let delta = self.sweep.shadow_frequency >> shift;
        let frequency = if nr10 & 8 != 0 {
            self.sweep.negate_used = true;
            self.sweep.shadow_frequency.wrapping_sub(delta)
        } else {
            self.sweep.shadow_frequency.wrapping_add(delta)
        };
        if frequency > 0x7ff {
            self.pulse1.enabled = false;
            return false;
        }
        if update && shift != 0 {
            self.sweep.shadow_frequency = frequency;
            Self::set_frequency(mmu, 0xff13, 0xff14, frequency);
        }
        true
    }

    fn trigger_sweep(&mut self, mmu: &mut MMU) {
        let nr10 = mmu.get_raw_byte(0xff10);
        let period = (nr10 >> 4) & 7;
        self.sweep.shadow_frequency = Self::frequency(mmu, 0xff13, 0xff14);
        self.sweep.timer = if period == 0 { 8 } else { period };
        self.sweep.enabled = period != 0 || nr10 & 7 != 0;
        self.sweep.negate_used = false;
        if nr10 & 7 != 0 {
            self.sweep_calculation(mmu, false);
        }
    }

    fn handle_writes(&mut self, mmu: &mut MMU, elapsed_cycles: i32) {
        if mmu.nr11_written {
            self.pulse1.length = 64 - u16::from(mmu.get_raw_byte(0xff11) & 0x3f);
            mmu.nr11_written = false;
        }
        if mmu.nr21_written {
            self.pulse2.length = 64 - u16::from(mmu.get_raw_byte(0xff16) & 0x3f);
            mmu.nr21_written = false;
        }
        if mmu.nr31_written {
            self.wave.length = 256 - u16::from(mmu.get_raw_byte(0xff1b));
            mmu.nr31_written = false;
        }
        if mmu.nr41_written {
            self.noise.length = 64 - u16::from(mmu.get_raw_byte(0xff20) & 0x3f);
            mmu.nr41_written = false;
        }

        if !self.powered {
            return;
        }

        if mmu.nr10_written {
            if mmu.nr10_old & 8 != 0 && mmu.get_raw_byte(0xff10) & 8 == 0 && self.sweep.negate_used
            {
                self.pulse1.enabled = false;
            }
            mmu.nr10_written = false;
        }

        if mmu.nr12_written {
            if mmu.get_raw_byte(0xff12) & 0xf8 == 0 {
                self.pulse1.enabled = false;
            }
            mmu.nr12_written = false;
        }
        if mmu.nr22_written {
            if mmu.get_raw_byte(0xff17) & 0xf8 == 0 {
                self.pulse2.enabled = false;
            }
            mmu.nr22_written = false;
        }
        if mmu.nr30_written {
            if mmu.get_raw_byte(0xff1a) & 0x80 == 0 {
                self.wave.enabled = false;
            }
            mmu.nr30_written = false;
        }
        if mmu.nr42_written {
            if mmu.get_raw_byte(0xff21) & 0xf8 == 0 {
                self.noise.enabled = false;
            }
            mmu.nr42_written = false;
        }

        let extra_clock = self.extra_length_clock();
        if mmu.nr14_written {
            let high = mmu.get_raw_byte(0xff14);
            Self::handle_length_enable(
                extra_clock,
                mmu.nr14_old,
                high,
                &mut self.pulse1.length,
                &mut self.pulse1.enabled,
            );
            if high & 0x80 != 0 {
                Self::trigger_pulse(&mut self.pulse1, mmu, 0xff12, high, extra_clock);
                self.trigger_sweep(mmu);
            }
            mmu.set_raw_byte(0xff14, high & 0x7f);
            mmu.nr14_written = false;
        }
        if mmu.nr24_written {
            let high = mmu.get_raw_byte(0xff19);
            Self::handle_length_enable(
                extra_clock,
                mmu.nr24_old,
                high,
                &mut self.pulse2.length,
                &mut self.pulse2.enabled,
            );
            if high & 0x80 != 0 {
                Self::trigger_pulse(&mut self.pulse2, mmu, 0xff17, high, extra_clock);
            }
            mmu.set_raw_byte(0xff19, high & 0x7f);
            mmu.nr24_written = false;
        }
        if mmu.nr34_written {
            let high = mmu.get_raw_byte(0xff1e);
            Self::handle_length_enable(
                extra_clock,
                mmu.nr34_old,
                high,
                &mut self.wave.length,
                &mut self.wave.enabled,
            );
            if high & 0x80 != 0 {
                if self.wave.length == 0 {
                    self.wave.length = 256;
                    if high & 0x40 != 0 && extra_clock {
                        self.wave.length -= 1;
                    }
                }
                self.wave.enabled = mmu.get_raw_byte(0xff1a) & 0x80 != 0;
                self.wave.timer = i32::from((2048 - Self::frequency(mmu, 0xff1d, 0xff1e)) * 2)
                    + elapsed_cycles
                    + 6;
                self.wave.position = 0;
                self.wave.fetched = false;
            }
            mmu.set_raw_byte(0xff1e, high & 0x7f);
            mmu.nr34_written = false;
        }
        if mmu.nr44_written {
            let high = mmu.get_raw_byte(0xff23);
            Self::handle_length_enable(
                extra_clock,
                mmu.nr44_old,
                high,
                &mut self.noise.length,
                &mut self.noise.enabled,
            );
            if high & 0x80 != 0 {
                if self.noise.length == 0 {
                    self.noise.length = 64;
                    if high & 0x40 != 0 && extra_clock {
                        self.noise.length -= 1;
                    }
                }
                let envelope = mmu.get_raw_byte(0xff21);
                self.noise.enabled = envelope & 0xf8 != 0;
                self.noise.envelope.trigger(envelope);
                self.noise.lfsr = 0x7fff;
                self.noise.timer = Self::noise_period(mmu.get_raw_byte(0xff22));
            }
            mmu.set_raw_byte(0xff23, high & 0x7f);
            mmu.nr44_written = false;
        }

        Self::set_status(mmu, 0, self.pulse1.enabled);
        Self::set_status(mmu, 1, self.pulse2.enabled);
        Self::set_status(mmu, 2, self.wave.enabled);
        Self::set_status(mmu, 3, self.noise.enabled);
    }

    fn clock_sweep(&mut self, mmu: &mut MMU) {
        self.sweep.timer -= 1;
        if self.sweep.timer != 0 {
            return;
        }
        let period = (mmu.get_raw_byte(0xff10) >> 4) & 7;
        self.sweep.timer = if period == 0 { 8 } else { period };
        if self.sweep.enabled && period != 0 && self.sweep_calculation(mmu, true) {
            self.sweep_calculation(mmu, false);
        }
    }

    fn clock_frame_sequencer(&mut self, mmu: &mut MMU) {
        if self.frame_step & 1 == 0 {
            Self::clock_length(
                &mut self.pulse1.length,
                &mut self.pulse1.enabled,
                mmu.get_raw_byte(0xff14) & 0x40 != 0,
            );
            Self::clock_length(
                &mut self.pulse2.length,
                &mut self.pulse2.enabled,
                mmu.get_raw_byte(0xff19) & 0x40 != 0,
            );
            Self::clock_length(
                &mut self.wave.length,
                &mut self.wave.enabled,
                mmu.get_raw_byte(0xff1e) & 0x40 != 0,
            );
            Self::clock_length(
                &mut self.noise.length,
                &mut self.noise.enabled,
                mmu.get_raw_byte(0xff23) & 0x40 != 0,
            );
        }
        if self.frame_step == 2 || self.frame_step == 6 {
            self.clock_sweep(mmu);
        }
        if self.frame_step == 7 {
            self.pulse1.envelope.clock();
            self.pulse2.envelope.clock();
            self.noise.envelope.clock();
        }
        self.frame_step = (self.frame_step + 1) & 7;
    }

    fn noise_period(nr43: u8) -> i32 {
        let divisor = [8, 16, 32, 48, 64, 80, 96, 112][usize::from(nr43 & 7)];
        divisor << (nr43 >> 4)
    }

    fn clock_channels(&mut self, cycles: i32, mmu: &mut MMU) {
        self.pulse1
            .clock_timer(cycles, Self::frequency(mmu, 0xff13, 0xff14));
        self.pulse2
            .clock_timer(cycles, Self::frequency(mmu, 0xff18, 0xff19));

        let wave_period = i32::from((2048 - Self::frequency(mmu, 0xff1d, 0xff1e)) * 2);
        for _ in 0..cycles {
            self.wave.timer -= 1;
            if self.wave.timer <= 0 {
                self.wave.timer += wave_period;
                self.wave.position = (self.wave.position + 1) & 31;
                self.wave.fetched = true;
                let byte = mmu.get_raw_byte(0xff30 + usize::from(self.wave.position / 2));
                self.wave.sample = if self.wave.position & 1 == 0 {
                    byte >> 4
                } else {
                    byte & 0x0f
                };
            }
        }
        mmu.wave_channel_active = self.wave.enabled;
        mmu.wave_ram_index = self.wave.position / 2;
        mmu.wave_sample_position = self.wave.position;
        mmu.wave_timer = self.wave.timer;
        mmu.wave_period = wave_period;
        mmu.wave_fetch_valid = self.wave.fetched;

        self.noise.timer -= cycles;
        let noise_period = Self::noise_period(mmu.get_raw_byte(0xff22));
        while self.noise.timer <= 0 {
            self.noise.timer += noise_period;
            let feedback = (self.noise.lfsr ^ (self.noise.lfsr >> 1)) & 1;
            self.noise.lfsr = (self.noise.lfsr >> 1) | (feedback << 14);
            if mmu.get_raw_byte(0xff22) & 8 != 0 {
                self.noise.lfsr = (self.noise.lfsr & !(1 << 6)) | (feedback << 6);
            }
        }
    }

    fn channel_outputs(&self, mmu: &MMU) -> [u8; 4] {
        let wave_shift = match (mmu.get_raw_byte(0xff1c) >> 5) & 3 {
            0 => 4,
            1 => 0,
            2 => 1,
            _ => 2,
        };
        [
            self.pulse1.output(mmu.get_raw_byte(0xff11) >> 6),
            self.pulse2.output(mmu.get_raw_byte(0xff16) >> 6),
            if self.wave.enabled {
                self.wave.sample >> wave_shift
            } else {
                0
            },
            if self.noise.enabled && self.noise.lfsr & 1 == 0 {
                self.noise.envelope.volume
            } else {
                0
            },
        ]
    }

    fn high_pass(input: f32, capacitor: &mut f32) -> f32 {
        let output = input - *capacitor;
        *capacitor = input - output * 0.996;
        output
    }

    fn generate_sample(&mut self, mmu: &mut MMU) -> (f32, f32) {
        let outputs = self.channel_outputs(mmu);
        mmu.pcm12 = outputs[0] | (outputs[1] << 4);
        mmu.pcm34 = outputs[2] | (outputs[3] << 4);
        let routing = mmu.get_raw_byte(0xff25);
        let volume = mmu.get_raw_byte(0xff24);
        let mut left = 0.0;
        let mut right = 0.0;
        let active = [
            self.pulse1.enabled,
            self.pulse2.enabled,
            self.wave.enabled,
            self.noise.enabled,
        ];
        for (channel, output) in outputs.iter().enumerate() {
            if !active[channel] {
                continue;
            }
            let dac = 1.0 - f32::from(*output) / 7.5;
            if routing & (1 << channel) != 0 {
                right += dac;
            }
            if routing & (1 << (channel + 4)) != 0 {
                left += dac;
            }
        }
        left *= f32::from(((volume >> 4) & 7) + 1) / 32.0;
        right *= f32::from((volume & 7) + 1) / 32.0;
        (
            Self::high_pass(left, &mut self.left_capacitor),
            Self::high_pass(right, &mut self.right_capacitor),
        )
    }

    pub fn cycle(&mut self, t_cycles: u8, mmu: &mut MMU) {
        self.handle_power(mmu);
        self.handle_writes(mmu, i32::from(t_cycles));
        if !self.powered {
            mmu.div_apu_increment_flag = false;
            mmu.wave_channel_active = false;
            return;
        }

        if mmu.div_apu_increment_flag {
            mmu.div_apu_increment_flag = false;
            self.clock_frame_sequencer(mmu);
        }
        self.clock_channels(i32::from(t_cycles), mmu);

        Self::set_status(mmu, 0, self.pulse1.enabled);
        Self::set_status(mmu, 1, self.pulse2.enabled);
        Self::set_status(mmu, 2, self.wave.enabled);
        Self::set_status(mmu, 3, self.noise.enabled);

        self.sample_clock += u32::from(t_cycles) * self.sample_rate;
        while self.sample_clock >= CPU_HZ {
            self.sample_clock -= CPU_HZ;
            let sample = self.generate_sample(mmu);
            let _ = self.audio_sender.send(sample);
        }
    }
}
