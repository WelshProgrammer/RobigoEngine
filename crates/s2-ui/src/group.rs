//! The fourteen ware groups the transport priorities and the ware counts over time both list.
//!
//! A group is a 13-byte record at `0x1fa97c`: the ware it stands for, the colour its line takes
//! on the graph, and an item of interface art for the two groups that cover more than one ware.

use s2_sim::{TRANSPORT_GROUPS, WareKind};

use crate::geometry::Rect;
use crate::icons;
use crate::paint::{Painter, Sheet};
use crate::picture::Picture;
use crate::skin::Skin;

/// One of the fourteen ware groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    /// The ware the group stands for, which names its icon when the record has no item.
    pub ware: WareKind,
    /// The palette index the group's line draws in.
    pub colour: u8,
    /// The item the record names, for the two groups that stand for more than one ware.
    pub picture: Option<Picture>,
}

impl Group {
    /// Draws the group's icon with its origin at the middle of `within`, where the icon control
    /// `0x4e258` builds puts it (`0x4e5c9`).
    ///
    /// The icon is held inside the control, which is what cuts the rows an icon taller than its
    /// control would otherwise lay above and below it.
    pub fn draw(&self, painter: &mut Painter, skin: &Skin, within: Rect) {
        let middle = within.middle();
        painter.push_clip(within);
        match self.picture {
            Some(picture) => {
                if let Some(item) = skin.picture(picture) {
                    painter.item(item, middle);
                }
            }
            None => {
                if let Some(slot) = icons::ware(self.ware) {
                    painter.game(Sheet::Landscape, slot, middle);
                }
            }
        }
        painter.pop_clip();
    }
}

/// The fourteen groups, in group order.
pub const GROUPS: [Group; TRANSPORT_GROUPS] = [
    Group {
        ware: WareKind::WOOD,
        colour: 128,
        picture: None,
    },
    Group {
        ware: WareKind::BOARDS,
        colour: 4,
        picture: None,
    },
    Group {
        ware: WareKind::STONES,
        colour: 58,
        picture: None,
    },
    Group {
        ware: WareKind::HAM,
        colour: 45,
        picture: Some(Picture::MERCHANDISE_FOOD),
    },
    Group {
        ware: WareKind::WATER,
        colour: 109,
        picture: None,
    },
    Group {
        ware: WareKind::BEER,
        colour: 141,
        picture: None,
    },
    Group {
        ware: WareKind::COAL,
        colour: 121,
        picture: None,
    },
    Group {
        ware: WareKind::IRON_ORE,
        colour: 61,
        picture: None,
    },
    Group {
        ware: WareKind::GOLD,
        colour: 27,
        picture: None,
    },
    Group {
        ware: WareKind::IRON,
        colour: 138,
        picture: None,
    },
    Group {
        ware: WareKind::COINS,
        colour: 41,
        picture: None,
    },
    Group {
        ware: WareKind::HAMMER,
        colour: 105,
        picture: None,
    },
    Group {
        ware: WareKind::SWORD,
        colour: 143,
        picture: Some(Picture::MERCHANDISE_WEAPONS),
    },
    Group {
        ware: WareKind::BOAT,
        colour: 96,
        picture: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_groups_name_an_item_and_the_rest_draw_their_ware() {
        let named: Vec<_> = GROUPS
            .iter()
            .enumerate()
            .filter(|(_, group)| group.picture.is_some())
            .map(|(place, _)| place)
            .collect();
        assert_eq!(named, vec![3, 12]);
        assert_eq!(GROUPS[0].ware, WareKind::WOOD);
        assert_eq!(GROUPS[TRANSPORT_GROUPS - 1].ware, WareKind::BOAT);
    }

    #[test]
    fn an_icon_is_held_inside_its_control() {
        let within = Rect::new(20, 30, 30, 30);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        GROUPS[0].draw(&mut painter, &Skin::new(), within);
        GROUPS[3].draw(&mut painter, &Skin::new(), within);
        assert!(!painter.ops().is_empty(), "a group with no item draws its ware");
        assert!(
            painter.ops().iter().all(|op| op.clip == within),
            "an icon taller than its control would otherwise lay a row above and below it"
        );
    }
}
