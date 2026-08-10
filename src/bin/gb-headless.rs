use deitygb::headless::{
    default_boot_rom_path, load_file, write_ppm, GameBoy, TestOutcome, DMG_CPU_FREQUENCY,
    DMG_FRAME_CYCLES,
};
use deitygb::mmu::JoypadButton;
use std::env;
use std::path::PathBuf;

fn parse_button_event(value: &str) -> (u64, JoypadButton, u64) {
    let (event, frames) = value.split_once('/').map_or((value, None), |(event, frames)| {
        (event, Some(frames.parse::<u64>().expect("tap length must be a frame count")))
    });
    let (button, second) = event
        .split_once('@')
        .expect("--press must use BUTTON@SECOND[/FRAMES], for example start@35 or left@180/4");
    let button = match button.to_ascii_lowercase().as_str() {
        "right" => JoypadButton::Right,
        "left" => JoypadButton::Left,
        "up" => JoypadButton::Up,
        "down" => JoypadButton::Down,
        "a" => JoypadButton::A,
        "b" => JoypadButton::B,
        "select" => JoypadButton::Select,
        "start" => JoypadButton::Start,
        _ => panic!("unknown button: {}", button),
    };
    let second = second.parse().expect("button event second must be an integer");
    let duration = frames.map_or(DMG_CPU_FREQUENCY, |frames| frames * DMG_FRAME_CYCLES);
    (second, button, duration)
}

fn main() {
    let mut args = env::args().skip(1);
    let rom_path = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: gb-headless <rom.gb> [--seconds N] [--boot path] [--dump-frame path.ppm] [--press BUTTON@SECOND] [--no-apu]");
            std::process::exit(2);
        }
    };

    let mut seconds = 20u64;
    let mut boot_path = Some(default_boot_rom_path());
    let mut dump_frame = None;
    let mut input_events = Vec::new();
    let mut apu_enabled = true;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => {
                let value = args.next().expect("--seconds requires a value");
                seconds = value.parse().expect("--seconds must be an integer");
            }
            "--boot" => {
                boot_path = Some(PathBuf::from(args.next().expect("--boot requires a path")));
            }
            "--no-boot" => {
                boot_path = None;
            }
            "--dump-frame" => {
                dump_frame = Some(PathBuf::from(args.next().expect("--dump-frame requires a path")));
            }
            "--press-start-at" => {
                let value = args.next().expect("--press-start-at requires a value");
                let second = value.parse::<u64>().expect("--press-start-at must be an integer");
                input_events.push((second, JoypadButton::Start, DMG_CPU_FREQUENCY));
            }
            "--press" => {
                let value = args.next().expect("--press requires BUTTON@SECOND");
                input_events.push(parse_button_event(&value));
            }
            "--no-apu" => {
                apu_enabled = false;
            }
            _ => {
                eprintln!("unknown argument: {}", arg);
                std::process::exit(2);
            }
        }
    }

    let rom = load_file(&rom_path).expect("failed to read ROM");
    let mut gb = GameBoy::new();
    gb.set_apu_enabled(apu_enabled);
    if let Some(path) = boot_path {
        let boot = load_file(&path).expect("failed to read boot ROM");
        gb.load_boot_rom(&boot);
    }
    gb.load_rom(&rom);

    let total_cycles = DMG_CPU_FREQUENCY * seconds;
    input_events.sort_by_key(|(second, _, _)| *second);
    let mut elapsed_cycles = 0u64;
    for (second, button, duration) in input_events {
        let event_cycle = (DMG_CPU_FREQUENCY * second).min(total_cycles);
        if event_cycle > elapsed_cycles {
            gb.run_for_cycles(event_cycle - elapsed_cycles);
            elapsed_cycles = event_cycle;
        }
        if elapsed_cycles >= total_cycles {
            break;
        }
        gb.set_button(button, true);
        let pulse_cycles = duration.min(total_cycles - elapsed_cycles);
        gb.run_for_cycles(pulse_cycles);
        elapsed_cycles += pulse_cycles;
        gb.set_button(button, false);
    }
    let report = gb.run_until(total_cycles - elapsed_cycles);
    let serial_text = String::from_utf8_lossy(&report.serial);
    println!("outcome: {:?}", report.outcome);
    println!("cycles: {}", report.cycles);
    println!("frames: {}", report.frames);
    println!("serial: {}", serial_text.escape_default());
    let pc = gb.cpu.program_counter();
    println!(
        "state: bank={:#04x} pc={:#06x} sp={:#06x} hl={:#06x} lcdc={:#04x} stat={:#04x} ly={} ie={:#04x} if={:#04x} bgp={:#04x} scx={} scy={}",
        gb.mmu.mapped_rom_bank(pc),
        pc,
        gb.cpu.stack_pointer(),
        gb.cpu.hl(),
        gb.mmu.get_byte(0xFF40),
        gb.mmu.get_byte(0xFF41),
        gb.mmu.get_byte(0xFF44),
        gb.mmu.get_ie(),
        gb.mmu.get_if(),
        gb.mmu.get_byte(0xFF47),
        gb.mmu.get_byte(0xFF43),
        gb.mmu.get_byte(0xFF42),
    );
    let tile_id = gb.mmu.get_byte(0x9800);
    let tile_address = 0x8000 + tile_id as usize * 16;
    println!(
        "bg: tile_id={:#04x} tile_data={:02x?} map={:02x?}",
        tile_id,
        &gb.mmu.memory[tile_address..tile_address + 16],
        &gb.mmu.memory[0x9800..0x9810],
    );
    if let Some(path) = dump_frame {
        let rgba = gb.framebuffer_rgba();
        write_ppm(&path, &rgba).expect("failed to write frame dump");
        println!("frame: {}", path.display());
    }

    if report.outcome == TestOutcome::Failed {
        std::process::exit(1);
    }
}
