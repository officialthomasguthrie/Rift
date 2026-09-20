//! The parts every page is built from: a section heading, a row of a list, a note, a button, a
//! switch and a slider, all in the app's colours.

use iced::widget::{
    button, column, container, row, rule, scrollable, slider, space, text, text_input, toggler,
};
use iced::{Border, Center, Color, Element, Fill, Font, Length, Theme, font};

use crate::theme::Colors;

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
/// How much space a page leaves at its edges, and between its sections.
pub const PAD: f32 = 20.0;
pub const GAP: f32 = 16.0;
/// How wide a field is.
const FIELD: f32 = 200.0;
/// How wide the label of a row is when its value fills the rest of the row.
const LABEL: f32 = 150.0;

/// The name over a group of rows.
pub fn heading<'a, M: 'a>(colors: Colors, label: &'a str) -> Element<'a, M> {
    text(label)
        .size(TEXT_SIZE)
        .font(BOLD)
        .color(colors.text)
        .into()
}

/// A line of plain text.
pub fn line<'a, M: 'a>(colors: Colors, said: &'a str) -> Element<'a, M> {
    text(said).size(TEXT_SIZE).color(colors.text).into()
}

/// A line that says less: a note under a heading, a credit, a unit.
pub fn note<'a, M: 'a>(colors: Colors, said: &'a str) -> Element<'a, M> {
    text(said).size(TEXT_SIZE).color(colors.dim).into()
}

/// The hairline between two rows.
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
    let mark: Element<'a, M> = if chosen {
        crate::icons::symbolic(colors.accent, "object-select-symbolic", 16.0)
    } else {
        space().width(16.0).height(16.0).into()
    };
    let mut inside = row![left.width(Fill)]
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
        .style(move |_: &Theme, status| {
            let (track, knob) = match status {
                toggler::Status::Active { is_toggled }
                | toggler::Status::Hovered { is_toggled } => (
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
        })
        .into()
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
            .style(move |_: &Theme, _| slider::Style {
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
            }),
        text(format!("{value} {unit}"))
            .size(TEXT_SIZE)
            .color(colors.dim)
            .width(Length::Fixed(52.0)),
    ]
    .align_y(Center)
    .spacing(GAP)
    .into()
}

/// A field text is typed into, in the app's colours, with the accent around it while the cursor is
/// in it. `secret` hides what is typed, for a password.
pub fn field<'a, M: Clone + 'a>(
    colors: Colors,
    hint: &'a str,
    value: &'a str,
    secret: bool,
    id: &'static str,
    typed: impl Fn(String) -> M + 'a,
    entered: M,
) -> Element<'a, M> {
    text_input(hint, value)
        .id(id)
        .secure(secret)
        .on_input(typed)
        .on_submit(entered)
        .size(TEXT_SIZE)
        .padding([6, 8])
        .width(Length::Fixed(FIELD))
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
        .into()
}

/// The operation that puts the cursor in a field.
pub fn focus(id: &'static str) -> iced::Task<crate::ui::Message> {
    iced::widget::operation::focus(id)
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
