//! Animals in the simulation step: wandering and resting (`0x41a80`) and the spawner that adds
//! animals to dense forest (`0x42188`).

use crate::figures::{AnimalState, FigureKind, Species};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::ground::Property;

/// How many speeds each slope offers; the shared cursor runs through them in turn.
const SPEED_STEPS: u8 = 4;

/// Walking speeds by height change, uphill, level and downhill (`0x1fa6b0`, `0x1fa6b4`,
/// `0x1fa6b8`), picked in turn by a cursor every walker shares.
const SPEEDS: [[u8; SPEED_STEPS as usize]; 3] = [[6, 7, 9, 10], [13, 14, 15, 16], [21, 22, 23, 24]];

/// An idle animal rests on one draw in this many. `S2.EXE` starts a rest when the draw's low
/// three bits are all set.
const REST_ODDS: u16 = 8;

/// How long a rest lasts, in steps, drawn inclusive.
const REST_STEPS: (u16, u16) = (10, 100);

/// What the spawner creates, by `random mod 8` (`0x1fbb14`).
const SPAWNED: [Species; 8] = [
    Species::LightRabbit,
    Species::LightRabbit,
    Species::GreyRabbit,
    Species::GreyRabbit,
    Species::Fox,
    Species::Stag,
    Species::Sheep,
    Species::Sheep,
];

/// Where the preferred and fallback candidates go in the animal's one 40-byte list. Preferred
/// entries can run past 20 into the fallback half, as in `S2.EXE`.
const FALLBACK: usize = 20;
const CANDIDATES: usize = 40;

/// The spawner's block size and the least area it walks over.
const BLOCK: u16 = 16;
const SPAWN_AREA: u16 = 256;

impl Game {
    /// The next walking speed for a height change of `rise` (`0x3308c`): the shared cursor
    /// moves on first, then picks from the table for the slope.
    pub(crate) fn walking_speed(&mut self, rise: i16) -> u8 {
        self.walk_cursor = (self.walk_cursor + 1) % SPEED_STEPS;
        let table = match rise {
            2.. => 0,
            -1..=1 => 1,
            _ => 2,
        };
        SPEEDS[table][usize::from(self.walk_cursor)]
    }

    /// One simulation step of animal `record` (`0x41a80`).
    pub(crate) fn step_animal(&mut self, record: u16) {
        let Some(figure) = self.world.figure_mut(record) else {
            return;
        };
        let FigureKind::Animal {
            species,
            state,
            hunter,
        } = figure.kind
        else {
            return;
        };
        if figure.walking() {
            figure.advance_walk();
            figure.counter = figure.counter.wrapping_add(1);
            return;
        }
        match state {
            AnimalState::Shot | AnimalState::Dead => return,
            AnimalState::Resting => {
                figure.counter = figure.counter.wrapping_sub(1);
                if figure.counter != 0 {
                    return;
                }
                figure.kind = FigureKind::Animal {
                    species,
                    state: AnimalState::Active,
                    hunter,
                };
            }
            AnimalState::Active => {}
        }
        let (here, facing) = (figure.node(), figure.direction);
        if self.rng.step() % REST_ODDS == REST_ODDS - 1 {
            let rest = self.rng.between(REST_STEPS.0, REST_STEPS.1);
            if let Some(figure) = self.world.figure_mut(record) {
                figure.kind = FigureKind::Animal {
                    species,
                    state: AnimalState::Resting,
                    hunter,
                };
                figure.counter = rest;
            }
            return;
        }
        if let Some(hunter) = hunter {
            self.shoot(hunter, record);
            return;
        }
        let Some(direction) = self.choose_way(species, here, facing) else {
            self.world.remove_figure(record);
            return;
        };
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let there = here.neighbour(direction, width, height);
        let rise = i16::from(self.nodes.get(there).altitude) - i16::from(self.nodes.get(here).altitude);
        let speed = self.walking_speed(rise) / species.traits().slowness;
        self.world.move_figure(record, there);
        if let Some(figure) = self.world.figure_mut(record) {
            figure.start_walk(direction, speed);
        }
    }

    /// Weighs the six neighbours of `here` and draws one; `None` when none will do.
    fn choose_way(&mut self, species: Species, here: Node, facing: Direction) -> Option<Direction> {
        let traits = species.traits();
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let mut list = [0_u8; CANDIDATES];
        let (mut preferred, mut fallback) = (0_usize, 0_usize);
        // One step either side of `facing`, never straight ahead: these get six extra entries.
        let beside = [(facing.index() + 5) % 6, (facing.index() + 1) % 6];
        // North-east first, then round to north-west: the order `0x41a80` tries the neighbours in.
        for step in 0..6 {
            let direction = Direction::from_index(step + 5);
            let there = here.neighbour(direction, width, height);
            let object = self.world.object(there);
            let accepted = if traits.polar {
                self.nodes.property(there).contains(Property::ANY_POLAR)
            } else if traits.swims {
                !object.is_flag() && self.nodes.edge_water(here, direction)
            } else {
                if !self.nodes.get(there).sites.passable()
                    || object.is_flag()
                    || !self.nodes.edge_walkable(here, direction)
                {
                    continue;
                }
                let open = self.nodes.property(there).contains(Property::ALL_BUILDABLE)
                    && self.nodes.get(there).roads.is_empty()
                    && !self.world.has_figures(there);
                if !open {
                    if let Some(slot) = list.get_mut(FALLBACK + fallback) {
                        *slot = direction_byte(direction);
                        fallback += 1;
                    }
                    continue;
                }
                true
            };
            if !accepted {
                continue;
            }
            let mut weight = 1;
            if beside.contains(&direction.index()) {
                weight += 6;
            }
            if traits.tree_weight > 0 && object.grown_tree() {
                weight += usize::from(traits.tree_weight);
            }
            for _ in 0..weight {
                if let Some(slot) = list.get_mut(preferred) {
                    *slot = direction_byte(direction);
                }
                preferred += 1;
            }
        }
        let (start, count) = match (preferred, fallback) {
            (0, 0) => return None,
            (0, n) => (FALLBACK, n),
            (n, _) => (0, n),
        };
        let highest = u16::try_from(count - 1).unwrap_or(u16::MAX);
        let pick = usize::from(self.rng.between(0, highest));
        Some(Direction::from_index(usize::from(list[start + pick])))
    }

    /// The animal spawner (`0x42188`), run every eighth step: walks a cursor over 16 × 16
    /// blocks and adds an animal on the first grown tree of a block with more than 64 grown
    /// trees and fewer than 4 counted animals.
    pub(crate) fn spawn_animals(&mut self) {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let (span_x, span_y) = (width.max(SPAWN_AREA), height.max(SPAWN_AREA));
        let (mut x, mut y) = (self.spawn_cursor.0 % span_x, self.spawn_cursor.1 % span_y);
        x += BLOCK;
        if x >= span_x {
            x = 0;
            y += BLOCK;
            if y >= span_y {
                y = 0;
            }
        }
        self.spawn_cursor = (x, y);
        if x >= width || y >= height {
            return;
        }
        let (mut trees, mut animals, mut first) = (0_u32, 0_u32, None);
        for row in y..y + BLOCK {
            for column in x..x + BLOCK {
                let node = Node::wrapped(column.into(), row.into(), width, height);
                if self.world.object(node).grown_tree() {
                    trees += 1;
                    first.get_or_insert(node);
                }
                for (_, figure) in self.world.figures_at(node) {
                    if let FigureKind::Animal { species, .. } = figure.kind
                        && species.traits().counted
                    {
                        animals += 1;
                    }
                }
            }
        }
        if let Some(node) = first
            && trees > 64
            && animals < 4
        {
            let species = SPAWNED[usize::from(self.rng.step()) % SPAWNED.len()];
            self.world
                .add_figure(node, crate::figures::Figure::animal(species));
        }
    }
}

fn direction_byte(direction: Direction) -> u8 {
    u8::try_from(direction.index()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use s2_formats::map::Map;

    use super::*;
    use crate::world::tests::wld;

    const SIZE: u16 = 32;
    const MEADOW: u8 = 8;
    const WATER: u8 = 5;
    const SHEEP: u8 = 6;
    const DUCK: u8 = 5;

    /// A grown tree of type 0: group `0xc4`, stage 3.
    const GROWN_TREE: u16 = 0xc430;

    fn game(terrain: u8, layers: &[(u8, Vec<u8>)], animals: &[(u8, u16, u16)]) -> Game {
        let nodes = usize::from(SIZE) * usize::from(SIZE);
        let mut all = vec![(1, vec![terrain; nodes]), (2, vec![terrain; nodes])];
        all.extend_from_slice(layers);
        Game::start(
            &Map::parse_wld(&wld(SIZE, SIZE, &all, animals)).unwrap(),
            &crate::Setup::default(),
        )
        .unwrap()
    }

    fn walking(game: &Game, record: u16) -> bool {
        game.world
            .figure(record)
            .is_some_and(crate::figures::Figure::walking)
    }

    #[test]
    fn the_first_walk_takes_the_second_level_speed_and_ceil_256_over_it_steps() {
        let mut game = game(MEADOW, &[], &[(SHEEP, 10, 10)]);
        let mut steps = 0;
        while !walking(&game, 0) {
            game.step();
            steps += 1;
            assert!(steps < 500, "the sheep never set off");
        }
        assert_eq!(
            game.world.figure(0).unwrap().speed,
            14,
            "the cursor moves before it picks"
        );
        let mut walk = 1;
        while walking(&game, 0) {
            game.step();
            walk += 1;
        }
        assert_eq!(walk, 256_u32.div_ceil(14));
    }

    #[test]
    fn a_duck_on_dry_land_is_removed_and_its_record_reused() {
        let mut game = game(MEADOW, &[], &[(DUCK, 10, 10)]);
        for _ in 0..200 {
            game.step();
        }
        assert_eq!(game.world.figures().count(), 0);
        let record = game
            .world
            .add_figure(Node { x: 3, y: 3 }, crate::figures::Figure::animal(Species::Fox));
        assert_eq!(record, 0);
    }

    #[test]
    fn a_duck_on_water_swims() {
        let mut game = game(WATER, &[], &[(DUCK, 10, 10)]);
        let start = game.world.figure(0).unwrap().node();
        for _ in 0..200 {
            game.step();
        }
        let duck = game.world.figure(0).expect("still there");
        assert_ne!(duck.node(), start);
        assert!(
            (3..=4).contains(&duck.speed),
            "level speeds 13 to 16, over 4 for ducks"
        );
    }

    #[test]
    fn dense_forest_gets_an_animal_on_its_first_tree() {
        let nodes = usize::from(SIZE) * usize::from(SIZE);
        let [grown_detail, grown_group] = GROWN_TREE.to_le_bytes();
        let mut detail = vec![0; nodes];
        let mut group = vec![0; nodes];
        for y in 0..16 {
            for x in 16..32 {
                detail[y * usize::from(SIZE) + x] = grown_detail;
                group[y * usize::from(SIZE) + x] = grown_group;
            }
        }
        let mut game = game(MEADOW, &[(4, detail), (5, group)], &[]);
        for _ in 0..7 {
            game.step();
        }
        assert_eq!(game.world.figures().count(), 0);
        game.step();
        let (_, animal) = game.world.figures().next().expect("one spawned at step 8");
        assert_eq!(animal.node(), Node { x: 16, y: 0 });
    }
}
