//! Savegames: the seven `SAVE/*nnn.DAT` files per slot, `DIR.DAT`, `RES.DAT` and `MISSION.DAT`.
//!
//! A slot is seven files that the save routine writes one after another. This module splits each
//! into its parts and checks their outer structure without interpreting game state. The world
//! decodes fully into a [`Map`], and mails and the window layout are fully framed. Transport,
//! control and computer-player state keep their bodies as [`Raw`] blocks until reverse
//! engineering fills them in.
//!
//! The files outside the slots are small settings files: [`Directory`] (`DIR.DAT`, the slot
//! names), [`MissionProgress`] (`MISSION.DAT`), [`WorldCampaignProgress`] (Gold's
//! `MISSION2.DAT`) and [`ScreenMode`] (`RES.DAT`).
//!
//! See `docs/formats/save.md` for the evidence behind each name.

mod meta;
mod parts;

use std::fmt;

use crate::bytes::Truncated;
use crate::map::{self, Map};

pub use meta::{
    CAMPAIGN_CHAPTERS, DIRECTORY_LEN, DIRECTORY_SLOTS, DirEntry, Directory, MISSION_BUFFER_LEN,
    MissionProgress, NAME_LEN, ScreenMode, WORLD_CAMPAIGN_LEN, WORLD_CONTINENTS, WorldCampaignProgress,
};
pub use parts::{
    AI_CELL_LEN, AiGrid, CONTROL_LEN_V10X, CONTROL_LEN_V151, ComputerPlayers, Control, ControlLayout,
    DESKTOP_LEN, Desktop, Desktops, MAIL_LEN, MAX_PLAYERS, Mail, Mailboxes, TRANSPORT_CLASS, Transport,
    WINDOW_SLOTS, Window,
};

/// Number of the first slot: directory entry `n` is saved as files numbered `FIRST_SLOT + n`.
pub const FIRST_SLOT: u16 = 900;

/// Why a savegame file could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The input ended early.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// The world or area table did not decode.
    #[error(transparent)]
    Map(#[from] map::Error),
    /// A serialised object carries the wrong class tag.
    #[error("expected class tag {expected:#06x}, found {found:#06x}")]
    UnexpectedClass {
        /// The tag the part requires.
        expected: u16,
        /// The tag in the file.
        found: u16,
    },
    /// A fixed-size file has the wrong length.
    #[error("file is {len} bytes, expected {expected}")]
    Length {
        /// Bytes in the file.
        len: usize,
        /// Bytes the game reads and writes.
        expected: usize,
    },
    /// A control block is longer than the game reads.
    #[error("control block is {0} bytes, more than the {CONTROL_LEN_V151} the game reads")]
    ControlLength(usize),
    /// A desktop file is not one or two desktop records long.
    #[error("desktop file is {0} bytes, expected {DESKTOP_LEN} or twice that")]
    DesktopLength(usize),
    /// Bytes follow the last structure of a part that should end there.
    #[error("{len} unexpected bytes at offset {offset}")]
    Trailing {
        /// Where the extra bytes start.
        offset: usize,
        /// How many there are.
        len: usize,
    },
    /// A mailbox presence byte is neither 0 nor 1.
    #[error("mailbox flag {flag} for player {player}")]
    MailboxFlag {
        /// Player index, 0 to 7.
        player: usize,
        /// The byte found.
        flag: u8,
    },
    /// The computer-player grid's cell count is not columns times rows.
    #[error("AI grid is {columns}x{rows} but claims {cells} cells")]
    AiGrid {
        /// Columns the header gives.
        columns: u32,
        /// Rows the header gives.
        rows: u32,
        /// Cells the header gives.
        cells: u32,
    },
    /// A part's grid size disagrees with the world it was saved with.
    #[error("size {found:?} does not match the world's {expected:?}")]
    SizeMismatch {
        /// Width and height the part records.
        found: (u32, u32),
        /// Width and height the world implies for that part.
        expected: (u32, u32),
    },
    /// `MISSION.DAT` is shorter than the chapter list or longer than the game's buffer.
    #[error("mission file is {0} bytes, expected at least {CAMPAIGN_CHAPTERS}")]
    MissionLength(usize),
    /// A chapter flag in `MISSION.DAT` is not `'0'` or `'1'`.
    #[error("chapter {chapter} has flag byte {byte:#04x}")]
    MissionFlag {
        /// Chapter index, from 0.
        chapter: usize,
        /// The byte found.
        byte: u8,
    },
    /// An error inside one of a slot's files.
    #[error("{part}: {source}")]
    InPart {
        /// The file that failed.
        part: Part,
        /// What went wrong in it.
        source: Box<Error>,
    },
}

impl Error {
    fn in_part(self, part: Part) -> Self {
        Self::InPart {
            part,
            source: Box::new(self),
        }
    }
}

/// Something unusual that decoding accepted without failing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quirk {
    /// A quirk in the world.
    World(map::Quirk),
    /// A control block of neither known length, holding this many bytes.
    ControlLength(usize),
}

/// One of the seven files a savegame slot consists of, in the order the game writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Part {
    /// `WORLDnnn.DAT`: the map layers.
    World,
    /// `TRANSnnn.DAT`: flags, roads, buildings and settlers.
    Transport,
    /// `CONTInnn.DAT`: the area table.
    Areas,
    /// `CNTRLnnn.DAT`: game clock and per-game settings.
    Control,
    /// `DESKTnnn.DAT`: open windows.
    Desktop,
    /// `MAILSnnn.DAT`: player messages.
    Mails,
    /// `COMPYnnn.DAT`: computer-player state.
    Computer,
}

impl Part {
    /// Every part, in write order.
    pub const ALL: [Self; 7] = [
        Self::World,
        Self::Transport,
        Self::Areas,
        Self::Control,
        Self::Desktop,
        Self::Mails,
        Self::Computer,
    ];

    /// The five-letter file name prefix, such as `"WORLD"`.
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::World => "WORLD",
            Self::Transport => "TRANS",
            Self::Areas => "CONTI",
            Self::Control => "CNTRL",
            Self::Desktop => "DESKT",
            Self::Mails => "MAILS",
            Self::Computer => "COMPY",
        }
    }

    /// The file name for slot number `slot`, such as `WORLD900.DAT`, relative to `SAVE/`.
    #[must_use]
    pub fn file_name(self, slot: u16) -> String {
        format!("{}{slot:03}.DAT", self.prefix())
    }
}

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.prefix())
    }
}

/// Bytes kept as found, with their offset in the file they came from.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Raw {
    offset: usize,
    data: Vec<u8>,
}

impl Raw {
    fn new(offset: usize, data: &[u8]) -> Self {
        Self {
            offset,
            data: data.to_vec(),
        }
    }

    /// Offset of the first byte in the source file.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// The bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

impl fmt::Debug for Raw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Raw")
            .field("offset", &self.offset)
            .field("len", &self.data.len())
            .finish()
    }
}

/// The contents of one slot's files. The world and area table are required; the rest may be
/// missing, as `CNTRL900.DAT` is in the GOG Gold install.
#[derive(Debug, Clone, Copy, Default)]
pub struct SlotFiles<'a> {
    /// `WORLDnnn.DAT`.
    pub world: &'a [u8],
    /// `CONTInnn.DAT`.
    pub areas: &'a [u8],
    /// `TRANSnnn.DAT`.
    pub transport: Option<&'a [u8]>,
    /// `CNTRLnnn.DAT`.
    pub control: Option<&'a [u8]>,
    /// `DESKTnnn.DAT`.
    pub desktop: Option<&'a [u8]>,
    /// `MAILSnnn.DAT`.
    pub mails: Option<&'a [u8]>,
    /// `COMPYnnn.DAT`.
    pub computer: Option<&'a [u8]>,
}

/// A decoded savegame slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    world: Map,
    transport: Option<Transport>,
    control: Option<Control>,
    desktops: Option<Desktops>,
    mails: Option<Mailboxes>,
    computer: Option<ComputerPlayers>,
    quirks: Vec<Quirk>,
}

impl Slot {
    /// Decodes every file the slot has and checks that their grid sizes agree with the world.
    ///
    /// # Errors
    ///
    /// [`Error::InPart`] naming the file that failed, around the cause.
    pub fn parse(files: &SlotFiles<'_>) -> Result<Self, Error> {
        let world = Map::parse_v102(files.world, files.areas).map_err(|e| {
            let part = match e {
                map::Error::AreaTableLength(_) => Part::Areas,
                _ => Part::World,
            };
            Error::from(e).in_part(part)
        })?;
        let size = (u32::from(world.width()), u32::from(world.height()));

        let transport = parse_part(files.transport, Part::Transport, |data| {
            let transport = Transport::parse(data)?;
            check_size(transport.size(), size)?;
            Ok(transport)
        })?;
        let control = parse_part(files.control, Part::Control, Control::parse)?;
        let desktops = parse_part(files.desktop, Part::Desktop, Desktops::parse)?;
        let mails = parse_part(files.mails, Part::Mails, Mailboxes::parse)?;
        let computer = parse_part(files.computer, Part::Computer, |data| {
            let computer = ComputerPlayers::parse(data)?;
            let grid = computer.grid();
            check_size((grid.columns(), grid.rows()), AiGrid::size_for(size))?;
            Ok(computer)
        })?;

        let quirks = world
            .quirks()
            .iter()
            .cloned()
            .map(Quirk::World)
            .chain(control.as_ref().and_then(Control::quirk))
            .collect();
        Ok(Self {
            world,
            transport,
            control,
            desktops,
            mails,
            computer,
            quirks,
        })
    }

    /// What was unusual about the slot's files. [`Slot::missing`] lists the missing files.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// The world, decoded from the world file and the area table.
    #[must_use]
    pub fn world(&self) -> &Map {
        &self.world
    }

    /// Transport state, if the slot has `TRANSnnn.DAT`.
    #[must_use]
    pub fn transport(&self) -> Option<&Transport> {
        self.transport.as_ref()
    }

    /// Control block, if the slot has `CNTRLnnn.DAT`.
    #[must_use]
    pub fn control(&self) -> Option<&Control> {
        self.control.as_ref()
    }

    /// Window layouts, if the slot has `DESKTnnn.DAT`.
    #[must_use]
    pub fn desktops(&self) -> Option<&Desktops> {
        self.desktops.as_ref()
    }

    /// Player messages, if the slot has `MAILSnnn.DAT`.
    #[must_use]
    pub fn mails(&self) -> Option<&Mailboxes> {
        self.mails.as_ref()
    }

    /// Computer-player state, if the slot has `COMPYnnn.DAT`.
    #[must_use]
    pub fn computer(&self) -> Option<&ComputerPlayers> {
        self.computer.as_ref()
    }

    /// The parts this slot lacks, in write order. The game writes all seven for a savegame.
    #[must_use]
    pub fn missing(&self) -> Vec<Part> {
        [
            (Part::Transport, self.transport.is_none()),
            (Part::Control, self.control.is_none()),
            (Part::Desktop, self.desktops.is_none()),
            (Part::Mails, self.mails.is_none()),
            (Part::Computer, self.computer.is_none()),
        ]
        .into_iter()
        .filter_map(|(part, missing)| missing.then_some(part))
        .collect()
    }
}

fn parse_part<T>(
    data: Option<&[u8]>,
    part: Part,
    parse: impl FnOnce(&[u8]) -> Result<T, Error>,
) -> Result<Option<T>, Error> {
    data.map(parse).transpose().map_err(|e| e.in_part(part))
}

fn check_size(found: (u32, u32), expected: (u32, u32)) -> Result<(), Error> {
    if found == expected {
        Ok(())
    } else {
        Err(Error::SizeMismatch { found, expected })
    }
}

#[cfg(test)]
mod tests;
