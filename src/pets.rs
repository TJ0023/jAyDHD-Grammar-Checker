//! Typing pets, ported from WordCraft: any number of floating pet boxes (up to eight), each with
//! a name, a size, a shake level and either the drawn creature or your own pictures (PNG, JPG,
//! animated GIF or APNG) for the idle / left / right poses, plus saved presets of three pictures.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};
use serde::{Deserialize, Serialize};

use crate::settings::Settings;
use crate::ui_kit;

/// Most typing pets on screen at once.
pub const MAX_PETS: usize = 8;
/// Most saved image presets.
pub const MAX_PRESETS: usize = 10;

/// Body colours the pets are given in turn.
const PET_COLORS: [[u8; 3]; 6] = [
    [246, 190, 120],
    [170, 200, 245],
    [190, 225, 170],
    [235, 170, 200],
    [200, 180, 235],
    [245, 225, 140],
];

/// How long after the last key a pet keeps its paw up before it relaxes, in seconds.
const PET_RELAX: f64 = 0.45;

/// The pet boxes are light with dark text, as in WordCraft.
const BOX_FILL: Color32 = Color32::from_rgb(252, 251, 248);
const BOX_TEXT: Color32 = Color32::from_gray(35);
const BOX_RADIUS: u8 = 8;

/// How big a pet is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PetSize {
    Small,
    #[default]
    Medium,
    Large,
    Custom,
}

/// One typing pet: its own floating box with an editable name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pet {
    pub id: u32,
    pub name: String,
    pub color: [u8; 3],
    pub collapsed: bool,
    /// Picture files for Basic/Idle, Left and Right. An empty slot uses the Basic picture; with
    /// no pictures at all the pet is the drawn one.
    pub images: [Option<String>; 3],
    pub size: PetSize,
    /// The scale used when `size` is Custom (1.0 is Medium).
    pub scale: f32,
    /// How much it bounces as you type: 0 none (the picture only swaps), 1 to 3 more and more.
    pub shake: u8,
}

impl Default for Pet {
    fn default() -> Self {
        Self {
            id: 1,
            name: "Pet".to_string(),
            color: PET_COLORS[0],
            collapsed: false,
            images: [None, None, None],
            size: PetSize::Medium,
            scale: 1.0,
            shake: 2,
        }
    }
}

impl Pet {
    /// How much bigger or smaller than Medium it is drawn.
    fn factor(&self) -> f32 {
        match self.size {
            PetSize::Small => 0.7,
            PetSize::Medium => 1.0,
            PetSize::Large => 1.5,
            PetSize::Custom => self.scale.clamp(0.3, 4.0),
        }
    }

    /// The height of its hop in points, from the shake level and the size.
    fn amplitude(&self) -> f32 {
        let level = [0.0_f32, 3.0, 6.0, 10.0]
            .get(usize::from(self.shake))
            .copied()
            .unwrap_or(0.0);
        level * self.factor().clamp(0.7, 2.0)
    }
}

/// A saved set of three pictures with a name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PetPreset {
    pub name: String,
    pub images: [Option<String>; 3],
}

/// Put pets and presets read from the settings file back in range.
pub fn sanitize(pets: &mut Vec<Pet>, presets: &mut Vec<PetPreset>) {
    pets.truncate(MAX_PETS);
    let mut seen = HashSet::new();
    let mut next = pets.iter().map(|p| p.id).max().unwrap_or(0) + 1;
    for p in pets.iter_mut() {
        p.name = p.name.chars().take(24).collect();
        p.shake = p.shake.min(3);
        if !p.scale.is_finite() {
            p.scale = 1.0;
        }
        if !seen.insert(p.id) {
            p.id = next;
            next += 1;
            seen.insert(p.id);
        }
    }
    presets.truncate(MAX_PRESETS);
    for p in presets.iter_mut() {
        p.name = p.name.chars().take(24).collect();
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Stance {
    Idle,
    Left,
    Right,
}

/// A loaded picture: one frame, or every frame of a GIF or APNG with how long each shows.
pub struct PetPicture {
    frames: Vec<(egui::TextureHandle, f32)>,
    total: f32,
    size: egui::Vec2,
    /// Per frame, the runs of visible pixels as (row, from column, to column), and the size of
    /// the frame in pixels. They give the window the exact outline of the picture.
    masks: Vec<Vec<[u16; 3]>>,
    mask_size: [u32; 2],
}

/// The Add Pet window (or, for an existing pet, its Images window).
pub struct PetEditor {
    /// The pet being edited, or `None` for a new one.
    target: Option<u32>,
    name: String,
    images: [Option<String>; 3],
    preset_name: String,
    preset_confirm: Option<usize>,
    note: String,
}

impl PetEditor {
    fn new(target: Option<u32>, name: String, images: [Option<String>; 3]) -> Self {
        Self {
            target,
            name,
            images,
            preset_name: String::new(),
            preset_confirm: None,
            note: String::new(),
        }
    }
}

/// Small bits of pet-box state that are not saved.
#[derive(Default)]
struct PetUi {
    /// The pet whose name is being edited, and whether its field still has to take focus.
    renaming: Option<u32>,
    focus_pending: bool,
    /// Pets whose cog options are open.
    settings_open: HashSet<u32>,
    /// Where each pet box was last frame: the pointer being inside it is "hovered".
    rects: HashMap<u32, egui::Rect>,
}

/// Everything about the pets that is not saved.
pub struct PetRuntime {
    next_id: u32,
    keys: u64,
    last_key: f64,
    /// Key presses seen system-wide that this frame has not counted yet.
    external: usize,
    pictures: HashMap<String, Option<Arc<PetPicture>>>,
    ui: PetUi,
    editor: Option<PetEditor>,
}

impl PetRuntime {
    pub fn new(settings: &Settings) -> Self {
        Self {
            next_id: settings.pets.iter().map(|p| p.id).max().unwrap_or(0) + 1,
            keys: 0,
            last_key: -10.0,
            external: 0,
            pictures: HashMap::new(),
            ui: PetUi::default(),
            editor: None,
        }
    }

    /// Key presses reported by the system-wide watcher (typing in another program).
    pub fn add_external_keys(&mut self, n: usize) {
        self.external += n;
    }

    /// Open the Add Pet window.
    pub fn open_add(&mut self) {
        self.editor = Some(PetEditor::new(None, String::new(), [None, None, None]));
    }
}

/// Key presses this frame (a letter arrives as a key and as text; the larger count is used).
fn typed_this_frame(ctx: &egui::Context) -> usize {
    ctx.input(|i| {
        let keys = i
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    egui::Event::Key {
                        pressed: true,
                        repeat: false,
                        ..
                    }
                )
            })
            .count();
        let texts = i
            .events
            .iter()
            .filter(|e| matches!(e, egui::Event::Text(_)))
            .count();
        keys.max(texts)
    })
}

/// Pet `index`'s stance and bounce now. Every key alternates the raised paw; neighbouring pets
/// start on opposite paws so they do not move in lockstep.
fn pose(rt: &PetRuntime, index: usize, now: f64) -> (Stance, f32) {
    let since = now - rt.last_key;
    if !(0.0..PET_RELAX).contains(&since) {
        return (Stance::Idle, 0.0);
    }
    let stance = if (rt.keys + index as u64).is_multiple_of(2) {
        Stance::Left
    } else {
        Stance::Right
    };
    // 0 at rest, down to -1 at the top of the hop: each pet scales it by its own shake level.
    let bounce = if since < 0.16 {
        -(std::f32::consts::PI * since as f32 / 0.16).sin()
    } else {
        0.0
    };
    (stance, bounce)
}

/// Count this frame's key presses and keep redrawing while a pet is mid-hop. `text_changed` is
/// true when the text changed this frame without a key event (so the pets still react).
fn tick(rt: &mut PetRuntime, ctx: &egui::Context, now: f64, text_changed: bool) {
    let mut n = typed_this_frame(ctx).max(std::mem::take(&mut rt.external));
    // A change with no key press (Apply Change, say) counts once; one that follows a key press
    // from the frame before is that same key, not a second one.
    if n == 0 && text_changed && now - rt.last_key > 0.2 {
        n = 1;
    }
    // One keystroke moves the paw once, however many events or frames it arrives as: the paw
    // changes at most once every 45 ms, which is faster than anyone types.
    if n > 0 {
        if now - rt.last_key > 0.045 {
            rt.keys += 1;
        }
        rt.last_key = now;
    }
    if now - rt.last_key < PET_RELAX + 0.05 {
        ctx.request_repaint();
    }
}

/// Which picture a stance shows: its own, else Basic, else any other. With one picture in all
/// three slots that is the same picture for every stance, so the pet stays still.
fn stance_path(pet: &Pet, stance: Stance) -> Option<&str> {
    let [idle, left, right] = &pet.images;
    let own = match stance {
        Stance::Idle => idle,
        Stance::Left => left,
        Stance::Right => right,
    };
    own.as_deref()
        .or(idle.as_deref())
        .or(left.as_deref())
        .or(right.as_deref())
}

/// Read a PNG, JPG, GIF, APNG (or WebP, BMP) file into textures. Large pictures are shrunk to
/// 320 points on the long side and long animations stop at 120 frames, to keep memory small.
fn load_picture(ctx: &egui::Context, path: &str) -> Result<PetPicture, String> {
    use image::AnimationDecoder;
    use std::io::Cursor;
    const MAX_BYTES: u64 = 25_000_000;
    const MAX_FRAMES: usize = 120;
    const MAX_SIDE: u32 = 320;

    let len = std::fs::metadata(path)
        .map_err(|e| format!("Could not open the file: {e}"))?
        .len();
    if len > MAX_BYTES {
        return Err("That file is over 25 MB.".to_string());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Could not read the file: {e}"))?;
    let format = image::guess_format(&bytes)
        .map_err(|e| format!("Not a picture this program can read: {e}"))?;
    let secs = |frame: &image::Frame| {
        let (n, d) = frame.delay().numer_denom_ms();
        let s = n as f32 / d.max(1) as f32 / 1000.0;
        if s < 0.02 { 0.1 } else { s }
    };
    let mut raw: Vec<(image::RgbaImage, f32)> = Vec::new();
    let bad = |e: image::ImageError| format!("Could not decode the picture: {e}");
    match format {
        image::ImageFormat::Gif => {
            let decoder =
                image::codecs::gif::GifDecoder::new(Cursor::new(&bytes[..])).map_err(bad)?;
            for frame in decoder.into_frames().take(MAX_FRAMES) {
                let frame = frame.map_err(bad)?;
                let d = secs(&frame);
                raw.push((frame.into_buffer(), d));
            }
        }
        image::ImageFormat::Png => {
            let decoder =
                image::codecs::png::PngDecoder::new(Cursor::new(&bytes[..])).map_err(bad)?;
            if decoder.is_apng().unwrap_or(false) {
                let apng = decoder.apng().map_err(bad)?;
                for frame in apng.into_frames().take(MAX_FRAMES) {
                    let frame = frame.map_err(bad)?;
                    let d = secs(&frame);
                    raw.push((frame.into_buffer(), d));
                }
            } else {
                raw.push((
                    image::DynamicImage::from_decoder(decoder)
                        .map_err(bad)?
                        .to_rgba8(),
                    0.0,
                ));
            }
        }
        _ => raw.push((
            image::load_from_memory(&bytes).map_err(bad)?.to_rgba8(),
            0.0,
        )),
    }
    let Some((first, _)) = raw.first() else {
        return Err("The picture has no frames.".to_string());
    };
    let (w0, h0) = first.dimensions();
    if w0 == 0 || h0 == 0 {
        return Err("The picture is empty.".to_string());
    }
    let shrink = (MAX_SIDE as f32 / w0.max(h0) as f32).min(1.0);
    let (w, h) = (
        ((w0 as f32 * shrink).round() as u32).max(1),
        ((h0 as f32 * shrink).round() as u32).max(1),
    );
    let mut frames = Vec::new();
    let mut masks: Vec<Vec<[u16; 3]>> = Vec::new();
    let mut total = 0.0;
    for (i, (buffer, delay)) in raw.into_iter().enumerate() {
        let buffer = if buffer.dimensions() == (w, h) {
            buffer
        } else {
            image::imageops::thumbnail(&buffer, w, h)
        };
        let color =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], buffer.as_raw());
        let texture = ctx.load_texture(
            format!("typing_pet:{path}:{i}"),
            color,
            egui::TextureOptions::LINEAR,
        );
        total += delay;
        frames.push((texture, delay));
        // Where the picture can be seen (a little alpha is enough).
        let mut runs: Vec<[u16; 3]> = Vec::new();
        for y in 0..h {
            let mut start: Option<u32> = None;
            for x in 0..w {
                let seen = buffer.get_pixel(x, y).0[3] > 24;
                match (seen, start) {
                    (true, None) => start = Some(x),
                    (false, Some(x0)) => {
                        runs.push([y as u16, x0 as u16, x as u16]);
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(x0) = start {
                runs.push([y as u16, x0 as u16, w as u16]);
            }
        }
        masks.push(runs);
    }
    Ok(PetPicture {
        frames,
        total,
        size: egui::vec2(w0 as f32, h0 as f32),
        masks,
        mask_size: [w, h],
    })
}

/// The cached picture for `path`, loading it the first time. A picture that fails is remembered
/// as failed, so it is not read again every frame.
fn picture(
    cache: &mut HashMap<String, Option<Arc<PetPicture>>>,
    ctx: &egui::Context,
    path: &str,
) -> Option<Arc<PetPicture>> {
    if let Some(entry) = cache.get(path) {
        return entry.clone();
    }
    let loaded = load_picture(ctx, path).ok().map(Arc::new);
    cache.insert(path.to_string(), loaded.clone());
    loaded
}

/// Load `path` now, reporting why it failed; used when a picture is picked.
fn check_picture(rt: &mut PetRuntime, ctx: &egui::Context, path: &str) -> Result<(), String> {
    match load_picture(ctx, path) {
        Ok(p) => {
            rt.pictures.insert(path.to_string(), Some(Arc::new(p)));
            Ok(())
        }
        Err(e) => {
            rt.pictures.insert(path.to_string(), None);
            Err(e)
        }
    }
}

/// Draw a picture fitted into `rect` (keeping its proportions), at the frame for time `now`.
/// Which frame of the picture shows at time `now`.
fn frame_index(pic: &PetPicture, now: f64) -> usize {
    if pic.frames.len() > 1 && pic.total > 0.0 {
        let t = (now % f64::from(pic.total)) as f32;
        let mut acc = 0.0;
        for (i, f) in pic.frames.iter().enumerate() {
            acc += f.1;
            if t < acc {
                return i;
            }
        }
    }
    0
}

/// Where the picture is drawn inside `rect`.
fn picture_dest(rect: egui::Rect, pic: &PetPicture, bounce: f32) -> egui::Rect {
    let fit = (rect.width() / pic.size.x.max(1.0)).min(rect.height() / pic.size.y.max(1.0));
    egui::Rect::from_center_size(rect.center() + egui::vec2(0.0, bounce), pic.size * fit)
}

fn draw_picture(
    painter: &egui::Painter,
    rect: egui::Rect,
    pic: &PetPicture,
    now: f64,
    bounce: f32,
) {
    let Some((texture, _)) = pic.frames.get(frame_index(pic, now)) else {
        return;
    };
    painter.image(
        texture.id(),
        picture_dest(rect, pic, bounce),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// The visible parts of the picture as rectangles (where it is drawn now), for cutting the
/// pet's window to the picture's outline. `None` when it would be too many to be worth it.
fn picture_outline(
    rect: egui::Rect,
    pic: &PetPicture,
    now: f64,
    bounce: f32,
) -> Option<Vec<egui::Rect>> {
    let runs = pic.masks.get(frame_index(pic, now))?;
    let [w, h] = pic.mask_size;
    if runs.is_empty() || runs.len() > 4000 || w == 0 || h == 0 {
        return None;
    }
    let dest = picture_dest(rect, pic, bounce);
    let (sx, sy) = (dest.width() / w as f32, dest.height() / h as f32);
    Some(
        runs.iter()
            .map(|&[y, x0, x1]| {
                egui::Rect::from_min_max(
                    egui::pos2(
                        dest.min.x + f32::from(x0) * sx,
                        dest.min.y + f32::from(y) * sy,
                    ),
                    egui::pos2(
                        dest.min.x + f32::from(x1) * sx,
                        dest.min.y + (f32::from(y) + 1.0) * sy,
                    ),
                )
            })
            .collect(),
    )
}

/// A plain chevron, up when the box is open (press to fold it) and down when it is folded.
fn arrow_button(ui: &mut Ui, up: bool, tint: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 20.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_black_alpha(22));
    }
    let c = rect.center();
    let (dx, dy) = (5.5, 3.0);
    let (left, mid, right) = if up {
        (
            egui::pos2(c.x - dx, c.y + dy),
            egui::pos2(c.x, c.y - dy),
            egui::pos2(c.x + dx, c.y + dy),
        )
    } else {
        (
            egui::pos2(c.x - dx, c.y - dy),
            egui::pos2(c.x, c.y + dy),
            egui::pos2(c.x + dx, c.y - dy),
        )
    };
    let stroke = Stroke::new(2.0, tint);
    ui.painter().line_segment([left, mid], stroke);
    ui.painter().line_segment([mid, right], stroke);
    response.clicked()
}

/// A small painted X for closing a box.
fn close_button(ui: &mut Ui, tint: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 20.0), egui::Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Remove this pet");
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_black_alpha(22));
    }
    let c = rect.center();
    let stroke = Stroke::new(1.8, tint);
    ui.painter().line_segment(
        [
            egui::pos2(c.x - 4.0, c.y - 4.0),
            egui::pos2(c.x + 4.0, c.y + 4.0),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            egui::pos2(c.x - 4.0, c.y + 4.0),
            egui::pos2(c.x + 4.0, c.y - 4.0),
        ],
        stroke,
    );
    response.clicked()
}

/// A painted pencil: press it to rename.
fn pencil_button(ui: &mut Ui, tint: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 20.0), egui::Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Rename");
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_black_alpha(22));
    }
    let c = rect.center();
    ui.painter().line_segment(
        [
            egui::pos2(c.x - 3.0, c.y + 3.0),
            egui::pos2(c.x + 5.0, c.y - 5.0),
        ],
        Stroke::new(3.4, tint),
    );
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(c.x - 7.0, c.y + 7.0),
            egui::pos2(c.x - 6.0, c.y + 2.0),
            egui::pos2(c.x - 2.0, c.y + 6.0),
        ],
        tint,
        Stroke::NONE,
    ));
    response.clicked()
}

/// A painted cog: press it for the pet's options.
fn cog_button(ui: &mut Ui, tint: Color32, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 20.0), egui::Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Size and shake");
    if response.hovered() || active {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_black_alpha(22));
    }
    let c = rect.center();
    let p = ui.painter();
    p.circle_stroke(c, 4.6, Stroke::new(1.8, tint));
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        let (sin, cos) = a.sin_cos();
        p.line_segment(
            [
                egui::pos2(c.x + cos * 6.2, c.y + sin * 6.2),
                egui::pos2(c.x + cos * 8.4, c.y + sin * 8.4),
            ],
            Stroke::new(2.4, tint),
        );
    }
    p.circle_filled(c, 1.6, tint);
    response.clicked()
}

/// The narrowest a pet box gets, so its buttons always fit.
const MIN_INNER: f32 = 176.0;

#[derive(Default)]
struct PetBoxOut {
    remove: bool,
    edit_images: bool,
}

/// A mix of `c` with white, for an option that is not the chosen one.
fn faded(c: Color32) -> Color32 {
    let m = |v: u8| (f32::from(v) * 0.45 + 255.0 * 0.55).round() as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

/// One option of a choice: a coloured button with dark text, strong with a dark outline when it is
/// the chosen one and pale when it is not. Returns true when pressed.
fn choice_chip(ui: &mut Ui, font_size: f32, label: &str, base: Color32, selected: bool) -> bool {
    let mut text = RichText::new(label)
        .size(font_size - 1.0)
        .color(Color32::from_gray(35));
    if selected {
        text = text.strong();
    }
    let outline = if selected {
        Stroke::new(2.0, Color32::from_gray(50))
    } else {
        Stroke::new(1.0, Color32::from_gray(190))
    };
    let chip = egui::Button::new(text)
        .fill(if selected { base } else { faded(base) })
        .stroke(outline)
        .corner_radius(CornerRadius::same(BOX_RADIUS));
    ui.add(chip).clicked()
}

/// One pet's box. Header: the name with a pencil to rename it, then the cog, fold and remove
/// buttons. Below: the pet (its pictures or the drawn one), the cog's options when open, and a
/// grip in the corner to drag the box bigger or smaller. When `visible` is false the frame and
/// the buttons are not drawn, only the pet; they still take their space.
#[allow(clippy::too_many_arguments)]
fn pet_box(
    ui: &mut Ui,
    font_size: f32,
    pet: &mut Pet,
    state: &mut PetUi,
    pic: Option<&PetPicture>,
    stance: Stance,
    unit: f32,
    now: f64,
    inner: f32,
    visible: bool,
) -> PetBoxOut {
    let mut out = PetBoxOut::default();
    let text_color = BOX_TEXT;
    let (fill, outline) = if visible {
        (BOX_FILL, Stroke::new(1.0, Color32::from_gray(200)))
    } else {
        (Color32::TRANSPARENT, Stroke::NONE)
    };
    let framed = Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(BOX_RADIUS))
        .stroke(outline)
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(inner);
            ui.scope(|ui| {
                if !visible {
                    ui.set_invisible();
                }
                ui.horizontal(|ui| {
                    let name_w = (inner - 5.0 * 24.0 - 12.0).max(40.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(name_w, 20.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            if state.renaming == Some(pet.id) {
                                let field = ui.add(
                                    egui::TextEdit::singleline(&mut pet.name)
                                        .font(egui::FontId::proportional(font_size))
                                        .text_color(text_color)
                                        .char_limit(24)
                                        .hint_text("Name your pet")
                                        .desired_width(name_w),
                                );
                                if state.focus_pending {
                                    field.request_focus();
                                    state.focus_pending = false;
                                } else if field.lost_focus() {
                                    state.renaming = None;
                                }
                            } else {
                                let shown = if pet.name.trim().is_empty() {
                                    "Name your pet"
                                } else {
                                    pet.name.as_str()
                                };
                                let name_label = ui.add(
                                    egui::Label::new(
                                        RichText::new(shown)
                                            .strong()
                                            .size(font_size)
                                            .color(text_color),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click_and_drag()),
                                );
                                if name_label.drag_started() {
                                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                                }
                                if pencil_button(ui, text_color) {
                                    state.renaming = Some(pet.id);
                                    state.focus_pending = true;
                                }
                            }
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if close_button(ui, text_color) {
                            out.remove = true;
                        }
                        if arrow_button(ui, !pet.collapsed, text_color) {
                            pet.collapsed = !pet.collapsed;
                        }
                        ui_kit::drag_handle(ui, text_color);
                        let open = state.settings_open.contains(&pet.id);
                        if cog_button(ui, text_color, open) {
                            if open {
                                state.settings_open.remove(&pet.id);
                            } else {
                                state.settings_open.insert(pet.id);
                            }
                        }
                    });
                });
            });
            if pet.collapsed {
                return;
            }
            let f = pet.factor();
            ui.vertical_centered(|ui| {
                // A little room above, so the hop is not clipped.
                let (rect, grab) = ui.allocate_exact_size(
                    egui::vec2(120.0 * f, 104.0 * f + 14.0),
                    egui::Sense::drag(),
                );
                // Grab the pet to move its window.
                if grab.drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                let area =
                    egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + 14.0), rect.max);
                let bounce = unit * pet.amplitude();
                if !visible {
                    // With no box showing, only the pet itself is visible and clickable: the
                    // window is cut to the outline of the picture.
                    let outline = pic.and_then(|p| picture_outline(area, p, now, bounce));
                    match outline {
                        Some(rects) => ui_kit::note_clip_rects(ui.ctx(), rects),
                        None => ui_kit::note_clip_rect(ui.ctx(), rect),
                    }
                }
                match pic {
                    Some(p) => draw_picture(ui.painter(), area, p, now, bounce),
                    None => draw_pet(ui.painter(), area, stance, bounce, pet.color),
                }
            });
            if state.settings_open.contains(&pet.id) {
                ui.separator();
                ui.label(
                    RichText::new("Size")
                        .strong()
                        .size(font_size)
                        .color(text_color),
                );
                ui.horizontal_wrapped(|ui| {
                    for (size, label, base) in [
                        (PetSize::Small, "Small", Color32::from_rgb(150, 200, 235)),
                        (PetSize::Medium, "Medium", Color32::from_rgb(165, 215, 170)),
                        (PetSize::Large, "Large", Color32::from_rgb(245, 200, 140)),
                        (PetSize::Custom, "Custom", Color32::from_rgb(205, 170, 235)),
                    ] {
                        if choice_chip(ui, font_size, label, base, pet.size == size) {
                            pet.size = size;
                        }
                    }
                });
                if pet.size == PetSize::Custom {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Scale")
                                .size(font_size - 1.0)
                                .color(text_color),
                        );
                        ui.add(egui::Slider::new(&mut pet.scale, 0.3..=4.0));
                    });
                }
                ui.label(
                    RichText::new("Shake")
                        .strong()
                        .size(font_size)
                        .color(text_color),
                );
                ui.label(
                    RichText::new(
                        "How much the pet bounces when you type. None only swaps the image.",
                    )
                    .size(font_size - 2.0)
                    .color(text_color),
                );
                ui.horizontal_wrapped(|ui| {
                    for (level, label, base) in [
                        (0u8, "None", Color32::from_rgb(205, 205, 212)),
                        (1, "Level 1", Color32::from_rgb(200, 225, 160)),
                        (2, "Level 2", Color32::from_rgb(245, 205, 140)),
                        (3, "Level 3", Color32::from_rgb(240, 150, 150)),
                    ] {
                        if choice_chip(ui, font_size, label, base, pet.shake == level) {
                            pet.shake = level;
                        }
                    }
                });
                if ui.button("Images\u{2026}").clicked() {
                    out.edit_images = true;
                }
            }
            // The resize grip: drag it to make the pet bigger or smaller (that makes the size Custom).
            ui.scope(|ui| {
                if !visible {
                    ui.set_invisible();
                }
                ui.allocate_ui_with_layout(
                    egui::vec2(inner, 18.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::drag());
                        let response = response
                            .on_hover_cursor(egui::CursorIcon::ResizeNwSe)
                            .on_hover_text("Drag to resize");
                        if response.dragged() {
                            let d = response.drag_delta();
                            pet.scale = (f + (d.x / 120.0 + d.y / 104.0) / 2.0).clamp(0.3, 4.0);
                            pet.size = PetSize::Custom;
                        }
                        let p = ui.painter();
                        for i in 1..=3 {
                            let o = i as f32 * 4.0;
                            p.line_segment(
                                [
                                    egui::pos2(rect.right() - o, rect.bottom() - 2.0),
                                    egui::pos2(rect.right() - 2.0, rect.bottom() - o),
                                ],
                                Stroke::new(1.4, Color32::from_gray(160)),
                            );
                        }
                    },
                );
            });
        });
    ui_kit::note_box_rect(ui.ctx(), framed.response.rect);
    out
}

/// Add a pet with these pictures.
fn add_pet(settings: &mut Settings, rt: &mut PetRuntime, name: &str, images: [Option<String>; 3]) {
    if settings.pets.len() >= MAX_PETS {
        return;
    }
    let id = rt.next_id;
    rt.next_id += 1;
    let color = PET_COLORS
        .get(id as usize % PET_COLORS.len())
        .copied()
        .unwrap_or(PET_COLORS[0]);
    let name: String = name.trim().chars().take(24).collect();
    let name = if name.is_empty() {
        format!("Pet {}", settings.pets.len() + 1)
    } else {
        name
    };
    settings.pets.push(Pet {
        id,
        name,
        color,
        images,
        ..Pet::default()
    });
}

/// Every pet as a floating box; drag each wherever you like. `text_changed` is true when the
/// text changed this frame without a key event.
pub fn overlay(
    settings: &mut Settings,
    rt: &mut PetRuntime,
    ctx: &egui::Context,
    text_changed: bool,
) {
    let now = ctx.input(|i| i.time);
    tick(rt, ctx, now, text_changed);
    if !settings.pets_open {
        return;
    }
    let font_size = settings.box_font_size;
    let mut remove = None;
    let mut edit_images = None;
    let mut animated = false;
    for index in 0..settings.pets.len() {
        let (stance, unit) = pose(rt, index, now);
        let Some(first) = settings.pets.get(index) else {
            continue;
        };
        let path = stance_path(first, stance).map(str::to_owned);
        let pic = path.and_then(|p| picture(&mut rt.pictures, ctx, &p));
        if pic.as_ref().is_some_and(|p| p.frames.len() > 1) {
            animated = true;
        }
        let Some(pet) = settings.pets.get_mut(index) else {
            continue;
        };
        let id = pet.id;
        let inner = (120.0 * pet.factor()).max(MIN_INNER);
        let ui_state = &mut rt.ui;
        let key = format!("pet{id}");
        let start = [16.0 + index as f32 * 30.0, 140.0 + index as f32 * 190.0];
        let at = settings.box_pos.get(&key).copied().unwrap_or(start);
        let moved = ui_kit::floating_box(
            ctx,
            ("typing_pet", id),
            "Pet",
            inner + 18.0,
            at,
            &mut |ui| {
                // Only the pet shows until the pointer is over its window. A folded box, one being
                // renamed and one with its options open stay shown, or they could not be found.
                let hovered = ui.ctx().pointer_hover_pos().is_some();
                let visible = hovered
                    || pet.collapsed
                    || ui_state.renaming == Some(id)
                    || ui_state.settings_open.contains(&id);
                let out = pet_box(
                    ui,
                    font_size,
                    pet,
                    ui_state,
                    pic.as_deref(),
                    stance,
                    unit,
                    now,
                    inner,
                    visible,
                );
                if out.remove {
                    remove = Some(id);
                }
                if out.edit_images {
                    edit_images = Some(id);
                }
            },
        );
        if let Some(p) = moved
            && at.iter().zip([p.x, p.y]).any(|(a, b)| (a - b).abs() > 0.5)
        {
            settings.box_pos.insert(key, [p.x, p.y]);
        }
    }
    if animated {
        ctx.request_repaint_after(Duration::from_millis(30));
    }
    if let Some(id) = edit_images
        && let Some(pet) = settings.pets.iter().find(|p| p.id == id)
    {
        rt.editor = Some(PetEditor::new(
            Some(id),
            pet.name.clone(),
            pet.images.clone(),
        ));
    }
    if let Some(id) = remove {
        settings.pets.retain(|p| p.id != id);
        rt.ui.settings_open.remove(&id);
        rt.ui.rects.remove(&id);
    }
}

/// The file name of a picture path, for the Add Pet window.
fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// The Add Pet window (and a pet's Images window): the three pictures, a note on what works, and
/// the saved presets.
pub fn editor_window(settings: &mut Settings, rt: &mut PetRuntime, ctx: &egui::Context) {
    let Some(mut editor) = rt.editor.take() else {
        return;
    };
    let presets = settings.presets.clone();
    let can_add = editor.target.is_some() || settings.pets.len() < MAX_PETS;
    let mut done = false;
    let mut pick: Option<usize> = None;
    let mut reset: Option<usize> = None;
    let mut save_preset = false;
    let mut apply: Option<usize> = None;
    let mut ask_delete: Option<usize> = None;
    let mut delete: Option<usize> = None;
    let mut cancel_delete = false;
    let title = if editor.target.is_some() {
        "Pet Images"
    } else {
        "Add Pet"
    };
    let closed = ui_kit::tool_window(ctx, "typing_pet_editor", title, [460.0, 680.0], &mut |ui| {
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.add(
                egui::TextEdit::singleline(&mut editor.name)
                    .char_limit(24)
                    .hint_text("Name your pet")
                    .desired_width(240.0),
            );
        });
        ui.separator();
        ui.label(RichText::new("Images").strong());
        ui.label("PNG, JPG, and animated GIF / APNG files work.");
        ui.label("For the most natural look, make all three images the same size and position.");
        ui.label(
            RichText::new(
                "An empty slot shows the Basic image. With only one image the pet stays still.",
            )
            .small(),
        );
        ui.add_space(4.0);
        for (slot, title) in [
            (0usize, "Basic / Idle"),
            (1, "Left (Typing Left)"),
            (2, "Right (Typing Right)"),
        ] {
            let current = editor.images.get(slot).cloned().flatten();
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 14.0), egui::Sense::hover());
                let dot_color = if current.is_some() {
                    Color32::from_rgb(80, 180, 105)
                } else {
                    Color32::from_gray(150)
                };
                ui.painter().circle_filled(dot.center(), 4.5, dot_color);
                ui.label(RichText::new(title).strong());
                ui.label(if current.is_some() {
                    "custom image"
                } else {
                    "not set"
                });
            });
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                match &current {
                    Some(path) => {
                        if ui.button("Change Image").clicked() {
                            pick = Some(slot);
                        }
                        if ui.button("Reset").clicked() {
                            reset = Some(slot);
                        }
                        ui.label(RichText::new(file_name(path)).small());
                    }
                    None => {
                        if ui.button("Insert Image").clicked() {
                            pick = Some(slot);
                        }
                    }
                }
            });
        }
        if !editor.note.is_empty() {
            ui.colored_label(Color32::from_rgb(190, 60, 60), editor.note.clone());
        }
        ui.separator();
        ui.label(RichText::new(format!("Presets ({} of {MAX_PRESETS})", presets.len())).strong());
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut editor.preset_name)
                    .char_limit(24)
                    .hint_text("Preset name")
                    .desired_width(200.0),
            );
            let name = editor.preset_name.trim();
            let exists = presets.iter().any(|p| p.name.eq_ignore_ascii_case(name));
            if ui
                .add_enabled(
                    !name.is_empty() && (exists || presets.len() < MAX_PRESETS),
                    egui::Button::new("Save preset"),
                )
                .clicked()
            {
                save_preset = true;
            }
        });
        egui::ScrollArea::vertical().max_height(150.0).id_salt("typing_pet_presets").show(ui, |ui| {
            if presets.is_empty() {
                ui.label("No presets yet. Set the three images, name the preset and press Save preset.");
            }
            for (i, preset) in presets.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(&preset.name);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if editor.preset_confirm == Some(i) {
                            if ui.button("Cancel").clicked() {
                                cancel_delete = true;
                            }
                            if ui.button("Yes, delete").clicked() {
                                delete = Some(i);
                            }
                        } else {
                            if ui.button("Delete").clicked() {
                                ask_delete = Some(i);
                            }
                            if ui.button("Apply").clicked() {
                                apply = Some(i);
                            }
                        }
                    });
                });
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            let label = if editor.target.is_some() {
                "Done"
            } else {
                "Add Pet"
            };
            if ui.add_enabled(can_add, egui::Button::new(label)).clicked() {
                done = true;
            }
            if !can_add {
                ui.label("The most pets are already out.");
            }
        });
    });

    if let Some(slot) = reset
        && let Some(entry) = editor.images.get_mut(slot)
    {
        *entry = None;
        editor.note.clear();
    }
    if let Some(i) = apply
        && let Some(p) = presets.get(i)
    {
        editor.images = p.images.clone();
        editor.note.clear();
    }
    if cancel_delete {
        editor.preset_confirm = None;
    }
    if let Some(i) = ask_delete {
        editor.preset_confirm = Some(i);
    }
    if let Some(i) = delete
        && settings.presets.get(i).is_some()
    {
        settings.presets.remove(i);
        editor.preset_confirm = None;
    }
    if save_preset {
        let name: String = editor.preset_name.trim().chars().take(24).collect();
        if !name.is_empty() {
            let preset = PetPreset {
                name: name.clone(),
                images: editor.images.clone(),
            };
            let at = settings
                .presets
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(&name));
            match at {
                Some(i) => {
                    if let Some(existing) = settings.presets.get_mut(i) {
                        *existing = preset;
                    }
                }
                None if settings.presets.len() < MAX_PRESETS => settings.presets.push(preset),
                None => {}
            }
        }
    }
    if let Some(slot) = pick
        && let Some(file) = rfd::FileDialog::new()
            .add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
            .pick_file()
    {
        let path = file.to_string_lossy().into_owned();
        match check_picture(rt, ctx, &path) {
            Ok(()) => {
                if let Some(entry) = editor.images.get_mut(slot) {
                    *entry = Some(path);
                }
                editor.note.clear();
            }
            Err(e) => editor.note = e,
        }
    }
    if done {
        match editor.target {
            Some(id) => {
                if let Some(pet) = settings.pets.iter_mut().find(|p| p.id == id) {
                    pet.images = editor.images.clone();
                    let name: String = editor.name.trim().chars().take(24).collect();
                    if !name.is_empty() {
                        pet.name = name;
                    }
                }
            }
            None => add_pet(settings, rt, &editor.name, editor.images.clone()),
        }
        return;
    }
    if !closed {
        rt.editor = Some(editor);
    }
}

fn shade(rgb: [u8; 3], factor: f32) -> Color32 {
    let m = |c: u8| (f32::from(c) * factor).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(m(rgb[0]), m(rgb[1]), m(rgb[2]))
}

/// The drawn pet: a round, eared creature at a tiny keyboard, one paw up while it types. `rect`
/// is 120 by 104 at Medium size and scales with the pet; `bounce` is its hop in points.
pub fn draw_pet(
    painter: &egui::Painter,
    rect: egui::Rect,
    stance: Stance,
    bounce: f32,
    rgb: [u8; 3],
) {
    let k = (rect.width() / 120.0).max(0.1);
    let at = |x: f32, y: f32| {
        egui::pos2(
            rect.center().x + x * k,
            rect.center().y + (y + 2.0) * k + bounce,
        )
    };
    let line = |w: f32| (w * k).max(1.0);
    let body = shade(rgb, 1.0);
    let edge = shade(rgb, 0.7);
    let belly = shade(rgb, 1.12);
    let ink = Color32::from_rgb(45, 40, 50);
    // A soft shadow on the ground; it narrows a little as the pet hops.
    let shadow = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.bottom() - 18.0 * k),
        egui::vec2((60.0 * k + bounce).max(10.0), 6.0 * k),
    );
    painter.rect_filled(shadow, CornerRadius::same(3), Color32::from_black_alpha(26));
    for side in [-1.0_f32, 1.0] {
        let ear = vec![
            at(side * 27.0, -12.0),
            at(side * 22.0, -36.0),
            at(side * 7.0, -25.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            ear,
            body,
            Stroke::new(line(1.5), edge),
        ));
        let inner = vec![
            at(side * 23.0, -17.0),
            at(side * 20.0, -30.0),
            at(side * 12.0, -24.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            inner,
            Color32::from_rgb(244, 190, 200),
            Stroke::NONE,
        ));
    }
    painter.circle(at(0.0, 0.0), 30.0 * k, body, Stroke::new(line(1.5), edge));
    painter.circle_filled(at(0.0, 12.0), 15.0 * k, belly);
    for side in [-1.0_f32, 1.0] {
        painter.circle_filled(at(side * 11.0, -4.0), 3.4 * k, ink);
        painter.circle_filled(at(side * 11.0 + 1.0, -5.2), 1.1 * k, Color32::WHITE);
        painter.circle_filled(
            at(side * 19.0, 3.0),
            4.0 * k,
            Color32::from_rgba_unmultiplied(240, 130, 150, 70),
        );
    }
    let mouth = Stroke::new(line(1.4), ink);
    painter.line_segment([at(-4.0, 3.0), at(0.0, 6.0)], mouth);
    painter.line_segment([at(0.0, 6.0), at(4.0, 3.0)], mouth);
    // The keyboard stays put; the paws are on it, and one lifts with each key.
    let board = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.bottom() - 9.0 * k),
        egui::vec2(84.0 * k, 12.0 * k),
    );
    painter.rect_filled(board, CornerRadius::same(4), Color32::from_gray(95));
    for i in 0..7 {
        let x = board.left() + (8.0 + i as f32 * 11.0) * k;
        painter.line_segment(
            [
                egui::pos2(x, board.center().y),
                egui::pos2(x + 6.0 * k, board.center().y),
            ],
            Stroke::new(line(2.0), Color32::from_gray(150)),
        );
    }
    let (left_lift, right_lift) = match stance {
        Stance::Idle => (0.0, 0.0),
        Stance::Left => (13.0, 0.0),
        Stance::Right => (0.0, 13.0),
    };
    for (side, lift) in [(-1.0_f32, left_lift), (1.0, right_lift)] {
        painter.circle(
            egui::pos2(
                at(side * 17.0, 0.0).x,
                rect.bottom() - (20.0 + lift) * k + bounce,
            ),
            8.0 * k,
            body,
            Stroke::new(line(1.5), edge),
        );
    }
}
