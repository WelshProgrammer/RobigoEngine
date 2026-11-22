//! End-to-end tests of `DataSet` over synthetic folders, images and cue sheets.

use std::io::{Read, Seek, SeekFrom};

use crate::testutil::{TempDir, audio_sectors, iso, to_raw_mode1};
use crate::{DataSet, EntryKind, Error, Layer, LayerKind, Release};

const ORIGINAL_EXE: &[u8] = b"MZ\0The Settlers II. V1.03. Copyright BlueByte 1995-96.\0";
const PATCHED_EXE: &[u8] = b"MZ\0The Settlers II Mission CD. V1.51. Copyright BlueByte 1996.\0";
const GOLD_EXE: &[u8] = b"MZ\0The Settlers II Mission CD. V1.51. Copyright BlueByte 1996.\0africa.wld\0";

fn video() -> Vec<u8> {
    (0..9000u32).flat_map(u32::to_le_bytes).collect()
}

fn retail_cd() -> Vec<u8> {
    let video = video();
    iso(&[
        ("AUTORUN.INF", b"[autorun]"),
        ("S2/ROOT/S2.EXE", ORIGINAL_EXE),
        ("S2/ROOT/START.BAT", b"s2"),
        ("S2/SETUP.EXE", b"setup"),
        ("S2/DATA/TXT/NAMES.ENG", b"cd names"),
        ("S2/DATA/RESOURCE.IDX", b"resource"),
        ("S2/DATA/TEXTURES/GOURAUD0.DAT", b"old gouraud"),
        ("S2/SAVE/EMPTY", b""),
        ("S2/VIDEO/INTRO.SMK", &video),
    ])
}

fn names(entries: &[crate::DirEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

fn installed(dir: &TempDir, exe: &[u8]) {
    dir.write("S2.EXE", exe);
    dir.write("DATA/TXT/NAMES.ENG", b"names");
    dir.write("DATA/RESOURCE.IDX", b"resource");
    dir.write("Update.txt", b"notes");
}

#[test]
fn folder_lookups_ignore_case_and_separators() {
    let dir = TempDir::new();
    installed(&dir, PATCHED_EXE);
    let data = DataSet::discover(dir.path()).unwrap();
    assert_eq!(data.layers()[0].kind(), LayerKind::Folder);
    assert_eq!(data.read("data\\txt\\names.eng").unwrap(), b"names");
    assert_eq!(data.read("/Data/Txt/NAMES.ENG").unwrap(), b"names");
    assert_eq!(data.read("UPDATE.TXT").unwrap(), b"notes");
    assert_eq!(data.file_size("data/resource.idx").unwrap(), 8);
    assert!(data.exists("DATA") && data.exists("") && !data.exists("DATA/NOPE"));
    assert_eq!(data.metadata("data").unwrap().kind, EntryKind::Directory);
    assert_eq!(
        names(&data.list_dir("").unwrap()),
        ["DATA", "S2.EXE", "Update.txt"]
    );
    assert!(matches!(data.read("DATA"), Err(Error::NotAFile(p)) if p == "DATA"));
    assert!(matches!(data.list_dir("s2.exe"), Err(Error::NotADirectory(_))));
    assert!(matches!(data.read("data/missing.dat"), Err(Error::NotFound(p)) if p == "DATA/MISSING.DAT"));
    assert!(matches!(data.read("../S2.EXE"), Err(Error::InvalidPath { .. })));
    assert_eq!(data.edition().unwrap().release(), Release::V151);
    assert!(data.cd_audio().is_none());
}

#[test]
fn cd_layout_is_presented_as_installed() {
    let dir = TempDir::new();
    let cooked = retail_cd();
    let raw = to_raw_mode1(&cooked);
    for (name, bytes) in [("s2.iso", &cooked), ("s2.bin", &raw)] {
        let data = DataSet::discover(dir.write(name, bytes)).unwrap();
        assert_eq!(data.layers()[0].kind(), LayerKind::Image);
        assert_eq!(data.read("s2.exe").unwrap(), ORIGINAL_EXE, "{name}");
        assert_eq!(data.read("SETUP.EXE").unwrap(), b"setup");
        assert_eq!(data.read("data/txt/names.eng").unwrap(), b"cd names");
        assert_eq!(data.file_size("SAVE/EMPTY").unwrap(), 0);
        assert!(!data.exists("ROOT/S2.EXE") && !data.exists("ROOT") && !data.exists("AUTORUN.INF"));
        assert_eq!(
            names(&data.list_dir("").unwrap()),
            ["DATA", "S2.EXE", "SAVE", "SETUP.EXE", "START.BAT", "VIDEO"]
        );
        let edition = data.edition().unwrap();
        assert_eq!(
            (edition.release(), edition.version()),
            (Release::Original, "1.03")
        );
    }
}

#[test]
fn streams_large_files_from_raw_images() {
    let dir = TempDir::new();
    let data = DataSet::discover(dir.write("cd.bin", &to_raw_mode1(&retail_cd()))).unwrap();
    let video = video();
    assert_eq!(data.read("VIDEO/INTRO.SMK").unwrap(), video);
    let mut reader = data.open("video\\intro.smk").unwrap();
    assert_eq!(reader.len(), video.len() as u64);
    let mut buf = [0; 100];
    reader.seek(SeekFrom::Start(4000)).unwrap();
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(buf, video[4000..4100]);
    reader.seek(SeekFrom::End(-50)).unwrap();
    let mut tail = Vec::new();
    reader.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, video[video.len() - 50..]);
}

#[test]
fn later_layers_win() {
    let cd = TempDir::new();
    let patch = TempDir::new();
    let cue = "FILE \"S2.BIN\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n";
    cd.write("s2.bin", &to_raw_mode1(&retail_cd()));
    let sheet = cd.write("s2.cue", cue.as_bytes());
    patch.write("S2.EXE", PATCHED_EXE);
    patch.write("data/textures/Gouraud0.dat", b"new gouraud");
    patch.write("data/TXT/names.ENG", b"patched names");
    patch.write("data/Maps3/OMAP00.WLD", b"map");

    let cd_only = DataSet::discover(&sheet).unwrap();
    assert_eq!(cd_only.edition().unwrap().release(), Release::Original);
    let data = cd_only.with_layer(Layer::folder(patch.path()).unwrap());
    assert_eq!(data.layers().len(), 2);
    assert_eq!(data.read("S2.EXE").unwrap(), PATCHED_EXE);
    assert_eq!(data.read("DATA/TEXTURES/GOURAUD0.DAT").unwrap(), b"new gouraud");
    assert_eq!(data.read("data/txt/names.eng").unwrap(), b"patched names");
    assert_eq!(data.read("DATA/RESOURCE.IDX").unwrap(), b"resource");
    assert_eq!(data.read("DATA/MAPS3/OMAP00.WLD").unwrap(), b"map");
    let textures = data.list_dir("DATA/TEXTURES").unwrap();
    assert_eq!(names(&textures), ["Gouraud0.dat"]);
    assert_eq!(textures[0].len, 11);
    assert_eq!(
        names(&data.list_dir("data").unwrap()),
        ["Maps3", "RESOURCE.IDX", "textures", "TXT"]
    );
    let edition = data.edition().unwrap();
    assert_eq!((edition.release(), edition.version()), (Release::V151, "1.51"));
}

fn gog_folder(dir: &TempDir) {
    installed(dir, GOLD_EXE);
    dir.write("DATA/MAPS2/AFRICA.WLD", b"africa");
    let mut gog = to_raw_mode1(&iso(&[]));
    let data_sectors = gog.len() / 2352;
    gog.extend(audio_sectors(2, 10));
    gog.extend(audio_sectors(3, 4));
    dir.write("SETTLERS2.gog", &gog);
    let ins = format!(
        "FILE \"SETTLERS2.gog\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n\
         TRACK 02 AUDIO\n    INDEX 01 00:00:{data_sectors:02}\n\
         TRACK 03 AUDIO\n    INDEX 01 00:00:{:02}\n",
        data_sectors + 10
    );
    dir.write("SETTLERS2.ins", ins.as_bytes());
}

#[test]
fn gog_folder_picks_up_its_audio() {
    let dir = TempDir::new();
    gog_folder(&dir);
    for start in [
        dir.path().to_owned(),
        dir.path().join("SETTLERS2.ins"),
        dir.path().join("SETTLERS2.gog"),
    ] {
        let data = DataSet::discover(&start).unwrap();
        assert_eq!(data.layers().len(), 1);
        assert_eq!(data.layers()[0].kind(), LayerKind::Folder);
        assert_eq!(data.edition().unwrap().release(), Release::Gold);
        let audio = data.cd_audio().expect("audio from SETTLERS2.ins");
        let table: Vec<_> = audio
            .tracks()
            .iter()
            .map(|t| (t.number(), t.sector_count()))
            .collect();
        assert_eq!(table, [(2, 10), (3, 4)]);
        let mut pcm = Vec::new();
        audio.open_track(3).unwrap().read_to_end(&mut pcm).unwrap();
        assert_eq!(pcm, audio_sectors(3, 4));
    }
}

#[test]
fn folder_of_images_is_searched() {
    let dir = TempDir::new();
    dir.write("disc/Settlers2.bin", &to_raw_mode1(&retail_cd()));
    dir.write(
        "disc/settlers2.cue",
        b"FILE Settlers2.bin BINARY\nTRACK 1 MODE1/2352\nINDEX 1 00:00:00\n",
    );
    let data = DataSet::discover(dir.path().join("disc")).unwrap();
    assert_eq!(data.edition().unwrap().release(), Release::Original);
    assert_eq!(data.cd_audio().unwrap().tracks(), []);
}

#[test]
fn explains_what_is_missing() {
    let dir = TempDir::new();
    dir.write("notes.txt", b"nothing to see");
    let err = DataSet::discover(dir.path()).unwrap_err();
    assert!(matches!(err, Error::GameNotFound(_)));
    assert!(err.to_string().starts_with("no S2.EXE found under"), "{err}");
    let err = DataSet::discover(dir.path().join("notes.txt")).unwrap_err();
    assert!(matches!(err, Error::UnsupportedSource(_)), "{err}");
    let err = DataSet::discover(dir.path().join("absent")).unwrap_err();
    assert!(matches!(err, Error::Io { .. }), "{err}");
    let err = DataSet::discover(dir.write("blank.iso", &vec![0; 40 * 2048])).unwrap_err();
    assert!(matches!(err, Error::Image { .. }), "{err}");
}

#[test]
fn audio_only_image_without_game_is_rejected() {
    let dir = TempDir::new();
    dir.write("x.iso", &iso(&[("README.TXT", b"hi")]));
    assert!(matches!(
        DataSet::discover(dir.path().join("x.iso")),
        Err(Error::GameNotFound(_))
    ));
}
