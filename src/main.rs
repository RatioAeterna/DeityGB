use std::env;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use deitygb::cartridge_save::CartridgeSave;
use deitygb::headless::{
    default_boot_rom_path, default_boot_rom_path_for_rom, is_cgb_rom, load_file,
};
use deitygb::mmu::JoypadButton;
use deitygb::{apu, cpu, mmu, ppu};
use macroquad::prelude::*;
use std::convert::TryInto;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const CYCLES_PER_FRAME: u32 = 70_224;
const STARTUP_SPLASH_SECONDS: f64 = 1.8;
const FIERCE_DEITY_PNG: &[u8] = include_bytes!("../assets/fierce_deity.png");
const SAVE_FLUSH_DEBOUNCE: Duration = Duration::from_secs(1);

const GB_SCREEN_DIM: u32 = 23040; // 160x144
const SCREEN_UPSCALE_FACTOR: f32 = 5.0; // gameboy screen is super tiny, so we upscale it

fn icon_pixels<const N: usize>(size: u32) -> [u8; N] {
    use image::imageops::FilterType;

    image::load_from_memory(FIERCE_DEITY_PNG)
        .expect("fierce_deity.png should be a valid image")
        .resize_exact(size, size, FilterType::Lanczos3)
        .to_rgba8()
        .into_raw()
        .try_into()
        .unwrap_or_else(|pixels: Vec<u8>| panic!("expected {N} icon bytes, got {}", pixels.len()))
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Fierce Deity's GB".to_owned(),
        window_width: 160 * 5,
        window_height: 144 * 5,
        icon: Some(macroquad::miniquad::conf::Icon {
            small: icon_pixels::<{ 16 * 16 * 4 }>(16),
            medium: icon_pixels::<{ 32 * 32 * 4 }>(32),
            big: icon_pixels::<{ 64 * 64 * 4 }>(64),
        }),
        ..Default::default()
    }
}

#[cfg(target_os = "macos")]
fn set_macos_application_icon() {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let application: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let data: *mut Object = msg_send![class!(NSData),
            dataWithBytes: FIERCE_DEITY_PNG.as_ptr()
            length: FIERCE_DEITY_PNG.len()
        ];
        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithData: data];
        let _: () = msg_send![application, setApplicationIconImage: image];
        let _: () = msg_send![image, release];
    }
}

#[cfg(not(target_os = "macos"))]
fn set_macos_application_icon() {}

async fn show_startup_splash() {
    let splash = Texture2D::from_file_with_format(FIERCE_DEITY_PNG, Some(ImageFormat::Png));
    splash.set_filter(FilterMode::Nearest);

    let started_at = get_time();
    while get_time() - started_at < STARTUP_SPLASH_SECONDS {
        clear_background(BLACK);

        let scale = (screen_width() / splash.width()).min(screen_height() / splash.height());
        let size = vec2(splash.width() * scale, splash.height() * scale);
        draw_texture_ex(
            &splash,
            (screen_width() - size.x) / 2.0,
            (screen_height() - size.y) / 2.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(size),
                ..Default::default()
            },
        );
        next_frame().await;
    }
}

fn host_frame_due(vblank_frame_ready: bool, accumulated_cycles: u32) -> bool {
    vblank_frame_ready || accumulated_cycles >= CYCLES_PER_FRAME
}

pub struct SimpleAudio {
    _stream: cpal::Stream,
}

impl SimpleAudio {
    pub fn new() -> (Self, mpsc::Sender<(f32, f32)>, u32) {
        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        let config = device.default_output_config().unwrap();
        let sample_rate = config.sample_rate().0;
        let channels = usize::from(config.channels());

        // Create a channel for sending samples from APU to audio thread
        let (sample_sender, sample_receiver) = mpsc::channel::<(f32, f32)>();

        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    for frame in data.chunks_mut(channels) {
                        let (left, right) = sample_receiver.try_recv().unwrap_or((0.0, 0.0));
                        if frame.len() == 1 {
                            frame[0] = (left + right) * 0.5;
                        } else {
                            frame[0] = left;
                            frame[1] = right;
                            for sample in &mut frame[2..] {
                                *sample = (left + right) * 0.5;
                            }
                        }
                    }
                },
                |_| {},
                None,
            )
            .unwrap();

        stream.play().unwrap();

        (Self { _stream: stream }, sample_sender, sample_rate)
    }
}

fn handle_input(mmu: &mut mmu::MMU) {
    mmu.set_joypad_button(JoypadButton::Up, is_key_down(KeyCode::W));
    mmu.set_joypad_button(JoypadButton::Left, is_key_down(KeyCode::A));
    mmu.set_joypad_button(JoypadButton::Down, is_key_down(KeyCode::S));
    mmu.set_joypad_button(JoypadButton::Right, is_key_down(KeyCode::D));
    mmu.set_joypad_button(JoypadButton::A, is_key_down(KeyCode::J));
    mmu.set_joypad_button(JoypadButton::B, is_key_down(KeyCode::K));
    mmu.set_joypad_button(JoypadButton::Select, is_key_down(KeyCode::LeftShift));
    mmu.set_joypad_button(JoypadButton::Start, is_key_down(KeyCode::Enter));
}

#[macroquad::main(window_conf)]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: DeityGB <rom.gb> [--apu] [--capture-lcd]");
        return;
    }
    let apu_enabled = args.iter().any(|arg| arg == "--apu");
    let capture_lcd = args.iter().any(|arg| arg == "--capture-lcd");

    set_macos_application_icon();
    show_startup_splash().await;

    let (_audio, sender, sample_rate, _silent_receiver) = if apu_enabled {
        let (audio, sender, sample_rate) = SimpleAudio::new();
        (Some(audio), sender, sample_rate, None)
    } else {
        let (sender, receiver) = mpsc::channel();
        (None, sender, 48_000, Some(receiver))
    };

    let mut mmu = mmu::MMU::new();
    let mut cpu = cpu::CPU::new();
    let mut ppu = ppu::PPU::new();
    let mut apu = apu::APU::with_sample_rate(sender, sample_rate);

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
    /*
    let hex_values: Vec<String> = byte_buffer.iter()
                                    .map(|byte| format!("{:02x}", byte))
                                    .collect();
    let hex_str = hex_values.join(" ");
    println!("{}", hex_str);
    */

    let rom_path = std::path::Path::new(&args[1]);
    let cartridge_byte_buffer = load_file(rom_path).expect("Couldn't read cartridge ROM");
    mmu.load_rom(&cartridge_byte_buffer);
    let boot_path = default_boot_rom_path_for_rom(&cartridge_byte_buffer);
    if is_cgb_rom(&cartridge_byte_buffer) && boot_path == default_boot_rom_path() {
        eprintln!(
            "boot: src/cgb_boot.bin not found; using DMG boot ROM compatibility path"
        );
    }
    let boot_byte_buffer = load_file(&boot_path).expect("Couldn't read boot ROM");
    mmu.load_boot_rom(&boot_byte_buffer);
    let mut cartridge_save = CartridgeSave::for_rom_path(rom_path);
    let save_report = cartridge_save.load_after_rom(&mut mmu);
    if save_report.enabled {
        if let Some(path) = &save_report.save_path {
            eprintln!("save-path: {}", path.display());
        }
        if let Some(path) = &save_report.rtc_path {
            eprintln!("rtc-path: {}", path.display());
        }
    }
    for message in &save_report.messages {
        eprintln!("save: {}", message);
    }

    let mut accumulated_cycles: u32 = 0;
    let mut screen_image = Image {
        bytes: vec![0; GB_SCREEN_DIM as usize * 4],
        width: 160,
        height: 144,
    };
    let screen_texture = Texture2D::from_rgba8(160, 144, &screen_image.bytes);
    screen_texture.set_filter(FilterMode::Nearest);

    let mut last_fps_check = get_time();
    let mut frames = 0;
    let mut fps_display = String::new();

    let target_fps = 59.7275;
    let frame_duration = Duration::from_secs_f64(1.0 / target_fps);
    let mut last_frame_time = Instant::now();

    let mut rendered_yet: bool = false;
    let mut last_lcd_enabled = mmu.get_byte(0xFF40) & 0x80 != 0;
    let mut lcd_transition = 0u32;
    let mut frames_since_lcd_transition = None;
    let mut last_save_flush = Instant::now();
    // Used to keep track of whether we have completed our *one* (1) per-frame render during
    // the vblank period of this frame, yet.
    loop {
        if is_quit_requested() {
            if let Err(error) = cartridge_save.flush_if_dirty(&mut mmu) {
                eprintln!(
                    "save: failed to flush cartridge persistence on shutdown: {}",
                    error
                );
            }
            break;
        }

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
        let peripheral_cycles = mmu.peripheral_cycles(cycles);
        mmu.tick_rtc(u64::from(peripheral_cycles));
        accumulated_cycles = accumulated_cycles.saturating_add(peripheral_cycles as u32);
        //println!("CYCLES: {}, rendered_yet: {}", cycles, rendered_yet);
        ppu.cycle(peripheral_cycles, &mut mmu);
        if apu_enabled {
            apu.cycle(peripheral_cycles, &mut mmu);
        }

        let lcd_enabled = mmu.get_byte(0xFF40) & 0x80 != 0;
        if capture_lcd && lcd_enabled != last_lcd_enabled {
            lcd_transition += 1;
            frames_since_lcd_transition = Some(0u32);
            eprintln!(
                "lcd transition={} enabled={} bank={:#04x} pc={:#06x} lcdc={:#04x} stat={:#04x} ly={} ie={:#04x} if={:#04x}",
                lcd_transition,
                lcd_enabled,
                mmu.mapped_rom_bank(cpu.program_counter()),
                cpu.program_counter(),
                mmu.get_byte(0xFF40),
                mmu.get_byte(0xFF41),
                mmu.get_byte(0xFF44),
                mmu.get_byte(0xFFFF),
                mmu.get_byte(0xFF0F),
            );
        }
        last_lcd_enabled = lcd_enabled;

        handle_input(&mut mmu);

        let vblank_frame_ready = ppu.reached_vblank() && !rendered_yet;
        if host_frame_due(vblank_frame_ready, accumulated_cycles) {
            if vblank_frame_ready {
                screen_image.bytes.copy_from_slice(ppu.get_rgba_buffer());
                rendered_yet = true;
            }
            accumulated_cycles = 0;
            screen_texture.update(&screen_image);
            draw_texture_ex(
                &screen_texture,
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width(), screen_height())),
                    ..Default::default()
                },
            );
            //handle_input(&mut mmu);

            frames += 1;

            let now = get_time();
            let elapsed = now - last_fps_check;

            if elapsed >= 1.0 {
                let fps = (frames as f64 / elapsed) as u32;
                fps_display = format!("FPS: {}", fps);
                frames = 0;
                last_fps_check = now;
            }

            let current_frame_elapsed = last_frame_time.elapsed();
            if current_frame_elapsed < frame_duration {
                std::thread::sleep(frame_duration - current_frame_elapsed);
            }
            last_frame_time = Instant::now();

            draw_text(&fps_display, 10.0, 20.0, 30.0, BLACK);

            if capture_lcd {
                if let Some(frame) = frames_since_lcd_transition {
                    if matches!(frame, 0 | 1 | 2 | 5 | 10 | 30 | 60 | 120) {
                        let prefix = format!("/tmp/deitygb-lcd-{:02}-{:03}", lcd_transition, frame);
                        screen_image.export_png(&format!("{}-framebuffer.png", prefix));
                        get_screen_data().export_png(&format!("{}-window.png", prefix));
                        eprintln!(
                            "lcd capture={} bank={:#04x} pc={:#06x} lcdc={:#04x} stat={:#04x} ly={}",
                            prefix,
                            mmu.mapped_rom_bank(cpu.program_counter()),
                            cpu.program_counter(),
                            mmu.get_byte(0xFF40),
                            mmu.get_byte(0xFF41),
                            mmu.get_byte(0xFF44),
                        );
                    }
                    frames_since_lcd_transition = Some(frame.saturating_add(1));
                }
            }

            next_frame().await;
        }

        if last_save_flush.elapsed() >= SAVE_FLUSH_DEBOUNCE {
            match cartridge_save.flush_report_if_dirty(&mut mmu) {
                Ok(report) => {
                    if report.cartridge_ram_written {
                        eprintln!(
                            "save: flushed cartridge RAM to {}",
                            cartridge_save.save_path().display()
                        );
                    }
                    if report.rtc_written {
                        eprintln!(
                            "save: flushed MBC3 RTC sidecar to {}",
                            cartridge_save.rtc_path().display()
                        );
                    }
                }
                Err(error) => eprintln!("save: failed to flush cartridge persistence: {}", error),
            }
            last_save_flush = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_frame_is_still_due_when_lcd_produces_no_vblank() {
        assert!(!host_frame_due(false, CYCLES_PER_FRAME - 1));
        assert!(host_frame_due(false, CYCLES_PER_FRAME));
        assert!(host_frame_due(true, 0));
    }
}
