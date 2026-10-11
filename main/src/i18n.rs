//! Software language.
//!
//! Three languages: English, German and Japanese. Switching the language
//! re-renders the whole UI on the next frame.
//!
//! English is the reference table. A missing German or Japanese entry falls
//! back to English rather than rendering an empty label, and a test asserts
//! that no entry is empty.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    English,
    German,
    Japanese,
}

impl Language {
    /// Every language the UI supports. Add one here to extend it.
    pub const ALL: [Language; 3] = [Language::English, Language::German, Language::Japanese];

    /// Stable identifier, also used in the settings file.
    #[allow(dead_code, reason = "part of the public language API")]
    pub const fn code(self) -> &'static str {
        match self {
            Language::English => "eng",
            Language::German => "ger",
            Language::Japanese => "jap",
        }
    }

    /// Name shown in the language picker, always in its own language.
    pub const fn native_name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::German => "Deutsch",
            Language::Japanese => "日本語",
        }
    }

    /// Short badge next to the name.
    pub const fn flag(self) -> &'static str {
        match self {
            Language::English => "EN",
            Language::German => "DE",
            Language::Japanese => "JA",
        }
    }

    #[allow(dead_code, reason = "part of the public language API")]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "eng" | "en" => Some(Language::English),
            "ger" | "de" => Some(Language::German),
            "jap" | "ja" => Some(Language::Japanese),
            _ => None,
        }
    }
}

/// Every translatable string in the app.
///
/// Strings are enum variants rather than literals so a typo is a compile error,
/// and `{placeholders}` are substituted with [`I18n::fill`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Msg {
    AppName,

    // Navigation and page headers
    NavMain,
    NavModels,
    NavAccount,
    NavSettings,
    NavPlugins,
    PageMain,
    PageModels,
    PageAccount,
    PageSettings,
    PagePlugins,

    // Main page
    WelcomeTitle,
    WelcomeBody,
    Status,
    Version,
    Backend,
    PythonSidecar,
    Running,
    Starting,
    Stopped,
    Unavailable,

    // Models page
    Download,
    DownloadAll,
    DownloadMissing,
    Cancel,
    Downloading,
    NotDownloaded,
    Ready,
    Failed,
    Cancelled,
    Queued,
    Size,
    Files,
    Models,
    ModelsQueued,
    AllModelsReady,
    DownloadFinished,

    // Account page
    SignIn,
    Register,
    CreateAccount,
    SignInSubtitle,
    RegisterSubtitle,
    Username,
    Password,
    ShowPassword,
    Working,
    AccountServer,
    SignedIn,
    SignedOut,
    ViewProfile,
    SignOut,
    MemberSince,
    ProfilePicture,
    ProfilePictureHint,
    Connections,
    ConnectionsHint,
    Connect,
    Disconnect,
    NoConnectedAvatars,
    NotConnected,
    NoPicture,
    HuggingFaceToken,
    HfTokenHint,
    SaveToken,
    TokenStored,
    TokenStoredAt,
    TokenNotStored,

    // Settings page
    AudioDevices,
    InputDevice,
    OutputDevice,
    Refresh,
    SystemDefault,
    NoDevices,
    AudioHint,
    SoftwareLanguage,
    LanguageHint,
    About,
    Build,
    DataFolder,

    // Plugins page
    PluginsSummary,
    PluginsHint,
    NoPlugins,
    ImportPlugin,
    PluginImported,
    ImportFailed,
    PluginRemoved,
    Remove,
    OpenFolder,

    // Errors
    OpenBrowserFailed,
}

/// One language's strings, as a lookup table.
struct Table(&'static [(Msg, &'static str)]);

impl Table {
    fn get(self, msg: Msg) -> Option<&'static str> {
        self.0.iter().find(|(m, _)| *m == msg).map(|(_, s)| *s)
    }
}

const EN: Table = Table(&[
    (Msg::AppName, "ProjectRem"),
    (Msg::NavMain, "Main"),
    (Msg::NavModels, "Models"),
    (Msg::NavAccount, "Account"),
    (Msg::NavSettings, "Settings"),
    (Msg::NavPlugins, "Plugins"),
    (Msg::PageMain, "Welcome"),
    (Msg::PageModels, "Speech and language models stored on this machine"),
    (Msg::PageAccount, "Sign in, connect services and manage tokens"),
    (Msg::PageSettings, "Audio devices, language and build information"),
    (Msg::PagePlugins, "Extend ProjectRem with your own plugins"),
    (Msg::WelcomeTitle, "Welcome to ProjectRem"),
    (Msg::WelcomeBody, "A local voice assistant. Nothing has to run in the cloud."),
    (Msg::Status, "Status"),
    (Msg::Version, "Version"),
    (Msg::Backend, "Server"),
    (Msg::PythonSidecar, "Python sidecar"),
    (Msg::Running, "running"),
    (Msg::Starting, "starting"),
    (Msg::Stopped, "stopped"),
    (Msg::Unavailable, "unavailable"),
    (Msg::Download, "Download"),
    (Msg::DownloadAll, "Download all"),
    (Msg::DownloadMissing, "Download missing"),
    (Msg::Cancel, "Cancel"),
    (Msg::Downloading, "Downloading"),
    (Msg::NotDownloaded, "Not downloaded"),
    (Msg::Ready, "Ready"),
    (Msg::Failed, "Failed"),
    (Msg::Cancelled, "Cancelled"),
    (Msg::Queued, "Queued"),
    (Msg::Size, "Size"),
    (Msg::Files, "Files"),
    (Msg::Models, "Models"),
    (Msg::ModelsQueued, "models queued"),
    (Msg::AllModelsReady, "Every model is already downloaded."),
    (Msg::DownloadFinished, "Download finished."),
    (Msg::SignIn, "Sign in"),
    (Msg::Register, "Register"),
    (Msg::CreateAccount, "Create account"),
    (Msg::SignInSubtitle, "Use your ProjectRem account."),
    (Msg::RegisterSubtitle, "Pick a username and a password."),
    (Msg::Username, "Username"),
    (Msg::Password, "Password"),
    (Msg::ShowPassword, "Show password"),
    (Msg::Working, "Working"),
    (Msg::AccountServer, "Accounts are handled by"),
    (Msg::SignedIn, "Signed in as {name}"),
    (Msg::SignedOut, "Signed out"),
    (Msg::ViewProfile, "View profile"),
    (Msg::SignOut, "Sign out"),
    (Msg::MemberSince, "Member since"),
    (Msg::ProfilePicture, "Profile picture"),
    (Msg::ProfilePictureHint, "Pick the picture from a connected service, or cancel all of them."),
    (Msg::Connections, "Connections"),
    (Msg::ConnectionsHint, "Connect a service to use its picture and models."),
    (Msg::Connect, "Connect"),
    (Msg::Disconnect, "Disconnect"),
    (Msg::NoConnectedAvatars, "No connected service offers a picture yet."),
    (Msg::NotConnected, "Not connected"),
    (Msg::NoPicture, "None"),
    (Msg::HuggingFaceToken, "Hugging Face token"),
    (
        Msg::HfTokenHint,
        "Add your own token so model downloads use your quota instead of the anonymous one.",
    ),
    (Msg::SaveToken, "Save"),
    (Msg::TokenStored, "Token stored."),
    (Msg::TokenStoredAt, "Token stored on {date}."),
    (Msg::TokenNotStored, "No token stored, downloads are anonymous."),
    (Msg::AudioDevices, "Audio"),
    (Msg::InputDevice, "Input device"),
    (Msg::OutputDevice, "Output device"),
    (Msg::Refresh, "Refresh"),
    (Msg::SystemDefault, "System default"),
    (Msg::NoDevices, "No devices found"),
    (Msg::AudioHint, "Enumerated by the Python sidecar."),
    (Msg::SoftwareLanguage, "Software language"),
    (Msg::LanguageHint, "Changes the interface immediately."),
    (Msg::About, "About"),
    (Msg::Build, "Build"),
    (Msg::DataFolder, "Data folder"),
    (Msg::PluginsSummary, "{on} of {total} enabled"),
    (Msg::PluginsHint, "Import a folder that contains a plugin.json to add a plugin."),
    (Msg::NoPlugins, "No plugins installed."),
    (Msg::ImportPlugin, "Import plugin"),
    (Msg::PluginImported, "Plugin imported."),
    (Msg::ImportFailed, "Import failed"),
    (Msg::PluginRemoved, "Plugin removed."),
    (Msg::Remove, "Remove"),
    (Msg::OpenFolder, "Open folder"),
    (Msg::OpenBrowserFailed, "Could not open a browser. Visit {url}"),
]);

const DE: Table = Table(&[
    (Msg::AppName, "ProjectRem"),
    (Msg::NavMain, "Start"),
    (Msg::NavModels, "Modelle"),
    (Msg::NavAccount, "Konto"),
    (Msg::NavSettings, "Einstellungen"),
    (Msg::NavPlugins, "Plugins"),
    (Msg::PageMain, "Willkommen"),
    (Msg::PageModels, "Sprach- und Sprachmodelle auf diesem Rechner"),
    (Msg::PageAccount, "Anmelden, Dienste verbinden, Tokens verwalten"),
    (Msg::PageSettings, "Audiogeräte, Sprache und Build-Informationen"),
    (Msg::PagePlugins, "ProjectRem mit eigenen Plugins erweitern"),
    (Msg::WelcomeTitle, "Willkommen bei ProjectRem"),
    (Msg::WelcomeBody, "Ein lokaler Sprachassistent. Nichts muss in der Cloud laufen."),
    (Msg::Status, "Status"),
    (Msg::Version, "Version"),
    (Msg::Backend, "Server"),
    (Msg::PythonSidecar, "Python-Sidecar"),
    (Msg::Running, "läuft"),
    (Msg::Starting, "startet"),
    (Msg::Stopped, "gestoppt"),
    (Msg::Unavailable, "nicht verfügbar"),
    (Msg::Download, "Herunterladen"),
    (Msg::DownloadAll, "Alle herunterladen"),
    (Msg::DownloadMissing, "Fehlende herunterladen"),
    (Msg::Cancel, "Abbrechen"),
    (Msg::Downloading, "Lädt herunter"),
    (Msg::NotDownloaded, "Nicht heruntergeladen"),
    (Msg::Ready, "Bereit"),
    (Msg::Failed, "Fehlgeschlagen"),
    (Msg::Cancelled, "Abgebrochen"),
    (Msg::Queued, "In Warteschlange"),
    (Msg::Size, "Größe"),
    (Msg::Files, "Dateien"),
    (Msg::Models, "Modelle"),
    (Msg::ModelsQueued, "Modelle eingereiht"),
    (Msg::AllModelsReady, "Alle Modelle sind bereits heruntergeladen."),
    (Msg::DownloadFinished, "Download abgeschlossen."),
    (Msg::SignIn, "Anmelden"),
    (Msg::Register, "Registrieren"),
    (Msg::CreateAccount, "Konto erstellen"),
    (Msg::SignInSubtitle, "Mit deinem ProjectRem-Konto anmelden."),
    (Msg::RegisterSubtitle, "Wähle Benutzername und Passwort."),
    (Msg::Username, "Benutzername"),
    (Msg::Password, "Passwort"),
    (Msg::ShowPassword, "Passwort anzeigen"),
    (Msg::Working, "Läuft"),
    (Msg::AccountServer, "Konten werden verwaltet von"),
    (Msg::SignedIn, "Angemeldet als {name}"),
    (Msg::SignedOut, "Abgemeldet"),
    (Msg::ViewProfile, "Profil ansehen"),
    (Msg::SignOut, "Abmelden"),
    (Msg::MemberSince, "Dabei seit"),
    (Msg::ProfilePicture, "Profilbild"),
    (
        Msg::ProfilePictureHint,
        "Wähle das Bild eines verbundenen Dienstes oder alle abbrechen.",
    ),
    (Msg::Connections, "Verbindungen"),
    (Msg::ConnectionsHint, "Verbinde einen Dienst für Bild und Modelle."),
    (Msg::Connect, "Verbinden"),
    (Msg::Disconnect, "Trennen"),
    (Msg::NoConnectedAvatars, "Kein verbundener Dienst bietet ein Bild an."),
    (Msg::NotConnected, "Nicht verbunden"),
    (Msg::NoPicture, "Keins"),
    (Msg::HuggingFaceToken, "Hugging-Face-Token"),
    (
        Msg::HfTokenHint,
        "Eigenes Token hinterlegen, damit Downloads dein Kontingent nutzen.",
    ),
    (Msg::SaveToken, "Speichern"),
    (Msg::TokenStored, "Token gespeichert."),
    (Msg::TokenStoredAt, "Token gespeichert am {date}."),
    (Msg::TokenNotStored, "Kein Token gespeichert, Downloads sind anonym."),
    (Msg::AudioDevices, "Audio"),
    (Msg::InputDevice, "Eingabegerät"),
    (Msg::OutputDevice, "Ausgabegerät"),
    (Msg::Refresh, "Aktualisieren"),
    (Msg::SystemDefault, "Systemstandard"),
    (Msg::NoDevices, "Keine Geräte gefunden"),
    (Msg::AudioHint, "Ermittelt vom Python-Sidecar."),
    (Msg::SoftwareLanguage, "Softwaresprache"),
    (Msg::LanguageHint, "Ändert die Oberfläche sofort."),
    (Msg::About, "Über"),
    (Msg::Build, "Build"),
    (Msg::DataFolder, "Datenordner"),
    (Msg::PluginsHint, "Importiere einen Ordner mit plugin.json, um ein Plugin hinzuzufügen."),
    (Msg::NoPlugins, "Keine Plugins installiert."),
    (Msg::ImportPlugin, "Plugin importieren"),
    (Msg::PluginImported, "Plugin importiert."),
    (Msg::ImportFailed, "Import fehlgeschlagen"),
    (Msg::PluginRemoved, "Plugin entfernt."),
    (Msg::Remove, "Entfernen"),
    (Msg::OpenFolder, "Ordner öffnen"),
    (Msg::OpenBrowserFailed, "Browser konnte nicht geöffnet werden. Öffne {url}"),
    (Msg::PluginsSummary, "{on} von {total} aktiviert"),
]);

const JA: Table = Table(&[
    (Msg::AppName, "ProjectRem"),
    (Msg::NavMain, "メイン"),
    (Msg::NavModels, "モデル"),
    (Msg::NavAccount, "アカウント"),
    (Msg::NavSettings, "設定"),
    (Msg::NavPlugins, "プラグイン"),
    (Msg::PageMain, "ようこそ"),
    (Msg::PageModels, "このマシンに保存される音声・言語モデル"),
    (Msg::PageAccount, "サインイン、サービスの接続、トークン管理"),
    (Msg::PageSettings, "オーディオデバイス、言語、ビルド情報"),
    (Msg::PagePlugins, "プラグインで ProjectRem を拡張"),
    (Msg::WelcomeTitle, "ProjectRem へようこそ"),
    (Msg::WelcomeBody, "ローカルの音声アシスタントです。クラウドは不要です。"),
    (Msg::Status, "状態"),
    (Msg::Version, "バージョン"),
    (Msg::Backend, "サーバー"),
    (Msg::PythonSidecar, "Python サイドカー"),
    (Msg::Running, "稼働中"),
    (Msg::Starting, "起動中"),
    (Msg::Stopped, "停止中"),
    (Msg::Unavailable, "利用不可"),
    (Msg::Download, "ダウンロード"),
    (Msg::DownloadAll, "すべてダウンロード"),
    (Msg::DownloadMissing, "不足分をダウンロード"),
    (Msg::Cancel, "キャンセル"),
    (Msg::Downloading, "ダウンロード中"),
    (Msg::NotDownloaded, "未ダウンロード"),
    (Msg::Ready, "準備完了"),
    (Msg::Failed, "失敗"),
    (Msg::Cancelled, "キャンセル"),
    (Msg::Queued, "待機中"),
    (Msg::Size, "サイズ"),
    (Msg::Files, "ファイル"),
    (Msg::Models, "モデル"),
    (Msg::ModelsQueued, "モデルを予約しました"),
    (Msg::AllModelsReady, "すべてのモデルはすでにダウンロード済みです。"),
    (Msg::DownloadFinished, "ダウンロードが完了しました。"),
    (Msg::SignIn, "ログイン"),
    (Msg::Register, "新規登録"),
    (Msg::CreateAccount, "アカウント作成"),
    (Msg::SignInSubtitle, "ProjectRem アカウントでログインします。"),
    (Msg::RegisterSubtitle, "ユーザー名とパスワードを決めてください。"),
    (Msg::Username, "ユーザー名"),
    (Msg::Password, "パスワード"),
    (Msg::ShowPassword, "パスワードを表示"),
    (Msg::Working, "処理中"),
    (Msg::AccountServer, "アカウントのサーバー"),
    (Msg::SignedIn, "{name} としてログイン中"),
    (Msg::SignedOut, "ログアウト"),
    (Msg::ViewProfile, "プロフィール"),
    (Msg::SignOut, "ログアウト"),
    (Msg::MemberSince, "登録日"),
    (Msg::ProfilePicture, "プロフィール画像"),
    (Msg::ProfilePictureHint, "接続中のサービスから画像を選ぶか、すべて解除します。"),
    (Msg::Connections, "接続"),
    (Msg::ConnectionsHint, "画像をモデルに利用できるよう、サービスを接続します。"),
    (Msg::Connect, "接続"),
    (Msg::Disconnect, "解除"),
    (Msg::NoConnectedAvatars, "画像を提供している接続済みサービスがありません。"),
    (Msg::NotConnected, "未接続"),
    (Msg::NoPicture, "なし"),
    (Msg::HuggingFaceToken, "Hugging Face トークン"),
    (
        Msg::HfTokenHint,
        "自分のトークンを保存すると、匿名の枠ではなく自分の枠でダウンロードできます。",
    ),
    (Msg::SaveToken, "保存"),
    (Msg::TokenStored, "トークンは保存済みです。"),
    (Msg::TokenStoredAt, "トークンは {date} に保存されました。"),
    (Msg::TokenNotStored, "トークンなし。ダウンロードは匿名で行われます。"),
    (Msg::AudioDevices, "オーディオ"),
    (Msg::InputDevice, "入力デバイス"),
    (Msg::OutputDevice, "出力デバイス"),
    (Msg::Refresh, "更新"),
    (Msg::SystemDefault, "システム既定"),
    (Msg::NoDevices, "デバイスが見つかりません"),
    (Msg::AudioHint, "Python サイドカーが取得します。"),
    (Msg::SoftwareLanguage, "ソフトウェア言語"),
    (Msg::LanguageHint, "すぐに表示が切り替わります。"),
    (Msg::About, "情報"),
    (Msg::Build, "ビルド"),
    (Msg::DataFolder, "データフォルダ"),
    (Msg::PluginsSummary, "{total} 件中 {on} 件が有効"),
    (Msg::PluginsHint, "plugin.json を含むフォルダをインポートすると追加できます。"),
    (Msg::NoPlugins, "インストール済みプラグインはありません。"),
    (Msg::ImportPlugin, "プラグインをインポート"),
    (Msg::PluginImported, "プラグインをインポートしました。"),
    (Msg::ImportFailed, "インポート失敗"),
    (Msg::PluginRemoved, "プラグインを削除しました。"),
    (Msg::Remove, "削除"),
    (Msg::OpenFolder, "フォルダを開く"),
    (Msg::OpenBrowserFailed, "ブラウザを開けませんでした。{url} を開いてください"),
]);

impl Msg {
    /// The text in `language`, falling back to English when a translation is
    /// missing so the UI never shows an empty label.
    pub fn text(self, language: Language) -> &'static str {
        let table = match language {
            Language::English => EN,
            Language::German => DE,
            Language::Japanese => JA,
        };
        table.get(self).or_else(|| EN.get(self)).unwrap_or("")
    }
}

/// Translation handle. Cheap to copy, holds nothing but the language.
#[derive(Debug, Clone, Copy, Default)]
pub struct I18n {
    pub language: Language,
}

impl I18n {
    pub fn new(language: Language) -> Self {
        Self { language }
    }

    pub fn t(self, msg: Msg) -> &'static str {
        msg.text(self.language)
    }

    /// Substitute `{placeholders}` in a translated string.
    pub fn fill(self, msg: Msg, pairs: &[(&str, &str)]) -> String {
        let mut text = msg.text(self.language).to_owned();
        for (key, value) in pairs {
            text = text.replace(&format!("{{{key}}}"), value);
        }
        text
    }
}

/// Every variant, so the tests can iterate the whole table.
#[cfg(test)]
const ALL_MESSAGES: &[Msg] = &[
    Msg::AppName,
    Msg::NavMain,
    Msg::NavModels,
    Msg::NavAccount,
    Msg::NavSettings,
    Msg::NavPlugins,
    Msg::PageMain,
    Msg::PageModels,
    Msg::PageAccount,
    Msg::PageSettings,
    Msg::PagePlugins,
    Msg::WelcomeTitle,
    Msg::WelcomeBody,
    Msg::Status,
    Msg::Version,
    Msg::Backend,
    Msg::PythonSidecar,
    Msg::Running,
    Msg::Starting,
    Msg::Stopped,
    Msg::Unavailable,
    Msg::Download,
    Msg::DownloadAll,
    Msg::DownloadMissing,
    Msg::Cancel,
    Msg::Downloading,
    Msg::NotDownloaded,
    Msg::Ready,
    Msg::Failed,
    Msg::Cancelled,
    Msg::Queued,
    Msg::Size,
    Msg::Files,
    Msg::Models,
    Msg::ModelsQueued,
    Msg::AllModelsReady,
    Msg::DownloadFinished,
    Msg::SignIn,
    Msg::Register,
    Msg::CreateAccount,
    Msg::SignInSubtitle,
    Msg::RegisterSubtitle,
    Msg::Username,
    Msg::Password,
    Msg::ShowPassword,
    Msg::Working,
    Msg::AccountServer,
    Msg::SignedIn,
    Msg::SignedOut,
    Msg::ViewProfile,
    Msg::SignOut,
    Msg::MemberSince,
    Msg::ProfilePicture,
    Msg::ProfilePictureHint,
    Msg::Connections,
    Msg::ConnectionsHint,
    Msg::Connect,
    Msg::Disconnect,
    Msg::NoConnectedAvatars,
    Msg::NotConnected,
    Msg::NoPicture,
    Msg::HuggingFaceToken,
    Msg::HfTokenHint,
    Msg::SaveToken,
    Msg::TokenStored,
    Msg::TokenStoredAt,
    Msg::TokenNotStored,
    Msg::AudioDevices,
    Msg::InputDevice,
    Msg::OutputDevice,
    Msg::Refresh,
    Msg::SystemDefault,
    Msg::NoDevices,
    Msg::AudioHint,
    Msg::SoftwareLanguage,
    Msg::LanguageHint,
    Msg::About,
    Msg::Build,
    Msg::DataFolder,
    Msg::PluginsSummary,
    Msg::PluginsHint,
    Msg::NoPlugins,
    Msg::ImportPlugin,
    Msg::PluginImported,
    Msg::ImportFailed,
    Msg::PluginRemoved,
    Msg::Remove,
    Msg::OpenFolder,
    Msg::OpenBrowserFailed,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_a_name() {
        for lang in Language::ALL {
            assert!(!lang.native_name().is_empty());
            assert_eq!(Language::from_code(lang.code()), Some(lang));
        }
    }

    #[test]
    fn no_entry_is_empty() {
        for lang in Language::ALL {
            for msg in ALL_MESSAGES {
                assert!(!msg.text(lang).is_empty(), "{msg:?} is empty in {lang:?}");
            }
        }
    }

    #[test]
    fn tables_cover_the_same_variants() {
        // If a variant is added to the enum but not to a table, `text` would
        // silently fall back to English. Catch that here instead.
        for lang in [Language::German, Language::Japanese] {
            for msg in ALL_MESSAGES {
                let table = match lang {
                    Language::German => DE,
                    _ => JA,
                };
                assert!(table.get(*msg).is_some(), "{msg:?} is missing from the {lang:?} table");
            }
        }
    }

    #[test]
    fn no_table_has_duplicate_entries() {
        // A duplicated entry would silently shadow the first one.
        for (lang, table) in
            [(Language::English, EN), (Language::German, DE), (Language::Japanese, JA)]
        {
            let mut seen = std::collections::HashSet::new();
            for (msg, _) in table.0 {
                assert!(seen.insert(*msg), "{lang:?} lists {msg:?} twice");
            }
            assert_eq!(seen.len(), table.0.len());
        }
    }

    #[test]
    fn placeholders_are_substituted() {
        let i18n = I18n::new(Language::English);
        assert_eq!(i18n.fill(Msg::SignedIn, &[("name", "nikla")]), "Signed in as nikla");
        assert_eq!(
            i18n.fill(Msg::OpenBrowserFailed, &[("url", "https://x")]),
            "Could not open a browser. Visit https://x"
        );
    }

    #[test]
    fn placeholders_survive_translation() {
        for lang in Language::ALL {
            let text = I18n::new(lang).fill(Msg::TokenStoredAt, &[("date", "2026-10-11")]);
            assert!(!text.contains("{date}"), "{lang:?} left the placeholder in: {text}");
        }
    }

    #[test]
    fn translations_are_actually_translated() {
        // A copy-paste that silently keeps the English string is a bug users
        // notice immediately, so check the big tables really differ.
        assert_ne!(Msg::SignIn.text(Language::German), Msg::SignIn.text(Language::English));
        assert_ne!(Msg::SignIn.text(Language::Japanese), Msg::SignIn.text(Language::English));
    }
}