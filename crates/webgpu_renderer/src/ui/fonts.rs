//! The C++ editor's faces: Roboto Medium for the UI, Cousine for paths and figures; licences in `assets/fonts/`.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

/// Byte for byte BeschleunigerBallett's `third_party/IMGUI/misc/fonts/Roboto-Medium.ttf` (Apache-2.0).
pub const ROBOTO_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Roboto-Medium.ttf");
/// Byte for byte BeschleunigerBallett's `third_party/IMGUI/misc/fonts/Cousine-Regular.ttf` (OFL-1.1).
pub const COUSINE_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Cousine-Regular.ttf");

/// The C++ editor's 15 px body size, in points.
pub const BODY_SIZE: f32 = 15.0;
pub const CAPTION_SCALE: f32 = 0.8;
pub const TITLE_SCALE: f32 = 1.2;
pub const FIGURE_SCALE: f32 = 1.8;

const UI_FACE: &str = "Roboto-Medium";
const MONO_FACE: &str = "Cousine-Regular";

/// Each family falls back on the other face, so a glyph one lacks still draws.
pub fn studio_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();
    for (name, bytes) in [(UI_FACE, ROBOTO_MEDIUM), (MONO_FACE, COUSINE_REGULAR)] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    let chain = |first: &str, second: &str| vec![first.to_owned(), second.to_owned()];
    fonts
        .families
        .insert(FontFamily::Proportional, chain(UI_FACE, MONO_FACE));
    fonts
        .families
        .insert(FontFamily::Monospace, chain(MONO_FACE, UI_FACE));
    fonts
}

pub fn body_font() -> FontId {
    FontId::proportional(BODY_SIZE)
}

pub fn caption_font() -> FontId {
    FontId::proportional(BODY_SIZE * CAPTION_SCALE)
}

pub fn title_font() -> FontId {
    FontId::proportional(BODY_SIZE * TITLE_SCALE)
}

pub fn figure_font() -> FontId {
    FontId::proportional(BODY_SIZE * FIGURE_SCALE)
}

pub fn mono_font() -> FontId {
    FontId::monospace(BODY_SIZE)
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn the_fonts_are_the_cpp_editors_files() {
        // Pinned, so this copy cannot drift from BeschleunigerBallett's unnoticed.
        assert_eq!(
            sha256_hex(ROBOTO_MEDIUM),
            "8559132c89ad51d8a2ba5b171887a44a7ba93776e205f553573de228e64b45f8"
        );
        assert_eq!(
            sha256_hex(COUSINE_REGULAR),
            "0d5d5eeb6a342432bd63a3c0d16e8470160e019933ee5af3e159d06d665dacce"
        );
    }

    #[test]
    fn the_licence_texts_beside_the_fonts_are_the_originals() {
        // Embedded, because the Windows arm64 lane stages no assets/ beside the test binary.
        assert_eq!(
            sha256_hex(include_bytes!("../../assets/fonts/Apache-2.0.txt")),
            "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"
        );
        assert_eq!(
            sha256_hex(include_bytes!("../../assets/fonts/OFL-1.1.txt")),
            "1d361a8f8e8ce6e68457dcd93fb56e162e6baa3bbb7e7573a290d44399f6b57e"
        );
    }

    #[test]
    fn each_family_starts_with_its_own_face() {
        let fonts = studio_fonts();
        assert_eq!(fonts.families[&FontFamily::Proportional][0], UI_FACE);
        assert_eq!(fonts.families[&FontFamily::Monospace][0], MONO_FACE);
        assert_eq!(fonts.font_data.len(), 2);
    }
}
