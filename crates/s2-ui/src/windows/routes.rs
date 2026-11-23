//! The trading routes screen.
//!
//! It needs two finished harbours of the player's own and builds nothing without them. The two
//! pictures are small views of the world round each harbour, which the renderer fills, so the
//! window gives their rectangles and the node each one is centred on.
//!
//! No button, key or menu in either shipped build opens window kind 46, so the layout here
//! comes from the constructor alone and has never been seen running.

use s2_sim::{BuildingState, Node, Player, World};

use crate::geometry::Rect;
use crate::paint::Painter;
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// The text slot the title shows.
const TITLE: u16 = 571;

/// Where the window sits.
const AT: (i32, i32) = (20, 20);

/// Where the panel the route draws in sits.
const PANEL: (i32, i32, u32, u32) = (11, 22, 300, 250);

/// The first step button's rectangle. The second is the same with its own width added to x.
const STEP: (i32, i32, u32, u32) = (26, 26, 16, 16);

/// Where each harbour's picture sits.
const PICTURES: [(i32, i32, u32, u32); 2] = [(10, 10, 40, 40), (100, 10, 40, 40)];

/// The picture both step buttons draw.
pub const STEP_PICTURE: Picture = Picture::GO_TO;

/// How many harbours the screen needs.
pub const HARBOURS: usize = 2;

/// What a release on one of the two step buttons asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The first button, command 12804.
    Back,
    /// The second, command 12805.
    On,
}

impl Step {
    /// Both of them, in button order.
    pub const ALL: [Self; 2] = [Self::Back, Self::On];
}

/// The trading routes screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutesWindow {
    /// The player whose harbours it shows.
    pub player: Player,
    /// The two harbours it found, in the order the pool holds them.
    pub harbours: [u16; HARBOURS],
}

impl RoutesWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 300;
    /// How tall it is.
    pub const HEIGHT: u32 = 200;
    /// Which window kind it opens as.
    pub const KIND: WindowId = kind::ROUTES;

    /// The window for `player`, or nothing when the world holds fewer than two finished harbours
    /// of theirs.
    #[must_use]
    pub fn new(world: &World, player: Player) -> Option<Self> {
        let mut found = [0; HARBOURS];
        let mut count = 0;
        for (index, harbour) in world.harbours() {
            if harbour.owner != player || !Self::finished(world, harbour.building) {
                continue;
            }
            let Some(slot) = found.get_mut(count) else { break };
            *slot = index;
            count += 1;
        }
        (count == HARBOURS).then_some(Self {
            player,
            harbours: found,
        })
    }

    /// Whether the harbour's building is up.
    fn finished(world: &World, building: u16) -> bool {
        world
            .building(building)
            .is_some_and(|record| record.state.contains(BuildingState::FINISHED))
    }

    /// Where the window sits and how big it is.
    #[must_use]
    pub const fn area() -> Rect {
        Rect::new(AT.0, AT.1, Self::WIDTH, Self::HEIGHT)
    }

    /// The text slot the title shows.
    #[must_use]
    pub const fn title() -> u16 {
        TITLE
    }

    /// Where the route panel sits inside a window of `area`.
    #[must_use]
    pub fn panel(area: Rect) -> Rect {
        let (x, y, width, height) = PANEL;
        Rect::new(area.x + x, area.y + y, width, height)
    }

    /// Where one of the two step buttons sits inside a window of `area`.
    #[must_use]
    pub fn step(step: Step, area: Rect) -> Rect {
        let (x, y, width, height) = STEP;
        let across = match step {
            Step::Back => 0,
            Step::On => i32::try_from(width).unwrap_or_default(),
        };
        Rect::new(area.x + x + across, area.y + y, width, height)
    }

    /// Where harbour `place`'s picture sits inside a window of `area`.
    #[must_use]
    pub fn picture(place: usize, area: Rect) -> Rect {
        let (x, y, width, height) = PICTURES[place.min(PICTURES.len() - 1)];
        Rect::new(area.x + x, area.y + y, width, height)
    }

    /// The node harbour `place`'s picture is centred on.
    #[must_use]
    pub fn centre(&self, world: &World, place: usize) -> Option<Node> {
        let index = *self.harbours.get(place)?;
        let harbour = world.harbour(index)?;
        world.building(harbour.building).map(|record| record.node)
    }

    /// Takes `mouse`, draws the window and gives back what a release asks for.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(Self::KIND)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let mut asked = None;
        for (place, &step) in Step::ALL.iter().enumerate() {
            let rect = Self::step(step, area);
            let widget = WidgetId::new(Self::KIND, u16::try_from(place).unwrap_or_default());
            let acted = ui.press(widget, rect, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), rect, look.edges());
            }
            if let Some(item) = skin.picture(STEP_PICTURE) {
                painter.item_centred(item, rect);
            }
            if acted && asked.is_none() {
                asked = Some(Outcome::Routes(step));
            }
        }
        painter.pop_clip();
        asked
    }
}

#[cfg(test)]
mod tests {
    use s2_render::terrain::Point;

    use super::*;
    use crate::paint::ItemId;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin
    }

    fn player() -> Player {
        Player::new(0).expect("player 0 exists")
    }

    #[test]
    fn fewer_than_two_harbours_builds_no_window() {
        let world = World::empty(32, 32);
        assert_eq!(RoutesWindow::new(&world, player()), None);
    }

    #[test]
    fn a_picture_with_no_harbour_behind_it_names_no_node() {
        let world = World::empty(32, 32);
        let window = RoutesWindow {
            player: player(),
            harbours: [0, 1],
        };
        assert_eq!(window.centre(&world, 0), None);
        assert_eq!(window.centre(&world, HARBOURS), None);
    }

    #[test]
    fn the_parts_sit_where_the_original_puts_them() {
        let area = RoutesWindow::area();
        assert_eq!(area, Rect::new(20, 20, 300, 200));
        assert_eq!(RoutesWindow::title(), 571);
        assert_eq!(RoutesWindow::panel(area), Rect::new(31, 42, 300, 250));
        assert_eq!(RoutesWindow::step(Step::Back, area), Rect::new(46, 46, 16, 16));
        assert_eq!(
            RoutesWindow::step(Step::On, area),
            Rect::new(62, 46, 16, 16),
            "the second button is the first with its own width added"
        );
        assert_eq!(RoutesWindow::picture(0, area), Rect::new(30, 30, 40, 40));
        assert_eq!(RoutesWindow::picture(1, area), Rect::new(120, 30, 40, 40));
    }

    #[test]
    fn each_button_asks_for_its_own_step() {
        let area = RoutesWindow::area();
        for (step, want) in [
            (Step::Back, Outcome::Routes(Step::Back)),
            (Step::On, Outcome::Routes(Step::On)),
        ] {
            let mut ui = Ui::new();
            ui.open(RoutesWindow::KIND, area);
            let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
            let mut window = RoutesWindow {
                player: player(),
                harbours: [0, 1],
            };
            let rect = RoutesWindow::step(step, area);
            let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
            window.run(&mut ui, &mut painter, &skin(), on.pressing());
            painter.reset();
            assert_eq!(
                window.run(&mut ui, &mut painter, &skin(), on.releasing()),
                Some(want)
            );
        }
    }
}
