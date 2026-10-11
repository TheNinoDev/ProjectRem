//! Well-known filesystem locations used by the GUI.
//!
//! Everything user-writable lives under `%APPDATA%/ProjectRem` on Windows so
//! that an installed build in `Out/<version>/` never writes next to itself.

use std::path::PathBuf;

/// Root data directory, created on first use.
pub fn data_dir() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("ProjectRem");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

/// Downloaded model weights live here, one sub-directory per model id.
pub fn models_dir() -> PathBuf {
    let dir = data_dir().join("models");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn model_dir(model_id: &str) -> PathBuf {
    let dir = models_dir().join(model_id);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Imported plugins live here as `<name>/plugin.json`.
pub fn plugins_dir() -> PathBuf {
    let dir = data_dir().join("plugins");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Directory that contains the Python sidecar.
///
/// * release build shipped by `build.bat`: the exe and `sidecar/` sit side by
///   side inside `Out/<version>/`, so use the exe's own folder;
/// * `cargo run` from the repository: `main/target/<profile>/`, so walk up to
///   the repository root and use `<root>/sidecar`.
pub fn sidecar_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from));

    if let Some(dir) = &exe_dir {
        let candidate = dir.join("sidecar");
        if candidate.join("main.py").is_file() {
            return candidate;
        }
        // target/<profile>/exe -> walk up until we find sidecar/main.py
        let mut up = dir.as_path();
        for _ in 0..5 {
            let candidate = up.join("sidecar");
            if candidate.join("main.py").is_file() {
                return candidate;
            }
            match up.parent() {
                Some(p) => up = p,
                None => break,
            }
        }
    }

    // Last resort: the path baked in at compile time.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sidecar")
}

/// Format a byte count for the UI: `1.2 GB`, `547.3 MB`, `12.0 KB`.
pub fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{b} B")
    }
}

/// Format seconds as `1h 02m 03s` / `2m 03s` / `12s`.
pub fn human_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_formatting() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
        assert_eq!(human_bytes(1_280_000_000), "1.19 GB");
    }

    #[test]
    fn duration_formatting() {
        assert_eq!(human_duration(12), "12s");
        assert_eq!(human_duration(75), "1m 15s");
        assert_eq!(human_duration(3_723), "1h 02m 03s");
    }
}