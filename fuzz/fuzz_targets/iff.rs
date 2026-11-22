//! IFF pictures and palettes (`.LBM`, `.BBM`) and gouraud shading tables.

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::gouraud::ShadingTable;
use s2_formats::{Palette, iff};
use s2_fuzz::{check, debug};

fuzz_target!(|data: &[u8]| {
    check(iff::parse(data), |picture| {
        debug(&picture);
        let palette = picture.palette.clone().unwrap_or_default();
        std::hint::black_box(picture.image.to_rgba(&palette));
    });
    check(iff::read_palette(data), |palette: Palette| {
        debug(&palette.colour(0));
    });
    check(ShadingTable::parse(data), |table| {
        debug(&(table.shade(u8::MAX, u8::MAX), table.row(0)));
    });
});
