//! `rift guide`: the guide that comes with the system. It is a set of pages on the drive itself,
//! so it is there on a machine with no network, and it opens in the browser. The Applications menu
//! has a row that runs this command.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

const USAGE: &str = "Usage: rift guide [<page>]\n       rift guide list";

const HELP: &str = "The guide is on the drive, so it is there with no network. With no page it \
opens at the contents; with a page it opens that one. list prints the pages.";

/// Where the pages are. The image writes them there; `RIFT_GUIDE` points somewhere else while one
/// is being written.
const GUIDE: &str = "/etc/rift/guide";
/// The page the guide opens at.
const FIRST: &str = "index";

pub fn run(args: &[String]) -> ExitCode {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] => open(FIRST),
        ["--help" | "-h", ..] => {
            println!("{USAGE}\n\n{HELP}");
            ExitCode::SUCCESS
        }
        ["list"] => list(),
        [page] => open(page),
        [_, extra, ..] => crate::text::unknown("guide", extra, USAGE),
    }
}

/// Where the guide is.
fn folder() -> PathBuf {
    env::var_os("RIFT_GUIDE").map_or_else(|| PathBuf::from(GUIDE), PathBuf::from)
}

/// The pages, in the order the contents lists them, each with the heading at the top of it.
fn pages(folder: &Path) -> Vec<(String, String)> {
    let Ok(contents) = fs::read_to_string(folder.join(format!("{FIRST}.html"))) else {
        return Vec::new();
    };
    let mut pages = Vec::new();
    for rest in contents.split("href=\"").skip(1) {
        let Some((link, _)) = rest.split_once('"') else {
            continue;
        };
        let Some(name) = link.strip_suffix(".html") else {
            continue;
        };
        // every page has the contents at the top of it, this one too, and the contents is not one
        // of the pages it lists
        if name == FIRST
            || name.contains('/')
            || pages.iter().any(|(had, _): &(String, String)| had == name)
        {
            continue;
        }
        let page = folder.join(link);
        let heading = fs::read_to_string(&page)
            .ok()
            .and_then(|text| heading(&text))
            .unwrap_or_else(|| name.replace('-', " "));
        pages.push((name.to_string(), heading));
    }
    pages
}

/// The heading at the top of a page.
fn heading(text: &str) -> Option<String> {
    let (_, rest) = text.split_once("<h1>")?;
    let (heading, _) = rest.split_once("</h1>")?;
    Some(heading.trim().to_string())
}

fn list() -> ExitCode {
    let folder = folder();
    let pages = pages(&folder);
    if pages.is_empty() {
        eprintln!("There is no guide in {}", folder.display());
        return ExitCode::FAILURE;
    }
    for (name, heading) in pages {
        println!("{name:<16} {heading}");
    }
    ExitCode::SUCCESS
}

/// Open one page in the browser.
fn open(page: &str) -> ExitCode {
    let folder = folder();
    let name = page.strip_suffix(".html").unwrap_or(page);
    if name.is_empty() || name.contains('/') || name.contains("..") {
        eprintln!("rift guide: {page} is not a page of the guide\n{USAGE}");
        return ExitCode::from(2);
    }
    let path = folder.join(format!("{name}.html"));
    if !path.is_file() {
        eprintln!("The guide has no page called {name}. Try rift guide list.");
        return ExitCode::FAILURE;
    }
    println!("Opening {}", path.display());
    match Command::new("xdg-open").arg(&path).status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("The browser exited with {status}");
            ExitCode::FAILURE
        }
        Err(why) => {
            eprintln!("Could not open the browser: {why}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_heading_comes_off_a_page() {
        assert_eq!(
            heading("<title>x</title>\n<h1>The desktop</h1>\n<p>Words.</p>"),
            Some("The desktop".to_string())
        );
        assert_eq!(heading("<p>No heading</p>"), None);
    }

    #[test]
    fn the_contents_says_what_the_pages_are() {
        let folder = std::env::temp_dir().join(format!("rift-guide-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("index.html"),
            "<h1>Rift guide</h1><a href=\"the-drive.html\">The drive</a>\
             <a href=\"the-desktop.html\">The desktop</a><a href=\"the-drive.html\">again</a>\
             <a href=\"https://example.invalid/away.html\">away</a>",
        )
        .unwrap();
        fs::write(folder.join("the-drive.html"), "<h1>The drive</h1>").unwrap();
        fs::write(folder.join("the-desktop.html"), "<h1>The desktop</h1>").unwrap();
        let pages = pages(&folder);
        fs::remove_dir_all(&folder).unwrap();
        assert_eq!(
            pages,
            vec![
                ("the-drive".to_string(), "The drive".to_string()),
                ("the-desktop".to_string(), "The desktop".to_string()),
            ]
        );
    }

    #[test]
    fn a_folder_with_no_guide_in_it_has_no_pages() {
        assert!(pages(Path::new("/nowhere-at-all")).is_empty());
    }

    /// The guide the image carries: every page is in the contents, every link in it goes to a page
    /// that is there, and every page has a title and a heading. An 80 minute image build is a slow
    /// way to find a link with a typo in it.
    #[test]
    fn the_guide_hangs_together() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nix/guide");
        let listed = pages(&folder);
        assert!(listed.len() > 5, "the guide has {} pages", listed.len());
        let mut files: Vec<String> = fs::read_dir(&folder)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                Path::new(name).extension().is_some_and(|end| end == "html") && name != "index.html"
            })
            .collect();
        files.sort();
        let mut named: Vec<String> = listed
            .iter()
            .map(|(name, _)| format!("{name}.html"))
            .collect();
        named.sort();
        assert_eq!(files, named, "a page of the guide is not in the contents");
        for (name, heading) in &listed {
            assert!(!heading.is_empty(), "{name} has no heading");
        }
        for name in files.iter().chain(["index.html".to_string()].iter()) {
            let text = fs::read_to_string(folder.join(name)).unwrap();
            assert!(text.contains("<title>"), "{name} has no title");
            assert!(text.contains("<h1>"), "{name} has no heading");
            assert!(
                text.is_ascii(),
                "{name} has something other than ascii in it"
            );
            for link in text.split("href=\"").skip(1) {
                let link = link.split('"').next().unwrap_or_default();
                if link.starts_with("http") {
                    continue;
                }
                let target = link.split('#').next().unwrap_or_default();
                assert!(
                    folder.join(target).is_file(),
                    "{name} links to {link}, which is not there"
                );
            }
        }
    }
}
