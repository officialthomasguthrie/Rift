//! The dock: the apps that stay in it, then the ones that are running, each with its own icon, a
//! mark per open window and the focused one marked out, and at the right the workspaces. What it
//! lists comes from the desktop entries and from Horizon's event stream, and a click goes back to
//! Horizon. It stands along the bottom of the screen from one side to the other with 32 pixel
//! icons, or where the owner put it on the Dock page: along the top, only as wide as what it holds
//! in the middle of its edge, with bigger icons.

use iced::widget::{button, column, container, mouse_area, row, space, text};
use iced::{Background, Border, Color, Element, Length, Shadow, Theme, window};
use librift::dock::{Edge, Options, Size};

use crate::bar;
use crate::horizon::{self, Open, Space};
use crate::icons;
use crate::launcher::App;
use crate::theme::Palette;
use crate::ui::Message;

/// The hairline along the edge of the dock that faces the windows, inside its height, and the
/// border around a dock that does not reach the sides.
const LINE: u32 = 1;
/// The room between an item and the dock's edges, above and under it together.
const ROOM: u32 = 3;
/// The padding at each end of the dock.
const PAD: u16 = 6;
/// The gap between two items.
const GAP: u32 = 4;
/// The space between the apps and the workspaces, in a dock only as wide as what it holds.
const BETWEEN: u32 = 12;
/// How far a dock that does not reach the sides stands off its edge: the gap between two windows.
pub const OFF_EDGE: u32 = 8;
/// The gap between the icon and the marks under it.
const ICON_GAP: u32 = 3;
/// One mark: a dot per open window.
const DOT: u32 = 3;
/// The gap between two marks.
const DOT_GAP: u32 = 4;
/// How many marks an item shows, however many windows the app has.
const DOTS: usize = 3;
/// The line under the focused item and under the active workspace.
const MARK: u32 = 2;
/// A workspace button.
const SPACE: u32 = 24;
/// The gap between two of them.
const SPACE_GAP: u32 = 4;
/// The corner of an item and of a workspace button.
const RADIUS: f32 = 4.0;

/// How wide the menu a right click opens is.
pub const MENU_WIDTH: u32 = 240;
/// The padding inside it.
const MENU_PAD: u32 = 8;
/// The same padding where a widget wants it.
const MENU_INSIDE: u16 = 8;
/// One row of it.
const MENU_ROW: u32 = 28;

/// How big one app in the dock is with icons of this size: the icon, the marks under it and the
/// line under those.
#[must_use]
pub const fn item(size: Size) -> u32 {
    size.icon() + ICON_GAP + DOT + MARK
}

/// How tall the dock is with icons of this size, in logical pixels: 44 with the small ones. It is
/// what the dock keeps of the screen as well, so windows stand clear of it and nothing is ever
/// hidden behind it.
#[must_use]
pub const fn height(size: Size) -> u32 {
    item(size) + LINE + ROOM
}

/// One app in the dock: pinned, running, or both.
#[derive(Debug, Clone)]
pub struct Item {
    /// What names it in the pinned list and in the state: the entry's id, or the app id of a
    /// window whose app has no entry here.
    pub key: String,
    /// The icon name to look up, when there is one.
    pub icon: Option<String>,
    /// The entry it starts from, when the app has one.
    pub app: Option<App>,
    /// Its open windows, oldest first.
    pub windows: Vec<(u64, String)>,
    /// Which of its windows has the keyboard, when one of them does.
    pub focused: Option<u64>,
    /// Whether it stays in the dock when it is not running.
    pub pinned: bool,
}

impl Item {
    /// Whether the app this item stands for is the one being used.
    #[must_use]
    pub const fn is_focused(&self) -> bool {
        self.focused.is_some()
    }

    /// The window a click goes to: the one after the one that has the focus, so a second click
    /// walks the app's windows, and the first one when none of them has it.
    #[must_use]
    pub fn next(&self) -> Option<u64> {
        let at = self
            .focused
            .and_then(|id| self.windows.iter().position(|(other, _)| *other == id));
        match at {
            Some(at) => self.windows.get((at + 1) % self.windows.len()),
            None => self.windows.first(),
        }
        .map(|(id, _)| *id)
    }

    /// What a right click on it offers: its windows by title, then the app itself.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = self
            .windows
            .iter()
            .map(|(id, title)| Row::Window(*id, title.clone()))
            .collect();
        if self.app.is_some() {
            rows.push(Row::New);
        }
        rows.push(Row::Pin(self.pinned));
        if !self.windows.is_empty() {
            rows.push(Row::Close);
        }
        rows
    }
}

/// What a row of the menu a right click opens does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// One of the app's windows, by title.
    Window(u64, String),
    /// Another window of the app.
    New,
    /// Keep the app in the dock, or stop keeping it.
    Pin(bool),
    /// Close every window the app has.
    Close,
}

impl Row {
    /// What the row says.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Window(_, title) => {
                if title.is_empty() {
                    "Window".to_string()
                } else {
                    title.clone()
                }
            }
            Self::New => "New window".to_string(),
            Self::Pin(true) => "Unpin".to_string(),
            Self::Pin(false) => "Pin to dock".to_string(),
            Self::Close => "Close".to_string(),
        }
    }
}

/// The menu a right click on an item opens, on a surface of its own over the dock.
#[derive(Debug)]
pub struct Menu {
    /// The surface it draws on.
    pub id: window::Id,
    /// The item it belongs to.
    pub key: String,
    /// What it offers, top to bottom.
    pub rows: Vec<Row>,
}

/// How tall a menu with this many rows is.
#[must_use]
pub fn menu_height(rows: usize) -> u32 {
    let rows = u32::try_from(rows).unwrap_or(0);
    MENU_PAD + rows * MENU_ROW + MENU_PAD
}

/// The dock while the shell runs.
#[derive(Debug)]
pub struct Dock {
    /// The surface it draws on.
    pub id: window::Id,
    /// Where it stands and how big its icons are.
    pub options: Options,
    /// The apps it keeps, in the order they are in.
    pub pinned: Vec<String>,
    /// What Horizon has open.
    pub open: Open,
    /// One item per app, the pinned ones first.
    pub items: Vec<Item>,
    /// The menu a right click opened, when there is one.
    pub menu: Option<Menu>,
}

impl Dock {
    /// The dock as it is when the shell starts: the pinned apps, nothing running yet.
    #[must_use]
    pub fn new(id: window::Id, apps: &[App]) -> Self {
        let mut dock = Self {
            id,
            options: Options::read(),
            pinned: librift::dock::pinned(),
            open: Open::default(),
            items: Vec::new(),
            menu: None,
        };
        dock.build(apps);
        dock
    }

    /// Horizon opened, closed, focused or moved something.
    pub fn changed(&mut self, apps: &[App], open: Open) {
        self.open = open;
        self.build(apps);
    }

    /// Keep this app in the dock, or stop keeping it, and write the list back so a restart of the
    /// shell finds it the way the owner left it.
    pub fn pin(&mut self, key: &str, apps: &[App]) {
        if let Some(at) = self.pinned.iter().position(|kept| kept == key) {
            self.pinned.remove(at);
        } else {
            self.pinned.push(key.to_string());
        }
        if let Err(why) = librift::dock::save(&self.pinned) {
            eprintln!("lens: {why}");
        }
        self.build(apps);
    }

    /// Read the list and the settings again, which Settings has just written.
    pub fn reload(&mut self, apps: &[App]) {
        self.options = Options::read();
        self.pinned = librift::dock::pinned();
        self.build(apps);
    }

    /// How tall it is, in logical pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        height(self.options.size)
    }

    /// How wide it is when it does not reach the sides: its padding and its border at each end,
    /// the apps, and the workspaces after a space.
    #[must_use]
    pub fn width(&self) -> u32 {
        let count = |length: usize| u32::try_from(length).unwrap_or(0);
        let (items, spaces) = (count(self.items.len()), count(self.open.spaces.len()));
        let apps = items * item(self.options.size) + items.saturating_sub(1) * GAP;
        let workspaces = spaces * SPACE + spaces.saturating_sub(1) * SPACE_GAP;
        let between = if items > 0 && spaces > 0 { BETWEEN } else { 0 };
        2 * (u32::from(PAD) + LINE) + apps + between + workspaces
    }

    /// The item with this key.
    #[must_use]
    pub fn item(&self, key: &str) -> Option<&Item> {
        self.items.iter().find(|item| item.key == key)
    }

    /// Where the left edge of an item is, counted from the left edge of a screen this wide, which
    /// is where the menu of a right click on it hangs. A dock that does not reach the sides stands
    /// in the middle, inside its border.
    #[must_use]
    pub fn left_of(&self, key: &str, screen: u32) -> i32 {
        let at = self
            .items
            .iter()
            .position(|item| item.key == key)
            .unwrap_or(0);
        let at = u32::try_from(at).unwrap_or(0);
        let start = if self.options.extend {
            0
        } else {
            screen.saturating_sub(self.width()) / 2 + LINE
        };
        i32::try_from(start + u32::from(PAD) + at * (item(self.options.size) + GAP)).unwrap_or(0)
    }

    /// One `key value` line for `lens --state`: what the dock lists, each with how many windows
    /// it has, and a star on the app that is being used.
    #[must_use]
    pub fn line(&self) -> String {
        self.items
            .iter()
            .map(|item| {
                let star = if item.is_focused() { "*" } else { "" };
                format!("{}:{}{star}", item.key, item.windows.len())
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The same for the workspaces: their numbers, and a star on the one on screen.
    #[must_use]
    pub fn spaces_line(&self) -> String {
        self.open
            .spaces
            .iter()
            .map(|workspace| {
                let star = if workspace.active { "*" } else { "" };
                format!("{}{star}", workspace.idx)
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn build(&mut self, apps: &[App]) {
        self.items = items(apps, &self.pinned, &self.open);
    }
}

/// The items in the order the dock draws them: the pinned apps in the order of the list, then the
/// running apps that are not pinned, in the order their first window opened.
#[must_use]
pub fn items(apps: &[App], pinned: &[String], open: &Open) -> Vec<Item> {
    let mut items: Vec<Item> = pinned
        .iter()
        .map(|key| {
            let app = apps.iter().find(|app| &app.id == key);
            Item {
                key: key.clone(),
                icon: app.and_then(|app| app.icon.clone()),
                app: app.cloned(),
                windows: Vec::new(),
                focused: None,
                pinned: true,
            }
        })
        .collect();
    for win in &open.windows {
        if win.app_id.is_empty() {
            continue;
        }
        let app = horizon::owner(apps, &win.app_id);
        let key = app.map_or_else(|| win.app_id.clone(), |app| app.id.clone());
        let found = items.iter().position(|item| item.key == key);
        let at = if let Some(at) = found {
            at
        } else {
            items.push(Item {
                key,
                icon: app.map_or_else(|| Some(win.app_id.clone()), |app| app.icon.clone()),
                app: app.cloned(),
                windows: Vec::new(),
                focused: None,
                pinned: false,
            });
            items.len() - 1
        };
        items[at].windows.push((win.id, win.title.clone()));
        if win.focused {
            items[at].focused = Some(win.id);
        }
    }
    items
}

/// The dock: the apps at the left, the workspaces at the right, on the bar's gray. One that runs
/// from side to side has a hairline along the edge that faces the windows, and one that is only as
/// wide as what it holds a border all round, like a menu.
pub fn view(look: Palette, dock: &Dock) -> Element<'_, Message> {
    let size = dock.options.size;
    let mut apps = row![].spacing(GAP).align_y(iced::Center);
    for item in &dock.items {
        apps = apps.push(item_view(look, item, size));
    }
    let mut spaces = row![].spacing(SPACE_GAP).align_y(iced::Center);
    for workspace in &dock.open.spaces {
        spaces = spaces.push(space_view(look, *workspace));
    }
    let background = move |_: &Theme| container::Style {
        background: Some(look.bar.into()),
        text_color: Some(look.text),
        ..container::Style::default()
    };
    if !dock.options.extend {
        let mut inside = row![apps].align_y(iced::Center).height(Length::Fill);
        if !dock.items.is_empty() && !dock.open.spaces.is_empty() {
            inside = inside.push(space().width(BETWEEN));
        }
        // the border is drawn over the edge of the padding, so the padding takes it in and what is
        // inside stands in the middle of the width worked out for it
        return container(inside.push(spaces))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([0, PAD + 1])
            .style(move |theme: &Theme| container::Style {
                border: Border {
                    color: look.edge,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..background(theme)
            })
            .into();
    }
    let hairline = container(space().width(Length::Fill).height(LINE)).style(move |_: &Theme| {
        container::Style {
            background: Some(look.line.into()),
            ..container::Style::default()
        }
    });
    let content = container(
        row![apps, space().width(Length::Fill), spaces]
            .align_y(iced::Center)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(dock.height() - LINE)
    .padding([0, PAD])
    .style(background);
    match dock.options.edge {
        Edge::Bottom => column![hairline, content].into(),
        Edge::Top => column![content, hairline].into(),
    }
}

/// One app: its icon, a mark per window under it, and the accent line under the one being used. A
/// left click starts it or goes to its window, a middle click opens another window, a right click
/// opens the menu.
fn item_view(look: Palette, item: &Item, size: Size) -> Element<'static, Message> {
    let whole = self::item(size);
    #[allow(clippy::cast_precision_loss)]
    let icon = size.icon() as f32;
    let body = column![
        icons::draw(look.text, item.icon.as_deref(), icon),
        space().height(ICON_GAP),
        marks(look, item.windows.len()),
        under(look, item.is_focused(), whole),
    ]
    .align_x(iced::Center);
    let inside = container(body)
        .width(whole)
        .height(whole)
        .align_x(iced::Center)
        .align_y(iced::Center)
        .clip(true);
    let focused = item.is_focused();
    let pressed = button(inside)
        .width(whole)
        .height(whole)
        .padding(0)
        .on_press(Message::Dock(item.key.clone()))
        .style(move |_: &Theme, status| fill(look, focused, status));
    mouse_area(pressed)
        .on_middle_press(Message::DockNew(item.key.clone()))
        .on_right_press(Message::DockMenu(item.key.clone()))
        .into()
}

/// One 3 px dot per open window, up to three.
fn marks(look: Palette, windows: usize) -> Element<'static, Message> {
    let mut dots = row![].spacing(DOT_GAP);
    for _ in 0..windows.min(DOTS) {
        dots = dots.push(
            container(space().width(DOT).height(DOT)).style(move |_: &Theme| container::Style {
                background: Some(look.text.into()),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    // a mark is a dot, so its corner is half of it
                    radius: 1.5.into(),
                },
                ..container::Style::default()
            }),
        );
    }
    container(dots).height(DOT).into()
}

/// The accent line along the bottom of the item or the workspace that is active, and the same
/// height of nothing under every other one, so they all stand on the same line.
fn under(look: Palette, active: bool, width: u32) -> Element<'static, Message> {
    let line = space().width(width).height(MARK);
    if !active {
        return line.into();
    }
    container(line)
        .style(move |_: &Theme| container::Style {
            background: Some(look.accent.into()),
            ..container::Style::default()
        })
        .into()
}

/// One workspace: its number, and the accent line under the one on screen. A click switches.
fn space_view(look: Palette, workspace: Space) -> Element<'static, Message> {
    let number = container(
        text(workspace.idx.to_string())
            .size(bar::TEXT_SIZE)
            .color(look.text),
    )
    .width(SPACE)
    .height(SPACE - MARK)
    .align_x(iced::Center)
    .align_y(iced::Center);
    let active = workspace.active;
    button(column![number, under(look, active, SPACE)])
        .width(SPACE)
        .height(SPACE)
        .padding(0)
        .on_press(Message::Space(workspace.idx))
        .style(move |_: &Theme, status| fill(look, active, status))
        .into()
}

/// The fill behind an item or a workspace button: the pressed one when it is the active one,
/// otherwise what the pointer is doing.
fn fill(look: Palette, active: bool, status: button::Status) -> button::Style {
    let background = if active {
        Some(look.press)
    } else {
        match status {
            button::Status::Hovered => Some(look.hover),
            button::Status::Pressed => Some(look.press),
            _ => None,
        }
    };
    button::Style {
        background: background.map(Background::from),
        text_color: look.text,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: RADIUS.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

/// The menu a right click opened: the app's windows by title, then what can be done with the app
/// itself. A rectangle with a border and no shadow, like every other menu of the shell.
pub fn menu_view(look: Palette, menu: &Menu) -> Element<'_, Message> {
    let mut rows = column![];
    for row in &menu.rows {
        rows = rows.push(menu_row(look, row));
    }
    container(rows)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(MENU_INSIDE)
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

fn menu_row(look: Palette, row: &Row) -> Element<'static, Message> {
    let label = container(
        text(row.label())
            .size(bar::TEXT_SIZE)
            .color(look.text)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(iced::Center)
    .clip(true);
    button(label)
        .width(Length::Fill)
        .height(MENU_ROW)
        .padding([0, 4])
        .on_press(Message::DockRow(row.clone()))
        .style(move |_: &Theme, status| fill(look, false, status))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::horizon::Win;
    use librift::apps::Category;

    fn app(id: &str, name: &str) -> App {
        App {
            id: id.to_string(),
            name: name.to_string(),
            exec: vec![id.to_string()],
            terminal: false,
            icon: Some(id.to_string()),
            wm_class: None,
            category: Category::Accessories,
        }
    }

    fn win(id: u64, app_id: &str, focused: bool) -> Win {
        Win {
            id,
            app_id: app_id.to_string(),
            title: format!("window {id}"),
            focused,
        }
    }

    fn apps() -> Vec<App> {
        vec![
            app("firefox", "Firefox"),
            app("com.mitchellh.ghostty", "Ghostty"),
            app("Helix", "Helix"),
        ]
    }

    fn kept() -> Vec<String> {
        vec!["firefox".to_string(), "com.mitchellh.ghostty".to_string()]
    }

    #[test]
    fn the_item_sizes_add_up_to_the_dock() {
        // the icon, the marks under it and the line under those are the whole item, and the item
        // stands inside the dock with its hairline. the small dock is the one the image has
        assert_eq!((item(Size::Small), height(Size::Small)), (40, 44));
        assert_eq!(height(Size::Large), 60);
        for size in Size::ALL {
            assert_eq!(item(size), size.icon() + ICON_GAP + DOT + MARK);
            assert!(item(size) + LINE < height(size));
            assert!(SPACE < item(size));
        }
    }

    #[test]
    fn the_pinned_apps_come_first_and_the_running_ones_after_them() {
        let open = Open {
            windows: vec![
                win(1, "Helix", false),
                win(2, "com.mitchellh.ghostty", true),
                win(3, "Helix", false),
            ],
            spaces: Vec::new(),
            ..Open::default()
        };
        let listed = items(&apps(), &kept(), &open);
        assert_eq!(
            listed
                .iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>(),
            ["firefox", "com.mitchellh.ghostty", "Helix"]
        );
        // an app that is not running keeps its place and has no marks
        assert!(listed[0].windows.is_empty());
        assert!(!listed[0].is_focused());
        assert!(listed[0].pinned);
        // the one being used is marked, and the one that is only running is not pinned
        assert!(listed[1].is_focused());
        assert_eq!(listed[2].windows.len(), 2);
        assert!(!listed[2].pinned);
    }

    #[test]
    fn a_window_of_an_app_with_no_entry_is_still_in_the_dock() {
        let open = Open {
            windows: vec![win(1, "org.gnome.Nautilus", true)],
            spaces: Vec::new(),
            ..Open::default()
        };
        let listed = items(&apps(), &[], &open);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].key, "org.gnome.Nautilus");
        assert_eq!(listed[0].icon.as_deref(), Some("org.gnome.Nautilus"));
        assert!(listed[0].app.is_none());
        // with no entry there is nothing to start, so the menu offers no new window
        assert_eq!(
            listed[0].rows(),
            [
                Row::Window(1, "window 1".to_string()),
                Row::Pin(false),
                Row::Close
            ]
        );
    }

    #[test]
    fn a_click_walks_the_windows_of_the_app() {
        let open = Open {
            windows: vec![
                win(1, "Helix", false),
                win(2, "Helix", true),
                win(3, "Helix", false),
            ],
            spaces: Vec::new(),
            ..Open::default()
        };
        // the one after the one that has the focus, and round the end
        assert_eq!(items(&apps(), &[], &open)[0].next(), Some(3));

        let open = Open {
            windows: vec![win(1, "Helix", false), win(2, "Helix", false)],
            spaces: Vec::new(),
            ..Open::default()
        };
        assert_eq!(
            items(&apps(), &[], &open)[0].next(),
            Some(1),
            "the first one when none has it"
        );

        // an app that is not running has no window to go to
        assert_eq!(items(&apps(), &kept(), &Open::default())[0].next(), None);
    }

    #[test]
    fn the_menu_of_an_app_that_is_running_and_one_that_is_not() {
        let open = Open {
            windows: vec![win(7, "firefox", true)],
            spaces: Vec::new(),
            ..Open::default()
        };
        let listed = items(&apps(), &kept(), &open);
        assert_eq!(
            listed[0].rows(),
            [
                Row::Window(7, "window 7".to_string()),
                Row::New,
                Row::Pin(true),
                Row::Close
            ]
        );
        // one that is not running has nothing to close
        assert_eq!(listed[1].rows(), [Row::New, Row::Pin(true)]);
        assert_eq!(Row::Pin(true).label(), "Unpin");
        assert_eq!(Row::Pin(false).label(), "Pin to dock");
    }

    #[test]
    fn the_state_line_says_what_is_in_the_dock_and_what_is_being_used() {
        let open = Open {
            windows: vec![
                win(1, "com.mitchellh.ghostty", true),
                win(2, "Helix", false),
            ],
            spaces: vec![
                Space {
                    idx: 1,
                    active: true,
                },
                Space {
                    idx: 2,
                    active: false,
                },
            ],
            ..Open::default()
        };
        let mut dock = Dock {
            id: window::Id::unique(),
            options: Options::default(),
            pinned: kept(),
            open: Open::default(),
            items: Vec::new(),
            menu: None,
        };
        dock.changed(&apps(), open);
        assert_eq!(dock.line(), "firefox:0 com.mitchellh.ghostty:1* Helix:1");
        assert_eq!(dock.spaces_line(), "1* 2");
        // and the menu of a right click hangs where the item is
        let step = i32::try_from(item(Size::Small) + GAP).unwrap();
        assert_eq!(dock.left_of("firefox", 1280), i32::from(PAD));
        assert_eq!(dock.left_of("Helix", 1280), i32::from(PAD) + 2 * step);

        // a dock only as wide as what it holds: three apps, the space, two workspaces, and the
        // padding and the border at each end, in the middle of the screen
        dock.options.extend = false;
        assert_eq!(dock.width(), 2 * (6 + 1) + 3 * 40 + 2 * 4 + 12 + 2 * 24 + 4);
        let left = i32::try_from((1280 - dock.width()) / 2 + 1).unwrap();
        assert_eq!(dock.left_of("firefox", 1280), left + i32::from(PAD));
        // and it grows with its icons
        dock.options.size = Size::Large;
        assert_eq!(dock.width(), 2 * (6 + 1) + 3 * 56 + 2 * 4 + 12 + 2 * 24 + 4);
    }

    #[test]
    fn pinning_an_app_adds_it_at_the_end_and_unpinning_takes_it_out() {
        let known = apps();
        let mut dock = Dock {
            id: window::Id::unique(),
            options: Options::default(),
            pinned: kept(),
            open: Open::default(),
            items: Vec::new(),
            menu: None,
        };
        dock.build(&known);
        dock.pinned.push("Helix".to_string());
        dock.build(&known);
        assert_eq!(dock.items.len(), 3);
        assert!(dock.item("Helix").is_some_and(|item| item.pinned));
        dock.pinned.retain(|key| key != "firefox");
        dock.build(&known);
        assert!(dock.item("firefox").is_none());
    }

    #[test]
    fn the_menu_is_as_tall_as_its_rows() {
        assert_eq!(menu_height(0), 2 * MENU_PAD);
        assert_eq!(menu_height(4), 2 * MENU_PAD + 4 * MENU_ROW);
    }
}
