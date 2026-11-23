//! Turning a Google Doc's structure into plain text and back.
//!
//! The Docs API describes a document as paragraphs of "text runs", each starting at an index
//! counted in UTF-16 code units. The checker works on plain text with byte offsets. This keeps
//! the two in step, so a flagged range in the plain text can become an edit in the document.
//! Nothing here talks to Google or to Windows, so it can be tested on its own.

use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// Where the run starts in the document (UTF-16 units).
    pub doc_start: usize,
    /// Where its text starts in the plain text (bytes).
    pub plain_start: usize,
    /// Its length in the plain text (bytes).
    pub len: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocText {
    pub text: String,
    pub runs: Vec<Run>,
}

/// Build the plain text from `(document index, content)` pairs in document order.
pub fn build(runs: &[(usize, String)]) -> DocText {
    let mut out = DocText::default();
    for (doc_start, content) in runs {
        let plain_start = out.text.len();
        // A soft line break (vertical tab) is one byte, like the newline that replaces it.
        out.text
            .extend(content.chars().map(|c| if c == '\u{b}' { '\n' } else { c }));
        out.runs.push(Run {
            doc_start: *doc_start,
            plain_start,
            len: out.text.len() - plain_start,
        });
    }
    out
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

impl DocText {
    /// The document index for a byte offset in the plain text.
    fn doc_index(&self, offset: usize) -> Option<usize> {
        let run = self
            .runs
            .iter()
            .find(|r| offset >= r.plain_start && offset < r.plain_start + r.len)
            .or_else(|| {
                self.runs
                    .iter()
                    .find(|r| offset == r.plain_start + r.len && r.len > 0)
            })?;
        let before = self.text.get(run.plain_start..offset)?;
        Some(run.doc_start + utf16_len(before))
    }

    /// The document range for a plain-text range. `None` when the text between is not one
    /// unbroken stretch of the document (a table edge or a picture sits inside it), because then
    /// an edit could land in the wrong place.
    pub fn to_doc_range(&self, range: Range<usize>) -> Option<Range<usize>> {
        if range.start >= range.end {
            return None;
        }
        let start = self.doc_index(range.start)?;
        let end = self.doc_index(range.end)?;
        let wanted = utf16_len(self.text.get(range)?);
        (end > start && end - start == wanted).then_some(start..end)
    }
}

/// The document id in a web address such as `docs.google.com/document/d/ID/edit`. Firefox hides
/// the `https://` part in the address bar, so this does not look for it.
pub fn id_in_address(address: &str) -> Option<String> {
    let after_docs = &address[address.find("docs.google.com/document")?..];
    let after_d = &after_docs[after_docs.find("/d/")? + 3..];
    let id: String = after_d
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (id.len() >= 20).then_some(id)
}

/// The tab in a Docs address (`...?tab=t.abc123`), when the document has tabs.
pub fn tab_in_address(address: &str) -> Option<String> {
    let after = &address[address.find("tab=")? + 4..];
    let tab: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    (!tab.is_empty()).then_some(tab)
}

/// What to type into the document's Find to land on `range` of `text`: the flagged text alone
/// when it occurs once, otherwise with a few neighbouring words from the same line until it does.
pub fn find_needle(text: &str, range: Range<usize>) -> Option<String> {
    let flagged = text.get(range.clone())?;
    if flagged.trim().is_empty() {
        return None;
    }
    let line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[range.end..]
        .find('\n')
        .map_or(text.len(), |i| range.end + i);
    let lower = text.to_lowercase();
    let mut best = flagged.to_string();
    for words in 0..=4 {
        let mut start = range.start;
        let mut end = range.end;
        for _ in 0..words {
            let before = &text[line_start..start];
            start = before
                .trim_end()
                .rfind(' ')
                .map_or(line_start, |i| line_start + i + 1);
            let after = &text[end..line_end];
            let skip = after.len() - after.trim_start().len();
            end = after[skip..].find(' ').map_or(line_end, |i| end + skip + i);
        }
        let candidate = text[start..end].trim();
        if candidate.chars().count() > 90 {
            break;
        }
        best = candidate.to_string();
        if lower.matches(&best.to_lowercase()).count() <= 1 {
            break;
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(parts: &[(usize, &str)]) -> DocText {
        build(
            &parts
                .iter()
                .map(|(i, s)| (*i, s.to_string()))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn maps_a_range_inside_one_run() {
        let d = doc(&[(1, "Hello wrold.\n")]);
        // "wrold" is bytes 6..11, document indices 7..12.
        assert_eq!(d.to_doc_range(6..11), Some(7..12));
    }

    #[test]
    fn maps_across_runs_that_touch() {
        // "Hello " (1..7) then "wrold" (7..12), as two runs with different styling.
        let d = doc(&[(1, "Hello "), (7, "wrold.\n")]);
        assert_eq!(d.to_doc_range(3..9), Some(4..10));
    }

    #[test]
    fn counts_utf16_units_for_emoji_and_accents() {
        // The emoji is 4 bytes but 2 UTF-16 units; "caf\u{e9}" is 5 bytes but 4 units.
        let d = doc(&[(1, "\u{1F600} caf\u{e9} wrold\n")]);
        let start = d.text.find("wrold").unwrap();
        let range = d.to_doc_range(start..start + 5).unwrap();
        assert_eq!(range, (1 + 2 + 1 + 4 + 1)..(1 + 2 + 1 + 4 + 1 + 5));
    }

    #[test]
    fn refuses_a_range_with_a_gap_inside() {
        // Index 7 is something that is not text (a picture): the runs do not touch.
        let d = doc(&[(1, "Hello "), (8, "wrold.\n")]);
        assert_eq!(d.to_doc_range(3..9), None);
    }

    #[test]
    fn soft_line_break_becomes_newline() {
        let d = doc(&[(1, "a\u{b}b\n")]);
        assert_eq!(d.text, "a\nb\n");
    }

    #[test]
    fn finds_the_document_id() {
        let id = "1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-aBc";
        assert_eq!(
            id_in_address(&format!("docs.google.com/document/d/{id}/edit?tab=t.0")).as_deref(),
            Some(id)
        );
        assert_eq!(
            id_in_address(&format!("https://docs.google.com/document/u/0/d/{id}/edit")).as_deref(),
            Some(id)
        );
        assert_eq!(id_in_address("docs.google.com/document/"), None);
        assert_eq!(id_in_address("example.com/d/abc"), None);
    }

    #[test]
    fn finds_the_tab() {
        assert_eq!(
            tab_in_address("docs.google.com/document/d/x/edit?tab=t.k2f9a8").as_deref(),
            Some("t.k2f9a8")
        );
        assert_eq!(tab_in_address("docs.google.com/document/d/x/edit"), None);
    }

    #[test]
    fn needle_grows_until_unique() {
        let text = "He said teh word. Later she said teh thing.\nOther line.";
        let first = text.find("teh").unwrap();
        assert_eq!(
            find_needle(text, first..first + 3).as_deref(),
            Some("said teh word.")
        );
        let unique = "A lone mistake here.\nOther.";
        let m = unique.find("mistake").unwrap();
        assert_eq!(find_needle(unique, m..m + 7).as_deref(), Some("mistake"));
    }
}
