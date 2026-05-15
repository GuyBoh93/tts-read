// Captures the currently selected text.
//
// Two strategies in priority order:
//
// 1. **UI Automation (Windows).** Queries the focused element's TextPattern
//    directly via COM. No clipboard or keyboard simulation — works in
//    browsers, Office, Notepad, VS Code, almost anything that exposes text
//    accessibility. This is what screen readers use.
//
// 2. **Clipboard + simulated Ctrl/Cmd+C.** Used when UIA fails (rare app
//    without TextPattern, or COM init issue). The clipboard is restored
//    afterwards.

use anyhow::Result;
use arboard::Clipboard;
use std::thread::sleep;
use std::time::Duration;

pub fn get_selected_text() -> Result<Option<String>> {
    // ── 1. Try UI Automation (Windows-only fast path) ────────────────────
    #[cfg(windows)]
    {
        match uia::get_selected_text() {
            Ok(Some(t)) => {
                tracing::info!("capture: got {} chars via UIA", t.len());
                return Ok(Some(t));
            }
            Ok(None) => {
                tracing::info!("capture: UIA returned no selection, falling back to clipboard");
            }
            Err(e) => {
                tracing::warn!("capture: UIA failed ({e}), falling back to clipboard");
            }
        }
    }

    // ── 2. Clipboard fallback ────────────────────────────────────────────
    clipboard_capture()
}

fn clipboard_capture() -> Result<Option<String>> {
    let mut board = Clipboard::new()?;
    let saved = board.get_text().ok();

    #[cfg(windows)]
    win::wait_for_alt_release(Duration::from_millis(400));
    #[cfg(not(windows))]
    sleep(Duration::from_millis(80));

    #[cfg(windows)]
    let seq_before = unsafe { win::clipboard_seq() };

    trigger_copy();

    #[cfg(windows)]
    {
        use std::time::Instant;
        let deadline = Instant::now() + Duration::from_millis(600);
        while Instant::now() < deadline {
            if unsafe { win::clipboard_seq() } != seq_before {
                break;
            }
            sleep(Duration::from_millis(20));
        }
    }
    #[cfg(not(windows))]
    sleep(Duration::from_millis(200));

    let captured = board.get_text().ok();

    if let Some(prev) = saved {
        let _ = board.set_text(prev);
    }

    match captured {
        Some(t) if !t.trim().is_empty() => Ok(Some(t.trim().to_string())),
        _ => Ok(None),
    }
}

#[cfg(windows)]
fn trigger_copy() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    unsafe {
        win::send_key(VK_MENU, KEYEVENTF_KEYUP);
        win::send_key(VK_LMENU, KEYEVENTF_KEYUP);
        win::send_key(VK_RMENU, KEYEVENTF_KEYUP);
        sleep(Duration::from_millis(30));

        win::send_key(VK_CONTROL, 0);
        sleep(Duration::from_millis(5));
        win::send_key(0x43, 0); // 'C'
        sleep(Duration::from_millis(30));
        win::send_key(0x43, KEYEVENTF_KEYUP);
        sleep(Duration::from_millis(5));
        win::send_key(VK_CONTROL, KEYEVENTF_KEYUP);
    }
}

#[cfg(not(windows))]
fn trigger_copy() {
    use rdev::{EventType, Key, simulate};

    #[cfg(target_os = "macos")]
    {
        let _ = simulate(&EventType::KeyPress(Key::MetaLeft));
        let _ = simulate(&EventType::KeyPress(Key::KeyC));
        sleep(Duration::from_millis(15));
        let _ = simulate(&EventType::KeyRelease(Key::KeyC));
        let _ = simulate(&EventType::KeyRelease(Key::MetaLeft));
    }

    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        let _ = simulate(&EventType::KeyRelease(Key::Alt));
        let _ = simulate(&EventType::KeyRelease(Key::AltGr));
        sleep(Duration::from_millis(30));
        let _ = simulate(&EventType::KeyPress(Key::ControlLeft));
        let _ = simulate(&EventType::KeyPress(Key::KeyC));
        sleep(Duration::from_millis(15));
        let _ = simulate(&EventType::KeyRelease(Key::KeyC));
        let _ = simulate(&EventType::KeyRelease(Key::ControlLeft));
    }
}

// ─── UI Automation (Windows only) ────────────────────────────────────────────

#[cfg(windows)]
mod uia {
    use anyhow::Result;
    use uiautomation::UIAutomation;
    use uiautomation::patterns::UITextPattern;

    /// Reads the currently selected text from the focused UI element via
    /// the Windows UI Automation API.
    ///
    /// Returns `Ok(None)` if the focused element supports TextPattern but
    /// has nothing selected, or returns an Err if UIA isn't usable
    /// (COM init issue, focused element doesn't support TextPattern, etc.).
    pub fn get_selected_text() -> Result<Option<String>> {
        let automation = UIAutomation::new()
            .map_err(|e| anyhow::anyhow!("UIAutomation::new: {e}"))?;

        let element = automation
            .get_focused_element()
            .map_err(|e| anyhow::anyhow!("get_focused_element: {e}"))?;

        let pattern: UITextPattern = element
            .get_pattern()
            .map_err(|e| anyhow::anyhow!("get TextPattern: {e}"))?;

        let ranges = pattern
            .get_selection()
            .map_err(|e| anyhow::anyhow!("get_selection: {e}"))?;

        let mut text = String::new();
        for range in &ranges {
            if let Ok(t) = range.get_text(-1) {
                text.push_str(&t);
            }
        }

        let trimmed = text.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }
}

// ─── Win32 helpers for the clipboard fallback ────────────────────────────────

#[cfg(windows)]
mod win {
    use std::time::{Duration, Instant};
    use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    pub unsafe fn send_key(vk: u16, flags: u32) {
        unsafe {
            let mut input: INPUT = std::mem::zeroed();
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki.wVk = vk;
            input.Anonymous.ki.dwFlags = flags;
            SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        }
    }

    pub fn wait_for_alt_release(timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            unsafe {
                let l = GetAsyncKeyState(VK_LMENU as i32) as u16;
                let r = GetAsyncKeyState(VK_RMENU as i32) as u16;
                let m = GetAsyncKeyState(VK_MENU as i32) as u16;
                if (l | r | m) & 0x8000 == 0 {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    pub unsafe fn clipboard_seq() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }
}
