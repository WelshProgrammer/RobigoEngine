//! XMIDI songs, each sequence rendered to a Standard MIDI File and read back, Standard MIDI
//! Files, and OPL timbre banks.

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::music::{Sequence, TimbreBank, Xmidi};
use s2_fuzz::{check, debug};

fuzz_target!(|data: &[u8]| {
    check(Xmidi::parse(data), |xmidi| {
        for sequence in &xmidi.sequences {
            debug(&(sequence.length_ticks(), sequence.channel_mask()));
            let smf = sequence.to_smf();
            assert!(Sequence::from_smf(&smf).is_ok(), "a rendered sequence reads back");
        }
    });
    check(Sequence::from_smf(data), |sequence| debug(&sequence.length_ticks()));
    check(TimbreBank::parse(data), |bank| debug(&bank));
});
