//! Notifications. Lens is the session's notification server, the way the shell of every desktop
//! is: it owns `org.freedesktop.Notifications` on the session bus and hands each notification to
//! the shell, which shows it under the bar at the right and keeps it for the clock menu. What
//! becomes of one on screen, that its time ran out, that the owner closed it or pressed one of its
//! buttons, goes back out on the bus as the specification's two signals.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use iced::Subscription;
use iced::futures::channel::mpsc::UnboundedSender;
use iced::window;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use crate::ui::Message;

/// The name the server owns on the session bus.
const NAME: &str = "org.freedesktop.Notifications";
/// The object it answers at.
const PATH: &str = "/org/freedesktop/Notifications";
/// The version of the specification it follows.
const SPEC: &str = "1.2";
/// How long to wait before asking for the name again when the bus would not give it.
const RETRY: Duration = Duration::from_secs(5);
/// How long a notification that is not critical stays on screen.
pub const SHOWN: Duration = Duration::from_secs(5);
/// The most notifications on screen at once. A new one past that takes the place of the oldest.
pub const ON_SCREEN: usize = 3;
/// The most the clock menu keeps. Past that the oldest are forgotten.
pub const KEPT: usize = 50;
/// The most buttons a notification shows, as GNOME does.
pub const BUTTONS: usize = 3;
/// The key of the action a click on the notification itself takes.
const DEFAULT: &str = "default";

/// How urgent a notification says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Urgency {
    /// Low.
    Low,
    /// Normal, which is what a notification that says nothing is.
    #[default]
    Normal,
    /// Critical: it stays on screen until it is closed, and shows while Do not disturb is on.
    Critical,
}

/// Why a notification closed, in the numbers the `NotificationClosed` signal carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Its time on screen ran out.
    Expired = 1,
    /// The owner closed it, pressed it, or pressed one of its buttons.
    Dismissed = 2,
    /// The app that sent it closed it with `CloseNotification`.
    Recalled = 3,
}

/// A notification as an app sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// The id the server gave it, which the app closes it by.
    pub id: u32,
    /// The name the app gave itself.
    pub app: String,
    /// The icon to show: an image the hints name, or the app's icon, as a name or a path.
    pub icon: Option<String>,
    /// The desktop entry the app says it is, whose icon is shown when it gave none.
    pub entry: Option<String>,
    /// One line that says what happened.
    pub summary: String,
    /// More about it, in plain text.
    pub body: String,
    /// Its buttons, each an action key and a label, without the default action.
    pub actions: Vec<(String, String)>,
    /// Whether a click on the notification itself does something in the app.
    pub default: bool,
    /// How urgent it is.
    pub urgency: Urgency,
    /// A transient notification is shown and not kept.
    pub transient: bool,
}

impl Notification {
    /// A notification from what `Notify` was called with. The hints this server reads are
    /// `urgency`, `transient`, `desktop-entry` and `image-path`; the rest are left alone.
    #[must_use]
    pub fn read(
        id: u32,
        app: &str,
        icon: String,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        mut hints: HashMap<String, OwnedValue>,
    ) -> Self {
        let text = |hints: &mut HashMap<String, OwnedValue>, key: &str| {
            hints
                .remove(key)
                .and_then(|value| String::try_from(value).ok())
                .filter(|text| !text.trim().is_empty())
        };
        let image = text(&mut hints, "image-path").or_else(|| text(&mut hints, "image_path"));
        let entry = text(&mut hints, "desktop-entry");
        let urgency = hints
            .remove("urgency")
            .and_then(|value| number(&value))
            .map_or(Urgency::Normal, |level| match level {
                0 => Urgency::Low,
                2 => Urgency::Critical,
                _ => Urgency::Normal,
            });
        let transient = hints.remove("transient").is_some_and(|value| {
            bool::try_from(&value).unwrap_or_else(|_| number(&value).is_some_and(|flag| flag != 0))
        });
        let mut pairs = Vec::new();
        let mut default = false;
        let mut words = actions.into_iter();
        while let (Some(key), Some(label)) = (words.next(), words.next()) {
            if key == DEFAULT {
                default = true;
            } else if !label.trim().is_empty() && pairs.len() < BUTTONS {
                pairs.push((key, label));
            }
        }
        Self {
            id,
            // one line, the way the apps that sent one are remembered and turned quiet
            app: app.split_whitespace().collect::<Vec<_>>().join(" "),
            icon: image.or_else(|| Some(icon).filter(|icon| !icon.trim().is_empty())),
            entry,
            // a summary is one line
            summary: summary.split_whitespace().collect::<Vec<_>>().join(" "),
            body: body.trim().to_string(),
            actions: pairs,
            default,
            urgency,
            transient,
        }
    }

    /// The icon name or path with a `file://` in front taken off.
    #[must_use]
    pub fn icon_name(&self) -> Option<&str> {
        self.icon
            .as_deref()
            .map(|icon| icon.strip_prefix("file://").unwrap_or(icon))
    }
}

/// A number a hint holds, whichever integer type the app sent it as.
fn number(value: &OwnedValue) -> Option<i64> {
    u8::try_from(value)
        .map(i64::from)
        .or_else(|_| i32::try_from(value).map(i64::from))
        .or_else(|_| u32::try_from(value).map(i64::from))
        .or_else(|_| i64::try_from(value))
        .ok()
}

/// What the shell says on the bus about a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// It closed, and why.
    Closed(u32, Reason),
    /// One of its actions was taken: a button, or a click on it for the default action.
    Action(u32, String),
}

/// Where the shell hands its signals to the thread that holds the connection, which sends them in
/// the order they came: an action is always said before the close that follows it.
#[derive(Debug, Clone)]
pub struct Outbox(Sender<Signal>);

impl Outbox {
    /// Send this signal once the connection gets to it.
    pub fn send(&self, signal: Signal) {
        let _ = self.0.send(signal);
    }
}

/// The object on the bus. It gives out the ids, so `Notify` can answer at once, and passes
/// everything else to the shell.
struct Server {
    shell: UnboundedSender<Message>,
    next: AtomicU32,
}

impl Server {
    /// The id for a notification: the one it replaces when that is one this server gave out,
    /// otherwise the next one.
    fn id_for(&self, replaces: u32) -> u32 {
        let next = self.next.load(Ordering::SeqCst);
        if replaces != 0 && replaces < next {
            return replaces;
        }
        self.next.fetch_add(1, Ordering::SeqCst)
    }
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    /// What this server does with a notification: a body, and buttons for its actions.
    #[allow(clippy::unused_self)]
    fn get_capabilities(&self) -> Vec<String> {
        vec!["actions".to_string(), "body".to_string()]
    }

    /// Show a notification, and give back its id. The time on screen is the server's, five
    /// seconds or until it is closed when it is critical, so the time an app asks for is not read.
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: String,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let _ = expire_timeout;
        let id = self.id_for(replaces_id);
        let notification =
            Notification::read(id, app_name, app_icon, summary, body, actions, hints);
        let _ = self.shell.unbounded_send(Message::Notified(notification));
        id
    }

    /// Close a notification the app sent. One that is already gone is left alone.
    fn close_notification(&self, id: u32) {
        let _ = self.shell.unbounded_send(Message::Recalled(id));
    }

    /// The server's name, who makes it, its version and the version of the specification.
    #[zbus(out_args("name", "vendor", "version", "spec_version"))]
    #[allow(clippy::unused_self)]
    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "Lens".to_string(),
            "Rift".to_string(),
            librift::VERSION.to_string(),
            SPEC.to_string(),
        )
    }

    /// A notification closed.
    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    /// An action of a notification was taken.
    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

/// The server, on a thread of its own: it takes the name on the session bus, hands the shell an
/// outbox for the signals, then every notification as it comes. A bus that will not give the name,
/// because another server has it, is asked again every few seconds.
pub fn serve() -> Subscription<Message> {
    Subscription::run_with("notifications", |_| {
        let (shell, receiver) = iced::futures::channel::mpsc::unbounded();
        thread::spawn(move || {
            let (outbox, signals) = mpsc::channel();
            if shell
                .unbounded_send(Message::Outbox(Outbox(outbox)))
                .is_err()
            {
                return;
            }
            let connection = loop {
                let server = Server {
                    shell: shell.clone(),
                    next: AtomicU32::new(1),
                };
                match connect(server) {
                    Ok(connection) => break connection,
                    Err(why) => eprintln!("lens: could not serve notifications: {why}"),
                }
                thread::sleep(RETRY);
                if shell.is_closed() {
                    return;
                }
            };
            eprintln!("lens: serving notifications as {NAME}");
            // the connection answers on threads of its own; this one sends what the shell says
            while let Ok(signal) = signals.recv() {
                if let Err(why) = emit(&connection, &signal) {
                    eprintln!("lens: could not signal {signal:?}: {why}");
                }
            }
        });
        receiver
    })
}

/// The session bus with the object served and the name taken.
fn connect(server: Server) -> zbus::Result<zbus::blocking::Connection> {
    zbus::blocking::connection::Builder::session()?
        .name(NAME)?
        .serve_at(PATH, server)?
        .build()
}

/// Send one signal.
fn emit(connection: &zbus::blocking::Connection, signal: &Signal) -> zbus::Result<()> {
    let object = connection.object_server().interface::<_, Server>(PATH)?;
    let emitter = object.signal_emitter();
    zbus::block_on(async {
        match signal {
            Signal::Closed(id, reason) => {
                Server::notification_closed(emitter, *id, *reason as u32).await
            }
            Signal::Action(id, key) => Server::action_invoked(emitter, *id, key).await,
        }
    })
}

/// A notification drawn for the screen: its icon found and its words cut to what fits, which is
/// worked out once, when it comes in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fitted {
    /// The icon's file, when it has one.
    pub icon: Option<PathBuf>,
    /// The summary, on one line.
    pub summary: String,
    /// The body, in at most three lines.
    pub body: String,
    /// How many lines the body takes.
    pub lines: u32,
    /// The summary as a row of the clock menu has room for it.
    pub row_summary: String,
    /// The body's first line as a row of the clock menu has room for it.
    pub row_body: String,
}

/// A notification on screen.
#[derive(Debug, Clone)]
pub struct Banner {
    /// Its surface.
    pub id: window::Id,
    /// What it shows.
    pub notification: Notification,
    /// How it shows it.
    pub fitted: Fitted,
    /// How tall its surface is.
    pub height: u32,
    /// How far under the bar its surface was put.
    pub top: u32,
    /// Which run of its time is the one that counts: a timer from an earlier run is ignored.
    pub epoch: u64,
    /// The pointer is over it, so its time does not run out.
    pub hovered: bool,
}

/// A notification the clock menu lists.
#[derive(Debug, Clone)]
pub struct Kept {
    /// What it said.
    pub notification: Notification,
    /// How the list shows it.
    pub fitted: Fitted,
    /// The minute it came in, `20:41`.
    pub time: String,
}

/// Something the shell does because the notifications changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Open a notification's surface, this tall, this far under the bar.
    Open(window::Id, u32, u32),
    /// Move one to this far under the bar.
    Move(window::Id, u32),
    /// Make one this tall.
    Resize(window::Id, u32),
    /// Close one.
    Close(window::Id),
    /// Say something on the bus.
    Signal(Signal),
    /// Start a notification's time on screen: when it runs out, this epoch of it expires.
    Time(u32, u64),
}

/// The notifications on screen and the ones kept for the clock menu.
#[derive(Debug, Default)]
pub struct Notices {
    /// On screen, top to bottom, the oldest at the top.
    pub banners: Vec<Banner>,
    /// Kept for the clock menu, the newest first.
    pub kept: Vec<Kept>,
    /// Do not disturb: only critical notifications show.
    pub quiet: bool,
    /// The apps whose banners the owner keeps off the screen, the same way.
    pub muted: Vec<String>,
    /// Counts the runs of every notification's time.
    epoch: u64,
}

/// The gap under the bar, between two notifications, and from the right edge of the screen.
pub const GAP: u32 = 8;

impl Notices {
    /// No notifications yet, with Do not disturb and the apps kept quiet as the owner left them.
    #[must_use]
    pub fn new(quiet: bool, muted: Vec<String>) -> Self {
        Self {
            quiet,
            muted,
            ..Self::default()
        }
    }

    /// Whether a notification that is not critical stays off the screen: Do not disturb is on, or
    /// its app is one the owner keeps quiet.
    #[must_use]
    pub fn hushed(&self, app: &str) -> bool {
        self.quiet || self.muted.iter().any(|muted| muted == app)
    }

    /// A notification came in, with its height on screen and the minute it came in.
    pub fn arrive(
        &mut self,
        notification: Notification,
        fitted: Fitted,
        height: u32,
        time: &str,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        let id = notification.id;
        // an app that sends a notification again with the same id replaces it
        self.kept.retain(|kept| kept.notification.id != id);
        if !notification.transient {
            self.kept.insert(
                0,
                Kept {
                    notification: notification.clone(),
                    fitted: fitted.clone(),
                    time: time.to_string(),
                },
            );
            self.kept.truncate(KEPT);
        }
        let critical = notification.urgency == Urgency::Critical;
        if let Some(banner) = self
            .banners
            .iter_mut()
            .find(|banner| banner.notification.id == id)
        {
            self.epoch += 1;
            banner.notification = notification;
            banner.fitted = fitted;
            banner.epoch = self.epoch;
            if banner.height != height {
                banner.height = height;
                effects.push(Effect::Resize(banner.id, height));
            }
            if !critical {
                effects.push(Effect::Time(id, self.epoch));
            }
            effects.extend(self.restack());
            return effects;
        }
        if self.hushed(&notification.app) && !critical {
            effects.push(Effect::Signal(Signal::Closed(id, Reason::Expired)));
            return effects;
        }
        if self.banners.len() >= ON_SCREEN {
            let oldest = self.banners.remove(0);
            effects.push(Effect::Close(oldest.id));
            effects.push(Effect::Signal(Signal::Closed(
                oldest.notification.id,
                Reason::Expired,
            )));
        }
        effects.extend(self.restack());
        self.epoch += 1;
        let top = GAP
            + self
                .banners
                .iter()
                .map(|banner| banner.height + GAP)
                .sum::<u32>();
        let banner = Banner {
            id: window::Id::unique(),
            notification,
            fitted,
            height,
            top,
            epoch: self.epoch,
            hovered: false,
        };
        effects.push(Effect::Open(banner.id, height, top));
        if !critical {
            effects.push(Effect::Time(id, self.epoch));
        }
        self.banners.push(banner);
        effects
    }

    /// A notification's time on screen ran out. A timer from an earlier run does nothing, and
    /// neither does one that ends while the pointer is over the notification.
    pub fn expire(&mut self, id: u32, epoch: u64) -> Vec<Effect> {
        let Some(at) = self
            .banners
            .iter()
            .position(|banner| banner.notification.id == id && banner.epoch == epoch)
        else {
            return Vec::new();
        };
        if self.banners[at].hovered {
            return Vec::new();
        }
        self.take_off(at, Reason::Expired)
    }

    /// The pointer came over a notification or left it. Leaving starts its time again.
    pub fn hover(&mut self, id: u32, over: bool) -> Vec<Effect> {
        let Some(banner) = self
            .banners
            .iter_mut()
            .find(|banner| banner.notification.id == id)
        else {
            return Vec::new();
        };
        banner.hovered = over;
        if over || banner.notification.urgency == Urgency::Critical {
            return Vec::new();
        }
        self.epoch += 1;
        banner.epoch = self.epoch;
        vec![Effect::Time(id, self.epoch)]
    }

    /// The owner closed a notification on screen: it goes, from the list as well.
    pub fn dismiss(&mut self, id: u32) -> Vec<Effect> {
        self.kept.retain(|kept| kept.notification.id != id);
        self.banners
            .iter()
            .position(|banner| banner.notification.id == id)
            .map_or_else(Vec::new, |at| self.take_off(at, Reason::Dismissed))
    }

    /// The owner pressed a notification, or one of its buttons with `Some(key)`: the app hears
    /// which, and the notification goes. A press on one with no default action only closes it.
    pub fn act(&mut self, id: u32, key: Option<&str>) -> Vec<Effect> {
        let Some(banner) = self
            .banners
            .iter()
            .find(|banner| banner.notification.id == id)
        else {
            return Vec::new();
        };
        let taken = match key {
            Some(key) => banner
                .notification
                .actions
                .iter()
                .any(|(action, _)| action == key)
                .then(|| key.to_string()),
            None => banner.notification.default.then(|| DEFAULT.to_string()),
        };
        let mut effects: Vec<Effect> = taken
            .map(|key| Effect::Signal(Signal::Action(id, key)))
            .into_iter()
            .collect();
        effects.extend(self.dismiss(id));
        effects
    }

    /// The app closed its notification: it goes from the screen and from the list.
    pub fn recall(&mut self, id: u32) -> Vec<Effect> {
        self.kept.retain(|kept| kept.notification.id != id);
        self.banners
            .iter()
            .position(|banner| banner.notification.id == id)
            .map_or_else(Vec::new, |at| self.take_off(at, Reason::Recalled))
    }

    /// Clear in the clock menu: every notification goes, the ones on screen as well.
    pub fn clear(&mut self) -> Vec<Effect> {
        self.kept.clear();
        let mut effects = Vec::new();
        for banner in std::mem::take(&mut self.banners) {
            effects.push(Effect::Close(banner.id));
            effects.push(Effect::Signal(Signal::Closed(
                banner.notification.id,
                Reason::Dismissed,
            )));
        }
        effects
    }

    /// Whether there is anything for Clear to clear.
    #[must_use]
    pub fn any(&self) -> bool {
        !self.kept.is_empty() || !self.banners.is_empty()
    }

    /// Take the notification at this place off the screen, and move the ones under it up.
    fn take_off(&mut self, at: usize, reason: Reason) -> Vec<Effect> {
        let banner = self.banners.remove(at);
        let mut effects = vec![
            Effect::Close(banner.id),
            Effect::Signal(Signal::Closed(banner.notification.id, reason)),
        ];
        effects.extend(self.restack());
        effects
    }

    /// Put the notifications on screen one under the other again, after one came, went or
    /// changed its height, and move the surfaces of the ones that are somewhere else now.
    fn restack(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let mut top = GAP;
        for banner in &mut self.banners {
            if banner.top != top {
                banner.top = top;
                effects.push(Effect::Move(banner.id, top));
            }
            top += banner.height + GAP;
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Str;

    fn sent(id: u32, summary: &str, hints: &[(&str, OwnedValue)]) -> Notification {
        Notification::read(
            id,
            "notify-send",
            String::new(),
            summary,
            "The body.",
            Vec::new(),
            hints
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.try_clone().expect("a value")))
                .collect(),
        )
    }

    fn critical(id: u32) -> Notification {
        sent(id, "Critical", &[("urgency", OwnedValue::from(2_u8))])
    }

    fn opened(effects: &[Effect]) -> Vec<(u32, u32)> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Open(_, height, top) => Some((*height, *top)),
                _ => None,
            })
            .collect()
    }

    fn signals(effects: &[Effect]) -> Vec<Signal> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Signal(signal) => Some(signal.clone()),
                _ => None,
            })
            .collect()
    }

    fn timer(effects: &[Effect]) -> Option<(u32, u64)> {
        effects.iter().find_map(|effect| match effect {
            Effect::Time(id, epoch) => Some((*id, *epoch)),
            _ => None,
        })
    }

    #[test]
    fn the_hints_say_how_urgent_and_whose_it_is() {
        let plain = sent(1, "Plain", &[]);
        assert_eq!(plain.urgency, Urgency::Normal);
        assert!(!plain.transient);
        assert_eq!(plain.icon, None);
        let urgent = sent(
            2,
            "Urgent",
            &[
                ("urgency", OwnedValue::from(2_u8)),
                ("transient", OwnedValue::from(true)),
                ("desktop-entry", OwnedValue::from(Str::from("firefox"))),
                (
                    "image-path",
                    OwnedValue::from(Str::from("file:///tmp/a.png")),
                ),
                ("sender-pid", OwnedValue::from(4242_i64)),
            ],
        );
        assert_eq!(urgent.urgency, Urgency::Critical);
        assert!(urgent.transient);
        assert_eq!(urgent.entry.as_deref(), Some("firefox"));
        assert_eq!(urgent.icon_name(), Some("/tmp/a.png"));
        // an app that sends the urgency as another integer still says it
        let low = sent(3, "Low", &[("urgency", OwnedValue::from(0_i32))]);
        assert_eq!(low.urgency, Urgency::Low);
    }

    #[test]
    fn actions_come_in_pairs_and_the_default_one_is_no_button() {
        let notification = Notification::read(
            7,
            "mail",
            "mail-unread".into(),
            "Two\nlines",
            "  body  ",
            ["default", "Open", "reply", "Reply", "later", "", "odd"]
                .map(String::from)
                .to_vec(),
            HashMap::new(),
        );
        assert!(notification.default);
        assert_eq!(
            notification.actions,
            [("reply".to_string(), "Reply".to_string())]
        );
        assert_eq!(notification.summary, "Two lines");
        assert_eq!(notification.body, "body");
        assert_eq!(notification.icon_name(), Some("mail-unread"));
    }

    #[test]
    fn notifications_stack_under_the_bar_and_the_oldest_makes_room() {
        let mut notices = Notices::new(false, Vec::new());
        let first = notices.arrive(sent(1, "One", &[]), Fitted::default(), 60, "09:00");
        assert_eq!(opened(&first), [(60, GAP)]);
        assert_eq!(timer(&first).map(|(id, _)| id), Some(1));
        let second = notices.arrive(sent(2, "Two", &[]), Fitted::default(), 80, "09:01");
        assert_eq!(opened(&second), [(80, GAP + 60 + GAP)]);
        notices.arrive(sent(3, "Three", &[]), Fitted::default(), 40, "09:02");
        let fourth = notices.arrive(sent(4, "Four", &[]), Fitted::default(), 40, "09:03");
        // the first leaves as if its time ran out, and the rest move up
        assert_eq!(signals(&fourth), [Signal::Closed(1, Reason::Expired)]);
        assert_eq!(notices.banners.len(), ON_SCREEN);
        assert_eq!(opened(&fourth), [(40, GAP + 80 + GAP + 40 + GAP)]);
        assert!(fourth.contains(&Effect::Move(notices.banners[0].id, GAP)));
        assert_eq!(notices.kept.len(), 4);
        assert_eq!(notices.kept[0].notification.summary, "Four");
        assert_eq!(notices.kept[0].time, "09:03");
    }

    #[test]
    fn time_runs_out_unless_it_is_critical_or_under_the_pointer() {
        let mut notices = Notices::new(false, Vec::new());
        let effects = notices.arrive(sent(1, "One", &[]), Fitted::default(), 60, "09:00");
        let (id, epoch) = timer(&effects).expect("a timer");
        // the pointer holds it
        assert!(notices.hover(id, true).is_empty());
        assert!(notices.expire(id, epoch).is_empty());
        let left = notices.hover(id, false);
        let (_, again) = timer(&left).expect("a new timer");
        // the old timer is spent, the new one closes it and it stays in the list
        assert!(notices.expire(id, epoch).is_empty());
        let gone = notices.expire(id, again);
        assert_eq!(signals(&gone), [Signal::Closed(1, Reason::Expired)]);
        assert!(notices.banners.is_empty());
        assert_eq!(notices.kept.len(), 1);

        let effects = notices.arrive(critical(2), Fitted::default(), 60, "09:01");
        assert_eq!(timer(&effects), None);
        assert!(notices.hover(2, false).is_empty());
    }

    #[test]
    fn closing_pressing_and_recalling_take_it_out_of_the_list() {
        let mut notices = Notices::new(false, Vec::new());
        let mut with_buttons = sent(1, "Buttons", &[]);
        with_buttons.actions = vec![("open".into(), "Open".into())];
        with_buttons.default = true;
        notices.arrive(with_buttons, Fitted::default(), 100, "09:00");
        notices.arrive(sent(2, "Plain", &[]), Fitted::default(), 60, "09:00");
        notices.arrive(sent(3, "Third", &[]), Fitted::default(), 60, "09:00");

        // a key the notification does not have is not said
        let pressed = notices.act(1, Some("delete"));
        assert_eq!(signals(&pressed), [Signal::Closed(1, Reason::Dismissed)]);
        assert!(pressed.contains(&Effect::Move(notices.banners[0].id, GAP)));

        let clicked = notices.act(2, None);
        assert_eq!(signals(&clicked), [Signal::Closed(2, Reason::Dismissed)]);
        let recalled = notices.recall(3);
        assert_eq!(signals(&recalled), [Signal::Closed(3, Reason::Recalled)]);
        assert!(notices.banners.is_empty());
        assert!(notices.kept.is_empty());

        let mut again = sent(4, "Again", &[]);
        again.actions = vec![("open".into(), "Open".into())];
        again.default = true;
        notices.arrive(again, Fitted::default(), 100, "09:00");
        let button = notices.act(4, Some("open"));
        assert_eq!(
            signals(&button),
            [
                Signal::Action(4, "open".into()),
                Signal::Closed(4, Reason::Dismissed)
            ]
        );
        notices.arrive(sent(5, "Default", &[]), Fitted::default(), 60, "09:00");
        notices.banners[0].notification.default = true;
        assert_eq!(
            signals(&notices.act(5, None)),
            [
                Signal::Action(5, "default".into()),
                Signal::Closed(5, Reason::Dismissed)
            ]
        );
        // an id that is gone is left alone
        assert!(notices.recall(5).is_empty());
        assert!(notices.act(5, None).is_empty());
    }

    #[test]
    fn a_notification_sent_again_is_replaced_where_it_is() {
        let mut notices = Notices::new(false, Vec::new());
        notices.arrive(sent(1, "One", &[]), Fitted::default(), 60, "09:00");
        notices.arrive(sent(2, "Two", &[]), Fitted::default(), 60, "09:00");
        let replaced = notices.arrive(sent(1, "One again", &[]), Fitted::default(), 80, "09:05");
        assert!(opened(&replaced).is_empty());
        assert!(replaced.contains(&Effect::Resize(notices.banners[0].id, 80)));
        assert!(replaced.contains(&Effect::Move(notices.banners[1].id, GAP + 80 + GAP)));
        assert!(timer(&replaced).is_some());
        assert_eq!(notices.banners[0].notification.summary, "One again");
        assert_eq!(notices.kept.len(), 2);
        assert_eq!(notices.kept[0].notification.summary, "One again");
    }

    #[test]
    fn an_app_kept_quiet_goes_into_the_list_without_showing() {
        let mut notices = Notices::new(false, vec!["notify-send".to_string()]);
        let quiet = notices.arrive(sent(1, "Quiet", &[]), Fitted::default(), 60, "09:00");
        assert!(opened(&quiet).is_empty());
        assert_eq!(signals(&quiet), [Signal::Closed(1, Reason::Expired)]);
        assert_eq!(notices.kept.len(), 1);
        // a critical one still shows, the way it does with Do not disturb
        assert_eq!(
            opened(&notices.arrive(critical(2), Fitted::default(), 60, "09:00")),
            [(60, GAP)]
        );
        // and another app's shows as ever
        let mut other = sent(3, "Other", &[]);
        other.app = "Firefox".to_string();
        assert_eq!(
            opened(&notices.arrive(other, Fitted::default(), 60, "09:00")).len(),
            1
        );
        assert!(!notices.hushed("Firefox") && notices.hushed("notify-send"));
    }

    #[test]
    fn do_not_disturb_keeps_all_but_critical_ones_off_the_screen() {
        let mut notices = Notices::new(true, Vec::new());
        let quiet = notices.arrive(sent(1, "Quiet", &[]), Fitted::default(), 60, "09:00");
        assert!(opened(&quiet).is_empty());
        assert_eq!(signals(&quiet), [Signal::Closed(1, Reason::Expired)]);
        assert_eq!(notices.kept.len(), 1);
        let loud = notices.arrive(critical(2), Fitted::default(), 60, "09:00");
        assert_eq!(opened(&loud), [(60, GAP)]);
        let mut passing = sent(3, "Passing", &[("transient", OwnedValue::from(true))]);
        passing.urgency = Urgency::Critical;
        notices.arrive(passing, Fitted::default(), 60, "09:00");
        assert_eq!(notices.kept.len(), 2, "a transient one is not kept");
        assert!(notices.any());
        let cleared = notices.clear();
        assert_eq!(
            signals(&cleared),
            [
                Signal::Closed(2, Reason::Dismissed),
                Signal::Closed(3, Reason::Dismissed)
            ]
        );
        assert!(!notices.any());
    }
}
