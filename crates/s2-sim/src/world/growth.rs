//! The second and third parts of the ageing pass: trees grow, fields ripen and geologists' signs
//! fade (`0x3dae8`).

use super::{MapObject, World};
use crate::grid::Node;

impl World {
    /// Growth over map row `row` (`0x3dae8`, second part): see [`MapObject::grow`]. A row past
    /// the map's height does nothing.
    pub(crate) fn grow_row(&mut self, row: u16) {
        self.change_row(row, MapObject::grow);
    }

    /// Fields and signs over map row `row` (`0x3dae8`, third part): see
    /// [`MapObject::ripen_and_fade`].
    pub(crate) fn age_marks_row(&mut self, row: u16) {
        self.change_row(row, MapObject::ripen_and_fade);
    }

    /// Runs `change` on each object of row `row`, writing back what it returns.
    fn change_row(&mut self, row: u16, change: impl Fn(MapObject) -> Option<MapObject>) {
        if row >= self.height {
            return;
        }
        for x in 0..self.width {
            let node = Node { x, y: row };
            if let Some(changed) = change(self.object(node)) {
                self.set_object(node, changed);
            }
        }
    }
}
