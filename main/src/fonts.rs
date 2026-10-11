//! Font setup.
//!
//! egui ships with a Latin-only font, so Japanese UI strings would render as
//! empty boxes. We add the first system font that covers CJK as a fallback for
//! both the proportional and monospace families.

use std::path::PathBuf;

/// Candidate system fonts, best first.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let windir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts = windir.join("Fonts");

    if cfg!(windows) {
        for name in [
            "YuGothM.ttc",   // Yu Gothic Medium
            "YuGothR.ttc",   // Yu Gothic Regular
            "msgothic.ttc",  // MS Gothic
            "malgun.ttf",    // Malgun Gothic
            "meiryo.ttc",    // Meiryo
            "segoeui.ttf",   // Latin fallback
        ] {
            out.push(fonts.join(name));
        }
    } else if cfg!(target_os = "macos") {
        for name in ["/System/Library/Fonts/Hiragino Sans GB.ttc", "/System/Library/Fonts/Supplemental/Arial Unicode.ttf"] {
            out.push(PathBuf::from(name));
        }
    } else {
        for name in ["/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc"] {
            out.push(PathBuf::from(name));
        }
    }
    out
}

/// Install the ProjectRem font stack.
pub fn install(ctx: &egui::Context) {
    let mut definitions = egui::FontDefinitions::default();

    for path in candidates() {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let data = egui::FontData::from_owned(bytes);
        definitions.font_data.insert("projectrem_system".to_owned(), data.into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            definitions.families.entry(family).or_default().push("projectrem_system".to_owned());
        }
        break;
    }

    ctx.set_fonts(definitions);
}