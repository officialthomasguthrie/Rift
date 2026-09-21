//! Keyboard layouts from a client's side: the ones there are to choose from, out of the list
//! xkeyboard-config keeps beside its rules, the ones the desktop types with, which systemd's
//! localed keeps and Horizon follows, and a new list of them handed to localed. The Keyboard page
//! asks all of it here.

use std::path::PathBuf;

#[cfg(feature = "bus")]
use crate::bus;

/// Where xkeyboard-config's data is, unless `XKB_CONFIG_ROOT` names another place, which is where
/// libxkbcommon looks too.
pub const ROOT: &str = "/etc/X11/xkb";
/// The list of layouts and variants with their names, under the root.
const LIST: &str = "rules/evdev.lst";
/// How many layouts a keyboard switches between at most: XKB has room for four groups.
pub const MOST: usize = 4;
/// And the sentence that says so.
pub const TOO_MANY: &str = "A keyboard switches between four layouts at most.";
/// The layout of a desktop where none was chosen, the one libxkbcommon falls back to.
pub const DEFAULT: &str = "us";
/// localed's one object.
#[cfg(feature = "bus")]
const OBJECT: &str = "/org/freedesktop/locale1";
/// The name a person knows it by, for the sentences.
#[cfg(feature = "bus")]
const NAME: &str = "localed";

/// One layout, or one variant of a layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The layout's name in XKB, `gb`.
    pub code: String,
    /// The variant, `dvorak`, or nothing for the layout itself.
    pub variant: String,
    /// What xkeyboard-config calls it, `English (UK)`, or its word where the list has no name.
    pub name: String,
}

impl Layout {
    /// A layout known by its code and variant alone, named by its word.
    #[must_use]
    pub fn bare(code: &str, variant: &str) -> Self {
        let mut layout = Self {
            code: code.to_string(),
            variant: variant.to_string(),
            name: String::new(),
        };
        layout.name = layout.word();
        layout
    }

    /// The word `rift-settings --set` takes and `--state` prints: `gb`, or `us(dvorak)` for a
    /// variant, the way XKB writes one.
    #[must_use]
    pub fn word(&self) -> String {
        if self.variant.is_empty() {
            self.code.clone()
        } else {
            format!("{}({})", self.code, self.variant)
        }
    }

    /// Whether this is the layout, or the variant, a word names.
    #[must_use]
    pub fn is(&self, word: &str) -> bool {
        let (code, variant) = split(word);
        self.code == code && self.variant == variant
    }

    /// Whether every word typed starts a word of its name, or is its code or its variant, whatever
    /// the case.
    #[must_use]
    pub fn matches(&self, typed: &str) -> bool {
        let known = words(&self.name);
        let typed = typed.to_lowercase();
        let mut asked = typed.split_whitespace().peekable();
        asked.peek().is_some()
            && asked.all(|word| {
                word == self.code
                    || word == self.variant
                    || known.iter().any(|known| known.starts_with(word))
            })
    }
}

/// A word of `rift-settings --set`, `us(dvorak)`, as its code and its variant.
fn split(word: &str) -> (&str, &str) {
    let word = word.trim();
    match word.split_once('(') {
        Some((code, rest)) => (code.trim(), rest.trim_end_matches(')').trim()),
        None => (word, ""),
    }
}

/// The words of a name in lower case, without the brackets and the commas between them.
fn words(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// The layouts and variants in xkeyboard-config's list, in the order of their names.
#[must_use]
pub fn parse(text: &str) -> Vec<Layout> {
    let mut section = "";
    let mut found = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix('!') {
            section = name.trim();
            continue;
        }
        let Some((first, rest)) = line.trim().split_once(char::is_whitespace) else {
            continue;
        };
        let rest = rest.trim();
        match section {
            // the placeholder xkeyboard-config keeps for a layout of the owner's own making, which
            // holds nothing until someone writes one
            "layout" if first == "custom" => {}
            "layout" => found.push(Layout {
                code: first.to_string(),
                variant: String::new(),
                name: rest.to_string(),
            }),
            "variant" => {
                if let Some((code, name)) = rest.split_once(':') {
                    found.push(Layout {
                        code: code.trim().to_string(),
                        variant: first.to_string(),
                        name: name.trim().to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    // the sort is stable, so a layout keeps its place before a variant that has the same name
    found.sort_by_cached_key(|layout| layout.name.to_lowercase());
    found
}

/// Where xkeyboard-config's list is on this system.
fn list() -> PathBuf {
    std::env::var_os("XKB_CONFIG_ROOT")
        .filter(|root| !root.is_empty())
        .map_or_else(|| PathBuf::from(ROOT), PathBuf::from)
        .join(LIST)
}

/// The layouts there are to choose from, out of the list the image carries. Nothing where it
/// cannot be read.
#[must_use]
pub fn installed() -> Vec<Layout> {
    std::fs::read_to_string(list())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// The layout or variant with this code and variant out of the list, or one named by its word for
/// a layout set somewhere else that the list does not have.
#[must_use]
pub fn named(list: &[Layout], code: &str, variant: &str) -> Layout {
    list.iter()
        .find(|layout| layout.code == code && layout.variant == variant)
        .cloned()
        .unwrap_or_else(|| Layout::bare(code, variant))
}

/// The layout or variant out of the list that a word names, `gb` or `us(dvorak)`. Nothing for a
/// word the list does not have.
#[must_use]
pub fn by_word<'a>(list: &'a [Layout], word: &str) -> Option<&'a Layout> {
    list.iter().find(|layout| layout.is(word))
}

/// The layouts a search finds, leaving out the ones already chosen: a layout whose code is what was
/// typed first, then one with a word of its name that is, then one whose name starts with it, then
/// the rest. Nothing when nothing was typed.
#[must_use]
pub fn find<'a>(list: &'a [Layout], typed: &str, chosen: &[Layout]) -> Vec<&'a Layout> {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<&Layout> = list
        .iter()
        .filter(|layout| {
            layout.matches(&typed)
                && !chosen
                    .iter()
                    .any(|one| one.code == layout.code && one.variant == layout.variant)
        })
        .collect();
    let rank = |layout: &Layout| {
        if layout.variant.is_empty() && layout.code == typed {
            0
        } else if words(&layout.name).contains(&typed) {
            1
        } else if layout.name.to_lowercase().starts_with(&typed) {
            2
        } else {
            3
        }
    };
    // the sort is stable, so each rank keeps the order of the names
    found.sort_by_key(|layout| rank(layout));
    found
}

/// The layouts localed says the desktop has, out of its two lists: `us,gb` and `,dvorak`. A desktop
/// where none was chosen types with the one every keyboard starts in.
#[must_use]
pub fn chosen(list: &[Layout], layouts: &str, variants: &str) -> Vec<Layout> {
    let mut variants = variants.split(',').map(str::trim);
    let found: Vec<Layout> = layouts
        .split(',')
        .map(|code| (code.trim(), variants.next().unwrap_or_default()))
        .filter(|(code, _)| !code.is_empty())
        .map(|(code, variant)| named(list, code, variant))
        .collect();
    if found.is_empty() {
        vec![named(list, DEFAULT, "")]
    } else {
        found
    }
}

/// The two lists localed takes for these layouts: the codes, and the variants, which is nothing at
/// all when none of them is one.
#[must_use]
pub fn joined(layouts: &[Layout]) -> (String, String) {
    let codes = layouts
        .iter()
        .map(|layout| layout.code.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let variants = if layouts.iter().all(|layout| layout.variant.is_empty()) {
        String::new()
    } else {
        layouts
            .iter()
            .map(|layout| layout.variant.as_str())
            .collect::<Vec<_>>()
            .join(",")
    };
    (codes, variants)
}

/// The layouts in words, the way a person reads a list of them: `English (US) and English (UK)`.
#[must_use]
pub fn said(layouts: &[Layout]) -> String {
    let names: Vec<&str> = layouts.iter().map(|layout| layout.name.as_str()).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => (*one).to_string(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

/// Hand localed the layouts the desktop types with, the first of them the one it starts in. The
/// model and the options are what localed has already, and the text console keeps its keymap.
/// localed writes them to the file the image keeps on persist, and Horizon follows it.
///
/// # Errors
///
/// A sentence when there are none or too many, or localed will not take them.
#[cfg(feature = "bus")]
pub fn set(layouts: &[Layout], model: &str, options: &str) -> Result<(), String> {
    if layouts.is_empty() {
        return Err("The desktop needs a layout to type with.".to_string());
    }
    if layouts.len() > MOST {
        return Err(TOO_MANY.to_string());
    }
    let (codes, variants) = joined(layouts);
    let connection = bus::connect(bus::PROPERTY_TIMEOUT)?;
    bus::object(
        &connection,
        crate::region::SERVICE,
        OBJECT,
        crate::region::SERVICE,
    )
    .and_then(|proxy| {
        proxy.call::<_, _, ()>(
            "SetX11Keyboard",
            &(
                codes.as_str(),
                model,
                variants.as_str(),
                options,
                false,
                false,
            ),
        )
    })
    .map_err(|e| {
        if bus::refused(&e) {
            "This account may not change the keyboard layout without an administrator's \
                 password."
                .to_string()
        } else {
            bus::sentence_for(NAME, e)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A few lines of xkeyboard-config's list, in its own order and spacing.
    const SAMPLE: &str = "! model\n\
        \x20 pc105           Generic 105-key PC\n\
        \n\
        ! layout\n\
        \x20 us              English (US)\n\
        \x20 fr              French\n\
        \x20 gb              English (UK)\n\
        \x20 ua              Ukrainian\n\
        \x20 de              German\n\
        \x20 custom          A user-defined custom Layout\n\
        \n\
        ! variant\n\
        \x20 dvorak          us: English (Dvorak)\n\
        \x20 extd            gb: English (UK, extended, Windows)\n\
        \x20 azerty          fr: French (AZERTY)\n\
        \n\
        ! option\n\
        \x20 grp                  Switching to another layout\n\
        \x20 grp:toggle           Right Alt\n";

    fn listed() -> Vec<Layout> {
        parse(SAMPLE)
    }

    #[test]
    fn the_list_has_every_layout_and_variant_by_name() {
        let list = listed();
        let names: Vec<&str> = list.iter().map(|layout| layout.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "English (Dvorak)",
                "English (UK)",
                "English (UK, extended, Windows)",
                "English (US)",
                "French",
                "French (AZERTY)",
                "German",
                "Ukrainian",
            ]
        );
        let extended = named(&list, "gb", "extd");
        assert_eq!(extended.word(), "gb(extd)");
        assert_eq!(by_word(&list, "gb(extd)"), Some(&extended));
        assert_eq!(by_word(&list, "gb(nothing)"), None);
        assert!(extended.is("gb(extd)") && extended.is(" gb(extd) "));
        assert!(!extended.is("gb"));
        // nothing from the models or the options, and not the placeholder for a layout of one's own
        assert!(
            list.iter()
                .all(|layout| !["pc105", "grp", "custom"].contains(&layout.code.as_str()))
        );
    }

    #[test]
    fn a_layout_the_list_does_not_have_is_its_word() {
        let bare = named(&listed(), "xx", "yy");
        assert_eq!(bare.name, "xx(yy)");
        assert_eq!(named(&[], "gb", "").name, "gb");
    }

    #[test]
    fn a_search_puts_the_code_and_whole_words_first() {
        let list = listed();
        let words = |typed: &str| -> Vec<String> {
            find(&list, typed, &[])
                .into_iter()
                .map(Layout::word)
                .collect()
        };
        // uk is a word of English (UK), and only the start of Ukrainian
        assert_eq!(words("uk"), ["gb", "gb(extd)", "ua"]);
        assert_eq!(words("gb"), ["gb", "gb(extd)"]);
        assert_eq!(words("French"), ["fr", "fr(azerty)"]);
        assert_eq!(words("english dvorak"), ["us(dvorak)"]);
        assert_eq!(words("DE"), ["de"]);
        assert!(words("  ").is_empty());
        assert!(words("klingon").is_empty());
        // a layout already chosen is not found again
        let chosen = [named(&list, "gb", "")];
        let found: Vec<String> = find(&list, "uk", &chosen)
            .into_iter()
            .map(Layout::word)
            .collect();
        assert_eq!(found, ["gb(extd)", "ua"]);
    }

    #[test]
    fn localed_lists_the_layouts_and_their_variants_side_by_side() {
        let list = listed();
        let two = chosen(&list, "us,gb", "");
        assert_eq!(said(&two), "English (US) and English (UK)");
        assert_eq!(joined(&two), ("us,gb".to_string(), String::new()));
        let variant = chosen(&list, "gb,us", ",dvorak");
        assert_eq!(said(&variant), "English (UK) and English (Dvorak)");
        assert_eq!(
            joined(&variant),
            ("gb,us".to_string(), ",dvorak".to_string())
        );
        let three = chosen(&list, "us, fr ,de", "");
        assert_eq!(said(&three), "English (US), French and German");
        // nothing chosen is the layout every keyboard starts in
        let nothing = chosen(&list, "", "");
        assert_eq!(nothing, [named(&list, "us", "")]);
        assert_eq!(said(&nothing), "English (US)");
        assert_eq!(said(&[]), "");
    }

    #[test]
    fn a_word_is_a_code_and_a_variant() {
        assert_eq!(split("gb"), ("gb", ""));
        assert_eq!(split("us(dvorak)"), ("us", "dvorak"));
        assert_eq!(split(" us ( dvorak ) "), ("us", "dvorak"));
        assert_eq!(Layout::bare("us", "dvorak").word(), "us(dvorak)");
    }
}
