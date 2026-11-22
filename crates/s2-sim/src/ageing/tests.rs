use crate::grid::Node;
use crate::settlers::tests::game;
use crate::world::ObjectWord;

/// The word at `node` each time it changes over `steps` steps, with the step it changed on.
fn changes(word: u16, node: Node, steps: u32) -> Vec<(u32, u16)> {
    let mut game = game();
    game.world.set_object(node, ObjectWord(word).into());
    let mut seen = Vec::new();
    let mut last = word;
    for _ in 0..steps {
        game.step();
        let now = ObjectWord::from(game.world().object(node)).0;
        if now != last {
            seen.push((game.clock().step(), now));
            last = now;
        }
    }
    seen
}

#[test]
fn a_sapling_grows_a_stage_every_512_steps() {
    let node = Node { x: 30, y: 5 };
    let seen = changes(0xc485, node, 2000);
    let words: Vec<u16> = seen.iter().map(|&(_, word)| word).collect();
    assert_eq!(words, [0xc495, 0xc4a5, 0xc4b5]);
    assert_eq!(seen[0].0, 2 * (5 + 1), "row 5 on growth's sixth turn");
    for pair in seen.windows(2) {
        assert_eq!(pair[1].0 - pair[0].0, 512);
    }
}

#[test]
fn a_falling_tree_does_not_grow() {
    let found = changes(0xc48d, Node { x: 30, y: 5 }, 2000);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn fields_ripen_and_signs_fade_on_two_of_three_turns() {
    let node = Node { x: 3, y: 0 };
    let field = changes(0xd000, node, 2000);
    assert_eq!(
        field.iter().map(|&(_, word)| word).collect::<Vec<_>>(),
        [0xd001, 0xd002, 0xd003]
    );
    assert_eq!(field[0].0, 2, "row 0 on the first turn");
    for pair in field.windows(2) {
        assert_eq!(pair[1].0 - pair[0].0, 768);
    }
    let sign = changes(0xd802, node, 3000);
    assert_eq!(
        sign.iter().map(|&(_, word)| word).collect::<Vec<_>>(),
        [0xd801, 0xd800, 0]
    );
}
