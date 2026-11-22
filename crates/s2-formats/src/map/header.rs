//! The `.WLD` header between the signature and the area table.

use super::{Error, MAX_PLAYERS, Position, Quirk};
use crate::bytes::Reader;
use crate::text::cp437;

pub(super) const TITLE_LEN: usize = 20;
pub(super) const AUTHOR_LEN: usize = 20;
/// The title plus the width and height fields it can run into.
pub(super) const TITLE_SPAN: usize = TITLE_LEN + 4;
pub(super) const NO_HEADQUARTERS: u16 = u16::MAX;

/// The landscape graphics a map uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Terrain {
    /// Temperate grass and forest.
    Greenland,
    /// Lava and wasteland.
    Wasteland,
    /// Snow and ice.
    Winter,
    /// A set the game does not ship, kept as found.
    Other(u8),
}

impl Terrain {
    /// The header byte for this set.
    #[must_use]
    pub fn byte(self) -> u8 {
        match self {
            Self::Greenland => 0,
            Self::Wasteland => 1,
            Self::Winter => 2,
            Self::Other(byte) => byte,
        }
    }

    /// The set for a header byte.
    #[must_use]
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Greenland,
            1 => Self::Wasteland,
            2 => Self::Winter,
            other => Self::Other(other),
        }
    }
}

/// The descriptive part of a `.WLD` file.
///
/// The header's own width and height are unreliable. The editor writes the title as an
/// unbounded string, and titles of 20 characters or more spill into them.
/// [`Map::width`](super::Map::width) and [`Map::height`](super::Map::height) come from the
/// layers. [`Header::title_bytes`] follows the spill only when the stored dimensions show it
/// happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub(super) title: Vec<u8>,
    pub(super) title_span: [u8; TITLE_SPAN],
    pub(super) author: [u8; AUTHOR_LEN],
    pub(super) stored_width: u16,
    pub(super) stored_height: u16,
    pub(super) overwritten: bool,
    pub(super) terrain: Terrain,
    pub(super) player_count: u8,
    pub(super) headquarters: [Option<Position>; MAX_PLAYERS],
    pub(super) positions: [[u16; MAX_PLAYERS]; 2],
    pub(super) unknown_54: u8,
    pub(super) leaders: [u8; MAX_PLAYERS],
}

impl Header {
    /// Parses the 82 bytes after the signature, given the map size the layers agree on, adding
    /// anything unusual to `quirks`.
    pub(super) fn parse(raw: &[u8], width: u16, height: u16, quirks: &mut Vec<Quirk>) -> Result<Self, Error> {
        let span = crate::bytes::slice(raw, 0, TITLE_SPAN)?;
        let mut r = Reader::new(raw);
        r.skip(TITLE_LEN)?;
        let stored_width = r.u16()?;
        let stored_height = r.u16()?;
        let terrain = Terrain::from_byte(r.u8()?);
        if let Terrain::Other(byte) = terrain {
            quirks.push(Quirk::Terrain(byte));
        }
        let player_count = r.u8()?;
        let author = r.array()?;
        let xs: [u16; MAX_PLAYERS] = read_u16s(&mut r)?;
        let ys: [u16; MAX_PLAYERS] = read_u16s(&mut r)?;
        let unknown_54 = r.u8()?;
        if unknown_54 != 0 {
            quirks.push(Quirk::Unknown54(unknown_54));
        }
        let leaders = r.array()?;

        let overwritten = (stored_width, stored_height) != (width, height);
        let limit = if overwritten { TITLE_SPAN } else { TITLE_LEN };
        let title = until_nul(span.get(..limit).unwrap_or(span)).to_vec();
        let mut headquarters = [None; MAX_PLAYERS];
        for ((slot, x), y) in headquarters.iter_mut().zip(xs).zip(ys) {
            *slot = (x != NO_HEADQUARTERS && y != NO_HEADQUARTERS).then_some(Position { x, y });
        }
        let mut title_span = [0; TITLE_SPAN];
        title_span.copy_from_slice(span);
        Ok(Self {
            title,
            title_span,
            author,
            stored_width,
            stored_height,
            overwritten,
            terrain,
            player_count,
            headquarters,
            positions: [xs, ys],
            unknown_54,
            leaders,
        })
    }

    /// The title, decoded from CP437.
    #[must_use]
    pub fn title(&self) -> String {
        cp437::decode(&self.title)
    }

    /// The title's raw CP437 bytes, without the terminator.
    #[must_use]
    pub fn title_bytes(&self) -> &[u8] {
        &self.title
    }

    /// The author, decoded from CP437.
    #[must_use]
    pub fn author(&self) -> String {
        cp437::decode(until_nul(&self.author))
    }

    /// The author field's 20 raw bytes, including whatever follows the terminator.
    #[must_use]
    pub fn author_bytes(&self) -> &[u8; AUTHOR_LEN] {
        &self.author
    }

    /// Width and height as the header stores them, possibly overwritten by the title.
    #[must_use]
    pub fn stored_dimensions(&self) -> (u16, u16) {
        (self.stored_width, self.stored_height)
    }

    /// Whether the stored dimensions disagree with the layers, a sign that the title ran into
    /// them.
    #[must_use]
    pub fn dimensions_overwritten(&self) -> bool {
        self.overwritten
    }

    /// Greenland, wasteland or winter, or the raw byte for a set the game does not ship.
    #[must_use]
    pub fn terrain(&self) -> Terrain {
        self.terrain
    }

    /// How many players the map is for.
    #[must_use]
    pub fn player_count(&self) -> u8 {
        self.player_count
    }

    /// Headquarters position per player slot; `None` where the file stores 65535.
    #[must_use]
    pub fn headquarters(&self) -> &[Option<Position>; MAX_PLAYERS] {
        &self.headquarters
    }

    /// The byte at file offset 84, zero in every shipped map.
    ///
    /// Other map tools treat a non-zero value as marking the map unusable and note that some
    /// editors leave it set anyway. Whether S2.EXE reads it is not yet checked, and the name
    /// stays neutral until it is. A non-zero value is reported as [`Quirk::Unknown54`].
    #[must_use]
    pub fn unknown_54(&self) -> u8 {
        self.unknown_54
    }

    /// A byte per player slot at file offset 85: the face each player takes.
    ///
    /// S2.EXE copies these into the player slots only when its game mode word (`0x1f78ea`) is 2,
    /// which nothing in S2 sets, and uses the slot number otherwise. Shipped maps hold 0 for
    /// slot 0 and distinct faces for the computer players, the African map's being the Africans'
    /// three.
    #[must_use]
    pub fn leaders(&self) -> &[u8; MAX_PLAYERS] {
        &self.leaders
    }
}

fn read_u16s<const N: usize>(r: &mut Reader<'_>) -> Result<[u16; N], Error> {
    let mut out = [0; N];
    for value in &mut out {
        *value = r.u16()?;
    }
    Ok(out)
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap_or(bytes)
}
