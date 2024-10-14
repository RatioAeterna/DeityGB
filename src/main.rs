use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use macroquad::prelude::*;

mod cpu;
mod mmu;
mod ppu;
//mod apu;
mod cpu_tables;


const CPU_FREQUENCY : u32 = 4194304; // 4.2 MHz
const FRAME_RATE: u32 = 60;
const CYCLES_PER_FRAME: u32 = CPU_FREQUENCY / FRAME_RATE;

const GB_SCREEN_DIM : u32 = 23040; // 160x144
const SCREEN_UPSCALE_FACTOR : f32 = 5.0; // gameboy screen is super tiny, so we upscale it

#[macroquad::main("Fierce Deity's GB")]
async fn main() {

    let args: Vec<String> = env::args().collect();
    let path = Path::new(&args[1]);
    let display = path.display();
    let mut cartridge = match File::open(&path) {
        Err(why) => panic!("couldn't open {} : {}", display, why),
        Ok(cartridge) => cartridge,
    };
    let mut byte_buffer = Vec::new();
    cartridge.read_to_end(&mut byte_buffer);

    let hex_values: Vec<String> = byte_buffer.iter()
                                    .map(|byte| format!("{:02x}", byte))
                                    .collect();
    let hex_str = hex_values.join(" ");
    println!("{}", hex_str);

    let mut mmu = mmu::MMU::new();
    let mut cpu = cpu::CPU::new();
    let mut ppu = ppu::PPU::new();
    //let apu = apu::APU::new(mmu_ref);

    mmu.load_rom(byte_buffer);
    // TODO only if we're running the BOOT ROM
    mmu.map_cartridge_nintendo_logo();

    let fd_title : String = "Fierce Deity's GB".to_string();
    let conf = Conf {
        window_title: fd_title,
        window_width: 160*5,
        window_height: 144*5,
        ..Default::default()
    };

    let mut accumulated_cycles : u16 = 0;
    let mut gb_screen : [u8; 5760] = [0; 5760];

    // NOTE: We have four pixels per bit, so we multiply by 4 once, and then there are four
    // CHANNELS per pixel (rgba) so we multiply by 4 again.
    let screen_dimension : usize = gb_screen.len() * 4 * 4;
    let mut screen_bitmap_rgba : Vec<u8> = vec![0; screen_dimension];




    // Used to keep track of whether we have completed our *one* (1) per-frame render during
    // the vblank period of this frame, yet.
    loop {
        let mut rendered_yet : bool = false;

        for scanline in 0..154 {
            // First the CPU runs, then we wait until 456 cycles have passed, corresponding to the time it takes
            // for the PPU to render a single scanline (one line of pixels).
            // After accumulating 456 cycles, we render a single line with the PPU.
            // Once 144 lines are rendered, we enter VBlank, where we can safely copy the screen buffer to display it.
            while accumulated_cycles <= 456 {
                        accumulated_cycles += cpu.cycle(&mut mmu) as u16;
            }
            // NOTE: each scanline takes exactly 456 CPU cycles to render.
            // This is based on a frame rate of 60FPS, and therefore 70224 cycles per frame
            // given 154 lines.
            accumulated_cycles -= 456;

            // PPU RENDERING: we *only* render updates to the actual screen on vblank
            ppu.render_line(&mut mmu);
            if ppu.reached_vblank() && !rendered_yet {
                gb_screen = *ppu.get_buffer();

                for i in 0..(gb_screen.len()) {

                    let four_pixels = gb_screen[i];

                    for j in (0..4).rev() {
                        let pixel = (four_pixels & (0b11 << (j*2))) >> (j*2);
                        // multiply by 16, because each group of four pixels,
                        // "each byte of gb_screen", takes up 16 bytes in our final
                        // render array.
                        let pixel_idx = 16*i+4*(3-j);

                        match pixel {
                            // the alleged color scheme
                            0b00 => { // white
                                      screen_bitmap_rgba[pixel_idx+0] = 0xC4;
                                      screen_bitmap_rgba[pixel_idx+1] = 0xCF;
                                      screen_bitmap_rgba[pixel_idx+2] = 0xA1;
                                    },
                            0b01 => { // light gray
                                      screen_bitmap_rgba[pixel_idx+0] = 0x8B;
                                      screen_bitmap_rgba[pixel_idx+1] = 0xAC;
                                      screen_bitmap_rgba[pixel_idx+2] = 0x0F;
                                    },
                            0b10 => { // dark gray
                                      screen_bitmap_rgba[pixel_idx+0] = 0x30;
                                      screen_bitmap_rgba[pixel_idx+1] = 0x62;
                                      screen_bitmap_rgba[pixel_idx+2] = 0x30;
                                    },
                            0b11 => { // black
                                      screen_bitmap_rgba[pixel_idx+0] = 0x0F;
                                      screen_bitmap_rgba[pixel_idx+1] = 0x38;
                                      screen_bitmap_rgba[pixel_idx+2] = 0x0F;
                                    },

                            _ => panic!("Invalid pixel value"),
                        }
                        // alpha channel
                        screen_bitmap_rgba[pixel_idx+3] = 255;
                    }
                }
                rendered_yet = true;
            }
        }
        let texture = Texture2D::from_rgba8(160, 144, &screen_bitmap_rgba);
        texture.set_filter(FilterMode::Nearest);
        draw_texture_ex(
            &texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                ..Default::default()
            },
        );
        next_frame().await
    }


}
