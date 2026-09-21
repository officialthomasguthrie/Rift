//! The Printers page: the printers CUPS has a queue for, what each of them is doing, which one the
//! owner's apps print to first, and the jobs waiting.
//!
//! A driverless printer on the network gets a queue by itself, from cups-browsed, so there is
//! nothing to add. Pressing a printer makes it the default in the owner's own lpoptions file, a
//! stopped printer has a button that resumes it, and a job a button that cancels it; each runs
//! CUPS's own command, the one a terminal would. The page asks CUPS as it comes up and every
//! two seconds while it is up, so a job is seen going through.

use std::thread;
use std::time::Duration;

use iced::futures::channel::{mpsc, oneshot};
use iced::widget::{column, container, row, text};
use iced::{Center, Element, Fill, Subscription, Task};
use librift::printers::{self, Job, Printer, Printers, State};

use crate::ai::said;
use crate::theme::Colors;
use crate::ui::{Message, Settings};
use crate::widgets::{GAP, TEXT_SIZE, action, group, heading, line, note, pressable};

/// How often the page asks CUPS again while it is up.
const EVERY: Duration = Duration::from_secs(2);

/// What the owner asked of a printer or a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Make this printer the one apps print to first.
    Default(String),
    /// Start this stopped printer printing again.
    Resume(String),
    /// Cancel the job with this number.
    Cancel(u32),
}

/// Ask CUPS now.
fn reading() -> Message {
    Message::Printers(printers::read())
}

/// The printers while the page is up: asked as the page comes up and every two seconds after, on a
/// thread that ends at the first send after the page has gone.
pub fn following() -> Subscription<Message> {
    Subscription::run_with("printers", |_| {
        let (sender, receiver) = mpsc::unbounded();
        thread::spawn(move || {
            while sender.unbounded_send(reading()).is_ok() {
                thread::sleep(EVERY);
            }
        });
        receiver
    })
}

/// Do what the owner asked on a thread of its own, then ask CUPS again, so the page shows what CUPS
/// did rather than what was asked for. The reading is asked for inside the closure, so it starts
/// once the command has finished.
pub fn asked(state: &mut Settings, asked: Asked) -> Task<Message> {
    state.problem = None;
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(match &asked {
            Asked::Default(printer) => printers::set_default(printer),
            Asked::Resume(printer) => printers::resume(printer),
            Asked::Cancel(job) => printers::cancel(*job),
        });
    });
    Task::perform(receiver, |said| {
        said.unwrap_or_else(|_| Err("It stopped before it finished.".to_string()))
    })
    .then(|said| Task::done(Message::Acted(said)).chain(read()))
}

/// Ask CUPS once, on a thread of its own.
fn read() -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(reading());
    });
    Task::perform(receiver, |answered| {
        answered.unwrap_or_else(|_| Message::Printers(Err("CUPS did not answer.".to_string())))
    })
}

/// The lines `rift-settings --state` prints, once CUPS has answered: how many printers there are,
/// a line for each with what it is doing and what it last said, the default, how many jobs are
/// waiting, and a line for each of those. `printers none` is CUPS not answering at all.
#[must_use]
pub fn state(state: &Settings) -> Vec<String> {
    let Some(answered) = state.printers.as_ref() else {
        return Vec::new();
    };
    let Ok(there) = answered else {
        return vec!["printers none".to_string()];
    };
    let mut lines = vec![format!("printers {}", there.printers.len())];
    for printer in &there.printers {
        let said = format!("printer {} {}", printer.name, printer.state.word());
        lines.push(if printer.message.trim().is_empty() {
            said
        } else {
            format!("{said} {}", printer.message.trim())
        });
    }
    lines.push(format!(
        "default-printer {}",
        there.default.as_deref().unwrap_or("none")
    ));
    lines.push(format!("jobs {}", there.jobs.len()));
    for job in &there.jobs {
        lines.push(format!(
            "job {} {} {} {}",
            job.id,
            job.printer,
            job.state.word(),
            job.name
        ));
    }
    lines
        .iter()
        .map(|line| line.trim_end().to_string())
        .collect()
}

/// The page.
pub fn view(state: &Settings, look: Colors) -> Element<'_, Message> {
    let mut page = column![].spacing(GAP).width(Fill);
    match state.printers.as_ref() {
        None => return note(look, "Asking CUPS about the printers."),
        Some(Err(why)) => {
            page = page.push(note(
                look,
                "The printers are not here, since CUPS is not answering.",
            ));
            page = page.push(note(look, why));
        }
        Some(Ok(there)) => {
            page = page.push(the_printers(look, there));
            page = page.push(the_jobs(look, there));
        }
    }
    page = page.push(note(look, NOT_YET));
    if let Some(why) = &state.problem {
        page = page.push(text(why).size(TEXT_SIZE).color(look.error));
    }
    page.into()
}

/// Every printer, pressed to make it the default, with a button on a stopped one.
fn the_printers(look: Colors, there: &Printers) -> Element<'_, Message> {
    let rows = if there.printers.is_empty() {
        vec![
            container(note(look, "No printer has been found."))
                .padding([8, 12])
                .into(),
        ]
    } else {
        there
            .printers
            .iter()
            .map(|printer| listed(look, printer, there.is_default(printer)))
            .collect()
    };
    column![
        heading(look, "Printers"),
        group(look, rows),
        note(look, PRINTERS)
    ]
    .spacing(8)
    .into()
}

/// One printer: what it is called, what it is doing, the name of its queue where that is not what
/// it is called, and the mark on the default.
fn listed(look: Colors, printer: &Printer, default: bool) -> Element<'_, Message> {
    let mut left = column![line(look, printer.title())].spacing(2);
    left = left.push(text(doing(printer)).size(TEXT_SIZE).color(look.dim));
    let beside: Option<Element<'_, Message>> = if printer.state == State::Stopped {
        Some(action(
            look,
            "Resume",
            Some(Message::Printer(Asked::Resume(printer.name.clone()))),
        ))
    } else if printer.title() == printer.name {
        None
    } else {
        Some(said(look, &printer.name))
    };
    pressable(
        look,
        left.into(),
        beside,
        default,
        Message::Printer(Asked::Default(printer.name.clone())),
    )
}

/// What a printer is doing, in words, with what it last said after it.
fn doing(printer: &Printer) -> String {
    let message = printer.message.trim();
    if message.is_empty() {
        printer.state.label().to_string()
    } else {
        format!("{}: {message}", printer.state.label())
    }
}

/// The jobs that have not finished, each with a button that cancels it.
fn the_jobs(look: Colors, there: &Printers) -> Element<'_, Message> {
    let mut part = column![heading(look, "Jobs")].spacing(8);
    if there.jobs.is_empty() {
        return part.push(note(look, "Nothing is waiting to print.")).into();
    }
    let rows = there
        .jobs
        .iter()
        .map(|job| waiting(look, there, job))
        .collect();
    part = part.push(group(look, rows));
    part.into()
}

/// One job: the document, what it is doing and on which printer, and the button.
fn waiting<'a>(look: Colors, there: &'a Printers, job: &'a Job) -> Element<'a, Message> {
    let name = if job.name.trim().is_empty() {
        "Untitled document"
    } else {
        job.name.trim()
    };
    let printer = there
        .named(&job.printer)
        .map_or(job.printer.as_str(), Printer::title);
    let under = format!("{} on {printer}", job.state.label());
    container(
        row![
            column![
                line(look, name),
                text(under).size(TEXT_SIZE).color(look.dim)
            ]
            .spacing(2)
            .width(Fill),
            action(
                look,
                "Cancel",
                Some(Message::Printer(Asked::Cancel(job.id))),
            ),
        ]
        .align_y(Center)
        .spacing(GAP),
    )
    .width(Fill)
    .padding([8, 12])
    .into()
}

/// How a printer gets here, and what pressing one does.
const PRINTERS: &str = "A printer on the network that prints without a driver is added by itself. \
                        Pressing one makes it the printer your apps use first.";
/// What the page cannot do yet, and where scanners are.
const NOT_YET: &str = "Adding a printer by its address is not in Settings yet. Document Scanner \
                       finds a scanner on the network or plugged in.";

#[cfg(test)]
mod tests {
    use super::*;
    use librift::printers::JobState;

    fn printer(name: &str, info: &str, state: State, message: &str) -> Printer {
        Printer {
            name: name.to_string(),
            info: info.to_string(),
            location: String::new(),
            model: String::new(),
            state,
            message: message.to_string(),
        }
    }

    fn settings(answered: Result<Printers, String>) -> Settings {
        let mut state = Settings::bare();
        state.printers = Some(answered);
        state
    }

    #[test]
    fn the_state_says_every_printer_the_default_and_every_job() {
        let kept = settings(Ok(Printers {
            printers: vec![
                printer("Office", "Office LaserJet", State::Idle, ""),
                printer("Rift_test", "", State::Stopped, "Paused by the boot test"),
            ],
            jobs: vec![Job {
                id: 4,
                printer: "Rift_test".to_string(),
                name: "Letter to the council".to_string(),
                owner: "rift".to_string(),
                state: JobState::Held,
            }],
            default: Some("Office".to_string()),
        }));
        assert_eq!(
            state(&kept),
            [
                "printers 2",
                "printer Office idle",
                "printer Rift_test stopped Paused by the boot test",
                "default-printer Office",
                "jobs 1",
                "job 4 Rift_test held Letter to the council",
            ]
        );
    }

    #[test]
    fn a_machine_with_no_printers_says_so() {
        let kept = settings(Ok(Printers::default()));
        assert_eq!(
            state(&kept),
            ["printers 0", "default-printer none", "jobs 0"]
        );
    }

    #[test]
    fn nothing_is_said_until_cups_answers_and_none_when_it_cannot() {
        assert!(state(&Settings::bare()).is_empty());
        let failed = settings(Err("CUPS is not answering.".to_string()));
        assert_eq!(state(&failed), ["printers none"]);
    }

    #[test]
    fn what_a_printer_is_doing_is_its_state_and_what_it_said() {
        assert_eq!(doing(&printer("Office", "", State::Idle, "")), "Idle");
        assert_eq!(
            doing(&printer("Office", "", State::Stopped, " Paused ")),
            "Stopped: Paused"
        );
    }

    #[test]
    fn the_sentences_are_sentences() {
        for sentence in [PRINTERS, NOT_YET] {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(sentence.is_ascii(), "{sentence}");
        }
        assert!(NOT_YET.contains("not in Settings yet"));
    }
}
