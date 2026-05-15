// Microsoft Edge Neural TTS backend with sentence-level pipelining.
//
// For long selections we don't wait for the full text to synthesise — we
// split on sentence boundaries, synthesise the first chunk, start playing
// it, then keep the rodio player queue topped up by synthesising subsequent
// chunks while the user is listening. Time-to-first-audio is the cost of
// one sentence (~200–400 ms), not the cost of the whole selection.
//
// Stop is honoured between chunks (at most one sentence of latency).

use crate::overlay::{OverlayState, SharedOverlay, set_state};
use crate::tts::TtsCommand;
use msedge_tts::tts::SpeechConfig;
use msedge_tts::tts::client::{MSEdgeTTSClient, connect};
use std::collections::VecDeque;
use std::io::Cursor;
use std::net::TcpStream;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};
use tracing::{info, warn};

const DEFAULT_VOICE: &str = "en-IE-EmilyNeural";

/// Minimum chunk length in characters. Anything shorter just gets glued to
/// the next sentence — avoids "Hi." being its own WebSocket round-trip.
const MIN_CHUNK_CHARS: usize = 30;

pub fn list_voices() -> Vec<String> {
    [
        "en-US-AriaNeural",
        "en-US-JennyNeural",
        "en-US-GuyNeural",
        "en-US-DavisNeural",
        "en-US-AmberNeural",
        "en-US-AnaNeural",
        "en-US-MichelleNeural",
        "en-US-AndrewNeural",
        "en-US-EmmaNeural",
        "en-US-BrianNeural",
        "en-GB-LibbyNeural",
        "en-GB-MaisieNeural",
        "en-GB-RyanNeural",
        "en-GB-SoniaNeural",
        "en-AU-NatashaNeural",
        "en-AU-WilliamNeural",
        "en-CA-ClaraNeural",
        "en-CA-LiamNeural",
        "en-IE-EmilyNeural",
        "en-IE-ConnorNeural",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn run(
    rx: Receiver<TtsCommand>,
    initial_voice: String,
    initial_speed: f32,
    overlay: SharedOverlay,
) {
    let mut current_voice = if initial_voice.is_empty() {
        DEFAULT_VOICE.to_string()
    } else {
        initial_voice
    };
    let mut current_speed = initial_speed;

    let Ok(device_sink) = rodio::DeviceSinkBuilder::open_default_sink() else {
        warn!("No audio output device available");
        while rx.recv().is_ok() {}
        return;
    };
    let player = rodio::Player::connect_new(device_sink.mixer());

    let mut client: Option<MSEdgeTTSClient<TcpStream>> = None;
    let mut pending: VecDeque<String> = VecDeque::new();
    let mut playing = false;

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(TtsCommand::Speak { text }) => {
                player.stop();
                pending.clear();
                playing = false;
                set_state(&overlay, OverlayState::Synthesizing);

                pending = split_into_chunks(&text);
                info!("edge: split into {} chunks", pending.len());

                // Synthesise the first chunk inline so audio starts ASAP.
                if let Some(first) = pending.pop_front() {
                    let t = Instant::now();
                    match synthesize(&mut client, &first, &current_voice, current_speed) {
                        Ok(audio) => {
                            info!(
                                "edge: first chunk synth {:?} ({} bytes, {} pending)",
                                t.elapsed(),
                                audio.len(),
                                pending.len()
                            );
                            match rodio::Decoder::new(Cursor::new(audio)) {
                                Ok(source) => {
                                    player.append(source);
                                    set_state(&overlay, OverlayState::Reading);
                                    playing = true;
                                }
                                Err(e) => {
                                    warn!("edge: decode failed: {e}");
                                    set_state(&overlay, OverlayState::Idle);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("edge: synth failed: {e:#}");
                            set_state(&overlay, OverlayState::Idle);
                        }
                    }
                }
            }
            Ok(TtsCommand::Stop) => {
                player.stop();
                pending.clear();
                playing = false;
                set_state(&overlay, OverlayState::Idle);
                info!("edge: stopped");
            }
            Ok(TtsCommand::SetVoice(v)) => {
                current_voice = if v.is_empty() {
                    DEFAULT_VOICE.to_string()
                } else {
                    v
                };
                info!("edge: voice -> {current_voice}");
            }
            Ok(TtsCommand::SetSpeed(s)) => {
                current_speed = s.clamp(0.5, 3.0);
                info!("edge: speed -> {:.2}x", current_speed);
            }
            Ok(TtsCommand::Quit) => {
                player.stop();
                pending.clear();
                set_state(&overlay, OverlayState::Idle);
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                // Keep the player queue topped up while we have chunks to go.
                // `len() < 2` means the queue is one chunk or less — synthesise
                // the next now so playback stays seamless.
                if playing && !pending.is_empty() && player.len() < 2 {
                    if let Some(chunk) = pending.pop_front() {
                        let t = Instant::now();
                        match synthesize(&mut client, &chunk, &current_voice, current_speed) {
                            Ok(audio) => {
                                info!(
                                    "edge: chunk synth {:?} ({} bytes, {} pending)",
                                    t.elapsed(),
                                    audio.len(),
                                    pending.len()
                                );
                                if let Ok(source) = rodio::Decoder::new(Cursor::new(audio)) {
                                    player.append(source);
                                }
                            }
                            Err(e) => warn!("edge: chunk synth failed: {e:#}"),
                        }
                    }
                }

                // Natural end of playback — clear state so the next hotkey
                // press starts a new read instead of stopping a phantom one.
                if playing && pending.is_empty() && player.empty() {
                    playing = false;
                    set_state(&overlay, OverlayState::Idle);
                    info!("edge: playback finished");
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Splits text into sentence-sized chunks. We break after `.`, `?`, `!`,
/// or `\n`, but only commit a chunk once it's at least MIN_CHUNK_CHARS
/// long, so short fragments stay glued to the next sentence.
fn split_into_chunks(text: &str) -> VecDeque<String> {
    let mut chunks: VecDeque<String> = VecDeque::new();
    let mut current = String::new();

    for c in text.chars() {
        current.push(c);
        if matches!(c, '.' | '?' | '!' | '\n') {
            if current.trim().chars().count() >= MIN_CHUNK_CHARS {
                let chunk = current.trim().to_string();
                if !chunk.is_empty() {
                    chunks.push_back(chunk);
                }
                current.clear();
            }
        }
    }

    let tail = current.trim();
    if !tail.is_empty() {
        chunks.push_back(tail.to_string());
    }

    if chunks.is_empty() {
        chunks.push_back(text.to_string());
    }

    chunks
}

fn synthesize(
    client: &mut Option<MSEdgeTTSClient<TcpStream>>,
    text: &str,
    voice: &str,
    speed: f32,
) -> anyhow::Result<Vec<u8>> {
    let rate_pct = ((speed - 1.0) * 100.0).round().clamp(-50.0, 200.0) as i32;
    let config = SpeechConfig {
        voice_name: voice.to_string(),
        audio_format: "audio-24khz-48kbitrate-mono-mp3".to_string(),
        pitch: 0,
        rate: rate_pct,
        volume: 0,
    };

    if let Some(c) = client.as_mut() {
        match c.synthesize(text, &config) {
            Ok(audio) => return Ok(audio.audio_bytes),
            Err(e) => {
                warn!("edge: connection died ({e:?}), reconnecting...");
                *client = None;
            }
        }
    }

    let mut new_client =
        connect().map_err(|e| anyhow::anyhow!("edge connect failed: {e:?}"))?;
    let audio = new_client
        .synthesize(text, &config)
        .map_err(|e| anyhow::anyhow!("edge synthesize failed: {e:?}"))?;
    *client = Some(new_client);
    Ok(audio.audio_bytes)
}
