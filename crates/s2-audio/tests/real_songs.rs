//! The songs of a player's own copy of the game, run only when `S2_DATA_PLAIN` or `S2_DATA_GOLD`
//! points at it: the score the sequencer reads from a baked MIDI file is the score of the XMIDI it
//! came from. No game data lives here.

use s2_audio::{Score, Sequencer};
use s2_data::DataSet;
use s2_formats::music::{Event, Sequence, Xmidi};

/// The songs the MIDI player offers.
const SONGS: u8 = 14;

fn datasets() -> Vec<(&'static str, DataSet)> {
    let found: Vec<_> = ["S2_DATA_PLAIN", "S2_DATA_GOLD"]
        .into_iter()
        .filter_map(|var| Some((var, DataSet::discover(std::env::var_os(var)?).unwrap())))
        .collect();
    if found.is_empty() {
        eprintln!("skipped: set S2_DATA_PLAIN or S2_DATA_GOLD to run this test");
    }
    found
}

#[test]
fn a_baked_song_plays_as_its_xmidi_does() {
    for (var, data) in datasets() {
        for number in 1..=SONGS {
            let bytes = data
                .read(&format!("DATA/SOUNDDAT/SNG/SNG_{number:04}.DAT"))
                .unwrap();
            let xmidi = Xmidi::parse(&bytes).unwrap().sequences.remove(0);
            let baked = Sequence::from_smf(&xmidi.to_smf()).unwrap();
            let score = Score::new(&xmidi);
            assert_eq!(Score::new(&baked), score, "{var}: song {number}");

            let mut sequencer = Sequencer::new(score.clone());
            let mut sent = Vec::new();
            for _ in 0..=score.end() {
                sequencer.tick(&mut sent);
            }
            let ons = sent
                .iter()
                .filter(|event| matches!(event, Event::NoteOn { .. }))
                .count();
            let offs = sent
                .iter()
                .filter(|event| matches!(event, Event::NoteOff { .. }))
                .count();
            assert!(ons > 0, "{var}: song {number} plays notes");
            assert!(offs <= ons, "{var}: song {number} sends no more offs than ons");
        }
    }
}
