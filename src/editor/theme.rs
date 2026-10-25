//! Every color the UI uses, by role. Customization later = replace these with values loaded from config.
//! Or if you're looking at this right now, change these before building.
use crossterm::style::Color;

const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb { r, g, b } }

pub const FLASH_BG: Color = rgb(0x3f, 0x5a, 0x8a);
pub const FLASH_SOFT_BG: Color = rgb(0x32, 0x48, 0x6f);

// Surfaces (navy-tinted charcoal)
pub const SURFACE: Color = rgb(0x1a, 0x1e, 0x27);        // status bar, header
pub const SURFACE_DEEP: Color = rgb(0x14, 0x17, 0x1e);   // buffer bar
pub const SURFACE_RAISED: Color = rgb(0x1c, 0x22, 0x30); // prompt box
pub const SURFACE_ACTIVE: Color = rgb(0x2a, 0x3b, 0x59); // focused / active
pub const SURFACE_FILTER: Color = rgb(0x24, 0x30, 0x47);
pub const SURFACE_FOLD: Color = rgb(0x1f, 0x29, 0x3d);
pub const SCOPE_BG: Color = rgb(0x1f, 0x29, 0x3d);
pub const SCOPE_CURSOR_BG: Color = rgb(0x2b, 0x34, 0x47);
pub const CURSOR_LINE: Color = rgb(0x1e, 0x24, 0x33);    // subtle, navy-tinted

// Text
pub const TEXT: Color = rgb(0xc5, 0xcf, 0xdb);
pub const TEXT_BRIGHT: Color = rgb(0xe6, 0xed, 0xf3);
pub const TEXT_SOFT: Color = rgb(0x9a, 0xa8, 0xc0);
pub const TEXT_MUTED: Color = rgb(0x8f, 0x9b, 0xb0);
pub const TEXT_DIM: Color = rgb(0x63, 0x75, 0x8a);
pub const ON_ACCENT: Color = rgb(0x11, 0x15, 0x1c);      // text on colored badges
pub const BADGE_NEUTRAL: Color = rgb(0x8b, 0x92, 0x9e);

// Lines
pub const LINE_DIM: Color = rgb(0x2e, 0x35, 0x42);
pub const SEPARATOR: Color = rgb(0x3a, 0x45, 0x56);
pub const GUIDE: Color = rgb(0x41, 0x4d, 0x60);
pub const GUTTER: Color = rgb(0x45, 0x59, 0x73);

// Accents
pub const BLUE: Color = rgb(0x82, 0xaa, 0xff);
pub const BLUE_SOFT: Color = rgb(0xb4, 0xcc, 0xff);
pub const BLUE_MUTED: Color = rgb(0x5c, 0x70, 0x99);
pub const PURPLE: Color = rgb(0xc7, 0x92, 0xea);
pub const GREEN: Color = rgb(0xc3, 0xe8, 0x8d);
pub const AMBER: Color = rgb(0xf6, 0xc8, 0x5f);          // warnings only
pub const RED: Color = rgb(0xf0, 0x71, 0x78);            // errors only
pub const MODIFIED: Color = PURPLE;                      // "unsaved" marker

// Syntax
pub const SYN_NUMBER: Color = rgb(0xf2, 0x8f, 0x79);
pub const SYN_MODULE: Color = rgb(0x89, 0xdd, 0xff);
pub const SYN_ATTRIBUTE: Color = rgb(0x7f, 0xdb, 0xca);
pub const SYN_TYPE: Color = rgb(0xa6, 0xe3, 0xa1);
pub const SYN_PROPERTY: Color = rgb(0x8f, 0xb8, 0xd8);
pub const SYN_PARAMETER: Color = rgb(0xe5, 0xc0, 0x7b);
pub const SYN_PUNCT: Color = rgb(0x71, 0x80, 0x96);
pub const SYN_CHAR: Color = rgb(0xf2, 0xcc, 0x60);
pub const SEL_BG: Color = rgb(0x26, 0x3b, 0x59);
pub const MATCH_BG: Color = rgb(0x26, 0x32, 0x38);
pub const MATCH_SELECTED_BG: Color = rgb(0x5a, 0x4a, 0x26);
pub const MATCH_SELECTED_FG: Color = rgb(0xf5, 0xf5, 0xf5);

pub fn mode_color(mode: &str) -> Color {
    match mode {
        "INPUT" => GREEN,
        "MARK" => PURPLE,
        "HELP" | "BROWSE" => BADGE_NEUTRAL,
        "REPLACE" => AMBER,
        _ => BLUE, // CONTROL
    }
}

#[allow(clippy::as_conversions, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb { r: r1, g: g1, b: b1 }, Color::Rgb { r: r2, g: g2, b: b2 }) => {
            let t = t.clamp(0.0, 1.0);
            let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
            Color::Rgb { r: l(r1, r2), g: l(g1, g2), b: l(b1, b2) }
        }
        _ => b,
    }
}