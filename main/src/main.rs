//! ProjectRem - a local voice assistant.
//!
//! A Rust/egui desktop app: sidebar navigation plus one page per section
//! (Main, Models, Account, Settings, Plugins). Accounts, connections and
//! tokens are handled by a Cloudflare Worker; audio device discovery by a
//! Python sidecar that speaks newline-delimited JSON.

mod account;
mod app;
mod downloader;
mod fonts;
mod i18n;
mod lucide;
mod models;
mod oauth;
mod paths;
mod plugins;
mod settings;
mod sidecar;
mod theme;
mod version;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(version::title())
            .with_app_id(version::APP_ID)
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([1080.0, 680.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        version::APP_ID,
        options,
        Box::new(|cc| Ok(Box::new(app::ProjectRemApp::new(cc)))),
    )
}
