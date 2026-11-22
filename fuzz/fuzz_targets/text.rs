//! String tables (`.ENG`), plain text pages and the CP437 decoder.

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::text::{self, Page, StringTable, cp437};
use s2_fuzz::{check, debug};

fuzz_target!(|data: &[u8]| {
    check(StringTable::parse(data), |table| {
        debug(&table.quirks());
        for slot in 0..table.len() {
            debug(&table.get(slot));
        }
        table.iter().for_each(|entry| debug(&entry));
    });
    let page = Page::parse(data);
    debug(&page.lines().count());
    debug(&page.raw_lines().count());
    let decoded = cp437::decode(data);
    debug(&text::with_newlines(&decoded));
    debug(&text::lines(&page.text()).count());
});
