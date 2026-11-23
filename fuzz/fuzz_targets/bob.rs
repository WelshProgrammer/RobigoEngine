//! BOB figure files with every composite rendered, and `ROM_ANIM.DAT` animation tables.

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::bob::{Bob, BodyType, Direction, FRAMES, RomAnim};
use s2_fuzz::{check, debug, player};

fuzz_target!(|data: &[u8]| {
    check(Bob::parse(data), |bob| {
        debug(&bob.quirks());
        bob.bodies().iter().for_each(player);
        bob.overlays().iter().filter_map(|o| o.bitmap()).for_each(player);
        for item in 0..bob.items() {
            for body in BodyType::ALL {
                for direction in Direction::ALL {
                    for frame in (0..).take(FRAMES) {
                        debug(&bob.composite(item, body, direction, frame));
                    }
                }
            }
        }
    });
    check(RomAnim::parse(data), |anim| {
        debug(&anim.quirks());
        for record in anim.records() {
            debug(&(record.step(), record.sprite(), record.is_blank()));
        }
    });
});
