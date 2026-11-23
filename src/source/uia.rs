//! Reading the focused text box through Windows UI Automation. All of this runs on the worker
//! thread: asking another program for its text can be slow, and must never freeze the window.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use uiautomation::patterns::{UITextPattern, UIValuePattern};
use uiautomation::{UIAutomation, UIElement};

use super::Snapshot;

/// How often the focused box is looked at.
const EVERY: Duration = Duration::from_millis(300);
/// The most text read from one box.
const MAX_CHARS: i32 = 100_000;

pub fn run(tx: Sender<Snapshot>, stop: Arc<AtomicBool>, repaint: impl Fn()) {
    let Ok(automation) = UIAutomation::new() else {
        return;
    };
    let own = std::process::id();
    let mut last: Option<u64> = None;
    while !stop.load(Ordering::Relaxed) {
        if let Some(snapshot) = read(&automation, own) {
            let mut h = DefaultHasher::new();
            (&snapshot.text, &snapshot.selected, snapshot.process_id).hash(&mut h);
            let stamp = h.finish();
            if last != Some(stamp) {
                last = Some(stamp);
                if tx.send(snapshot).is_err() {
                    return;
                }
                repaint();
            }
        }
        thread::sleep(EVERY);
    }
}

/// The focused text box, if there is one we may and can read. Our own windows, password fields
/// and controls that share no text give `None`, which leaves the last snapshot in place.
fn read(automation: &UIAutomation, own_pid: u32) -> Option<Snapshot> {
    let element: UIElement = automation.get_focused_element().ok()?;
    let process_id = element.get_process_id().ok()?;
    if process_id == own_pid {
        return None;
    }
    if element.is_password().unwrap_or(false) {
        return None;
    }
    let control = element
        .get_control_type()
        .map_or("?".to_string(), |k| format!("{k:?}"));
    let name = element.get_name().unwrap_or_default();

    if let Ok(pattern) = element.get_pattern::<UITextPattern>() {
        let text = pattern
            .get_document_range()
            .and_then(|r| r.get_text(MAX_CHARS))
            .ok()?;
        let selected = pattern
            .get_selection()
            .ok()
            .and_then(|ranges| ranges.into_iter().next())
            .and_then(|r| r.get_text(1000).ok())
            .unwrap_or_default();
        return Some(Snapshot {
            text,
            selected,
            process_id,
            control,
            name,
            how: "TextPattern",
        });
    }
    if let Ok(pattern) = element.get_pattern::<UIValuePattern>() {
        let text = pattern.get_value().ok()?;
        return Some(Snapshot {
            text,
            selected: String::new(),
            process_id,
            control,
            name,
            how: "ValuePattern",
        });
    }
    None
}
