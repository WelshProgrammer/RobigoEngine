use super::*;
use crate::nodes::NodeData;

fn map() -> NodeMap {
    NodeMap::filled(64, 64, NodeData::default()).unwrap()
}

fn triangles(map: &NodeMap, player: Player) -> usize {
    (0..map.height())
        .flat_map(|y| (0..map.width()).map(move |x| Node { x, y }))
        .map(|node| map.seen(node, player).iter().filter(|&&seen| seen).count())
        .sum()
}

#[test]
fn a_reveal_marks_six_r_squared_triangles_away_from_the_edges() {
    for radius in [1, 2, 3, 4, 6, 12, 20] {
        let mut map = map();
        map.reveal(Node { x: 30, y: 30 }, Player::ALL[0], radius);
        assert_eq!(
            triangles(&map, Player::ALL[0]),
            6 * usize::from(radius).pow(2),
            "radius {radius}"
        );
        assert_eq!(triangles(&map, Player::ALL[1]), 0);
    }
}

#[test]
fn players_past_1_have_no_fog() {
    let mut map = map();
    let player = Player::ALL[2];
    assert!(map.fully_seen(Node { x: 5, y: 5 }, player));
    map.reveal(Node { x: 5, y: 5 }, player, 4);
    assert!(map.get(Node { x: 5, y: 5 }).seen.is_empty());
}

#[test]
fn a_node_shows_when_a_triangle_round_it_is_seen() {
    let mut map = map();
    let node = Node { x: 10, y: 10 };
    let player = Player::ALL[1];
    assert!(!map.visible(node, player));
    let west = node.neighbour(Direction::West, 64, 64);
    let mut data = map.get(west);
    data.seen |= Seen::PLAYER_1_SECOND;
    map.set(west, data);
    assert!(map.visible(node, player));
    assert!(!map.fully_seen(node, player));
}

#[test]
fn a_headquarters_shows_its_owner_twelve_rings_round_it() {
    let game = crate::settlers::tests::game();
    let player = crate::settlers::tests::player();
    assert_eq!(triangles(game.nodes(), player), 6 * 12 * 12);
    assert_eq!(triangles(game.nodes(), Player::ALL[1]), 0);
}
