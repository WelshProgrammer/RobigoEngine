//! The game's windows.
//!
//! Each window reads the world through read methods, draws through a [`Painter`](crate::Painter)
//! and gives back the [`Command`](s2_sim::Command) a release on one of its controls makes. The
//! coordinates and the `IO.DAT` indices the original uses are being recovered screen by screen;
//! until a screen has them, its placement here holds the window together and the test covers
//! what the window shows rather than where it puts it.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::picture::Picture;
use crate::text::{Label, Pen};
use crate::widget::WindowId;

pub mod action;
pub mod building;
pub mod buildings;
pub mod demolish;
pub mod diary;
pub mod distribution;
pub mod graph;
pub mod helpline;
pub mod map;
pub mod menu;
pub mod merchandise;
pub mod military;
pub mod music;
pub mod notice;
pub mod observe;
pub mod options;
pub mod post;
pub mod production;
pub mod quit;
pub mod reading;
pub mod routes;
pub mod rows;
pub mod saves;
pub mod select;
pub mod ships;
pub mod sound_test;
pub mod statistics;
pub mod stock;
pub mod tools;
pub mod transport;
pub mod winner;

pub use action::{ActionView, ActionWindow, Shown as ActionShown};
pub use building::{BuildingView, BuildingWindow, Page as BuildingPage, Shelf};
pub use buildings::{BuildingsRow, BuildingsView, BuildingsWindow};
pub use demolish::{DemolishView, DemolishWindow, Subject};
pub use diary::DiaryWindow;
pub use distribution::{DistributionView, DistributionWindow};
pub use graph::Plot;
pub use helpline::HelpLineWindow;
pub use map::{MapNode, MapWindow, Overlays, Shown as MapShown};
pub use menu::{MenuView, MenuWindow, Pick};
pub use merchandise::{MerchandiseView, MerchandiseWindow};
pub use military::{MilitaryView, MilitaryWindow};
pub use music::{CdWindow, MidiWindow, Mode, Play};
pub use notice::NoticeWindow;
pub use observe::ObserveWindow;
pub use options::{Change, OptionsView, OptionsWindow};
pub use post::{PostView, PostWindow, Shown};
pub use production::{ProductionRow, ProductionView, ProductionWindow};
pub use quit::{Answer, QuitWindow};
pub use reading::{Reading, ReadingWindow};
pub use routes::{RoutesWindow, Step};
pub use saves::{Ask, SavesWindow, Slot};
pub use select::{Opener, SelectWindow};
pub use ships::{Expedition, ShipsView, ShipsWindow, Walk};
pub use sound_test::{SoundTestWindow, TestEffect};
pub use statistics::{StatisticsLine, StatisticsView, StatisticsWindow};
pub use stock::{StockPage, StockRow, StockView, StockWindow};
pub use tools::{ToolsView, ToolsWindow};
pub use transport::{Move, TransportView, TransportWindow};
pub use winner::{WinnerView, WinnerWindow};

/// The window kinds the original's factory at `0x45690` knows, as far as each one is named.
pub mod kind {
    use crate::widget::WindowId;

    /// The screen that opens the others.
    pub const SELECT: WindowId = WindowId(0);
    /// How goods are shared out.
    pub const DISTRIBUTION: WindowId = WindowId(1);
    /// The military settings.
    pub const MILITARY: WindowId = WindowId(2);
    /// The game menu.
    pub const MENU: WindowId = WindowId(3);
    /// The prompt that asks before a building or a flag goes.
    pub const DEMOLISH: WindowId = WindowId(25);
    /// The buildings a player owns.
    pub const BUILDINGS: WindowId = WindowId(12);
    /// How long each building takes.
    pub const PRODUCTION: WindowId = WindowId(13);
    /// Ware counts over time.
    pub const MERCHANDISE: WindowId = WindowId(14);
    /// The tool orders.
    pub const TOOLS: WindowId = WindowId(21);
    /// The statistics graph.
    pub const STATISTICS: WindowId = WindowId(22);
    /// The ships a player owns.
    pub const SHIPS: WindowId = WindowId(23);
    /// The post office.
    pub const POST: WindowId = WindowId(15);
    /// The transport priorities.
    pub const TRANSPORT: WindowId = WindowId(24);
    /// The player who won.
    pub const WINNER: WindowId = WindowId(27);
    /// What a storehouse holds.
    pub const STOCK: WindowId = WindowId(28);
    /// The line of help the pointer's own key asks for.
    pub const HELP_LINE: WindowId = WindowId(26);
    /// The CD player.
    pub const CD_PLAYER: WindowId = WindowId(29);
    /// The MIDI player.
    pub const MIDI_PLAYER: WindowId = WindowId(30);
    /// The screen that loads a game, which `F1` opens.
    pub const LOAD_GAME: WindowId = WindowId(4);
    /// The screen that saves one, which `F2` opens.
    pub const SAVE_GAME: WindowId = WindowId(5);
    /// The readme, which `F9` opens.
    pub const README: WindowId = WindowId(18);
    /// The keyboard layout, which `F8` opens.
    pub const KEYS: WindowId = WindowId(19);
    /// A help page, which the button every screen carries opens.
    pub const HELP_PAGE: WindowId = WindowId(20);
    /// The prompt that asks before leaving, which Alt and `q` open.
    pub const QUIT: WindowId = WindowId(31);
    /// The diary, which shows the mission's story.
    pub const DIARY: WindowId = WindowId(16);
    /// The diary at its small size, which the view's own word asks for.
    pub const NOTE: WindowId = WindowId(17);
    /// The map, which the `l` key opens.
    pub const MAP: WindowId = WindowId(6);
    /// A second picture of the world around a node.
    pub const OBSERVE: WindowId = WindowId(7);
    /// The trading routes between two of a player's harbours.
    pub const ROUTES: WindowId = WindowId(46);
    /// The action menu, which the original builds outside the factory, so the number is this
    /// crate's own.
    pub const ACTION: WindowId = WindowId(48);
    /// The toolbar along the bottom of the view, which the original builds as a child of the
    /// view, so the number is this crate's own.
    pub const TOOLBAR: WindowId = WindowId(49);
    /// Robigo's own options, which S2 does not have.
    pub const OPTIONS: WindowId = WindowId(50);
    /// Robigo's own notices, which S2 does not have.
    pub const NOTICE: WindowId = WindowId(51);
    /// The main menu, which the original builds as a screen of its own outside the factory, so
    /// the number is this crate's own.
    pub const MAIN_MENU: WindowId = WindowId(52);
    /// Unlimited play's settings screen, which keeps its controls under this id.
    pub const SETTINGS: WindowId = WindowId(53);
    /// The map list over Unlimited play's settings.
    pub const MAP_LIST: WindowId = WindowId(54);
    /// The campaign's chapter screen, which keeps its buttons under this id.
    pub const CHAPTER: WindowId = WindowId(55);
    /// The campaign's chapter list, which keeps its buttons under this id.
    pub const CHAPTER_LIST: WindowId = WindowId(56);
    /// Robigo's own sound test, which the options open.
    pub const SOUND_TEST: WindowId = WindowId(57);
    /// Robigo's list of what a load took out of a save made by other rules.
    pub const REPAIRS: WindowId = WindowId(59);
    /// The World Campaign's world screen, whose buttons are kept under it.
    pub const WORLD: WindowId = WindowId(58);
    /// The first of the buildings' own windows, one per building open. The original keeps them
    /// on a list of the view's (`+0x4e1`) outside the factory, so the numbers are this crate's.
    pub const FIRST_BUILDING: WindowId = WindowId(64);

    /// Whether `id` is one of the buildings' own windows.
    #[must_use]
    pub const fn is_building(id: WindowId) -> bool {
        id.0 >= FIRST_BUILDING.0
    }
}

/// Whether a window of `id` can send a [`s2_sim::Command`].
///
/// The question a replay being watched and a map editor ask (#68): both refuse the windows that
/// change the world and leave the rest usable. A paused game asks [`live_while_paused`] instead.
#[must_use]
pub const fn sends_commands(id: WindowId) -> bool {
    matches!(
        id,
        kind::DISTRIBUTION
            | kind::TRANSPORT
            | kind::TOOLS
            | kind::MILITARY
            | kind::SHIPS
            | kind::STOCK
            | kind::DEMOLISH
            | kind::ACTION
    ) || kind::is_building(id)
}

/// Whether a window of `id` stays bright and takes input while the game is paused: the hub,
/// the game menu and every window the game menu opens. Every other window dims, ignores the
/// pointer and cannot open until the pause comes off ([ui.md](../../../docs/design/ui.md),
/// "Pausing").
#[must_use]
pub const fn live_while_paused(id: WindowId) -> bool {
    matches!(
        id,
        kind::SELECT
            | kind::MENU
            | kind::LOAD_GAME
            | kind::SAVE_GAME
            | kind::CD_PLAYER
            | kind::MIDI_PLAYER
            | kind::README
            | kind::KEYS
            | kind::QUIT
            | kind::OPTIONS
            | kind::SOUND_TEST
            | kind::NOTICE
            | kind::REPAIRS
    )
}

/// Whether a window of `id` may open while the game is paused, which only the windows
/// [`live_while_paused`] names may.
#[must_use]
pub const fn opens_while_paused(id: WindowId) -> bool {
    live_while_paused(id)
}

/// The text slot the title of a window of `id` shows, or nothing for a window with no title.
///
/// The factory table lists the slot each constructor sets. The readme and the keyboard layout take
/// theirs from the constructor argument instead, and the prompt that asks before leaving picks
/// between two by the campaign flag, so the campaign's slot is the one left out here.
#[must_use]
pub const fn title(id: WindowId) -> Option<u16> {
    let slot = match id {
        kind::ACTION => 156,
        kind::SELECT => 175,
        kind::DISTRIBUTION => 174,
        kind::MILITARY => 176,
        kind::MENU => 155,
        kind::LOAD_GAME => 195,
        kind::SAVE_GAME => 194,
        kind::MAP => 157,
        kind::OBSERVE => 158,
        kind::BUILDINGS => 142,
        kind::PRODUCTION => 147,
        kind::MERCHANDISE => 178,
        kind::POST => 179,
        kind::DIARY | kind::NOTE => 180,
        kind::README => 172,
        kind::KEYS => 182,
        kind::HELP_PAGE => 183,
        kind::TOOLS => 184,
        kind::STATISTICS => 185,
        kind::SHIPS => 187,
        kind::TRANSPORT => 189,
        kind::DEMOLISH => 197,
        kind::HELP_LINE => 554,
        kind::WINNER => 561,
        kind::STOCK => 148,
        kind::CD_PLAYER => 181,
        kind::MIDI_PLAYER => 188,
        kind::QUIT => 190,
        kind::ROUTES => 571,
        _ => return None,
    };
    Some(slot)
}

/// The item a window of `id` fills its client area with, or nothing for the one the base
/// window class draws ([`Control::Pattern`](crate::skin::Control::Pattern)).
///
/// The base class names `patter01` (`0x8eca4`). Eight classes make the client widget themselves
/// and name an item of their own: `back_01` for the ring of node actions (`0x5afb0`) and
/// `back_05` for the rest (`0x4afdc`, `0x5c8ed`, `0x5cd30`, `0x5d5e0`, `0x5dbe0`, `0x66415`,
/// `0x668e6`).
#[must_use]
pub const fn backdrop(id: WindowId) -> Option<Picture> {
    match id {
        kind::SELECT
        | kind::DISTRIBUTION
        | kind::MILITARY
        | kind::TOOLS
        | kind::TRANSPORT
        | kind::DIARY
        | kind::NOTE
        | kind::WINNER => Some(Picture::SCREEN_BACKDROP),
        kind::ACTION => Some(Picture::ACTION_BACKDROP),
        _ => None,
    }
}

/// Draws `text` in the middle of `rect` in the font and colour window style `style` names,
/// which is what a label the original builds with `0x4f128` holds (`0x4deae`). A label whose
/// flags carry no fill leaves the background as it is.
pub fn label(painter: &mut Painter, rect: Rect, style: u16, text: Label) {
    let Some(style) = painter.lettering(style) else {
        return;
    };
    painter.text_aligned(
        Point::new(rect.x, rect.y),
        style.font,
        style.colour,
        text,
        Pen::TIGHT,
        Align::CentredInBox(rect.width, rect.height),
    );
}

/// Where a window of `id` shows the line of help the pointer is over, for the windows that
/// carry a strip for it.
///
/// The original hands each of these labels to the view (`0x44490`), which writes one text into
/// every one of them at once (`0x4444c`), so a window does not fetch the line itself. The line of
/// help's own window keeps its label rather than registering it, so it is not here.
#[must_use]
pub fn help_strip(id: WindowId, area: Rect) -> Option<Rect> {
    let height = i32::try_from(area.height).unwrap_or_default();
    let narrower = |less: u32| area.width.saturating_sub(less);
    let (x, y, strip, tall) = match id {
        kind::SELECT => (12, 166, 166, 12),
        kind::BUILDINGS => (12, height - 34, narrower(24), 12),
        kind::TOOLS => (11, height - 68, 144, 16),
        kind::STOCK => (12, 306, narrower(24), 12),
        kind::ACTION => (11, height - 31, narrower(22), 14),
        _ => return None,
    };
    Some(Rect::new(area.x + x, area.y + y, strip, tall))
}

/// Which corner buttons a window of `id` carries.
///
/// The screens build their title bar through `0x4e934` with flags 11 (bits 0, 1 and 3), which
/// puts the close button and the roll button in their corners. The prompt that asks before leaving
/// goes through `0x4ee48`, which passes 0, so its four corners draw the blank plate and it closes
/// only through its own buttons. The diary's letter passes 8 (`0x6600a`), which leaves it the
/// blank plates as well. The window that names the winner passes 15 to `0x8e92c` (`0x665e5`),
/// which adds the resize button.
#[must_use]
pub const fn corner_flags(id: WindowId) -> u16 {
    match id {
        kind::QUIT | kind::MAP_LIST | kind::DIARY => 0,
        kind::WINNER => crate::chrome::ORIGINAL_FLAGS | crate::chrome::Corner::Resize.flag(),
        _ => crate::chrome::ORIGINAL_FLAGS,
    }
}

/// The window a kind takes the place of when it opens.
///
/// The two save screens replace each other: the button that opens one closes the other first
/// (`0x44d00`, `0x44d50`).
#[must_use]
pub const fn replaces(id: WindowId) -> Option<WindowId> {
    match id {
        kind::LOAD_GAME => Some(kind::SAVE_GAME),
        kind::SAVE_GAME => Some(kind::LOAD_GAME),
        _ => None,
    }
}

/// The text slot the button that opens a help page reports, which every screen carrying one
/// passes.
pub const HELP_BUTTON: u16 = 504;

/// The text slot the button that puts a screen's defaults back reports.
pub const DEFAULTS_BUTTON: u16 = 543;

/// Where the names of the building types start in the names table, which the type's own number
/// steps through.
pub const BUILDING_NAMES: u16 = 50;

/// The style the strips and the line of help's own label draw their text in.
///
/// Every strip is built with style 0 and the line of help's own label with style 5.
pub const HELP_STYLE: u16 = 0;

/// Whether a window of `id` draws its content as the frame's own client widget, which the frame
/// builds through its class's `+0x60` before the four corner buttons, so the corners draw over
/// it. The panels of text are such widgets. Every other window's controls are children built after
/// the corners and draw over them.
#[must_use]
pub const fn content_is_client(id: WindowId) -> bool {
    matches!(id, kind::README | kind::KEYS | kind::HELP_PAGE | kind::REPAIRS)
}

/// Whether a window of `id` carries the frame the base window class draws
/// ([chrome](crate::chrome)). The toolbar is the one window that does not: the node menu is
/// built outside the factory and still takes the frame and the title, through `0x4e934` at
/// `0x5ae71`.
#[must_use]
pub const fn framed(id: WindowId) -> bool {
    !matches!(id, kind::TOOLBAR)
}

/// What a release on one of a window's controls does.
///
/// Most controls hand the sim a command. A few ask for something the interface owns instead, such
/// as the help page a window's own button opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A command for the sim.
    Command(s2_sim::Command),
    /// A command for the sim, after which the window that sent it closes, the way a prompt
    /// that goes ahead closes itself.
    CommandAndClose(s2_sim::Command, WindowId),
    /// The help page a window asks for.
    Help(u16),
    /// A window kind to open.
    Open(WindowId),
    /// A window kind to roll up, close or open, which is what `0x46254` does.
    Ask(WindowId),
    /// The construction aid should go on or off.
    ConstructionAid,
    /// A window kind to roll up or down.
    Roll(WindowId),
    /// A window kind to close.
    Close(WindowId),
    /// The node the view should move to.
    Centre(s2_sim::Node),
    /// Open the demolish prompt for the building on the node, in place of any open one, and
    /// close the window that asked (`0x569dc`).
    Demolish(s2_sim::Node, WindowId),
    /// The view should move to the player's own headquarters.
    Headquarters,
    /// The house names should go on or off.
    HouseNames,
    /// The game is over and the player asked to leave it.
    EndGame,
    /// What one of the two music screens asks for.
    Play(music::Play),
    /// What one of the two save screens asks for.
    Saves(saves::Ask),
    /// What the prompt that asks before leaving was answered with.
    Quit(quit::Answer),
    /// What a press on the game menu asks for.
    Menu(menu::Pick),
    /// What a press on the map screen asks for.
    Map(map::Shown),
    /// Which way a press on the trading routes screen steps.
    Routes(routes::Step),
    /// What a press on an observation window asks for.
    Observe(observe::Asked),
    /// What a press on Robigo's options asks to change.
    Options(options::Change),
    /// Play an effect from the sound test.
    TestEffect(sound_test::TestEffect),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screens_that_name_a_backdrop_fill_with_their_own_item() {
        for id in [kind::SELECT, kind::TRANSPORT, kind::DIARY, kind::WINNER] {
            assert_eq!(backdrop(id), Some(Picture::SCREEN_BACKDROP));
        }
        assert_eq!(backdrop(kind::ACTION), Some(Picture::ACTION_BACKDROP));
        for id in [kind::MENU, kind::POST, kind::MAP, kind::STOCK] {
            assert_eq!(backdrop(id), None, "{id:?} takes the one the base class draws");
        }
    }

    #[test]
    fn a_strip_sits_where_the_window_that_carries_one_puts_it() {
        let hub = Rect::new(20, 30, 190, 238);
        assert_eq!(help_strip(kind::SELECT, hub), Some(Rect::new(32, 196, 166, 12)));
        let tools = Rect::new(0, 0, 166, 432);
        assert_eq!(help_strip(kind::TOOLS, tools), Some(Rect::new(11, 364, 144, 16)));
        let stock = Rect::new(0, 0, 167, 370);
        assert_eq!(help_strip(kind::STOCK, stock), Some(Rect::new(12, 306, 143, 12)));
        let buildings = Rect::new(2, 2, 187, 370);
        assert_eq!(
            help_strip(kind::BUILDINGS, buildings),
            Some(Rect::new(14, 338, 163, 12))
        );
        for id in [kind::MENU, kind::POST, kind::PRODUCTION, kind::HELP_LINE] {
            assert_eq!(help_strip(id, hub), None, "{id:?} registers no label");
        }
    }

    #[test]
    fn a_paused_game_leaves_only_the_hub_and_the_game_menu_live() {
        for id in [
            kind::SELECT,
            kind::MENU,
            kind::SAVE_GAME,
            kind::LOAD_GAME,
            kind::CD_PLAYER,
            kind::MIDI_PLAYER,
            kind::README,
            kind::KEYS,
            kind::QUIT,
        ] {
            assert!(live_while_paused(id), "{id:?}");
            assert!(opens_while_paused(id), "{id:?}");
        }
        for id in [
            kind::DIARY,
            kind::MAP,
            kind::POST,
            kind::BUILDINGS,
            kind::HELP_PAGE,
            kind::DISTRIBUTION,
            kind::TRANSPORT,
            kind::STOCK,
            kind::DEMOLISH,
            kind::ACTION,
            kind::FIRST_BUILDING,
        ] {
            assert!(!live_while_paused(id), "{id:?}");
        }
    }
}
