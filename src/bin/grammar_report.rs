//! How good is the grammar checker? This runs every sentence in `grammar_cases/*.tsv` through
//! the checker and reports two numbers:
//!
//! * **Caught** (recall): of the sentences with a known mistake, how many were flagged.
//! * **False alarms** (precision): of the correct sentences, how many were flagged by mistake.
//!
//! Run it with `cargo run --bin grammar_report`. A good change to the checker raises "caught"
//! without raising "false alarms". Pass `-v` to list every miss and every false alarm.
//! Spelling (the dictionary) is not part of this check; it uses only the grammar rules.

#[allow(dead_code)]
#[path = "../analysis.rs"]
mod analysis;

use analysis::{EnabledCategories, check_grammar};

fn main() {
    let verbose = std::env::args().any(|a| a == "-v");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("grammar_cases");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    files.sort();
    let on = EnabledCategories::default();
    let (mut bad_total, mut bad_caught, mut good_total, mut good_flagged) = (0, 0, 0, 0);
    let (mut skipped_bad, mut skipped_caught) = (0, 0);
    let mut misses: Vec<String> = Vec::new();
    let mut alarms: Vec<String> = Vec::new();
    for file in files {
        if file.extension().and_then(|e| e.to_str()) != Some("tsv") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            let [id, topic, kind, sentence, skip] = match cols.as_slice() {
                [a, b, c, d, e] => [*a, *b, *c, *d, *e],
                [a, b, c, d] => [*a, *b, *c, *d, ""],
                _ => continue,
            };
            let issues = check_grammar(sentence, &on);
            let flagged = !issues.is_empty();
            let what = issues
                .iter()
                .map(|i| i.title)
                .collect::<Vec<_>>()
                .join(", ");
            match kind {
                "bad" if !skip.is_empty() => {
                    skipped_bad += 1;
                    skipped_caught += usize::from(flagged);
                }
                "bad" => {
                    bad_total += 1;
                    if flagged {
                        bad_caught += 1;
                    } else {
                        misses.push(format!("#{id} {topic}: {sentence}"));
                    }
                }
                "good" => {
                    good_total += 1;
                    if flagged {
                        good_flagged += 1;
                        alarms.push(format!("#{id} {topic}: {sentence}   -> {what}"));
                    }
                }
                _ => {}
            }
        }
    }
    let pct = |a: usize, b: usize| if b == 0 { 0.0 } else { a as f64 * 100.0 / b as f64 };
    println!(
        "Caught:       {bad_caught} of {bad_total} sentences with a mistake ({:.0}%)",
        pct(bad_caught, bad_total)
    );
    println!(
        "False alarms: {good_flagged} of {good_total} correct sentences ({:.0}%)",
        pct(good_flagged, good_total)
    );
    println!("(Not counted: {skipped_bad} doubtful cases, of which {skipped_caught} were flagged.)");
    if verbose {
        println!("\nMissed:");
        for m in &misses {
            println!("  {m}");
        }
        println!("\nFalse alarms:");
        for a in &alarms {
            println!("  {a}");
        }
    }
}
