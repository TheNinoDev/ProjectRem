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
#[cfg(test)]
mod ui_tests;
mod version;
mod widgets;

use eframe::egui;

/// Wraps the app so egui's memory is not persisted.
///
/// egui's memory holds panel geometry keyed by widget id. An earlier,
/// completely different layout had left a panel collapsed to a 72 px server
/// rail and a scroll area parked off-screen at x = 1360..1720, and those
/// entries are restored forever — a panel restored to the wrong size swallows
/// the clicks meant for what is inside it. The app persists everything it cares
/// about deliberately (`settings.json`, `session.json`), so egui's copy is pure
/// staleness risk.
struct ProjectRemAppWrapper {
    inner: app::ProjectRemApp,
}

impl eframe::App for ProjectRemAppWrapper {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.inner.ui(ui, frame);
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.inner.on_exit(gl);
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(version::title())
            .with_app_id(version::APP_ID)
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([1080.0, 680.0])
            // No position: let the window manager place the window.
            .with_drag_and_drop(true),
        // eframe restored the saved position verbatim, and after a monitor
        // change the window came back at x = -1621 — entirely off-screen, with
        // the sidebar (on the window's left edge) unreachable. That is what
        // "I can't click the tabs" turned out to be.
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        version::APP_ID,
        options,
        Box::new(|cc| {
            Ok(Box::new(ProjectRemAppWrapper { inner: app::ProjectRemApp::new(cc) }))
        }),
    )
}
