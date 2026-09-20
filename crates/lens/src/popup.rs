//! The key popup: what a volume or a brightness key did, for a second, at the bottom middle of the
//! screen over the dock. The icon at the left and a bar with the level, on the menu's gray inside
//! a border, and nothing to press.

use std::time::Duration;

use iced::widget::{container, row, space};
use iced::{Border, Element, Length, Theme, window};
use librift::sound::{Side, Volume};

use crate::control::Level;
use crate::icons;
use crate::theme::Palette;
use crate::ui::Message;

/// How wide the popup is in logical pixels.
pub const WIDTH: u32 = 220;
/// How tall it is.
pub const HEIGHT: u32 = 56;
/// How far above the dock it stands.
pub const ABOVE: u32 = 96;
/// How long it stays after the last key.
pub const SHOWN: Duration = Duration::from_secs(1);
/// The padding inside it.
const PAD: u16 = 16;
/// The icon.
const ICON: f32 = 24.0;
/// The gap between the icon and the bar.
const GAP: f32 = 16.0;
/// How thick the bar is.
const THICK: f32 = 6.0;
/// The corner of the bar.
const RADIUS: f32 = 2.0;
/// How long the bar is: what is left of the popup beside the icon.
const LONG: f32 = 148.0;

/// The popup while it is up.
#[derive(Debug, Clone, Copy)]
pub struct Popup {
    /// Its surface.
    pub id: window::Id,
    /// What it shows.
    pub level: Level,
    /// Which key's second it is showing: a timer from an earlier key is ignored.
    pub epoch: u64,
}

/// The icon for what the key changed.
#[must_use]
pub fn icon(level: Level) -> &'static str {
    match level {
        Level::Volume { level, muted } => Volume {
            level: u16::from(level),
            muted,
        }
        .icon(Side::Output),
        Level::Brightness(_) => "display-brightness-symbolic",
    }
}

/// How much of the bar is filled: nothing for a muted sink, whatever its level.
#[must_use]
pub fn filled(level: Level) -> f32 {
    let percent = match level {
        Level::Volume { muted: true, .. } => 0,
        Level::Volume { level, .. } | Level::Brightness(level) => level.min(100),
    };
    LONG * f32::from(percent) / 100.0
}

/// The popup: the icon and the level.
pub fn view(look: Palette, popup: &Popup) -> Element<'static, Message> {
    let part = |colour: iced::Color| {
        move |_: &Theme| container::Style {
            background: Some(colour.into()),
            border: Border {
                radius: RADIUS.into(),
                ..Border::default()
            },
            ..container::Style::default()
        }
    };
    let level =
        container(space().width(filled(popup.level)).height(THICK)).style(part(look.accent));
    let bar = container(level)
        .width(LONG)
        .height(THICK)
        .style(part(look.track));
    container(
        row![icons::symbolic(look.text, icon(popup.level), ICON), bar]
            .spacing(GAP)
            .align_y(iced::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(PAD)
    .align_y(iced::Center)
    .style(move |_: &Theme| container::Style {
        background: Some(look.menu.into()),
        text_color: Some(look.text),
        border: Border {
            color: look.edge,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_is_what_is_left_beside_the_icon() {
        let wide = f32::from(u16::try_from(WIDTH).expect("a width"));
        let tall = f32::from(u16::try_from(HEIGHT).expect("a height"));
        assert!((wide - 2.0 * f32::from(PAD) - ICON - GAP - LONG).abs() < f32::EPSILON);
        assert!(tall - 2.0 * f32::from(PAD) >= ICON);
    }

    #[test]
    fn the_level_fills_its_share_of_the_bar() {
        let loud = Level::Volume {
            level: 50,
            muted: false,
        };
        assert!((filled(loud) - LONG / 2.0).abs() < f32::EPSILON);
        assert!(
            filled(Level::Volume {
                level: 50,
                muted: true
            })
            .abs()
                < f32::EPSILON
        );
        assert!((filled(Level::Brightness(100)) - LONG).abs() < f32::EPSILON);
        assert_eq!(icon(loud), "audio-volume-medium-symbolic");
        assert_eq!(
            icon(Level::Volume {
                level: 50,
                muted: true
            }),
            "audio-volume-muted-symbolic"
        );
        assert_eq!(icon(Level::Brightness(10)), "display-brightness-symbolic");
    }
}
