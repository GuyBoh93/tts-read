# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & run

The `build.bat` (Windows) and `build.sh` (macOS/Linux) wrappers cover every common workflow. Pass one of the modes; default is `installer`.

| Mode | What it does |
|---|---|
| `installer` (default) | Builds release binary + packages `.exe` (Windows) or `.dmg` (macOS) into `dist/` via `cargo-packager` |
| `dev` | `cargo build` + `cargo run` — for iterating locally |
| `release` | Optimized binary only, no installer |
| `clean` | `cargo clean` |

The `installer` mode auto-installs `cargo-packager` if missing and regenerates `assets/icon.png` if absent. On macOS it also syncs `CFBundleVersion` in `macos/Info.plist` from `Cargo.toml` before packaging — the CI workflow does the same. **Cargo.toml is the single source of truth for the version**; don't hand-edit `Info.plist`.

Release flow: tag `v*.*.*` and push — `.github/workflows/release.yml` builds Windows + macOS in parallel and publishes a GitHub Release.

There are no tests. `cargo clippy --release --all-targets` is the closest thing to a check.

## Architecture

Four threads, communicating by channels and one shared `OverlayState`:

```
main thread     — tray icon + menu event loop (tray.rs)
worker thread   — global hotkey listener + text capture + orchestration (app.rs)
TTS thread      — owns the active engine, processes Speak/Stop (tts/*.rs)
overlay thread  — Win32 layered window showing Thinking/Reading state (overlay.rs)
```

`main.rs` wires them: spawns the overlay + worker, then runs the tray on the main thread (required for NSStatusBar on macOS and clean message-pump behaviour on Windows).

**Single source of truth for "is something playing":** the `SharedOverlay` (Arc<Mutex<OverlayState>>). The TTS thread flips it to `Idle` when playback ends naturally, so the worker doesn't need a "wasted" hotkey press to clear a stale flag. When you add a new engine, make sure it manages overlay state correctly — see `edge.rs` for the canonical pattern (Synthesizing → Reading → Idle).

**Engine abstraction:** any TTS engine is a function `run(rx: Receiver<TtsCommand>, voice, speed, overlay)` plus a `list_voices()`. `tts/mod.rs::spawn` selects by `Config::engine` ("edge" default, "native" fallback). Adding Kokoro means filling in `tts/kokoro.rs` and adding a match arm — the rest of the system needs no changes.

**Text capture (capture.rs)** tries two strategies in order:
1. **UI Automation** (Windows only) — reads the focused element's `TextPattern` via COM. Same mechanism screen readers use; works in browsers, Office, IDEs, almost anything with text accessibility.
2. **Clipboard fallback** — simulates Ctrl/Cmd+C, reads clipboard, restores previous contents.

Both must wait for Alt to be released before sending Ctrl+C, otherwise Windows sees a "bare Alt tap" and pops up the menu bar, clearing the selection. See the `wait_for_alt_release` helper.

## Critical non-obvious constraints

- **Do NOT suppress the R key in the hotkey grab callback** ([src/hotkey.rs](src/hotkey.rs)). An earlier version suppressed it, which made the focused app see Alt-down → (nothing) → Alt-up — a "bare Alt tap" — which activates the Windows menu bar and steals focus, clearing the user's selection before capture runs. Let R through; most apps ignore Alt+R, and the tradeoff (e.g. Word's Review ribbon also firing) is acceptable.
- **Edge TTS sends selected text to a Microsoft endpoint** over a public WebSocket (no API key). This is documented in the README's privacy section. If a user wants offline, they switch `engine` to `"native"`.
- **macOS overlay is intentionally stubbed** — `overlay::spawn` on non-Windows is a no-op with a warning log. macOS users get tray + hotkey + TTS but no on-screen status pill. Win32 layered windows don't translate trivially to AppKit; if you implement this, it's a from-scratch Cocoa effort.
- **Voices must not sound robotic.** SAPI/OneCore voices are explicitly rejected as a default. Edge Neural is the floor for voice quality. Kokoro-82M is the planned local upgrade path (see [src/tts/kokoro.rs](src/tts/kokoro.rs) header comment for integration notes).
- **Sentence-level pipelining** in `edge.rs`: synthesise + play the first chunk before later chunks are synthesised. `MIN_CHUNK_CHARS` (30) prevents tiny sentence fragments becoming their own round-trip.

## Architectural reference

This project mirrors the user's `Wisper Free Flow` STT app (at `C:\Scripts\Wisper Free Flow`) — same tray + hotkey + worker + overlay pattern. When in doubt about cross-platform tray/hotkey/window behaviour, that's the prior art.

## Issues log

[docs/issues.md](docs/issues.md) is a running log of non-obvious bugs and platform quirks already encountered. **Check it before debugging anything weird** — there's a good chance the trap you're about to walk into is already documented there. When you hit and fix a new non-obvious issue during a session, append a new dated entry at the top using the format in the file header.
