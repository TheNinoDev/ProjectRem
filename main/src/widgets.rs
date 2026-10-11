//! Small reusable widgets shared by every page.
//!
//! Everything here is plain egui built from a `Frame`, so there are no extra
//! dependencies and the whole app stays visually consistent.

use std::collections::HashMap;

use egui::{
    Align, Align2, Color32, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, TextStyle,
    Ui, Vec2,
};

use crate::account::Provider;
use crate::lucide::{self, Name as Icon};
use crate::theme;

const RADIUS: u8 = 10;
const BTN_HEIGHT: f32 = 34.0;
const ICON: f32 = 15.0;

/// Public so callers can reserve the right amount of space in a row before
/// laying out the text column next to these controls.
pub const ICON_BUTTON_WIDTH: f32 = 32.0;
pub const SWITCH_WIDTH: f32 = 38.0;

// ----------------------------------------------------------------- avatars

/// Decoded pixels on their way from the download thread to the UI thread.
///
/// `egui::ColorImage` is not `Send`, so the raw bytes travel instead and the
/// image is rebuilt on the UI side.
struct Rgba {
    size: [usize; 2],
    pixels: Vec<u8>,
}

/// Downloaded profile pictures, keyed by URL.
///
/// Avatars are fetched on a background thread and decoded here, because egui's
/// own HTTP loader is not compiled into this build.
pub struct AvatarCache {
    textures: HashMap<String, egui::TextureHandle>,
    incoming: std::sync::mpsc::Receiver<(String, Rgba)>,
    sender: std::sync::mpsc::Sender<(String, Rgba)>,
    requested: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl AvatarCache {
    pub fn new() -> Self {
        let (sender, incoming) = std::sync::mpsc::channel();
        Self {
            textures: HashMap::new(),
            incoming,
            sender,
            requested: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// Move anything that finished downloading into the texture cache.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((url, rgba)) = self.incoming.try_recv() {
            let image = egui::ColorImage::from_rgba_unmultiplied(rgba.size, &rgba.pixels);
            let handle = ctx.load_texture(url.clone(), image, egui::TextureOptions::LINEAR);
            self.textures.insert(url, handle);
        }
    }

    fn get(&self, url: &str) -> Option<&egui::TextureHandle> {
        self.textures.get(url)
    }

    /// Kick off a download the first time we see a URL.
    fn request(&self, url: &str) {
        let mut requested = self.requested.lock().unwrap();
        if !requested.insert(url.to_owned()) {
            return;
        }
        let url = url.to_owned();
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let Ok(response) = reqwest::blocking::get(&url) else { return };
            let Ok(bytes) = response.bytes() else { return };
            let Ok(decoded) = image::load_from_memory(&bytes) else { return };
            let image = decoded.to_rgba8();
            let (w, h) = (image.width() as usize, image.height() as usize);
            if w == 0 || h == 0 {
                return;
            }
            let _ = sender.send((url, Rgba { size: [w, h], pixels: image.into_raw() }));
        });
    }
}

/// Draw a remote picture, or a neutral placeholder until it arrives.
pub fn avatar_image(ui: &mut Ui, cache: &mut AvatarCache, url: &str, rect: Rect) {
    if !cache.get(url).is_some() {
        cache.request(url);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
    }

    if let Some(handle) = cache.get(url).cloned() {
        // Cover-fit: keep the aspect ratio and crop the overflow.
        let size = handle.size_vec2();
        let uv = if size.x > 0.0 && size.y > 0.0 {
            let target = rect.width() / rect.height().max(1.0);
            let source = size.x / size.y.max(1.0);
            if source > target {
                let keep = target / source;
                Rect::from_min_max(
                    egui::pos2((1.0 - keep) / 2.0, 0.0),
                    egui::pos2((1.0 + keep) / 2.0, 1.0),
                )
            } else {
                let keep = source / target;
                Rect::from_min_max(
                    egui::pos2(0.0, (1.0 - keep) / 2.0),
                    egui::pos2(1.0, (1.0 + keep) / 2.0),
                )
            }
        } else {
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
        };

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(0), theme::BG_INPUT);
        painter.image(handle.id(), rect, uv, Color32::WHITE);
        return;
    }

    ui.painter().circle_filled(rect.center(), rect.width() / 2.0, theme::BG_INPUT);
    lucide::paint(
        ui.painter(),
        Icon::CircleUser,
        Rect::from_center_size(rect.center(), Vec2::splat(rect.width() * 0.6)),
        theme::TEXT_FAINT,
        2.0,
    );
}

// A neutral placeholder used when there is no picture yet.
pub fn fallback_avatar(ui: &mut Ui, rect: Rect, color: Color32) {
    ui.painter().circle_filled(rect.center(), rect.width() / 2.0, color);
    lucide::paint(
        ui.painter(),
        Icon::CircleUser,
        Rect::from_center_size(rect.center(), Vec2::splat(rect.width() * 0.6)),
        Color32::WHITE,
        2.0,
    );
}

// ------------------------------------------------------------- containers

/// A padded surface with a hairline border - the app's basic grouping element.
pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(theme::BG_CARD)
        .corner_radius(CornerRadius::same(RADIUS))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// A small coloured label, e.g. the model kind.
pub fn pill(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.5).strong().color(color));
        });
}

/// A section heading: icon + title, with a hairline under it.
pub fn section(ui: &mut Ui, icon: Icon, title: &str) {
    ui.horizontal(|ui| {
        let size = Vec2::splat(16.0);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        lucide::paint(ui.painter(), icon, rect, theme::TEXT_MUTED, 2.0);
        ui.add_space(2.0);
        ui.label(RichText::new(title).size(13.5).strong().color(theme::TEXT_STRONG));
    });
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, theme::BORDER));
}

// ---------------------------------------------------------------- buttons

/// Width a label needs at the body font size.
pub fn text_width(ui: &Ui, label: &str) -> f32 {
    if label.is_empty() {
        return 0.0;
    }
    let font = TextStyle::Body.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(label.to_owned(), font, theme::TEXT_STRONG)
        .size()
        .x
}

/// Width [`button`] will allocate for this icon and label.
///
/// Exposed so a caller can centre a button by padding with `space`, rather
/// than laying it out twice to find out how wide it came out.
pub fn button_width(ui: &Ui, icon: Icon, label: &str) -> f32 {
    button_measure(ui, Some(icon), label).0
}

/// Shared by [`button`] and [`button_width`] so the two cannot drift apart.
fn button_measure(ui: &Ui, icon: Option<Icon>, label: &str) -> (f32, f32) {
    let gap = 7.0;
    let label_w = text_width(ui, label);
    let icon_w = if icon.is_some() { ICON + gap } else { 0.0 };
    let width = (icon_w + label_w + 26.0).max(if label.is_empty() { BTN_HEIGHT } else { 0.0 });
    (width, label_w)
}

fn button(
    ui: &mut Ui,
    icon: Option<Icon>,
    label: &str,
    fill: Color32,
    text_color: Color32,
    enabled: bool,
) -> egui::Response {
    let (width, _) = button_measure(ui, icon, label);
    let gap = 7.0;

    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, BTN_HEIGHT), sense);

    if enabled {
        let fill = if response.is_pointer_button_down_on() {
            fill.gamma_multiply(0.85)
        } else {
            fill
        };
        ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
    } else {
        ui.painter().rect_filled(rect, CornerRadius::same(7), theme::BG_INPUT);
    }

    let color = if !enabled {
        theme::TEXT_FAINT
    } else if response.hovered() {
        text_color
    } else {
        text_color.gamma_multiply(0.94)
    };

    let mut x = rect.left() + 13.0;
    if let Some(glyph) = icon {
        let box_rect =
            Rect::from_min_size(egui::pos2(x, rect.center().y - ICON / 2.0), Vec2::splat(ICON));
        lucide::paint(ui.painter(), glyph, box_rect, color, 2.0);
        x += ICON + gap;
    }
    if !label.is_empty() {
        ui.painter().text(
            egui::pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            TextStyle::Body.resolve(ui.style()),
            color,
        );
    }
    response
}

pub fn primary_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    button(ui, Some(icon), label, theme::ACCENT, Color32::WHITE, true)
}

pub fn secondary_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    button(ui, Some(icon), label, theme::BG_INPUT, theme::TEXT_STRONG, true)
}

pub fn danger_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    button(ui, Some(icon), label, theme::DANGER, Color32::WHITE, true)
}

/// Greyed-out button used while a request is in flight.
pub fn disabled_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    button(ui, Some(icon), label, theme::BG_INPUT, theme::TEXT_FAINT, false)
}

/// Square, icon-only button.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str, subtle: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ICON_BUTTON_WIDTH), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(7), theme::BG_HOVER);
    }
    let color = if response.hovered() {
        theme::TEXT_STRONG
    } else if subtle {
        theme::TEXT_MUTED
    } else {
        theme::TEXT_NORMAL
    };
    lucide::paint(
        ui.painter(),
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
        color,
        2.0,
    );
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// A flat button: no fill until hovered.
pub fn ghost_button_icon(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    tooltip: &str,
) -> egui::Response {
    let width = (text_width(ui, label) + ICON + 7.0 + 26.0).max(34.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, BTN_HEIGHT), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(7), theme::BG_HOVER);
    }
    let color = if response.hovered() { theme::TEXT_STRONG } else { theme::TEXT_MUTED };
    lucide::paint(
        ui.painter(),
        icon,
        Rect::from_min_size(egui::pos2(rect.left() + 13.0, rect.center().y - ICON / 2.0), Vec2::splat(ICON)),
        color,
        2.0,
    );
    if !label.is_empty() {
        ui.painter().text(
            egui::pos2(rect.left() + 13.0 + ICON + 7.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            TextStyle::Body.resolve(ui.style()),
            color,
        );
    }
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// A two-way toggle pill, used for Login / Register.
pub fn segment(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let width = (text_width(ui, label) + 34.0).max(84.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(7), theme::ACCENT);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(7), theme::BG_HOVER);
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        TextStyle::Body.resolve(ui.style()),
        if selected { Color32::WHITE } else { theme::TEXT_MUTED },
    );
    response
}

/// An on/off switch.
pub fn switch(ui: &mut Ui, on: &mut bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(SWITCH_WIDTH, 22.0), Sense::click());
    if response.clicked() {
        *on = !*on;
    }
    let track = if *on { theme::ACCENT } else { theme::BG_INPUT };
    ui.painter().rect_filled(rect, CornerRadius::same(11), track);
    let knob_x = if *on { rect.right() - 11.0 } else { rect.left() + 11.0 };
    ui.painter().circle_filled(egui::pos2(knob_x, rect.center().y), 8.0, Color32::WHITE);
    response
}

// ----------------------------------------------------------------- labels

/// Label + value on one line, used on the About card.
pub fn key_value(ui: &mut Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 20.0], egui::Label::new(RichText::new(key).size(12.5).color(theme::TEXT_MUTED)).truncate());
        ui.label(RichText::new(value).size(12.5).color(theme::TEXT_NORMAL));
    });
}

/// A dot + label + value row for the status list.
pub fn status_row(ui: &mut Ui, label: &str, value: &str, ok: bool) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [180.0, 22.0],
            egui::Label::new(RichText::new(label).size(13.0).color(theme::TEXT_MUTED)).truncate(),
        );
        ui.painter().circle_filled(
            ui.next_widget_position() + Vec2::new(4.0, 7.0),
            3.5,
            if ok { theme::SUCCESS } else { theme::TEXT_FAINT },
        );
        ui.add_space(10.0);
        ui.label(RichText::new(value).size(13.0).color(theme::TEXT_NORMAL));
    });
}

/// One avatar option in the profile-picture picker.
pub fn avatar_choice(
    ui: &mut Ui,
    cache: &mut AvatarCache,
    provider: Provider,
    name: &str,
    url: &str,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(82.0, 82.0), Sense::click());

    let ring = if selected { theme::ACCENT } else { theme::BORDER };
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(if selected { 2.0 } else { 1.0 }, ring),
        egui::StrokeKind::Middle,
    );

    let inner = Rect::from_center_size(rect.center(), Vec2::splat(60.0));
    avatar_image(ui, cache, url, inner);

    fn small() -> egui::FontId {
        egui::FontId::proportional(10.5)
    }
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y + 13.0),
        Align2::CENTER_CENTER,
        provider.display(),
        small(),
        theme::TEXT_MUTED,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y + 26.0),
        Align2::CENTER_CENTER,
        name,
        small(),
        theme::TEXT_FAINT,
    );
    // The labels live below the allocated box, so grow the row manually.
    ui.min_rect().max.x = ui.min_rect().max.x.max(rect.right());
    ui.min_rect().max.y = ui.min_rect().max.y.max(rect.max.y + 32.0);
    response.on_hover_text(name)
}

/// The "no picture" option, which clears the profile picture.
pub fn avatar_choice_none(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(82.0, 82.0), Sense::click());
    let ring = if selected { theme::ACCENT } else { theme::BORDER };
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(if selected { 2.0 } else { 1.0 }, ring),
        egui::StrokeKind::Middle,
    );
    lucide::paint(
        ui.painter(),
        Icon::X,
        Rect::from_center_size(rect.center(), Vec2::splat(24.0)),
        theme::TEXT_FAINT,
        2.0,
    );
    fn small() -> egui::FontId {
        egui::FontId::proportional(10.5)
    }
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y + 13.0),
        Align2::CENTER_CENTER,
        label,
        small(),
        theme::TEXT_MUTED,
    );
    ui.min_rect().max.x = ui.min_rect().max.x.max(rect.right());
    ui.min_rect().max.y = ui.min_rect().max.y.max(rect.max.y + 32.0);
    response
}

/// Which action a connection row asked for.
pub enum RowClick {
    Connect(Provider),
    Disconnect(Provider),
}

/// A connected service row: icon, name, state and the connect/disconnect action.
pub fn connection_row(
    ui: &mut Ui,
    provider: Provider,
    detail: &str,
    connected: bool,
    busy: bool,
    disconnect_label: &str,
    connect_label: &str,
) -> Option<RowClick> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(40.0, 36.0), Sense::hover());
        lucide::paint(
            ui.painter(),
            provider_icon(provider),
            Rect::from_center_size(icon_rect.center(), Vec2::splat(19.0)),
            if connected { theme::SUCCESS } else { theme::TEXT_MUTED },
            2.0,
        );

        ui.vertical(|ui| {
            ui.label(RichText::new(provider.display()).size(13.5).color(theme::TEXT_STRONG));
            ui.label(
                RichText::new(detail.to_owned())
                    .size(12.0)
                    .color(if connected { theme::SUCCESS } else { theme::TEXT_FAINT }),
            );
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if connected {
                if secondary_button(ui, Icon::Unlink, disconnect_label).clicked() {
                    clicked = Some(RowClick::Disconnect(provider));
                }
            } else if busy {
                disabled_button(ui, Icon::LoaderCircle, "");
            } else if secondary_button(ui, Icon::Link, connect_label).clicked() {
                clicked = Some(RowClick::Connect(provider));
            }
        });
    });
    clicked
}

pub fn provider_icon(provider: Provider) -> Icon {
    match provider {
        Provider::Github => Icon::Github,
        Provider::Discord => Icon::MessageCircle,
        Provider::Huggingface => Icon::CircleUser,
    }
}