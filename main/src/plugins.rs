//! Plugins.
//!
//! Three plugins ship as mocks so the tab has something to show. Real plugins
//! are imported by picking a folder that contains a `plugin.json`:
//!
//! ```json
//! { "name": "My Plugin", "version": "0.1.0", "author": "someone",
//!   "description": "What it does" }
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;

pub const MANIFEST_FILE: &str = "plugin.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
}

fn default_version() -> String {
    "0.0.0".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub manifest: Manifest,
    /// Where the plugin came from.
    pub origin: Origin,
    /// Runtime on/off switch, persisted in settings.
    pub enabled: bool,
    /// Folder the plugin lives in (imported plugins only).
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// One of the three plugins bundled with the GUI.
    BuiltIn,
    /// Imported by the user.
    Imported,
}

/// The three mock plugins that always exist.
pub fn builtin_plugins() -> Vec<Plugin> {
    vec![
        Plugin {
            manifest: Manifest {
                name: "Long-term memory".into(),
                version: "0.1.0".into(),
                author: "ProjectRem".into(),
                description: "Keeps conversation notes between sessions.".into(),
            },
            origin: Origin::BuiltIn,
            enabled: true,
            path: None,
        },
        Plugin {
            manifest: Manifest {
                name: "TTS voice presets".into(),
                version: "0.2.0".into(),
                author: "ProjectRem".into(),
                description: "Named voice presets for the text-to-speech model.".into(),
            },
            origin: Origin::BuiltIn,
            enabled: false,
            path: None,
        },
        Plugin {
            manifest: Manifest {
                name: "Whisper VAD".into(),
                version: "1.0.0".into(),
                author: "ProjectRem".into(),
                description: "Voice activity detection to skip silence.".into(),
            },
            origin: Origin::BuiltIn,
            enabled: true,
            path: None,
        },
    ]
}

impl Plugin {
    /// Stable key used to persist the enabled flag.
    pub fn key(&self) -> String {
        format!("{}@{}", self.manifest.name, self.manifest.version)
    }
}

#[derive(Debug)]
pub enum ImportError {
    NoManifest(PathBuf),
    BadManifest(PathBuf, String),
    AlreadyExists(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::NoManifest(path) => {
                write!(f, "no {MANIFEST_FILE} in {}", path.display())
            }
            ImportError::BadManifest(path, err) => {
                write!(f, "{} is not valid: {err}", path.display())
            }
            ImportError::AlreadyExists(name) => {
                write!(f, "a plugin called \"{name}\" is already installed")
            }
        }
    }
}

/// Read a `plugin.json` from a folder.
pub fn read_manifest(folder: &Path) -> Result<Manifest, ImportError> {
    let manifest_path = folder.join(MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Err(ImportError::NoManifest(folder.to_path_buf()));
    }
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| ImportError::BadManifest(manifest_path.clone(), e.to_string()))?;
    let manifest: Manifest = serde_json::from_str(&raw)
        .map_err(|e| ImportError::BadManifest(manifest_path.clone(), e.to_string()))?;
    if manifest.name.trim().is_empty() {
        return Err(ImportError::BadManifest(manifest_path, "\"name\" must not be empty".into()));
    }
    Ok(manifest)
}

/// Copy a plugin folder into the plugins directory.
pub fn import_from(folder: &Path) -> Result<PathBuf, ImportError> {
    let manifest = read_manifest(folder)?;
    let target = paths::plugins_dir().join(sanitize(&manifest.name));

    if target.exists() {
        return Err(ImportError::AlreadyExists(manifest.name));
    }
    if let Err(err) = copy_dir(folder, &target) {
        return Err(ImportError::BadManifest(folder.to_path_buf(), err));
    }
    Ok(target)
}

pub fn remove(path: &Path) -> Result<(), String> {
    if !path.starts_with(paths::plugins_dir()) {
        return Err("refusing to remove a folder outside the plugins directory".into());
    }
    std::fs::remove_dir_all(path).map_err(|e| format!("could not remove {}: {e}", path.display()))
}

/// Load every imported plugin from the plugins directory.
pub fn load_imported() -> Vec<Plugin> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(paths::plugins_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Ok(manifest) = read_manifest(&path) {
            out.push(Plugin { manifest, origin: Origin::Imported, enabled: false, path: Some(path) });
        }
    }
    out.sort_by(|a, b| a.manifest.name.to_lowercase().cmp(&b.manifest.name.to_lowercase()));
    out
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_owned();
    if trimmed.is_empty() { "plugin".to_owned() } else { trimmed }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let entries = std::fs::read_dir(from).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_builtin_plugins() {
        let plugins = builtin_plugins();
        assert_eq!(plugins.len(), 3);
        assert!(plugins.iter().all(|p| p.origin == Origin::BuiltIn));
        assert!(plugins.iter().all(|p| !p.manifest.name.is_empty()));
    }

    #[test]
    fn names_are_sanitised_for_folders() {
        assert_eq!(sanitize("My Plugin!"), "My-Plugin");
        assert_eq!(sanitize("///"), "plugin");
    }
}