//! LST archives, single tagged items and stand-alone fonts, with every decoded item rendered.

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::lst::{Archive, Font, Item};
use s2_fuzz::{check, debug, item};

fuzz_target!(|data: &[u8]| {
    check(Archive::parse(data), |archive| {
        debug(&archive.quirks());
        archive.iter().for_each(item);
    });
    check(Item::parse_tagged(data), |(tagged, _)| item(&tagged));
    check(Font::parse_file(data), |font| item(&Item::Font(font)));
});
