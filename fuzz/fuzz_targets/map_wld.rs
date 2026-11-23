//! `.WLD`/`.SWD` maps, with every accessor walked, and each decoded map written out and decoded
//! again to the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::map::Map;
use s2_fuzz::{check, map};

fuzz_target!(|data: &[u8]| {
    check(Map::parse_wld(data), |m| {
        map(&m);
        let written = m.to_wld();
        let again = Map::parse_wld(&written).expect("a written map decodes");
        assert_eq!(again.to_wld(), written);
    });
});
