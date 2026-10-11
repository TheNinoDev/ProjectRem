//! Palette and global style.
//!
//! A neutral dark surface palette with a single blue accent. Deliberately not
//! a copy of any other product: the app is a plain desktop tool, so it uses
//! plain surfaces, one accent colour and Lucide icons.

use egui::{epaint, Color32, CornerRadius, Stroke, Style, Visuals};

// Surfaces ---------------------------------------------------------------
pub const BG_APP: Color32 = Color32::from_rgb(0x16, 0x17, 0x1A);
pub const BG_SIDEBAR: Color32 = Color32::from_rgb(0x1B, 0x1C, 0x20);
pub const BG_HEADER: Color32 = Color32::from_rgb(0x1B, 0x1C, 0x20);
pub const BG_CARD: Color32 = Color32::from_rgb(0x21, 0x22, 0x27);
pub const BG_INPUT: Color32 = Color32::from_rgb(0x2A, 0x2C, 0x33);
pub const BG_HOVER: Color32 = Color32::from_rgb(0x31, 0x33, 0x3B);
pub const BG_SIDEBAR_ITEM_HOVER: Color32 = Color32::from_rgb(0x26, 0x27, 0x2D);
pub const BG_SIDEBAR_ITEM_ACTIVE: Color32 = Color32::from_rgb(0x2A, 0x2C, 0x34);

// Lines ------------------------------------------------------------------
pub const BORDER: Color32 = Color32::from_rgb(0x30, 0x32, 0x3A);

// Accent + status --------------------------------------------------------
pub const ACCENT: Color32 = Color32::from_rgb(0x4F, 0x7C, 0xF7);
pub const SUCCESS: Color32 = Color32::from_rgb(0x3D, 0xB9, 0x7E);
pub const WARNING: Color32 = Color32::from_rgb(0xE0, 0xA8, 0x3E);
pub const DANGER: Color32 = Color32::from_rgb(0xE0, 0x54, 0x54);

// Text -------------------------------------------------------------------
pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xF0, 0xF1, 0xF4);
pub const TEXT_NORMAL: Color32 = Color32::from_rgb(0xD2, 0xD5, 0xDB);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x93, 0x98, 0xA3);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x6E, 0x74, 0x80);

// Metrics ----------------------------------------------------------------
pub const SIDEBAR_WIDTH: f32 = 236.0;
pub const HEADER_HEIGHT: f32 = 66.0;
pub const ACTION_BAR_HEIGHT: f32 = 62.0;

/// Global egui style.
pub fn style() -> Style {
    let mut style = Style::default();

    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG_APP;
    visuals.window_fill = BG_CARD;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_shadow = epaint::Shadow::NONE;
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.popup_shadow = epaint::Shadow::NONE;
    visuals.extreme_bg_color = BG_INPUT;
    visuals.faint_bg_color = BG_CARD;
    visuals.override_text_color = Some(TEXT_NORMAL);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.5);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    let w = &mut visuals.widgets;

    // `bg_fill` stays opaque (egui paints slider and checkbox tracks with it);
    // `weak_bg_fill` is the optional one, so buttons can stay flat.
    w.noninteractive.bg_fill = BG_INPUT;
    w.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_NORMAL);
    w.noninteractive.bg_stroke = Stroke::NONE;
    w.noninteractive.corner_radius = CornerRadius::same(6);

    for state in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        state.bg_fill = BG_INPUT;
        state.weak_bg_fill = BG_INPUT;
        state.fg_stroke = Stroke::new(1.0, TEXT_NORMAL);
        state.bg_stroke = Stroke::NONE;
        state.corner_radius = CornerRadius::same(6);
    }

    style.visuals = visuals;

    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.interact_size.y = 26.0;
    style.spacing.slider_width = 180.0;
    style.spacing.scroll.bar_width = 10.0;
    style.spacing.scroll.floating = true;

    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(18.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(13.5, egui::FontFamily::Proportional),
    );

    style
}