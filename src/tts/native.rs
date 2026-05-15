// Platform native TTS backend.
//
// Windows: tts crate → WinRT SpeechSynthesizer (Microsoft neural voices).
//          Created inside this thread; WinRT COM is MTA-safe.
//
// macOS:   `say` subprocess. Instant start, killed on Stop. Speed = WPM via -r.
//
// Both update the overlay: Reading while audio is playing, Idle otherwise.
// (We don't have a separate Synthesizing state here — native engines have no
//  perceptible synthesis pause.)

use crate::overlay::{OverlayState, SharedOverlay, set_state};
use crate::tts::TtsCommand;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use tracing::{info, warn};

pub fn run(
    rx: Receiver<TtsCommand>,
    initial_voice: String,
    initial_speed: f32,
    overlay: SharedOverlay,
) {
    #[cfg(target_os = "macos")]
    run_macos(rx, initial_voice, initial_speed, overlay);

    #[cfg(not(target_os = "macos"))]
    run_winrt(rx, initial_voice, initial_speed, overlay);
}

// ─── macOS: `say` subprocess ─────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn run_macos(
    rx: Receiver<TtsCommand>,
    initial_voice: String,
    initial_speed: f32,
    overlay: SharedOverlay,
) {
    use std::process::Child;

    let mut current_voice = if initial_voice.is_empty() {
        None
    } else {
        Some(initial_voice)
    };
    let mut current_speed = initial_speed;
    let mut child: Option<Child> = None;

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(TtsCommand::Speak { text }) => {
                stop_child(&mut child);

                let wpm = (175.0 * current_speed).round().clamp(60.0, 600.0) as u32;

                let mut cmd = std::process::Command::new("say");
                if let Some(ref v) = current_voice {
                    cmd.arg("-v").arg(v);
                }
                cmd.arg("-r").arg(wpm.to_string());
                cmd.arg(&text);

                match cmd.spawn() {
                    Ok(c) => {
                        info!("say: speaking {} chars at {} wpm", text.len(), wpm);
                        child = Some(c);
                        set_state(&overlay, OverlayState::Reading);
                    }
                    Err(e) => {
                        warn!("say spawn failed: {e}");
                        set_state(&overlay, OverlayState::Idle);
                    }
                }
            }
            Ok(TtsCommand::Stop) => {
                stop_child(&mut child);
                set_state(&overlay, OverlayState::Idle);
                info!("say: stopped");
            }
            Ok(TtsCommand::SetVoice(v)) => {
                current_voice = if v.is_empty() { None } else { Some(v) };
                info!("voice -> {:?}", current_voice);
            }
            Ok(TtsCommand::SetSpeed(s)) => {
                current_speed = s.clamp(0.5, 3.0);
                info!("speed -> {:.2}x", current_speed);
            }
            Ok(TtsCommand::Quit) => {
                stop_child(&mut child);
                set_state(&overlay, OverlayState::Idle);
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                // Detect when the `say` subprocess has exited.
                if let Some(c) = child.as_mut() {
                    if matches!(c.try_wait(), Ok(Some(_))) {
                        child = None;
                        set_state(&overlay, OverlayState::Idle);
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(target_os = "macos")]
fn stop_child(child: &mut Option<std::process::Child>) {
    if let Some(mut c) = child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

#[cfg(target_os = "macos")]
pub fn list_voices() -> Vec<String> {
    let Ok(out) = std::process::Command::new("say").arg("-v").arg("?").output() else {
        return vec![];
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let name = line.split_whitespace().next()?;
            if name.is_empty() { None } else { Some(name.to_string()) }
        })
        .collect()
}

// ─── Windows / other: tts crate (WinRT / SAPI) ───────────────────────────────

#[cfg(not(target_os = "macos"))]
fn run_winrt(
    rx: Receiver<TtsCommand>,
    initial_voice: String,
    initial_speed: f32,
    overlay: SharedOverlay,
) {
    let mut tts = match tts::Tts::default() {
        Ok(t) => t,
        Err(e) => {
            warn!("TTS init failed: {e}. No speech output.");
            while rx.recv().is_ok() {}
            return;
        }
    };

    if !initial_voice.is_empty() {
        if let Ok(voices) = tts.voices() {
            if let Some(v) = voices.iter().find(|v| v.name() == initial_voice) {
                let _ = tts.set_voice(v);
            }
        }
    }

    let mut current_speed = initial_speed;
    apply_speed(&mut tts, current_speed);

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(TtsCommand::Speak { text }) => {
                apply_speed(&mut tts, current_speed);
                info!("tts: speaking {} chars at {:.2}x", text.len(), current_speed);
                if let Err(e) = tts.speak(&text, true) {
                    warn!("tts speak: {e}");
                    set_state(&overlay, OverlayState::Idle);
                } else {
                    set_state(&overlay, OverlayState::Reading);
                }
            }
            Ok(TtsCommand::Stop) => {
                if let Err(e) = tts.stop() {
                    warn!("tts stop: {e}");
                }
                set_state(&overlay, OverlayState::Idle);
                info!("tts: stopped");
            }
            Ok(TtsCommand::SetVoice(name)) => {
                if let Ok(voices) = tts.voices() {
                    if let Some(v) = voices.iter().find(|v| v.name() == name) {
                        let _ = tts.set_voice(v);
                        info!("voice -> {name}");
                    } else {
                        warn!("voice not found: {name}");
                    }
                }
            }
            Ok(TtsCommand::SetSpeed(s)) => {
                current_speed = s.clamp(0.5, 3.0);
                apply_speed(&mut tts, current_speed);
                info!("speed -> {:.2}x", current_speed);
            }
            Ok(TtsCommand::Quit) => {
                let _ = tts.stop();
                set_state(&overlay, OverlayState::Idle);
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                // tts crate doesn't expose a sync "is speaking" — best-effort:
                // if speech is still in progress, the overlay stays Reading.
                if let Ok(false) = tts.is_speaking() {
                    set_state(&overlay, OverlayState::Idle);
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_speed(tts: &mut tts::Tts, speed: f32) {
    let normal = tts.normal_rate();
    let min = tts.min_rate();
    let max = tts.max_rate();

    let target = if speed >= 1.0 {
        normal + (speed - 1.0) * (max - normal) / 2.0
    } else {
        normal - (1.0 - speed) * (normal - min) * 2.0
    };
    let target = target.clamp(min, max);

    if let Err(e) = tts.set_rate(target) {
        warn!("set_rate({target}): {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn list_voices() -> Vec<String> {
    tts::Tts::default()
        .ok()
        .and_then(|tts| tts.voices().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|v| v.name().to_string())
        .collect()
}
