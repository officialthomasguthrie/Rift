//! Printing from a client's side: the printers CUPS has a queue for, what each of them is doing, the
//! jobs waiting, and the printer the owner's apps print to first. The Printers page asks all of it
//! here.
//!
//! CUPS is asked over IPP, the protocol it serves on its local socket, which every one of its
//! clients speaks: lpstat, the print dialog of an app and cups-browsed ask it the same questions.
//! The answers are numbers and keywords rather than sentences in the words of the locale, and they
//! name the document a job prints, which lpstat never shows.
//!
//! Which printer is the default is not the scheduler's to say alone. The CUPS library works it out
//! on the client, from the environment and the lpoptions files before it asks the scheduler, and
//! the same order is followed here, so the page marks the printer an app prints to.
//!
//! What changes something runs CUPS's own commands as the owner: `lpoptions -d`, `cupsenable` and
//! `cancel`. CUPS then decides who may do what exactly as it does for the same command in a
//! terminal, and says why not in its own words.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::os;

/// Where cupsd listens on the machine, unless `CUPS_SERVER` names another socket.
pub const SOCKET: &str = "/run/cups/cups.sock";
/// Where the system's lpoptions file is, unless `CUPS_SERVERROOT` names another folder.
const SERVER_ROOT: &str = "/etc/cups";
/// How long to wait for CUPS to take a request or to answer it.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The operations asked for.
const GET_JOBS: u16 = 0x000a;
const CUPS_GET_DEFAULT: u16 = 0x4001;
const CUPS_GET_PRINTERS: u16 = 0x4002;
/// The status of an answer that found nothing: no printers, or no default.
const NOT_FOUND: u16 = 0x0406;

/// The tags that start a group of attributes, and the one that ends the message.
const OPERATION: u8 = 0x01;
const JOB: u8 = 0x02;
const END: u8 = 0x03;
const PRINTER: u8 = 0x04;
/// The tags of the values asked for and answered.
const INTEGER: u8 = 0x21;
const ENUM: u8 = 0x23;
const TEXT_WITH_LANGUAGE: u8 = 0x35;
const NAME_WITH_LANGUAGE: u8 = 0x36;
const NAME: u8 = 0x42;
const KEYWORD: u8 = 0x44;
const URI: u8 = 0x45;
const CHARSET: u8 = 0x47;
const LANGUAGE: u8 = 0x48;

/// What the page asks about each printer.
const PRINTER_ATTRIBUTES: [&str; 6] = [
    "printer-name",
    "printer-info",
    "printer-location",
    "printer-make-and-model",
    "printer-state",
    "printer-state-message",
];
/// And about each job.
const JOB_ATTRIBUTES: [&str; 5] = [
    "job-id",
    "job-name",
    "job-state",
    "job-printer-uri",
    "job-originating-user-name",
];

/// What a printer is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Waiting for a job.
    Idle,
    /// Printing one.
    Printing,
    /// Stopped: CUPS sends it nothing until it is resumed.
    Stopped,
}

impl State {
    /// Out of IPP's printer-state: 3 is idle, 4 processing and 5 stopped.
    #[must_use]
    pub const fn from_number(number: i32) -> Self {
        match number {
            4 => Self::Printing,
            5 => Self::Stopped,
            _ => Self::Idle,
        }
    }

    /// The word `rift-settings --state` prints for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Printing => "printing",
            Self::Stopped => "stopped",
        }
    }

    /// What the page says it is doing.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Printing => "Printing",
            Self::Stopped => "Stopped",
        }
    }
}

/// One printer CUPS has a queue for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printer {
    /// The name of its queue, which `lp -d` and lpstat know it by: `Office_LaserJet`.
    pub name: String,
    /// What it is called, printer-info. cups-browsed takes it from what the printer announces on
    /// the network. Empty when it has none.
    pub info: String,
    /// Where it is, when someone has said.
    pub location: String,
    /// Its make and model, from what it says about itself.
    pub model: String,
    /// What it is doing.
    pub state: State,
    /// What it last said about what it is doing, printer-state-message. Empty when it said nothing.
    pub message: String,
}

impl Printer {
    /// What a person calls it: its description, or the name of its queue where it has none.
    #[must_use]
    pub fn title(&self) -> &str {
        if self.info.trim().is_empty() {
            &self.name
        } else {
            self.info.trim()
        }
    }
}

/// What a job is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// In the queue, behind another or waiting for its printer.
    Waiting,
    /// Held: it prints once it is released.
    Held,
    /// Printing now.
    Printing,
    /// Stopped with its printer.
    Stopped,
    /// Finished one way or another: printed, cancelled or given up on.
    Done,
}

impl JobState {
    /// Out of IPP's job-state: 3 pending, 4 held, 5 processing, 6 stopped, and 7 to 9 the ways a
    /// job ends.
    #[must_use]
    pub const fn from_number(number: i32) -> Self {
        match number {
            3 => Self::Waiting,
            4 => Self::Held,
            5 => Self::Printing,
            6 => Self::Stopped,
            _ => Self::Done,
        }
    }

    /// The word `rift-settings --state` prints for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Held => "held",
            Self::Printing => "printing",
            Self::Stopped => "stopped",
            Self::Done => "done",
        }
    }

    /// What the page says it is doing.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Waiting => "Waiting",
            Self::Held => "Held",
            Self::Printing => "Printing",
            Self::Stopped => "Stopped",
            Self::Done => "Done",
        }
    }
}

/// One job in a queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// Its number, which `cancel` takes.
    pub id: u32,
    /// The queue it is in.
    pub printer: String,
    /// The name of the document. Empty when CUPS keeps it from whoever asked, which it does for a
    /// job that is not theirs.
    pub name: String,
    /// Who sent it, with the same reserve.
    pub owner: String,
    /// What it is doing.
    pub state: JobState,
}

/// The printers there are, the jobs waiting, and the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Printers {
    /// Every queue, in the order CUPS lists them, which is by name.
    pub printers: Vec<Printer>,
    /// The jobs that have not finished, oldest first.
    pub jobs: Vec<Job>,
    /// The printer the owner's apps print to when they are not told another, by the name of its
    /// queue.
    pub default: Option<String>,
}

impl Printers {
    /// The printer with this queue's name. CUPS takes a name in any case.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&Printer> {
        self.printers
            .iter()
            .find(|printer| printer.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Whether this is the printer the owner's apps print to first.
    #[must_use]
    pub fn is_default(&self, printer: &Printer) -> bool {
        self.default
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(&printer.name))
    }
}

/// Ask CUPS for the printers, the jobs that have not finished, and the default.
///
/// # Errors
///
/// A sentence when CUPS is not answering on its socket.
pub fn read() -> Result<Printers, String> {
    let socket = socket();
    let user = user();
    let printers = ask(&socket, &printers_request(user.as_deref()))?;
    let jobs = ask(&socket, &jobs_request(user.as_deref()))?;
    let default = user_default(
        std::env::var_os("HOME").map(PathBuf::from).as_deref(),
        &server_root(),
    )
    .or_else(|| {
        ask(&socket, &default_request(user.as_deref()))
            .ok()
            .and_then(|answer| default_from(&answer))
    });
    Ok(Printers {
        printers: printers_from(&printers),
        jobs: jobs_from(&jobs),
        default,
    })
}

/// Make this printer the one the owner's apps print to first. `lpoptions -d` writes it into the
/// owner's own lpoptions file, which needs no administrator, and it is the file the CUPS library
/// reads before it asks the scheduler.
///
/// # Errors
///
/// What lpoptions said when it refused, for a printer that is not there.
pub fn set_default(printer: &str) -> Result<(), String> {
    os::change("lpoptions", &["-d", printer.trim()])
}

/// Start a stopped printer printing again, which is `cupsenable`. CUPS allows it to the groups
/// named in its `SystemGroup` line, and the owner is in wheel, which is one of them on this system.
///
/// # Errors
///
/// What cupsenable said when CUPS refused.
pub fn resume(printer: &str) -> Result<(), String> {
    os::change("cupsenable", &[printer.trim()])
}

/// Take a job out of its queue, which is `cancel`. CUPS allows whoever sent a job to cancel it.
///
/// # Errors
///
/// What cancel said when CUPS refused, for a job that has already finished.
pub fn cancel(job: u32) -> Result<(), String> {
    os::change("cancel", &[&job.to_string()])
}

/// The socket cupsd listens on: the one `CUPS_SERVER` names, when it names a socket rather than a
/// host, or the machine's own.
fn socket() -> PathBuf {
    std::env::var("CUPS_SERVER")
        .ok()
        .filter(|server| server.starts_with('/'))
        .map_or_else(|| PathBuf::from(SOCKET), PathBuf::from)
}

/// The folder the system's lpoptions file is in.
fn server_root() -> PathBuf {
    std::env::var_os("CUPS_SERVERROOT")
        .filter(|root| !root.is_empty())
        .map_or_else(|| PathBuf::from(SERVER_ROOT), PathBuf::from)
}

/// The account asking, which CUPS shows its own jobs' names to.
fn user() -> Option<String> {
    ["USER", "LOGNAME"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|user| !user.is_empty()))
}

/// The default the CUPS library would find before it asks the scheduler: `LPDEST`, then `PRINTER`
/// unless it is `lp`, then the owner's `~/.cups/lpoptions`, then the system's lpoptions.
fn user_default(home: Option<&Path>, server_root: &Path) -> Option<String> {
    let named = |variable: &str| {
        std::env::var(variable)
            .ok()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
    };
    if let Some(name) = named("LPDEST") {
        return Some(name);
    }
    if let Some(name) = named("PRINTER").filter(|name| name != "lp") {
        return Some(name);
    }
    let from_file = |path: PathBuf| {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| named_default(&text))
    };
    home.and_then(|home| from_file(home.join(".cups").join("lpoptions")))
        .or_else(|| from_file(server_root.join("lpoptions")))
}

/// The printer an lpoptions file names as the default, without the instance after a slash. The
/// file is lines of a keyword and a value; `Default Office` names one.
#[must_use]
pub fn named_default(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        // a queue's name never holds a hash, so everything after one is a comment
        let line = line.split('#').next().unwrap_or_default().trim();
        let (keyword, value) = line.split_once(char::is_whitespace)?;
        if !keyword.eq_ignore_ascii_case("default") {
            return None;
        }
        let printer = value.split_whitespace().next()?;
        let printer = printer.split('/').next().unwrap_or(printer);
        (!printer.is_empty()).then(|| printer.to_string())
    })
}

/// The request for every printer and what the page shows of each.
fn printers_request(user: Option<&str>) -> Vec<u8> {
    let mut attributes = vec![(KEYWORD, "requested-attributes", PRINTER_ATTRIBUTES.to_vec())];
    if let Some(user) = user {
        attributes.push((NAME, "requesting-user-name", vec![user]));
    }
    request(CUPS_GET_PRINTERS, 1, &attributes)
}

/// The request for every job of every printer that has not finished. A printer's address with no
/// printer in it is every printer, which is how lpstat asks.
fn jobs_request(user: Option<&str>) -> Vec<u8> {
    let mut attributes = vec![(URI, "printer-uri", vec!["ipp://localhost/"])];
    if let Some(user) = user {
        attributes.push((NAME, "requesting-user-name", vec![user]));
    }
    attributes.push((KEYWORD, "which-jobs", vec!["not-completed"]));
    attributes.push((KEYWORD, "requested-attributes", JOB_ATTRIBUTES.to_vec()));
    request(GET_JOBS, 2, &attributes)
}

/// The request for the scheduler's own default.
fn default_request(user: Option<&str>) -> Vec<u8> {
    let mut attributes = vec![(KEYWORD, "requested-attributes", vec!["printer-name"])];
    if let Some(user) = user {
        attributes.push((NAME, "requesting-user-name", vec![user]));
    }
    request(CUPS_GET_DEFAULT, 3, &attributes)
}

/// An IPP request: version 2.0, the operation, its number, then the operation's attributes, which
/// always start with the character set and the language.
fn request(operation: u16, id: u32, attributes: &[(u8, &str, Vec<&str>)]) -> Vec<u8> {
    let mut bytes = vec![2, 0];
    bytes.extend(operation.to_be_bytes());
    bytes.extend(id.to_be_bytes());
    bytes.push(OPERATION);
    let first = [
        (CHARSET, "attributes-charset", vec!["utf-8"]),
        (LANGUAGE, "attributes-natural-language", vec!["en"]),
    ];
    for (tag, name, values) in first.iter().chain(attributes) {
        for (at, value) in values.iter().enumerate() {
            bytes.push(*tag);
            // the second and later values of an attribute carry no name
            push(&mut bytes, if at == 0 { name.as_bytes() } else { &[] });
            push(&mut bytes, value.as_bytes());
        }
    }
    bytes.push(END);
    bytes
}

/// A length of two bytes and the bytes it counts. Nothing asked for comes near the limit, and what
/// would is cut at it rather than written with a length that lies.
fn push(bytes: &mut Vec<u8>, value: &[u8]) {
    let length = u16::try_from(value.len()).unwrap_or(u16::MAX);
    bytes.extend(length.to_be_bytes());
    bytes.extend(&value[..usize::from(length)]);
}

/// One value of an attribute in an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    /// An integer or an enum.
    Number(i32),
    /// Text, a name, a keyword, an address, or anything else that is a string.
    Words(String),
    /// Anything the page never asks for.
    Other,
}

/// One group of attributes in an answer: the operation's own, a printer, or a job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Group {
    /// The tag that started it.
    tag: u8,
    /// Its attributes in order, each with its values.
    attributes: Vec<(String, Vec<Value>)>,
}

impl Group {
    /// The first value of the attribute with this name.
    fn first(&self, name: &str) -> Option<&Value> {
        self.attributes
            .iter()
            .find(|(named, _)| named == name)
            .and_then(|(_, values)| values.first())
    }

    /// The same, as words, or nothing.
    fn words(&self, name: &str) -> String {
        match self.first(name) {
            Some(Value::Words(words)) => words.clone(),
            _ => String::new(),
        }
    }

    /// The same, as a number.
    fn number(&self, name: &str) -> Option<i32> {
        match self.first(name) {
            Some(Value::Number(number)) => Some(*number),
            _ => None,
        }
    }
}

/// What CUPS answered: the status and the groups, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Answer {
    /// The status of the operation: under 0x0100 it went through.
    status: u16,
    /// Every group of attributes.
    groups: Vec<Group>,
}

impl Answer {
    /// The groups with this tag: every printer, or every job.
    fn groups(&self, tag: u8) -> impl Iterator<Item = &Group> {
        self.groups.iter().filter(move |group| group.tag == tag)
    }
}

/// Read an IPP answer: the version, the status, the request's number, then tagged groups of
/// attributes until the end tag.
fn parse(bytes: &[u8]) -> Result<Answer, String> {
    let short = || "CUPS answered with less than a whole message.".to_string();
    let head = bytes.get(..8).ok_or_else(short)?;
    let mut answer = Answer {
        status: u16::from_be_bytes([head[2], head[3]]),
        groups: Vec::new(),
    };
    let mut at = 8;
    loop {
        let tag = *bytes.get(at).ok_or_else(short)?;
        at += 1;
        if tag == END {
            return Ok(answer);
        }
        if tag < 0x10 {
            answer.groups.push(Group {
                tag,
                attributes: Vec::new(),
            });
            continue;
        }
        let name = take(bytes, &mut at).ok_or_else(short)?;
        let value = decode(tag, take(bytes, &mut at).ok_or_else(short)?);
        let Some(group) = answer.groups.last_mut() else {
            return Err("CUPS answered with a value outside any group.".to_string());
        };
        if name.is_empty() {
            // one more value of the attribute before it
            if let Some((_, values)) = group.attributes.last_mut() {
                values.push(value);
            }
        } else {
            group
                .attributes
                .push((String::from_utf8_lossy(name).into_owned(), vec![value]));
        }
    }
}

/// A length of two bytes and what it counts, from `at`, which moves past both.
fn take<'a>(bytes: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
    let length = bytes.get(*at..*at + 2)?;
    let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
    let value = bytes.get(*at + 2..*at + 2 + length)?;
    *at += 2 + length;
    Some(value)
}

/// One value, by its tag.
fn decode(tag: u8, value: &[u8]) -> Value {
    match tag {
        INTEGER | ENUM => <[u8; 4]>::try_from(value)
            .map_or(Value::Other, |four| Value::Number(i32::from_be_bytes(four))),
        // the language comes first with a length of its own, then the text with its length
        TEXT_WITH_LANGUAGE | NAME_WITH_LANGUAGE => {
            let mut at = 0;
            let _language = take(value, &mut at);
            take(value, &mut at).map_or(Value::Other, |text| {
                Value::Words(String::from_utf8_lossy(text).into_owned())
            })
        }
        0x40..=0x5f => Value::Words(String::from_utf8_lossy(value).into_owned()),
        _ => Value::Other,
    }
}

/// The printers in an answer to CUPS-Get-Printers.
fn printers_from(answer: &Answer) -> Vec<Printer> {
    answer
        .groups(PRINTER)
        .map(|group| Printer {
            name: group.words("printer-name"),
            info: group.words("printer-info"),
            location: group.words("printer-location"),
            model: group.words("printer-make-and-model"),
            state: State::from_number(group.number("printer-state").unwrap_or(3)),
            message: group.words("printer-state-message"),
        })
        .filter(|printer| !printer.name.is_empty())
        .collect()
}

/// The jobs in an answer to Get-Jobs, oldest first.
fn jobs_from(answer: &Answer) -> Vec<Job> {
    let mut jobs: Vec<Job> = answer
        .groups(JOB)
        .filter_map(|group| {
            let id = u32::try_from(group.number("job-id")?).ok()?;
            let uri = group.words("job-printer-uri");
            Some(Job {
                id,
                printer: uri.rsplit('/').next().unwrap_or_default().to_string(),
                name: group.words("job-name"),
                owner: group.words("job-originating-user-name"),
                state: JobState::from_number(group.number("job-state").unwrap_or(3)),
            })
        })
        .filter(|job| job.state != JobState::Done)
        .collect();
    jobs.sort_by_key(|job| job.id);
    jobs
}

/// The scheduler's default in an answer to CUPS-Get-Default.
fn default_from(answer: &Answer) -> Option<String> {
    answer
        .groups(PRINTER)
        .map(|group| group.words("printer-name"))
        .find(|name| !name.is_empty())
}

/// Send one request to cupsd and read what it answered. An answer that found nothing is an answer
/// with nothing in it.
fn ask(socket: &Path, request: &[u8]) -> Result<Answer, String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|e| format!("CUPS is not answering on {}: {e}", socket.to_string_lossy()))?;
    let _ = stream.set_read_timeout(Some(TIMEOUT));
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    let answer = parse(&exchange(&mut stream, request)?)?;
    match answer.status {
        status if status < 0x0100 || status == NOT_FOUND => Ok(answer),
        status => Err(format!("CUPS answered with IPP status {status:#06x}.")),
    }
}

/// Post an IPP request over HTTP and read the body of the answer.
fn exchange(stream: &mut (impl Read + Write), request: &[u8]) -> Result<Vec<u8>, String> {
    let head = format!(
        "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/ipp\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        request.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(request))
        .and_then(|()| stream.flush())
        .map_err(|e| format!("Could not ask CUPS: {e}"))?;
    body(stream)
}

/// The body of an HTTP answer: after the head, as long as its length says, or in chunks when it
/// says it is chunked, or up to the end of the connection when it says neither.
fn body(stream: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut read = Vec::new();
    let end = loop {
        if let Some(end) = read.windows(4).position(|four| four == b"\r\n\r\n") {
            break end;
        }
        if !more(stream, &mut read)? {
            return Err("CUPS closed the connection before it answered.".to_string());
        }
    };
    let head = String::from_utf8_lossy(&read[..end]).into_owned();
    let mut rest = read.split_off(end + 4);
    let status = head.split("\r\n").next().unwrap_or_default().to_string();
    let header = |wanted: &str| {
        head.split("\r\n").skip(1).find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case(wanted)
                .then(|| value.trim().to_string())
        })
    };
    match status.split_whitespace().nth(1) {
        Some("200") => {}
        Some("401" | "403") => {
            return Err("CUPS will not answer this account without a password.".to_string());
        }
        _ => return Err(format!("CUPS answered {}.", status.trim())),
    }
    if header("Transfer-Encoding").is_some_and(|coding| coding.eq_ignore_ascii_case("chunked")) {
        while more(stream, &mut rest)? {}
        return dechunk(&rest);
    }
    match header("Content-Length").and_then(|length| length.parse::<usize>().ok()) {
        Some(length) => {
            while rest.len() < length {
                if !more(stream, &mut rest)? {
                    return Err(
                        "CUPS closed the connection in the middle of its answer.".to_string()
                    );
                }
            }
            rest.truncate(length);
        }
        None => while more(stream, &mut rest)? {},
    }
    Ok(rest)
}

/// Read what the stream has next onto the end of `read`. False at the end of the connection.
fn more(stream: &mut impl Read, read: &mut Vec<u8>) -> Result<bool, String> {
    let mut chunk = [0; 4096];
    let count = stream
        .read(&mut chunk)
        .map_err(|e| format!("CUPS did not answer: {e}"))?;
    read.extend_from_slice(&chunk[..count]);
    Ok(count > 0)
}

/// A chunked body put back together: each chunk is its length in hex on a line, then the chunk,
/// and a chunk of length 0 ends it.
fn dechunk(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let broken = || "CUPS answered in chunks that do not add up.".to_string();
    let mut body = Vec::new();
    let mut at = 0;
    loop {
        let line = bytes
            .get(at..)
            .and_then(|rest| rest.windows(2).position(|two| two == b"\r\n"))
            .ok_or_else(broken)?;
        let size = String::from_utf8_lossy(&bytes[at..at + line]).into_owned();
        let size = size.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size, 16).map_err(|_| broken())?;
        at += line + 2;
        if size == 0 {
            return Ok(body);
        }
        body.extend_from_slice(bytes.get(at..at + size).ok_or_else(broken)?);
        at += size + 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One value of an answer: its tag, its name and itself.
    type Said<'a> = (u8, &'a str, &'a [u8]);

    /// An answer as cupsd writes one: the version, the status, the request's number, then the
    /// groups of attributes.
    fn answer(status: u16, groups: &[(u8, Vec<Said<'_>>)]) -> Vec<u8> {
        let mut bytes = vec![2, 0];
        bytes.extend(status.to_be_bytes());
        bytes.extend(7u32.to_be_bytes());
        for (tag, values) in groups {
            bytes.push(*tag);
            for (value_tag, name, value) in values {
                bytes.push(*value_tag);
                push(&mut bytes, name.as_bytes());
                push(&mut bytes, value);
            }
        }
        bytes.push(END);
        bytes
    }

    fn operation() -> (u8, Vec<Said<'static>>) {
        (
            OPERATION,
            vec![
                (CHARSET, "attributes-charset", b"utf-8".as_slice()),
                (LANGUAGE, "attributes-natural-language", b"en".as_slice()),
            ],
        )
    }

    fn printer<'a>(
        name: &'a str,
        info: &'a str,
        state: &'a [u8],
        message: &'a str,
    ) -> Vec<Said<'a>> {
        vec![
            (NAME, "printer-name", name.as_bytes()),
            (0x41, "printer-info", info.as_bytes()),
            (0x41, "printer-location", b"".as_slice()),
            (0x41, "printer-make-and-model", b"Test Printer".as_slice()),
            (ENUM, "printer-state", state),
            (0x41, "printer-state-message", message.as_bytes()),
        ]
    }

    #[test]
    fn a_request_starts_with_the_character_set_and_the_language() {
        let asked = printers_request(Some("rift"));
        assert_eq!(&asked[..8], [2, 0, 0x40, 0x02, 0, 0, 0, 1]);
        assert_eq!(asked[8], OPERATION);
        assert_eq!(asked[9], CHARSET);
        assert_eq!(&asked[10..12], [0, 18]);
        assert_eq!(&asked[12..30], b"attributes-charset");
        assert_eq!(&asked[30..32], [0, 5]);
        assert_eq!(&asked[32..37], b"utf-8");
        assert_eq!(asked[37], LANGUAGE);
        assert_eq!(asked.last(), Some(&END));
        // the second value of requested-attributes has no name of its own
        let second = b"printer-info";
        let at = asked
            .windows(second.len())
            .position(|window| window == second)
            .unwrap();
        assert_eq!(&asked[at - 5..at], [KEYWORD, 0, 0, 0, 12]);
        // and the request reads back as the attributes it was built from
        let parsed = parse(&asked).unwrap();
        assert_eq!(parsed.groups.len(), 1);
        let asked_for = &parsed.groups[0].attributes[2];
        assert_eq!(asked_for.0, "requested-attributes");
        assert_eq!(asked_for.1.len(), PRINTER_ATTRIBUTES.len());
        assert_eq!(
            parsed.groups[0].first("requesting-user-name"),
            Some(&Value::Words("rift".to_string()))
        );
    }

    #[test]
    fn the_printers_come_out_of_their_groups() {
        let bytes = answer(
            0,
            &[
                operation(),
                (
                    PRINTER,
                    printer("Office", "Office LaserJet", &[0, 0, 0, 3], ""),
                ),
                (
                    PRINTER,
                    printer("Rift_test", "", &[0, 0, 0, 5], "Paused by the boot test"),
                ),
            ],
        );
        let printers = printers_from(&parse(&bytes).unwrap());
        assert_eq!(printers.len(), 2);
        assert_eq!(printers[0].name, "Office");
        assert_eq!(printers[0].title(), "Office LaserJet");
        assert_eq!(printers[0].state, State::Idle);
        assert_eq!(printers[0].model, "Test Printer");
        assert_eq!(printers[1].title(), "Rift_test");
        assert_eq!(printers[1].state, State::Stopped);
        assert_eq!(printers[1].message, "Paused by the boot test");
    }

    #[test]
    fn a_scheduler_with_no_printers_answers_not_found_with_nothing_in_it() {
        let bytes = answer(NOT_FOUND, &[operation()]);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.status, NOT_FOUND);
        assert!(printers_from(&parsed).is_empty());
        assert_eq!(default_from(&parsed), None);
    }

    #[test]
    fn the_jobs_come_out_of_their_groups_oldest_first() {
        let job = |id: &'static [u8], state: &'static [u8], name: &'static str| {
            (
                JOB,
                vec![
                    (INTEGER, "job-id", id),
                    (NAME, "job-name", name.as_bytes()),
                    (ENUM, "job-state", state),
                    (
                        URI,
                        "job-printer-uri",
                        b"ipp://localhost:631/printers/Rift_test".as_slice(),
                    ),
                    (NAME, "job-originating-user-name", b"rift".as_slice()),
                ],
            )
        };
        let bytes = answer(
            0,
            &[
                operation(),
                job(&[0, 0, 0, 5], &[0, 0, 0, 4], "os-release"),
                job(&[0, 0, 0, 2], &[0, 0, 0, 5], "report.pdf"),
                job(&[0, 0, 0, 3], &[0, 0, 0, 9], "printed.pdf"),
            ],
        );
        let jobs = jobs_from(&parse(&bytes).unwrap());
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, 2);
        assert_eq!(jobs[0].state, JobState::Printing);
        assert_eq!(jobs[1].id, 5);
        assert_eq!(jobs[1].printer, "Rift_test");
        assert_eq!(jobs[1].name, "os-release");
        assert_eq!(jobs[1].owner, "rift");
        assert_eq!(jobs[1].state, JobState::Held);
    }

    #[test]
    fn a_value_with_its_language_is_its_text() {
        let mut value = Vec::new();
        push(&mut value, b"en");
        push(&mut value, b"Kitchen");
        assert_eq!(
            decode(TEXT_WITH_LANGUAGE, &value),
            Value::Words("Kitchen".to_string())
        );
        assert_eq!(decode(0x22, &[1]), Value::Other);
        assert_eq!(decode(ENUM, &[0, 0, 0, 4]), Value::Number(4));
        assert_eq!(decode(ENUM, &[4]), Value::Other);
        assert_eq!(decode(0x31, &[0; 11]), Value::Other);
    }

    #[test]
    fn a_message_cut_short_is_an_error() {
        let bytes = answer(0, &[operation()]);
        assert!(parse(&bytes[..bytes.len() - 1]).is_err());
        assert!(parse(&bytes[..5]).is_err());
        assert!(parse(&[2, 0, 0, 0, 0, 0, 0, 1, CHARSET, 0, 1]).is_err());
    }

    #[test]
    fn the_default_is_the_first_default_line_without_its_instance() {
        assert_eq!(
            named_default("Default Office\n"),
            Some("Office".to_string())
        );
        assert_eq!(
            named_default(
                "Dest Kitchen sides=two-sided-long-edge\ndefault Office/draft copies=2\n"
            ),
            Some("Office".to_string())
        );
        assert_eq!(
            named_default("# Default Old\n  Default\tLab # the one downstairs\n"),
            Some("Lab".to_string())
        );
        assert_eq!(named_default("Dest Office\n"), None);
        assert_eq!(named_default(""), None);
    }

    #[test]
    fn the_owners_lpoptions_come_before_the_systems() {
        let folder = std::env::temp_dir().join(format!("rift-printers-{}", std::process::id()));
        let home = folder.join("home");
        let root = folder.join("etc");
        std::fs::create_dir_all(home.join(".cups")).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("lpoptions"), "Default Hall\n").unwrap();
        // the environment can name one first, and the tests run where nothing does
        if std::env::var_os("LPDEST").is_none() && std::env::var_os("PRINTER").is_none() {
            assert_eq!(user_default(Some(&home), &root), Some("Hall".to_string()));
            std::fs::write(home.join(".cups/lpoptions"), "Default Office\n").unwrap();
            assert_eq!(user_default(Some(&home), &root), Some("Office".to_string()));
            assert_eq!(user_default(None, &folder), None);
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_default_printer_is_marked_in_any_case() {
        let printers = Printers {
            printers: printers_from(
                &parse(&answer(
                    0,
                    &[(PRINTER, printer("Office", "", &[0, 0, 0, 3], ""))],
                ))
                .unwrap(),
            ),
            jobs: Vec::new(),
            default: Some("office".to_string()),
        };
        assert!(printers.is_default(&printers.printers[0]));
        assert!(printers.named(" OFFICE ").is_some());
        assert!(printers.named("Kitchen").is_none());
    }

    /// A connection that answers with the bytes it was given and keeps what was written to it.
    struct Canned {
        answer: std::io::Cursor<Vec<u8>>,
        asked: Vec<u8>,
    }

    impl Read for Canned {
        fn read(&mut self, into: &mut [u8]) -> std::io::Result<usize> {
            // a little at a time, the way a socket hands it over
            let most = into.len().min(7);
            self.answer.read(&mut into[..most])
        }
    }

    impl Write for Canned {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.asked.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn canned(answer: &[u8]) -> Canned {
        Canned {
            answer: std::io::Cursor::new(answer.to_vec()),
            asked: Vec::new(),
        }
    }

    #[test]
    fn a_request_goes_out_as_an_http_post_and_the_body_comes_back() {
        let ipp = answer(0, &[operation()]);
        let mut http = format!(
            "HTTP/1.1 200 OK\r\nContent-Language: en\r\nContent-Type: application/ipp\r\n\
             Content-Length: {}\r\n\r\n",
            ipp.len()
        )
        .into_bytes();
        http.extend(&ipp);
        // anything after the length is the next answer's, not this one's
        http.extend(b"HTTP/1.1 200 OK\r\n");
        let mut connection = canned(&http);
        let asked = printers_request(None);
        assert_eq!(exchange(&mut connection, &asked).unwrap(), ipp);
        let written = String::from_utf8_lossy(&connection.asked).into_owned();
        assert!(written.starts_with("POST / HTTP/1.1\r\nHost: localhost\r\n"));
        assert!(written.contains(&format!("Content-Length: {}\r\n", asked.len())));
        assert!(connection.asked.ends_with(&asked));
    }

    #[test]
    fn a_chunked_answer_is_put_back_together() {
        let mut http = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        http.extend(b"5\r\nhello\r\n7;name=x\r\n, world\r\n0\r\n\r\n");
        assert_eq!(body(&mut canned(&http)).unwrap(), b"hello, world");
        assert!(dechunk(b"5\r\nhel").is_err());
    }

    #[test]
    fn a_refusal_or_a_short_answer_is_a_sentence() {
        let refused = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
        assert!(body(&mut canned(refused)).unwrap_err().contains("password"));
        let broken = b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
        assert!(body(&mut canned(broken)).unwrap_err().contains("500"));
        let cut = b"HTTP/1.1 200 OK\r\nContent-Length: 40\r\n\r\nshort";
        assert!(body(&mut canned(cut)).is_err());
        assert!(body(&mut canned(b"HTTP/1.1 200")).is_err());
    }

    #[test]
    fn the_states_read_from_their_numbers() {
        assert_eq!(State::from_number(3), State::Idle);
        assert_eq!(State::from_number(4), State::Printing);
        assert_eq!(State::from_number(5), State::Stopped);
        assert_eq!(JobState::from_number(3), JobState::Waiting);
        assert_eq!(JobState::from_number(4), JobState::Held);
        assert_eq!(JobState::from_number(6), JobState::Stopped);
        for done in 7..=9 {
            assert_eq!(JobState::from_number(done), JobState::Done);
        }
        for state in [State::Idle, State::Printing, State::Stopped] {
            assert!(state.label().starts_with(char::is_uppercase));
            assert_eq!(state.label().to_lowercase(), state.word());
        }
    }
}
