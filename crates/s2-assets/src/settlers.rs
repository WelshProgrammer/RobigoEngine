//! What settlers are drawn from: the carrier and job BOB files, the Roman animation bitmaps and
//! the animation script that runs them.

use s2_data::DataSet;
use s2_formats::bob::{Bob, BodyType, Direction, Link, RomAnim};

use crate::{Error, Result, Sprite, SpriteSheet};

/// Carriers with a ware.
pub const CARRIER_BOB: &str = "DATA/BOBS/CARRIER.BOB";

/// Every job, and carriers with nothing to carry.
pub const JOBS_BOB: &str = "DATA/BOBS/JOBS.BOB";

/// The bitmaps animation frames of kind 0 draw.
pub const ROM_BOBS: &str = "DATA/CBOB/ROM_BOBS.LST";

/// The animation script.
pub const ROM_ANIM: &str = "DATA/ANIMDAT/ROM_ANIM.DAT";

/// One figure as a BOB file draws it: a body, then an overlay `dy` rows lower. Both are placed
/// with their hotspot on the figure's anchor.
#[derive(Debug, Clone, Copy)]
pub struct BobParts<'a> {
    /// Drawn first.
    pub body: &'a Sprite,
    /// Head and load, drawn over the body.
    pub overlay: &'a Sprite,
    /// Rows the overlay moves down.
    pub dy: i16,
}

/// A BOB file's bodies and overlays as sprites, and the links that pair them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BobSprites {
    bodies: Box<[Sprite]>,
    overlays: Box<[Option<Sprite>]>,
    links: Box<[Link]>,
}

impl BobSprites {
    /// The sprites of every part of `bob`. Overlays no link names stay empty.
    #[must_use]
    pub fn from_bob(bob: &Bob) -> Self {
        let bodies = bob.bodies().iter().map(Sprite::from_player_bitmap).collect();
        let overlays = bob
            .overlays()
            .iter()
            .map(|overlay| overlay.bitmap().map(Sprite::from_player_bitmap))
            .collect();
        Self::new(bodies, overlays, bob.links().into())
    }

    /// The parts `bodies`, in [`Bob::body_index`] order, and `overlays` paired by `links`, in
    /// [`Bob::link_index`] order.
    #[must_use]
    pub const fn new(bodies: Box<[Sprite]>, overlays: Box<[Option<Sprite>]>, links: Box<[Link]>) -> Self {
        Self {
            bodies,
            overlays,
            links,
        }
    }

    /// The bodies, in [`Bob::body_index`] order.
    #[must_use]
    pub fn bodies(&self) -> &[Sprite] {
        &self.bodies
    }

    /// The overlays by number, empty for one no link names.
    #[must_use]
    pub fn overlays(&self) -> &[Option<Sprite>] {
        &self.overlays
    }

    /// The links, in [`Bob::link_index`] order.
    #[must_use]
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    /// Reads and decodes the BOB file at `path` in `data`.
    ///
    /// # Errors
    ///
    /// [`Error::Data`] if the file cannot be read, [`Error::Bob`] if it does not decode.
    pub fn load(data: &DataSet, path: &'static str) -> Result<Self> {
        let bytes = data.read(path)?;
        let bob = Bob::parse(&bytes).map_err(|source| Error::Bob { path, source })?;
        Ok(Self::from_bob(&bob))
    }

    /// The parts for one item, body type, direction and frame, or `None` when any of them is out
    /// of range.
    #[must_use]
    pub fn parts(
        &self,
        item: usize,
        body: BodyType,
        direction: Direction,
        frame: u8,
    ) -> Option<BobParts<'_>> {
        let link = *self.links.get(Bob::link_index(item, body, direction, frame)?)?;
        Some(BobParts {
            body: self.bodies.get(Bob::body_index(body, direction, frame)?)?,
            overlay: self.overlays.get(usize::from(link.overlay))?.as_ref()?,
            dy: link.dy,
        })
    }
}

/// Everything settlers are drawn from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlers {
    /// `CARRIER.BOB`.
    pub carrier: BobSprites,
    /// `JOBS.BOB`.
    pub jobs: BobSprites,
    /// `ROM_BOBS.LST`.
    pub roman: SpriteSheet,
    /// `ROM_ANIM.DAT`, which the simulation steps and the renderer draws from.
    pub script: RomAnim,
}

impl Settlers {
    /// Reads all four files from `data`.
    ///
    /// # Errors
    ///
    /// [`Error::Data`] if a file cannot be read, [`Error::Bob`] or [`Error::Archive`] if one does
    /// not decode.
    pub fn load(data: &DataSet) -> Result<Self> {
        Ok(Self {
            carrier: BobSprites::load(data, CARRIER_BOB)?,
            jobs: BobSprites::load(data, JOBS_BOB)?,
            roman: SpriteSheet::load(data, ROM_BOBS)?,
            script: Self::script(data)?,
        })
    }

    /// Reads and decodes `ROM_ANIM.DAT` from `data`.
    ///
    /// # Errors
    ///
    /// [`Error::Data`] if the file cannot be read, [`Error::Bob`] if it does not decode.
    pub fn script(data: &DataSet) -> Result<RomAnim> {
        RomAnim::parse(&data.read(ROM_ANIM)?).map_err(|source| Error::Bob {
            path: ROM_ANIM,
            source,
        })
    }
}
