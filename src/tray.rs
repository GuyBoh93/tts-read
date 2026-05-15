// System tray icon. Runs on the main thread.
//
// Windows: tray-icon's hidden window posts menu events into the thread's
//          message queue; we PeekMessage + dispatch in a tight loop.
//
// macOS:   tao EventLoop pumps the Cocoa run loop on the main thread, which
//          is required for NSStatusBar (tray-icon's macOS backend).

use anyhow::{Context, Result};
use std::sync::mpsc::Sender;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu},
};

#[cfg(windows)]
use std::{mem, time::Duration};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub enum ControlEvent {
    SetVoice(String),
    SetSpeed(f32),
    Quit,
}

struct VoiceEntry {
    id: MenuId,
    name: String,
    item: CheckMenuItem,
}

struct SpeedEntry {
    id: MenuId,
    value: f32,
    item: CheckMenuItem,
}

/// Speed options surfaced in the tray menu. 0.1× increments give fine-grained
/// control without making the submenu unmanageably long.
const SPEED_OPTIONS: &[(f32, &str)] = &[
    (0.5, "0.5× (half)"),
    (0.6, "0.6×"),
    (0.7, "0.7×"),
    (0.8, "0.8×"),
    (0.9, "0.9×"),
    (1.0, "1.0× (normal)"),
    (1.1, "1.1×"),
    (1.2, "1.2×"),
    (1.3, "1.3×"),
    (1.4, "1.4×"),
    (1.5, "1.5×"),
    (1.6, "1.6×"),
    (1.7, "1.7×"),
    (1.8, "1.8×"),
    (1.9, "1.9×"),
    (2.0, "2.0× (double)"),
];

struct Installed {
    tray: TrayIcon,
    quit_id: MenuId,
    voices: Vec<VoiceEntry>,
    speeds: Vec<SpeedEntry>,
}

#[cfg(windows)]
pub fn run_until_quit(
    voices: Vec<String>,
    current_voice: &str,
    current_speed: f32,
    ctrl_tx: Sender<ControlEvent>,
) -> Result<()> {
    let installed = install(voices, current_voice, current_speed)?;
    let _tray_guard = installed.tray;
    let rx = MenuEvent::receiver();

    unsafe {
        let mut msg: MSG = mem::zeroed();
        loop {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            while let Ok(event) = rx.try_recv() {
                if event.id == installed.quit_id {
                    tracing::info!("quit requested from tray");
                    let _ = ctrl_tx.send(ControlEvent::Quit);
                    return Ok(());
                }
                handle_click(&event.id, &installed.voices, &installed.speeds, &ctrl_tx);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(target_os = "macos")]
pub fn run_until_quit(
    voices: Vec<String>,
    current_voice: &str,
    current_speed: f32,
    ctrl_tx: Sender<ControlEvent>,
) -> Result<()> {
    use tao::event::Event;
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};

    let mut builder = EventLoopBuilder::new();
    builder.with_activation_policy(ActivationPolicy::Accessory);
    let event_loop = builder.build();

    let installed = install(voices, current_voice, current_speed)?;
    let _tray_guard = installed.tray;
    let menu_rx = MenuEvent::receiver();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if matches!(event, Event::NewEvents(_)) {
            while let Ok(ev) = menu_rx.try_recv() {
                if ev.id == installed.quit_id {
                    tracing::info!("quit requested from tray");
                    let _ = ctrl_tx.send(ControlEvent::Quit);
                    *control_flow = ControlFlow::Exit;
                } else {
                    handle_click(&ev.id, &installed.voices, &installed.speeds, &ctrl_tx);
                }
            }
        }
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn run_until_quit(
    voices: Vec<String>,
    current_voice: &str,
    current_speed: f32,
    ctrl_tx: Sender<ControlEvent>,
) -> Result<()> {
    let installed = install(voices, current_voice, current_speed)?;
    let _tray_guard = installed.tray;
    let rx = MenuEvent::receiver();

    while let Ok(event) = rx.recv() {
        if event.id == installed.quit_id {
            let _ = ctrl_tx.send(ControlEvent::Quit);
            return Ok(());
        }
        handle_click(&event.id, &installed.voices, &installed.speeds, &ctrl_tx);
    }
    Ok(())
}

fn handle_click(
    id: &MenuId,
    voices: &[VoiceEntry],
    speeds: &[SpeedEntry],
    tx: &Sender<ControlEvent>,
) {
    if let Some(picked) = voices.iter().find(|e| &e.id == id) {
        for e in voices {
            e.item.set_checked(e.id == *id);
        }
        tracing::info!("voice selected: {}", picked.name);
        let _ = tx.send(ControlEvent::SetVoice(picked.name.clone()));
        return;
    }
    if let Some(picked) = speeds.iter().find(|e| &e.id == id) {
        for e in speeds {
            e.item.set_checked(e.id == *id);
        }
        tracing::info!("speed selected: {}x", picked.value);
        let _ = tx.send(ControlEvent::SetSpeed(picked.value));
    }
}

fn install(
    voices: Vec<String>,
    current_voice: &str,
    current_speed: f32,
) -> Result<Installed> {
    let menu = Menu::new();

    let header = MenuItem::new(
        format!("TTS Read v{}", env!("CARGO_PKG_VERSION")),
        false,
        None,
    );
    menu.append(&header).context("header")?;
    menu.append(&PredefinedMenuItem::separator()).context("sep")?;

    // Voice submenu — only built if the backend provides voices.
    let mut voice_entries: Vec<VoiceEntry> = Vec::new();
    if !voices.is_empty() {
        let voice_sub = Submenu::new("Voice", true);
        for name in &voices {
            let checked = name == current_voice
                || (current_voice.is_empty() && voice_entries.is_empty());
            let item = CheckMenuItem::new(name, true, checked, None);
            let id = item.id().clone();
            voice_sub.append(&item).context("voice item")?;
            voice_entries.push(VoiceEntry { id, name: name.clone(), item });
        }
        menu.append(&voice_sub).context("voice submenu")?;
    }

    // Speed submenu — always present.
    let speed_sub = Submenu::new("Speed", true);
    let mut speed_entries: Vec<SpeedEntry> = Vec::new();
    for (value, label) in SPEED_OPTIONS {
        let checked = (current_speed - value).abs() < 0.01;
        let item = CheckMenuItem::new(*label, true, checked, None);
        let id = item.id().clone();
        speed_sub.append(&item).context("speed item")?;
        speed_entries.push(SpeedEntry { id, value: *value, item });
    }
    menu.append(&speed_sub).context("speed submenu")?;
    menu.append(&PredefinedMenuItem::separator()).context("sep")?;

    let hint = MenuItem::new("Hotkey: Alt+R", false, None);
    menu.append(&hint).context("hint")?;
    menu.append(&PredefinedMenuItem::separator()).context("sep")?;

    let quit_item = MenuItem::new("Quit TTS Read", true, None);
    let quit_id = quit_item.id().clone();
    menu.append(&quit_item).context("quit")?;

    let (rgba, w, h) = generate_icon();
    let icon = Icon::from_rgba(rgba, w, h).context("building tray icon")?;

    let tray = TrayIconBuilder::new()
        .with_tooltip("TTS Read — Alt+R to read selected text")
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .build()
        .context("building tray icon")?;

    tracing::info!("tray icon installed");
    Ok(Installed { tray, quit_id, voices: voice_entries, speeds: speed_entries })
}

/// "T" silhouette built from two rounded pill bars — the Whisper FreeFlow
/// logo language, repurposed as a T.
fn generate_icon() -> (Vec<u8>, u32, u32) {
    let pixmap = render_t_logo(64);
    (pixmap.data().to_vec(), 64, 64)
}

/// Draws the "T" mark at the given canvas size. Geometry defined in a 64×64
/// design space, then scaled to fill `size`.
pub fn render_t_logo(size: u32) -> tiny_skia::Pixmap {
    use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

    let mut pixmap = Pixmap::new(size, size).expect("pixmap");
    pixmap.fill(Color::TRANSPARENT);

    let s = size as f32 / 64.0;

    // Cream / off-white, same shade Whisper FreeFlow uses — reads cleanly on
    // both dark and light taskbars at small sizes.
    let mut paint = Paint::default();
    paint.set_color_rgba8(250, 249, 245, 255);
    paint.anti_alias = true;

    // Two pill bars in a 64×64 design space:
    //   horizontal top bar — (x=4, y=6, w=56, h=12)
    //   vertical stem      — (x=26, y=18, w=12, h=44)
    let bars: [(f32, f32, f32, f32); 2] = [
        (4.0, 6.0, 56.0, 12.0),
        (26.0, 18.0, 12.0, 44.0),
    ];

    for (bx, by, bw, bh) in bars {
        let x = bx * s;
        let y = by * s;
        let w = bw * s;
        let h = bh * s;
        let r = (w.min(h)) / 2.0;

        let mut pb = PathBuilder::new();
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
        if let Some(p) = pb.finish() {
            pixmap.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    pixmap
}
