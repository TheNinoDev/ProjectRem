// Reads the single source of truth for the ProjectRem version (the `VERSION`
// file in the repository root) and bakes it into the binary.
//
// Doing it here means:
//   * `cargo run`, `cargo build` and `build.bat` can never disagree,
//   * the version never has to be duplicated in Cargo.toml, the window title,
//     the settings file, the About panel or the Python sidecar.

use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let version_file = Path::new(&manifest_dir).join("..").join("VERSION");

    println!("cargo:rerun-if-changed={}", version_file.display());

    let raw = std::fs::read_to_string(&version_file).unwrap_or_else(|err| {
        panic!(
            "could not read {}: {err}. The VERSION file is the single source of \
             truth for the ProjectRem version.",
            version_file.display()
        )
    });

    let version = raw.trim();
    assert_valid(version);

    println!("cargo:rustc-env=PROJECTREM_VERSION={version}");
}

/// `MAJOR.MINOR.PATCH`, numeric parts only.
fn assert_valid(version: &str) {
    let parts: Vec<&str> = version.split('.').collect();
    assert!(
        parts.len() == 3,
        "VERSION must look like 0.0.0 (major.minor.patch), got {version:?}"
    );
    for part in parts {
        assert!(
            !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()),
            "VERSION part {part:?} is not a number, full value: {version:?}"
        );
    }
}