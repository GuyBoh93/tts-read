// Worker thread. Owns the hotkey listener and orchestrates:
//   hotkey press → capture selected text → send to TTS thread.
//
// Stop/start decision uses the shared overlay state as the single source of
// truth — the TTS thread flips it to Idle when playback ends naturally, so
// the next hotkey press starts a new read without needing a "wasted" press
// to clear a stale flag.

use crate::capture;
use crate::config::Config;
use crate::hotkey::{HotkeyEvent, spawn_listener};
use crate::overlay::{self, OverlayState, SharedOverlay};
use crate::tray::ControlEvent;
use crate::tts::{self, TtsCommand};
use anyhow::Result;
use std::sync::mpsc::Receiver;
use std::time::Duration;

pub fn worker_loop(
    mut cfg: Config,
    overlay: SharedOverlay,
    ctrl_rx: Receiver<ControlEvent>,
) -> Result<()> {
    let tts_tx = tts::spawn(&cfg.engine, cfg.voice.clone(), cfg.speed, overlay.clone());

    let events = spawn_listener(&cfg.hotkey)?;
    tracing::info!("ready — press [{}] to read selected text", cfg.hotkey);

    loop {
        // Drain tray control events first.
        while let Ok(ev) = ctrl_rx.try_recv() {
            match ev {
                ControlEvent::SetVoice(name) => {
                    tracing::info!("voice change: {name}");
                    cfg.voice = name.clone();
                    if let Err(e) = cfg.save() {
                        tracing::warn!("saving config: {e:#}");
                    }
                    let _ = tts_tx.send(TtsCommand::SetVoice(name));
                }
                ControlEvent::SetSpeed(speed) => {
                    tracing::info!("speed change: {speed:.2}x");
                    cfg.speed = speed;
                    if let Err(e) = cfg.save() {
                        tracing::warn!("saving config: {e:#}");
                    }
                    let _ = tts_tx.send(TtsCommand::SetSpeed(speed));
                }
                ControlEvent::Quit => {
                    tracing::info!("quit signal received");
                    let _ = tts_tx.send(TtsCommand::Quit);
                    return Ok(());
                }
            }
        }

        match events.recv_timeout(Duration::from_millis(33)) {
            Ok(HotkeyEvent::Triggered) => {
                let is_active = { overlay.lock().state != OverlayState::Idle };
                if is_active {
                    tracing::info!("stopping speech");
                    let _ = tts_tx.send(TtsCommand::Stop);
                    overlay::set_state(&overlay, OverlayState::Idle);
                } else {
                    tracing::info!("hotkey triggered, capturing selection...");
                    match capture::get_selected_text() {
                        Ok(Some(text)) => {
                            tracing::info!("speaking {} chars", text.len());
                            // Mark synthesizing immediately so a second hotkey
                            // press detects active state even before the TTS
                            // thread has picked up the command.
                            overlay::set_state(&overlay, OverlayState::Synthesizing);
                            let _ = tts_tx.send(TtsCommand::Speak { text });
                        }
                        Ok(None) => {
                            tracing::info!("nothing selected");
                        }
                        Err(e) => {
                            tracing::warn!("capture failed: {e:#}");
                        }
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                tracing::error!("hotkey listener disconnected");
                let _ = tts_tx.send(TtsCommand::Quit);
                break;
            }
        }
    }

    Ok(())
}
