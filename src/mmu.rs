
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
        pub rom_data : Vec<u8>,
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
            rom_data : vec![],
        }
    }

    pub fn set_byte(&mut self, mut addr: usize, data : u8) {
        if echo_ram(addr) { addr = echo_ram_sub(addr); }
        //handle_bank_switch(addr, data);
        self.memory[addr] = data;

    }

    /*
    fn handle_bank_switch(&mut self, address: usize, value: u8) {
        if (0x2000..=0x3FFF).contains(&address) {
            match value {
                0 => load_into_switchable_bank(32768..49152),  // 33-48 KB
                1 => load_into_switchable_bank(49152..65536),  // 49-64 KB
                _ => {} // Ignore other values for now 
            }
        }
    }
    */


    pub fn set_word(&mut self, mut addr: usize, data: u16) {
        if echo_ram(addr) { addr = echo_ram_sub(addr); }
        self.memory[addr] = (data & 0x00FF) as u8;
        self.memory[addr + 1] = (data >> 8) as u8;
    }


    // check if we have we set the 'BOOT' reg, i.e., at the end of the boot sequence
    pub fn get_boot(&mut self) -> u8 {
        self.memory[0xFF50]
    }

    pub fn get_byte(&self, mut addr: usize) -> u8 {
        if echo_ram(addr) { addr = echo_ram_sub(addr); }
        return self.memory[addr];
    }

    pub fn enable_interrupts(&mut self) {
        self.memory[0xFFFF] = 0xFF;
    }

    pub fn disable_interrupts(&mut self) {
        self.memory[0xFFFF] = 0x00;
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





    pub fn load_rom(&mut self, rom_data : &Vec<u8>) {
        self.rom_data = rom_data.to_vec();
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
