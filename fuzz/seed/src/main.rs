//! Rebuilds the fuzz seed corpus from the player's own copy of the game.
//!
//! Reads every edition named by `S2_DATA_PLAIN`, `S2_DATA_GOLD` and `S2_DATA_CD` (anything
//! `s2_data::DataSet::discover` accepts) and writes seeds to `fuzz/corpus/<target>/`, or to
//! the directory given as the only argument. Files too big to mutate quickly are cut down
//! first: archives and indexes are split into single items, maps are cropped, videos keep a
//! few frames. A few seeds are also built from scratch. Seeds are named by content hash, so
//! editions sharing a file share a seed.

mod shrink;
mod synthetic;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use s2_data::{DataSet, EntryKind};
use s2_formats::idx::Index;
use s2_formats::lst::{Archive, Item};
use s2_formats::map::{Map, rle};
use s2_formats::save::{Part, Slot, SlotFiles};
use s2_formats::smacker::Video;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const VARS: [&str; 3] = ["S2_DATA_PLAIN", "S2_DATA_GOLD", "S2_DATA_CD"];

/// The slot parts the slot target's presence byte covers, from bit 0 up.
const OPTIONAL_PARTS: [Part; 5] = [
    Part::Transport,
    Part::Control,
    Part::Desktop,
    Part::Mails,
    Part::Computer,
];

/// Seeds past this size are left out; libFuzzer would only truncate them.
const MAX_SEED: usize = 1 << 20;

fn main() -> ExitCode {
    let out = std::env::args_os().nth(1).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus"),
        PathBuf::from,
    );
    let mut corpus = Corpus::new(out);
    let smacker = synthetic::smacker();
    corpus.check("smacker", decodes(&smacker));
    if let Err(error) = corpus.add("smacker", &smacker) {
        eprintln!("synthetic seeds: {error}");
        return ExitCode::FAILURE;
    }
    let mut editions = 0;
    for var in VARS {
        let Some(path) = std::env::var_os(var) else {
            continue;
        };
        let result = DataSet::discover(&path)
            .map_err(Into::into)
            .and_then(|data| seed(&data, &mut corpus));
        match result {
            Ok(()) => editions += 1,
            Err(error) => eprintln!("{var}: {error}"),
        }
    }
    if editions == 0 {
        eprintln!("no game data: set any of {}", VARS.join(", "));
        return ExitCode::FAILURE;
    }
    for (target, count) in &corpus.written {
        println!("{target:>10}: {count} seeds");
    }
    for (target, count) in &corpus.broken {
        eprintln!("{target:>10}: {count} cut-down seeds no longer decode");
    }
    ExitCode::SUCCESS
}

/// Seed files written so far, counted per target.
struct Corpus {
    root: PathBuf,
    written: BTreeMap<&'static str, usize>,
    broken: BTreeMap<&'static str, usize>,
}

impl Corpus {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            written: BTreeMap::new(),
            broken: BTreeMap::new(),
        }
    }

    fn add(&mut self, target: &'static str, bytes: &[u8]) -> Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_SEED {
            return Ok(());
        }
        let dir = self.root.join(target);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{:016x}", fnv(bytes)));
        if !path.exists() {
            std::fs::write(path, bytes)?;
            *self.written.entry(target).or_default() += 1;
        }
        Ok(())
    }

    /// Notes a cut-down seed that its parser now rejects, which would make it a poor seed.
    fn check(&mut self, target: &'static str, decodes: bool) {
        if !decodes {
            *self.broken.entry(target).or_default() += 1;
        }
    }

    /// Adds files joined with the length-prefixed framing `s2_fuzz::Parts` reads.
    fn add_parts<'a>(
        &mut self,
        target: &'static str,
        prefix: &[u8],
        parts: impl IntoIterator<Item = &'a [u8]>,
    ) -> Result<()> {
        let mut out = prefix.to_vec();
        for part in parts {
            out.extend(u32::try_from(part.len())?.to_le_bytes());
            out.extend(part);
        }
        self.add(target, &out)
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn walk(data: &DataSet, dir: &str, files: &mut Vec<String>) -> Result<()> {
    for entry in data.list_dir(dir)? {
        let path = if dir.is_empty() {
            entry.name.to_ascii_uppercase()
        } else {
            format!("{dir}/{}", entry.name.to_ascii_uppercase())
        };
        match entry.kind {
            EntryKind::Directory => walk(data, &path, files)?,
            EntryKind::File => files.push(path),
        }
    }
    Ok(())
}

fn seed(data: &DataSet, corpus: &mut Corpus) -> Result<()> {
    let mut files = Vec::new();
    walk(data, "", &mut files)?;
    for path in &files {
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
        let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
        let read = || data.read(path);
        match ext {
            "LST" => lst(&read()?, corpus)?,
            "FNT" => corpus.add("lst", &read()?)?,
            "IDX" => {
                let dat = format!("{}.DAT", path.trim_end_matches(".IDX"));
                if data.exists(&dat) {
                    idx(&read()?, &data.read(&dat)?, corpus)?;
                }
            }
            "BOB" => corpus.add("bob", &read()?)?,
            "LBM" | "BBM" => corpus.add("iff", &read()?)?,
            "WLD" | "SWD" => wld(&read()?, corpus)?,
            "ENG" | "TXT" => corpus.add("text", &read()?)?,
            "RTX" => corpus.add("rtx", &read()?)?,
            "OPL" | "AD" => corpus.add("music", &read()?)?,
            "SMK" => smacker(data, path, corpus)?,
            "DAT" if dir == "SAVE" => save(data, name, corpus)?,
            "DAT" if name == "ROM_ANIM.DAT" => corpus.add("bob", &read()?)?,
            "DAT" if name.starts_with("GOU") => corpus.add("iff", &read()?)?,
            "DAT" if dir.ends_with("/SNG") => corpus.add("music", &read()?)?,
            "DAT" if name.starts_with("WORLD") => world(data, dir, name, corpus)?,
            _ => {}
        }
    }
    Ok(())
}

/// The whole archive, each slot as a one-slot archive, and each item as a tagged item. Sound
/// items that hold XMIDI also seed the music target.
fn lst(data: &[u8], corpus: &mut Corpus) -> Result<()> {
    corpus.add("lst", data)?;
    for slot in shrink::lst_slots(data) {
        corpus.check("lst", Archive::parse(&slot.archive).is_ok());
        corpus.add("lst", &slot.archive)?;
        corpus.add("lst", &slot.tagged)?;
    }
    if let Ok(archive) = Archive::parse(data) {
        for item in &archive {
            if let Item::Sound(sound) = item
                && sound.bytes().starts_with(b"FORM")
            {
                corpus.add("music", sound.bytes())?;
            }
        }
    }
    Ok(())
}

/// The whole pair, and one single-entry pair (plus its tagged item) per distinct offset.
fn idx(idx: &[u8], dat: &[u8], corpus: &mut Corpus) -> Result<()> {
    corpus.add_parts("idx", &[], [idx, dat])?;
    for (entry, item) in shrink::idx_entries(idx, dat) {
        corpus.check("idx", Index::parse(&entry, item).is_ok());
        corpus.add_parts("idx", &[], [&entry[..], item])?;
        corpus.add("lst", item)?;
    }
    Ok(())
}

/// A cropped map in the tagged and compact layouts, plus its packed layers for the RLE target.
fn wld(data: &[u8], corpus: &mut Corpus) -> Result<()> {
    let Ok(map) = Map::parse_wld(data) else {
        return Ok(());
    };
    for compact in [false, true] {
        let seed = shrink::wld(data, &map, compact);
        corpus.check(
            "map_wld",
            Map::parse_wld(&seed).is_ok_and(|m| m.quirks().is_empty()),
        );
        corpus.add("map_wld", &seed)?;
    }
    for layer in shrink::packed_layers(&map) {
        let expected = usize::from(layer[0]) | usize::from(layer[1]) << 8 | usize::from(layer[2]) << 16;
        corpus.check("map_rle", rle::decode(&layer[3..], expected).is_ok());
        corpus.add("map_rle", &layer)?;
    }
    Ok(())
}

/// A cropped `WORLDnnn.DAT` with its `CONTInnn.DAT`, from the map folders.
fn world(data: &DataSet, dir: &str, name: &str, corpus: &mut Corpus) -> Result<()> {
    let conti = format!("{dir}/{}", name.replacen("WORLD", "CONTI", 1));
    if !data.exists(&conti) {
        return Ok(());
    }
    let (world, conti) = (data.read(&format!("{dir}/{name}"))?, data.read(&conti)?);
    if let Ok(map) = Map::parse_v102(&world, &conti) {
        let seed = shrink::container(&map);
        corpus.check("map_v102", Map::parse_v102(&seed, &conti).is_ok());
        corpus.add_parts("map_v102", &[], [&seed[..], &conti])?;
    }
    Ok(())
}

/// Every save file for the single-file target, and each whole slot, cropped, for the slot
/// target. A slot is assembled once, from its world file.
fn save(data: &DataSet, name: &str, corpus: &mut Corpus) -> Result<()> {
    let bytes = data.read(&format!("SAVE/{name}"))?;
    corpus.add("save_meta", &bytes)?;
    let Some(slot) = name
        .strip_prefix(Part::World.prefix())
        .and_then(|rest| rest.strip_suffix(".DAT"))
        .and_then(|digits| digits.parse::<u16>().ok())
    else {
        return Ok(());
    };
    let read = |part: Part| {
        let path = format!("SAVE/{}", part.file_name(slot));
        data.exists(&path).then(|| data.read(&path)).transpose()
    };
    let Some(areas) = read(Part::Areas)? else {
        return Ok(());
    };
    let Ok(map) = Map::parse_v102(&bytes, &areas) else {
        return Ok(());
    };
    let world = shrink::container(&map);
    let mut optional: [Option<Vec<u8>>; 5] = Default::default();
    for (bytes, part) in optional.iter_mut().zip(OPTIONAL_PARTS) {
        *bytes = read(part)?.map(|b| shrink::save_part(part, b));
    }
    let [transport, control, desktop, mails, computer] = optional.each_ref().map(Option::as_deref);
    let files = SlotFiles {
        world: &world,
        areas: &areas,
        transport,
        control,
        desktop,
        mails,
        computer,
    };
    corpus.check("save_slot", Slot::parse(&files).is_ok());
    let present = (0..)
        .zip(&optional)
        .fold(0u8, |mask, (bit, bytes)| mask | u8::from(bytes.is_some()) << bit);
    let parts = [&world[..], &areas[..]]
        .into_iter()
        .chain(optional.iter().flatten().map(Vec::as_slice));
    corpus.add_parts("save_slot", &[present], parts)
}

/// A few short runs of frames spread through the video.
fn smacker(data: &DataSet, path: &str, corpus: &mut Corpus) -> Result<()> {
    for seed in shrink::smacker(&data.read(path)?) {
        corpus.check("smacker", decodes(&seed));
        corpus.add("smacker", &seed)?;
    }
    Ok(())
}

fn decodes(smacker: &[u8]) -> bool {
    let Ok(video) = Video::parse(smacker) else {
        return false;
    };
    let mut decoder = video.decoder();
    loop {
        match decoder.decode_next() {
            Ok(Some(_)) => {}
            Ok(None) => return true,
            Err(_) => return false,
        }
    }
}
