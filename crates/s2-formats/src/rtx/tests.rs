use super::*;

fn parse(text: &str) -> Result<Script, Error> {
    Script::parse(text.as_bytes())
}

fn ops(text: &str) -> Vec<Op> {
    parse(text).unwrap().commands().map(|c| c.op).collect()
}

const SAMPLE: &str = "# heading comment\r\n\
    [GLOBAL_MAP_SETTINGS]\r\n\
    !GLOBAL_SET_MAP    7\r\n\
    !FM_HQ_POS\t 12,   34   #2\r\n\
    \r\n\
    [MAPCOMMANDS]\r\n\
    !SET_ACT_PLAYER\t0\r\n\
    !SET_HOUSE 24, 10, 20\r\n\
    #!ADD_WARE 1 1\r\n\
    !ENABLE_ALL_HOUSES\r\n\
    !MET_POSITION_OCCUPIED 3 255 5 6 -1 -1 1\r\n\
    [ENDE]\r\n\
    \x1a";

#[test]
fn reads_sections_and_commands_with_line_numbers() {
    let script = parse(SAMPLE).unwrap();
    let names: Vec<&str> = script.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, [GLOBAL_MAP_SETTINGS, MAPCOMMANDS, "ENDE"]);

    let global = script.global_settings().unwrap();
    assert_eq!(global.line, 2);
    assert_eq!(
        global.commands,
        [
            Command {
                line: 3,
                op: Op::GlobalSetMap { map: 7 }
            },
            Command {
                line: 4,
                op: Op::FmHqPos {
                    pos: Pos { x: 12, y: 34 }
                }
            },
        ]
    );

    let map = script.map_commands().unwrap();
    let lines: Vec<usize> = map.commands.iter().map(|c| c.line).collect();
    assert_eq!(lines, [7, 8, 10, 11]);
    assert_eq!(
        map.commands[3].op,
        Op::MetPositionOccupied(PositionEvent {
            event: 3,
            next: 255,
            pos: Pos { x: 5, y: 6 },
            text: -1,
            text2: -1,
            flag: 1,
        })
    );
    assert!(
        script.section("ENDE").unwrap().commands.is_empty(),
        "{:?}",
        script.section("ENDE").unwrap().commands
    );
}

#[test]
fn arguments_are_digit_runs_separated_by_anything() {
    assert_eq!(
        ops("[S]\n!ADD_ANIMAL 1,2;x3 (4) trailing 99\n"),
        [Op::AddAnimal {
            species: 1,
            amount: 2,
            pos: Pos { x: 3, y: 4 }
        }]
    );
    assert_eq!(
        ops("[S]\n!ADD_WARE 09 -04\n"),
        [Op::AddWare { ware: 9, amount: -4 }]
    );
    assert_eq!(ops("[S]\n!ADD_WARE 1-2\n"), [Op::AddWare { ware: 1, amount: -2 }]);
}

#[test]
fn numbers_wrap_like_the_game() {
    assert_eq!(
        ops("[S]\n!GLOBAL_SET_MAP 4294967297\n"),
        [Op::GlobalSetMap { map: 1 }]
    );
    assert_eq!(
        ops("[S]\n!GLOBAL_SET_MAP 2147483648\n"),
        [Op::GlobalSetMap { map: i32::MIN }]
    );
}

#[test]
fn high_bytes_use_the_masked_class_table() {
    let script = Script::parse(b"[S]\n!GLOBAL_SET_MAP 1\xb5\n").unwrap();
    let op = script.commands().next().unwrap().op;
    assert_eq!(op, Op::GlobalSetMap { map: 10 + 181 - 48 });
    assert!(Script::parse(b"[S]\n\x81\x81!ENABLE_ALL_HOUSES\n").is_ok());
}

#[test]
fn line_ends_follow_the_game() {
    assert_eq!(ops("[S]\n!ENABLE_HOUSE 3\n!ENABLE_HOUSE 4").len(), 1);
    assert_eq!(ops("[S]\r\n!ENABLE_HOUSE 3\r\n\x1a").len(), 1);
    let found = parse("").unwrap().sections;
    assert!(found.is_empty(), "{found:?}");
    let found = parse("[S]").unwrap().sections;
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(
        ops("[S]\n!ENABLE_HOUSE 3\0 garbage\n"),
        [Op::EnableHouse { building: 3 }]
    );
}

#[test]
fn ignored_line_kinds() {
    assert!(parse("[S]\n\"quoted\"\n123\nword\n  \t# indented\n(\n").is_ok());
}

#[test]
fn errors_carry_line_numbers() {
    assert_eq!(
        parse("[S]\n!ADD_WARE 1\n"),
        Err(Error::MissingArgument {
            line: 2,
            keyword: Keyword::AddWare,
            found: 1
        })
    );
    assert_eq!(parse("[S]\n! 1 2\n"), Err(Error::MissingKeyword { line: 2 }));
    assert_eq!(
        parse("[S]\n*\n"),
        Err(Error::UnexpectedLine { line: 2, byte: b'*' })
    );
}

#[test]
fn unknown_keywords_are_skipped() {
    let script = parse("[S]\n\n!NOT_A_KEYWORD 1\n!set_house 1 2 3\n!ENABLE_HOUSE 4\n").unwrap();
    assert_eq!(
        script.commands().map(|c| c.op).collect::<Vec<_>>(),
        [Op::EnableHouse { building: 4 }]
    );
    assert_eq!(
        script.quirks(),
        [
            Quirk::UnknownKeyword {
                line: 3,
                name: "NOT_A_KEYWORD".into()
            },
            Quirk::UnknownKeyword {
                line: 4,
                name: "set_house".into()
            },
        ]
    );
}

#[test]
fn commands_outside_sections_are_skipped() {
    let script = parse("!ADD_WARE 1\n[S]\n!ENABLE_HOUSE 2\n").unwrap();
    assert_eq!(script.commands().count(), 1);
    assert_eq!(script.quirks(), [Quirk::CommandOutsideSection { line: 1 }]);
}

#[test]
fn nameless_headers_close_the_section() {
    let script = parse("[S]\n!ENABLE_HOUSE 1\n[]\n!ENABLE_HOUSE 2\n[T]\n!ENABLE_HOUSE 3\n").unwrap();
    let names: Vec<&str> = script.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["S", "T"]);
    assert_eq!(script.section("S").unwrap().commands.len(), 1);
    assert_eq!(script.section("T").unwrap().commands.len(), 1);
    assert_eq!(
        script.quirks(),
        [
            Quirk::MissingSectionName { line: 3 },
            Quirk::CommandOutsideSection { line: 4 }
        ]
    );
}

#[test]
fn long_lines_are_read_whole() {
    let long = format!("[S]\n!ADD_WARE 1{} 2\n", " ".repeat(MAX_LINE_LEN));
    let script = parse(&long).unwrap();
    assert_eq!(
        script.commands().next().unwrap().op,
        Op::AddWare { ware: 1, amount: 2 }
    );
    assert_eq!(
        script.quirks(),
        [Quirk::LongLine {
            line: 2,
            len: MAX_LINE_LEN + 13
        }]
    );
    assert!(
        parse(SAMPLE).unwrap().quirks().is_empty(),
        "{:?}",
        parse(SAMPLE).unwrap().quirks()
    );
}

#[test]
fn every_keyword_parses_with_its_arity() {
    for k in Keyword::ALL {
        let args: Vec<String> = (1..=k.arity()).map(|n| n.to_string()).collect();
        let text = format!("[S]\n!{} {}\n", k.name(), args.join(" "));
        let script = parse(&text).unwrap();
        assert_eq!(script.commands().next().unwrap().op.keyword(), k);
        if k.arity() > 0 {
            let short = format!("[S]\n!{} {}\n", k.name(), args[..k.arity() - 1].join(" "));
            assert!(matches!(parse(&short), Err(Error::MissingArgument { keyword, .. }) if keyword == k));
        }
    }
}

#[test]
fn arbitrary_bytes_never_panic() {
    const ALPHABET: &[u8] = b"!#[]\r\n 0123456789-,ADD_WARE\x1a\0\xb5";
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..2_000 {
        let len = usize::try_from(state % 300).unwrap();
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            data.push(ALPHABET[usize::try_from(state).unwrap() % ALPHABET.len()]);
        }
        let _ = Script::parse(&data);
    }
}
