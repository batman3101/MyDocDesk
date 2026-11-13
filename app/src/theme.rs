//! Colours, fonts and small drawing helpers, following the design tokens of the ALMUS
//! WorkBoard app on this PC: a grey ground, white bordered surfaces, a blue accent, small
//! corners (6 px cards, 4 px controls), Inter + Noto Sans KR, and light/dark themes.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Margin, RichText, Stroke};
use std::sync::{Arc, RwLock};

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub ground: Color32,
    pub surface: Color32,
    pub surface2: Color32,
    pub surface3: Color32,
    pub hover: Color32,
    pub line: Color32,
    pub line2: Color32,
    pub line_input: Color32,
    pub ink: Color32,
    pub ink2: Color32,
    pub ink3: Color32,
    pub accent: Color32,
    pub accent_h: Color32,
    pub accent_weak: Color32,
    pub accent_ink: Color32,
    pub ok: Color32,
    pub ok_weak: Color32,
    pub warn: Color32,
    pub warn_weak: Color32,
    pub danger: Color32,
    pub danger_weak: Color32,
    pub sel: Color32,
    /// The navy brand colour (used for AI items).
    pub brand: Color32,
    pub brand_weak: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const LIGHT: Palette = Palette {
    dark: false,
    ground: hex(0xe9edf2),
    surface: hex(0xffffff),
    surface2: hex(0xf5f7fa),
    surface3: hex(0xebeef3),
    hover: hex(0xf0f3f7),
    line: hex(0xe0e4ea),
    line2: hex(0xc9d0da),
    line_input: hex(0x858e9e),
    ink: hex(0x1c2230),
    ink2: hex(0x525b6c),
    ink3: hex(0x636b7b),
    accent: hex(0x1f5bd1),
    accent_h: hex(0x1649ae),
    accent_weak: hex(0xe6edfb),
    accent_ink: hex(0xffffff),
    ok: hex(0x147a4a),
    ok_weak: hex(0xe3f4eb),
    warn: hex(0x9c5b07),
    warn_weak: hex(0xfcf1e0),
    danger: hex(0xb42a37),
    danger_weak: hex(0xfbe8ea),
    sel: hex(0xdce7fc),
    brand: hex(0x1f3d7c),
    brand_weak: hex(0xe3e8f3),
};

pub const DARK: Palette = Palette {
    dark: true,
    ground: hex(0x0e1115),
    surface: hex(0x171b21),
    surface2: hex(0x1c2128),
    surface3: hex(0x242a33),
    hover: hex(0x20262e),
    line: hex(0x2a303a),
    line2: hex(0x3a424f),
    line_input: hex(0x66707f),
    ink: hex(0xe3e7ed),
    ink2: hex(0xa8b0be),
    ink3: hex(0x8a94a5),
    accent: hex(0x6d9bf2),
    accent_h: hex(0x8db2f6),
    accent_weak: hex(0x1b2842),
    accent_ink: hex(0x0d1016),
    ok: hex(0x35b97a),
    ok_weak: hex(0x16301f),
    warn: hex(0xe9a23e),
    warn_weak: hex(0x35270f),
    danger: hex(0xee6b7a),
    danger_weak: hex(0x3a1d22),
    sel: hex(0x1e2d4a),
    brand: hex(0x8db2f6),
    brand_weak: hex(0x1e2d4a),
};

static PAL: RwLock<Palette> = RwLock::new(LIGHT);

pub fn pal() -> Palette {
    *PAL.read().unwrap_or_else(|e| e.into_inner())
}

pub fn is_dark() -> bool {
    pal().dark
}

/// Switches between the light and dark palette and restyles every widget.
pub fn set_dark(ctx: &egui::Context, dark: bool) {
    if let Ok(mut p) = PAL.write() {
        *p = if dark { DARK } else { LIGHT };
    }
    apply(ctx);
}

// Short names used throughout the screens.
#[allow(non_snake_case)]
pub fn BG() -> Color32 { pal().ground }
#[allow(non_snake_case)]
pub fn SIDEBAR() -> Color32 { pal().surface }
#[allow(non_snake_case)]
pub fn CARD() -> Color32 { pal().surface }
#[allow(non_snake_case)]
pub fn CARD_HI() -> Color32 { pal().surface3 }
#[allow(non_snake_case)]
pub fn FIELD() -> Color32 { pal().surface2 }
#[allow(non_snake_case)]
pub fn LINE() -> Color32 { pal().line }
#[allow(non_snake_case)]
pub fn LINE2() -> Color32 { pal().line2 }
#[allow(non_snake_case)]
pub fn HOVER() -> Color32 { pal().hover }
#[allow(non_snake_case)]
pub fn ACCENT() -> Color32 { pal().accent }
#[allow(non_snake_case)]
pub fn ACCENT_SOFT() -> Color32 { pal().accent_weak }
#[allow(non_snake_case)]
pub fn ACCENT_INK() -> Color32 { pal().accent_ink }
#[allow(non_snake_case)]
pub fn TEXT() -> Color32 { pal().ink }
#[allow(non_snake_case)]
pub fn TEXT2() -> Color32 { pal().ink2 }
#[allow(non_snake_case)]
pub fn MUTED() -> Color32 { pal().ink3 }
#[allow(non_snake_case)]
pub fn GOOD() -> Color32 { pal().ok }
#[allow(non_snake_case)]
pub fn WARN() -> Color32 { pal().warn }
#[allow(non_snake_case)]
pub fn BAD() -> Color32 { pal().danger }
#[allow(non_snake_case)]
pub fn PURPLE() -> Color32 { pal().brand }

/// The pale background that goes with a status colour (pill style).
pub fn weak_of(c: Color32) -> Color32 {
    let p = pal();
    if c == p.accent {
        p.accent_weak
    } else if c == p.ok {
        p.ok_weak
    } else if c == p.warn {
        p.warn_weak
    } else if c == p.danger {
        p.danger_weak
    } else if c == p.brand {
        p.brand_weak
    } else {
        p.surface3
    }
}

fn user_fonts_dir() -> String {
    std::env::var("LOCALAPPDATA").map(|d| format!("{d}\\Microsoft\\Windows\\Fonts")).unwrap_or_default()
}

fn first_font(paths: &[String]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

/// Inter for Latin letters and Noto Sans KR for Korean (as WorkBoard uses), falling back to
/// Segoe UI + Malgun Gothic on Windows and Apple SD Gothic Neo on macOS.
pub fn install_fonts(ctx: &egui::Context) {
    let u = user_fonts_dir();
    let w = "C:\\Windows\\Fonts";
    let latin = [format!("{u}\\Inter_18pt-Regular.ttf"), format!("{w}\\Inter_18pt-Regular.ttf"), format!("{w}\\segoeui.ttf")];
    let latin_b = [format!("{u}\\Inter_18pt-SemiBold.ttf"), format!("{w}\\Inter_18pt-SemiBold.ttf"), format!("{w}\\seguisb.ttf"), format!("{w}\\segoeuib.ttf")];
    let korean = [
        format!("{w}\\NotoSansKR-Regular.ttf"),
        format!("{u}\\NotoSansKR-Regular.ttf"),
        format!("{w}\\malgun.ttf"),
        "/System/Library/Fonts/AppleSDGothicNeo.ttc".into(),
        "/System/Library/Fonts/Supplemental/AppleGothic.ttf".into(),
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".into(),
    ];
    let korean_b = [format!("{w}\\NotoSansKR-Medium.ttf"), format!("{u}\\NotoSansKR-Medium.ttf"), format!("{w}\\NotoSansKR-Bold.ttf"), format!("{w}\\malgunbd.ttf")];

    let mut fonts = FontDefinitions::default();
    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut regular: Vec<String> = Vec::new();
    let mut boldf: Vec<String> = Vec::new();
    for (name, list, target) in [("latin", &latin[..], 0), ("korean", &korean[..], 0), ("latin-b", &latin_b[..], 1), ("korean-b", &korean_b[..], 1)] {
        if let Some(bytes) = first_font(list) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            if target == 0 { regular.push(name.into()) } else { boldf.push(name.into()) }
        }
    }
    let mut prop = regular.clone();
    prop.extend(defaults.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, prop.clone());
    let mut b = boldf;
    b.extend(prop.iter().cloned());
    fonts.families.insert(FontFamily::Name("bold".into()), b);
    if let Some(k) = regular.iter().find(|n| n.as_str() == "korean") {
        fonts.families.entry(FontFamily::Monospace).or_default().push(k.clone());
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context) {
    let p = pal();
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.ground;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface2;
    v.code_bg_color = p.surface2;
    v.override_text_color = Some(p.ink);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.sel;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.window_corner_radius = CornerRadius::same(6);
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if p.dark { 128 } else { 40 }) };
    v.popup_shadow = v.window_shadow;
    v.menu_corner_radius = CornerRadius::same(6);
    let r = CornerRadius::same(4);
    v.widgets.noninteractive.bg_fill = p.surface;
    v.widgets.noninteractive.weak_bg_fill = p.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.ink);
    // outlined buttons (.btn.o): surface with a line-2 border
    v.widgets.inactive.bg_fill = p.surface3;
    v.widgets.inactive.weak_bg_fill = p.surface;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.line2);
    v.widgets.inactive.corner_radius = r;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.ink);
    v.widgets.hovered.bg_fill = p.surface3;
    v.widgets.hovered.weak_bg_fill = p.surface2;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.line_input);
    v.widgets.hovered.corner_radius = r;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.ink);
    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.weak_bg_fill = p.surface3;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.corner_radius = r;
    v.widgets.active.fg_stroke = Stroke::new(1.0, p.ink);
    v.widgets.open.corner_radius = r;
    v.widgets.open.weak_bg_fill = p.surface2;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.line2);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 7.0);
        s.spacing.interact_size.y = 32.0;
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(13.5));
        s.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        s.text_styles.insert(egui::TextStyle::Heading, egui::FontId::new(22.0, FontFamily::Name("bold".into())));
        s.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.5));
    });
}

pub fn bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).family(FontFamily::Name("bold".into())).size(size).color(TEXT())
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(TEXT2()).size(13.0)
}

/// A WorkBoard card: white surface, 1 px line, 6 px corners.
pub fn card() -> egui::Frame {
    egui::Frame::default()
        .fill(CARD())
        .stroke(Stroke::new(1.0, LINE()))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(16))
}

pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(ACCENT_INK()).size(13.5))
            .fill(ACCENT())
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(4))
            .min_size(egui::vec2(0.0, 32.0)),
    )
}

/// A pill: 22 px tall, pale background in the status colour's family.
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    let fg = if color == TEXT2() || color == MUTED() { TEXT2() } else { color };
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(12.0), fg);
    let size = egui::vec2(galley.size().x + 16.0, 22.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(11), weak_of(color));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
}

/// A thin progress bar on a visible track (WorkBoard style), with its label beside it.
pub fn bar(ui: &mut egui::Ui, frac: f32, width: f32, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(4), pal().surface3);
        let w = (rect.width() * frac.clamp(0.0, 1.0)).max(if frac > 0.0 { 4.0 } else { 0.0 });
        ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(w, 8.0)), CornerRadius::same(4), color);
        if !label.is_empty() {
            ui.label(RichText::new(label).color(TEXT2()).size(12.5));
        }
    });
}

/// A WorkBoard tab: grey label, blue underline when chosen.
pub fn tab(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let font = if selected { egui::FontId::new(13.5, FontFamily::Name("bold".into())) } else { egui::FontId::proportional(13.5) };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, if selected { TEXT() } else { TEXT2() });
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(galley.size().x + 20.0, 36.0), egui::Sense::click());
    if resp.hovered() && !selected {
        ui.painter().rect_filled(rect, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, pal().hover);
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, TEXT());
    if selected {
        ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.0), rect.right_bottom()), CornerRadius::ZERO, ACCENT());
    }
    resp
}

/// A segmented switch (like WorkBoard's KO | VI); returns the option clicked.
pub fn seg(ui: &mut egui::Ui, options: &[&str], selected: usize) -> Option<usize> {
    let mut clicked = None;
    let mut rects = Vec::new();
    let gap = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 0.0;
    for (i, l) in options.iter().enumerate() {
        let on = i == selected;
        let g = ui.painter().layout_no_wrap(l.to_string(), egui::FontId::proportional(13.0), if on { ACCENT() } else { TEXT2() });
        let (r, resp) = ui.allocate_exact_size(egui::vec2(g.size().x + 22.0, 30.0), egui::Sense::click());
        if on {
            ui.painter().rect_filled(r, CornerRadius::ZERO, ACCENT_SOFT());
        } else if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::ZERO, HOVER());
        }
        ui.painter().galley(r.center() - g.size() / 2.0, g, TEXT());
        if resp.clicked() {
            clicked = Some(i);
        }
        rects.push(r);
    }
    if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
        let all = first.union(*last);
        ui.painter().rect_stroke(all, CornerRadius::same(4), Stroke::new(1.0, LINE2()), egui::StrokeKind::Inside);
        for r in rects.iter().skip(1) {
            ui.painter().line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0, LINE2()));
        }
    }
    ui.spacing_mut().item_spacing.x = gap;
    clicked
}
