//! Mission scripts (`.RTX`).

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::rtx::Script;
use s2_fuzz::{check, debug};

fuzz_target!(|data: &[u8]| {
    check(Script::parse(data), |script| {
        debug(&script);
        debug(&(
            script.global_settings(),
            script.map_commands(),
            script.commands().count(),
        ));
    });
});
