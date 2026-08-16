use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use deitygb::cartridge_save::CartridgeSave;
use deitygb::headless::{is_cgb_rom, load_file};
#[cfg(not(target_os = "linux"))]
use deitygb::host_audio::SimpleAudio;
use deitygb::mmu::JoypadButton;
use deitygb::{apu, cpu, mmu, ppu};
use macroquad::prelude::*;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use rfd::AsyncFileDialog;
use std::convert::TryInto;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const CYCLES_PER_FRAME: u32 = 70_224;
const GAME_BOY_CLOCK_HZ: f64 = 4_194_304.0;
const GAME_BOY_FRAME_SECONDS: f64 = CYCLES_PER_FRAME as f64 / GAME_BOY_CLOCK_HZ;
const FRAME_PACING_EARLY_TOLERANCE_SECONDS: f64 = 0.001;
const STARTUP_SPLASH_SECONDS: f64 = 1.8;
const FIERCE_DEITY_PNG: &[u8] = include_bytes!("../assets/fierce_deity.png");
const DEITYGB_ICON_PNG: &[u8] = include_bytes!("../assets/deitygb-icon-512.png");
const DMG_BOOT_ROM: &[u8] = include_bytes!("dmg_boot.bin");
const CGB_BOOT_ROM: &[u8] = include_bytes!("cgb_boot.bin");
const SAVE_FLUSH_DEBOUNCE: Duration = Duration::from_secs(1);
const FAST_FORWARD_FRAMES_PER_HOST_FRAME: u32 = 2;
const AUDIO_QUEUE_CAPACITY: usize = 2_048;
const ROM_NAVIGATION_INITIAL_DELAY: f64 = 0.28;
const ROM_NAVIGATION_REPEAT_INTERVAL: f64 = 0.060;
const ROM_NAVIGATION_FAST_AFTER: f64 = 0.9;
const ROM_NAVIGATION_FAST_INTERVAL: f64 = 0.025;
const HELP_LINES: &[(&str, &str)] = &[
    ("W A S D", "D-pad"),
    ("J", "A button"),
    ("K", "B button"),
    ("Enter", "Start"),
    ("Left Shift", "Select"),
    ("Tab", "Fast-forward while held"),
    ("H / F1", "Show or hide controls"),
];

const GB_SCREEN_DIM: u32 = 23040; // 160x144
const SCREEN_UPSCALE_FACTOR: f32 = 5.0; // gameboy screen is super tiny, so we upscale it

fn icon_pixels<const N: usize>(size: u32) -> [u8; N] {
    use image::imageops::FilterType;

    image::load_from_memory(DEITYGB_ICON_PNG)
        .expect("deitygb-icon-512.png should be a valid image")
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
            dataWithBytes: DEITYGB_ICON_PNG.as_ptr()
            length: DEITYGB_ICON_PNG.len()
        ];
        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithData: data];
        let _: () = msg_send![application, setApplicationIconImage: image];
        let _: () = msg_send![image, release];
    }
}

#[cfg(not(target_os = "macos"))]
fn set_macos_application_icon() {}

async fn show_startup_splash(wait_for_enter: bool) -> bool {
    let splash = Texture2D::from_file_with_format(FIERCE_DEITY_PNG, Some(ImageFormat::Png));
    splash.set_filter(FilterMode::Nearest);

    let started_at = get_time();
    loop {
        if is_quit_requested() {
            return false;
        }
        if wait_for_enter && is_key_pressed(KeyCode::Enter) {
            return true;
        }
        if !wait_for_enter && get_time() - started_at >= STARTUP_SPLASH_SECONDS {
            return true;
        }

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
        if wait_for_enter {
            let prompt = "Press Enter to choose a ROM folder";
            let prompt_size = 28.0;
            let width = measure_text(prompt, None, prompt_size as u16, 1.0).width;
            draw_text(
                prompt,
                (screen_width() - width) / 2.0,
                screen_height() - 34.0,
                prompt_size,
                WHITE,
            );
        }
        next_frame().await;
    }
}

fn collect_roms(directory: &Path, roms: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_roms(&path, roms)?;
        } else if file_type.is_file()
            && path.extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("gb") || extension.eq_ignore_ascii_case("gbc")
            })
        {
            roms.push(path);
        }
    }
    Ok(())
}

fn discover_roms(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut roms = Vec::new();
    collect_roms(directory, &mut roms)?;
    roms.sort_by_key(|path| {
        path.strip_prefix(directory)
            .unwrap_or(path)
            .to_string_lossy()
            .to_ascii_lowercase()
    });
    Ok(roms)
}

fn moved_selection(selected: usize, count: usize, delta: isize) -> usize {
    if count == 0 {
        return 0;
    }
    (selected as isize + delta).rem_euclid(count as isize) as usize
}

#[derive(Default)]
struct NavigationRepeat {
    direction: isize,
    held_since: f64,
    next_repeat_at: f64,
}

impl NavigationRepeat {
    fn update(&mut self, now: f64, direction: isize) -> isize {
        if direction == 0 {
            *self = Self::default();
            return 0;
        }

        if direction != self.direction {
            self.direction = direction;
            self.held_since = now;
            self.next_repeat_at = now + ROM_NAVIGATION_INITIAL_DELAY;
            return direction;
        }

        if now < self.next_repeat_at {
            return 0;
        }

        let interval = if now - self.held_since >= ROM_NAVIGATION_FAST_AFTER {
            ROM_NAVIGATION_FAST_INTERVAL
        } else {
            ROM_NAVIGATION_REPEAT_INTERVAL
        };
        let repeats = (((now - self.next_repeat_at) / interval).floor() as isize + 1).min(8);
        self.next_repeat_at += repeats as f64 * interval;
        direction * repeats
    }
}

fn draw_rom_browser(
    directory: &Path,
    roms: &[PathBuf],
    selected: usize,
    loading: bool,
    error: Option<&str>,
) {
    clear_background(Color::new(0.05, 0.07, 0.05, 1.0));
    draw_text(
        "Choose a Game",
        32.0,
        48.0,
        38.0,
        Color::new(0.92, 0.96, 0.78, 1.0),
    );
    draw_text(
        &directory.display().to_string(),
        32.0,
        78.0,
        20.0,
        Color::new(0.70, 0.76, 0.62, 1.0),
    );

    if loading {
        draw_text(
            "Scanning for .gb and .gbc files...",
            32.0,
            132.0,
            24.0,
            Color::new(0.92, 0.96, 0.78, 1.0),
        );
    } else if let Some(message) = error {
        draw_text(message, 32.0, 132.0, 24.0, Color::new(1.0, 0.55, 0.50, 1.0));
    } else if roms.is_empty() {
        draw_text(
            "No .gb or .gbc files were found in this folder.",
            32.0,
            132.0,
            24.0,
            Color::new(1.0, 0.75, 0.48, 1.0),
        );
    } else {
        let row_height = 31.0;
        let available_height = (screen_height() - 155.0).max(row_height);
        let visible_rows = (available_height / row_height) as usize;
        let first = selected
            .saturating_sub(visible_rows / 2)
            .min(roms.len().saturating_sub(visible_rows));
        let last = (first + visible_rows).min(roms.len());
        for (row, path) in roms[first..last].iter().enumerate() {
            let index = first + row;
            let y = 118.0 + row as f32 * row_height;
            if index == selected {
                draw_rectangle(
                    22.0,
                    y - 24.0,
                    screen_width() - 44.0,
                    row_height,
                    Color::new(0.28, 0.36, 0.23, 1.0),
                );
            }
            let label = path
                .strip_prefix(directory)
                .unwrap_or(path)
                .display()
                .to_string();
            draw_text(
                &label,
                32.0,
                y,
                23.0,
                if index == selected {
                    Color::new(0.98, 1.0, 0.86, 1.0)
                } else {
                    Color::new(0.78, 0.84, 0.68, 1.0)
                },
            );
        }
    }

    draw_text(
        "Hold W/S, D-pad or Tab: scroll    J/A or Enter: play    K/B or Esc: folder",
        24.0,
        screen_height() - 20.0,
        19.0,
        Color::new(0.78, 0.84, 0.68, 1.0),
    );
}

async fn choose_rom_from_directory(directory: &Path) -> Option<PathBuf> {
    let (scan_sender, scan_receiver) = mpsc::sync_channel(1);
    let scan_directory = directory.to_path_buf();
    std::thread::spawn(move || {
        let _ = scan_sender.send(discover_roms(&scan_directory));
    });

    let scan_result = loop {
        if is_quit_requested() {
            return None;
        }
        match scan_receiver.try_recv() {
            Ok(result) => break result,
            Err(mpsc::TryRecvError::Empty) => {
                draw_rom_browser(directory, &[], 0, true, None);
                next_frame().await;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                break Err(io::Error::other("ROM scan worker stopped unexpectedly"));
            }
        }
    };

    let (roms, error) = match scan_result {
        Ok(roms) => (roms, None),
        Err(error) => (Vec::new(), Some(format!("Could not read folder: {error}"))),
    };
    let mut selected = 0usize;
    let mut navigation_repeat = NavigationRepeat::default();

    loop {
        if is_quit_requested() {
            return None;
        }
        let up = is_key_down(KeyCode::W) || is_key_down(KeyCode::Up);
        let down =
            is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) || is_key_down(KeyCode::Tab);
        let direction = match (up, down) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        let navigation_delta = navigation_repeat.update(get_time(), direction);
        let page_up = is_key_pressed(KeyCode::A) || is_key_pressed(KeyCode::Left);
        let page_down = is_key_pressed(KeyCode::D) || is_key_pressed(KeyCode::Right);
        if navigation_delta != 0 {
            selected = moved_selection(selected, roms.len(), navigation_delta);
        } else if page_up {
            selected = moved_selection(selected, roms.len(), -10);
        } else if page_down {
            selected = moved_selection(selected, roms.len(), 10);
        }

        if !roms.is_empty() && (is_key_pressed(KeyCode::J) || is_key_pressed(KeyCode::Enter)) {
            return Some(roms[selected].clone());
        }
        if is_key_pressed(KeyCode::K) || is_key_pressed(KeyCode::Escape) {
            return Some(PathBuf::new());
        }

        draw_rom_browser(directory, &roms, selected, false, error.as_deref());
        next_frame().await;
    }
}

fn draw_folder_picker_wait() {
    clear_background(Color::new(0.05, 0.07, 0.05, 1.0));
    let prompt = "Choose a ROM folder in the system dialog";
    let prompt_size = 28.0;
    let width = measure_text(prompt, None, prompt_size as u16, 1.0).width;
    draw_text(
        prompt,
        (screen_width() - width) / 2.0,
        screen_height() / 2.0,
        prompt_size,
        Color::new(0.92, 0.96, 0.78, 1.0),
    );
}

async fn present_startup_stage(message: &str) {
    let started_at = get_time();
    while get_time() - started_at < 0.25 {
        clear_background(Color::new(0.05, 0.07, 0.05, 1.0));
        let size = 28.0;
        let width = measure_text(message, None, size as u16, 1.0).width;
        draw_text(
            message,
            (screen_width() - width) / 2.0,
            screen_height() / 2.0,
            size,
            Color::new(0.92, 0.96, 0.78, 1.0),
        );
        next_frame().await;
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "unknown initialization panic".to_owned()
    }
}

async fn show_startup_error(message: &str) {
    loop {
        if is_quit_requested() || is_key_pressed(KeyCode::Escape) {
            return;
        }
        clear_background(Color::new(0.05, 0.07, 0.05, 1.0));
        draw_text(
            "Could not start this ROM",
            32.0,
            72.0,
            34.0,
            Color::new(1.0, 0.55, 0.50, 1.0),
        );
        draw_text(message, 32.0, 116.0, 22.0, WHITE);
        draw_text("Press Escape to close", 32.0, 158.0, 20.0, GRAY);
        next_frame().await;
    }
}

#[cfg(target_os = "macos")]
async fn pick_rom_directory() -> Option<PathBuf> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = Command::new("/usr/bin/osascript")
            .args([
                "-e",
                "POSIX path of (choose folder with prompt \"Choose your ROM folder\")",
            ])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout);
                    let path = path.trim_end_matches(['\r', '\n']);
                    (!path.is_empty()).then(|| PathBuf::from(path))
                } else {
                    None
                }
            });
        let _ = sender.send(result);
    });

    loop {
        if is_quit_requested() {
            return None;
        }
        match receiver.try_recv() {
            Ok(result) => return result,
            Err(mpsc::TryRecvError::Empty) => {
                draw_folder_picker_wait();
                next_frame().await;
            }
            Err(mpsc::TryRecvError::Disconnected) => return None,
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_start_directory() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(target_os = "linux")]
fn linux_directories(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut directories = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => Some(entry.path()),
            _ => None,
        })
        .collect::<Vec<_>>();
    directories.sort_by_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase()
    });
    Ok(directories)
}

#[cfg(target_os = "linux")]
fn draw_linux_folder_browser(
    directory: &Path,
    directories: &[PathBuf],
    selected: usize,
    loading: bool,
    error: Option<&str>,
) {
    clear_background(Color::new(0.05, 0.07, 0.05, 1.0));
    draw_text(
        "Choose your ROM folder",
        32.0,
        48.0,
        36.0,
        Color::new(0.92, 0.96, 0.78, 1.0),
    );
    draw_text(
        &directory.display().to_string(),
        32.0,
        78.0,
        19.0,
        Color::new(0.70, 0.76, 0.62, 1.0),
    );

    if loading {
        draw_text("Reading folders...", 32.0, 126.0, 24.0, WHITE);
    } else if let Some(message) = error {
        draw_text(message, 32.0, 126.0, 22.0, Color::new(1.0, 0.55, 0.50, 1.0));
    } else {
        let row_height = 31.0;
        let item_count = directories.len() + 1;
        let available_height = (screen_height() - 155.0).max(row_height);
        let visible_rows = (available_height / row_height) as usize;
        let first = selected
            .saturating_sub(visible_rows / 2)
            .min(item_count.saturating_sub(visible_rows));
        let last = (first + visible_rows).min(item_count);

        for index in first..last {
            let row = index - first;
            let y = 118.0 + row as f32 * row_height;
            if index == selected {
                draw_rectangle(
                    22.0,
                    y - 24.0,
                    screen_width() - 44.0,
                    row_height,
                    Color::new(0.28, 0.36, 0.23, 1.0),
                );
            }
            let label = if index == 0 {
                "[ Use this folder ]".to_owned()
            } else {
                format!(
                    "{}/",
                    directories[index - 1]
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            };
            draw_text(
                &label,
                32.0,
                y,
                23.0,
                if index == selected {
                    Color::new(0.98, 1.0, 0.86, 1.0)
                } else {
                    Color::new(0.78, 0.84, 0.68, 1.0)
                },
            );
        }
    }

    draw_text(
        "W/S: move    J or Enter: choose/open    K: parent    Esc: cancel",
        24.0,
        screen_height() - 20.0,
        19.0,
        Color::new(0.78, 0.84, 0.68, 1.0),
    );
}

#[cfg(target_os = "linux")]
async fn read_linux_directories(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let directory = directory.to_path_buf();
    let worker_directory = directory.clone();
    std::thread::spawn(move || {
        let _ = sender.send(linux_directories(&worker_directory));
    });

    loop {
        if is_quit_requested() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "application closed",
            ));
        }
        match receiver.try_recv() {
            Ok(result) => return result,
            Err(mpsc::TryRecvError::Empty) => {
                draw_linux_folder_browser(&directory, &[], 0, true, None);
                next_frame().await;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(io::Error::other("folder scan worker stopped unexpectedly"));
            }
        }
    }
}

#[cfg(target_os = "linux")]
async fn pick_rom_directory() -> Option<PathBuf> {
    let mut current = linux_start_directory();

    'directory: loop {
        let (directories, error) = match read_linux_directories(&current).await {
            Ok(directories) => (directories, None),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => return None,
            Err(error) => (Vec::new(), Some(format!("Could not read folder: {error}"))),
        };
        let item_count = directories.len() + 1;
        let mut selected = 0usize;
        let mut navigation_repeat = NavigationRepeat::default();

        loop {
            if is_quit_requested() || is_key_pressed(KeyCode::Escape) {
                return None;
            }
            let up = is_key_down(KeyCode::W) || is_key_down(KeyCode::Up);
            let down = is_key_down(KeyCode::S) || is_key_down(KeyCode::Down);
            let direction = match (up, down) {
                (true, false) => -1,
                (false, true) => 1,
                _ => 0,
            };
            let navigation_delta = navigation_repeat.update(get_time(), direction);
            if navigation_delta != 0 {
                selected = moved_selection(selected, item_count, navigation_delta);
            } else if is_key_pressed(KeyCode::A) || is_key_pressed(KeyCode::Left) {
                selected = moved_selection(selected, item_count, -10);
            } else if is_key_pressed(KeyCode::D) || is_key_pressed(KeyCode::Right) {
                selected = moved_selection(selected, item_count, 10);
            }

            if is_key_pressed(KeyCode::K) || is_key_pressed(KeyCode::Backspace) {
                if let Some(parent) = current.parent() {
                    current = parent.to_path_buf();
                    continue 'directory;
                }
            }
            if is_key_pressed(KeyCode::J) || is_key_pressed(KeyCode::Enter) {
                if selected == 0 {
                    return Some(current);
                }
                current = directories[selected - 1].clone();
                continue 'directory;
            }

            draw_linux_folder_browser(&current, &directories, selected, false, error.as_deref());
            next_frame().await;
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
async fn pick_rom_directory() -> Option<PathBuf> {
    draw_folder_picker_wait();
    next_frame().await;
    AsyncFileDialog::new()
        .set_title("Choose your ROM folder")
        .pick_folder()
        .await
        .map(|directory| directory.path().to_path_buf())
}

async fn choose_rom_interactively() -> Option<PathBuf> {
    loop {
        let directory = match pick_rom_directory().await {
            Some(directory) => directory,
            None => {
                if !show_startup_splash(true).await {
                    return None;
                }
                continue;
            }
        };
        match choose_rom_from_directory(&directory).await {
            Some(path) if path.as_os_str().is_empty() => continue,
            result => return result,
        }
    }
}

fn host_frame_due(vblank_frame_ready: bool, accumulated_cycles: u32, lcd_enabled: bool) -> bool {
    vblank_frame_ready || (!lcd_enabled && accumulated_cycles >= CYCLES_PER_FRAME)
}

fn host_frames_per_present(fast_forward: bool) -> u32 {
    if fast_forward {
        FAST_FORWARD_FRAMES_PER_HOST_FRAME
    } else {
        1
    }
}

fn help_toggle_requested() -> bool {
    is_key_pressed(KeyCode::H) || is_key_pressed(KeyCode::F1)
}

fn draw_help_overlay() {
    let panel_width = 420.0_f32.min(screen_width() - 32.0);
    let row_height = 30.0;
    let padding = 22.0;
    let title_height = 34.0;
    let panel_height = padding * 2.0 + title_height + row_height * HELP_LINES.len() as f32;
    let x = (screen_width() - panel_width) / 2.0;
    let y = (screen_height() - panel_height) / 2.0;

    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::new(0.0, 0.0, 0.0, 0.35),
    );
    draw_rectangle(
        x,
        y,
        panel_width,
        panel_height,
        Color::new(0.04, 0.05, 0.04, 0.88),
    );
    draw_rectangle_lines(
        x,
        y,
        panel_width,
        panel_height,
        2.0,
        Color::new(0.76, 0.82, 0.63, 1.0),
    );

    draw_text(
        "Controls",
        x + padding,
        y + padding + 22.0,
        28.0,
        Color::new(0.92, 0.96, 0.78, 1.0),
    );

    let mut row_y = y + padding + title_height + 22.0;
    for (control, action) in HELP_LINES {
        draw_text(
            control,
            x + padding,
            row_y,
            22.0,
            Color::new(0.92, 0.96, 0.78, 1.0),
        );
        draw_text(
            action,
            x + 170.0,
            row_y,
            22.0,
            Color::new(0.84, 0.89, 0.72, 1.0),
        );
        row_y += row_height;
    }
}

fn draw_game_frame(screen_texture: &Texture2D, fps_display: &str, show_help: bool) {
    draw_texture_ex(
        screen_texture,
        0.0,
        0.0,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(screen_width(), screen_height())),
            ..Default::default()
        },
    );
    draw_text(fps_display, 10.0, 20.0, 30.0, BLACK);
    draw_text("H/F1 controls", 10.0, screen_height() - 12.0, 20.0, BLACK);

    if show_help {
        draw_help_overlay();
    }
}

fn next_frame_deadline(previous_deadline: f64, now: f64) -> f64 {
    let next = previous_deadline + GAME_BOY_FRAME_SECONDS;
    if now - next > GAME_BOY_FRAME_SECONDS * 2.0 {
        now + GAME_BOY_FRAME_SECONDS
    } else {
        next
    }
}

#[cfg(target_os = "linux")]
struct FrontendAudio {
    child: std::process::Child,
    _writer: std::thread::JoinHandle<()>,
}

#[cfg(target_os = "linux")]
impl Drop for FrontendAudio {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[cfg(target_os = "linux")]
fn start_frontend_audio() -> Result<(FrontendAudio, mpsc::SyncSender<(f32, f32)>, u32), String> {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;

    let helper = env::current_exe()
        .map_err(|error| format!("could not locate application executable: {error}"))?
        .with_file_name("deitygb-audio");
    let mut child = Command::new(&helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("could not launch {}: {error}", helper.display()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "audio helper did not expose stdin".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "audio helper did not expose stdout".to_owned())?;
    let mut reader = BufReader::new(stdout);
    let mut handshake = String::new();
    if reader
        .read_line(&mut handshake)
        .map_err(|error| format!("could not read audio helper handshake: {error}"))?
        == 0
    {
        let _ = child.wait();
        return Err("audio helper exited during initialization".to_owned());
    }
    let sample_rate = handshake
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("audio helper returned an invalid sample rate: {error}"))?;

    let (sender, receiver) = mpsc::sync_channel::<(f32, f32)>(AUDIO_QUEUE_CAPACITY);
    let writer = std::thread::spawn(move || {
        let mut stdin = stdin;
        while let Ok((left, right)) = receiver.recv() {
            let mut frame = [0u8; 8];
            frame[0..4].copy_from_slice(&left.to_le_bytes());
            frame[4..8].copy_from_slice(&right.to_le_bytes());
            if stdin.write_all(&frame).is_err() {
                break;
            }
        }
    });

    Ok((
        FrontendAudio {
            child,
            _writer: writer,
        },
        sender,
        sample_rate,
    ))
}

#[cfg(not(target_os = "linux"))]
type FrontendAudio = SimpleAudio;

#[cfg(not(target_os = "linux"))]
fn start_frontend_audio() -> Result<(FrontendAudio, mpsc::SyncSender<(f32, f32)>, u32), String> {
    SimpleAudio::new()
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
    let apu_enabled = !args.iter().any(|arg| arg == "--no-apu");
    let host_audio_enabled = apu_enabled && option_env!("DEITYGB_DISABLE_HOST_AUDIO").is_none();
    let capture_lcd = args.iter().any(|arg| arg == "--capture-lcd");
    let choose_rom_immediately = args.iter().any(|arg| arg == "--choose-rom");
    let command_line_rom = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with("--"))
        .map(PathBuf::from);

    set_macos_application_icon();
    if !choose_rom_immediately && !show_startup_splash(command_line_rom.is_none()).await {
        return;
    }

    // Allocate the gameplay texture before opening a native chooser. Some Linux
    // compositors recreate or disturb the focused GL drawable while a helper
    // dialog owns focus; no GPU resource creation is needed after it returns.
    let mut screen_image = Image {
        bytes: vec![0; GB_SCREEN_DIM as usize * 4],
        width: 160,
        height: 144,
    };
    let screen_texture = Texture2D::from_rgba8(160, 144, &screen_image.bytes);
    screen_texture.set_filter(FilterMode::Nearest);

    let rom_path = match command_line_rom {
        Some(path) => path,
        None => match choose_rom_interactively().await {
            Some(path) => path,
            None => return,
        },
    };

    present_startup_stage("Loading selected ROM...").await;
    let cartridge_byte_buffer = match load_file(&rom_path) {
        Ok(bytes) if bytes.len() >= 0x8000 => bytes,
        Ok(bytes) => {
            show_startup_error(&format!(
                "The file is only {} bytes; a cartridge ROM must be at least 32768 bytes.",
                bytes.len()
            ))
            .await;
            return;
        }
        Err(error) => {
            show_startup_error(&format!("Could not read {}: {error}", rom_path.display())).await;
            return;
        }
    };

    present_startup_stage("Initializing Game Boy hardware...").await;

    let hardware = std::panic::catch_unwind(|| {
        let mut mmu = mmu::MMU::new();
        let cpu = cpu::CPU::new();
        let ppu = ppu::PPU::new();
        mmu.load_rom(&cartridge_byte_buffer);
        let boot_rom = if is_cgb_rom(&cartridge_byte_buffer) {
            CGB_BOOT_ROM.to_vec()
        } else {
            DMG_BOOT_ROM.to_vec()
        };
        mmu.load_boot_rom(&boot_rom);
        let mut cartridge_save = CartridgeSave::for_rom_path(&rom_path);
        let save_report = cartridge_save.load_after_rom(&mut mmu);
        (mmu, cpu, ppu, cartridge_save, save_report)
    });
    let (mut mmu, mut cpu, mut ppu, cartridge_save, save_report) = match hardware {
        Ok(hardware) => hardware,
        Err(payload) => {
            show_startup_error(&format!(
                "Emulator initialization failed: {}",
                panic_message(payload)
            ))
            .await;
            return;
        }
    };
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
    present_startup_stage("Starting audio...").await;
    let (_audio, sender, sample_rate, _silent_receiver) = if host_audio_enabled {
        match start_frontend_audio() {
            Ok((audio, sender, sample_rate)) => (Some(audio), sender, sample_rate, None),
            Err(error) => {
                eprintln!("audio: {error}; continuing without host audio output");
                let (sender, receiver) = mpsc::sync_channel(AUDIO_QUEUE_CAPACITY);
                (None, sender, 48_000, Some(receiver))
            }
        }
    } else {
        let (sender, receiver) = mpsc::sync_channel(AUDIO_QUEUE_CAPACITY);
        (None, sender, 48_000, Some(receiver))
    };
    let mut apu = apu::APU::with_bounded_sample_rate(sender, sample_rate);
    present_startup_stage("Starting game...").await;

    let mut last_fps_check = get_time();
    let mut emulated_frames = 0;
    let mut fps_display = String::new();
    let mut emulated_frames_since_present = 0u32;
    let mut frame_deadline = get_time();

    let mut rendered_yet: bool = false;
    let mut last_lcd_enabled = mmu.get_byte(0xFF40) & 0x80 != 0;
    let mut lcd_transition = 0u32;
    let mut frames_since_lcd_transition = None;
    let mut last_save_flush = Instant::now();
    let mut show_help = false;
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
        let cycles = cpu.cycle_with_ppu(&mut mmu, &mut ppu);
        let peripheral_cycles = mmu.peripheral_cycles(cycles);
        mmu.tick_rtc(u64::from(peripheral_cycles));
        accumulated_cycles = accumulated_cycles.saturating_add(peripheral_cycles as u32);
        //println!("CYCLES: {}, rendered_yet: {}", cycles, rendered_yet);
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
        if host_frame_due(vblank_frame_ready, accumulated_cycles, lcd_enabled) {
            if vblank_frame_ready {
                screen_image.bytes.copy_from_slice(ppu.get_rgba_buffer());
                rendered_yet = true;
            }
            accumulated_cycles = 0;
            emulated_frames_since_present += 1;
            let fast_forward = is_key_down(KeyCode::Tab);
            emulated_frames += 1;
            let should_present =
                emulated_frames_since_present >= host_frames_per_present(fast_forward);
            if !should_present {
                continue;
            }
            emulated_frames_since_present = 0;
            if help_toggle_requested() {
                show_help = !show_help;
            }
            screen_texture.update(&screen_image);

            let now = get_time();
            let elapsed = now - last_fps_check;

            if elapsed >= 1.0 {
                let fps = (emulated_frames as f64 / elapsed) as u32;
                fps_display = if fast_forward {
                    format!("FPS: {} FAST 2x", fps)
                } else {
                    format!("FPS: {}", fps)
                };
                emulated_frames = 0;
                last_fps_check = now;
            }

            draw_game_frame(&screen_texture, &fps_display, show_help);

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

            frame_deadline = next_frame_deadline(frame_deadline, get_time());
            next_frame().await;

            // VSync follows the monitor, not the Game Boy. On a 120 Hz display,
            // one VSync per emulated frame would run the machine at 2x. Keep
            // presenting the most recent texture until the 59.7275 Hz Game Boy
            // frame deadline arrives; no CPU, PPU, timer, or APU time advances
            // in these host-only refreshes.
            while get_time() + FRAME_PACING_EARLY_TOLERANCE_SECONDS < frame_deadline {
                if is_quit_requested() {
                    break;
                }
                handle_input(&mut mmu);
                if help_toggle_requested() {
                    show_help = !show_help;
                }
                draw_game_frame(&screen_texture, &fps_display, show_help);
                next_frame().await;
            }
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
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn host_frame_is_still_due_when_lcd_produces_no_vblank() {
        assert!(!host_frame_due(false, CYCLES_PER_FRAME - 1, false));
        assert!(host_frame_due(false, CYCLES_PER_FRAME, false));
        assert!(host_frame_due(true, 0, true));
    }

    #[test]
    fn enabled_lcd_does_not_present_a_fallback_frame_before_vblank() {
        assert!(!host_frame_due(false, CYCLES_PER_FRAME, true));
        assert!(!host_frame_due(false, CYCLES_PER_FRAME * 2, true));
        assert!(host_frame_due(true, CYCLES_PER_FRAME * 2, true));
    }

    #[test]
    fn tab_fast_forward_presents_every_second_emulated_frame() {
        assert_eq!(host_frames_per_present(false), 1);
        assert_eq!(host_frames_per_present(true), 2);
    }

    #[test]
    fn frame_deadline_uses_game_boy_time_and_discards_large_host_stalls() {
        let expected = CYCLES_PER_FRAME as f64 / GAME_BOY_CLOCK_HZ;
        assert!((next_frame_deadline(10.0, 10.0) - (10.0 + expected)).abs() < f64::EPSILON);

        let resynchronized = next_frame_deadline(10.0, 20.0);
        assert!((resynchronized - (20.0 + expected)).abs() < f64::EPSILON);
    }

    #[test]
    fn rom_discovery_is_recursive_filtered_and_sorted() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("deitygb-rom-browser-{nonce}"));
        let nested = root.join("Color");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("Zelda.GB"), []).unwrap();
        fs::write(nested.join("Pokemon.gbc"), []).unwrap();
        fs::write(root.join("notes.txt"), []).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&root, nested.join("back-to-root")).unwrap();

        let roms = discover_roms(&root).unwrap();
        let relative: Vec<_> = roms
            .iter()
            .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
            .collect();

        assert_eq!(
            relative,
            vec![
                PathBuf::from("Color/Pokemon.gbc"),
                PathBuf::from("Zelda.GB")
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rom_selection_wraps_in_both_directions() {
        assert_eq!(moved_selection(0, 3, -1), 2);
        assert_eq!(moved_selection(2, 3, 1), 0);
        assert_eq!(moved_selection(1, 3, 10), 2);
        assert_eq!(moved_selection(4, 0, 1), 0);
    }

    #[test]
    fn held_rom_navigation_repeats_and_accelerates() {
        let mut repeat = NavigationRepeat::default();

        assert_eq!(repeat.update(10.0, 1), 1);
        assert_eq!(repeat.update(10.20, 1), 0);
        assert_eq!(repeat.update(10.28, 1), 1);
        assert_eq!(repeat.update(10.34, 1), 1);

        assert_eq!(repeat.update(11.0, 1), 8);
        assert_eq!(repeat.update(11.01, -1), -1);
        assert_eq!(repeat.update(11.02, 0), 0);
        assert_eq!(repeat.update(11.03, 1), 1);
    }
}
