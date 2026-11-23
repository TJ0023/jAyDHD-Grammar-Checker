//! The window: the editor fills it, and the Grammar Checker, Dialogue Formatting, Reading Level,
//! Keyword Density and Sentence Length boxes float over it (drag each by its title), as in
//! WordCraft. The typing pets float too.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};

use crate::analysis::{
    self, Category, EnabledCategories, Issue, Keyword, LengthBucket, SentenceLength,
};
use crate::dialogue::{self, DialogueIssue, QuoteStyle};
use crate::thesaurus;
use crate::pets::{self, PetRuntime};
use crate::settings::{DIALOGUE_SIZE, MAX_ENTRY_CHARS, Settings};
use crate::source::{self, TextSource};
use crate::ui_kit::{self, BOX_W, Icon, color};

const EDITOR_ID: &str = "grammar_desk_editor";

/// How often the whole-document numbers (counts, keywords, sentence lengths) catch up while typing.
const GLOBAL_EVERY: f64 = 0.5;

/// One grammar flag, in byte offsets of the whole text.
#[derive(Clone)]
struct Flag {
    range: Range<usize>,
    issue: Issue,
    shown: String,
}

/// One dialogue flag, in byte offsets of the whole text.
#[derive(Clone)]
struct DFlag {
    range: Range<usize>,
    issue: DialogueIssue,
    shown: String,
    /// Where the fix goes and what goes there.
    edit: Option<(Range<usize>, String)>,
    preview: Arc<dialogue::Preview>,
}

/// One sentence with its length range, for the Sentence Length box and the page tint.
struct Tint {
    range: Range<usize>,
    info: SentenceLength,
    /// The start of the sentence, for the list.
    preview: String,
}

/// Something a click asks for; run after the frame is drawn.
enum Action {
    /// Select this byte range in the editor.
    GoTo(Range<usize>),
    /// Replace this byte range with the text, and select the new text.
    Replace(Range<usize>, String),
    AddToDictionary(String),
}

/// What changes the paragraph analysis: if this differs, every paragraph is checked again.
#[derive(PartialEq, Clone)]
struct RuleKey {
    grammar_on: bool,
    categories: [bool; 6],
    dialogue_on: bool,
    style: QuoteStyle,
}

pub struct App {
    text: String,
    status: String,
    settings: Settings,
    saved_settings: Settings,

    // Analysis, kept by paragraph text so unchanged paragraphs are never checked again.
    analysed: Option<(String, RuleKey)>,
    grammar_cache: HashMap<String, Vec<Issue>>,
    dialogue_cache: HashMap<String, Vec<(DialogueIssue, Arc<dialogue::Preview>)>>,
    flags: Vec<Flag>,
    dflags: Vec<DFlag>,
    detected_single: bool,

    // The whole-document numbers, refreshed at most twice a second.
    globals_for: Option<(u64, bool, usize)>,
    globals_at: f64,
    counts: analysis::Counts,
    grade: Option<analysis::DaleChall>,
    keywords: Vec<Keyword>,
    tints: Vec<Tint>,
    bucket_counts: [usize; 6],
    sentence_total: usize,
    sentence_words: usize,
    sentence_longest: usize,

    // The boxes.
    ignored: HashSet<String>,
    dialogue_ignored: HashSet<String>,
    current: usize,
    dialogue_current: usize,
    bucket_filter: Option<LengthBucket>,
    shown_words: HashSet<String>,
    /// Bumped by "Reset box positions" so every box goes back to its first place.
    layout_gen: u32,

    // Windows.
    guide_open: Option<&'static str>,
    guide_gen: u32,
    guide_category: &'static str,
    guide_query: String,
    dictionary_open: bool,
    /// When the Save settings button was last pressed (for the "Saved" note).
    saved_at: Option<f64>,
    dictionary_input: String,
    dictionary_confirm: Option<usize>,
    dictionary_backup: Option<(usize, String)>,

    pets: PetRuntime,

    /// Watching other windows: the running watcher, what it last saw, and the selected text.
    watch: Option<TextSource>,
    watching: String,
    selected: String,

    // The Thesaurus box: the word being looked up, what was found, and which meaning shows.
    thes_query: String,
    thes_seen: String,
    thes_for: String,
    thes_senses: Vec<thesaurus::Sense>,
    thes_index: usize,
    thes_warm: bool,

    /// The capture hotkeys (for Google Docs), what they last brought back, and a note about it.
    capture: Option<crate::capture::Hotkeys>,
    /// While set, the text came from a hotkey capture of this process (the browser), and
    /// whatever Windows reads from that process is ignored.
    captured_pid: Option<u32>,
    capture_note: String,
    /// Google Docs through its own interface: the worker, how sign-in stands, which browser
    /// process holds the open document, the document's name, a note, and a pasted link.
    docs: Option<crate::docs::Docs>,
    /// Typing and selecting seen system-wide (only that it happened, never which key).
    input: Option<std::sync::mpsc::Receiver<crate::inputwatch::Input>>,
    docs_setup: Option<crate::docs::Setup>,
    docs_pid: Option<u32>,
    docs_title: String,
    docs_note: String,
    docs_link: String,
    /// The tray icon.
    tray: Option<crate::tray::Tray>,
}

fn hash_of(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // SVG pictures for the placeholders, and a random one to start with.
        egui_extras::install_image_loaders(&cc.egui_ctx);
        ui_kit::set_random_placeholder_seed(&cc.egui_ctx);
        // The boxes are drawn for dark text on a light surface, as in WordCraft.
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let settings = Settings::load();
        let pets = PetRuntime::new(&settings);
        let watch = if settings.active {
            let repaint = cc.egui_ctx.clone();
            source::start(move || repaint.request_repaint())
        } else {
            None
        };
        let (input_rx, docs_input_rx) = {
            let repaint = cc.egui_ctx.clone();
            crate::inputwatch::start(move || repaint.request_repaint())
        };
        let docs = {
            let repaint = cc.egui_ctx.clone();
            let d = crate::docs::start(move || repaint.request_repaint(), docs_input_rx);
            d.send(crate::docs::Command::AutoSelect(settings.docs_auto_select));
            Some(d)
        };
        Self {
            text: String::new(),
            status: String::new(),
            saved_settings: settings.clone(),
            settings,
            analysed: None,
            grammar_cache: HashMap::new(),
            dialogue_cache: HashMap::new(),
            flags: Vec::new(),
            dflags: Vec::new(),
            detected_single: false,
            globals_for: None,
            globals_at: -10.0,
            counts: analysis::Counts::default(),
            grade: None,
            keywords: Vec::new(),
            tints: Vec::new(),
            bucket_counts: [0; 6],
            sentence_total: 0,
            sentence_words: 0,
            sentence_longest: 0,
            ignored: HashSet::new(),
            dialogue_ignored: HashSet::new(),
            current: 0,
            dialogue_current: 0,
            bucket_filter: None,
            shown_words: HashSet::new(),
            layout_gen: 0,
            guide_open: None,
            guide_gen: 0,
            saved_at: None,
            guide_category: "dialogue",
            guide_query: String::new(),
            dictionary_open: false,
            dictionary_input: String::new(),
            dictionary_confirm: None,
            dictionary_backup: None,
            pets,
            watch,
            watching: String::new(),
            selected: String::new(),
            thes_query: String::new(),
            thes_seen: String::new(),
            thes_for: String::new(),
            thes_senses: Vec::new(),
            thes_index: 0,
            thes_warm: false,
            capture: {
                let repaint = cc.egui_ctx.clone();
                crate::capture::start(move || repaint.request_repaint())
            },
            captured_pid: None,
            capture_note: String::new(),
            docs,
            input: Some(input_rx),
            docs_setup: None,
            docs_pid: None,
            docs_title: String::new(),
            docs_note: String::new(),
            docs_link: String::new(),
            tray: {
                let repaint = cc.egui_ctx.clone();
                crate::tray::Tray::new(move || repaint.request_repaint())
            },
        }
    }

    // ---------------------------------------------------------------------------------------
    // Settings-derived helpers
    // ---------------------------------------------------------------------------------------

    fn enabled(&self) -> EnabledCategories {
        let on = |c: Category| {
            Category::ALL
                .iter()
                .position(|x| *x == c)
                .and_then(|i| self.settings.categories.get(i))
                .copied()
                .unwrap_or(true)
        };
        EnabledCategories {
            subject_verb: on(Category::SubjectVerb),
            fragment: on(Category::Fragment),
            punctuation: on(Category::Punctuation),
            tense: on(Category::Tense),
            word_choice: on(Category::WordChoice),
            preposition: on(Category::Preposition),
        }
    }

    fn style(&self) -> QuoteStyle {
        match self.settings.dialogue_style {
            1 => QuoteStyle::Double,
            2 => QuoteStyle::Single,
            _ if self.detected_single => QuoteStyle::Single,
            _ => QuoteStyle::Double,
        }
    }

    fn rule_key(&self) -> RuleKey {
        RuleKey {
            grammar_on: self.settings.grammar_on,
            categories: self.settings.categories,
            dialogue_on: self.settings.dialogue_on,
            style: self.style(),
        }
    }

    // ---------------------------------------------------------------------------------------
    // Analysis
    // ---------------------------------------------------------------------------------------

    fn refresh(&mut self, now: f64) {
        // The quotation style first: it decides how the dialogue rules read the text.
        if self.analysed.as_ref().is_none_or(|(t, _)| *t != self.text) {
            self.detected_single = dialogue::detect_style(&self.text) == QuoteStyle::Single;
        }
        let key = self.rule_key();
        let up_to_date = self
            .analysed
            .as_ref()
            .is_some_and(|(t, k)| *t == self.text && *k == key);
        if !up_to_date {
            if self.analysed.as_ref().is_none_or(|(_, k)| *k != key) {
                self.grammar_cache.clear();
                self.dialogue_cache.clear();
            }
            self.analyse_paragraphs(key);
        }
        self.refresh_globals(now);
    }

    fn analyse_paragraphs(&mut self, key: RuleKey) {
        let on = self.enabled();
        let style = key.style;
        let text = self.text.clone();
        let mut grammar_fresh: HashMap<String, Vec<Issue>> = HashMap::new();
        let mut dialogue_fresh: HashMap<String, Vec<(DialogueIssue, Arc<dialogue::Preview>)>> =
            HashMap::new();
        let mut flags = Vec::new();
        let mut dflags = Vec::new();
        let mut start = 0usize;
        for para in text.split('\n') {
            if !grammar_fresh.contains_key(para) {
                let found = self.grammar_cache.remove(para).unwrap_or_else(|| {
                    if key.grammar_on {
                        crate::analysis_proof::check_all(para, &on)
                    } else {
                        Vec::new()
                    }
                });
                grammar_fresh.insert(para.to_string(), found);
            }
            for issue in grammar_fresh.get(para).map_or(&[][..], Vec::as_slice) {
                let range = start + issue.range.start..start + issue.range.end;
                let shown = para.get(issue.range.clone()).unwrap_or("").to_string();
                flags.push(Flag {
                    range,
                    issue: issue.clone(),
                    shown,
                });
            }
            if key.dialogue_on {
                if !dialogue_fresh.contains_key(para) {
                    let found = self.dialogue_cache.remove(para).unwrap_or_else(|| {
                        dialogue::check(para, style)
                            .into_iter()
                            .map(|issue| {
                                let preview = Arc::new(dialogue::preview(para, &issue));
                                (issue, preview)
                            })
                            .collect()
                    });
                    dialogue_fresh.insert(para.to_string(), found);
                }
                for (issue, preview) in dialogue_fresh.get(para).map_or(&[][..], Vec::as_slice) {
                    let range = start + issue.range.start..start + issue.range.end;
                    let shown = para.get(issue.range.clone()).unwrap_or("").to_string();
                    let edit = issue
                        .edit()
                        .map(|(r, new)| (start + r.start..start + r.end, new));
                    dflags.push(DFlag {
                        range,
                        issue: issue.clone(),
                        shown,
                        edit,
                        preview: Arc::clone(preview),
                    });
                }
            }
            start += para.len() + 1;
        }
        self.grammar_cache = grammar_fresh;
        self.dialogue_cache = dialogue_fresh;
        self.flags = flags;
        self.dflags = dflags;
        self.analysed = Some((text, key));
    }

    /// Counts, reading level, keywords and sentence lengths for the whole text. After an edit
    /// they wait up to half a second, so typing stays smooth. Returns whether they changed.
    fn refresh_globals(&mut self, now: f64) -> bool {
        let s = &self.settings;
        if !(s.reading_on || s.keywords_on || s.sentences_on) {
            return false;
        }
        let stamp = (
            hash_of(&self.text),
            self.settings.keywords_skip_stop_words,
            self.settings.keywords_shown,
        );
        if self.globals_for == Some(stamp) {
            return false;
        }
        let text_only = self
            .globals_for
            .is_some_and(|(h, a, b)| (a, b) == (stamp.1, stamp.2) && h != stamp.0);
        if text_only && now - self.globals_at < GLOBAL_EVERY {
            return false;
        }
        let text = &self.text;
        self.counts = analysis::counts(text);
        self.grade = analysis::dale_chall(text);
        self.keywords = analysis::keyword_density(
            text,
            self.settings.keywords_skip_stop_words,
            self.settings.keywords_shown,
        );
        let lengths = analysis::sentence_lengths(text);
        let mut tints = Vec::with_capacity(lengths.len());
        let mut counts = [0usize; 6];
        let (mut total, mut words, mut longest) = (0usize, 0usize, 0usize);
        for info in lengths {
            let sentence = text.get(info.range.clone()).unwrap_or("");
            let mut preview: String = sentence.chars().take(60).collect();
            if sentence.chars().count() > 60 {
                preview.push('\u{2026}');
            }
            if let Some(i) = LengthBucket::ALL.iter().position(|x| *x == info.bucket) {
                counts[i] += 1;
            }
            total += 1;
            words += info.words;
            longest = longest.max(info.words);
            tints.push(Tint {
                range: info.range.clone(),
                info,
                preview,
            });
        }
        self.tints = tints;
        self.bucket_counts = counts;
        self.sentence_total = total;
        self.sentence_words = words;
        self.sentence_longest = longest;
        self.globals_for = Some(stamp);
        self.globals_at = now;
        true
    }

    /// True when there is nothing to check: no letters or digits in the text, or no window
    /// being watched yet.
    fn nothing_loaded(&self) -> bool {
        !self.text.chars().any(char::is_alphanumeric)
            || (self.watch.is_some() && self.watching.is_empty())
    }

    fn dictionary_set(&self) -> HashSet<String> {
        self.settings
            .dictionary
            .iter()
            .map(|d| d.to_lowercase())
            .collect()
    }

    /// Indices into `self.flags` of the flags that are not ignored or in the dictionary.
    fn visible_flags(&self) -> Vec<usize> {
        let dict = self.dictionary_set();
        (0..self.flags.len())
            .filter(|&i| {
                let f = &self.flags[i];
                !self
                    .ignored
                    .contains(&format!("{}|{}", f.issue.rule, f.shown))
                    && !dict.contains(&f.shown.trim().to_lowercase())
            })
            .collect()
    }

    /// What Ignore remembers about a dialogue flag: the rule, the flagged text and the text
    /// around it, so the Ignore ends as soon as that paragraph changes.
    fn dialogue_key(f: &DFlag) -> String {
        format!("{}|{}|{}", f.issue.rule, f.shown, f.preview.window)
    }

    fn visible_dialogue(&self) -> Vec<usize> {
        (0..self.dflags.len())
            .filter(|&i| {
                !self
                    .dialogue_ignored
                    .contains(&Self::dialogue_key(&self.dflags[i]))
            })
            .collect()
    }

    fn run(&mut self, action: Action) {
        match action {
            // Selecting text in the other window comes with Apply Change, in the next step.
            Action::GoTo(range) => {
                if self.docs_pid.is_some() {
                    if let (Some(docs), Some(needle)) =
                        (&self.docs, crate::doc_map::find_needle(&self.text, range))
                    {
                        docs.send(crate::docs::Command::Find(needle));
                        self.docs_note = "Jumping to it in the document\u{2026}".to_string();
                    }
                }
            }
            Action::Replace(range, new) => {
                if self.docs_pid.is_some() {
                    if let (Some(docs), Some(old)) = (&self.docs, self.text.get(range.clone())) {
                        docs.send(crate::docs::Command::Apply {
                            range,
                            old: old.to_string(),
                            new,
                        });
                        self.docs_note = "Applying the fix in Google Docs\u{2026}".to_string();
                    }
                    return;
                }
                if self.captured_pid.is_some() {
                    // The text lives in another program we cannot edit yet: hand over the fix.
                    self.capture_note = if crate::capture::copy_text(&new) {
                        format!(
                            "Copied \u{201C}{new}\u{201D}. In the document, select the flagged text and press Ctrl+V."
                        )
                    } else {
                        "Could not copy to the clipboard.".to_string()
                    };
                    return;
                }
                if self.watch.is_some() {
                    self.status =
                        "Applying changes to other windows comes in the next step.".to_string();
                    return;
                }
                if self.text.get(range.clone()).is_some() {
                    self.text.replace_range(range, &new);
                }
            }
            Action::AddToDictionary(word) => self.settings.add_word(&word),
        }
    }

    // ---------------------------------------------------------------------------------------
    // Files
    // ---------------------------------------------------------------------------------------

    // ---------------------------------------------------------------------------------------
    // Toolbar and editor
    // ---------------------------------------------------------------------------------------

    /// The Google Docs part of the control panel: sign-in help and status.
    fn docs_panel(&mut self, ui: &mut Ui) {
        use crate::docs::{Command, Setup};
        match self.docs_setup.clone() {
            Some(Setup::NoClientFile(path)) => {
                ui.label(
                    RichText::new(format!(
                        "Google Docs: put your downloaded Google file here, named client_secret.json:\n{}",
                        path.parent().map(|p| p.display().to_string()).unwrap_or_default()
                    ))
                    .small(),
                );
                if ui.small_button("Open that folder").clicked() {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                        #[cfg(windows)]
                        let _ = std::process::Command::new("explorer").arg(dir).spawn();
                    }
                }
            }
            Some(Setup::NeedSignIn) => {
                ui.label(RichText::new("Google Docs is open. Sign in once to check it live.").small());
                if ui.button("Sign in to Google").clicked() {
                    if let Some(d) = &self.docs {
                        d.send(Command::SignIn);
                    }
                }
            }
            Some(Setup::SigningIn) => {
                ui.label(RichText::new("Finish signing in in your browser\u{2026}").small());
            }
            Some(Setup::Ready) | None => {}
        }
        if self.docs_pid.is_some() {
            ui.label(RichText::new(format!("Live: {}", self.docs_title)).small());
        }
        if !self.docs_note.is_empty() {
            ui.label(RichText::new(&self.docs_note).small());
        }
        if matches!(self.docs_setup, Some(Setup::Ready)) {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.docs_link)
                        .hint_text("Paste a Docs link (only if it was not found)")
                        .desired_width(210.0),
                );
                if ui.small_button("Use").clicked() && !self.docs_link.trim().is_empty() {
                    if let Some(d) = &self.docs {
                        d.send(Command::UseLink(std::mem::take(&mut self.docs_link)));
                    }
                }
            });
            if ui
                .checkbox(
                    &mut self.settings.docs_auto_select,
                    "Pick up the words I select (presses Copy for me)",
                )
                .changed()
            {
                if let Some(d) = &self.docs {
                    d.send(Command::AutoSelect(self.settings.docs_auto_select));
                }
            }
            if ui.small_button("Sign out of Google").clicked() {
                if let Some(d) = &self.docs {
                    d.send(Command::SignOut);
                }
            }
        }
    }

    /// Count the typing seen anywhere, so the pets move while you type in another program.
    fn poll_input(&mut self) {
        let Some(rx) = &self.input else { return };
        let keys = rx
            .try_iter()
            .filter(|i| *i == crate::inputwatch::Input::Key)
            .count();
        if keys > 0 {
            self.pets.add_external_keys(keys);
        }
    }

    /// Take what the Google Docs worker found.
    fn poll_docs(&mut self) {
        use crate::docs::{Event, Setup};
        let Some(docs) = &self.docs else { return };
        let events: Vec<_> = docs.rx.try_iter().collect();
        for event in events {
            match event {
                Event::Setup(s) => {
                    if s == Setup::Ready {
                        self.docs_note.clear();
                    }
                    self.docs_setup = Some(s);
                }
                Event::Doc { title, pid, text } => {
                    // The live document replaces anything captured by hotkey.
                    self.captured_pid = None;
                    self.capture_note.clear();
                    self.docs_pid = Some(pid);
                    self.docs_title = title.clone();
                    self.watching = format!("Google Docs: {title}");
                    self.selected.clear();
                    if text != self.text {
                        self.text = text;
                    }
                }
                Event::Left => {
                    if self.docs_pid.take().is_some() {
                        self.docs_title.clear();
                        self.text.clear();
                        self.watching.clear();
                    }
                }
                Event::Note(n) => self.docs_note = n,
                Event::Selection(text) => self.selected = text,
            }
        }
    }

    /// Take what the hotkeys captured, if anything.
    fn poll_capture(&mut self) {
        let Some(hotkeys) = &self.capture else { return };
        let events: Vec<_> = hotkeys.rx.try_iter().collect();
        for event in events {
            match event {
                Ok(c) => {
                    let words = analysis::counts(&c.text).words;
                    self.capture_note = format!(
                        "Checking {} captured with the hotkey ({} words). Press it again to refresh.",
                        if c.whole_document { "the whole document" } else { "the selection" },
                        ui_kit::group(words)
                    );
                    self.captured_pid = Some(c.pid);
                    self.watching = "text captured with the hotkey".to_string();
                    self.selected.clear();
                    self.text = c.text;
                }
                Err(message) => self.capture_note = message,
            }
        }
    }

    /// Take the newest snapshot from the watcher, if there is one, as the text to check.
    fn poll_watch(&mut self) {
        let Some(watch) = &self.watch else { return };
        let mut latest = None;
        while let Ok(snapshot) = watch.rx.try_recv() {
            latest = Some(snapshot);
        }
        if let Some(snapshot) = latest {
            // After a hotkey capture the browser's own text boxes (address bar, search) would
            // replace the captured document, so they are ignored until you stop using it or
            // switch to another program.
            if self.docs_pid == Some(snapshot.process_id) {
                return;
            }
            if let Some(pid) = self.captured_pid {
                if snapshot.process_id == pid {
                    return;
                }
                self.captured_pid = None;
                self.capture_note.clear();
            }
            self.watching = snapshot.describe();
            self.selected = snapshot.selected;
            let text = snapshot.text.replace("\r\n", "\n").replace('\r', "\n");
            if text != self.text {
                self.text = text;
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // The boxes
    // ---------------------------------------------------------------------------------------

    fn grammar_box(&mut self, ui: &mut Ui, s: &Settings) -> (bool, Option<Action>) {
        let text_color = color(s.box_text);
        let vis = self.visible_flags();
        let total = vis.len();
        if self.current >= total {
            self.current = 0;
        }
        let cur = vis.get(self.current).map(|&i| self.flags[i].clone());
        let current = self.current;
        let nothing_loaded = self.nothing_loaded();
        let mut action = None;
        let mut ignore = None;
        let mut step = 0isize;
        let toggled = ui_kit::card(
            ui,
            s,
            Some(Icon::Magnifier),
            "Grammar Checker",
            s.collapsed[0],
            |ui| {
                let Some(f) = &cur else {
                    let msg = if !s.grammar_on {
                        "Grammar checking is off."
                    } else if nothing_loaded {
                        "Nothing to check yet."
                    } else {
                        "No issues found."
                    };
                    if nothing_loaded || !s.grammar_on {
                        ui_kit::empty_state(ui, 0, text_color, s.box_font_size, msg);
                    } else {
                        ui.label(RichText::new(msg).color(text_color).size(s.box_font_size));
                    }
                    return;
                };
                // The suggestion on the left, Ignore on the right.
                ui.horizontal(|ui| {
                    if let Some(rep) = &f.issue.replacement {
                        ui.label(
                            RichText::new(rep)
                                .strong()
                                .size(s.box_font_size)
                                .color(text_color),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Ignore").clicked() {
                            ignore = Some(format!("{}|{}", f.issue.rule, f.shown));
                        }
                    });
                });
                ui.vertical_centered(|ui| {
                    ui_kit::chip(ui, s, Color32::from_gray(225), f.shown.clone())
                });
                ui.label(
                    RichText::new(format!("Issue: {}", f.issue.title))
                        .strong()
                        .size(s.box_font_size)
                        .color(text_color),
                );
                ui.label(
                    RichText::new(&f.issue.explanation)
                        .size(s.box_font_size - 1.0)
                        .color(text_color),
                );
                ui.add_space(2.0);
                // The two things you do with the issue: apply the fix (left) or teach it (right).
                ui.horizontal(|ui| {
                    let fix = f.issue.replacement.clone();
                    if ui
                        .add_enabled(fix.is_some(), egui::Button::new("Apply Change"))
                        .clicked()
                        && let Some(text) = fix
                    {
                        action = Some(Action::Replace(f.range.clone(), text));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Add to Dictionary").clicked() {
                            action = Some(Action::AddToDictionary(f.shown.clone()));
                        }
                    });
                });
                // Stepping through the issues sits alone, centred at the bottom, away from the
                // buttons above so it is not pressed by mistake.
                if total > 1 {
                    step = ui_kit::stepper(ui, text_color, current, total);
                }
            },
        );
        if let Some(key) = ignore {
            self.ignored.insert(key);
        }
        if step != 0 && total > 0 {
            self.current = (self.current as isize + step).clamp(0, total as isize - 1) as usize;
            if let Some(&i) = vis.get(self.current) {
                action = Some(Action::GoTo(self.flags[i].range.clone()));
            }
        }
        (toggled, action)
    }

    #[allow(clippy::too_many_arguments)]
    fn preview_box(
        ui: &mut Ui,
        s: &Settings,
        title: &str,
        text: &str,
        mark: &Range<usize>,
        ends: (bool, bool),
        fill: Color32,
        mark_color: Color32,
        max_h: f32,
        id: &str,
    ) {
        Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(s.corner_radius.min(8)))
            .inner_margin(Margin::same(6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(title)
                        .small()
                        .strong()
                        .color(Color32::from_gray(80)),
                );
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .id_salt(id)
                    .show(ui, |ui| {
                        let font = egui::FontId::proportional(s.box_font_size);
                        let plain = TextFormat {
                            font_id: font.clone(),
                            color: Color32::from_gray(35),
                            ..Default::default()
                        };
                        let marked = TextFormat {
                            font_id: font,
                            color: Color32::from_gray(20),
                            background: mark_color,
                            ..Default::default()
                        };
                        let mut job = LayoutJob::default();
                        job.wrap.max_width = ui.available_width();
                        if ends.0 {
                            job.append("\u{2026} ", 0.0, plain.clone());
                        }
                        job.append(text.get(..mark.start).unwrap_or(text), 0.0, plain.clone());
                        job.append(text.get(mark.clone()).unwrap_or(""), 0.0, marked);
                        job.append(text.get(mark.end..).unwrap_or(""), 0.0, plain.clone());
                        if ends.1 {
                            job.append(" \u{2026}", 0.0, plain);
                        }
                        ui.label(job);
                    });
            });
    }

    /// The Dialogue Formatting box: the quotation style in use (and a way to choose it), the
    /// current issue with a preview of the text as it is and as it would be fixed, Apply Change,
    /// and a Read More button for the guide. Drag its corner grip to make it wider or its
    /// previews taller. Returns whether its arrow was pressed and the action to run.
    fn dialogue_box(&mut self, ui: &mut Ui, s: &Settings) -> (bool, Option<Action>) {
        let text_color = color(s.box_text);
        let mut size = s.dialogue_size;
        let width = size[0];
        let vis = self.visible_dialogue();
        let total = vis.len();
        if self.dialogue_current >= total {
            self.dialogue_current = 0;
        }
        let cur = vis
            .get(self.dialogue_current)
            .map(|&i| self.dflags[i].clone());
        let current = self.dialogue_current;
        let style_label = self.style().label().to_lowercase();
        let nothing_loaded = self.nothing_loaded();
        let mut choice = s.dialogue_style;
        let mut action = None;
        let mut guide: Option<&'static str> = None;
        let mut ignore = None;
        let mut step = 0isize;
        let toggled = ui_kit::card_ex(
            ui,
            s,
            width,
            Some(Icon::Quotes),
            "Dialogue Formatting",
            s.dialogue_collapsed,
            |_| {},
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Quotes")
                            .size(s.box_font_size - 1.0)
                            .color(text_color),
                    );
                    for (value, label) in [
                        (0u8, "Auto"),
                        (1, "\u{201C} \u{201D}"),
                        (2, "\u{2018} \u{2019}"),
                    ] {
                        if ui_kit::choice_chip(
                            ui,
                            s,
                            label,
                            Color32::from_rgb(190, 215, 235),
                            choice == value,
                        ) {
                            choice = value;
                        }
                    }
                });
                let how = if choice == 0 { " (detected)" } else { "" };
                ui.label(
                    RichText::new(format!("Checking for {style_label}{how}"))
                        .size(s.box_font_size - 2.0)
                        .color(text_color),
                );
                ui.add_space(2.0);
                let Some(f) = &cur else {
                    if nothing_loaded {
                        ui_kit::empty_state(
                            ui,
                            1,
                            text_color,
                            s.box_font_size,
                            "No dialogue to check yet.",
                        );
                    } else {
                        ui.label(
                            RichText::new("No dialogue problems found.")
                                .size(s.box_font_size)
                                .color(text_color),
                        );
                    }
                    if ui.button("Read More").clicked() {
                        guide = Some("");
                    }
                    ui_kit::resize_grip(ui, &mut size, width - 18.0, DIALOGUE_SIZE);
                    return;
                };
                ui.horizontal(|ui| {
                    if let Some(rep) = &f.issue.replacement {
                        let shown = if rep.is_empty() {
                            "Remove this mark"
                        } else {
                            rep.as_str()
                        };
                        ui.label(
                            RichText::new(shown)
                                .strong()
                                .size(s.box_font_size)
                                .color(text_color),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Ignore").clicked() {
                            ignore = Some(Self::dialogue_key(f));
                        }
                    });
                });
                ui.label(
                    RichText::new(format!("Issue: {}", f.issue.title))
                        .strong()
                        .size(s.box_font_size)
                        .color(text_color),
                );
                ui.label(
                    RichText::new(&f.issue.explanation)
                        .size(s.box_font_size - 1.0)
                        .color(text_color),
                );
                ui.add_space(4.0);
                // The two previews: the text as written, and as it would read once the change is applied.
                let p = &f.preview;
                let ends = (p.lead, p.trail);
                Self::preview_box(
                    ui,
                    s,
                    "Original",
                    &p.window,
                    &p.hl,
                    ends,
                    Color32::from_rgb(250, 232, 232),
                    Color32::from_rgb(240, 150, 150),
                    size[1],
                    "dialogue_before",
                );
                ui.add_space(4.0);
                match (&p.fixed, &p.fixed_hl) {
                    (Some(fixed), Some(hl)) => Self::preview_box(
                        ui,
                        s,
                        "Formatted",
                        fixed,
                        hl,
                        ends,
                        Color32::from_rgb(230, 246, 234),
                        Color32::from_rgb(130, 205, 150),
                        size[1],
                        "dialogue_after",
                    ),
                    _ => {
                        let note = match &p.far_edit {
                            Some((at_start, text)) => format!(
                                "Apply Change adds {text} at the {} of this paragraph.",
                                if *at_start { "start" } else { "end" }
                            ),
                            None => "No automatic fix for this one.".to_string(),
                        };
                        Frame::new()
                            .fill(Color32::from_rgb(230, 246, 234))
                            .corner_radius(CornerRadius::same(s.corner_radius.min(8)))
                            .inner_margin(Margin::same(6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    RichText::new("Formatted")
                                        .small()
                                        .strong()
                                        .color(Color32::from_gray(80)),
                                );
                                ui.label(
                                    RichText::new(note)
                                        .size(s.box_font_size - 1.0)
                                        .color(Color32::from_gray(35)),
                                );
                            });
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let fix = f.edit.clone();
                    if ui
                        .add_enabled(fix.is_some(), egui::Button::new("Apply Change"))
                        .clicked()
                        && let Some((range, text)) = fix
                    {
                        action = Some(Action::Replace(range, text));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Read More").clicked() {
                            guide = Some(f.issue.guide);
                        }
                    });
                });
                if total > 1 {
                    step = ui_kit::stepper(ui, text_color, current, total);
                }
                ui_kit::resize_grip(ui, &mut size, width - 18.0, DIALOGUE_SIZE);
            },
        );
        self.settings.dialogue_size = size;
        if choice != self.settings.dialogue_style {
            self.settings.dialogue_style = choice;
        }
        if let Some(key) = ignore {
            self.dialogue_ignored.insert(key);
        }
        if let Some(section) = guide {
            self.open_guide(section);
        }
        if step != 0 && total > 0 {
            self.dialogue_current =
                (self.dialogue_current as isize + step).clamp(0, total as isize - 1) as usize;
            if let Some(&i) = vis.get(self.dialogue_current) {
                action = Some(Action::GoTo(self.dflags[i].range.clone()));
            }
        }
        (toggled, action)
    }

    /// Keep the Thesaurus in step with the word that is selected (or typed into its box).
    fn sync_thesaurus(&mut self) {
        if !self.thes_warm {
            // Read the word lists in the background so the first look-up is quick.
            self.thes_warm = true;
            std::thread::spawn(thesaurus::warm_up);
        }
        if self.selected != self.thes_seen {
            self.thes_seen = self.selected.clone();
            let picked = self.selected.trim();
            // A word or a short phrase; a selected paragraph is not a look-up.
            if !picked.is_empty() && picked.split_whitespace().count() <= 3 && picked.len() <= 40 {
                self.thes_query = picked.to_string();
            }
        }
        if self.thes_query != self.thes_for {
            self.thes_for = self.thes_query.clone();
            self.thes_senses = thesaurus::lookup(&self.thes_query);
            self.thes_index = 0;
        }
    }

    fn thesaurus_box(&mut self, ui: &mut Ui, s: &Settings) -> bool {
        let text_color = color(s.box_text);
        let size = s.box_font_size;
        let query = &mut self.thes_query;
        let senses = &self.thes_senses;
        let index = &mut self.thes_index;
        let shown_for = &self.thes_for;
        // Colours for the part of speech and for the two word lists.
        let pos_color = |p: thesaurus::Pos| match p {
            thesaurus::Pos::Noun => Color32::from_rgb(160, 195, 245),
            thesaurus::Pos::Verb => Color32::from_rgb(250, 205, 145),
            thesaurus::Pos::Adj => Color32::from_rgb(205, 175, 235),
            thesaurus::Pos::Adv => Color32::from_rgb(150, 225, 212),
        };
        let synonym_color = Color32::from_rgb(165, 220, 170);
        let antonym_color = Color32::from_rgb(245, 165, 165);
        ui_kit::card(
            ui,
            s,
            Some(Icon::Thesaurus),
            "Thesaurus",
            s.thesaurus_collapsed,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 9.0);
                ui.add_space(2.0);
                ui.add(
                    egui::TextEdit::singleline(query)
                        .hint_text("Select a word, or type one")
                        .font(egui::FontId::proportional(size))
                        .text_color(text_color)
                        .margin(Margin::symmetric(8, 5))
                        .desired_width(f32::INFINITY),
                );
                let Some(sense) = senses.get(*index) else {
                    let note = if shown_for.trim().is_empty() {
                        "Select a word in the window you are typing in.".to_string()
                    } else {
                        format!("No entry for \u{201C}{}\u{201D}.", shown_for.trim())
                    };
                    ui.label(RichText::new(note).size(size - 1.0).color(text_color));
                    // A picture at the bottom, just for company.
                    ui.add_space(6.0);
                    ui_kit::placeholder(ui, 5, 90.0);
                    return;
                };
                // The word itself, in the middle, with its part of speech.
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(&sense.lemma)
                            .strong()
                            .size(size + 9.0)
                            .color(text_color),
                    );
                    ui_kit::tag(ui, size, pos_color(sense.pos), &capitalised(sense.pos.label()));
                });
                if senses.len() > 1 {
                    let step = ui_kit::stepper(ui, text_color, *index, senses.len());
                    if step < 0 {
                        *index = index.saturating_sub(1);
                    } else if step > 0 {
                        *index = (*index + 1).min(senses.len() - 1);
                    }
                }
                // The meaning and an example, on a soft panel of their own.
                Frame::new()
                    .fill(Color32::from_black_alpha(10))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.label(RichText::new(&sense.meaning).size(size).color(text_color));
                        if let Some(example) = &sense.example {
                            ui.label(
                                RichText::new(format!("\u{201C}{example}\u{201D}"))
                                    .italics()
                                    .size(size - 1.0)
                                    .color(text_color),
                            );
                        }
                    });
                let group = |ui: &mut Ui, title: &str, hue: Color32, list: &[thesaurus::Related]| {
                    ui.add_space(2.0);
                    ui_kit::tag(ui, size, hue, title);
                    if list.is_empty() {
                        ui.label(
                            RichText::new("none in WordNet")
                                .italics()
                                .size(size - 1.0)
                                .color(text_color),
                        );
                    }
                    let [r, g, b, _] = hue.to_array();
                    for item in list {
                        Frame::new()
                            .fill(Color32::from_rgba_unmultiplied(r, g, b, 60))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(10, 7))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 3.0;
                                ui.label(
                                    RichText::new(&item.word)
                                        .strong()
                                        .size(size)
                                        .color(text_color),
                                );
                                if let Some(e) = &item.example {
                                    ui.label(
                                        RichText::new(format!("\u{201C}{e}\u{201D}"))
                                            .italics()
                                            .size(size - 1.0)
                                            .color(text_color),
                                    );
                                }
                            });
                    }
                };
                group(ui, "Synonyms", synonym_color, &sense.synonyms);
                group(ui, "Antonyms", antonym_color, &sense.antonyms);
                ui.add_space(6.0);
                ui_kit::placeholder(ui, 5, 80.0);
                ui.add_space(2.0);
            },
        )
    }

    fn reading_box(&self, ui: &mut Ui, s: &Settings) -> bool {
        ui_kit::card(
            ui,
            s,
            Some(Icon::BookPen),
            "Reading Level & Word Count",
            s.collapsed[1],
            |ui| {
                if self.counts.words == 0 {
                    ui_kit::empty_state(
                        ui,
                        2,
                        color(s.box_text),
                        s.box_font_size,
                        "Nothing to measure yet.",
                    );
                    return;
                }
                let level = self
                    .grade
                    .map_or("-", |d| analysis::dale_chall_label(d.score));
                ui_kit::chip(
                    ui,
                    s,
                    ui_kit::grade_color(self.grade.map(|d| d.score)),
                    format!("Reading Level : {level}"),
                );
                if let Some(d) = self.grade {
                    // The score gets a box of its own, in a calm lavender so it does not read as
                    // another grade colour, and a size between the chip text and the small note.
                    Frame::new()
                        .fill(Color32::from_rgb(222, 212, 245))
                        .corner_radius(CornerRadius::same(s.corner_radius))
                        .inner_margin(Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("Dale-Chall score : {:.1}", d.score))
                                    .strong()
                                    .size(s.box_font_size + 1.0)
                                    .color(Color32::from_gray(35)),
                            );
                        });
                    ui.label(
                        RichText::new(format!(
                            "{:.0}% difficult words \u{b7} {:.1} words/sentence",
                            d.difficult_percent, d.avg_sentence_len
                        ))
                        .size(s.box_font_size - 1.0)
                        .color(color(s.box_text)),
                    );
                }
                ui.add_space(2.0);
                let words = self.counts.words;
                ui_kit::chip(
                    ui,
                    s,
                    ui_kit::count_color(words, s.word_goal),
                    format!(
                        "Word Count: {} words, {} characters",
                        ui_kit::group(words),
                        ui_kit::group(self.counts.characters_with_spaces)
                    ),
                );
                if s.word_goal > 0 {
                    let pct = words as f64 * 100.0 / s.word_goal as f64;
                    ui.label(
                        RichText::new(format!(
                            "{pct:.0}% of your {} word goal",
                            ui_kit::group(s.word_goal)
                        ))
                        .small()
                        .color(color(s.box_text)),
                    );
                }
            },
        )
    }

    fn keywords_box(&mut self, ui: &mut Ui, s: &Settings) -> bool {
        /// About the height of one row of the list.
        const ROW: f32 = 26.0;
        let text_color = color(s.box_text);
        let mut height = s.keywords_height;
        let keywords = &self.keywords;
        let toggled = ui_kit::card(
            ui,
            s,
            Some(Icon::Bars),
            "Keyword Density",
            s.collapsed[2],
            |ui| {
                if keywords.is_empty() {
                    ui_kit::empty_state(ui, 3, text_color, s.box_font_size, "No keywords yet.");
                    return;
                }
                egui::ScrollArea::vertical()
                    .max_height(height)
                    .id_salt("keyword_density")
                    .show(ui, |ui| {
                        for k in keywords {
                            ui.horizontal(|ui| {
                                // The word sits on a faint grey tile, easier on the eyes than bare text.
                                Frame::new()
                                    .fill(Color32::from_rgba_unmultiplied(110, 110, 110, 34))
                                    .corner_radius(CornerRadius::same(s.corner_radius.min(6)))
                                    .inner_margin(Margin::symmetric(6, 1))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(&k.word)
                                                .size(s.box_font_size)
                                                .color(text_color),
                                        )
                                    });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        Frame::new()
                                            .fill(ui_kit::heat(k.percent / ui_kit::KEYWORD_RED_AT))
                                            .corner_radius(CornerRadius::same(
                                                s.corner_radius.min(6),
                                            ))
                                            .inner_margin(Margin::symmetric(6, 1))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "{} ({})",
                                                        k.count,
                                                        k.percent_label()
                                                    ))
                                                    .size(s.box_font_size)
                                                    .color(Color32::from_gray(35)),
                                                )
                                            });
                                    },
                                );
                            });
                        }
                    });
                // A grip: drag to make the list taller or shorter, double-click to fit the whole list.
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 12.0),
                    egui::Sense::click_and_drag(),
                );
                let response = response
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                    .on_hover_text("Drag to resize the list. Double-click to fit it all.");
                if response.dragged() {
                    height = (height + response.drag_delta().y).clamp(60.0, 2000.0);
                }
                if response.double_clicked() {
                    height = (keywords.len() as f32 * ROW + 8.0).clamp(60.0, 2000.0);
                }
                // Dragging can slip off the window while it catches up, so the same resizing is
                // on buttons too: two more rows, two fewer, or the whole list.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let rows = ((height - 8.0) / ROW).round().max(1.0) as usize;
                    ui.label(
                        RichText::new(format!("{} of {}", rows.min(keywords.len()), keywords.len()))
                            .size(s.box_font_size - 2.0)
                            .color(text_color),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("All").clicked() {
                            height = (keywords.len() as f32 * ROW + 8.0).clamp(60.0, 2000.0);
                        }
                        if ui.small_button("More").clicked() {
                            height = (height + 4.0 * ROW).clamp(60.0, 2000.0);
                        }
                        if ui.small_button("Fewer").clicked() {
                            height = (height - 4.0 * ROW).clamp(60.0, 2000.0);
                        }
                    });
                });
                let c = rect.center();
                for dy in [-2.0_f32, 2.0] {
                    ui.painter().line_segment(
                        [
                            egui::pos2(c.x - 14.0, c.y + dy),
                            egui::pos2(c.x + 14.0, c.y + dy),
                        ],
                        Stroke::new(1.5, Color32::from_gray(170)),
                    );
                }
            },
        );
        self.settings.keywords_height = height;
        toggled
    }

    fn sentences_box(&mut self, ui: &mut Ui, s: &Settings) -> bool {
        let text_color = color(s.box_text);
        let mut filter = self.bucket_filter;
        let toggled = ui_kit::card(ui, s, None, "Sentence Length", s.collapsed[3], |ui| {
            if self.sentence_total == 0 {
                ui_kit::empty_state(
                    ui,
                    4,
                    text_color,
                    s.box_font_size,
                    "No sentences yet.",
                );
                return;
            }
            // One chip per range, with how many sentences fall in it. Press one to list them.
            ui.horizontal_wrapped(|ui| {
                for (i, b) in LengthBucket::ALL.into_iter().enumerate() {
                    let n = self.bucket_counts.get(i).copied().unwrap_or(0);
                    let selected = filter == Some(b);
                    let mut chip = egui::Button::new(
                        RichText::new(format!("{} \u{00B7} {n}", b.label()))
                            .size(s.box_font_size - 2.0)
                            .color(Color32::from_gray(40)),
                    )
                    .fill(ui_kit::bucket_color(b).to_opaque())
                    .corner_radius(CornerRadius::same(s.corner_radius));
                    if selected {
                        chip = chip.stroke(Stroke::new(1.5, Color32::from_gray(60)));
                    }
                    if ui.add(chip).clicked() {
                        filter = if selected { None } else { Some(b) };
                    }
                }
            });
            if self.sentence_total > 0 {
                let average = self.sentence_words as f64 / self.sentence_total as f64;
                ui.label(
                    RichText::new(format!(
                        "{} sentences \u{00B7} average {average:.1} words \u{00B7} longest {}",
                        ui_kit::group(self.sentence_total),
                        self.sentence_longest
                    ))
                    .size(s.box_font_size - 1.0)
                    .color(text_color),
                );
            }
            if let Some(b) = filter {
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .id_salt("sentence_list")
                    .show(ui, |ui| {
                        for t in self.tints.iter().filter(|t| t.info.bucket == b).take(300) {
                            ui.label(
                                RichText::new(format!("{} ({})", t.preview, t.info.words))
                                    .size(s.box_font_size - 1.0)
                                    .color(text_color),
                            );
                        }
                    });
            }
        });
        self.bucket_filter = filter;
        toggled
    }

    /// Where a box starts: where it was last left, or `default`.
    fn box_start(&self, key: &str, default: [f32; 2]) -> [f32; 2] {
        self.settings.box_pos.get(key).copied().unwrap_or(default)
    }

    /// Remember where a box's window now is, once it has been moved.
    fn remember(&mut self, key: &str, start: [f32; 2], now: Option<egui::Pos2>) {
        if let Some(p) = now
            && ((p.x - start[0]).abs() > 0.5 || (p.y - start[1]).abs() > 0.5)
        {
            self.settings.box_pos.insert(key.to_string(), [p.x, p.y]);
        }
    }

    /// The floating boxes. Each is a window of its own that can be dragged anywhere on the
    /// screen by its title. A fresh start puts them in a column down the right edge.
    fn boxes(&mut self, ctx: &egui::Context) {
        let s = self.settings.clone();
        let generation = self.layout_gen;
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1600.0, 900.0));
        let x = (monitor.x - BOX_W - 16.0).max(16.0);
        let lowest = (monitor.y - 200.0).max(60.0);
        let mut action = None;
        let mut toggled = [false; 6];
        if s.thesaurus_on {
            self.sync_thesaurus();
        }

        if s.grammar_on {
            let start = self.box_start("grammar", [x, 60.0_f32.min(lowest)]);
            let pos = ui_kit::floating_box(
                ctx,
                ("grammar", generation),
                "Grammar Checker",
                BOX_W,
                start,
                &mut |ui| {
                    let (t, a) = self.grammar_box(ui, &s);
                    toggled[0] = t;
                    if a.is_some() {
                        action = a;
                    }
                },
            );
            self.remember("grammar", start, pos);
        }
        if s.dialogue_on {
            let start = self.box_start("dialogue", [x, 280.0_f32.min(lowest)]);
            let pos = ui_kit::floating_box(
                ctx,
                ("dialogue", generation),
                "Dialogue Formatting",
                s.dialogue_size[0],
                start,
                &mut |ui| {
                    let (t, a) = self.dialogue_box(ui, &s);
                    toggled[4] = t;
                    if a.is_some() {
                        action = a;
                    }
                },
            );
            self.remember("dialogue", start, pos);
        }
        if s.reading_on {
            let start = self.box_start("reading", [x, 560.0_f32.min(lowest)]);
            let pos = ui_kit::floating_box(
                ctx,
                ("reading", generation),
                "Reading Level & Word Count",
                BOX_W,
                start,
                &mut |ui| toggled[1] = self.reading_box(ui, &s),
            );
            self.remember("reading", start, pos);
        }
        if s.keywords_on {
            let start = self.box_start("keywords", [x, 660.0_f32.min(lowest)]);
            let pos = ui_kit::floating_box(
                ctx,
                ("keywords", generation),
                "Keyword Density",
                BOX_W,
                start,
                &mut |ui| toggled[2] = self.keywords_box(ui, &s),
            );
            self.remember("keywords", start, pos);
        }
        if s.sentences_on {
            let start = self.box_start("sentences", [x, 880.0_f32.min(lowest)]);
            let pos = ui_kit::floating_box(
                ctx,
                ("sentences", generation),
                "Sentence Length",
                BOX_W,
                start,
                &mut |ui| toggled[3] = self.sentences_box(ui, &s),
            );
            self.remember("sentences", start, pos);
        }
        if s.thesaurus_on {
            let start = self.box_start("thesaurus", [(x - BOX_W - 16.0).max(16.0), 60.0_f32.min(lowest)]);
            let pos = ui_kit::floating_box(
                ctx,
                ("thesaurus", generation),
                "Thesaurus",
                BOX_W,
                start,
                &mut |ui| toggled[5] = self.thesaurus_box(ui, &s),
            );
            self.remember("thesaurus", start, pos);
        }
        for (i, pressed) in toggled.iter().take(4).enumerate() {
            if *pressed {
                self.settings.collapsed[i] = !self.settings.collapsed[i];
            }
        }
        if toggled[4] {
            self.settings.dialogue_collapsed = !self.settings.dialogue_collapsed;
        }
        if toggled[5] {
            self.settings.thesaurus_collapsed = !self.settings.thesaurus_collapsed;
        }
        if let Some(a) = action {
            self.run(a);
            ctx.request_repaint();
        }
    }

    /// Act on what was chosen from the tray icon's menu.
    fn poll_tray(&mut self, ctx: &egui::Context) {
        use crate::tray::Command;
        let commands = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for command in commands {
            match command {
                Command::ShowPanel => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Command::HidePanel => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                Command::ToggleBoxes => {
                    self.settings.boxes_open = !self.settings.boxes_open;
                }
                Command::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint();
        }
    }

    /// Start or stop watching the window you are typing in.
    fn set_active(&mut self, on: bool, ctx: &egui::Context) {
        self.settings.active = on;
        self.watching.clear();
        self.selected.clear();
        if on {
            let repaint = ctx.clone();
            self.watch = source::start(move || repaint.request_repaint());
            self.status = if self.watch.is_some() {
                "Watching: click into another window and type.".to_string()
            } else {
                "Watching other windows only works on Windows.".to_string()
            };
        } else {
            self.watch = None;
            self.status = "Not watching. Use the test text below to try the boxes.".to_string();
        }
    }

    /// One row of the control panel: a switch and what it turns on.
    fn feature_row(ui: &mut Ui, on: &mut bool, name: &str, note: &str) {
        ui.horizontal(|ui| {
            ui_kit::toggle_switch(ui, on);
            ui.label(RichText::new(name).strong());
            ui.label(RichText::new(note).weak().small());
        });
    }

    /// The control panel: the one window that is always there. It turns each feature on or off
    /// and holds the settings. Every box is a separate window you place wherever you like.
    fn control_panel(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // The controls scroll in the space above the picture at the bottom.
        let bunny_height = 110.0;
        let show_bunny = ui.available_height() > 360.0;
        let reserved = if show_bunny { bunny_height + 8.0 } else { 0.0 };
        let scroll_height = (ui.available_height() - reserved).max(100.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(scroll_height)
            .show(ui, |ui| {
                ui.heading("jAyDHD-Grammar-Checker");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("Save settings")
                        .on_hover_text("Changes are also saved automatically.")
                        .clicked()
                    {
                        self.settings.save();
                        self.saved_at = Some(ctx.input(|i| i.time));
                    }
                    if let Some(t) = self.saved_at {
                        if ctx.input(|i| i.time) - t < 2.5 {
                            ui.label(RichText::new("Saved").weak());
                            ctx.request_repaint_after(Duration::from_millis(300));
                        }
                    }
                });
                ui.add_space(4.0);

                let mut active = self.settings.active;
                ui.horizontal(|ui| {
                    if ui_kit::toggle_switch(ui, &mut active) {
                        self.set_active(active, &ctx);
                    }
                    ui.label(RichText::new("Watch the window I'm typing in").strong());
                });
                if self.watch.is_some() {
                    let line = if self.watching.is_empty() {
                        "Click into a text box in another program.".to_string()
                    } else {
                        self.watching.clone()
                    };
                    ui.label(RichText::new(line).weak().small());
                    if !self.selected.is_empty() {
                        let word: String = self.selected.chars().take(60).collect();
                        ui.label(RichText::new(format!("Selected: {word:?}")).small());
                    }
                } else if !self.status.is_empty() {
                    ui.label(RichText::new(&self.status).weak().small());
                }

                ui.add_space(2.0);
                self.docs_panel(ui);
                ui.label(
                    RichText::new(
                        "Google Docs and other programs that do not share their text: click into the document, then press Ctrl+Alt+G (whole document) or Ctrl+Alt+S (selected text).",
                    )
                    .weak()
                    .small(),
                );
                if !self.capture_note.is_empty() {
                    ui.label(RichText::new(&self.capture_note).small());
                }
                if self.captured_pid.is_some()
                    && ui.small_button("Stop using the captured text").clicked()
                {
                    self.captured_pid = None;
                    self.capture_note.clear();
                    self.text.clear();
                }

                ui.separator();
                ui.label(RichText::new("Features").strong());
                let s = &mut self.settings;
                Self::feature_row(
                    ui,
                    &mut s.grammar_on,
                    "Grammar Checker",
                    "spelling and grammar",
                );
                Self::feature_row(
                    ui,
                    &mut s.dialogue_on,
                    "Dialogue Formatting",
                    "quotation marks and speech",
                );
                Self::feature_row(ui, &mut s.reading_on, "Reading Level & Word Count", "");
                Self::feature_row(ui, &mut s.keywords_on, "Keyword Density", "most-used words");
                Self::feature_row(ui, &mut s.sentences_on, "Sentence Length", "");
                Self::feature_row(ui, &mut s.pets_open, "Typing Pets", "");
                Self::feature_row(ui, &mut s.thesaurus_on, "Thesaurus", "select a word to look it up");

                ui.separator();
                egui::CollapsingHeader::new("Detect").show(ui, |ui| {
                    for (i, c) in Category::ALL.iter().enumerate() {
                        if let Some(on) = self.settings.categories.get_mut(i) {
                            ui.checkbox(on, c.label());
                        }
                    }
                });
                egui::CollapsingHeader::new("Dialogue quotes").show(ui, |ui| {
                    let s = &mut self.settings;
                    ui.radio_value(&mut s.dialogue_style, 0u8, "Detect from the text");
                    ui.radio_value(&mut s.dialogue_style, 1u8, "\u{201C} \u{201D}  double");
                    ui.radio_value(&mut s.dialogue_style, 2u8, "\u{2018} \u{2019}  single");
                });
                egui::CollapsingHeader::new("Appearance").show(ui, |ui| {
                    let s = &mut self.settings;
                    Self::edit_color(ui, "Box color", &mut s.box_fill);
                    Self::edit_color(ui, "Text color", &mut s.box_text);
                    ui.add(egui::Slider::new(&mut s.corner_radius, 0..=20).text("Corners"));
                    ui.add(egui::Slider::new(&mut s.box_font_size, 10.0..=32.0).text("Font size"));
                    ui.add(
                        egui::Slider::new(&mut s.box_scale, 0.75..=3.0)
                            .text("Interface scale"),
                    )
                    .on_hover_text("Scales every box and the control panel. Try 1.5 to 2 on 4K.");
                    ui.horizontal(|ui| {
                        ui.label("Word goal");
                        ui.add(
                            egui::DragValue::new(&mut s.word_goal)
                                .range(0..=10_000_000)
                                .speed(100),
                        );
                    });
                    ui.checkbox(
                        &mut s.keywords_skip_stop_words,
                        "Skip common words in Keyword Density",
                    );
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Add pet").clicked() {
                        self.pets.open_add();
                        self.settings.pets_open = true;
                    }
                    if ui.button("Dictionary").clicked() {
                        self.dictionary_open = true;
                    }
                    if ui.button("Writing Guide").clicked() {
                        self.open_guide("");
                    }
                    if ui.button("Reset box positions").clicked() {
                        self.layout_gen = self.layout_gen.wrapping_add(1);
                        self.settings.box_pos.clear();
                    }
                });
                ui.add_space(4.0);
                ui.checkbox(&mut self.settings.boxes_open, "Show the boxes");
                ui.checkbox(
                    &mut self.settings.always_on_top,
                    "Keep the boxes on top of other windows",
                )
                .on_hover_text("The control panel itself never stays on top.");

                if self.watch.is_none() {
                    ui.separator();
                    ui.label(RichText::new("Test text").strong());
                    ui.label(
                        RichText::new(
                            "Type or paste here to try the boxes without another program.",
                        )
                        .weak()
                        .small(),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut self.text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(8),
                    );
                }
            });
        if show_bunny {
            ui.add_space(4.0);
            ui_kit::placeholder(ui, 7, bunny_height);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Windows
    // ---------------------------------------------------------------------------------------

    fn open_guide(&mut self, section: &'static str) {
        self.guide_open = Some(section);
        self.guide_gen = self.guide_gen.wrapping_add(1);
        // Open the tab the rule is on (a plain Read More opens the dialogue tab).
        self.guide_category = dialogue::GUIDE
            .iter()
            .find(|g| g.id == section)
            .map_or("dialogue", |g| g.category);
        self.guide_query.clear();
    }

    fn edit_color(ui: &mut Ui, label: &str, c: &mut [u8; 4]) {
        ui.horizontal(|ui| {
            let mut col = color(*c);
            if egui::color_picker::color_edit_button_srgba(
                ui,
                &mut col,
                egui::color_picker::Alpha::OnlyBlend,
            )
            .changed()
            {
                *c = col.to_srgba_unmultiplied();
            }
            ui.label(label);
        });
    }

    /// The "Writing Dictionary" window: every word or phrase that is never flagged. Click an
    /// entry to edit its spelling; Remove asks first.
    fn dictionary_window(&mut self, ctx: &egui::Context) {
        if !self.dictionary_open {
            return;
        }
        let mut add = false;
        let mut ask_remove = None;
        let mut remove = None;
        let mut cancel = false;
        let mut commit = None;
        let mut focused = None;
        let closed = ui_kit::tool_window(
            ctx,
            "writing_dictionary",
            "Writing Dictionary",
            [420.0, 540.0],
            &mut |ui| {
                ui.label("Words and phrases listed here are never flagged. Add one from the Grammar Checker box, or type it below.");
                ui.horizontal(|ui| {
                    let field = ui.text_edit_singleline(&mut self.dictionary_input);
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Add").clicked() || enter {
                        add = true;
                    }
                });
                ui.separator();
                ui.label(RichText::new("Words and phrases (click to edit spelling)").strong());
                let pending = self.dictionary_confirm;
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .id_salt("writing_dictionary_list")
                    .show(ui, |ui| {
                        if self.settings.dictionary.is_empty() {
                            ui.label("Nothing here yet.");
                        }
                        for (i, entry) in self.settings.dictionary.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 170.0).max(80.0);
                                let field =
                                    ui.add(egui::TextEdit::singleline(entry).desired_width(width));
                                if field.gained_focus() {
                                    focused = Some((i, entry.clone()));
                                }
                                if field.lost_focus() {
                                    commit = Some(i);
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if pending == Some(i) {
                                            if ui.button("Cancel").clicked() {
                                                cancel = true;
                                            }
                                            if ui.button("Yes, remove").clicked() {
                                                remove = Some(i);
                                            }
                                        } else if ui.button("Remove").clicked() {
                                            ask_remove = Some(i);
                                        }
                                    },
                                );
                            });
                        }
                    });
            },
        );
        self.dictionary_open = !closed;
        if let Some(entry) = focused {
            self.dictionary_backup = Some(entry);
        }
        if let Some(i) = commit {
            self.finish_edit(i);
        }
        if add {
            let text = std::mem::take(&mut self.dictionary_input);
            self.settings.add_word(&text);
            self.dictionary_confirm = None;
        }
        if cancel {
            self.dictionary_confirm = None;
        }
        if let Some(i) = ask_remove {
            self.dictionary_confirm = Some(i);
        }
        if let Some(i) = remove
            && self.settings.dictionary.get(i).is_some()
        {
            self.settings.dictionary.remove(i);
            self.dictionary_confirm = None;
            self.dictionary_backup = None;
        }
    }

    /// An entry's field lost focus: keep the new spelling, or put the old one back when the edit
    /// left it empty, too long or the same as another entry.
    fn finish_edit(&mut self, i: usize) {
        let backup = self.dictionary_backup.take();
        let Some(entry) = self.settings.dictionary.get(i).cloned() else {
            return;
        };
        let text = entry.trim().to_string();
        let lower = text.to_lowercase();
        let duplicate = self
            .settings
            .dictionary
            .iter()
            .enumerate()
            .any(|(j, d)| j != i && d.to_lowercase() == lower);
        if text.is_empty() || text.chars().count() > MAX_ENTRY_CHARS || duplicate {
            match backup {
                Some((at, old)) if at == i => {
                    if let Some(e) = self.settings.dictionary.get_mut(i) {
                        *e = old;
                    }
                }
                _ if text.is_empty() => {
                    self.settings.dictionary.remove(i);
                }
                _ => {}
            }
        } else if text != entry
            && let Some(e) = self.settings.dictionary.get_mut(i)
        {
            *e = text;
        }
    }

    /// The Writing Guide window: tabs for Dialogue, Sentence Structure, Punctuation and Word
    /// Choice, and a search box that looks through all of them. Every rule has three correct and
    /// three incorrect examples. The Dialogue tab ends with credited lines from published books.
    fn guide_window(&mut self, ctx: &egui::Context) {
        let Some(target) = self.guide_open else {
            return;
        };
        let generation = self.guide_gen;
        let s = self.settings.clone();
        let mut category = self.guide_category;
        let mut query = std::mem::take(&mut self.guide_query);
        let closed = ui_kit::tool_window(
            ctx,
            "writing_guide",
            "Writing Guide",
            [640.0, 700.0],
            &mut |ui| {
                // The scroll area fills the window (less room for the picture at the bottom), so
                // dragging the window's edge or corner resizes it.
                let scroll_height = (ui.available_height() - 100.0).max(120.0);
                egui::ScrollArea::vertical()
                    .id_salt("dialogue_guide_scroll")
                    .auto_shrink([false, false])
                    .max_height(scroll_height)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 10.0;
                        for p in dialogue::GUIDE_INTRO {
                            ui.label(guide_text(p));
                        }
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Search").size(15.0));
                            ui.add(
                                egui::TextEdit::singleline(&mut query)
                                    .hint_text("a word, a mark, a rule")
                                    .desired_width(280.0),
                            );
                            if !query.is_empty() && ui.button("Clear").clicked() {
                                query.clear();
                            }
                        });
                        let needle = query.trim().to_lowercase();
                        let searching = !needle.is_empty();
                        ui.horizontal_wrapped(|ui| {
                            for cat in dialogue::GUIDE_CATEGORIES {
                                let base = match cat.id {
                                    "dialogue" => Color32::from_rgb(190, 215, 235),
                                    "structure" => Color32::from_rgb(245, 200, 140),
                                    "punctuation" => Color32::from_rgb(205, 170, 235),
                                    _ => Color32::from_rgb(165, 215, 170),
                                };
                                if ui_kit::choice_chip(
                                    ui,
                                    &s,
                                    cat.title,
                                    base,
                                    !searching && category == cat.id,
                                ) {
                                    category = cat.id;
                                    query.clear();
                                }
                            }
                        });
                        if !searching
                            && let Some(cat) =
                                dialogue::GUIDE_CATEGORIES.iter().find(|c| c.id == category)
                        {
                            for p in cat.blurb {
                                ui.label(guide_text(p));
                            }
                        }
                        ui.add_space(4.0);
                        let mut shown = 0;
                        for section in dialogue::GUIDE.iter().filter(|g| {
                            if searching {
                                guide_matches(g, &needle)
                            } else {
                                g.category == category
                            }
                        }) {
                            shown += 1;
                            guide_section(ui, section, section.id == target, generation, searching);
                        }
                        if searching {
                            let text = if shown == 0 {
                                "Nothing in the guide matches that.".to_string()
                            } else {
                                format!("{shown} matching rules.")
                            };
                            ui.label(RichText::new(text).size(13.0).weak());
                        }
                        if !searching && category == "dialogue" {
                            ui.add_space(8.0);
                            ui.separator();
                            ui.label(RichText::new("From published books").strong().size(16.0));
                            ui.label(guide_text(dialogue::BOOKS_INTRO));
                            for b in dialogue::BOOKS {
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(b.quote)
                                        .italics()
                                        .size(15.0)
                                        .line_height(Some(23.0)),
                                );
                                ui.label(
                                    RichText::new(format!("\u{2014} {}", b.credit)).size(13.0),
                                );
                                ui.label(RichText::new(b.shows).size(13.0).weak());
                            }
                            ui.add_space(8.0);
                            ui.separator();
                            ui.label(
                                RichText::new("Breaking the rules on purpose")
                                    .strong()
                                    .size(16.0),
                            );
                            for p in dialogue::BREAKING_RULES {
                                ui.label(guide_text(p));
                            }
                        }
                        ui.add_space(8.0);
                        ui.separator();
                        ui.label(RichText::new("Further reading").strong().size(16.0));
                        for (title, url, note) in dialogue::FURTHER_READING {
                            ui.hyperlink_to(RichText::new(*title).size(15.0), *url);
                            ui.label(RichText::new(*note).size(13.0).weak());
                        }
                        ui.add_space(12.0);
                    });
                ui.add_space(6.0);
                ui_kit::placeholder(ui, 6, 80.0);
            },
        );
        self.guide_category = category;
        self.guide_query = query;
        if closed {
            self.guide_open = None;
        }
    }
}

/// Body text of the guide: a size larger, with a tall line so paragraphs breathe.
fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn guide_text(text: &str) -> RichText {
    RichText::new(text).size(15.0).line_height(Some(23.0))
}

/// Does `section` mention `needle` (already lowercase) in its title, text or examples?
fn guide_matches(section: &dialogue::GuideSection, needle: &str) -> bool {
    let has = |t: &str| t.to_lowercase().contains(needle);
    has(section.title)
        || section.body.iter().any(|b| has(b))
        || section
            .correct
            .iter()
            .chain(section.incorrect.iter())
            .any(|e| has(e.text) || has(e.why))
}

/// One rule of the guide as a fold-out: its explanation, then three correct and three incorrect
/// examples. `open_it` opens it at first; `with_tab` puts its tab's name in front of the title
/// (for search results, which come from every tab).
fn guide_section(
    ui: &mut Ui,
    section: &dialogue::GuideSection,
    open_it: bool,
    generation: u32,
    with_tab: bool,
) {
    let green = Color32::from_rgb(40, 150, 70);
    let red = Color32::from_rgb(200, 60, 60);
    let title = if with_tab {
        let tab = dialogue::GUIDE_CATEGORIES
            .iter()
            .find(|c| c.id == section.category)
            .map_or("", |c| c.title);
        format!("{tab} \u{2014} {}", section.title)
    } else {
        section.title.to_string()
    };
    egui::CollapsingHeader::new(RichText::new(title).strong().size(16.0))
        .id_salt((section.id, generation))
        .default_open(open_it)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            for p in section.body {
                ui.label(guide_text(p));
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Correct").strong().size(15.0).color(green));
            for e in &section.correct {
                ui.label(
                    RichText::new(e.text)
                        .monospace()
                        .size(15.0)
                        .line_height(Some(22.0)),
                );
                ui.label(RichText::new(e.why).size(13.0).weak());
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Incorrect").strong().size(15.0).color(red));
            for e in &section.incorrect {
                ui.label(
                    RichText::new(e.text)
                        .monospace()
                        .size(15.0)
                        .line_height(Some(22.0)),
                );
                ui.label(RichText::new(e.why).size(13.0).weak());
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
        });
    ui.add_space(4.0);
}

impl eframe::App for App {
    /// The boxes and pets are windows of their own with see-through corners, so nothing is
    /// painted behind them.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        let now = ctx.input(|i| i.time);
        if (ctx.zoom_factor() - self.settings.box_scale).abs() > 1e-3 {
            ctx.set_zoom_factor(self.settings.box_scale);
        }
        self.poll_input();
        self.poll_docs();
        self.poll_capture();
        self.poll_watch();
        self.poll_tray(ctx);
        let typed = self.analysed.as_ref().is_some_and(|(t, _)| *t != self.text);
        self.refresh(now);
        // Poll twice a second: the whole-document numbers catch up after you stop typing.
        ctx.request_repaint_after(Duration::from_millis(500));

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(Color32::from_rgb(246, 246, 248))
                    .inner_margin(Margin::same(12)),
            )
            .show(root, |ui| self.control_panel(ui));

        ui_kit::set_boxes_on_top(ctx, self.settings.always_on_top);
        if self.settings.boxes_open {
            self.boxes(ctx);
        }
        self.dictionary_window(ctx);
        self.guide_window(ctx);
        pets::overlay(&mut self.settings, &mut self.pets, ctx, typed);
        pets::editor_window(&mut self.settings, &mut self.pets, ctx);

        // Keep the settings file up to date, but not while a slider is being dragged.
        if self.settings != self.saved_settings && !ctx.input(|i| i.pointer.any_down()) {
            self.settings.save();
            self.saved_settings = self.settings.clone();
        }
    }
}
