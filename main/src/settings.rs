/// Persisted user settings (`%APPDATA%/ProjectRem/settings.json`).

use std::collections::BTreeMap;

use crate::i18n::Language;
use crate::paths;
use serde::{Deserialize, Serialize};

/// Special value meaning "let the OS decide".
pub const SYSTEM_DEFAULT: &str = "system-default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Microphone used for speech-to-text.
    pub input_device: String,

    /// Speaker / headphones used for text-to-speech.
    pub output_device: String,

    /// Language of the ProjectRem interface itself.
    pub software_language: Language,

    /// Language the synthesised voice should use.
    /// Selected in the Models tab, stored here, not used for synthesis yet.
    pub voice_language: Language,

    /// Which tab was open when the app was last closed.
    pub last_tab: Option<String>,

    /// Plugin on/off state, keyed by `name@version`.
    ///
    /// This lives here rather than in a separate file so that a single
    /// `Settings::save` can never drop it.
    pub enabled_plugins: BTreeMap<String, bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            input_device: SYSTEM_DEFAULT.to_owned(),
            output_device: SYSTEM_DEFAULT.to_owned(),
            software_language: Language::English,
            voice_language: Language::English,
            last_tab: None,
            enabled_plugins: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Load from disk, falling back to defaults when the file is missing or
    /// unreadable. A broken settings file never stops the app from starting.
    pub fn load() -> Self {
        let path = paths::settings_file();
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(&raw) {
            Ok(settings) => settings,
            Err(err) => {
                eprintln!("[settings] {} is invalid ({err}), using defaults", path.display());
                Self::default()
            }
        }
    }

    /// Write to disk. Returns `Err` with a human-readable reason on failure.
    pub fn save(&self) -> Result<(), String> {
        let path = paths::settings_file();
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("could not serialize settings: {e}"))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_use_system_default_devices() {
        let s = Settings::default();
        assert_eq!(s.input_device, SYSTEM_DEFAULT);
        assert_eq!(s.output_device, SYSTEM_DEFAULT);
        assert_eq!(s.software_language, Language::English);
        assert_eq!(s.voice_language, Language::English);
    }

    #[test]
    fn round_trips_through_json() {
        let mut s = Settings::default();
        s.input_device = "Headset Microphone".into();
        s.software_language = Language::Japanese;
        s.enabled_plugins.insert("Themes@1.0.0".into(), true);
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn plugin_state_survives_a_save() {
        let mut s = Settings::default();
        s.enabled_plugins.insert("Memory@0.1.0".into(), true);
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.enabled_plugins.get("Memory@0.1.0"), Some(&true));
    }
}