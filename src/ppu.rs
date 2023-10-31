use crate::mmu as mmu;

// TODO inline all the simple functions


macro_rules! ternary {
    ($c:expr, $v:expr, $v1:expr) => {
        if $c {$v} else {$v1}
    };
}

//#[derive(Copy, Clone)]
pub struct PPU {
    // pixel data to be drawn to screen
    screen : [u8; 5760],
    vblank : bool,
    tilemap_start : usize,
    tiledata_start : usize,
}

impl PPU {


    // NOTE: VRAM ranges from 0x8000 to 0x9FFF


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
        // TODO is this correct, or should it be 154?
        if (prev_val >= 153) {
            new_val = 0; 
            self.vblank = false;
        }
        else if (prev_val >= 144) {
            self.vblank = true;
            // TODO trigger vblank by setting interrupt flag register
        }
        mmu_ref.set_byte(0xFF44 as usize, new_val);
    }

    pub fn get_ly(&mut self, mmu_ref : &mut mmu::MMU) -> u8 {
        mmu_ref.get_byte(0xFF44 as usize)
    }



    // should return the OFFSET into the tile data
    // here x and y are the TILE BYTE INDICES on the 32x32 byte tile map
    pub fn tilemap_fetch_offset(&mut self, x_tile_byte : u8, y_tile_byte : u8, mmu_ref : &mut mmu::MMU) -> u8 {
        // we're fetching the ONE BYTE of the TILE that the CURRENT PIXEL is in
        mmu_ref.get_byte(self.tilemap_start + ((x_tile_byte + y_tile_byte*32) as usize))
    }

    // should return a SINGLE PIXEL of tile data
    // here x and y are the literal pixel coordinates... the tiles are distributed evenly across
    // the screen so we know exactly what pixel of a tile a particular pixel on the screen is.
    pub fn tiledata_fetch_pixel(&mut self, x: u8, y: u8, offset : u8, mmu_ref : &mut mmu::MMU) -> u8 {
        // TODO assuming unsigned offsets for now
        // fetch the HALF-ROW that this PIXEL is on, zero out the other pixels, return it
        let tile_half_row = mmu_ref.get_byte(self.tiledata_start + (offset + ((x%8)/4) + (y%8)*2) as usize);
        return tile_half_row;
    }

    
    // NOTE: pixel_data should be shifted into its particular position.
    // i.e., it should preserve its ordering in the byte.
    // e.g., 0b00001100 -> "the third pixel is black"
    pub fn set_screen_pixel(&mut self, lx : u8, ly : u8, pixel_data : u8) { 
        // compute x byte
        let x_idx = lx / 4;
        let x_offset = lx % 4;
        let mask = (0b11111100) << (x_offset*2);
        
        println!("STUFF!! {}, {}, {}", (x_idx + 38*ly), x_idx, ly);
        let original = self.screen[(x_idx + 38*ly) as usize];
        self.screen[(x_idx + 38*ly) as usize] = (mask & original) | pixel_data;
    }



    /* Render a single line of the screen i.e., increment the scanline by ONE,
     * so calls 144 - 153 we're in VBLANK and not drawing anything */

	pub fn render_line(&mut self, mmu_ref : &mut mmu::MMU)  {
        if self.vblank {
            self.inc_ly(mmu_ref);
        }
		
        let scy = self.get_scy(mmu_ref);
        let scx = self.get_scx(mmu_ref);

        let lcdc = self.get_lcdc(mmu_ref);

        self.tilemap_start = ternary!((lcdc & 0b00010000) != 0, 0x8000, 0x8800);
        self.tiledata_start = ternary!((lcdc & 0b00001000) != 0, 0x9800, 0x9C00);

        let ly : u8 = self.get_ly(mmu_ref);

        // FOR EACH PIXEL
        // draw the actual line into the screen buffer by fetching tiles
        for lx in 0..160 {

            // first, fetch tile map associated with that pixel by indexing using scx, scy, lx, ly
            // next, index into tile data to get the TILE data: we can mod the coordinates by 8 to
            // get the actual byte that we want

            // first, index into the tilemap using scx and scy
            let offset : u8 = self.tilemap_fetch_offset(scx + (lx / 8), scy + (ly / 8), mmu_ref);
            let pixel_data : u8 = self.tiledata_fetch_pixel(lx, ly, offset, mmu_ref);
            self.set_screen_pixel(lx, ly, pixel_data); // sets the actual pixel into the screen 
        }
        self.inc_ly(mmu_ref);
	}


    pub fn reached_vblank(&mut self) -> bool {
        return self.vblank;
    }

    pub fn get_buffer(&mut self) -> &mut [u8; 5760] {
        &mut self.screen
    }
}
