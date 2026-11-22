//! Starting a game on a real map, shared by the tests that read a player's copy of the game.

use std::path::PathBuf;
use std::sync::Arc;

use s2_assets::{Landscape, LandscapeAssets, Nation, SpriteSheet};
use s2_data::DataSet;
use s2_formats::map::Map;
use s2_render::MapView;
use s2_render::terrain::{Fog, Terrain};
use s2_sim::{Game, Node, Player, Setup};

/// How far a headquarters lifts the fog when a game starts (byte `+0x13` of its type entry).
const HQ_SIGHT: u16 = 12;

/// The data set the variable `var` points at, or `None` with a note when it is unset.
pub fn data(var: &str) -> Option<DataSet> {
    let Some(path) = std::env::var_os(var).map(PathBuf::from) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    Some(DataSet::discover(path).unwrap())
}

/// A game started on `path` as the viewer starts one, and the view of it. The game has a Roman
/// headquarters at every start position and the map's animals. With `fog`, the view has the fog
/// player 0 sees at the start.
pub fn start(data: &DataSet, path: &str, fog: bool) -> (MapView, Game, Node) {
    let map = Map::parse_wld(&data.read(path).unwrap()).unwrap();
    let header = map.header().unwrap();
    let landscape = Landscape::from_terrain(header.terrain()).unwrap();
    let assets = LandscapeAssets::load(data, landscape).unwrap();
    let game = Game::start(&map, &Setup::default()).unwrap();
    let mut terrain = Terrain::with_assets(game.nodes().clone(), &assets);
    let hq = header.headquarters()[0].unwrap();
    let hq = Node { x: hq.x, y: hq.y };
    if fog {
        let mut seen = Fog::new(map.width(), map.height());
        seen.reveal(hq, HQ_SIGHT);
        terrain.set_fog(Some(seen));
    }
    let mission = landscape
        .free_play_mission_set()
        .map(|path| Arc::new(SpriteSheet::load(data, path).unwrap()));
    let mut graphics = s2_render::ObjectGraphics::new(
        Arc::new(assets.objects().unwrap().clone()),
        assets.shadow().clone(),
        assets.grey_shadow().clone(),
    )
    .with_mission(mission);
    let romans = Arc::new(SpriteSheet::load(data, Nation::Romans.sprites_path(landscape)).unwrap());
    for index in 0..7 {
        graphics.set_nation(Player::new(index).unwrap(), Nation::Romans, Arc::clone(&romans));
    }
    (MapView::new(terrain, graphics), game, hq)
}
