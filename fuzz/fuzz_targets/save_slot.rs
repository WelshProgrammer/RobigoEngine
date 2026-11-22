//! Whole savegame slots. The input starts with a presence byte: bit `n` is set when optional part
//! `n` follows, in the order transport, control, desktop, mails, computer. The world, the area
//! table and each present part follow, all length-prefixed (see `s2_fuzz::Parts`).

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::save::{Slot, SlotFiles};
use s2_fuzz::{Parts, check, debug, map};

fuzz_target!(|data: &[u8]| {
    let Some((&present, rest)) = data.split_first() else {
        return;
    };
    let mut parts = Parts::new(rest);
    let world = parts.next().unwrap_or_default();
    let areas = parts.next().unwrap_or_default();
    let mut optional = (0..5).map(|bit| (present >> bit & 1 != 0).then(|| parts.next()).flatten());
    let mut next = || optional.next().flatten();
    let files = SlotFiles {
        world,
        areas,
        transport: next(),
        control: next(),
        desktop: next(),
        mails: next(),
        computer: next(),
    };
    check(Slot::parse(&files), |slot| {
        map(slot.world());
        debug(&slot);
        debug(&slot.missing());
    });
});
