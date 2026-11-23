//! IDX/DAT pairs: input is the index then the DAT, each length-prefixed (see `s2_fuzz::Parts`).

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::idx::Index;
use s2_fuzz::{Parts, check, debug, item};

fuzz_target!(|data: &[u8]| {
    let mut parts = Parts::new(data);
    let (idx, dat) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    check(Index::parse(idx, dat), |index| {
        debug(&index.quirks());
        for entry in &index {
            debug(&entry.name());
            item(entry.item());
        }
    });
});
