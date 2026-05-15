// TTS engine abstraction.
//
// Active engine is chosen from `Config::engine`:
//   "edge"   — Microsoft Edge Neural TTS (default, natural voices, online)
//   "native" — platform native (offline fallback: SAPI/WinRT on Windows, `say` on macOS)

pub mod edge;
pub mod kokoro;
pub mod native;

use crate::overlay::SharedOverlay;
use std::sync::mpsc::Sender;

/// Commands sent from the app worker to the TTS thread.
pub enum TtsCommand {
    Speak { text: String },
    Stop,
    SetVoice(String),
    SetSpeed(f32),
    Quit,
}

/// Spawn the TTS worker thread for the named engine.
pub fn spawn(
    engine: &str,
    initial_voice: String,
    initial_speed: f32,
    overlay: SharedOverlay,
) -> Sender<TtsCommand> {
    let (tx, rx) = std::sync::mpsc::channel::<TtsCommand>();

    match engine {
        "native" => {
            std::thread::spawn(move || {
                native::run(rx, initial_voice, initial_speed, overlay);
            });
        }
        _ => {
            std::thread::spawn(move || {
                edge::run(rx, initial_voice, initial_speed, overlay);
            });
        }
    }

    tx
}

pub fn list_voices_for(engine: &str) -> Vec<String> {
    match engine {
        "native" => native::list_voices(),
        _ => edge::list_voices(),
    }
}
