//! Dialogue formatting: a rule checker for the punctuation and capitals around spoken lines and
//! their tags, plus the guide the Dialogue Formatting box opens with "Read More".
//!
//! Pure text in, plain data out, like [`crate::analysis`]: byte ranges into one paragraph. Nothing
//! here panics; every slice goes through `str::get`. The rules are deliberately conservative, and
//! every example in the guide is tested against them, so the guide and the checker agree.

use std::ops::Range;

/// Which quotation marks mark speech.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuoteStyle {
    /// "Like this," she said. American style, and the default.
    Double,
    /// 'Like this,' she said. British style.
    Single,
}

impl QuoteStyle {
    pub fn label(self) -> &'static str {
        match self {
            QuoteStyle::Double => "Double quotes",
            QuoteStyle::Single => "Single quotes",
        }
    }
}

fn opens_a_quote(prev: Option<char>) -> bool {
    match prev {
        None => true,
        Some(c) => {
            c.is_whitespace()
                || matches!(
                    c,
                    '(' | '['
                        | '{'
                        | '\u{2014}'
                        | '\u{2013}'
                        | '-'
                        | '\u{201C}'
                        | '\u{2018}'
                        | '"'
                        | '\''
                )
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    DOpen,
    DClose,
    SOpen,
    SClose,
}

impl Kind {
    fn double(self) -> bool {
        matches!(self, Kind::DOpen | Kind::DClose)
    }
    fn opens(self) -> bool {
        matches!(self, Kind::DOpen | Kind::SOpen)
    }
}

/// One quotation mark found in the text.
#[derive(Clone, Copy, Debug)]
struct Mark {
    at: usize,
    len: usize,
    ch: char,
    kind: Kind,
}

/// How the quotation marks of a paragraph pair up, whatever kind they are.
#[derive(Default)]
struct Pairing {
    /// Quotations at the outermost level that were opened and closed properly, by kind.
    doubles: usize,
    singles: usize,
    /// An opening mark closed by the other kind of mark: single opened and double closed, or the
    /// reverse. Nothing else pairs with either of them.
    mismatches: Vec<(Mark, Mark)>,
    /// A curly single opening mark that never closes, inside a quotation that does.
    loose_singles: Vec<Mark>,
}

fn clause_punctuation(c: char) -> bool {
    matches!(
        c,
        ',' | '.' | '!' | '?' | ';' | ':' | '\u{2014}' | '\u{2013}' | '\u{2026}'
    )
}

/// Pair up every quotation mark, single or double, straight or curly, the way a reader would. A
/// single mark inside a double quotation is a nested quotation, and an apostrophe is not a mark.
fn pair(text: &str) -> Pairing {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Pairing::default();
    let mut stack: Vec<Mark> = Vec::new();
    let (mut open_d, mut open_s) = (0usize, 0usize);
    for (k, &(at, c)) in chars.iter().enumerate() {
        if !matches!(
            c,
            '\u{201C}' | '\u{201D}' | '\u{2018}' | '\u{2019}' | '"' | '\''
        ) {
            continue;
        }
        let prev = k.checked_sub(1).and_then(|p| chars.get(p)).map(|x| x.1);
        let next = chars.get(k + 1).map(|x| x.1);
        let between_letters =
            prev.is_some_and(char::is_alphabetic) && next.is_some_and(char::is_alphabetic);
        let after_clause = prev.is_some_and(clause_punctuation);
        let kind = match c {
            '\u{201C}' => Kind::DOpen,
            '\u{201D}' => Kind::DClose,
            '"' => {
                if open_d > 0
                    && matches!(prev, Some('\u{2014}' | '\u{2013}'))
                    && !next.is_some_and(char::is_alphanumeric)
                {
                    Kind::DClose
                } else if opens_a_quote(prev) {
                    Kind::DOpen
                } else {
                    Kind::DClose
                }
            }
            '\u{2018}' => {
                if between_letters {
                    continue;
                }
                Kind::SOpen
            }
            // A closing single mark is also the apostrophe, so it only counts as a mark when
            // something is waiting to be closed, or when it follows punctuation like a mark does.
            '\u{2019}' => {
                if between_letters || !(open_s > 0 || after_clause) {
                    continue;
                }
                Kind::SClose
            }
            _ => {
                if between_letters {
                    continue;
                }
                if opens_a_quote(prev) && next.is_some_and(char::is_alphanumeric) {
                    Kind::SOpen
                } else if open_s > 0 || after_clause {
                    Kind::SClose
                } else {
                    continue;
                }
            }
        };
        let mark = Mark {
            at,
            len: c.len_utf8(),
            ch: c,
            kind,
        };
        if kind.opens() {
            stack.push(mark);
            if kind.double() {
                open_d += 1
            } else {
                open_s += 1
            }
            continue;
        }
        let waiting = if kind.double() { open_d } else { open_s };
        match stack.last().copied() {
            Some(top) if top.kind.double() == kind.double() => {
                stack.pop();
                if kind.double() {
                    open_d -= 1
                } else {
                    open_s -= 1
                }
                if stack.is_empty() {
                    if kind.double() {
                        out.doubles += 1
                    } else {
                        out.singles += 1
                    }
                }
            }
            Some(_) if waiting > 0 => {
                // Something of the same kind further down is the real opener: the marks above it
                // were never closed.
                if let Some(pos) = stack.iter().rposition(|m| m.kind.double() == kind.double()) {
                    for m in stack.drain(pos + 1..) {
                        if m.kind.double() {
                            open_d -= 1
                        } else {
                            open_s -= 1
                        }
                        if m.ch == '\u{2018}' {
                            out.loose_singles.push(m);
                        }
                    }
                    stack.pop();
                    if kind.double() {
                        open_d -= 1
                    } else {
                        open_s -= 1
                    }
                    if stack.is_empty() {
                        if kind.double() {
                            out.doubles += 1
                        } else {
                            out.singles += 1
                        }
                    }
                }
            }
            Some(top) => {
                stack.pop();
                if top.kind.double() {
                    open_d -= 1
                } else {
                    open_s -= 1
                }
                out.mismatches.push((top, mark));
            }
            None => {}
        }
    }
    out
}

/// Which style the text uses, by counting the quotations that are opened and closed properly with
/// each kind of mark. A mark that does not pair up (a typo, a leftover) counts for nothing, and
/// ties, and text with no quotations, count as double.
pub fn detect_style(text: &str) -> QuoteStyle {
    let p = pair(text);
    if p.singles > p.doubles {
        QuoteStyle::Single
    } else {
        QuoteStyle::Double
    }
}

/// One suspected formatting error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogueIssue {
    /// Byte range of the flagged text.
    pub range: Range<usize>,
    /// Stable rule id, used with the flagged text to remember "Ignore".
    pub rule: &'static str,
    /// The box's "Issue:" line.
    pub title: &'static str,
    pub explanation: String,
    /// What the flagged text should become, when the fix is clear.
    pub replacement: Option<String>,
    /// The guide section that explains it (see [`GUIDE`]).
    pub guide: &'static str,
    /// An edit that is not just "replace `range` with `replacement`": a fix that adds a mark
    /// somewhere else, such as a missing closing quotation mark at the end of the paragraph.
    pub edit: Option<(Range<usize>, String)>,
}

impl DialogueIssue {
    /// The change that fixes the issue: the byte range to replace and what to put there. `None`
    /// only when no fix is known; every rule currently has one.
    pub fn edit(&self) -> Option<(Range<usize>, String)> {
        match (&self.edit, &self.replacement) {
            (Some(e), _) => Some(e.clone()),
            (None, Some(r)) => Some((self.range.clone(), r.clone())),
            _ => None,
        }
    }

    pub fn ignore_key(&self, text: &str) -> String {
        format!(
            "{}|{}",
            self.rule,
            text.get(self.range.clone()).unwrap_or("")
        )
    }
}

const PRONOUNS: &[&str] = &["he", "she", "they", "we", "i", "it", "you"];

const SPEECH_VERBS: &[&str] = &[
    "said",
    "says",
    "asked",
    "asks",
    "replied",
    "replies",
    "answered",
    "answers",
    "whispered",
    "whispers",
    "shouted",
    "shouts",
    "yelled",
    "yells",
    "cried",
    "cries",
    "called",
    "calls",
    "muttered",
    "mutters",
    "murmured",
    "murmurs",
    "exclaimed",
    "declared",
    "added",
    "adds",
    "continued",
    "continues",
    "remarked",
    "snapped",
    "demanded",
    "insisted",
    "suggested",
    "offered",
    "admitted",
    "agreed",
    "warned",
    "urged",
    "pleaded",
    "begged",
    "explained",
    "announced",
    "interrupted",
    "stammered",
    "groaned",
    "hissed",
    "growled",
    "barked",
    "roared",
    "bellowed",
    "gasped",
    "wondered",
    "greeted",
    "told",
];

/// Things a person does that are not a way of speaking: "she smiled" cannot follow a comma as a tag.
const ACTION_VERBS: &[&str] = &[
    "smiled", "nodded", "grinned", "shrugged", "frowned", "grimaced", "winced", "laughed",
    "giggled", "chuckled", "sighed", "stood", "turned", "walked", "looked", "glanced", "rose",
    "stepped", "crossed", "waved", "smirked", "blushed", "paused", "stared", "blinked", "leaned",
    "sat", "bowed", "winked",
];

const COMMON_STARTS: &[&str] = &[
    "the", "a", "an", "that", "this", "these", "those", "it", "its", "what", "who", "how", "why",
    "where", "when", "which", "we", "you", "they", "he", "she", "there", "here", "if", "and",
    "but", "so", "to", "of", "in", "on", "at", "for", "with", "well", "now", "oh", "yes", "no",
    "please", "can", "could", "would", "should", "do", "does", "did", "is", "are", "was", "were",
    "will", "just", "not",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum TagKind {
    /// "she said", "Tom asked"
    Speech,
    /// "said Tom"
    Inverted,
    /// "she smiled": an action, not a tag
    Action,
}

struct Tag {
    kind: TagKind,
    /// A comma sits outside the closing quote.
    comma_outside: bool,
    /// The first word after the quote, and whether it is a pronoun.
    word: Range<usize>,
    pronoun: bool,
}

struct Quote {
    open: usize,
    open_len: usize,
    close: Option<(usize, usize)>,
}

fn word_at(text: &str, start: usize) -> Option<Range<usize>> {
    let rest = text.get(start..)?;
    let mut it = rest.char_indices().peekable();
    let mut end = 0;
    while let Some((i, c)) = it.next() {
        if c.is_alphabetic() {
            end = i + c.len_utf8();
        } else if end > 0
            && matches!(c, '\'' | '\u{2019}' | '-')
            && it.peek().is_some_and(|&(_, n)| n.is_alphabetic())
        {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        None
    } else {
        Some(start..start + end)
    }
}

fn capitalized(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
}

/// What follows a closing quote at `from`: a dialogue tag ("she said", "said Tom", "Tom asked"), an
/// action beat ("she smiled"), or neither.
fn parse_tag(text: &str, from: usize) -> Option<Tag> {
    let after = text.get(from..)?;
    let comma_outside = after.starts_with(',');
    let skip = usize::from(comma_outside);
    let rest = after.get(skip..)?;
    let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    if spaces == 0 {
        return None;
    }
    let word = word_at(text, from + skip + spaces)?;
    let first = text.get(word.clone())?;
    let lower = first.to_lowercase();
    let second = {
        let tail = text.get(word.end..)?;
        let gap = tail.len() - tail.trim_start_matches(' ').len();
        if gap > 0 {
            word_at(text, word.end + gap)
                .and_then(|r| text.get(r))
                .map(str::to_lowercase)
        } else {
            None
        }
    };
    let pronoun = PRONOUNS.contains(&lower.as_str());
    let kind = if pronoun || capitalized(first) {
        match second.as_deref() {
            Some(v) if SPEECH_VERBS.contains(&v) => TagKind::Speech,
            Some(v) if ACTION_VERBS.contains(&v) => TagKind::Action,
            _ if SPEECH_VERBS.contains(&lower.as_str()) => TagKind::Inverted,
            _ => return None,
        }
    } else if SPEECH_VERBS.contains(&lower.as_str()) {
        TagKind::Inverted
    } else {
        return None;
    };
    Some(Tag {
        kind,
        comma_outside,
        word,
        pronoun,
    })
}

fn lowercase_first(word: &str) -> String {
    let mut it = word.chars();
    match it.next() {
        Some(c) => c.to_lowercase().chain(it).collect(),
        None => String::new(),
    }
}

fn uppercase_first(word: &str) -> String {
    let mut it = word.chars();
    match it.next() {
        Some(c) => c.to_uppercase().chain(it).collect(),
        None => String::new(),
    }
}

fn scan(text: &str, style: QuoteStyle) -> (Vec<Quote>, Vec<(usize, usize)>) {
    let (curly_open, curly_close, straight) = match style {
        QuoteStyle::Double => ('\u{201C}', '\u{201D}', '"'),
        QuoteStyle::Single => ('\u{2018}', '\u{2019}', '\''),
    };
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut quotes: Vec<Quote> = Vec::new();
    let mut strays: Vec<(usize, usize)> = Vec::new();
    let mut open: Option<usize> = None;
    for (k, &(at, c)) in chars.iter().enumerate() {
        if c != curly_open && c != curly_close && c != straight {
            continue;
        }
        let prev = k.checked_sub(1).and_then(|p| chars.get(p)).map(|x| x.1);
        let next = chars.get(k + 1).map(|x| x.1);
        // An apostrophe inside a word is not a quotation mark.
        if style == QuoteStyle::Single
            && c != curly_open
            && prev.is_some_and(char::is_alphabetic)
            && next.is_some_and(char::is_alphabetic)
        {
            continue;
        }
        let opening = if c == curly_open {
            true
        } else if c == curly_close {
            false
        } else if open.is_some()
            && matches!(prev, Some('\u{2014}' | '\u{2013}'))
            && !next.is_some_and(char::is_alphanumeric)
        {
            // A straight mark right after a dash inside speech ends an interruption:
            // "I was just--" he began.
            false
        } else {
            opens_a_quote(prev)
        };
        let len = c.len_utf8();
        if opening {
            quotes.push(Quote {
                open: at,
                open_len: len,
                close: None,
            });
            open = Some(quotes.len() - 1);
        } else if let Some(i) = open.take() {
            if let Some(q) = quotes.get_mut(i) {
                q.close = Some((at, len));
            }
        } else {
            strays.push((at, len));
        }
    }
    (quotes, strays)
}

fn issue(
    range: Range<usize>,
    rule: &'static str,
    title: &'static str,
    explanation: &str,
    replacement: Option<String>,
    guide: &'static str,
) -> DialogueIssue {
    DialogueIssue {
        range,
        rule,
        title,
        explanation: explanation.to_string(),
        replacement,
        guide,
        edit: None,
    }
}

/// Check one paragraph of `text` written in `style`, in document order.
pub fn check(text: &str, style: QuoteStyle) -> Vec<DialogueIssue> {
    let (quotes, strays) = scan(text, style);
    let mut out: Vec<DialogueIssue> = Vec::new();

    for (qi, q) in quotes.iter().enumerate() {
        let Some((ci, cl)) = q.close else { continue };
        let inner_start = q.open + q.open_len;
        let inner = text.get(inner_start..ci).unwrap_or("");
        let trimmed = inner.trim_end();
        let mut tail = trimmed.chars().rev();
        let last = tail.next();
        let before_last = tail.next();
        let last_at = inner_start + trimmed.len() - last.map_or(0, char::len_utf8);
        let closer = text.get(ci..ci + cl).unwrap_or("");
        let tag = parse_tag(text, ci + cl);

        // "Hello." she said  ->  "Hello," she said
        if last == Some('.') && !matches!(before_last, Some('.' | '?' | '!')) {
            if let Some(t) = &tag {
                if !t.comma_outside && t.kind != TagKind::Action {
                    let first = text.get(t.word.clone()).unwrap_or("");
                    let fixed = if t.pronoun && first != "I" {
                        lowercase_first(first)
                    } else {
                        first.to_string()
                    };
                    let gap = text.get(ci + cl..t.word.start).unwrap_or(" ");
                    out.push(issue(
                        last_at..t.word.end,
                        "tag-period",
                        "Period Before a Tag",
                        "A dialogue tag continues the sentence, so the speech ends with a comma, not a period.",
                        Some(format!(",{closer}{gap}{fixed}")),
                        "tag-punctuation",
                    ));
                }
            }
        }
        // "Hello," She said  ->  "Hello," she said
        if matches!(last, Some(',' | '?' | '!')) {
            if let Some(t) = &tag {
                if t.kind == TagKind::Speech && t.pronoun {
                    if let Some(w) = text.get(t.word.clone()) {
                        if capitalized(w) && w != "I" {
                            out.push(issue(
                                t.word.clone(),
                                "tag-capital",
                                "Capital Letter in a Tag",
                                "A dialogue tag continues the sentence, so it starts with a lowercase letter. Names stay capitalized.",
                                Some(lowercase_first(w)),
                                "lowercase-tags",
                            ));
                        }
                    }
                }
            }
        }
        // "Hello", she said  ->  "Hello," she said   (American style)
        if style == QuoteStyle::Double
            && !matches!(
                last,
                Some('.' | '?' | '!' | ',' | '\u{2014}' | '\u{2013}' | ':' | ';') | None
            )
        {
            if let Some(t) = &tag {
                if t.comma_outside {
                    out.push(issue(
                        ci..ci + cl + 1,
                        "comma-outside",
                        "Comma Outside the Quotes",
                        "In American style, the comma goes inside the closing quotation mark.",
                        Some(format!(",{closer}")),
                        "tag-punctuation",
                    ));
                }
            }
        }
        // "Hello," she smiled.  ->  "Hello." She smiled.
        if last == Some(',') {
            if let Some(t) = &tag {
                if t.kind == TagKind::Action && !t.comma_outside {
                    let first = text.get(t.word.clone()).unwrap_or("");
                    let gap = text.get(ci + cl..t.word.start).unwrap_or(" ");
                    out.push(issue(
                        last_at..t.word.end,
                        "beat-comma",
                        "Action Used as a Tag",
                        "Smiling, nodding and the like are actions, not ways of speaking. End the speech with a period and start a new sentence, or use a real tag such as 'said'.",
                        Some(format!(".{closer}{gap}{}", uppercase_first(first))),
                        "beats-vs-tags",
                    ));
                }
            }
        }
        // "Where?," she asked  ->  "Where?" she asked
        if matches!(last, Some(',' | '.')) && matches!(before_last, Some('?' | '!' | '\u{2014}')) {
            let mark_len = before_last.map_or(0, char::len_utf8);
            let start = last_at - mark_len;
            if let Some(mark) = text.get(start..last_at) {
                out.push(issue(
                    start..last_at + 1,
                    "double-punctuation",
                    "Extra Punctuation",
                    "A question mark, exclamation mark or dash already ends the speech, so no comma or period follows it.",
                    Some(mark.to_string()),
                    "questions-exclamations",
                ));
            }
        }
        // "Well," she said, "That's odd."  ->  "Well," she said, "that's odd."
        if last == Some(',') {
            if let (Some(t), Some(next)) = (&tag, quotes.get(qi + 1)) {
                let joined = text
                    .get(ci + cl..next.open)
                    .is_some_and(|mid| mid.trim_end().ends_with(','));
                if t.kind != TagKind::Action && joined {
                    if let Some(w) = word_at(text, next.open + next.open_len) {
                        let word = text.get(w.clone()).unwrap_or("");
                        let base = word.split(['\'', '\u{2019}']).next().unwrap_or(word);
                        if capitalized(base)
                            && base != "I"
                            && COMMON_STARTS.contains(&base.to_lowercase().as_str())
                        {
                            let first_len = word.chars().next().map_or(0, char::len_utf8);
                            out.push(issue(
                                w.start..w.start + first_len,
                                "split-capital",
                                "Capital After a Tag",
                                "When a tag interrupts a sentence, the speech picks up in lowercase. A new sentence starts with a capital only if the first part ended with a period, question mark or exclamation mark.",
                                Some(lowercase_first(word).chars().take(1).collect()),
                                "split-dialogue",
                            ));
                        }
                    }
                }
            }
        }
    }

    // A pair whose two marks are different kinds, such as ‘To be or not to be.” : change the one
    // that is the wrong kind for the style, matching the other mark's curliness.
    let pairing = pair(text);
    let mut involved: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for (opener, closer) in &pairing.mismatches {
        involved.insert(opener.at);
        involved.insert(closer.at);
        let wrong = match style {
            QuoteStyle::Double => {
                if !opener.kind.double() {
                    opener
                } else {
                    closer
                }
            }
            QuoteStyle::Single => {
                if opener.kind.double() {
                    opener
                } else {
                    closer
                }
            }
        };
        let other = if std::ptr::eq(wrong, opener) {
            closer
        } else {
            opener
        };
        let curly = !matches!(other.ch, '"' | '\'');
        let fixed = match (style, wrong.kind.opens(), curly) {
            (QuoteStyle::Double, true, true) => "\u{201C}",
            (QuoteStyle::Double, false, true) => "\u{201D}",
            (QuoteStyle::Double, _, false) => "\"",
            (QuoteStyle::Single, true, true) => "\u{2018}",
            (QuoteStyle::Single, false, true) => "\u{2019}",
            (QuoteStyle::Single, _, false) => "'",
        };
        out.push(issue(
            wrong.at..wrong.at + wrong.len,
            "mismatched-quotes",
            "Mismatched Quotation Marks",
            "The opening and closing quotation marks around this speech are different kinds: one is a single mark and the other a double mark. Apply Change changes this one so the pair matches.",
            Some(fixed.to_string()),
            "paired-quotes",
        ));
    }
    if style == QuoteStyle::Double {
        for m in &pairing.loose_singles {
            involved.insert(m.at);
            out.push(issue(
                m.at..m.at + m.len,
                "unclosed-single",
                "Single Quotation Mark Never Closed",
                "This single opening quotation mark has no closing mark. If it is left over from changing the quotation style, remove it. If it was meant as an apostrophe, use \u{2019} instead.",
                Some(String::new()),
                "paired-quotes",
            ));
        }
    }

    // Quotation marks that do not pair up, and mixed straight and curly marks: double quotes only,
    // because in single-quote text an apostrophe cannot be told from a closing mark.
    if style == QuoteStyle::Double {
        let first_text = text.find(|c: char| !c.is_whitespace()).unwrap_or(0);
        let end_of_text = text.trim_end().len();
        for q in &quotes {
            if q.close.is_none() && q.open > first_text && !involved.contains(&q.open) {
                let closing = if text.get(q.open..q.open + q.open_len) == Some("\u{201C}") {
                    "\u{201D}"
                } else {
                    "\""
                };
                out.push(DialogueIssue {
                    range: q.open..q.open + q.open_len,
                    rule: "unclosed-quote",
                    title: "Unclosed Quotation",
                    explanation: "This quotation mark opens speech that is never closed in this paragraph. Apply Change adds a closing mark at the end of the paragraph; move it if the speech ends sooner. (Speech that runs on into the next paragraph is fine: that paragraph starts with an opening mark again.)".to_string(),
                    replacement: Some(closing.to_string()),
                    guide: "paired-quotes",
                    edit: Some((end_of_text..end_of_text, closing.to_string())),
                });
            }
        }
        for &(at, len) in strays.iter().filter(|(at, _)| !involved.contains(at)) {
            let opening = if text.get(at..at + len) == Some("\u{201D}") {
                "\u{201C}"
            } else {
                "\""
            };
            out.push(DialogueIssue {
                range: at..at + len,
                rule: "stray-closer",
                title: "Closing Quote Without an Opening One",
                explanation: "This closing quotation mark has nothing to close in this paragraph. Apply Change adds an opening mark at the start of the paragraph; move it if the speech starts later.".to_string(),
                replacement: Some(opening.to_string()),
                guide: "paired-quotes",
                edit: Some((first_text..first_text, opening.to_string())),
            });
        }
        let curly = text.contains(['\u{201C}', '\u{201D}']);
        if curly && text.contains('"') {
            for (at, c) in text.char_indices().filter(|&(_, c)| c == '"') {
                let prev = text.get(..at).and_then(|h| h.chars().next_back());
                let curly_mark = if opens_a_quote(prev) {
                    "\u{201C}"
                } else {
                    "\u{201D}"
                };
                out.push(issue(
                    at..at + c.len_utf8(),
                    "mixed-quotes",
                    "Mixed Quotation Marks",
                    "This paragraph mixes straight (\") and curly (\u{201C} \u{201D}) quotation marks. Apply Change turns this one into a curly mark, to match the others.",
                    Some(curly_mark.to_string()),
                    "paired-quotes",
                ));
            }
        }
    }
    out.sort_by_key(|i| (i.range.start, i.range.end));
    out
}

// ---------------------------------------------------------------------------------------------
// Before and after
// ---------------------------------------------------------------------------------------------

/// The text around an issue as it is, and as it would be once fixed, for the box's two previews.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// The paragraph, or the stretch of it around the issue when it is long.
    pub window: String,
    /// The flagged text within `window`.
    pub hl: Range<usize>,
    /// `window` with the fix applied, and the part that changed within it.
    pub fixed: Option<String>,
    pub fixed_hl: Option<Range<usize>>,
    /// The window starts or ends mid-paragraph (the box shows an ellipsis).
    pub lead: bool,
    pub trail: bool,
    /// When the fix is made far from the flagged text (a missing quotation mark at the other end of
    /// a long paragraph) it is not in the window: whether it goes at the start, and what is added.
    pub far_edit: Option<(bool, String)>,
}

/// Build the two previews for `issue`, found in `text`. Long paragraphs are cut to about 300
/// characters either side of the issue, at word boundaries.
pub fn preview(text: &str, issue: &DialogueIssue) -> Preview {
    const RADIUS: usize = 300;
    let edit = issue.edit();
    let (mut lo, mut hi) = (issue.range.start, issue.range.end);
    let mut far_edit = None;
    if let Some((r, new)) = &edit {
        let (union_lo, union_hi) = (lo.min(r.start), hi.max(r.end));
        if union_hi - union_lo <= 2 * RADIUS {
            lo = union_lo;
            hi = union_hi;
        } else {
            far_edit = Some((r.start < issue.range.start, new.clone()));
        }
    }
    let lo = lo.min(text.len());
    let hi = hi.min(text.len()).max(lo);
    let mut start = text
        .get(..lo)
        .and_then(|h| h.char_indices().rev().nth(RADIUS - 1))
        .map_or(0, |(b, _)| b);
    let mut end = text
        .get(hi..)
        .and_then(|t| t.char_indices().nth(RADIUS))
        .map_or(text.len(), |(b, _)| hi + b);
    if start > 0 {
        if let Some((i, c)) = text
            .get(start..lo)
            .and_then(|seg| seg.char_indices().find(|(_, c)| c.is_whitespace()))
        {
            start += i + c.len_utf8();
        }
    }
    if end < text.len() {
        if let Some((i, _)) = text
            .get(hi..end)
            .and_then(|seg| seg.char_indices().rev().find(|(_, c)| c.is_whitespace()))
        {
            end = hi + i;
        }
    }
    let window = text.get(start..end).unwrap_or("").to_string();
    let shift = |n: usize| n.saturating_sub(start);
    let hl = shift(issue.range.start)..shift(issue.range.end);
    let (fixed, fixed_hl) = match &edit {
        Some((r, new)) => {
            let (a, b) = (shift(r.start), shift(r.end));
            let mut f = window.clone();
            let inside = r.start >= start && r.end <= end;
            if inside && a <= b && b <= f.len() && f.is_char_boundary(a) && f.is_char_boundary(b) {
                f.replace_range(a..b, new);
                (Some(f), Some(a..a + new.len()))
            } else {
                (None, None)
            }
        }
        None => (None, None),
    };
    Preview {
        window,
        hl,
        fixed,
        fixed_hl,
        lead: start > 0,
        trail: end < text.len(),
        far_edit,
    }
}

// ---------------------------------------------------------------------------------------------
// The guide
// ---------------------------------------------------------------------------------------------

/// One example line and why it is right or wrong.
pub struct Example {
    pub text: &'static str,
    pub why: &'static str,
}

/// One rule of the guide: what it says, three correct lines and three incorrect ones.
pub struct GuideSection {
    pub id: &'static str,
    /// Which tab of the guide it sits under (see [`GUIDE_CATEGORIES`]).
    pub category: &'static str,
    pub title: &'static str,
    pub body: &'static [&'static str],
    pub correct: [Example; 3],
    pub incorrect: [Example; 3],
    /// True when the checker catches every incorrect example and none of the correct ones; the
    /// unit tests hold the guide to that.
    pub checked: bool,
}

/// A short line from a published book, with its credit and what it shows.
pub struct BookExample {
    pub quote: &'static str,
    pub credit: &'static str,
    pub shows: &'static str,
}

const fn ex(text: &'static str, why: &'static str) -> Example {
    Example { text, why }
}

/// A tab of the guide.
pub struct GuideCategory {
    pub id: &'static str,
    pub title: &'static str,
    /// A short introduction shown at the top of the tab.
    pub blurb: &'static [&'static str],
}

pub const GUIDE_INTRO: &[&str] = &[
    "A reference for the writing rules that are easiest to forget when you are tired or in a rush. Pick a category, or search for a word or a mark.",
    "Every rule has a short explanation, three correct examples and three incorrect ones, so you can see the difference and not only read about it. The checker boxes catch some of these automatically; the rest are habits to build.",
];

pub const GUIDE_CATEGORIES: &[GuideCategory] = &[
    GuideCategory {
        id: "dialogue",
        title: "Dialogue",
        blurb: &[
            "Dialogue has a few small rules that, once you see the pattern, you stop having to think about. They are written here for American-style double quotation marks; British-style single quotation marks follow the same rules with single marks, except that a comma or period may sit outside the closing mark.",
            "Two words come up again and again. A dialogue TAG says who is speaking and how: 'she said', 'Tom asked'. An action BEAT shows what someone is doing: 'She smiled.' A tag belongs to the sentence the speech started; a beat is a sentence of its own.",
        ],
    },
    GuideCategory {
        id: "structure",
        title: "Sentence Structure",
        blurb: &[
            "How a sentence is built: whether it is complete, whether its parts are joined properly, and whether a list hangs together.",
        ],
    },
    GuideCategory {
        id: "punctuation",
        title: "Punctuation",
        blurb: &[
            "Commas, semicolons, colons, apostrophes, brackets, dashes and hyphens: what each one is for, and the mistakes that come with it. Examples use the Oxford comma.",
        ],
    },
    GuideCategory {
        id: "words",
        title: "Word Choice",
        blurb: &[
            "Words that sound alike or look alike and are easy to swap by accident, with a way to tell them apart and where each one belongs.",
        ],
    },
];

/// Other guides worth reading: (title, address, what it covers).
pub const FURTHER_READING: &[(&str, &str, &str)] = &[(
    "University of Hull Library: Grammar resource",
    "https://libguides.hull.ac.uk/grammar",
    "A student-focused guide to sentence structure, punctuation, word choice and formatting dialogue.",
)];

pub const GUIDE: &[GuideSection] = &[
    GuideSection {
        id: "tag-punctuation",
        category: "dialogue",
        title: "1. A comma, not a period, before a tag",
        body: &[
            "Speech and its tag are one sentence, so the speech ends with a comma (or a question or exclamation mark), never a period. In American style the comma goes inside the closing quotation mark.",
            "A quick test: read the line aloud as one sentence. 'I'm tired she said' has no full stop in the middle, so the written version needs no period either.",
        ],
        correct: [
            ex(
                "\"I'm tired,\" she said.",
                "Comma inside the quotes, then a lowercase tag.",
            ),
            ex(
                "\"We should leave now,\" he said.",
                "The tag continues the sentence.",
            ),
            ex(
                "\"Come here,\" she whispered.",
                "Any speech verb works the same way.",
            ),
        ],
        incorrect: [
            ex(
                "\"I'm tired.\" she said.",
                "A period ends the sentence; the tag is left hanging.",
            ),
            ex(
                "\"We should leave now.\" He said.",
                "Period and a capital: the tag is cut off from its speech.",
            ),
            ex(
                "\"Come here\", she whispered.",
                "The comma sits outside the quotation mark.",
            ),
        ],
        checked: true,
    },
    GuideSection {
        id: "lowercase-tags",
        category: "dialogue",
        title: "2. Tags start with a lowercase letter",
        body: &[
            "Because a tag continues the sentence, it starts with a lowercase letter, even after a question mark or an exclamation mark. Names and 'I' stay capitalized, as always.",
        ],
        correct: [
            ex(
                "\"Where are you going?\" she asked.",
                "Lowercase 'she' after a question mark.",
            ),
            ex(
                "\"Stop right there!\" he shouted.",
                "Lowercase 'he' after an exclamation mark.",
            ),
            ex(
                "\"Where are you going?\" Tom asked.",
                "A name keeps its capital.",
            ),
        ],
        incorrect: [
            ex(
                "\"Where are you going?\" She asked.",
                "The tag is part of the sentence, so 'she' is lowercase.",
            ),
            ex(
                "\"Stop right there!\" He shouted.",
                "Same mistake after an exclamation mark.",
            ),
            ex(
                "\"I know,\" She said.",
                "A capital after a comma can never be right.",
            ),
        ],
        checked: true,
    },
    GuideSection {
        id: "beats-vs-tags",
        category: "dialogue",
        title: "3. Actions are beats, not tags",
        body: &[
            "You can say, ask, whisper or shout words. You cannot smile, nod, shrug or laugh them. When a character does something, that is a beat: it gets its own sentence, so the speech ends with a period and the beat starts with a capital.",
            "If you want to join the two, use a real tag and add the action after it: 'he said, and nodded'. Putting the beat before the speech also works and needs no tag at all.",
        ],
        correct: [
            ex(
                "\"Thanks.\" She smiled.",
                "Period, then the beat as its own sentence.",
            ),
            ex(
                "\"Come in,\" he said, and nodded.",
                "A real tag first, then the action.",
            ),
            ex(
                "She smiled. \"Thanks.\"",
                "The beat comes first, so no tag is needed.",
            ),
        ],
        incorrect: [
            ex(
                "\"Thanks,\" she smiled.",
                "Smiling is not a way of speaking.",
            ),
            ex("\"Come in,\" he nodded.", "A nod cannot say anything."),
            ex(
                "\"Fine,\" Tom shrugged.",
                "Use a period and a new sentence: \"Fine.\" Tom shrugged.",
            ),
        ],
        checked: true,
    },
    GuideSection {
        id: "punctuation-inside",
        category: "dialogue",
        title: "4. Commas and periods go inside the quotation marks",
        body: &[
            "In American style, commas and periods always sit inside the closing quotation mark, even when the quoted part is only a word or two. Question marks and exclamation marks go inside only when they belong to the quoted words.",
            "British style is more logical: the mark goes inside only when it belongs to the quotation.",
        ],
        correct: [
            ex(
                "She called it \"a mistake.\"",
                "The period sits inside the closing mark.",
            ),
            ex(
                "\"Fine,\" he said, \"but only once.\"",
                "Commas inside, both times.",
            ),
            ex(
                "Did she really say \"never\"?",
                "The question mark belongs to the whole sentence, so it sits outside.",
            ),
        ],
        incorrect: [
            ex(
                "She called it \"a mistake\".",
                "American style puts the period inside.",
            ),
            ex(
                "\"Fine\", he said, \"but only once\".",
                "Both marks sit outside.",
            ),
            ex(
                "Did she really say \"never?\"",
                "The question is the whole sentence's, not the quotation's.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "split-dialogue",
        category: "dialogue",
        title: "5. When a tag splits a sentence",
        body: &[
            "A tag can interrupt a line of speech. If the speech is one sentence, the first part ends with a comma, the tag ends with a comma, and the second part starts in lowercase, because the sentence has not ended.",
            "If the first part is a complete sentence (it ends with a period, question mark or exclamation mark), the tag ends with a period and the second part starts with a capital.",
        ],
        correct: [
            ex(
                "\"Well,\" she said, \"that's odd.\"",
                "One sentence: comma, tag, comma, lowercase.",
            ),
            ex(
                "\"I think,\" he said, \"we should go.\"",
                "The sentence is not finished at the tag.",
            ),
            ex(
                "\"Stop!\" she said. \"That's enough.\"",
                "Two sentences: a period after the tag and a capital.",
            ),
        ],
        incorrect: [
            ex(
                "\"Well,\" she said, \"That's odd.\"",
                "The sentence continues, so 'that's' is lowercase.",
            ),
            ex("\"I think,\" he said, \"We should go.\"", "Same mistake."),
            ex(
                "\"If you want,\" Tom said, \"You can stay.\"",
                "Same mistake, with a name in the tag.",
            ),
        ],
        checked: true,
    },
    GuideSection {
        id: "paired-quotes",
        category: "dialogue",
        title: "6. Quotation marks come in pairs",
        body: &[
            "Every opening quotation mark needs a closing one. Use one kind of mark all the way through: curly marks (\u{201C} \u{201D}) or straight ones (\"), not both.",
            "One exception: when one character speaks for several paragraphs, leave the closing mark off every paragraph but the last. Each new paragraph of the speech starts with an opening mark again.",
            "The two marks of a pair must also be the same kind. Opening with a single mark and closing with a double one (or the other way round) is a mismatch, and it usually happens when you change one mark and forget the other.",
        ],
        correct: [
            ex("She said, \"Come here.\"", "Opened and closed."),
            ex(
                "\"Come here,\" she said, \"and bring the lamp.\"",
                "Two pairs in one sentence.",
            ),
            ex(
                "\u{201C}Come here,\u{201D} she said.",
                "Curly marks used consistently.",
            ),
        ],
        incorrect: [
            ex("She said, \"Come here.", "The quotation is never closed."),
            ex(
                "Come here.\" she said.",
                "A closing mark with no opening one.",
            ),
            ex(
                "\"Hello,\u{201D} she said.",
                "A straight mark mixed with a curly one.",
            ),
        ],
        checked: true,
    },
    GuideSection {
        id: "new-speaker",
        category: "dialogue",
        title: "7. A new speaker gets a new paragraph",
        body: &[
            "Start a new paragraph every time a different character speaks, even for one word. It tells the reader who is talking without a tag every time. A beat for the same speaker can stay in the same paragraph as the speech.",
            "The checker does not flag this one, because it cannot tell who is speaking. Use it as a habit.",
        ],
        correct: [
            ex(
                "\"Are you coming?\" Mara asked.\n\"Not tonight,\" Jon said.",
                "Two speakers, two paragraphs.",
            ),
            ex("\"Ready?\" she asked.\n\"Always,\" he said.", "The same."),
            ex(
                "Mara turned. \"Are you coming?\"\n\"Not tonight.\"",
                "Her beat and her speech share a paragraph; Jon's reply gets his own.",
            ),
        ],
        incorrect: [
            ex(
                "\"Are you coming?\" Mara asked. \"Not tonight,\" Jon said.",
                "Two speakers in one paragraph.",
            ),
            ex(
                "\"Ready?\" she asked. \"Always,\" he said.",
                "The reader has to work out who answered.",
            ),
            ex(
                "\"Where were you?\" Tom said. \"Out,\" Ben said.",
                "The same mistake with names.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "quote-style",
        category: "dialogue",
        title: "8. Double quotes, single quotes and quotes inside quotes",
        body: &[
            "American style uses double quotation marks for speech and single ones for a quotation inside speech. British style does the opposite. Pick one and keep to it; the Dialogue Formatting box detects which one your document uses (you can also set it by hand).",
            "Never nest the same kind of mark inside itself: it cannot be read.",
        ],
        correct: [
            ex(
                "\"She told me, 'Wait here,' and left,\" he said.",
                "American: single marks inside double.",
            ),
            ex(
                "'She told me, \"Wait here,\" and left,' he said.",
                "British: double marks inside single.",
            ),
            ex("\"Hello,\" she said.", "The simple case."),
        ],
        incorrect: [
            ex(
                "\"She told me, \"Wait here,\" and left,\" he said.",
                "Double inside double: the reader cannot tell where anything ends.",
            ),
            ex(
                "'Hello,' she said. \"Goodbye,\" he said.",
                "Two styles in one passage.",
            ),
            ex(
                "\u{201C}Hello,\" she said.",
                "A curly opening mark with a straight closing one.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "questions-exclamations",
        category: "dialogue",
        title: "9. Questions and exclamations before a tag",
        body: &[
            "A question mark or exclamation mark already ends the speech, so no comma or period is added after it. The tag still starts in lowercase.",
        ],
        correct: [
            ex(
                "\"Where are you?\" she asked.",
                "The question mark does the job of the comma.",
            ),
            ex(
                "\"Stop!\" he shouted.",
                "The same with an exclamation mark.",
            ),
            ex("\"Why?\" she asked.", "Even a single word."),
        ],
        incorrect: [
            ex(
                "\"Where are you?,\" she asked.",
                "A comma after a question mark.",
            ),
            ex(
                "\"Stop!,\" he shouted.",
                "A comma after an exclamation mark.",
            ),
            ex("\"Why?.\" she asked.", "A period after a question mark."),
        ],
        checked: true,
    },
    GuideSection {
        id: "interruptions",
        category: "dialogue",
        title: "10. Interruptions and trailing off",
        body: &[
            "Use an em dash (\u{2014}) with no spaces when speech is cut off, and an ellipsis (...) when it trails away. After an interruption the dash is the end punctuation, so no comma follows it.",
            "Do not use a hyphen for an interruption: a hyphen joins words, a dash breaks them off.",
        ],
        correct: [
            ex(
                "\"If you'd just\u{2014}\" he began, but she cut him off.",
                "The dash ends the speech.",
            ),
            ex(
                "\"I thought we could...\" She trailed off.",
                "An ellipsis, then a beat as a new sentence.",
            ),
            ex(
                "\"But I never\u{2014}\" Tom stopped.",
                "A dash, then a beat.",
            ),
        ],
        incorrect: [
            ex("\"I was just\u{2014},\" he began.", "A comma after a dash."),
            ex(
                "\"You can't just-\" she began.",
                "A hyphen where a dash belongs.",
            ),
            ex(
                "\"I was just \u{2014}\" he began.",
                "A space before the dash.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "direct-indirect",
        category: "dialogue",
        title: "11. Direct and indirect speech",
        body: &[
            "Direct speech gives the speaker's exact words inside quotation marks. Indirect (reported) speech tells the reader what was said without quoting it, so it takes no quotation marks and no comma before 'that'.",
            "Reported speech usually moves the tense back and changes the pronouns: \"I am tired\" becomes 'she said she was tired'. A reported question is a statement, so it keeps normal word order and ends with a full stop.",
        ],
        correct: [
            ex(
                "She said she was tired.",
                "Indirect speech: no quotation marks, and 'am' becomes 'was'.",
            ),
            ex(
                "He asked whether we were ready.",
                "A reported question: normal word order and a full stop.",
            ),
            ex(
                "\"I am tired,\" she said.",
                "Direct speech: her exact words in quotation marks.",
            ),
        ],
        incorrect: [
            ex(
                "She said \"she was tired.\"",
                "Quotation marks promise exact words, but these are reported ones.",
            ),
            ex(
                "He asked whether were we ready?",
                "A reported question keeps statement word order and takes no question mark.",
            ),
            ex(
                "She said, that she was tired.",
                "No comma between 'said' and 'that' in reported speech.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "fragments",
        category: "structure",
        title: "1. Sentence fragments",
        body: &[
            "A sentence needs a subject and a verb, and it has to make sense on its own. A fragment is a piece of a sentence punctuated as if it were whole: it starts a thought and never finishes it.",
            "Fiction uses fragments on purpose for rhythm, and that is fine when you choose it. The trouble is the accidental ones, usually a clause starting with 'because', 'although' or 'which' left standing alone.",
        ],
        correct: [
            ex(
                "Because the lights failed, the match was postponed.",
                "The 'because' clause is attached to a main clause.",
            ),
            ex(
                "The old house stood at the end of the lane.",
                "Subject, verb, and a complete thought.",
            ),
            ex(
                "We waited. Nobody came.",
                "Short, but each one has a subject and a verb.",
            ),
        ],
        incorrect: [
            ex(
                "Because the lights failed.",
                "'Because' opens a clause that needs a main clause to finish it.",
            ),
            ex(
                "The old house at the end of the lane.",
                "A noun phrase with no verb.",
            ),
            ex(
                "Running down the hill as fast as she could.",
                "An -ing phrase with no subject and no main verb.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "run-ons",
        category: "structure",
        title: "2. Run-ons and comma splices",
        body: &[
            "Two complete sentences cannot be joined by nothing (a run-on) or by a comma alone (a comma splice). A comma is too weak to hold two sentences together.",
            "There are four easy fixes: a full stop, a semicolon, a comma plus a joining word (and, but, so, or), or turning one half into a clause that depends on the other.",
        ],
        correct: [
            ex("It was late, so we went home.", "A comma plus 'so'."),
            ex(
                "It was late; we went home.",
                "A semicolon joins two closely related sentences.",
            ),
            ex("It was late. We went home.", "Two sentences."),
        ],
        incorrect: [
            ex(
                "It was late we went home.",
                "A run-on: two sentences with nothing between them.",
            ),
            ex("It was late, we went home.", "A comma splice."),
            ex(
                "I like tea, she prefers coffee.",
                "A comma splice: use a semicolon, a full stop, or add 'but'.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "parallel",
        category: "structure",
        title: "3. Keep lists parallel",
        body: &[
            "Items in a list, or the two halves of a pair, should have the same grammatical shape: all -ing words, all infinitives, all adjectives. When one item changes shape, the sentence limps.",
            "Read the list aloud with the opening words repeated in front of each item. If one version sounds wrong, that item is out of step.",
        ],
        correct: [
            ex(
                "She likes hiking, swimming, and cycling.",
                "Three -ing words.",
            ),
            ex(
                "He wanted to read, to write, and to sleep.",
                "Three infinitives.",
            ),
            ex("The plan is quick, cheap, and safe.", "Three adjectives."),
        ],
        incorrect: [
            ex(
                "She likes hiking, to swim, and cycling.",
                "Two -ing words and an infinitive.",
            ),
            ex(
                "He wanted to read, writing, and to sleep.",
                "The middle item changes shape.",
            ),
            ex(
                "The plan is quick, cheap, and being safe.",
                "Two adjectives and a different kind of phrase.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "commas",
        category: "punctuation",
        title: "1. Commas and the Oxford comma",
        body: &[
            "Use a comma to separate items in a list, including before the final 'and' or 'or'. That last comma is the Oxford (serial) comma, and it is used throughout this guide. It stops a list from blurring together: 'I thanked my parents, Ayn Rand and God' sounds very different from 'my parents, Ayn Rand, and God'.",
            "Also use commas after an opening word or phrase ('However,' 'After dinner,'), before a joining word that links two complete sentences ('..., but ...'), and in pairs around extra information that could be taken out ('My sister, who lives in Leeds, is a chef').",
            "Do not put a single comma between a subject and its verb, and do not use only one comma where extra information needs a pair.",
        ],
        correct: [
            ex(
                "We packed tents, stoves, and a kettle.",
                "The Oxford comma before 'and'.",
            ),
            ex(
                "My sister, who lives in Leeds, is a chef.",
                "A pair of commas around extra information.",
            ),
            ex(
                "However, the village was silent.",
                "A comma after an opening word.",
            ),
        ],
        incorrect: [
            ex(
                "We packed tents, stoves and a kettle.",
                "No comma before 'and'. Fine in some styles, but not if you use the Oxford comma, so keep it consistent.",
            ),
            ex(
                "My sister, who lives in Leeds is a chef.",
                "The extra information needs a second comma to close it.",
            ),
            ex(
                "The dog, barked at the postman.",
                "A comma should not split a subject from its verb.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "semicolons",
        category: "punctuation",
        title: "2. Semicolons",
        body: &[
            "A semicolon is a stronger pause than a comma and a weaker stop than a full stop. It has three jobs.",
            "First, it joins two complete sentences that are closely related. Second, it comes before a word like 'however' or 'therefore' that links two complete sentences. Third, it separates list items that already contain commas.",
            "Do not use a semicolon before a plain 'and' or 'but' (use a comma), after a greeting, or after a clause that cannot stand alone.",
        ],
        correct: [
            ex(
                "It was late; we went home.",
                "Two related complete sentences.",
            ),
            ex(
                "It rained all day; however, the match went ahead.",
                "A semicolon before 'however', with a comma after it.",
            ),
            ex(
                "We visited Paris, France; Rome, Italy; and Madrid, Spain.",
                "Semicolons keep the items apart when each contains a comma.",
            ),
        ],
        incorrect: [
            ex(
                "It was late; and we went home.",
                "A semicolon before a plain 'and': use a comma, or drop 'and'.",
            ),
            ex(
                "Dear John; I hope you are well.",
                "A greeting takes a comma (or a colon in formal letters).",
            ),
            ex(
                "Because it was late; we went home.",
                "The first part cannot stand alone, so it needs a comma.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "colons",
        category: "punctuation",
        title: "3. Colons",
        body: &[
            "A colon says 'here comes the explanation'. Use it after a complete sentence to introduce a list, an example, a reason, or a quotation.",
            "Avoid putting a colon straight after a verb or a preposition, because the sentence is not complete yet: write 'You will need three things: a pencil, a ruler, and patience', not 'You will need: ...'.",
        ],
        correct: [
            ex(
                "You will need three things: a pencil, a ruler, and patience.",
                "A complete sentence, then the list.",
            ),
            ex(
                "He had one fear: failure.",
                "The colon introduces the explanation.",
            ),
            ex(
                "Remember this: nobody is watching.",
                "A complete sentence, then what to remember.",
            ),
        ],
        incorrect: [
            ex(
                "You will need: a pencil, a ruler, and patience.",
                "The colon comes straight after a verb.",
            ),
            ex(
                "We went to: Paris, Rome, and Madrid.",
                "The colon comes straight after a preposition.",
            ),
            ex(
                "He had one fear; failure.",
                "A semicolon needs a complete sentence after it. A colon is what introduces an explanation.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "apostrophes",
        category: "punctuation",
        title: "4. Apostrophes",
        body: &[
            "An apostrophe does two jobs. It shows letters have been left out (don't, it's, I'll), and it shows that something belongs to someone (the dog's bowl).",
            "For belonging: add apostrophe-s to a singular noun (the dog's bowl), add just an apostrophe to a plural that ends in s (the dogs' bowls), and add apostrophe-s to a plural that does not end in s (the children's toys). For a singular name ending in s, 'James's book' and 'James' book' are both used: pick one and keep to it.",
            "An apostrophe never makes a plural (apples, not apple's; the 1990s, not the 1990's), and it is never part of a possessive pronoun: its, hers, ours, yours, theirs, whose.",
        ],
        correct: [
            ex(
                "The dogs' bowls were empty.",
                "A plural that ends in s: the apostrophe goes after the s.",
            ),
            ex(
                "It's the children's turn.",
                "'It's' is 'it is'; 'children's' belongs to the children.",
            ),
            ex(
                "We sold apples and pears in the 1990s.",
                "No apostrophes in plurals.",
            ),
        ],
        incorrect: [
            ex(
                "We sold apple's and pear's.",
                "Plurals do not take apostrophes.",
            ),
            ex(
                "The childrens' toys were everywhere.",
                "'Children' is already plural, so the form is children's.",
            ),
            ex(
                "The cat licked it's paw.",
                "'Its' meaning belonging to it has no apostrophe.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "brackets",
        category: "punctuation",
        title: "5. Brackets",
        body: &[
            "Round brackets (parentheses) hold an aside or extra detail. The sentence must still make sense if you take the brackets and what is inside them out. Do not add commas around them.",
            "If the bracketed words are part of the sentence, the full stop goes after the closing bracket. Only a bracketed sentence that stands completely alone keeps its own full stop inside.",
            "Square brackets are for a writer's own words inside someone else's quotation: a clarification, or a changed word. They are rare in fiction.",
        ],
        correct: [
            ex(
                "The cottage (built in 1820) was freezing.",
                "An aside, with no extra commas.",
            ),
            ex(
                "She said she would come (although I doubted it).",
                "The full stop goes after the bracket.",
            ),
            ex(
                "\"He [the captain] never looked back.\"",
                "Square brackets add a clarification inside a quotation.",
            ),
        ],
        incorrect: [
            ex(
                "The cottage, (built in 1820), was freezing.",
                "Brackets and commas together: use one or the other.",
            ),
            ex(
                "She said she would come (although I doubted it.)",
                "The bracketed words are part of the sentence, so the full stop goes outside.",
            ),
            ex(
                "The cottage (built in 1820 was freezing.",
                "The bracket is never closed.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "dashes",
        category: "punctuation",
        title: "6. Dashes",
        body: &[
            "The em dash (—) marks a break, an interruption, or an emphatic aside. In American style it has no spaces around it. A pair of em dashes works like brackets but louder.",
            "The en dash (–) is shorter. It shows a range or a link between two things: Monday–Friday, 1990–1999, the London–Paris train. British style often uses a spaced en dash ( – ) where American style uses an unspaced em dash. Pick one and be consistent.",
            "A hyphen (-) is not a dash, and two hyphens in a row are only a stand-in for an em dash. To type them on Windows hold Alt and type 0151 (em) or 0150 (en) on the number pad; on a Mac use Option-Shift-hyphen (em) or Option-hyphen (en).",
        ],
        correct: [
            ex(
                "The answer—if there was one—was hidden.",
                "A pair of em dashes around an aside.",
            ),
            ex(
                "The shop is open Monday–Friday.",
                "An en dash shows a range.",
            ),
            ex(
                "She stopped—then she smiled.",
                "A single em dash marks the break.",
            ),
        ],
        incorrect: [
            ex(
                "The answer - if there was one - was hidden.",
                "Hyphens with spaces are standing in for dashes.",
            ),
            ex(
                "The shop is open Monday-Friday.",
                "A range traditionally takes an en dash, not a hyphen.",
            ),
            ex(
                "She stopped--then she smiled.",
                "Two hyphens are a typewriter habit: use a true em dash.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "hyphens",
        category: "punctuation",
        title: "7. Hyphens",
        body: &[
            "A hyphen joins words so they work as one. Use it in a compound that comes before a noun ('a well-known author') but not usually after it ('the author is well known'), and in numbers from 21 to 99 ('twenty-five').",
            "Do not put a hyphen after an adverb ending in -ly ('a poorly written note'). A hanging hyphen saves repeating a word: 'two- and three-year-olds'.",
        ],
        correct: [
            ex(
                "She is a well-known author.",
                "The compound comes before the noun, so it is hyphenated.",
            ),
            ex("The author is well known.", "After the noun, no hyphen."),
            ex(
                "We sold twenty-five tickets.",
                "Numbers from 21 to 99 take a hyphen.",
            ),
        ],
        incorrect: [
            ex(
                "She is a well known author.",
                "The compound before the noun needs its hyphen.",
            ),
            ex(
                "It was a poorly-written note.",
                "No hyphen after an -ly adverb.",
            ),
            ex(
                "We sold twenty five tickets.",
                "Twenty-five needs its hyphen.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "its-its",
        category: "words",
        title: "1. its / it's",
        body: &[
            "'Its' belongs to it, like 'his' and 'her'. 'It's' is a short way of writing 'it is' or 'it has'.",
            "The test: say 'it is' or 'it has' in its place. If the sentence still works, write it's. If it does not, write its.",
        ],
        correct: [
            ex(
                "The dog wagged its tail.",
                "Belonging to the dog: no apostrophe.",
            ),
            ex("It's raining again.", "'It is raining again.'"),
            ex(
                "It's clear the company has lost its way.",
                "The first is 'it is'; the second belongs to the company.",
            ),
        ],
        incorrect: [
            ex(
                "The dog wagged it's tail.",
                "'It is tail' makes no sense, so it is 'its'.",
            ),
            ex(
                "Its raining again.",
                "'It is raining' works, so it is 'it's'.",
            ),
            ex("The company lost it's way.", "Belonging, not 'it is'."),
        ],
        checked: false,
    },
    GuideSection {
        id: "their-there",
        category: "words",
        title: "2. their / there / they're",
        body: &[
            "'Their' belongs to them. 'There' is a place, or the 'there is' of 'there is no milk'. 'They're' is 'they are'.",
            "'There' hides the word 'here', which is also a place. 'Their' hides the word 'heir', someone who owns things.",
        ],
        correct: [
            ex("Their car broke down.", "The car belongs to them."),
            ex("There is a café over there.", "'There is' and a place."),
            ex(
                "They're late because their train was delayed.",
                "'They are' late; the train belongs to them.",
            ),
        ],
        incorrect: [
            ex("There car broke down.", "Belonging: their."),
            ex("Their is no milk left.", "'There is'."),
            ex("Their going to be late.", "'They are going': they're."),
        ],
        checked: false,
    },
    GuideSection {
        id: "your-youre",
        category: "words",
        title: "3. your / you're",
        body: &[
            "'Your' belongs to you. 'You're' is 'you are'. If you can say 'you are' in its place, write you're.",
        ],
        correct: [
            ex("Your coat is on the chair.", "The coat belongs to you."),
            ex("You're welcome to stay.", "'You are welcome.'"),
            ex(
                "You're sure your keys are in your bag?",
                "'You are sure'; the keys and bag are yours.",
            ),
        ],
        incorrect: [
            ex("Your welcome to stay.", "'You are welcome': you're."),
            ex("You're coat is on the chair.", "Belonging: your."),
            ex("I think your right.", "'You are right': you're."),
        ],
        checked: false,
    },
    GuideSection {
        id: "to-too-two",
        category: "words",
        title: "4. to / too / two",
        body: &[
            "'Two' is the number. 'Too' means 'also' or 'more than enough'. 'To' is everything else: a direction (to the market) or the start of a verb (to swim).",
            "'Too' has an extra o, which is the extra bit: also, excessively.",
        ],
        correct: [
            ex("We went to the market.", "A direction."),
            ex(
                "It was too cold to swim.",
                "'Too' means excessively; 'to' starts the verb.",
            ),
            ex("Two friends came too.", "The number, then 'also'."),
        ],
        incorrect: [
            ex("We went too the market.", "A direction takes 'to'."),
            ex("It was to cold to swim.", "Excessively: too."),
            ex("I bought to tickets.", "The number: two."),
        ],
        checked: false,
    },
    GuideSection {
        id: "then-than",
        category: "words",
        title: "5. then / than",
        body: &[
            "'Then' is about time or what comes next. 'Than' is for comparing.",
            "'Then' and 'when' both rhyme and both concern time. 'Than' and 'comparison' share the letter a.",
        ],
        correct: [
            ex("First we ate, then we slept.", "Order in time."),
            ex("She is taller than her brother.", "A comparison."),
            ex(
                "If it rains, then we stay home.",
                "What follows from the 'if'.",
            ),
        ],
        incorrect: [
            ex("She is taller then her brother.", "A comparison: than."),
            ex("First we ate, than we slept.", "Order in time: then."),
            ex("I would rather walk then drive.", "A comparison: than."),
        ],
        checked: false,
    },
    GuideSection {
        id: "affect-effect",
        category: "words",
        title: "6. affect / effect",
        body: &[
            "Most of the time 'affect' is the verb (to influence) and 'effect' is the noun (the result): 'The noise affected her sleep; the effect was dramatic.'",
            "There are two rarer uses. 'Effect' can be a verb meaning 'to bring about' ('to effect change'), and 'affect' can be a noun in psychology. For everyday writing, the verb/noun split is the one to remember.",
        ],
        correct: [
            ex(
                "The noise affected her sleep.",
                "'Affected' is the verb: it influenced her sleep.",
            ),
            ex(
                "The new rule had a strong effect.",
                "'Effect' is the noun: the result.",
            ),
            ex(
                "The effect of the delay was obvious.",
                "The result of the delay.",
            ),
        ],
        incorrect: [
            ex(
                "The noise effected her sleep.",
                "The verb for influence is 'affected'.",
            ),
            ex(
                "The new rule had a strong affect.",
                "The noun for a result is 'effect'.",
            ),
            ex(
                "The delay will effect everyone.",
                "To influence everyone: affect.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "lose-loose",
        category: "words",
        title: "7. lose / loose",
        body: &[
            "'Lose' (one o) is a verb: to misplace something or fail to win. 'Loose' (two o's) means not tight. The extra o makes it looser.",
        ],
        correct: [
            ex("Don't lose your keys.", "To misplace."),
            ex("The screw is loose.", "Not tight."),
            ex("We may lose the match.", "To fail to win."),
        ],
        incorrect: [
            ex("Don't loose your keys.", "To misplace: lose."),
            ex("The screw is lose.", "Not tight: loose."),
            ex("We might loose the game.", "To fail to win: lose."),
        ],
        checked: false,
    },
    GuideSection {
        id: "whose-whos",
        category: "words",
        title: "8. whose / who's",
        body: &[
            "'Whose' asks about or shows belonging. 'Who's' is 'who is' or 'who has'. Say 'who is' in its place: if it works, write who's.",
        ],
        correct: [
            ex("Whose coat is this?", "Belonging."),
            ex("Who's coming tonight?", "'Who is coming'."),
            ex(
                "The man whose car was stolen called the police.",
                "The car belongs to the man.",
            ),
        ],
        incorrect: [
            ex("Who's coat is this?", "Belonging: whose."),
            ex("Whose coming tonight?", "'Who is coming': who's."),
            ex(
                "The woman who's dog barked apologized.",
                "The dog belongs to the woman: whose.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "accept-except",
        category: "words",
        title: "9. accept / except",
        body: &[
            "'Accept' means to receive or agree to. 'Except' means 'leaving out'. 'Except' begins with ex, which is also in 'exclude'.",
        ],
        correct: [
            ex("She will accept the offer.", "To agree to."),
            ex("Everyone except Tom came.", "Leaving Tom out."),
            ex("I accept your apology.", "To receive it."),
        ],
        incorrect: [
            ex("She will except the offer.", "To agree to: accept."),
            ex("Everyone accept Tom came.", "Leaving out: except."),
            ex("I except your apology.", "To receive: accept."),
        ],
        checked: false,
    },
    GuideSection {
        id: "advice-advise",
        category: "words",
        title: "10. advice / advise",
        body: &[
            "'Advice' is a noun, the thing you give. 'Advise' is a verb, what you do. The noun has a c as in 'ice'; the verb has an s.",
            "The same noun -ce, verb -se pattern applies to practice/practise and licence/license in British spelling. In American spelling, practice and license are used for both.",
        ],
        correct: [
            ex("She gave me good advice.", "The noun."),
            ex("I advise you to wait.", "The verb."),
            ex(
                "She asked her lawyer to advise her.",
                "The verb after 'to'.",
            ),
        ],
        incorrect: [
            ex("She gave me good advise.", "The noun is advice."),
            ex("I advice you to wait.", "The verb is advise."),
            ex("He asked for some advise.", "The noun is advice."),
        ],
        checked: false,
    },
    GuideSection {
        id: "principal-principle",
        category: "words",
        title: "11. principal / principle",
        body: &[
            "'Principal' means main or chief, and is also the head of a school. 'Principle' means a rule or belief. 'Principle' and 'rule' both end in 'le'.",
        ],
        correct: [
            ex(
                "The principal spoke to the class.",
                "The head of the school.",
            ),
            ex("It is a matter of principle.", "A belief."),
            ex("Her principal concern was safety.", "Her main concern."),
        ],
        incorrect: [
            ex("The principle spoke to the class.", "A person: principal."),
            ex("It is a matter of principal.", "A belief: principle."),
            ex("Her principle concern was safety.", "Main: principal."),
        ],
        checked: false,
    },
    GuideSection {
        id: "complement-compliment",
        category: "words",
        title: "12. complement / compliment",
        body: &[
            "'Complement' with an e completes something or goes well with it. 'Compliment' with an i is praise: 'I like it' has an i.",
        ],
        correct: [
            ex("Red wine complements the dish.", "It goes well with it."),
            ex("She gave him a compliment.", "Praise."),
            ex(
                "The two skills complement each other.",
                "Each completes the other.",
            ),
        ],
        incorrect: [
            ex(
                "Red wine compliments the dish.",
                "A drink cannot praise a dish: complements.",
            ),
            ex("She gave him a complement.", "Praise: compliment."),
            ex(
                "The two skills compliment each other.",
                "Each completes the other: complement.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "fewer-less",
        category: "words",
        title: "13. fewer / less",
        body: &[
            "Use 'fewer' for things you can count (fewer mistakes, fewer cars) and 'less' for amounts you cannot (less time, less noise).",
            "Distances, sums of money and periods of time count as single amounts, so 'less than ten miles' and 'less than twenty dollars' are correct.",
        ],
        correct: [
            ex(
                "There were fewer mistakes this time.",
                "Mistakes can be counted.",
            ),
            ex("I have less time than I thought.", "Time is an amount."),
            ex("There are fewer cars and less noise.", "One of each."),
        ],
        incorrect: [
            ex("There were less mistakes this time.", "Countable: fewer."),
            ex("I have fewer time than I thought.", "An amount: less."),
            ex("Less people came this year.", "Countable: fewer."),
        ],
        checked: false,
    },
    GuideSection {
        id: "who-whom",
        category: "words",
        title: "14. who / whom",
        body: &[
            "'Who' is the subject, the one doing the verb. 'Whom' is the object, the one the verb or preposition is aimed at. The test: if you could answer with 'he', use who. If you could answer with 'him', use whom.",
            "In casual speech 'who' is often used for both, but in writing keep whom for objects and after a preposition.",
        ],
        correct: [
            ex("Who called you?", "'He called me': who."),
            ex(
                "To whom did you speak?",
                "'I spoke to him': whom, after 'to'.",
            ),
            ex("The woman whom I met was kind.", "'I met her': whom."),
        ],
        incorrect: [
            ex("Whom called you?", "'He called': who."),
            ex("To who did you speak?", "After 'to': whom."),
            ex("Whom is at the door?", "'He is at the door': who."),
        ],
        checked: false,
    },
    GuideSection {
        id: "lay-lie",
        category: "words",
        title: "15. lay / lie",
        body: &[
            "'Lay' needs an object: you lay something down. 'Lie' is what a person or thing does, with no object: you lie down.",
            "The past tense of 'lie' is 'lay' (she lay awake). The past tense of 'lay' is 'laid' (she laid the book down). That overlap is what trips people up.",
        ],
        correct: [
            ex(
                "Please lay the book on the table.",
                "'Lay' takes an object: the book.",
            ),
            ex("I need to lie down.", "No object."),
            ex("She lay awake all night.", "The past tense of 'lie'."),
        ],
        incorrect: [
            ex(
                "Please lie the book on the table.",
                "The book is an object: lay.",
            ),
            ex("I need to lay down.", "No object: lie."),
            ex(
                "She laid awake all night.",
                "The past tense of 'lie' is lay.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "stationary-stationery",
        category: "words",
        title: "16. stationary / stationery",
        body: &[
            "'Stationary' with an a means not moving ('car' has an a). 'Stationery' with an e is paper, pens and envelopes ('envelope' has an e).",
        ],
        correct: [
            ex("The car was stationary.", "Not moving."),
            ex("She bought new stationery.", "Paper and pens."),
            ex("The traffic stayed stationary for an hour.", "Not moving."),
        ],
        incorrect: [
            ex("The car was stationery.", "Not moving: stationary."),
            ex("She bought new stationary.", "Paper and pens: stationery."),
            ex(
                "The stationary cupboard is full.",
                "Paper and pens: stationery.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "weather-whether",
        category: "words",
        title: "17. weather / whether",
        body: &[
            "'Weather' is rain, sun and wind. 'Whether' means 'if' or introduces a choice. 'Whether' has the h of 'which', and a choice is what it asks.",
        ],
        correct: [
            ex("The weather is awful.", "Rain, sun and wind."),
            ex("I don't know whether to go.", "'If to go'."),
            ex("Whether or not it rains, we will go.", "A choice."),
        ],
        incorrect: [
            ex("The whether is awful.", "Rain, sun and wind: weather."),
            ex("I don't know weather to go.", "'If': whether."),
            ex("Weather or not it rains, we will go.", "A choice: whether."),
        ],
        checked: false,
    },
    GuideSection {
        id: "breath-breathe",
        category: "words",
        title: "18. breath / breathe",
        body: &[
            "'Breath' is the noun, the air itself. 'Breathe' is the verb, the act, and it has the extra e. You take a breath; you breathe.",
        ],
        correct: [
            ex("Take a deep breath.", "The noun."),
            ex("I can't breathe.", "The verb."),
            ex(
                "She held her breath, then began to breathe slowly.",
                "A noun, then a verb.",
            ),
        ],
        incorrect: [
            ex("Take a deep breathe.", "The noun: breath."),
            ex("I can't breath.", "The verb: breathe."),
            ex("He was out of breathe.", "The noun: breath."),
        ],
        checked: false,
    },
    GuideSection {
        id: "eg-ie",
        category: "words",
        title: "19. e.g. / i.e.",
        body: &[
            "'E.g.' means 'for example' and introduces some of the possibilities. 'I.e.' means 'that is' and restates or explains exactly what you mean.",
            "In American style both are followed by a comma: e.g., tea or soup. Use them inside brackets, not in the middle of a flowing sentence.",
        ],
        correct: [
            ex("Bring a hot drink (e.g., tea or soup).", "Two examples."),
            ex(
                "Bring a hot drink (i.e., not iced).",
                "A restatement: what 'hot' means here.",
            ),
            ex(
                "Pack something warm (e.g., a scarf or gloves).",
                "Examples of something warm.",
            ),
        ],
        incorrect: [
            ex(
                "Bring a hot drink (i.e., tea or soup).",
                "These are examples, so e.g.",
            ),
            ex(
                "Bring a hot drink (e.g., not iced).",
                "This is a restatement, so i.e.",
            ),
            ex(
                "Pack something warm (i.e., a scarf or gloves).",
                "Examples again: e.g.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "imply-infer",
        category: "words",
        title: "20. imply / infer",
        body: &[
            "The speaker or writer implies: they hint at something without saying it. The listener or reader infers: they work something out from what they were told.",
        ],
        correct: [
            ex(
                "Are you implying I'm late?",
                "The speaker would be hinting.",
            ),
            ex(
                "I infer from your silence that you're upset.",
                "The listener works it out.",
            ),
            ex("Her smile implied that she knew.", "Her smile hinted."),
        ],
        incorrect: [
            ex(
                "Are you inferring I'm late?",
                "The speaker hints: implying.",
            ),
            ex(
                "I imply from your silence that you're upset.",
                "The listener works it out: infer.",
            ),
            ex(
                "His tone inferred that he was angry.",
                "The tone hinted: implied.",
            ),
        ],
        checked: false,
    },
    GuideSection {
        id: "alot",
        category: "words",
        title: "21. a lot / alot / allot",
        body: &[
            "'A lot' is two words, always. 'Alot' is not a word. 'Allot' (with two l's) is a real word that means to give out a share.",
        ],
        correct: [
            ex("We have a lot of work.", "Two words."),
            ex("She will allot each team a room.", "To assign."),
            ex("A lot of people came.", "Two words."),
        ],
        incorrect: [
            ex("I like it alot.", "'Alot' is not a word: a lot."),
            ex("Alot of people came.", "Two words: a lot."),
            ex("They will alot each student a locker.", "To assign: allot."),
        ],
        checked: false,
    },
    GuideSection {
        id: "could-have",
        category: "words",
        title: "22. could have / should have / would have",
        body: &[
            "Spoken, 'could have' sounds like 'could of', so it is easy to write the wrong thing. 'Of' never belongs there. The word is 'have', or 've in a contraction (could've).",
        ],
        correct: [
            ex("I could have gone.", "'Have'."),
            ex("You should have called.", "'Have'."),
            ex("She would've stayed.", "'Would've' is 'would have'."),
        ],
        incorrect: [
            ex("I could of gone.", "'Could have'."),
            ex("You should of called.", "'Should have'."),
            ex("She would of stayed.", "'Would have'."),
        ],
        checked: false,
    },
    GuideSection {
        id: "lead-led",
        category: "words",
        title: "23. lead / led",
        body: &[
            "'Lead' rhymes with 'seed' when it is the present tense verb (to lead a group). The past tense is 'led', which rhymes with 'red'. The metal 'lead' also rhymes with 'red', which is where the confusion comes from.",
        ],
        correct: [
            ex("She will lead the group.", "Present or future tense."),
            ex("She led the group last year.", "Past tense."),
            ex("The pipe is made of lead.", "The metal."),
        ],
        incorrect: [
            ex("She lead the group last year.", "Past tense: led."),
            ex("She will led the group.", "Future: lead."),
            ex("The pipe is made of led.", "The metal: lead."),
        ],
        checked: false,
    },
    GuideSection {
        id: "spelling-variants",
        category: "words",
        title: "24. British and American spellings",
        body: &[
            "Neither is wrong. They are two standards, and the mistake is mixing them. The usual differences are -our / -or (colour, color), -re / -er (centre, center), -ise / -ize (organise, organize), a doubled l in British words like travelled, and a few single words like grey / gray.",
            "Pick the one your readers expect and check that the whole piece agrees with itself.",
        ],
        correct: [
            ex("The colour of the centre was grey.", "British throughout."),
            ex("The color of the center was gray.", "American throughout."),
            ex(
                "We travelled to the harbour to organise the festival.",
                "British throughout.",
            ),
        ],
        incorrect: [
            ex(
                "The colour of the center was grey.",
                "Mixes British 'colour' with American 'center'.",
            ),
            ex(
                "The color of the centre was gray.",
                "Mixes American 'color' and 'gray' with British 'centre'.",
            ),
            ex(
                "We travelled to the harbor to organise the festival.",
                "'Harbor' is American; the rest is British.",
            ),
        ],
        checked: false,
    },
];

pub const BOOKS_INTRO: &str = "Short lines from public-domain books, with the author and where to find them, so you can see these rules in published writing.";

pub const BOOKS: &[BookExample] = &[
    BookExample {
        quote: "\u{201C}My dear Mr. Bennet,\u{201D} said his lady to him one day",
        credit: "Jane Austen, Pride and Prejudice (1813), chapter 1",
        shows: "Speech ends with a comma inside the quotes, and the tag after it is lowercase. The sentence goes on after the tag.",
    },
    BookExample {
        quote: "\"Bah!\" said Scrooge, \"Humbug!\"",
        credit: "Charles Dickens, A Christmas Carol (1843), Stave I",
        shows: "A tag in the middle. The first part ends with an exclamation mark, which ends a sentence, so the second part starts with a capital.",
    },
    BookExample {
        quote: "\u{2018}Curiouser and curiouser!\u{2019} cried Alice",
        credit: "Lewis Carroll, Alice's Adventures in Wonderland (1865), chapter II",
        shows: "British-style single quotation marks, with a lowercase tag after an exclamation mark.",
    },
];

pub const BREAKING_RULES: &[&str] = &[
    "Professional writers break these rules on purpose, and the rules themselves have changed over time. The Alice line above uses single quotation marks because the book is British and old; today's American editors would use double ones.",
    "Some novelists drop quotation marks from dialogue altogether. Cormac McCarthy does this in The Road, and others use dashes or no marks at all. It works because they do it consistently and the reader is never left unsure who is speaking.",
    "So treat the checker as a second pair of eyes, not a judge. If you broke a rule deliberately, press Ignore. If you did it by accident because you were rushing or tired, that is exactly what it is here to catch.",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> Vec<&'static str> {
        check(text, QuoteStyle::Double)
            .into_iter()
            .map(|i| i.rule)
            .collect()
    }

    fn fixed(text: &str) -> String {
        let mut s = text.to_string();
        let mut edits: Vec<(Range<usize>, String)> = check(text, QuoteStyle::Double)
            .iter()
            .filter_map(DialogueIssue::edit)
            .collect();
        edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
        for (range, new) in edits {
            s.replace_range(range, &new);
        }
        s
    }

    #[test]
    fn the_guide_and_the_checker_agree() {
        for section in GUIDE {
            assert_eq!(section.correct.len(), 3, "{}", section.id);
            assert_eq!(section.incorrect.len(), 3, "{}", section.id);
            if !section.checked {
                continue;
            }
            for e in &section.correct {
                assert!(
                    check(e.text, QuoteStyle::Double).is_empty(),
                    "{}: should be clean: {}",
                    section.id,
                    e.text
                );
            }
            for e in &section.incorrect {
                let found = check(e.text, QuoteStyle::Double);
                assert!(
                    !found.is_empty(),
                    "{}: should be flagged: {}",
                    section.id,
                    e.text
                );
                assert!(
                    found.iter().any(|i| i.guide == section.id),
                    "{}: flagged by the wrong rule: {}",
                    section.id,
                    e.text
                );
            }
        }
    }

    #[test]
    fn the_guide_is_complete_and_organised() {
        let mut ids = std::collections::HashSet::new();
        for s in GUIDE {
            assert!(ids.insert(s.id), "duplicate section id {}", s.id);
            assert!(
                GUIDE_CATEGORIES.iter().any(|c| c.id == s.category),
                "{}: unknown category {}",
                s.id,
                s.category
            );
            assert!(!s.title.is_empty() && !s.body.is_empty(), "{}", s.id);
            for e in s.correct.iter().chain(s.incorrect.iter()) {
                assert!(
                    !e.text.is_empty() && !e.why.is_empty(),
                    "{}: empty example",
                    s.id
                );
            }
        }
        for c in GUIDE_CATEGORIES {
            assert!(
                GUIDE.iter().any(|s| s.category == c.id),
                "category {} has no sections",
                c.id
            );
        }
        assert!(GUIDE.len() >= 40, "{}", GUIDE.len());
        // Only the dialogue rules the checker can see are held to it.
        let checked: Vec<&str> = GUIDE.iter().filter(|s| s.checked).map(|s| s.id).collect();
        assert_eq!(
            checked,
            vec![
                "tag-punctuation",
                "lowercase-tags",
                "beats-vs-tags",
                "split-dialogue",
                "paired-quotes",
                "questions-exclamations"
            ]
        );
    }

    #[test]
    fn correct_examples_never_trip_the_dialogue_checker() {
        for s in GUIDE {
            for e in &s.correct {
                let style = detect_style(e.text);
                assert!(
                    check(e.text, style).is_empty(),
                    "{}: flagged a correct example: {}",
                    s.id,
                    e.text
                );
            }
        }
    }

    #[test]
    fn lists_in_the_guide_use_the_oxford_comma() {
        // "a, b and c" without a comma before "and" would contradict the guide's own advice. The
        // one deliberate exception is the example that shows the missing comma.
        for s in GUIDE
            .iter()
            .filter(|s| s.id != "commas" && s.category != "dialogue")
        {
            for e in &s.correct {
                let t = e.text;
                if let Some(at) = t.find(" and ") {
                    let before = &t[..at];
                    let commas = before.matches(',').count();
                    assert!(
                        commas < 2 || before.ends_with(',') || before.ends_with(';'),
                        "{}: {}",
                        s.id,
                        t
                    );
                }
            }
        }
    }

    #[test]
    fn every_issue_names_a_real_guide_section() {
        for text in [
            "\"Hi.\" she said.",
            "\"Hi\", she said.",
            "\"Hi,\" she smiled.",
            "\"Hi?,\" she said.",
            "\"Well,\" she said, \"That's odd.\"",
            "She said, \"Hi.",
            "Hi.\" she said.",
            "\"Hi,\u{201D} she said.",
        ] {
            for i in check(text, QuoteStyle::Double) {
                assert!(GUIDE.iter().any(|s| s.id == i.guide), "{text}: {}", i.guide);
            }
        }
    }

    #[test]
    fn replacements_repair_the_text() {
        assert_eq!(
            fixed("\"I'm tired.\" she said."),
            "\"I'm tired,\" she said."
        );
        assert_eq!(
            fixed("\"We should leave now.\" He said."),
            "\"We should leave now,\" he said."
        );
        assert_eq!(
            fixed("\"Come here\", she whispered."),
            "\"Come here,\" she whispered."
        );
        assert_eq!(
            fixed("\"Where are you going?\" She asked."),
            "\"Where are you going?\" she asked."
        );
        assert_eq!(fixed("\"Thanks,\" she smiled."), "\"Thanks.\" She smiled.");
        assert_eq!(fixed("\"Fine,\" Tom shrugged."), "\"Fine.\" Tom shrugged.");
        assert_eq!(
            fixed("\"Where are you?,\" she asked."),
            "\"Where are you?\" she asked."
        );
        assert_eq!(
            fixed("\"Well,\" she said, \"That's odd.\""),
            "\"Well,\" she said, \"that's odd.\""
        );
        assert_eq!(fixed("\"Hi.\" said Tom."), "\"Hi,\" said Tom.");
    }

    #[test]
    fn unpaired_and_mixed_quotes_can_be_repaired() {
        assert_eq!(fixed("She said, \"Come here."), "She said, \"Come here.\"");
        assert_eq!(fixed("Come here.\" she said."), "\"Come here.\" she said.");
        assert_eq!(
            fixed("\"Hello,\u{201D} she said."),
            "\u{201C}Hello,\u{201D} she said."
        );
        assert_eq!(
            fixed("\u{201C}Hello,\" she said."),
            "\u{201C}Hello,\u{201D} she said."
        );
        assert_eq!(
            fixed("She said, \u{201C}Come here."),
            "She said, \u{201C}Come here.\u{201D}"
        );
    }

    #[test]
    fn every_issue_has_a_fix() {
        for section in GUIDE.iter().filter(|s| s.checked) {
            for e in &section.incorrect {
                for i in check(e.text, QuoteStyle::Double) {
                    assert!(i.edit().is_some(), "{}: no fix for {}", i.rule, e.text);
                }
            }
        }
        for text in [
            "She said, \"Hi.",
            "Hi.\" she said.",
            "\"Hi,\u{201D} she said.",
            "\"Hi?,\" she said.",
        ] {
            for i in check(text, QuoteStyle::Double) {
                assert!(i.edit().is_some(), "{}: no fix for {text}", i.rule);
            }
        }
    }

    #[test]
    fn previews_show_the_text_before_and_after() {
        let t = "\"Thanks,\" she smiled.";
        let i = check(t, QuoteStyle::Double).into_iter().next().unwrap();
        let p = preview(t, &i);
        assert_eq!(p.window, t);
        assert_eq!(p.fixed.as_deref(), Some("\"Thanks.\" She smiled."));
        let f = p.fixed.as_deref().unwrap();
        assert_eq!(&f[p.fixed_hl.clone().unwrap()], ".\" She");
        assert_eq!(&p.window[p.hl.clone()], ",\" she");
        assert!(!p.lead && !p.trail);

        // An insertion elsewhere: the flagged mark and the added mark are both inside the window.
        let t = "She said, \"Come here.";
        let i = check(t, QuoteStyle::Double).into_iter().next().unwrap();
        let p = preview(t, &i);
        assert_eq!(&p.window[p.hl.clone()], "\"");
        assert_eq!(p.fixed.as_deref(), Some("She said, \"Come here.\""));

        // A long paragraph is cut around the issue, at word boundaries.
        let long = format!(
            "{} \"Hi.\" she said. {}",
            "word ".repeat(200),
            "more ".repeat(200)
        );
        let i = check(&long, QuoteStyle::Double).into_iter().next().unwrap();
        let p = preview(&long, &i);
        assert!(p.lead && p.trail);
        assert!(p.window.len() < long.len() / 2);
        assert!(p.window.starts_with("word"));
        assert_eq!(&p.window[p.hl.clone()], ".\" she");
        assert!(p.window.contains("\"Hi.\" she said."));
    }

    #[test]
    fn previews_never_panic() {
        for t in [
            "\"",
            "\"a.\" she",
            "\u{201C}\u{e9}\u{e9}\u{e9}",
            "x\u{1F600}\"y.\" she said.",
            "\"a,\" she smiled",
        ] {
            for style in [QuoteStyle::Double, QuoteStyle::Single] {
                for i in check(t, style) {
                    let _ = preview(t, &i);
                }
            }
        }
        let big = "\u{e9}".repeat(5000) + "\"Hi.\" she said." + &"\u{e9}".repeat(5000);
        for i in check(&big, QuoteStyle::Double) {
            let p = preview(&big, &i);
            assert!(p.window.chars().count() < 800);
        }
        // A stray closing mark far from the start: the fix (an opening mark at the start) is far
        // away, so it is described and not shown.
        let far = format!("{} Come here.\" she said.", "word ".repeat(300));
        let i = check(&far, QuoteStyle::Double).into_iter().next().unwrap();
        let p = preview(&far, &i);
        assert_eq!(p.far_edit, Some((true, "\"".to_string())));
        assert!(p.fixed.is_none());
        assert!(p.window.len() < 700);
        // A fix close to the flagged text is shown.
        let near = check("Come here.\" she said.", QuoteStyle::Double)
            .into_iter()
            .next()
            .unwrap();
        let p = preview("Come here.\" she said.", &near);
        assert!(p.far_edit.is_none());
        assert_eq!(p.fixed.as_deref(), Some("\"Come here.\" she said."));
    }

    #[test]
    fn mismatched_pairs_are_found_and_repaired() {
        let c = |t: &str| rules(t);
        // The case from real use: a curly single opener, a curly double closer.
        let t = "\u{2018}To be or not to be, that is the question.\u{201D}";
        assert_eq!(detect_style(t), QuoteStyle::Double);
        assert_eq!(c(t), vec!["mismatched-quotes"]);
        assert_eq!(
            fixed(t),
            "\u{201C}To be or not to be, that is the question.\u{201D}"
        );
        // In single-quote text the double mark is the odd one out.
        let single = check(t, QuoteStyle::Single);
        assert_eq!(single.len(), 1);
        assert_eq!(
            single[0].edit().map(|(_, s)| s),
            Some("\u{2019}".to_string())
        );
        // The other combinations, curly and straight.
        assert_eq!(
            fixed("\u{201C}Hello,\u{2019} she said."),
            "\u{201C}Hello,\u{201D} she said."
        );
        assert_eq!(fixed("'Hello,\" she said."), "\"Hello,\" she said.");
        assert_eq!(fixed("\"Hello,' she said."), "\"Hello,\" she said.");
        assert_eq!(fixed("\u{2018}Hello,\" she said."), "\"Hello,\" she said.");
        // No second, misleading "add an opening mark" fix on top of it.
        assert_eq!(
            c("\u{2018}Hello,\u{201D} she said."),
            vec!["mismatched-quotes"]
        );
    }

    #[test]
    fn a_leftover_single_mark_inside_a_quotation_is_found() {
        // What the old Apply Change produced from the case above.
        let t = "\u{201C}\u{2018}To be or not to be, that is the question.\u{201D}";
        assert_eq!(rules(t), vec!["unclosed-single"]);
        assert_eq!(
            fixed(t),
            "\u{201C}To be or not to be, that is the question.\u{201D}"
        );
        // A closed nested quotation, and apostrophes, are fine.
        for ok in [
            "\"She told me, 'Wait here,' and left,\" he said.",
            "\u{201C}She told me, \u{2018}Wait here,\u{2019} and left,\u{201D} he said.",
            "\"The boys' toys are here,\" she said.",
            "\u{201C}The boys\u{2019} toys are here,\u{201D} she said.",
            "\"I did it 'cause I could,\" she said.",
            "\u{201C}\u{2019}Cause I said so,\u{201D} she said.",
            "\"It's the dogs' bowls,\" Tom said.",
        ] {
            assert!(
                check(ok, QuoteStyle::Double).is_empty(),
                "flagged: {ok} -> {:?}",
                rules(ok)
            );
        }
    }

    #[test]
    fn the_style_is_not_fooled_by_a_stray_mark() {
        // One mismatched mark must not turn a double-quote document into a single-quote one.
        assert_eq!(detect_style("\u{2018}To be.\u{201D}"), QuoteStyle::Double);
        assert_eq!(
            detect_style("\"One,\" he said. \"Two,\" she said. 'x\" "),
            QuoteStyle::Double
        );
        // Real single-quote text still reads as single.
        assert_eq!(
            detect_style("\u{2018}One,\u{2019} he said. \u{2018}Two,\u{2019} she said."),
            QuoteStyle::Single
        );
    }

    #[test]
    fn hostile_quote_runs_are_fast() {
        for unit in [
            "\"",
            "'",
            "\u{2018}",
            "\u{201C}",
            "\u{2019}",
            "\"'",
            "'\"",
            "\u{201C}\u{2018}",
            "a'",
        ] {
            let big = unit.repeat(60_000);
            let _ = detect_style(&big);
            let _ = check(&big, QuoteStyle::Double);
            let _ = check(&big, QuoteStyle::Single);
        }
    }

    #[test]
    fn an_interruption_ending_in_a_dash_is_a_closed_quotation() {
        for text in [
            "\"I was just\u{2014}\" he began.",
            "\"But I never\u{2014}\" Tom stopped.",
            "\"Wait\u{2014}\" she said, \"I can explain.\"",
            "He said\u{2014}\"Wait.\"",
        ] {
            assert!(
                check(text, QuoteStyle::Double).is_empty(),
                "flagged: {text} -> {:?}",
                rules(text)
            );
        }
        assert_eq!(
            fixed("\"I was just\u{2014},\" he began."),
            "\"I was just\u{2014}\" he began."
        );
    }

    #[test]
    fn correct_dialogue_is_left_alone() {
        for text in [
            "\"Hello,\" she said.",
            "\"Hello.\" She smiled.",
            "\"Hello!\" he said. \"How are you?\" Then he waited.",
            "\"Well,\" she said, \"I suppose so.\"",
            "\"Stop!\" Tom said. \"That's enough.\"",
            "She said nothing. He didn't either.",
            "He said, \"I'll go.\"",
            "\"I know,\" I said.",
            "\"Yes,\" said Mara, \"I know.\"",
            "It's the dog's bone, isn't it? \"Maybe,\" she said.",
            "\"Well...\" she said.",
            "\"So,\" Alice said, \"I am here.\"",
            "",
            "   ",
        ] {
            assert!(
                check(text, QuoteStyle::Double).is_empty(),
                "flagged: {text} -> {:?}",
                rules(text)
            );
        }
    }

    #[test]
    fn single_quote_style_is_checked_without_the_american_rules() {
        let s = QuoteStyle::Single;
        assert!(check("'Hello,' she said.", s).is_empty());
        assert!(check("'Hello', she said.", s).is_empty());
        assert_eq!(
            check("'Hello.' she said.", s).first().map(|i| i.rule),
            Some("tag-period")
        );
        assert_eq!(
            check("'Where?' She asked.", s).first().map(|i| i.rule),
            Some("tag-capital")
        );
        assert!(check("It isn\u{2019}t the dogs\u{2019} fault, Tom said.", s).is_empty());
        assert!(check("\u{2018}Curiouser and curiouser!\u{2019} cried Alice", s).is_empty());
    }

    #[test]
    fn style_is_detected_from_the_opening_marks() {
        assert_eq!(
            detect_style("\"Hi,\" she said. \"Bye,\" he said."),
            QuoteStyle::Double
        );
        assert_eq!(
            detect_style("\u{2018}Hi,\u{2019} she said. \u{2018}Bye,\u{2019} he said."),
            QuoteStyle::Single
        );
        assert_eq!(
            detect_style("'Hi,' she said. 'Bye,' he said. It isn't odd."),
            QuoteStyle::Single
        );
        assert_eq!(
            detect_style("No quotes here, and it's fine."),
            QuoteStyle::Double
        );
        assert_eq!(detect_style(""), QuoteStyle::Double);
        assert_eq!(
            detect_style("\"One,\" he said, \"I can't.\" 'x'"),
            QuoteStyle::Double
        );
    }

    #[test]
    fn book_quotes_are_short_and_credited() {
        for b in BOOKS {
            assert!(b.quote.split_whitespace().count() < 15, "{}", b.quote);
            assert!(!b.credit.is_empty() && !b.shows.is_empty());
        }
        assert_eq!(BOOKS.len(), 3);
        // Each book is quoted once.
        let mut credits: Vec<&str> = BOOKS
            .iter()
            .map(|b| b.credit.split(',').next().unwrap_or(""))
            .collect();
        credits.sort_unstable();
        credits.dedup();
        assert_eq!(credits.len(), BOOKS.len());
    }

    #[test]
    fn hostile_input_never_panics() {
        for t in [
            "\"",
            "\u{201C}",
            "'",
            "\u{2019}",
            "\"\"",
            "\",",
            "\"a\",",
            ",\"",
            "\"\"\"\"",
            "\"a.\" ",
            "\"a.\" she",
            "\"a.\" she said",
            "\"a,\" \u{e9} said",
            "\"a?,\"",
            "\u{2014}\",",
            "\"x\" \"y\"",
            "\"x,\" he said, \"",
            "\"x,\" he said, \"Y",
            "\u{0}",
            "\"\u{1F600}\" she said",
            "\"a.\"\n\"b.\" he said",
        ] {
            for style in [QuoteStyle::Double, QuoteStyle::Single] {
                let _ = check(t, style);
            }
            let _ = detect_style(t);
        }
        let big = "\"Hi.\" she said. ".repeat(20_000);
        assert_eq!(check(&big, QuoteStyle::Double).len(), 20_000);
    }
}
