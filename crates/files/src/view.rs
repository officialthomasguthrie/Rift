//! How a window is drawn: the header bar along the top with the way back and forward and the path
//! of the folder, the places down the left, the list beside them, and over the list whatever stands
//! there for a moment: the progress of a job, a toast, what is selected, a menu or a dialog.

use std::path::{Path, PathBuf};

use iced::widget::{button, column, container, mouse_area, opaque, pin, row, space, stack, text};
use iced::{Border, Center, Color, Element, Fill, Length, Theme, window};

use crate::actions::PATIENCE;
use crate::browser::{Browser, Location};
use crate::dialogs;
use crate::icons;
use crate::jobs::Job;
use crate::list::{self, HEADS, path_id};
use crate::menus;
use crate::theme::Colors;
use crate::ui::{Act, Files, Message};
use crate::widgets::{
    BOLD, TEXT_SIZE, action, fill, menu, progress, scroll, shade, toast, tool, wide_field,
};

/// How wide the sidebar is.
pub const SIDEBAR: f32 = 208.0;
/// How tall the header bar is. It is the title bar of the window, which the app draws itself.
pub const HEADER: f32 = 45.0;
/// Where the first row of the list is from the top of the window.
pub const LIST_TOP: f32 = HEADER + 1.0 + HEADS + 1.0;
/// How tall a row of the sidebar is.
const SIDE_ROW: f32 = 34.0;
/// How many parts of a path the path bar shows before it leaves out the middle.
const CRUMBS: usize = 5;

/// A window.
pub fn window(state: &Files, id: window::Id) -> Element<'_, Message> {
    let Some(browser) = state.windows.get(&id) else {
        return space().into();
    };
    let look = state.colors();
    let body = row![
        sidebar(state, id, browser, look),
        container(list::view(state, id, browser, look))
            .width(Fill)
            .height(Fill)
            .style(move |_: &Theme| fill(look.view)),
    ]
    .height(Fill);
    let mut layers: Vec<Element<'_, Message>> =
        vec![column![header(browser, id, look), body].into()];
    if let Some(bottom) = bottom(state, id, browser, look) {
        layers.push(
            container(bottom)
                .width(Fill)
                .height(Fill)
                .padding(iced::Padding {
                    left: SIDEBAR + 16.0,
                    ..iced::Padding::new(16.0)
                })
                .align_bottom(Fill)
                .center_x(Fill)
                .into(),
        );
    }
    if let Some(summary) = summary(browser, look) {
        layers.push(
            container(summary)
                .width(Fill)
                .height(Fill)
                .padding(12)
                .align_bottom(Fill)
                .align_right(Fill)
                .into(),
        );
    }
    if let Some(open) = &browser.menu {
        // a press anywhere else closes the menu, the way a popover goes
        layers.push(
            mouse_area(space().width(Fill).height(Fill))
                .on_press(Message::CloseMenu(id))
                .on_right_press(Message::CloseMenu(id))
                .into(),
        );
        let items = menus::items(state, browser, id, open);
        layers.push(pin(menu(look, items)).x(open.at.x).y(open.at.y).into());
    }
    if let Some(shown) = &browser.dialog {
        let folder = browser
            .location
            .folder()
            .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
        layers.push(opaque(shade()));
        layers.push(
            container(dialogs::view(shown, look, id, browser.number, &folder))
                .width(Fill)
                .height(Fill)
                .center(Fill)
                .into(),
        );
    }
    stack(layers).into()
}

/// The title bar the app draws for itself: the app's name over the sidebar, back and forward, the
/// path of the folder, the menu, and the close button at the right end, the way GNOME's Files lays
/// its own out.
fn header(browser: &Browser, id: window::Id, look: Colors) -> Element<'_, Message> {
    let name = container(text("Files").size(TEXT_SIZE).font(BOLD).color(look.text))
        .width(Length::Fixed(SIDEBAR - 8.0))
        .padding([0, 6])
        .center_y(Fill);
    let back = tool(
        look,
        "go-previous-symbolic",
        (!browser.back.is_empty()).then_some(Message::Back(id)),
    );
    let forward = tool(
        look,
        "go-next-symbolic",
        (!browser.forward.is_empty()).then_some(Message::Forward(id)),
    );
    let close = button(icons::symbolic(look.text, "window-close-symbolic", 16.0))
        .padding(7)
        .on_press(Message::Do(id, Act::Close))
        .style(move |_: &Theme, status| button::Style {
            background: Some(
                match status {
                    button::Status::Hovered | button::Status::Pressed => look.hover,
                    _ => look.button,
                }
                .into(),
            ),
            text_color: look.text,
            border: Border {
                radius: 15.0.into(),
                ..Border::default()
            },
            ..button::Style::default()
        });
    let line = container(space().height(1.0).width(Fill)).style(move |_: &Theme| fill(look.line));
    column![
        container(
            row![
                name,
                back,
                forward,
                path_bar(browser, id, look),
                tool(look, "open-menu-symbolic", Some(Message::MainMenu(id))),
                close,
            ]
            .align_y(Center)
            .spacing(4)
            .padding([0, 8]),
        )
        .width(Fill)
        .height(Length::Fixed(HEADER))
        .style(move |_: &Theme| fill(look.header)),
        line,
    ]
    .into()
}

/// The path of the folder as a button for each folder on the way to it, the last in bold, in a box
/// of its own. A press between them, or Ctrl and L, makes it a field to type a path in.
fn path_bar(browser: &Browser, id: window::Id, look: Colors) -> Element<'_, Message> {
    if let Some(typed) = &browser.typing {
        return container(wide_field(
            look,
            "A folder to go to",
            typed,
            path_id(browser.number),
            move |typed| Message::PathTyped(id, typed),
            Message::PathEntered(id),
        ))
        .width(Fill)
        .padding([0, 6])
        .into();
    }
    let mut crumbs = row![].spacing(0).align_y(Center);
    match &browser.location {
        Location::Trash => {
            crumbs = crumbs.push(crumb(look, "Trash".to_string(), true, None));
        }
        Location::Folder(path) => {
            let parts = parts(path);
            let count = parts.len();
            let hidden_to = count.saturating_sub(CRUMBS - 2);
            for (at, (label, place)) in parts.into_iter().enumerate() {
                // a deep folder shows where it starts and the last few folders on the way, with a
                // button for the last of the ones left out between them
                if count > CRUMBS && at > 0 && at < hidden_to {
                    if at + 1 == hidden_to {
                        crumbs = crumbs.push(slash(look));
                        crumbs = crumbs.push(tool(
                            look,
                            "pan-start-symbolic",
                            Some(Message::Go(id, Location::Folder(place))),
                        ));
                    }
                    continue;
                }
                if at > 0 {
                    crumbs = crumbs.push(slash(look));
                }
                let last = at + 1 == count;
                crumbs = crumbs.push(crumb(
                    look,
                    label,
                    last,
                    Some(Message::Go(id, Location::Folder(place))),
                ));
            }
        }
    }
    let boxed = container(crumbs.padding([0, 4]))
        .width(Fill)
        .height(Length::Fixed(32.0))
        .center_y(Length::Fixed(32.0))
        .style(move |_: &Theme| container::Style {
            background: Some(look.field.into()),
            border: Border {
                color: look.edge,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        });
    container(mouse_area(boxed).on_press(Message::Do(id, Act::Location)))
        .width(Fill)
        .padding([0, 6])
        .into()
}

/// The folders on the way to a folder, with their names: Home and what is under it, or the root
/// and what is under that.
fn parts(path: &Path) -> Vec<(String, PathBuf)> {
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from);
    let (mut parts, start) = match home.as_deref().filter(|home| path.starts_with(home)) {
        Some(home) => (
            vec![("Home".to_string(), home.to_path_buf())],
            home.to_path_buf(),
        ),
        None => (
            vec![("/".to_string(), PathBuf::from("/"))],
            PathBuf::from("/"),
        ),
    };
    let mut place = start.clone();
    if let Ok(rest) = path.strip_prefix(&start) {
        for part in rest.components() {
            place.push(part);
            parts.push((
                part.as_os_str().to_string_lossy().into_owned(),
                place.clone(),
            ));
        }
    }
    parts
}

fn slash<'a>(look: Colors) -> Element<'a, Message> {
    text("/").size(TEXT_SIZE).color(look.dim).into()
}

/// One folder of the path bar, pressed to go there.
fn crumb<'a>(
    look: Colors,
    label: String,
    last: bool,
    press: Option<Message>,
) -> Element<'a, Message> {
    let words = text(label)
        .size(TEXT_SIZE)
        .color(look.text)
        .font(if last { BOLD } else { crate::widgets::FONT })
        .wrapping(text::Wrapping::None);
    let mut pressable = button(words)
        .padding([4, 8])
        .style(move |_: &Theme, status| button::Style {
            background: Some(
                match status {
                    button::Status::Hovered | button::Status::Pressed => look.hover,
                    _ => Color::TRANSPARENT,
                }
                .into(),
            ),
            text_color: look.text,
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

/// The places, one row each, the one the window shows in the accent, and the trash last.
fn sidebar<'a>(
    state: &'a Files,
    id: window::Id,
    browser: &Browser,
    look: Colors,
) -> Element<'a, Message> {
    let mut rows = column![].width(Fill).spacing(2).padding([8, 8]);
    for place in &state.places {
        let here = browser.location.folder() == Some(place.path.as_path());
        rows = rows.push(side_row(
            look,
            place.icon,
            place.name.clone(),
            here,
            Message::Go(id, Location::Folder(place.path.clone())),
        ));
    }
    rows = rows.push(space().height(8.0));
    rows = rows.push(side_row(
        look,
        if state.trash_full {
            "user-trash-full-symbolic"
        } else {
            "user-trash-symbolic"
        },
        "Trash".to_string(),
        browser.location == Location::Trash,
        Message::Go(id, Location::Trash),
    ));
    row![
        container(scroll(look, rows).height(Fill))
            .width(Length::Fixed(SIDEBAR))
            .height(Fill)
            .style(move |_: &Theme| fill(look.side)),
        container(space().width(1.0).height(Fill)).style(move |_: &Theme| fill(look.line)),
    ]
    .into()
}

/// One row of the sidebar, the way Settings draws its own.
fn side_row<'a>(
    look: Colors,
    icon: &str,
    label: String,
    here: bool,
    press: Message,
) -> Element<'a, Message> {
    let colour = if here { look.on_accent } else { look.text };
    // a button lays its content out at the top of its box, so the row is centred by hand
    button(
        container(
            row![
                icons::symbolic(colour, icon, 16.0),
                text(label).size(TEXT_SIZE).color(colour),
            ]
            .align_y(Center)
            .spacing(10),
        )
        .center_y(Fill),
    )
    .width(Fill)
    .height(Length::Fixed(SIDE_ROW))
    .padding([0, 10])
    .on_press(press)
    .style(move |_: &Theme, status| button::Style {
        background: Some(
            match (here, status) {
                (true, _) => look.accent,
                (false, button::Status::Hovered | button::Status::Pressed) => look.hover,
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
    })
    .into()
}

/// What stands at the bottom of the list: the job this window started that has taken a while, with
/// how far it has got, or else the toast.
fn bottom<'a>(
    state: &'a Files,
    id: window::Id,
    browser: &'a Browser,
    look: Colors,
) -> Option<Element<'a, Message>> {
    let slow = state
        .jobs
        .iter()
        .find(|job| job.running() && job.window == Some(id) && job.started.elapsed() >= PATIENCE);
    if let Some(job) = slow {
        return Some(working(job, id, look));
    }
    let said = browser.toast.as_ref()?;
    Some(toast(
        look,
        said.said.clone(),
        said.undo
            .map(|number| ("Undo", Message::Do(id, Act::Undo(number)))),
    ))
}

/// A job that is running: what it is doing, how far it has got, and Stop when it can stop.
fn working(job: &Job, id: window::Id, look: Colors) -> Element<'_, Message> {
    let mut inside = row![
        text(job.work.doing()).size(TEXT_SIZE).color(look.text),
        progress(look, job.percent()),
    ]
    .spacing(16)
    .align_y(Center);
    if job.work.stoppable() {
        inside = inside.push(action(
            look,
            "Stop",
            Some(Message::Do(id, Act::Stop(job.number))),
        ));
    }
    container(inside)
        .padding([8, 14])
        .style(move |_: &Theme| container::Style {
            background: Some(look.header.into()),
            border: Border {
                color: look.edge,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// What is selected, at the bottom right of the list, the way GNOME's Files says it: one thing by
/// its name with its size or what it holds, several by how many and their size together.
fn summary<'a>(browser: &Browser, look: Colors) -> Option<Element<'a, Message>> {
    if browser.toast.is_some() || browser.menu.is_some() {
        return None;
    }
    let chosen = browser.chosen();
    let said = match chosen.as_slice() {
        [] => return None,
        [one] => match (one.kind, one.items) {
            (librift::files::Kind::Folder, Some(items)) => format!(
                "{} selected, {}",
                one.label,
                librift::files::items_words(items).to_lowercase()
            ),
            (librift::files::Kind::Folder, None) => format!("{} selected", one.label),
            _ => format!(
                "{} selected, {}",
                one.label,
                librift::files::size_words(one.size)
            ),
        },
        more => {
            let bytes: u64 = more.iter().map(|entry| entry.size).sum();
            format!(
                "{} items selected, {}",
                more.len(),
                librift::files::size_words(bytes)
            )
        }
    };
    Some(
        container(
            text(said)
                .size(TEXT_SIZE)
                .color(look.dim)
                .wrapping(text::Wrapping::None),
        )
        .padding([6, 10])
        .style(move |_: &Theme| container::Style {
            background: Some(look.header.into()),
            border: Border {
                color: look.edge,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        })
        .into(),
    )
}
