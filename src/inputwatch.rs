//! A system-wide watcher for typing and selecting, for programs that do not tell us anything
//! (Google Docs in a browser, mainly).
//!
//! It only reports THAT something happened, never which key: a `Key` means "a character-making
//! key was pressed" (so the pets can move their paws) and a `Select` means "the selection may
//! have just changed" (a mouse button was let go, or Shift+arrow / Ctrl+A was pressed), so the
//! Google Docs worker can look at it. Keys pressed together with Ctrl or Alt are shortcuts, not
//! typing, and are ignored; so is anything this program itself sends.
//!
//! The hooks live on a thread of their own with a message loop. The hook function only sends
//! on a channel, so it returns at once.

use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Key,
    Select,
}

type Fanout = Vec<Sender<Input>>;

static OUT: OnceLock<Fanout> = OnceLock::new();
static REPAINT: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

fn send(i: Input) {
    if let Some(out) = OUT.get() {
        for tx in out {
            let _ = tx.send(i);
        }
    }
    if i == Input::Key {
        if let Some(r) = REPAINT.get() {
            r();
        }
    }
}

/// Two receivers get every event: the app (for the pets) and the Google Docs worker.
pub fn start(repaint: impl Fn() + Send + Sync + 'static) -> (Receiver<Input>, Receiver<Input>) {
    let (a_tx, a_rx) = channel();
    let (b_tx, b_rx) = channel();
    if OUT.set(vec![a_tx, b_tx]).is_ok() {
        let _ = REPAINT.set(Box::new(repaint));
        listen();
    }
    (a_rx, b_rx)
}

#[cfg(not(windows))]
fn listen() {}

#[cfg(windows)]
fn listen() {
    std::thread::spawn(|| {
        // SAFETY: standard hook set-up; the hook functions below follow the documented shapes.
        unsafe {
            let module = ffi::GetModuleHandleW(std::ptr::null());
            ffi::SetWindowsHookExW(13, keyboard, module, 0); // WH_KEYBOARD_LL
            ffi::SetWindowsHookExW(14, mouse, module, 0); // WH_MOUSE_LL
            let mut msg = ffi::Msg::default();
            while ffi::GetMessageW(&mut msg, 0, 0, 0) > 0 {}
        }
    });
}

#[cfg(windows)]
mod ffi {
    #[repr(C)]
    #[derive(Default)]
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
    pub struct KeyHook {
        pub vk: u32,
        pub scan: u32,
        pub flags: u32,
        pub time: u32,
        pub extra: usize,
    }

    #[repr(C)]
    pub struct MouseHook {
        pub x: i32,
        pub y: i32,
        pub data: u32,
        pub flags: u32,
        pub time: u32,
        pub extra: usize,
    }

    pub type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn SetWindowsHookExW(id: i32, proc: HookProc, module: isize, thread: u32) -> isize;
        pub fn CallNextHookEx(hook: isize, code: i32, w: usize, l: isize) -> isize;
        pub fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
        pub fn GetAsyncKeyState(vk: i32) -> i16;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetModuleHandleW(name: *const u16) -> isize;
    }
}

#[cfg(windows)]
fn down(vk: i32) -> bool {
    // SAFETY: plain query of a key's state.
    unsafe { ffi::GetAsyncKeyState(vk) < 0 }
}

#[cfg(windows)]
unsafe extern "system" fn keyboard(code: i32, w: usize, l: isize) -> isize {
    const WM_KEYDOWN: usize = 0x100;
    const WM_SYSKEYDOWN: usize = 0x104;
    if code == 0 && (w == WM_KEYDOWN || w == WM_SYSKEYDOWN) && l != 0 {
        // SAFETY: for a keyboard hook, `l` points to a KBDLLHOOKSTRUCT.
        let k = unsafe { &*(l as *const ffi::KeyHook) };
        let injected = k.flags & 0x10 != 0; // LLKHF_INJECTED
        if !injected {
            let vk = k.vk;
            let modifier =
                matches!(vk, 0x10..=0x12 | 0x5B | 0x5C | 0xA0..=0xA5 | 0x14 | 0x90 | 0x91);
            let ctrl = down(0x11);
            let alt = down(0x12);
            let shift = down(0x10);
            if !modifier && !ctrl && !alt {
                send(Input::Key);
            }
            let moves = matches!(vk, 0x21..=0x28); // PageUp .. arrows
            if (moves && shift) || (ctrl && vk == 0x41) {
                send(Input::Select);
            }
        }
    }
    // SAFETY: passing the event on, as hooks must.
    unsafe { ffi::CallNextHookEx(0, code, w, l) }
}

#[cfg(windows)]
unsafe extern "system" fn mouse(code: i32, w: usize, l: isize) -> isize {
    const WM_LBUTTONUP: usize = 0x202;
    if code == 0 && w == WM_LBUTTONUP && l != 0 {
        // SAFETY: for a mouse hook, `l` points to an MSLLHOOKSTRUCT.
        let m = unsafe { &*(l as *const ffi::MouseHook) };
        if m.flags & 1 == 0 {
            send(Input::Select);
        }
    }
    // SAFETY: passing the event on, as hooks must.
    unsafe { ffi::CallNextHookEx(0, code, w, l) }
}
