//! What Unlimited play's settings screen holds: the map, who plays each of its players and the
//! six settings, with what each of the screen's controls does to them.

use s2_mods::Sheets;
use s2_sim::{Controller, Objective, PlayerSlots, Portraits, Roster, StartGoods, StartOrder};
use s2_ui::screens::maps::{self, MapList, MapListView, MapPick, Scrolled};
use s2_ui::screens::settings::{Pick, RowView, SettingsView};
use s2_ui::text::Labels;

use crate::error::Result;
use crate::scene::{AllianceMode, Start, source};

/// The map the screen opens on (`0x5e10e`).
pub(crate) const FIRST_MAP: &str = "DATA/MAPS3/OMAP00.WLD";

/// How many values the three-way settings take.
const THREE: u8 = 3;

/// The folder the map list shows and the ending of its maps (`0x51e78` with 1).
const FOLDER: &str = "DATA/MAPS3";
const ENDING: &str = ".WLD";

/// The map list while it is open.
pub(crate) struct Maps {
    /// The maps, by data path.
    paths: Vec<String>,
    /// Their names as the list shows them: the file name in lower case without its ending.
    names: Vec<Vec<u8>>,
    chosen: usize,
    first: usize,
    /// The three lines about the chosen map.
    lines: [Vec<u8>; 3],
}

impl Maps {
    /// The list of the maps the data `start` names, on `current`, with words from `texts`.
    pub(crate) fn open(start: &Start, current: &str, texts: &impl Labels) -> Self {
        let paths = start.data_set().map_or_else(
            |_| Vec::new(),
            |data| {
                Sheets {
                    stack: start.mods.as_ref(),
                    data: &data,
                }
                .maps_in(FOLDER, ENDING)
            },
        );
        let names = paths.iter().map(|path| list_name(path)).collect();
        let chosen = paths
            .iter()
            .position(|path| path.eq_ignore_ascii_case(current))
            .unwrap_or_default();
        let mut list = Self {
            paths,
            names,
            chosen,
            first: 0,
            lines: nothing(),
        };
        list.describe(start, texts);
        list
    }

    /// What the window shows, with rows `line` pixels tall.
    pub(crate) fn view(&self, line: u32) -> MapListView<'_> {
        MapListView {
            names: &self.names,
            chosen: self.chosen,
            first: self.first,
            lines: &self.lines,
            line,
        }
    }

    /// Carries out `pick`; gives back the map to take on Ok, and whether the list closes.
    pub(crate) fn pick(
        &mut self,
        pick: MapPick,
        start: &Start,
        texts: &impl Labels,
    ) -> (Option<String>, bool) {
        match pick {
            MapPick::Row(row) if row < self.paths.len() => {
                self.chosen = row;
                self.describe(start, texts);
                (None, false)
            }
            MapPick::Row(_) => (None, false),
            MapPick::Ok => (self.paths.get(self.chosen).cloned(), true),
            MapPick::Back => (None, true),
        }
    }

    /// Moves the list `way`, no further than [`MapList::most`].
    pub(crate) fn scroll(&mut self, way: Scrolled) {
        let most = MapList::most(self.paths.len());
        self.first = match way {
            Scrolled::Stay => self.first,
            Scrolled::Up => self.first.saturating_sub(1),
            Scrolled::Down => (self.first + 1).min(most),
        };
    }

    /// Reads the chosen map's header into the three lines.
    fn describe(&mut self, start: &Start, texts: &impl Labels) {
        let Some(path) = self.paths.get(self.chosen) else {
            self.lines = nothing();
            return;
        };
        let loaded = start.data_set().ok().and_then(|data| {
            let sheets = Sheets {
                stack: start.mods.as_ref(),
                data: &data,
            };
            source::load(&sheets, path).ok()
        });
        self.lines = loaded
            .as_ref()
            .and_then(|loaded| loaded.map.header().map(|header| (loaded, header)))
            .map_or_else(nothing, |(loaded, header)| {
                let mut title = vec![b'\''];
                title.extend_from_slice(trimmed(header.title_bytes()));
                title.push(b'\'');
                let mut by = texts.slot(maps::BY).to_vec();
                by.extend_from_slice(b": ");
                by.extend_from_slice(trimmed(header.author_bytes()));
                let mut size = texts.slot(maps::WIDTH).to_vec();
                size.extend_from_slice(format!(":{} ", loaded.map.width()).as_bytes());
                size.extend_from_slice(texts.slot(maps::HEIGHT));
                size.extend_from_slice(format!(":{}", loaded.map.height()).as_bytes());
                [title, by, size]
            });
    }
}

/// The three lines for a map the list cannot read.
fn nothing() -> [Vec<u8>; 3] {
    [
        maps::NOTHING.to_vec(),
        maps::NOTHING.to_vec(),
        maps::NOTHING.to_vec(),
    ]
}

/// The name the list shows for the map at data path `path`.
fn list_name(path: &str) -> Vec<u8> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    stem.to_ascii_lowercase().into_bytes()
}

/// The settings and the players.
pub(crate) struct Unlimited {
    /// The map, as a data path.
    map: String,
    /// The map's title as the header stores it.
    title: Vec<u8>,
    /// How many players the map offers.
    offered: usize,
    roster: Roster,
    alliances: u8,
    objective: u8,
    goods: u8,
    exploration: bool,
    in_sequence: bool,
}

/// What a pick on the screen leads to besides a change to the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Leads {
    /// Nothing more: the screen shows the change.
    Stay,
    /// Back to the main menu.
    Back,
    /// The map list.
    Maps,
    /// Start the game.
    Play,
}

impl Unlimited {
    /// The screen on `map` from the data `start` names, with the settings the executable ships
    /// with.
    ///
    /// # Errors
    ///
    /// [`crate::error::Error`] when the data or the map cannot be read.
    pub(crate) fn open(start: &Start, map: &str) -> Result<Self> {
        let mut unlimited = Self {
            map: String::new(),
            title: Vec::new(),
            offered: 0,
            roster: Roster::offered(0, &Portraits::ORIGINAL),
            alliances: 0,
            objective: 0,
            goods: 1,
            exploration: true,
            in_sequence: true,
        };
        unlimited.choose_map(start, map)?;
        Ok(unlimited)
    }

    /// Takes `map` from the data `start` names, with a row for each player its header offers
    /// and has a start position for ([`PlayerSlots::room`]).
    ///
    /// # Errors
    ///
    /// [`crate::error::Error`] when the data or the map cannot be read.
    pub(crate) fn choose_map(&mut self, start: &Start, map: &str) -> Result<()> {
        let data = start.data_set()?;
        let sheets = Sheets {
            stack: start.mods.as_ref(),
            data: &data,
        };
        let loaded = source::load(&sheets, map)?;
        let header = loaded.map.header();
        self.offered = PlayerSlots::room(&loaded.map);
        self.title = header.map_or_else(
            || loaded.title.clone().into_bytes(),
            |header| trimmed(header.title_bytes()).to_vec(),
        );
        self.roster = Roster::offered(self.offered, &Portraits::ORIGINAL);
        map.clone_into(&mut self.map);
        Ok(())
    }

    /// The map the settings are on, as a data path.
    pub(crate) fn map(&self) -> &str {
        &self.map
    }

    /// Carries out `pick` and says where it leads.
    pub(crate) fn pick(&mut self, pick: Pick) -> Leads {
        let player = |row: u8| s2_sim::Player::new(row);
        match pick {
            Pick::InGame(row) => {
                if let Some(player) = player(row) {
                    self.roster.toggle(player);
                }
            }
            Pick::Leader(row) => {
                if let Some(player) = player(row) {
                    self.roster.next_face(player);
                }
            }
            Pick::Mode(row) => {
                if let Some(player) = player(row) {
                    self.roster.next_controller(player);
                }
            }
            Pick::Alliances => self.alliances = (self.alliances + 1) % THREE,
            Pick::Objective => self.objective = (self.objective + 1) % THREE,
            Pick::Goods => self.goods = (self.goods + 1) % THREE,
            Pick::Exploration => self.exploration = !self.exploration,
            Pick::StartPosition => self.in_sequence = !self.in_sequence,
            Pick::Scenario => return Leads::Maps,
            Pick::Return => return Leads::Back,
            Pick::Start => return Leads::Play,
        }
        Leads::Stay
    }

    /// The rows the screen shows, one per player the map offers.
    pub(crate) fn rows(&self) -> Vec<RowView> {
        s2_sim::Player::ALL
            .into_iter()
            .take(self.offered)
            .map(|player| {
                let entry = self.roster.entry(player);
                RowView {
                    in_game: entry.in_game,
                    face: entry.portrait.0,
                    controller: controller_index(entry.controller),
                }
            })
            .collect()
    }

    /// What the screen shows, with `rows` from [`Self::rows`].
    pub(crate) fn view<'a>(&'a self, rows: &'a [RowView]) -> SettingsView<'a> {
        SettingsView {
            rows,
            map: &self.title,
            alliances: self.alliances,
            objective: self.objective,
            goods: self.goods,
            exploration: self.exploration,
            in_sequence: self.in_sequence,
            can_start: self.roster.can_start(),
        }
    }

    /// `start` turned into the game these settings describe.
    pub(crate) fn game(&self, start: &Start) -> Start {
        Start {
            map: Some(self.map.clone()),
            load: None,
            mission: None,
            slots: Some(self.roster.slots()),
            goods: match self.goods {
                0 => StartGoods::Low,
                2 => StartGoods::High,
                _ => StartGoods::Normal,
            },
            alliances: match self.alliances {
                1 => AllianceMode::Controllers,
                2 => AllianceMode::Nations,
                _ => AllianceMode::None,
            },
            starts: if self.in_sequence {
                StartOrder::InSequence
            } else {
                StartOrder::Random
            },
            objective: match self.objective {
                1 => Objective::Land,
                2 => Objective::Domination,
                _ => Objective::None,
            },
            exploration: self.exploration,
            ..start.clone()
        }
    }
}

/// The byte the original stores for `controller` (`+0x03` of the slot record).
fn controller_index(controller: Controller) -> u8 {
    Controller::ALL
        .iter()
        .position(|&each| each == controller)
        .and_then(|place| u8::try_from(place).ok())
        .unwrap_or_default()
}

/// `title` up to its first zero byte, without the spaces after it.
fn trimmed(title: &[u8]) -> &[u8] {
    let end = title.iter().position(|&byte| byte == 0).unwrap_or(title.len());
    title[..end].trim_ascii_end()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_stops_at_its_first_zero_and_loses_its_trailing_spaces() {
        assert_eq!(trimmed(b"Island of Hills\0\0junk"), b"Island of Hills");
        assert_eq!(trimmed(b"Two  \0"), b"Two");
        assert_eq!(trimmed(b"Full"), b"Full");
    }

    #[test]
    fn the_list_names_a_map_by_its_file_in_lower_case() {
        assert_eq!(list_name("DATA/MAPS3/OMAP00.WLD"), b"omap00");
        assert_eq!(list_name("ISLAND.WLD"), b"island");
    }

    #[test]
    fn controllers_take_the_bytes_the_original_stores() {
        assert_eq!(controller_index(Controller::First), 0);
        assert_eq!(controller_index(Controller::Computer), 2);
        assert_eq!(controller_index(Controller::Beaten), 3);
    }
}
