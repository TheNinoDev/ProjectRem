//! Application shell.
//!
//! Layout is a conventional desktop app: a fixed sidebar with the five
//! sections, and one scrolling page on the right. There is no server rail, no
//! channel list and no message composer.
//!
//! Every page follows the same shape so the borrow checker stays happy and the
//! UI thread never blocks:
//!
//! 1. the page *gathers* what it needs from `self` (cheap, copyable data),
//! 2. a free function *renders* it and writes back at most one pending action,
//! 3. the action runs on `self` once the rendering is done.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{
    Align, Align2, Color32, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2,
};

use crate::account::{self, AccountState, Api, Provider};
use crate::downloader::{Downloader, Event as DownloadEvent, State as DownloadState};
use crate::i18n::{I18n, Language, Msg};
use crate::lucide::Name as Icon;
use crate::models::{self, ModelDef};
use crate::oauth::{self, Callback, CallbackServer};
use crate::plugins::{self, Origin, Plugin};
use crate::settings::{Settings, SYSTEM_DEFAULT};
use crate::sidecar::{AudioDevices, DeviceInfo, Sidecar, Status as SidecarStatus};
use crate::{paths, theme, version, widgets};

/// Translation lookup. `I18n` is `Copy`, so this works inside closures.
type Tr = fn(I18n, Msg) -> &'static str;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Main,
    Models,
    Account,
    Settings,
    Plugins,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Main, Tab::Models, Tab::Account, Tab::Settings, Tab::Plugins];

    const fn icon(self) -> Icon {
        match self {
            Tab::Main => Icon::MessageCircle,
            Tab::Models => Icon::Cpu,
            Tab::Account => Icon::User,
            Tab::Settings => Icon::Settings,
            Tab::Plugins => Icon::Puzzle,
        }
    }

    const fn name(self) -> Msg {
        match self {
            Tab::Main => Msg::NavMain,
            Tab::Models => Msg::NavModels,
            Tab::Account => Msg::NavAccount,
            Tab::Settings => Msg::NavSettings,
            Tab::Plugins => Msg::NavPlugins,
        }
    }

    const fn description(self) -> Msg {
        match self {
            Tab::Main => Msg::PageMain,
            Tab::Models => Msg::PageModels,
            Tab::Account => Msg::PageAccount,
            Tab::Settings => Msg::PageSettings,
            Tab::Plugins => Msg::PagePlugins,
        }
    }

    const fn slug(self) -> &'static str {
        match self {
            Tab::Main => "main",
            Tab::Models => "models",
            Tab::Account => "account",
            Tab::Settings => "settings",
            Tab::Plugins => "plugins",
        }
    }

    fn from_slug(slug: &str) -> Option<Tab> {
        Tab::ALL.into_iter().find(|t| t.slug() == slug)
    }
}

// ------------------------------------------------------------------ state

struct Toast {
    text: String,
    color: Color32,
    until: Instant,
}

enum ToolbarAction {
    DownloadMissing,
    Cancel,
    OpenModelsFolder,
    RefreshDevices,
    ImportPlugin,
}

/// Which half of the Account page is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    Login,
    Register,
}

/// What a page asks the shell to do once it has finished rendering.
#[derive(Default)]
enum Pending {
    #[default]
    Nothing,
    Toolbar(ToolbarAction),
    Account(AccountAction),
    Plugins(PluginsAction),
}

#[derive(Debug, Clone, PartialEq)]
enum AccountAction {
    SignOut,
    Connect(Provider),
    Disconnect(Provider),
    PickAvatar(Option<String>),
    SaveHfToken(String),
}

/// A long-running call to the Worker, so the UI thread never blocks.
enum AccountJob {
    Register { username: String, password: String },
    Login { username: String, password: String },
    Logout,
    Disconnect(Provider),
    PickAvatar(Option<String>),
    SaveHfToken(String),
    /// Finished an OAuth handshake in the browser.
    Connected {
        provider: Provider,
        username: Option<String>,
        avatar_url: Option<String>,
        access_token: Option<String>,
    },
}

enum AccountReply {
    Refreshed(AccountState),
    SignedIn { token: String, user: account::User },
    Failed(String),
}

/// Background thread that owns the Worker client.
struct AccountWorker {
    tx: std::sync::mpsc::Sender<AccountJob>,
    rx: std::sync::mpsc::Receiver<AccountReply>,
}

impl AccountWorker {
    fn new() -> Self {
        let (tx, job_rx) = std::sync::mpsc::channel::<AccountJob>();
        let (reply_tx, rx) = std::sync::mpsc::channel::<AccountReply>();

        std::thread::Builder::new()
            .name("projectrem-account".into())
            .spawn(move || {
                let api = Api::new();
                let mut state = AccountState::default();

                // Restore a previous session without making the user sign in
                // again on every launch.
                state.token = account::load_token();
                if let Some(token) = state.token.clone() {
                    match api.me(&token) {
                        Ok(user) => {
                            state.user = Some(user);
                            refresh(&api, &mut state);
                        }
                        Err(_) => {
                            account::clear_token();
                            state.token = None;
                        }
                    }
                }
                let initial = state.clone();
                let _ = reply_tx.send(AccountReply::Refreshed(initial));

                while let Ok(job) = job_rx.recv() {
                    let token = state.token.clone().unwrap_or_default();
                    let reply = match job {
                        AccountJob::Register { username, password } => {
                            match api.register(&username, &password) {
                                // Registering does not open a session, so log
                                // straight in afterwards.
                                Ok(_) => match api.login(&username, &password) {
                                    Ok((token, user)) => {
                                        account::save_token(&token);
                                        state.token = Some(token.clone());
                                        state.user = Some(user);
                                        refresh(&api, &mut state);
                                        AccountReply::SignedIn {
                                            token,
                                            user: user_of(&state),
                                        }
                                    }
                                    Err(err) => AccountReply::Failed(err.to_string()),
                                },
                                Err(err) => AccountReply::Failed(err.to_string()),
                            }
                        }
                        AccountJob::Login { username, password } => match api.login(&username, &password) {
                            Ok((token, user)) => {
                                account::save_token(&token);
                                state.token = Some(token.clone());
                                state.user = Some(user);
                                refresh(&api, &mut state);
                                AccountReply::SignedIn { token, user: user_of(&state) }
                            }
                            Err(err) => AccountReply::Failed(err.to_string()),
                        },
                        AccountJob::Logout => {
                            let _ = api.logout(&token);
                            account::clear_token();
                            state = AccountState::default();
                            AccountReply::Refreshed(state.clone())
                        }
                        AccountJob::Disconnect(provider) => match api.disconnect(&token, provider) {
                            Ok(()) => {
                                refresh(&api, &mut state);
                                AccountReply::Refreshed(state.clone())
                            }
                            Err(err) => AccountReply::Failed(err.to_string()),
                        },
                        AccountJob::PickAvatar(url) => match api.set_avatar(&token, url.as_deref()) {
                            Ok(user) => {
                                state.user = Some(user);
                                refresh(&api, &mut state);
                                AccountReply::Refreshed(state.clone())
                            }
                            Err(err) => AccountReply::Failed(err.to_string()),
                        },
                        AccountJob::SaveHfToken(value) => match api.store_token(&token, Provider::Huggingface, &value)
                        {
                            Ok(()) => {
                                refresh(&api, &mut state);
                                AccountReply::Refreshed(state.clone())
                            }
                            Err(err) => AccountReply::Failed(err.to_string()),
                        },
                        AccountJob::Connected { provider, username, avatar_url, access_token } => {
                            match api.save_connection(
                                &token,
                                provider,
                                username.as_deref(),
                                avatar_url.as_deref(),
                                access_token.as_deref(),
                            ) {
                                Ok(()) => {
                                    if let Ok(user) = api.me(&token) {
                                        state.user = Some(user);
                                    }
                                    refresh(&api, &mut state);
                                    AccountReply::Refreshed(state.clone())
                                }
                                Err(err) => AccountReply::Failed(err.to_string()),
                            }
                        }
                    };
                    let _ = reply_tx.send(reply);
                }
            })
            .ok();

        Self { tx, rx }
    }

    fn send(&self, job: AccountJob) {
        let _ = self.tx.send(job);
    }

    fn poll(&self) -> Option<AccountReply> {
        self.rx.try_recv().ok()
    }
}

fn refresh(api: &Api, state: &mut AccountState) {
    let Some(token) = state.token.clone() else { return };
    if let Ok(connections) = api.connections(&token) {
        state.connections = connections;
    }
    if let Ok(bots) = api.bots(&token) {
        state.bots = bots;
    }
    if let Ok(status) = api.token_status(&token, Provider::Huggingface) {
        state.hf_token_stored = status.stored;
        state.hf_token_stored_at = status.created_at;
    }
}

fn user_of(state: &AccountState) -> account::User {
    state.user.clone().unwrap_or_else(|| account::User {
        id: String::new(),
        username: String::new(),
        display_name: String::new(),
        avatar_url: None,
        created_at: String::new(),
    })
}

// ------------------------------------------------------------------- app

pub struct ProjectRemApp {
    i18n: I18n,
    settings: Settings,
    tab: Tab,

    account: AccountState,
    account_worker: AccountWorker,
    auth_mode: AuthMode,
    username: String,
    password: String,
    show_password: bool,
    account_busy: bool,
    callback_server: Option<CallbackServer>,
    oauth_replies: Option<std::sync::mpsc::Receiver<Callback>>,
    oauth_busy: bool,
    hf_token_input: String,

    catalog: Vec<ModelDef>,
    downloads: HashMap<String, DownloadState>,

    devices: AudioDevices,
    devices_requested: bool,

    plugins: Vec<Plugin>,
    avatars: widgets::AvatarCache,

    sidecar: Sidecar,
    downloader: Downloader,
    toast: Option<Toast>,
    #[cfg(feature = "screenshot-hook")]
    shot_frame: u64,
}

impl ProjectRemApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        let tab = settings.last_tab.as_deref().and_then(Tab::from_slug).unwrap_or(Tab::Main);
        #[cfg(feature = "screenshot-hook")]
        let tab = match std::env::var("PROJECTREM_TAB").ok().as_deref().and_then(Tab::from_slug) {
            Some(forced) => forced,
            None => tab,
        };
        let plugins = load_plugins_with_state(&settings.enabled_plugins);

        crate::fonts::install(&cc.egui_ctx);
        cc.egui_ctx.set_global_style(theme::style());
        cc.egui_ctx.set_theme(egui::Theme::Dark);

        let mut sidecar = Sidecar::new();
        sidecar.ensure_started();

        Self {
            i18n: I18n::new(settings.software_language),
            settings,
            tab,
            account: AccountState::default(),
            account_worker: AccountWorker::new(),
            auth_mode: AuthMode::Login,
            username: String::new(),
            password: String::new(),
            show_password: false,
            account_busy: false,
            callback_server: None,
            oauth_replies: None,
            oauth_busy: false,
            hf_token_input: String::new(),
            catalog: models::catalog(),
            downloads: HashMap::new(),
            devices: AudioDevices::default(),
            devices_requested: true,
            plugins,
            avatars: widgets::AvatarCache::new(),
            sidecar,
            downloader: Downloader::new(),
            toast: None,
            #[cfg(feature = "screenshot-hook")]
            shot_frame: 0,
        }
    }

    fn t(&self, msg: Msg) -> &'static str {
        self.i18n.t(msg)
    }

    fn toast(&mut self, text: impl Into<String>, color: Color32) {
        self.toast =
            Some(Toast { text: text.into(), color, until: Instant::now() + Duration::from_secs(4) });
    }

    // ------------------------------------------------------------- frame

    fn render(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        #[cfg(feature = "screenshot-hook")]
        self.verify_screenshot(ui);
        self.avatars.poll(ui.ctx());
        self.pump_downloads();
        self.pump_account();
        self.pump_sidecar();
        self.pump_oauth();

        if self.downloads.values().any(|s| s.is_active()) || self.oauth_busy {
            ui.ctx().request_repaint_after(Duration::from_millis(120));
        }

        ui.painter().rect_filled(ui.available_rect_before_wrap(), 0.0, theme::BG_APP);

        // Order matters: the first panel added is the outermost, so the sidebar
        // is full height and the header / action bar only span the content.
        egui::Panel::left("sidebar")
            .exact_size(theme::SIDEBAR_WIDTH)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(theme::BG_SIDEBAR))
            .show(ui, |ui| self.sidebar(ui));

        egui::Panel::top("header")
            .exact_size(theme::HEADER_HEIGHT)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(theme::BG_HEADER))
            .show(ui, |ui| self.header(ui));

        if matches!(self.tab, Tab::Models | Tab::Settings | Tab::Plugins) {
            egui::Panel::bottom("action_bar")
                .exact_size(theme::ACTION_BAR_HEIGHT)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(theme::BG_CARD))
                .show(ui, |ui| self.action_bar(ui));
        }

        // Whatever is left over is the page body.
        self.page(ui);
        self.draw_toast(ui);
    }

    fn header(&mut self, ui: &mut Ui) {
        let i18n = self.i18n;
        let t: Tr = I18n::t;
        egui::Frame::new()
            .fill(theme::BG_HEADER)
            .inner_margin(Margin::symmetric(24, 13))
            .show(ui, |ui| {
                // A `Frame` sizes itself to its content, so the width has to be
                // claimed from inside the frame for the header to span the page.
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(t(i18n, self.tab.name()))
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT_STRONG),
                );
                ui.label(
                    RichText::new(t(i18n, self.tab.description()))
                        .size(12.5)
                        .color(theme::TEXT_FAINT),
                );
            });
    }

    // ----------------------------------------------------------- sidebar

    fn sidebar(&mut self, ui: &mut Ui) {
        let i18n = self.i18n;
        let t: Tr = I18n::t;
        let mut picked: Option<Tab> = None;

        egui::Frame::new()
            .fill(theme::BG_SIDEBAR)
            .inner_margin(Margin::symmetric(14, 16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(t(i18n, Msg::AppName)).size(16.0).strong().color(theme::TEXT_STRONG));
                ui.label(RichText::new(version::VERSION).size(11.0).color(theme::TEXT_FAINT));
            });

        ui.add_space(14.0);
        // The nav rows are inset so the active pill does not touch the window
        // edge and the accent bar has somewhere to sit.
        egui::Frame::new()
            .inner_margin(Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for tab in Tab::ALL {
                    picked = nav_item(ui, tab, self.tab == tab, t(i18n, tab.name()));
                }
            });
        if let Some(tab) = picked {
            self.select_tab(tab);
        }

        // The account row is pinned to the bottom of the sidebar. `bottom_up`
        // only places what a layout actually *allocates*, so the footer has to
        // reserve its own rect instead of painting wherever the cursor happens
        // to be.
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| self.account_footer(ui));
    }

    fn account_footer(&mut self, ui: &mut Ui) {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), Sense::click());

        // A hairline so the row reads as separate from the nav above it.
        ui.painter()
            .hline(rect.x_range(), rect.top(), Stroke::new(1.0, theme::BORDER));

        let avatar =
            Rect::from_center_size(egui::pos2(rect.left() + 28.0, rect.center().y), Vec2::splat(32.0));
        match self.account.user.as_ref().and_then(|u| u.avatar_url.clone()) {
            Some(url) => widgets::avatar_image(ui, &mut self.avatars, &url, avatar),
            None => widgets::fallback_avatar(ui, avatar, theme::ACCENT),
        }

        let name = self
            .account
            .user
            .as_ref()
            .map(|u| u.display_name.clone())
            .unwrap_or_else(|| self.t(Msg::SignedOut).to_owned());
        let subtitle = if self.account.signed_in() {
            self.t(Msg::ViewProfile).to_owned()
        } else {
            self.t(Msg::SignIn).to_owned()
        };

        let small = egui::FontId::proportional(13.0);
        ui.painter().text(
            egui::pos2(avatar.right() + 12.0, rect.center().y - 8.0),
            Align2::LEFT_CENTER,
            name,
            small.clone(),
            theme::TEXT_STRONG,
        );
        ui.painter().text(
            egui::pos2(avatar.right() + 12.0, rect.center().y + 9.0),
            Align2::LEFT_CENTER,
            subtitle,
            egui::FontId::proportional(11.5),
            theme::TEXT_FAINT,
        );

        if response.clicked() {
            self.select_tab(Tab::Account);
        }
    }

    // -------------------------------------------------------------- page

    fn page(&mut self, ui: &mut Ui) {
        let i18n = self.i18n;
        let mut pending = Pending::default();
        let mut pending_model: Option<ModelDef> = None;

        match self.tab {
            Tab::Main => view_main(ui, &self.main_data(i18n)),
            Tab::Models => match view_models(ui, &self.models_data()) {
                Some(index) => {
                    let model = self.catalog[index].clone();
                    pending_model = Some(model);
                }
                None => {}
            },
            Tab::Account => {
                let data = self.account_data(i18n, self.auth_mode, self.account_busy, self.oauth_busy);
                let outcome = view_account(
                    ui,
                    &data,
                    &mut self.avatars,
                    &mut self.username,
                    &mut self.password,
                    &mut self.show_password,
                    &mut self.hf_token_input,
                );
                pending = match outcome {
                    AccountPage::Nothing => Pending::Nothing,
                    AccountPage::Submit => {
                        let job = match self.auth_mode {
                            AuthMode::Login => AccountJob::Login {
                                username: self.username.clone(),
                                password: self.password.clone(),
                            },
                            AuthMode::Register => AccountJob::Register {
                                username: self.username.clone(),
                                password: self.password.clone(),
                            },
                        };
                        self.account_busy = true;
                        self.account_worker.send(job);
                        Pending::Nothing
                    }
                    AccountPage::SwitchMode(mode) => {
                        self.auth_mode = mode;
                        Pending::Nothing
                    }
                    AccountPage::Action(action) => Pending::Account(action),
                };
            }
            Tab::Settings => {
                let data = self.settings_data(i18n);
                let mut action = None;
                view_settings(ui, &data, &mut self.settings, &mut action);
                if let Some(action) = action {
                    pending = Pending::Toolbar(action);
                }
            }
            Tab::Plugins => {
                let data = self.plugins_data(i18n);
                pending = match view_plugins(ui, &data) {
                    Some(action) => Pending::Plugins(action),
                    None => Pending::Nothing,
                };
            }
        }

        if let Some(model) = pending_model {
            self.toggle_download(&model);
        }
        match pending {
            Pending::Nothing => {}
            Pending::Toolbar(action) => self.run_toolbar(action),
            Pending::Account(action) => self.run_account_action(action),
            Pending::Plugins(action) => self.run_plugins_action(action),
        }
    }

    // ------------------------------------------------------ page data

    fn main_data(&self, i18n: I18n) -> MainView<'static> {
        let t: Tr = I18n::t;
        MainView {
            title: t(i18n, Msg::WelcomeTitle),
            body: t(i18n, Msg::WelcomeBody),
            stamp: version::build_stamp(),
            status_label: t(i18n, Msg::Status),
            rows: self.status_rows(i18n),
        }
    }

    fn status_rows(&self, i18n: I18n) -> Vec<(String, String, bool)> {
        let t: Tr = I18n::t;
        let ready = self.catalog.iter().filter(|m| m.downloaded()).count();
        vec![
            (t(i18n, Msg::Version).to_owned(), version::VERSION.to_owned(), true),
            (t(i18n, Msg::Backend).to_owned(), account::base_url(), true),
            (
                t(i18n, Msg::PythonSidecar).to_owned(),
                self.sidecar_status_text(i18n).to_owned(),
                self.sidecar.status() == SidecarStatus::Running,
            ),
            (
                t(i18n, Msg::Models).to_owned(),
                format!("{ready} / {}", self.catalog.len()),
                ready == self.catalog.len(),
            ),
        ]
    }

    fn models_data(&self) -> ModelsView {
        ModelsView {
            models: self
                .catalog
                .iter()
                .map(|model| {
                    let state = if model.downloaded() {
                        DownloadState::Done
                    } else {
                        self.downloads.get(&model.id).cloned().unwrap_or(DownloadState::Missing)
                    };
                    let missing = model.source.missing_count(&model.id);
                    (model.clone(), state, missing)
                })
                .collect(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn account_data(
        &self,
        i18n: I18n,
        mode: AuthMode,
        busy: bool,
        oauth_busy: bool,
    ) -> AccountView {
        let t: Tr = I18n::t;
        AccountView {
            signed_in: self.account.signed_in(),
            mode,
            busy,
            oauth_busy,
            user: user_of(&self.account),
            avatar_choices: self.account.avatar_choices(),
            connections: self.account.connections.clone(),
            hf_token_stored: self.account.hf_token_stored,
            hf_token_stored_at: self.account.hf_token_stored_at.clone(),
            server: account::base_url(),
            sign_in: t(i18n, Msg::SignIn).to_owned(),
            register: t(i18n, Msg::Register).to_owned(),
            create_account: t(i18n, Msg::CreateAccount).to_owned(),
            sign_in_subtitle: t(i18n, Msg::SignInSubtitle).to_owned(),
            register_subtitle: t(i18n, Msg::RegisterSubtitle).to_owned(),
            username_label: t(i18n, Msg::Username).to_owned(),
            password_label: t(i18n, Msg::Password).to_owned(),
            show_password_label: t(i18n, Msg::ShowPassword).to_owned(),
            working_label: t(i18n, Msg::Working).to_owned(),
            account_server_label: t(i18n, Msg::AccountServer).to_owned(),
            profile_picture: t(i18n, Msg::ProfilePicture).to_owned(),
            profile_picture_hint: t(i18n, Msg::ProfilePictureHint).to_owned(),
            connections_label: t(i18n, Msg::Connections).to_owned(),
            connections_hint: t(i18n, Msg::ConnectionsHint).to_owned(),
            connect: t(i18n, Msg::Connect).to_owned(),
            disconnect: t(i18n, Msg::Disconnect).to_owned(),
            no_picture: t(i18n, Msg::NoPicture).to_owned(),
            no_avatars: t(i18n, Msg::NoConnectedAvatars).to_owned(),
            not_connected: t(i18n, Msg::NotConnected).to_owned(),
            hf_token: t(i18n, Msg::HuggingFaceToken).to_owned(),
            hf_token_hint: t(i18n, Msg::HfTokenHint).to_owned(),
            save_token: t(i18n, Msg::SaveToken).to_owned(),
            token_stored: t(i18n, Msg::TokenStored).to_owned(),
            token_stored_at: t(i18n, Msg::TokenStoredAt).to_owned(),
            token_not_stored: t(i18n, Msg::TokenNotStored).to_owned(),
            member_since: t(i18n, Msg::MemberSince).to_owned(),
            sign_out: t(i18n, Msg::SignOut).to_owned(),
        }
    }

    fn settings_data(&self, i18n: I18n) -> SettingsView {
        let t: Tr = I18n::t;
        SettingsView {
            devices: self.devices.clone(),
            input_device: self.settings.input_device.clone(),
            output_device: self.settings.output_device.clone(),
            software_language: self.settings.software_language,
            refresh: t(i18n, Msg::Refresh).to_owned(),
            system_default: t(i18n, Msg::SystemDefault).to_owned(),
            no_devices: t(i18n, Msg::NoDevices).to_owned(),
            audio_hint: t(i18n, Msg::AudioHint).to_owned(),
            language_hint: t(i18n, Msg::LanguageHint).to_owned(),
            input_label: t(i18n, Msg::InputDevice).to_owned(),
            output_label: t(i18n, Msg::OutputDevice).to_owned(),
            audio: t(i18n, Msg::AudioDevices).to_owned(),
            language: t(i18n, Msg::SoftwareLanguage).to_owned(),
            about: t(i18n, Msg::About).to_owned(),
            version_label: t(i18n, Msg::Version).to_owned(),
            build_label: t(i18n, Msg::Build).to_owned(),
            backend_label: t(i18n, Msg::Backend).to_owned(),
            data_folder_label: t(i18n, Msg::DataFolder).to_owned(),
            build: version::build_stamp(),
            data_dir: paths::data_dir().display().to_string(),
            api: account::base_url(),
        }
    }

    fn plugins_data(&self, i18n: I18n) -> PluginsView {
        let t: Tr = I18n::t;
        PluginsView {
            plugins: self.plugins.clone(),
            hint: t(i18n, Msg::PluginsHint).to_owned(),
            empty: t(i18n, Msg::NoPlugins).to_owned(),
            remove: t(i18n, Msg::Remove).to_owned(),
        }
    }

    // ------------------------------------------------------------ plumbing

    /// The persistent bar at the bottom of the Models / Settings / Plugins
    /// pages.
    fn action_bar(&mut self, ui: &mut Ui) {
        let i18n = self.i18n;
        let t: Tr = I18n::t;

        let download_missing = t(i18n, Msg::DownloadMissing).to_owned();
        let cancel = t(i18n, Msg::Cancel).to_owned();
        let refresh = t(i18n, Msg::Refresh).to_owned();
        let import = t(i18n, Msg::ImportPlugin).to_owned();
        let open_folder = t(i18n, Msg::OpenFolder).to_owned();
        let models_dir = paths::models_dir().display().to_string();
        let summary = format!(
            "{}/{} {}",
            self.catalog.iter().filter(|m| m.downloaded()).count(),
            self.catalog.len(),
            t(i18n, Msg::Models)
        );
        let any_active = self.downloads.values().any(|s| s.is_active());
        let tab = self.tab;
        let mut pressed: Option<ToolbarAction> = None;

        egui::Frame::new()
            .fill(theme::BG_CARD)
            .inner_margin(Margin::symmetric(24, 14))
            .show(ui, |ui| {
                // See `header`: the frame only spans the page if its content
                // claims the full width.
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| match tab {
                        Tab::Models => {
                            if widgets::primary_button(ui, Icon::Download, &download_missing).clicked()
                            {
                                pressed = Some(ToolbarAction::DownloadMissing);
                            }
                            ui.add_space(6.0);
                            if any_active
                                && widgets::secondary_button(ui, Icon::X, &cancel).clicked()
                            {
                                pressed = Some(ToolbarAction::Cancel);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if widgets::ghost_button_icon(
                                    ui,
                                    Icon::FolderOpen,
                                    &open_folder,
                                    &models_dir,
                                )
                                .clicked()
                                {
                                    pressed = Some(ToolbarAction::OpenModelsFolder);
                                }
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(summary.clone())
                                        .size(12.5)
                                        .color(theme::TEXT_FAINT),
                                );
                            });
                        }
                        Tab::Settings => {
                            if widgets::secondary_button(ui, Icon::RefreshCw, &refresh).clicked() {
                                pressed = Some(ToolbarAction::RefreshDevices);
                            }
                        }
                        Tab::Plugins => {
                            if widgets::primary_button(ui, Icon::FolderOpen, &import).clicked() {
                                pressed = Some(ToolbarAction::ImportPlugin);
                            }
                        }
                    _ => {}
                });
            });

        if let Some(action) = pressed {
            self.run_toolbar(action);
        }
    }

    fn toggle_download(&mut self, model: &ModelDef) {
        let active = self
            .downloads
            .get(&model.id)
            .map(|s| s.is_active())
            .unwrap_or(false);
        if active {
            self.downloader.cancel();
            return;
        }
        if model.downloaded() {
            return;
        }
        self.downloads.insert(model.id.clone(), DownloadState::Queued { position: 1 });
        self.downloader.enqueue(model);
    }

    fn run_toolbar(&mut self, action: ToolbarAction) {
        match action {
            ToolbarAction::DownloadMissing => {
                let queued = self.downloader.enqueue_missing(&self.catalog);
                let text = if queued == 0 {
                    self.t(Msg::AllModelsReady).to_owned()
                } else {
                    format!("{queued} {}", self.t(Msg::ModelsQueued))
                };
                self.toast(text, if queued == 0 { theme::SUCCESS } else { theme::ACCENT });
            }
            ToolbarAction::Cancel => self.downloader.cancel(),
            ToolbarAction::OpenModelsFolder => open_in_file_manager(&paths::models_dir()),
            ToolbarAction::RefreshDevices => self.devices_requested = true,
            ToolbarAction::ImportPlugin => self.import_plugin(),
        }
    }

    fn run_account_action(&mut self, action: AccountAction) {
        match action {
            AccountAction::SignOut => {
                self.account_busy = true;
                self.account_worker.send(AccountJob::Logout);
            }
            AccountAction::PickAvatar(url) => {
                self.account_busy = true;
                self.account_worker.send(AccountJob::PickAvatar(url));
            }
            AccountAction::SaveHfToken(value) if !value.trim().is_empty() => {
                self.account_busy = true;
                self.account_worker.send(AccountJob::SaveHfToken(value.trim().to_owned()));
            }
            AccountAction::SaveHfToken(_) => {}
            AccountAction::Connect(provider) => self.begin_oauth(provider),
            AccountAction::Disconnect(provider) => {
                self.account_busy = true;
                self.account_worker.send(AccountJob::Disconnect(provider));
            }
        }
    }

    fn run_plugins_action(&mut self, action: PluginsAction) {
        match action {
            PluginsAction::Toggle(index, enabled) => {
                if let Some(plugin) = self.plugins.get_mut(index) {
                    plugin.enabled = enabled;
                }
                self.persist_plugin_states();
            }
            PluginsAction::Remove(index) => {
                let Some(path) = self.plugins.get(index).and_then(|p| p.path.clone()) else { return };
                match plugins::remove(&path) {
                    Ok(()) => {
                        self.plugins.remove(index);
                        self.persist_plugin_states();
                        let text = self.t(Msg::PluginRemoved);
                        self.toast(text, theme::SUCCESS);
                    }
                    Err(err) => self.toast(err, theme::DANGER),
                }
            }
        }
    }

    fn import_plugin(&mut self) {
        let title = self.t(Msg::ImportPlugin).to_owned();
        let Some(folder) = rfd::FileDialog::new().set_title(&title).pick_folder() else {
            return;
        };
        match plugins::import_from(&folder) {
            Ok(path) => match plugins::read_manifest(&path) {
                Ok(manifest) => {
                    self.plugins.push(Plugin {
                        manifest,
                        origin: Origin::Imported,
                        enabled: false,
                        path: Some(path),
                    });
                    self.persist_plugin_states();
                    let text = self.t(Msg::PluginImported);
                    self.toast(text, theme::SUCCESS);
                }
                Err(err) => self.toast(err.to_string(), theme::DANGER),
            },
            Err(err) => {
                let text = format!("{}: {err}", self.t(Msg::ImportFailed));
                self.toast(text, theme::DANGER);
            }
        }
    }

    fn persist_plugin_states(&mut self) {
        self.settings.enabled_plugins =
            self.plugins.iter().map(|p| (p.key(), p.enabled)).collect();
        let _ = self.settings.save();
    }

    fn select_tab(&mut self, tab: Tab) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.settings.last_tab = Some(tab.slug().to_owned());
        let _ = self.settings.save();
    }

    /// Poll the callback listener each frame; the code-for-token exchange runs
    /// on a worker thread so the UI keeps painting.
    fn pump_oauth(&mut self) {
        if let Some(callback) = self.oauth_replies.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.oauth_replies = None;
            self.oauth_busy = false;
            match callback {
                Callback::Done { provider, profile } => {
                    self.account_busy = true;
                    self.account_worker.send(AccountJob::Connected {
                        provider,
                        username: profile.username,
                        avatar_url: profile.avatar_url,
                        access_token: profile.access_token,
                    });
                }
                Callback::Failed { provider, error } => {
                    let text = format!("{}: {error}", provider.display());
                    self.toast(text, theme::DANGER);
                }
            }
            return;
        }

        let params = match self.callback_server.as_ref().and_then(|server| server.poll()) {
            Some(params) => params,
            None => return,
        };
        let provider =
            self.callback_server.as_ref().map(|s| s.provider()).unwrap_or(Provider::Discord);
        self.callback_server = None;

        let (tx, rx) = std::sync::mpsc::channel::<Callback>();
        self.oauth_replies = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(oauth::finish(provider, &params));
        });
    }

    fn begin_oauth(&mut self, provider: Provider) {
        let url = match oauth::authorize_url(provider) {
            Ok(url) => url,
            Err(message) => {
                self.toast(message, theme::WARNING);
                return;
            }
        };
        let server = match CallbackServer::bind(provider) {
            Ok(server) => server,
            Err(message) => {
                self.toast(message, theme::DANGER);
                return;
            }
        };
        self.callback_server = Some(server);
        self.oauth_busy = true;
        if open_in_browser(&url).is_err() {
            let text = self.i18n.fill(Msg::OpenBrowserFailed, &[("url", &url)]);
            self.toast(text, theme::DANGER);
        }
    }

    fn pump_account(&mut self) {
        while let Some(reply) = self.account_worker.poll() {
            match reply {
                AccountReply::Refreshed(state) => {
                    self.account = state;
                    self.account_busy = false;
                }
                AccountReply::SignedIn { token, user } => {
                    self.account.token = Some(token);
                    self.account.user = Some(user);
                    self.account_busy = false;
                    self.password.clear();
                    let name = self.account.user.as_ref().map(|u| u.display_name.clone()).unwrap_or_default();
                    let text = self.i18n.fill(Msg::SignedIn, &[("name", &name)]);
                    self.toast(text, theme::SUCCESS);
                }
                AccountReply::Failed(error) => {
                    self.account_busy = false;
                    self.toast(error, theme::DANGER);
                }
            }
        }
    }

    fn pump_downloads(&mut self) {
        for event in self.downloader.poll() {
            match event {
                DownloadEvent::Started { model_id } => {
                    self.downloads.insert(
                        model_id,
                        DownloadState::Running {
                            bytes_done: 0,
                            bytes_total: 0,
                            current_file: String::new(),
                            started: Instant::now(),
                        },
                    );
                }
                DownloadEvent::Total { model_id, bytes } => {
                    if let Some(DownloadState::Running { bytes_total, .. }) =
                        self.downloads.get_mut(&model_id)
                    {
                        *bytes_total = bytes;
                    }
                }
                DownloadEvent::Progress { model_id, bytes_done, current_file } => {
                    if let Some(DownloadState::Running { bytes_done: done, current_file: file, .. }) =
                        self.downloads.get_mut(&model_id)
                    {
                        *done = bytes_done;
                        *file = current_file;
                    }
                }
                DownloadEvent::FileDone { .. } => {}
                DownloadEvent::Finished { model_id } => {
                    self.downloads.insert(model_id, DownloadState::Done);
                    let text = self.t(Msg::DownloadFinished);
                    self.toast(text, theme::SUCCESS);
                }
                DownloadEvent::Cancelled { model_id } => {
                    self.downloads.insert(model_id, DownloadState::Cancelled);
                }
                DownloadEvent::Failed { model_id, error } => {
                    self.downloads.insert(model_id, DownloadState::Failed(error));
                }
            }
        }
    }

    fn pump_sidecar(&mut self) {
        if self.sidecar.status() == SidecarStatus::Running && self.devices_requested {
            self.devices_requested = false;
            let devices = self.sidecar.audio_devices();
            if devices.inputs.is_empty() && devices.outputs.is_empty() {
                eprintln!("[settings] the sidecar reported no audio devices");
            }
            self.devices = devices;
        }
    }

    fn sidecar_status_text(&self, i18n: I18n) -> &'static str {
        let t: Tr = I18n::t;
        match self.sidecar.status() {
            SidecarStatus::Running => t(i18n, Msg::Running),
            SidecarStatus::Starting => t(i18n, Msg::Starting),
            SidecarStatus::Stopped => t(i18n, Msg::Stopped),
            SidecarStatus::Failed => t(i18n, Msg::Unavailable),
        }
    }

    fn draw_toast(&mut self, ui: &mut Ui) {
        if self.toast.as_ref().is_none_or(|t| Instant::now() >= t.until) {
            self.toast = None;
            return;
        }
        let Some(toast) = self.toast.as_ref() else { return };
        let text = toast.text.clone();
        let color = toast.color;

        egui::Area::new(egui::Id::new("projectrem_toast"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-24.0, -24.0))
            .order(egui::Order::Tooltip)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(theme::BG_CARD)
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(14, 10))
                    .stroke(Stroke::new(1.0, color))
                    .show(ui, |ui| {
                        ui.label(RichText::new(text.clone()).size(13.0).color(theme::TEXT_STRONG));
                    });
            });
    }

    /// Development-only: dump one frame so the layout can be eyeballed.
    #[cfg(feature = "screenshot-hook")]
    fn verify_screenshot(&mut self, ui: &mut Ui) {
        use std::io::Write as _;
        let Some(path) = std::env::var_os("PROJECTREM_SHOT") else { return };
        self.shot_frame += 1;
        // egui only paints when something changed, so on an idle page the frame
        // counter would stall and the capture would never happen. Ask for the
        // next frame until we have one worth shooting.
        if self.shot_frame < 20 {
            ui.ctx().request_repaint_after(Duration::from_millis(24));
        }
        // Park the cursor outside the window for the whole capture. egui keeps
        // a tooltip alive for roughly a second after the pointer leaves, so a
        // one-off move would still leave stray labels in the dump.
        // (egui 0.36 has no `ViewportCommand::WarpMouse`, so feed the event.)
        if (2..20).contains(&self.shot_frame) {
            ui.ctx().input_mut(|input| {
                input.events.push(egui::Event::PointerMoved(egui::pos2(-80.0, -80.0)));
            });
        }
        if self.shot_frame == 20 {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(())));
        }
        let shot = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = shot else { return };
        let [w, h] = image.size;
        let mut buf = Vec::new();
        write!(&mut buf, "{w} {h} ").ok();
        buf.extend_from_slice(image.as_raw());
        let _ = std::fs::write(path, buf);
        std::process::exit(0);
    }

    fn shutdown(&mut self) {
        self.sidecar.stop();
        let _ = self.settings.save();
    }
}

impl eframe::App for ProjectRemApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        self.render(ui, frame);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown();
    }
}

/// Built-in plugins plus everything imported, with their saved on/off state.
fn load_plugins_with_state(saved: &std::collections::BTreeMap<String, bool>) -> Vec<Plugin> {
    let mut list = plugins::builtin_plugins();
    list.extend(plugins::load_imported());
    for plugin in &mut list {
        if let Some(on) = saved.get(&plugin.key()) {
            plugin.enabled = *on;
        }
    }
    list
}

// ------------------------------------------------------------- view models

/// Everything a page needs, copied out of `self` before rendering.
pub struct MainView<'a> {
    title: &'a str,
    body: &'a str,
    stamp: String,
    status_label: &'a str,
    rows: Vec<(String, String, bool)>,
}

pub struct ModelsView {
    models: Vec<(ModelDef, DownloadState, usize)>,
}

pub struct AccountView {
    signed_in: bool,
    mode: AuthMode,
    busy: bool,
    oauth_busy: bool,
    user: account::User,
    avatar_choices: Vec<(Provider, String, String)>,
    connections: Vec<account::Connection>,
    hf_token_stored: bool,
    hf_token_stored_at: Option<String>,
    server: String,
    sign_in: String,
    register: String,
    create_account: String,
    sign_in_subtitle: String,
    register_subtitle: String,
    username_label: String,
    password_label: String,
    show_password_label: String,
    working_label: String,
    account_server_label: String,
    profile_picture: String,
    profile_picture_hint: String,
    connections_label: String,
    connections_hint: String,
    connect: String,
    disconnect: String,
    no_picture: String,
    no_avatars: String,
    not_connected: String,
    hf_token: String,
    hf_token_hint: String,
    save_token: String,
    token_stored: String,
    token_stored_at: String,
    token_not_stored: String,
    member_since: String,
    sign_out: String,
}

pub struct SettingsView {
    devices: AudioDevices,
    input_device: String,
    output_device: String,
    software_language: Language,
    refresh: String,
    system_default: String,
    no_devices: String,
    audio_hint: String,
    language_hint: String,
    input_label: String,
    output_label: String,
    audio: String,
    language: String,
    about: String,
    version_label: String,
    build_label: String,
    backend_label: String,
    data_folder_label: String,
    build: String,
    data_dir: String,
    api: String,
}

pub struct PluginsView {
    plugins: Vec<Plugin>,
    hint: String,
    empty: String,
    remove: String,
}

/// A plugin row was switched or removed.
enum PluginsAction {
    Toggle(usize, bool),
    Remove(usize),
}

include!("views.rs");

// ------------------------------------------------------------------- helpers

/// A full-height vertical scroll area.
fn scroll(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(ui.available_height())
        .show(ui, add);
}

fn nav_item(ui: &mut Ui, tab: Tab, selected: bool, label: &str) -> Option<Tab> {
    let color = if selected { theme::TEXT_STRONG } else { theme::TEXT_MUTED };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());

    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(8), theme::BG_SIDEBAR_ITEM_ACTIVE);
        ui.painter().rect_filled(
            Rect::from_min_size(
                egui::pos2(rect.left() + 5.0, rect.center().y - 10.0),
                Vec2::new(3.0, 20.0),
            ),
            CornerRadius::same(2),
            theme::ACCENT,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), theme::BG_SIDEBAR_ITEM_HOVER);
    }

    crate::lucide::paint(
        ui.painter(),
        tab.icon(),
        Rect::from_center_size(egui::pos2(rect.left() + 26.0, rect.center().y), Vec2::splat(18.0)),
        color,
        2.0,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 46.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        color,
    );

    let clicked = response.clicked();
    response.on_hover_text(label);
    if clicked && !selected {
        Some(tab)
    } else {
        None
    }
}

fn open_in_file_manager(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

fn open_in_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    return std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .map(|_| ());
    #[cfg(target_os = "macos")]
    return std::process::Command::new("open").arg(url).spawn().map(|_| ());
    #[cfg(all(unix, not(target_os = "macos")))]
    return std::process::Command::new("xdg-open").arg(url).spawn().map(|_| ());
}