// Extra grammar rules, pulled into `analysis.rs` with `include!`. They share its imports,
// `Issue`, `Category`, `Token`, `push`, `with_case_of` and friends.
//
// Every rule here is a pattern over lower-cased word tokens. "Adjacent" always means the next
// word follows after plain spaces only, so a comma, full stop, quote or line break stops a
// pattern. That one habit is what keeps most rules from firing across clause boundaries.

type Out = Vec<Issue>;

/// Context for one pass over a text.
struct Cx<'a> {
    text: &'a str,
    toks: &'a [Token],
    /// Lower-cased tokens, curly apostrophes turned straight.
    w: Vec<String>,
    on: &'a EnabledCategories,
}

impl<'a> Cx<'a> {
    fn n(&self) -> usize {
        self.toks.len()
    }
    fn lw(&self, i: usize) -> &str {
        self.w.get(i).map_or("", String::as_str)
    }
    fn raw(&self, i: usize) -> &'a str {
        self.toks.get(i).map_or("", |t| t.text(self.text))
    }
    fn gap(&self, i: usize) -> &'a str {
        match (self.toks.get(i), self.toks.get(i + 1)) {
            (Some(a), Some(b)) => self.text.get(a.end..b.start).unwrap_or(""),
            _ => "",
        }
    }
    /// Token `i` and `i + 1` are separated by spaces only.
    fn adj(&self, i: usize) -> bool {
        let g = self.gap(i);
        self.toks.get(i + 1).is_some()
            && !g.is_empty()
            && g.chars().all(|c| c == ' ' || c == '\t' || c == '\u{a0}')
    }
    /// The `k`-th word after `i`, or "" when anything but spaces lies between.
    fn nx(&self, i: usize, k: usize) -> &str {
        for j in 0..k {
            if !self.adj(i + j) {
                return "";
            }
        }
        self.lw(i + k)
    }
    /// The word before `i`, or "" when anything but spaces lies between.
    fn pv(&self, i: usize) -> &str {
        if i > 0 && self.adj(i - 1) {
            self.lw(i - 1)
        } else {
            ""
        }
    }
    fn starts_clause(&self, i: usize) -> bool {
        i == 0
            || self.gap(i - 1).chars().any(|c| {
                matches!(
                    c,
                    '.' | '!' | '?' | ',' | ';' | ':' | '\n' | '\r' | '"' | '\u{201C}' | '\u{201D}'
                        | '(' | '\u{2014}' | '\u{2013}'
                )
            })
    }
    fn starts_sentence(&self, i: usize) -> bool {
        i == 0
            || self
                .gap(i - 1)
                .chars()
                .any(|c| matches!(c, '.' | '!' | '?' | '\n' | '\r'))
    }
    /// A punctuation mark, a line break or the end of the text follows token `i`.
    fn clause_end(&self, i: usize) -> bool {
        let end = self.toks.get(i).map_or(self.text.len(), |t| t.end);
        let rest = self.text.get(end..).unwrap_or("");
        match rest.trim_start_matches([' ', '\t']).chars().next() {
            None => true,
            Some(c) => matches!(
                c,
                '.' | ',' | '!' | '?' | ';' | ':' | '\n' | '\r' | '"' | '\u{201D}' | '\u{2014}' | ')'
            ),
        }
    }
    /// All words of `pat` (lower case) follow each other from token `i`.
    fn run(&self, i: usize, pat: &[&str]) -> bool {
        pat.iter().enumerate().all(|(k, p)| {
            self.lw(i + k) == *p && (k == 0 || self.adj(i + k - 1))
        })
    }
    /// Walk back over the clause (adjacent tokens only) looking for one of `words`.
    fn clause_has_before(&self, i: usize, words: &[&str], reach: usize) -> bool {
        let mut j = i;
        for _ in 0..reach {
            if j == 0 || !self.adj(j - 1) {
                return false;
            }
            j -= 1;
            if words.contains(&self.lw(j)) {
                return true;
            }
        }
        false
    }
    /// Walk forward over the clause looking for one of `words`; returns its token index.
    fn find_after(&self, i: usize, words: &[&str], reach: usize) -> Option<usize> {
        let mut j = i;
        for _ in 0..reach {
            if !self.adj(j) {
                return None;
            }
            j += 1;
            if words.contains(&self.lw(j)) {
                return Some(j);
            }
        }
        None
    }
}

fn flag(
    cx: &Cx,
    out: &mut Out,
    cat: Category,
    rule: &'static str,
    title: &'static str,
    a: usize,
    b: usize,
    msg: impl Into<String>,
    rep: Option<String>,
) {
    if !cx.on.is_on(cat) {
        return;
    }
    let (Some(x), Some(y)) = (cx.toks.get(a), cx.toks.get(b)) else {
        return;
    };
    push(out, x.start..y.end, cat, rule, title, msg.into(), rep);
}

// ---------------------------------------------------------------------------------------------
// Word lists
// ---------------------------------------------------------------------------------------------

const SUBJ_PRON: &[&str] = &["i", "you", "he", "she", "it", "we", "they"];
const OBJ_PRON: &[&str] = &["me", "you", "him", "her", "it", "us", "them"];
const NUM_WORDS: &[&str] = &[
    "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
    "fifteen", "twenty", "thirty", "forty", "fifty", "hundred",
];
const UNITS: &[&str] = &[
    "second", "seconds", "minute", "minutes", "hour", "hours", "day", "days", "week", "weeks",
    "month", "months", "year", "years", "decade", "decades",
];
const MONTHS: &[&str] = &[
    "january", "february", "march", "april", "june", "july", "august", "september", "october",
    "november", "december",
];
const WEEKDAYS: &[&str] = &[
    "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
];

/// Irregular past forms: (past, base).
const IRREG_PAST: &[(&str, &str)] = &[
    ("saw", "see"),
    ("went", "go"),
    ("came", "come"),
    ("took", "take"),
    ("gave", "give"),
    ("ate", "eat"),
    ("wrote", "write"),
    ("made", "make"),
    ("knew", "know"),
    ("thought", "think"),
    ("bought", "buy"),
    ("brought", "bring"),
    ("told", "tell"),
    ("found", "find"),
    ("felt", "feel"),
    ("kept", "keep"),
    ("held", "hold"),
    ("sat", "sit"),
    ("stood", "stand"),
    ("understood", "understand"),
    ("lost", "lose"),
    ("paid", "pay"),
    ("met", "meet"),
    ("sent", "send"),
    ("spent", "spend"),
    ("built", "build"),
    ("fell", "fall"),
    ("drove", "drive"),
    ("flew", "fly"),
    ("spoke", "speak"),
    ("chose", "choose"),
    ("drank", "drink"),
    ("sang", "sing"),
    ("swam", "swim"),
    ("threw", "throw"),
    ("wore", "wear"),
    ("broke", "break"),
    ("began", "begin"),
    ("forgot", "forget"),
    ("caught", "catch"),
    ("taught", "teach"),
    ("sold", "sell"),
    ("heard", "hear"),
    ("said", "say"),
    ("left", "leave"),
    ("got", "get"),
    ("had", "have"),
];

/// Past forms that cannot be a participle: (past, participle). Only the ones the older
/// rule in `check_grammar` does not already know.
const PAST_TO_PARTICIPLE: &[(&str, &str)] = &[
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
    ("saw", "seen"),
    ("was", "been"),
    ("were", "been"),
    ("fell", "fallen"),
    ("stole", "stolen"),
    ("froze", "frozen"),
    ("grew", "grown"),
    ("blew", "blown"),
    ("shook", "shaken"),
    ("tore", "torn"),
    ("rode", "ridden"),
    ("drew", "drawn"),
    ("swore", "sworn"),
    ("woke", "woken"),
    ("hid", "hidden"),
];

/// Base verbs: used for "he need", "enjoy to go" and the like.
const BASE_VERBS: &[&str] = &[
    "need", "want", "go", "love", "think", "know", "say", "see", "make", "take", "get", "give",
    "come", "work", "live", "play", "eat", "run", "look", "feel", "seem", "tell", "ask", "try",
    "use", "find", "keep", "help", "begin", "show", "hear", "turn", "start", "bring", "write",
    "sit", "stand", "lose", "pay", "meet", "learn", "change", "lead", "understand", "watch",
    "follow", "stop", "speak", "buy", "wait", "send", "expect", "build", "stay", "fall", "reach",
    "kill", "remain", "suggest", "raise", "pass", "sell", "require", "decide", "pull", "hate",
    "enjoy", "believe", "hope", "wish", "walk", "talk", "drive", "sleep", "wear", "drink", "leave",
    "call", "move", "open", "close", "swim", "cook", "sing", "dance", "ride", "fly", "visit",
    "travel", "study", "finish", "arrive", "forget", "remember", "carry", "catch", "teach",
    "clean", "check", "break", "join", "break",
];

/// Third-person singular forms of common verbs.
const VERB_S: &[&str] = &[
    "goes", "needs", "wants", "likes", "thinks", "knows", "says", "sees", "makes", "takes",
    "gets", "gives", "comes", "works", "lives", "plays", "eats", "runs", "looks", "feels",
    "seems", "tells", "asks", "tries", "uses", "finds", "keeps", "helps", "begins", "shows",
    "hears", "turns", "starts", "brings", "writes", "reads", "sits", "stands", "loses", "pays",
    "meets", "learns", "changes", "leads", "watches", "follows", "stops", "speaks", "buys",
    "waits", "sends", "expects", "builds", "stays", "falls", "reaches", "kills", "remains",
    "suggests", "raises", "passes", "sells", "requires", "decides", "pulls", "loves", "hates",
    "enjoys", "believes", "hopes", "wishes", "walks", "talks", "drives", "sleeps", "wears",
    "drinks", "leaves", "calls", "moves", "opens", "closes", "does", "has", "is", "was",
];

/// Verbs that are safe to test as "<plural pronoun> + verb-s" (never also a plural noun).
const VERB_S_SAFE: &[&str] = &[
    "goes", "thinks", "knows", "says", "sees", "takes", "gets", "gives", "comes", "seems",
    "tells", "asks", "tries", "finds", "keeps", "begins", "hears", "brings", "writes", "sits",
    "stands", "loses", "meets", "learns", "watches", "follows", "stops", "speaks", "buys",
    "waits", "expects", "builds", "stays", "falls", "reaches", "remains", "suggests", "decides",
    "believes", "hopes", "wishes", "walks", "talks", "drives", "sleeps", "wears", "drinks",
    "loves", "hates", "enjoys", "wants", "needs", "likes", "lives", "makes", "plays", "looks",
    "feels", "does", "has", "runs", "eats", "works",
];

/// Words that look plural but are not: nothing to fix after "every" or "less".
const NOT_PLURAL: &[&str] = &[
    "always", "perhaps", "sometimes", "besides", "towards", "afterwards", "whereas", "series",
    "species", "news", "lens", "gas", "bus", "canvas", "atlas", "bias", "chaos", "focus",
    "status", "virus", "campus", "bonus", "genius", "minus", "plus", "thus", "this", "his",
    "its", "was", "has", "does", "goes", "says", "yes", "less", "unless", "across", "means",
    "headquarters", "crossroads", "mathematics", "physics", "economics", "politics",
    "athletics", "gymnastics", "linguistics", "genetics", "measles", "mumps", "ethics",
    "sometimes", "nowadays", "anyways", "overseas", "upstairs", "downstairs", "outdoors",
    "indoors", "ours", "yours", "hers", "theirs", "whats", "thats", "lets", "others",
];

const IRREG_PLURALS: &[(&str, &str)] = &[
    ("children", "child"),
    ("men", "man"),
    ("women", "woman"),
    ("people", "person"),
    ("feet", "foot"),
    ("teeth", "tooth"),
    ("mice", "mouse"),
];

fn looks_plural(w: &str) -> bool {
    let n = w.chars().count();
    if n < 4 || !w.ends_with('s') || NOT_PLURAL.contains(&w) || VERB_S.contains(&w) {
        return false;
    }
    !(w.ends_with("ss") || w.ends_with("us") || w.ends_with("is") || w.ends_with("as")
        || w.ends_with("os"))
}

fn is_irregular_plural(w: &str) -> bool {
    IRREG_PLURALS.iter().any(|(p, _)| *p == w)
        || matches!(w, "police" | "cattle" | "staff" | "data" | "media" | "sheep" | "fish" | "deer")
}

fn singular_of(w: &str) -> Option<String> {
    if let Some(&(_, s)) = IRREG_PLURALS.iter().find(|(p, _)| *p == w) {
        return Some(s.to_string());
    }
    if !looks_plural(w) {
        return None;
    }
    if let Some(stem) = w.strip_suffix("ies") {
        return Some(format!("{stem}y"));
    }
    for tail in ["ches", "shes", "xes", "sses", "zes"] {
        if w.ends_with(tail) {
            return Some(w.get(..w.len() - 2)?.to_string());
        }
    }
    w.strip_suffix('s').map(str::to_string)
}

fn pluralize(w: &str) -> String {
    if let Some(&(p, _)) = IRREG_PLURALS.iter().find(|(_, s)| *s == w) {
        return p.to_string();
    }
    if let Some(stem) = w.strip_suffix('y') {
        if !stem.ends_with(['a', 'e', 'i', 'o', 'u']) && !stem.is_empty() {
            return format!("{stem}ies");
        }
    }
    if w.ends_with(['s', 'x', 'z']) || w.ends_with("ch") || w.ends_with("sh") {
        return format!("{w}es");
    }
    format!("{w}s")
}

fn gerund(v: &str) -> String {
    const DOUBLE: &[&str] = &[
        "run", "sit", "get", "put", "stop", "plan", "begin", "swim", "win", "cut", "hit", "set",
        "let", "shop", "chat", "drop", "hug", "forget", "meet",
    ];
    if v == "be" {
        return "being".into();
    }
    if v == "see" || v.ends_with("ee") || v.ends_with("ye") || v.ends_with("oe") {
        return format!("{v}ing");
    }
    if v == "die" || v == "lie" {
        return format!("{}ying", &v[..v.len() - 2]);
    }
    if DOUBLE.contains(&v) {
        let last = v.chars().last().unwrap_or('x');
        return format!("{v}{last}ing");
    }
    if let Some(stem) = v.strip_suffix('e') {
        return format!("{stem}ing");
    }
    format!("{v}ing")
}

fn plural_verb_to_singular(v: &str) -> Option<&'static str> {
    Some(match v {
        "are" => "is",
        "were" => "was",
        "have" => "has",
        "do" => "does",
        "don't" => "doesn't",
        _ => return None,
    })
}

fn singular_verb_to_plural(v: &str) -> Option<&'static str> {
    Some(match v {
        "is" => "are",
        "was" => "were",
        "has" => "have",
        "does" => "do",
        "doesn't" => "don't",
        _ => return None,
    })
}

fn base_of_past(w: &str) -> Option<&'static str> {
    IRREG_PAST.iter().find(|(p, _)| *p == w).map(|&(_, b)| b)
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// Tables of single-word fixes -----------------------------------------------------------------

const MISSPELLINGS: &[(&str, &str)] = &[
    ("fourty", "forty"),
    ("tryed", "tried"),
    ("studyed", "studied"),
    ("carryed", "carried"),
    ("copyed", "copied"),
    ("cryed", "cried"),
    ("alot", "a lot"),
    ("wouldnt", "wouldn't"),
    ("definately", "definitely"),
    ("definatly", "definitely"),
    ("seperate", "separate"),
    ("seperately", "separately"),
    ("occured", "occurred"),
    ("occurence", "occurrence"),
    ("recieve", "receive"),
    ("recieved", "received"),
    ("untill", "until"),
    ("becuase", "because"),
    ("beacause", "because"),
    ("thier", "their"),
    ("tommorow", "tomorrow"),
    ("tomorow", "tomorrow"),
    ("beleive", "believe"),
    ("freind", "friend"),
    ("goverment", "government"),
    ("neccessary", "necessary"),
    ("necesary", "necessary"),
    ("accomodate", "accommodate"),
    ("embarass", "embarrass"),
    ("existance", "existence"),
    ("grammer", "grammar"),
    ("publically", "publicly"),
    ("wierd", "weird"),
    ("truely", "truly"),
    ("writting", "writing"),
    ("begining", "beginning"),
    ("comming", "coming"),
    ("finaly", "finally"),
    ("sucess", "success"),
    ("enviroment", "environment"),
    ("adress", "address"),
    ("calender", "calendar"),
    ("collegue", "colleague"),
    ("concious", "conscious"),
    ("dissapear", "disappear"),
    ("dissapoint", "disappoint"),
    ("exagerate", "exaggerate"),
    ("foriegn", "foreign"),
    ("gaurd", "guard"),
    ("hieght", "height"),
    ("immediatly", "immediately"),
    ("independant", "independent"),
    ("knowlege", "knowledge"),
    ("liason", "liaison"),
    ("maintainance", "maintenance"),
    ("millenium", "millennium"),
    ("noticable", "noticeable"),
    ("ocassion", "occasion"),
    ("persistant", "persistent"),
    ("posession", "possession"),
    ("prefered", "preferred"),
    ("privelege", "privilege"),
    ("pronounciation", "pronunciation"),
    ("reccomend", "recommend"),
    ("refered", "referred"),
    ("relevent", "relevant"),
    ("religous", "religious"),
    ("rythm", "rhythm"),
    ("sentance", "sentence"),
    ("succesful", "successful"),
    ("tounge", "tongue"),
    ("tyrany", "tyranny"),
    ("unfortunatly", "unfortunately"),
    ("wich", "which"),
    ("whereever", "wherever"),
    ("aswell", "as well"),
    ("incase", "in case"),
    ("infact", "in fact"),
    ("eachother", "each other"),
    ("atleast", "at least"),
    ("noone", "no one"),
    ("thankyou", "thank you"),
    ("everytime", "every time"),
    ("inspite", "in spite"),
    ("alittle", "a little"),
    ("ofcourse", "of course"),
    ("themself", "themselves"),
    ("theirselves", "themselves"),
    ("hisself", "himself"),
    ("ourselfs", "ourselves"),
    ("themselfs", "themselves"),
    ("yourselfs", "yourselves"),
    ("informations", "information"),
    ("advices", "advice"),
    ("equipments", "equipment"),
    ("furnitures", "furniture"),
    ("homeworks", "homework"),
    ("luggages", "luggage"),
    ("knowledges", "knowledge"),
];

const MISSING_APOSTROPHE: &[(&str, &str)] = &[
    ("cant", "can't"),
    ("dont", "don't"),
    ("doesnt", "doesn't"),
    ("didnt", "didn't"),
    ("isnt", "isn't"),
    ("wasnt", "wasn't"),
    ("arent", "aren't"),
    ("werent", "weren't"),
    ("couldnt", "couldn't"),
    ("wouldnt", "wouldn't"),
    ("shouldnt", "shouldn't"),
    ("havent", "haven't"),
    ("hasnt", "hasn't"),
    ("hadnt", "hadn't"),
    ("youre", "you're"),
    ("theyre", "they're"),
    ("ive", "I've"),
    ("im", "I'm"),
    ("theres", "there's"),
    ("wheres", "where's"),
    ("whats", "what's"),
    ("thats", "that's"),
    ("your's", "yours"),
    ("her's", "hers"),
    ("their's", "theirs"),
    ("our's", "ours"),
];

/// Over-regularised verbs ("goed", "buyed"): (wrong, right).
const OVERREGULARISED: &[(&str, &str)] = &[
    ("choosed", "chose"),
    ("teached", "taught"),
    ("buyed", "bought"),
    ("goed", "went"),
    ("runned", "ran"),
    ("flied", "flew"),
    ("swimmed", "swam"),
    ("eated", "ate"),
    ("writed", "wrote"),
    ("bringed", "brought"),
    ("thinked", "thought"),
    ("catched", "caught"),
    ("comed", "came"),
    ("taked", "took"),
    ("gived", "gave"),
    ("knowed", "knew"),
    ("falled", "fell"),
    ("feeled", "felt"),
    ("keeped", "kept"),
    ("telled", "told"),
    ("builded", "built"),
    ("payed", "paid"),
    ("losed", "lost"),
    ("sleeped", "slept"),
    ("drawed", "drew"),
    ("drinked", "drank"),
    ("speaked", "spoke"),
];

/// Multi-word phrases that are wrong as written: (phrase, replacement, why).
const PHRASE_FIXES: &[(&str, &str, &str)] = &[
    ("case and point", "case in point", "The idiom is 'case in point'."),
    ("for all intensive purposes", "for all intents and purposes", "The idiom is 'for all intents and purposes'."),
    ("nip it in the butt", "nip it in the bud", "The idiom is 'nip it in the bud'."),
    ("one in the same", "one and the same", "The idiom is 'one and the same'."),
    ("wet your appetite", "whet your appetite", "The idiom is 'whet your appetite'."),
    ("tow the line", "toe the line", "The idiom is 'toe the line'."),
    ("bare with me", "bear with me", "The idiom is 'bear with me'."),
    ("free reign", "free rein", "The idiom is 'free rein'."),
    ("mute point", "moot point", "The phrase is 'moot point'."),
    ("sneak peak", "sneak peek", "The phrase is 'sneak peek'."),
    ("deep seeded", "deep-seated", "The phrase is 'deep-seated'."),
    ("tongue and cheek", "tongue in cheek", "The idiom is 'tongue in cheek'."),
    ("on accident", "by accident", "The usual idiom is 'by accident'."),
    ("in regards to", "in regard to", "The standard phrase is 'in regard to' (or 'regarding')."),
    ("hone in on", "home in on", "The idiom is 'home in on'."),
    ("peaked my interest", "piqued my interest", "The word is 'piqued'."),
    ("peak my interest", "pique my interest", "The word is 'pique'."),
    ("by in large", "by and large", "The idiom is 'by and large'."),
    ("make due", "make do", "The idiom is 'make do'."),
    ("all of the sudden", "all of a sudden", "The idiom is 'all of a sudden'."),
    ("all the sudden", "all of a sudden", "The idiom is 'all of a sudden'."),
    ("take it for granite", "take it for granted", "The idiom is 'take it for granted'."),
    ("in the other hand", "on the other hand", "The idiom is 'on the other hand'."),
    ("could care less", "couldn't care less", "The idiom is 'couldn't care less'."),
    ("due to fact", "due to the fact", "An article is missing: 'due to the fact'."),
    ("a part from", "apart from", "'Apart' is one word here."),
    ("each others", "each other's", "Use 'each other' or the possessive 'each other's'."),
    ("a lot of times", "many times", "'Many times' reads better."),
    ("revert back", "revert", "'Revert' already means 'go back'."),
    ("repeat again", "repeat", "'Repeat' already means 'do again'."),
    ("return back", "return", "'Return' already means 'come back'."),
    ("reply back", "reply", "'Reply' already means 'answer back'."),
    ("combine together", "combine", "'Combine' already means 'put together'."),
    ("join together", "join", "'Join' already means 'put together'."),
    ("merge together", "merge", "'Merge' already means 'come together'."),
    ("past history", "history", "'History' is already about the past."),
    ("free gift", "gift", "A gift is already free."),
    ("end result", "result", "'Result' is already the end."),
    ("unexpected surprise", "surprise", "A surprise is already unexpected."),
    ("added bonus", "bonus", "A bonus is already an addition."),
    ("close proximity", "proximity", "'Proximity' already means 'closeness'."),
    ("sum total", "total", "'Total' says it already."),
    ("true fact", "fact", "A fact is already true."),
    ("basic fundamentals", "fundamentals", "'Fundamentals' are already basic."),
    ("the affect of", "the effect of", "As a noun, the word is 'effect'."),
    ("side affect", "side effect", "As a noun, the word is 'effect'."),
    ("side affects", "side effects", "As a noun, the word is 'effects'."),
    ("an affect on", "an effect on", "As a noun, the word is 'effect'."),
    ("no affect on", "no effect on", "As a noun, the word is 'effect'."),
    ("brake the news", "break the news", "To 'break' news, spell it with 'ea'."),
    ("brake the ice", "break the ice", "The idiom is 'break the ice'."),
    ("brake the silence", "break the silence", "To 'break' the silence, spell it with 'ea'."),
    ("brake a leg", "break a leg", "The idiom is 'break a leg'."),
    ("take a brake", "take a break", "A rest is a 'break'."),
    ("hit the breaks", "hit the brakes", "On a car, they are 'brakes'."),
    ("on the breaks", "on the brakes", "On a car, they are 'brakes'."),
    ("wind blue", "wind blew", "The past tense of 'blow' is 'blew'."),
    ("returned back", "returned", "'Returned' already means 'came back'."),
    ("joined together", "joined", "'Joined' already means 'put together'."),
    ("reversed back", "reversed", "'Reversed' already means 'went back'."),
    ("reason why is because", "reason is that", "'Reason why … because' says the same thing twice."),
];

// ---------------------------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------------------------

fn extra_rules(text: &str, toks: &[Token], on: &EnabledCategories, out: &mut Out) {
    let w: Vec<String> = toks
        .iter()
        .map(|t| t.text(text).to_lowercase().replace('\u{2019}', "'"))
        .collect();
    let cx = Cx { text, toks, w, on };
    for i in 0..cx.n() {
        fixed_words(&cx, i, out);
        phrases(&cx, i, out);
        confusables(&cx, i, out);
        agreement(&cx, i, out);
        quantity(&cx, i, out);
        verbs(&cx, i, out);
        prepositions(&cx, i, out);
        structure(&cx, i, out);
        capitals(&cx, i, out);
        polish(&cx, i, out);
    }
    char_rules(text, on, out);
}

include!("rules_polish.rs");

fn fixed_words(cx: &Cx, i: usize, out: &mut Out) {
    let w = cx.lw(i);
    let raw = cx.raw(i);
    if let Some(&(_, r)) = MISSPELLINGS.iter().find(|(x, _)| *x == w) {
        flag(cx, out, Category::WordChoice, "misspelling", "Spelling", i, i,
            format!("'{raw}' is a common misspelling of '{r}'."), Some(with_case_of(raw, r)));
    }
    if let Some(&(_, r)) = MISSING_APOSTROPHE.iter().find(|(x, _)| *x == w) {
        // "im" and "ive" in all capitals are probably not the contraction.
        let shouty = raw.chars().count() > 1 && raw.chars().all(char::is_uppercase);
        if !shouty && !(w == "ive" && cx.starts_clause(i) && false) {
            let rep = if r.starts_with(char::is_uppercase) {
                r.to_string()
            } else {
                with_case_of(raw, r)
            };
            flag(cx, out, Category::WordChoice, "missing-apostrophe", "Apostrophe", i, i,
                format!("'{raw}' needs an apostrophe: '{rep}'."), Some(rep));
        }
    }
    if let Some(&(_, r)) = OVERREGULARISED.iter().find(|(x, _)| *x == w) {
        flag(cx, out, Category::Tense, "overregularised", "Verb Form", i, i,
            format!("'{raw}' is not a word; the past form is '{r}'."), Some(with_case_of(raw, r)));
    }
}

fn phrases(cx: &Cx, i: usize, out: &mut Out) {
    for &(pat, fixed, why) in PHRASE_FIXES {
        let words: Vec<&str> = pat.split(' ').collect();
        if cx.run(i, &words) {
            let last = i + words.len() - 1;
            flag(cx, out, Category::WordChoice, "phrase-fix", "Word Choice", i, last, why,
                Some(with_case_of(cx.raw(i), fixed)));
            return;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Confusable words
// ---------------------------------------------------------------------------------------------

const POSSESSIBLE: &[&str] = &[
    "answer", "name", "car", "house", "home", "mother", "father", "friend", "friends", "book",
    "books", "hand", "hands", "eyes", "face", "hair", "dog", "cat", "family", "job", "life",
    "turn", "own", "best", "favorite", "favourite", "idea", "opinion", "money", "phone", "room",
    "work", "parents", "children", "kids", "brother", "sister", "wife", "husband", "boss",
    "team", "country", "school", "teacher", "bag", "coat", "shoes", "plan", "plans", "mind",
    "head", "heart", "birthday", "email", "message", "order", "bill", "seat", "place",
];
const YOURE_PREDICATE: &[&str] = &[
    "welcome", "going", "doing", "being", "not", "right", "wrong", "so", "very", "too", "always",
    "never", "supposed", "getting", "coming", "kidding", "joking", "crazy", "lucky", "gonna",
    "late", "ready", "sure", "mad", "just", "still", "really", "all", "the",
];

fn confusables(cx: &Cx, i: usize, out: &mut Out) {
    let w = cx.lw(i);
    let raw = cx.raw(i);
    let n1 = cx.nx(i, 1);
    let n2 = cx.nx(i, 2);
    let p = cx.pv(i);
    let wc = Category::WordChoice;

    // your / you're
    // "You're friends now" is fine; "you're friend is" is not: these nouns need a verb after them.
    let ambiguous = matches!(n1, "friend" | "friends" | "family" | "home" | "parents" | "kids" | "children"
        | "brother" | "sister" | "wife" | "husband" | "boss" | "team" | "teacher" | "mother" | "father"
        | "best" | "own" | "work" | "money" | "place" | "room" | "school" | "country")
        && !matches!(n1, "own")
        && !matches!(n2, "is" | "was" | "are" | "were" | "has" | "had" | "will" | "does" | "did" | "looks" | "seems");
    if w == "you're" && POSSESSIBLE.contains(&n1) && !matches!(n1, "welcome") && !ambiguous {
        flag(cx, out, wc, "your-youre", "Your / You're", i, i,
            "'you're' means 'you are'. The possessive is 'your'.", Some(with_case_of(raw, "your")));
    }
    if w == "your" && !n1.is_empty() && YOURE_PREDICATE.contains(&n1) {
        let tight = matches!(n1, "welcome" | "right" | "wrong" | "late" | "ready" | "sure" | "lucky"
            | "crazy" | "mad" | "all" | "the" | "just" | "still" | "really");
        let ok = !tight
            || cx.clause_end(i + 1)
            || matches!(n2, "about" | "to" | "and" | "but" | "if" | "for" | "that" | "when")
            || (n1 == "just" || n1 == "still" || n1 == "really") && !n2.is_empty()
                && !POSSESSIBLE.contains(&n2);
        let skip_the = n1 == "the" || n1 == "all";
        if ok && !skip_the {
            flag(cx, out, wc, "your-youre", "Your / You're", i, i,
                "'your' is possessive. Here 'you're' (you are) is meant.", Some(with_case_of(raw, "you're")));
        }
    }

    // its / it's (more cases than the older rule)
    if w == "its" {
        let day_part = WEEKDAYS.contains(&n1)
            || matches!(n1, "morning" | "afternoon" | "evening" | "night" | "noon" | "midnight"
                | "raining" | "snowing" | "getting" | "been" | "my" | "your" | "his" | "our" | "their"
                | "me" | "you" | "him" | "us" | "them" | "the" | "not" | "okay" | "ok")
            || (n1 == "time" && n2 == "to")
            || (n1 == "her" && !POSSESSIBLE.contains(&n2));
        if day_part {
            flag(cx, out, wc, "its-contraction", "Contraction", i, i + 1,
                "The possessive 'its' is used where 'it's' (it is) is meant.",
                Some(format!("{} {}", with_case_of(raw, "it's"), cx.raw(i + 1))));
        }
    }
    if w == "it's"
        && matches!(n1, "head" | "eyes" | "legs" | "nest" | "paws" | "fur" | "mouth" | "body"
            | "wings" | "colours" | "colors" | "teeth" | "claws" | "feathers" | "roots" | "branches")
    {
        flag(cx, out, wc, "its-possessive", "Possessive Pronoun", i, i + 1,
            "Incorrect use of contraction it's (it is) instead of the possessive pronoun 'its'.",
            Some(format!("{} {}", with_case_of(raw, "its"), cx.raw(i + 1))));
    }

    // their / there / they're
    if w == "there" && (matches!(n1, "own") || (POSSESSIBLE.contains(&n1)
        && matches!(n1, "children" | "kids" | "parents" | "mother" | "father" | "house" | "home"
            | "car" | "names" | "eyes" | "hands" | "family" | "dog" | "friends")
        && !matches!(p, "over" | "in" | "out" | "from" | "up" | "down" | "to" | "of")))
    {
        flag(cx, out, wc, "their-there", "There / Their", i, i,
            "'there' points to a place. The possessive is 'their'.", Some(with_case_of(raw, "their")));
    }
    if w == "their" && matches!(n1, "is" | "are" | "was" | "were" | "will" | "has" | "have") {
        flag(cx, out, wc, "their-there", "There / Their", i, i,
            "'their' is possessive. 'There' is meant here.", Some(with_case_of(raw, "there")));
    }
    // "Their going was sudden" is a fine possessive; "Their going to the park" is not.
    let gerund_ok = !matches!(n1, "going" | "coming" | "doing" | "being")
        || matches!(n2, "to" | "a" | "an" | "so" | "very" | "too" | "not" | "always" | "never" | "just"
            | "really" | "still" | "out" | "over" | "home" | "back" | "away");
    if w == "their" && matches!(n1, "going" | "coming" | "doing" | "being" | "not" | "always"
        | "never" | "just" | "very" | "all") && gerund_ok
        && !(n1 == "all" && !cx.clause_end(i + 1) && POSSESSIBLE.contains(&n2))
    {
        flag(cx, out, wc, "their-theyre", "Their / They're", i, i,
            "'their' is possessive. Here 'they're' (they are) is meant.", Some(with_case_of(raw, "they're")));
    }
    if w == "they're"
        && matches!(n1, "own" | "house" | "home" | "car" | "mother" | "father" | "parents"
            | "children" | "kids" | "names" | "eyes" | "dog")
    {
        flag(cx, out, wc, "their-theyre", "Their / They're", i, i,
            "'they're' means 'they are'. The possessive is 'their'.", Some(with_case_of(raw, "their")));
    }
    if w == "there" && matches!(n1, "going" | "coming" | "doing" | "being" | "getting")
        && !matches!(p, "went" | "go" | "goes" | "get" | "got" | "come" | "came" | "over" | "out")
    {
        flag(cx, out, wc, "their-theyre", "There / They're", i, i,
            "'there' is a place. Here 'they're' (they are) is meant.", Some(with_case_of(raw, "they're")));
    }

    // to / too
    if w == "to" && matches!(n1, "much" | "many") {
        flag(cx, out, wc, "to-too", "To / Too", i, i,
            "'to' is a preposition. Use 'too' to mean 'excessively'.", Some(with_case_of(raw, "too")));
    }
    if w == "to" && matches!(n1, "late" | "early" | "long" | "short" | "big" | "small" | "hot"
        | "cold" | "far" | "soon") && matches!(p, "is" | "was" | "are" | "were" | "be" | "been"
        | "am" | "it's" | "that's" | "way" | "far" | "much" | "not" | "get" | "getting")
        && cx.clause_end(i + 1)
    {
        flag(cx, out, wc, "to-too", "To / Too", i, i,
            "'to' is a preposition. Use 'too' to mean 'excessively'.", Some(with_case_of(raw, "too")));
    }
    if w == "too" && n1 == "enough" {
        flag(cx, out, wc, "too-enough", "Too / Enough", i, i + 1,
            "'too' and 'enough' do not go together; use one of them.", None);
    }
    if w == "too" {
        // "too honest so that she cannot lie": "too ... that"
        if let Some(j) = cx.find_after(i, &["so"], 6) {
            if cx.nx(j, 1) == "that" {
                flag(cx, out, Category::Fragment, "too-so-that", "Sentence Structure", j, j + 1,
                    "After 'too' use 'that' (or 'to'), not 'so that'.", Some("that".to_string()));
            }
        }
    }

    // then / than
    if w == "then" {
        let comparative = matches!(p, "more" | "less" | "rather" | "better" | "worse" | "bigger"
            | "smaller" | "larger" | "faster" | "slower" | "taller" | "higher" | "lower" | "older"
            | "younger" | "greater" | "easier" | "harder" | "longer" | "shorter" | "stronger"
            | "other" | "sooner" | "later" | "different" | "prettier" | "cheaper" | "happier"
            | "worse" | "further" | "farther");
        if comparative {
            flag(cx, out, wc, "then-than", "Then / Than", i, i,
                "Use 'than' to compare; 'then' is about time.", Some(with_case_of(raw, "than")));
        }
    }
    if w == "than" && p == "and" {
        flag(cx, out, wc, "then-than", "Then / Than", i, i,
            "'than' compares. 'And then' is about time.", Some(with_case_of(raw, "then")));
    }

    // who's / whose
    if w == "whose" && matches!(n1, "there" | "going" | "coming" | "been" | "got" | "the") {
        flag(cx, out, wc, "whos-whose", "Who's / Whose", i, i,
            "'whose' is possessive. Here 'who's' (who is) is meant.", Some(with_case_of(raw, "who's")));
    }
    if w == "who's" && matches!(n1, "book" | "car" | "name" | "turn" | "idea" | "fault" | "house"
        | "phone" | "bag" | "coat" | "dog" | "cat" | "mother" | "father" | "side")
    {
        flag(cx, out, wc, "whos-whose", "Who's / Whose", i, i,
            "'who's' means 'who is'. The possessive is 'whose'.", Some(with_case_of(raw, "whose")));
    }

    // weather / whether
    if w == "whether" && matches!(p, "the" | "good" | "bad" | "nice" | "cold" | "hot" | "warm"
        | "this" | "today's")
    {
        flag(cx, out, wc, "weather-whether", "Weather / Whether", i, i,
            "'whether' introduces a choice. The word for rain and sun is 'weather'.",
            Some(with_case_of(raw, "weather")));
    }
    if w == "weather" && (n1 == "or" && n2 == "not" || (n1 == "to" && BASE_VERBS.contains(&cx.nx(i, 2)))) {
        flag(cx, out, wc, "weather-whether", "Weather / Whether", i, i,
            "'weather' is about rain and sun. To express a choice use 'whether'.",
            Some(with_case_of(raw, "whether")));
    }

    // lose / loose
    if w == "loose" && matches!(p, "to" | "will" | "would" | "could" | "can" | "can't" | "don't"
        | "didn't" | "won't" | "might" | "may" | "shall" | "not" | "never" | "to") && !matches!(n1, "the" if false)
    {
        flag(cx, out, wc, "lose-loose", "Lose / Loose", i, i,
            "'loose' means 'not tight'. The verb is 'lose'.", Some(with_case_of(raw, "lose")));
    }
    if w == "lose" && matches!(p, "too" | "very" | "so" | "quite" | "rather") {
        flag(cx, out, wc, "lose-loose", "Lose / Loose", i, i,
            "'lose' is a verb. The adjective is 'loose'.", Some(with_case_of(raw, "loose")));
    }

    // passed / past
    if w == "past" && matches!(p, "i" | "he" | "she" | "they" | "we" | "it" | "has" | "have" | "had"
        | "was" | "were" | "been" | "be" | "will" | "would" | "could" | "can" | "to" | "just")
        && !matches!(p, "to" | "was" | "were" | "be" | "been" if matches!(n1, "the" if false))
        && !(matches!(p, "was" | "were" | "be" | "been") && !matches!(n1, "the" | "a" | "an" | "me" | "him" | "her" | "us" | "them" | "my" | "his" | "their" | "our"))
    {
        flag(cx, out, wc, "passed-past", "Passed / Past", i, i,
            "'past' is a time or a place. The verb form is 'passed'.", Some(with_case_of(raw, "passed")));
    }
    if w == "passed" && matches!(p, "the" | "in" | "a" | "for" | "over") && matches!(n1, "few" | "several"
        | "year" | "years" | "decade" | "decades" | "week" | "weeks" | "month" | "months" | "century"
        | "centuries" | "week")
    {
        flag(cx, out, wc, "passed-past", "Passed / Past", i, i,
            "'passed' is a verb. For time gone by, use 'past'.", Some(with_case_of(raw, "past")));
    }

    // affect / effect, adverse / averse
    if w == "affect" && n1 == "on" && matches!(p, "an" | "the" | "no" | "any" | "big" | "negative"
        | "positive" | "bad" | "great" | "huge" | "major" | "little" | "strong" | "a" | "good" | "lasting")
    {
        flag(cx, out, wc, "affect-effect", "Affect / Effect", i, i,
            "As a noun the word is 'effect'.", Some(with_case_of(raw, "effect")));
    }
    if matches!(w, "effect" | "effects" | "effected" | "effecting")
        && OBJ_PRON.contains(&n1) && n1 != "it"
        && !matches!(p, "the" | "an" | "a" | "no" | "any" | "its" | "their" | "his" | "her" | "my"
            | "your" | "our" | "this" | "that" | "side" | "ill" | "to")
    {
        let fix = match w {
            "effect" => "affect",
            "effects" => "affects",
            "effected" => "affected",
            _ => "affecting",
        };
        flag(cx, out, wc, "affect-effect", "Affect / Effect", i, i,
            "As a verb meaning 'to influence' the word is 'affect'.", Some(with_case_of(raw, fix)));
    }
    if w == "averse" && matches!(n1, "effect" | "effects" | "reaction" | "conditions" | "weather"
        | "impact" | "consequences" | "event" | "events" | "side")
    {
        flag(cx, out, wc, "adverse-averse", "Adverse / Averse", i, i,
            "'averse' means 'opposed'. Harmful is 'adverse'.", Some(with_case_of(raw, "adverse")));
    }
    if w == "disinterested" && n1 == "in" {
        flag(cx, out, wc, "disinterested", "Disinterested / Uninterested", i, i,
            "'disinterested' means 'impartial'. 'Not interested' is 'uninterested'.",
            Some(with_case_of(raw, "uninterested")));
    }

    // advice / advise
    if w == "advice" && matches!(p, "i" | "we" | "to" | "would" | "will" | "can" | "could" | "should") {
        flag(cx, out, wc, "advice-advise", "Advice / Advise", i, i,
            "'advice' is a noun. The verb is 'advise'.", Some(with_case_of(raw, "advise")));
    }
    if w == "advise" && matches!(p, "some" | "good" | "bad" | "my" | "your" | "his" | "her"
        | "their" | "an" | "any" | "no" | "this" | "that" | "the" | "little" | "sound" | "legal"
        | "medical" | "expert" | "great" | "valuable" | "helpful" | "free")
    {
        flag(cx, out, wc, "advice-advise", "Advice / Advise", i, i,
            "'advise' is a verb. The noun is 'advice'.", Some(with_case_of(raw, "advice")));
    }

    // lightning, pour/pore, tough/though, wreck/wreak
    if w == "lighting" && (matches!(n1, "strike" | "strikes" | "struck" | "bolt" | "bolts" | "storm"
        | "flashed" | "flash" | "rod" | "hit")
        || (p == "of" && matches!(cx.pv(i.saturating_sub(1)), "flash" | "bolt" | "flashes" | "bolts" | "crack")))
    {
        flag(cx, out, wc, "lightning", "Lightning / Lighting", i, i,
            "The flash in the sky is 'lightning'.", Some(with_case_of(raw, "lightning")));
    }
    if matches!(w, "pore" | "pores" | "pored" | "poring") && matches!(n1, "from" | "down" | "out") {
        let fix = match w {
            "pore" => "pour",
            "pores" => "pours",
            "pored" => "poured",
            _ => "pouring",
        };
        flag(cx, out, wc, "pour-pore", "Pour / Pore", i, i,
            "Liquid and words 'pour'. To 'pore over' is to study closely.", Some(with_case_of(raw, fix)));
    }
    if w == "tough" && (p == "even" || (n1 == "that" && matches!(n2, "may" | "might" | "seems" | "is" | "was"))) {
        flag(cx, out, wc, "tough-though", "Tough / Though", i, i,
            "'tough' means 'hard'. The contrast word is 'though'.", Some(with_case_of(raw, "though")));
    }
    if w == "wreak" && matches!(p, "a" | "an" | "the" | "complete" | "total" | "burning"
        | "smouldering" | "smoldering" | "nervous" | "emotional" | "utter" | "physical" | "perfect")
    {
        flag(cx, out, wc, "wreck-wreak", "Wreck / Wreak", i, i,
            "A ruined thing is a 'wreck'. 'Wreak' goes with 'havoc'.", Some(with_case_of(raw, "wreck")));
    }
    if matches!(w, "wreck" | "wrecks" | "wrecked" | "wrecking") && n1 == "havoc" {
        let fix = match w {
            "wreck" => "wreak",
            "wrecks" => "wreaks",
            "wrecked" => "wreaked",
            _ => "wreaking",
        };
        flag(cx, out, wc, "wreck-wreak", "Wreck / Wreak", i, i,
            "The idiom is 'wreak havoc'.", Some(with_case_of(raw, fix)));
    }

    // alternate / among / beside / deathly / hardly / breathe / chose
    if w == "alternate" && (n1 == "to" || (cx.clause_end(i) && matches!(p, "a" | "an" | "the"
        | "healthier" | "better" | "good" | "great" | "viable" | "possible" | "only" | "another"
        | "cheaper" | "safer" | "easier")))
    {
        flag(cx, out, wc, "alternative", "Alternate / Alternative", i, i,
            "As a noun meaning 'another choice' the word is 'alternative'.",
            Some(with_case_of(raw, "alternative")));
    }
    if w == "among" && (matches!(n1, "two" | "both") || (matches!(n1, "the" | "these" | "those")
        && matches!(n2, "two" | "both" | "pair")))
    {
        let last = if matches!(n1, "two" | "both") { i + 1 } else { i + 2 };
        let tail: Vec<&str> = (i + 1..=last).map(|k| cx.raw(k)).collect();
        flag(cx, out, wc, "between-among", "Between / Among", i, last,
            "With two things use 'between'. 'Among' is for three or more.",
            Some(format!("{} {}", with_case_of(raw, "between"), tail.join(" "))));
    }
    if w == "besides" && matches!(p, "sit" | "sits" | "sat" | "sitting" | "stand" | "stands" | "stood"
        | "standing" | "lie" | "lies" | "lay" | "lying" | "seated" | "placed" | "put" | "walk"
        | "walked" | "walking" | "next" | "right" | "close")
    {
        flag(cx, out, wc, "beside-besides", "Beside / Besides", i, i,
            "'beside' means 'next to'; 'besides' means 'in addition'.", Some(with_case_of(raw, "beside")));
    }
    if w == "deathly" && matches!(p, "be" | "is" | "are" | "was" | "were" | "been" | "can" | "could"
        | "may" | "might" | "become" | "very" | "too") && (cx.clause_end(i) || matches!(n1, "to" | "for" | "if" | "when" | "and" | "but" | "because"))
    {
        flag(cx, out, wc, "deadly-deathly", "Deadly / Deathly", i, i,
            "Something that can kill is 'deadly'; 'deathly' goes with pale, silent, afraid.",
            Some(with_case_of(raw, "deadly")));
    }
    if w == "hardly" && (matches!(p, "a" | "an" | "the") || (cx.clause_end(i) && matches!(p, "work"
        | "works" | "worked" | "working" | "try" | "tries" | "tried" | "trying" | "study"
        | "studied" | "studies" | "studying" | "push" | "pushed" | "hit" | "play" | "played")))
    {
        flag(cx, out, wc, "hard-hardly", "Hard / Hardly", i, i,
            "'hardly' means 'barely'. Here 'hard' is meant.", Some(with_case_of(raw, "hard")));
    }
    if w == "breath" && matches!(p, "to" | "can't" | "cannot" | "can" | "couldn't" | "could" | "will"
        | "would" | "won't" | "don't" | "didn't" | "must" | "should" | "help" | "let" | "make")
    {
        flag(cx, out, wc, "breath-breathe", "Breath / Breathe", i, i,
            "The verb is 'breathe'; 'breath' is the noun.", Some(with_case_of(raw, "breathe")));
    }
    if w == "breathe" && matches!(p, "a" | "deep" | "last" | "his" | "her" | "my" | "your" | "their"
        | "our" | "the" | "one" | "each" | "every" | "short" | "long" | "fresh" | "bad" | "same")
    {
        flag(cx, out, wc, "breath-breathe", "Breath / Breathe", i, i,
            "The noun is 'breath'; 'breathe' is the verb.", Some(with_case_of(raw, "breath")));
    }
    if w == "chose" && matches!(p, "to" | "will" | "would" | "can" | "could" | "should" | "must" | "may"
        | "might" | "didn't" | "don't" | "doesn't" | "did" | "do" | "does")
    {
        flag(cx, out, wc, "choose-chose", "Choose / Chose", i, i,
            "After this word the verb stays in its base form: 'choose'.", Some(with_case_of(raw, "choose")));
    }
    if w == "accept" && n1 == "for" && !matches!(p, "to" | "will" | "would" | "can" | "i" | "we" | "they") {
        flag(cx, out, wc, "accept-except", "Accept / Except", i, i,
            "'except for' excludes; 'accept' means 'receive'.", Some(with_case_of(raw, "except")));
    }
    if w == "quite" && matches!(n1, "down") {
        flag(cx, out, wc, "quiet-quite", "Quiet / Quite", i, i,
            "'quite' means 'rather'. The word for silence is 'quiet'.", Some(with_case_of(raw, "quiet")));
    }

    // awhile / a while, everyday / every day
    if w == "awhile" && matches!(p, "for" | "in" | "after" | "once") {
        flag(cx, out, wc, "a-while", "A While / Awhile", i, i,
            "After a preposition write 'a while' as two words.", Some(with_case_of(raw, "a while")));
    }
    if w == "everyday" && cx.clause_end(i) {
        flag(cx, out, wc, "everyday", "Everyday / Every Day", i, i,
            "'everyday' is an adjective. For 'each day' use two words.",
            Some(with_case_of(raw, "every day")));
    }
    if w == "every" && n1 == "day" && matches!(p, "his" | "her" | "their" | "my" | "our" | "your"
        | "its" | "the") && (looks_plural(n2) || matches!(n2, "life" | "routine" | "use" | "wear"
        | "clothes" | "work"))
    {
        flag(cx, out, wc, "everyday", "Everyday / Every Day", i, i + 1,
            "Before a noun, 'everyday' is one word.", Some("everyday".to_string()));
    }

    // used to / suppose to
    if w == "suppose" && n1 == "to" && matches!(p, "am" | "i'm" | "is" | "are" | "was" | "were"
        | "be" | "been" | "he's" | "she's" | "it's" | "you're" | "we're" | "they're" | "that's")
    {
        flag(cx, out, wc, "supposed-to", "Supposed To", i, i,
            "The phrase is 'supposed to'.", Some(with_case_of(raw, "supposed")));
    }
    if w == "use" && n1 == "to" && matches!(p, "i" | "we" | "they" | "he" | "she" | "it" | "you" | "who")
        && BASE_VERBS.contains(&n2)
    {
        flag(cx, out, wc, "used-to", "Used To", i, i,
            "For a past habit the phrase is 'used to'.", Some(with_case_of(raw, "used")));
    }
}

// ---------------------------------------------------------------------------------------------
// Agreement
// ---------------------------------------------------------------------------------------------

fn agreement(cx: &Cx, i: usize, out: &mut Out) {
    let w = cx.lw(i);
    let raw = cx.raw(i);
    let n1 = cx.nx(i, 1);
    let n2 = cx.nx(i, 2);
    let p = cx.pv(i);
    let sv = Category::SubjectVerb;

    // every / each + plural noun
    if matches!(w, "every" | "each") && looks_plural(n1) && !(w == "each" && VERB_S.contains(&n1))
        && !matches!(n1, "other" | "others") || (matches!(w, "every" | "each") && is_irregular_plural(n1)
        && matches!(n1, "children" | "men" | "women" | "people" | "feet" | "teeth" | "mice")
        && n1 != "people")
    {
        if let Some(s) = singular_of(n1) {
            flag(cx, out, sv, "every-plural", "Subject-Verb Agreement", i, i + 1,
                format!("'{raw}' goes with a singular noun: '{s}'."),
                Some(format!("{raw} {s}")));
        }
    }
    // every/each + singular noun + plural verb
    if matches!(w, "every" | "each") && !n1.is_empty() && !looks_plural(n1)
        && !matches!(n1, "of" | "other" | "one" | "and" | "or" | "but" | "which" | "who" | "that"
            | "time" | "is" | "are")
        && plural_verb_to_singular(n2).is_some() && !is_irregular_plural(n1)
    {
        if let Some(fix) = plural_verb_to_singular(n2) {
            flag(cx, out, sv, "every-verb", "Subject-Verb Agreement", i + 2, i + 2,
                format!("'{raw} {n1}' is singular, so the verb should be '{fix}'."),
                Some(fix.to_string()));
        }
    }
    // everyone / nobody / something + plural verb
    if matches!(w, "everyone" | "everybody" | "everything" | "someone" | "somebody" | "something"
        | "anyone" | "anybody" | "nobody" | "nothing" | "anything")
        && matches!(n1, "are" | "were" | "have")
        && !(n1 == "have" && matches!(p, "let" | "lets" | "make" | "makes" | "made" | "help"
            | "helps" | "get" | "got" | "gets" | "tell" | "told" | "ask" | "asked" | "to" | "that"
            | "see" | "watch" | "hear" | "heard" | "saw" | "want" | "wants" | "wanted" | "have"
            | "if" | "so" | "and" | "or" | "for" | "than"))
        && !(matches!(n1, "are" | "were") && matches!(p, "let" | "make" | "made" | "if" | "that" | "than"))
    {
        if let Some(fix) = plural_verb_to_singular(n1) {
            flag(cx, out, sv, "indefinite-pronoun", "Subject-Verb Agreement", i + 1, i + 1,
                format!("'{raw}' is singular, so the verb should be '{fix}'."),
                Some(fix.to_string()));
        }
    }
    // collective plurals + singular verb
    if matches!(w, "police" | "people" | "children" | "men" | "women" | "cattle")
        && matches!(n1, "is" | "was" | "has" | "doesn't" | "does")
        && !matches!(p, "what" | "how" | "where" | "who" | "when" | "why")
    {
        if let Some(fix) = singular_verb_to_plural(n1) {
            flag(cx, out, sv, "plural-collective", "Subject-Verb Agreement", i + 1, i + 1,
                format!("'{raw}' is plural, so the verb should be '{fix}'."), Some(fix.to_string()));
        }
    }
    // mathematics are
    if matches!(w, "mathematics" | "physics" | "news" | "economics" | "linguistics" | "genetics"
        | "gymnastics" | "measles" | "mumps") && matches!(n1, "are" | "were" | "have")
    {
        if let Some(fix) = plural_verb_to_singular(n1) {
            flag(cx, out, sv, "singular-s-noun", "Subject-Verb Agreement", i + 1, i + 1,
                format!("'{raw}' takes a singular verb: '{fix}'."), Some(fix.to_string()));
        }
    }
    // either X or Y + plural verb; neither X nor Y + plural verb
    if matches!(w, "either" | "neither") {
        let conj = if w == "either" { "or" } else { "nor" };
        if let Some(j) = cx.find_after(i, &[conj], 5) {
            let mut y = j + 1;
            if cx.adj(j) && matches!(cx.lw(y), "the" | "a" | "an" | "his" | "her" | "my" | "their") {
                y += 1;
            }
            let yw = cx.lw(y);
            let verb = if cx.adj(y.saturating_sub(1)) && y > j { cx.nx(y, 1) } else { "" };
            if !yw.is_empty() && cx.adj(j) && !looks_plural(yw) && !is_irregular_plural(yw)
                && !matches!(yw, "you" | "they" | "we" | "i" | "them" | "us" | "those" | "these" | "both")
            {
                if let Some(fix) = plural_verb_to_singular(verb) {
                    flag(cx, out, sv, "either-or", "Subject-Verb Agreement", y + 1, y + 1,
                        format!("After '{w} … {conj}' the verb agrees with '{yw}': '{fix}'."),
                        Some(fix.to_string()));
                }
            }
        }
    }
    // "X as well as Y are"
    if w == "as" && n1 == "well" && n2 == "as" && !p.is_empty() && !looks_plural(p)
        && !is_irregular_plural(p) && !SUBJ_PRON.contains(&p)
        && !matches!(p, "much" | "good" | "well" | "so" | "just" | "also" | "both" | "and" | "or")
        && !cx.clause_has_before(i, &["and", "both", "or", "two", "three", "four", "these", "those"], 6)
        && !SUBJ_PRON.contains(&cx.nx(i, 3))
    {
        if let Some(j) = cx.find_after(i + 2, &["are", "were", "have"], 4) {
            if cx.lw(j) != "have" || true {
                if let Some(fix) = plural_verb_to_singular(cx.lw(j)) {
                    flag(cx, out, sv, "as-well-as", "Subject-Verb Agreement", j, j,
                        format!("The subject '{p}' is singular, so the verb should be '{fix}'."),
                        Some(fix.to_string()));
                }
            }
        }
    }
    // one of the + singular noun + is
    if w == "one" && n1 == "of" && matches!(n2, "the" | "these" | "those" | "my" | "his" | "her"
        | "our" | "your" | "their")
    {
        let noun = cx.nx(i, 3);
        let verb = cx.nx(i, 4);
        if !noun.is_empty() && matches!(verb, "is" | "was" | "has" | "does") && !noun.ends_with('s')
            && !is_irregular_plural(noun) && !noun.ends_with("est") && !matches!(noun, "few" | "most"
            | "first" | "last" | "only" | "rest" | "best" | "worst" | "other" | "same" | "many"
            | "main" | "two" | "three" | "four" | "five" | "ones" | "men")
            && !noun.ends_with("ing") && !noun.ends_with("ed")
        {
            flag(cx, out, sv, "one-of-the", "Plural Noun", i + 3, i + 3,
                format!("After 'one of the' the noun is plural: '{}'.", pluralize(noun)),
                Some(pluralize(noun)));
        }
    }
    // each of the X are
    if matches!(w, "each" | "everyone" | "everybody") && n1 == "of" {
        let mut j = i + 1;
        for _ in 0..5 {
            if !cx.adj(j) {
                break;
            }
            j += 1;
            let t = cx.lw(j);
            if matches!(t, "who" | "that" | "which" | "and" | "but" | "or" | "if" | "when" | "because"
                | "while") {
                break;
            }
            if matches!(t, "are" | "were" | "have" | "don't" | "do") {
                if let Some(fix) = plural_verb_to_singular(t) {
                    flag(cx, out, sv, "each-of", "Subject-Verb Agreement", j, j,
                        format!("'{raw} of …' is singular, so the verb should be '{fix}'."),
                        Some(fix.to_string()));
                }
                break;
            }
            if matches!(t, "is" | "was" | "has" | "does" | "doesn't") {
                break;
            }
        }
    }
    // there is + plural quantity
    if matches!(w, "there" | "there's") {
        let (is_tok, q) = if w == "there's" { (i, n1) } else { (i + 1, n2) };
        let verb_ok = w == "there's" || matches!(n1, "is" | "was");
        if verb_ok && (NUM_WORDS.contains(&q) || matches!(q, "many" | "several" | "numerous" | "few"
            | "both")) && !matches!(q, "hundred")
        {
            let (fix, from, to) = if w == "there's" {
                ("there are", i, i)
            } else if n1 == "was" {
                ("there were", i, i + 1)
            } else {
                ("there are", i, i + 1)
            };
            let _ = is_tok;
            flag(cx, out, sv, "there-is-plural", "Subject-Verb Agreement", from, to,
                "A plural quantity follows, so the verb should be plural.",
                Some(with_case_of(raw, fix)));
        }
    }
    // he need / they goes
    if matches!(w, "he" | "she") && BASE_VERBS.contains(&n1) && !matches!(n1, "like" | "read" | "set"
        | "lead" | "cut" | "put")
        && !matches!(p, "that" | "lest" | "let" | "make" | "makes" | "made" | "help" | "helps"
            | "see" | "saw" | "watch" | "hear" | "heard" | "have" | "had" | "get" | "got" | "to"
            | "can" | "could" | "will" | "would" | "should" | "may" | "might" | "must" | "shall"
            | "did" | "does" | "do" | "didn't" | "doesn't" | "don't" | "if" | "whether" | "how"
            | "what" | "where" | "when" | "why" | "who" | "wish" | "demand" | "insist" | "suggest"
            | "recommend" | "require" | "ask" | "request" | "than" | "as")
        && !p.ends_with("n't")
        && !cx.clause_has_before(i, &["did", "does", "do", "can", "could", "will", "would", "should",
            "may", "might", "must", "shall", "let", "make", "made", "demand", "insist", "suggest",
            "recommend", "require", "request", "wish"], 3)
    {
        let fixed = match n1 {
            "go" => "goes".to_string(),
            "do" => "does".to_string(),
            "have" => "has".to_string(),
            "try" => "tries".to_string(),
            "carry" => "carries".to_string(),
            "study" => "studies".to_string(),
            v if v.ends_with(['s', 'x', 'z']) || v.ends_with("ch") || v.ends_with("sh") => format!("{v}es"),
            v => format!("{v}s"),
        };
        flag(cx, out, sv, "he-base-verb", "Subject-Verb Agreement", i, i + 1,
            format!("After '{raw}' the verb needs an -s: '{fixed}'."),
            Some(format!("{raw} {fixed}")));
    }
    if matches!(w, "i" | "we" | "they") && VERB_S_SAFE.contains(&n1) && !matches!(n1, "does" | "has")
        && !cx.starts_clause(i.saturating_add(0)) | true
        && !matches!(p, "that" if false)
    {
        let base = match n1 {
            "goes" => "go".to_string(),
            "does" => "do".to_string(),
            "has" => "have".to_string(),
            v if v.ends_with("ies") => format!("{}y", &v[..v.len() - 3]),
            v if v.ends_with("ches") || v.ends_with("shes") || v.ends_with("sses") || v.ends_with("xes") => {
                v[..v.len() - 2].to_string()
            }
            v => v[..v.len() - 1].to_string(),
        };
        flag(cx, out, sv, "pronoun-verb-s", "Subject-Verb Agreement", i, i + 1,
            format!("After '{raw}' the verb has no -s: '{base}'."),
            Some(format!("{raw} {base}")));
    }
}

// ---------------------------------------------------------------------------------------------
// Quantity words
// ---------------------------------------------------------------------------------------------

const UNCOUNTABLE: &[&str] = &[
    "money", "information", "advice", "water", "furniture", "equipment", "homework", "traffic",
    "news", "luggage", "rain", "bread", "rice", "milk", "sugar", "salt", "oil", "work", "research",
    "knowledge", "weather", "music", "time", "food", "paper",
];
const COUNTABLE_PLURALS: &[&str] = &[
    "people", "friends", "children", "books", "cars", "students", "things", "times", "days",
    "years", "words", "problems", "questions", "kids", "items", "dollars", "mistakes", "ideas",
    "houses", "people", "women", "men", "boys", "girls",
];
const COMMON_NOUNS: &[&str] = &[
    "book", "car", "boy", "girl", "friend", "thing", "student", "house", "dog", "cat", "tree",
    "apple", "flower", "shirt", "dress", "problem", "question", "mistake", "idea", "child", "man",
    "woman", "person", "teacher", "road", "town", "city", "chair", "table", "bird", "horse",
    "story", "sentence", "letter", "picture", "window", "door", "player", "doctor", "farmer",
];

fn quantity(cx: &Cx, i: usize, out: &mut Out) {
    let w = cx.lw(i);
    let raw = cx.raw(i);
    let n1 = cx.nx(i, 1);
    let n2 = cx.nx(i, 2);
    let p = cx.pv(i);
    let wc = Category::WordChoice;
    let sv = Category::SubjectVerb;

    // the number of ... are / a number of ... is
    if w == "number" && n1 == "of" && matches!(p, "the" | "greater" | "growing" | "total" | "increasing") {
        if let Some(j) = cx.find_after(i + 1, &["are", "were", "have"], 5) {
            if !cx.clause_has_before(i, &["a"], 2) {
                if let Some(fix) = plural_verb_to_singular(cx.lw(j)) {
                    let between_ok = (i + 2..j).all(|k| !matches!(cx.lw(k), "who" | "that" | "which" | "and" | "or"));
                    if between_ok {
                        flag(cx, out, sv, "number-of", "Subject-Verb Agreement", j, j,
                            format!("'The number of …' is singular, so the verb should be '{fix}'."),
                            Some(fix.to_string()));
                    }
                }
            }
        }
    }
    if w == "number" && n1 == "of" && p == "a" {
        if let Some(j) = cx.find_after(i + 1, &["is", "was", "has"], 4) {
            let between_ok = (i + 2..j).all(|k| !matches!(cx.lw(k), "who" | "that" | "which" | "and" | "or"));
            if between_ok {
                if let Some(fix) = singular_verb_to_plural(cx.lw(j)) {
                    flag(cx, out, sv, "number-of", "Subject-Verb Agreement", j, j,
                        format!("'A number of …' is plural, so the verb should be '{fix}'."),
                        Some(fix.to_string()));
                }
            }
        }
    }
    // amount of + countable
    if w == "amount" && n1 == "of" && (COUNTABLE_PLURALS.contains(&n2)
        || (looks_plural(n2) && !matches!(n2, "dollars" | "calories" | "pounds" | "euros")
            && !n2.ends_with("ings") && !matches!(n2, "savings" | "earnings" | "goods" | "clothes")))
        && !matches!(n2, "dollars" | "money")
    {
        flag(cx, out, wc, "amount-number", "Amount / Number", i, i + 1,
            format!("'{n2}' can be counted, so use 'number of', not 'amount of'."),
            Some(format!("{} of", with_case_of(raw, "number"))));
    }
    // less + plural / fewer + mass
    if w == "less" && n1 != "than" && (COUNTABLE_PLURALS.contains(&n1) || looks_plural(n1))
        && !matches!(n1, "its" | "is" | "was" | "has" | "this" | "less" | "sales" | "savings" | "bills"
            | "taxes" | "politics" | "thanks" | "sometimes" | "dollars" | "years" | "miles"
            | "hours" | "minutes" | "days" | "weeks" | "months" | "pounds" | "words" | "feet")
        && !VERB_S.contains(&n1) && !n1.ends_with("ing") && !n1.ends_with("ly")
        && !matches!(p, "much" | "far" | "even" | "a" | "rather" | "no" | "any" | "of")
    {
        flag(cx, out, wc, "fewer-less", "Fewer / Less", i, i,
            format!("'{n1}' can be counted, so use 'fewer'."), Some(with_case_of(raw, "fewer")));
    }
    if w == "fewer" && UNCOUNTABLE.contains(&n1) {
        flag(cx, out, wc, "fewer-less", "Fewer / Less", i, i,
            format!("'{n1}' cannot be counted, so use 'less'."), Some(with_case_of(raw, "less")));
    }
    // many / much
    if w == "many" && matches!(n1, "money" | "information" | "advice" | "water" | "furniture"
        | "equipment" | "homework" | "traffic" | "luggage" | "rain" | "knowledge")
    {
        flag(cx, out, wc, "many-much", "Many / Much", i, i,
            format!("'{n1}' cannot be counted, so use 'much'."), Some(with_case_of(raw, "much")));
    }
    if w == "much" && matches!(n1, "people" | "friends" | "children" | "books" | "cars" | "students"
        | "things" | "times" | "words" | "problems" | "questions" | "kids" | "items" | "dollars"
        | "mistakes" | "ideas" | "houses" | "women" | "men" | "boys" | "girls")
        && !matches!(p, "how" | "as" | "so" | "too")
        || (w == "much" && matches!(p, "how" | "as" | "so" | "too") && matches!(n1, "people" | "friends"
            | "children" | "books" | "cars" | "students" | "things" | "kids" | "items" | "mistakes"
            | "ideas" | "houses" | "women" | "men" | "boys" | "girls"))
    {
        flag(cx, out, wc, "many-much", "Many / Much", i, i,
            format!("'{n1}' can be counted, so use 'many'."), Some(with_case_of(raw, "many")));
    }
    // two + singular noun
    if (NUM_WORDS.contains(&w) && w != "hundred" || matches!(w, "several" | "many"))
        && COMMON_NOUNS.contains(&n1)
        && !matches!(p, "a" | "an" | "every" | "each" | "no" | "one" | "another" | "the" | "of")
        && !(w == "many" && matches!(p, "so" | "too" | "how" | "as"))
    {
        flag(cx, out, wc, "number-noun", "Plural Noun", i + 1, i + 1,
            format!("After '{w}' the noun is plural: '{}'.", pluralize(n1)), Some(pluralize(n1)));
    }
    // this / these
    if w == "this" && matches!(n1, "books" | "men" | "women" | "children" | "things" | "days"
        | "students" | "cars" | "houses" | "ideas" | "boys" | "girls" | "kids" | "problems"
        | "questions" | "mistakes" | "friends")
    {
        flag(cx, out, wc, "this-these", "This / These", i, i,
            format!("'{n1}' is plural, so use 'these'."), Some(with_case_of(raw, "these")));
    }
    if matches!(w, "these" | "those") && matches!(n1, "kind" | "sort" | "type") && n2 == "of" {
        let sing = if w == "these" { "this" } else { "that" };
        flag(cx, out, wc, "these-kind", "These Kind Of", i, i + 1,
            format!("'{raw} {n1} of' mixes plural and singular. Use '{sing} {n1} of' or '{w} {n1}s of'."),
            Some(format!("{w} {n1}s")));
        let _ = sing;
    }
    if matches!(w, "these" | "those") && COMMON_NOUNS.contains(&n1)
        && !matches!(n1, "man" | "woman" | "child" | "person")
    {
        flag(cx, out, wc, "this-these", "This / These", i + 1, i + 1,
            format!("After '{w}' the noun is plural: '{}'.", pluralize(n1)), Some(pluralize(n1)));
    }
    // these ... over there
    if w == "these" {
        if let Some(j) = cx.find_after(i, &["over"], 3) {
            if cx.nx(j, 1) == "there" {
                flag(cx, out, wc, "these-those", "These / Those", i, i,
                    "Things that are far away are 'those'.", Some(with_case_of(raw, "those")));
            }
        }
    }
    if w == "those" {
        if let Some(j) = cx.find_after(i, &["over"], 3) {
            if cx.nx(j, 1) == "here" {
                flag(cx, out, wc, "these-those", "These / Those", i, i,
                    "Things that are close are 'these'.", Some(with_case_of(raw, "these")));
            }
        }
    }
    // a + plural noun
    if matches!(w, "a" | "an") && matches!(n1, "books" | "cars" | "boys" | "girls" | "friends"
        | "things" | "students" | "houses" | "dogs" | "cats" | "trees" | "apples" | "flowers"
        | "shirts" | "dresses" | "problems" | "questions" | "mistakes" | "ideas")
    {
        if let Some(s) = singular_of(n1) {
            flag(cx, out, wc, "a-plural", "Singular Noun", i, i + 1,
                format!("'{raw}' goes with a singular noun: '{s}'."), Some(format!("{raw} {s}")));
        }
    }
    // most / more + unique
    if matches!(w, "most" | "more" | "very") && n1 == "unique" {
        flag(cx, out, wc, "most-unique", "Absolute Adjective", i, i + 1,
            "'Unique' means one of a kind, so it cannot be more or less so.",
            Some("unique".to_string()));
    }
    // more + short adjective + than, more + comparative
    const SHORT_ADJ: &[(&str, &str)] = &[
        ("tall", "taller"), ("short", "shorter"), ("big", "bigger"), ("small", "smaller"),
        ("fast", "faster"), ("slow", "slower"), ("long", "longer"), ("old", "older"),
        ("young", "younger"), ("high", "higher"), ("low", "lower"), ("cheap", "cheaper"),
        ("strong", "stronger"), ("weak", "weaker"), ("hot", "hotter"), ("cold", "colder"),
        ("warm", "warmer"), ("nice", "nicer"), ("near", "nearer"), ("quick", "quicker"),
        ("thick", "thicker"), ("thin", "thinner"), ("deep", "deeper"), ("wide", "wider"),
        ("smart", "smarter"), ("dark", "darker"), ("clean", "cleaner"), ("kind", "kinder"),
        ("fat", "fatter"), ("sad", "sadder"), ("happy", "happier"), ("pretty", "prettier"),
        ("easy", "easier"), ("early", "earlier"), ("funny", "funnier"), ("busy", "busier"),
    ];
    if w == "more" && n2 == "than" {
        if let Some(&(_, c)) = SHORT_ADJ.iter().find(|(a, _)| *a == n1) {
            flag(cx, out, wc, "more-short-adjective", "Comparative", i, i + 1,
                format!("For a short adjective use '{c}', not 'more {n1}'."), Some(c.to_string()));
        }
    }
    const COMPARATIVES: &[&str] = &[
        "taller", "shorter", "bigger", "smaller", "faster", "slower", "longer", "older", "younger",
        "higher", "lower", "cheaper", "stronger", "weaker", "hotter", "colder", "warmer", "nicer",
        "better", "worse", "easier", "harder", "happier", "larger", "greater", "smarter",
        "tallest", "shortest", "funniest", "funnier", "prettier", "prettiest", "busier", "busiest", "biggest", "smallest", "fastest", "slowest", "longest", "oldest",
        "youngest", "highest", "lowest", "cheapest", "strongest", "best", "worst", "easiest",
        "happiest", "largest", "greatest", "smartest",
    ];
    if matches!(w, "more" | "most") && COMPARATIVES.contains(&n1) {
        flag(cx, out, wc, "double-comparative", "Comparative", i, i + 1,
            format!("'{n1}' already means 'more'; drop '{w}'."), Some(n1.to_string()));
    }
}

// ---------------------------------------------------------------------------------------------
// Verbs and tense
// ---------------------------------------------------------------------------------------------

fn verbs(cx: &Cx, i: usize, out: &mut Out) {
    let w = cx.lw(i);
    let raw = cx.raw(i);
    let n1 = cx.nx(i, 1);
    let n2 = cx.nx(i, 2);
    let n3 = cx.nx(i, 3);
    let p = cx.pv(i);
    let tn = Category::Tense;

    // did not + past form
    if matches!(w, "did" | "didn't" | "does" | "doesn't" | "do" | "don't") && !matches!(p, "what" | "that" | "which") {
        let (idx, x) = if n1 == "not" {
            (i + 2, n2)
        } else if SUBJ_PRON.contains(&n1) {
            (i + 2, n2)
        } else {
            (i + 1, n1)
        };
        let irregular = IRREG_PAST.iter().find(|(past, _)| *past == x);
        let ed = matches!(w, "did" | "didn't")
            && x.chars().count() > 4 && x.ends_with("ed") && !x.ends_with("eed")
            && !matches!(x, "needed" | "wanted" if false);
        if let Some(&(_, base)) = irregular {
            if !matches!(x, "had" | "left" | "got" | "said" | "found" | "felt" | "kept" | "held" | "heard"
                | "told" | "paid" | "sent" | "sold" | "built" | "spent" | "lost" | "met" | "sat" | "stood"
                if matches!(w, "do" | "does" | "don't" | "doesn't") && false) {
                flag(cx, out, tn, "did-past", "Verb Form", idx, idx,
                    format!("After '{w}' the verb stays in its base form: '{base}', not '{x}'."),
                    Some(base.to_string()));
            }
        } else if ed && !matches!(x, "advanced" | "limited" | "required" | "needed" if false) {
            flag(cx, out, tn, "did-past", "Verb Form", idx, idx,
                format!("After '{w}' the verb stays in its base form, not '{x}'."), None);
        }
    }
    // modal + past form / verb-s
    if matches!(w, "can" | "could" | "will" | "would" | "should" | "must" | "may" | "might" | "shall") {
        if let Some(&(past, base)) = IRREG_PAST.iter().find(|(past, _)| *past == n1) {
            if !matches!(past, "had" | "left" | "got" | "said" | "found" | "felt" | "kept" | "held"
                | "told" | "paid" | "sent" | "sold" | "built" | "spent" | "lost" | "met" | "sat"
                | "stood" | "heard" | "made" | "thought" | "taught" | "caught" | "fell" | "broke" | "wore"
                | "ate" | "saw" | "knew" | "took" | "gave" | "wrote" | "spoke" | "chose" | "drank"
                | "sang" | "swam" | "threw" | "began" | "forgot" | "flew" | "drove" | "bought"
                | "brought" | "understood") || matches!(past, "went" | "came" | "took" | "gave"
                | "ate" | "wrote" | "saw" | "knew" | "began" | "drove" | "flew" | "spoke" | "chose"
                | "drank" | "sang" | "swam" | "threw" | "wore" | "broke" | "forgot")
            {
                // "will/can left" can be nouns ("the can left") only for left/got/found: skipped above
                if matches!(past, "went" | "came" | "took" | "gave" | "ate" | "wrote" | "saw" | "knew"
                    | "began" | "drove" | "flew" | "spoke" | "chose" | "drank" | "sang" | "swam"
                    | "threw" | "wore" | "broke" | "forgot" | "bought" | "brought" | "understood" | "thought")
                    && !(w == "can" && matches!(past, "saw")) && !(w == "will" && matches!(past, "wore" | "thought"))
                    && !matches!((w, past), ("may", "wore") | ("will", "saw") | ("might", "saw"))
                {
                    flag(cx, out, tn, "modal-past", "Verb Form", i, i + 1,
                        format!("After '{w}' the verb stays in its base form: '{base}'."),
                        Some(format!("{raw} {base}")));
                }
            }
        }
        if VERB_S_SAFE.contains(&n1) && !matches!(n1, "looks" | "works" | "plays" | "lives" | "runs" | "feels" | "likes" | "stands" | "falls" | "needs" if matches!(p, "the" | "a")) {
            let base = match n1 {
                "goes" => "go",
                "has" => "have",
                "does" => "do",
                "makes" => "make",
                "takes" => "take",
                "gets" => "get",
                "gives" => "give",
                "needs" => "need",
                "wants" => "want",
                v if v.ends_with("ies") => "",
                _ => "",
            };
            let base = if base.is_empty() { n1.trim_end_matches('s') } else { base };
            flag(cx, out, tn, "modal-verb-s", "Verb Form", i, i + 1,
                format!("After '{w}' the verb has no -s: '{base}'."), Some(format!("{raw} {base}")));
        }
    }
    // to + past form, modal + to
    if w == "to" && matches!(n1, "went" | "came" | "ate" | "took" | "gave" | "wrote" | "knew"
        | "began" | "flew" | "drove" | "swam" | "sang" | "spoke" | "chose" | "broke" | "wore"
        | "threw" | "forgot" | "bought" | "brought" | "caught" | "taught" | "understood" | "was" | "were")
    {
        if let Some(base) = base_of_past(n1) {
            flag(cx, out, tn, "to-past", "Verb Form", i, i + 1,
                format!("After 'to' the verb stays in its base form: '{base}'."),
                Some(format!("{raw} {base}")));
        }
    }
    if w == "to" && matches!(n1, "goes" | "has" | "does" | "says" | "makes" | "takes" | "gets" | "gives" | "comes" | "needs" | "wants")
        && !matches!(p, "according" | "due" | "next" | "close" | "up" | "back" | "from")
    {
        let base = match n1 { "goes" => "go", "has" => "have", "does" => "do", v => v.trim_end_matches('s') };
        flag(cx, out, tn, "to-verb-s", "Verb Form", i, i + 1,
            format!("After 'to' the verb has no -s: '{base}'."), Some(format!("{raw} {base}")));
    }
    if matches!(w, "must" | "should" | "would" | "could" | "shall") && n1 == "to" && BASE_VERBS.contains(&n2) {
        flag(cx, out, tn, "modal-to", "Verb Form", i, i + 1,
            format!("'{w}' is followed by the bare verb, without 'to'."), Some(raw.to_string()));
    }
    if matches!(w, "can" | "will") && n1 == "to" && BASE_VERBS.contains(&n2)
        && matches!(p, "i" | "you" | "he" | "she" | "it" | "we" | "they" | "who")
    {
        flag(cx, out, tn, "modal-to", "Verb Form", i, i + 1,
            format!("'{w}' is followed by the bare verb, without 'to'."), Some(raw.to_string()));
    }
    // let / make + object + to
    if matches!(w, "let" | "lets" | "letting" | "make" | "makes" | "made" | "making")
        && OBJ_PRON.contains(&n1) && n1 != "it" && n2 == "to" && BASE_VERBS.contains(&n3)
    {
        flag(cx, out, tn, "let-to", "Verb Form", i + 2, i + 2,
            format!("After '{w} {n1}' the verb has no 'to'."), Some(String::new()));
    }
    // have / has / had / 've + simple past
    let contraction_have = w.ends_with("'ve");
    if matches!(w, "have" | "has" | "had") || contraction_have {
        let (x, k) = if SUBJ_PRON.contains(&n1) && !matches!(w, "have" | "has" | "had") {
            ("", 0)
        } else if SUBJ_PRON.contains(&n1) && n1 != "it" && cx.lw(i + 1) != "" {
            (n2, 2)
        } else {
            (n1, 1)
        };
        if let Some(&(_, part)) = PAST_TO_PARTICIPLE.iter().find(|(past, _)| *past == x) {
            let objecty = matches!(w, "have" | "has" | "had") && k == 2 && matches!(x, "was" | "were");
            let rose = x == "rose";
            // "have saw" etc. Skip "had" + "did" (e.g. "everything he had did") only when ambiguous.
            if !objecty && !rose && !(matches!(w, "had") && x == "was" && p == "i" && false) {
                let idx = i + k;
                if k == 1 && !contraction_have {
                    flag(cx, out, tn, "participle", "Verb Tense", i, idx,
                        format!("After '{w}' use the past participle '{part}', not '{x}'."),
                        Some(format!("{raw} {part}")));
                } else if k == 1 && contraction_have {
                    flag(cx, out, tn, "participle", "Verb Tense", i, idx,
                        format!("After '{w}' use the past participle '{part}', not '{x}'."),
                        Some(format!("{raw} {part}")));
                } else {
                    flag(cx, out, tn, "participle", "Verb Tense", idx, idx,
                        format!("After '{w} {n1}' use the past participle '{part}', not '{x}'."),
                        Some(part.to_string()));
                }
            }
        }
    }
    // "I seen", "he done"
    if matches!(w, "i" | "he" | "she" | "they" | "we") && matches!(n1, "seen") {
        flag(cx, out, tn, "seen-saw", "Verb Tense", i, i + 1,
            "'seen' needs 'have' or 'has'; the simple past is 'saw'.",
            Some(format!("{raw} saw")));
    }
    // present perfect with a finished time
    if matches!(w, "have" | "has") && !cx.clause_has_before(i, &["since", "for", "how", "ever", "never"], 3) {
        let part = n1;
        let ed = part.ends_with("ed") && part.chars().count() > 4;
        let known = PAST_TO_PARTICIPLE.iter().any(|(_, pp)| *pp == part) || ed;
        if known {
            let mut j = i + 1;
            for _ in 0..7 {
                if !cx.adj(j) {
                    break;
                }
                j += 1;
                let t = cx.lw(j);
                let finished = t == "yesterday"
                    || (t == "ago" && !matches!(cx.lw(j.saturating_sub(1)), "long" | "how"))
                    || (t == "last" && matches!(cx.nx(j, 1), "night" | "week" | "month" | "year"
                        | "summer" | "winter" | "spring" | "autumn" | "weekend" | "monday" | "tuesday"
                        | "wednesday" | "thursday" | "friday" | "saturday" | "sunday")
                        && !matches!(cx.lw(j.saturating_sub(1)), "since" | "for" | "over" | "in" | "during" | "the" | "until" | "till"));
                if finished {
                    let rep = if ed { Some(part.to_string()) } else {
                        PAST_TO_PARTICIPLE.iter().find(|(_, pp)| *pp == part).map(|&(past, _)| past.to_string())
                    };
                    flag(cx, out, tn, "perfect-finished-time", "Verb Tense", i, i + 1,
                        "A finished time like 'yesterday' or 'ago' takes the simple past, not 'have'.",
                        rep.filter(|_| !matches!(part, "been" | "come" | "run" | "become")));
                    break;
                }
            }
        }
    }
    // present continuous with "since" / "for the last"
    if matches!(w, "am" | "is" | "are" | "i'm" | "he's" | "she's" | "it's" | "we're" | "they're" | "you're")
        && n1.ends_with("ing") && n1.chars().count() > 4 && n1 != "being"
        && !matches!(n1, "interesting" | "amazing" | "boring" | "exciting" | "surprising" | "something"
            | "anything" | "nothing" | "everything" | "morning" | "evening" | "during" | "king" | "thing")
    {
        let mut j = i + 1;
        for _ in 0..8 {
            if !cx.adj(j) {
                break;
            }
            j += 1;
            let t = cx.lw(j);
            let n = cx.nx(j, 1);
            let since_time = t == "since" && (matches!(n, "morning" | "yesterday" | "monday" | "tuesday"
                | "wednesday" | "thursday" | "friday" | "saturday" | "sunday" | "january" | "february"
                | "march" | "april" | "june" | "july" | "august" | "september" | "october" | "november"
                | "december" | "childhood" | "noon" | "birth" | "then")
                || cx.text.get(cx.toks.get(j).map_or(0, |t| t.end)..).is_some_and(|r| {
                    r.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit())
                }));
            let for_last = t == "for" && matches!(n, "the") && matches!(cx.nx(j, 2), "last" | "past");
            if since_time || for_last {
                flag(cx, out, tn, "continuous-since", "Verb Tense", i, i + 1,
                    "A time that began in the past and goes on takes the present perfect continuous, e.g. 'have been waiting'.",
                    None);
                break;
            }
        }
    }
    // gerund after enjoy, avoid ...
    if matches!(w, "enjoy" | "enjoys" | "enjoyed" | "enjoying" | "avoid" | "avoids" | "avoided"
        | "avoiding" | "dislike" | "dislikes" | "disliked" | "finish" | "finishes" | "finished"
        | "finishing" | "postpone" | "postponed" | "delay" | "delayed" | "practice" | "practise"
        | "practiced" | "practised") && n1 == "to" && BASE_VERBS.contains(&n2)
    {
        flag(cx, out, tn, "gerund", "Verb Form", i + 1, i + 2,
            format!("'{w}' is followed by an -ing form: '{}'.", gerund(n2)), Some(gerund(n2)));
    }
    if w == "forward" && n1 == "to" && matches!(p, "look" | "looks" | "looked" | "looking")
        && BASE_VERBS.contains(&n2)
    {
        flag(cx, out, tn, "gerund", "Verb Form", i + 1, i + 2,
            format!("After 'look forward to' use an -ing form: '{}'.", gerund(n2)), Some(gerund(n2)));
    }
    // know to drive -> know how to drive
    if matches!(w, "know" | "knows" | "knew") && n1 == "to"
        && matches!(n2, "drive" | "swim" | "cook" | "play" | "read" | "write" | "ride" | "dance"
            | "sing" | "fly" | "type" | "sew" | "ski" | "skate" | "code" | "speak" | "use")
        && !matches!(p, "to")
    {
        flag(cx, out, tn, "know-how", "Verb Form", i, i + 1,
            "To talk about a skill use 'know how to'.", Some(format!("{raw} how to")));
    }
    // listen music -> listen to music
    if matches!(w, "listen" | "listens" | "listened" | "listening")
        && matches!(n1, "music" | "radio" | "songs" | "song" | "podcast" | "podcasts" | "him" | "her"
            | "them" | "me" | "us" | "the" | "a" | "an" | "my" | "your" | "his" | "their" | "our")
    {
        flag(cx, out, Category::Preposition, "listen-to", "Preposition", i, i,
            "'listen' needs 'to' before what you hear.", Some(format!("{raw} to")));
    }
    // explain me
    if matches!(w, "explain" | "explains" | "explained" | "explaining")
        && matches!(n1, "me" | "him" | "her" | "us" | "them") && matches!(n2, "the" | "this" | "that"
            | "what" | "how" | "why" | "it")
    {
        flag(cx, out, Category::Preposition, "explain-to", "Preposition", i, i + 1,
            format!("Use 'explain … to {n1}', e.g. 'explain it to {n1}'."), None);
    }
}

// ---------------------------------------------------------------------------------------------
// Prepositions
// ---------------------------------------------------------------------------------------------

fn prepositions(cx: &Cx, i: usize, out: &mut Out) {
    let w = cx.lw(i);
    let raw = cx.raw(i);
    let n1 = cx.nx(i, 1);
    let n2 = cx.nx(i, 2);
    let p = cx.pv(i);
    let pr = Category::Preposition;

    const PAIRS: &[(&str, &str, &str)] = &[
        ("senior", "than", "to"),
        ("junior", "than", "to"),
        ("superior", "than", "to"),
        ("inferior", "than", "to"),
        ("similar", "than", "to"),
        ("responsible", "of", "for"),
        ("proud", "for", "of"),
        ("angry", "on", "with"),
        ("accused", "for", "of"),
        ("based", "of", "on"),
        ("according", "with", "to"),
        ("participate", "at", "in"),
        ("participated", "at", "in"),
        ("participating", "at", "in"),
        ("participates", "at", "in"),
        ("participate", "on", "in"),
        ("jealous", "from", "of"),
        ("aware", "from", "of"),
        ("fond", "from", "of"),
        ("different", "to", "from"),
    ];
    if let Some(&(_, bad, good)) = PAIRS.iter().find(|(a, b, _)| *a == w && *b == n1) {
        // "different to" is normal in British English; leave it out.
        if !(w == "different" && bad == "to") {
            flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
                format!("'{w}' takes '{good}', not '{bad}'."), Some(format!("{raw} {good}")));
        }
    }
    if matches!(w, "emphasize" | "emphasizes" | "emphasized" | "emphasizing" | "emphasise"
        | "emphasises" | "emphasised" | "emphasising") && n1 == "on"
    {
        flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
            format!("'{w}' is not followed by 'on'."), Some(raw.to_string()));
    }
    if matches!(w, "mention" | "mentions" | "mentioned" | "mentioning") && n1 == "about" {
        flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
            format!("'{w}' is not followed by 'about'."), Some(raw.to_string()));
    }
    if w == "across" && n1 == "with" && matches!(p, "come" | "came" | "comes" | "coming") {
        flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
            "'come across' is not followed by 'with'.", Some(raw.to_string()));
    }
    if w == "blind" && n1 == "with" && matches!(n2, "one" | "his" | "her" | "my" | "left" | "right" | "both") {
        flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
            "Say 'blind in one eye'.", Some(format!("{raw} in")));
    }
    if matches!(w, "wait" | "waits" | "waited" | "waiting") && n1 == "on"
        && matches!(cx.nx(i, 3), "train" | "bus" | "taxi" | "flight" | "plane" | "subway" | "results"
            | "news" | "answer" | "reply" | "response" | "decision" | "call" | "email" | "message"
            | "letter" | "package" | "delivery" | "doctor" | "friend" | "mom" | "dad" | "tram")
        && matches!(n2, "the" | "a" | "an" | "my" | "his" | "her" | "their")
    {
        flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
            "To wait until something arrives, use 'wait for'.", Some(format!("{raw} for")));
    }
    if matches!(w, "arrive" | "arrives" | "arrived" | "arriving") && n1 == "to"
        && matches!(n2, "the" | "a" | "an" | "my" | "his" | "her" | "their" | "our" | "school" | "work")
    {
        flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
            "Use 'arrive at' (a place) or 'arrive in' (a city or country).", None);
    }
    if w == "good" && n1 == "in" && matches!(n2, "math" | "maths" | "english" | "science" | "sports" | "cooking" | "singing" | "chess" | "history" | "drawing" | "writing") {
        flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
            "Say 'good at' for a skill.", Some(format!("{raw} at")));
    }
    if w == "their" && cx.clause_end(i) && matches!(p, "over" | "in" | "out" | "up" | "down" | "from" | "go" | "went" | "to" | "stay" | "stayed" | "put" | "left") {
        flag(cx, out, Category::WordChoice, "their-there", "There / Their", i, i,
            "'their' is possessive. 'There' is meant here.", Some(with_case_of(raw, "there")));
    }
    if w == "your" && n1 == "the" && cx.starts_clause(i) && matches!(n2, "best" | "worst" | "most" | "only" | "first" | "last" | "greatest" | "one") {
        flag(cx, out, Category::WordChoice, "your-youre", "Your / You're", i, i,
            "'your' is possessive. Here 'you're' (you are) is meant.", Some(with_case_of(raw, "you're")));
    }
    if w == "habit" && n1 == "to" && matches!(p, "the" | "a" | "his" | "her" | "my" | "their" | "bad"
        | "good" | "same")
    {
        flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
            "After 'habit' use 'of' with an -ing form: 'the habit of arriving'.", None);
    }
    if w == "admission" && n1 == "for" && matches!(p, "grants" | "gives" | "granted" | "gave" | "get" | "gets") {
        flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
            "Admission is 'to' a place or event.", Some(format!("{raw} to")));
    }
    if matches!(w, "at" | "in" | "on") {
        if w == "in" && matches!(n1, "sunday" | "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "saturday") {
            flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
                "Days of the week take 'on'.", Some(format!("{} {}", with_case_of(raw, "on"), cx.raw(i + 1))));
        }
        if w == "in" && MONTHS.contains(&n1) {
            // "in January 13th" / "in March 5": a day number follows
            let after = cx.toks.get(i + 1).map_or(0, |t| t.end);
            let rest = cx.text.get(after..).unwrap_or("");
            let digits: String = rest.trim_start_matches(' ').chars().take_while(char::is_ascii_digit).collect();
            if !digits.is_empty() && digits.len() <= 2 {
                flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
                    "With a full date (month and day) use 'on'.",
                    Some(format!("{} {}", with_case_of(raw, "on"), cx.raw(i + 1))));
            }
        }
        if w == "at" && matches!(n1, "morning" | "evening" | "afternoon") && p == "the" {
            // "at the morning": handled below via the previous word
        }
        if w == "at" && n1 == "the" && matches!(n2, "morning" | "evening" | "afternoon") {
            flag(cx, out, pr, "preposition", "Preposition", i, i + 2,
                format!("Say 'in the {n2}'."), Some(format!("{} the {}", with_case_of(raw, "in"), n2)));
        }
        if matches!(w, "in" | "on") && n1 == "night" && !matches!(p, "the" | "last" | "that" | "this" | "one"
            | "every" | "each" | "all" | "good" | "late" | "a" | "dead" | "of")
        {
            flag(cx, out, pr, "preposition", "Preposition", i, i + 1,
                "Say 'at night'.", Some(format!("{} night", with_case_of(raw, "at"))));
        }
    }
    // since + number of units -> for
    if w == "since" {
        let after = cx.toks.get(i).map_or(0, |t| t.end);
        let rest = cx.text.get(after..).unwrap_or("");
        let trimmed = rest.trim_start_matches(' ');
        let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
        let unit_after_digits = !digits.is_empty() && digits.len() <= 2 && {
            let tail = trimmed.get(digits.len()..).unwrap_or("").trim_start_matches(' ');
            let word: String = tail.chars().take_while(|c| c.is_alphabetic()).collect();
            UNITS.contains(&word.to_lowercase().as_str())
        };
        let word_unit = (NUM_WORDS.contains(&n1) || matches!(n1, "a" | "an" | "few" | "several" | "many" | "one"))
            && UNITS.contains(&n2);
        if unit_after_digits || word_unit {
            flag(cx, out, pr, "since-for", "Since / For", i, i,
                "'since' goes with a point in time ('since May'); a length of time takes 'for'.",
                Some(with_case_of(raw, "for")));
        }
    }
    // five minutes before -> ago
    if w == "before" && cx.clause_end(i) && UNITS.contains(&p) && !p.is_empty() {
        let q = if i >= 2 { cx.lw(i - 2) } else { "" };
        let has_num = NUM_WORDS.contains(&q) || matches!(q, "a" | "an" | "few" | "several" | "one")
            || cx.text.get(..cx.toks.get(i - 1).map_or(0, |t| t.start)).is_some_and(|s| {
                s.trim_end_matches(' ').chars().last().is_some_and(|c| c.is_ascii_digit())
            });
        let mut has_past_perfect = false;
        let mut k = i;
        while k > 0 {
            k -= 1;
            if matches!(cx.lw(k), "had" | "hadn't" | "would" | "could" | "'d") || cx.lw(k).ends_with("'d") {
                has_past_perfect = true;
            }
            if k > 0 && !cx.adj(k - 1) && cx.gap(k - 1).chars().any(|c| matches!(c, '.' | '!' | '?' | '\n')) {
                break;
            }
            if i - k > 12 {
                break;
            }
        }
        if has_num && !has_past_perfect {
            flag(cx, out, Category::WordChoice, "ago-before", "Ago / Before", i, i,
                "Counting back from now, use 'ago'.", Some(with_case_of(raw, "ago")));
        }
    }
    // two last -> last two
    if matches!(w, "last" | "next") && NUM_WORDS.contains(&p) && !matches!(p, "hundred")
        && !matches!(n1, "name" | "names" | "time" | "times")
        && !matches!(cx.pv(i.saturating_sub(1)), "a" | "an" | "one" if false)
    {
        flag(cx, out, Category::WordChoice, "ordinal-order", "Word Order", i - 1, i,
            format!("Put the ordinal first: '{w} {p}'."), Some(format!("{w} {p}")));
    }
    // between you and I, let he go
    if w == "i" && cx.raw(i) == "I" && p == "and" && i >= 3 && cx.adj(i - 2) && cx.adj(i - 3)
        && cx.raw(i - 2).chars().next().is_some_and(char::is_uppercase)
        && matches!(cx.lw(i - 3), "to" | "for" | "with" | "from" | "about" | "of" | "by" | "at" | "behind" | "beside" | "near")
    {
        flag(cx, out, Category::WordChoice, "pronoun-case", "Pronoun Case", i, i,
            "After a preposition use the object form 'me', not 'I'.", Some("me".to_string()));
    }
    if w == "me" && p == "and" && matches!(n1, "are" | "were" | "will" | "have" | "can" | "could" | "would" | "went")
        && !cx.clause_has_before(i, &["to", "for", "with", "from", "about", "of", "by", "at", "between", "like", "than", "as", "give", "told", "tell", "ask", "asked", "let", "saw", "see", "call"], 4)
    {
        flag(cx, out, Category::WordChoice, "pronoun-case", "Pronoun Case", i, i,
            "As part of the subject use 'I', not 'me'.", Some("I".to_string()));
    }
    if w == "i" && cx.raw(i) == "I" && p == "and" && i >= 2 && matches!(cx.lw(i - 2), "you" | "him"
        | "her" | "them" | "us" | "me") && i >= 3 && cx.adj(i - 3) && matches!(cx.lw(i - 3), "between"
        | "for" | "with" | "to" | "from" | "of" | "about" | "like" | "except" | "beside" | "behind"
        | "near" | "around" | "among" | "without" | "by" | "at" | "on" | "upon" | "toward" | "towards")
        && cx.adj(i - 2) && cx.adj(i - 1)
    {
        flag(cx, out, Category::WordChoice, "pronoun-case", "Pronoun Case", i, i,
            "After a preposition use the object form 'me', not 'I'.", Some("me".to_string()));
    }
    if w == "me" && p == "and" && i >= 2 && cx.starts_clause(i - 2) && cx.adj(i - 2) {
        let verb = n1;
        let verbish = verb.ends_with("ed") && verb.chars().count() > 3
            || matches!(verb, "went" | "saw" | "had" | "are" | "were" | "was" | "did" | "will" | "would"
                | "can" | "could" | "should" | "have" | "like" | "love" | "need" | "want" | "go" | "got"
                | "came" | "made" | "took" | "ran" | "play" | "live" | "work" | "ate" | "wrote");
        let first_raw = cx.raw(i - 2);
        let name_or_pron = first_raw.chars().next().is_some_and(char::is_uppercase)
            || matches!(cx.lw(i - 2), "he" | "she" | "they" | "you");
        if verbish && name_or_pron && !matches!(cx.lw(i - 2), "between" | "me") {
            flag(cx, out, Category::WordChoice, "pronoun-case", "Pronoun Case", i - 2, i,
                "As part of the subject use 'I', not 'me'.",
                Some(format!("{} and I", first_raw)));
        }
    }
    if w == "me" && n1 == "and" && cx.starts_sentence(i) && !cx.nx(i, 2).is_empty() {
        let verb = cx.nx(i, 3);
        if verb.ends_with("ed") && verb.chars().count() > 3 || matches!(verb, "went" | "saw" | "had" | "are"
            | "were" | "did" | "will" | "can" | "could" | "have" | "like" | "love" | "got" | "came" | "ran") {
            let other = cx.raw(i + 2);
            let rep = if OBJ_PRON.contains(&cx.nx(i, 2)) {
                None
            } else {
                Some(format!("{other} and I"))
            };
            flag(cx, out, Category::WordChoice, "pronoun-case", "Pronoun Case", i, i + 2,
                "As part of the subject use 'I', not 'me'.", rep);
        }
    }
    if w == "let" && matches!(n1, "he" | "she" | "i" | "we" | "they") && cx.nx(i, 2) != "who" {
        let fix = match n1 {
            "he" => "him",
            "she" => "her",
            "i" => "me",
            "we" => "us",
            _ => "them",
        };
        flag(cx, out, Category::WordChoice, "pronoun-case", "Pronoun Case", i, i + 1,
            format!("After 'let' use the object form '{fix}'."), Some(format!("{raw} {fix}")));
    }
    if w == "to" && n1 == "who" && matches!(n2, "i" | "we" | "you" | "he" | "she" | "they") {
        flag(cx, out, Category::WordChoice, "who-whom", "Who / Whom", i, i + 1,
            "After a preposition use 'whom'.", Some(format!("{raw} whom")));
    }
    // one another (between two)
    if w == "one" && n1 == "another" && cx.clause_has_before(i, &["both"], 5) {
        flag(cx, out, Category::WordChoice, "each-other", "Each Other", i, i + 1,
            "For two people or things use 'each other'.", Some("each other".to_string()));
    }
}

// ---------------------------------------------------------------------------------------------
// Sentence structure
// ---------------------------------------------------------------------------------------------

fn structure(cx: &Cx, i: usize, out: &mut Out) {
    let w = cx.lw(i);
    let raw = cx.raw(i);
    let n1 = cx.nx(i, 1);
    let n2 = cx.nx(i, 2);
    let st = Category::Fragment;

    // Although ..., but ...    Because ..., so ...
    if cx.starts_sentence(i) && matches!(w, "although" | "though" | "because") || (w == "even" && n1 == "though" && cx.starts_sentence(i)) {
        let stop = if w == "because" { "so" } else { "but" };
        let mut j = i;
        let limit = i + 25;
        while j < limit && j + 1 < cx.n() {
            let g = cx.gap(j);
            if g.chars().any(|c| matches!(c, '.' | '!' | '?' | '\n' | '\r')) {
                break;
            }
            if g.trim() == "," && cx.lw(j + 1) == stop {
                if let (Some(a), Some(b)) = (cx.toks.get(j + 1), cx.toks.get(j + 2)) {
                    if cx.gap(j + 1).chars().all(|c| c == ' ') {
                        if cx.on.is_on(st) {
                            push(out, a.start..b.start, st, "double-conjunction", "Sentence Structure",
                                format!("'{raw}' already links the clauses, so '{stop}' is not needed."),
                                Some(String::new()));
                        }
                    }
                }
                break;
            }
            j += 1;
        }
    }
    // If I will ... / when ... will
    if w == "will" && SUBJ_PRON.contains(&cx.pv(i)) && i >= 2 {
        let subj = cx.lw(i - 1);
        let conj = cx.lw(i - 2);
        let adj_chain = cx.adj(i - 2) && cx.adj(i - 1);
        if adj_chain && conj == "if" && cx.starts_sentence(i - 2) {
            let rep = if matches!(subj, "he" | "she" | "it") { None } else { Some(String::new()) };
            let (a, b) = if rep.is_some() { (cx.toks.get(i).map(|t| t.start), cx.toks.get(i + 1).map(|t| t.start)) } else { (None, None) };
            if let (Some(a), Some(b)) = (a, b) {
                if cx.on.is_on(Category::Tense) {
                    push(out, a..b, Category::Tense, "if-will", "Verb Tense",
                        "After 'if' in a condition the present tense is used, not 'will'.".to_string(), rep);
                }
            } else {
                flag(cx, out, Category::Tense, "if-will", "Verb Tense", i, i,
                    "After 'if' in a condition the present tense is used, not 'will'.", None);
            }
        } else if adj_chain && matches!(conj, "when" | "until" | "till" | "before" | "after" | "once" | "while" | "unless")
            && (0..i - 2).any(|k| matches!(cx.lw(k), "will" | "i'll" | "you'll" | "he'll" | "she'll" | "we'll" | "they'll" | "it'll" | "won't"))
            && !cx.starts_sentence(i - 2)
            && (0..i - 2).rev().take_while(|k| !cx.gap(*k).chars().any(|c| matches!(c, '.' | '!' | '?' | '\n'))).any(|k| matches!(cx.lw(k), "will" | "i'll" | "you'll" | "he'll" | "she'll" | "we'll" | "they'll" | "it'll" | "won't"))
        {
            flag(cx, out, Category::Tense, "time-will", "Verb Tense", i, i,
                format!("After '{conj}' in a time clause the present tense is used, not 'will'."), None);
        }
    }
    if w == "will" && cx.starts_sentence(i.saturating_sub(2)) && false {}
    // Double negatives
    if (w.ends_with("n't") && w != "ain't") || w == "never" {
        let mut j = i;
        for _ in 0..4 {
            if !cx.adj(j) {
                break;
            }
            j += 1;
            let t = cx.lw(j);
            if matches!(t, "nobody" | "nothing" | "nowhere" | "none" | "no") {
                let prevw = if j > 0 { cx.lw(j - 1) } else { "" };
                let ok_idiom = matches!(prevw, "say" | "said" | "says" | "saying" | "answer" | "take" | "taking")
                    || (t == "no" && matches!(cx.nx(j, 1), "to" | "matter" | "longer"));
                let mixed_modal = matches!(w, "can't" | "couldn't" | "won't" | "wouldn't") && t == "nothing" && false;
                if !ok_idiom && !mixed_modal && !(w == "never" && t == "no" && false) {
                    flag(cx, out, st, "double-negative", "Double Negative", i, j,
                        format!("'{w}' and '{t}' cancel each other out; use one negative."), None);
                }
                break;
            }
            if matches!(t, "but" | "and" | "or" | "that" | "which" | "because") {
                break;
            }
        }
        if (w.ends_with("n't") || w == "never") && n1 == "no" && n2 == "one" && false {}
    }
    // not ... neither
    if w == "not" || w.ends_with("n't") {
        if let Some(j) = cx.find_after(i, &["neither"], 7) {
            if !cx.clause_has_before(j, &["nor"], 1) {
                flag(cx, out, st, "not-neither", "Double Negative", j, j,
                    "'neither' repeats the negative of 'not'. Use 'either … or' or 'neither … nor' without 'not'.", None);
            }
        }
    }
    // The reason is because / due to
    if w == "reason" && matches!(n1, "is" | "was") && (n2 == "because" || (n2 == "due" && cx.nx(i, 3) == "to" && cx.nx(i, 4) != "the")) {
        let last = if n2 == "because" { i + 2 } else { i + 3 };
        flag(cx, out, Category::WordChoice, "reason-because", "Reason Is Because", i + 1, last,
            "After 'the reason is' use 'that'.", Some(format!("{} that", cx.raw(i + 1))));
    }
    // must have to
    if w == "must" && n1 == "have" && n2 == "to" {
        flag(cx, out, Category::Tense, "must-have-to", "Verb Form", i, i + 2,
            "'must have to' doubles the obligation; use 'must' or 'have to'.", None);
    }
    // One ... his
    if w == "one" && cx.starts_sentence(i) && matches!(n1, "should" | "must" | "can" | "could" | "would" | "may" | "might") {
        let mut j = i;
        while j + 1 < cx.n() && !cx.gap(j).chars().any(|c| matches!(c, '.' | '!' | '?' | '\n')) {
            j += 1;
            if cx.lw(j) == "his" {
                flag(cx, out, Category::WordChoice, "one-his", "Pronoun Agreement", j, j,
                    "After 'one' keep the pronoun consistent: use 'one's' (or 'his or her').",
                    Some("one's".to_string()));
                break;
            }
        }
    }
    // good -> well after action verbs
    if w == "good" && cx.clause_end(i) && matches!(cx.pv(i), "cook" | "cooks" | "cooked" | "sing" | "sings"
        | "sang" | "play" | "plays" | "played" | "write" | "writes" | "wrote" | "speak" | "speaks"
        | "spoke" | "drive" | "drives" | "drove" | "dance" | "dances" | "danced" | "swim" | "swims"
        | "swam" | "paint" | "paints" | "painted" | "draw" | "draws" | "drew" | "dress" | "dresses")
    {
        flag(cx, out, Category::WordChoice, "good-well", "Good / Well", i, i,
            "An action is described with the adverb 'well', not 'good'.", Some(with_case_of(raw, "well")));
    }
    // lets -> let's
    if w == "lets" && cx.starts_clause(i) && matches!(n1, "go" | "get" | "see" | "try" | "start"
        | "make" | "do" | "talk" | "take" | "eat" | "be" | "stop" | "just" | "not" | "all" | "say"
        | "face" | "hope" | "meet" | "leave" | "keep" | "look" | "move" | "pretend" | "play" | "forget" | "begin" | "figure")
    {
        flag(cx, out, Category::WordChoice, "lets-lets", "Let's", i, i,
            "'let's' (let us) needs an apostrophe.", Some(with_case_of(raw, "let's")));
    }
}

// ---------------------------------------------------------------------------------------------
// Capital letters
// ---------------------------------------------------------------------------------------------

fn capitals(cx: &Cx, i: usize, out: &mut Out) {
    let raw = cx.raw(i);
    let lw = cx.lw(i);
    let pu = Category::Punctuation;
    let tok = match cx.toks.get(i) {
        Some(t) => t,
        None => return,
    };
    let before = cx.text.get(..tok.start).and_then(|s| s.chars().last());
    let after_text = cx.text.get(tok.end..).unwrap_or("");
    let after = after_text.chars().next();
    let after2 = after_text.chars().nth(1);

    // lower-case "i"
    if matches!(lw, "i" | "i'm" | "i've" | "i'll" | "i'd") && raw.starts_with('i')
        && !matches!(before, Some('.' | '/' | '@' | '_' | '#' | '\\' | '\'' | '$'))
        && !before.is_some_and(|c| c.is_ascii_digit())
        && !(after == Some('.') && after2.is_some_and(char::is_alphabetic))
        && !matches!(after, Some(')' | '/' | '@' | '_'))
        && !matches!(cx.pv(i), "chapter" | "part" | "section" | "volume" | "book" | "act" | "scene"
            | "type" | "class" | "article" | "table" | "figure" | "fig" | "vitamin" | "step" | "phase"
            | "level" | "appendix" | "world" | "war" | "grade")
        && !(raw == "i" && cx.n() == 1)
    {
        let rep = format!("I{}", raw.get(1..).unwrap_or(""));
        flag(cx, out, pu, "capital-i", "Capitalization", i, i,
            "The pronoun 'I' is always capitalized.", Some(rep));
    }

    // lower-case word at the start of a sentence after ". "
    if i > 0 {
        let g = cx.gap(i - 1);
        let mut gc = g.chars();
        let first = gc.next();
        let rest_spaces = g.len() > 1 && gc.all(|c| c == ' ' || c == '\t');
        let prevw = cx.lw(i - 1);
        let abbreviation = matches!(prevw, "mr" | "mrs" | "ms" | "dr" | "prof" | "sr" | "jr" | "st" | "vs"
            | "etc" | "inc" | "ltd" | "co" | "no" | "fig" | "vol" | "approx" | "dept" | "est" | "min"
            | "max" | "cf" | "al" | "e" | "g" | "i" | "mt" | "ave" | "p" | "pp" | "ch" | "eg" | "ie"
            | "a" | "m" | "am" | "pm")
            || cx.raw(i - 1).chars().count() == 1;
        let starts_lower = raw.chars().next().is_some_and(char::is_lowercase)
            && !raw.chars().any(char::is_uppercase)
            && (raw.chars().count() > 1 || raw == "a");
        let abbr_after = after == Some('.') && after2.is_some_and(char::is_alphabetic);
        if matches!(first, Some('.' | '!' | '?')) && rest_spaces && !abbreviation && starts_lower
            && !abbr_after && !matches!(lw, "e" | "i" | "etc" | "vs" | "eg" | "ie" | "cf")
            && !(first == Some('.') && cx.text.get(..tok.start).is_some_and(|s| s.trim_end().ends_with("..")))
        {
            let cap = capitalised(raw);
            flag(cx, out, pu, "sentence-capital", "Capitalization", i, i,
                "A sentence starts with a capital letter.", Some(cap));
        }
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    fn rules(text: &str) -> Vec<&'static str> {
        check_grammar(text, &EnabledCategories::default())
            .into_iter()
            .map(|i| i.rule)
            .collect()
    }
    fn flagged(text: &str) -> bool {
        !rules(text).is_empty()
    }

    #[test]
    fn confusables_fire() {
        assert!(rules("What was you're answer?").contains(&"your-youre"));
        assert!(rules("Your welcome.").contains(&"your-youre"));
        assert!(rules("Parents work for there children.").contains(&"their-there"));
        assert!(rules("Whose there?").contains(&"whos-whose"));
        assert!(rules("I don't want to loose the job.").contains(&"lose-loose"));
        assert!(rules("She is taller then me.").contains(&"then-than"));
    }

    #[test]
    fn agreement_fires() {
        assert!(rules("Every students is intelligent.").contains(&"every-plural"));
        assert!(rules("The police is coming.").contains(&"plural-collective"));
        assert!(rules("Each of the cars are fast.").contains(&"each-of"));
        assert!(rules("The number of vehicles are increasing.").contains(&"number-of"));
        assert!(rules("Mathematics are his favourite subject.").contains(&"singular-s-noun"));
        assert!(rules("Either Jack or Joan are correct.").contains(&"either-or"));
    }

    #[test]
    fn tense_and_structure_fire() {
        assert!(rules("I did not saw him yesterday.").contains(&"did-past"));
        assert!(rules("If I will visit London, I will meet you.").contains(&"if-will"));
        assert!(rules("Although it was raining, but we went.").contains(&"double-conjunction"));
        assert!(rules("I didn't meet no one.").contains(&"double-negative"));
        assert!(rules("He should have went.").contains(&"participle"));
    }

    #[test]
    fn plain_prose_is_left_alone() {
        for s in [
            "\"Where are you going?\" she asked. \"Home,\" he said.",
            "He walked to the store, bought some bread, and came back.",
            "The number of people who came was surprising.",
            "Each of them has a key.",
            "Everyone has to leave by nine.",
            "Let him go.",
            "I wonder if he will come.",
            "I will wait until he arrives.",
            "It's a lovely day and the dog wagged its tail.",
            "You're right, and your answer is correct.",
            "They're going to their house over there.",
            "She has more tall trees than he does.",
            "There are two books on the table.",
            "Neither the teacher nor the students are ready.",
            "He didn't say anything to anyone.",
            "One of the best is here.",
            "I can't help but laugh.",
            "She said no.",
        ] {
            assert!(!flagged(s), "false alarm on: {s}  -> {:?}", rules(s));
        }
    }

    #[test]
    fn capitals() {
        assert!(rules("i think so.").contains(&"capital-i"));
        assert!(rules("It rained. then we left.").contains(&"sentence-capital"));
        assert!(!flagged("Dr. smith"));
        assert!(!flagged("\"Stop!\" he cried."));
    }
}
