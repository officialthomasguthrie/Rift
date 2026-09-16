//! The shell's colours: one table for the dark theme and one for the light one, neutral grays
//! with one blue accent. `~/.config/rift/theme` picks which is in use until Settings writes it.

use iced::Color;

/// Every colour the bar, the dock and the menus draw with.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// The bar and the dock.
    pub bar: Color,
    /// The hairline along their edge.
    pub line: Color,
    /// A menu, a notification or a key popup.
    pub menu: Color,
    /// The border around one of those.
    pub edge: Color,
    /// Text and symbolic icons.
    pub text: Color,
    /// Text that says less: a hint, a body line, an icon that is off.
    pub dim: Color,
    /// Under the pointer.
    pub hover: Color,
    /// Held down, or open, or active.
    pub press: Color,
    /// The inside of the field.
    pub field: Color,
    /// Focus, selection, the active item.
    pub accent: Color,
    /// Text on a selected row.
    pub selected: Color,
    /// What went wrong.
    pub error: Color,
    /// What worked.
    pub ok: Color,
    /// What is waiting.
    pub warn: Color,
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

/// Neutral dark grays with the blue accent for dark screens. The menu is the lightest gray the
/// design rules allow, so it reads against the desktop; the field in it is sunken to the bar's own
/// gray, the way a dark entry sits in a dark surface.
pub const DARK: Palette = Palette {
    bar: rgb(0x1e_1e1e),
    line: rgb(0x14_1414),
    menu: rgb(0x2e_2e2e),
    edge: rgb(0x3c_3c3c),
    text: rgb(0xe6_e6e6),
    dim: rgb(0x8c_8c8c),
    hover: rgb(0x2a_2a2a),
    press: rgb(0x3c_3c3c),
    field: rgb(0x1e_1e1e),
    accent: rgb(0x78_aeed),
    selected: rgb(0x1e_1e1e),
    error: rgb(0xe0_6d6d),
    ok: rgb(0x66_b366),
    warn: rgb(0xd9_b34d),
};

/// White and light gray surfaces with the darker blue accent.
pub const LIGHT: Palette = Palette {
    bar: rgb(0xeb_ebeb),
    line: rgb(0xd0_d0d0),
    menu: rgb(0xfa_fafa),
    edge: rgb(0xd0_d0d0),
    text: rgb(0x1f_1f1f),
    dim: rgb(0x6b_6b6b),
    hover: rgb(0xdf_dfdf),
    press: rgb(0xd0_d0d0),
    field: rgb(0xff_ffff),
    accent: rgb(0x35_84e4),
    selected: rgb(0xff_ffff),
    error: rgb(0xc0_1c28),
    ok: rgb(0x26_a269),
    warn: rgb(0xc8_8800),
};

/// The palette the session runs with. `~/.config/rift/theme` holds `light` or `dark`; anything
/// else, or no file, is dark.
#[must_use]
pub fn load() -> Palette {
    match setting().as_deref() {
        Some("light") => LIGHT,
        _ => DARK,
    }
}

fn setting() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home).join(".config/rift/theme");
    let text = std::fs::read_to_string(path).ok()?;
    Some(text.trim().to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hex_table_reads_back() {
        assert!((DARK.bar.r - 30.0 / 255.0).abs() < f32::EPSILON);
        assert!((DARK.accent.b - 237.0 / 255.0).abs() < f32::EPSILON);
        assert!((LIGHT.field.r - 1.0).abs() < f32::EPSILON);
        assert!((LIGHT.text.g - 31.0 / 255.0).abs() < f32::EPSILON);
    }

    /// The boot test tells the bar, the menu, the field and the desktop apart by their gray, so
    /// no two of them may be within a few steps of each other. The desktop's is in
    /// nix/modules/horizon.nix.
    #[test]
    fn the_grays_of_the_dark_theme_are_far_enough_apart() {
        let desktop = rgb(0x24_2424);
        let grays = [
            ("desktop", desktop),
            ("bar", DARK.bar),
            ("line", DARK.line),
            ("menu", DARK.menu),
            ("edge", DARK.edge),
        ];
        for (name, one) in grays {
            for (other_name, other) in grays {
                if name == other_name {
                    continue;
                }
                let apart = (f64::from(one.r) - f64::from(other.r)).abs() * 255.0;
                assert!(apart >= 6.0, "{name} and {other_name} are {apart:.1} apart");
            }
        }
        // the field is the bar's gray on purpose, and they never share a row
        assert!((DARK.field.r - DARK.bar.r).abs() < f32::EPSILON);
    }

    #[test]
    fn dark_is_dark_and_light_is_light() {
        // the accent is the only colour in either, and each theme's text reads on its bar
        for palette in [DARK, LIGHT] {
            assert!(
                (palette.bar.r - palette.text.r).abs() > 0.5,
                "too little contrast"
            );
            for gray in [
                palette.bar,
                palette.line,
                palette.menu,
                palette.edge,
                palette.text,
                palette.dim,
                palette.hover,
                palette.press,
                palette.field,
            ] {
                assert!(
                    (gray.r - gray.g).abs() < f32::EPSILON,
                    "{gray:?} has a tint"
                );
                assert!(
                    (gray.g - gray.b).abs() < f32::EPSILON,
                    "{gray:?} has a tint"
                );
            }
        }
    }
}
