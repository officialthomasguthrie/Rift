//! The parts every page is built from: a section heading, a row of a list, a note, a button, a
//! switch and the sliders, all in the app's colours.

use iced::widget::{
    button, checkbox, column, container, progress_bar, row, rule, scrollable, slider, space, text,
    text_input, toggler,
};
use iced::{
    Border, Center, Color, Element, Fill, Font, Length, Point, Shadow, Theme, Vector, font,
};

use librift::appearance::{Accent, Theme as Mode};

use crate::theme::{Colors, hex};

/// The interface font.
pub const FONT: Font = Font {
    family: font::Family::Name("Noto Sans"),
    ..Font::DEFAULT
};
/// The same font in bold, for a heading and for the title in the header bar.
pub const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..FONT
};
/// The font a terminal draws in, for the sample of a terminal colour scheme.
pub const MONO: Font = Font {
    family: font::Family::Name("DejaVu Sans Mono"),
    ..Font::DEFAULT
};
/// Body text, at eleven points on a ninety-six dot screen.
pub const TEXT_SIZE: f32 = 14.0;
/// One step above it, for the name at the top of a page.
pub const TITLE_SIZE: f32 = 17.0;
/// How much space a page leaves at its edges.
pub const PAD: f32 = 20.0;
/// How much space there is between the sections of a page.
pub const GAP: f32 = 16.0;
/// How wide a field is.
const FIELD: f32 = 200.0;
/// How wide a menu is at the least, and at the most.
const MENU_WIDTHS: (f32, f32) = (200.0, 380.0);
/// About how wide a character of a menu's rows is, to make the menu as wide as its longest row.
const MENU_CHARACTER: f32 = 7.6;
/// How tall a row of a menu is.
pub const MENU_ROW: f32 = 30.0;
/// The space around the rows of a menu.
pub const MENU_PAD: f32 = 6.0;
/// How tall the line between two groups of a menu's rows is, with the space around it.
pub const MENU_LINE: f32 = 9.0;
/// How wide a dialog is.
pub const DIALOG_WIDTH: f32 = 420.0;
/// How wide and tall a swatch of an accent colour is.
const SWATCH: f32 = 28.0;
/// How wide the label of a row is when its value fills the rest of the row.
const LABEL: f32 = 150.0;

/// The name over a group of rows.
#[must_use]
pub fn heading<'a, M: 'a>(colors: Colors, label: &'a str) -> Element<'a, M> {
    text(label)
        .size(TEXT_SIZE)
        .font(BOLD)
        .color(colors.text)
        .into()
}

/// A line of plain text.
#[must_use]
pub fn line<'a, M: 'a>(colors: Colors, said: &'a str) -> Element<'a, M> {
    text(said).size(TEXT_SIZE).color(colors.text).into()
}

/// A line that says less: a note under a heading, a credit, a unit.
#[must_use]
pub fn note<'a, M: 'a>(colors: Colors, said: &'a str) -> Element<'a, M> {
    text(said).size(TEXT_SIZE).color(colors.dim).into()
}

/// The hairline between two rows.
#[must_use]
pub fn hairline<'a, M: 'a>(colors: Colors) -> Element<'a, M> {
    rule::horizontal(1)
        .style(move |_: &Theme| rule::Style {
            color: colors.line,
            radius: 0.0.into(),
            fill_mode: rule::FillMode::Full,
            snap: true,
        })
        .into()
}

/// A group of rows with a border around it and a hairline between them, the way a settings page
/// puts a list together.
#[must_use]
pub fn group<'a, M: 'a>(colors: Colors, rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    let mut inside = column![].width(Fill);
    for (at, one) in rows.into_iter().enumerate() {
        if at > 0 {
            inside = inside.push(hairline(colors));
        }
        inside = inside.push(one);
    }
    container(inside)
        .width(Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(colors.view.into()),
            border: Border {
                color: colors.line,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// One row of a group: what it says at the left and what it is set to at the right.
pub fn setting<'a, M: 'a>(
    colors: Colors,
    label: &'a str,
    under: Option<&'a str>,
    right: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let mut left = column![line(colors, label)].spacing(2);
    if let Some(under) = under {
        left = left.push(note(colors, under));
    }
    container(
        row![left.width(Fill), right.into()]
            .align_y(Center)
            .spacing(GAP),
    )
    .width(Fill)
    .padding([8, 12])
    .into()
}

/// A row of a group whose label is a fixed width and whose value fills the rest of it, for a value
/// too long to sit at the right end of the row.
#[must_use]
pub fn fact<'a, M: 'a>(colors: Colors, label: &'a str, value: String) -> Element<'a, M> {
    container(
        row![
            container(line(colors, label)).width(Length::Fixed(LABEL)),
            text(value).size(TEXT_SIZE).color(colors.dim).width(Fill),
        ]
        .align_y(Center)
        .spacing(GAP),
    )
    .width(Fill)
    .padding([8, 12])
    .into()
}

/// A button that does something, with the verb on it. Without a press it is dimmed, which is how a
/// button whose work is already running is drawn.
pub fn action<'a, M: Clone + 'a>(
    colors: Colors,
    label: &'a str,
    press: Option<M>,
) -> Element<'a, M> {
    let mut pressable =
        button(text(label).size(TEXT_SIZE))
            .padding([5, 14])
            .style(move |_: &Theme, status| button::Style {
                background: Some(
                    match status {
                        button::Status::Hovered | button::Status::Pressed => colors.hover,
                        button::Status::Disabled => colors.track,
                        button::Status::Active => colors.button,
                    }
                    .into(),
                ),
                text_color: match status {
                    button::Status::Disabled => colors.dim,
                    _ => colors.text,
                },
                border: Border {
                    color: colors.edge,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..button::Style::default()
            });
    if let Some(press) = press {
        pressable = pressable.on_press(press);
    }
    pressable.into()
}

/// The default button of a page, the one that goes on: filled with the accent. Without a press it
/// is dimmed like any other button that cannot be pressed.
pub fn primary<'a, M: Clone + 'a>(
    colors: Colors,
    label: &'a str,
    press: Option<M>,
) -> Element<'a, M> {
    let mut pressable =
        button(text(label).size(TEXT_SIZE))
            .padding([5, 14])
            .style(move |_: &Theme, status| {
                let (background, text_color, edge) = match status {
                    button::Status::Disabled => (colors.track, colors.dim, colors.edge),
                    button::Status::Hovered | button::Status::Pressed => (
                        Color {
                            a: 0.85,
                            ..colors.accent
                        },
                        colors.on_accent,
                        colors.accent,
                    ),
                    button::Status::Active => (colors.accent, colors.on_accent, colors.accent),
                };
                button::Style {
                    background: Some(background.into()),
                    text_color,
                    border: Border {
                        color: edge,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..button::Style::default()
                }
            });
    if let Some(press) = press {
        pressable = pressable.on_press(press);
    }
    pressable.into()
}

/// A box that is ticked or not. Without `toggle` it stands still and dimmed.
#[must_use]
pub fn tick<'a, M: 'a>(
    colors: Colors,
    ticked: bool,
    toggle: Option<Box<dyn Fn(bool) -> M + 'a>>,
) -> Element<'a, M> {
    let mut boxed = checkbox(ticked).size(18.0).style(move |_: &Theme, status| {
        let (on, still) = match status {
            checkbox::Status::Active { is_checked } | checkbox::Status::Hovered { is_checked } => {
                (is_checked, false)
            }
            checkbox::Status::Disabled { is_checked } => (is_checked, true),
        };
        let fill = match (on, still) {
            (true, false) => colors.accent,
            (true, true) => colors.track,
            (false, _) => colors.field,
        };
        checkbox::Style {
            background: fill.into(),
            icon_color: if still { colors.dim } else { colors.on_accent },
            border: Border {
                color: if on && !still {
                    colors.accent
                } else {
                    colors.edge
                },
                width: 1.0,
                radius: 3.0.into(),
            },
            text_color: None,
        }
    });
    if let Some(toggle) = toggle {
        boxed = boxed.on_toggle(toggle);
    }
    boxed.into()
}

/// How far something has got, out of a hundred, as a thin bar with the number beside it.
#[must_use]
pub fn progress<'a, M: 'a>(colors: Colors, percent: u32) -> Element<'a, M> {
    #[allow(clippy::cast_precision_loss)]
    let value = percent.min(100) as f32;
    row![
        progress_bar(0.0..=100.0, value)
            .length(Length::Fixed(140.0))
            .girth(Length::Fixed(6.0))
            .style(move |_: &Theme| progress_bar::Style {
                background: colors.track.into(),
                bar: colors.accent.into(),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 3.0.into(),
                },
            }),
        text(format!("{percent}%"))
            .size(TEXT_SIZE)
            .color(colors.dim)
            .align_x(iced::alignment::Horizontal::Right)
            .width(Length::Fixed(44.0)),
    ]
    .align_y(Center)
    .spacing(10)
    .into()
}

/// A row of a group that is pressed to choose it, with a mark at the right when it is the one in
/// use, and whatever `beside` is between the two.
pub fn choice<'a, M: Clone + 'a>(
    colors: Colors,
    label: &'a str,
    under: Option<&'a str>,
    beside: Option<Element<'a, M>>,
    chosen: bool,
    press: M,
) -> Element<'a, M> {
    let mut left = column![line(colors, label)].spacing(2);
    if let Some(under) = under {
        left = left.push(note(colors, under));
    }
    pressable(colors, left.into(), beside, chosen, press)
}

/// The same row with whatever is at its left, for a row whose words are worked out as it is drawn.
pub fn pressable<'a, M: Clone + 'a>(
    colors: Colors,
    left: Element<'a, M>,
    beside: Option<Element<'a, M>>,
    chosen: bool,
    press: M,
) -> Element<'a, M> {
    let mark: Element<'a, M> = if chosen {
        crate::icons::symbolic(colors.accent, "object-select-symbolic", 16.0)
    } else {
        space().width(16.0).height(16.0).into()
    };
    let mut inside = row![container(left).width(Fill)]
        .align_y(Center)
        .spacing(GAP)
        .width(Fill);
    if let Some(beside) = beside {
        inside = inside.push(beside);
    }
    button(inside.push(mark))
        .width(Fill)
        .padding([8, 12])
        .on_press(press)
        .style(move |_: &Theme, status| button::Style {
            background: Some(
                match status {
                    button::Status::Hovered | button::Status::Pressed => colors.hover,
                    _ => Color::TRANSPARENT,
                }
                .into(),
            ),
            text_color: colors.text,
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..button::Style::default()
        })
        .into()
}

/// The switch of a row that is on or off.
pub fn switch<'a, M: Clone + 'a>(
    colors: Colors,
    on: bool,
    toggle: impl Fn(bool) -> M + 'a,
) -> Element<'a, M> {
    toggler(on)
        .on_toggle(toggle)
        .size(20.0)
        .style(move |_: &Theme, status| switched(colors, status))
        .into()
}

/// The same switch, dimmed and still, for a setting that does nothing the way things stand.
#[must_use]
pub fn still<'a, M: Clone + 'a>(colors: Colors, on: bool) -> Element<'a, M> {
    toggler(on)
        .size(20.0)
        .style(move |_: &Theme, status| switched(colors, status))
        .into()
}

/// How a switch is drawn: the track in the accent while it is on, and dimmed while it cannot move.
fn switched(colors: Colors, status: toggler::Status) -> toggler::Style {
    let (track, knob) = match status {
        toggler::Status::Active { is_toggled } | toggler::Status::Hovered { is_toggled } => (
            if is_toggled {
                colors.accent
            } else {
                colors.track
            },
            colors.knob,
        ),
        toggler::Status::Disabled { .. } => (colors.track, colors.dim),
    };
    toggler::Style {
        background: track.into(),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: knob.into(),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: None,
        border_radius: None,
        padding_ratio: 0.15,
    }
}

/// The slider of a row that is a number between two others, with the number and its unit beside it.
pub fn steps<'a, M: Clone + 'a>(
    colors: Colors,
    range: std::ops::RangeInclusive<u32>,
    step: u32,
    value: u32,
    unit: &'static str,
    moved: impl Fn(u32) -> M + 'a,
    released: M,
) -> Element<'a, M> {
    row![
        slider(range, value, moved)
            .step(step)
            .on_release(released)
            .width(Length::Fixed(200.0))
            .style(move |_: &Theme, _| rail(colors)),
        text(format!("{value} {unit}"))
            .size(TEXT_SIZE)
            .color(colors.dim)
            .width(Length::Fixed(52.0)),
    ]
    .align_y(Center)
    .spacing(GAP)
    .into()
}

/// The slider of a speed, slow at its left end and fast at its right, with no number: a speed has
/// no unit a person would know.
pub fn speed<'a, M: Clone + 'a>(
    colors: Colors,
    value: i32,
    moved: impl Fn(i32) -> M + 'a,
    released: M,
) -> Element<'a, M> {
    row![
        note(colors, "Slow"),
        slider(
            librift::pointer::SPEED_LEAST..=librift::pointer::SPEED_MOST,
            value,
            moved
        )
        .step(1)
        .on_release(released)
        .width(Length::Fixed(160.0))
        .style(move |_: &Theme, _| rail(colors)),
        note(colors, "Fast"),
    ]
    .align_y(Center)
    .spacing(10)
    .into()
}

/// How a slider is drawn: the part up to the handle in the accent, the rest in the track's gray.
fn rail(colors: Colors) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (colors.accent.into(), colors.track.into()),
            width: 4.0,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 2.0.into(),
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 8.0 },
            background: colors.text.into(),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
        },
    }
}

/// A field text is typed into, in the app's colours, with the accent around it while the cursor is
/// in it. `secret` hides what is typed, for a password.
pub fn field<'a, M: Clone + 'a>(
    colors: Colors,
    hint: &'a str,
    value: &'a str,
    secret: bool,
    id: impl Into<iced::widget::Id>,
    typed: impl Fn(String) -> M + 'a,
    entered: M,
) -> Element<'a, M> {
    entry(colors, hint, value, secret, id, typed, entered)
        .width(Length::Fixed(FIELD))
        .into()
}

/// The same field as wide as the space it is in, for a name in a dialog or a path in a header bar.
pub fn wide_field<'a, M: Clone + 'a>(
    colors: Colors,
    hint: &'a str,
    value: &'a str,
    id: impl Into<iced::widget::Id>,
    typed: impl Fn(String) -> M + 'a,
    entered: M,
) -> Element<'a, M> {
    entry(colors, hint, value, false, id, typed, entered)
        .width(Fill)
        .into()
}

fn entry<'a, M: Clone + 'a>(
    colors: Colors,
    hint: &'a str,
    value: &'a str,
    secret: bool,
    id: impl Into<iced::widget::Id>,
    typed: impl Fn(String) -> M + 'a,
    entered: M,
) -> text_input::TextInput<'a, M> {
    text_input(hint, value)
        .id(id)
        .secure(secret)
        .on_input(typed)
        .on_submit(entered)
        .size(TEXT_SIZE)
        .padding([6, 8])
        .style(move |_: &Theme, status| {
            let (edge, width) = match status {
                text_input::Status::Focused { .. } => (colors.accent, 2.0),
                _ => (colors.edge, 1.0),
            };
            text_input::Style {
                background: colors.field.into(),
                border: Border {
                    color: edge,
                    width,
                    radius: 4.0.into(),
                },
                icon: colors.text,
                placeholder: colors.dim,
                value: colors.text,
                selection: Color {
                    a: 0.4,
                    ..colors.accent
                },
            }
        })
}

/// The nine accent colours, each a square of itself, the one in use ringed. Pressing one sends
/// `chose` with it.
pub fn swatches<'a, M: Clone + 'a>(
    colors: Colors,
    mode: Mode,
    chosen: Accent,
    chose: impl Fn(Accent) -> M,
) -> Element<'a, M> {
    let mut swatches = row![].spacing(10).align_y(Center);
    for accent in Accent::ALL {
        let colour = hex(accent.hex(mode));
        let here = accent == chosen;
        swatches = swatches.push(
            button(
                container(
                    space()
                        .width(Length::Fixed(SWATCH))
                        .height(Length::Fixed(SWATCH)),
                )
                .style(move |_: &Theme| fill(colour)),
            )
            .padding(2)
            .on_press(chose(accent))
            .style(move |_: &Theme, status| button::Style {
                background: None,
                text_color: colors.text,
                border: Border {
                    color: match (here, status) {
                        (true, _) => colors.text,
                        (false, button::Status::Hovered | button::Status::Pressed) => colors.edge,
                        _ => Color::TRANSPARENT,
                    },
                    width: 2.0,
                    radius: 6.0.into(),
                },
                ..button::Style::default()
            }),
        );
    }
    swatches.into()
}

/// The operation that puts the cursor in a field.
pub fn focus<M: Send + 'static>(id: impl Into<iced::widget::Id>) -> iced::Task<M> {
    iced::widget::operation::focus(id)
}

/// A button filled with the red of what cannot be undone, for the one button of a dialog that
/// deletes. Without a press it is dimmed like any other button that cannot be pressed.
pub fn destructive<'a, M: Clone + 'a>(
    colors: Colors,
    label: &'a str,
    press: Option<M>,
) -> Element<'a, M> {
    let mut pressable =
        button(text(label).size(TEXT_SIZE))
            .padding([5, 14])
            .style(move |_: &Theme, status| {
                let (background, text_color) = match status {
                    button::Status::Disabled => (colors.track, colors.dim),
                    button::Status::Hovered | button::Status::Pressed => (
                        Color {
                            a: 0.85,
                            ..colors.error
                        },
                        Color::WHITE,
                    ),
                    button::Status::Active => (colors.error, Color::WHITE),
                };
                button::Style {
                    background: Some(background.into()),
                    text_color,
                    border: Border {
                        color: background,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..button::Style::default()
                }
            });
    if let Some(press) = press {
        pressable = pressable.on_press(press);
    }
    pressable.into()
}

/// A button in a header bar: a symbolic icon with nothing around it until the pointer is over it.
/// Without a press the icon is dimmed.
pub fn tool<'a, M: Clone + 'a>(colors: Colors, icon: &str, press: Option<M>) -> Element<'a, M> {
    let colour = if press.is_some() {
        colors.text
    } else {
        colors.dim
    };
    let mut pressable = button(crate::icons::symbolic(colour, icon, 16.0))
        .padding(7)
        .style(move |_: &Theme, status| button::Style {
            background: Some(
                match status {
                    button::Status::Hovered | button::Status::Pressed => colors.hover,
                    _ => Color::TRANSPARENT,
                }
                .into(),
            ),
            text_color: colour,
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..button::Style::default()
        });
    if let Some(press) = press {
        pressable = pressable.on_press(press);
    }
    pressable.into()
}

/// A part of the window that says where a press lands before whatever is under the pointer
/// handles it: a press of any button over `content` sends `at` with the pointer's place in the
/// window first.
pub fn pointed<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    at: impl Fn(Point) -> M + 'a,
) -> Element<'a, M> {
    crate::pointed::Pointed::new(content, at).into()
}

/// One row of a menu.
#[derive(Debug, Clone)]
pub enum Item<M> {
    /// Something to do: its words, what pressing it sends, which dims it when there is nothing,
    /// and for a row that is on or off, whether it is on.
    Do {
        /// The words on the row.
        label: String,
        /// What pressing it sends.
        press: Option<M>,
        /// Whether a row that is on or off is on.
        ticked: Option<bool>,
    },
    /// The line between two groups of rows.
    Line,
}

impl<M> Item<M> {
    /// A row that does something.
    pub fn new(label: impl Into<String>, press: Option<M>) -> Self {
        Self::Do {
            label: label.into(),
            press,
            ticked: None,
        }
    }

    /// A row that is on or off.
    pub fn ticked(label: impl Into<String>, on: bool, press: M) -> Self {
        Self::Do {
            label: label.into(),
            press: Some(press),
            ticked: Some(on),
        }
    }
}

/// How wide a menu of these rows is: as wide as its longest row, with room for the mark and the
/// padding, within [`MENU_WIDTHS`].
#[must_use]
pub fn menu_width<M>(items: &[Item<M>]) -> f32 {
    let longest = items
        .iter()
        .map(|item| match item {
            Item::Do { label, .. } => label.chars().count(),
            Item::Line => 0,
        })
        .max()
        .unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    let words = longest as f32 * MENU_CHARACTER;
    (words + 20.0 + 16.0 + GAP + 2.0 * MENU_PAD + 2.0).clamp(MENU_WIDTHS.0, MENU_WIDTHS.1)
}

/// How tall a menu of these rows is, to put it where it fits.
#[must_use]
pub fn menu_height<M>(items: &[Item<M>]) -> f32 {
    items
        .iter()
        .map(|item| match item {
            Item::Do { .. } => MENU_ROW,
            Item::Line => MENU_LINE,
        })
        .sum::<f32>()
        + 2.0 * MENU_PAD
        + 2.0
}

/// A menu: its rows in a box with a border and a soft shadow under it, the way the shell's menus
/// and a GTK popover look. A row on or off has the mark at its right while it is on.
#[must_use]
pub fn menu<'a, M: Clone + 'a>(colors: Colors, items: Vec<Item<M>>) -> Element<'a, M> {
    let width = menu_width(&items);
    let mut rows = column![].width(Fill);
    for item in items {
        match item {
            Item::Line => {
                rows = rows.push(
                    container(hairline(colors))
                        .height(Length::Fixed(MENU_LINE))
                        .center_y(Length::Fixed(MENU_LINE)),
                );
            }
            Item::Do {
                label,
                press,
                ticked,
            } => {
                let colour = if press.is_some() {
                    colors.text
                } else {
                    colors.dim
                };
                let mark: Element<'a, M> = if ticked == Some(true) {
                    crate::icons::symbolic(colour, "object-select-symbolic", 16.0)
                } else {
                    space().width(16.0).height(16.0).into()
                };
                let mut pressable = button(
                    container(
                        row![
                            text(label)
                                .size(TEXT_SIZE)
                                .color(colour)
                                .wrapping(text::Wrapping::None)
                                .width(Fill),
                            mark
                        ]
                        .align_y(Center)
                        .spacing(GAP),
                    )
                    .center_y(Fill),
                )
                .width(Fill)
                .height(Length::Fixed(MENU_ROW))
                .padding([0, 10])
                .style(move |_: &Theme, status| button::Style {
                    background: Some(
                        match status {
                            button::Status::Hovered | button::Status::Pressed => colors.hover,
                            _ => Color::TRANSPARENT,
                        }
                        .into(),
                    ),
                    text_color: colour,
                    border: Border {
                        radius: 4.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                });
                if let Some(press) = press {
                    pressable = pressable.on_press(press);
                }
                rows = rows.push(pressable);
            }
        }
    }
    container(rows)
        .width(Length::Fixed(width))
        .padding(MENU_PAD)
        .style(move |_: &Theme| raised(colors, colors.header))
        .into()
}

/// A dialog: its title in bold, what it says, and its buttons at the bottom right, in a box of its
/// own in the middle of the window. The window behind it is dimmed with [`shade`].
#[must_use]
pub fn dialog<'a, M: 'a>(
    colors: Colors,
    title: String,
    body: Vec<Element<'a, M>>,
    buttons: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let mut inside = column![text(title).size(TEXT_SIZE).font(BOLD).color(colors.text)]
        .spacing(12)
        .width(Fill);
    for part in body {
        inside = inside.push(part);
    }
    let mut foot = row![space().width(Fill)].spacing(8).align_y(Center);
    for pressable in buttons {
        foot = foot.push(pressable);
    }
    container(inside.push(container(foot).padding([6, 0])))
        .width(Length::Fixed(DIALOG_WIDTH))
        .padding(PAD)
        .style(move |_: &Theme| raised(colors, colors.page))
        .into()
}

/// The gray a dialog dims the window behind it with.
#[must_use]
pub fn shade<'a, M: 'a>() -> Element<'a, M> {
    container(space().width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .style(|_: &Theme| {
            fill(Color {
                a: 0.35,
                ..Color::BLACK
            })
        })
        .into()
}

/// A short line at the bottom of a window that says what just happened, with a button for what
/// can follow it, the way a GNOME app says a file went into the trash with Undo beside it.
pub fn toast<'a, M: Clone + 'a>(
    colors: Colors,
    said: String,
    action: Option<(&'a str, M)>,
) -> Element<'a, M> {
    let mut inside = row![text(said).size(TEXT_SIZE).color(colors.text)]
        .spacing(GAP)
        .align_y(Center);
    if let Some((label, press)) = action {
        inside = inside.push(action_button(colors, label, press));
    }
    container(inside)
        .padding([8, 14])
        .style(move |_: &Theme| raised(colors, colors.header))
        .into()
}

fn action_button<'a, M: Clone + 'a>(colors: Colors, label: &'a str, press: M) -> Element<'a, M> {
    action(colors, label, Some(press))
}

/// A box that stands over the window: a menu, a dialog or a toast, with a border and the soft
/// shadow the design rules allow under windows and menus.
fn raised(colors: Colors, background: Color) -> container::Style {
    container::Style {
        background: Some(background.into()),
        border: Border {
            color: colors.edge,
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.35,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 2.0),
            blur_radius: 10.0,
        },
        ..container::Style::default()
    }
}

/// A container filled with one colour and nothing else.
#[must_use]
pub fn fill(colour: Color) -> container::Style {
    container::Style {
        background: Some(colour.into()),
        ..container::Style::default()
    }
}

/// A list that scrolls, with the thin scroller the shell's menus have.
pub fn scroll<'a, M: 'a>(
    colors: Colors,
    inside: impl Into<Element<'a, M>>,
) -> scrollable::Scrollable<'a, M> {
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: colors.edge.into(),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 2.0.into(),
            },
        },
    };
    scrollable(inside)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .style(move |_: &Theme, _| scrollable::Style {
            container: container::Style::default(),
            vertical_rail: rail,
            horizontal_rail: rail,
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: colors.page.into(),
                border: Border {
                    color: colors.edge,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                icon: colors.text,
            },
        })
}
