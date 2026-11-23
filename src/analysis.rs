//! Writing analysis for the side panels: word and character counts, reading level, keyword
//! density, sentence lengths and a rule-based English grammar check.
//!
//! Pure text in, plain data out: no document model, no UI. The front end feeds it a paragraph
//! (or the whole document's text) and maps the returned **byte ranges** back to positions.
//! Nothing here panics: every slice goes through `str::get`, every index through `get`.
//!
//! The grammar rules are deliberately conservative. A false alarm costs the writer more than a
//! missed error, so each rule only fires on a pattern that is wrong in nearly every context.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

// ---------------------------------------------------------------------------------------------
// Tokens and sentences
// ---------------------------------------------------------------------------------------------

/// A word: a run of letters, with apostrophes and hyphens kept when a letter follows them
/// (`don't`, `well-known`). Byte offsets into the analysed text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn text<'a>(&self, text: &'a str) -> &'a str {
        text.get(self.start..self.end).unwrap_or("")
    }
}

/// Split `text` into word tokens.
pub fn tokens(text: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&(start, c)) = chars.get(i) {
        if !c.is_alphabetic() {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while let Some(&(_, cj)) = chars.get(j) {
            if cj.is_alphabetic() {
                j += 1;
                continue;
            }
            if matches!(cj, '\'' | '\u{2019}' | '-') {
                if let Some(&(_, next)) = chars.get(j + 1) {
                    if next.is_alphabetic() {
                        j += 2;
                        continue;
                    }
                }
            }
            break;
        }
        let end = chars.get(j).map_or(text.len(), |&(b, _)| b);
        out.push(Token { start, end });
        i = j;
    }
    out
}

const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "mt", "vs", "e.g", "i.e", "cf", "fig",
    "approx", "inc",
];

fn is_abbreviation(chars: &[(usize, char)], dot: usize) -> bool {
    let mut p = dot;
    while p > 0 {
        match chars.get(p - 1) {
            Some(&(_, c)) if c.is_alphabetic() || c == '.' => p -= 1,
            _ => break,
        }
    }
    let word: String = chars
        .get(p..dot)
        .unwrap_or(&[])
        .iter()
        .map(|&(_, c)| c)
        .collect();
    if word.is_empty() {
        return false;
    }
    let lower = word.to_lowercase();
    if ABBREVIATIONS.contains(&lower.as_str()) {
        return true;
    }
    // A single capital is an initial ("J. R. R. Tolkien"), except the pronoun "I".
    let mut it = word.chars();
    matches!((it.next(), it.next()), (Some(c), None) if c.is_uppercase() && c != 'I')
}

fn emit(text: &str, chars: &[(usize, char)], from: usize, to: usize, out: &mut Vec<Range<usize>>) {
    let mut a = from;
    let mut b = to;
    while a < b && chars.get(a).is_some_and(|&(_, c)| c.is_whitespace()) {
        a += 1;
    }
    while b > a && chars.get(b - 1).is_some_and(|&(_, c)| c.is_whitespace()) {
        b -= 1;
    }
    let start = chars.get(a).map_or(text.len(), |&(x, _)| x);
    let end = chars.get(b).map_or(text.len(), |&(x, _)| x);
    if start < end
        && text
            .get(start..end)
            .is_some_and(|s| s.chars().any(char::is_alphanumeric))
    {
        out.push(start..end);
    }
}

/// Sentence ranges (byte offsets, trimmed). A line break always ends a sentence; `.`, `!`, `?`
/// and `…` end one when followed by a capital, digit, quote or the end. Abbreviations ("Dr.",
/// "e.g.") and initials do not end one, nor does an ellipsis followed by a lowercase letter.
pub fn sentences(text: &str) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < n {
        let Some(&(_, c)) = chars.get(i) else { break };
        if c == '\n' || c == '\r' {
            emit(text, &chars, start, i, &mut out);
            start = i + 1;
            i += 1;
            continue;
        }
        if matches!(c, '.' | '!' | '?' | '\u{2026}') {
            let mut j = i;
            while chars
                .get(j)
                .is_some_and(|&(_, x)| matches!(x, '.' | '!' | '?' | '\u{2026}'))
            {
                j += 1;
            }
            let run = j - i;
            let mut k = j;
            while chars.get(k).is_some_and(|&(_, x)| {
                matches!(x, '"' | '\'' | '\u{201D}' | '\u{2019}' | ')' | ']')
            }) {
                k += 1;
            }
            // "3.14" and "e.g.x": no whitespace after the mark, no boundary.
            if chars.get(k).is_some_and(|&(_, x)| !x.is_whitespace()) {
                i = j;
                continue;
            }
            let mut m = k;
            while chars
                .get(m)
                .is_some_and(|&(_, x)| x.is_whitespace() && x != '\n')
            {
                m += 1;
            }
            let mut boundary = !chars.get(m).is_some_and(|&(_, x)| x.is_lowercase());
            if boundary && run == 1 && c == '.' && is_abbreviation(&chars, i) {
                boundary = false;
            }
            if boundary {
                emit(text, &chars, start, k, &mut out);
                start = k;
            }
            i = k;
            continue;
        }
        i += 1;
    }
    emit(text, &chars, start, n, &mut out);
    out
}

// ---------------------------------------------------------------------------------------------
// Counts and reading level
// ---------------------------------------------------------------------------------------------

/// Counts for the Word Count box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Whitespace-separated runs containing a letter or digit (how Word counts).
    pub words: usize,
    /// Characters including spaces; line breaks do not count.
    pub characters_with_spaces: usize,
    pub characters_no_spaces: usize,
    pub sentences: usize,
}

pub fn counts(text: &str) -> Counts {
    let words = text
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count();
    let characters_with_spaces = text.chars().filter(|c| !matches!(c, '\n' | '\r')).count();
    let characters_no_spaces = text.chars().filter(|c| !c.is_whitespace()).count();
    Counts {
        words,
        characters_with_spaces,
        characters_no_spaces,
        sentences: sentences(text).len(),
    }
}

/// English syllable estimate for one word (vowel groups, minus a silent final "e"/"ed").
pub fn syllables(word: &str) -> usize {
    let w: Vec<char> = word
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect();
    if w.is_empty() {
        return 0;
    }
    let vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
    let mut count = 0usize;
    let mut prev = false;
    for &c in &w {
        let v = vowel(c);
        if v && !prev {
            count += 1;
        }
        prev = v;
    }
    let len = w.len();
    let last = w.get(len.wrapping_sub(1)).copied().unwrap_or(' ');
    let before = len
        .checked_sub(2)
        .and_then(|i| w.get(i))
        .copied()
        .unwrap_or(' ');
    let before2 = len
        .checked_sub(3)
        .and_then(|i| w.get(i))
        .copied()
        .unwrap_or(' ');
    if count > 1 {
        if last == 'e' && !(before == 'l' && !vowel(before2)) {
            count -= 1;
        } else if last == 'd' && before == 'e' && !matches!(before2, 't' | 'd') && !vowel(before2) {
            count -= 1;
        }
    }
    count.max(1)
}

/// Flesch-Kincaid grade level; `None` for text with no words or no sentences.
pub fn reading_grade(text: &str) -> Option<f64> {
    let words = tokens(text);
    if words.is_empty() {
        return None;
    }
    let sentence_count = sentences(text).len();
    if sentence_count == 0 {
        return None;
    }
    let syl: usize = words.iter().map(|t| syllables(t.text(text))).sum();
    let w = words.len() as f64;
    let grade = 0.39 * (w / sentence_count as f64) + 11.8 * (syl as f64 / w) - 15.59;
    Some(grade.max(0.0))
}

/// The grade as a band for display: "11-12th Grade".
pub fn grade_label(grade: f64) -> &'static str {
    let g = grade.round();
    if g <= 5.0 {
        "5th Grade or below"
    } else if g <= 8.0 {
        "6-8th Grade"
    } else if g <= 10.0 {
        "9-10th Grade"
    } else if g <= 12.0 {
        "11-12th Grade"
    } else if g <= 16.0 {
        "College"
    } else {
        "Graduate"
    }
}

// ---------------------------------------------------------------------------------------------
// Dale-Chall readability
// ---------------------------------------------------------------------------------------------

/// Familiar words, one or more per line, separated by spaces. Replace the file
/// `src/dale_chall_words.txt` (the official ~3,000-word list).
const FAMILIAR_WORDS: &str = include_str!("dale_chall_words.txt");

fn familiar_set() -> &'static std::collections::HashSet<String> {
    static SET: std::sync::OnceLock<std::collections::HashSet<String>> =
        std::sync::OnceLock::new();
    SET.get_or_init(|| {
        // A list saved at %APPDATA%\GrammarDesk\dale_chall_words.txt replaces the built-in one.
        let custom = std::env::var_os("APPDATA")
            .map(|d| std::path::PathBuf::from(d).join("GrammarDesk").join("dale_chall_words.txt"))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .filter(|t| t.split_whitespace().count() > 500);
        let text = custom.as_deref().unwrap_or(FAMILIAR_WORDS);
        let mut set = std::collections::HashSet::new();
        for raw in text.split_whitespace() {
            let w = raw
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
                .to_lowercase();
            if w.is_empty() {
                continue;
            }
            // "baby-sitter" also makes "baby" and "sitter" familiar, since words are split at hyphens.
            if w.contains('-') {
                for part in w.split('-').filter(|p| !p.is_empty()) {
                    set.insert(part.to_string());
                }
            }
            set.insert(w);
        }
        set
    })
}

/// True when `word` (any case) is on the familiar list or is a regular inflection of a word on it.
fn is_familiar(word: &str) -> bool {
    let set = familiar_set();
    let w: String = word.to_lowercase().replace('\u{2019}', "'");
    let has = |s: &str| set.contains(s);
    if has(&w) {
        return true;
    }
    // Possessives and contractions: "dog's", "dogs'".
    let w = w.trim_end_matches('\'');
    if let Some(base) = w.strip_suffix("'s") {
        return is_familiar(base);
    }
    if w.contains('\'') {
        return w.split('\'').next().is_some_and(|b| !b.is_empty() && has(b));
    }
    if has(w) {
        return true;
    }
    let try_suffix = |suffix: &str, add: &str| -> bool {
        w.strip_suffix(suffix)
            .is_some_and(|stem| stem.len() >= 2 && has(&format!("{stem}{add}")))
    };
    // Doubled final consonant: "stopped", "running", "bigger".
    let undouble = |suffix: &str| -> bool {
        w.strip_suffix(suffix).is_some_and(|stem| {
            let b = stem.as_bytes();
            b.len() >= 3 && b[b.len() - 1] == b[b.len() - 2] && has(&stem[..stem.len() - 1])
        })
    };
    try_suffix("s", "")
        || try_suffix("es", "")
        || try_suffix("ies", "y")
        || try_suffix("ed", "")
        || try_suffix("ed", "e")
        || try_suffix("d", "")
        || try_suffix("ied", "y")
        || try_suffix("ing", "")
        || try_suffix("ing", "e")
        || try_suffix("ly", "")
        || try_suffix("ily", "y")
        || try_suffix("er", "")
        || try_suffix("er", "e")
        || try_suffix("est", "")
        || try_suffix("est", "e")
        || undouble("ed")
        || undouble("ing")
        || undouble("er")
        || undouble("est")
}

/// Result of a Dale-Chall measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DaleChall {
    /// The adjusted score (3.6365 added when more than 5% of words are difficult).
    pub score: f64,
    /// Share of words that are not on the familiar list, 0 to 100.
    pub difficult_percent: f64,
    /// Average words per sentence.
    pub avg_sentence_len: f64,
}

/// Dale-Chall readability: `0.1579 * difficult% + 0.0496 * words-per-sentence`, plus 3.6365 when
/// more than 5% of the words are difficult. Proper nouns (capitalised words that do not start a
/// sentence) count as familiar, and numbers are not counted as words. `None` without words.
pub fn dale_chall(text: &str) -> Option<DaleChall> {
    let toks = tokens(text);
    if toks.is_empty() {
        return None;
    }
    let sents = sentences(text);
    if sents.is_empty() {
        return None;
    }
    let starts: Vec<usize> = sents
        .iter()
        .filter_map(|r| toks.iter().find(|t| t.start >= r.start && t.start < r.end))
        .map(|t| t.start)
        .collect();
    let mut difficult = 0usize;
    for t in &toks {
        let w = t.text(text);
        let capital = w.chars().next().is_some_and(char::is_uppercase);
        if capital && !starts.contains(&t.start) {
            continue;
        }
        if !is_familiar(w) {
            difficult += 1;
        }
    }
    let words = toks.len() as f64;
    let pct = difficult as f64 * 100.0 / words;
    let avg = words / sents.len() as f64;
    let mut score = 0.1579 * pct + 0.0496 * avg;
    if pct > 5.0 {
        score += 3.6365;
    }
    Some(DaleChall {
        score,
        difficult_percent: pct,
        avg_sentence_len: avg,
    })
}

/// The Dale-Chall score as the grades a reader needs to understand the text.
pub fn dale_chall_label(score: f64) -> &'static str {
    if score < 5.0 {
        "4th Grade or below"
    } else if score < 6.0 {
        "5-6th Grade"
    } else if score < 7.0 {
        "7-8th Grade"
    } else if score < 8.0 {
        "9-10th Grade"
    } else if score < 9.0 {
        "11-12th Grade"
    } else {
        "College"
    }
}

#[cfg(test)]
mod compound_subject_tests {
    use super::*;

    #[test]
    fn compound_subjects_are_not_flagged() {
        let on = EnabledCategories::default();
        for t in ["John and I are planning a trip.", "Tom and she are late.", "Either he or I am wrong."] {
            assert!(check_grammar(t, &on).is_empty(), "{t}");
        }
        assert!(!check_grammar("I are late.", &on).is_empty());
    }
}

#[cfg(test)]
mod dale_chall_tests {
    use super::*;

    #[test]
    fn easy_text_scores_low() {
        let d = dale_chall("The cat sat on the bed. The dog ran to the barn.").unwrap();
        assert!(d.difficult_percent < 5.0, "{d:?}");
        assert!(d.score < 5.0, "{d:?}");
        assert_eq!(dale_chall_label(d.score), "4th Grade or below");
    }

    #[test]
    fn hard_text_scores_high_with_adjustment() {
        let d = dale_chall(
            "Epistemological considerations notwithstanding, institutional heterogeneity \
             necessitates comprehensive jurisprudential reevaluation.",
        )
        .unwrap();
        assert!(d.difficult_percent > 5.0);
        assert!(d.score >= 9.0, "{d:?}");
    }

    #[test]
    fn inflections_and_names() {
        assert!(is_familiar("Walked"));
        assert!(is_familiar("stopped"));
        assert!(is_familiar("dogs"));
        assert!(is_familiar("don't"));
        assert!(!is_familiar("jurisprudence"));
        let d = dale_chall("Yesterday Zephyrine walked home.").unwrap();
        assert_eq!(d.difficult_percent, 0.0);
    }

    #[test]
    fn empty_is_none() {
        assert_eq!(dale_chall(""), None);
        assert_eq!(dale_chall("1234 5678."), None);
    }
}

// ---------------------------------------------------------------------------------------------
// Keyword density
// ---------------------------------------------------------------------------------------------

/// One row of the Keyword Density box.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyword {
    /// The word as first written (so "Priestess" stays capitalised).
    pub word: String,
    pub count: usize,
    /// Share of all words in the text, 0-100.
    pub percent: f64,
}

impl Keyword {
    /// "4%", or "0.4%" below one percent.
    pub fn percent_label(&self) -> String {
        if self.percent >= 1.0 {
            format!("{:.0}%", self.percent)
        } else {
            format!("{:.1}%", self.percent)
        }
    }
}

const STOP_WORDS: &[&str] = &[
    "a", "about", "after", "again", "all", "also", "am", "an", "and", "any", "are", "as", "at",
    "be", "because", "been", "before", "being", "but", "by", "can", "could", "did", "do", "does",
    "doing", "down", "each", "for", "from", "had", "has", "have", "having", "he", "her", "here",
    "hers", "him", "himself", "his", "how", "i", "if", "in", "into", "is", "it", "its", "itself",
    "just", "me", "my", "myself", "no", "nor", "not", "now", "of", "off", "on",
    "once", "only", "or", "other", "our", "ours", "out", "over", "own", "same", "she", "should",
    "so", "some", "such", "than", "that", "the", "their", "theirs", "them", "then", "there",
    "these", "they", "this", "those", "through", "to", "too", "under", "until", "up", "us", "very",
    "was", "we", "were", "what", "when", "where", "which", "while", "who", "whom", "why", "will",
    "with", "would", "you", "your", "yours", "yourself", "yet", "s", "t", "d", "ll", "re", "ve",
    "m", "don", "didn",
];

/// How a word is counted as a keyword: lowercase, with a possessive "'s" folded into its word.
pub fn keyword_key(raw: &str) -> String {
    raw.strip_suffix("'s")
        .or_else(|| raw.strip_suffix("\u{2019}s"))
        .unwrap_or(raw)
        .to_lowercase()
}

/// Byte ranges of every occurrence of the given keywords (as [`keyword_key`] writes them).
pub fn keyword_spans(text: &str, keys: &HashSet<String>) -> Vec<Range<usize>> {
    if keys.is_empty() {
        return Vec::new();
    }
    tokens(text)
        .into_iter()
        .filter(|t| keys.contains(&keyword_key(t.text(text))))
        .map(|t| t.start..t.end)
        .collect()
}

/// The most common words, most frequent first (ties alphabetical). Possessive "'s" is folded
/// into the base word. `skip_stop_words` drops "the", "and" and friends; `limit` caps the list.
pub fn keyword_density(text: &str, skip_stop_words: bool, limit: usize) -> Vec<Keyword> {
    let toks = tokens(text);
    let total = toks.len();
    if total == 0 || limit == 0 {
        return Vec::new();
    }
    let mut map: HashMap<String, (String, usize)> = HashMap::new();
    // Percentages are out of the words that are counted (stop words left out), as word counters do.
    let mut counted = 0usize;
    for t in &toks {
        let raw = t.text(text);
        let base = raw
            .strip_suffix("'s")
            .or_else(|| raw.strip_suffix("\u{2019}s"))
            .unwrap_or(raw);
        let key = base.to_lowercase();
        if key.chars().count() < 2 || (skip_stop_words && STOP_WORDS.contains(&key.as_str())) {
            continue;
        }
        counted += 1;
        map.entry(key).or_insert_with(|| (base.to_string(), 0)).1 += 1;
    }
    let total = counted.max(1);
    let mut rows: Vec<Keyword> = map
        .into_iter()
        .map(|(_, (word, count))| Keyword {
            word,
            count,
            percent: count as f64 * 100.0 / total as f64,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.word.to_lowercase().cmp(&b.word.to_lowercase()))
    });
    rows.truncate(limit);
    rows
}

// ---------------------------------------------------------------------------------------------
// Sentence length
// ---------------------------------------------------------------------------------------------

/// The six length ranges of the Sentence Length box, shortest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LengthBucket {
    One,
    TwoToSix,
    SevenToFifteen,
    SixteenToTwentyFive,
    TwentySixToThirtyNine,
    FortyPlus,
}

impl LengthBucket {
    pub const ALL: [LengthBucket; 6] = [
        LengthBucket::One,
        LengthBucket::TwoToSix,
        LengthBucket::SevenToFifteen,
        LengthBucket::SixteenToTwentyFive,
        LengthBucket::TwentySixToThirtyNine,
        LengthBucket::FortyPlus,
    ];

    /// `None` for zero words.
    pub fn of(words: usize) -> Option<LengthBucket> {
        match words {
            0 => None,
            1 => Some(LengthBucket::One),
            2..=6 => Some(LengthBucket::TwoToSix),
            7..=15 => Some(LengthBucket::SevenToFifteen),
            16..=25 => Some(LengthBucket::SixteenToTwentyFive),
            26..=39 => Some(LengthBucket::TwentySixToThirtyNine),
            _ => Some(LengthBucket::FortyPlus),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LengthBucket::One => "1 word",
            LengthBucket::TwoToSix => "2-6 words",
            LengthBucket::SevenToFifteen => "7-15 words",
            LengthBucket::SixteenToTwentyFive => "16-25 words",
            LengthBucket::TwentySixToThirtyNine => "26-39 words",
            LengthBucket::FortyPlus => "40+ words",
        }
    }
}

/// One sentence with its length, for tinting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentenceLength {
    pub range: Range<usize>,
    pub words: usize,
    pub bucket: LengthBucket,
}

pub fn sentence_lengths(text: &str) -> Vec<SentenceLength> {
    let toks = tokens(text);
    sentences(text)
        .into_iter()
        .filter_map(|range| {
            let words = toks
                .iter()
                .filter(|t| t.start >= range.start && t.end <= range.end)
                .count();
            LengthBucket::of(words).map(|bucket| SentenceLength {
                range,
                words,
                bucket,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Grammar
// ---------------------------------------------------------------------------------------------

/// The six issue types of the Grammar Checker box, each switchable in settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    SubjectVerb,
    Fragment,
    Punctuation,
    Tense,
    WordChoice,
    Preposition,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::SubjectVerb,
        Category::Fragment,
        Category::Punctuation,
        Category::Tense,
        Category::WordChoice,
        Category::Preposition,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::SubjectVerb => "Subject-verb agreement",
            Category::Fragment => "Sentence fragments",
            Category::Punctuation => "Hyphenation and punctuation",
            Category::Tense => "Tense errors",
            Category::WordChoice => "Incorrect word choices and typos",
            Category::Preposition => "Missing or incorrect prepositions",
        }
    }
}

/// Which categories are on. Everything is on by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnabledCategories {
    pub subject_verb: bool,
    pub fragment: bool,
    pub punctuation: bool,
    pub tense: bool,
    pub word_choice: bool,
    pub preposition: bool,
}

impl Default for EnabledCategories {
    fn default() -> Self {
        EnabledCategories {
            subject_verb: true,
            fragment: true,
            punctuation: true,
            tense: true,
            word_choice: true,
            preposition: true,
        }
    }
}

impl EnabledCategories {
    pub fn is_on(&self, c: Category) -> bool {
        match c {
            Category::SubjectVerb => self.subject_verb,
            Category::Fragment => self.fragment,
            Category::Punctuation => self.punctuation,
            Category::Tense => self.tense,
            Category::WordChoice => self.word_choice,
            Category::Preposition => self.preposition,
        }
    }
}

/// One suspected error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Byte range of the flagged text (highlighted yellow).
    pub range: Range<usize>,
    pub category: Category,
    /// Stable rule id, used with the flagged text to remember "Ignore".
    pub rule: &'static str,
    /// The box's "Issue:" line, e.g. "Possessive Pronoun".
    pub title: &'static str,
    /// The one-sentence explanation under it.
    pub explanation: String,
    /// What the flagged text should become, when the fix is unambiguous.
    pub replacement: Option<String>,
}

impl Issue {
    /// Identity for the ignore list: the rule plus the flagged text, so an ignored
    /// suggestion stays ignored while the text around it changes.
    pub fn ignore_key(&self, text: &str) -> String {
        format!(
            "{}|{}",
            self.rule,
            text.get(self.range.clone()).unwrap_or("")
        )
    }
}

fn with_case_of(original: &str, replacement: &str) -> String {
    match (original.chars().next(), replacement.chars().next()) {
        (Some(o), Some(r)) if o.is_uppercase() && r.is_lowercase() => {
            let mut s: String = r.to_uppercase().collect();
            s.push_str(replacement.get(r.len_utf8()..).unwrap_or(""));
            s
        }
        _ => replacement.to_string(),
    }
}

/// True when the clause ends right after `end`: end of text, or sentence punctuation or a
/// closing quote comes next.
fn ends_clause(text: &str, end: usize) -> bool {
    match text.get(end..).and_then(|r| r.chars().next()) {
        None => true,
        Some(c) => matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '"' | '\u{201D}'),
    }
}

/// Only whitespace (no line break) between two tokens.
fn adjacent(text: &str, a: &Token, b: &Token) -> bool {
    text.get(a.end..b.start).is_some_and(|g| {
        !g.is_empty()
            && g.chars()
                .all(|c| c.is_whitespace() && c != '\n' && c != '\r')
    })
}

const AUXILIARIES: &[&str] = &[
    "did", "does", "do", "should", "would", "could", "might", "may", "will", "can", "must",
    "shall", "whether",
];

const ITS_OWNED: &[&str] = &[
    "own", "owner", "name", "color", "colour", "size", "shape", "purpose", "tail", "wings",
    "surface", "edges", "contents", "meaning", "origin", "creator",
];
const ITS_IS: &[&str] = &[
    "a",
    "an",
    "been",
    "not",
    "going",
    "just",
    "very",
    "too",
    "also",
    "really",
    "always",
    "never",
    "important",
    "impossible",
    "necessary",
    "because",
    "okay",
];

/// Words that, right after "its" and at the end of a clause ("Its good."), can only be the
/// predicate of "it's". Nouns are left out on purpose: "its tail." is correct.
const ITS_PREDICATE: &[&str] = &[
    "good", "bad", "great", "fine", "true", "false", "clear", "easy", "hard", "ready", "late",
    "early", "over", "done", "cold", "hot", "warm", "nice", "cool", "sad", "fun", "funny", "wrong",
    "time",
];

/// Phrases that are wrong as written: (first word, second word, replacement, why).
const PHRASES: &[(&str, &str, &str, &str)] = &[(
    "by",
    "surprises",
    "by surprise",
    "The idiom is 'by surprise', in the singular.",
)];

/// Single words that should be something else: (word, replacement, why).
const WORD_FIXES: &[(&str, &str, &str)] = &[
    (
        "irregardless",
        "regardless",
        "'Irregardless' is nonstandard; use 'regardless'.",
    ),
    ("alot", "a lot", "'A lot' is two words."),
];

const A_AN_SOUNDS_CONSONANT: &[&str] = &[
    "one",
    "once",
    "ones",
    "university",
    "universal",
    "unique",
    "unit",
    "united",
    "union",
    "uniform",
    "user",
    "users",
    "useful",
    "european",
];
const A_AN_SILENT_H: &[&str] = &[
    "hour",
    "hours",
    "hourly",
    "honest",
    "honor",
    "honour",
    "heir",
    "heirs",
    "honorable",
];

const PARTICIPLES: &[(&str, &str)] = &[
    ("went", "gone"),
    ("did", "done"),
    ("ate", "eaten"),
    ("wrote", "written"),
    ("took", "taken"),
    ("began", "begun"),
    ("gave", "given"),
    ("came", "come"),
    ("drank", "drunk"),
    ("ran", "run"),
    ("spoke", "spoken"),
    ("chose", "chosen"),
    ("drove", "driven"),
    ("flew", "flown"),
    ("forgot", "forgotten"),
    ("knew", "known"),
    ("sang", "sung"),
    ("swam", "swum"),
    ("threw", "thrown"),
    ("wore", "worn"),
    ("broke", "broken"),
];

const PREPOSITION_FIXES: &[(&str, &str, &str)] = &[
    ("discuss", "about", "discuss"),
    ("discusses", "about", "discusses"),
    ("discussed", "about", "discussed"),
    ("discussing", "about", "discussing"),
    ("depend", "of", "depend on"),
    ("depends", "of", "depends on"),
    ("depended", "of", "depended on"),
    ("depending", "of", "depending on"),
    ("interested", "on", "interested in"),
    ("married", "with", "married to"),
    ("capable", "to", "capable of"),
    ("afraid", "from", "afraid of"),
    ("consists", "in", "consists of"),
    ("consisted", "in", "consisted of"),
    ("consist", "in", "consist of"),
];

fn push(
    out: &mut Vec<Issue>,
    range: Range<usize>,
    category: Category,
    rule: &'static str,
    title: &'static str,
    explanation: String,
    replacement: Option<String>,
) {
    out.push(Issue {
        range,
        category,
        rule,
        title,
        explanation,
        replacement,
    });
}

/// Check `text` and return the issues of the enabled categories in document order.
///
/// Sentence fragments are not detected yet: they need a part-of-speech tagger, and a
/// pattern-only guess would flag valid sentences. The category exists so its switch and its
/// place in the box are ready.
pub fn check_grammar(text: &str, on: &EnabledCategories) -> Vec<Issue> {
    let toks = tokens(text);
    let lower: Vec<String> = toks.iter().map(|t| t.text(text).to_lowercase()).collect();
    let mut out = Vec::new();

    for i in 0..toks.len() {
        let (Some(a), Some(la)) = (toks.get(i), lower.get(i)) else {
            continue;
        };
        let a_text = a.text(text);
        let prev = i
            .checked_sub(1)
            .and_then(|p| lower.get(p))
            .map(String::as_str);
        let after_aux = prev.is_some_and(|p| AUXILIARIES.contains(&p));
        let next = match (toks.get(i + 1), lower.get(i + 1)) {
            (Some(b), Some(lb)) if adjacent(text, a, b) => Some((b, lb.as_str())),
            _ => None,
        };

        // its / it's, doubled words, a / an, "of" for "have": word choice and typos.
        if on.word_choice {
            let is_its_contraction = la == "it's" || la == "it\u{2019}s";
            if let Some((_, nb)) = next {
                if is_its_contraction && ITS_OWNED.contains(&nb) {
                    if let Some(b) = toks.get(i + 1) {
                        let rep = format!("{} {}", with_case_of(a_text, "its"), b.text(text));
                        push(&mut out, a.start..b.end, Category::WordChoice, "its-possessive", "Possessive Pronoun",
                            "Incorrect use of contraction it's (it is) instead of the possessive pronoun 'its'.".to_string(), Some(rep));
                    }
                } else if la == "its"
                    && toks.get(i + 1).is_some_and(|b| {
                        ITS_IS.contains(&nb)
                            || (ITS_PREDICATE.contains(&nb) && ends_clause(text, b.end))
                    })
                {
                    if let Some(b) = toks.get(i + 1) {
                        let rep = format!("{} {}", with_case_of(a_text, "it's"), b.text(text));
                        push(&mut out, a.start..b.end, Category::WordChoice, "its-contraction", "Contraction",
                            "The possessive 'its' is used where the contraction it's (it is or it has) is meant.".to_string(), Some(rep));
                    }
                }
                if let Some(&(_, _, fixed, why)) = PHRASES
                    .iter()
                    .find(|(x, y, _, _)| *x == la.as_str() && *y == nb)
                {
                    if let Some(b) = toks.get(i + 1) {
                        push(
                            &mut out,
                            a.start..b.end,
                            Category::WordChoice,
                            "phrase",
                            "Word Choice",
                            why.to_string(),
                            Some(with_case_of(a_text, fixed)),
                        );
                    }
                }
                if la == nb && la.chars().count() > 1 && !matches!(la.as_str(), "had" | "that") {
                    if let Some(b) = toks.get(i + 1) {
                        push(
                            &mut out,
                            a.start..b.end,
                            Category::WordChoice,
                            "doubled-word",
                            "Repeated Word",
                            format!("The word '{a_text}' appears twice in a row."),
                            Some(a_text.to_string()),
                        );
                    }
                }
                if la == "a" || la == "an" {
                    let b_text = toks.get(i + 1).map_or("", |b| b.text(text));
                    let acronym =
                        b_text.chars().count() > 1 && b_text.chars().all(char::is_uppercase);
                    let starts_vowel = nb
                        .chars()
                        .next()
                        .is_some_and(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u'));
                    let head = nb.split('-').next().unwrap_or(nb);
                    let silent_h = A_AN_SILENT_H.contains(&head);
                    let consonant_sound = A_AN_SOUNDS_CONSONANT.contains(&head);
                    if !acronym {
                        if la == "a" && ((starts_vowel && !consonant_sound) || silent_h) {
                            if let Some(b) = toks.get(i + 1) {
                                push(
                                    &mut out,
                                    a.start..b.end,
                                    Category::WordChoice,
                                    "a-an",
                                    "Article",
                                    format!(
                                        "Use 'an' before '{b_text}', which starts with a vowel sound."
                                    ),
                                    Some(format!("{} {b_text}", with_case_of(a_text, "an"))),
                                );
                            }
                        } else if la == "an" && ((!starts_vowel && !silent_h) || consonant_sound) {
                            if let Some(b) = toks.get(i + 1) {
                                push(
                                    &mut out,
                                    a.start..b.end,
                                    Category::WordChoice,
                                    "a-an",
                                    "Article",
                                    format!(
                                        "Use 'a' before '{b_text}', which starts with a consonant sound."
                                    ),
                                    Some(format!("{} {b_text}", with_case_of(a_text, "a"))),
                                );
                            }
                        }
                    }
                }
                if matches!(la.as_str(), "should" | "would" | "could" | "must" | "might")
                    && nb == "of"
                {
                    if let Some(b) = toks.get(i + 1) {
                        push(
                            &mut out,
                            a.start..b.end,
                            Category::Tense,
                            "modal-of",
                            "Verb Form",
                            format!("'{a_text} of' is a common mishearing of '{a_text} have'."),
                            Some(format!("{a_text} have")),
                        );
                    }
                }
            }
        }

        if on.word_choice {
            if let Some(&(_, fixed, why)) = WORD_FIXES.iter().find(|(w, _, _)| *w == la.as_str()) {
                push(
                    &mut out,
                    a.start..a.end,
                    Category::WordChoice,
                    "word-fix",
                    "Word Choice",
                    why.to_string(),
                    Some(with_case_of(a_text, fixed)),
                );
            }
        }

        // Subject-verb agreement.
        // "John and I are": a compound subject, so the last word is not the whole subject.
        let compound = matches!(prev, Some("and" | "or" | "nor"));
        if on.subject_verb && !after_aux && !compound {
            if let Some((b, nb)) = next {
                let subject = la.as_str();
                let fix: Option<(&str, &str)> = if a_text == "I" {
                    match nb {
                        "is" | "are" => Some(("am", "'I' takes 'am'.")),
                        "has" => Some(("have", "'I' takes 'have'.")),
                        "doesn't" => Some(("don't", "'I' takes 'don't'.")),
                        _ => None,
                    }
                } else if matches!(subject, "he" | "she" | "it") {
                    match nb {
                        "don't" => Some(("doesn't", "A singular subject takes 'doesn't'.")),
                        "have" => Some(("has", "A singular subject takes 'has'.")),
                        "are" => Some(("is", "A singular subject takes 'is'.")),
                        "do" => Some(("does", "A singular subject takes 'does'.")),
                        _ => None,
                    }
                } else if matches!(subject, "they" | "we" | "you") {
                    match nb {
                        "is" => Some(("are", "A plural subject takes 'are'.")),
                        "was" => Some(("were", "A plural subject takes 'were'.")),
                        "has" => Some(("have", "A plural subject takes 'have'.")),
                        "doesn't" => Some(("don't", "A plural subject takes 'don't'.")),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some((verb, why)) = fix {
                    push(
                        &mut out,
                        a.start..b.end,
                        Category::SubjectVerb,
                        "subject-verb",
                        "Subject-Verb Agreement",
                        format!(
                            "The verb '{}' does not agree with '{a_text}'. {why}",
                            b.text(text)
                        ),
                        Some(format!("{a_text} {verb}")),
                    );
                }
            }
        }

        // Verb forms after have / has / had.
        if on.tense && matches!(la.as_str(), "have" | "has" | "had") {
            if let Some((b, nb)) = next {
                if let Some(&(_, participle)) = PARTICIPLES.iter().find(|(past, _)| *past == nb) {
                    push(
                        &mut out,
                        a.start..b.end,
                        Category::Tense,
                        "participle",
                        "Verb Tense",
                        format!(
                            "After '{a_text}' use the past participle '{participle}', not '{nb}'."
                        ),
                        Some(format!("{a_text} {participle}")),
                    );
                }
            }
        }

        // Prepositions that do not go with the verb.
        if on.preposition {
            if let Some((b, nb)) = next {
                if let Some(&(_, _, fixed)) = PREPOSITION_FIXES
                    .iter()
                    .find(|(v, p, _)| *v == la.as_str() && *p == nb)
                {
                    push(
                        &mut out,
                        a.start..b.end,
                        Category::Preposition,
                        "preposition",
                        "Preposition",
                        format!("'{}' does not take '{nb}' here.", la),
                        Some(with_case_of(a_text, fixed)),
                    );
                }
            }
        }
    }

    extra_rules(text, &toks, on, &mut out);
    if on.punctuation {
        punctuation_issues(text, &mut out);
    }
    out.sort_by_key(|i| (i.range.start, i.range.end));
    // Two rules can flag the same words; keep the first and drop anything overlapping it.
    let mut kept: Vec<Issue> = Vec::with_capacity(out.len());
    for issue in out {
        if kept.last().is_some_and(|k| issue.range.start < k.range.end) {
            continue;
        }
        kept.push(issue);
    }
    kept
}

include!("rules_extra.rs");

fn punctuation_issues(text: &str, out: &mut Vec<Issue>) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for i in 0..chars.len() {
        let Some(&(at, c)) = chars.get(i) else {
            continue;
        };
        let before = i.checked_sub(1).and_then(|p| chars.get(p)).map(|&(_, x)| x);
        let after = chars.get(i + 1).map(|&(_, x)| x);
        match c {
            ' ' if after.is_some_and(|x| matches!(x, ',' | ';' | ':' | '!' | '?'))
                && before.is_some_and(char::is_alphabetic) =>
            {
                let Some(&(_, mark)) = chars.get(i + 1) else {
                    continue;
                };
                let stop = chars.get(i + 2).map_or(text.len(), |&(b, _)| b);
                push(
                    out,
                    at..stop,
                    Category::Punctuation,
                    "space-before-mark",
                    "Spacing",
                    format!("Remove the space before '{mark}'."),
                    Some(mark.to_string()),
                );
            }
            ',' if after == Some(',') => {
                let stop = chars.get(i + 2).map_or(text.len(), |&(b, _)| b);
                push(
                    out,
                    at..stop,
                    Category::Punctuation,
                    "double-comma",
                    "Punctuation",
                    "Two commas in a row.".to_string(),
                    Some(",".to_string()),
                );
            }
            ',' if before.is_some_and(char::is_alphabetic)
                && after.is_some_and(char::is_alphabetic) =>
            {
                let Some(&(start, b)) = i.checked_sub(1).and_then(|p| chars.get(p)) else {
                    continue;
                };
                let Some(&(_, a)) = chars.get(i + 1) else {
                    continue;
                };
                let stop = chars.get(i + 2).map_or(text.len(), |&(x, _)| x);
                push(
                    out,
                    start..stop,
                    Category::Punctuation,
                    "comma-space",
                    "Spacing",
                    "Add a space after the comma.".to_string(),
                    Some(format!("{b}, {a}")),
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> EnabledCategories {
        EnabledCategories::default()
    }

    fn fixes(text: &str) -> Vec<(String, Option<String>)> {
        check_grammar(text, &all())
            .into_iter()
            .map(|i| {
                (
                    text.get(i.range.clone()).unwrap_or("").to_string(),
                    i.replacement,
                )
            })
            .collect()
    }

    #[test]
    fn splits_sentences_and_keeps_abbreviations() {
        let t = "Dr. Smith arrived. He sat down!  Then... he left. J. R. R. Tolkien wrote it.\nNew line";
        let s: Vec<&str> = sentences(t).into_iter().filter_map(|r| t.get(r)).collect();
        assert_eq!(
            s,
            vec![
                "Dr. Smith arrived.",
                "He sat down!",
                "Then... he left.",
                "J. R. R. Tolkien wrote it.",
                "New line"
            ]
        );
        assert!(sentences("").is_empty());
        assert!(sentences("...").is_empty());
        assert_eq!(sentences("It costs 3.14 dollars.").len(), 1);
    }

    #[test]
    fn counts_like_word() {
        let c = counts("Hello world. 42 apples\nend");
        assert_eq!(c.words, 5);
        assert_eq!(
            c.characters_with_spaces,
            "Hello world. 42 applesend".chars().count()
        );
        assert_eq!(
            c.characters_no_spaces,
            "Helloworld.42applesend".chars().count()
        );
        assert_eq!(c.sentences, 3);
        assert_eq!(counts("").words, 0);
    }

    #[test]
    fn syllable_estimates() {
        for (w, n) in [
            ("cat", 1),
            ("water", 2),
            ("table", 2),
            ("jumped", 1),
            ("wanted", 2),
            ("beautiful", 3),
            ("the", 1),
            ("rhythm", 1),
        ] {
            assert_eq!(syllables(w), n, "{w}");
        }
        assert_eq!(syllables(""), 0);
    }

    #[test]
    fn reading_level_orders_simple_below_dense() {
        let easy = reading_grade("The cat sat on the mat. The dog ran to the log.").unwrap_or(99.0);
        let hard = reading_grade(
            "Notwithstanding considerable epistemological disagreement, the committee's comprehensive deliberations ultimately produced unanimous institutional recommendations.",
        )
        .unwrap_or(0.0);
        assert!(easy < 3.0, "{easy}");
        assert!(hard > 16.0, "{hard}");
        assert_eq!(reading_grade(""), None);
        assert_eq!(reading_grade("1234 5678."), None);
        assert_eq!(grade_label(easy), "5th Grade or below");
        assert_eq!(grade_label(11.6), "11-12th Grade");
        assert_eq!(grade_label(30.0), "Graduate");
    }

    #[test]
    fn keyword_density_counts_and_skips_stop_words() {
        let t =
            "The Priestess spoke. The priestess's staff glowed. A wizard laughed at the Priestess.";
        let k = keyword_density(t, true, 10);
        assert_eq!(
            k.first().map(|k| (k.word.as_str(), k.count)),
            Some(("Priestess", 3))
        );
        assert!(k.iter().all(|k| k.word.to_lowercase() != "the"));
        assert_eq!(
            k.first().map(|k| k.percent_label()),
            Some("38%".to_string())
        );
        let with_stop = keyword_density(t, false, 10);
        assert!(
            with_stop
                .iter()
                .any(|k| k.word.to_lowercase() == "the" && k.count == 3)
        );
        assert_eq!(keyword_density(t, true, 2).len(), 2);
        assert!(keyword_density("", true, 5).is_empty());
        let small = Keyword {
            word: "x".into(),
            count: 1,
            percent: 0.36,
        };
        assert_eq!(small.percent_label(), "0.4%");
    }

    #[test]
    fn keyword_spans_find_every_form() {
        let t = "The Priestess spoke. The priestess's staff glowed; priestesses sang.";
        let keys: HashSet<String> = ["priestess".to_string()].into_iter().collect();
        let found: Vec<&str> = keyword_spans(t, &keys)
            .into_iter()
            .filter_map(|r| t.get(r))
            .collect();
        assert_eq!(found, vec!["Priestess", "priestess's"]);
        assert!(keyword_spans(t, &HashSet::new()).is_empty());
        assert_eq!(keyword_key("Priestess\u{2019}s"), "priestess");
    }

    #[test]
    fn sentence_length_buckets() {
        assert_eq!(LengthBucket::of(0), None);
        for (n, b) in [
            (1, LengthBucket::One),
            (2, LengthBucket::TwoToSix),
            (6, LengthBucket::TwoToSix),
            (7, LengthBucket::SevenToFifteen),
            (15, LengthBucket::SevenToFifteen),
            (16, LengthBucket::SixteenToTwentyFive),
            (25, LengthBucket::SixteenToTwentyFive),
            (26, LengthBucket::TwentySixToThirtyNine),
            (39, LengthBucket::TwentySixToThirtyNine),
            (40, LengthBucket::FortyPlus),
            (400, LengthBucket::FortyPlus),
        ] {
            assert_eq!(LengthBucket::of(n), Some(b), "{n}");
        }
        let t = "Stop. This sentence has exactly seven words in it.";
        let l = sentence_lengths(t);
        assert_eq!(
            l.iter().map(|s| (s.words, s.bucket)).collect::<Vec<_>>(),
            vec![(1, LengthBucket::One), (8, LengthBucket::SevenToFifteen)]
        );
    }

    #[test]
    fn its_and_its() {
        assert_eq!(
            fixes("The dog chased it's tail."),
            vec![("it's tail".to_string(), Some("its tail".to_string()))]
        );
        assert_eq!(
            fixes("Its a good day."),
            vec![("Its a".to_string(), Some("It's a".to_string()))]
        );
        assert!(fixes("It's a good day and its tail wagged.").is_empty());
        let i = check_grammar("it's own", &all());
        assert_eq!(i.first().map(|i| i.title), Some("Possessive Pronoun"));
        assert_eq!(
            i.first().map(|i| i.explanation.as_str()),
            Some(
                "Incorrect use of contraction it's (it is) instead of the possessive pronoun 'its'."
            )
        );
    }

    #[test]
    fn its_before_a_predicate_and_fixed_phrases() {
        assert_eq!(
            fixes("Its good. Really."),
            vec![("Its good".to_string(), Some("It's good".to_string()))]
        );
        assert_eq!(
            fixes("I think its great!"),
            vec![("its great".to_string(), Some("it's great".to_string()))]
        );
        assert!(fixes("Its good name was kept. The dog wagged its tail.").is_empty());
        assert_eq!(
            fixes("It caught the market by surprises!"),
            vec![("by surprises".to_string(), Some("by surprise".to_string()))]
        );
        assert_eq!(
            fixes("Irregardless, we left."),
            vec![("Irregardless".to_string(), Some("Regardless".to_string()))]
        );
        let t = "Its good. Jensen Huang\u{2019}s wang caught the US Stock Market by surprises!";
        assert_eq!(check_grammar(t, &all()).len(), 2);
    }

    #[test]
    fn doubled_words_and_articles() {
        assert_eq!(
            fixes("This is is odd."),
            vec![("is is".to_string(), Some("is".to_string()))]
        );
        assert!(fixes("She had had enough.").is_empty());
        assert_eq!(
            fixes("I ate a apple."),
            vec![("a apple".to_string(), Some("an apple".to_string()))]
        );
        assert_eq!(
            fixes("An dog barked."),
            vec![("An dog".to_string(), Some("A dog".to_string()))]
        );
        assert!(fixes("A university, an hour, a one-off, an FBI agent, a UFO.").is_empty());
    }

    #[test]
    fn agreement() {
        assert_eq!(
            fixes("He don't know."),
            vec![("He don't".to_string(), Some("He doesn't".to_string()))]
        );
        assert_eq!(
            fixes("They was late."),
            vec![("They was".to_string(), Some("They were".to_string()))]
        );
        assert_eq!(
            fixes("I is ready."),
            vec![("I is".to_string(), Some("I am".to_string()))]
        );
        assert!(fixes("Did he have the key? Should she do it?").is_empty());
        assert!(fixes("They are late and he has a key.").is_empty());
    }

    #[test]
    fn tense_and_prepositions() {
        assert_eq!(
            fixes("She has went home."),
            vec![("has went".to_string(), Some("has gone".to_string()))]
        );
        assert_eq!(
            fixes("You should of called."),
            vec![("should of".to_string(), Some("should have".to_string()))]
        );
        assert!(fixes("She has gone home and went out.").is_empty());
        assert_eq!(
            fixes("We discussed about it."),
            vec![("discussed about".to_string(), Some("discussed".to_string()))]
        );
        assert_eq!(
            fixes("It depends of the weather."),
            vec![("depends of".to_string(), Some("depends on".to_string()))]
        );
    }

    #[test]
    fn punctuation() {
        assert_eq!(
            fixes("Hello , world."),
            vec![(" ,".to_string(), Some(",".to_string()))]
        );
        assert_eq!(
            fixes("Red,blue."),
            vec![("d,b".to_string(), Some("d, b".to_string()))]
        );
        assert_eq!(
            fixes("Wait,, what?"),
            vec![(",,".to_string(), Some(",".to_string()))]
        );
        assert!(fixes("It costs 1,000 dollars, e.g. a lot.").is_empty());
    }

    #[test]
    fn categories_can_be_switched_off() {
        let mut on = all();
        on.subject_verb = false;
        assert!(check_grammar("He don't know.", &on).is_empty());
        on.subject_verb = true;
        on.punctuation = false;
        assert!(check_grammar("Hello , world.", &on).is_empty());
    }

    #[test]
    fn ignore_key_survives_edits_elsewhere() {
        let a = "He don't know.";
        let b = "Well, he said. He don't know.";
        let ka = check_grammar(a, &all()).first().map(|i| i.ignore_key(a));
        let kb = check_grammar(b, &all()).first().map(|i| i.ignore_key(b));
        assert!(ka.is_some());
        assert_eq!(ka, kb);
    }

    #[test]
    fn hostile_input_never_panics() {
        for t in [
            "",
            " ",
            "'",
            "\u{2019}",
            "-",
            "a-",
            "-a",
            "é'é",
            "I",
            "it's",
            "a",
            "an",
            ", ,",
            ",,",
            "\u{0}",
            "😀 😀",
            "x\u{2019}",
            "....",
            "\n\n",
            "a,b,c",
        ] {
            let _ = check_grammar(t, &all());
            let _ = sentences(t);
            let _ = counts(t);
            let _ = reading_grade(t);
            let _ = keyword_density(t, true, 3);
            let _ = sentence_lengths(t);
        }
        let big = "word, ".repeat(50_000);
        assert!(check_grammar(&big, &all()).is_empty());
    }
}
