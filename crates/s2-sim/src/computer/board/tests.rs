use super::*;

const WAITS: [u16; KINDS] = [30, 30, 90, 150, 120, 12, 180, 60, 180];

fn ledger(land: u32, sites: u16) -> Ledger {
    Ledger {
        land,
        sites,
        land_per_site: 80,
        take_waits: WAITS,
    }
}

fn node(x: u16, y: u16) -> Node {
    Node { x, y }
}

#[test]
fn a_kind_holds_ten_requests_at_most() {
    let mut board = RequestBoard::default();
    for n in 0..10 {
        assert_eq!(
            board.add(1, (n, n), 30, false),
            Some(usize::from(n.unsigned_abs()))
        );
    }
    assert_eq!(board.add(1, (0, 0), 30, false), None);
    assert_eq!(board.count(1), 10);
    board.remove_first(1);
    assert_eq!(board.count(1), 9);
    assert_eq!(board.entries()[0], None, "the first entry of the kind goes");
}

#[test]
fn a_grant_takes_the_nearest_request_within_reach_and_adds_the_follow_ons() {
    let mut board = RequestBoard::default();
    board.add(2, (40, 40), 30, false);
    board.add(2, (12, 10), 30, false);
    assert!(board.grant(2, node(10, 10), &ledger(800, 0), 30));
    assert_eq!(board.count(2), 1);
    assert_eq!(
        board.entries()[0].map(|r| r.at),
        Some((40, 40)),
        "the nearer one went"
    );
    assert_eq!(
        board.entries()[1],
        Some(Request {
            kind: 7,
            at: (10, 10)
        }),
        "the coal request takes the freed entry"
    );
    assert_eq!(
        (board.count(7), board.count(5)),
        (1, 1),
        "weapons bring coal and food"
    );
    assert!(
        !board.grant(2, node(40, 40), &ledger(800, 0), 30),
        "the cooldown the grant started"
    );
}

#[test]
fn a_request_out_of_reach_or_short_of_land_is_refused() {
    let mut board = RequestBoard::default();
    board.add(1, (0, 0), 30, false);
    assert!(
        !board.grant(1, node(20, 20), &ledger(800, 0), 30),
        "800 squared is past 625"
    );
    assert!(
        !board.grant(1, node(0, 0), &ledger(800, 10), 30),
        "10 sites on 800 land"
    );
    assert!(board.grant(1, node(0, 0), &ledger(800, 9), 30));
    let mut gold = RequestBoard::default();
    gold.add(4, (0, 0), 30, false);
    assert!(
        gold.grant(4, node(0, 0), &ledger(0, 50), 30),
        "gold is never held back for land"
    );
}

#[test]
fn a_request_for_anywhere_is_taken_at_once() {
    let mut board = RequestBoard::default();
    board.add(6, (5, 5), 30, false);
    board.add(6, (-1, 0), 30, false);
    assert!(board.grant(6, node(100, 100), &ledger(800, 0), 30));
    assert_eq!(board.entries()[1], None);
    assert!(board.entries()[0].is_some());
}
