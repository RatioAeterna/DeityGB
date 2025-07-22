use std::sync::mpsc;
use crate::mmu as mmu;


#[derive(Clone)]
pub struct APU {
    pub audio_sender: mpsc::Sender<f32>,
    pub accumulated_cycles : u16,

    pub square1_phase : f32,
    pub square2_phase : f32,
    pub wave_phase : f32,
    pub noise_phase : f32,

    pub master_audio_enabled : bool,

    pub square1_length_counter : u8,
    pub square2_length_counter : u8,
    pub wave_length_counter : u8,
    pub noise_length_counter : u8,

    pub length_timer : u16,
}

impl APU {


    pub fn new(sender : mpsc::Sender<f32>) -> APU {
        APU {
            audio_sender : sender,
            accumulated_cycles : 0,
            square1_phase : 0.0,
            square2_phase : 0.0,
            wave_phase : 0.0,
            noise_phase : 0.0,
            master_audio_enabled : true,

            square1_length_counter: 0,
            square2_length_counter: 0,
            wave_length_counter: 0,
            noise_length_counter: 0,

            length_timer : 0,
        }
    }


    fn update_length_counters(&mut self, mmu: &mut mmu::MMU) {
        if self.square1_length_counter > 0 {
            let nr14 = mmu.get_raw_byte(0xFF14);
            let length_enabled = (nr14 & 0x40) != 0;  // Bit 6 enables length
            
            if length_enabled {
                println!("Square1 length counter: {}", self.square1_length_counter);
                self.square1_length_counter -= 1;
                if self.square1_length_counter == 0 {
                    // disable the channel
                    let nr52 = mmu.get_raw_byte(0xFF26);
                    let new_val = nr52 & 0b11111110;
                    mmu.set_raw_byte(0xFF26, new_val);
                    println!("DISABLING CHANNEL 1", );
                    println!("NR52: 0b{:08b}", new_val);
                }
            }
        }
        
        if self.square2_length_counter > 0 {
            let nr24 = mmu.get_raw_byte(0xFF19);  // NR24
            let length_enabled = (nr24 & 0x40) != 0;
            
            if length_enabled {
                self.square2_length_counter -= 1;
                if self.square2_length_counter == 0 {
                    // disable the channel
                    let nr52 = mmu.get_raw_byte(0xFF26);
                    let new_val = nr52 & 0b11111101;
                    mmu.set_raw_byte(0xFF26, new_val);
                    println!("DISABLING CHANNEL 2");
                }
            }
        }

        if self.wave_length_counter > 0 {
            let nr34 = mmu.get_raw_byte(0xFF1E);  // NR34
            let length_enabled = (nr34 & 0x40) != 0;
            
            if length_enabled {
                self.wave_length_counter -= 1;
                if self.wave_length_counter == 0 {
                    // disable the channel
                    let nr52 = mmu.get_raw_byte(0xFF26);
                    let new_val = nr52 & 0b11111011;
                    mmu.set_raw_byte(0xFF26, new_val);
                    println!("DISABLING CHANNEL 3");
                }
            }
        }

        if self.noise_length_counter > 0 {
            let nr44 = mmu.get_raw_byte(0xFF23);  // NR34
            let length_enabled = (nr44 & 0x40) != 0;
            
            if length_enabled {
                self.noise_length_counter -= 1;
                if self.noise_length_counter == 0 {
                    // disable the channel
                    let nr52 = mmu.get_raw_byte(0xFF26);
                    let new_val = nr52 & 0b11110111;
                    mmu.set_raw_byte(0xFF26, new_val);
                    println!("DISABLING CHANNEL 4");
                }
            }
        }
    }



    fn check_triggers(&mut self, mmu: &mut mmu::MMU) {
        // before checking triggers, first check writes to the length regs
        if mmu.nr11_written {
            mmu.nr11_written = false;
            let nr11 = mmu.get_raw_byte(0xFF11);
            self.square1_length_counter = 64 - (nr11 & 0x3F);
            println!("RESETTING CHANNEL 1");
        }
        if mmu.nr21_written {
            mmu.nr21_written = false;
            let nr21 = mmu.get_raw_byte(0xFF16);
            self.square2_length_counter = 64 - (nr21 & 0x3F);
            println!("RESETTING CHANNEL 2");
        }
        if mmu.nr31_written {
            mmu.nr31_written = false;
            let nr31 = mmu.get_raw_byte(0xFF1B);
            self.wave_length_counter = 64 - (nr31 & 0x3F);
            println!("RESETTING CHANNEL 3");
        }
        if mmu.nr41_written {
            mmu.nr41_written = false;
            let nr41 = mmu.get_raw_byte(0xFF20);
            self.noise_length_counter = 64 - (nr41 & 0x3F);
            println!("RESETTING CHANNEL 4");
        }


        let nr14 = mmu.get_raw_byte(0xFF14);
        if (nr14 & 0x80) != 0 {
            // Handle trigger
            let nr11 = mmu.get_raw_byte(0xFF11);
            self.square1_length_counter = 64 - (nr11 & 0x3F);
            
            // Enable channel in NR52
            let nr52 = mmu.get_raw_byte(0xFF26);
            mmu.set_raw_byte(0xFF26, nr52 | 0b00000001);
            
            // Clear trigger bit (it's write-only, should read as 0)
            mmu.set_raw_byte(0xFF14, nr14 & !0x80);
        }

        let nr24 = mmu.get_raw_byte(0xFF24);
        if (nr24 & 0x80) != 0 {
            // Handle trigger
            let nr21 = mmu.get_raw_byte(0xFF16);
            self.square2_length_counter = 64 - (nr21 & 0x3F);
            
            // Enable channel in NR52
            let nr52 = mmu.get_raw_byte(0xFF26);
            mmu.set_raw_byte(0xFF26, nr52 | 0b00000001);
            
            // Clear trigger bit (it's write-only, should read as 0)
            mmu.set_raw_byte(0xFF24, nr24 & !0x80);
        }

        let nr34 = mmu.get_raw_byte(0xFF1E);
        if (nr34 & 0x80) != 0 {
            // Handle trigger
            let nr31 = mmu.get_raw_byte(0xFF1B);
            self.wave_length_counter = 64 - (nr31 & 0x3F);
            
            // Enable channel in NR52
            let nr52 = mmu.get_raw_byte(0xFF26);
            mmu.set_raw_byte(0xFF26, nr52 | 0b00000001);
            
            // Clear trigger bit (it's write-only, should read as 0)
            mmu.set_raw_byte(0xFF1E, nr34 & !0x80);
        }

        let nr44 = mmu.get_raw_byte(0xFF23);
        if (nr44 & 0x80) != 0 {
            // Handle trigger
            let nr41 = mmu.get_raw_byte(0xFF20);
            self.noise_length_counter = 64 - (nr41 & 0x3F);
            
            // Enable channel in NR52
            let nr52 = mmu.get_raw_byte(0xFF26);
            mmu.set_raw_byte(0xFF26, nr52 | 0b00000001);
            
            // Clear trigger bit (it's write-only, should read as 0)
            mmu.set_raw_byte(0xFF23, nr44 & !0x80);
        }
    }


    fn generate_square1_sample(&mut self, mmu_ref : &mut mmu::MMU) -> f32 {
        let nr52 = mmu_ref.get_byte(0xFF26 as usize);
        if (nr52 & 0b00000001) == 0 {
            return 0.0;
        }


        let nr11 = mmu_ref.get_byte(0xFF11 as usize);  // Duty cycle + sound length
        let nr12 = mmu_ref.get_byte(0xFF12 as usize);         // Volume envelope
        let nr13 = mmu_ref.get_byte(0xFF13 as usize);         // Frequency low 8 bits
        let nr14 = mmu_ref.get_byte(0xFF14 as usize);        // Frequency high 3 bits + trigger 

        /*
        let current_length = 64 - (nr11 & 0x3F);
        if self.square1_length_counter == 0 {
            // disable the channel
            let new_val = nr52 & 0b11111110;
            mmu_ref.set_raw_byte(0xFF26 as usize, new_val);
        }
        */

        let duty_pattern = (nr11 >> 6) & 0x03;     // Top 2 bits
        let frequency = nr13 as u16 | ((nr14 as u16 & 0x07) << 8);  // 11-bit freq



        let volume = (nr12 >> 4) & 0x0F;                 // Initial volume
        
        // 4. Generate the actual square wave sample
        // Update internal phase based on frequency
        self.square1_phase += (131072.0 / (2048.0 - frequency as f32)) / 44100.0;
        if self.square1_phase >= 1.0 { self.square1_phase -= 1.0; }
        
        // 5. Apply duty cycle pattern and volume
        let duty_output = match duty_pattern {
            0 => self.square1_phase < 0.125,      // 12.5%
            1 => self.square1_phase < 0.25,       // 25%
            2 => self.square1_phase < 0.5,        // 50%
            3 => self.square1_phase < 0.75,       // 75%
            _ => false,
        };
        
        if duty_output {
            (volume as f32 / 15.0) * 0.1  // Scale volume and keep reasonable amplitude
        } else {
            -(volume as f32 / 15.0) * 0.1
        }
    }

    fn generate_square2_sample(&mut self, mmu_ref : &mut mmu::MMU) -> f32 {
        let nr52 = mmu_ref.get_byte(0xFF26 as usize);
        if (nr52 & 0b00000010) == 0 {
            return 0.0;
        }

        let nr21 = mmu_ref.get_byte(0xFF11 as usize);  // Duty cycle + sound length
        let nr22 = mmu_ref.get_byte(0xFF12 as usize);         // Volume envelope
        let nr23 = mmu_ref.get_byte(0xFF13 as usize);         // Frequency low 8 bits
        let nr24 = mmu_ref.get_byte(0xFF14 as usize);        // Frequency high 3 bits + trigger 

        /*
        let current_length = 64 - (nr21 & 0x3F);
        if self.square2_length_counter == 0 {
            let new_val = nr52 & 0b11111101;
            mmu_ref.set_raw_byte(0xFF26 as usize, new_val);
        }
        */
        return 0.0;
    }

    fn generate_wave_sample(&mut self, mmu_ref : &mut mmu::MMU) -> f32 {
        let nr52 = mmu_ref.get_byte(0xFF26 as usize);
        if (nr52 & 0b00000100) == 0 {
            return 0.0;
        }

        /*
        if self.wave_length_counter == 0 {
            let new_val = nr52 & 0b11111011;
            mmu_ref.set_raw_byte(0xFF26 as usize, new_val);
        }
        */
        return 0.0;
    }

    fn generate_noise_sample(&mut self, mmu_ref : &mut mmu::MMU) -> f32 {
        let nr52 = mmu_ref.get_byte(0xFF26 as usize);
        if (nr52 & 0b00001000) == 0 {
            return 0.0;
        }
        /*
        if self.noise_length_counter == 0 {
            let new_val = nr52 & 0b11110111;
            mmu_ref.set_raw_byte(0xFF26 as usize, new_val);
        }
        */
        return 0.0;
    }
    
    pub fn generate_sample(&mut self, mmu_ref : &mut mmu::MMU) -> f32 {
        let square1 = self.generate_square1_sample(mmu_ref);
        let square2 = self.generate_square2_sample(mmu_ref);
        let wave = self.generate_wave_sample(mmu_ref);
        let noise = self.generate_noise_sample(mmu_ref);
        
        // Mix all channels
        (square1 + square2 + wave + noise) * 0.25  // Scale to prevent clipping
    }

    pub fn push_sample_to_audio(&mut self, sample : f32) {
        self.audio_sender.send(sample); 
    }

    fn check_disabled(&mut self, mmu_ref : &mut mmu::MMU) {
        let enabled = (mmu_ref.get_byte(0xFF26 as usize) & 0b10000000) != 0;

        if (!enabled) && (self.master_audio_enabled) {
            //println!("CLEARING APU REGS");
            for addr in 0xFF10..0xFF26 {
                mmu_ref.set_raw_byte(addr as usize, 0);
            }
        }
        self.master_audio_enabled = enabled;
    }

    pub fn cycle(&mut self, t_cycles: u8, mmu_ref : &mut mmu::MMU) {
        self.accumulated_cycles = self.accumulated_cycles.wrapping_add(t_cycles as u16);

        self.check_disabled(mmu_ref);
        if !self.master_audio_enabled {
            return;
        }

        /*
        self.check_triggers(mmu_ref);

        self.length_timer += t_cycles as u16;
        // TODO timing is imprecise here
        if self.length_timer >= 15625 {
            self.length_timer -= 15625;
            self.update_length_counters(mmu_ref);
        }
        */


        const CYCLES_PER_SAMPLE: u16 = 91;
        
        while self.accumulated_cycles >= CYCLES_PER_SAMPLE {
            self.accumulated_cycles -= CYCLES_PER_SAMPLE;
            
            // Generate one audio sample from all 4 channels
            let sample = self.generate_sample(mmu_ref);
            
            // Send it to your audio buffer/cpal
            self.push_sample_to_audio(sample);
        }
    }
}
