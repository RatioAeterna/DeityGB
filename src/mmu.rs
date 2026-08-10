

#[derive(Clone)]
struct MBC1 {
    ram_enabled: bool,
    rom_bank: u8, // lower bits of ROM bank
    ram_bank: u8,
    //upper_bits: u8, // either upper bits of ROM bank (mode 0) or RAM bank (mode 1)
    banking_mode : u8,
}

#[derive(Clone)]
struct MBC3 {
    ram_enabled: bool,
    rom_bank: u8,
    ram_bank: u8,
    rtc: [u8; 5],
    latched_rtc: [u8; 5],
    rtc_latched: bool,
    latch_write: u8,
    rtc_cycles: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JoypadButton {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}


#[derive(Clone)]
pub struct MMU {

	/*
	00 	3FFF 	16KB ROM bank 00 	From cartridge, usually a fixed bank
	4000 	7FFF 	16KB ROM Bank 01~NN 	From cartridge, switchable bank via MBC (if any)
	8000 	9FFF 	8KB Video RAM (VRAM) 	Only bank 0 in Non-CGB mode

	Switchable bank 0/1 in CGB mode
	A000 	BFFF 	8KB External RAM 	In cartridge, switchable bank if any
	C000 	CFFF 	4KB Work RAM (WRAM) bank 0 	
	D000 	DFFF 	4KB Work RAM (WRAM) bank 1~N 	Only bank 1 in Non-CGB mode
	^ Second half of WRAM: Switchable bank 1~7 in CGB mode

	E000 	FDFF 	Mirror of C000~DDFF (ECHO RAM) 	Typically not used
	FE00 	FE9F 	Sprite attribute table (OAM) 	
	FEA0 	FEFF 	Not Usable 	
	FF00 	FF7F 	I/O Registers 	
	FF80 	FFFE 	High RAM (HRAM) 	
	FFFF 	FFFF 	Interrupts Enable Register (IE) 	
	*/

	/*

	Interrupt flags data

	Bit 0: VBlank   Interrupt Enable  (INT 40h)  (1=Enable)
	Bit 1: LCD STAT Interrupt Enable  (INT 48h)  (1=Enable)
	Bit 2: Timer    Interrupt Enable  (INT 50h)  (1=Enable)
	Bit 3: Serial   Interrupt Enable  (INT 58h)  (1=Enable)
	Bit 4: Joypad   Interrupt Enable  (INT 60h)  (1=Enable)
	*/

	pub memory : Vec<u8>, 
        pub boot_rom : Vec<u8>,
        pub rom_data : Vec<u8>,
        pub div_internal: u16,

        cgb_mode: bool,
        vram_bank: u8,
        vram_bank_1: Vec<u8>,
        wram_bank: u8,
        wram_banks: Vec<u8>,
        bg_palette_index: u8,
        obj_palette_index: u8,
        bg_palette_data: [u8; 64],
        obj_palette_data: [u8; 64],
        double_speed: bool,
        speed_switch_armed: bool,
        hdma_source: u16,
        hdma_destination: u16,
        hdma_blocks_remaining: u8,
        hdma_active: bool,

        pub vram_banned : bool,
        pub oam_banned : bool,

        pub joypad_state : u8,
        joypad_buttons: u8,
        joypad_dpad: u8,

        pub cartridge_type_code : u8,
        pub rom_banks : usize,
        pub ram_banks : usize,

        pub mbc1 : MBC1,
        mbc3: MBC3,

        pub last_input_dpad : bool,

        pub ram_size : usize, // size of external ram
        pub external_ram : Vec<u8>,

        pub nr11_written : bool,
        pub nr21_written : bool,
        pub nr31_written : bool,
        pub nr41_written : bool,

        // DAC-relevant registers
        pub nr12_written : bool,
        pub nr22_written : bool,
        pub nr30_written : bool,
        pub nr42_written : bool,

        // trigger registers (we only trigger on WRITES to these
        // when the trigger bit is set)
        pub nr14_written : bool,
        pub nr24_written : bool,
        pub nr34_written : bool,
        pub nr44_written : bool,

        pub div_apu_increment_flag : bool,

        serial_output : Vec<u8>,
}

fn echo_ram_sub(addr: usize) -> usize {
    addr - 0x2000
}

fn echo_ram(addr: usize) -> bool {
    (0xE000..=0xFDFF).contains(&addr)
}


impl MMU {


    pub fn new() -> MMU {
        MMU {
            memory : vec![0; 0x10000],
            boot_rom : vec![0; 0x0100],
            rom_data : vec![],
            div_internal : 0,

            cgb_mode: false,
            vram_bank: 0,
            vram_bank_1: vec![0; 0x2000],
            wram_bank: 1,
            wram_banks: vec![0; 8 * 0x1000],
            bg_palette_index: 0,
            obj_palette_index: 0,
            bg_palette_data: [0xFF; 64],
            obj_palette_data: [0xFF; 64],
            double_speed: false,
            speed_switch_armed: false,
            hdma_source: 0,
            hdma_destination: 0x8000,
            hdma_blocks_remaining: 0,
            hdma_active: false,

            vram_banned : false,
            oam_banned : false,
            joypad_state : 0xCF,
            joypad_buttons: 0x0F,
            joypad_dpad: 0x0F,
            cartridge_type_code : 0x00,
            mbc1 : MBC1 { ram_enabled : false, rom_bank : 1, ram_bank : 0, banking_mode : 0 },
            mbc3: MBC3 {
                ram_enabled: false,
                rom_bank: 1,
                ram_bank: 0,
                rtc: [0; 5],
                latched_rtc: [0; 5],
                rtc_latched: false,
                latch_write: 0xFF,
                rtc_cycles: 0,
            },
            rom_banks : 0,
            ram_banks : 0,

            last_input_dpad : false,
            ram_size : 0,
            external_ram : vec![],

            nr11_written : false,
            nr21_written : false,
            nr31_written : false,
            nr41_written : false,

            nr12_written : false,
            nr22_written : false,
            nr30_written : false,
            nr42_written : false,

            nr14_written : false,
            nr24_written : false,
            nr34_written : false,
            nr44_written : false,

            div_apu_increment_flag : false,
            serial_output : Vec::new(),
        }
    }

    pub fn is_cgb_register(&self, addr : usize) -> bool {
        matches!(
            addr,
            0xFF4C..=0xFF4F
            | 0xFF51..=0xFF56
            | 0xFF68..=0xFF6C
            | 0xFF70
            | 0xFF76
            | 0xFF77
        )

    }

    pub fn cgb_mode(&self) -> bool {
        self.cgb_mode
    }

    pub fn double_speed(&self) -> bool {
        self.double_speed
    }

    pub fn peripheral_cycles(&self, cpu_cycles: u8) -> u8 {
        if self.double_speed { cpu_cycles / 2 } else { cpu_cycles }
    }

    pub fn tick_rtc(&mut self, base_cycles: u64) {
        if !self.has_mbc3_rtc() || self.mbc3.rtc[4] & 0x40 != 0 {
            return;
        }
        self.mbc3.rtc_cycles += base_cycles;
        while self.mbc3.rtc_cycles >= 4_194_304 {
            self.mbc3.rtc_cycles -= 4_194_304;
            self.increment_rtc_second();
        }
    }

    fn increment_rtc_second(&mut self) {
        self.mbc3.rtc[0] += 1;
        if self.mbc3.rtc[0] < 60 {
            return;
        }
        self.mbc3.rtc[0] = 0;
        self.mbc3.rtc[1] += 1;
        if self.mbc3.rtc[1] < 60 {
            return;
        }
        self.mbc3.rtc[1] = 0;
        self.mbc3.rtc[2] += 1;
        if self.mbc3.rtc[2] < 24 {
            return;
        }
        self.mbc3.rtc[2] = 0;
        let day = u16::from(self.mbc3.rtc[3]) | (u16::from(self.mbc3.rtc[4] & 1) << 8);
        let next_day = day + 1;
        self.mbc3.rtc[3] = next_day as u8;
        self.mbc3.rtc[4] = (self.mbc3.rtc[4] & 0xC0) | ((next_day >> 8) as u8 & 1);
        if next_day > 0x1FF {
            self.mbc3.rtc[3] = 0;
            self.mbc3.rtc[4] = (self.mbc3.rtc[4] & 0x40) | 0x80;
        }
    }

    pub fn perform_speed_switch(&mut self) -> bool {
        if !self.cgb_mode || !self.speed_switch_armed {
            return false;
        }
        self.double_speed = !self.double_speed;
        self.speed_switch_armed = false;
        true
    }

    pub fn read_vram_bank(&self, bank: u8, addr: usize) -> u8 {
        debug_assert!((0x8000..=0x9FFF).contains(&addr));
        if bank & 1 == 0 {
            self.memory[addr]
        } else {
            self.vram_bank_1[addr - 0x8000]
        }
    }

    fn write_vram_bank(&mut self, bank: u8, addr: usize, data: u8) {
        debug_assert!((0x8000..=0x9FFF).contains(&addr));
        if bank & 1 == 0 {
            self.memory[addr] = data;
        } else {
            self.vram_bank_1[addr - 0x8000] = data;
        }
    }

    pub fn cgb_bg_color(&self, palette: u8, color: u8) -> u16 {
        self.cgb_palette_color(false, palette, color)
    }

    pub fn cgb_obj_color(&self, palette: u8, color: u8) -> u16 {
        self.cgb_palette_color(true, palette, color)
    }

    fn cgb_palette_color(&self, object: bool, palette: u8, color: u8) -> u16 {
        let data = if object { &self.obj_palette_data } else { &self.bg_palette_data };
        let offset = ((palette as usize & 7) * 8) + ((color as usize & 3) * 2);
        u16::from(data[offset]) | (u16::from(data[offset + 1]) << 8)
    }

    fn transfer_hdma_block(&mut self) {
        for offset in 0..0x10u16 {
            let value = self.get_byte(self.hdma_source.wrapping_add(offset) as usize);
            let destination = 0x8000 | (self.hdma_destination.wrapping_add(offset) & 0x1FFF);
            self.write_vram_bank(self.vram_bank, destination as usize, value);
        }
        self.hdma_source = self.hdma_source.wrapping_add(0x10);
        self.hdma_destination = 0x8000 | (self.hdma_destination.wrapping_add(0x10) & 0x1FFF);
        self.hdma_blocks_remaining = self.hdma_blocks_remaining.saturating_sub(1);
        if self.hdma_blocks_remaining == 0 {
            self.hdma_active = false;
        }
    }

    pub fn hdma_hblank_step(&mut self) {
        if self.cgb_mode && self.hdma_active {
            self.transfer_hdma_block();
        }
    }

    pub fn apu_reg_set(&mut self, addr : usize, data : u8) {
        // first, check if APU is enabled. Ignore write if not.
        let enabled = (self.get_byte(0xFF26 as usize) & 0b10000000) != 0;
        if !enabled && (addr != 0xFF26) && (addr < 0xFF30) {
            return; // if we're not writing to wave RAM
        }
        
        let set_val = match addr {
            0xFF26 => { // NR52
                let master_enable = data & 0x80; // only writeable bit
                let current_status = self.memory[addr] & 0x0F; // keep channel status bits
                master_enable | current_status
            },
            0xFF11 => {
                self.nr11_written = true;
                data
            },
            0xFF16 => {
                self.nr21_written = true;
                data
            },
            0xFF1B => {
                self.nr31_written = true;
                data
            },
            0xFF20 => {
                self.nr41_written = true;
                data
            },
            // possibly writing to DAC
            0xFF12 => {
                self.nr12_written = true;
                data
            },
            0xFF17 => {
                self.nr22_written = true;
                data
            },
            0xFF1A => {
                self.nr30_written = true;
                data
            },
            0xFF21 => {
                self.nr42_written = true;
                data
            },
            0xFF14 => {
                self.nr14_written = true;
                data
            },
            0xFF19 => {
                self.nr24_written = true;
                data
            },
            0xFF1E => {
                self.nr34_written = true;
                data
            },
            0xFF23 => {
                self.nr44_written = true;
                data
            },
            _ => data,
        };
        self.memory[addr] = set_val;
    }

    pub fn apu_reg_get(&self, addr : usize) -> u8 {
        match addr {
            0xFF10 => self.memory[addr] | 0x80,
            0xFF11 => self.memory[addr] | 0x3F,
            0xFF12 => self.memory[addr] | 0x00,
            0xFF13 => self.memory[addr] | 0xFF,
            0xFF14 => self.memory[addr] | 0xBF,

            0xFF15 => self.memory[addr] | 0xFF,
            0xFF16 => self.memory[addr] | 0x3F,
            0xFF17 => self.memory[addr] | 0x00,
            0xFF18 => self.memory[addr] | 0xFF,
            0xFF19 => self.memory[addr] | 0xBF,

            0xFF1A => self.memory[addr] | 0x7F,
            0xFF1B => self.memory[addr] | 0xFF,
            0xFF1C => self.memory[addr] | 0x9F,
            0xFF1D => self.memory[addr] | 0xFF,
            0xFF1E => self.memory[addr] | 0xBF,

            0xFF1F => self.memory[addr] | 0xFF,
            0xFF20 => self.memory[addr] | 0xFF,
            0xFF21 => self.memory[addr] | 0x00,
            0xFF22 => self.memory[addr] | 0x00,
            0xFF23 => self.memory[addr] | 0xBF,

            0xFF24 => self.memory[addr] | 0x00,
            0xFF25 => self.memory[addr] | 0x00,
            0xFF26 => {
                //println!("READING FROM NR52 (actual val): 0b{:08b}", self.memory[addr]);
                //println!("READING FROM NR52: 0b{:08b}", self.memory[addr] | 0x70);
                self.memory[addr] | 0x70
            },

            0xFF27 => self.memory[addr] | 0xFF,
            0xFF28 => self.memory[addr] | 0xFF,
            0xFF29 => self.memory[addr] | 0xFF,
            0xFF2A => self.memory[addr] | 0xFF,
            0xFF2B => self.memory[addr] | 0xFF,
            0xFF2C => self.memory[addr] | 0xFF,
            0xFF2D => self.memory[addr] | 0xFF,
            0xFF2E => self.memory[addr] | 0xFF,
            0xFF2F => self.memory[addr] | 0xFF,

            // 0xFF30-0xFF3F (WAVE RAM) (16 bytes) 
            0xFF30 => self.memory[addr] | 0x00,
            0xFF31 => self.memory[addr] | 0x00,
            0xFF32 => self.memory[addr] | 0x00,
            0xFF33 => self.memory[addr] | 0x00,
            0xFF34 => self.memory[addr] | 0x00,
            0xFF35 => self.memory[addr] | 0x00,
            0xFF36 => self.memory[addr] | 0x00,
            0xFF37 => self.memory[addr] | 0x00,
            0xFF38 => self.memory[addr] | 0x00,
            0xFF39 => self.memory[addr] | 0x00,
            0xFF3A => self.memory[addr] | 0x00,
            0xFF3B => self.memory[addr] | 0x00,
            0xFF3C => self.memory[addr] | 0x00,
            0xFF3D => self.memory[addr] | 0x00,
            0xFF3E => self.memory[addr] | 0x00,
            0xFF3F => self.memory[addr] | 0x00,
            _ => 0xFF // TODO is this what we want..?
        }
    }



    pub fn set_joypad_state(&mut self, data : u8) {
        self.joypad_state = data;
    }

    fn read_joypad(&self) -> u8 {
        let mut value = 0xC0 | (self.joypad_state & 0x30) | 0x0F;
        if value & 0x10 == 0 {
            value &= 0xF0 | self.joypad_dpad;
        }
        if value & 0x20 == 0 {
            value &= 0xF0 | self.joypad_buttons;
        }
        value
    }

    fn request_joypad_interrupt_on_falling_edge(&mut self, previous: u8) {
        let current = self.read_joypad();
        if (previous & !current & 0x0F) != 0 {
            self.memory[0xFF0F] |= 0x10;
        }
    }

    pub fn set_joypad_button(&mut self, button: JoypadButton, pressed: bool) {
        let previous = self.read_joypad();
        let (state, bit) = match button {
            JoypadButton::Right => (&mut self.joypad_dpad, 0),
            JoypadButton::Left => (&mut self.joypad_dpad, 1),
            JoypadButton::Up => (&mut self.joypad_dpad, 2),
            JoypadButton::Down => (&mut self.joypad_dpad, 3),
            JoypadButton::A => (&mut self.joypad_buttons, 0),
            JoypadButton::B => (&mut self.joypad_buttons, 1),
            JoypadButton::Select => (&mut self.joypad_buttons, 2),
            JoypadButton::Start => (&mut self.joypad_buttons, 3),
        };
        if pressed {
            *state &= !(1 << bit);
        } else {
            *state |= 1 << bit;
        }
        self.request_joypad_interrupt_on_falling_edge(previous);
    }


    fn get_rom_bank_mask(&self) -> u8 {
        match self.rom_banks {
            2 => 0x01,
            4 => 0x03,
            8 => 0x07,
            16 => 0x0F,
            _ => 0x1F,
        }
    }

    fn is_mbc1(&self) -> bool {
        matches!(self.cartridge_type_code, 0x01..=0x03)
    }

    fn is_mbc3(&self) -> bool {
        matches!(self.cartridge_type_code, 0x0F..=0x13)
    }

    fn has_mbc3_rtc(&self) -> bool {
        matches!(self.cartridge_type_code, 0x0F | 0x10)
    }
    
    fn compute_zero_bank_number(&self) -> u8 {
        if self.rom_banks <= 32 {
            return 0;
        }
        if self.rom_banks == 64 {
            return (self.mbc1.ram_bank & 0b1) << 5;
            // TODO multi cart roms are an exception.. do we care?
        }
        if self.rom_banks == 128 {
            return (self.mbc1.ram_bank) << 5;
            // should be 0x00, 0x20, 0x40, or 0x60
        }
        else {
            // TODO handle
            return 0;
        }
    }

    fn compute_high_bank_number(&self) -> u8 {
        let base = self.mbc1.rom_bank & self.get_rom_bank_mask();
        if self.rom_banks <= 32 || self.mbc1.banking_mode == 1 {
            return base;
        }
        if self.rom_banks == 64 {
            let bit = self.mbc1.ram_bank & 0b1;
            if bit == 0 {
                return base & 0b11011111;
            }
            else {
                return base | 0b00100000;
            }
        }
        if self.rom_banks == 128 {
            let mut base = base;
            let ram_bank = self.mbc1.ram_bank << 5;
            base &= 0b10011111;
            base |= ram_bank;
            return base;
        }
        else {
            return 0;
        }
    }

    pub fn mapped_rom_bank(&self, addr: u16) -> u8 {
        if self.is_mbc3() {
            return if addr < 0x4000 { 0 } else if addr < 0x8000 { self.mbc3.rom_bank } else { 0 };
        }
        if addr < 0x4000 {
            if self.mbc1.banking_mode == 0 {
                0
            } else {
                self.compute_zero_bank_number()
            }
        } else if addr < 0x8000 {
            self.compute_high_bank_number()
        } else {
            0
        }
    }

    fn read_external_ram(&self, mut addr: usize) -> u8 {
        //println!("READING FROM RAM");
        let ram_enabled = if self.is_mbc3() { self.mbc3.ram_enabled } else { self.mbc1.ram_enabled };
        let ram_bank = if self.is_mbc3() { self.mbc3.ram_bank } else { self.mbc1.ram_bank };
        if !ram_enabled {
            return 0xFF;
        }
        if self.is_mbc3() && ram_bank > 3 {
            if self.has_mbc3_rtc() && (0x08..=0x0C).contains(&ram_bank) {
                let rtc = if self.mbc3.rtc_latched {
                    &self.mbc3.latched_rtc
                } else {
                    &self.mbc3.rtc
                };
                return rtc[(ram_bank - 0x08) as usize];
            }
            return 0xFF;
        }
        if self.ram_size == 0 {
            return 0xFF;
        }
        if (self.ram_size == 2048) || (self.ram_size == 8192) {
            addr = (addr - 0xA000) % self.ram_size;
            return self.external_ram[addr];
        }
        if (self.ram_size == (4 * 8192)) {
            if self.is_mbc1() && self.mbc1.banking_mode == 0 {
                addr = (addr - 0xA000);
                return self.external_ram[addr];
            }
            else {
                addr = 0x2000 * (ram_bank as usize) + (addr - 0xA000);
                //println!("READING FROM RAM IN MODE 1");
                return self.external_ram[addr];
            }
        }
        else {
            // TODO implement this case!!
            return 0xFF;
        }
    }

    fn write_external_ram(&mut self, mut addr: usize, data : u8) {
        //println!("WRITING TO RAM");
        let ram_enabled = if self.is_mbc3() { self.mbc3.ram_enabled } else { self.mbc1.ram_enabled };
        let ram_bank = if self.is_mbc3() { self.mbc3.ram_bank } else { self.mbc1.ram_bank };
        if !ram_enabled {
            return;
        }
        if self.is_mbc3() && ram_bank > 3 {
            if self.has_mbc3_rtc() && (0x08..=0x0C).contains(&ram_bank) {
                let index = (ram_bank - 0x08) as usize;
                self.mbc3.rtc[index] = match ram_bank {
                    0x08 | 0x09 => data & 0x3F,
                    0x0A => data & 0x1F,
                    0x0B => data,
                    0x0C => data & 0xC1,
                    _ => unreachable!(),
                };
            }
            return;
        }
        if self.ram_size == 0 {
            return;
        }
        if (self.ram_size == 2048) || (self.ram_size == 8192) {
            addr = (addr - 0xA000) % self.ram_size;
            self.external_ram[addr] = data;
            return;
        }
        if (self.ram_size == (4 * 8192)) {
            if self.is_mbc1() && self.mbc1.banking_mode == 0 {
                addr = (addr - 0xA000);
                self.external_ram[addr] = data;
                return;
            }
            else {
                addr = 0x2000 * (ram_bank as usize) + (addr - 0xA000);
                self.external_ram[addr] = data;
                //println!("WRITING TO RAM IN MODE 1");
                return;
            }
        }
        else {
            // TODO implement this case!!
        }
    }

    // we need a function that just bypasses all the bullshit
    pub fn set_raw_byte(&mut self, mut addr: usize, data : u8) {
        self.memory[addr] = data;        
    }

    pub fn get_raw_byte(&mut self, mut addr: usize) -> u8 {
        return self.memory[addr];
    }


    pub fn set_byte(&mut self, mut addr: usize, data : u8) {
        if echo_ram(addr) { addr = echo_ram_sub(addr); }

        if self.cgb_mode {
            match addr {
                0xFF4D => {
                    self.speed_switch_armed = data & 0x01 != 0;
                    return;
                }
                0xFF4F => {
                    self.vram_bank = data & 0x01;
                    return;
                }
                0xFF51 => {
                    self.hdma_source = (u16::from(data) << 8) | (self.hdma_source & 0x00FF);
                    return;
                }
                0xFF52 => {
                    self.hdma_source = (self.hdma_source & 0xFF00) | u16::from(data & 0xF0);
                    return;
                }
                0xFF53 => {
                    self.hdma_destination = 0x8000
                        | (u16::from(data & 0x1F) << 8)
                        | (self.hdma_destination & 0x00FF);
                    return;
                }
                0xFF54 => {
                    self.hdma_destination = (self.hdma_destination & 0xFF00) | u16::from(data & 0xF0);
                    return;
                }
                0xFF55 => {
                    if self.hdma_active && data & 0x80 == 0 {
                        self.hdma_active = false;
                        return;
                    }
                    self.hdma_blocks_remaining = (data & 0x7F).wrapping_add(1);
                    self.hdma_active = data & 0x80 != 0;
                    if !self.hdma_active {
                        while self.hdma_blocks_remaining != 0 {
                            self.transfer_hdma_block();
                        }
                    }
                    return;
                }
                0xFF68 => {
                    self.bg_palette_index = data & 0xBF;
                    return;
                }
                0xFF69 => {
                    let index = (self.bg_palette_index & 0x3F) as usize;
                    self.bg_palette_data[index] = data;
                    if self.bg_palette_index & 0x80 != 0 {
                        self.bg_palette_index = 0x80 | ((self.bg_palette_index + 1) & 0x3F);
                    }
                    return;
                }
                0xFF6A => {
                    self.obj_palette_index = data & 0xBF;
                    return;
                }
                0xFF6B => {
                    let index = (self.obj_palette_index & 0x3F) as usize;
                    self.obj_palette_data[index] = data;
                    if self.obj_palette_index & 0x80 != 0 {
                        self.obj_palette_index = 0x80 | ((self.obj_palette_index + 1) & 0x3F);
                    }
                    return;
                }
                0xFF6C => {
                    self.memory[addr] = data & 0x01;
                    return;
                }
                0xFF70 => {
                    let bank = data & 0x07;
                    self.wram_bank = if bank == 0 { 1 } else { bank };
                    return;
                }
                _ => {}
            }
        }

        /*
        if addr == 0xFF45 {
            println!("SETTING LYC! {}", data);
        }
        */

        if addr == 0xFF01 {
            self.memory[addr] = data;
            return;
        }

        if addr == 0xFF02 {
            self.memory[addr] = data;
            let transfer_requested = (data & 0x80) != 0;
            let internal_clock = (data & 0x01) != 0;
            if transfer_requested && internal_clock {
                let outbound = self.memory[0xFF01];
                self.serial_output.push(outbound);
                self.memory[0xFF01] = 0xFF;
                self.memory[0xFF02] = data & !0x80;
                let interrupt_flags = self.get_if() | 0b0000_1000;
                self.set_if(interrupt_flags);
            }
            return;
        }


        if (addr >= 0xFF10) && (addr <= 0xFF3F) {
            self.apu_reg_set(addr, data);
            return;
        }

        // OAM DMA
        // TODO this should take 160 M cycles.
        if (addr == 0xFF46) {
            let source = (data as u16) << 8;
            for i in 0..160 {
                let obj_byte : u8 = self.get_byte((source + i) as usize);
                self.set_oam(i as u8, obj_byte) 
            }            
        }

        if addr == 0xFF00 {
            let previous = self.read_joypad();
            self.joypad_state = 0xC0 | (data & 0x30) | 0x0F;
            self.request_joypad_interrupt_on_falling_edge(previous);
            return;
        }

        // Any non-zero write permanently unmaps the boot ROM until a hardware reset.
        if addr == 0xFF50 {
            if self.memory[0xFF50] == 0 && data != 0 {
                // Preserve DeityGB's established post-boot sentinel. Some existing
                // game paths observe this register even though the boot ROM is
                // already permanently unmapped.
                self.memory[0xFF50] = 10;
            }
            return;
        }


        if self.is_mbc1() && addr <= 0x1FFF {
            self.mbc1.ram_enabled = (data & 0x0F) == 0x0A;
        }
        if self.is_mbc1() && (addr >= 0x2000) && (addr <= 0x3FFF) {
            // restrict to only 5 bit register
            let actual_data = data & 0b00011111;

            if actual_data == 0 {
                self.mbc1.rom_bank = 1;
            }
            else {
                self.mbc1.rom_bank = actual_data & self.get_rom_bank_mask();
                //self.mbc1.rom_bank = if masked == 0 { 1 } else { masked };
            }
        }
        if self.is_mbc1() && (addr >= 0x4000) && (addr <= 0x5FFF) {
            self.mbc1.ram_bank = data & 0b00000011;
        }

        if self.is_mbc1() && (addr >= 0x6000) && (addr <= 0x7FFF) {
            self.mbc1.banking_mode = data & 0b1;
        }

        if self.is_mbc3() && addr <= 0x1FFF {
            self.mbc3.ram_enabled = (data & 0x0F) == 0x0A;
        }
        if self.is_mbc3() && (0x2000..=0x3FFF).contains(&addr) {
            let bank = data & 0x7F;
            self.mbc3.rom_bank = if bank == 0 { 1 } else { bank % self.rom_banks as u8 };
            if self.mbc3.rom_bank == 0 {
                self.mbc3.rom_bank = 1;
            }
        }
        if self.is_mbc3() && (0x4000..=0x5FFF).contains(&addr) {
            self.mbc3.ram_bank = data;
        }
        if self.is_mbc3() && (0x6000..=0x7FFF).contains(&addr) {
            if self.mbc3.latch_write == 0 && data == 1 {
                self.mbc3.latched_rtc = self.mbc3.rtc;
                self.mbc3.rtc_latched = true;
            }
            self.mbc3.latch_write = data;
            return;
        }

        if (self.is_mbc1() || self.is_mbc3()) && (addr >= 0xA000) && (addr <= 0xBFFF) {
            self.write_external_ram(addr, data);
        }


        if addr == 0xFF05 {
        }

        // catch-all for writes to ROM (both valid and invalid)
        if addr < 0x8000 {
            //println!("ROM WRITE");
            return;
        }

        if (0x8000..=0x9FFF).contains(&addr) {
            if !self.vram_banned {
                let bank = if self.cgb_mode { self.vram_bank } else { 0 };
                self.write_vram_bank(bank, addr, data);
            }
            return;
        }

        if (0xC000..=0xCFFF).contains(&addr) {
            self.memory[addr] = data;
            return;
        }

        if (0xD000..=0xDFFF).contains(&addr) {
            if self.cgb_mode && self.wram_bank > 1 {
                let offset = self.wram_bank as usize * 0x1000 + (addr - 0xD000);
                self.wram_banks[offset] = data;
            } else {
                self.memory[addr] = data;
            }
            return;
        }

        if self.is_cgb_register(addr) {
            return;
        }

        if self.vram_banned && (addr >= 0x8000) && (addr <= 0x9FFF) {
            return;
        }
        if self.oam_banned && (addr >= 0xFE00) && (addr <= 0xFE9F) {
            return;
        }
        
        if addr == 0xFF04 { // DIV write resets it
            // DIV-APU increment, if any
            let old_val = self.div_internal;
            let actual_old_val = old_val >> 8;
            if (actual_old_val & 0b00010000) != 0 {
                self.div_apu_increment_flag = true;
            }
            self.div_internal = 0;
        }
        //handle_bank_switch(addr, data);
        self.memory[addr] = data;
    }

    pub fn increment_div(&mut self, cycles : u8) {
        let old_val = self.div_internal;
        self.div_internal = self.div_internal.wrapping_add(cycles as u16); 
        //println!("DIV INTERNAL: {:#04x}, cycles: {}", self.div_internal, cycles);
        //
        //println!("DIV INTERNAL: {} aka {:016b}, cycles: {}", self.div_internal, self.div_internal, cycles);
        // DIV-APU increment, if any
        let actual_old_div = old_val >> 8;
        let actual_new_div = self.div_internal >> 8;

        if ((actual_old_div & 0b00010000) != 0) && ((actual_new_div & 0b00010000) == 0) {
            self.div_apu_increment_flag = true;
        }
    }

    pub fn increment_tima(&mut self) {
        // overflow
        let tima_val = self.memory[0xFF05];
        if  tima_val == 0xFF {
            //println!("SETTING TIMEROO");
            self.memory[0xFF05] = self.memory[0xFF06];
            // request interrupt, turn on timer bit
            let if_reg = self.get_if();
            let mod_if_reg = if_reg | 0b00000100;
            self.set_if(mod_if_reg);
        }
        else {
            self.memory[0xFF05] = tima_val + 1;
        }
    }

    pub fn fetch_div(&mut self) -> u16 {
        return self.div_internal;
    }

    // Non-zero means the boot ROM has been permanently unmapped.
    pub fn get_boot(&self) -> u8 {
        self.memory[0xFF50]
    }

    
    pub fn get_oam(&self, index : u8) -> u8 {
        let addr : usize = (0xFE00 + index as u16).into(); 
        return self.memory[addr];
    }

    pub fn set_oam(&mut self, index : u8, data : u8) {
        let addr : usize = (0xFE00 + index as u16).into(); 
        self.memory[addr] = data;
    }


    pub fn toggle_vram_ban(&mut self, val : bool) {
        self.vram_banned = val;
    }

    pub fn toggle_oam_ban(&mut self, val : bool) {
        self.oam_banned = val;
    }

    pub fn get_byte(&self, mut addr: usize) -> u8 {
        if echo_ram(addr) { addr = echo_ram_sub(addr); }

        if addr < 0x0100 && (self.get_boot() == 0) {
            return self.boot_rom[addr];
        }

        if (addr >= 0xFF10) && (addr <= 0xFF3F) {
            return self.apu_reg_get(addr);
        }

        if self.cgb_mode {
            match addr {
                0xFF4D => return (if self.double_speed { 0x80 } else { 0 })
                    | 0x7E
                    | u8::from(self.speed_switch_armed),
                0xFF4F => return 0xFE | self.vram_bank,
                0xFF51..=0xFF54 => return 0xFF,
                0xFF55 => {
                    return if self.hdma_active {
                        self.hdma_blocks_remaining.saturating_sub(1) & 0x7F
                    } else if self.hdma_blocks_remaining == 0 {
                        0xFF
                    } else {
                        0x80 | self.hdma_blocks_remaining.saturating_sub(1)
                    }
                }
                0xFF56 => return self.memory[addr] | 0x3C,
                0xFF68 => return self.bg_palette_index | 0x40,
                0xFF69 => return self.bg_palette_data[(self.bg_palette_index & 0x3F) as usize],
                0xFF6A => return self.obj_palette_index | 0x40,
                0xFF6B => return self.obj_palette_data[(self.obj_palette_index & 0x3F) as usize],
                0xFF6C => return 0xFE | (self.memory[addr] & 0x01),
                0xFF70 => return 0xF8 | self.wram_bank,
                0xFF76 | 0xFF77 => return 0x00,
                _ => {}
            }
        }

        if self.is_cgb_register(addr) {
            println!("TRYING TO READ : {:#04x}", addr);
            return 0xFF;
        }
    
        if self.is_mbc3() && addr <= 0x3FFF {
            return self.rom_data[addr];
        }

        if self.is_mbc1() && addr <= 0x3FFF {
            if self.mbc1.banking_mode == 0 {
                return self.rom_data[addr];
            }
            else {
                let zero_bank_number = self.compute_zero_bank_number();
                addr = 0x4000 * (zero_bank_number as usize) + addr; 
                return self.rom_data[addr];
            }
        }

        if (self.is_mbc1() || self.is_mbc3()) && (addr >= 0x4000) && (addr <= 0x7FFF) {
            let high_bank_number = if self.is_mbc3() { self.mbc3.rom_bank } else { self.compute_high_bank_number() };
            //println!("HIGH BANK NUMBER {}", high_bank_number);
            let bank_offset = high_bank_number as usize * 0x4000;
            return self.rom_data[bank_offset + (addr - 0x4000) as usize]
        }

        if (self.is_mbc1() || self.is_mbc3()) && (addr >= 0xA000) && (addr <= 0xBFFF) {
            return self.read_external_ram(addr);
        }


        if addr == 0xFF00 {
            return self.read_joypad();
        }
        
        if addr == 0xFF01 {
            return self.memory[addr];
        }

        if (0x8000..=0x9FFF).contains(&addr) {
            if self.vram_banned {
                return 0xFF;
            }
            let bank = if self.cgb_mode { self.vram_bank } else { 0 };
            return self.read_vram_bank(bank, addr);
        }

        if (0xC000..=0xCFFF).contains(&addr) {
            return self.memory[addr];
        }

        if (0xD000..=0xDFFF).contains(&addr) {
            if self.cgb_mode && self.wram_bank > 1 {
                let offset = self.wram_bank as usize * 0x1000 + (addr - 0xD000);
                return self.wram_banks[offset];
            }
            return self.memory[addr];
        }

        if self.vram_banned && (addr >= 0x8000) && (addr <= 0x9FFF) {
            return 0xFF;
        }
        if self.oam_banned && (addr >= 0xFE00) && (addr <= 0xFE9F) {
            return 0xFF;
        }



        // DIV read
        if addr == 0xFF04 { 
            return (self.div_internal >> 8) as u8;
        }

        else {
            return self.memory[addr];
        }
    }

    pub fn get_ie(&mut self) -> u8 {
        self.memory[0xFFFF]
    }

    pub fn get_if(&mut self) -> u8 {
        self.memory[0xFF0F]
    }

    pub fn set_if(&mut self, new_val : u8) {
        self.memory[0xFF0F] = new_val;
    }

    pub fn serial_output(&self) -> &[u8] {
        &self.serial_output
    }

    pub fn drain_serial_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.serial_output)
    }

    pub fn map_cartridge_nintendo_logo(&mut self) {
        // The Nintendo logo bytes
        let nintendo_logo: [u8; 48] = [
            0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
            0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
            0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E
        ];

        let nintendo_logo_r: [u8; 8] = [0x3C, 0x42, 0xB9, 0xA5, 0xB9, 0xA5, 0x42, 0x3C];

        // Copy the logo into the memory starting at address 0x0104
        self.memory[0x0104..0x0104 + nintendo_logo.len()].copy_from_slice(&nintendo_logo);
        self.memory[0x0134..0x0134 + nintendo_logo_r.len()].copy_from_slice(&nintendo_logo_r);

        // need to set the checksum, last thing checked in the boot rom
        self.memory[0x014D] = 0x2F;
    }


    pub fn load_boot_rom(&mut self, rom_data : &Vec<u8>) {
        self.rom_data = rom_data.to_vec();
        for i in 0x0000..0x0100 {
            self.boot_rom[i] = rom_data[i];
        }
    }



    pub fn load_rom(&mut self, rom_data : &Vec<u8>) {
        self.rom_data = rom_data.to_vec();

        self.cgb_mode = matches!(rom_data[0x0143], 0x80 | 0xC0);
        self.vram_bank = 0;
        self.wram_bank = 1;
        self.double_speed = false;
        self.speed_switch_armed = false;
        self.hdma_active = false;
        self.bg_palette_index = 0;
        self.obj_palette_index = 0;
        self.bg_palette_data = [0xFF; 64];
        self.obj_palette_data = [0xFF; 64];
        self.mbc3.rtc = [0; 5];
        self.mbc3.latched_rtc = [0; 5];
        self.mbc3.rtc_latched = false;
        self.mbc3.latch_write = 0xFF;
        self.mbc3.rtc_cycles = 0;

        self.cartridge_type_code = rom_data[0x0147];
        let rom_size_code = rom_data[0x0148];
        let ram_size_code = rom_data[0x0149];


        let do_we_even_use_ram = matches!(self.cartridge_type_code, 0x02 | 0x03 | 0x10 | 0x12 | 0x13);

        self.rom_banks = match rom_size_code {
            0x00 => 2,    
            0x01 => 4,
            0x02 => 8,
            0x03 => 16,
            0x04 => 32,
            0x05 => 64,
            0x06 => 128,
            0x07 => 256,
            0x08 => 512,
            _ => 0,
        };

        if do_we_even_use_ram {
            self.ram_banks = match ram_size_code {
                0x00 => 0,    
                0x02 => 1,
                0x03 => 4,
                0x04 => 16,
                0x05 => 8,
                _ => 0,
            };

            self.ram_size = match ram_size_code {
                0x00 => 2048,
                0x02 => 8192,
                0x03 => 4*8192,
                0x04 => 16*8192,
                0x05 => 8*8192,
                _ => 0,
            };

            self.external_ram = vec![0; self.ram_size];
        }


        println!("ROM BANKS: {}", self.rom_banks);
        println!("RAM BANKS: {}", self.ram_banks);
        println!("do we even use ram: {}", do_we_even_use_ram);
        println!("CGB MODE: {}", self.cgb_mode);


        // First, load fixed bank
        for i in 0x0000..0x4000 {
            // should probably only happen with boot ROM
            if(i >= rom_data.len()) {
                return;
            }
            self.memory[i] = rom_data[i];
            //println!("{:#02x}", rom_data[i]);
        }
        // Next, load switchable bank
        for i in 0x4000..0x8000 {
            self.memory[i] = rom_data[i];
        }
    }
}
