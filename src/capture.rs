//! Clipboard capture, for programs that do not share their text with Windows (Google Docs in
//! a browser is the main one).
//!
//! Two global hotkeys work in any program:
//!
//! * Ctrl+Alt+G  checks the whole document: it presses Select All and Copy for you, reads the
//!   clipboard, puts your old clipboard back, then moves the cursor to the end so a stray key
//!   cannot replace the selected text.
//! * Ctrl+Alt+S  checks only the text you have selected (it presses Copy).
//!
//! The hotkeys live on a thread of their own with its own message loop. That thread never
//! touches the app's windows, so it cannot get stuck waiting for the app (an earlier version of
//! the window shaping did, and froze).

use std::sync::mpsc::{Receiver, Sender, channel};

/// What a capture brought back.
#[derive(Clone, Debug)]
pub struct Captured {
    pub text: String,
    /// The process that owned the window the text came from (the browser).
    pub pid: u32,
    pub whole_document: bool,
}

pub type Event = Result<Captured, String>;

pub struct Hotkeys {
    pub rx: Receiver<Event>,
}

/// Start listening for the hotkeys. `repaint` wakes the app when a capture is ready.
pub fn start(repaint: impl Fn() + Send + 'static) -> Option<Hotkeys> {
    let (tx, rx) = channel::<Event>();
    #[cfg(windows)]
    {
        std::thread::spawn(move || listen(tx, repaint));
        Some(Hotkeys { rx })
    }
    #[cfg(not(windows))]
    {
        let _ = (tx, repaint);
        None
    }
}

/// Put `text` on the clipboard. Returns false if Windows would not let us.
pub fn copy_text(text: &str) -> bool {
    #[cfg(windows)]
    {
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text.to_string()))
            .is_ok()
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        false
    }
}

/// Docs hands over "\r\n" and the odd Unicode line separator; the checker wants "\n".
pub fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace(['\r', '\u{2028}', '\u{2029}'], "\n")
}

#[cfg(windows)]
mod ffi {
    #[repr(C)]
    pub struct Msg {
        pub hwnd: isize,
        pub message: u32,
        pub w_param: usize,
        pub l_param: isize,
        pub time: u32,
        pub pt_x: i32,
        pub pt_y: i32,
        pub l_private: u32,
    }

    #[repr(C)]
    pub struct KeybdInput {
        pub vk: u16,
        pub scan: u16,
        pub flags: u32,
        pub time: u32,
        pub extra: usize,
    }

    /// Windows' INPUT on 64-bit: a 4-byte type, padding, then a 32-byte union.
    #[repr(C)]
    pub struct Input {
        pub kind: u32,
        pub ki: KeybdInput,
        pub _union_tail: [u8; 8],
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn RegisterHotKey(hwnd: isize, id: i32, modifiers: u32, vk: u32) -> i32;
        pub fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
        pub fn GetAsyncKeyState(vk: i32) -> i16;
        pub fn GetForegroundWindow() -> isize;
        pub fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        pub fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
        pub fn SetForegroundWindow(hwnd: isize) -> i32;
    }
}

#[cfg(windows)]
const VK_SHIFT: u16 = 0x10;
#[cfg(windows)]
const VK_CONTROL: u16 = 0x11;
#[cfg(windows)]
const VK_MENU: u16 = 0x12; // Alt
#[cfg(windows)]
const VK_END: u16 = 0x23;
#[cfg(windows)]
const VK_RETURN: u16 = 0x0D;
#[cfg(windows)]
const VK_ESCAPE: u16 = 0x1B;
#[cfg(windows)]
const VK_F: u16 = 0x46;
#[cfg(windows)]
const VK_A: u16 = 0x41;
#[cfg(windows)]
const VK_C: u16 = 0x43;
#[cfg(windows)]
const VK_G: u16 = 0x47;
#[cfg(windows)]
const VK_S: u16 = 0x53;

#[cfg(windows)]
fn key_event(vk: u16, up: bool, extended: bool) {
    let flags = u32::from(up) * 2 + u32::from(extended);
    let input = ffi::Input {
        kind: 1, // INPUT_KEYBOARD
        ki: ffi::KeybdInput {
            vk,
            scan: 0,
            flags,
            time: 0,
            extra: 0,
        },
        _union_tail: [0; 8],
    };
    // SAFETY: `input` is a valid INPUT for the whole call.
    unsafe {
        ffi::SendInput(1, &input, std::mem::size_of::<ffi::Input>() as i32);
    }
}

/// Press the keys together (modifier first), then release them.
#[cfg(windows)]
fn chord(keys: &[(u16, bool)]) {
    use std::{thread::sleep, time::Duration};
    for &(vk, ext) in keys {
        key_event(vk, false, ext);
        sleep(Duration::from_millis(15));
    }
    for &(vk, ext) in keys.iter().rev() {
        key_event(vk, true, ext);
        sleep(Duration::from_millis(15));
    }
}

#[cfg(windows)]
fn is_down(vk: u16) -> bool {
    // SAFETY: plain query of a key's state.
    unsafe { ffi::GetAsyncKeyState(i32::from(vk)) < 0 }
}

/// Wait (up to two seconds) until the hotkey's own keys are let go, or the Ctrl+Alt we are
/// still holding would turn our Ctrl+C into Ctrl+Alt+C.
#[cfg(windows)]
fn wait_for_release(letter: u16) {
    use std::{
        thread::sleep,
        time::{Duration, Instant},
    };
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2)
        && [VK_CONTROL, VK_MENU, VK_SHIFT, letter]
            .iter()
            .any(|&k| is_down(k))
    {
        sleep(Duration::from_millis(20));
    }
}

#[cfg(windows)]
fn foreground_pid() -> u32 {
    let mut pid = 0u32;
    // SAFETY: `pid` outlives the call.
    unsafe {
        let hwnd = ffi::GetForegroundWindow();
        ffi::GetWindowThreadProcessId(hwnd, &mut pid);
    }
    pid
}

#[cfg(windows)]
fn capture(whole_document: bool, letter: u16) -> Event {
    use std::{
        thread::sleep,
        time::{Duration, Instant},
    };
    const SENTINEL: &str = "\u{1}jAyDHD-capture-waiting\u{1}";

    wait_for_release(letter);
    let pid = foreground_pid();
    if pid == std::process::id() {
        return Err("Click into the document first, then press the hotkey.".into());
    }
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Could not open the clipboard: {e}"))?;
    let old = clipboard.get_text().ok();
    clipboard
        .set_text(SENTINEL.to_string())
        .map_err(|e| format!("Could not use the clipboard: {e}"))?;

    if whole_document {
        chord(&[(VK_CONTROL, false), (VK_A, false)]);
        sleep(Duration::from_millis(60));
    }
    chord(&[(VK_CONTROL, false), (VK_C, false)]);

    let start = Instant::now();
    let mut copied = None;
    while start.elapsed() < Duration::from_millis(2500) {
        sleep(Duration::from_millis(40));
        if let Ok(t) = clipboard.get_text() {
            if t != SENTINEL {
                copied = Some(t);
                break;
            }
        }
    }
    if whole_document {
        // Leave the document with the cursor at the end, not everything selected.
        chord(&[(VK_CONTROL, false), (VK_END, true)]);
    }
    // Put back what was on the clipboard (text only; a picture cannot be restored).
    if let Some(old) = old {
        let _ = clipboard.set_text(old);
    }
    match copied {
        Some(text) if text.chars().any(char::is_alphanumeric) => Ok(Captured {
            text: clean(&text),
            pid,
            whole_document,
        }),
        Some(_) => Err("The document looks empty.".into()),
        None => Err(if whole_document {
            "Nothing was copied. Click inside the document text, then try again.".into()
        } else {
            "Nothing was copied. Select some text first, then try again.".into()
        }),
    }
}

#[cfg(windows)]
fn listen(tx: Sender<Event>, repaint: impl Fn()) {
    const MOD_ALT: u32 = 1;
    const MOD_CONTROL: u32 = 2;
    const MOD_NOREPEAT: u32 = 0x4000;
    const WM_HOTKEY: u32 = 0x0312;
    let mods = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
    // SAFETY: registering on this thread, which then runs the message loop below.
    let (g, s) = unsafe {
        (
            ffi::RegisterHotKey(0, 1, mods, u32::from(VK_G)),
            ffi::RegisterHotKey(0, 2, mods, u32::from(VK_S)),
        )
    };
    if g == 0 || s == 0 {
        let _ = tx.send(Err(
            "Ctrl+Alt+G or Ctrl+Alt+S is already used by another program, so the capture hotkeys are off."
                .into(),
        ));
        repaint();
        if g == 0 && s == 0 {
            return;
        }
    }
    let mut msg = ffi::Msg {
        hwnd: 0,
        message: 0,
        w_param: 0,
        l_param: 0,
        time: 0,
        pt_x: 0,
        pt_y: 0,
        l_private: 0,
    };
    loop {
        // SAFETY: `msg` is valid for the call; 0 means every message for this thread.
        let got = unsafe { ffi::GetMessageW(&mut msg, 0, 0, 0) };
        if got <= 0 {
            break;
        }
        if msg.message != WM_HOTKEY {
            continue;
        }
        let (whole, letter) = match msg.w_param {
            1 => (true, VK_G),
            2 => (false, VK_S),
            _ => continue,
        };
        let _ = tx.send(capture(whole, letter));
        repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn line_breaks_become_plain_newlines() {
        assert_eq!(clean("a\r\nb\rc\u{2028}d"), "a\nb\nc\nd");
    }
}

// -------------------------------------------------------------------------------------------
// Helpers for the Google Docs worker (they run on its thread, never on the app's windows)
// -------------------------------------------------------------------------------------------

/// Wait (up to `max_ms`) until Ctrl, Alt and Shift are all let go. False if they never were.
#[cfg(windows)]
fn wait_for_modifiers(max_ms: u64) -> bool {
    use std::{
        thread::sleep,
        time::{Duration, Instant},
    };
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(max_ms) {
        if ![VK_CONTROL, VK_MENU, VK_SHIFT].iter().any(|&k| is_down(k)) {
            return true;
        }
        sleep(Duration::from_millis(20));
    }
    false
}

/// Copy whatever is selected in the window you are in and return it, or `None` when nothing
/// is selected (or a modifier key stayed held). The old text on the clipboard is put back.
#[cfg(windows)]
pub fn selection_now() -> Option<String> {
    use std::{
        thread::sleep,
        time::{Duration, Instant},
    };
    const SENTINEL: &str = "\u{1}jAyDHD-selection-waiting\u{1}";
    if !wait_for_modifiers(1500) {
        return None;
    }
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let old = clipboard.get_text().ok();
    clipboard.set_text(SENTINEL.to_string()).ok()?;
    chord(&[(VK_CONTROL, false), (VK_C, false)]);
    let start = Instant::now();
    let mut copied = None;
    while start.elapsed() < Duration::from_millis(500) {
        sleep(Duration::from_millis(30));
        if let Ok(t) = clipboard.get_text() {
            if t != SENTINEL {
                copied = Some(t);
                break;
            }
        }
    }
    if let Some(old) = old {
        let _ = clipboard.set_text(old);
    }
    copied
        .map(|t| clean(&t))
        .filter(|t| t.chars().any(char::is_alphanumeric))
}

#[cfg(not(windows))]
pub fn selection_now() -> Option<String> {
    None
}

/// Type text into the window you are in, one UTF-16 unit at a time (any character works).
#[cfg(windows)]
fn type_text(text: &str) {
    use std::{thread::sleep, time::Duration};
    for unit in text.encode_utf16() {
        for up in [false, true] {
            let input = ffi::Input {
                kind: 1,
                ki: ffi::KeybdInput {
                    vk: 0,
                    scan: unit,
                    flags: 4 | u32::from(up) * 2, // KEYEVENTF_UNICODE (| KEYUP)
                    time: 0,
                    extra: 0,
                },
                _union_tail: [0; 8],
            };
            // SAFETY: `input` is a valid INPUT for the whole call.
            unsafe {
                ffi::SendInput(1, &input, std::mem::size_of::<ffi::Input>() as i32);
            }
        }
        sleep(Duration::from_millis(4));
    }
}

/// Bring the browser window forward and use the document's own Find to jump to `needle`:
/// Ctrl+F, type it, Enter, Escape. The match stays selected in the document.
#[cfg(windows)]
pub fn find_in_document(hwnd: isize, needle: &str) {
    use std::{thread::sleep, time::Duration};
    // SAFETY: plain window call.
    unsafe {
        ffi::SetForegroundWindow(hwnd);
    }
    sleep(Duration::from_millis(300));
    if !wait_for_modifiers(1500) {
        return;
    }
    chord(&[(VK_CONTROL, false), (VK_F, false)]);
    sleep(Duration::from_millis(400));
    type_text(needle);
    sleep(Duration::from_millis(200));
    chord(&[(VK_RETURN, false)]);
    sleep(Duration::from_millis(250));
    chord(&[(VK_ESCAPE, false)]);
}

#[cfg(not(windows))]
pub fn find_in_document(_hwnd: isize, _needle: &str) {}
