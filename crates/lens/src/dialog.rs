//! The dialogs the shell asks in: whether to log out, restart or shut down, and the password for a
//! network. A plain dialog in the middle of the screen with a title, one sentence and two buttons,
//! on a surface of its own that holds the keyboard until it is answered, so a stray click on a
//! window does not throw away a password half typed.

use iced::widget::{button, column, container, row, space, text, text_input};
use iced::{Background, Border, Color, Element, Length, Shadow, Theme, window};
use librift::network::Network;
use librift::os::Action;

use crate::bar;
use crate::theme::Palette;
use crate::ui::{HEADING, Message};

/// How wide a dialog is in logical pixels.
pub const WIDTH: u32 = 400;
/// The padding inside it.
const PAD: u32 = 16;
/// The same padding where a widget wants it.
const INSIDE: u16 = 16;
/// The line the title, the sentence and the line under the field each take.
const LINE: u32 = 20;
/// The field and the buttons.
const CONTROL: u32 = 32;
/// The gap between the title and the sentence, and between the field and its line.
const GAP: u32 = 8;
/// The gap before the field and before the buttons.
const SECTION: u32 = 16;
/// The padding at each end of a button's word.
const BUTTON_PAD: u16 = 16;
/// The line of text inside the field: 18 + 7 + 7 is the field's height.
const FIELD_LINE: f32 = 18.0;
/// The padding inside the field.
const FIELD_PAD: u16 = 7;
/// The field's widget id, for the focus operation.
const FIELD_ID: &str = "password";

/// What a dialog asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// Whether to run a command that ends the session or the machine's run: log out, restart or
    /// shut down, planned the way the field plans them.
    Command(Action),
    /// The password for a network, to join it through this wireless card.
    Password {
        /// The network.
        network: Network,
        /// The card's object on the bus.
        device: String,
    },
}

/// What a dialog's field and buttons ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// New characters in the field.
    Input(String),
    /// The default button, or Enter.
    Confirm,
    /// Cancel, or Escape.
    Cancel,
}

/// A dialog while it is open.
#[derive(Debug)]
pub struct Dialog {
    /// The surface it draws on.
    pub id: window::Id,
    /// What it asks.
    pub ask: Ask,
    /// What is in the password field.
    pub input: String,
    /// The answer is being acted on: the buttons wait.
    pub busy: bool,
    /// What went wrong the last time.
    pub error: Option<String>,
}

impl Dialog {
    /// A dialog that has just opened.
    #[must_use]
    pub const fn new(id: window::Id, ask: Ask) -> Self {
        Self {
            id,
            ask,
            input: String::new(),
            busy: false,
            error: None,
        }
    }

    /// The title: the command as a question, or the network to join.
    #[must_use]
    pub fn title(&self) -> String {
        match &self.ask {
            Ask::Command(action) => format!("{}?", action.summary),
            Ask::Password { network, .. } => format!("Connect to {}", network.name),
        }
    }

    /// The one sentence under the title.
    #[must_use]
    pub const fn sentence(&self) -> &'static str {
        match &self.ask {
            Ask::Command(_) => "Apps that are open will close.",
            Ask::Password { .. } => "Type the password for this network.",
        }
    }

    /// The word on the default button.
    #[must_use]
    pub fn verb(&self) -> &'static str {
        match &self.ask {
            Ask::Command(action) => match action.args.first().map(String::as_str) {
                Some("reboot") => "Restart",
                Some("poweroff") => "Shut down",
                _ => "Log out",
            },
            Ask::Password { .. } => "Connect",
        }
    }

    /// Whether the default button can be pressed: nothing is running, and a password is long
    /// enough for the network.
    #[must_use]
    pub fn ready(&self) -> bool {
        !self.busy
            && match &self.ask {
                Ask::Command(_) => true,
                Ask::Password { network, .. } => {
                    self.input.chars().count() >= network.security.shortest()
                }
            }
    }

    /// The line under the field: what went wrong, or that it is connecting.
    #[must_use]
    pub fn line(&self) -> Option<(String, bool)> {
        if let Some(why) = &self.error {
            return Some((why.clone(), true));
        }
        match &self.ask {
            Ask::Password { network, .. } if self.busy => {
                Some((format!("Connecting to {}", network.name), false))
            }
            _ => None,
        }
    }

    /// How tall the dialog is. A password dialog keeps a line under its field whether or not it
    /// has anything to say, so it does not jump while it connects.
    #[must_use]
    pub const fn height(&self) -> u32 {
        let top = PAD + LINE + GAP + LINE;
        let field = match &self.ask {
            Ask::Command(_) => 0,
            Ask::Password { .. } => SECTION + CONTROL + GAP + LINE,
        };
        top + field + SECTION + CONTROL + PAD
    }
}

/// The dialog: the title, the sentence, the field for a password, and the two buttons at the
/// right, on the menu's gray inside a border.
pub fn view(look: Palette, dialog: &Dialog) -> Element<'_, Message> {
    let title = container(
        text(dialog.title())
            .size(bar::TEXT_SIZE)
            .font(HEADING)
            .color(look.text)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(LINE)
    .align_y(iced::Center)
    .clip(true);
    let failed = matches!(dialog.ask, Ask::Command(_)) && dialog.error.is_some();
    let sentence = container(
        text(if failed {
            dialog.error.clone().unwrap_or_default()
        } else {
            dialog.sentence().to_string()
        })
        .size(bar::TEXT_SIZE)
        .color(if failed { look.error } else { look.dim })
        .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(LINE)
    .align_y(iced::Center)
    .clip(true);
    let mut body = column![title, space().height(GAP), sentence];
    if let Ask::Password { .. } = dialog.ask {
        let field = text_input("Password", &dialog.input)
            .id(FIELD_ID)
            .secure(true)
            .on_input_maybe((!dialog.busy).then_some(|value| Message::Dialog(Event::Input(value))))
            .on_submit(Message::Dialog(Event::Confirm))
            .size(bar::TEXT_SIZE)
            .line_height(iced::widget::text::LineHeight::Absolute(FIELD_LINE.into()))
            .padding([FIELD_PAD, 8])
            .style(move |_: &Theme, status| field_style(look, status));
        let (said, wrong) = dialog.line().unwrap_or_default();
        let under = container(
            text(said)
                .size(bar::TEXT_SIZE)
                .color(if wrong { look.error } else { look.dim })
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Length::Fill)
        .height(LINE)
        .align_y(iced::Center)
        .clip(true);
        body = body
            .push(space().height(SECTION))
            .push(field)
            .push(space().height(GAP))
            .push(under);
    }
    let buttons = row![
        space().width(Length::Fill),
        dialog_button(look, "Cancel", false, Some(Message::Dialog(Event::Cancel))),
        dialog_button(
            look,
            dialog.verb(),
            true,
            dialog.ready().then_some(Message::Dialog(Event::Confirm))
        ),
    ]
    .spacing(8)
    .align_y(iced::Center);
    body = body.push(space().height(SECTION)).push(buttons);
    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(INSIDE)
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

/// A button of the dialog: the default one in the accent, the other with a border.
fn dialog_button(
    look: Palette,
    label: &'static str,
    default: bool,
    on_press: Option<Message>,
) -> Element<'static, Message> {
    let enabled = on_press.is_some();
    let word = container(text(label).size(bar::TEXT_SIZE)).center_y(Length::Fill);
    button(word)
        .height(CONTROL)
        .padding([0, BUTTON_PAD])
        .on_press_maybe(on_press)
        .style(move |_: &Theme, state| {
            let (fill, words, edge) = if default {
                let fill = match state {
                    button::Status::Disabled => look.press,
                    _ => look.accent,
                };
                (
                    Some(fill),
                    if enabled { look.selected } else { look.dim },
                    Color::TRANSPARENT,
                )
            } else {
                let fill = match state {
                    button::Status::Hovered => Some(look.hover),
                    button::Status::Pressed => Some(look.press),
                    button::Status::Active | button::Status::Disabled => None,
                };
                (fill, look.text, look.edge)
            };
            button::Style {
                background: fill.map(Background::from),
                text_color: words,
                border: Border {
                    color: edge,
                    width: if default { 0.0 } else { 1.0 },
                    radius: 4.0.into(),
                },
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .into()
}

fn field_style(look: Palette, status: text_input::Status) -> text_input::Style {
    let (edge, width) = match status {
        text_input::Status::Focused { .. } => (look.accent, 2.0),
        _ => (look.edge, 1.0),
    };
    text_input::Style {
        background: look.field.into(),
        border: Border {
            color: edge,
            width,
            radius: 4.0.into(),
        },
        icon: look.text,
        placeholder: look.dim,
        value: look.text,
        selection: Color {
            a: 0.4,
            ..look.accent
        },
    }
}

/// The operation that puts the cursor in the password field.
pub fn focus_field() -> iced::Task<Message> {
    iced::widget::operation::focus(FIELD_ID)
}

#[cfg(test)]
mod tests {
    use super::*;
    use librift::network::Security;
    use librift::os;

    fn command(words: &[&str]) -> Action {
        match os::parse(words) {
            Some(Ok(action)) => action,
            other => panic!("{words:?} is not a command: {other:?}"),
        }
    }

    fn password() -> Ask {
        Ask::Password {
            network: Network {
                name: "Home".into(),
                ssid: b"Home".to_vec(),
                strength: 70,
                security: Security::Password,
                point: "/ap/1".into(),
                active: false,
                saved: None,
            },
            device: "/devices/3".into(),
        }
    }

    #[test]
    fn a_command_is_asked_as_a_question_with_its_own_verb() {
        let dialog = Dialog::new(
            window::Id::unique(),
            Ask::Command(command(&["power", "reboot"])),
        );
        assert_eq!(dialog.title(), "Restart the computer?");
        assert_eq!(dialog.verb(), "Restart");
        assert!(dialog.ready());
        let off = Dialog::new(
            window::Id::unique(),
            Ask::Command(command(&["power", "off"])),
        );
        assert_eq!(off.title(), "Turn the computer off?");
        assert_eq!(off.verb(), "Shut down");
        let out = Dialog::new(
            window::Id::unique(),
            Ask::Command(command(&["power", "logout"])),
        );
        assert_eq!(out.title(), "Log out?");
        assert_eq!(out.verb(), "Log out");
        assert_eq!(
            dialog.height(),
            PAD + LINE + GAP + LINE + SECTION + CONTROL + PAD
        );
    }

    #[test]
    fn a_password_has_to_be_long_enough_before_it_is_tried() {
        let mut dialog = Dialog::new(window::Id::unique(), password());
        assert_eq!(dialog.title(), "Connect to Home");
        assert!(!dialog.ready());
        dialog.input = "seven77".into();
        assert!(!dialog.ready());
        dialog.input = "eight888".into();
        assert!(dialog.ready());
        dialog.busy = true;
        assert!(!dialog.ready(), "nothing is pressed twice");
        assert_eq!(
            dialog.line(),
            Some(("Connecting to Home".to_string(), false))
        );
        dialog.busy = false;
        dialog.error = Some("Could not connect to Home. Check the password and try again.".into());
        assert!(dialog.line().is_some_and(|(_, wrong)| wrong));
        // the line is there whether or not it says anything, so the height does not change
        let command = Dialog::new(
            window::Id::unique(),
            Ask::Command(command(&["power", "off"])),
        );
        assert_eq!(
            dialog.height(),
            command.height() + SECTION + CONTROL + GAP + LINE
        );
    }
}
