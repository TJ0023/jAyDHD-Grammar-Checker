//! Word look-up for the Thesaurus box, from the Princeton WordNet 3.1 database files in
//! `wordnet/` (compiled into the program). Pure std: a word goes in, senses come out, each with
//! a meaning, an example, and up to three synonyms and three antonyms with how they are used.
//!
//! WordNet files are plain text. The index files list, for each word, the byte offsets of its
//! synsets (groups of words with one meaning) in the data files, so a look-up is a hash probe
//! plus a few reads at known offsets; nothing is parsed up front except the word lists.

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pos {
    Noun,
    Verb,
    Adj,
    Adv,
}

impl Pos {
    const ALL: [Pos; 4] = [Pos::Noun, Pos::Verb, Pos::Adj, Pos::Adv];

    pub fn label(self) -> &'static str {
        match self {
            Pos::Noun => "noun",
            Pos::Verb => "verb",
            Pos::Adj => "adjective",
            Pos::Adv => "adverb",
        }
    }

    fn slot(self) -> usize {
        match self {
            Pos::Noun => 0,
            Pos::Verb => 1,
            Pos::Adj => 2,
            Pos::Adv => 3,
        }
    }

    fn from_char(c: char) -> Option<Pos> {
        match c {
            'n' => Some(Pos::Noun),
            'v' => Some(Pos::Verb),
            'a' | 's' => Some(Pos::Adj),
            'r' => Some(Pos::Adv),
            _ => None,
        }
    }

    fn data(self) -> &'static str {
        match self {
            Pos::Noun => include_str!("../wordnet/data.noun"),
            Pos::Verb => include_str!("../wordnet/data.verb"),
            Pos::Adj => include_str!("../wordnet/data.adj"),
            Pos::Adv => include_str!("../wordnet/data.adv"),
        }
    }

    fn index(self) -> &'static str {
        match self {
            Pos::Noun => include_str!("../wordnet/index.noun"),
            Pos::Verb => include_str!("../wordnet/index.verb"),
            Pos::Adj => include_str!("../wordnet/index.adj"),
            Pos::Adv => include_str!("../wordnet/index.adv"),
        }
    }

    fn exceptions(self) -> &'static str {
        match self {
            Pos::Noun => include_str!("../wordnet/noun.exc"),
            Pos::Verb => include_str!("../wordnet/verb.exc"),
            Pos::Adj => include_str!("../wordnet/adj.exc"),
            Pos::Adv => include_str!("../wordnet/adv.exc"),
        }
    }

    /// WordNet's rules for undoing endings (from its `morphy`): (ending, replacement).
    fn rules(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Pos::Noun => &[
                ("s", ""),
                ("ses", "s"),
                ("xes", "x"),
                ("zes", "z"),
                ("ches", "ch"),
                ("shes", "sh"),
                ("men", "man"),
                ("ies", "y"),
            ],
            Pos::Verb => &[
                ("s", ""),
                ("ies", "y"),
                ("es", "e"),
                ("es", ""),
                ("ed", "e"),
                ("ed", ""),
                ("ing", "e"),
                ("ing", ""),
            ],
            Pos::Adj => &[("er", ""), ("est", ""), ("er", "e"), ("est", "e")],
            Pos::Adv => &[],
        }
    }
}

/// The word lists, built once on first use.
struct Db {
    /// Per part of speech: word (lowercase, underscores for spaces) to its index line.
    index: [HashMap<&'static str, &'static str>; 4],
    /// Per part of speech: an irregular form to its base forms ("mice" to "mouse").
    exc: [HashMap<&'static str, Vec<&'static str>>; 4],
}

fn db() -> &'static Db {
    static DB: OnceLock<Db> = OnceLock::new();
    DB.get_or_init(|| {
        let mut index: [HashMap<&'static str, &'static str>; 4] = Default::default();
        let mut exc: [HashMap<&'static str, Vec<&'static str>>; 4] = Default::default();
        for pos in Pos::ALL {
            let slot = pos.slot();
            for line in pos.index().lines() {
                // The license text at the top starts with spaces.
                if line.starts_with(' ') {
                    continue;
                }
                if let Some(word) = line.split(' ').next() {
                    index[slot].insert(word, line);
                }
            }
            for line in pos.exceptions().lines() {
                let mut parts = line.split_whitespace();
                if let Some(form) = parts.next() {
                    exc[slot].insert(form, parts.collect());
                }
            }
        }
        Db { index, exc }
    })
}

/// Load the word lists now (call from a background thread so the first look-up is instant).
pub fn warm_up() {
    let _ = db();
}

/// What an index line holds for a word: the offsets of its synsets, and how often it is tagged
/// in WordNet's sample texts (a measure of how common this part of speech is for the word).
struct IndexEntry {
    offsets: Vec<usize>,
    tagged: usize,
}

fn index_entry(pos: Pos, word: &str) -> Option<IndexEntry> {
    let line = db().index[pos.slot()].get(word)?;
    let f: Vec<&str> = line.split_whitespace().collect();
    let synsets: usize = f.get(2)?.parse().ok()?;
    let pointers: usize = f.get(3)?.parse().ok()?;
    let tagged: usize = f.get(5 + pointers)?.parse().ok()?;
    let first = 6 + pointers;
    let offsets = f
        .get(first..first + synsets)?
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    Some(IndexEntry { offsets, tagged })
}

/// The base forms a word could have, per part of speech, most direct first.
fn base_forms(pos: Pos, word: &str) -> Vec<String> {
    let db = db();
    let slot = pos.slot();
    let mut out: Vec<String> = Vec::new();
    let push = |w: String, out: &mut Vec<String>| {
        if db.index[slot].contains_key(w.as_str()) && !out.contains(&w) {
            out.push(w);
        }
    };
    push(word.to_string(), &mut out);
    if let Some(bases) = db.exc[slot].get(word) {
        for b in bases {
            push((*b).to_string(), &mut out);
        }
    }
    for (ending, replacement) in pos.rules() {
        if let Some(stem) = word.strip_suffix(ending)
            && !stem.is_empty()
        {
            push(format!("{stem}{replacement}"), &mut out);
        }
    }
    out
}

/// One word linked to a sense, with a sentence that uses it when WordNet has one.
#[derive(Clone, Debug, PartialEq)]
pub struct Related {
    pub word: String,
    pub example: Option<String>,
}

/// One meaning of a word.
#[derive(Clone, Debug, PartialEq)]
pub struct Sense {
    /// The word itself, in its base form ("run" for "running").
    pub lemma: String,
    pub pos: Pos,
    pub meaning: String,
    pub example: Option<String>,
    pub synonyms: Vec<Related>,
    pub antonyms: Vec<Related>,
}

struct Pointer {
    symbol: String,
    offset: usize,
    pos: Pos,
    /// Which word of this synset the link starts from (1-based), 0 for the whole synset.
    from: usize,
    /// Which word of the target synset it points to (1-based), 0 for the whole synset.
    to: usize,
}

struct Synset {
    pos: Pos,
    words: Vec<String>,
    pointers: Vec<Pointer>,
    meaning: String,
    examples: Vec<String>,
}

fn synset(pos: Pos, offset: usize) -> Option<Synset> {
    let line = pos.data().get(offset..)?.split('\n').next()?;
    if !line.starts_with(&format!("{offset:08}")) {
        return None;
    }
    let (head, gloss) = line.split_once(" | ").unwrap_or((line, ""));
    let f: Vec<&str> = head.split_whitespace().collect();
    let ss_type = f.get(2)?.chars().next()?;
    let pos = Pos::from_char(ss_type).unwrap_or(pos);
    let word_count = usize::from_str_radix(f.get(3)?, 16).ok()?;
    let mut words = Vec::new();
    for i in 0..word_count {
        let w = f.get(4 + 2 * i)?;
        // Adjectives may carry a marker such as "(a)" or "(ip)".
        let w = w.split('(').next().unwrap_or(w);
        words.push(w.replace('_', " "));
    }
    let mut at = 4 + 2 * word_count;
    let pointer_count: usize = f.get(at)?.parse().ok()?;
    at += 1;
    let mut pointers = Vec::new();
    for i in 0..pointer_count {
        let base = at + 4 * i;
        let (Some(symbol), Some(off), Some(p), Some(st)) =
            (f.get(base), f.get(base + 1), f.get(base + 2), f.get(base + 3))
        else {
            break;
        };
        let (Ok(offset), Some(pos), true) = (
            off.parse::<usize>(),
            p.chars().next().and_then(Pos::from_char),
            st.len() == 4,
        ) else {
            continue;
        };
        pointers.push(Pointer {
            symbol: (*symbol).to_string(),
            offset,
            pos,
            from: usize::from_str_radix(&st[..2], 16).unwrap_or(0),
            to: usize::from_str_radix(&st[2..], 16).unwrap_or(0),
        });
    }
    let (meaning, examples) = split_gloss(gloss);
    Some(Synset {
        pos,
        words,
        pointers,
        meaning,
        examples,
    })
}

/// Split a WordNet gloss into its meaning and its quoted example sentences:
/// `a small mammal; "the cat sat"; "a cat purred"`.
fn split_gloss(gloss: &str) -> (String, Vec<String>) {
    let gloss = gloss.trim();
    let cut = if gloss.starts_with('"') {
        Some(0)
    } else {
        gloss.find("; \"")
    };
    let (meaning, rest) = match cut {
        Some(i) => (gloss[..i].trim(), &gloss[i..]),
        None => (gloss, ""),
    };
    let examples = rest
        .split('"')
        .enumerate()
        .filter(|(i, s)| i % 2 == 1 && !s.trim().is_empty())
        .map(|(_, s)| s.trim().to_string())
        .collect();
    (meaning.trim_end_matches(';').trim().to_string(), examples)
}

fn contains_word(sentence: &str, word: &str) -> bool {
    let (s, w) = (sentence.to_lowercase(), word.to_lowercase());
    s.match_indices(&w).any(|(i, _)| {
        let before = s[..i].chars().next_back();
        let after = s[i + w.len()..].chars().next();
        !before.is_some_and(char::is_alphabetic) && !after.is_some_and(char::is_alphabetic)
    })
}

/// A sentence that uses `word`: one of this synset's examples, else one from the word's own
/// first few senses.
fn example_for(word: &str, within: &Synset) -> Option<String> {
    if let Some(e) = within.examples.iter().find(|e| contains_word(e, word)) {
        return Some(e.clone());
    }
    let key = word.to_lowercase().replace(' ', "_");
    let entry = index_entry(within.pos, &key)?;
    for offset in entry.offsets.into_iter().take(6) {
        if let Some(s) = synset(within.pos, offset)
            && let Some(e) = s.examples.iter().find(|e| contains_word(e, word))
        {
            return Some(e.clone());
        }
    }
    None
}

/// Words that mean the opposite of word number `from` (1-based) of `syn`.
fn antonym_words(syn: &Synset, from: usize) -> Vec<(String, Synset)> {
    let mut out = Vec::new();
    for p in syn.pointers.iter().filter(|p| p.symbol == "!") {
        if p.from != from && from != 0 {
            continue;
        }
        if let Some(target) = synset(p.pos, p.offset)
            && let Some(word) = target.words.get(p.to.saturating_sub(1)).cloned()
        {
            out.push((word, target));
        }
    }
    out
}

fn build_sense(lemma: &str, pos: Pos, offset: usize) -> Option<Sense> {
    let syn = synset(pos, offset)?;
    let shown = lemma.replace('_', " ");
    let position = syn
        .words
        .iter()
        .position(|w| w.eq_ignore_ascii_case(&shown))
        .map_or(0, |i| i + 1);

    // Synonyms: the other words of the synset; adjectives borrow from "similar to" synsets.
    let mut synonym_words: Vec<(String, Option<String>)> = syn
        .words
        .iter()
        .filter(|w| !w.eq_ignore_ascii_case(&shown))
        .map(|w| (w.clone(), example_for(w, &syn)))
        .collect();
    if synonym_words.len() < 3 && syn.pos == Pos::Adj {
        for p in syn.pointers.iter().filter(|p| p.symbol == "&") {
            if let Some(similar) = synset(p.pos, p.offset) {
                for w in &similar.words {
                    if !w.eq_ignore_ascii_case(&shown)
                        && !synonym_words.iter().any(|(s, _)| s.eq_ignore_ascii_case(w))
                    {
                        synonym_words.push((w.clone(), example_for(w, &similar)));
                    }
                }
            }
        }
    }
    let synonyms = synonym_words
        .into_iter()
        .take(3)
        .map(|(word, example)| Related { word, example })
        .collect();

    // Antonyms: direct ones, or for a satellite adjective those of the main adjective it hangs on.
    let mut found = antonym_words(&syn, position);
    if found.is_empty() && syn.pos == Pos::Adj {
        for p in syn.pointers.iter().filter(|p| p.symbol == "&") {
            if let Some(head) = synset(p.pos, p.offset) {
                found.extend(antonym_words(&head, 0));
            }
        }
    }
    let mut antonyms: Vec<Related> = Vec::new();
    for (word, target) in found {
        if antonyms.iter().any(|a| a.word.eq_ignore_ascii_case(&word)) {
            continue;
        }
        let example = example_for(&word, &target);
        antonyms.push(Related { word, example });
        if antonyms.len() == 3 {
            break;
        }
    }

    let example = syn
        .examples
        .iter()
        .find(|e| contains_word(e, &shown))
        .or_else(|| syn.examples.first())
        .cloned();
    Some(Sense {
        lemma: shown,
        pos: syn.pos,
        meaning: syn.meaning,
        example,
        synonyms,
        antonyms,
    })
}

/// Tidy what was selected into a look-up key: no surrounding punctuation, lowercase, spaces
/// as underscores.
fn normalise(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches(|c: char| !c.is_alphanumeric());
    trimmed
        .to_lowercase()
        .replace('\u{2019}', "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}

const MAX_SENSES: usize = 20;

/// Every sense of the word (or phrase) in WordNet, the most common first. Empty when it is not
/// found. A selection of several words that is not a known phrase looks up its first word.
pub fn lookup(raw: &str) -> Vec<Sense> {
    let key = normalise(raw);
    if key.is_empty() || key.chars().count() > 40 {
        return Vec::new();
    }
    let mut senses = lookup_key(&key);
    if senses.is_empty()
        && let Some((first, _)) = key.split_once('_')
    {
        senses = lookup_key(first);
    }
    senses
}

fn lookup_key(key: &str) -> Vec<Sense> {
    // The parts of speech the word exists in, the one used most often first.
    let mut groups: Vec<(Pos, String, IndexEntry)> = Vec::new();
    for pos in Pos::ALL {
        for base in base_forms(pos, key) {
            if let Some(entry) = index_entry(pos, &base) {
                groups.push((pos, base, entry));
            }
        }
    }
    groups.sort_by_key(|(_, _, e)| std::cmp::Reverse(e.tagged));
    let mut out = Vec::new();
    for (pos, base, entry) in groups {
        for offset in entry.offsets {
            if let Some(sense) = build_sense(&base, pos, offset) {
                out.push(sense);
            }
            if out.len() >= MAX_SENSES {
                return out;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_plain_word() {
        let senses = lookup("dog");
        let first = senses.first().expect("dog is in WordNet");
        assert_eq!(first.lemma, "dog");
        assert_eq!(first.pos, Pos::Noun);
        assert!(first.meaning.contains("Canis"), "{}", first.meaning);
        assert!(first.synonyms.iter().any(|s| s.word == "domestic dog"));
    }

    #[test]
    fn undoes_endings_and_irregular_forms() {
        assert_eq!(lookup("dogs")[0].lemma, "dog");
        assert!(lookup("running").iter().any(|s| s.lemma == "run"));
        assert!(lookup("mice").iter().any(|s| s.lemma == "mouse"));
        assert!(lookup("happier").iter().any(|s| s.lemma == "happy"));
        assert!(lookup("went").iter().any(|s| s.lemma == "go"));
    }

    #[test]
    fn punctuation_and_case_are_ignored() {
        assert_eq!(lookup("  \u{201C}Happy,\u{201D} "), lookup("happy"));
    }

    #[test]
    fn antonyms_come_from_the_right_word() {
        let hot = lookup("hot");
        assert!(
            hot.iter()
                .flat_map(|s| &s.antonyms)
                .any(|a| a.word == "cold"),
            "{hot:?}"
        );
        let happy = lookup("happy");
        assert!(happy[0].antonyms.iter().any(|a| a.word == "unhappy"));
        // A satellite adjective borrows its main adjective's antonym.
        let glad = lookup("glad");
        assert!(glad.iter().any(|s| !s.antonyms.is_empty()));
    }

    #[test]
    fn examples_are_split_from_meanings() {
        let (m, e) = split_gloss("a small cat; \"the cat sat\"; \"a cat purred; softly\"");
        assert_eq!(m, "a small cat");
        assert_eq!(e, vec!["the cat sat", "a cat purred; softly"]);
        let (m, e) = split_gloss("no examples here");
        assert_eq!((m.as_str(), e.len()), ("no examples here", 0));
    }

    #[test]
    fn unknown_and_empty_input_find_nothing() {
        assert!(lookup("").is_empty());
        assert!(lookup("   ...  ").is_empty());
        assert!(lookup("zzqxjv").is_empty());
        assert!(lookup(&"a".repeat(100)).is_empty());
    }

    #[test]
    fn phrases_and_first_word_fallback() {
        assert!(lookup("ice cream").iter().any(|s| s.lemma == "ice cream"));
        assert!(!lookup("happy zzqxjv").is_empty());
    }

    #[test]
    fn every_synset_offset_is_valid() {
        // The data files must be byte for byte as WordNet ships them (offsets are byte counts).
        for word in ["dog", "run", "good", "quickly", "light", "set"] {
            for s in lookup(word) {
                assert!(!s.meaning.is_empty(), "{word}: {s:?}");
            }
        }
    }
}
