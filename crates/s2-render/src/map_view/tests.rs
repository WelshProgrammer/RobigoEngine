use s2_assets::{Ink, Origin, Palette, Remap, Sprite, Tint};

use super::*;
use crate::terrain::testing::{shading, terrain, texture, varied};
use crate::terrain::{Fog, TILE_MARGIN};
use s2_sim::{
    Building, BuildingKind, BuildingState, Direction, Figure, Flag, FlagKind, Granite, MapObject, Node,
    Roads, Species, Tree, claim, mark_borders,
};

const VIEW: (u32, u32) = (448, 300);
const SCROLL: Point = Point::new(420, 100);

/// A `width` × `height` blob of colour `colour` with a player-colour band, or a shadow.
fn blob(width: u16, height: u16, colour: u8, ink: Ink) -> Sprite {
    let pixels = (0..height).flat_map(move |y| {
        (0..width).map(move |x| {
            let corner = (x == 0 || x == width - 1) && (y == 0 || y == height - 1);
            match (corner, ink) {
                (true, _) => None,
                (false, Ink::Colour) if y == 1 => Some((Ink::Player, u8::try_from(x % 4).unwrap())),
                (false, ink) => Some((ink, colour.wrapping_add(u8::try_from(x).unwrap()))),
            }
        })
    });
    Sprite::build(
        Origin {
            x: i16::try_from(width / 2).unwrap(),
            y: i16::try_from(height - 1).unwrap(),
        },
        width,
        height,
        pixels,
    )
}

fn sheet(entries: &[(usize, Sprite)]) -> Arc<SpriteSheet> {
    let len = entries.iter().map(|(slot, _)| slot + 1).max().unwrap_or(0);
    let mut slots = vec![None; len];
    for (slot, sprite) in entries {
        slots[*slot] = Some(sprite.clone());
    }
    Arc::new(SpriteSheet::from_slots(slots))
}

fn graphics() -> ObjectGraphics {
    let mut map = Vec::new();
    for phase in 0..8 {
        map.push((200 + phase, blob(20, 40, 10, Ink::Colour)));
        map.push((350 + phase, blob(30, 12, 0, Ink::Shadow)));
        map.push((3162 + phase, blob(6, 3, 60, Ink::Colour)));
    }
    map.push((516, blob(24, 18, 30, Ink::Colour)));
    map.push((616, blob(26, 8, 0, Ink::Shadow)));
    map.push((1700, blob(9, 6, 70, Ink::Colour)));
    map.push((1737, blob(10, 3, 0, Ink::Shadow)));
    for frame in 0..36 {
        map.push((1800 + frame, blob(12, 8, 90, Ink::Colour)));
    }
    map.push((1841, blob(12, 3, 0, Ink::Shadow)));
    let mut nation = vec![(0, blob(5, 7, 100, Ink::Colour))];
    for phase in 0..8 {
        nation.push((100 + phase, blob(8, 20, 110, Ink::Colour)));
        nation.push((110 + phase, blob(14, 6, 0, Ink::Shadow)));
    }
    nation.push((250, blob(60, 70, 140, Ink::Colour)));
    nation.push((251, blob(50, 30, 0, Ink::Shadow)));
    let palette = {
        let mut p = Palette::default();
        for (i, c) in p.0.iter_mut().enumerate() {
            *c = [u8::try_from(i).unwrap(); 3];
        }
        p
    };
    let mut graphics = ObjectGraphics::new(
        sheet(&map),
        Remap::nearest(&palette, Tint::SHADOW),
        Remap::nearest(&palette, Tint::GREY_SHADOW),
    );
    graphics.set_nation(Player::new(1).unwrap(), Nation::Romans, sheet(&nation));
    graphics
}

fn scene() -> (MapView, World) {
    let mut map = varied(24, 20);
    let mut road = map.get(Node { x: 9, y: 6 });
    road.roads = Roads::from_byte(0b01_11_10_01);
    map.set(Node { x: 9, y: 6 }, road);
    let owner = Player::new(1).unwrap();
    let hq = Node { x: 12, y: 8 };
    claim(&mut map, hq, 4, owner);
    mark_borders(&mut map);
    let terrain = terrain(
        map,
        texture(|u, v| (u ^ v) | 1),
        shading(|level, t| t.wrapping_add(level / 8)),
    );
    let mut world = World::empty(24, 20);
    let tree = Tree {
        kind: 0,
        stage: Tree::GROWN,
        felling: false,
        phase: 2,
    };
    world.set_object(Node { x: 10, y: 5 }, MapObject::Tree(tree));
    let granite = Granite {
        kind: 0,
        size: 1,
        spare: 0,
    };
    world.set_object(Node { x: 14, y: 6 }, MapObject::Granite(granite));
    let building = world
        .add_building(Building::new(
            BuildingKind::HEADQUARTERS,
            owner,
            hq,
            0,
            BuildingState::NEW_HEADQUARTERS,
        ))
        .unwrap();
    world.set_object(hq, MapObject::Building(building));
    let flag_node = Node { x: 12, y: 9 };
    let flag = world
        .add_flag(Flag::new(FlagKind::Normal, owner, flag_node))
        .unwrap();
    world.set_object(flag_node, MapObject::Flag(flag));
    world.add_figure(Node { x: 11, y: 10 }, Figure::animal(Species::LightRabbit));
    let mut fox = Figure::animal(Species::Fox);
    fox.direction = Direction::SouthEast;
    fox.remaining = 100;
    fox.counter = 3;
    world.add_figure(Node { x: 13, y: 9 }, fox);
    (MapView::new(terrain, graphics()), world)
}

fn draw(view: &MapView, world: &World) -> (Frame, Provenance) {
    let mut frame = Frame::new(VIEW.0, VIEW.1, 0);
    let mut provenance = Provenance::new(VIEW.0, VIEW.1);
    view.draw(world, SCROLL, anim(5), &mut frame, Some(&mut provenance));
    (frame, provenance)
}

/// Clocks with the animation counter at `anim` and the step counter at 0.
fn anim(anim: u32) -> Clocks {
    Clocks { anim, step: 0 }
}

#[test]
fn every_layer_draws_something() {
    let (view, world) = scene();
    let (_, provenance) = draw(&view, &world);
    for (name, layer) in Layers::named().filter(|&(_, l)| l != Layers::FOG) {
        assert!(provenance.mask(layer).contains(&true), "{name} drew nothing");
    }
}

#[test]
fn hiding_a_layer_changes_only_pixels_it_wrote() {
    let (mut view, world) = scene();
    let (all, provenance) = draw(&view, &world);
    for (name, layer) in Layers::named().filter(|&(_, l)| l != Layers::FOG) {
        view.set_layers(Layers::ALL - layer);
        let (without, _) = draw(&view, &world);
        let mut changed = 0;
        for y in 0..VIEW.1 {
            for x in 0..VIEW.0 {
                let i = (y * VIEW.0 + x) as usize;
                if all.pixels()[i] != without.pixels()[i] {
                    changed += 1;
                    assert!(
                        provenance.at(x, y).contains(layer),
                        "hiding {name} changed ({x}, {y}), written by {}",
                        provenance.at(x, y)
                    );
                }
            }
        }
        assert!(changed > 0, "hiding {name} changed nothing");
    }
}

#[test]
fn fog_off_draws_as_if_there_were_no_fog() {
    let (mut view, world) = scene();
    let (clear, _) = draw(&view, &world);
    let mut fog = Fog::new(24, 20);
    fog.reveal(Node { x: 12, y: 8 }, 2);
    view.terrain_mut().set_fog(Some(fog));
    let (fogged, _) = draw(&view, &world);
    assert_ne!(fogged, clear);
    view.set_layers(Layers::ALL - Layers::FOG);
    assert_eq!(draw(&view, &world).0, clear);
}

#[test]
fn fog_hides_whole_nodes_of_objects() {
    let (mut view, world) = scene();
    let mut fog = Fog::new(24, 20);
    fog.reveal(Node { x: 12, y: 8 }, 1);
    view.terrain_mut().set_fog(Some(fog));
    let (_, provenance) = draw(&view, &world);
    assert!(provenance.mask(Layers::BUILDINGS).contains(&true));
    assert!(
        !provenance.mask(Layers::FIGURES).contains(&true),
        "animals stand outside"
    );
}

#[test]
fn tracing_does_not_change_the_picture() {
    let (view, world) = scene();
    let (traced, _) = draw(&view, &world);
    let mut plain = Frame::new(VIEW.0, VIEW.1, 0);
    view.draw(&world, SCROLL, anim(5), &mut plain, None);
    assert_eq!(plain, traced);
}

#[test]
fn tiles_and_objects_match_the_direct_view_and_follow_layer_changes() {
    let (mut view, world) = scene();
    let mut tiles = TileCache::new(view.terrain());
    let inside = |a: &Frame, b: &Frame| {
        let margin = TILE_MARGIN;
        (margin..VIEW.1 - margin).all(|y| {
            (margin..VIEW.0 - margin).all(|x| {
                let i = (y * VIEW.0 + x) as usize;
                a.pixels()[i] == b.pixels()[i]
            })
        })
    };
    for layers in [
        Layers::ALL,
        Layers::ALL - Layers::STONES - Layers::ROADS,
        Layers::ALL,
    ] {
        view.set_layers(layers);
        let mut composed = Frame::new(VIEW.0, VIEW.1, 0);
        view.compose(&world, &mut tiles, SCROLL, anim(5), &mut composed);
        assert!(inside(&composed, &draw(&view, &world).0), "{layers}");
    }
    assert!(tiles.painted() > tiles.cached() as u64, "tiles were redrawn");
}

#[test]
fn syncing_with_a_changed_game_redraws_the_tiles_over_the_change() {
    let (mut view, world) = scene();
    let mut tiles = TileCache::new(view.terrain());
    let mut frame = Frame::new(VIEW.0, VIEW.1, 0);
    view.compose(&world, &mut tiles, SCROLL, anim(0), &mut frame);
    let mut nodes = view.terrain().map().clone();
    assert_eq!(view.sync(&nodes, &mut tiles), 0);
    let painted = tiles.painted();
    view.compose(&world, &mut tiles, SCROLL, anim(0), &mut frame);
    assert_eq!(tiles.painted(), painted, "nothing changed, nothing redrawn");

    let at = Node { x: 11, y: 7 };
    let mut data = nodes.get(at);
    data.roads = Roads::from_byte(0b01_00_00_01);
    nodes.set(at, data);
    assert_eq!(view.sync(&nodes, &mut tiles), 1);
    assert_eq!(view.terrain().map().get(at).roads, data.roads);
    view.compose(&world, &mut tiles, SCROLL, anim(0), &mut frame);
    let redrawn = tiles.painted() - painted;
    assert!(redrawn > 0, "the tiles over the road are redrawn");
    let mut fresh = Frame::new(VIEW.0, VIEW.1, 0);
    view.compose(
        &world,
        &mut TileCache::new(view.terrain()),
        SCROLL,
        anim(0),
        &mut fresh,
    );
    assert!(frame.pixels() == fresh.pixels(), "no stale tile is left");
}
