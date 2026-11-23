use super::{MOST_MESSAGES, Message, MessageId, MessageTypes};
use crate::grid::Node;
use crate::world::{BuildingKind, Player, World};

fn player() -> Player {
    Player::new(0).expect("player 0")
}

fn post(world: &mut World, id: MessageId, node: Option<Node>, step: u32) {
    world.post_message(id, player(), node, Some(BuildingKind::SAWMILL), 0, step);
}

#[test]
fn a_message_takes_the_step_and_the_next_serial() {
    let mut world = World::empty(32, 32);
    let node = Node { x: 4, y: 6 };
    post(&mut world, MessageId::BUILDING_LOST, Some(node), 70);
    post(&mut world, MessageId::BUILDING_TAKEN, None, 71);

    let held = world.messages(player());
    assert!(held.posted());
    assert!(!held.dropped());
    let list = held.list();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].step, 70);
    assert_eq!(list[0].serial, 0);
    assert_eq!(list[0].node, Some(node));
    assert_eq!(list[1].serial, 1);
    assert_eq!(list[1].node, None);
    assert_eq!(list[0].building, Some(BuildingKind::SAWMILL));
}

#[test]
fn a_full_list_drops_its_oldest() {
    let mut world = World::empty(32, 32);
    for step in 0..u32::try_from(MOST_MESSAGES).expect("fits") {
        post(&mut world, MessageId::BUILDING_LOST, None, step);
    }
    assert!(!world.messages(player()).dropped());
    post(&mut world, MessageId::BUILDING_LOST, None, 100);

    let held = world.messages(player());
    assert!(held.dropped());
    assert_eq!(held.list().len(), MOST_MESSAGES);
    assert_eq!(held.list()[0].step, 1, "the first one has gone");
    assert_eq!(held.list()[MOST_MESSAGES - 1].step, 100);
}

#[test]
fn an_under_attack_message_for_a_node_is_posted_once() {
    let mut world = World::empty(32, 32);
    let node = Node { x: 9, y: 9 };
    let other = Node { x: 9, y: 11 };
    post(&mut world, MessageId::UNDER_ATTACK, Some(node), 1);
    post(&mut world, MessageId::UNDER_ATTACK, Some(node), 2);
    assert_eq!(world.messages(player()).list().len(), 1);
    post(&mut world, MessageId::UNDER_ATTACK, Some(other), 3);
    assert_eq!(world.messages(player()).list().len(), 2);
    post(&mut world, MessageId::BUILDING_LOST, Some(node), 4);
    post(&mut world, MessageId::BUILDING_LOST, Some(node), 5);
    assert_eq!(
        world.messages(player()).list().len(),
        4,
        "every other kind stacks up"
    );
}

#[test]
fn each_player_keeps_their_own_list() {
    let mut world = World::empty(32, 32);
    let other = Player::new(2).expect("player 2");
    post(&mut world, MessageId::BUILDING_LOST, None, 1);
    world.post_message(MessageId::LAND_LOST, other, None, None, 0, 2);
    assert_eq!(world.messages(player()).list().len(), 1);
    assert_eq!(world.messages(other).list().len(), 1);
    assert_eq!(world.messages(other).list()[0].id, MessageId::LAND_LOST);
    let third = Player::new(3).expect("player 3");
    assert_eq!(world.messages(third).list(), []);
    assert!(!world.messages(third).posted());
}

#[test]
fn the_type_table_gives_a_category_and_an_effect_up_to_its_last_id() {
    let table = MessageTypes::ORIGINAL;
    assert_eq!(table.get(MessageId(0)).effect, 60);
    assert_eq!(table.get(MessageId::GEOLOGIST).category, 1);
    assert_eq!(table.get(MessageId(7)).effect, 255);
    assert_eq!(table.get(MessageId::BUILDING_TAKEN).effect, 60);
    assert_eq!(table.get(MessageId::UNDER_ATTACK).category, 0);
    let past = table.get(MessageId(25));
    assert_eq!((past.category, past.effect), (0, 0));
}

#[test]
fn a_messages_category_comes_from_the_table() {
    let message = Message {
        id: MessageId::GEOLOGIST,
        owner: player(),
        step: 0,
        serial: 0,
        node: None,
        building: None,
        value: 0,
    };
    assert_eq!(MessageTypes::ORIGINAL.get(message.id).category, 1);
}

#[test]
fn a_find_carries_the_sign_kind_as_its_value() {
    let mut world = World::empty(32, 32);
    let node = Node { x: 3, y: 3 };
    world.post_message(MessageId::GEOLOGIST, player(), Some(node), None, 2, 9);
    let held = world.messages(player()).list();
    assert_eq!(held[0].value, 2);
    assert_eq!(held[0].building, None);
}
