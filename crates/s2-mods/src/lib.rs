//! The base mod baked from the player's own Settlers II data, and the mods layered over it
//! (`docs/design/mods.md`).
//!
//! [`bake()`] turns the original files into the base mod: files anyone can open, written to a folder
//! on the player's machine. [`ModStack`] opens the base mod and the player's mods in order, finds
//! each file in the last mod that has it, and merges each table row by row. Nothing baked ships
//! with Robigo; every player bakes from their own copy.

mod audio;
mod bake;
mod bobs;
mod cd;
mod effects;
mod error;
mod flac;
mod gold;
mod interface;
mod manifest;
mod maps;
mod missions;
mod music;
mod pictures;
mod rows;
mod sim;
mod soundfont;
mod sprites;
mod stack;
mod tables;
mod terrain;
mod texts;
mod video;
mod wav;

pub use audio::{AudioStream, Decoded, ENDINGS, LOOP_LENGTH, LOOP_START, decode};
pub use bake::{BAKER, BakeRecord, Progress, Quiet, bake, is_current, is_usable, step};
pub use bobs::{BOBS, bob_folder};
pub use cd::{CD, cd_path, cd_paths};
pub use effects::{EFFECT_SAMPLES, EFFECTS_TABLE, Sounds, effect_path, effects};
pub use error::{Error, Result};
pub use gold::{GOLD, GOLD_NAME, is_gold};
pub use interface::{FONTS, FontGlyphs, INTERFACE, font_folder, interface_folder};
pub use manifest::Manifest;
pub use maps::{MAPS, map_path};
pub use missions::{MISSIONS, mission_path};
pub use music::{Rendition, SONGS, SOUND_FONT, midi_current, render_midi, song_path, song_paths};
pub use pictures::{Backdrop, PICTURES, REMAP, REMAP_BAKED, picture_path};
pub use sim::{ANIMATIONS, SIM, tables_hash};
pub use soundfont::{Bank, GENERALUSER_GS, MUSESCORE_GENERAL, OFFERED, Offered};
pub use sprites::{Kind, PLAYER_RAMP, PLAYER_SHADES, Row, SPRITES, Sheets, sheet, sprite_folder};
pub use stack::{ModStack, Table};
pub use terrain::{TERRAIN, terrain_folder};
pub use texts::{ENGLISH, TEXTS, Text, text_path, text_path_in};
pub use video::{VIDEOS, video_path};
pub use wav::{read_wav, write_wav};

/// The base mod's id and its folder's name under the mods folder.
pub const BASE: &str = "base";

/// The name the base mod gives itself, which the player sees.
pub const BASE_NAME: &str = "Settlers 2";

/// The version of Robigo this is, which a mod's manifest can name: the release's tag when the build
/// sets `ROBIGO_VERSION`, and the workspace's version in any other build.
pub const ROBIGO_VERSION: &str = match option_env!("ROBIGO_VERSION") {
    Some(tagged) => tagged,
    None => env!("CARGO_PKG_VERSION"),
};

/// The file that names a mod and what it depends on.
pub const MANIFEST: &str = "mod.toml";
