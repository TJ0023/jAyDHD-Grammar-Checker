//! What is kept between runs: `settings.json` in the user's config folder.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::pets::{Pet, PetPreset};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Whether the floating boxes are shown at all.
    pub boxes_open: bool,
    /// Whether the boxes (not the control panel) stay above other windows.
    pub always_on_top: bool,
    /// Read the words you select in Google Docs (for the Thesaurus) by pressing Copy for you.
    pub docs_auto_select: bool,
    /// Whether Grammar Desk watches the window you are typing in.
    pub active: bool,
    /// Which boxes are switched on.
    pub reading_on: bool,
    pub keywords_on: bool,
    pub sentences_on: bool,
    /// The Thesaurus box (on from the start) and whether it is folded.
    pub thesaurus_on: bool,
    /// Set once the Thesaurus has been switched on for you (it starts on, even in older files).
    pub thesaurus_seeded: bool,
    pub thesaurus_collapsed: bool,
    /// Where each floating box last was on the screen.
    pub box_pos: std::collections::BTreeMap<String, [f32; 2]>,
    pub grammar_on: bool,
    /// Subject-verb, fragments, punctuation, tense, word choice, prepositions.
    pub categories: [bool; 6],
    /// Highlight colour of a suspected error (RGBA).
    pub highlight: [u8; 4],
    pub box_fill: [u8; 4],
    pub box_text: [u8; 4],
    /// Rounded-corner radius of the boxes and chips.
    pub corner_radius: u8,
    /// Text size in the boxes, and in the editor.
    pub box_font_size: f32,
    /// Scales the whole interface (every window), for high-resolution screens.
    pub box_scale: f32,
    pub font_size: f32,
    pub keywords_skip_stop_words: bool,
    pub keywords_shown: usize,
    pub sentence_tint_on: bool,
    /// Highlight colour of a keyword shown on the page with its eye button (RGBA).
    pub keyword_color: [u8; 4],
    pub dialogue_on: bool,
    /// 0 detect from the text, 1 double quotes, 2 single quotes.
    pub dialogue_style: u8,
    pub dialogue_color: [u8; 4],
    pub dialogue_collapsed: bool,
    /// Word-count goal; 0 means no goal.
    pub word_goal: usize,
    /// Which boxes are folded to their title: grammar, reading, keywords, sentences.
    pub collapsed: [bool; 4],
    /// The Dialogue Formatting box's width and the height of each preview.
    pub dialogue_size: [f32; 2],
    /// Height of the Keyword Density list.
    pub keywords_height: f32,
    /// Words and phrases that are never flagged.
    pub dictionary: Vec<String>,
    /// Whether the typing pets are shown, the pets themselves, and the saved picture presets.
    pub pets_open: bool,
    pub pets: Vec<Pet>,
    pub presets: Vec<PetPreset>,
}

pub const DIALOGUE_SIZE: [f32; 2] = [300.0, 90.0];

impl Default for Settings {
    fn default() -> Self {
        Self {
            boxes_open: true,
            always_on_top: true,
            docs_auto_select: true,
            active: true,
            reading_on: true,
            keywords_on: true,
            sentences_on: true,
            thesaurus_on: true,
            thesaurus_seeded: false,
            thesaurus_collapsed: false,
            box_pos: std::collections::BTreeMap::new(),
            grammar_on: true,
            categories: [true; 6],
            highlight: [255, 214, 0, 150],
            box_fill: [248, 248, 250, 255],
            box_text: [40, 40, 48, 255],
            corner_radius: 8,
            box_font_size: 13.0,
            box_scale: 1.0,
            font_size: 17.0,
            keywords_skip_stop_words: true,
            keywords_shown: 50,
            sentence_tint_on: true,
            keyword_color: [255, 120, 190, 115],
            dialogue_on: true,
            dialogue_style: 0,
            dialogue_color: [40, 190, 185, 120],
            dialogue_collapsed: false,
            word_goal: 0,
            collapsed: [false; 4],
            dialogue_size: DIALOGUE_SIZE,
            keywords_height: 216.0,
            dictionary: Vec::new(),
            pets_open: true,
            pets: vec![Pet::default()],
            presets: Vec::new(),
        }
    }
}

pub const MAX_ENTRY_CHARS: usize = 200;
const MAX_DICTIONARY: usize = 5000;

impl Settings {
    /// Put anything unreasonable back into range.
    fn sanitize(&mut self) {
        if !self.font_size.is_finite() || !(10.0..=32.0).contains(&self.font_size) {
            self.font_size = 17.0;
        }
        if !self.box_font_size.is_finite() || !(10.0..=32.0).contains(&self.box_font_size) {
            self.box_font_size = 13.0;
        }
        if !self.box_scale.is_finite() || !(0.75..=3.0).contains(&self.box_scale) {
            self.box_scale = 1.0;
        }
        self.corner_radius = self.corner_radius.min(20);
        self.dialogue_style = self.dialogue_style.min(2);
        let [w, h] = self.dialogue_size;
        if !((260.0..=900.0).contains(&w) && (40.0..=600.0).contains(&h)) {
            self.dialogue_size = DIALOGUE_SIZE;
        }
        // 180 was the old default (about 6 rows); the new one shows 8.
        if !(60.0..=2000.0).contains(&self.keywords_height) || self.keywords_height == 180.0 {
            self.keywords_height = 216.0;
        }
        self.keywords_shown = self.keywords_shown.clamp(1, 200);
        crate::pets::sanitize(&mut self.pets, &mut self.presets);
        self.dictionary = std::mem::take(&mut self.dictionary)
            .into_iter()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty() && w.chars().count() <= MAX_ENTRY_CHARS)
            .take(MAX_DICTIONARY)
            .collect();
    }

    /// Add a word or phrase to the dictionary (ignoring case duplicates and silly sizes).
    pub fn add_word(&mut self, text: &str) {
        let t = text.trim();
        if t.is_empty()
            || t.chars().count() > MAX_ENTRY_CHARS
            || self.dictionary.len() >= MAX_DICTIONARY
        {
            return;
        }
        let lower = t.to_lowercase();
        if !self.dictionary.iter().any(|d| d.to_lowercase() == lower) {
            self.dictionary.push(t.to_string());
        }
    }

    pub fn load() -> Self {
        let Some(path) = config_file() else {
            return Self::default();
        };
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        let mut s: Settings = serde_json::from_slice(&bytes).unwrap_or_default();
        if !s.thesaurus_seeded {
            s.thesaurus_on = true;
            s.thesaurus_seeded = true;
        }
        s.sanitize();
        s
    }

    pub fn save(&self) {
        let Some(path) = config_file() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

fn config_file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("GrammarDesk").join("settings.json"))
}
