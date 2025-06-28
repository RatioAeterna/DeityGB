use crate::mmu as mmu;

// TODO inline all the simple functions


macro_rules! ternary {
    ($c:expr, $v:expr, $v1:expr) => {
        if $c {$v} else {$v1}
    };
}

pub enum PpuMode {
    OAM,      // Mode 2
    Transfer, // Mode 3
    HBlank,   // Mode 0
    VBlank,   // Mode 1
}

//#[derive(Copy, Clone)]
pub struct PPU {
    // pixel data to be drawn to screen
    screen : [u8; 5760],
    vblank : bool,
    tilemap_start : usize,
    tiledata_start : usize,
    accumulated_cycles : u16
}

impl PPU {


    // NOTE: VRAM ranges from 0x8000 to 0x9FFF
    // NOTE: The GB screen is 160x144 pixels
    // NOTE: Background itself is 256x256 pixels! 


    pub fn new() -> PPU {
        PPU {
            // pixel data to be drawn to screen
            screen : [0; 5760],
            vblank : false,
            tilemap_start : 0x0000,
            tiledata_start : 0x0000,

        }
    }
    
    // TODO these should most likely NOT be public

    pub fn get_scy(&mut self, mmu_ref : &mut mmu::MMU) -> u8 {
        mmu_ref.get_byte(0xFF42 as usize)
    }
    pub fn get_scx(&mut self, mmu_ref : &mut mmu::MMU) -> u8 {
        mmu_ref.get_byte(0xFF43 as usize)
    }
    pub fn get_lcdc(&mut self, mmu_ref : &mut mmu::MMU) -> u8 {
        mmu_ref.get_byte(0xFF40 as usize)
    }



    pub fn inc_ly(&mut self, mmu_ref : &mut mmu::MMU) {
        let prev_val : u8 = mmu_ref.get_byte(0xFF44 as usize);
        let mut new_val : u8 = prev_val+1;
        if (prev_val >= 153) {
            new_val = 0; 
            self.vblank = false;
        }
        else if (prev_val >= 143) {
            self.vblank = true;
            let interrupt_flag = mmu_ref.get_byte(0xFF0F);
            mmu_ref.set_byte(0xFF0F, interrupt_flag | 0x01);  // Set bit 0 (VBLANK)
        }
        mmu_ref.set_byte(0xFF44 as usize, new_val);
    }

    pub fn get_ly(&mut self, mmu_ref : &mut mmu::MMU) -> u8 {
        mmu_ref.get_byte(0xFF44 as usize)
    }


    // should return the ID of the particular tile that we are looking at
    // here x and y are the TILE BYTE INDICES on the 32x32 byte tile map
    pub fn tilemap_fetch_id(&mut self, x_tile_num : usize, y_tile_num : usize, mmu_ref : &mut mmu::MMU) -> u8 {
        // we're fetching the ONE BYTE of the TILE that the CURRENT PIXEL is in
        mmu_ref.get_byte(self.tilemap_start + x_tile_num + y_tile_num*32)
    }

    // should return a SINGLE PIXEL of tile data
    // here x and y are the literal pixel coordinates... the tiles are distributed evenly across
    // the screen so we know exactly what pixel of a tile a particular pixel on the screen is.
    pub fn tiledata_fetch_pixel(&mut self, x: usize, y: usize, tile_id: usize, mmu_ref : &mut mmu::MMU) -> u8 {

        // Tells us "how many bytes into the tile data do we go, for the full data of this particular
        // tile?"
        let tile_offset = 16 * tile_id;

        // First, we answer this question: "What row of the tile do we care about"
        let row = y % 8;

        // Fetch the ROW PAIR that you need. 
        let tile_row_part_1 = mmu_ref.get_byte(self.tiledata_start + tile_offset + 2*row);
        let tile_row_part_2 = mmu_ref.get_byte(self.tiledata_start + tile_offset + 2*row+1);

        // TODO assuming unsigned offsets for now

        // Next, we answer this question: "What column of that row do we care about"
        let col = x % 8;
        let pixel_lower_bit = (tile_row_part_1 & (0b10000000 >> col)) >> (7 - col);
        let pixel_upper_bit = (tile_row_part_2 & (0b10000000 >> col)) >> (7 - col);
        let pixel_value = (pixel_upper_bit << 1) | pixel_lower_bit;

        /*
        if(pixel_value == 0b00000011) {
            println!("DARK PIXEL!!!! {} {}, tile_id: {}. {} {}", x, y, tile_id, pixel_upper_bit, pixel_lower_bit); 
        }
        else {
            println!("TILE_ID: {}.... {} {}", tile_id, pixel_upper_bit, pixel_lower_bit);
        }

        if(tile_id == 33) {
            println!("Printing TILE 31!!");
            for i in 0..16 {
                println!("0b{:08b}", mmu_ref.get_byte(self.tiledata_start + tile_offset + i));
            }
        }
        */

        // NOTE: these are ALL of the form 0b000000XX (i.e., the actual pixel value stored at the
        // end of the byte)
        return pixel_value;
    }

    
    // NOTE: pixel_data should be shifted into its particular position.
    // i.e., it should preserve its ordering in the byte.
    // e.g., 0b00001100 -> "the third pixel is black"
    pub fn set_screen_pixel(&mut self, lx : u8, ly : u8, pixel_data : u8) { 
        // compute which byte offset into the screen buffer corresponds to 'lx'
        // divide by four because there are four pixels per byte
        let x_idx = lx / 4; 
        // "what is the offset into the four pixel cluster of the pixel in
        // question?"
        let x_offset = lx % 4;
        // shift by 0, 2, 4, or 6
        let mask = (0b11111100) << ((3-x_offset)*2);
        // we also gotta correspondingly shift the raw pixel color into position
        let shifted_pixel_data = pixel_data << ((3-x_offset)*2);

        // NOTE: The reason we keep using '40' for stuff here is that there are 4 pixels per byte,
        // and 160 pixels per "row" of the GB screen, so we have 160/4 = 40 bytes per "row"
        //println!("STUFF!! {}, {}, {}", (x_idx as usize + 40 * ly as usize), x_idx, ly);
        let original = self.screen[(x_idx as usize + 40*ly as usize) as usize];
        self.screen[(x_idx as usize + 40*ly as usize) as usize] = (mask & original) | shifted_pixel_data;
    }


    pub fn cycle(&mut self, cycles: u8) {
        self.accumulated_cycles = self.accumulated_cycles.wrapping_add(cycles);

        // figure out which MODE we are in
        match self.mode {
            OAM => {
                if self.accumulated_cycles >= 80 {
                    self.mode = Transfer;
                    // optionally: fire STAT interrupt if enabled
                }
            }
            Transfer => {
                if self.accumulated_cycles >= 80 + 172 {
                    self.mode = HBlank;
                    // this is where you can render the scanline
                    // optionally: fire STAT interrupt if enabled
                }
            }
            HBlank => {
                if self.accumulated_cycles >= 456 {
                    self.inc_ly(mmu_ref);
                    self.accumulated_cycles = 0;

                    if self.get_ly(mmu_ref) == 144 {
                        self.mode = VBlank;
                        // fire VBlank interrupt
                    } else {
                        self.mode = OAM;
                        // optionally: fire STAT interrupt if enabled
                    }
                }
            }
            VBlank => {
                if self.accumulated_cycles >= 456 {
                    self.inc_ly(mmu_ref);
                    self.accumulated_cycles = 0;

                    if self.get_ly(mmu_ref) > 153 {
                        self.inc_ly(mmu_ref) = 0;
                        self.mode = OAM;
                        // optionally: fire STAT interrupt if enabled
                    }
                }
            }
        }
    }




    /* Render a single line of the screen i.e., increment the scanline by ONE,
     * so calls 144 - 153 we're in VBLANK and not drawing anything */

    pub fn render_line(&mut self, mmu_ref : &mut mmu::MMU)  {
        if self.vblank {
            self.inc_ly(mmu_ref);
            //println!("VBLANK!");
            return;
        }
        //println!("====================");
		
        let scy = self.get_scy(mmu_ref);
        //println!("SCY: {}", scy);
        let scx = self.get_scx(mmu_ref);

        let lcdc = self.get_lcdc(mmu_ref);

        self.tilemap_start = ternary!((lcdc & 0b00001000) != 0, 0x9C00, 0x9800);
        self.tiledata_start = ternary!((lcdc & 0b00010000) != 0, 0x8000, 0x8800);

        //println!("Tilemap Start: {:#06X}, Tiledata Start: {:#06X}", self.tilemap_start, self.tiledata_start);



        let ly : u8 = self.get_ly(mmu_ref);

        // FOR EACH PIXEL
        // draw the actual line into the screen buffer by fetching tiles
        for lx in 0..160 {

            // first, fetch tile map associated with that pixel by indexing using scx, scy, lx, ly
            // next, index into tile data to get the TILE data: we can mod the coordinates by 8 to
            // get the actual byte that we want

            let bg_x = ((scx as u16 + lx as u16) % 256) as u8;
            let bg_y = ((scy as u16 + ly as u16) % 256) as u8;

            // first, index into the tilemap using bg_x and bg_y
            let tile_id : u8 = self.tilemap_fetch_id((bg_x / 8) as usize, (bg_y / 8) as usize, mmu_ref);
            let pixel_data : u8 = self.tiledata_fetch_pixel(bg_x as usize, bg_y as usize, tile_id as usize, mmu_ref);
            self.set_screen_pixel(lx, ly, pixel_data); // sets the actual pixel into the screen 
        }
        self.inc_ly(mmu_ref);
    }


    pub fn reached_vblank(&mut self) -> bool {
        matches!(self.mode, VBlank)
        return self.vblank;
    }

    pub fn get_buffer(&mut self) -> &mut [u8; 5760] {
        &mut self.screen
    }
}
