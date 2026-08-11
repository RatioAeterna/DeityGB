use deitygb::apu::APU;
use deitygb::cartridge_save::CartridgeSave;
use deitygb::cpu::CPU;
use deitygb::headless::{
    default_boot_rom_path, load_file, GameBoy, TestOutcome, DMG_CPU_FREQUENCY, DMG_FRAME_CYCLES,
};
use deitygb::mmu::{JoypadButton, MMU};
use deitygb::ppu::{Sprite, PPU};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn cgb_test_rom() -> Vec<u8> {
    let mut rom = vec![0; 0x8000];
    rom[0x0143] = 0x80;
    rom
}

fn temp_rom_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("deitygb-save-test-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn mbc3_battery_ram_rom() -> Vec<u8> {
    let mut rom = vec![0; 0x8000];
    rom[0x0147] = 0x10;
    rom[0x0148] = 0x00;
    rom[0x0149] = 0x03;
    rom
}

fn mbc3_crystal_battery_ram_rom() -> Vec<u8> {
    let mut rom = mbc3_battery_ram_rom();
    rom[0x0149] = 0x05;
    rom
}

#[test]
fn serial_transfer_records_internal_clock_byte() {
    let mut mmu = MMU::new();
    let mut cpu = CPU::new();
    mmu.set_byte(0xFF01, b'P');
    mmu.set_byte(0xFF02, 0x81);

    assert!(mmu.serial_output().is_empty());
    assert_ne!(mmu.get_byte(0xFF02) & 0x80, 0);
    cpu.update_timers(255, &mut mmu);
    for _ in 0..15 {
        cpu.update_timers(255, &mut mmu);
    }
    cpu.update_timers(16, &mut mmu);

    assert_eq!(mmu.serial_output(), b"P");
    assert_eq!(mmu.get_byte(0xFF01), 0xFF);
    assert_eq!(mmu.get_byte(0xFF02) & 0x80, 0);
    assert_eq!(mmu.get_if() & 0b0000_1000, 0b0000_1000);
}

#[test]
fn headless_framebuffer_is_rgba_sized() {
    let mut gb = GameBoy::new();
    assert_eq!(gb.framebuffer_rgba().len(), 160 * 144 * 4);
}

#[test]
fn headless_decodes_blargg_memory_protocol() {
    let mut gb = GameBoy::new();
    gb.mmu.set_raw_byte(0xA000, 0);
    gb.mmu.set_raw_byte(0xA001, 0xDE);
    gb.mmu.set_raw_byte(0xA002, 0xB0);
    gb.mmu.set_raw_byte(0xA003, 0x61);
    for (offset, byte) in b"Passed\0".iter().enumerate() {
        gb.mmu.set_raw_byte(0xA004 + offset, *byte);
    }

    assert_eq!(gb.blargg_memory_text().as_deref(), Some("Passed"));
}

#[test]
fn apu_trigger_and_dac_control_channel_status() {
    let (sender, _receiver) = mpsc::channel();
    let mut apu = APU::new(sender);
    let mut mmu = MMU::new();
    mmu.set_byte(0xFF26, 0x80);
    mmu.set_byte(0xFF11, 0x80);
    mmu.set_byte(0xFF12, 0xF0);
    mmu.set_byte(0xFF13, 0xFF);
    mmu.set_byte(0xFF14, 0x87);
    apu.cycle(4, &mut mmu);
    assert_ne!(mmu.get_byte(0xFF26) & 1, 0);

    mmu.set_byte(0xFF12, 0);
    apu.cycle(4, &mut mmu);
    assert_eq!(mmu.get_byte(0xFF26) & 1, 0);
}

#[test]
fn cgb_vram_and_wram_banks_are_isolated() {
    let mut mmu = MMU::new();
    mmu.load_rom(&cgb_test_rom());

    mmu.set_byte(0x8000, 0x11);
    mmu.set_byte(0xFF4F, 1);
    mmu.set_byte(0x8000, 0x22);
    assert_eq!(mmu.get_byte(0x8000), 0x22);
    assert_eq!(mmu.read_vram_bank(0, 0x8000), 0x11);

    mmu.set_byte(0xD000, 0x33);
    mmu.set_byte(0xFF70, 0);
    assert_eq!(mmu.get_byte(0xD000), 0x33);
    assert_eq!(mmu.get_byte(0xFF70) & 0x07, 1);
    mmu.set_byte(0xFF70, 2);
    mmu.set_byte(0xD000, 0x44);
    assert_eq!(mmu.get_byte(0xD000), 0x44);
    mmu.set_byte(0xFF70, 1);
    assert_eq!(mmu.get_byte(0xD000), 0x33);
}

#[test]
fn cgb_palette_ports_auto_increment_and_decode_rgb555() {
    let mut mmu = MMU::new();
    mmu.load_rom(&cgb_test_rom());

    mmu.set_byte(0xFF68, 0x80 | 10);
    mmu.set_byte(0xFF69, 0x1F);
    mmu.set_byte(0xFF69, 0x00);

    assert_eq!(mmu.get_byte(0xFF68) & 0x3F, 12);
    assert_eq!(mmu.cgb_bg_color(1, 1), 0x001F);
}

#[test]
fn cgb_general_dma_copies_to_selected_vram_bank() {
    let mut mmu = MMU::new();
    mmu.load_rom(&cgb_test_rom());
    for offset in 0..0x10 {
        mmu.set_byte(0xC100 + offset, offset as u8 ^ 0xA5);
    }
    mmu.set_byte(0xFF4F, 1);
    mmu.set_byte(0xFF51, 0xC1);
    mmu.set_byte(0xFF52, 0x00);
    mmu.set_byte(0xFF53, 0x00);
    mmu.set_byte(0xFF54, 0x00);
    mmu.set_byte(0xFF55, 0x00);

    for offset in 0..0x10 {
        assert_eq!(mmu.read_vram_bank(1, 0x8000 + offset), offset as u8 ^ 0xA5);
    }
    assert_eq!(mmu.get_byte(0xFF55), 0xFF);
}

#[test]
fn cgb_boot_handoff_and_stop_switch_to_double_speed() {
    let mut rom = cgb_test_rom();
    rom[0] = 0x10; // STOP
    rom[1] = 0x00; // STOP padding byte
    let mut mmu = MMU::new();
    let mut cpu = CPU::new();
    mmu.load_rom(&rom);
    mmu.set_raw_byte(0xFF50, 1);
    mmu.set_byte(0xFF4D, 1);

    assert_eq!(cpu.cycle(&mut mmu), 4);
    assert_eq!(cpu.accumulator(), 0x11);
    assert!(mmu.double_speed());
    assert_eq!(mmu.get_byte(0xFF4D) & 0x81, 0x80);
    assert!(cpu.cycle(&mut mmu) > 0);
}

#[test]
fn cgb_ppu_uses_tile_attributes_and_color_palette() {
    let mut mmu = MMU::new();
    let mut ppu = PPU::new();
    mmu.load_rom(&cgb_test_rom());
    mmu.set_raw_byte(0xFF40, 0x91);
    mmu.set_raw_byte(0xFF44, 0);

    mmu.set_byte(0xFF4F, 0);
    mmu.set_byte(0x9800, 0);
    mmu.set_byte(0x8000, 0x80);
    mmu.set_byte(0x8001, 0x00);
    mmu.set_byte(0xFF4F, 1);
    mmu.set_byte(0x9800, 0x01); // Palette 1.
    mmu.set_byte(0xFF68, 0x80 | 10); // Palette 1, color 1.
    mmu.set_byte(0xFF69, 0x1F);
    mmu.set_byte(0xFF69, 0x00);

    ppu.render_line(&mut mmu);
    assert_eq!(&ppu.get_rgba_buffer()[0..4], &[0xFF, 0x00, 0x00, 0xFF]);
}

#[test]
fn boot_rom_cannot_be_remapped_without_reset() {
    let mut mmu = MMU::new();

    mmu.set_byte(0xFF50, 1);
    assert_eq!(mmu.get_boot(), 10);

    mmu.set_byte(0xFF50, 0);
    assert_eq!(mmu.get_boot(), 10);
}

#[test]
fn cgb_boot_rom_maps_extended_boot_window() {
    let mut mmu = MMU::new();
    let mut rom = cgb_test_rom();
    rom[0x0200] = 0x44;
    mmu.load_rom(&rom);
    let mut boot = vec![0; 0x0900];
    boot[0x0000] = 0x11;
    boot[0x0200] = 0x22;
    boot[0x08FF] = 0x33;
    mmu.load_boot_rom(&boot);

    assert_eq!(mmu.get_byte(0x0000), 0x11);
    assert_eq!(mmu.get_byte(0x0200), 0x22);
    assert_eq!(mmu.get_byte(0x08FF), 0x33);

    mmu.set_byte(0xFF50, 1);
    assert_eq!(mmu.get_byte(0x0200), 0x44);
}

#[test]
fn offscreen_sprite_coordinates_do_not_overflow() {
    let mut mmu = MMU::new();
    let mut ppu = PPU::new();
    mmu.set_raw_byte(0xFF40, 0x02);
    mmu.set_raw_byte(0xFF44, 0);

    assert!(ppu.sprite_on_scanline(
        &Sprite {
            y: 9,
            ..Sprite::default()
        },
        &mut mmu
    ));
    assert!(!ppu.sprite_on_scanline(
        &Sprite {
            y: 0,
            ..Sprite::default()
        },
        &mut mmu
    ));
}

#[test]
fn disabling_lcd_resets_scanline_and_mode() {
    let mut mmu = MMU::new();
    let mut ppu = PPU::new();
    mmu.set_raw_byte(0xFF40, 0);
    mmu.set_raw_byte(0xFF41, 0x03);
    mmu.set_raw_byte(0xFF44, 42);

    ppu.cycle(4, &mut mmu);

    assert_eq!(mmu.get_byte(0xFF44), 0);
    assert_eq!(mmu.get_byte(0xFF41) & 0x03, 0);
}

#[test]
fn joypad_selects_buttons_and_requests_interrupt() {
    let mut mmu = MMU::new();
    mmu.set_byte(0xFF00, 0x10);
    mmu.set_joypad_button(JoypadButton::Start, true);

    assert_eq!(mmu.get_byte(0xFF00) & 0x0F, 0x07);
    assert_ne!(mmu.get_if() & 0x10, 0);

    mmu.set_joypad_button(JoypadButton::Start, false);
    assert_eq!(mmu.get_byte(0xFF00) & 0x0F, 0x0F);
}

#[test]
fn halted_cpu_does_not_fetch_cb_prefix_or_advance_pc() {
    let mut mmu = MMU::new();
    let mut cpu = CPU::new();
    mmu.set_raw_byte(0xFF50, 1);
    mmu.set_raw_byte(0x0000, 0x76); // HALT
    mmu.set_raw_byte(0x0001, 0xCB); // Kirby waits with HALT; BIT 6,(HL).
    mmu.set_raw_byte(0x0002, 0x76);

    assert_eq!(cpu.cycle(&mut mmu), 4);
    assert!(cpu.is_halted());
    assert_eq!(cpu.program_counter(), 0x0001);

    for _ in 0..8 {
        assert_eq!(cpu.cycle(&mut mmu), 4);
        assert!(cpu.is_halted());
        assert_eq!(cpu.program_counter(), 0x0001);
    }
}

#[test]
fn interrupt_after_halt_is_serviced_before_cb_prefix_fetch() {
    let mut mmu = MMU::new();
    let mut cpu = CPU::new();
    mmu.set_raw_byte(0xFF50, 1);
    mmu.set_raw_byte(0x0000, 0xFB); // EI
    mmu.set_raw_byte(0x0001, 0x00); // NOP; IME becomes active afterward.
    mmu.set_raw_byte(0x0002, 0x76); // HALT
    mmu.set_raw_byte(0x0003, 0xCB); // Kirby's next instruction begins with CB.
    mmu.set_raw_byte(0x0004, 0x76);

    cpu.cycle(&mut mmu);
    cpu.cycle(&mut mmu);
    cpu.cycle(&mut mmu);
    assert!(cpu.is_halted());
    assert_eq!(cpu.program_counter(), 0x0003);

    mmu.set_raw_byte(0xFFFF, 0x01);
    mmu.set_if(0x01);
    assert_eq!(cpu.cycle(&mut mmu), 20);
    assert_eq!(cpu.program_counter(), 0x0040);
    assert_eq!(mmu.get_raw_byte(0xFFFE), 0x03);
    assert_eq!(mmu.get_raw_byte(0xFFFF), 0x00);
}

#[test]
fn halt_bug_reuses_next_opcode_byte_as_immediate() {
    let mut mmu = MMU::new();
    let mut cpu = CPU::new();
    mmu.set_raw_byte(0xFF50, 1);
    mmu.set_raw_byte(0x0000, 0x76); // HALT with IME clear and interrupt pending.
    mmu.set_raw_byte(0x0001, 0x3E); // LD A,d8; opcode is read twice by the bug.
    mmu.set_raw_byte(0x0002, 0x12);
    mmu.set_raw_byte(0xFFFF, 0x01);
    mmu.set_if(0x01);

    assert_eq!(cpu.cycle(&mut mmu), 4);
    assert!(!cpu.is_halted());
    assert_eq!(cpu.program_counter(), 0x0001);

    assert_eq!(cpu.cycle(&mut mmu), 8);
    assert_eq!(cpu.accumulator(), 0x3E);
    assert_eq!(cpu.program_counter(), 0x0002);
}

#[test]
fn tima_overflow_reloads_and_requests_interrupt_one_machine_cycle_later() {
    let mut mmu = MMU::new();
    let mut cpu = CPU::new();
    mmu.set_raw_byte(0xFF05, 0xFF);
    mmu.set_raw_byte(0xFF06, 0x42);
    mmu.set_raw_byte(0xFF07, 0x05); // Enabled, divider bit 3.
    mmu.div_internal = 0x000F;

    cpu.update_timers(1, &mut mmu);
    assert_eq!(mmu.get_byte(0xFF05), 0x00);
    assert_eq!(mmu.get_if() & 0x04, 0);

    cpu.update_timers(3, &mut mmu);
    assert_eq!(mmu.get_byte(0xFF05), 0x00);
    assert_eq!(mmu.get_if() & 0x04, 0);

    cpu.update_timers(1, &mut mmu);
    assert_eq!(mmu.get_byte(0xFF05), 0x42);
    assert_eq!(mmu.get_if() & 0x04, 0x04);
}

#[test]
fn mbc1_mode_one_keeps_switchable_rom_bank_in_lower_region() {
    let mut rom = vec![0; 64 * 0x4000];
    for bank in 0..64 {
        rom[bank * 0x4000] = bank as u8;
    }
    rom[0x0147] = 0x03;
    rom[0x0148] = 0x05;
    rom[0x0149] = 0x03;

    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    mmu.set_raw_byte(0xFF50, 1);
    mmu.set_byte(0x2000, 1);
    mmu.set_byte(0x4000, 1);

    assert_eq!(mmu.get_byte(0x4000), 33);
    mmu.set_byte(0x6000, 1);
    assert_eq!(mmu.get_byte(0x0000), 32);
    assert_eq!(mmu.get_byte(0x4000), 1);
}

#[test]
fn mbc3_selects_seven_bit_rom_and_ram_banks() {
    let mut rom = vec![0; 64 * 0x4000];
    for bank in 0..64 {
        rom[bank * 0x4000] = bank as u8;
    }
    rom[0x0147] = 0x13;
    rom[0x0148] = 0x05;
    rom[0x0149] = 0x03;

    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    mmu.set_raw_byte(0xFF50, 1);
    mmu.set_byte(0x2000, 33);
    assert_eq!(mmu.get_byte(0x0000), 0);
    assert_eq!(mmu.get_byte(0x4000), 33);

    mmu.set_byte(0x0000, 0x0A);
    mmu.set_byte(0x4000, 1);
    mmu.set_byte(0xA000, 0x5A);
    mmu.set_byte(0x4000, 0);
    assert_eq!(mmu.get_byte(0xA000), 0);
    mmu.set_byte(0x4000, 1);
    assert_eq!(mmu.get_byte(0xA000), 0x5A);
}

#[test]
fn mbc5_selects_nine_bit_rom_and_four_bit_ram_banks() {
    let mut rom = vec![0; 512 * 0x4000];
    rom[0x0147] = 0x1B;
    rom[0x0148] = 0x08;
    rom[0x0149] = 0x03;
    rom[1 * 0x4000] = 0x11;
    rom[257 * 0x4000] = 0x57;

    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    mmu.set_raw_byte(0xFF50, 1);
    assert_eq!(mmu.get_byte(0x4000), 0x11);
    mmu.set_byte(0x2000, 1);
    mmu.set_byte(0x3000, 1);
    assert_eq!(mmu.mapped_rom_bank(0x4000), 257);
    assert_eq!(mmu.get_byte(0x4000), 0x57);

    mmu.set_byte(0x0000, 0x0A);
    mmu.set_byte(0x4000, 3);
    mmu.set_byte(0xA000, 0x5A);
    mmu.set_byte(0x4000, 0);
    assert_eq!(mmu.get_byte(0xA000), 0);
    mmu.set_byte(0x4000, 3);
    assert_eq!(mmu.get_byte(0xA000), 0x5A);
}

#[test]
fn mbc3_rtc_ticks_halts_and_latches() {
    let mut rom = vec![0; 0x8000];
    rom[0x0147] = 0x10;
    rom[0x0149] = 0x03;
    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    mmu.set_byte(0x0000, 0x0A);
    mmu.set_byte(0x4000, 0x08);

    mmu.tick_rtc(DMG_CPU_FREQUENCY);
    assert_eq!(mmu.get_byte(0xA000), 1);
    mmu.set_byte(0x6000, 0);
    mmu.set_byte(0x6000, 1);
    mmu.tick_rtc(DMG_CPU_FREQUENCY);
    assert_eq!(mmu.get_byte(0xA000), 1);

    mmu.set_byte(0x4000, 0x0C);
    mmu.set_byte(0xA000, 0x40);
    mmu.tick_rtc(DMG_CPU_FREQUENCY);
    mmu.set_byte(0x6000, 0);
    mmu.set_byte(0x6000, 1);
    mmu.set_byte(0x4000, 0x08);
    assert_eq!(mmu.get_byte(0xA000), 2);
}

#[test]
fn battery_save_clean_boot_does_not_create_file_until_dirty() {
    let rom_path = temp_rom_path("clean.gbc");
    let rom = mbc3_battery_ram_rom();
    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    let mut save = CartridgeSave::for_rom_path(&rom_path);

    let report = save.load_after_rom_at(&mut mmu, 1_000);
    assert!(report.enabled);
    assert_eq!(save.save_path(), rom_path.with_extension("sav"));
    assert!(!save.save_path().exists());
    assert!(!save.flush_if_dirty_at(&mut mmu, 1_000).unwrap());
    assert!(!save.save_path().exists());
}

#[test]
fn battery_sram_banks_round_trip_with_exact_declared_size() {
    let rom_path = temp_rom_path("silver.gbc");
    let rom = mbc3_battery_ram_rom();
    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    let mut save = CartridgeSave::for_rom_path(&rom_path);
    save.load_after_rom_at(&mut mmu, 1_000);

    mmu.set_byte(0x0000, 0x0A);
    mmu.set_byte(0x4000, 0);
    mmu.set_byte(0xA123, 0x11);
    mmu.set_byte(0x4000, 3);
    mmu.set_byte(0xA123, 0x33);
    assert!(save.flush_if_dirty_at(&mut mmu, 1_001).unwrap());
    assert_eq!(fs::metadata(save.save_path()).unwrap().len(), 4 * 8192);
    assert!(!save.save_path().with_extension("sav.tmp").exists());
    assert!(!save.flush_if_dirty_at(&mut mmu, 1_002).unwrap());

    let mut loaded = MMU::new();
    loaded.load_rom(&rom);
    let mut loaded_save = CartridgeSave::for_rom_path(&rom_path);
    loaded_save.load_after_rom_at(&mut loaded, 1_003);
    loaded.set_byte(0x0000, 0x0A);
    loaded.set_byte(0x4000, 0);
    assert_eq!(loaded.get_byte(0xA123), 0x11);
    loaded.set_byte(0x4000, 3);
    assert_eq!(loaded.get_byte(0xA123), 0x33);
}

#[test]
fn mbc3_crystal_uses_all_eight_sram_banks_before_rtc_registers() {
    let rom_path = temp_rom_path("crystal.gbc");
    let rom = mbc3_crystal_battery_ram_rom();
    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    let mut save = CartridgeSave::for_rom_path(&rom_path);
    save.load_after_rom_at(&mut mmu, 1_000);

    mmu.set_byte(0x0000, 0x0A);
    mmu.set_byte(0x4000, 0);
    mmu.set_byte(0xA456, 0x10);
    mmu.set_byte(0x4000, 7);
    mmu.set_byte(0xA456, 0x77);
    mmu.set_byte(0x4000, 0x08);
    mmu.set_byte(0xA000, 12);

    assert!(save.flush_if_dirty_at(&mut mmu, 1_001).unwrap());
    assert_eq!(fs::metadata(save.save_path()).unwrap().len(), 8 * 8192);
    assert!(save.rtc_path().exists());

    let mut loaded = MMU::new();
    loaded.load_rom(&rom);
    let mut loaded_save = CartridgeSave::for_rom_path(&rom_path);
    loaded_save.load_after_rom_at(&mut loaded, 1_001);
    loaded.set_byte(0x0000, 0x0A);
    loaded.set_byte(0x4000, 0);
    assert_eq!(loaded.get_byte(0xA456), 0x10);
    loaded.set_byte(0x4000, 7);
    assert_eq!(loaded.get_byte(0xA456), 0x77);
    loaded.set_byte(0x4000, 0x08);
    assert_eq!(loaded.get_byte(0xA000), 12);
}

#[test]
fn non_battery_cartridge_never_persists_external_ram() {
    let rom_path = temp_rom_path("volatile.gbc");
    let mut rom = mbc3_battery_ram_rom();
    rom[0x0147] = 0x12;
    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    let mut save = CartridgeSave::for_rom_path(&rom_path);
    let report = save.load_after_rom_at(&mut mmu, 1_000);

    mmu.set_byte(0x0000, 0x0A);
    mmu.set_byte(0xA000, 0x44);
    assert!(!report.enabled);
    assert!(!save.flush_if_dirty_at(&mut mmu, 1_001).unwrap());
    assert!(!save.save_path().exists());
}

#[test]
fn truncated_and_oversized_saves_load_without_changing_declared_flush_size() {
    let rom_path = temp_rom_path("odd-size.gbc");
    let rom = mbc3_battery_ram_rom();

    fs::write(rom_path.with_extension("sav"), [0xAA; 16]).unwrap();
    let mut truncated = MMU::new();
    truncated.load_rom(&rom);
    let mut save = CartridgeSave::for_rom_path(&rom_path);
    save.load_after_rom_at(&mut truncated, 1_000);
    truncated.set_byte(0x0000, 0x0A);
    assert_eq!(truncated.get_byte(0xA000), 0xAA);
    assert_eq!(truncated.get_byte(0xA010), 0x00);

    fs::write(rom_path.with_extension("sav"), vec![0x55; (4 * 8192) + 9]).unwrap();
    let mut oversized = MMU::new();
    oversized.load_rom(&rom);
    let mut oversized_save = CartridgeSave::for_rom_path(&rom_path);
    oversized_save.load_after_rom_at(&mut oversized, 1_000);
    oversized.set_byte(0x0000, 0x0A);
    assert_eq!(oversized.get_byte(0xA000), 0x55);
    oversized.set_byte(0xA000, 0x66);
    oversized_save
        .flush_if_dirty_at(&mut oversized, 1_001)
        .unwrap();
    assert_eq!(
        fs::metadata(oversized_save.save_path()).unwrap().len(),
        4 * 8192
    );
}

#[test]
fn mbc3_rtc_sidecar_round_trips_and_applies_elapsed_host_time() {
    let rom_path = temp_rom_path("clock.gbc");
    let rom = mbc3_battery_ram_rom();
    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    let mut save = CartridgeSave::for_rom_path(&rom_path);
    save.load_after_rom_at(&mut mmu, 1_000);

    mmu.set_byte(0x0000, 0x0A);
    mmu.set_byte(0x4000, 0x08);
    mmu.set_byte(0xA000, 58);
    mmu.set_byte(0x4000, 0x09);
    mmu.set_byte(0xA000, 59);
    mmu.set_byte(0x4000, 0x0A);
    mmu.set_byte(0xA000, 23);
    mmu.set_byte(0x4000, 0x0B);
    mmu.set_byte(0xA000, 0xFF);
    mmu.set_byte(0x4000, 0x0C);
    mmu.set_byte(0xA000, 0x00);
    assert!(save.flush_if_dirty_at(&mut mmu, 1_000).unwrap());
    assert!(save.rtc_path().exists());

    let mut loaded = MMU::new();
    loaded.load_rom(&rom);
    let mut loaded_save = CartridgeSave::for_rom_path(&rom_path);
    loaded_save.load_after_rom_at(&mut loaded, 1_003);
    loaded.set_byte(0x0000, 0x0A);
    loaded.set_byte(0x4000, 0x08);
    assert_eq!(loaded.get_byte(0xA000), 1);
    loaded.set_byte(0x4000, 0x09);
    assert_eq!(loaded.get_byte(0xA000), 0);
    loaded.set_byte(0x4000, 0x0A);
    assert_eq!(loaded.get_byte(0xA000), 0);
    loaded.set_byte(0x4000, 0x0B);
    assert_eq!(loaded.get_byte(0xA000), 0);
    loaded.set_byte(0x4000, 0x0C);
    assert_eq!(loaded.get_byte(0xA000) & 0x01, 1);
}

#[test]
fn mbc3_halted_rtc_sidecar_does_not_advance_during_host_elapsed_time() {
    let rom_path = temp_rom_path("halted-clock.gbc");
    let rom = mbc3_battery_ram_rom();
    let mut mmu = MMU::new();
    mmu.load_rom(&rom);
    let mut save = CartridgeSave::for_rom_path(&rom_path);
    save.load_after_rom_at(&mut mmu, 1_000);
    mmu.set_byte(0x0000, 0x0A);
    mmu.set_byte(0x4000, 0x08);
    mmu.set_byte(0xA000, 10);
    mmu.set_byte(0x4000, 0x0C);
    mmu.set_byte(0xA000, 0x40);
    save.flush_if_dirty_at(&mut mmu, 1_000).unwrap();

    let mut loaded = MMU::new();
    loaded.load_rom(&rom);
    let mut loaded_save = CartridgeSave::for_rom_path(&rom_path);
    loaded_save.load_after_rom_at(&mut loaded, 9_000);
    loaded.set_byte(0x0000, 0x0A);
    loaded.set_byte(0x4000, 0x08);
    assert_eq!(loaded.get_byte(0xA000), 10);
    loaded.set_byte(0x4000, 0x0C);
    assert_eq!(loaded.get_byte(0xA000) & 0x40, 0x40);
}

#[test]
#[ignore = "runs the local DMG Acid2 ROM and checks the complete reference image"]
fn dmg_acid2_matches_reference_layout() {
    let rom = load_file(&repo_path("src/roms/dmg-acid2.gb")).unwrap();
    let boot = load_file(&default_boot_rom_path()).unwrap();
    let mut gb = GameBoy::new();
    gb.set_apu_enabled(false);
    gb.load_boot_rom(&boot);
    gb.load_rom(&rom);

    gb.run_for_cycles(DMG_CPU_FREQUENCY * 8);

    assert_eq!(fnv1a(&gb.framebuffer_rgba()), 0x04ae_9fcd_4a05_02fd);
}

#[test]
#[ignore = "runs the local CGB Acid2 ROM and checks the complete reference image"]
fn cgb_acid2_matches_reference_image() {
    let rom = load_file(&repo_path("src/roms/cgb-acid2.gbc")).unwrap();
    let boot = load_file(&default_boot_rom_path()).unwrap();
    let mut gb = GameBoy::new();
    gb.set_apu_enabled(false);
    gb.load_boot_rom(&boot);
    gb.load_rom(&rom);

    gb.run_for_cycles(DMG_CPU_FREQUENCY * 8);

    assert_eq!(fnv1a(&gb.framebuffer_rgba()), 0x71a4_a863_fe5b_cde0);
}

#[test]
#[ignore = "runs the local Pokemon Red ROM for a deterministic title-to-menu smoke test"]
fn pokemon_red_reaches_new_game_menu() {
    let rom = load_file(&repo_path("src/roms/pokemon_red.gb")).unwrap();
    let boot = load_file(&default_boot_rom_path()).unwrap();
    let mut gb = GameBoy::new();
    gb.set_apu_enabled(false);
    gb.load_boot_rom(&boot);
    gb.load_rom(&rom);

    gb.run_for_cycles(DMG_CPU_FREQUENCY * 35);
    gb.set_button(JoypadButton::Start, true);
    gb.run_for_cycles(DMG_CPU_FREQUENCY);
    gb.set_button(JoypadButton::Start, false);
    gb.run_for_cycles(DMG_CPU_FREQUENCY * 6);

    assert_eq!(fnv1a(&gb.framebuffer_rgba()), 0xd70b_a3bc_7247_de85);
}

#[test]
#[ignore = "runs the local Kirby ROM for a deterministic title-to-gameplay smoke test"]
fn kirby_enters_green_greens_after_stage_intro() {
    let rom = load_file(&repo_path("src/roms/kirby.gb")).unwrap();
    let boot = load_file(&default_boot_rom_path()).unwrap();
    let mut gb = GameBoy::new();
    gb.set_apu_enabled(false);
    gb.load_boot_rom(&boot);
    gb.load_rom(&rom);

    gb.run_for_cycles(DMG_CPU_FREQUENCY * 10);
    gb.set_button(JoypadButton::Start, true);
    gb.run_for_cycles(DMG_CPU_FREQUENCY);
    gb.set_button(JoypadButton::Start, false);
    gb.run_for_cycles(DMG_CPU_FREQUENCY * 7);

    // CGB work also corrected DMG OBJ transparency and behind-BG priority.
    assert_eq!(fnv1a(&gb.framebuffer_rgba()), 0xd5a4_c17f_b316_c4e3);
}

#[test]
#[ignore = "runs the local Pokemon Silver ROM for a deterministic CGB title-screen smoke test"]
fn pokemon_silver_reaches_color_title_screen() {
    let rom = load_file(&repo_path("src/roms/pokemon_silver.gbc")).unwrap();
    let boot = load_file(&default_boot_rom_path()).unwrap();
    let mut gb = GameBoy::new();
    gb.set_apu_enabled(false);
    gb.load_boot_rom(&boot);
    gb.load_rom(&rom);

    gb.run_for_cycles(DMG_CPU_FREQUENCY * 38);
    let rgba = gb.framebuffer_rgba();
    let distinct_colors = rgba
        .chunks_exact(4)
        .map(|pixel| &pixel[..3])
        .collect::<std::collections::HashSet<_>>();
    assert!(distinct_colors.len() >= 3);
    assert_eq!(fnv1a(&rgba), 0x726a_43cc_196d_20cf);
}

#[test]
#[ignore = "runs Link's Awakening DX from title into the opening house scene"]
fn links_awakening_dx_reaches_opening_dialogue() {
    let rom = load_file(&repo_path("src/roms/links_awakening.gbc")).unwrap();
    let boot = load_file(&default_boot_rom_path()).unwrap();
    let mut gb = GameBoy::new();
    gb.load_boot_rom(&boot);
    gb.load_rom(&rom);

    let events = [
        (18, JoypadButton::Start),
        (25, JoypadButton::Start),
        (32, JoypadButton::A),
        (38, JoypadButton::A),
        (45, JoypadButton::Start),
        (52, JoypadButton::A),
    ];
    let mut second = 0;
    for (event_second, button) in events {
        gb.run_for_cycles(DMG_CPU_FREQUENCY * (event_second - second));
        gb.set_button(button, true);
        gb.run_for_cycles(DMG_FRAME_CYCLES * 4);
        gb.set_button(button, false);
        second = event_second;
    }
    gb.run_for_cycles(DMG_CPU_FREQUENCY * (75 - second));

    assert_eq!(fnv1a(&gb.framebuffer_rgba()), 0xa496_46be_eec5_c5f2);
}

#[test]
#[ignore = "runs bundled Blargg CPU instruction ROM; use for emulator regression checks"]
fn blargg_cpu_instrs_completes_with_serial_result() {
    let rom = load_file(&repo_path("src/roms/gb-test-roms/cpu_instrs/cpu_instrs.gb")).unwrap();
    let boot = load_file(&default_boot_rom_path()).unwrap();
    let mut gb = GameBoy::new();
    gb.load_boot_rom(&boot);
    gb.load_rom(&rom);

    let report = gb.run_until(DMG_CPU_FREQUENCY * 120);
    assert_ne!(
        report.outcome,
        TestOutcome::Timeout,
        "serial: {}",
        String::from_utf8_lossy(&report.serial)
    );
}

#[test]
#[ignore = "runs bundled Mooneye acceptance ROM; use for emulator regression checks"]
fn mooneye_daa_reports_pass_or_fail() {
    let rom = load_file(&repo_path(
        "src/roms/mts-20240926-1737-443f6e1/acceptance/instr/daa.gb",
    ))
    .unwrap();
    let boot = load_file(&default_boot_rom_path()).unwrap();
    let mut gb = GameBoy::new();
    gb.load_boot_rom(&boot);
    gb.load_rom(&rom);

    let report = gb.run_until(DMG_CPU_FREQUENCY * 30);
    assert_ne!(
        report.outcome,
        TestOutcome::Timeout,
        "serial: {:?}",
        report.serial
    );
}

#[test]
#[ignore = "runs the bundled Blargg DMG/CGB sound core ROMs"]
fn blargg_sound_core_roms_pass() {
    let cases = [
        ("dmg_sound", "01-registers.gb"),
        ("dmg_sound", "02-len ctr.gb"),
        ("dmg_sound", "03-trigger.gb"),
        ("dmg_sound", "04-sweep.gb"),
        ("dmg_sound", "05-sweep details.gb"),
        ("dmg_sound", "06-overflow on trigger.gb"),
        ("dmg_sound", "07-len sweep period sync.gb"),
        ("dmg_sound", "08-len ctr during power.gb"),
        ("dmg_sound", "09-wave read while on.gb"),
        ("dmg_sound", "10-wave trigger while on.gb"),
        ("dmg_sound", "11-regs after power.gb"),
        ("dmg_sound", "12-wave write while on.gb"),
        ("cgb_sound", "01-registers.gb"),
        ("cgb_sound", "02-len ctr.gb"),
        ("cgb_sound", "03-trigger.gb"),
        ("cgb_sound", "04-sweep.gb"),
        ("cgb_sound", "05-sweep details.gb"),
        ("cgb_sound", "06-overflow on trigger.gb"),
        ("cgb_sound", "07-len sweep period sync.gb"),
        ("cgb_sound", "08-len ctr during power.gb"),
        ("cgb_sound", "09-wave read while on.gb"),
        ("cgb_sound", "10-wave trigger while on.gb"),
        ("cgb_sound", "11-regs after power.gb"),
        ("cgb_sound", "12-wave.gb"),
    ];
    let boot = load_file(&default_boot_rom_path()).unwrap();
    for (suite, name) in cases {
        let rom = load_file(&repo_path(&format!(
            "src/roms/gb-test-roms/{suite}/rom_singles/{name}"
        )))
        .unwrap();
        let mut gb = GameBoy::new();
        gb.load_boot_rom(&boot);
        gb.load_rom(&rom);
        let report = gb.run_until(DMG_CPU_FREQUENCY * 30);
        assert_eq!(
            report.outcome,
            TestOutcome::Passed,
            "{suite}/{name}: {:?}",
            gb.blargg_memory_text()
        );
    }
}
