//! Icons drawn in the shell. Where each one is comes from librift, the way the XDG icon theme
//! specification says; a status icon is the symbolic drawing the bar paints in one colour, and an
//! app icon is the app's own drawing in its own colours.

use std::path::Path;

use iced::widget::{image, space, svg};
use iced::{Color, Element, Theme};
pub use librift::icons::{UNKNOWN_APP, app, find};

/// An app's own drawing at this size, for a menu row or a dock item. A symbolic drawing has no
/// colours of its own, so it is painted in `text`, the way the status icons in the bar are, and a
/// name no theme has falls back to the drawing for a program with nothing of its own.
#[must_use]
pub fn draw<'a, Message: 'a>(text: Color, name: Option<&str>, size: f32) -> Element<'a, Message> {
    let found = name.and_then(app).or_else(|| app(UNKNOWN_APP));
    let Some(path) = found else {
        return space().width(size).height(size).into();
    };
    picture(text, &path, size)
}

/// An icon file that was already found, at this size: a symbolic drawing painted in `text`, any
/// other in its own colours.
#[must_use]
pub fn picture<'a, Message: 'a>(text: Color, path: &Path, size: f32) -> Element<'a, Message> {
    let colour = path.to_string_lossy().contains("symbolic").then_some(text);
    if path.extension().is_some_and(|ending| ending == "svg") {
        svg(svg::Handle::from_path(path))
            .width(size)
            .height(size)
            .style(move |_: &Theme, _| svg::Style { color: colour })
            .into()
    } else {
        image(image::Handle::from_path(path))
            .width(size)
            .height(size)
            .into()
    }
}

/// The drawing of a file's kind at this size, the first of `names` a theme has, the one that fits
/// best first, in its own colours, or a symbolic one painted in `text` when no theme has one in
/// colour. A row of a search draws a file this way, the same as a row of Files.
#[must_use]
pub fn of_file<'a, Message: 'a>(text: Color, names: &[String], size: f32) -> Element<'a, Message> {
    match librift::icons::file(names) {
        Some(path) => picture(text, &path, size),
        None => space().width(size).height(size).into(),
    }
}

/// A symbolic icon by name at this size, painted in one colour, or the same space left empty when no
/// theme on the machine has it.
#[must_use]
pub fn symbolic<'a, Message: 'a>(colour: Color, name: &str, size: f32) -> Element<'a, Message> {
    let Some(path) = find(name) else {
        return space().width(size).height(size).into();
    };
    svg(svg::Handle::from_path(path))
        .width(size)
        .height(size)
        .style(move |_: &Theme, _| svg::Style {
            color: Some(colour),
        })
        .into()
}
