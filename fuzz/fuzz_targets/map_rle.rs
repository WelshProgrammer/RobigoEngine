//! The map layer RLE: a little-endian `u24` output length, then the packed stream.

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::map::rle;
use s2_fuzz::check;

fuzz_target!(|data: &[u8]| {
    let Some(([a, b, c], packed)) = data.split_first_chunk::<3>() else {
        return;
    };
    let expected = usize::from(*a) | usize::from(*b) << 8 | usize::from(*c) << 16;
    check(rle::decode(packed, expected), |out| {
        assert_eq!(out.len(), expected);
    });
});
