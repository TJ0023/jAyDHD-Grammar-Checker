//! Where the text comes from. Grammar Desk used to have its own text box; now it can instead
//! watch the window you are typing in. A worker thread asks Windows (UI Automation) what the
//! focused text box holds and sends a [`Snapshot`] to the app whenever it changes.
//!
//! It reads Notepad, Word and ordinary web text boxes, and can write a fix back into the box it
//! last read ([`Replace`]): it finds the flagged text there, selects it and types the fix.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};

#[cfg(windows)]
mod uia;

/// What one text box held at one moment.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// All the text in the box (the first 100,000 characters).
    pub text: String,
    /// The selected text, empty when nothing is selected.
    pub selected: String,
    /// The process the box belongs to, and what kind of control it is.
    pub process_id: u32,
    pub control: String,
    pub name: String,
    /// "TextPattern" or "ValuePattern": which way the text was read.
    pub how: &'static str,
}

impl Snapshot {
    /// One line for the status bar.
    pub fn describe(&self) -> String {
        let name = if self.name.is_empty() {
            String::new()
        } else {
            format!(" \"{}\"", self.name)
        };
        format!(
            "{}{} (process {}, read by {})",
            self.control, name, self.process_id, self.how
        )
    }
}

/// A fix to make in the text box that was read last (Apply Change).
#[derive(Clone, Debug)]
pub struct Replace {
    /// The whole text as the app last saw it, line breaks as "\n". The fix is refused when the
    /// box no longer holds exactly this, so a stale fix never lands in the wrong place.
    pub expected: String,
    /// Which match of `old` to change, counting from 0, left to right without overlaps.
    pub occurrence: usize,
    /// The text to find (never empty, no line breaks) and what goes in its place.
    pub old: String,
    pub new: String,
    /// A few characters just before and after `old` on its line. When the box's text is not
    /// exactly `expected` (Word reports small differences), the fix is still made if
    /// before + old + after appears exactly once.
    pub before: String,
    pub after: String,
}

/// How a [`Replace`] went. When `ok` is false nothing was changed, and `new` is the fix so the
/// app can copy it to the clipboard instead.
#[derive(Clone, Debug)]
pub struct Applied {
    pub ok: bool,
    pub message: String,
    pub new: String,
}

/// A running watcher. Dropping it stops the worker thread.
pub struct TextSource {
    pub rx: Receiver<Snapshot>,
    /// The answers to [`TextSource::replace`].
    pub applied: Receiver<Applied>,
    commands: Sender<Replace>,
    stop: Arc<AtomicBool>,
}

impl TextSource {
    /// Ask the worker to make a fix in the box it read last. The answer arrives on `applied`.
    pub fn replace(&self, fix: Replace) -> bool {
        self.commands.send(fix).is_ok()
    }
}

impl Drop for TextSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Start watching. `repaint` is called whenever a new snapshot is ready, so the window redraws
/// even when nobody is touching it. Returns `None` where this is not supported (not Windows).
#[cfg(windows)]
pub fn start(repaint: impl Fn() + Send + 'static) -> Option<TextSource> {
    let (tx, rx) = std::sync::mpsc::channel();
    let (commands, command_rx) = std::sync::mpsc::channel();
    let (applied_tx, applied) = std::sync::mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    std::thread::spawn(move || uia::run(tx, command_rx, applied_tx, flag, repaint));
    Some(TextSource {
        rx,
        applied,
        commands,
        stop,
    })
}

#[cfg(not(windows))]
pub fn start(_repaint: impl Fn() + Send + 'static) -> Option<TextSource> {
    None
}
