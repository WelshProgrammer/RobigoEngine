//! Smacker videos: parse, then decode every frame and audio chunk and render the first picture.

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::smacker::{MAX_TRACKS, Video};
use s2_fuzz::{check, debug};

fuzz_target!(|data: &[u8]| {
    check(Video::parse(data), |video| {
        debug(&video.header().frame_rate());
        let mut decoder = video.decoder();
        loop {
            match decoder.decode_next() {
                Ok(Some(frame)) => {
                    for track in 0..MAX_TRACKS {
                        debug(&frame.audio(track).map(|pcm| pcm.len()));
                    }
                    if frame.index == 0 {
                        std::hint::black_box(frame.image.to_rgba(frame.palette));
                    }
                }
                Ok(None) => break,
                Err(error) => debug(&error.to_string()),
            }
        }
    });
});
