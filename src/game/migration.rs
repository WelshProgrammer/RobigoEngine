//! The list a load shows of what it took out of a save made by other rules.

use std::collections::BTreeMap;
use std::fmt::Write;

use s2_sim::{BuildingKind, Repair, Repaired, WareKind};
use s2_ui::windows::BUILDING_NAMES;

/// Something the list names from the game's names table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Named {
    /// A building type.
    Building(BuildingKind),
    /// A ware.
    Ware(WareKind),
}

impl Named {
    /// Its slot in the names table.
    pub(crate) fn slot(self) -> u16 {
        match self {
            Self::Building(kind) => BUILDING_NAMES + u16::from(kind.0),
            Self::Ware(ware) => u16::from(ware.0),
        }
    }
}

/// What a building with no name in the table goes by.
const UNKNOWN: &str = "Unknown building";

/// The list for `repaired`, building by building in the order the repair found them, with
/// `name` giving the game's name for a building type or a ware. Buildings of a type are
/// numbered from 1 as the list reaches them.
pub(crate) fn log(repaired: &[Repaired], name: impl Fn(Named) -> String) -> String {
    let mut text = String::from("This game was saved under other rules. Some repairs have been applied:\r\n");
    let mut counts: BTreeMap<u8, u32> = BTreeMap::new();
    for building in repaired.chunk_by(|one, next| one.node == next.node && one.kind == next.kind) {
        let Some(first) = building.first() else {
            continue;
        };
        let title = name(Named::Building(first.kind));
        let site = if first.site { " (construction site)" } else { "" };
        if title.is_empty() {
            let _ = write!(text, "\r\n{UNKNOWN}{site}:\r\n");
        } else {
            let count = counts.entry(first.kind.0).or_default();
            *count += 1;
            let _ = write!(text, "\r\n{title} {count}{site}:\r\n");
        }
        for found in building {
            let _ = write!(text, "- {}\r\n", line(found.what, &name));
        }
    }
    text
}

/// What the list says for `what`.
fn line(what: Repair, name: &impl Fn(Named) -> String) -> String {
    match what {
        Repair::Removed => "Removed".to_owned(),
        Repair::Worker => "Worker replaced".to_owned(),
        Repair::Restarted => "Production restarted".to_owned(),
        Repair::WrongStock {
            ware: Some(ware),
            lost,
        } => {
            format!(
                "Wrong stock: {lost} {} removed",
                name(Named::Ware(ware)).to_lowercase()
            )
        }
        Repair::WrongStock { ware: None, lost } => format!("Wrong stock: {lost} removed"),
        Repair::Overstock { ware, kept } => {
            format!(
                "Overstock: {}, reduced to {kept}",
                name(Named::Ware(ware)).to_lowercase()
            )
        }
        Repair::Orders { cancelled } => format!("Orders cancelled: {cancelled}"),
        Repair::Soldiers { left } => format!("Soldiers sent home: {left}"),
        Repair::Materials { lost } => format!("Materials removed: {lost}"),
    }
}

#[cfg(test)]
mod tests {
    use s2_sim::{Node, Player};

    use super::*;

    fn name(named: Named) -> String {
        match named {
            Named::Building(BuildingKind::SAWMILL) => "Sawmill".to_owned(),
            Named::Building(BuildingKind::BARRACKS) => "Barracks".to_owned(),
            Named::Ware(WareKind::WOOD) => "Wood".to_owned(),
            _ => String::new(),
        }
    }

    fn found(kind: BuildingKind, x: u16, site: bool, what: Repair) -> Repaired {
        Repaired {
            kind,
            node: Node { x, y: 0 },
            owner: Player::new(0).unwrap(),
            site,
            what,
        }
    }

    #[test]
    fn each_building_gets_a_heading_and_a_line_for_each_thing_taken() {
        let ware = WareKind::WOOD;
        let repaired = [
            found(BuildingKind::SAWMILL, 1, false, Repair::Worker),
            found(
                BuildingKind::SAWMILL,
                1,
                false,
                Repair::Overstock { ware, kept: 2 },
            ),
            found(BuildingKind::BARRACKS, 2, false, Repair::Soldiers { left: 1 }),
            found(BuildingKind::SAWMILL, 3, true, Repair::Materials { lost: 3 }),
            found(BuildingKind::SAWMILL, 3, true, Repair::Orders { cancelled: 1 }),
            found(BuildingKind::MILL, 4, false, Repair::Removed),
            found(
                BuildingKind::SAWMILL,
                5,
                false,
                Repair::WrongStock {
                    ware: Some(ware),
                    lost: 4,
                },
            ),
        ];
        let text = log(&repaired, name);
        let lines: Vec<&str> = text.split("\r\n").skip(1).collect();
        assert_eq!(
            lines,
            [
                "",
                "Sawmill 1:",
                "- Worker replaced",
                "- Overstock: wood, reduced to 2",
                "",
                "Barracks 1:",
                "- Soldiers sent home: 1",
                "",
                "Sawmill 2 (construction site):",
                "- Materials removed: 3",
                "- Orders cancelled: 1",
                "",
                "Unknown building:",
                "- Removed",
                "",
                "Sawmill 3:",
                "- Wrong stock: 4 wood removed",
                "",
            ]
        );
    }
}
