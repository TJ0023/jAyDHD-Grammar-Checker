//! Reading the focused text box through Windows UI Automation. All of this runs on the worker
//! thread: asking another program for its text can be slow, and must never freeze the window.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use uiautomation::patterns::{UITextPattern, UITextRange, UIValuePattern};
use uiautomation::types::{TextPatternRangeEndpoint, TextUnit};
use uiautomation::{UIAutomation, UIElement};

use super::{Applied, Replace, Snapshot};

/// How often the focused box is looked at.
const EVERY: Duration = Duration::from_millis(300);
/// The most text read from one box.
const MAX_CHARS: i32 = 100_000;

/// The text box read last, kept so Apply Change can write into it after you click a box (by
/// then the focus is in this program, not in the text box).
struct Target {
    element: UIElement,
    /// The window it sits in, brought to the front before typing. 0 when unknown.
    hwnd: isize,
}

pub fn run(
    tx: Sender<Snapshot>,
    commands: Receiver<Replace>,
    applied: Sender<Applied>,
    stop: Arc<AtomicBool>,
    repaint: impl Fn(),
) {
    let Ok(automation) = UIAutomation::new() else {
        return;
    };
    let own = std::process::id();
    let mut last: Option<u64> = None;
    let mut target: Option<Target> = None;
    while !stop.load(Ordering::Relaxed) {
        if let Some((snapshot, element)) = read(&automation, own) {
            // The focused box sits in the window in front (a browser may show its text boxes
            // from a different process than the window, so the process is not compared).
            let (hwnd, front_pid) = crate::capture::foreground_window();
            target = Some(Target {
                element,
                hwnd: if front_pid == own { 0 } else { hwnd },
            });
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
        // Wait for the next look, but answer an Apply Change at once.
        match commands.recv_timeout(EVERY) {
            Ok(fix) => {
                log(&format!(
                    "Apply Change: {:?} -> {:?} (match {}), window {}",
                    fix.old,
                    fix.new,
                    fix.occurrence,
                    target.as_ref().map_or(0, |t| t.hwnd)
                ));
                let result = match &target {
                    Some(t) => apply(t, &fix),
                    None => Err("Click into the text in the other program first.".to_string()),
                };
                log(&format!("  result: {result:?}"));
                let (ok, message) = match result {
                    Ok(m) => (true, m),
                    Err(m) => (false, m),
                };
                if applied
                    .send(Applied {
                        ok,
                        message,
                        new: fix.new,
                    })
                    .is_err()
                {
                    return;
                }
                // Send the next snapshot even if it looks the same, so the boxes catch up.
                last = None;
                repaint();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Line breaks as the app keeps them, so texts can be compared.
fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Notes on each Apply Change, so a fix that does not land can be traced:
/// `%APPDATA%\GrammarDesk\apply_debug.txt` (restarted once it passes 200 KB).
fn log(line: &str) {
    use std::io::Write;
    let Some(dir) = crate::docs::folder() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("apply_debug.txt");
    if std::fs::metadata(&path).map(|m| m.len() > 200_000).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

/// Up to `n` characters around byte `at`, for the log.
fn around(text: &str, at: usize, n: usize) -> String {
    let at = (0..=at.min(text.len())).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0);
    let lo: String = text[..at].chars().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect();
    let hi: String = text[at..].chars().take(n).collect();
    format!("{lo:?} | {hi:?}")
}

/// Which match of `fix.old` to change in `now` (line breaks already normalized). The app's
/// own count when the text is unchanged; otherwise found again from the text around it.
fn which_match(now: &str, fix: &Replace) -> Result<usize, String> {
    if now == fix.expected {
        return Ok(fix.occurrence);
    }
    let diff = now
        .bytes()
        .zip(fix.expected.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(now.len().min(fix.expected.len()));
    log(&format!(
        "  text differs: now {} bytes, expected {} bytes, first difference at {diff}\n    now:      {}\n    expected: {}",
        now.len(),
        fix.expected.len(),
        around(now, diff, 25),
        around(&fix.expected, diff, 25)
    ));
    let needle = format!("{}{}{}", fix.before, fix.old, fix.after);
    let spots: Vec<usize> = now.match_indices(needle.as_str()).map(|(i, _)| i).collect();
    log(&format!("  looked for {needle:?} by its surroundings: {} match(es)", spots.len()));
    let &[spot] = spots.as_slice() else {
        return Err(CHANGED.into());
    };
    let at = spot + fix.before.len();
    now.match_indices(fix.old.as_str())
        .position(|(i, _)| i == at)
        .ok_or_else(|| CHANGED.to_string())
}

const CHANGED: &str = "The text changed since it was checked. Try again in a moment.";
const NOT_FOUND: &str = "Could not find the flagged text in the other program.";

/// The `n`th match (from 0) of `needle` inside `range`. Moves the start of `range` as it goes.
fn find_nth(range: &UITextRange, needle: &str, n: usize) -> Result<UITextRange, String> {
    let mut hit = range
        .find_text(needle, false, false)
        .map_err(|_| NOT_FOUND.to_string())?;
    for _ in 0..n {
        range
            .move_endpoint_by_range(
                TextPatternRangeEndpoint::Start,
                &hit,
                TextPatternRangeEndpoint::End,
            )
            .map_err(|_| NOT_FOUND.to_string())?;
        hit = range
            .find_text(needle, false, false)
            .map_err(|_| NOT_FOUND.to_string())?;
    }
    Ok(hit)
}

/// Search for before + old + after, then pick `old` out of what was found. `None` when the
/// longer text is not found (or there is no text around it to help).
fn find_with_context(doc: &UITextRange, fix: &Replace) -> Option<UITextRange> {
    if fix.before.is_empty() && fix.after.is_empty() {
        return None;
    }
    let needle = format!("{}{}{}", fix.before, fix.old, fix.after);
    let around = doc.find_text(&needle, false, false).ok()?;
    let got = around.get_text(MAX_CHARS).map(|s| normalize(&s)).ok()?;
    if got != needle {
        log(&format!("  surroundings search returned {got:?}"));
        return None;
    }
    // `old` may also appear inside `before`: skip those matches.
    let skip = needle
        .match_indices(fix.old.as_str())
        .position(|(i, _)| i == fix.before.len())?;
    find_nth(&around, &fix.old, skip).ok()
}

/// The current selection (or cursor) as character offsets from the start of the document,
/// line breaks counted once. `None` when the program does not say.
fn selection_offsets(pattern: &UITextPattern) -> Option<(usize, usize)> {
    let sel = pattern.get_selection().ok()?.into_iter().next()?;
    let selected = normalize(&sel.get_text(MAX_CHARS).ok()?);
    let head = pattern.get_document_range().ok()?;
    head.move_endpoint_by_range(
        TextPatternRangeEndpoint::End,
        &sel,
        TextPatternRangeEndpoint::Start,
    )
    .ok()?;
    let before = normalize(&head.get_text(MAX_CHARS).ok()?);
    let s = before.chars().count();
    Some((s, s + selected.chars().count()))
}

/// Move the cursor to character `target` with the arrow keys, checking where it is after each
/// move. False when the program does not report the cursor or it cannot be got there.
fn move_caret_to(pattern: &UITextPattern, target: usize) -> bool {
    use crate::capture::{EditKey, press_key};
    for attempt in 0..5 {
        let Some((s, e)) = selection_offsets(pattern) else {
            log("  the program does not report where the cursor is");
            return false;
        };
        log(&format!("  cursor move {attempt}: at {s}..{e}, want {target}"));
        if s == e && s == target {
            return true;
        }
        if s != e {
            // Something is selected: Left drops the selection, leaving the cursor at its start.
            press_key(EditKey::Left, 1);
        } else if s.abs_diff(target) > 20_000 {
            log("  too far to move with the arrow keys");
            return false;
        } else if target > s {
            press_key(EditKey::Right, target - s);
        } else {
            press_key(EditKey::Left, s - target);
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Where (byte offset in `now`) the flagged text starts: by its surroundings if they are
/// unique, otherwise the app's own count of matches.
fn spot(now: &str, fix: &Replace, occurrence: usize) -> Option<usize> {
    let needle = format!("{}{}{}", fix.before, fix.old, fix.after);
    let spots: Vec<usize> = now.match_indices(needle.as_str()).map(|(i, _)| i).collect();
    match spots.as_slice() {
        &[s] => Some(s + fix.before.len()),
        _ => now.match_indices(fix.old.as_str()).nth(occurrence).map(|(i, _)| i),
    }
}

/// The flagged text found by counting characters from the start of the document, for programs
/// that cannot search. `now` is the box's text exactly as Windows gave it.
fn by_offset(
    pattern: &UITextPattern,
    now: &str,
    fix: &Replace,
    occurrence: usize,
) -> Option<UITextRange> {
    // Where the flagged text starts in `now`: by its surroundings if that is unique, otherwise
    // the app's own count of matches.
    let needle = format!("{}{}{}", fix.before, fix.old, fix.after);
    let spots: Vec<usize> = now.match_indices(needle.as_str()).map(|(i, _)| i).collect();
    let at = match spots.as_slice() {
        &[spot] => spot + fix.before.len(),
        _ => now.match_indices(fix.old.as_str()).nth(occurrence)?.0,
    };
    let start = now[..at].chars().count() as i32;
    let len = fix.old.chars().count() as i32;
    log(&format!("  counting: starts at character {start}, {len} long"));
    // A fresh range over the whole document, shrunk to nothing at its start, then grown.
    let r = pattern.get_document_range().ok()?;
    r.move_endpoint_by_range(
        TextPatternRangeEndpoint::End,
        &r,
        TextPatternRangeEndpoint::Start,
    )
    .ok()?;
    let moved_end = r
        .move_endpoint_by_unit(TextPatternRangeEndpoint::End, TextUnit::Character, start + len)
        .ok()?;
    let moved_start = r
        .move_endpoint_by_unit(TextPatternRangeEndpoint::Start, TextUnit::Character, start)
        .ok()?;
    if moved_end != start + len || moved_start != start {
        log(&format!("  counting moved {moved_start}/{moved_end} instead"));
        return None;
    }
    Some(r)
}

/// Make one fix in the target box. Ok carries a short note for the control panel; Err means
/// nothing was changed.
fn apply(t: &Target, fix: &Replace) -> Result<String, String> {
    if fix.old.is_empty() || fix.old.contains('\n') || fix.new.contains('\n') {
        return Err("This fix crosses a line break, so it cannot be applied from here.".into());
    }
    // Typing goes to the window in front, so put the text box there first.
    if t.hwnd != 0 {
        crate::capture::bring_to_front(t.hwnd);
    }
    let _ = t.element.set_focus();
    thread::sleep(Duration::from_millis(120));

    if let Ok(pattern) = t.element.get_pattern::<UITextPattern>() {
        let doc = pattern
            .get_document_range()
            .map_err(|_| "Could not read the other program's text.".to_string())?;
        let now = doc
            .get_text(MAX_CHARS)
            .map_err(|_| "Could not read the other program's text.".to_string())?;
        let occurrence = which_match(&normalize(&now), fix)?;
        log(&format!("  TextPattern: changing match {occurrence} of {:?}", fix.old));
        // Find the flagged text, trying three ways and keeping the first that gives exactly the
        // flagged text: (1) search for it with the words around it, (2) search for it alone,
        // (3) count characters from the start of the document. Some programs (WordCraft and
        // other apps whose text comes through AccessKit) do not support searching at all, so
        // the third way needs no search.
        let is_old = |r: &UITextRange| {
            r.get_text(MAX_CHARS).map(|s| normalize(&s)).ok().as_deref() == Some(fix.old.as_str())
        };
        let mut hit = None;
        if let Some(r) = find_with_context(&doc, fix) {
            if is_old(&r) {
                log("  found it by its surroundings");
                hit = Some(r);
            }
        }
        if hit.is_none() {
            log("  not found by its surroundings, searching for the flagged text alone");
            if let Ok(fresh) = pattern.get_document_range() {
                match find_nth(&fresh, &fix.old, occurrence) {
                    Ok(r) if is_old(&r) => {
                        log("  found it alone");
                        hit = Some(r);
                    }
                    Ok(r) => log(&format!(
                        "  search returned {:?}",
                        r.get_text(MAX_CHARS).unwrap_or_default()
                    )),
                    Err(_) => log("  search not supported or no match"),
                }
            }
        }
        if hit.is_none() {
            log("  counting characters from the start instead");
            match by_offset(&pattern, &now, fix, occurrence) {
                Some(r) if is_old(&r) => {
                    log("  found it by counting");
                    hit = Some(r);
                }
                Some(r) => log(&format!(
                    "  counting landed on {:?}",
                    r.get_text(MAX_CHARS).unwrap_or_default()
                )),
                None => log("  counting not supported"),
            }
        }
        let Some(hit) = hit else {
            return Err(NOT_FOUND.into());
        };
        // Ask the program to select the flagged text. Some programs (WordCraft) ignore this and
        // leave the cursor where you last clicked, so nothing is typed until the cursor is
        // known to be at the flagged text.
        let _ = hit.select();
        thread::sleep(Duration::from_millis(80));
        let now_n = normalize(&now);
        let Some(at) = spot(&now_n, fix, occurrence) else {
            return Err(NOT_FOUND.into());
        };
        let start = now_n[..at].chars().count();
        let old_len = fix.old.chars().count();
        let end = start + old_len;
        let selection = selection_offsets(&pattern);
        log(&format!(
            "  flagged text is characters {start}..{end}; selection after select(): {selection:?}"
        ));
        if selection == Some((start, end)) {
            if fix.new.is_empty() {
                crate::capture::press_delete();
            } else {
                crate::capture::type_text(&fix.new);
            }
        } else {
            // Move the cursor to just after the flagged text with the arrow keys, check that it
            // got there, then remove the old text with Backspace and type the fix.
            if !move_caret_to(&pattern, end) {
                return Err("Could not move the cursor to the flagged text.".into());
            }
            crate::capture::press_key(crate::capture::EditKey::Backspace, old_len);
            if !fix.new.is_empty() {
                crate::capture::type_text(&fix.new);
            }
        }
        // What the text should read now.
        let replaced = format!("{}{}{}", &now_n[..at], fix.new, &now_n[at + fix.old.len()..]);
        // Wait (up to a second and a half) to see the change.
        let mut after = now_n.clone();
        for _ in 0..15 {
            thread::sleep(Duration::from_millis(100));
            if let Ok(a) = pattern
                .get_document_range()
                .and_then(|d| d.get_text(MAX_CHARS))
                .map(|s| normalize(&s))
            {
                after = a;
                if after == replaced {
                    return Ok("Fixed.".into());
                }
            }
        }
        if after == now_n {
            return Err("The other program did not take the change.".into());
        }
        log("  text after the fix is not what was expected");
        return Ok("Changed, but not as expected. Check the result (Ctrl+Z undoes it).".into());
    }
    if let Ok(pattern) = t.element.get_pattern::<UIValuePattern>() {
        if pattern.is_readonly().unwrap_or(true) {
            return Err("That text box cannot be edited.".into());
        }
        let now = pattern
            .get_value()
            .map_err(|_| "Could not read the other program's text.".to_string())?;
        let occurrence = which_match(&normalize(&now), fix)?;
        log(&format!("  ValuePattern: changing match {occurrence} of {:?}", fix.old));
        let Some((at, _)) = now.match_indices(fix.old.as_str()).nth(occurrence) else {
            return Err(NOT_FOUND.into());
        };
        let mut value = now.clone();
        value.replace_range(at..at + fix.old.len(), &fix.new);
        pattern
            .set_value(&value)
            .map_err(|_| "The other program would not take the change.".to_string())?;
        return Ok("Fixed.".into());
    }
    Err("This program does not let other programs change its text.".into())
}

/// The focused text box, if there is one we may and can read. Our own windows, password fields
/// and controls that share no text give `None`, which leaves the last snapshot in place.
fn read(automation: &UIAutomation, own_pid: u32) -> Option<(Snapshot, UIElement)> {
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
        return Some((
            Snapshot {
                text,
                selected,
                process_id,
                control,
                name,
                how: "TextPattern",
            },
            element,
        ));
    }
    if let Ok(pattern) = element.get_pattern::<UIValuePattern>() {
        let text = pattern.get_value().ok()?;
        return Some((
            Snapshot {
                text,
                selected: String::new(),
                process_id,
                control,
                name,
                how: "ValuePattern",
            },
            element,
        ));
    }
    None
}
