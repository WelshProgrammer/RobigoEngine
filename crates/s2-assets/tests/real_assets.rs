//! Builds every landscape from a player's copy of the game. Each test runs only when its variable
//! is set, and passes with a note otherwise:
//!
//! - `S2_DATA_PLAIN`: an installed, patched S2 folder
//! - `S2_DATA_GOLD`: the GOG Gold Edition folder
//!
//! Only counts and hashes are asserted; no game data lives here.

use std::path::PathBuf;

use s2_assets::{Landscape, LandscapeAssets, Nation, SHIP_SPRITES, SpriteSheet};
use s2_data::DataSet;

fn data(var: &str) -> Option<DataSet> {
    let Some(path) = std::env::var_os(var).map(PathBuf::from) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    Some(DataSet::discover(path).unwrap())
}

fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Hashes of the DAC palette, shading table and texture sheet, then the object sheet's sprite
/// count, drawn pixel count and hash.
type Summary = ([u64; 3], Option<(usize, usize, u64)>);

/// Sprite count, drawn pixels and hash of each landscape's object sheet.
const OBJECTS: [(usize, usize, u64); 3] = [
    (1123, 303_878, 12_526_002_455_218_314_187),
    (1123, 288_498, 6_238_891_791_732_840_417),
    (1123, 300_725, 6_415_063_864_059_440_041),
];

/// Hashes of each landscape's normal and grey shadow tables.
const SHADOWS: [[u64; 2]; 3] = [
    [17_122_230_581_537_701_610, 637_573_891_885_965_816],
    [15_291_858_411_251_259_905, 12_584_481_296_979_532_300],
    [17_933_699_159_306_444_392, 4_756_477_907_389_298_601],
];

/// Palette, shading table and texture sheet hashes per landscape.
const FILES: [[u64; 3]; 3] = [
    [
        16_693_018_962_440_396_318,
        8_523_515_252_507_211_313,
        16_054_164_438_892_955_505,
    ],
    [
        12_573_103_514_975_507_544,
        8_748_624_426_668_391_588,
        12_429_101_482_379_831_463,
    ],
    [
        7_856_296_780_898_781_048,
        18_227_197_458_892_504_834,
        12_446_289_799_267_600_461,
    ],
];

/// Sprite count, drawn pixels and hash of each nation's sheet, plain then winter.
const NATIONS: [[(usize, usize, u64); 2]; 4] = [
    [
        (240, 200_061, 18_318_557_646_136_675_240),
        (244, 201_210, 7_961_181_144_556_515_313),
    ],
    [
        (230, 171_099, 6_731_555_175_890_349_315),
        (235, 171_496, 10_440_825_582_617_205_776),
    ],
    [
        (239, 181_858, 11_701_654_112_535_310_433),
        (238, 182_215, 11_985_747_077_632_703_817),
    ],
    [
        (230, 175_299, 18_439_369_441_739_047_826),
        (231, 173_123, 1_689_841_127_264_536_875),
    ],
];

/// Sprite count, drawn pixels and a hash over every sprite's slot, origin, size and runs.
fn sheet_summary(sheet: &SpriteSheet) -> (usize, usize, u64) {
    let mut bytes = Vec::new();
    for (slot, sprite) in sheet.iter() {
        bytes.extend(u32::try_from(slot).unwrap().to_le_bytes());
        bytes.extend(sprite.origin().x.to_le_bytes());
        bytes.extend(sprite.origin().y.to_le_bytes());
        bytes.extend(sprite.width().to_le_bytes());
        bytes.extend(sprite.height().to_le_bytes());
        for y in 0..sprite.height() {
            for run in sprite.row(y) {
                bytes.extend(run.x.to_le_bytes());
                bytes.extend(run.len.to_le_bytes());
                bytes.push(run.ink as u8);
                bytes.extend(sprite.values(run));
            }
        }
    }
    let drawn = sheet.iter().map(|(_, s)| s.drawn()).sum();
    (sheet.iter().count(), drawn, fnv(bytes))
}

fn summary(assets: &LandscapeAssets) -> Summary {
    let objects = assets.objects().map(sheet_summary);
    let palette = fnv(assets.palette().0.iter().flatten().copied());
    let shading = fnv((0..=u8::MAX).flat_map(|level| *assets.shading().row(level)));
    let texture = fnv(assets.texture().iter().copied());
    ([palette, shading, texture], objects)
}

fn shadows(assets: &LandscapeAssets) -> [u64; 2] {
    [
        fnv(*assets.shadow().as_array()),
        fnv(*assets.grey_shadow().as_array()),
    ]
}

#[test]
fn gold_landscapes() {
    let Some(data) = data("S2_DATA_GOLD") else { return };
    for ((landscape, objects), files) in Landscape::ALL.into_iter().zip(OBJECTS).zip(FILES) {
        let assets = LandscapeAssets::load(&data, landscape).unwrap();
        assert_eq!(summary(&assets), (files, Some(objects)), "{landscape}");
    }
}

/// The shadow tables are built from each landscape's palette; both editions share them.
#[test]
fn shadow_tables() {
    for var in ["S2_DATA_GOLD", "S2_DATA_PLAIN"] {
        let Some(data) = data(var) else { continue };
        let found: Vec<[u64; 2]> = Landscape::ALL
            .into_iter()
            .map(|landscape| shadows(&LandscapeAssets::load(&data, landscape).unwrap()))
            .collect();
        assert_eq!(found, SHADOWS, "{var}");
    }
}

/// Every nation's sheet, plain and winter, from Gold.
#[test]
fn gold_nations() {
    let Some(data) = data("S2_DATA_GOLD") else { return };
    let found: Vec<[(usize, usize, u64); 2]> = Nation::ALL
        .into_iter()
        .map(|nation| {
            [Landscape::Greenland, Landscape::Winter].map(|landscape| {
                sheet_summary(&SpriteSheet::load(&data, nation.sprites_path(landscape)).unwrap())
            })
        })
        .collect();
    assert_eq!(found, NATIONS);
}

/// The base game shares Gold's palettes, tables and sheets but ships no winter object archive.
#[test]
fn plain_landscapes() {
    let Some(data) = data("S2_DATA_PLAIN") else { return };
    for ((landscape, objects), files) in Landscape::ALL.into_iter().zip(OBJECTS).zip(FILES) {
        let assets = LandscapeAssets::load(&data, landscape).unwrap();
        let objects = (landscape != Landscape::Winter).then_some(objects);
        assert_eq!(summary(&assets), (files, objects), "{landscape}");
    }
}

/// The ship sheet: twelve ship pictures at rest and twelve under sail (body and shadow per
/// direction), three site stages with shadows, six cargo pictures and six pennant frames from
/// slot 40. Both editions ship the same file.
#[test]
fn ship_sheet() {
    for var in ["S2_DATA_PLAIN", "S2_DATA_GOLD"] {
        let Some(data) = data(var) else { continue };
        let sheet = SpriteSheet::load(&data, SHIP_SPRITES).unwrap();
        let slots: Vec<usize> = sheet.iter().map(|(slot, _)| slot).collect();
        assert_eq!(slots, (0..36).chain(40..46).collect::<Vec<_>>(), "{var}");
        assert_eq!(
            sheet_summary(&sheet),
            (42, 43_482, 10_045_092_154_497_420_939),
            "{var}"
        );
    }
}
