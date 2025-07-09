

#[derive(Clone)]
struct MBC1 {
    ram_enabled: bool,
    rom_bank: u8,
    // Ignore ram_bank and banking_mode for now
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

        pub vram_banned : bool,
        pub oam_banned : bool,

        pub joypad_state : u8,

        pub cartridge_type_code : u8,
        pub rom_banks : usize,
        pub ram_banks : usize,

        pub mbc1 : MBC1,

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

            vram_banned : false,
            oam_banned : false,
            joypad_state : 0xCF,
            cartridge_type_code : 0x00,
            mbc1 : MBC1 { ram_enabled : false, rom_bank : 0 },
            rom_banks : 0,
            ram_banks : 0,
        }
    }

    pub fn is_cgb_register(&self, addr : usize) -> bool {
        matches!(
            addr,
            0xFF4C..=0xFF4F
            | 0xFF51..=0xFF56
            | 0xFF68..=0xFF6C
            | 0xFF6C
            | 0xFF70
            | 0xFF76
            | 0xFF77
        )

    }

    pub fn set_joypad_state(&mut self, data : u8) {
        self.joypad_state = data;
    }

    pub fn set_byte(&mut self, mut addr: usize, data : u8) {
        if echo_ram(addr) { addr = echo_ram_sub(addr); }

        if (self.rom_banks > 2) && (addr >= 0x00) && (addr <= 0x1FFF) {
            self.mbc1.ram_enabled = (data & 0x0F) == 0x0A;
        }
        if (self.rom_banks > 2) && (addr >= 0x2000) && (addr <= 0x3FFF) {
            self.mbc1.rom_bank = data & 0x1F;
            if self.mbc1.rom_bank == 0 {
                self.mbc1.rom_bank = 1;
            }
        }


        if addr == 0xFF05 {
            println!("WRITING TO TIMA: {}", data);
        }

        if addr < 0x8000 {
            println!("ROM WRITE");
            return;
        }

        if self.is_cgb_register(addr) {
            println!("FORBIDDEN. VAL {}", data);
            return;
        }

        if self.vram_banned && (addr >= 0x8000) && (addr <= 0x9FFF) {
            return;
        }
        if self.oam_banned && (addr >= 0xFE00) && (addr <= 0xFE9F) {
            return;
        }
        
        if addr == 0xFF04 { // DIV write resets it
            self.div_internal = 0;
        }
        //handle_bank_switch(addr, data);
        self.memory[addr] = data;
    }

    pub fn increment_div(&mut self, cycles : u8) {
        self.div_internal = self.div_internal.wrapping_add(cycles as u16); 
        //println!("DIV INTERNAL: {:#04x}, cycles: {}", self.div_internal, cycles);
        //
        //println!("DIV INTERNAL: {} aka {:016b}, cycles: {}", self.div_internal, self.div_internal, cycles);
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


    pub fn set_word(&mut self, mut addr: usize, data: u16) {

        if (addr >= 0x00) && (addr <= 0x1FFF) {
            self.mbc1.ram_enabled = (data & 0x0F) == 0x0A;
        }
        if (addr >= 0x2000) && (addr <= 0x3FFF) {
            self.mbc1.rom_bank = (data & 0x001F) as u8;
            if self.mbc1.rom_bank == 0 {
                self.mbc1.rom_bank = 1;            }
        }


        if addr == 0xFF4D {
            println!("FORBIDDEN. VAL {}", data);
        }

        if addr < 0x8000 {
            println!("ROM WRITE");
            return;
        }

        if echo_ram(addr) { addr = echo_ram_sub(addr); }

        if self.vram_banned && (addr >= 0x8000) && (addr <= 0x9FFF) {
            return;
        }
        if self.oam_banned && (addr >= 0xFE00) && (addr <= 0xFE9F) {
            return;
        }

        self.memory[addr] = (data & 0x00FF) as u8;
        self.memory[addr + 1] = (data >> 8) as u8;
    }


    // check if we have we set the 'BOOT' reg, i.e., at the end of the boot sequence
    pub fn get_boot(&self) -> u8 {
        self.memory[0xFF50]
    }

    
    pub fn get_oam(&self, index : u8) -> u8 {
        let addr : usize = (0xFE00 + index as u16).into(); 
        return self.memory[addr];
    }


    pub fn toggle_vram_ban(&mut self, val : bool) {
        self.vram_banned = val;
    }

    pub fn toggle_oam_ban(&mut self, val : bool) {
        self.oam_banned = val;
    }

    pub fn get_byte(&self, mut addr: usize) -> u8 {
        if echo_ram(addr) { addr = echo_ram_sub(addr); }

        if self.is_cgb_register(addr) {
            println!("TRYING TO READ : {:#04x}", addr);
            return 0xFF;
        }

        if (self.rom_banks > 2) && (addr >= 0x4000) && (addr <= 0x7FFF) {
            let bank_offset = self.mbc1.rom_bank as usize * 0x4000;
            return self.rom_data[bank_offset + (addr - 0x4000) as usize]
        }


        if addr == 0xFF00 {
            return self.joypad_state;
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

        if addr < 0x0100 && (self.get_boot() == 0) {
            return self.boot_rom[addr];
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

        self.cartridge_type_code = rom_data[0x0147];
        let rom_size_code = rom_data[0x0148];
        let ram_size_code = rom_data[0x0148];

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

        self.ram_banks = match rom_size_code {
            0x00 => 0,    
            0x02 => 1,
            0x03 => 4,
            0x04 => 16,
            0x05 => 8,
            _ => 0,
        };


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
