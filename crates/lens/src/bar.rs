//! The top bar, laid out like Tails: the Applications button at the left, the clock in the middle
//! of the screen and the status icons at the right. The sizes and the colours below are the ones
//! the boot test counts.

use iced::widget::{button, column, container, row, space, stack, svg, text};
use iced::{Background, Border, Color, Element, Length, Shadow, Theme};

use crate::icons;
use crate::status::Status;
use crate::theme::Palette;
use crate::ui::Message;

/// How tall the bar is in logical pixels. It is the exclusive zone as well, so windows sit under
/// it and the desktop never jumps.
pub const HEIGHT: u32 = 32;
/// The same height where a length is wanted.
const TALL: f32 = 32.0;
/// The hairline along the bottom edge of the bar, inside its height.
const LINE: f32 = 1.0;
/// The padding at each end of the bar.
const PAD: u16 = 8;
/// How tall a button in the bar is.
const ITEM: f32 = 24.0;
/// The bar's text, 10.5 pt at 96 dpi.
pub const TEXT_SIZE: f32 = 14.0;
/// A symbolic status icon.
const ICON: f32 = 16.0;
/// The gap between two status icons.
const ICON_GAP: f32 = 8.0;

/// The button at the left, and the menu it opens.
pub const APPLICATIONS: &str = "Applications";

/// The bar. `open` is whether the Applications menu is showing, which marks its button.
pub fn view<'a>(
    look: Palette,
    clock: &'a str,
    status: &Status,
    open: bool,
) -> Element<'a, Message> {
    let items = row![
        applications(look, open),
        space().width(Length::Fill),
        status_icons(look, status),
    ]
    .align_y(iced::Center)
    .height(Length::Fill);
    // the clock sits in the middle of the screen, not of what is left over, so it is a layer of
    // its own under the buttons. neither its text nor the space beside them takes a click
    let middle = container(text(clock).size(TEXT_SIZE).color(look.text)).center(Length::Fill);
    let content = container(stack![middle, items])
        .width(Length::Fill)
        .height(TALL - LINE)
        .padding([0, PAD])
        .style(move |_: &Theme| container::Style {
            background: Some(look.bar.into()),
            text_color: Some(look.text),
            ..container::Style::default()
        });
    let hairline = container(space().width(Length::Fill).height(LINE)).style(move |_: &Theme| {
        container::Style {
            background: Some(look.line.into()),
            ..container::Style::default()
        }
    });
    column![content, hairline].into()
}

fn applications(look: Palette, open: bool) -> Element<'static, Message> {
    // a button lays its content out at the top of its box, so the label is centred by hand
    let label = container(text(APPLICATIONS).size(TEXT_SIZE)).center_y(Length::Fill);
    button(label)
        .height(ITEM)
        .padding([0, PAD])
        .on_press(Message::ToggleMenu)
        .style(move |_: &Theme, state: button::Status| {
            let fill = if open {
                Some(look.press)
            } else {
                match state {
                    button::Status::Hovered => Some(look.hover),
                    button::Status::Pressed => Some(look.press),
                    _ => None,
                }
            };
            button::Style {
                background: fill.map(Background::from),
                text_color: look.text,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 4.0.into(),
                },
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .into()
}

/// The status icons, in the order every desktop puts them: the network, then the volume, then the
/// battery. An icon is there only when the system has something to say. P1.11 makes them one
/// button that opens the system menu.
fn status_icons(look: Palette, status: &Status) -> Element<'static, Message> {
    let mut names = vec![status.network.icon()];
    if let Some(volume) = status.volume {
        names.push(volume.icon().to_string());
    }
    if let Some(battery) = status.battery {
        names.push(battery.icon());
    }
    let mut line = row![].spacing(ICON_GAP).align_y(iced::Center);
    for name in names {
        if let Some(path) = icons::find(&name) {
            line = line.push(
                svg(svg::Handle::from_path(path))
                    .width(ICON)
                    .height(ICON)
                    .style(move |_: &Theme, _| svg::Style {
                        color: Some(look.text),
                    }),
            );
        }
    }
    container(line).center_y(Length::Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_height_is_the_same_number_twice() {
        assert!((f64::from(HEIGHT) - f64::from(TALL)).abs() < f64::EPSILON);
        assert!(
            f64::from(LINE) < f64::from(ITEM),
            "the hairline is not a row"
        );
    }
}
