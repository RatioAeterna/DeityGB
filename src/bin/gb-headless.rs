use deitygb::headless::{
    default_boot_rom_path_for_rom, load_file, write_ppm, GameBoy, HardwareModel, TestOutcome,
    DMG_CPU_FREQUENCY, DMG_FRAME_CYCLES,
};
use deitygb::mmu::JoypadButton;
use std::env;
use std::path::PathBuf;

fn parse_button_event(value: &str) -> (u64, JoypadButton, u64) {
    let (event, frames) = value
        .split_once('/')
        .map_or((value, None), |(event, frames)| {
            (
                event,
                Some(
                    frames
                        .parse::<u64>()
                        .expect("tap length must be a frame count"),
                ),
            )
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
    let second = second
        .parse()
        .expect("button event second must be an integer");
    let duration = frames.map_or(DMG_CPU_FREQUENCY, |frames| frames * DMG_FRAME_CYCLES);
    (second, button, duration)
}

fn main() {
    let mut args = env::args().skip(1);
    let rom_path = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: gb-headless <rom.gb> [--seconds N] [--boot path] [--dump-frame path.ppm] [--press BUTTON@SECOND] [--no-apu] [--dmg] [--model dmg0|dmgabc|mgb|sgb|sgb2] [--trace]");
            std::process::exit(2);
        }
    };

    let mut seconds = 20u64;
    let mut boot_path = None;
    let mut load_boot = true;
    let mut dump_frame = None;
    let mut input_events = Vec::new();
    let mut apu_enabled = true;
    let mut force_dmg = false;
    let mut hardware_model = None;
    let mut trace = false;

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
                load_boot = false;
                boot_path = None;
            }
            "--dump-frame" => {
                dump_frame = Some(PathBuf::from(
                    args.next().expect("--dump-frame requires a path"),
                ));
            }
            "--press-start-at" => {
                let value = args.next().expect("--press-start-at requires a value");
                let second = value
                    .parse::<u64>()
                    .expect("--press-start-at must be an integer");
                input_events.push((second, JoypadButton::Start, DMG_CPU_FREQUENCY));
            }
            "--press" => {
                let value = args.next().expect("--press requires BUTTON@SECOND");
                input_events.push(parse_button_event(&value));
            }
            "--no-apu" => {
                apu_enabled = false;
            }
            "--dmg" => {
                force_dmg = true;
            }
            "--model" => {
                let value = args.next().expect("--model requires a value");
                hardware_model = Some(
                    HardwareModel::parse(&value)
                        .unwrap_or_else(|| panic!("unknown hardware model: {}", value)),
                );
                load_boot = false;
                boot_path = None;
            }
            "--trace" => {
                trace = true;
            }
            _ => {
                eprintln!("unknown argument: {}", arg);
                std::process::exit(2);
            }
        }
    }

    let mut gb = GameBoy::new();
    gb.set_apu_enabled(apu_enabled);
    gb.set_force_dmg(force_dmg);
    gb.cpu.set_trace_enabled(trace);
    let save_report = gb
        .load_rom_from_path(&rom_path)
        .expect("failed to read ROM");
    if let Some(model) = hardware_model {
        gb.apply_hardware_model_post_boot(model);
    }
    if load_boot && boot_path.is_none() {
        let rom = load_file(&rom_path).expect("failed to reread ROM for boot ROM selection");
        boot_path = Some(default_boot_rom_path_for_rom(&rom));
    }
    if load_boot {
        if let Some(path) = boot_path {
            let boot = load_file(&path).expect("failed to read boot ROM");
            gb.load_boot_rom(&boot);
        }
    }
    if save_report.enabled {
        if let Some(path) = &save_report.save_path {
            println!("save-path: {}", path.display());
        }
        if let Some(path) = &save_report.rtc_path {
            println!("rtc-path: {}", path.display());
        }
    }
    for message in &save_report.messages {
        println!("save: {}", message);
    }

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
    if let Some(text) = gb.blargg_memory_text() {
        println!("blargg-memory-status: {:#04x}", gb.mmu.get_byte(0xA000));
        println!("blargg-memory: {}", text.escape_default());
    }
    let pc = gb.cpu.program_counter();
    println!(
        "state: bank={:#04x} pc={:#06x} sp={:#06x} bc={:#06x} de={:#06x} hl={:#06x} halted={} ime={} cgb={} double_speed={} key1={:#04x} lcdc={:#04x} stat={:#04x} ly={} lyc={} ie={:#04x} if={:#04x} div={:#06x} tima={:#04x} tma={:#04x} tac={:#04x} joyp={:#04x} ff80={:#04x} ff81={:#04x} ff82={:#04x} ff83={:#04x} ff84={:#04x} ff85={:#04x} ff86={:#04x} ff87={:#04x} ff88={:#04x} ff89={:#04x} ff8a={:#04x} ff8b={:#04x} ff8c={:#04x} ff8d={:#04x} ff8e={:#04x} ff8f={:#04x} ff90={:#04x} ff94={:#04x} d03b={:#04x} bgp={:#04x} scx={} scy={}",
        gb.mmu.mapped_rom_bank(pc),
        pc,
        gb.cpu.stack_pointer(),
        gb.cpu.bc(),
        gb.cpu.de(),
        gb.cpu.hl(),
        gb.cpu.is_halted(),
        gb.cpu.interrupts_enabled(),
        gb.mmu.cgb_mode(),
        gb.mmu.double_speed(),
        gb.mmu.get_byte(0xFF4D),
        gb.mmu.get_byte(0xFF40),
        gb.mmu.get_byte(0xFF41),
        gb.mmu.get_byte(0xFF44),
        gb.mmu.get_byte(0xFF45),
        gb.mmu.get_ie(),
        gb.mmu.get_if(),
        gb.mmu.fetch_div(),
        gb.mmu.get_byte(0xFF05),
        gb.mmu.get_byte(0xFF06),
        gb.mmu.get_byte(0xFF07),
        gb.mmu.get_byte(0xFF00),
        gb.mmu.get_byte(0xFF80),
        gb.mmu.get_byte(0xFF81),
        gb.mmu.get_byte(0xFF82),
        gb.mmu.get_byte(0xFF83),
        gb.mmu.get_byte(0xFF84),
        gb.mmu.get_byte(0xFF85),
        gb.mmu.get_byte(0xFF86),
        gb.mmu.get_byte(0xFF87),
        gb.mmu.get_byte(0xFF88),
        gb.mmu.get_byte(0xFF89),
        gb.mmu.get_byte(0xFF8A),
        gb.mmu.get_byte(0xFF8B),
        gb.mmu.get_byte(0xFF8C),
        gb.mmu.get_byte(0xFF8D),
        gb.mmu.get_byte(0xFF8E),
        gb.mmu.get_byte(0xFF8F),
        gb.mmu.get_byte(0xFF90),
        gb.mmu.get_byte(0xFF94),
        gb.mmu.get_byte(0xD03B),
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
    println!(
        "apu: nr10-nr14={:02x?} nr21-nr24={:02x?} nr30-nr34={:02x?} nr41-nr44={:02x?} nr50={:#04x} nr51={:#04x} nr52={:#04x} pcm12={:#04x} pcm34={:#04x}",
        &gb.mmu.memory[0xFF10..=0xFF14],
        &gb.mmu.memory[0xFF16..=0xFF19],
        &gb.mmu.memory[0xFF1A..=0xFF1E],
        &gb.mmu.memory[0xFF20..=0xFF23],
        gb.mmu.get_raw_byte(0xFF24),
        gb.mmu.get_raw_byte(0xFF25),
        gb.mmu.get_raw_byte(0xFF26),
        gb.mmu.pcm12,
        gb.mmu.pcm34,
    );
    if let Some(path) = dump_frame {
        let rgba = gb.framebuffer_rgba();
        write_ppm(&path, &rgba).expect("failed to write frame dump");
        println!("frame: {}", path.display());
    }

    match gb.flush_save_if_dirty() {
        Ok(true) => println!("save: flushed dirty cartridge persistence"),
        Ok(false) => {}
        Err(error) => eprintln!("save: failed to flush cartridge persistence: {}", error),
    }

    if report.outcome == TestOutcome::Failed {
        std::process::exit(1);
    }
}
