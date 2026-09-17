//! `rift wallpaper`: the picture or the flat colour behind the windows. Lists the photographs Rift
//! ships and the flat grays, and sets the wallpaper to one of them, to any JPEG or PNG picture, or
//! to a colour.

use std::env;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use librift::wallpaper::{self, Shipped, Wallpaper};

use crate::text;

const USAGE: &str =
    "Usage: rift wallpaper [list]\n       rift wallpaper set <name, picture or #rrggbb>";

const HELP: &str = "The wallpaper is a photograph or a picture of your own, which Horizon scales \
to fill each screen, or a flat colour. list shows the photographs Rift ships, with who took each \
one, and the flat grays, and says what the desktop shows now. set takes a name from the list, the \
path of a JPEG or PNG picture, or a colour as # and six hex digits, and the desktop changes at \
once. Each photograph has a text file next to it with its source and its license.";

pub fn run(args: &[String]) -> ExitCode {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["list"] => list(),
        ["--help" | "-h", ..] => {
            println!("{USAGE}\n\n{HELP}");
            ExitCode::SUCCESS
        }
        ["set", choice] => set(choice),
        ["set"] => {
            eprintln!("rift wallpaper set: a name, a picture or a colour is needed\n{USAGE}");
            ExitCode::from(2)
        }
        ["list", extra, ..] | ["set", _, extra, ..] | [extra, ..] => {
            text::unknown("wallpaper", extra, USAGE)
        }
    }
}

fn list() -> ExitCode {
    print!("{}", rows(&wallpaper::shipped(), &Wallpaper::read()));
    ExitCode::SUCCESS
}

fn set(choice: &str) -> ExitCode {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let chosen = match wallpaper::choose(choice, &cwd) {
        Ok(chosen) => chosen,
        Err(why) => {
            eprintln!("{why}");
            return ExitCode::FAILURE;
        }
    };
    match wallpaper::set(&chosen) {
        Ok(()) => {
            println!("{}", showing(&chosen));
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// What the desktop shows, in a sentence.
fn showing(current: &Wallpaper) -> String {
    format!("The wallpaper is {current}.")
}

/// The photographs and the grays under a header, their columns lined up, then what is up now.
fn rows(photos: &[Shipped], current: &Wallpaper) -> String {
    let mut lines: Vec<(&str, &str, &str)> = photos
        .iter()
        .map(|photo| {
            (
                photo.name.as_str(),
                photo.title.as_str(),
                photo.credit.as_str(),
            )
        })
        .collect();
    lines.extend(
        wallpaper::GRAYS
            .iter()
            .map(|(color, name)| (*color, *name, "")),
    );

    let name_width = lines
        .iter()
        .map(|(name, _, _)| name.len())
        .chain(["Name".len()])
        .max()
        .unwrap_or(0);
    let title_width = lines
        .iter()
        .map(|(_, title, _)| title.len())
        .chain(["Title".len()])
        .max()
        .unwrap_or(0);
    let mut out = format!(
        "{:<name_width$}  {:<title_width$}  Credit\n",
        "Name", "Title"
    );
    for (name, title, credit) in lines {
        let line = format!("{name:<name_width$}  {title:<title_width$}  {credit}");
        let _ = writeln!(out, "{}", line.trim_end());
    }
    let _ = writeln!(out, "\n{}", showing(current));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(name: &str, title: &str, credit: &str) -> Shipped {
        Shipped {
            name: name.to_string(),
            title: title.to_string(),
            credit: credit.to_string(),
            path: PathBuf::from(format!("{}/{name}.jpg", wallpaper::SHIPPED)),
        }
    }

    #[test]
    fn the_list_lines_up_and_says_what_is_up() {
        let photos = [
            photo("aurora", "Aurora over Manitoba", "NASA"),
            photo("earthset", "Earthset", "NASA"),
        ];
        let current = Wallpaper::Picture(photos[1].path.clone());
        assert_eq!(
            rows(&photos, &current),
            "Name      Title                 Credit\n\
             aurora    Aurora over Manitoba  NASA\n\
             earthset  Earthset              NASA\n\
             #242424   Dark gray\n\
             #f2f1f0   Light gray\n\
             \n\
             The wallpaper is earthset.\n"
        );
    }

    #[test]
    fn a_picture_of_your_own_goes_by_its_path_and_a_colour_by_itself() {
        assert_eq!(
            showing(&Wallpaper::Picture("/home/rift/Pictures/lake.jpg".into())),
            "The wallpaper is /home/rift/Pictures/lake.jpg."
        );
        assert_eq!(
            showing(&Wallpaper::Color("#242424".into())),
            "The wallpaper is #242424."
        );
    }

    #[test]
    fn with_nothing_shipped_the_grays_are_still_there() {
        let listed = rows(&[], &Wallpaper::Color("#242424".into()));
        assert!(listed.starts_with("Name     Title       Credit\n#242424  Dark gray\n"));
        assert!(listed.ends_with("\nThe wallpaper is #242424.\n"));
    }
}
