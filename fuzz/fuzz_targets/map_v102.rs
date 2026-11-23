//! v1.02 `WORLDnnn.DAT` with its `CONTInnn.DAT`, each length-prefixed (see `s2_fuzz::Parts`).

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::map::Map;
use s2_fuzz::{Parts, check, map};

fuzz_target!(|data: &[u8]| {
    let mut parts = Parts::new(data);
    let (world, conti) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    check(Map::parse_v102(world, conti), |m| map(&m));
});
