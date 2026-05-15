// Global keyboard listener via rdev::grab.
//
// Suppresses R (the hotkey's main key) so Alt+R doesn't leak an "r" into the
// focused app. Alt itself is passed through — a bare Alt tap is harmless in
// most apps. Press once → Triggered. Modifier tracking is strict: Alt must be
// the only modifier held when R is pressed.

use anyhow::{Result, anyhow};
use parking_lot::Mutex;
use rdev::{Event, EventType, Key, grab};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Triggered,
}

#[derive(Debug, Clone)]
struct HotkeyDef {
    modifiers: HashSet<Key>,
    key: Key,
}

struct ListenerState {
    held: HashSet<Key>,
    /// True while the main key is still physically held after triggering.
    /// Prevents re-firing until key is released and pressed again.
    key_down: bool,
}

pub fn spawn_listener(hotkey_str: &str) -> Result<Receiver<HotkeyEvent>> {
    let hotkey = parse(hotkey_str)?;
    let (tx, rx) = channel();

    std::thread::spawn(move || {
        let state = Arc::new(Mutex::new(ListenerState {
            held: HashSet::new(),
            key_down: false,
        }));

        let callback = move |event: Event| -> Option<Event> {
            handle_event(event, &hotkey, &state, &tx)
        };

        if let Err(e) = grab(callback) {
            tracing::error!("rdev grab error: {e:?}");
        }
    });

    Ok(rx)
}

fn handle_event(
    event: Event,
    hotkey: &HotkeyDef,
    state: &Arc<Mutex<ListenerState>>,
    tx: &Sender<HotkeyEvent>,
) -> Option<Event> {
    // We deliberately do NOT suppress any keys. Earlier versions suppressed
    // the main key (R), which caused the focused app to see only Alt-down
    // followed (eventually) by Alt-up with nothing in between — a "bare Alt
    // tap" that activates the Windows menu bar and steals focus from the
    // document, clearing the user's text selection before we could read it.
    //
    // Letting R pass through means the focused app sees a proper Alt+R combo,
    // which most apps simply ignore (browsers, PDF viewers, etc.). The
    // tradeoff: in apps that DO bind Alt+R (e.g., Word's Review ribbon),
    // that action will also fire. Acceptable for the read-aloud use case.

    match event.event_type {
        EventType::KeyPress(k) => {
            let mut s = state.lock();
            s.held.insert(k);
            if k == hotkey.key && !s.key_down && combo_satisfied(hotkey, &s.held) {
                s.key_down = true;
                let _ = tx.send(HotkeyEvent::Triggered);
            }
        }
        EventType::KeyRelease(k) => {
            let mut s = state.lock();
            s.held.remove(&k);
            if k == hotkey.key {
                s.key_down = false;
            }
        }
        _ => {}
    }

    Some(event)
}

fn combo_satisfied(hk: &HotkeyDef, held: &HashSet<Key>) -> bool {
    held.contains(&hk.key) && hk.modifiers.iter().all(|m| held.contains(m))
}

fn parse(s: &str) -> Result<HotkeyDef> {
    let mut modifiers = HashSet::new();
    let mut key: Option<Key> = None;

    for raw in s.split('+').map(|p| p.trim().to_lowercase()) {
        match raw.as_str() {
            "shift" | "lshift" | "left_shift" => {
                modifiers.insert(Key::ShiftLeft);
            }
            "rshift" | "right_shift" => {
                modifiers.insert(Key::ShiftRight);
            }
            "ctrl" | "control" | "lctrl" | "left_ctrl" => {
                modifiers.insert(Key::ControlLeft);
            }
            "rctrl" | "right_ctrl" => {
                modifiers.insert(Key::ControlRight);
            }
            "alt" => {
                modifiers.insert(Key::Alt);
            }
            "ralt" | "right_alt" | "altgr" => {
                modifiers.insert(Key::AltGr);
            }
            "cmd" | "win" | "super" | "meta" => {
                modifiers.insert(Key::MetaLeft);
            }
            other => {
                key = Some(named_key(other)?);
            }
        }
    }

    let key = key.ok_or_else(|| anyhow!("hotkey must include a non-modifier key: {s}"))?;
    Ok(HotkeyDef { modifiers, key })
}

fn named_key(name: &str) -> Result<Key> {
    use Key::*;
    let k = match name {
        "space" => Space,
        "tab" => Tab,
        "enter" | "return" => Return,
        "escape" | "esc" => Escape,
        "backspace" => Backspace,
        "capslock" | "caps_lock" => CapsLock,
        "insert" => Insert,
        "delete" | "del" => Delete,
        "home" => Home,
        "end" => End,
        "pageup" | "page_up" => PageUp,
        "pagedown" | "page_down" => PageDown,
        "f1" => F1, "f2" => F2, "f3" => F3, "f4" => F4,
        "f5" => F5, "f6" => F6, "f7" => F7, "f8" => F8,
        "f9" => F9, "f10" => F10, "f11" => F11, "f12" => F12,
        other if other.len() == 1 => {
            let c = other.chars().next().unwrap().to_ascii_uppercase();
            match c {
                'A' => KeyA, 'B' => KeyB, 'C' => KeyC, 'D' => KeyD,
                'E' => KeyE, 'F' => KeyF, 'G' => KeyG, 'H' => KeyH,
                'I' => KeyI, 'J' => KeyJ, 'K' => KeyK, 'L' => KeyL,
                'M' => KeyM, 'N' => KeyN, 'O' => KeyO, 'P' => KeyP,
                'Q' => KeyQ, 'R' => KeyR, 'S' => KeyS, 'T' => KeyT,
                'U' => KeyU, 'V' => KeyV, 'W' => KeyW, 'X' => KeyX,
                'Y' => KeyY, 'Z' => KeyZ,
                '0' => Num0, '1' => Num1, '2' => Num2, '3' => Num3, '4' => Num4,
                '5' => Num5, '6' => Num6, '7' => Num7, '8' => Num8, '9' => Num9,
                _ => return Err(anyhow!("unsupported key: {other}")),
            }
        }
        _ => return Err(anyhow!("unknown key name: {name}")),
    };
    Ok(k)
}
