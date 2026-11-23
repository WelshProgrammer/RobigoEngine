//! Where a ware, a person or a building takes its picture in the game's own archives.
//!
//! `IO.DAT` holds the interface's own art. Everything that stands for something in the world
//! comes from the archives the game loads for the map: the landscape's `MAP_<set>_Y.LST` for the
//! wares and the people, and the player's `<NATION>_ICON.LST` for the buildings.

use s2_sim::WareKind;
use s2_systems::job::Job;

/// The first ware picture in the landscape archive, which [`WARE_PICTURES`] steps from.
pub const WARE_BASE: u16 = 2200;

/// The picture of each ware, in ware order (`0x1fa8e2`, one word every five bytes).
pub const WARE_PICTURES: [u16; 31] = [
    72, 73, 74, 83, 77, 68, 69, 82, 70, 61, 50, 81, 80, 79, 67, 78, 51, 53, 54, 55, 52, 56, 57, 58, 59, 62,
    63, 64, 66, 71, 65,
];

/// The first person picture, which [`PERSON_PICTURES`] steps from.
pub const PERSON_BASE: u16 = 2300;

/// The picture of each person, in job order (`0x1fad99`, one byte every eight).
pub const PERSON_PICTURES: [u8; 30] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 26, 21, 22, 23, 24, 25, 28, 27,
    29,
];

/// The upper part of the box an icon sits in.
pub const CELL_TOP: u16 = 2298;

/// The lower part, drawn `CELL_DROP` pixels under it.
pub const CELL_BOTTOM: u16 = 2299;

/// How far under the upper part the lower one sits.
pub const CELL_DROP: i32 = 18;

/// The picture `ware` draws from, or nothing for a ware the table has no picture for.
#[must_use]
pub fn ware(ware: WareKind) -> Option<u16> {
    WARE_PICTURES
        .get(ware.index())
        .map(|&picture| WARE_BASE + picture)
}

/// The picture `job` draws from, or nothing for a job the table has no picture for.
#[must_use]
pub fn person(job: Job) -> Option<u16> {
    PERSON_PICTURES
        .get(job.index())
        .map(|&picture| PERSON_BASE + u16::from(picture))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pictures_land_where_the_original_reads_them() {
        assert_eq!(ware(WareKind(0)), Some(2200 + 72));
        assert_eq!(ware(WareKind(30)), Some(2200 + 65));
        assert_eq!(ware(WareKind(31)), None);
        assert_eq!(person(Job(0)), Some(2300));
        assert_eq!(person(Job(29)), Some(2300 + 29));
        assert_eq!(person(Job(30)), None);
    }

    #[test]
    fn every_ware_and_person_has_its_own_picture() {
        let mut wares = WARE_PICTURES;
        wares.sort_unstable();
        wares.windows(2).for_each(|pair| assert_ne!(pair[0], pair[1]));
        let mut people = PERSON_PICTURES;
        people.sort_unstable();
        people.windows(2).for_each(|pair| assert_ne!(pair[0], pair[1]));
        assert!(
            WARE_BASE + wares[wares.len() - 1] < CELL_TOP,
            "the ware pictures stop before the box"
        );
        let last = PERSON_BASE + u16::from(people[people.len() - 1]);
        assert!(CELL_BOTTOM < PERSON_BASE && PERSON_BASE < last);
    }
}
