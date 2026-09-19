//! The app's colours: one table for the dark theme and one for the light one, neutral grays with
//! the owner's accent. They are the window colours the GTK apps of the session draw with, so
//! Settings sits beside them without standing out.

use iced::Color;
use librift::appearance::{Accent, Theme};

/// Every colour the window draws with.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    /// The header bar along the top, the title bar the app draws for itself.
    pub header: Color,
    /// The sidebar down the left.
    pub side: Color,
    /// The page beside it.
    pub page: Color,
    /// The inside of a list of rows on the page.
    pub view: Color,
    /// The hairline between them.
    pub line: Color,
    /// The border of a button, a field or a swatch.
    pub edge: Color,
    /// A button.
    pub button: Color,
    /// Under the pointer.
    pub hover: Color,
    /// Text and symbolic icons.
    pub text: Color,
    /// Text that says less: a note, a credit, a unit.
    pub dim: Color,
    /// A switch that is off, and the part of a slider past its handle.
    pub track: Color,
    /// The knob of a switch, which is the same near white in both themes, as Adwaita has it.
    pub knob: Color,
    /// Focus, selection, the chosen thing.
    pub accent: Color,
    /// Text on top of the accent.
    pub on_accent: Color,
    /// What went wrong.
    pub error: Color,
}

const fn rgb(hex: u32) -> Color {
    #[allow(clippy::cast_precision_loss)]
    Color {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// Neutral dark grays, in the order Adwaita stacks them: the title bar lightest, the sidebar a
/// step under it, the page the window gray, and a list sunk below all three. The page is two steps
/// off the desktop's own gray on purpose: the boot test finds the edges of a window by the pixels
/// that are not the desktop, the way it does for every GTK app of the image.
const DARK: Colors = Colors {
    header: rgb(0x30_3030),
    side: rgb(0x2e_2e2e),
    page: rgb(0x26_2626),
    view: rgb(0x1e_1e1e),
    line: rgb(0x18_1818),
    edge: rgb(0x4a_4a4a),
    button: rgb(0x3a_3a3a),
    hover: rgb(0x46_4646),
    text: rgb(0xe6_e6e6),
    dim: rgb(0x9a_9a9a),
    track: rgb(0x54_5454),
    knob: rgb(0xe6_e6e6),
    accent: rgb(0x78_aeed),
    on_accent: rgb(0x1e_1e1e),
    error: rgb(0xe0_6d6d),
};

/// White and light grays, the same three surfaces the other way up.
const LIGHT: Colors = Colors {
    header: rgb(0xeb_ebeb),
    side: rgb(0xf2_f1f0),
    page: rgb(0xfa_fafa),
    view: rgb(0xff_ffff),
    line: rgb(0xd0_d0d0),
    edge: rgb(0xc4_c4c4),
    button: rgb(0xff_ffff),
    hover: rgb(0xdf_dfdf),
    text: rgb(0x1f_1f1f),
    dim: rgb(0x6b_6b6b),
    track: rgb(0xc4_c4c4),
    knob: rgb(0xff_ffff),
    accent: rgb(0x35_84e4),
    on_accent: rgb(0xff_ffff),
    error: rgb(0xc0_1c28),
};

/// The colours of a theme, with the owner's accent in place of the blue.
#[must_use]
pub fn colors(theme: Theme, accent: Accent) -> Colors {
    let table = match theme {
        Theme::Dark => DARK,
        Theme::Light => LIGHT,
    };
    Colors {
        accent: hex(accent.hex(theme)),
        ..table
    }
}

/// A colour the appearance settings name, `#rrggbb`. A word that is not one is black, which no
/// accent is.
#[must_use]
pub fn hex(text: &str) -> Color {
    let digits = text.strip_prefix('#').unwrap_or(text);
    rgb(u32::from_str_radix(digits, 16).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hex_table_reads_back() {
        assert!((DARK.page.r - 38.0 / 255.0).abs() < f32::EPSILON);
        assert!((LIGHT.view.r - 1.0).abs() < f32::EPSILON);
        assert_eq!(hex("#78aeed"), DARK.accent);
        assert_eq!(hex("#3584e4"), LIGHT.accent);
    }

    /// The boot test looks for the title bar the app draws along the top of its window, so the
    /// header bar is a gray of its own, and every surface carries the text. The dark grays are
    /// neutral to the step, as the design rules ask; the light ones are the session's own
    /// `#f2f1f0` and white, a step off neutral at most.
    #[test]
    fn the_header_bar_is_told_apart_from_the_page() {
        for (table, tint) in [(DARK, 0.0), (LIGHT, 2.5 / 255.0)] {
            let apart = (f64::from(table.header.r) - f64::from(table.page.r)).abs() * 255.0;
            assert!(apart >= 8.0, "{apart:.1} apart");
            // and the page is not the desktop's own gray, which is #242424 on dark
            let desktop = (f64::from(table.page.r) * 255.0 - 36.0).abs();
            assert!(desktop >= 2.0, "the page is {desktop:.1} from the desktop");
            for surface in [table.header, table.side, table.page, table.view] {
                assert!(
                    (surface.r - table.text.r).abs() > 0.5,
                    "too little contrast"
                );
                assert!(
                    (surface.r - surface.g).abs() <= tint,
                    "{surface:?} has a tint"
                );
                assert!(
                    (surface.g - surface.b).abs() <= tint,
                    "{surface:?} has a tint"
                );
            }
        }
    }

    #[test]
    fn an_accent_takes_the_blues_place() {
        for accent in Accent::ALL {
            for theme in [Theme::Dark, Theme::Light] {
                assert_eq!(colors(theme, accent).accent, hex(accent.hex(theme)));
                assert_eq!(colors(theme, accent).text, colors(theme, Accent::Blue).text);
            }
        }
    }
}
