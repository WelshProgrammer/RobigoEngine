//! The parts of the game world that other crates need without the sim: the node grid, players,
//! the typed ids for buildings, wares and jobs, and the data tables a mod replaces.
//!
//! `s2-sim` re-exports all of it, so a caller that wants the game world still needs only that
//! crate. The renderer, the interface and the netplay code depend on this one for the pieces
//! they use on their own.
//!
//! Everything here sits in the deterministic path and carries the same `clippy.toml` as
//! `s2-sim`: no floats, no hash-ordered collections, no wall-clock time.

#![deny(clippy::float_arithmetic, clippy::disallowed_types)]

pub mod building_types;
pub mod economy;
pub mod grid;
pub mod job;
pub mod military;
pub mod player;
pub mod ware;
pub mod waypoints;
