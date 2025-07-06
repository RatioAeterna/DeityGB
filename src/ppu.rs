use crate::mmu as mmu;
use PpuMode::*;
use std::cmp::Reverse;

// TODO inline all the simple functions


macro_rules! ternary {
    ($c:expr, $v:expr, $v1:expr) => {
        if $c {$v} else {$v1}
    };
}

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum PpuMode {
    OAM,      // Mode 2
    Transfer, // Mode 3
    HBlank,   // Mode 0
    VBlank,   // Mode 1
}


#[derive(Default, Debug, Clone, Copy)]
pub struct Sprite {
    pub y: u8,              // Actual on-screen Y = y - 16
    pub x: u8,              // Actual on-screen X = x - 8
    pub tile_index: u8,     // Index into tile data
    pub attributes: u8,     // Raw attributes byte

    // Parsed attributes:
    pub priority: bool,     // Bit 7: 0 = in front of BG, 1 = behind BG
    pub y_flip: bool,       // Bit 6
    pub x_flip: bool,       // Bit 5
    pub palette: u8,        // Bit 4: 0 = OBP0, 1 = OBP1
}


//#[derive(Copy, Clone)]
pub struct PPU {
    // pixel data to be drawn to screen
    screen : [u8; 5760],
    vblank : bool,
    tilemap_start : usize,
    tiledata_start : usize,
    accumulated_cycles : u16,
    mode : PpuMode,
    sprites : [Sprite; 40],
    window_line_counter : u8,
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
            accumulated_cycles : 0,
            mode : OAM,
            sprites : [Sprite::default(); 40],
            window_line_counter : 0,
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
        }
        mmu_ref.set_byte(0xFF44 as usize, new_val);
        //println!("INCREMENTED LY! NEW VAL {}", new_val);
        //println!("LYC: {}", self.get_lyc(mmu_ref));
    }

    pub fn get_ly(&mut self, mmu_ref : &mut mmu::MMU) -> u8 {
        mmu_ref.get_byte(0xFF44 as usize)
    }

    pub fn get_lyc(&mut self, mmu_ref : &mut mmu::MMU) -> u8 {
        mmu_ref.get_byte(0xFF45 as usize)
    }


    // should return the ID of the particular tile that we are looking at
    // here x and y are the TILE BYTE INDICES on the 32x32 byte tile map
    pub fn tilemap_fetch_id(&mut self, x_tile_num : usize, y_tile_num : usize, mmu_ref : &mut mmu::MMU) -> u8 {
        // we're fetching the ONE BYTE of the TILE that the CURRENT PIXEL is in
        mmu_ref.get_byte(self.tilemap_start + x_tile_num + y_tile_num*32)
    }

    pub fn tilemap_fetch_id_window(&mut self, x_tile_num : usize, y_tile_num : usize, mmu_ref : &mut mmu::MMU) -> u8 {
        // we're fetching the ONE BYTE of the TILE that the CURRENT PIXEL is in
        /*
        println!("x_tile_num : {} y_tile_num : {}, window_line: {}", x_tile_num, y_tile_num, self.window_line_counter);
        println!("TILEMAP START {:#04X}", self.tilemap_start);
        println!("RETURNING thing at {:#04X}", self.tilemap_start + x_tile_num + y_tile_num*32);
        */
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

    pub fn tiledata_fetch_pixel_signed(&mut self, x: usize, y: usize, tile_id: isize, mmu_ref : &mut mmu::MMU) -> u8 {

        let actual_tiledata_start : isize = 0x9000;

        // Tells us "how many bytes into the tile data do we go, for the full data of this particular
        // tile?"
        let tile_offset = (16 as isize) * tile_id;

        // First, we answer this question: "What row of the tile do we care about"
        let row = y % 8;

        let addr_raw = actual_tiledata_start + tile_offset;
        let orig_tile_offset = 16*tile_id;

        let addr = actual_tiledata_start + tile_offset + (2*row) as isize;
        let tile_row_part_1 = mmu_ref.get_byte(addr as usize);

        //println!("TILE ID {:#04X}, found at: {:#04X}, tile offset: {}, supposed to be: {}", tile_id, addr_raw, tile_offset, orig_tile_offset);

        let addr = actual_tiledata_start + tile_offset + (2*row+1) as isize;
        let tile_row_part_2 = mmu_ref.get_byte(addr as usize);


        let col = x % 8;
        let pixel_lower_bit = (tile_row_part_1 & (0b10000000 >> col)) >> (7 - col);
        let pixel_upper_bit = (tile_row_part_2 & (0b10000000 >> col)) >> (7 - col);
        let pixel_value = (pixel_upper_bit << 1) | pixel_lower_bit;

        return pixel_value;
    }

    
    // NOTE: pixel_data should be shifted into its particular position.
    // i.e., it should preserve its ordering in the byte.
    // e.g., 0b00001100 -> "the third pixel is black"
    pub fn set_screen_pixel(&mut self, lx : u8, ly : u8, pixel_data : u8, sprite : bool) { 
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

        if sprite {
            let actual_pixel = shifted_pixel_data & !mask;
            if actual_pixel == 0 {
                return;
            }
        }

        // NOTE: The reason we keep using '40' for stuff here is that there are 4 pixels per byte,
        // and 160 pixels per "row" of the GB screen, so we have 160/4 = 40 bytes per "row"
        //println!("STUFF!! {}, {}, {}", (x_idx as usize + 40 * ly as usize), x_idx, ly);
        let original = self.screen[(x_idx as usize + 40*ly as usize) as usize];
        self.screen[(x_idx as usize + 40*ly as usize) as usize] = (mask & original) | shifted_pixel_data;
    }

    pub fn get_stat(&self, mmu_ref : &mut mmu::MMU) -> u8 {
        return mmu_ref.get_byte(0xFF41 as usize);
    }

    pub fn toggle_stat(&self, bit : u8, value : bool, mmu_ref : &mut mmu::MMU) {
        let mask = (1 as u8) << bit;
        let old_stat = mmu_ref.get_byte(0xFF41 as usize);
        if value {
            mmu_ref.set_byte(0xFF41, old_stat | mask);
        }
        else {
            mmu_ref.set_byte(0xFF41, old_stat & !mask);
            //println!("TOGGLE STAT: {:#010b} !MASK: {:#01b}, MASK: {:#01b}", old_stat, !mask, mask);
        }
    }

    pub fn toggle_stat_mode(&self, mode : u8, mmu_ref : &mut mmu::MMU) {
        // TODO when PPU is turned off (??) we need to call this function to set
        // mode zero
        assert!(mode >= 0 && mode < 4);
        let old_stat = mmu_ref.get_byte(0xFF41 as usize);
        mmu_ref.set_byte(0xFF41, old_stat | mode);

        // trigger STAT interrupt if appropriate
        match mode {
            0 => {
                if (old_stat & 0b00001000) != 0 {
                    let mut if_reg = mmu_ref.get_if();
                    if_reg |= 0b10;
                    mmu_ref.set_if(if_reg);
                }
            },
            1 => {
                if (old_stat & 0b00010000) != 0 {
                    let mut if_reg = mmu_ref.get_if();
                    if_reg |= 0b10;
                    mmu_ref.set_if(if_reg);
                }
            },
            2 => {
                if (old_stat & 0b00100000) != 0 {
                    let mut if_reg = mmu_ref.get_if();
                    if_reg |= 0b10;
                    mmu_ref.set_if(if_reg);
                }
            },
            _ => ()
        }
    }

    pub fn check_ly_eq_lyc(&mut self, mmu_ref : &mut mmu::MMU) {
        if self.get_ly(mmu_ref) == self.get_lyc(mmu_ref) {
            let old_stat = self.get_stat(mmu_ref);
            self.toggle_stat(2, true, mmu_ref);
            let new_stat = self.get_stat(mmu_ref);
            
            if (old_stat^new_stat != 0) && ((new_stat & 0b01000000) != 0) {
                let mut if_reg = mmu_ref.get_if();
                if_reg |= 0b10;
                mmu_ref.set_if(if_reg);
                //println!("LY==LYC! {} {}", self.get_ly(mmu_ref), self.get_lyc(mmu_ref));
            }
            else {
                //println!("OLD NEWS LY LYC! {} {}", self.get_ly(mmu_ref), self.get_lyc(mmu_ref));
                //println!("OLD STAT: {:#010b} NEW STAT: {:#01b}", old_stat, new_stat);
            }
        }
        else {
            self.toggle_stat(2, false, mmu_ref);
            //println!("LY!=LYC! {} {}, stat{:#010b}", self.get_ly(mmu_ref), self.get_lyc(mmu_ref), self.get_stat(mmu_ref));
        }
    }

    pub fn cycle(&mut self, t_cycles: u8, mmu_ref : &mut mmu::MMU) {
        self.accumulated_cycles = self.accumulated_cycles.wrapping_add(t_cycles as u16);

        self.check_ly_eq_lyc(mmu_ref);
        
        // figure out which MODE we are in
        match self.mode {
            OAM => {
                if self.accumulated_cycles >= 80 {
                    self.mode = Transfer;
                    self.toggle_stat_mode(3, mmu_ref);
                    //mmu_ref.toggle_vram_ban(true);
                    return;
                }
                
                for i in 0..40 {
                    let index : u8 = i as u8 * 4;
                    let y      = mmu_ref.get_oam(index);
                    let x      = mmu_ref.get_oam(index + 1);
                    let tile   = mmu_ref.get_oam(index + 2);
                    let attr   = mmu_ref.get_oam(index + 3);

                    self.sprites[i] = Sprite {
                        y,
                        x,
                        tile_index: tile,
                        attributes: attr,
                        priority: (attr & 0x80) != 0,
                        y_flip:   (attr & 0x40) != 0,
                        x_flip:   (attr & 0x20) != 0,
                        palette:  (attr & 0x10) >> 4,
                    };
                }
            }
            Transfer => {
                if self.accumulated_cycles >= 80 + 172 {
                    self.mode = HBlank;
                    //self.toggle_stat_mode(0, mmu_ref);

                    // this is where you can render the scanline
                    // optionally: fire STAT interrupt if enabled
                    self.render_line(mmu_ref);
                    self.render_sprite_line(mmu_ref);
                    self.render_window_line(mmu_ref);

                    //mmu_ref.toggle_vram_ban(false);
                    //mmu_ref.toggle_oam_ban(false);
                    return;
                }
            }
            HBlank => {
                if self.accumulated_cycles >= 456 {
                    self.inc_ly(mmu_ref);
                    //self.check_ly_eq_lyc(mmu_ref);
                    self.accumulated_cycles = 0;

                    if self.get_ly(mmu_ref) == 144 {
                        self.mode = VBlank;
                        self.toggle_stat_mode(1, mmu_ref);

                        // fire VBlank interrupt
                        let interrupt_flag = mmu_ref.get_byte(0xFF0F);
                        mmu_ref.set_byte(0xFF0F, interrupt_flag | 0x01);  // Set bit 0 (VBLANK)
                        return;
                    } else {
                        self.mode = OAM;
                        self.toggle_stat_mode(2, mmu_ref);

                        //mmu_ref.toggle_oam_ban(true);
                        return;
                    }
                }
            }
            VBlank => {
                if self.accumulated_cycles >= 456 {
                    self.inc_ly(mmu_ref);
                    //self.check_ly_eq_lyc(mmu_ref);
                    self.accumulated_cycles = 0;
                    if self.get_ly(mmu_ref) == 0 {
                        self.mode = OAM;
                        self.toggle_stat_mode(2, mmu_ref);
                        return;
                    }
                }
            }
        }
    }

    fn apply_palette_to_four_pixels(&mut self, four_pixels: u8, palette: u8) -> u8 {
       let mut result = 0u8;
       
       for i in 0..4 {
           // Extract 2-bit pixel value
           let shift = i * 2;
           let pixel_value = (four_pixels >> shift) & 0b11;
           
           // Apply palette lookup
           let palette_shift = pixel_value * 2;
           let final_pixel = (palette >> palette_shift) & 0b11;
           
           // Put the result back in the same position
           result |= final_pixel << shift;
       }
       
       result
    }

    pub fn sprite_on_scanline(&mut self, sprite : &Sprite, mmu_ref : &mut mmu::MMU) -> bool {
        let lcdc = self.get_lcdc(mmu_ref);
        let obj_size_flag = (lcdc >> 2) & 1;
        let sprite_height = if obj_size_flag == 1 { 16 } else { 8 };

        let ly : u8 = self.get_ly(mmu_ref);
        let sprite_y : u8 = sprite.y - 16;
        if (ly >= sprite_y) && (ly < sprite_y + sprite_height) {
            return true;
        }
        return false;

    }

    pub fn render_sprite_line(&mut self, mmu_ref : &mut mmu::MMU) {
        let lcdc = self.get_lcdc(mmu_ref);
        let obj_enabled = lcdc & 0b10 != 0;
        if !obj_enabled {
            return;
        }

        let ly : u8 = self.get_ly(mmu_ref);

        let obj_size_flag = (lcdc >> 2) & 1;
        let sprite_height = if obj_size_flag == 1 { 16 } else { 8 };


        self.tiledata_start = 0x8000;

        let mut sprites_drawn = 0;

        let sprites = self.sprites.clone();


        let mut sprites_with_indices: Vec<(usize, Sprite)> = sprites
            .iter()
            .enumerate()
            .filter(|(_, sprite)| self.sprite_on_scanline(sprite.clone(), mmu_ref))
            .map(|(index, sprite)| (index, *sprite))  // or (sprite.clone(), index)
            .collect();

        sprites_with_indices.truncate(10);

        sprites_with_indices.sort_by_key(|(oam_index, sprite)| (Reverse(sprite.x), Reverse(*oam_index)));

        /*
        println!("LY : {}", ly);
        println!("All sprites with indices:");
        for (index, sprite) in &sprites_with_indices {
            println!("  OAM[{}]: x={}, y={}, tile_id={:#04X}", 
                     index, sprite.x, sprite.y, sprite.tile_index);
        }
        */

        //for (i, sprite) in sprites.iter().enumerate() {
        for (i, sprite) in sprites_with_indices {
            let sprite_y : u8 = sprite.y - 16;
            let sprite_x : u8 = sprite.x - 8;

            let mut tile_id : u8 = sprite.tile_index;
            if sprite_height == 16 {
                tile_id = tile_id & 0b11111110;
            }

            let obp = sprite.palette;
            let mut palette : u8;
            if obp != 0 {
                palette = mmu_ref.get_byte(0xFF49 as usize);
            }
            else {
                palette = mmu_ref.get_byte(0xFF48 as usize);
            }

            if (ly >= sprite_y) && (ly < sprite_y + sprite_height) {
                let scy = self.get_scy(mmu_ref);
                let scx = self.get_scx(mmu_ref);

                // check if we're in the lower half of a tall sprite
                // use the consecutive tile ID if so
                if sprite_height == 16 {
                    let drawing_second_half = ly >= (sprite_y + 8);
                    if sprite.y_flip {
                        if !drawing_second_half {
                            tile_id = tile_id+1;
                        }
                    }
                    else {
                        if drawing_second_half {
                            tile_id = tile_id+1;
                        }

                    }
                }

                for fake_lx in sprite_x..(sprite_x+8) {
                    let tile_x = if sprite.x_flip { 7 - fake_lx } else { fake_lx };
                    let tile_y = if sprite.y_flip { 7 - ly } else { ly };

                    let pixel_data : u8 = self.tiledata_fetch_pixel(tile_x as usize, tile_y as usize, tile_id as usize, mmu_ref);
                    let colored_pixel_data : u8 = self.apply_palette_to_four_pixels(pixel_data, palette);
                    /*
                    if tile_id == 0x0A {
                        println!("COVER.. FAKE_LX: {}, COLORED PIXEL {:#010b}", fake_lx, colored_pixel_data);
                    }
                    if tile_id == 0x09 {
                        println!("MOLE.. FAKE_LX: {}, COLORED PIXEL {:#010b}, original: {:#010b}", fake_lx, colored_pixel_data, pixel_data);
                    }
                    */
                    self.set_screen_pixel(fake_lx, ly, colored_pixel_data, true); // sets the actual pixel into the screen 
                }
                sprites_drawn += 1;
                if sprites_drawn == 10 {
                    return;
                }
            }
        }
    }


    pub fn render_window_line(&mut self, mmu_ref : &mut mmu::MMU)  {
        let lcdc = self.get_lcdc(mmu_ref);

        let lcdc = self.get_lcdc(mmu_ref);
        let window_enabled = lcdc & 0x20 != 0;
        if !window_enabled {
            self.window_line_counter = 0;
            return;
        }

        let scy = self.get_scy(mmu_ref);
        let scx = self.get_scx(mmu_ref);


        self.tilemap_start = ternary!((lcdc & 0b01000000) != 0, 0x9C00, 0x9800);
        self.tiledata_start = ternary!((lcdc & 0b00010000) != 0, 0x8000, 0x8800);

        let signed_tiledata_indices : bool = self.tiledata_start == 0x8800;

        let ly : u8 = self.get_ly(mmu_ref);

        let mut wx : u8 = mmu_ref.get_byte(0xFF4B as usize);
        let mut wy : u8 = mmu_ref.get_byte(0xFF4A as usize);

        if (ly < wy) || (wx > 166) || (wy > 143) {
            return;
        }

        let start_x_val : u8;
        if wx >= 7 {
            start_x_val = wx - 7;
        }
        else {
            start_x_val = 0;
        }

        // FOR EACH PIXEL
        // draw the actual line into the screen buffer by fetching tiles
        for fake_lx in start_x_val..160 {

            // first, fetch tile map associated with that pixel by indexing using scx, scy, lx, ly
            // next, index into tile data to get the TILE data: we can mod the coordinates by 8 to
            // get the actual byte that we want

            //let bg_x = ((scx as u16 + fake_lx as u16) % 256) as u8;
            //let bg_y = ((scy as u16 + ly as u16) % 256) as u8;
            let window_x = fake_lx - start_x_val;
            //let window_y = ly - wy;

            // first, index into the tilemap using bg_x and bg_y
            let tile_id : u8 = self.tilemap_fetch_id_window((window_x / 8) as usize, ((self.window_line_counter) / 8) as usize, mmu_ref);

            let pixel_data : u8;

            if signed_tiledata_indices {
                let signed_tile_id : isize = tile_id as isize;
                pixel_data = self.tiledata_fetch_pixel_signed(fake_lx as usize, ly as usize, tile_id as isize, mmu_ref);
            }
            else {
                pixel_data = self.tiledata_fetch_pixel(fake_lx as usize, ly as usize, tile_id as usize, mmu_ref);
            }

            self.set_screen_pixel(fake_lx, ly, pixel_data, false); // sets the actual pixel into the screen 
        }
        self.window_line_counter = self.window_line_counter.wrapping_add(1);
    }





    /* Render a single line of the screen i.e., increment the scanline by ONE,
     * so calls 144 - 153 we're in VBLANK and not drawing anything */
    pub fn render_line(&mut self, mmu_ref : &mut mmu::MMU)  {
        let lcdc = self.get_lcdc(mmu_ref);
        let bg_enabled = lcdc & 0x01 != 0;

        if !bg_enabled {
            let ly : u8 = self.get_ly(mmu_ref);
            for lx in 0..160 {
                self.set_screen_pixel(lx, ly, 0b00000000, false);
            }
            return;
        }


        let scy = self.get_scy(mmu_ref);
        let scx = self.get_scx(mmu_ref);


        self.tilemap_start = ternary!((lcdc & 0b00001000) != 0, 0x9C00, 0x9800);
        self.tiledata_start = ternary!((lcdc & 0b00010000) != 0, 0x8000, 0x8800);

        let signed_tiledata_indices : bool = self.tiledata_start == 0x8800;

        //println!("Tilemap Start: {:#06X}, Tiledata Start: {:#06X}", self.tilemap_start, self.tiledata_start);

        let palette : u8 = mmu_ref.get_byte(0xFF47);

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

            let pixel_data : u8;

            if signed_tiledata_indices {
                let signed_tile_id : isize = tile_id as isize;
                pixel_data = self.tiledata_fetch_pixel_signed(bg_x as usize, bg_y as usize, signed_tile_id, mmu_ref);
            }
            else {
                pixel_data = self.tiledata_fetch_pixel(bg_x as usize, bg_y as usize, tile_id as usize, mmu_ref);
            }

            //let pixel_data : u8 = self.tiledata_fetch_pixel(bg_x as usize, bg_y as usize, tile_id as usize, mmu_ref);
            let colored_pixel_data : u8 = self.apply_palette_to_four_pixels(pixel_data, palette);
            self.set_screen_pixel(lx, ly, pixel_data, false); // sets the actual pixel into the screen 
        }
    }


    pub fn reached_vblank(&mut self) -> bool {
        matches!(self.mode, VBlank)
    }

    pub fn reached_oam(&mut self) -> bool {
        matches!(self.mode, OAM)
    }

    pub fn get_buffer(&mut self) -> &mut [u8; 5760] {
        &mut self.screen
    }
}
