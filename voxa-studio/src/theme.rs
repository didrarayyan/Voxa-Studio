use crate::state::ThemeMode;
use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Response, Sense, Shadow, Stroke, StrokeKind, Vec2,
};

#[derive(Debug, Clone, Copy)]
pub struct StudioPalette {
    pub is_dark: bool,
    pub bg: Color32,
    pub panel_bg: Color32,
    pub card_bg: Color32,
    pub card_alt: Color32,
    pub input_bg: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub ink: Color32,
    pub ink_muted: Color32,
    pub primary: Color32,
    pub primary_hover: Color32,
    pub on_primary: Color32,
    pub accent_blue: Color32,
    pub accent_blue_soft: Color32,
    pub success: Color32,
    pub success_soft: Color32,
    pub warning: Color32,
    pub warning_soft: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
    pub emotion_chip: Color32,
    pub event_chip: Color32,
    pub emphasis_chip: Color32,
    pub wave_unplayed: Color32,
}

impl StudioPalette {
    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self {
                is_dark: true,
                bg: Color32::from_rgb(14, 16, 21),               // #0E1015 deep obsidian canvas
                panel_bg: Color32::from_rgb(21, 24, 33),         // #151821 docked sidebars/header
                card_bg: Color32::from_rgb(28, 32, 43),          // #1C202B elevated studio cards
                card_alt: Color32::from_rgb(35, 40, 54),         // #232836 interactive surface
                input_bg: Color32::from_rgb(17, 19, 26),         // #11131A recessed editor
                border: Color32::from_rgb(38, 44, 58),           // #262C3A 1px structural hairline
                border_strong: Color32::from_rgb(54, 63, 84),    // #363F54 crisp field border
                ink: Color32::from_rgb(241, 245, 249),           // #F1F5F9
                ink_muted: Color32::from_rgb(148, 163, 184),     // #94A3B8
                primary: Color32::from_rgb(0, 212, 229),         // #00D4E5 CapCut electric cyan
                primary_hover: Color32::from_rgb(0, 184, 200),   // #00B8C8
                on_primary: Color32::from_rgb(7, 19, 24),        // #071318 >10:1 contrast on cyan
                accent_blue: Color32::from_rgb(56, 189, 248),    // #38BDF8
                accent_blue_soft: Color32::from_rgb(17, 40, 59), // #11283B
                success: Color32::from_rgb(52, 211, 153),        // #34D399
                success_soft: Color32::from_rgb(12, 54, 43),
                warning: Color32::from_rgb(251, 191, 36),
                warning_soft: Color32::from_rgb(66, 42, 14),
                danger: Color32::from_rgb(248, 113, 113),
                danger_soft: Color32::from_rgb(69, 24, 28),
                emotion_chip: Color32::from_rgb(167, 139, 250),
                event_chip: Color32::from_rgb(56, 189, 248),
                emphasis_chip: Color32::from_rgb(251, 191, 36),
                wave_unplayed: Color32::from_rgb(68, 79, 102),
            },
            ThemeMode::Light => Self {
                is_dark: false,
                bg: Color32::from_rgb(241, 245, 249),            // #F1F5F9 cool studio slate
                panel_bg: Color32::WHITE,                        // #FFFFFF
                card_bg: Color32::WHITE,                         // #FFFFFF
                card_alt: Color32::from_rgb(248, 250, 252),      // #F8FAFC
                input_bg: Color32::from_rgb(248, 250, 252),      // #F8FAFC
                border: Color32::from_rgb(226, 232, 240),        // #E2E8F0
                border_strong: Color32::from_rgb(203, 213, 225), // #CBD5E1
                ink: Color32::from_rgb(15, 23, 42),              // #0F172A
                ink_muted: Color32::from_rgb(71, 85, 105),       // #475569
                primary: Color32::from_rgb(8, 145, 178),         // #0891B2 deep cyan-teal
                primary_hover: Color32::from_rgb(14, 116, 144),  // #0E7490
                on_primary: Color32::WHITE,                      // #FFFFFF
                accent_blue: Color32::from_rgb(2, 132, 199),     // #0284C7
                accent_blue_soft: Color32::from_rgb(224, 242, 254), // #E0F2FE
                success: Color32::from_rgb(5, 150, 105),
                success_soft: Color32::from_rgb(236, 253, 245),
                warning: Color32::from_rgb(180, 83, 9),
                warning_soft: Color32::from_rgb(254, 243, 199),
                danger: Color32::from_rgb(220, 38, 38),
                danger_soft: Color32::from_rgb(254, 242, 242),
                emotion_chip: Color32::from_rgb(124, 58, 237),
                event_chip: Color32::from_rgb(2, 132, 199),
                emphasis_chip: Color32::from_rgb(217, 119, 6),
                wave_unplayed: Color32::from_rgb(203, 213, 225),
            },
        }
    }
}

pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode) -> StudioPalette {
    let pal = StudioPalette::for_mode(mode);
    let mut style = (*ctx.style()).clone();

    style.visuals = match mode {
        ThemeMode::Light => egui::Visuals::light(),
        ThemeMode::Dark => egui::Visuals::dark(),
    };

    style.visuals.window_fill = pal.bg;
    style.visuals.panel_fill = pal.bg;
    style.visuals.extreme_bg_color = pal.input_bg;
    style.visuals.faint_bg_color = pal.card_alt;
    style.visuals.slider_trailing_fill = true;

    let control_bg = if pal.is_dark {
        Color32::from_rgb(44, 51, 68) // #2C3344 clearly visible against card_bg/card_alt
    } else {
        Color32::from_rgb(226, 232, 240) // #E2E8F0 clearly visible slider rail & chip bg
    };
    let control_hover = if pal.is_dark {
        Color32::from_rgb(56, 65, 86) // #384156
    } else {
        Color32::from_rgb(203, 213, 225) // #CBD5E1
    };

    style.visuals.widgets.noninteractive.bg_fill = pal.card_bg;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.ink);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.border);
    style.visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);

    style.visuals.widgets.inactive.bg_fill = control_bg;
    style.visuals.widgets.inactive.weak_bg_fill = control_bg;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.2_f32, pal.ink);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, pal.border_strong);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(8);

    style.visuals.widgets.hovered.bg_fill = control_hover;
    style.visuals.widgets.hovered.weak_bg_fill = control_hover;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.4_f32, pal.ink);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, pal.primary);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(8);

    // Note: egui::Visuals::strong_text_color() reads widgets.active.fg_stroke.color,
    // so keep active.fg_stroke as high-contrast pal.ink (primary buttons set pal.on_primary explicitly).
    style.visuals.widgets.active.bg_fill = pal.primary;
    style.visuals.widgets.active.weak_bg_fill = pal.primary;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.4_f32, pal.ink);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.4_f32, pal.primary_hover);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(8);

    style.visuals.selection.bg_fill =
        pal.primary.gamma_multiply(if pal.is_dark { 0.42 } else { 0.30 });
    style.visuals.selection.stroke = Stroke::new(1.4_f32, pal.primary);

    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(11.0, 6.0);
    style.spacing.slider_width = 140.0;
    style.spacing.scroll.bar_width = 7.0;

    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::proportional(18.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(14.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(13.5),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::proportional(12.0),
    );

    ctx.set_style(style);
    pal
}

/// Sleek studio card container that strictly constrains inner width and stacks children top-down.
pub fn styled_card(ui: &mut egui::Ui, pal: &StudioPalette, add_contents: impl FnOnce(&mut egui::Ui)) {
    let avail_w = ui.available_width();
    let shadow = if pal.is_dark {
        Shadow::NONE
    } else {
        Shadow {
            offset: [0, 1],
            blur: 6,
            spread: 0,
            color: Color32::from_black_alpha(10),
        }
    };
    egui::Frame::new()
        .fill(pal.card_bg)
        .stroke(Stroke::new(1.0_f32, pal.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(14))
        .shadow(shadow)
        .show(ui, |ui| {
            let inner_w = (avail_w - 30.0).max(120.0);
            ui.set_min_width(inner_w);
            ui.set_max_width(inner_w);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add_contents);
        });
}

/// Safe left-vs-right header bar using `egui::Sides` so neither side expands vertically.
pub fn card_header_row(
    ui: &mut egui::Ui,
    left: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui),
) {
    egui::Sides::new().show(ui, left, right);
}

pub fn pill_badge(ui: &mut egui::Ui, text: &str, bg: Color32, fg: Color32) {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.0).strong().color(fg));
        });
}

// ── Custom Vector Icons ──────────────────────────────────────────────────────────────────

pub fn draw_star_icon(painter: &egui::Painter, rect: Rect, filled: bool, color: Color32) {
    let c = rect.center();
    let r_outer = rect.width().min(rect.height()) * 0.45;
    let r_inner = r_outer * 0.44;
    let mut pts = Vec::with_capacity(10);
    for i in 0..10 {
        let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * (std::f32::consts::PI / 5.0);
        let r = if i % 2 == 0 { r_outer } else { r_inner };
        pts.push(Pos2::new(c.x + r * angle.cos(), c.y + r * angle.sin()));
    }
    if filled {
        for i in 0..10 {
            let p1 = pts[i];
            let p2 = pts[(i + 1) % 10];
            painter.add(egui::Shape::convex_polygon(
                vec![c, p1, p2],
                color,
                Stroke::NONE,
            ));
        }
    } else {
        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5_f32, color)));
    }
}

pub fn draw_play_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.36;
    let pts = vec![
        Pos2::new(c.x - r * 0.6, c.y - r),
        Pos2::new(c.x + r * 0.95, c.y),
        Pos2::new(c.x - r * 0.6, c.y + r),
    ];
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

pub fn draw_pause_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.36;
    let bar_w = (r * 0.52).max(2.5);
    let gap = r * 0.34;
    let left_rect = Rect::from_min_max(
        Pos2::new(c.x - gap - bar_w, c.y - r),
        Pos2::new(c.x - gap, c.y + r),
    );
    let right_rect = Rect::from_min_max(
        Pos2::new(c.x + gap, c.y - r),
        Pos2::new(c.x + gap + bar_w, c.y + r),
    );
    painter.rect_filled(left_rect, CornerRadius::same(2), color);
    painter.rect_filled(right_rect, CornerRadius::same(2), color);
}

pub fn draw_stop_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.30;
    let sq = Rect::from_center_size(c, Vec2::splat(r * 2.0));
    painter.rect_filled(sq, CornerRadius::same(2), color);
}

pub fn draw_replay_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.36;
    let mut arc = Vec::with_capacity(18);
    for i in 0..=16 {
        let t = i as f32 / 16.0;
        let angle = -std::f32::consts::FRAC_PI_4 + t * std::f32::consts::PI * 1.55;
        arc.push(Pos2::new(c.x + r * angle.cos(), c.y + r * angle.sin()));
    }
    painter.add(egui::Shape::line(arc, Stroke::new(1.7_f32, color)));
    let tip = Pos2::new(c.x - r * 0.72, c.y - r * 0.65);
    let pts = vec![
        tip,
        Pos2::new(tip.x + r * 0.55, tip.y - r * 0.05),
        Pos2::new(tip.x + r * 0.05, tip.y + r * 0.55),
    ];
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

pub fn draw_sparkle_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.44;
    let inner = r * 0.28;
    let pts = vec![
        Pos2::new(c.x, c.y - r),
        Pos2::new(c.x + inner, c.y - inner),
        Pos2::new(c.x + r, c.y),
        Pos2::new(c.x + inner, c.y + inner),
        Pos2::new(c.x, c.y + r),
        Pos2::new(c.x - inner, c.y + inner),
        Pos2::new(c.x - r, c.y),
        Pos2::new(c.x - inner, c.y - inner),
    ];
    for i in 0..8 {
        painter.add(egui::Shape::convex_polygon(
            vec![c, pts[i], pts[(i + 1) % 8]],
            color,
            Stroke::NONE,
        ));
    }
}

pub fn draw_search_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.28;
    let lens_c = Pos2::new(c.x - r * 0.25, c.y - r * 0.25);
    painter.circle_stroke(lens_c, r, Stroke::new(1.6_f32, color));
    let handle_start = Pos2::new(lens_c.x + r * 0.7, lens_c.y + r * 0.7);
    let handle_end = Pos2::new(c.x + r * 1.15, c.y + r * 1.15);
    painter.line_segment([handle_start, handle_end], Stroke::new(1.8_f32, color));
}

pub fn draw_theme_icon(painter: &egui::Painter, rect: Rect, is_dark: bool, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.32;
    if is_dark {
        painter.circle_filled(c, r * 0.62, color);
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            let p1 = Pos2::new(c.x + r * 0.90 * a.cos(), c.y + r * 0.90 * a.sin());
            let p2 = Pos2::new(c.x + r * 1.30 * a.cos(), c.y + r * 1.30 * a.sin());
            painter.line_segment([p1, p2], Stroke::new(1.5_f32, color));
        }
    } else {
        let mut outer = Vec::with_capacity(14);
        for i in 0..=12 {
            let a = -std::f32::consts::FRAC_PI_3 + (i as f32 / 12.0) * std::f32::consts::PI * 1.35;
            outer.push(Pos2::new(c.x + r * a.cos(), c.y + r * a.sin()));
        }
        painter.add(egui::Shape::line(outer, Stroke::new(2.0_f32, color)));
        painter.circle_stroke(c, r * 0.95, Stroke::new(1.2_f32, color.gamma_multiply(0.35)));
    }
}

pub fn draw_close_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.28;
    let s = Stroke::new(1.7_f32, color);
    painter.line_segment([Pos2::new(c.x - r, c.y - r), Pos2::new(c.x + r, c.y + r)], s);
    painter.line_segment([Pos2::new(c.x + r, c.y - r), Pos2::new(c.x - r, c.y + r)], s);
}

pub fn draw_mic_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.35;
    let body = Rect::from_center_size(Pos2::new(c.x, c.y - r * 0.25), Vec2::new(r * 0.9, r * 1.45));
    painter.rect_filled(body, CornerRadius::same(6), color);
    let s = Stroke::new(1.6_f32, color);
    painter.line_segment(
        [Pos2::new(c.x - r * 0.85, c.y + r * 0.35), Pos2::new(c.x + r * 0.85, c.y + r * 0.35)],
        s,
    );
    painter.line_segment([Pos2::new(c.x, c.y + r * 0.35), Pos2::new(c.x, c.y + r * 0.95)], s);
    painter.line_segment(
        [Pos2::new(c.x - r * 0.45, c.y + r * 0.95), Pos2::new(c.x + r * 0.45, c.y + r * 0.95)],
        s,
    );
}

pub fn draw_voice_avatar(ui: &mut egui::Ui, pal: &StudioPalette, name: &str, selected: bool) {
    let size = 30.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter_at(rect);
    let initial = name
        .chars()
        .next()
        .unwrap_or('K')
        .to_ascii_uppercase()
        .to_string();
    let bg = if selected {
        pal.primary
    } else {
        pal.accent_blue_soft
    };
    let fg = if selected {
        pal.on_primary
    } else {
        pal.accent_blue
    };
    p.rect_filled(rect, CornerRadius::same(8), bg);
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::proportional(13.5),
        fg,
    );
}

pub fn vector_icon_button(
    ui: &mut egui::Ui,
    size: Vec2,
    fill: Color32,
    stroke: Stroke,
    rounding: u8,
    enabled: bool,
    draw_icon: impl FnOnce(&egui::Painter, Rect, Color32),
    icon_color: Color32,
) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let p = ui.painter_at(rect);
        let bg = if !enabled {
            fill.gamma_multiply(0.4)
        } else if resp.is_pointer_button_down_on() {
            fill.gamma_multiply(0.82)
        } else if resp.hovered() {
            fill.gamma_multiply(0.92)
        } else {
            fill
        };
        let fg = if enabled {
            icon_color
        } else {
            icon_color.gamma_multiply(0.4)
        };
        p.rect_filled(rect, CornerRadius::same(rounding), bg);
        if stroke.width > 0.0 {
            p.rect_stroke(rect, CornerRadius::same(rounding), stroke, StrokeKind::Inside);
        }
        draw_icon(&p, rect.shrink(6.0), fg);
    }
    resp
}

/// Precision geometric Voxa Studio acoustic V-crest monogram.
pub fn draw_voxa_logo(ui: &mut egui::Ui, pal: &StudioPalette, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter_at(rect);
    let u = size / 512.0;
    let pt = |x: f32, y: f32| Pos2::new(rect.left() + x * u, rect.top() + y * u);

    let bg_fill = Color32::from_rgb(13, 16, 23);
    p.rect_filled(rect, CornerRadius::same(10), bg_fill);
    p.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.3_f32, pal.primary.gamma_multiply(0.65)),
        StrokeKind::Inside,
    );

    // Symmetrical left & right geometric V wings (convex quads for clean fill)
    let left_wing = vec![
        pt(96.0, 132.0),
        pt(166.0, 132.0),
        pt(274.0, 412.0),
        pt(238.0, 412.0),
    ];
    let right_wing = vec![
        pt(346.0, 132.0),
        pt(416.0, 132.0),
        pt(274.0, 412.0),
        pt(238.0, 412.0),
    ];
    p.add(egui::Shape::convex_polygon(left_wing, pal.primary, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(right_wing, pal.primary, Stroke::NONE));

    // Three centered acoustic equalizer bars inside the V crest
    let left_bar = Rect::from_min_max(pt(202.0, 114.0), pt(224.0, 186.0));
    let mid_bar = Rect::from_min_max(pt(244.0, 92.0), pt(268.0, 232.0));
    let right_bar = Rect::from_min_max(pt(288.0, 114.0), pt(310.0, 186.0));
    let bar_r = CornerRadius::same(((11.0 * u).round() as u8).max(2));
    p.rect_filled(left_bar, bar_r, pal.accent_blue);
    p.rect_filled(mid_bar, bar_r, pal.primary);
    p.rect_filled(right_bar, bar_r, pal.accent_blue);
}

/// Studio timeline audio track with top ruler ticks, clip container, waveform, and playhead cap.
pub fn waveform_widget(
    ui: &mut egui::Ui,
    pal: &StudioPalette,
    peaks: &[f32],
    progress: f32,
    duration_secs: f32,
    height: f32,
) -> Option<f32> {
    let desired = Vec2::new(ui.available_width().max(120.0), height);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
    let p = ui.painter_at(rect);

    p.rect_filled(rect, CornerRadius::same(8), pal.input_bg);
    p.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0_f32, pal.border),
        StrokeKind::Inside,
    );

    // Top time ruler strip (14px tall)
    let ruler_h = 14.0;
    let ruler_bottom = rect.top() + ruler_h;
    p.line_segment(
        [Pos2::new(rect.left(), ruler_bottom), Pos2::new(rect.right(), ruler_bottom)],
        Stroke::new(1.0_f32, pal.border),
    );

    let tick_count = 16usize;
    let track_w = (rect.width() - 16.0).max(40.0);
    for t in 0..=tick_count {
        let frac = t as f32 / tick_count as f32;
        let x = rect.left() + 8.0 + frac * track_w;
        let major = t % 4 == 0;
        let tick_len = if major { 6.0 } else { 3.0 };
        p.line_segment(
            [Pos2::new(x, ruler_bottom - tick_len), Pos2::new(x, ruler_bottom)],
            Stroke::new(1.0_f32, pal.ink_muted.gamma_multiply(if major { 0.55 } else { 0.3 })),
        );
        if major && duration_secs > 0.0 && t < tick_count {
            let secs = frac * duration_secs;
            p.text(
                Pos2::new(x + 3.0, rect.top() + 2.0),
                egui::Align2::LEFT_TOP,
                format!("{secs:.1}s"),
                egui::FontId::monospace(9.5),
                pal.ink_muted,
            );
        }
    }

    let clip_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 6.0, ruler_bottom + 3.0),
        Pos2::new(rect.right() - 6.0, rect.bottom() - 4.0),
    );

    if peaks.is_empty() {
        p.line_segment(
            [
                Pos2::new(clip_rect.left() + 8.0, clip_rect.center().y),
                Pos2::new(clip_rect.right() - 8.0, clip_rect.center().y),
            ],
            Stroke::new(1.0_f32, pal.border_strong),
        );
        p.text(
            clip_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Audio track ready — click Generate Speech to synthesize",
            egui::FontId::proportional(12.0),
            pal.ink_muted,
        );
        return None;
    }

    // Subtle audio clip pill container inside timeline
    p.rect_filled(
        clip_rect,
        CornerRadius::same(6),
        pal.accent_blue_soft.gamma_multiply(0.55),
    );
    p.rect_stroke(
        clip_rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, pal.primary.gamma_multiply(0.35)),
        StrokeKind::Inside,
    );

    let inner = clip_rect.shrink2(Vec2::new(6.0, 3.0));
    let n = peaks.len();
    let slot_w = inner.width() / n as f32;
    let bar_w = (slot_w * 0.62).clamp(1.5, 4.5);
    let clamped_prog = progress.clamp(0.0, 1.0);
    let mid_y = inner.center().y;
    let max_half = inner.height() * 0.45;

    for (idx, &peak) in peaks.iter().enumerate() {
        let frac = (idx as f32 + 0.5) / n as f32;
        let x = inner.left() + idx as f32 * slot_w + (slot_w - bar_w) * 0.5;
        let half_h = (peak.clamp(0.05, 1.0) * max_half).max(1.5);
        let bar_rect = Rect::from_min_max(
            Pos2::new(x, mid_y - half_h),
            Pos2::new(x + bar_w, mid_y + half_h),
        );
        let color = if frac <= clamped_prog {
            pal.primary
        } else {
            pal.wave_unplayed
        };
        p.rect_filled(bar_rect, CornerRadius::same(2), color);
    }

    // CapCut-style vertical playhead needle + top cap
    if duration_secs > 0.0 {
        let head_x = inner.left() + clamped_prog * inner.width();
        p.line_segment(
            [Pos2::new(head_x, rect.top() + 2.0), Pos2::new(head_x, rect.bottom() - 2.0)],
            Stroke::new(2.0_f32, pal.ink),
        );
        let cap = vec![
            Pos2::new(head_x - 5.0, rect.top() + 2.0),
            Pos2::new(head_x + 5.0, rect.top() + 2.0),
            Pos2::new(head_x + 5.0, rect.top() + 7.0),
            Pos2::new(head_x, rect.top() + 12.0),
            Pos2::new(head_x - 5.0, rect.top() + 7.0),
        ];
        p.add(egui::Shape::convex_polygon(cap, pal.primary, Stroke::NONE));
    }

    if let Some(hover_pos) = response.hover_pos() {
        if rect.contains(hover_pos) && duration_secs > 0.0 {
            let hover_frac = ((hover_pos.x - inner.left()) / inner.width()).clamp(0.0, 1.0);
            let hover_secs = hover_frac * duration_secs;
            p.line_segment(
                [Pos2::new(hover_pos.x, rect.top() + 2.0), Pos2::new(hover_pos.x, rect.bottom() - 2.0)],
                Stroke::new(1.0_f32, pal.ink_muted.gamma_multiply(0.5)),
            );
            response
                .clone()
                .on_hover_text(format!("Jump to {}", format_mmss(hover_secs)));
        }
    }

    if (response.clicked() || response.dragged()) && duration_secs > 0.0 {
        if let Some(pos) = response.interact_pointer_pos() {
            let frac = ((pos.x - inner.left()) / inner.width()).clamp(0.0, 1.0);
            return Some(frac);
        }
    }

    None
}

pub fn format_mmss(secs: f32) -> String {
    let total = secs.max(0.0).round() as u32;
    let mins = total / 60;
    let rem = total % 60;
    format!("{mins:02}:{rem:02}")
}
