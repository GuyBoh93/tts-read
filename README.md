# TTS Read

Select text anywhere on your computer, press **Alt+R**, hear it read aloud.

Fast, local-feeling, natural-sounding text-to-speech that lives in your system tray. No window, no setup, no API key. Press the hotkey again to stop.

## Features

- **One hotkey, anywhere.** Works in browsers, PDFs, Office, IDEs, terminals — anywhere with text.
- **Natural voices.** Defaults to Microsoft Edge's neural voices (Aria, Jenny, Guy, Emily, and others — US, UK, AU, CA, IE accents).
- **Low latency.** Long passages stream sentence-by-sentence — audio starts in ~300 ms, doesn't wait for the whole selection to synthesise.
- **System-tray app.** No window, no dock icon. Quit / change voice / change speed from the tray menu.
- **Adjustable speed.** 0.5× to 2.0× in 0.1× steps.
- **Offline fallback.** A `native` engine using the OS's built-in TTS (WinRT on Windows, `say` on macOS) is one config edit away.
- **Autostart.** Registers itself to launch on login.

## Install

Grab the latest installer from [Releases](../../releases/latest):

- **Windows:** `tts-read_x.y.z_x64-setup.exe` — runs from `%LOCALAPPDATA%`, no admin needed.
- **macOS:** `tts-read_x.y.z_x64.dmg` — drag to Applications, grant Accessibility permission on first run (System Settings → Privacy & Security → Accessibility) so the global hotkey works.

The installer registers TTS Read to autostart on login. A small lips icon appears in the system tray.

## Usage

| Action | Hotkey |
|---|---|
| Read selected text | `Alt + R` |
| Stop reading | `Alt + R` again |
| Change voice / speed | Tray menu |
| Quit | Tray menu → Quit TTS Read |

Selected text is captured via UI Automation (Windows accessibility API — same mechanism screen readers use). If that fails, it falls back to simulating Ctrl+C and reading the clipboard, restoring the previous clipboard contents afterwards.

## Privacy

By default, TTS Read uses Microsoft Edge's public Read Aloud endpoint to synthesise speech. **Your selected text is sent over the network to Microsoft** to produce the audio. This is the same endpoint Edge browser uses for its built-in Read Aloud feature; no API key or account is required.

If you'd rather keep everything offline, switch to the native engine:

1. Quit TTS Read from the tray.
2. Open the config file:
   - Windows: `%APPDATA%\TTSRead\config.json`
   - macOS: `~/Library/Application Support/TTSRead/config.json`
3. Change `"engine": "edge"` to `"engine": "native"` and save.
4. Relaunch TTS Read.

Native voices are noticeably more robotic but are 100% local.

A local neural engine (Kokoro-82M ONNX) is on the roadmap — see [src/tts/kokoro.rs](src/tts/kokoro.rs).

## Build from source

You need a Rust toolchain (stable, edition 2024+). Then:

**Windows** — produces an NSIS installer in `dist/`:
```cmd
build.bat
```

**macOS** — produces a `.dmg` in `dist/`:
```sh
./build.sh
```

Both scripts install `cargo-packager` if needed and generate the app icon on first run. To just build the release binary without packaging an installer, use `build.bat release` / `./build.sh release`.

## Architecture

```
main thread     — tray icon + menu event loop
worker thread   — global hotkey listener + text capture + orchestration
TTS thread      — owns the active engine, streams audio
overlay thread  — always-on-top status pill (Win32 layered window)
```

Engines live behind a thin `TtsCommand` channel abstraction in [src/tts/mod.rs](src/tts/mod.rs); adding a new one means implementing `run(rx, voice, speed, overlay)` and a `list_voices()` function.

## Platform support

| | Status |
|---|---|
| Windows 10 / 11 | ✅ Full support including overlay |
| macOS 11+ | ✅ Tray + hotkey + TTS; overlay window not yet implemented |
| Linux | Not officially supported (rdev/tray-icon do work — PRs welcome) |

## Contributing

Issues and pull requests are welcome. If you're adding a new TTS engine, the existing `edge.rs` is the cleanest reference.

## License

MIT — see [LICENSE](LICENSE).
