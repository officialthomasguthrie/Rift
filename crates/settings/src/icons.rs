//! Icons drawn in the window. Where each one is comes from librift, the same lookup the shell
//! makes: a symbolic drawing painted in one colour for the sidebar and the rows, and an app's own
//! drawing in its own colours where a page lists apps.

use std::path::Path;

use iced::widget::{image, space, svg};
use iced::{Color, Element, Theme};
pub use librift::icons::find;
use librift::icons::{UNKNOWN_APP, app};

/// A symbolic icon by name at this size, painted in one colour, or the same space left empty when
/// no theme on the machine has it.
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

/// An app's own drawing at this size, the one its desktop entry names. A symbolic drawing has no
/// colours of its own, so it is painted in `text`, and a name no theme has falls back to the
/// drawing for a program with nothing of its own, the way the dock draws it.
#[must_use]
pub fn of_app<'a, Message: 'a>(text: Color, name: Option<&str>, size: f32) -> Element<'a, Message> {
    let found = name.and_then(app).or_else(|| app(UNKNOWN_APP));
    let Some(path) = found else {
        return space().width(size).height(size).into();
    };
    picture(text, &path, size)
}

/// An icon file at this size: a symbolic drawing painted in `text`, any other in its own colours.
fn picture<'a, Message: 'a>(text: Color, path: &Path, size: f32) -> Element<'a, Message> {
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
