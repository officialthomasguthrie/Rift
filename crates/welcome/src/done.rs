//! The Done page, and the rows that say how each install is going, which the Apps page shows too.
//! Done writes the note that says the drive has been welcomed and closes the window; the installs
//! go on after it.

use iced::widget::{column, container, row, text};
use iced::{Center, Element, Fill};

use crate::install::Doing;
use crate::theme::Colors;
use crate::ui::{Message, Welcome};
use crate::widgets::{GAP, TEXT_SIZE, group, heading, line, note, progress};

/// The page.
pub fn view(state: &Welcome, look: Colors) -> Element<'_, Message> {
    let said = if state.busy() {
        "The apps you chose go on installing after Welcome closes, and each one is in the \
         Applications menu once it is done."
    } else if state
        .installs
        .iter()
        .any(|install| install.doing == Doing::Installed)
    {
        "The apps you chose are in the Applications menu."
    } else {
        "Rift is ready to use."
    };
    let mut page = column![
        line(look, said),
        note(
            look,
            "Welcome stays in the Applications menu if you want it again, and Settings has \
             everything on these pages and more.",
        ),
    ]
    .spacing(GAP)
    .width(Fill);
    if !state.installs.is_empty() {
        page = page.push(installs(state, look));
    }
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// A row for every install asked for: the app, and how it is going at the right, with what
/// flatpak said under the name when it could not.
pub fn installs<'a>(state: &'a Welcome, look: Colors) -> Element<'a, Message> {
    let rows = state
        .installs
        .iter()
        .map(|install| {
            let mut left = column![line(look, &install.name)].spacing(2);
            if let Doing::Failed(why) = &install.doing {
                left = left.push(text(why).size(TEXT_SIZE).color(look.error));
            }
            let right: Element<'a, Message> = match &install.doing {
                Doing::Waiting => note(look, "Waiting"),
                Doing::Running(percent) => progress(look, *percent),
                Doing::Installed => note(look, "Installed"),
                Doing::Failed(_) => text("Could not install")
                    .size(TEXT_SIZE)
                    .color(look.error)
                    .into(),
            };
            container(row![left.width(Fill), right].align_y(Center).spacing(GAP))
                .width(Fill)
                .padding([8, 12])
                .into()
        })
        .collect();
    let title = if state.busy() {
        "Installing"
    } else if state
        .installs
        .iter()
        .all(|install| install.doing == Doing::Installed)
    {
        "Installed"
    } else {
        "Installs"
    };
    column![heading(look, title), group(look, rows)]
        .spacing(8)
        .into()
}
