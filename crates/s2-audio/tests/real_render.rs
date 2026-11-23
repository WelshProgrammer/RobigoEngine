//! Every song of a player's own copy rendered as the bake renders it, run only when
//! `S2_DATA_PLAIN` points at the game, and through a sound font as well when `S2_SOUND_FONT`
//! names an SF2 file. A looping song repeats one pass of itself, and going round the loop must
//! sound as going on would; no game data lives here.

use std::sync::Arc;

use s2_audio::{GeneralMidi, Instrument, Opl3, Score, Song, render_song};
use s2_data::DataSet;
use s2_formats::music::{Sequence, TimbreBank, Xmidi};

/// The most a seam may differ from going on, in sample units: about −48 dBFS.
const SEAM: i32 = 128;

/// The songs that play once.
const ONCE: [u8; 2] = [7, 14];

/// Renders every song side by side, each through the instrument `make` gives it.
fn render_all(songs: &[(u8, Sequence)], make: &(dyn Fn() -> Box<dyn Instrument> + Sync)) -> Vec<(u8, Song)> {
    std::thread::scope(|scope| {
        let renders: Vec<_> = songs
            .iter()
            .map(|(number, song)| {
                scope.spawn(move || {
                    let mut instrument = make();
                    (*number, render_song(instrument.as_mut(), Score::new(song)))
                })
            })
            .collect();
        renders.into_iter().map(|render| render.join().unwrap()).collect()
    })
}

fn check(name: &str, rendered: &[(u8, Song)]) {
    for (number, song) in rendered {
        println!(
            "{name} song {number}: repeat {:?}, seam {}, blended {}",
            song.repeat, song.seam, song.blended
        );
        match song.repeat {
            Some((start, length)) => {
                assert_eq!(
                    start + length,
                    song.frames.len(),
                    "{name} song {number} ends with its repeat"
                );
                assert!(
                    song.seam <= SEAM || song.blended > 0,
                    "{name} song {number}: seam {} and no blend",
                    song.seam
                );
                if song.blended > 0 {
                    assert_eq!(
                        song.frames.last(),
                        song.frames.get(start - 1),
                        "{name} song {number}: the blend ends on the frame before the repeat"
                    );
                }
                assert!(!ONCE.contains(number), "{name} song {number} has no next-loop");
            }
            None => assert!(ONCE.contains(number), "{name} song {number} loops"),
        }
    }
}

#[test]
fn every_song_loops_without_a_seam_or_plays_once() {
    let Some(data) = std::env::var_os("S2_DATA_PLAIN") else {
        eprintln!("skipped: set S2_DATA_PLAIN to run this test");
        return;
    };
    let data = DataSet::discover(data).unwrap();
    let bank = TimbreBank::parse(&data.read("DRIVERS/SETT_SND.OPL").unwrap()).unwrap();
    let songs: Vec<_> = (1..=14u8)
        .map(|number| {
            let path = format!("DATA/SOUNDDAT/SNG/SNG_{number:04}.DAT");
            (
                number,
                Xmidi::parse(&data.read(&path).unwrap())
                    .unwrap()
                    .sequences
                    .remove(0),
            )
        })
        .collect();
    check("OPL3", &render_all(&songs, &|| Box::new(Opl3::new(bank.clone()))));
    if let Some(font) = std::env::var_os("S2_SOUND_FONT") {
        let font = Arc::new(rustysynth::SoundFont::new(&mut std::fs::File::open(font).unwrap()).unwrap());
        check(
            "General MIDI",
            &render_all(&songs, &|| Box::new(GeneralMidi::new(&font).unwrap())),
        );
    }
}
