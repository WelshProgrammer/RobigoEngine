//! A World Campaign continent's start against what the Gold Edition holds after Start on its
//! world screen, read out of its memory under DOSBox-X. Runs only when `S2_DATA_GOLD` names the
//! player's own Gold Edition install.

use std::path::PathBuf;

use s2_data::DataSet;
use s2_formats::map::Map;
use s2_sim::{Alliances, Controller, Game, Objective, Player, PlayerSlots, Setup, StartGoods, WareKind};

const EUROPE: &str = "DATA/MAPS2/EUROPE.WLD";

/// The faces of Europe's six players as the Gold Edition seats them.
const FACES: [u8; 6] = [0, 4, 5, 2, 1, 3];

/// Player 0's ware counts in the Gold Edition once Europe has started, by ware id: the
/// headquarters' table at twice its base counts.
const WARES: [u16; 31] = [
    24, 44, 68, 0, 0, 0, 4, 6, 8, 0, 6, 16, 16, 0, 0, 0, 0, 6, 2, 2, 16, 4, 4, 6, 8, 2, 2, 2, 6, 6, 12,
];

fn europe() -> Option<Map> {
    let data = DataSet::discover(PathBuf::from(std::env::var_os("S2_DATA_GOLD")?)).unwrap();
    Some(Map::parse_wld(&data.read(EUROPE).unwrap()).unwrap())
}

#[test]
fn europe_starts_as_the_gold_edition_seats_it() {
    let Some(map) = europe() else {
        eprintln!("skipped: set S2_DATA_GOLD to run this test");
        return;
    };
    let setup = Setup {
        players: None,
        animals: true,
        goods: StartGoods::Normal,
        slots: Some(PlayerSlots::world_campaign(&map)),
        starts: s2_sim::StartOrder::InSequence,
        objective: Objective::Headquarters,
        bug_fixes: s2_sim::BugFixes::Off,
        mission: None,
    };
    let mut game = Game::start(&map, &setup).unwrap();
    game.set_alliances(Alliances::AgainstFirst);
    let slots = game.world().slots();
    let seated: Vec<_> = Player::ALL
        .iter()
        .filter_map(|&player| slots.get(player))
        .map(|slot| (slot.controller, slot.portrait.0))
        .collect();
    let expected: Vec<_> = FACES
        .iter()
        .enumerate()
        .map(|(index, &face)| {
            let controller = if index == 0 {
                Controller::First
            } else {
                Controller::Computer
            };
            (controller, face)
        })
        .collect();
    assert_eq!(seated, expected);
    for a in Player::ALL.into_iter().take(FACES.len()) {
        for b in Player::ALL.into_iter().take(FACES.len()) {
            let computers = a.index() > 0 && b.index() > 0 && a != b;
            if a != b {
                assert_eq!(game.world().allied(a, b), computers, "{a:?} and {b:?}");
            }
        }
    }
    let first = Player::ALL[0];
    let wares: Vec<u16> = (0..WARES.len())
        .map(|ware| {
            game.world()
                .owned_wares(first, WareKind(u8::try_from(ware).unwrap()))
        })
        .collect();
    assert_eq!(wares, WARES);
}
