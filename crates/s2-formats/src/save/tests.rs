use super::*;
use crate::map::{AREA_TABLE_LEN, LayerKind};

const W: u16 = 16;
const H: u16 = 16;
const NODES: usize = W as usize * H as usize;
const ESCAPE: u8 = 254;
const MAP_CLASS: u16 = 10_001;
/// The fill byte of memory the game never wrote.
const UNINITIALISED: u8 = 205;

fn le16(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

fn le32(v: usize) -> [u8; 4] {
    u32::try_from(v).unwrap().to_le_bytes()
}

/// A compact world with every layer RLE-packed as one run of its slot number.
fn world() -> Vec<u8> {
    let mut out = MAP_CLASS.to_le_bytes().to_vec();
    out.extend(u32::MAX.to_le_bytes());
    out.extend([0; 4]);
    out.extend(le16(W));
    out.extend(le16(H));
    for kind in LayerKind::STORED {
        let mut packed = vec![ESCAPE];
        let mut left = NODES;
        while left > 0 {
            let run = left.min(128);
            packed.extend([ESCAPE, u8::try_from(run).unwrap(), kind.slot()]);
            left -= run;
        }
        out.extend(le32(packed.len()));
        out.extend(packed);
    }
    out
}

fn areas() -> Vec<u8> {
    let mut out = vec![0; AREA_TABLE_LEN];
    out[..9].copy_from_slice(&[1, 2, 0, 3, 0, 0, 1, 0, 0]);
    out
}

fn transport(width: u16, height: u16) -> Vec<u8> {
    let mut out = TRANSPORT_CLASS.to_le_bytes().to_vec();
    for v in [width, height, 1000, 2000, 1000, 1, 1, 0] {
        out.extend(le16(v));
    }
    out.extend([1, 0, 170, 187]);
    out
}

fn control() -> Vec<u8> {
    let mut out = vec![0; CONTROL_LEN_V10X];
    out[..4].copy_from_slice(&104_652_u32.to_le_bytes());
    out
}

fn desktop() -> Vec<u8> {
    let mut out = vec![0; DESKTOP_LEN];
    out[..4].copy_from_slice(&[1, 0, 2, 0]);
    let window = 4 + 16 * 18;
    for (i, field) in [1_u16, 20, 20, 600, 440].into_iter().enumerate() {
        out[window + 2 * i..window + 2 * i + 2].copy_from_slice(&le16(field));
    }
    out[window + 10..window + 18].fill(UNINITIALISED);
    for (i, word) in out[DESKTOP_LEN - 12..]
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .enumerate()
    {
        word.copy_from_slice(&le32(i + 1));
    }
    out
}

fn mails() -> Vec<u8> {
    let mut out = le32(75).to_vec();
    out.push(1);
    out.extend(le32(1));
    out.extend([7; MAIL_LEN]);
    out.push(1);
    out.extend(le32(0));
    out.extend([0; MAX_PLAYERS - 2]);
    out
}

fn computer(columns: usize, rows: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [columns, rows, columns * rows, 64, 0, 14] {
        out.extend(le32(v));
    }
    out.extend(vec![3; columns * rows * AI_CELL_LEN]);
    out.extend([9; 40]);
    out
}

struct Owned {
    world: Vec<u8>,
    areas: Vec<u8>,
    transport: Vec<u8>,
    control: Vec<u8>,
    desktop: Vec<u8>,
    mails: Vec<u8>,
    computer: Vec<u8>,
}

impl Owned {
    fn new() -> Self {
        Self {
            world: world(),
            areas: areas(),
            transport: transport(W, H),
            control: control(),
            desktop: desktop(),
            mails: mails(),
            computer: computer(2, 2),
        }
    }

    fn files(&self) -> SlotFiles<'_> {
        SlotFiles {
            world: &self.world,
            areas: &self.areas,
            transport: Some(&self.transport),
            control: Some(&self.control),
            desktop: Some(&self.desktop),
            mails: Some(&self.mails),
            computer: Some(&self.computer),
        }
    }
}

fn part_error(err: Error) -> (Part, Error) {
    match err {
        Error::InPart { part, source } => (part, *source),
        other => panic!("expected a part error, got {other:?}"),
    }
}

#[test]
fn full_slot_decodes() {
    let owned = Owned::new();
    let slot = Slot::parse(&owned.files()).unwrap();

    let world = slot.world();
    assert_eq!((world.width(), world.height()), (W, H));
    assert_eq!(world.layers().len(), LayerKind::STORED.len());
    assert_eq!(world.get(LayerKind::Areas, 15, 15), Some(LayerKind::Areas.slot()));
    let found = world.trailer();
    assert!(found.is_empty(), "{found:?}");

    let transport = slot.transport().unwrap();
    assert_eq!((transport.width(), transport.height()), (W, H));
    assert_eq!(transport.pool_sizes(), [1000, 2000]);
    assert_eq!(transport.body().offset(), 10);
    assert_eq!(transport.body().data().len(), owned.transport.len() - 10);

    let control = slot.control().unwrap();
    assert_eq!(control.layout(), ControlLayout::V10x);
    assert_eq!(control.game_time(), 104_652);

    let desktops = slot.desktops().unwrap().desktops();
    assert_eq!(desktops.len(), 1);
    assert_eq!(desktops[0].header(), [1, 2]);
    assert_eq!(desktops[0].trailer(), [1, 2, 3]);
    let open: Vec<_> = desktops[0].windows().iter().filter(|w| w.is_open()).collect();
    assert_eq!(open.len(), 1);
    assert_eq!(
        (open[0].x, open[0].y, open[0].width, open[0].height),
        (20, 20, 600, 440)
    );
    assert_eq!(open[0].extra, [UNINITIALISED; 8]);

    let mails = slot.mails().unwrap();
    assert_eq!(mails.counter(), 75);
    assert_eq!(mails.players().len(), MAX_PLAYERS);
    assert_eq!(mails.players()[0].as_ref().unwrap()[0].bytes, [7; MAIL_LEN]);
    assert_eq!(mails.players()[1].as_deref(), Some(&[][..]));
    assert!(mails.players()[2..].iter().all(Option::is_none));

    let computer = slot.computer().unwrap();
    assert_eq!((computer.grid().columns(), computer.grid().rows()), (2, 2));
    assert_eq!(computer.grid().params(), [64, 0, 14]);
    assert_eq!(computer.grid().cells().offset(), 24);
    assert_eq!(computer.grid().cells().data().len(), 4 * AI_CELL_LEN);
    assert_eq!(computer.players().data(), &[9; 40]);

    let found = slot.missing();
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn absent_parts_are_reported() {
    let owned = Owned::new();
    let files = SlotFiles {
        control: None,
        mails: None,
        ..owned.files()
    };
    let slot = Slot::parse(&files).unwrap();
    assert_eq!(slot.missing(), [Part::Control, Part::Mails]);
}

#[test]
fn errors_name_their_part() {
    let mut owned = Owned::new();
    owned.areas.pop();
    let (part, err) = part_error(Slot::parse(&owned.files()).unwrap_err());
    assert_eq!(part, Part::Areas);
    assert!(matches!(err, Error::Map(map::Error::AreaTableLength(2249))));

    let mut owned = Owned::new();
    owned.world.truncate(30);
    let (part, err) = part_error(Slot::parse(&owned.files()).unwrap_err());
    assert_eq!(part, Part::World);
    assert!(matches!(err, Error::Map(_)));
}

#[test]
fn transport_must_match_the_world() {
    let mut owned = Owned::new();
    owned.transport = transport(W, H * 2);
    let (part, err) = part_error(Slot::parse(&owned.files()).unwrap_err());
    assert_eq!(part, Part::Transport);
    assert_eq!(
        err,
        Error::SizeMismatch {
            found: (16, 32),
            expected: (16, 16)
        }
    );
}

#[test]
fn transport_needs_its_class_tag() {
    let mut data = transport(W, H);
    data[0] = 17;
    assert_eq!(
        Transport::parse(&data),
        Err(Error::UnexpectedClass {
            expected: TRANSPORT_CLASS,
            found: u16::from_le_bytes([17, TRANSPORT_CLASS.to_le_bytes()[1]])
        })
    );
}

#[test]
fn ai_grid_must_match_the_world() {
    let mut owned = Owned::new();
    owned.computer = computer(4, 2);
    let (part, err) = part_error(Slot::parse(&owned.files()).unwrap_err());
    assert_eq!(part, Part::Computer);
    assert!(matches!(err, Error::SizeMismatch { found: (4, 2), .. }));
}

#[test]
fn ai_grid_rejects_inconsistent_or_huge_counts() {
    let mut data = computer(2, 2);
    data[8] = 5;
    assert!(matches!(
        ComputerPlayers::parse(&data),
        Err(Error::AiGrid { cells: 5, .. })
    ));

    let mut huge = Vec::new();
    let side = u32::from(u16::MAX);
    for v in [side, side, side * side, 0, 0, 0] {
        huge.extend(v.to_le_bytes());
    }
    assert!(matches!(ComputerPlayers::parse(&huge), Err(Error::Truncated(_))));

    let mut overflow = Vec::new();
    for v in [1_u32 << 16, 1 << 16, 0, 0, 0, 0] {
        overflow.extend(v.to_le_bytes());
    }
    assert!(matches!(
        ComputerPlayers::parse(&overflow),
        Err(Error::AiGrid { .. })
    ));
}

#[test]
fn control_accepts_any_length_the_game_reads() {
    let v151 = Control::parse(&[0; CONTROL_LEN_V151]).unwrap();
    assert_eq!(
        (v151.layout(), v151.len()),
        (ControlLayout::V151, CONTROL_LEN_V151)
    );
    assert_eq!(v151.quirk(), None);
    let old = Control::parse(&control()).unwrap();
    assert_eq!((old.layout(), old.game_time()), (ControlLayout::V10x, 104_652));

    let odd = Control::parse(&[7, 0, 0, 0, 1]).unwrap();
    assert_eq!(
        (odd.layout(), odd.len(), odd.game_time()),
        (ControlLayout::Other, 5, 7)
    );
    assert_eq!(odd.quirk(), Some(Quirk::ControlLength(5)));
    let empty = Control::parse(&[]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.game_time(), 0);

    let long = vec![0; CONTROL_LEN_V151 + 1];
    assert_eq!(
        Control::parse(&long),
        Err(Error::ControlLength(CONTROL_LEN_V151 + 1))
    );
}

#[test]
fn slot_reports_an_odd_control_length() {
    let mut owned = Owned::new();
    assert!(
        Slot::parse(&owned.files()).unwrap().quirks().is_empty(),
        "{:?}",
        Slot::parse(&owned.files()).unwrap().quirks()
    );
    owned.control.truncate(100);
    let slot = Slot::parse(&owned.files()).unwrap();
    assert_eq!(slot.quirks(), [Quirk::ControlLength(100)]);
    assert_eq!(slot.control().map(Control::game_time), Some(104_652));
}

#[test]
fn desktops_come_in_ones_and_twos() {
    let one = desktop();
    let two = [one.clone(), one.clone()].concat();
    assert_eq!(Desktops::parse(&two).unwrap().desktops().len(), 2);
    for len in [0, DESKTOP_LEN - 1, DESKTOP_LEN + 1, DESKTOP_LEN * 3] {
        let data = vec![0; len];
        assert_eq!(Desktops::parse(&data), Err(Error::DesktopLength(len)));
    }
}

#[test]
fn mailboxes_are_strict() {
    let mut data = mails();
    data[4] = 2;
    assert_eq!(
        Mailboxes::parse(&data),
        Err(Error::MailboxFlag { player: 0, flag: 2 })
    );

    let mut data = mails();
    data.push(0);
    assert_eq!(
        Mailboxes::parse(&data),
        Err(Error::Trailing {
            offset: data.len() - 1,
            len: 1
        })
    );

    let mut data = le32(0).to_vec();
    data.push(1);
    data.extend(u32::MAX.to_le_bytes());
    assert!(matches!(Mailboxes::parse(&data), Err(Error::Truncated(_))));
}

fn directory() -> Vec<u8> {
    let mut out = le32(3).to_vec();
    for slot in 0..DIRECTORY_SLOTS {
        let mut entry = vec![UNINITIALISED; 57];
        entry[..7].fill(0);
        if slot == 1 {
            entry[..7].copy_from_slice(&[1, 0, 4, 3, 0, 0, 0]);
            let name = b"'Hello'-1:05\0";
            entry[7..7 + name.len()].copy_from_slice(name);
            entry[9] = 0x84;
        } else {
            entry[7] = 0;
        }
        out.extend(entry);
    }
    out
}

#[test]
fn directory_decodes() {
    let dir = Directory::parse(&directory()).unwrap();
    assert_eq!(dir.counter(), 3);
    assert_eq!(dir.entries().len(), DIRECTORY_SLOTS);
    let used: Vec<_> = dir.entries().iter().filter(|e| e.is_used()).collect();
    assert_eq!(used.len(), 1);
    let entry = used[0];
    assert_eq!(entry.slot(), 901);
    assert_eq!((entry.kind(), entry.sequence()), (4, 3));
    assert_eq!(entry.name_bytes(), b"'H\x84llo'-1:05");
    assert_eq!(entry.name(), "'Hällo'-1:05");
    assert_eq!(dir.entries()[0].name_bytes(), b"");
    assert_eq!(dir.entries()[10].slot(), 910);
    assert_eq!(
        Directory::parse(&[0; 630]),
        Err(Error::Length {
            len: 630,
            expected: DIRECTORY_LEN
        })
    );
}

#[test]
fn mission_progress_reads_flags() {
    let plain = MissionProgress::parse(b"1100000000\r\n\x1a").unwrap();
    assert_eq!(plain.unlocked()[..3], [true, true, false]);
    assert_eq!(plain.tail(), b"\r\n\x1a");

    let mut gold = b"1111000000\r\n\x1a".to_vec();
    gold.resize(MISSION_BUFFER_LEN, b'A');
    let gold = MissionProgress::parse(&gold).unwrap();
    assert_eq!(gold.unlocked().iter().filter(|&&u| u).count(), 4);

    assert_eq!(
        MissionProgress::parse(b"1102000000"),
        Err(Error::MissionFlag {
            chapter: 3,
            byte: b'2'
        })
    );
    assert_eq!(MissionProgress::parse(b"110"), Err(Error::MissionLength(3)));
}

#[test]
fn mission_progress_drops_bytes_past_the_buffer() {
    let mut edited = b"1111111111\r\n\x1a".to_vec();
    edited.resize(MISSION_BUFFER_LEN, 0);
    edited.extend(b"\r\n");
    let mission = MissionProgress::parse(&edited).unwrap();
    assert!(mission.unlocked().iter().all(|&u| u));
    assert_eq!(mission.bytes(), &edited[..MISSION_BUFFER_LEN]);
    assert_eq!(mission.tail().len(), MISSION_BUFFER_LEN - CAMPAIGN_CHAPTERS);
}

#[test]
fn world_campaign_is_a_fixed_block() {
    let data = [5; WORLD_CAMPAIGN_LEN];
    assert_eq!(WorldCampaignProgress::parse(&data).unwrap().bytes(), &data);
    assert!(WorldCampaignProgress::parse(&data[1..]).is_err());
}

#[test]
fn world_campaign_progress_names_the_won_continents_and_the_chosen_one() {
    let mut data = [0u8; WORLD_CAMPAIGN_LEN];
    data[0] = 1;
    data[2] = 1;
    data[18] = 1;
    let progress = WorldCampaignProgress::parse(&data).unwrap();
    assert_eq!(progress.won()[..3], [true, true, false]);
    assert_eq!(progress.chosen(), 1);
}

#[test]
fn screen_mode_follows_the_game() {
    let mode = ScreenMode::parse(b"2\0").unwrap();
    assert_eq!((mode.mode(), mode.digit(), mode.trailer()), (2, b'2', &[0][..]));
    assert_eq!(
        ScreenMode::parse(b"\0").unwrap().mode(),
        0u16.wrapping_sub(u16::from(b'0'))
    );
    assert!(ScreenMode::parse(b"").is_err());
}

#[test]
fn file_names() {
    assert_eq!(Part::World.file_name(900), "WORLD900.DAT");
    assert_eq!(Part::Areas.file_name(7), "CONTI007.DAT");
    assert_eq!(Part::Computer.to_string(), "COMPY");
}

type Parser = fn(&[u8]) -> bool;

#[test]
fn no_prefix_panics() {
    let owned = Owned::new();
    for len in 0..owned.world.len() {
        let files = SlotFiles {
            world: &owned.world[..len],
            ..owned.files()
        };
        assert!(Slot::parse(&files).is_err());
    }
    let parsers: [(&[u8], Parser); 8] = [
        (&owned.transport, |d| Transport::parse(d).is_ok()),
        (&owned.control, |d| Control::parse(d).is_ok()),
        (&owned.desktop, |d| Desktops::parse(d).is_ok()),
        (&owned.mails, |d| Mailboxes::parse(d).is_ok()),
        (&owned.computer, |d| ComputerPlayers::parse(d).is_ok()),
        (&directory(), |d| Directory::parse(d).is_ok()),
        (b"1100000000\r\n\x1a", |d| MissionProgress::parse(d).is_ok()),
        (b"2\0", |d| ScreenMode::parse(d).is_ok()),
    ];
    for (data, parse) in parsers {
        assert!(parse(data));
        for len in 0..data.len() {
            let _ = parse(&data[..len]);
        }
    }
}
