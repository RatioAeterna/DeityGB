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
mod disassembler;


const CPU_FREQUENCY : u32 = 4194304; // 4.2 MHz
const FRAME_RATE: u32 = 60;
const CYCLES_PER_FRAME: u32 = CPU_FREQUENCY / FRAME_RATE;

const GB_SCREEN_DIM : u32 = 23040; // 160x144
const SCREEN_UPSCALE_FACTOR : f32 = 5.0; // gameboy screen is super tiny, so we upscale it


fn handle_input(mmu: &mut mmu::MMU) {
    // Joypad register address
    let joypad_reg = 0xFF00;
    let mut joypad_state = mmu.get_byte(joypad_reg);


    // Reset lower 4 bits
    joypad_state |= 0x0F;

    let select_buttons = (joypad_state & 0b00100000) == 0;
    let select_dpad = (joypad_state & 0b00010000) == 0;


    if select_dpad {
        if is_key_down(KeyCode::W) {
            joypad_state &= !0b00000100; // Up
            println!("UP");
        }
        if is_key_down(KeyCode::A) {
            joypad_state &= !0b00000010; // Left
            println!("LEFT");
        }
        if is_key_down(KeyCode::S) {
            joypad_state &= !0b00001000; // Down
            println!("DOWN");
        }
        if is_key_down(KeyCode::D) {
            joypad_state &= !0b00000001; // Right
            println!("RIGHT");
        }
    }
    if select_buttons {
        if is_key_down(KeyCode::J) {
            joypad_state &= !0b00000001; // A
            println!("A");
        }
        if is_key_down(KeyCode::K) {
            joypad_state &= !0b00000010; // B
            println!("B");
        }
        if is_key_down(KeyCode::LeftShift) {
            joypad_state &= !0b00000100; // Select
            println!("SELECT");
        }
        if is_key_down(KeyCode::Enter) {
            joypad_state &= !0b00001000; // Start
            println!("START");
        }
    }

    mmu.set_joypad_state(joypad_state);
}

pub fn load_file_bytes(path: &str) -> Vec<u8> {
    let mut file = File::open(path).expect("Couldn't open file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Couldn't read file");
    buffer
}


#[macroquad::main("Fierce Deity's GB")]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let mut mmu = mmu::MMU::new();
    let mut cpu = cpu::CPU::new();
    let mut ppu = ppu::PPU::new();
    //let apu = apu::APU::new(mmu_ref);
    
    /*
    let path = Path::new(&args[1]);
    let display = path.display();
    let mut cartridge = match File::open(&path) {
        Err(why) => panic!("couldn't open {} : {}", display, why),
        Ok(cartridge) => cartridge,
    };
    let mut byte_buffer = Vec::new();
    cartridge.read_to_end(&mut byte_buffer);
    */
    let mut boot_byte_buffer = load_file_bytes("dmg_boot.bin");
    mmu.load_boot_rom(&boot_byte_buffer);
    mmu.map_cartridge_nintendo_logo();


    /*
    let hex_values: Vec<String> = byte_buffer.iter()
                                    .map(|byte| format!("{:02x}", byte))
                                    .collect();
    let hex_str = hex_values.join(" ");
    println!("{}", hex_str);
    */

    if args.len() > 1 {
        let mut cartridge_byte_buffer = load_file_bytes(&args[1]);
        mmu.load_rom(&cartridge_byte_buffer);
    }

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


    let mut last_fps_check = get_time();
    let mut frames = 0;
    let mut fps_display = String::new();



    let mut rendered_yet : bool = false;
    // Used to keep track of whether we have completed our *one* (1) per-frame render during
    // the vblank period of this frame, yet.
    loop {
        if ppu.reached_oam() && rendered_yet {
            // the beginning of a new 'cycle' for the PPU (tho that is a super overloaded term in
            // this project)
            rendered_yet = false;
        }

        // First the CPU runs, then we wait until 456 cycles have passed, corresponding to the time it takes
        // for the PPU to render a single scanline (one line of pixels).
        // After accumulating 456 cycles, we render a single line with the PPU.
        // Once 144 lines are rendered, we enter VBlank, where we can safely copy the screen buffer to display it.
        let cycles = cpu.cycle(&mut mmu);
        ppu.cycle(cycles, &mut mmu);

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
            handle_input(&mut mmu);

            frames += 1;

            let now = get_time();
            let elapsed = now - last_fps_check;

            if elapsed >= 1.0 {
                let fps = (frames as f64 / elapsed) as u32;
                fps_display = format!("FPS: {}", fps);
                frames = 0;
                last_fps_check = now;
            }

            draw_text(&fps_display, 10.0, 20.0, 30.0, BLACK);
            next_frame().await;
        }
    }


}
