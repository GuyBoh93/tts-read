// Kokoro-82M ONNX local neural TTS engine (not yet implemented).
//
// ─── How to integrate ────────────────────────────────────────────────────────
//
// 1. Add to Cargo.toml:
//      ort = "2"           # ONNX Runtime Rust bindings
//      hound = "3"         # WAV writer for streaming audio
//      rodio = { version = "0", default-features = false, features = ["wav"] }
//
// 2. Download model on first run from Hugging Face:
//      https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX
//    Recommended: `kokoro-v1.0-int8.onnx` (~90MB, fast on CPU)
//    Store in: config::app_data_dir()?.join("models/kokoro-v1.0-int8.onnx")
//
// 3. Implement phonemizer (Kokoro uses espeak-ng for G2P):
//      - Ship espeak-ng as a bundled binary or use `espeak-ng` crate
//      - Or use `kokoroxide` crate (https://crates.io/crates/kokoroxide)
//        which wraps both phonemizer and ONNX inference.
//
// 4. Replace `tts::spawn()` in main.rs with `kokoro::spawn()`.
//
// ─── Recommended voices ──────────────────────────────────────────────────────
//
//   "af_heart"   — warm US female, most natural-sounding (MOS 4.2)
//   "bm_george"  — natural British male
//   "af_sky"     — energetic US female
//   "am_michael" — US male
//
// ─── Streaming approach ──────────────────────────────────────────────────────
//
// For minimum latency, split text at sentence boundaries and synthesise each
// sentence independently, streaming the first audio chunk to rodio's Sink
// before the second sentence is synthesised.
//
// Expected first-audio latency: ~100–200ms on a modern laptop CPU with int8.
//
// ─────────────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct KokoroEngine;
