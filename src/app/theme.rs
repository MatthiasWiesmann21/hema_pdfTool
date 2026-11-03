//! Visual style: palette, light/dark visuals, fonts and small styled widgets.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Shadow, Stroke, TextStyle,
    Theme, Visuals,
};

pub const ACCENT: Color32 = Color32::from_rgb(91, 91, 214);

/// Colors used to tell source documents apart.
pub const DOC_PALETTE: [Color32; 8] = [
    Color32::from_rgb(91, 91, 214),
    Color32::from_rgb(16, 163, 127),
    Color32::from_rgb(234, 128, 32),
    Color32::from_rgb(219, 68, 110),
    Color32::from_rgb(14, 140, 214),
    Color32::from_rgb(160, 90, 220),
    Color32::from_rgb(200, 160, 20),
    Color32::from_rgb(90, 130, 90),
];

pub const CARD_RADIUS: u8 = 12;

/// Registers the icon font, both theme variants and spacing.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);

    ctx.set_visuals_of(Theme::Light, light());
    ctx.set_visuals_of(Theme::Dark, dark());

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 6.0);
        s.button_padding = egui::vec2(10.0, 5.0);
        s.interact_size.y = 26.0;
        s.window_margin = Margin::same(16);
        s.menu_margin = Margin::same(8);
        s.combo_width = 120.0;
    });
}

fn common(v: &mut Visuals, dark: bool) {
    // Tinted selection (selected buttons, combo entries, text highlight):
    // the stroke color doubles as the text color on that tint.
    if dark {
        v.selection.bg_fill = Color32::from_rgb(72, 72, 160);
        v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(228, 228, 255));
    } else {
        v.selection.bg_fill = Color32::from_rgb(224, 224, 250);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
    }
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 45 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 90 } else { 35 }),
    };
    let w = &mut v.widgets;
    for wv in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        wv.corner_radius = CornerRadius::same(8);
    }
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.hovered.expansion = 0.0;
    w.active.expansion = 0.0;
}

fn light() -> Visuals {
    let mut v = Visuals::light();
    common(&mut v, false);
    v.panel_fill = Color32::from_rgb(244, 245, 250);
    v.window_fill = Color32::WHITE;
    v.faint_bg_color = Color32::WHITE;
    v.extreme_bg_color = Color32::from_rgb(236, 238, 245);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(224, 227, 236));
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(226, 229, 238));
    w.noninteractive.bg_fill = Color32::WHITE;
    w.inactive.weak_bg_fill = Color32::from_rgb(234, 236, 244);
    w.inactive.bg_fill = Color32::from_rgb(226, 229, 240);
    w.hovered.weak_bg_fill = Color32::from_rgb(224, 227, 242);
    w.hovered.bg_fill = Color32::from_rgb(220, 223, 240);
    w.active.weak_bg_fill = Color32::from_rgb(210, 214, 238);
    w.active.bg_fill = Color32::from_rgb(210, 214, 238);
    w.open.weak_bg_fill = Color32::from_rgb(224, 227, 242);
    v
}

fn dark() -> Visuals {
    let mut v = Visuals::dark();
    common(&mut v, true);
    v.panel_fill = Color32::from_rgb(22, 24, 31);
    v.window_fill = Color32::from_rgb(32, 35, 45);
    v.faint_bg_color = Color32::from_rgb(33, 36, 47);
    v.extreme_bg_color = Color32::from_rgb(17, 19, 25);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(50, 54, 68));
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(48, 52, 66));
    w.noninteractive.bg_fill = Color32::from_rgb(33, 36, 47);
    w.inactive.weak_bg_fill = Color32::from_rgb(44, 48, 62);
    w.inactive.bg_fill = Color32::from_rgb(52, 56, 72);
    w.hovered.weak_bg_fill = Color32::from_rgb(54, 58, 76);
    w.hovered.bg_fill = Color32::from_rgb(60, 64, 82);
    w.active.weak_bg_fill = Color32::from_rgb(66, 70, 92);
    w.active.bg_fill = Color32::from_rgb(66, 70, 92);
    w.open.weak_bg_fill = Color32::from_rgb(54, 58, 76);
    v
}

/// Background of the header bars (white in light mode, raised in dark mode).
pub fn bar_fill(v: &Visuals) -> Color32 {
    v.window_fill
}

/// "icon  label", or just the icon when `compact`.
pub fn label(icon: &str, text: &str, compact: bool) -> String {
    if compact {
        icon.to_string()
    } else {
        format!("{icon}  {text}")
    }
}

/// Filled accent button for the main action.
pub fn primary(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(Color32::WHITE).strong()).fill(ACCENT)
}

/// Borderless icon button that only shows a background on hover.
pub fn icon_button(icon: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(icon).size(16.0)).frame_when_inactive(false)
}

/// Small rounded badge with text.
pub fn pill(ui: &mut egui::Ui, text: impl Into<String>, fill: Color32, fg: Color32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().color(fg));
        });
}

/// Muted pill using the current theme's colors.
pub fn soft_pill(ui: &mut egui::Ui, text: impl Into<String>) {
    let v = ui.visuals();
    let (fill, fg) = (v.widgets.inactive.weak_bg_fill, v.text_color());
    pill(ui, text, fill, fg);
}
