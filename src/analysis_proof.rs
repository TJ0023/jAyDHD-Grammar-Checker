//! The writing boxes' full check: the rules in [`crate::analysis`] plus the spelling checker
//! WordCraft already uses for its red squiggles (`craft_proof`), so a typo shows up in the
//! Grammar Checker box with the dictionary's best suggestion. Words you added to your own
//! dictionary are respected, because that is the same checker.

use std::ops::Range;

use crate::analysis::{self, Category, EnabledCategories, Issue};

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// Grammar rules and spelling for one paragraph, in document order. Where the dictionary and
/// a grammar rule flag the same words, the rule wins: it has the more specific explanation.
pub fn check_all(text: &str, on: &EnabledCategories) -> Vec<Issue> {
    let mut out = analysis::check_grammar(text, on);
    if on.word_choice {
        for found in craft_proof::check_spelling(text) {
            if found.kind != craft_proof::IssueKind::Spelling {
                continue;
            }
            let range = found.start..found.end;
            if out.iter().any(|i| overlaps(&i.range, &range)) {
                continue;
            }
            let word = text.get(range.clone()).unwrap_or("");
            let replacement = craft_proof::suggest(word, 1).into_iter().next();
            out.push(Issue {
                range,
                category: Category::WordChoice,
                rule: "spelling",
                title: "Spelling",
                explanation: format!("'{word}' is not in the dictionary."),
                replacement,
            });
        }
    }
    out.sort_by_key(|i| (i.range.start, i.range.end));
    out
}
