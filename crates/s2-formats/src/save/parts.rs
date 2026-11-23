//! The slot files other than the world: transport, control, desktop, mails and computer players.

use super::{Error, Quirk, Raw};
use crate::bytes::{Reader, Truncated, to_usize};

/// Class tag of the object `TRANSnnn.DAT` serialises.
pub const TRANSPORT_CLASS: u16 = 11_100;

/// Bytes in a control block written by the v1.0x builds.
pub const CONTROL_LEN_V10X: usize = 358;

/// Bytes in a control block written by v1.51.
pub const CONTROL_LEN_V151: usize = 578;

/// Window records in a desktop.
pub const WINDOW_SLOTS: usize = 50;

/// Bytes in one desktop record.
pub const DESKTOP_LEN: usize = DESKTOP_HEADER_LEN + WINDOW_SLOTS * WINDOW_LEN + DESKTOP_TRAILER_LEN;

/// Bytes in one message record.
pub const MAIL_LEN: usize = 28;

/// Player slots in the mail and computer-player files.
pub const MAX_PLAYERS: usize = 8;

/// Bytes per cell of the computer players' shared grid.
pub const AI_CELL_LEN: usize = 21;

const DESKTOP_HEADER_LEN: usize = 4;
const WINDOW_LEN: usize = 18;
const DESKTOP_TRAILER_LEN: usize = 12;
const MAX_DESKTOPS: usize = 2;
const AI_CELL_NODES: u32 = 8;

/// `TRANSnnn.DAT`: one serialised object holding the flag, road, building and settler pools.
///
/// Only the first five fields are framed here. The body is a run of pool sections whose record
/// layouts vary per class; decoding them is RE issue #31.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transport {
    width: u16,
    height: u16,
    pool_sizes: [u16; 2],
    body: Raw,
}

impl Transport {
    /// Decodes the header and keeps the rest as a raw block.
    ///
    /// # Errors
    ///
    /// [`Error::UnexpectedClass`] when the file does not start with [`TRANSPORT_CLASS`], or
    /// [`Error::Truncated`] when the header is cut short.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        let found = r.u16()?;
        if found != TRANSPORT_CLASS {
            return Err(Error::UnexpectedClass {
                expected: TRANSPORT_CLASS,
                found,
            });
        }
        let width = r.u16()?;
        let height = r.u16()?;
        let pool_sizes = [r.u16()?, r.u16()?];
        Ok(Self {
            width,
            height,
            pool_sizes,
            body: rest(&r),
        })
    }

    /// Map width in nodes, which must match the world's.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Map height in nodes, which must match the world's.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    /// Two pool sizes stored ahead of the sections; 1000 and 2000 in every save seen so far.
    #[must_use]
    pub fn pool_sizes(&self) -> [u16; 2] {
        self.pool_sizes
    }

    /// The pool sections, undecoded.
    #[must_use]
    pub fn body(&self) -> &Raw {
        &self.body
    }

    pub(super) fn size(&self) -> (u32, u32) {
        (u32::from(self.width), u32::from(self.height))
    }
}

/// Which build wrote a control block, told apart by its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlLayout {
    /// [`CONTROL_LEN_V10X`] bytes, as v1.03 writes.
    V10x,
    /// [`CONTROL_LEN_V151`] bytes, as v1.51 writes.
    V151,
    /// Any other length up to [`CONTROL_LEN_V151`], which v1.51's reader also accepts.
    Other,
}

/// `CNTRLnnn.DAT`: a fixed block of game settings copied straight from memory.
///
/// Only the game clock is named; the rest is per-player configuration that RE issue #31 owns.
#[derive(Clone, PartialEq, Eq)]
pub struct Control {
    layout: ControlLayout,
    data: Vec<u8>,
}

impl Control {
    /// Keeps the block. v1.51's reader asks for [`CONTROL_LEN_V151`] bytes and takes however
    /// many the file holds, and this parser accepts any length up to that. A length other than
    /// the two known ones is [`ControlLayout::Other`] and a quirk in
    /// [`Slot::quirks`](super::Slot::quirks).
    ///
    /// # Errors
    ///
    /// [`Error::ControlLength`] when the block is longer than [`CONTROL_LEN_V151`].
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let layout = match data.len() {
            CONTROL_LEN_V10X => ControlLayout::V10x,
            CONTROL_LEN_V151 => ControlLayout::V151,
            len if len < CONTROL_LEN_V151 => ControlLayout::Other,
            len => return Err(Error::ControlLength(len)),
        };
        Ok(Self {
            layout,
            data: data.to_vec(),
        })
    }

    /// Which build's layout the block has.
    #[must_use]
    pub fn layout(&self) -> ControlLayout {
        self.layout
    }

    /// Bytes in the block.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the block is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// The game clock when the slot was saved, in simulation ticks; 0 when the block is too
    /// short to hold it.
    #[must_use]
    pub fn game_time(&self) -> u32 {
        Reader::new(&self.data).u32().unwrap_or_default()
    }

    pub(super) fn quirk(&self) -> Option<Quirk> {
        (self.layout == ControlLayout::Other).then_some(Quirk::ControlLength(self.data.len()))
    }

    /// The whole block, game clock included.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Control")
            .field("layout", &self.layout)
            .field("game_time", &self.game_time())
            .finish_non_exhaustive()
    }
}

/// One window record in a desktop. Records of closed windows hold stale or uninitialised bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Window {
    /// Non-zero when the window is open.
    pub open: u8,
    /// Unknown per-window byte; set on a few records.
    pub flags: u8,
    /// Left edge in screen pixels.
    pub x: i16,
    /// Top edge in screen pixels.
    pub y: i16,
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Unknown; uninitialised (filled with 205) in the shipped saves.
    pub extra: [u8; 8],
}

impl Window {
    /// Whether the window is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open != 0
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, Truncated> {
        Ok(Self {
            open: r.u8()?,
            flags: r.u8()?,
            x: r.i16()?,
            y: r.i16()?,
            width: r.u16()?,
            height: r.u16()?,
            extra: r.array()?,
        })
    }
}

/// One player view's window layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Desktop {
    header: [u16; 2],
    windows: Vec<Window>,
    trailer: [u32; 3],
}

impl Desktop {
    fn read(r: &mut Reader<'_>) -> Result<Self, Truncated> {
        let header = [r.u16()?, r.u16()?];
        let windows = (0..WINDOW_SLOTS)
            .map(|_| Window::read(r))
            .collect::<Result<_, _>>()?;
        let trailer = [r.u32()?, r.u32()?, r.u32()?];
        Ok(Self {
            header,
            windows,
            trailer,
        })
    }

    /// Two unknown words ahead of the window records.
    #[must_use]
    pub fn header(&self) -> [u16; 2] {
        self.header
    }

    /// All [`WINDOW_SLOTS`] window records, closed ones included.
    #[must_use]
    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// Three unknown words after the window records; all 1 in the shipped saves.
    #[must_use]
    pub fn trailer(&self) -> [u32; 3] {
        self.trailer
    }
}

/// `DESKTnnn.DAT`: one desktop per local player view, one in a normal game and two in
/// split-screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Desktops {
    desktops: Vec<Desktop>,
}

impl Desktops {
    /// Decodes one or two desktop records.
    ///
    /// # Errors
    ///
    /// [`Error::DesktopLength`] when the file is not [`DESKTOP_LEN`] or twice that.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let count = data.len() / DESKTOP_LEN;
        if !data.len().is_multiple_of(DESKTOP_LEN) || !(1..=MAX_DESKTOPS).contains(&count) {
            return Err(Error::DesktopLength(data.len()));
        }
        let mut r = Reader::new(data);
        let desktops = (0..count)
            .map(|_| Desktop::read(&mut r))
            .collect::<Result<_, _>>()?;
        Ok(Self { desktops })
    }

    /// The desktops in file order.
    #[must_use]
    pub fn desktops(&self) -> &[Desktop] {
        &self.desktops
    }
}

/// One message record, kept whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mail {
    /// The record's bytes.
    pub bytes: [u8; MAIL_LEN],
}

/// `MAILSnnn.DAT`: a counter, then each player's message list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailboxes {
    counter: u32,
    players: Vec<Option<Vec<Mail>>>,
}

impl Mailboxes {
    /// Decodes the counter and all [`MAX_PLAYERS`] mailboxes.
    ///
    /// # Errors
    ///
    /// [`Error::Truncated`] when a list runs past the end, [`Error::MailboxFlag`] on a presence
    /// byte other than 0 or 1, and [`Error::Trailing`] when bytes follow the last mailbox.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        let counter = r.u32()?;
        let players = (0..MAX_PLAYERS)
            .map(|player| read_mailbox(&mut r, player))
            .collect::<Result<_, _>>()?;
        expect_end(&r)?;
        Ok(Self { counter, players })
    }

    /// Unknown counter ahead of the mailboxes, perhaps the next message number.
    #[must_use]
    pub fn counter(&self) -> u32 {
        self.counter
    }

    /// Each player's messages, `None` for players without a mailbox.
    #[must_use]
    pub fn players(&self) -> &[Option<Vec<Mail>>] {
        &self.players
    }
}

fn read_mailbox(r: &mut Reader<'_>, player: usize) -> Result<Option<Vec<Mail>>, Error> {
    match r.u8()? {
        0 => Ok(None),
        1 => {
            let count = to_usize(r.u32()?);
            let offset = r.position();
            let len = count.checked_mul(MAIL_LEN).ok_or(Truncated {
                offset,
                needed: usize::MAX,
            })?;
            let records = r.bytes(len)?;
            let mails = records
                .as_chunks::<MAIL_LEN>()
                .0
                .iter()
                .map(|chunk| {
                    let mut bytes = [0; MAIL_LEN];
                    bytes.copy_from_slice(chunk);
                    Mail { bytes }
                })
                .collect();
            Ok(Some(mails))
        }
        flag => Err(Error::MailboxFlag { player, flag }),
    }
}

/// The computer players' shared map grid: one record per 8×8-node cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiGrid {
    columns: u32,
    rows: u32,
    params: [u32; 3],
    cells: Raw,
}

impl AiGrid {
    /// Cells across.
    #[must_use]
    pub fn columns(&self) -> u32 {
        self.columns
    }

    /// Cells down.
    #[must_use]
    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// The three header words after the cell count; the first is 64 in every save seen.
    #[must_use]
    pub fn params(&self) -> [u32; 3] {
        self.params
    }

    /// The [`AI_CELL_LEN`]-byte cell records, row by row, undecoded.
    #[must_use]
    pub fn cells(&self) -> &Raw {
        &self.cells
    }

    /// The grid size the game derives from a map of `width` by `height` nodes.
    #[must_use]
    pub fn size_for((width, height): (u32, u32)) -> (u32, u32) {
        (width / AI_CELL_NODES, height / AI_CELL_NODES)
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        let columns = r.u32()?;
        let rows = r.u32()?;
        let cells = r.u32()?;
        let params = [r.u32()?, r.u32()?, r.u32()?];
        if columns.checked_mul(rows) != Some(cells) {
            return Err(Error::AiGrid { columns, rows, cells });
        }
        let offset = r.position();
        let len = to_usize(cells).checked_mul(AI_CELL_LEN).ok_or(Truncated {
            offset,
            needed: usize::MAX,
        })?;
        let cells = Raw::new(offset, r.bytes(len)?);
        Ok(Self {
            columns,
            rows,
            params,
            cells,
        })
    }
}

/// `COMPYnnn.DAT`: the shared grid, then one variable-length record per computer player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputerPlayers {
    grid: AiGrid,
    players: Raw,
}

impl ComputerPlayers {
    /// Decodes the grid and keeps the player records as a raw block.
    ///
    /// # Errors
    ///
    /// [`Error::AiGrid`] when the header's cell count is inconsistent, or
    /// [`Error::Truncated`] when the grid runs past the end.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        let grid = AiGrid::read(&mut r)?;
        Ok(Self {
            grid,
            players: rest(&r),
        })
    }

    /// The shared grid.
    #[must_use]
    pub fn grid(&self) -> &AiGrid {
        &self.grid
    }

    /// The per-player records, undecoded.
    #[must_use]
    pub fn players(&self) -> &Raw {
        &self.players
    }
}

fn rest(r: &Reader<'_>) -> Raw {
    let offset = r.position();
    Raw::new(offset, r.data().get(offset..).unwrap_or_default())
}

pub(super) fn expect_end(r: &Reader<'_>) -> Result<(), Error> {
    match r.remaining() {
        0 => Ok(()),
        len => Err(Error::Trailing {
            offset: r.position(),
            len,
        }),
    }
}
