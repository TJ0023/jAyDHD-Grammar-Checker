//! Developer tool: run the grammar rules over a file of sentences (one per line) and print every
//! flag as tab-separated columns: line number, start, end, rule, category, flagged text, fix.
//! Used with `scripts/jfleg_report.py` to compare the checker against the JFLEG test set.
//! Usage: `cargo run --bin flag_dump -- path/to/sentences.txt`

#[allow(dead_code)]
#[path = "../analysis.rs"]
mod analysis;

use analysis::{EnabledCategories, check_grammar};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: flag_dump <file with one sentence per line>");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let on = EnabledCategories::default();
    for (n, line) in text.lines().enumerate() {
        for issue in check_grammar(line, &on) {
            let flagged = line.get(issue.range.clone()).unwrap_or("");
            println!(
                "{}\t{}\t{}\t{}\t{:?}\t{}\t{}",
                n,
                issue.range.start,
                issue.range.end,
                issue.rule,
                issue.category,
                flagged,
                issue.replacement.as_deref().unwrap_or("")
            );
        }
    }
}
