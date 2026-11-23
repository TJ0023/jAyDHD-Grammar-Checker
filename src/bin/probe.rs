//! A throwaway test: what can Windows tell us about the window you are typing in?
//!
//! Run it with `cargo run --bin probe`, then click into Notepad (or Word, a Chrome text box,
//! Discord, VS Code...) and type or select some text. Every time something changes it prints
//! what UI Automation can see: the kind of control, whether it offers the text, and the text
//! and selection themselves. Press Ctrl+C in this window to stop.
//!
//! It only reads. It never changes anything in the other window, and it skips password fields.

#[cfg(windows)]
fn main() {
    use std::thread;
    use std::time::Duration;

    use uiautomation::UIAutomation;

    let automation = match UIAutomation::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Could not start UI Automation: {e}");
            return;
        }
    };
    println!("Probe running. Click into the window you want to test, then type or select text.");
    println!("(The first line you see may describe this console window itself.)\n");

    let mut last = String::new();
    loop {
        let report = match automation.get_focused_element() {
            Ok(element) => describe(&element),
            Err(e) => format!("No focused element ({e})"),
        };
        if report != last {
            println!("{report}\n{}", "-".repeat(60));
            last = report;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// Everything worth knowing about the focused control, as a few lines of text.
#[cfg(windows)]
fn describe(element: &uiautomation::UIElement) -> String {
    use uiautomation::patterns::{UITextPattern, UIValuePattern};

    let name = element.get_name().unwrap_or_default();
    let class = element.get_classname().unwrap_or_default();
    let kind = element
        .get_control_type()
        .map_or("?".to_string(), |k| format!("{k:?}"));
    let pid = element
        .get_process_id()
        .map_or("?".to_string(), |p| p.to_string());
    let mut out = format!("Control: {kind}  class: {class}  name: {name:?}  process id: {pid}");

    // Never read password fields.
    if element.is_password().unwrap_or(false) {
        out.push_str("\nPassword field: skipped.");
        return out;
    }

    let mut got_text = false;
    match element.get_pattern::<UITextPattern>() {
        Ok(text) => {
            out.push_str("\nTextPattern: YES");
            got_text = true;
            match text.get_document_range().and_then(|r| r.get_text(2000)) {
                Ok(t) => out.push_str(&format!(
                    "\n  text ({} chars shown, max 2000): {}",
                    t.chars().count(),
                    shorten(&t)
                )),
                Err(e) => out.push_str(&format!("\n  could not read the text: {e}")),
            }
            match text.get_selection() {
                Ok(ranges) if ranges.is_empty() => out.push_str("\n  selection: none"),
                Ok(ranges) => {
                    for r in ranges {
                        let sel = r.get_text(500).unwrap_or_default();
                        out.push_str(&format!("\n  selection: {:?}", shorten(&sel)));
                    }
                }
                Err(e) => out.push_str(&format!("\n  could not read the selection: {e}")),
            }
        }
        Err(_) => out.push_str("\nTextPattern: no"),
    }
    match element.get_pattern::<UIValuePattern>() {
        Ok(value) => {
            out.push_str("\nValuePattern: YES");
            if !got_text {
                match value.get_value() {
                    Ok(v) => out.push_str(&format!("\n  value: {}", shorten(&v))),
                    Err(e) => out.push_str(&format!("\n  could not read the value: {e}")),
                }
            }
        }
        Err(_) => out.push_str("\nValuePattern: no"),
    }
    if !got_text && out.contains("ValuePattern: no") {
        out.push_str(
            "\nThis control does not share its text. It would need a bridge or the clipboard.",
        );
    }
    out
}

/// The first 200 characters, on one line.
#[cfg(windows)]
fn shorten(s: &str) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let mut short: String = flat.chars().take(200).collect();
    if flat.chars().count() > 200 {
        short.push('\u{2026}');
    }
    format!("{short:?}")
}

#[cfg(not(windows))]
fn main() {
    println!("The probe only runs on Windows.");
}
