//! A mod over the base mod, built from made-up samples and rows in a scratch folder.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use s2_audio::{Effect, Effects, Sample};
use s2_mods::{EFFECTS_TABLE, Error, ModStack, effect_path, effects, write_wav};

/// A fresh scratch folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("s2-mods-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, path: &str, bytes: &[u8]) {
    let file = dir.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, bytes).unwrap();
}

fn manifest(id: &str, depends: &str) -> String {
    format!("id = \"{id}\"\nname = \"{id}\"\nversion = \"1\"\ndepends = [{depends}]\n")
}

#[test]
fn a_mod_replaces_one_sample_and_one_field_and_keeps_the_rest() {
    let root = scratch("override");
    let base = root.join("base");
    write(&base, "mod.toml", manifest("base", "").as_bytes());
    write(
        &base,
        EFFECTS_TABLE,
        b"[1]\nvolume = 60\njitter = 0\nrate = 9000\n\n[2]\nvolume = 70\njitter = 0\nrate = 16000\n",
    );
    write(&base, &effect_path(1), &write_wav(9000, &[1, 2, 3]));
    write(&base, &effect_path(2), &write_wav(16_000, &[4, 5]));
    let louder = root.join("louder");
    write(&louder, "mod.toml", manifest("louder", "\"base\"").as_bytes());
    write(&louder, EFFECTS_TABLE, b"[2]\nvolume = 100\n");
    write(&louder, &effect_path(1), &write_wav(9000, &[9, 9]));

    let stack = ModStack::open(&root, &["louder".into()]).unwrap();
    let sounds = effects(&stack).unwrap();
    assert_eq!(
        sounds.table.get(2),
        Effect {
            volume: 100,
            jitter: 0,
            rate: 16_000,
            playback: None,
        },
        "the mod's volume over the base's rate"
    );
    assert_eq!(sounds.table.get(1).volume, 60, "a row the mod leaves alone");
    let sample = |id| sounds.samples[usize::from(Effects::item(id))].clone();
    let widened = [(9 - 128) << 8; 2].map(|value: i16| [value, value]);
    assert_eq!(
        sample(1),
        Some(Sample::Smooth {
            rate: 9000,
            frames: widened.to_vec().into()
        }),
        "the mod's own sample, which plays smooth"
    );
    assert_eq!(
        sample(2),
        Some(Sample::Original(Arc::from([4, 5].as_slice()))),
        "the base's sample, which plays the original way"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_row_can_keep_a_mods_sample_on_the_original_path_and_refuses_one_that_does_not_fit() {
    let root = scratch("playback");
    let base = root.join("base");
    write(&base, "mod.toml", manifest("base", "").as_bytes());
    write(
        &base,
        EFFECTS_TABLE,
        b"[1]\nvolume = 60\njitter = 0\nrate = 9000\n",
    );
    let crisp = root.join("crisp");
    write(&crisp, "mod.toml", manifest("crisp", "\"base\"").as_bytes());
    write(&crisp, EFFECTS_TABLE, b"[1]\nplayback = \"original\"\n");
    write(&crisp, &effect_path(1), &write_wav(9000, &[9, 9]));
    let stack = ModStack::open(&root, &["crisp".into()]).unwrap();
    assert_eq!(
        effects(&stack).unwrap().samples[usize::from(Effects::item(1))],
        Some(Sample::Original(Arc::from([9, 9].as_slice())))
    );
    let mut wide = write_wav(9000, &[0, 0, 0, 0]);
    wide[34] = 16;
    write(&crisp, &effect_path(1), &wide);
    assert!(matches!(effects(&stack), Err(Error::Wav { .. })));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_mod_before_what_it_needs_is_refused() {
    let root = scratch("order");
    write(&root.join("base"), "mod.toml", manifest("base", "").as_bytes());
    write(&root.join("a"), "mod.toml", manifest("a", "\"b\"").as_bytes());
    write(&root.join("b"), "mod.toml", manifest("b", "").as_bytes());
    let refused = ModStack::open(&root, &["a".into(), "b".into()]).unwrap_err();
    assert!(matches!(refused, Error::Depends { ref id, ref needs } if id == "a" && needs == "b"));
    assert!(ModStack::open(&root, &["b".into(), "a".into()]).is_ok());
    assert!(matches!(
        ModStack::open(&root, &["c".into()]),
        Err(Error::Missing(_))
    ));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_row_the_table_does_not_take_is_named() {
    let root = scratch("row");
    let base = root.join("base");
    write(&base, "mod.toml", manifest("base", "").as_bytes());
    write(
        &base,
        EFFECTS_TABLE,
        b"[3]\nvolume = 60\njitter = 0\nrate = 9000\npitch = 4\n",
    );
    let stack = ModStack::open(&root, &[]).unwrap();
    let refused = effects(&stack).unwrap_err();
    assert!(
        matches!(refused, Error::Row { ref row, .. } if row == "3"),
        "{refused}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_folder_lists_the_files_of_every_mod_once() {
    let root = scratch("list");
    let base = root.join("base");
    write(&base, "mod.toml", manifest("base", "").as_bytes());
    write(&base, "maps/maps3/omap00.wld", b"a");
    write(&base, "maps/maps3/omap01.wld", b"b");
    let more = root.join("more");
    write(&more, "mod.toml", manifest("more", "\"base\"").as_bytes());
    write(&more, "maps/maps3/omap01.wld", b"c");
    write(&more, "maps/maps3/island.wld", b"d");
    std::fs::create_dir_all(more.join("maps/maps3/folder")).unwrap();

    let stack = ModStack::open(&root, &["more".into()]).unwrap();
    assert_eq!(
        stack.list("maps/maps3"),
        ["island.wld", "omap00.wld", "omap01.wld"],
        "sorted, each once, without the folder"
    );
    assert_eq!(stack.list("maps/nowhere"), Vec::<String>::new());
    let data = s2_data::DataSet::new([]);
    let sheets = s2_mods::Sheets {
        stack: Some(&stack),
        data: &data,
    };
    assert_eq!(
        sheets.maps_in("DATA/MAPS3", ".WLD"),
        [
            "DATA/MAPS3/ISLAND.WLD",
            "DATA/MAPS3/OMAP00.WLD",
            "DATA/MAPS3/OMAP01.WLD"
        ]
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_text_is_read_in_the_stacks_language_and_in_english_where_that_lacks_it() {
    let root = scratch("language");
    let base = root.join("base");
    write(&base, "mod.toml", manifest("base", "").as_bytes());
    write(
        &base,
        "texts/en/txt/ongame.toml",
        b"slots = 3\n0 = \"Game menu\"\n2 = \"Quit\"\n",
    );
    write(
        &base,
        "texts/de/txt/ongame.toml",
        b"slots = 3\n0 = \"Spielmen\xc3\xbc\"\n",
    );
    write(&base, "texts/en/txt2/keys.txt", b"keys");
    write(&base, "texts/en/online/help0000.txt", b"help");
    write(&base, "texts/de/online/help0000.txt", b"Hilfe");
    let data = s2_data::DataSet::new([]);
    let strings = |stack: &ModStack| {
        let sheets = s2_mods::Sheets {
            stack: Some(stack),
            data: &data,
        };
        let s2_mods::Text::Table(table) = sheets.text("DATA/TXT/ONGAME.ENG").unwrap() else {
            panic!("a table");
        };
        let page = |path: &str| match sheets.text(path).unwrap() {
            s2_mods::Text::Page(bytes) => bytes,
            s2_mods::Text::Table(_) => panic!("a page"),
        };
        (
            table
                .iter()
                .map(|(slot, bytes)| (slot, bytes.to_vec()))
                .collect::<Vec<_>>(),
            page("DATA/ONLINE/HELP0000.ENG"),
            page("DATA/TXT2/KEYS.ENG"),
        )
    };
    let stack = ModStack::open(&root, &[]).unwrap();
    assert_eq!(
        strings(&stack),
        (
            vec![(0, b"Game menu".to_vec()), (2, b"Quit".to_vec())],
            b"help".to_vec(),
            b"keys".to_vec()
        )
    );
    let german = stack.in_language("de");
    assert_eq!(
        strings(&german),
        (
            vec![(0, b"Spielmen\x81".to_vec()), (2, b"Quit".to_vec())],
            b"Hilfe".to_vec(),
            b"keys".to_vec()
        ),
        "the German slot and page, and the English ones German lacks"
    );
    let _ = std::fs::remove_dir_all(root);
}
