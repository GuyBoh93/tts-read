// TTS Read — select text anywhere, press Alt+R, hear it spoken aloud.
//
// Thread model:
//   main thread  — tray event loop (Win32 message pump / Cocoa)
//   worker thread — hotkey listener + capture + orchestration (app::worker_loop)
//   TTS thread   — owns the TTS engine, processes Speak/Stop commands
//   overlay thread — Win32 layered window showing Thinking / Reading state

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod autostart;
mod capture;
mod config;
mod hotkey;
mod overlay;
mod tray;
mod tts;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("tts-read {}", env!("CARGO_PKG_VERSION"));

    let cfg = config::Config::load_or_default()?;
    tracing::info!(
        "config: hotkey={} engine={} voice={:?}",
        cfg.hotkey,
        cfg.engine,
        cfg.voice
    );

    if let Err(e) = autostart::sync(cfg.autostart) {
        tracing::warn!("autostart sync failed: {e:#}");
    }

    let voices = tts::list_voices_for(&cfg.engine);
    tracing::info!("{} voices available for engine '{}'", voices.len(), cfg.engine);

    let overlay_shared = overlay::new_shared();
    overlay::spawn(overlay_shared.clone());

    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel();

    let worker_cfg = cfg.clone();
    let worker_overlay = overlay_shared.clone();
    std::thread::spawn(move || {
        if let Err(e) = app::worker_loop(worker_cfg, worker_overlay, ctrl_rx) {
            tracing::error!("worker crashed: {e:#}");
        }
    });

    tray::run_until_quit(voices, &cfg.voice, cfg.speed, ctrl_tx)
}
