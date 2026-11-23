//! The tables the sim plays by, and the animation script, as TOML files under `sim/`. Rows and
//! keys name building types, wares, jobs, transport groups and nations by their names in
//! `docs/design/mods.md`.
//!
//! | File | Holds |
//! |---|---|
//! | `sim/buildings.toml` | a row a building type |
//! | `sim/economy.toml` | `construction`, `type_weights` by building type, `tools` by ware, `transport` by group, the `distribution` pages |
//! | `sim/military.toml` | each default military setting |
//! | `sim/cargo.toml` | the ships' weights, `wares` by ware and `people` by job |
//! | `sim/missions.toml` | a row a mission number with rules besides its script |
//! | `sim/waypoints/<nation>.toml` | a row a building type |
//! | `sim/fights/<nation>/<side>.toml` | a row a rank |
//! | `sim/fights/deaths.toml` | the death animation of each side |
//! | `sim/animations.toml` | a row a record of `ROM_ANIM.DAT`, by number from 0 |

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use s2_assets::{Nation, ROM_ANIM, Settlers};
use s2_data::DataSet;
use s2_formats::bob::RomAnim;
use s2_sim::replay::content_hash;
use s2_sim::{
    BUILDING_TYPES, BuildingKind, BuildingType, BuildingTypes, CargoWeights, Category, ComputerBan, Consumer,
    DistributionGroup, EconomyTables, FightAnimations, FighterAnimations, Job, MILITARY_SETTINGS,
    MilitaryTables, MissionTables, SimTables, Size, TRANSPORT_GROUP_NAMES, TRANSPORT_GROUPS, WARE_TYPES,
    WAYPOINT_SETS, WareKind, WaypointSet, Waypoints,
};

use crate::rows::{
    by_name, integer, lookup, merged, name_of, named_values, numbered, rows, value, whole, write,
};
use crate::sprites::{Sheets, row_error};
use crate::{ModStack, Result};

/// Where the sim's tables go in a mod.
pub const SIM: &str = "sim";

/// The animation script's file in a mod.
pub const ANIMATIONS: &str = "sim/animations.toml";

/// The table files.
const BUILDINGS: &str = "sim/buildings.toml";
const ECONOMY: &str = "sim/economy.toml";
const MILITARY: &str = "sim/military.toml";
const CARGO: &str = "sim/cargo.toml";
const MISSIONS: &str = "sim/missions.toml";
const DEATHS: &str = "sim/fights/deaths.toml";

/// The sides of a fight: the soldier that walked up, then the one that waited.
const SIDES: [&str; 2] = ["walked_up", "waited"];

/// The military settings' names, in setting order.
const MILITARY_NAMES: [&str; MILITARY_SETTINGS] = [
    "recruiting",
    "rank",
    "defence",
    "attack",
    "occupation_centre",
    "occupation_middle",
    "occupation_border",
];

/// The building categories' names.
const CATEGORIES: [(&str, Category); 3] = [
    ("workplace", Category::Workplace),
    ("storehouse", Category::Storehouse),
    ("military", Category::Military),
];

/// The building sizes' names.
const SIZES: [(&str, Size); 4] = [
    ("small", Size::Small),
    ("medium", Size::Medium),
    ("mine", Size::Mine),
    ("large", Size::Large),
];

/// The consumer a distribution page names for building sites.
const CONSTRUCTION: &str = "construction";

/// The most inputs a building type takes.
const INPUTS: usize = 4;

/// Ranks in the fight table, private first.
const RANKS: usize = Job::RANKS.len();

/// One building type's row.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildingRow {
    category: String,
    group: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worker: Option<String>,
    boards: u8,
    stones: u8,
    size: String,
    levelled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    duration: u8,
    rated: bool,
    stoppable: bool,
    sight: u8,
    strength: u8,
    soldiers: u8,
    #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
    inputs: toml::Table,
}

/// The economy file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EconomyFile {
    construction: u8,
    type_weights: toml::Table,
    tools: toml::Table,
    transport: toml::Table,
    distribution: Vec<PageRow>,
}

/// One page of the distribution window.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PageRow {
    wares: Vec<String>,
    consumers: Vec<String>,
}

/// The cargo file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoFile {
    wares: toml::Table,
    people: toml::Table,
}

/// One mission's rules besides its script.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissionRow {
    #[serde(default)]
    computers_never_build: Vec<String>,
}

/// One building type's waypoints.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaypointRow {
    first: u8,
    x: [i8; s2_sim::WAYPOINTS],
    y: [i8; s2_sim::WAYPOINTS],
}

/// One rank's fight animations on one side.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FighterRow {
    attack: u16,
    hit_offset: u16,
    block: u16,
    dodge: [u16; 2],
    hit: u16,
    ready: u16,
}

/// One record of the animation script: kind, argument, column and row offsets, operation.
type RecordRow = (u8, u16, i8, i8, u8);

/// Where a nation's waypoints go.
fn waypoints_path(nation: Nation) -> String {
    format!("{SIM}/waypoints/{nation}.toml")
}

/// Where a nation's fight animations for side `side` go.
fn fights_path(nation: Nation, side: &str) -> String {
    format!("{SIM}/fights/{nation}/{side}.toml")
}

impl Sheets<'_> {
    /// The tables the sim plays by.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when the stack lacks a table, [`Error::Row`] for a row that does
    /// not decode or names something the game does not have, and the stack's errors for a
    /// table that does not load.
    pub fn sim_tables(&self) -> Result<SimTables> {
        self.stack.map_or(Ok(SimTables::ORIGINAL), tables)
    }

    /// The animation script's bytes, as `ROM_ANIM.DAT` holds them.
    ///
    /// # Errors
    ///
    /// [`Error::NotInMods`] when the stack lacks the script, [`Error::Row`] for a record that
    /// does not decode or a gap in the numbering, and [`Error::Data`] for an original that
    /// cannot be read.
    pub fn animation_script(&self) -> Result<Vec<u8>> {
        let Some(stack) = self.stack else {
            return Ok(self.data.read(ROM_ANIM)?);
        };
        script_bytes(rows::<RecordRow>(stack, ANIMATIONS)?)
    }
}

/// The tables the stack holds.
fn tables(stack: &ModStack) -> Result<SimTables> {
    Ok(SimTables {
        building_types: buildings(stack)?,
        economy: economy(stack)?,
        military: military(stack)?,
        waypoints: waypoints(stack)?,
        cargo: cargo(stack)?,
        fights: fights(stack)?,
        missions: missions(stack)?,
    })
}

/// A hash of `tables`, equal for equal tables whatever files they came from.
///
/// # Errors
///
/// [`Error::Write`] for a table that does not write as TOML.
pub fn tables_hash(tables: &SimTables) -> Result<u64> {
    let mut all = Vec::new();
    for (path, text) in table_files(tables)? {
        all.extend_from_slice(path.as_bytes());
        all.push(b'\n');
        all.extend_from_slice(text.as_bytes());
    }
    Ok(content_hash(&all))
}

/// The files `tables` bake to, by path inside the mod.
pub(crate) fn table_files(tables: &SimTables) -> Result<Vec<(String, String)>> {
    let mut files = vec![
        (
            BUILDINGS.to_owned(),
            write(BUILDINGS, &buildings_table(&tables.building_types)?)?,
        ),
        (
            ECONOMY.to_owned(),
            write(ECONOMY, &economy_file(&tables.economy))?,
        ),
        (
            MILITARY.to_owned(),
            write(MILITARY, &military_table(&tables.military))?,
        ),
        (CARGO.to_owned(), write(CARGO, &cargo_file(&tables.cargo))?),
        (
            MISSIONS.to_owned(),
            write(MISSIONS, &missions_table(&tables.missions)?)?,
        ),
    ];
    for (nation, set) in Nation::ALL.into_iter().zip(&tables.waypoints) {
        let path = waypoints_path(nation);
        let text = write(&path, &waypoints_table(&path, set)?)?;
        files.push((path, text));
    }
    for (index, nation) in (0..).zip(Nation::ALL) {
        for (side, name) in SIDES.into_iter().enumerate() {
            let path = fights_path(nation, name);
            let text = write(&path, &fighters_table(&path, &tables.fights, index, side)?)?;
            files.push((path, text));
        }
    }
    let deaths: toml::Table = SIDES
        .into_iter()
        .enumerate()
        .map(|(side, name)| (name.to_owned(), integer(tables.fights.death(side))))
        .collect();
    files.push((DEATHS.to_owned(), write(DEATHS, &deaths)?));
    Ok(files)
}

/// The animation script in `data` as the file it bakes to.
pub(crate) fn bake_script(data: &DataSet) -> Result<(String, Vec<u8>)> {
    Ok((
        ANIMATIONS.to_owned(),
        script_file(&Settlers::script(data)?)?.into_bytes(),
    ))
}

/// `script` as the file it bakes to.
fn script_file(script: &RomAnim) -> Result<String> {
    let rows: toml::Table = script
        .records()
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let row = [
                integer(record.kind),
                integer(record.arg),
                integer(record.dx),
                integer(record.dy),
                integer(record.op),
            ];
            (index.to_string(), toml::Value::Array(row.into()))
        })
        .collect();
    write(ANIMATIONS, &rows)
}

/// The ware named `name`, for row `key` of the file at `path`.
fn ware(path: &str, key: &str, name: &str) -> Result<WareKind> {
    WareKind::from_name(name).ok_or_else(|| row_error(path, key, format!("no ware {name}")))
}

/// The building type named `name`, for row `key` of the file at `path`.
fn building_kind(path: &str, key: &str, name: &str) -> Result<BuildingKind> {
    BuildingKind::from_name(name).ok_or_else(|| row_error(path, key, format!("no building type {name}")))
}

/// The names of ware types and jobs, as owned strings.
fn owned(name: Option<&str>) -> Option<String> {
    name.map(str::to_owned)
}

fn buildings_table(types: &BuildingTypes) -> Result<toml::Table> {
    types
        .types()
        .iter()
        .zip(BuildingKind::NAMES)
        .map(|(row, name)| {
            let inputs = row
                .inputs
                .iter()
                .zip(row.capacity)
                .filter_map(|(input, stock)| {
                    Some((input.and_then(WareKind::name)?.to_owned(), integer(stock)))
                })
                .collect();
            let row = BuildingRow {
                category: name_of(&CATEGORIES, &row.category).into(),
                group: row.group,
                worker: owned(row.worker.and_then(Job::name)),
                boards: row.boards,
                stones: row.stones,
                size: name_of(&SIZES, &row.size).into(),
                levelled: row.levelled,
                output: owned(row.output.and_then(WareKind::name)),
                duration: row.duration,
                rated: row.rated,
                stoppable: row.stoppable,
                sight: row.sight,
                strength: row.strength,
                soldiers: row.soldiers,
                inputs,
            };
            Ok((name.to_owned(), value(BUILDINGS, row)?))
        })
        .collect()
}

fn buildings(stack: &ModStack) -> Result<BuildingTypes> {
    let rows = by_name(
        BUILDINGS,
        "",
        &BuildingKind::NAMES,
        rows::<BuildingRow>(stack, BUILDINGS)?,
    )?;
    let types = rows
        .into_iter()
        .zip(BuildingKind::NAMES)
        .map(|(row, key)| building(key, row))
        .collect::<Result<Vec<_>>>()?;
    BuildingTypes::new(types).map_err(|error| row_error(BUILDINGS, "", error.to_string()))
}

/// Building type row `key` decoded.
fn building(key: &str, row: BuildingRow) -> Result<BuildingType> {
    let path = BUILDINGS;
    let category = lookup(&CATEGORIES, &row.category)
        .ok_or_else(|| row_error(path, key, format!("no category {}", row.category)))?;
    let size =
        lookup(&SIZES, &row.size).ok_or_else(|| row_error(path, key, format!("no size {}", row.size)))?;
    let worker = row
        .worker
        .map(|name| Job::from_name(&name).ok_or_else(|| row_error(path, key, format!("no job {name}"))))
        .transpose()?;
    let output = row.output.map(|name| ware(path, key, &name)).transpose()?;
    if row.inputs.len() > INPUTS {
        return Err(row_error(path, key, format!("more than {INPUTS} inputs")));
    }
    let mut inputs = [None; INPUTS];
    let mut capacity = [0; INPUTS];
    for (slot, (name, stock)) in row.inputs.into_iter().enumerate() {
        inputs[slot] = Some(ware(path, key, &name)?);
        capacity[slot] = stock
            .as_integer()
            .and_then(|stock| u8::try_from(stock).ok())
            .ok_or_else(|| row_error(path, key, format!("{name} stocks 0 to 255")))?;
    }
    Ok(BuildingType {
        category,
        group: row.group,
        worker,
        boards: row.boards,
        stones: row.stones,
        size,
        levelled: row.levelled,
        output,
        duration: row.duration,
        inputs,
        capacity,
        rated: row.rated,
        stoppable: row.stoppable,
        sight: row.sight,
        strength: row.strength,
        soldiers: row.soldiers,
    })
}

/// The tool names, tongs to bow.
fn tool_names() -> &'static [&'static str] {
    let first = WareKind::FIRST_TOOL.index();
    &WareKind::NAMES[first..first + WareKind::TOOLS]
}

fn economy_file(tables: &EconomyTables) -> EconomyFile {
    let consumer = |consumer: &Consumer| match consumer {
        Consumer::Building(kind) => kind.name().unwrap_or_default().to_owned(),
        Consumer::Construction => CONSTRUCTION.to_owned(),
    };
    EconomyFile {
        construction: tables.construction(),
        type_weights: BuildingKind::NAMES
            .iter()
            .zip(0..)
            .map(|(&name, kind)| (name.to_owned(), integer(tables.type_weight(BuildingKind(kind)))))
            .collect(),
        tools: tool_names()
            .iter()
            .zip(tables.tools())
            .map(|(&name, &weight)| (name.to_owned(), integer(weight)))
            .collect(),
        transport: TRANSPORT_GROUP_NAMES
            .iter()
            .zip(tables.ranks())
            .map(|(&name, &place)| (name.to_owned(), integer(place)))
            .collect(),
        distribution: tables
            .groups()
            .iter()
            .map(|group| PageRow {
                wares: group.wares.iter().filter_map(|ware| owned(ware.name())).collect(),
                consumers: group.consumers.iter().map(consumer).collect(),
            })
            .collect(),
    }
}

fn economy(stack: &ModStack) -> Result<EconomyTables> {
    let path = ECONOMY;
    let file: EconomyFile = whole(stack, path)?;
    let type_weights: [u8; BUILDING_TYPES] =
        named_values(path, "type_weights", &BuildingKind::NAMES, file.type_weights)?;
    let tools: [u8; WareKind::TOOLS] = named_values(path, "tools", tool_names(), file.tools)?;
    let ranks: [u8; TRANSPORT_GROUPS] =
        named_values(path, "transport", &TRANSPORT_GROUP_NAMES, file.transport)?;
    let groups = file
        .distribution
        .into_iter()
        .enumerate()
        .map(|(page, row)| {
            let key = format!("distribution.{page}");
            let wares = row
                .wares
                .iter()
                .map(|name| ware(path, &key, name))
                .collect::<Result<Vec<_>>>()?;
            let consumers = row
                .consumers
                .iter()
                .map(|name| {
                    if name == CONSTRUCTION {
                        Ok(Consumer::Construction)
                    } else {
                        building_kind(path, &key, name).map(Consumer::Building)
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(DistributionGroup {
                wares: wares.into(),
                consumers: consumers.into(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    EconomyTables::new(type_weights.to_vec(), file.construction, tools, groups, ranks)
        .map_err(|error| row_error(path, "", error.to_string()))
}

fn military_table(tables: &MilitaryTables) -> toml::Table {
    MILITARY_NAMES
        .iter()
        .zip(tables.settings())
        .map(|(&name, value)| (name.to_owned(), integer(value)))
        .collect()
}

fn military(stack: &ModStack) -> Result<MilitaryTables> {
    let settings = named_values(MILITARY, "", &MILITARY_NAMES, merged(stack, MILITARY)?)?;
    MilitaryTables::new(settings).map_err(|error| row_error(MILITARY, "", error.to_string()))
}

fn cargo_file(weights: &CargoWeights) -> CargoFile {
    let table = |names: &[&str], weights: &[u8]| -> toml::Table {
        names
            .iter()
            .zip(weights)
            .map(|(&name, &weight)| (name.to_owned(), integer(weight)))
            .collect()
    };
    CargoFile {
        wares: table(&WareKind::NAMES, weights.wares()),
        people: table(&Job::NAMES, weights.people()),
    }
}

fn cargo(stack: &ModStack) -> Result<CargoWeights> {
    let file: CargoFile = whole(stack, CARGO)?;
    let wares: [u8; WARE_TYPES] = named_values(CARGO, "wares", &WareKind::NAMES, file.wares)?;
    let people: [u8; Job::PEOPLE] = named_values(CARGO, "people", &Job::NAMES, file.people)?;
    Ok(CargoWeights::new(wares, people))
}

fn missions_table(tables: &MissionTables) -> Result<toml::Table> {
    let mut rows: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    for ban in tables.computer_bans() {
        rows.entry(ban.mission)
            .or_default()
            .extend(owned(ban.kind.name()));
    }
    rows.into_iter()
        .map(|(mission, computers_never_build)| {
            let row = MissionRow {
                computers_never_build,
            };
            Ok((mission.to_string(), value(MISSIONS, row)?))
        })
        .collect()
}

fn missions(stack: &ModStack) -> Result<MissionTables> {
    let mut bans = Vec::new();
    for (key, row) in rows::<MissionRow>(stack, MISSIONS)? {
        let mission = key
            .parse()
            .map_err(|_| row_error(MISSIONS, &key, "not a mission number"))?;
        for name in &row.computers_never_build {
            bans.push(ComputerBan {
                mission,
                kind: building_kind(MISSIONS, &key, name)?,
            });
        }
    }
    Ok(MissionTables::new(bans))
}

fn waypoints_table(path: &str, set: &WaypointSet) -> Result<toml::Table> {
    BuildingKind::NAMES
        .iter()
        .zip(0..)
        .filter_map(|(&name, kind)| Some((name, set.get(BuildingKind(kind))?)))
        .map(|(name, row)| {
            let row = WaypointRow {
                first: row.first,
                x: row.x,
                y: row.y,
            };
            Ok((name.to_owned(), value(path, row)?))
        })
        .collect()
}

fn waypoints(stack: &ModStack) -> Result<[WaypointSet; WAYPOINT_SETS]> {
    let sets = Nation::ALL
        .into_iter()
        .map(|nation| {
            let path = waypoints_path(nation);
            let rows = by_name(
                &path,
                "",
                &BuildingKind::NAMES,
                rows::<WaypointRow>(stack, &path)?,
            )?;
            let rows = rows
                .into_iter()
                .map(|row| Waypoints {
                    first: row.first,
                    x: row.x,
                    y: row.y,
                })
                .collect();
            WaypointSet::new(rows).map_err(|error| row_error(&path, "", error.to_string()))
        })
        .collect::<Result<Vec<_>>>()?;
    sets.try_into()
        .map_err(|_| row_error(SIM, "waypoints", format!("{WAYPOINT_SETS} sets")))
}

/// The rank names, private first.
fn rank_names() -> [&'static str; RANKS] {
    Job::RANKS.map(|rank| rank.name().unwrap_or_default())
}

fn fighters_table(path: &str, fights: &FightAnimations, nation: u8, side: usize) -> Result<toml::Table> {
    rank_names()
        .into_iter()
        .enumerate()
        .map(|(rank, name)| {
            let fighter = fights.fighter(nation, rank, side);
            let row = FighterRow {
                attack: fighter.attack,
                hit_offset: fighter.hit_offset,
                block: fighter.block,
                dodge: fighter.dodge,
                hit: fighter.hit,
                ready: fighter.ready,
            };
            Ok((name.to_owned(), value(path, row)?))
        })
        .collect()
}

fn fights(stack: &ModStack) -> Result<FightAnimations> {
    let blank = FighterAnimations {
        attack: 0,
        hit_offset: 0,
        block: 0,
        dodge: [0; 2],
        hit: 0,
        ready: 0,
    };
    let mut table = [[[blank; 2]; RANKS]; Nation::ALL.len()];
    for (nation, ranks) in Nation::ALL.into_iter().zip(&mut table) {
        for (side, name) in SIDES.into_iter().enumerate() {
            let path = fights_path(nation, name);
            let rows = by_name(&path, "", &rank_names(), rows::<FighterRow>(stack, &path)?)?;
            for (rank, row) in ranks.iter_mut().zip(rows) {
                rank[side] = FighterAnimations {
                    attack: row.attack,
                    hit_offset: row.hit_offset,
                    block: row.block,
                    dodge: row.dodge,
                    hit: row.hit,
                    ready: row.ready,
                };
            }
        }
    }
    let death: [u16; 2] = named_values(DEATHS, "", &SIDES, merged(stack, DEATHS)?)?;
    FightAnimations::new(table, death).map_err(|error| row_error(SIM, "fights", error.to_string()))
}

/// The bytes of `ROM_ANIM.DAT` holding `rows`, numbered from 0 with none missing.
fn script_bytes(rows: Vec<(String, RecordRow)>) -> Result<Vec<u8>> {
    let records = numbered(ANIMATIONS, rows)?;
    let count = u16::try_from(records.len())
        .map_err(|_| row_error(ANIMATIONS, "", format!("more than {} records", u16::MAX)))?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&RomAnim::TAG.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    for (kind, arg, dx, dy, op) in records {
        let [low, high] = arg.to_le_bytes();
        bytes.extend_from_slice(&[kind, low, high, dx.cast_unsigned(), dy.cast_unsigned(), op]);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::rows::testing::stack;

    fn original() -> Vec<(String, String)> {
        table_files(&SimTables::ORIGINAL).unwrap()
    }

    #[test]
    fn every_name_reads_back() {
        for (index, name) in BuildingKind::NAMES.into_iter().enumerate() {
            assert_eq!(
                BuildingKind::from_name(name).map(BuildingKind::index),
                Some(index)
            );
        }
        for (index, name) in WareKind::NAMES.into_iter().enumerate() {
            assert_eq!(WareKind::from_name(name).map(WareKind::index), Some(index));
        }
        for (index, name) in Job::NAMES.into_iter().enumerate() {
            assert_eq!(Job::from_name(name).map(Job::index), Some(index));
        }
        assert_eq!(
            rank_names(),
            ["private", "private_first_class", "sergeant", "officer", "general"]
        );
    }

    #[test]
    fn the_original_tables_come_back_from_their_files() {
        let (root, stack) = stack("original", &original(), None);
        let tables = tables(&stack).unwrap();
        assert_eq!(tables, SimTables::ORIGINAL);
        assert_eq!(
            tables_hash(&tables).unwrap(),
            tables_hash(&SimTables::ORIGINAL).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_mod_changes_one_field_and_the_hash() {
        let (root, stack) = stack(
            "one-field",
            &original(),
            Some((BUILDINGS, "[storehouse]\nboards = 9\n")),
        );
        let tables = tables(&stack).unwrap();
        let storehouse = tables.building_types.get(BuildingKind::STOREHOUSE).unwrap();
        let before = BuildingTypes::ORIGINAL.get(BuildingKind::STOREHOUSE).unwrap();
        assert_eq!(storehouse.boards, 9);
        assert_eq!(storehouse.stones, before.stones);
        assert_ne!(
            tables_hash(&tables).unwrap(),
            tables_hash(&SimTables::ORIGINAL).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_mod_lifts_the_catapult_ban_and_bans_fortresses_elsewhere() {
        let (root, stack) = stack(
            "missions",
            &original(),
            Some((
                MISSIONS,
                "[1]\ncomputers_never_build = []\n\n[4]\ncomputers_never_build = [\"fortress\"]\n",
            )),
        );
        let tables = tables(&stack).unwrap();
        assert_eq!(
            tables.missions.computer_bans(),
            [ComputerBan {
                mission: 4,
                kind: BuildingKind::FORTRESS,
            }]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_names_and_fields_are_refused() {
        let refused = |name: &str, path: &str, text: &str| {
            let (root, stack) = stack(name, &original(), Some((path, text)));
            let error = tables(&stack).unwrap_err();
            std::fs::remove_dir_all(root).unwrap();
            error
        };
        assert!(matches!(
            refused("castle", BUILDINGS, "[castle]\nboards = 1\n"),
            Error::Row { row, .. } if row == "castle"
        ));
        assert!(matches!(
            refused("typo", BUILDINGS, "[storehouse]\nbords = 1\n"),
            Error::Row { row, .. } if row == "storehouse"
        ));
        assert!(matches!(
            refused("ware", BUILDINGS, "[mill.inputs]\ncake = 6\n"),
            Error::Row { row, .. } if row == "mill"
        ));
        assert!(matches!(
            refused("tool", ECONOMY, "[tools]\nspoon = 10\n"),
            Error::Row { row, .. } if row == "tools.spoon"
        ));
        assert!(matches!(
            refused("setting", MILITARY, "attack = 101\n"),
            Error::Row { .. }
        ));
        assert!(matches!(
            refused("mission", MISSIONS, "[first]\ncomputers_never_build = []\n"),
            Error::Row { row, .. } if row == "first"
        ));
        assert!(matches!(
            refused("catapults", MISSIONS, "[1]\ncomputers_never_build = [\"trebuchet\"]\n"),
            Error::Row { row, .. } if row == "1"
        ));
    }

    #[test]
    fn the_script_comes_back_byte_for_byte() {
        let mut file = RomAnim::TAG.to_le_bytes().to_vec();
        file.extend_from_slice(&3_u16.to_le_bytes());
        file.extend_from_slice(&[
            1,
            2,
            3,
            4,
            5,
            u8::MAX,
            0,
            9,
            0,
            0,
            0,
            4,
            u8::MAX,
            u8::MAX,
            u8::MAX,
            u8::MAX,
            u8::MAX,
            u8::MAX,
        ]);
        let text = script_file(&RomAnim::parse(&file).unwrap()).unwrap();
        let (root, stack) = stack("script", &[(ANIMATIONS.to_owned(), text)], None);
        assert_eq!(
            script_bytes(rows::<RecordRow>(&stack, ANIMATIONS).unwrap()).unwrap(),
            file
        );
        std::fs::remove_dir_all(root).unwrap();
        let gap = [
            ("0".to_owned(), (0, 0, 0, 0, 0)),
            ("2".to_owned(), (0, 0, 0, 0, 0)),
        ];
        assert!(script_bytes(gap.into()).is_err());
    }
}
