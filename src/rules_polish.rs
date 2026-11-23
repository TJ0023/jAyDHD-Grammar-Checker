// Round-three rules ("polish"), pulled in by `rules_extra.rs` with `include!`. They were written
// from a stress test of mistakes that fiction writers make (grammar_cases/round3.tsv). The same
// habit as everywhere else applies: patterns only see words joined by plain spaces, so a comma,
// a quote or a line break stops them. Every rule here is narrow on purpose; a rule that fires on
// correct prose is worse than a rule that is missing.

const EXTRA_BASE: &[&str] = &[
    "bark", "shake", "jump", "fight", "cry", "laugh", "smile", "shout", "whisper", "listen",
    "sing", "wait", "grow", "die", "live", "stay", "sleep", "happen", "matter", "belong", "fit",
    "cost", "hurt", "burn", "break", "shine", "hold", "stand", "rise", "wake", "ring", "swing",
    "cut", "hit", "hide", "pick", "throw", "push", "touch", "agree", "return", "appear",
    "disappear", "offer", "serve", "worry", "bother", "cover", "fill", "mean", "own", "pick",
    "wish", "rain", "snow", "fail", "win", "teach", "fix", "mix", "kiss", "miss", "guess",
    "press", "cross", "discuss", "dress", "pass", "bless", "chase", "climb", "scream", "answer",
    "like", "do", "have", "explain", "apologize", "prove", "describe", "imagine", "consider",
    "create", "destroy", "enter", "decide", "forgive", "realize", "notice", "mention", "allow",
    "prefer", "promise", "refuse", "manage", "learn", "act", "plan", "prepare", "protect",
    "remind", "report", "save", "share", "smell", "taste", "trust", "warn", "worry",
    "gallop", "paint", "draw", "read", "kick", "hug", "yell", "mutter", "nod", "shrug", "sigh",
    "frown", "glare", "stare", "gaze", "blink", "breathe", "gasp", "grab", "drop", "lift", "toss",
    "tear", "roll", "spin", "slide", "slip", "trip", "crawl", "sail", "float", "sink", "flee",
    "escape", "hunt", "trap", "shoot", "heal", "bleed", "shiver", "tremble", "glow", "sparkle",
    "flash", "flicker", "fade", "vanish", "emerge", "depart", "exit", "approach", "retreat",
    "advance", "wander", "roam", "invite", "welcome", "greet", "thank", "curse", "swear", "lie",
    "deny", "admit", "confess", "accuse", "blame", "judge", "doubt", "fear", "dream", "wonder",
    "recall", "recognize", "count", "measure", "weigh", "waste", "earn", "owe", "borrow", "steal",
    "trade", "charge", "repair", "smash", "crush", "bend", "fold", "wrap", "tie", "carve", "dig",
    "bury", "plant", "harvest", "feed", "cough", "sneeze", "snore", "rest", "relax", "stretch",
    "lean", "kneel", "bow", "twist", "drag", "lower", "point", "wave", "signal", "whistle", "hum",
    "murmur", "roar", "growl", "howl", "crash", "knock", "tap", "type", "reply", "respond",
    "argue", "disagree", "insist", "demand", "beg", "complain", "chat", "joke", "tease", "annoy",
    "disturb", "interrupt", "ignore", "observe", "grip", "squeeze", "rub", "scratch", "wipe",
    "wash", "brush", "paint", "ride", "stumble", "race", "hurry", "rush", "shout", "laugh",
];

fn is_base_verb(w: &str) -> bool {
    BASE_VERBS.contains(&w) || EXTRA_BASE.contains(&w)
}

/// "goes" gives "go", "tries" gives "try", "watches" gives "watch".
fn verb_s_base(w: &str) -> Option<String> {
    if matches!(w, "is" | "was" | "his" | "this" | "its" | "as" | "us" | "yes" | "has" | "does") {
        return match w {
            "has" => Some("have".into()),
            "does" => Some("do".into()),
            _ => None,
        };
    }
    let cands: Vec<String> = if let Some(stem) = w.strip_suffix("ies") {
        vec![format!("{stem}y")]
    } else if let Some(stem) = w.strip_suffix("es") {
        vec![stem.to_string(), format!("{stem}e")]
    } else if let Some(stem) = w.strip_suffix('s') {
        vec![stem.to_string()]
    } else {
        return None;
    };
    cands.into_iter().find(|b| b.len() > 1 && is_base_verb(b))
}

/// "go" gives "goes", "try" gives "tries".
fn verb_to_s(base: &str) -> String {
    match base {
        "have" => "has".into(),
        "do" => "does".into(),
        "go" => "goes".into(),
        _ if base.ends_with(['s', 'x', 'z']) || base.ends_with("ch") || base.ends_with("sh") => {
            format!("{base}es")
        }
        _ => match base.strip_suffix('y') {
            Some(stem) if !stem.ends_with(['a', 'e', 'i', 'o', 'u']) => format!("{stem}ies"),
            _ => format!("{base}s"),
        },
    }
}

fn is_gerund_of_verb(w: &str) -> bool {
    w.len() > 4
        && w.ends_with("ing")
        && BASE_VERBS
            .iter()
            .chain(EXTRA_BASE.iter())
            .any(|b| gerund(b) == w)
        && !matches!(w, "being" | "during")
}

fn past_of_base(base: &str) -> Option<&'static str> {
    IRREG_PAST.iter().find(|(_, b)| *b == base).map(|&(p, _)| p)
}

fn sentence_bounds(cx: &Cx, i: usize) -> (usize, usize) {
    let ends = |g: &str| g.chars().any(|c| matches!(c, '.' | '!' | '?' | '\n' | '\r'));
    let mut a = i;
    while a > 0 && !ends(cx.gap(a - 1)) {
        a -= 1;
    }
    let mut b = i;
    while b + 1 < cx.n() && !ends(cx.gap(b)) {
        b += 1;
    }
    (a, b)
}

fn sentence_has(cx: &Cx, i: usize, words: &[&str]) -> bool {
    let (a, b) = sentence_bounds(cx, i);
    (a..=b).any(|k| words.contains(&cx.lw(k)))
}

/// The sentence talks about the past: "yesterday", "ago", "last night" and so on.
fn sentence_is_past(cx: &Cx, i: usize) -> bool {
    let (a, b) = sentence_bounds(cx, i);
    (a..=b).any(|k| {
        matches!(cx.lw(k), "yesterday" | "ago")
            || (cx.lw(k) == "last"
                && matches!(
                    cx.nx(k, 1),
                    "night" | "week" | "month" | "year" | "summer" | "winter" | "weekend" | "time"
                        | "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "saturday"
                        | "sunday"
                ))
    })
}

const PREPS: &[&str] = &[
    "of", "in", "on", "for", "with", "by", "to", "from", "about", "at", "near", "behind", "beside",
    "among", "between", "against", "toward", "towards", "under", "over", "around", "like",
];

const DETERMINERS: &[&str] = &[
    "the", "a", "an", "my", "his", "her", "their", "our", "your", "some", "this", "that", "these",
    "those", "its",
];

fn subject_form(w: &str) -> &'static str {
    match w {
        "him" => "he",
        "her" => "she",
        "them" => "they",
        "me" => "I",
        "us" => "we",
        _ => "",
    }
}

fn object_form(w: &str) -> &'static str {
    match w {
        "he" => "him",
        "she" => "her",
        "they" => "them",
        "we" => "us",
        _ => "",
    }
}

fn polish(cx: &Cx, i: usize, out: &mut Out) {
    let w = cx.lw(i);
    let raw = cx.raw(i);
    let n1 = cx.nx(i, 1);
    let n2 = cx.nx(i, 2);
    let n3 = cx.nx(i, 3);
    let n4 = cx.nx(i, 4);
    let p = cx.pv(i);
    let sv = Category::SubjectVerb;
    let tn = Category::Tense;
    let wc = Category::WordChoice;
    let pr = Category::Preposition;
    let fr = Category::Fragment;

    // ----- Agreement -----------------------------------------------------------------------

    // The list of items were ...  (the head noun is singular)
    if w == "the"
        && matches!(n1, "list" | "stack" | "pile" | "row" | "box" | "bag" | "collection" | "set" | "series")
        && n2 == "of"
    {
        if let Some(j) = cx.find_after(i + 2, &["are", "were", "have"], 5) {
            let clean = (i + 3..j).all(|k| !matches!(cx.lw(k), "who" | "that" | "which" | "where" | "and" | "or"));
            if let (true, Some(fix)) = (clean, plural_verb_to_singular(cx.lw(j))) {
                flag(cx, out, sv, "head-noun", "Subject-Verb Agreement", j, j,
                    format!("The subject is the singular '{n1}', so the verb should be '{fix}'."),
                    Some(fix.to_string()));
            }
        }
    }
    // Neither of the options are ...
    if w == "neither" && n1 == "of" && matches!(n2, "the" | "these" | "those" | "my" | "his" | "her" | "our" | "their" | "your" | "them" | "us" | "you") {
        if let Some(j) = cx.find_after(i + 2, &["are", "were"], 4) {
            let clean = (i + 3..j).all(|k| !matches!(cx.lw(k), "who" | "that" | "which" | "where" | "and" | "or"));
            if let (true, Some(fix)) = (clean, plural_verb_to_singular(cx.lw(j))) {
                flag(cx, out, sv, "neither-of", "Subject-Verb Agreement", j, j,
                    format!("'Neither' is singular, so the verb should be '{fix}'."),
                    Some(fix.to_string()));
            }
        }
    }
    // The dogs barks ...  (plural noun, singular verb)
    if looks_plural(w)
        && !w.ends_with("ics")
        && !matches!(w, "means" | "headquarters" | "works" | "crossroads" | "barracks" | "gallows"
            | "billiards" | "athletics" | "measles" | "mumps" | "always" | "perhaps")
        && matches!(p, "the" | "these" | "those" | "my" | "your" | "our" | "their" | "his" | "her" | "some"
            | "many" | "few" | "several" | "both" | "two" | "three" | "four" | "five" | "six" | "all")
        && !(i >= 2 && PREPS.contains(&cx.pv(i - 1)))
    {
        // A plural noun followed by a word that could also be a noun ("the kids drinks") is left
        // alone unless the word after it shows that it is working as a verb.
        let follows_ok = cx.clause_end(i + 1)
            || DETERMINERS.contains(&n2)
            || PREPS.contains(&n2)
            || n2.ends_with("ly")
            || matches!(n2, "and" | "but" | "so" | "when" | "because" | "while" | "until" | "if" | "as" | "that"
                | "it" | "them" | "him" | "me" | "us" | "you" | "away" | "out" | "up" | "down" | "home"
                | "there" | "here" | "now" | "then" | "always" | "often" | "never" | "just" | "still"
                | "very" | "not" | "too" | "again" | "across" | "through" | "into" | "over" | "outside"
                | "inside" | "together" | "alone" | "upstairs" | "downstairs" | "nearby" | "today"
                | "tomorrow" | "tonight" | "all" | "every" | "each" | "some" | "many" | "most" | "no");
        if let (true, Some(base)) = (follows_ok, verb_s_base(n1)) {
            if !matches!(base.as_str(), "have" | "do") && !matches!(n1, "does" | "has") {
                flag(cx, out, sv, "plural-subject-verb-s", "Subject-Verb Agreement", i + 1, i + 1,
                    format!("'{raw}' is plural, so the verb should be '{base}'."), Some(base));
            }
        }
    }
    // My brother and I was ...
    if w == "and" && matches!(n1, "i" | "he" | "she" | "we" | "they" | "you") && matches!(n2, "was" | "is" | "has") {
        let before = if i >= 2 { cx.pv(i - 1) } else { "" };
        let named = i >= 1
            && cx.toks.get(i - 1).is_some_and(|t| t.text(cx.text).starts_with(char::is_uppercase))
            && cx.starts_clause(i - 1);
        if DETERMINERS.contains(&before) || named {
            let fix = match n2 { "was" => "were", "is" => "are", _ => "have" };
            flag(cx, out, sv, "compound-subject", "Subject-Verb Agreement", i + 2, i + 2,
                format!("Two people make a plural subject, so the verb should be '{fix}'."),
                Some(fix.to_string()));
        }
    }
    // The man who live next door ...
    if w == "who"
        && matches!(p, "man" | "woman" | "person" | "boy" | "girl" | "guy" | "one" | "someone" | "anyone"
            | "child" | "friend" | "neighbor" | "neighbour" | "teacher" | "doctor" | "lady" | "kid")
    {
        if let Some(fix) = plural_verb_to_singular(n1) {
            flag(cx, out, sv, "who-singular", "Subject-Verb Agreement", i + 1, i + 1,
                format!("'{p} who …' is singular, so the verb should be '{fix}'."), Some(fix.to_string()));
        } else if is_base_verb(n1) && !matches!(n1, "read" | "put" | "set" | "let" | "cut" | "hit" | "hurt" | "cost" | "shut" | "quit" | "fit" | "beat" | "burst" | "spread" | "split") {
            let fix = verb_to_s(n1);
            flag(cx, out, sv, "who-singular", "Subject-Verb Agreement", i + 1, i + 1,
                format!("'{p} who …' is singular, so the verb should be '{fix}'."), Some(fix));
        }
    }
    // One of my friends are coming / Each and every one of them are here
    if (w == "one" && n1 == "of" && (DETERMINERS.contains(&n2) || matches!(n2, "them" | "us" | "you")))
        || (w == "each" && cx.run(i, &["each", "and", "every", "one", "of"]))
    {
        let from = if w == "one" { i + 2 } else { i + 4 };
        if let Some(j) = cx.find_after(from, &["are", "were", "have"], 4) {
            let clean = (from + 1..j).all(|k| !matches!(cx.lw(k), "who" | "that" | "which" | "where" | "and" | "or" | "but"));
            if let (true, Some(fix)) = (clean, plural_verb_to_singular(cx.lw(j))) {
                flag(cx, out, sv, "one-of", "Subject-Verb Agreement", j, j,
                    format!("The subject is 'one', so the verb should be '{fix}'."), Some(fix.to_string()));
            }
        }
    }
    // Me and my friend goes there
    if cx.starts_clause(i) && w == "me" && n1 == "and" {
        let (len, who) = if DETERMINERS.contains(&n2) { (2, format!("{n2} {n3}")) } else { (1, n2.to_string()) };
        let verb = cx.nx(i, 3 + (len - 1));
        let verbs = ["went", "go", "goes", "are", "were", "is", "was", "have", "had", "has", "will", "would", "can", "could", "do", "did", "does", "ran", "walked", "came", "come", "said", "like", "likes", "love", "loves", "got", "get", "gets", "been", "always", "often", "never", "both"];
        if !who.trim().is_empty() && verbs.contains(&verb) {
            let who = capitalised(&who);
            flag(cx, out, Category::WordChoice, "me-and", "Pronoun Case", i, i + 1 + len,
                format!("As the subject, say '{who} and I' (and put yourself last)."),
                Some(format!("{who} and I")));
        }
    }
    // an advice / a furniture
    if matches!(w, "an" | "a") && matches!(n1, "advice" | "information" | "furniture" | "luggage" | "equipment" | "homework" | "knowledge" | "news" | "research" | "evidence" | "feedback" | "software" | "baggage" | "traffic" | "weather" | "progress" | "garbage" | "jewelry" | "scenery")
        // "a traffic jam", "a news story", "a software engineer" are fine: the word is describing the next noun.
        && (cx.clause_end(i + 1) || PREPS.contains(&n2) || matches!(n2, "is" | "was" | "are" | "were" | "that" | "which" | "and" | "but" | "or" | "so" | "i" | "he" | "she" | "we" | "they" | "you" | "it" | "will" | "would" | "can" | "could" | "has" | "had" | "does" | "did" | "when" | "because" | "if" | "as")) {
        flag(cx, out, wc, "article-uncountable", "Singular and Plural", i, i + 1,
            format!("'{n1}' cannot be counted, so it takes no 'a' or 'an': 'some {n1}'."),
            Some(format!("some {n1}")));
    }
    // We is, you is, they is
    if matches!(w, "we" | "you" | "they") && n1 == "is" && !matches!(p, "what" | "that") {
        flag(cx, out, sv, "pronoun-is", "Subject-Verb Agreement", i + 1, i + 1,
            format!("'{raw}' goes with 'are'."), Some("are".to_string()));
    }

    // ----- Tense and verb forms ---------------------------------------------------------------

    // Yesterday I go to the store.
    if (matches!(w, "i" | "we" | "they" | "you") && past_of_base(n1).is_some())
        || (matches!(w, "he" | "she" | "it") && verb_s_base(n1).is_some_and(|b| past_of_base(&b).is_some()))
    {
        let base = if matches!(w, "he" | "she" | "it") { verb_s_base(n1).unwrap_or_default() } else { n1.to_string() };
        let habitual = sentence_has(cx, i, &["every", "always", "usually", "often", "never", "sometimes",
            "each", "daily", "whenever", "when", "if", "once", "will", "would", "can", "could", "tomorrow",
            "next", "gonna", "going", "should", "must", "might", "may", "maybe", "perhaps"]);
        let clause_word = matches!(p, "if" | "when" | "whenever" | "that" | "unless" | "until" | "before"
            | "after" | "while" | "as" | "how" | "what" | "where" | "why" | "whether" | "to" | "who" | "let" | "lets");
        if !habitual && !clause_word && sentence_is_past(cx, i) && !matches!(base.as_str(), "read" | "put" | "set" | "let" | "cut" | "hit" | "hurt" | "cost") {
            if let Some(past) = past_of_base(&base) {
                flag(cx, out, tn, "past-time-present-verb", "Verb Tense", i + 1, i + 1,
                    format!("The sentence is about the past, so the verb should be '{past}'."),
                    Some(past.to_string()));
            }
        }
    }
    // We done our best.
    if matches!(w, "i" | "we" | "you" | "they" | "he" | "she")
        && n1 == "done"
        && matches!(n2, "my" | "our" | "his" | "her" | "their" | "your" | "the" | "it" | "that" | "this" | "what" | "all" | "everything" | "a" | "an" | "some")
    {
        flag(cx, out, tn, "done-for-did", "Verb Form", i + 1, i + 1,
            "'Done' needs a helper ('have done'); on its own the past tense is 'did'.", Some("did".to_string()));
    }
    // She sung, he drunk ...
    if matches!(w, "i" | "we" | "you" | "they" | "he" | "she") {
        if let Some(&(_, r)) = [("sung", "sang"), ("drunk", "drank"), ("swum", "swam"), ("begun", "began"), ("rung", "rang"), ("shrunk", "shrank"), ("sprung", "sprang")]
            .iter().find(|(x, _)| *x == n1)
        {
            flag(cx, out, tn, "participle-as-past", "Verb Form", i + 1, i + 1,
                format!("On its own the past tense is '{r}'; '{n1}' needs 'have' or 'had'."), Some(r.to_string()));
        }
    }
    // If I was you ...
    if w == "if" && n1 == "i" && n2 == "was" && matches!(n3, "you" | "him" | "her" | "them" | "in") {
        flag(cx, out, tn, "if-i-was", "Verb Form", i + 2, i + 2,
            "For something imagined, use 'were': 'If I were you'.", Some("were".to_string()));
    }
    // ... did good on the test
    if matches!(w, "did" | "done") && n1 == "good" && matches!(n2, "on" | "at") {
        flag(cx, out, wc, "did-good", "Word Choice", i + 1, i + 1,
            "A verb takes an adverb: 'did well'.", Some("well".to_string()));
    }
    // can't hardly
    if matches!(w, "can't" | "couldn't" | "cannot" | "cant") && matches!(n1, "hardly" | "scarcely" | "barely") {
        let pos = match w { "couldn't" => "could", _ => "can" };
        flag(cx, out, wc, "hardly-negative", "Double Negative", i, i + 1,
            format!("'Hardly' is already negative: '{pos} {n1}'."), Some(with_case_of(raw, &format!("{pos} {n1}"))));
    }
    // He didn't say nothing.
    if matches!(w, "didn't" | "don't" | "doesn't" | "can't" | "couldn't" | "won't" | "wouldn't" | "never")
        && is_base_verb(n1) && matches!(n2, "nothing" | "nobody" | "nowhere")
        && (cx.clause_end(i + 2) || matches!(n3, "to" | "about" | "at" | "for" | "else" | "of" | "anymore" | "now" | "again" | "more"))
    {
        let fix = match n2 { "nothing" => "anything", "nobody" => "anybody", _ => "anywhere" };
        flag(cx, out, fr, "double-negative", "Double Negative", i + 2, i + 2,
            format!("Two negatives cancel out; say '{fix}'."), Some(fix.to_string()));
    }
    // I going to the store / She going home / What you doing here?
    if matches!(w, "i" | "he" | "she" | "we" | "they" | "you") && is_gerund_of_verb(n1) && cx.starts_clause(i)
        && !cx.starts_clause(i + 1)
    {
        let aux = match w { "i" => "am", "he" | "she" => "is", _ => "are" };
        flag(cx, out, tn, "missing-be", "Missing Verb", i, i + 1,
            format!("The helper verb is missing: '{raw} {aux} {n1}'."), Some(format!("{raw} {aux} {n1}")));
    }
    if matches!(w, "what" | "where" | "why" | "how" | "who") && matches!(n1, "you" | "he" | "she" | "they" | "we") && is_gerund_of_verb(n2)
        && cx.starts_sentence(i)
    {
        let aux = match n1 { "he" | "she" => "is", _ => "are" };
        flag(cx, out, tn, "missing-be", "Missing Verb", i, i + 2,
            format!("The helper verb is missing: '{raw} {aux} {n1} {n2}'."), Some(format!("{raw} {aux} {n1} {n2}")));
    }
    // He a good man.
    if matches!(w, "he" | "she" | "it") && matches!(n1, "a" | "an") && cx.starts_sentence(i) {
        let mut k = i + 2;
        while k < cx.n() && k <= i + 4 && cx.adj(k - 1) && !cx.clause_end(k - 1) {
            k += 1;
        }
        if cx.clause_end(k - 1) && k - 1 <= i + 4 && k - 1 > i + 1 {
            flag(cx, out, tn, "missing-be", "Missing Verb", i, i + 1,
                format!("The verb is missing: '{raw} is {n1}'."), Some(format!("{raw} is {n1}")));
        }
    }
    // I am agree
    if matches!(w, "am" | "is" | "are" | "was" | "were") && n1 == "agree" {
        flag(cx, out, tn, "am-agree", "Verb Form", i, i + 1,
            "'Agree' is a verb on its own: 'I agree'.", Some("agree".to_string()));
    }
    // He is knowing
    if matches!(w, "am" | "is" | "are" | "was" | "were" | "'m")
        && matches!(n1, "knowing" | "wanting" | "needing" | "liking" | "hating" | "believing" | "understanding" | "preferring" | "owning" | "belonging" | "resembling" | "possessing")
    {
        let stem = n1.trim_end_matches("ing");
        flag(cx, out, tn, "stative-progressive", "Verb Form", i + 1, i + 1,
            format!("'{stem}' describes a state, so it is not used with '-ing': use the simple form."), None);
    }
    // He does not goes / do you wants / does he likes
    if matches!(w, "do" | "does" | "did" | "don't" | "doesn't" | "didn't") {
        let (idx, x) = if n1 == "not" || n1 == "n't" { (i + 2, n2) } else if SUBJ_PRON.contains(&n1) { (i + 2, n2) } else { (i + 1, "") };
        if !x.is_empty() {
            if let Some(base) = verb_s_base(x) {
                if !matches!(x, "does" | "has") {
                    flag(cx, out, tn, "do-support-s", "Verb Form", idx, idx,
                        format!("After '{w}' the verb stays in its base form: '{base}'."), Some(base));
                }
            }
        }
    }
    // He can swims / She must goes
    if matches!(w, "can" | "could" | "would" | "should" | "must" | "might" | "shall" | "will" | "may")
        && SUBJ_PRON.contains(&p)
    {
        if let Some(base) = verb_s_base(n1) {
            if !matches!(n1, "does" | "has") {
                flag(cx, out, tn, "modal-s", "Verb Form", i + 1, i + 1,
                    format!("After '{w}' the verb stays in its base form: '{base}'."), Some(base));
            }
        }
    }
    // They can't to come
    if matches!(w, "can" | "could" | "would" | "should" | "must" | "might" | "can't" | "couldn't" | "wouldn't" | "shouldn't" | "mustn't")
        && n1 == "to" && is_base_verb(n2)
    {
        flag(cx, out, tn, "modal-to", "Verb Form", i + 1, i + 1,
            format!("'{raw}' is followed by the bare verb, without 'to'."), Some(String::new()));
    }
    // He made me to cry / Let me to explain
    if matches!(w, "made" | "make" | "makes" | "let" | "lets") && matches!(n1, "me" | "him" | "her" | "us" | "them" | "you")
        && n2 == "to" && is_base_verb(n3)
    {
        flag(cx, out, tn, "causative-to", "Verb Form", i + 2, i + 2,
            format!("After '{raw} {n1}' the verb has no 'to'."), Some(String::new()));
    }

    // ----- Pronouns -----------------------------------------------------------------------------

    // Her and I are friends / Him and his brother left
    if cx.starts_clause(i) && matches!(w, "her" | "him" | "them") && n1 == "and"
        && matches!(n2, "i" | "he" | "she" | "they" | "we" | "my" | "his" | "her" | "their" | "our" | "the" | "a" | "me")
    {
        let r = subject_form(w);
        flag(cx, out, Category::WordChoice, "object-as-subject", "Pronoun Case", i, i,
            format!("As the subject, use '{r}'."), Some(with_case_of(raw, r)));
    }
    // Them are my books / Me went home / Him is my friend
    if cx.starts_clause(i) {
        let her_verbs = ["is", "are", "was", "were", "went", "goes", "came", "comes", "has", "have", "had", "did", "does", "said"];
        let more = ["will", "can", "could", "would", "should", "don't", "didn't", "doesn't", "won't", "ran", "walked", "looked", "thought", "knew", "am", "do", "go", "come", "want", "like", "love", "need"];
        let hit = match w {
            "her" => her_verbs.contains(&n1),
            "him" | "them" | "me" => her_verbs.contains(&n1) || more.contains(&n1),
            _ => false,
        };
        if hit {
            let r = subject_form(w);
            flag(cx, out, Category::WordChoice, "object-as-subject", "Pronoun Case", i, i,
                format!("As the subject, use '{r}'."), Some(with_case_of(raw, if r == "I" { "I" } else { r })));
        }
    }
    // I gave she the keys
    if matches!(w, "gave" | "give" | "gives" | "giving" | "sent" | "send" | "handed" | "showed" | "offered" | "passed" | "lent" | "brought")
        && matches!(n1, "he" | "she" | "they" | "we") && DETERMINERS.contains(&n2)
    {
        let r = object_form(n1);
        flag(cx, out, Category::WordChoice, "subject-as-object", "Pronoun Case", i + 1, i + 1,
            format!("After '{raw}' the person receiving takes the object form: '{r}'."), Some(r.to_string()));
    }
    // agree with he
    if matches!(w, "with" | "to" | "for" | "from" | "at" | "about" | "of" | "on" | "by" | "behind" | "beside" | "near" | "after" | "against" | "toward" | "towards")
        && matches!(n1, "he" | "she" | "they" | "we")
        && (cx.clause_end(i + 1) || n2.ends_with("ly") || matches!(n2, "too" | "also" | "again" | "now" | "here" | "there" | "today" | "always" | "anyway" | "either" | "instead"))
    {
        let r = object_form(n1);
        flag(cx, out, Category::WordChoice, "subject-as-object", "Pronoun Case", i + 1, i + 1,
            format!("After '{w}' use the object form: '{r}'."), Some(r.to_string()));
    }

    // ----- Confusable words --------------------------------------------------------------------

    if w == "you're" && matches!(n1, "decision" | "problem" | "mistake" | "story" | "lead" | "turn" | "own" | "fault" | "choice" | "opinion" | "advice" | "eyes" | "hands" | "hair" | "mind" | "heart" | "voice" | "car" | "job" | "book")
        || (w == "you're" && n1 == "name" && matches!(n2, "is" | "was"))
    {
        flag(cx, out, wc, "your-youre", "Word Choice", i, i,
            "'You're' means 'you are'. For ownership, use 'your'.", Some(with_case_of(raw, "your")));
    }
    if w == "principle" && matches!(n1, "reason" | "cause" | "purpose" | "goal" | "concern" | "focus" | "function" | "source" | "difference" | "objective" | "argument" | "factor" | "ingredient" | "aim" | "character" | "actor" | "investigator" | "architect" | "dancer") {
        flag(cx, out, wc, "principal", "Word Choice", i, i,
            "'Principal' means main; a 'principle' is a rule or belief.", Some(with_case_of(raw, "principal")));
    }
    // He was to tired to continue
    if w == "to"
        && matches!(n1, "tired" | "sleepy" | "hungry" | "scared" | "afraid" | "busy" | "heavy" | "late" | "early" | "slow" | "fast" | "far" | "soon" | "hot" | "cold" | "big" | "small" | "young" | "old" | "weak" | "tight" | "loud" | "sick" | "sad" | "angry" | "tall" | "short" | "long")
        && (matches!(n2, "to" | "for" | "and" | "but" | "so") || cx.clause_end(i + 1))
        && !cx.clause_has_before(i, &["from", "go", "goes", "went", "going", "turn", "turned", "change", "changed", "changes", "move", "moved", "close", "closer", "back", "next", "compared", "relative"], 6)
    {
        flag(cx, out, wc, "to-too", "Word Choice", i, i,
            "'Too' (meaning 'excessively') is spelled with two o's.", Some(with_case_of(raw, "too")));
    }
    if w == "to"
        && matches!(n1, "quiet" | "good" | "bad" | "nice" | "easy" | "hard" | "difficult" | "dark" | "bright" | "warm" | "cool" | "kind" | "strange" | "simple" | "perfect" | "beautiful" | "pretty" | "light" | "deep" | "wide" | "narrow" | "thick" | "thin" | "rich" | "poor" | "high" | "low" | "much" | "many" | "few")
        && matches!(n2, "to" | "for") && !cx.clause_has_before(i, &["from", "go", "goes", "went", "going", "turn", "turned", "change", "changed", "compared", "relative", "back", "next", "close", "closer", "due", "up", "owe", "owed"], 6)
        && !matches!(cx.pv(i), "up" | "due" | "close" | "next" | "back")
    {
        flag(cx, out, wc, "to-too", "Word Choice", i, i,
            "'Too' (meaning 'excessively') is spelled with two o's.", Some(with_case_of(raw, "too")));
    }
    // she don't care
    if matches!(w, "he" | "she" | "it") && n1 == "don't" {
        flag(cx, out, sv, "he-dont", "Subject-Verb Agreement", i + 1, i + 1,
            format!("'{raw}' goes with 'doesn't'."), Some("doesn't".to_string()));
    }
    // ran passed the house
    if w == "passed" && matches!(p, "ran" | "walked" | "went" | "drove" | "rushed" | "hurried" | "strolled" | "hurtled" | "sped" | "rode" | "swam" | "flew" | "marched" | "jogged" | "crept" | "slipped")
        && (DETERMINERS.contains(&n1) || matches!(n1, "him" | "her" | "them" | "me" | "us" | "it"))
    {
        flag(cx, out, wc, "passed-past", "Word Choice", i, i,
            "Here 'past' (beyond) is meant; 'passed' is the verb 'to pass'.", Some(with_case_of(raw, "past")));
    }
    if matches!(w, "i" | "he" | "she" | "we" | "they" | "you") && n1 == "past" && (DETERMINERS.contains(&n2) || matches!(n2, "him" | "her" | "them" | "me" | "us" | "it")) {
        flag(cx, out, wc, "passed-past", "Word Choice", i + 1, i + 1,
            "The verb is 'passed'; 'past' means beyond.", Some("passed".to_string()));
    }
    // to by a new coat
    if w == "by" && p == "to" && (DETERMINERS.contains(&n1) || matches!(n1, "new" | "more" | "some" | "two" | "three" | "something" | "food")) {
        flag(cx, out, wc, "by-buy", "Word Choice", i, i,
            "To purchase something, the word is 'buy'.", Some(with_case_of(raw, "buy")));
    }
    // She herd a noise
    if w == "herd" && matches!(p, "i" | "he" | "she" | "we" | "they" | "you" | "had" | "have" | "has") {
        flag(cx, out, wc, "herd-heard", "Word Choice", i, i,
            "To hear something is 'heard'; a 'herd' is a group of animals.", Some(with_case_of(raw, "heard")));
    }
    // walked threw the hall
    if w == "threw" && matches!(p, "walked" | "ran" | "went" | "came" | "passed" | "looked" | "moved" | "drove" | "pushed" | "crawled" | "climbed" | "stepped" | "burst" | "cut" | "sailed" | "flew" | "swam" | "stared" | "peered" | "walk" | "run" | "go" | "come" | "look" | "pass" | "walking" | "running" | "going" | "looking")
        && (DETERMINERS.contains(&n1) || matches!(n1, "it" | "them" | "him" | "her" | "me" | "us"))
    {
        flag(cx, out, wc, "threw-through", "Word Choice", i, i,
            "Moving from one side to the other is 'through'; 'threw' is from 'throw'.", Some(with_case_of(raw, "through")));
    }
    // We went their yesterday.
    if w == "their"
        && (matches!(p, "went" | "go" | "going" | "goes" | "been" | "get" | "got" | "stay" | "stayed" | "live" | "lived" | "arrive" | "arrived" | "come" | "came" | "walk" | "walked" | "drive" | "drove" | "fly" | "flew" | "stood" | "sat")
            && (cx.clause_end(i) || matches!(n1, "yesterday" | "today" | "tomorrow" | "last" | "every" | "again" | "before" | "after" | "at" | "until" | "during" | "early" | "late" | "now" | "later" | "soon" | "often" | "once" | "twice" | "tonight" | "alone" | "together" | "too" | "and" | "but" | "so" | "when" | "if" | "because")))
    {
        flag(cx, out, wc, "their-there", "Word Choice", i, i,
            "For a place, the word is 'there'.", Some(with_case_of(raw, "there")));
    }
    // going to meat him
    if w == "meat" && p == "to" && matches!(n1, "him" | "her" | "them" | "me" | "us" | "you" | "up" | "at" | "with" | "again") {
        flag(cx, out, wc, "meat-meet", "Word Choice", i, i,
            "To meet someone is spelled 'meet'.", Some(with_case_of(raw, "meet")));
    }
    // He lead the team
    if w == "lead" && matches!(p, "he" | "she" | "it") && (DETERMINERS.contains(&n1) || matches!(n1, "us" | "them" | "me" | "him" | "her")) {
        flag(cx, out, tn, "lead-led", "Verb Form", i, i,
            "The past tense of 'lead' is 'led' (present: 'leads').", Some(with_case_of(raw, "led")));
    }
    // Everyday I walk
    if w == "everyday" && (matches!(n1, "i" | "we" | "he" | "she" | "they" | "you" | "my" | "the") && cx.starts_clause(i)) {
        flag(cx, out, wc, "every-day", "Word Choice", i, i,
            "As two words, 'every day' means each day; 'everyday' means ordinary.", Some(with_case_of(raw, "every day")));
    }
    // a criteria / a phenomena
    if w == "a" && matches!(n1, "criteria" | "phenomena" | "stimuli" | "alumni" | "bacteria" if n1 != "bacteria") {
        let (single, _) = match n1 {
            "criteria" => ("criterion", 0),
            "phenomena" => ("phenomenon", 0),
            "stimuli" => ("stimulus", 0),
            _ => ("alumnus", 0),
        };
        flag(cx, out, sv, "latin-plural", "Singular and Plural", i + 1, i + 1,
            format!("'{n1}' is a plural; the singular is '{single}'."), Some(single.to_string()));
    }
    // the one whom called me / the man which called me
    if w == "whom" && !PREPS.contains(&p)
        && matches!(n1, "called" | "came" | "went" | "left" | "said" | "is" | "was" | "are" | "were" | "has" | "had" | "wants" | "wanted" | "lives" | "lived" | "works" | "worked" | "knows" | "knew" | "runs" | "ran" | "took" | "made" | "saw" | "asked" | "told" | "tried" | "started" | "loves" | "loved" | "likes" | "liked" | "needs" | "needed" | "seems" | "seemed" | "looked")
    {
        flag(cx, out, wc, "who-whom", "Who or Whom", i, i,
            "'Whom' is for the object. Here the person does the action, so use 'who'.", Some(with_case_of(raw, "who")));
    }
    if w == "which" && matches!(p, "man" | "woman" | "person" | "boy" | "girl" | "guy" | "people" | "friend" | "child" | "children" | "men" | "women" | "doctor" | "teacher" | "neighbor" | "neighbour" | "someone" | "anyone" | "everyone" | "those")
        && !matches!(n1, "of")
    {
        flag(cx, out, wc, "which-who", "Who or Which", i, i,
            "For people, use 'who' (or 'that'), not 'which'.", Some(with_case_of(raw, "who")));
    }

    // ----- Prepositions ----------------------------------------------------------------------

    // about of
    if w == "about" && n1 == "of" {
        flag(cx, out, pr, "about-of", "Preposition", i, i + 1, "'About of' is not needed: use 'about'.", Some(with_case_of(raw, "about")));
    }
    // good at to dance
    if matches!(w, "at" | "of" | "with") && n1 == "to" && is_base_verb(n2) && !matches!(p, "get" | "got" | "gets" | "getting" | "up" | "back" | "come" | "came") {
        flag(cx, out, pr, "prep-to", "Preposition", i + 1, i + 1,
            format!("After '{w}' a verb takes '-ing' (not 'to')."), None);
    }
    // agree to you
    if matches!(w, "agree" | "agrees" | "agreed" | "agreeing") && n1 == "to" && matches!(n2, "you" | "me" | "him" | "us") && cx.clause_end(i + 2) {
        flag(cx, out, pr, "agree-with", "Preposition", i + 1, i + 1,
            "You agree 'with' a person (and 'to' a plan).", Some("with".to_string()));
    }
    // suggested me to go
    if matches!(w, "suggest" | "suggests" | "suggested" | "suggesting") && matches!(n1, "me" | "him" | "her" | "us" | "them" | "you") && n2 == "to" {
        flag(cx, out, pr, "suggest-object", "Verb Pattern", i, i + 2,
            "'Suggest' does not take 'someone to do': say 'suggested that I go'.", None);
    }
    // I am waiting you
    if matches!(w, "wait" | "waits" | "waited" | "waiting") && matches!(n1, "you" | "him" | "her" | "them" | "me" | "us")
        && (cx.clause_end(i + 1) || matches!(n2, "at" | "in" | "here" | "there" | "outside" | "inside" | "downstairs" | "upstairs" | "since" | "until" | "all" | "every" | "tomorrow" | "today" | "tonight"))
    {
        flag(cx, out, pr, "wait-for", "Preposition", i, i, "You wait 'for' someone.", Some(format!("{raw} for")));
    }
    // He entered into the room
    if matches!(w, "entered" | "enter" | "enters" | "entering") && n1 == "into"
        && matches!(if DETERMINERS.contains(&n2) && !matches!(n3, "room" | "house" | "kitchen" | "building" | "hall" | "office" | "shop" | "store" | "church" | "cave" | "forest" | "tent" | "car" | "garden" | "city" | "town" | "bedroom" | "bathroom" | "barn" | "castle" | "village" | "lobby" | "hallway") { n4 } else { n3 }, "room" | "house" | "kitchen" | "building" | "hall" | "office" | "shop" | "store" | "church" | "cave" | "forest" | "tent" | "car" | "garden" | "city" | "town" | "bedroom" | "bathroom" | "barn" | "castle" | "village" | "lobby" | "hallway")
        && (DETERMINERS.contains(&n2))
    {
        flag(cx, out, pr, "enter-into", "Preposition", i + 1, i + 1, "'Enter' already means 'go into'.", Some(String::new()));
    }

    // ----- Style and punctuation words ---------------------------------------------------------

    if matches!(w, "however" | "moreover" | "furthermore" | "nevertheless" | "nonetheless") && cx.starts_sentence(i)
        && matches!(n1, "i" | "he" | "she" | "we" | "they" | "you" | "it" | "the" | "this" | "that" | "there" | "my" | "his" | "her" | "our" | "their" | "i'm" | "it's" | "that's" | "there's")
    {
        flag(cx, out, Category::Punctuation, "intro-comma", "Comma", i, i,
            format!("A comma usually follows an introductory '{w}'."), Some(format!("{raw},")));
    }
    if MONTHS.iter().chain(WEEKDAYS.iter()).any(|m| *m == w) && raw.starts_with(char::is_lowercase) && w != "march" {
        let rep = capitalised(raw);
        flag(cx, out, Category::Punctuation, "capital-name", "Capitalization", i, i,
            "Days and months start with a capital letter.", Some(rep));
    }
    if matches!(w, "paris" | "london" | "tokyo" | "berlin" | "madrid" | "moscow" | "beijing" | "america" | "england" | "france" | "germany" | "japan" | "canada" | "australia" | "europe" | "asia" | "africa" | "christmas" | "easter" | "halloween" | "thanksgiving")
        && raw.starts_with(char::is_lowercase)
    {
        flag(cx, out, Category::Punctuation, "capital-name", "Capitalization", i, i,
            "Names of places and holidays start with a capital letter.", Some(capitalised(raw)));
    }
    // she walked in.   (the very first word of the text)
    if i == 0 && raw.chars().count() > 2 && raw.starts_with(char::is_lowercase) && !raw.chars().any(char::is_uppercase)
        && !cx.text.trim_start().starts_with(['"', '\u{201C}', '(', '[']) && cx.text.get(..cx.toks[0].start).is_some_and(|s| s.trim().is_empty())
        && cx.text[cx.toks[0].end..].chars().next().is_some_and(|c| c == ' ')
        && cx.text.contains(['.', '!', '?'])
    {
        flag(cx, out, Category::Punctuation, "capital-start", "Capitalization", i, i,
            "A sentence starts with a capital letter.", Some(capitalised(raw)));
    }
}

/// Spacing problems that are about characters, not words.
fn char_rules(text: &str, on: &EnabledCategories, out: &mut Out) {
    if !on.is_on(Category::Punctuation) {
        return;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for k in 1..chars.len().saturating_sub(1) {
        let (pos, c) = chars[k];
        let before = chars[k - 1].1;
        let after = chars[k + 1].1;
        // hello.She  /  Wait!why   (no space after the mark)
        if matches!(c, '.' | '!' | '?') && before.is_alphabetic() && after.is_alphabetic() {
            if c == '.' {
                // Only a lower-case word, then Capital + lower-case: "hello.She", not "U.S.A" or "a.m.".
                let word_before: String = text[..pos].chars().rev().take_while(|c| c.is_alphabetic()).collect::<Vec<_>>().into_iter().rev().collect();
                let word_after: String = text[pos + 1..].chars().take_while(|c| c.is_alphabetic()).collect();
                let lower_before = word_before.chars().count() >= 2 && word_before.chars().all(char::is_lowercase);
                let cap_after = word_after.chars().count() >= 2 && word_after.starts_with(char::is_uppercase) && word_after.chars().skip(1).all(char::is_lowercase);
                let abbr = matches!(word_before.to_lowercase().as_str(), "mr" | "mrs" | "ms" | "dr" | "prof" | "st" | "vs" | "etc" | "inc" | "ltd" | "no");
                if lower_before && cap_after && !abbr {
                    push(out, pos..pos + 1, Category::Punctuation, "space-after-stop", "Spacing",
                        "A space is missing after the full stop.".to_string(), Some(". ".to_string()));
                }
            } else if after.is_lowercase() || after.is_uppercase() {
                let word_after: String = text[pos + c.len_utf8()..].chars().take_while(|c| c.is_alphabetic()).collect();
                if word_after.chars().count() >= 2 {
                    push(out, pos..pos + c.len_utf8(), Category::Punctuation, "space-after-mark", "Spacing",
                        format!("A space is missing after '{c}'."), Some(format!("{c} ")));
                }
            }
        }
        // two spaces inside a sentence
        if c == ' ' && after == ' ' && before.is_alphanumeric() {
            let run_end = text[pos..].find(|c: char| c != ' ').map_or(text.len(), |n| pos + n);
            let next = text[run_end..].chars().next();
            if next.is_some_and(|n| n.is_alphanumeric()) && !matches!(before, '.' | '!' | '?' | ':') {
                push(out, pos..run_end, Category::Punctuation, "double-space", "Spacing",
                    "There are two spaces in a row.".to_string(), Some(" ".to_string()));
            }
        }
        // stopped..and looked   (exactly two dots)
        if c == '.' && after == '.' && before != '.' && chars.get(k + 2).is_none_or(|&(_, d)| d != '.') {
            push(out, pos..pos + 2, Category::Punctuation, "two-dots", "Ellipsis",
                "Two dots in a row: use one full stop or a three-dot ellipsis.".to_string(), Some("...".to_string()));
        }
    }
}

#[cfg(test)]
mod polish_tests {
    use super::*;

    fn rules(text: &str) -> Vec<&'static str> {
        check_grammar(text, &EnabledCategories::default())
            .into_iter()
            .map(|i| i.rule)
            .collect()
    }

    #[test]
    fn catches_the_new_patterns() {
        for (text, rule) in [
            ("The dogs barks all night.", "plural-subject-verb-s"),
            ("My brother and I was there.", "compound-subject"),
            ("Yesterday I go to the store.", "past-time-present-verb"),
            ("I going to the store.", "missing-be"),
            ("What you doing here?", "missing-be"),
            ("She does not likes it.", "do-support-s"),
            ("He can swims well.", "modal-s"),
            ("Me went home.", "object-as-subject"),
            ("I gave she the keys.", "subject-as-object"),
            ("He ran passed the house.", "passed-past"),
            ("It was hello.She waved.", "space-after-stop"),
        ] {
            assert!(rules(text).contains(&rule), "{text} should give {rule}, got {:?}", rules(text));
        }
    }

    #[test]
    fn leaves_correct_prose_alone() {
        for text in [
            "The leader of the dogs barks at night.",
            "You're friends now, and you're family.",
            "Their going was sudden.",
            "He went from tall to short.",
            "Yesterday I went to the store.",
            "I go there every day.",
            "The kids drinks menu is long.",
            "Her will to live was strong.",
            "Hello there.  She nodded.",
        ] {
            assert!(rules(text).is_empty(), "{text} should be clean, got {:?}", rules(text));
        }
    }
}
