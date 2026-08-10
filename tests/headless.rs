use deitygb::cpu::CPU;
use deitygb::headless::{default_boot_rom_path, load_file, GameBoy, TestOutcome, DMG_CPU_FREQUENCY};
use deitygb::mmu::{JoypadButton, MMU};
use deitygb::ppu::{PPU, Sprite};
use std::path::{Path, PathBuf};

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn serial_transfer_records_internal_clock_byte() {
    let mut mmu = MMU::new();
    mmu.set_byte(0xFF01, b'P');
    mmu.set_byte(0xFF02, 0x81);

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
fn boot_rom_cannot_be_remapped_without_reset() {
    let mut mmu = MMU::new();

    mmu.set_byte(0xFF50, 1);
    assert_eq!(mmu.get_boot(), 10);

    mmu.set_byte(0xFF50, 0);
    assert_eq!(mmu.get_boot(), 10);
}

#[test]
fn offscreen_sprite_coordinates_do_not_overflow() {
    let mut mmu = MMU::new();
    let mut ppu = PPU::new();
    mmu.set_raw_byte(0xFF40, 0x02);
    mmu.set_raw_byte(0xFF44, 0);

    assert!(ppu.sprite_on_scanline(&Sprite { y: 9, ..Sprite::default() }, &mut mmu));
    assert!(!ppu.sprite_on_scanline(&Sprite { y: 0, ..Sprite::default() }, &mut mmu));
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

    assert_eq!(fnv1a(&gb.framebuffer_rgba()), 0xe214_9d85_9336_6ec9);
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
    assert_ne!(report.outcome, TestOutcome::Timeout, "serial: {}", String::from_utf8_lossy(&report.serial));
}

#[test]
#[ignore = "runs bundled Mooneye acceptance ROM; use for emulator regression checks"]
fn mooneye_daa_reports_pass_or_fail() {
    let rom = load_file(&repo_path("src/roms/mts-20240926-1737-443f6e1/acceptance/instr/daa.gb")).unwrap();
    let boot = load_file(&default_boot_rom_path()).unwrap();
    let mut gb = GameBoy::new();
    gb.load_boot_rom(&boot);
    gb.load_rom(&rom);

    let report = gb.run_until(DMG_CPU_FREQUENCY * 30);
    assert_ne!(report.outcome, TestOutcome::Timeout, "serial: {:?}", report.serial);
}
