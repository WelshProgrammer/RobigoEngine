//! The base mod baked from a real copy of the game gives exactly what reading the copy gives.
//! Runs only when `S2_DATA_PLAIN` or `S2_DATA_GOLD` points at one; no game data lives here, and
//! the bake goes to a scratch folder that is removed afterwards.

use std::collections::BTreeMap;
use std::sync::Arc;

use s2_assets::SpriteSheet;
use s2_audio::{Effects, LAST_EFFECT, Sample};
use std::io::Read;

use s2_audio::{OPL_RATE, Opl3, Score, render_song};
use s2_data::{DataSet, Layer};
use s2_formats::bob::{BodyType, Direction, LINKS_PER_ITEM};
use s2_formats::lst::{Archive, Item, Sound};
use s2_formats::music::{TimbreBank, Xmidi};
use s2_mods::{
    AudioStream, GOLD, ModStack, Quiet, Rendition, SONGS, Sheets, Text, bake, bob_folder, cd_path, decode,
    effects, font_folder, interface_folder, is_current, is_gold, map_path, song_path, sprite_folder,
    tables_hash,
};
use s2_render::ObjectTables;
use s2_render::terrain::TerrainTables;
use s2_sim::{MessageTypes, SimTables};
use s2_ui::Styles;

/// `ROM_ANIM.DAT`'s size in bytes, in S2 and Gold alike.
const ROM_ANIM_SIZE: usize = 38_404;

/// The credits' portraits, and the palette indices six of the base game's are drawn in: a ramp
/// of greys in the palette they were drawn for and of browns in the one the credits show in. The
/// bake draws those six in the shown palette's greys.
const PORTRAITS: &str = "DATA/CREDITS.LST";
const GREY_RAMP: std::ops::RangeInclusive<u8> = 192..=223;

/// Whether `sprite` draws in the grey ramp alone.
fn on_the_grey_ramp(sprite: &s2_assets::Sprite) -> bool {
    sprite.colours().all(|index| GREY_RAMP.contains(&index))
}

/// The data sets to bake: a v1.51 install, the Gold Edition, and the retail CD image with the
/// v1.51 patch over it, whichever the environment points at.
fn datasets() -> Vec<(&'static str, DataSet)> {
    let mut found: Vec<_> = ["S2_DATA_PLAIN", "S2_DATA_GOLD"]
        .into_iter()
        .filter_map(|var| {
            let path = std::env::var_os(var)?;
            Some((var, DataSet::discover(path).unwrap()))
        })
        .collect();
    if let (Some(cue), Some(patch)) = (std::env::var_os("S2_DATA_CUE"), std::env::var_os("S2_DATA_PATCH")) {
        let data = DataSet::discover(cue)
            .unwrap()
            .with_layer(Layer::folder(patch).unwrap());
        found.push(("S2_DATA_CUE", data));
    }
    if found.is_empty() {
        eprintln!(
            "skipped: set S2_DATA_PLAIN, S2_DATA_GOLD or S2_DATA_CUE and S2_DATA_PATCH to run this test"
        );
    }
    found
}

/// A bake's folder, removed once the test is done with it, whether it passed or failed.
struct BakeFolder(std::path::PathBuf);

impl Drop for BakeFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_baked_mods_give_what_the_originals_give() {
    for (var, data) in datasets() {
        let folder =
            BakeFolder(std::env::temp_dir().join(format!("s2-mods-real-{var}-{}", std::process::id())));
        let root = folder.0.clone();
        let _ = std::fs::remove_dir_all(&root);
        assert!(!is_current(&data, &root), "{var}: nothing baked yet");
        bake(&data, &root, &Quiet).unwrap();
        assert!(is_current(&data, &root), "{var}: the bake records what it baked");

        let has_gold = data.exists("DATA/MAPS2/AFRICA.WLD");
        assert_eq!(
            root.join(GOLD).is_dir(),
            has_gold,
            "{var}: a gold mod only from Gold data"
        );
        let order: Vec<String> = if has_gold { vec![GOLD.into()] } else { Vec::new() };
        let stack = ModStack::open(&root, &order).unwrap();
        let base_only = ModStack::open(&root, &[]).unwrap();
        let mods = Mods {
            var,
            data: &data,
            all: Sheets {
                stack: Some(&stack),
                data: &data,
            },
            without_gold: Sheets {
                stack: Some(&base_only),
                data: &data,
            },
            original: Sheets {
                stack: None,
                data: &data,
            },
        };
        mods.effects(&stack);
        mods.sheets(&stack);
        mods.interface(&stack, has_gold);
        mods.maps();
        mods.missions();
        mods.bobs(&stack);
        mods.terrain();
        mods.texts();
        mods.sim();
        mods.looks();
        mods.music(&stack);
        mods.cd(&stack);
        mods.videos();
        mods.pictures(has_gold);
    }
}

/// One baked data set, read three ways: every mod, the base mod alone, and the original files.
struct Mods<'a> {
    var: &'a str,
    data: &'a DataSet,
    all: Sheets<'a>,
    without_gold: Sheets<'a>,
    original: Sheets<'a>,
}

impl Mods<'_> {
    fn pictures(&self, has_gold: bool) {
        let var = self.var;
        for path in [
            "GFX/PICS/SETUP010.LBM",
            "GFX/PICS/SETUP996.LBM",
            "GFX/PICS2/CREDIT00.LBM",
        ] {
            let baked = self.without_gold.backdrop(path).unwrap();
            assert_eq!(
                (baked.width, baked.height),
                (640, 480),
                "{var}: {path} is full-screen"
            );
            assert_eq!(
                baked,
                self.original.backdrop(path).unwrap(),
                "{var}: {path} keeps every pixel and its palette"
            );
        }
        if has_gold {
            let path = "GFX/PICS/SETUP990.LBM";
            assert!(
                self.without_gold.backdrop(path).is_err(),
                "{var}: {path} is Gold's"
            );
            assert_eq!(
                self.all.backdrop(path).unwrap(),
                self.original.backdrop(path).unwrap(),
                "{var}: {path} from the gold mod"
            );
        }
    }

    fn videos(&self) {
        let var = self.var;
        let videos: Vec<String> = self
            .data
            .list_dir("VIDEO")
            .unwrap()
            .into_iter()
            .map(|entry| format!("VIDEO/{}", entry.name))
            .filter(|path| path.to_ascii_uppercase().ends_with(".SMK"))
            .collect();
        assert!(!videos.is_empty(), "{var}: the data has videos");
        for path in videos {
            assert_eq!(
                self.all.video(&path).unwrap(),
                self.original.video(&path).unwrap(),
                "{var}: {path} copied byte for byte"
            );
        }
    }

    fn cd(&self, stack: &ModStack) {
        let var = self.var;
        let Some(audio) = self.data.cd_audio() else {
            assert!(
                stack.file(&cd_path(2)).is_none(),
                "{var}: no disc image, no CD tracks"
            );
            return;
        };
        assert!(!audio.tracks().is_empty(), "{var}: the image has audio tracks");
        for track in audio.tracks() {
            let number = track.number();
            let file = stack
                .file(&cd_path(number))
                .unwrap_or_else(|| panic!("{var}: track {number} baked"));
            let baked = decode(&file, std::fs::read(&file).unwrap()).unwrap();
            assert_eq!(baked.rate, 44_100, "{var}: track {number}'s rate");
            let mut pcm = Vec::new();
            audio.open_track(number).unwrap().read_to_end(&mut pcm).unwrap();
            let original: Vec<[i16; 2]> = pcm
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&[l0, l1, r0, r1]| [i16::from_le_bytes([l0, l1]), i16::from_le_bytes([r0, r1])])
                .collect();
            assert!(
                *baked.frames == *original,
                "{var}: track {number} keeps every sample ({} frames baked, {} on the disc)",
                baked.frames.len(),
                original.len()
            );
        }
    }

    fn music(&self, stack: &ModStack) {
        let var = self.var;
        let bank = TimbreBank::parse(&self.data.read("DRIVERS/SETT_SND.OPL").unwrap()).unwrap();
        for number in 1..=SONGS {
            let file = stack
                .file(&song_path(Rendition::Opl3, number))
                .unwrap_or_else(|| panic!("{var}: song {number} baked"));
            let stream = AudioStream::open(&file).unwrap();
            assert_eq!(
                stream.rate(),
                OPL_RATE,
                "{var}: song {number} at the chip's own rate"
            );
            let repeat = stream.repeat();
            let frames = decode(&file, std::fs::read(&file).unwrap()).unwrap().frames;
            match repeat {
                Some((start, length)) => assert_eq!(
                    start + length,
                    u64::try_from(frames.len()).unwrap(),
                    "{var}: song {number} ends where its repeat does"
                ),
                None => assert!(matches!(number, 7 | 14), "{var}: song {number} loops"),
            }
            if number == 1 {
                let song = Xmidi::parse(&self.data.read("DATA/SOUNDDAT/SNG/SNG_0001.DAT").unwrap())
                    .unwrap()
                    .sequences
                    .remove(0);
                let rendered = render_song(&mut Opl3::new(bank.clone()), Score::new(&song));
                assert!(
                    *frames == *rendered.frames,
                    "{var}: song 1 keeps every sample of its rendering"
                );
                assert_eq!(
                    repeat,
                    rendered.repeat.map(|(start, length)| (
                        u64::try_from(start).unwrap(),
                        u64::try_from(length).unwrap()
                    )),
                    "{var}: song 1's repeat"
                );
            }
        }
    }

    fn looks(&self) {
        let var = self.var;
        assert_eq!(
            self.all.terrain_tables().unwrap(),
            TerrainTables::ORIGINAL,
            "{var}: terrain tables"
        );
        assert_eq!(
            self.all.object_tables().unwrap(),
            ObjectTables::ORIGINAL,
            "{var}: object tables"
        );
        assert_eq!(
            self.all.message_types().unwrap(),
            MessageTypes::ORIGINAL,
            "{var}: message types"
        );
        assert_eq!(
            self.all.styles().unwrap(),
            Styles::ORIGINAL,
            "{var}: window styles"
        );
        assert_eq!(
            self.all.pictures().unwrap(),
            self.original.pictures().unwrap(),
            "{var}: pictures"
        );
    }

    fn sim(&self) {
        let var = self.var;
        let script = self.all.animation_script().unwrap();
        assert_eq!(
            script,
            self.original.animation_script().unwrap(),
            "{var}: the baked animation script is the original file, byte for byte"
        );
        assert_eq!(script.len(), ROM_ANIM_SIZE, "{var}: ROM_ANIM.DAT's size");
        let tables = self.all.sim_tables().unwrap();
        assert_eq!(tables, SimTables::ORIGINAL, "{var}: the baked sim tables");
        assert_eq!(
            tables_hash(&tables).unwrap(),
            tables_hash(&self.original.sim_tables().unwrap()).unwrap(),
            "{var}: the tables hash"
        );
    }

    fn texts(&self) {
        let var = self.var;
        let mut files: Vec<String> = ["DATA/TXT", "DATA/TXT2", "DATA/TXT3", "DATA/ONLINE"]
            .into_iter()
            .flat_map(|folder| {
                self.data
                    .list_dir(folder)
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |entry| format!("{folder}/{}", entry.name.to_ascii_uppercase()))
            })
            .collect();
        files.push("README.TXT".into());
        let (mut tables, mut pages) = (0, 0);
        for path in files {
            let baked = self.all.text(&path).unwrap();
            match (&baked, self.original.text(&path).unwrap()) {
                (Text::Table(mine), Text::Table(theirs)) => {
                    assert_eq!(mine.len(), theirs.len(), "{var}: {path} slots");
                    assert!(mine.iter().eq(theirs.iter()), "{var}: {path} strings");
                    tables += 1;
                }
                (Text::Page(mine), Text::Page(theirs)) => {
                    assert_eq!(mine, &theirs, "{var}: {path}");
                    pages += 1;
                }
                _ => panic!("{var}: {path} changed kind"),
            }
            assert_eq!(
                self.without_gold.text(&path).is_ok(),
                !is_gold(&path),
                "{var}: {path} only with the gold mod when it is Gold's"
            );
        }
        assert!(
            tables >= 13 && pages >= 60,
            "{var}: {tables} tables, {pages} pages"
        );
    }

    fn terrain(&self) {
        let var = self.var;
        let mut landscapes = 0;
        for landscape in s2_assets::Landscape::ALL {
            let Ok(original) = self.original.landscape(landscape) else {
                continue;
            };
            let baked = self.all.landscape(landscape).unwrap();
            assert_eq!(baked.palette(), original.palette(), "{var}: {landscape} palette");
            assert!(
                baked.texture() == original.texture(),
                "{var}: {landscape} terrain sheet"
            );
            assert!(
                baked.shading() == original.shading(),
                "{var}: {landscape} shading table"
            );
            for (name, mine, theirs) in [
                ("shadow", baked.shadow(), original.shadow()),
                ("grey shadow", baked.grey_shadow(), original.grey_shadow()),
                ("work", baked.work(), original.work()),
                ("paused", baked.paused(), original.paused()),
                ("highlight", baked.highlight(), original.highlight()),
                ("disabled", baked.disabled(), original.disabled()),
            ] {
                assert_eq!(mine, theirs, "{var}: {landscape} {name} table");
            }
            landscapes += 1;
        }
        assert_eq!(landscapes, 3, "{var}: every landscape's terrain");
    }

    fn bobs(&self, stack: &ModStack) {
        let var = self.var;
        for path in [s2_assets::CARRIER_BOB, s2_assets::JOBS_BOB] {
            assert!(
                stack.file(&format!("{}/bodies.toml", bob_folder(path))).is_some(),
                "{var}: {path} baked"
            );
            let baked = self.all.bob(path).unwrap();
            let original = self.original.bob(path).unwrap();
            assert_eq!(baked.bodies(), original.bodies(), "{var}: {path} bodies");
            assert_eq!(baked.links(), original.links(), "{var}: {path} links");
            let items = original.links().len() / LINKS_PER_ITEM;
            assert!(items > 10, "{var}: {path} has {items} items");
            let mut drawn = 0;
            for item in 0..items {
                for body in BodyType::ALL {
                    for direction in Direction::ALL {
                        for frame in 0..8 {
                            let mine = baked.parts(item, body, direction, frame);
                            let theirs = original.parts(item, body, direction, frame);
                            assert_eq!(
                                mine.map(|p| (p.body, p.overlay, p.dy)),
                                theirs.map(|p| (p.body, p.overlay, p.dy)),
                                "{var}: {path} item {item} {body:?} {direction:?} frame {frame}"
                            );
                            drawn += usize::from(theirs.is_some());
                        }
                    }
                }
            }
            assert!(drawn > items * 50, "{var}: {path} draws {drawn} figures");
        }
    }

    fn maps(&self) {
        let var = self.var;
        let mut maps = 0;
        for folder in ["DATA/MAPS", "DATA/MAPS2", "DATA/MAPS3", "DATA/MAPS4"] {
            for entry in self.data.list_dir(folder).unwrap_or_default() {
                let path = format!("{folder}/{}", entry.name.to_ascii_uppercase());
                assert!(map_path(&path).is_some(), "{var}: {path} has a place in a mod");
                let original = self.data.read(&path).unwrap();
                assert_eq!(self.all.map_file(&path).unwrap(), original, "{var}: {path}");
                assert_eq!(
                    self.without_gold.has_map_file(&path),
                    !is_gold(&path),
                    "{var}: {path} only with the gold mod when it is Gold's"
                );
                maps += 1;
            }
        }
        assert!(maps >= 20, "{var}: {maps} maps");
        assert!(
            self.without_gold.map_file("DATA/MAPS2/AFRICA.WLD").is_err(),
            "{var}: no World Campaign without the gold mod"
        );
    }

    fn missions(&self) {
        let var = self.var;
        let mut scripts = 0;
        for entry in self.data.list_dir("DATA/MISSIONS").unwrap_or_default() {
            let path = format!("DATA/MISSIONS/{}", entry.name.to_ascii_uppercase());
            let Some(number) = path
                .strip_prefix("DATA/MISSIONS/MIS_")
                .and_then(|rest| rest.strip_suffix(".RTX"))
                .and_then(|digits| digits.parse().ok())
            else {
                continue;
            };
            let original = self.data.read(&path).unwrap();
            assert_eq!(
                self.without_gold.mission_script(number).unwrap(),
                original,
                "{var}: {path}"
            );
            scripts += 1;
        }
        assert!(scripts >= 10, "{var}: {scripts} mission scripts");
    }

    fn effects(&self, stack: &ModStack) {
        let var = self.var;
        let sounds = effects(stack).unwrap();
        assert_eq!(sounds.table, Effects::ORIGINAL, "{var}: the table");
        let archive = Archive::parse(&self.data.read("DATA/SOUNDDAT/SOUND.LST").unwrap()).unwrap();
        let items = archive.into_items();
        for id in 1..=LAST_EFFECT {
            let item = usize::from(Effects::item(id));
            let original = match items.get(item) {
                Some(Item::Sound(Sound::Pcm(pcm))) => Some(Sample::Original(Arc::from(pcm.as_slice()))),
                _ => None,
            };
            assert_eq!(sounds.samples[item], original, "{var}: effect {id}");
        }
    }

    fn sheets(&self, stack: &ModStack) {
        let var = self.var;
        let mut archives = 0;
        for folder in ["DATA", "DATA/MBOB", "DATA/CBOB"] {
            for entry in self.data.list_dir(folder).unwrap_or_default() {
                let name = entry.name.to_ascii_uppercase();
                let is_list = std::path::Path::new(&name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lst"));
                if !is_list || name == "SOUND.LST" {
                    continue;
                }
                let path = format!("{folder}/{name}");
                let original =
                    SpriteSheet::from_archive(&Archive::parse(&self.data.read(&path).unwrap()).unwrap());
                assert!(
                    stack.file(&format!("{}.toml", sprite_folder(&path))).is_some(),
                    "{var}: {path} baked"
                );
                let baked = self.all.sheet(&path).unwrap();
                for slot in 0..original.len().max(baked.len()) {
                    let (was, is) = (original.get(slot), baked.get(slot));
                    match (was, is) {
                        (Some(was), Some(is)) if path == PORTRAITS && on_the_grey_ramp(was) => {
                            assert_eq!(
                                (is.width(), is.height(), is.drawn()),
                                (was.width(), was.height(), was.drawn()),
                                "{var}: {path} item {slot}"
                            );
                            assert!(
                                !is.colours().any(|index| GREY_RAMP.contains(&index)),
                                "{var}: {path} item {slot} is off the ramp"
                            );
                        }
                        _ => assert_eq!(is, was, "{var}: {path} item {slot}"),
                    }
                }
                assert_eq!(
                    self.without_gold.sheet(&path).is_ok(),
                    !is_gold(&path),
                    "{var}: {path} only with the gold mod when it is Gold's"
                );
                archives += 1;
            }
        }
        assert!(archives > 20, "{var}: {archives} archives");
    }

    fn interface(&self, stack: &ModStack, has_gold: bool) {
        let var = self.var;
        let mut pairs = 0;
        for stem in ["DATA/IO/IO", "DATA/RESOURCE", "DATA/IO/EDITIO", "DATA/EDITRES"] {
            if !self.data.exists(&format!("{stem}.IDX")) {
                continue;
            }
            assert!(
                stack.file(&format!("{}.toml", interface_folder(stem))).is_some(),
                "{var}: {stem} baked"
            );
            let from_mod: BTreeMap<_, _> = self.all.named(stem).unwrap().into_iter().collect();
            let from_data: BTreeMap<_, _> = self.original.named(stem).unwrap().into_iter().collect();
            assert!(
                from_data.len() > 50,
                "{var}: {stem} holds {} pictures",
                from_data.len()
            );
            assert_eq!(from_mod, from_data, "{var}: {stem}");
            match self.without_gold.named(stem) {
                Ok(base) => {
                    let names: Vec<_> = base.iter().map(|(name, _)| name.as_str()).collect();
                    assert!(!is_gold(stem), "{var}: {stem} is Gold's");
                    assert!(!names.contains(&"iobob_03"), "{var}: {stem} without Gold's items");
                    assert_eq!(
                        base.len() + 2 * usize::from(has_gold && stem == "DATA/IO/IO"),
                        from_data.len(),
                        "{var}: {stem} base pictures"
                    );
                }
                Err(_) => assert!(is_gold(stem), "{var}: {stem} is in the base mod"),
            }
            pairs += 1;
        }
        assert!(pairs >= 2, "{var}: {pairs} pairs");
        for name in ["font09", "font11", "font14"] {
            assert!(
                stack.file(&format!("{}.toml", font_folder(name))).is_some(),
                "{var}: {name} baked"
            );
            let from_data = self.original.font(name).unwrap();
            assert!(from_data.glyphs.len() > 100, "{var}: {name} glyphs");
            assert_eq!(self.all.font(name).unwrap(), from_data, "{var}: {name}");
        }
    }
}

#[test]
fn the_original_cd_does_not_bake_until_patched() {
    let Some(cue) = std::env::var_os("S2_DATA_CUE") else {
        eprintln!("skipped: set S2_DATA_CUE to run this test");
        return;
    };
    let data = DataSet::discover(cue).unwrap();
    let folder = BakeFolder(std::env::temp_dir().join(format!("s2-mods-real-cue-{}", std::process::id())));
    let root = folder.0.clone();
    let refused = bake(&data, &root, &Quiet).unwrap_err();
    assert!(matches!(refused, s2_mods::Error::Unpatched(_)), "{refused}");
    assert!(refused.to_string().contains("patch it to v1.51"), "{refused}");
    assert!(!root.exists(), "nothing written");
}
