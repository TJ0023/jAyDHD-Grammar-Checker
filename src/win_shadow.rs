//! Windows draws a soft drop shadow and rounded corners around every top-level window, and
//! fills the part of a window nobody painted. For the borderless floating boxes this module
//! (1) turns the shadow and the rounded window corners off, and (2) cuts each window to exactly
//! the shape the box occupies, so nothing outside the box is visible or clickable.
//!
//! The UI says what shape a window should have with [`set_shape`] (keyed by the window's title);
//! a background thread applies it to the real window.

#[cfg(windows)]
use std::collections::HashMap;
#[cfg(windows)]
use std::sync::Mutex;

/// What part of a window is shown (everything else is cut away, and cannot be clicked).
#[cfg(windows)]
#[derive(Clone, PartialEq)]
enum Shape {
    /// Left, top, right, bottom and corner radius, in pixels.
    Round([i32; 5]),
    /// A union of rectangles (left, top, right, bottom), in pixels: the shape of a picture.
    Rects(Vec<[i32; 4]>),
}

/// The shape each window has now, so an unchanged one is not set again.
#[cfg(windows)]
static APPLIED: Mutex<Option<HashMap<isize, Shape>>> = Mutex::new(None);

/// Ask for the window titled `title` to be cut to this rectangle (window pixels, top-left is
/// 0,0) with rounded corners of `radius` pixels.
#[cfg(windows)]
pub fn set_shape(title: &str, rect: [i32; 4], radius: i32) {
    publish(
        title,
        Shape::Round([rect[0], rect[1], rect[2], rect[3], radius]),
    );
}

/// Record the wanted shape and apply it right now (on the calling thread), so the window is
/// cut in the same frame that draws the picture. The background thread covers windows that did
/// not exist yet.
#[cfg(windows)]
fn publish(title: &str, shape: Shape) {
    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that lives through the call.
    let h = unsafe { ffi::FindWindowW(std::ptr::null(), wide.as_ptr()) };
    if h != 0 {
        apply(h, &shape);
    }
}

/// Ask for the window titled `title` to show only these rectangles (window pixels).
#[cfg(windows)]
pub fn set_rects(title: &str, rects: Vec<[i32; 4]>) {
    publish(title, Shape::Rects(rects));
}

#[cfg(not(windows))]
pub fn set_shape(_title: &str, _rect: [i32; 4], _radius: i32) {}

#[cfg(not(windows))]
pub fn set_rects(_title: &str, _rects: Vec<[i32; 4]>) {}

/// Nothing to start: shapes are applied by [`set_shape`] and [`set_rects`] on the UI thread.
/// (A helper thread that also called Win32 window functions deadlocked with the UI thread.)
#[cfg(windows)]
pub fn start() {}

#[cfg(not(windows))]
pub fn start() {}

#[cfg(windows)]
mod ffi {
    use std::ffi::c_void;

    pub type Hwnd = isize;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn FindWindowW(class: *const u16, title: *const u16) -> isize;
        pub fn SetWindowRgn(h: Hwnd, region: isize, redraw: i32) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        pub fn CreateRoundRectRgn(l: i32, t: i32, r: i32, b: i32, w: i32, h: i32) -> isize;
        pub fn ExtCreateRegion(xform: *const c_void, count: u32, data: *const c_void) -> isize;
    }

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        pub fn DwmSetWindowAttribute(h: Hwnd, attr: u32, value: *const c_void, size: u32) -> i32;
        pub fn DwmExtendFrameIntoClientArea(h: Hwnd, margins: *const [i32; 4]) -> i32;
    }
}

/// Cut the window `h` to `shape`, and turn its shadow and rounded corners off.
#[cfg(windows)]
fn apply(h: ffi::Hwnd, shape: &Shape) {
    use ffi::*;
    const DWMWA_NCRENDERING_POLICY: u32 = 2;
    const DWMNCRP_DISABLED: i32 = 1;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_DONOTROUND: i32 = 1;

    let unchanged = APPLIED
        .lock()
        .ok()
        .is_some_and(|g| g.as_ref().and_then(|m| m.get(&h)) == Some(shape));
    if unchanged {
        return;
    }
    // SAFETY: plain Win32 calls; the buffers passed are valid for the lengths given.
    unsafe {
        // Shadow and rounded corners off. winit's "undecorated shadow" is a one pixel
        // extension of the window frame; removing it removes the shadow.
        let policy = DWMNCRP_DISABLED;
        DwmSetWindowAttribute(
            h,
            DWMWA_NCRENDERING_POLICY,
            &policy as *const i32 as *const std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
        let corners = DWMWCP_DONOTROUND;
        DwmSetWindowAttribute(
            h,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corners as *const i32 as *const std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
        DwmExtendFrameIntoClientArea(h, &[0, 0, 0, 0]);

        let region = match shape {
            Shape::Round([l, t, r, b, radius]) => {
                if r <= l || b <= t {
                    return;
                }
                let d = (*radius).max(0) * 2;
                CreateRoundRectRgn(*l, *t, *r + 1, *b + 1, d, d)
            }
            Shape::Rects(rects) => {
                if rects.is_empty() {
                    return;
                }
                // RGNDATA: a 32-byte header, then one RECT (4 i32) per rectangle.
                let (mut l, mut t, mut r, mut b) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
                let mut data: Vec<i32> = Vec::with_capacity(8 + rects.len() * 4);
                data.extend_from_slice(&[0; 8]);
                for q in rects {
                    data.extend_from_slice(q);
                    l = l.min(q[0]);
                    t = t.min(q[1]);
                    r = r.max(q[2]);
                    b = b.max(q[3]);
                }
                let bytes = (rects.len() * 16) as i32;
                data[0] = 32; // header size
                data[1] = 1; // RDH_RECTANGLES
                data[2] = rects.len() as i32;
                data[3] = bytes;
                data[4] = l;
                data[5] = t;
                data[6] = r;
                data[7] = b;
                ExtCreateRegion(
                    std::ptr::null(),
                    (32 + bytes) as u32,
                    data.as_ptr() as *const std::ffi::c_void,
                )
            }
        };
        if region != 0 {
            // The system owns the region once this succeeds.
            SetWindowRgn(h, region, 1);
            if let Ok(mut guard) = APPLIED.lock() {
                guard
                    .get_or_insert_with(HashMap::new)
                    .insert(h, shape.clone());
            }
        }
    }
}
