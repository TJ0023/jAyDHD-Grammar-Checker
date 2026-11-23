//! The look of the boxes, ported from WordCraft's writing panel: painted icons and buttons,
//! the card frame with its fold arrow, coloured chips and the colour scales.

use std::time::Duration;

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};

use crate::analysis::LengthBucket;
use crate::settings::Settings;

/// Width of a box, in points.
pub const BOX_W: f32 = 300.0;

pub fn color(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// Pastel tints for the six length ranges, short to long: light purple, blue, green, yellow,
/// orange, red. Low alpha on the page so the grammar highlight stays the strongest.
pub fn bucket_color(b: LengthBucket) -> Color32 {
    let (r, g, bl) = match b {
        LengthBucket::One => (190, 170, 230),
        LengthBucket::TwoToSix => (150, 190, 240),
        LengthBucket::SevenToFifteen => (160, 215, 170),
        LengthBucket::SixteenToTwentyFive => (250, 235, 150),
        LengthBucket::TwentySixToThirtyNine => (250, 200, 150),
        LengthBucket::FortyPlus => (245, 160, 160),
    };
    Color32::from_rgba_unmultiplied(r, g, bl, 90)
}

/// Reading level colour from a Dale-Chall score: green for plain text through red for dense text.
pub fn grade_color(score: Option<f64>) -> Color32 {
    match score {
        None => Color32::from_rgb(225, 225, 230),
        Some(g) if g < 5.0 => Color32::from_rgb(170, 220, 175),
        Some(g) if g < 6.0 => Color32::from_rgb(205, 228, 160),
        Some(g) if g < 7.0 => Color32::from_rgb(250, 235, 150),
        Some(g) if g < 8.0 => Color32::from_rgb(250, 205, 150),
        Some(g) if g < 9.0 => Color32::from_rgb(245, 175, 160),
        Some(_) => Color32::from_rgb(225, 170, 200),
    }
}

/// Word count colour: by progress toward the goal when there is one (red, orange, green),
/// otherwise a calm blue.
pub fn count_color(words: usize, goal: usize) -> Color32 {
    if goal == 0 {
        return Color32::from_rgb(205, 222, 245);
    }
    let done = words as f64 / goal as f64;
    if done < 0.5 {
        Color32::from_rgb(245, 170, 170)
    } else if done < 1.0 {
        Color32::from_rgb(250, 215, 150)
    } else {
        Color32::from_rgb(170, 220, 175)
    }
}

/// Walks a pastel scale from cool to hot: `t` is 0 (pale blue) to 1 (red).
pub fn heat(t: f64) -> Color32 {
    const STOPS: [(f64, f64, f64); 5] = [
        (190.0, 215.0, 245.0),
        (170.0, 220.0, 175.0),
        (250.0, 235.0, 150.0),
        (250.0, 200.0, 150.0),
        (245.0, 160.0, 160.0),
    ];
    let t = t.clamp(0.0, 1.0) * (STOPS.len() - 1) as f64;
    let i = (t.floor() as usize).min(STOPS.len() - 2);
    let f = t - i as f64;
    let (a, b) = (
        STOPS.get(i).copied().unwrap_or(STOPS[0]),
        STOPS.get(i + 1).copied().unwrap_or(STOPS[0]),
    );
    let mix = |x: f64, y: f64| (x + (y - x) * f).round() as u8;
    Color32::from_rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// A keyword's share of the document at which its colour reaches full red.
pub const KEYWORD_RED_AT: f64 = 4.0;

/// 10794 -> "10,794".
pub fn group(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A mix of `c` with white, for an option that is not the chosen one.
fn faded(c: Color32) -> Color32 {
    let m = |v: u8| (f32::from(v) * 0.45 + 255.0 * 0.55).round() as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

/// One option of a choice: a coloured button with dark text, strong with a dark outline when it is
/// the chosen one and pale when it is not. Returns true when pressed.
pub fn choice_chip(ui: &mut Ui, s: &Settings, label: &str, base: Color32, selected: bool) -> bool {
    let mut text = RichText::new(label)
        .size(s.box_font_size - 1.0)
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
        .corner_radius(CornerRadius::same(s.corner_radius.min(8)));
    ui.add(chip).clicked()
}

/// A coloured, rounded line of text.
pub fn chip(ui: &mut Ui, s: &Settings, fill: Color32, text: String) {
    Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(s.corner_radius))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .size(s.box_font_size)
                    .color(Color32::from_gray(35)),
            );
        });
}

/// A small coloured label such as "Synonyms" or "Noun".
pub fn tag(ui: &mut Ui, size: f32, fill: Color32, text: &str) {
    Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .strong()
                    .size((size - 1.0).max(9.0))
                    .color(Color32::from_gray(35)),
            );
        });
}

#[derive(Clone, Copy)]
pub enum Icon {
    Quotes,
    Magnifier,
    Bars,
    BookPen,
    Thesaurus,
}

/// A small line drawing beside a box title. Painted, not a font symbol, so it looks the same on
/// every system.
pub fn icon(ui: &mut Ui, kind: Icon, tint: Color32) {
    let width = if matches!(kind, Icon::BookPen) {
        34.0
    } else {
        20.0
    };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 18.0), egui::Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let stroke = Stroke::new(1.6, tint);
    match kind {
        Icon::Quotes => {
            for dx in [-5.0_f32, 4.0] {
                p.circle_filled(egui::pos2(c.x + dx, c.y - 1.5), 3.0, tint);
                p.line_segment(
                    [
                        egui::pos2(c.x + dx + 2.0, c.y - 0.5),
                        egui::pos2(c.x + dx - 0.5, c.y + 5.5),
                    ],
                    Stroke::new(2.0, tint),
                );
            }
        }
        Icon::Magnifier => {
            p.circle_stroke(egui::pos2(c.x - 2.0, c.y - 2.0), 5.5, stroke);
            p.line_segment(
                [
                    egui::pos2(c.x + 2.0, c.y + 2.0),
                    egui::pos2(c.x + 7.0, c.y + 7.0),
                ],
                Stroke::new(2.4, tint),
            );
        }
        Icon::Thesaurus => {
            // A little dictionary: a page with lines of text.
            let page = egui::Rect::from_center_size(c, egui::vec2(13.0, 16.0));
            p.rect_stroke(
                page,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            for (i, w) in [7.0_f32, 7.0, 4.0].into_iter().enumerate() {
                let y = c.y - 3.5 + i as f32 * 3.6;
                p.line_segment(
                    [egui::pos2(c.x - 3.5, y), egui::pos2(c.x - 3.5 + w, y)],
                    Stroke::new(1.3, tint),
                );
            }
        }
        Icon::Bars => {
            let base = c.y + 7.0;
            for (i, h) in [7.0_f32, 13.0, 9.0].into_iter().enumerate() {
                let x = c.x - 8.0 + i as f32 * 6.0;
                p.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(x, base - h), egui::pos2(x + 4.0, base)),
                    CornerRadius::same(1),
                    tint,
                );
            }
        }
        Icon::BookPen => {
            let x0 = c.x - 15.0;
            let left = vec![
                egui::pos2(x0, c.y - 6.0),
                egui::pos2(x0 + 8.0, c.y - 4.0),
                egui::pos2(x0 + 8.0, c.y + 6.0),
                egui::pos2(x0, c.y + 4.0),
            ];
            let right = vec![
                egui::pos2(x0 + 8.5, c.y - 4.0),
                egui::pos2(x0 + 16.5, c.y - 6.0),
                egui::pos2(x0 + 16.5, c.y + 4.0),
                egui::pos2(x0 + 8.5, c.y + 6.0),
            ];
            p.add(egui::Shape::closed_line(left, stroke));
            p.add(egui::Shape::closed_line(right, stroke));
            p.line_segment(
                [
                    egui::pos2(c.x + 5.0, c.y + 4.0),
                    egui::pos2(c.x + 12.0, c.y - 7.0),
                ],
                Stroke::new(2.4, tint),
            );
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x + 2.5, c.y + 8.0),
                    egui::pos2(c.x + 4.0, c.y + 3.5),
                    egui::pos2(c.x + 6.5, c.y + 6.0),
                ],
                tint,
                Stroke::NONE,
            ));
        }
    }
}

/// An eye: open with a coloured outline when the word is shown on the page, struck through and
/// grey when it is not. Returns true when pressed.
pub fn eye_button(ui: &mut Ui, on: bool, grey: Color32, accent: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 18.0), egui::Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if on {
            "Hide this word on the page"
        } else {
            "Show this word on the page"
        });
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_black_alpha(22));
    }
    let c = rect.center();
    let tint = if on { accent.to_opaque() } else { grey };
    let mut outline = Vec::new();
    for i in 0..=16 {
        let x = -8.0 + i as f32;
        let y = 5.0 * (1.0 - (x / 8.0) * (x / 8.0));
        outline.push(egui::pos2(c.x + x, c.y - y));
    }
    for i in (0..=16).rev() {
        let x = -8.0 + i as f32;
        let y = 5.0 * (1.0 - (x / 8.0) * (x / 8.0));
        outline.push(egui::pos2(c.x + x, c.y + y));
    }
    let p = ui.painter();
    p.add(egui::Shape::closed_line(outline, Stroke::new(1.6, tint)));
    p.circle_filled(c, 2.4, tint);
    if !on {
        p.line_segment(
            [
                egui::pos2(c.x - 7.0, c.y + 6.0),
                egui::pos2(c.x + 7.0, c.y - 6.0),
            ],
            Stroke::new(1.6, tint),
        );
    }
    response.clicked()
}

/// An ON/OFF switch: grey and "OFF" when off, green and "ON" when on. Returns true when pressed.
pub fn toggle_switch(ui: &mut Ui, on: &mut bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(50.0, 20.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let mut pressed = false;
    if response.clicked() {
        *on = !*on;
        pressed = true;
    }
    let fill = if *on {
        Color32::from_rgb(80, 180, 105)
    } else {
        Color32::from_gray(150)
    };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(10), fill);
    let knob_x = if *on {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    p.circle_filled(egui::pos2(knob_x, rect.center().y), 8.0, Color32::WHITE);
    let (label, label_x) = if *on {
        ("ON", rect.left() + 16.0)
    } else {
        ("OFF", rect.right() - 17.0)
    };
    p.text(
        egui::pos2(label_x, rect.center().y),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.5),
        Color32::WHITE,
    );
    pressed
}

/// A left or right chevron for stepping through the issues.
pub fn side_arrow(ui: &mut Ui, left: bool, tint: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 20.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_black_alpha(22));
    }
    let c = rect.center();
    let (dx, dy) = (3.0, 5.5);
    let (top, mid, bottom) = if left {
        (
            egui::pos2(c.x + dx, c.y - dy),
            egui::pos2(c.x - dx, c.y),
            egui::pos2(c.x + dx, c.y + dy),
        )
    } else {
        (
            egui::pos2(c.x - dx, c.y - dy),
            egui::pos2(c.x + dx, c.y),
            egui::pos2(c.x - dx, c.y + dy),
        )
    };
    let stroke = Stroke::new(2.0, tint);
    ui.painter().line_segment([top, mid], stroke);
    ui.painter().line_segment([mid, bottom], stroke);
    response.clicked()
}

/// A plain chevron, up when the box is open (press to fold it) and down when it is folded.
pub fn arrow_button(ui: &mut Ui, up: bool, tint: Color32) -> bool {
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

/// The move handle: four arrows. Press and hold it to drag the box's window.
pub fn drag_handle(ui: &mut Ui, tint: Color32) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 20.0), egui::Sense::drag());
    let response = response
        .on_hover_cursor(egui::CursorIcon::Grab)
        .on_hover_text("Drag to move");
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), Color32::from_black_alpha(22));
    }
    if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let c = rect.center();
    let p = ui.painter();
    let stroke = Stroke::new(1.6, tint);
    let (len, head) = (6.5, 2.6);
    for (dx, dy) in [(1.0_f32, 0.0_f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let tip = egui::pos2(c.x + dx * len, c.y + dy * len);
        p.line_segment([c, tip], stroke);
        let (px, py) = (-dy, dx);
        for side in [-1.0_f32, 1.0] {
            let back = egui::pos2(
                tip.x - dx * head + px * head * side,
                tip.y - dy * head + py * head * side,
            );
            p.line_segment([tip, back], stroke);
        }
    }
}

/// The card frame: a rounded box with an icon, a bold title, an optional control left of the fold
/// arrow, and a body. Returns true when the arrow was pressed.
#[allow(clippy::too_many_arguments)]
pub fn card_ex(
    ui: &mut Ui,
    s: &Settings,
    width: f32,
    icon_kind: Option<Icon>,
    title: &str,
    collapsed: bool,
    header: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui),
) -> bool {
    let mut toggled = false;
    let text_color = color(s.box_text);
    let framed = Frame::new()
        .fill(color(s.box_fill))
        .corner_radius(CornerRadius::same(s.corner_radius))
        .stroke(Stroke::new(1.0, Color32::from_gray(200)))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(width - 18.0);
            ui.horizontal(|ui| {
                if let Some(kind) = icon_kind {
                    icon(ui, kind, text_color);
                }
                // The title is the handle: drag it to move the box's window.
                let handle = ui
                    .add(
                        egui::Label::new(
                            RichText::new(title)
                                .strong()
                                .size(s.box_font_size)
                                .color(text_color),
                        )
                        .sense(egui::Sense::click_and_drag()),
                    )
                    .on_hover_cursor(egui::CursorIcon::Grab);
                if handle.drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if arrow_button(ui, !collapsed, text_color) {
                        toggled = true;
                    }
                    drag_handle(ui, text_color);
                    header(ui);
                });
            });
            if !collapsed {
                body(ui);
            }
        });
    note_box_rect(ui.ctx(), framed.response.rect);
    toggled
}

/// Ask for this window to show only this part of itself (a pet with no box shows just the pet).
pub fn note_clip_rect(ctx: &egui::Context, rect: egui::Rect) {
    let key = egui::Id::new(("clip_rect", ctx.viewport_id()));
    ctx.data_mut(|d| d.insert_temp(key, Some(rect)));
}

/// Ask for this window to show only these parts of itself (the opaque pixels of a pet picture).
pub fn note_clip_rects(ctx: &egui::Context, rects: Vec<egui::Rect>) {
    let key = egui::Id::new(("clip_rects", ctx.viewport_id()));
    ctx.data_mut(|d| d.insert_temp(key, rects));
}

/// Remember where a box's frame ended this frame, so its window can be cut to exactly that.
pub fn note_box_rect(ctx: &egui::Context, rect: egui::Rect) {
    let key = egui::Id::new(("box_rect", ctx.viewport_id()));
    ctx.data_mut(|d| d.insert_temp(key, Some(rect)));
}

pub fn card(
    ui: &mut Ui,
    s: &Settings,
    icon_kind: Option<Icon>,
    title: &str,
    collapsed: bool,
    body: impl FnOnce(&mut Ui),
) -> bool {
    card_ex(ui, s, BOX_W, icon_kind, title, collapsed, |_| {}, body)
}

/// "1 of 5" with arrows, centred. Arrows only where they lead somewhere; a blank keeps the
/// layout from jumping. Returns -1 or 1 when an arrow was pressed.
pub fn stepper(ui: &mut Ui, text_color: Color32, current: usize, total: usize) -> isize {
    let mut step = 0;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let label = format!("{} of {}", current + 1, total);
        let galley = ui.painter().layout_no_wrap(
            label.clone(),
            egui::FontId::proportional(11.0),
            text_color,
        );
        let gap = ui.spacing().item_spacing.x;
        let width = 22.0 + gap + galley.size().x + gap + 22.0;
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
        if current > 0 {
            if side_arrow(ui, true, text_color) {
                step = -1;
            }
        } else {
            ui.allocate_space(egui::vec2(22.0, 20.0));
        }
        ui.label(RichText::new(label).size(11.0).color(text_color));
        if current + 1 < total {
            if side_arrow(ui, false, text_color) {
                step = 1;
            }
        } else {
            ui.allocate_space(egui::vec2(22.0, 20.0));
        }
    });
    step
}

/// A grip in a box's corner: drag to change its width (left to right) and the height of what it
/// shows (up and down); double-click to go back to the default.
pub fn resize_grip(ui: &mut Ui, size: &mut [f32; 2], row_width: f32, default: [f32; 2]) {
    ui.allocate_ui_with_layout(
        egui::vec2(row_width, 18.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click_and_drag());
            let response = response
                .on_hover_cursor(egui::CursorIcon::ResizeNwSe)
                .on_hover_text("Drag to resize. Double-click to reset.");
            if response.dragged() {
                let d = response.drag_delta();
                size[0] = (size[0] + d.x).clamp(260.0, 900.0);
                size[1] = (size[1] + d.y).clamp(40.0, 600.0);
            }
            if response.double_clicked() {
                *size = default;
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
}

/// A box that is a window of its own: it can be dragged anywhere on the screen, sits above other
/// windows and has no title bar or taskbar button. The window is as wide as `width` and as tall
/// as its content. Returns where the window is, so the position can be remembered. `start` is
/// where it first appears.
pub fn floating_box(
    ctx: &egui::Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    title: &str,
    width: f32,
    start: [f32; 2],
    content: &mut dyn FnMut(&mut Ui),
) -> Option<egui::Pos2> {
    let viewport_id = egui::ViewportId::from_hash_of(id);
    // The window's first size and place are fixed when it is made. egui compares the builder
    // every frame and moves or resizes the window to match any change, which fights the person
    // dragging it (it shakes). After the first frame only our own height-fitting resizes it.
    let first_key = egui::Id::new(("floating_box_first", viewport_id));
    let [first_width, first_x, first_y] = ctx.data_mut(|d| {
        *d.get_temp_mut_or_insert_with::<[f32; 3]>(first_key, || [width, start[0], start[1]])
    });
    // The title is how the window is found again to be shaped, so it must be unique.
    let window_title = format!("{title} \u{b7} {viewport_id:?}");
    let builder = egui::ViewportBuilder::default()
        .with_title(window_title.clone())
        .with_inner_size([first_width, 140.0])
        .with_position([first_x, first_y])
        .with_decorations(false)
        .with_has_shadow(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_window_level(window_level(ctx))
        .with_taskbar(false);
    let mut position = None;
    ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
        let mut height = 0.0;
        egui::CentralPanel::default()
            .frame(Frame::new())
            .show(ui, |ui| {
                content(ui);
                height = ui.min_rect().height();
            });
        // The frame the box drew is what the window must fit (not the empty space around it).
        let vp = ui.ctx().viewport_id();
        let noted = ui
            .ctx()
            .data_mut(|d| d.remove_temp::<Option<egui::Rect>>(egui::Id::new(("box_rect", vp))))
            .flatten();
        let clip = ui
            .ctx()
            .data_mut(|d| d.remove_temp::<Option<egui::Rect>>(egui::Id::new(("clip_rect", vp))))
            .flatten();
        let clip_rects = ui
            .ctx()
            .data_mut(|d| d.remove_temp::<Vec<egui::Rect>>(egui::Id::new(("clip_rects", vp))))
            .unwrap_or_default();
        if let Some(r) = noted {
            height = r.max.y.max(r.height());
        }
        // Cut the window to the box (or to what a pet asks to show), so no empty window
        // shows around it and no shadow can be drawn.
        let shape = clip.or(noted).unwrap_or_else(|| {
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))
        });
        let ppp = ui.ctx().pixels_per_point();
        let radius = if clip.is_some() { 0.0 } else { 8.0 };
        if !clip_rects.is_empty() {
            let rects = clip_rects
                .iter()
                .map(|r| {
                    [
                        (r.min.x * ppp).floor() as i32,
                        (r.min.y * ppp).floor() as i32,
                        (r.max.x * ppp).ceil() as i32,
                        (r.max.y * ppp).ceil() as i32,
                    ]
                })
                .collect();
            crate::win_shadow::set_rects(&window_title, rects);
        } else {
            crate::win_shadow::set_shape(
            &window_title,
            [
                (shape.min.x * ppp).floor() as i32,
                (shape.min.y * ppp).floor() as i32,
                (shape.max.x * ppp).ceil() as i32,
                (shape.max.y * ppp).ceil() as i32,
            ],
            (radius * ppp) as i32,
        );
        }
        // Fit the window to what the box holds. Compare with the window's real size (not just
        // the last request) so a window that ended up taller than its box is pulled back in.
        let key = egui::Id::new(("floating_box_size", viewport_id));
        let now = ui.ctx().input(|i| i.time);
        let last = ui
            .ctx()
            .data(|d| d.get_temp::<[f32; 3]>(key))
            .unwrap_or([0.0, 0.0, -1.0]);
        let want = height.ceil();
        let actual = ui
            .ctx()
            .input(|i| i.viewport().inner_rect)
            .map(|r| r.size());
        let off = actual.is_none_or(|a| (a.y - want).abs() > 1.5 || (a.x - width).abs() > 1.5);
        if height > 1.0 && off && now - last[2] as f64 > 0.2 {
            ui.ctx()
                .data_mut(|d| d.insert_temp(key, [width, want, now as f32]));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, want)));
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
        position = ui.ctx().input(|i| i.viewport().outer_rect).map(|r| r.min);
    });
    position
}

/// A window of its own for the guide, the dictionary and similar: no title bar. It is a rounded
/// card with a small header (title, move handle, close button) and a grip in the corner to
/// resize it. Returns true once the person has closed it.
pub fn tool_window(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: [f32; 2],
    content: &mut dyn FnMut(&mut Ui),
) -> bool {
    let viewport_id = egui::ViewportId::from_hash_of(id);
    let window_title = format!("{title} \u{b7} {viewport_id:?}");
    // As with the floating boxes, the builder must not change after the window is made.
    let first_key = egui::Id::new(("tool_window_first", viewport_id));
    let first = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with::<[f32; 2]>(first_key, || size));
    let builder = egui::ViewportBuilder::default()
        .with_title(window_title.clone())
        .with_inner_size(first)
        .with_decorations(false)
        .with_has_shadow(false)
        .with_transparent(true)
        .with_window_level(window_level(ctx))
        .with_resizable(false);
    let mut closed = false;
    ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
        let ink = Color32::from_rgb(40, 40, 48);
        let window = ui.ctx().input(|i| i.viewport().inner_rect).map(|r| r.size());
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(Color32::from_rgb(248, 248, 250))
                    .corner_radius(CornerRadius::same(10))
                    .stroke(Stroke::new(1.0, Color32::from_gray(200)))
                    .inner_margin(Margin::same(12)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let handle = ui
                        .add(
                            egui::Label::new(RichText::new(title).strong().size(15.0).color(ink))
                                .sense(egui::Sense::click_and_drag()),
                        )
                        .on_hover_cursor(egui::CursorIcon::Grab);
                    if handle.drag_started() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(24.0, 22.0), egui::Sense::click());
                        let response = response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text("Close");
                        if response.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(5),
                                Color32::from_black_alpha(22),
                            );
                        }
                        let c = rect.center();
                        let stroke = Stroke::new(1.8, ink);
                        for (a, b) in [((-4.0, -4.0), (4.0, 4.0)), ((-4.0, 4.0), (4.0, -4.0))] {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(c.x + a.0, c.y + a.1),
                                    egui::pos2(c.x + b.0, c.y + b.1),
                                ],
                                stroke,
                            );
                        }
                        if response.clicked() {
                            closed = true;
                        }
                        drag_handle(ui, ink);
                    });
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| content(ui));
            });
        // A grip in the bottom right corner: drag to resize.
        if let Some(size) = window {
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let grip = egui::Rect::from_min_size(
                screen.right_bottom() - egui::vec2(20.0, 20.0),
                egui::vec2(18.0, 18.0),
            );
            let response = ui
                .interact(grip, egui::Id::new("tool_window_grip"), egui::Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
            let p = ui.painter();
            for k in [5.0_f32, 9.0, 13.0] {
                p.line_segment(
                    [
                        egui::pos2(grip.right() - 2.0, grip.bottom() - 2.0 - k),
                        egui::pos2(grip.right() - 2.0 - k, grip.bottom() - 2.0),
                    ],
                    Stroke::new(1.2, Color32::from_gray(150)),
                );
            }
            if response.dragged() {
                let d = response.drag_delta();
                let new = egui::vec2((size.x + d.x).max(320.0), (size.y + d.y).max(240.0));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(new));
            }
            let ppp = ui.ctx().pixels_per_point();
            crate::win_shadow::set_shape(
                &window_title,
                [0, 0, (size.x * ppp).ceil() as i32, (size.y * ppp).ceil() as i32],
                (10.0 * ppp) as i32,
            );
        }
    });
    closed
}


// ---------------------------------------------------------------------------------------------
// Always on top, and the bunny placeholder
// ---------------------------------------------------------------------------------------------

fn on_top_key() -> egui::Id {
    egui::Id::new("boxes_on_top")
}

/// The app tells every window whether the boxes stay above other programs.
pub fn set_boxes_on_top(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(on_top_key(), on));
}

/// The level a box window should have right now. egui compares the builder every frame, so a
/// change of this setting moves every open box up or down without reopening it.
pub fn window_level(ctx: &egui::Context) -> egui::WindowLevel {
    if ctx.data(|d| d.get_temp::<bool>(on_top_key())).unwrap_or(true) {
        egui::WindowLevel::AlwaysOnTop
    } else {
        egui::WindowLevel::Normal
    }
}

struct Placeholder {
    uri: &'static str,
    bytes: &'static [u8],
    /// Width divided by height.
    aspect: f32,
}

/// The pictures a box can show when it has nothing to show. Each box starts on a different one
/// (picked at random when the program starts); clicking a picture moves on to the next.
const PLACEHOLDERS: [Placeholder; 4] = [
    Placeholder {
        uri: "bytes://placeholder_usagigi.svg",
        bytes: include_bytes!("../assets/usagigi.svg"),
        aspect: 298.0 / 393.0,
    },
    Placeholder {
        uri: "bytes://placeholder_typing.svg",
        bytes: include_bytes!("../assets/placeholder_typing.svg"),
        aspect: 388.0 / 390.0,
    },
    Placeholder {
        uri: "bytes://placeholder_loaf.svg",
        bytes: include_bytes!("../assets/placeholder_loaf.svg"),
        aspect: 889.0 / 668.0,
    },
    Placeholder {
        uri: "bytes://placeholder_67.svg",
        bytes: include_bytes!("../assets/placeholder_67.svg"),
        aspect: 154.0 / 153.0,
    },
];

fn seed_key() -> egui::Id {
    egui::Id::new("placeholder_seed")
}

/// A different starting picture each run: the clock decides.
pub fn set_random_placeholder_seed(ctx: &egui::Context) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as usize ^ d.as_secs() as usize);
    ctx.data_mut(|d| d.insert_temp(seed_key(), nanos % PLACEHOLDERS.len()));
}

/// A picture, centred, `height` points tall. `slot` is the box it is in, so neighbouring boxes
/// show different pictures.
pub fn placeholder(ui: &mut Ui, slot: usize, height: f32) {
    let seed = ui.ctx().data(|d| d.get_temp::<usize>(seed_key())).unwrap_or(0);
    let Some(p) = PLACEHOLDERS.get((seed + slot) % PLACEHOLDERS.len()) else {
        return;
    };
    let size = egui::vec2(height * p.aspect, height);
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        let response = ui
            .add(
                egui::Image::from_bytes(p.uri, p.bytes)
                    .fit_to_exact_size(size)
                    .sense(egui::Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        clicked = response.clicked();
    });
    if clicked {
        ui.ctx()
            .data_mut(|d| d.insert_temp(seed_key(), seed.wrapping_add(1)));
    }
}

/// What a box shows when there is nothing to show yet: a picture and a short note.
pub fn empty_state(ui: &mut Ui, slot: usize, ink: Color32, size: f32, note: &str) {
    ui.add_space(4.0);
    placeholder(ui, slot, 96.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(note).size(size).color(ink));
    });
    ui.add_space(2.0);
}

const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// The program's icon as RGBA pixels, `size` pixels square, or None if it can't be read.
pub fn icon_pixels(size: u32) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(ICON_PNG).ok()?.to_rgba8();
    let small = image::imageops::resize(&img, size, size, image::imageops::FilterType::Lanczos3);
    Some((small.into_raw(), size, size))
}
