//! The Developer page: the languages on the drive and the commands that run them, then the
//! languages and databases that are not, each as the Podman command that runs it in a container
//! of its own, with a button that copies the command. Podman is on the drive and runs rootless for
//! the owner; the first run of each downloads its image.

use iced::widget::{column, container, row, text};
use iced::{Center, Element, Fill, Length, Task};

use crate::theme::Colors;
use crate::ui::{Message, Welcome};
use crate::widgets::{GAP, MONO, TEXT_SIZE, action, group, heading, line, note};

/// How wide the name of a language on the drive is drawn, so the commands line up.
const NAME: f32 = 130.0;

/// The languages on the drive, and the commands that run them.
const ON_DRIVE: [(&str, &str); 7] = [
    ("Rust", "cargo, rustc"),
    ("C and C++", "cc, c++, clang, cmake, make, ninja"),
    ("Python", "python3"),
    ("JavaScript", "node, npm, bun"),
    ("Go", "go"),
    ("Zig", "zig"),
    ("Java", "java, javac"),
];

/// The rest, in containers: the name, the word `--set copy` takes for it, and the command. The
/// databases listen on this machine only.
pub const CONTAINERS: [(&str, &str, &str); 10] = [
    (
        "Haskell",
        "haskell",
        "podman run -it --rm docker.io/library/haskell ghci",
    ),
    (
        "Julia",
        "julia",
        "podman run -it --rm docker.io/library/julia",
    ),
    (
        "Ruby",
        "ruby",
        "podman run -it --rm docker.io/library/ruby irb",
    ),
    ("PHP", "php", "podman run -it --rm docker.io/library/php -a"),
    (
        "Elixir",
        "elixir",
        "podman run -it --rm docker.io/library/elixir",
    ),
    (
        "Erlang",
        "erlang",
        "podman run -it --rm docker.io/library/erlang",
    ),
    ("R", "r", "podman run -it --rm docker.io/library/r-base"),
    (
        "PostgreSQL",
        "postgres",
        "podman run -d -p 127.0.0.1:5432:5432 -e POSTGRES_PASSWORD=postgres docker.io/library/postgres",
    ),
    (
        "MariaDB",
        "mariadb",
        "podman run -d -p 127.0.0.1:3306:3306 -e MARIADB_ROOT_PASSWORD=mariadb docker.io/library/mariadb",
    ),
    (
        "Redis",
        "redis",
        "podman run -d -p 127.0.0.1:6379:6379 docker.io/library/redis",
    ),
];

/// The place in the list of the command a word names.
#[must_use]
pub fn named(word: &str) -> Option<usize> {
    CONTAINERS.iter().position(|(name, short, _)| {
        short.eq_ignore_ascii_case(word) || name.eq_ignore_ascii_case(word)
    })
}

/// Copy the command at this place in the list, the way the button does.
pub fn copy(state: &mut Welcome, at: usize) -> Task<Message> {
    let Some((_, _, command)) = CONTAINERS.get(at) else {
        return Task::none();
    };
    state.copied = Some(at);
    iced::clipboard::write((*command).to_string())
}

/// The page.
pub fn view(state: &Welcome, look: Colors) -> Element<'_, Message> {
    let drive = ON_DRIVE
        .iter()
        .map(|(name, commands)| {
            container(
                row![
                    container(line(look, name)).width(Length::Fixed(NAME)),
                    text(*commands)
                        .size(TEXT_SIZE)
                        .font(MONO)
                        .color(look.dim)
                        .width(Fill),
                ]
                .align_y(Center)
                .spacing(GAP),
            )
            .width(Fill)
            .padding([8, 12])
            .into()
        })
        .collect();
    let contained = CONTAINERS
        .iter()
        .enumerate()
        .map(|(at, (name, _, command))| {
            let label = if state.copied == Some(at) {
                "Copied"
            } else {
                "Copy"
            };
            container(
                row![
                    column![
                        line(look, name),
                        text(*command).size(TEXT_SIZE).font(MONO).color(look.dim),
                    ]
                    .spacing(2)
                    .width(Fill),
                    action(look, label, Some(Message::Copy(at))),
                ]
                .align_y(Center)
                .spacing(GAP),
            )
            .width(Fill)
            .padding([8, 12])
            .into()
        })
        .collect();
    column![
        column![
            heading(look, "On this drive"),
            note(look, "These work with no network."),
            group(look, drive),
        ]
        .spacing(8),
        column![
            heading(look, "In a container"),
            note(
                look,
                "Podman is on the drive and runs each of these in a container of its own. The \
                 first run downloads it. To work on the folder you are in, add -v \"$PWD:/work\" \
                 -w /work to the command.",
            ),
            group(look, contained),
        ]
        .spacing(8),
    ]
    .spacing(GAP)
    .width(Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_named_by_its_word() {
        for (at, (name, short, command)) in CONTAINERS.iter().enumerate() {
            assert_eq!(named(short), Some(at));
            assert_eq!(named(name), Some(at));
            // podman, and an image named in full, since a short name asks which registry
            assert!(command.starts_with("podman run "), "{command}");
            assert!(command.contains(" docker.io/library/"), "{command}");
        }
        assert_eq!(named("cobol"), None);
    }
}
