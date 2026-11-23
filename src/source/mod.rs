//! Where the text comes from. Grammar Desk used to have its own text box; now it can instead
//! watch the window you are typing in. A worker thread asks Windows (UI Automation) what the
//! focused text box holds and sends a [`Snapshot`] to the app whenever it changes.
//!
//! Step 1 reads Notepad, Word and ordinary web text boxes. Still to come: the clipboard hotkey
//! (for Google Docs and other apps that do not share their text) and writing changes back.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;

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

/// A running watcher. Dropping it stops the worker thread.
pub struct TextSource {
    pub rx: Receiver<Snapshot>,
    stop: Arc<AtomicBool>,
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
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    std::thread::spawn(move || uia::run(tx, flag, repaint));
    Some(TextSource { rx, stop })
}

#[cfg(not(windows))]
pub fn start(_repaint: impl Fn() + Send + 'static) -> Option<TextSource> {
    None
}
