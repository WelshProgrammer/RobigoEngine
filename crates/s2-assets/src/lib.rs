//! Turns `s2-formats` output into plain buffers and rectangles a renderer can use as they are.
//!
//! One [`LandscapeAssets`] holds everything a map view needs for one landscape: the frame
//! palette as the VGA shows it, the Gouraud shading table, the terrain texture sheet as palette
//! indices, the shadow [`Remap`] tables and a [`SpriteSheet`] of the map objects. Each
//! [`Nation`]'s buildings, flags and border stones load as their own sheet. [`PaletteCycle`]
//! animates the water and lava colours. Nothing here touches the GPU.

#![forbid(unsafe_code)]

mod error;
mod landscape;
mod load;
mod nation;
mod palette;
mod remap;
mod settlers;
mod sprite;

pub use error::{Error, Result};
pub use landscape::{Landscape, UnknownLandscape};
pub use load::{LandscapeAssets, LandscapeFiles, TEXTURE_SIDE, TEXTURE_SIZE};
pub use nation::{Nation, UnknownNation};
pub use palette::{CYCLE_PERIOD, CYCLE_TICKS, CYCLES, CycleRange, PaletteCycle, TICK_RATE, dac_palette};
pub use remap::{Remap, Tint};
pub use s2_formats::Palette;
pub use s2_formats::gouraud::ShadingTable;
pub use s2_formats::lst::Origin;
pub use settlers::{BobParts, BobSprites, CARRIER_BOB, JOBS_BOB, ROM_ANIM, ROM_BOBS, Settlers};
pub use sprite::{Ink, Run, SHIP_SPRITES, Sprite, SpriteSheet};
