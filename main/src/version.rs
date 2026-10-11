//! ProjectRem version information.
//!
//! `VERSION` is injected at compile time by `build.rs` from the `VERSION` file
//! in the repository root. There is exactly one place to change the version.

/// e.g. `"0.0.3"`.
pub const VERSION: &str = env!("PROJECTREM_VERSION");

/// Human readable product name.
pub const NAME: &str = "ProjectRem";

/// Stable identifier, also used as the window-title fallback.
pub const APP_ID: &str = "com.projectrem.gui";

/// `(major, minor, patch)`.
///
/// Parsed at runtime rather than in a `const`, because `str::split` and
/// `parse` are not const-stable.
pub fn numbers() -> (u32, u32, u32) {
    let parts: Vec<&str> = VERSION.split('.').collect();
    let part = |i: usize| parts.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
    (part(0), part(1), part(2))
}

/// Window title, e.g. `ProjectRem v0.0.3`.
pub fn title() -> String {
    format!("{NAME} v{VERSION}")
}

/// One-line build stamp shown in Settings -> About.
pub fn build_stamp() -> String {
    let (major, minor, patch) = numbers();
    format!("{NAME} {VERSION} (build {major}.{minor}.{patch})")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_three_numeric_parts() {
        let parts: Vec<&str> = super::VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "version must be major.minor.patch");
        for p in parts {
            assert!(p.chars().all(|c| c.is_ascii_digit()), "{p} is not numeric");
        }
    }
}