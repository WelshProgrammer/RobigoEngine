//! What stands on a node, as a typed value. `S2.EXE` packs it into one word per node (slot 5 the
//! high byte, slot 4 the low byte). [`ObjectWord`] is that word, read from map files and compared
//! against the original; [`MapObject`] is what the sim and renderer hold, and the two convert both
//! ways without loss.

use std::hash::{Hash, Hasher};

use crate::packed::Bits;

/// A node's object word as `S2.EXE` keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct ObjectWord(pub u16);

/// The class: 0 nothing or a plain object, 1 a building, 2 a flag, 3 a static object.
const CLASS: Bits = Bits::new(14, 2);
const PLAIN_CLASS: u16 = 0;
const BUILDING_CLASS: u16 = 1;
const FLAG_CLASS: u16 = 2;
const STATIC_CLASS: u16 = 3;

/// A building's or flag's record number.
const RECORD: Bits = Bits::new(0, 14);

/// A static object's group: slot 5 is `0xc0 + 4 × group + set`.
const GROUP: Bits = Bits::new(10, 4);
const TREE_GROUP: u16 = 1;
const DECORATION_GROUP: u16 = 2;
const GRANITE_GROUP: u16 = 3;
const FIELD_GROUP: u16 = 4;
const REMNANT_GROUP: u16 = 5;
const SIGN_GROUP: u16 = 6;
const FIRE_GROUP: u16 = 7;
const REMAINS_GROUP: u16 = 8;
const SHIP_GROUP: u16 = 9;

/// Slot 5's low two bits, which most groups use as a type or a size.
const SET: Bits = Bits::new(8, 2);

const TREE_PHASE: Bits = Bits::new(0, 3);
const TREE_FELLING: Bits = Bits::new(3, 1);
const TREE_STAGE: Bits = Bits::new(4, 2);
const TREE_KIND: Bits = Bits::new(6, 4);

const DECORATION_INDEX: Bits = Bits::new(0, 8);

/// Granite's size and a field's stage.
const AMOUNT: Bits = Bits::new(0, 3);
/// The bits between [`AMOUNT`] and [`SET`] that granite leaves unused.
const AMOUNT_SPARE: Bits = Bits::new(3, 5);

const REMNANT_LOOK: Bits = Bits::new(0, 7);
const REMNANT_AGE: Bits = Bits::new(7, 3);

const SIGN_LIFE: Bits = Bits::new(0, 5);
const SIGN_AMOUNT: Bits = Bits::new(5, 2);
const SIGN_FOUND: Bits = Bits::new(7, 3);

const BURNING_COUNT: Bits = Bits::new(0, 5);
const BURNING_SPARE: Bits = Bits::new(5, 3);

const SHIP_STAGE: Bits = Bits::new(0, 4);
const SHIP_PART: Bits = Bits::new(4, 4);

/// What stands on a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MapObject {
    /// Nothing.
    #[default]
    Nothing,
    /// Class 0 with a non-zero word, drawn as item 59 of the object archive. Never seen in maps.
    Plain(u16),
    /// A building record.
    Building(u16),
    /// A flag record.
    Flag(u16),
    /// A tree.
    Tree(Tree),
    /// A decoration from the decoration table.
    Decoration(Decoration),
    /// Granite, two kinds of rock.
    Granite(Granite),
    /// A farmer's field.
    Field(Field),
    /// A stump, stubble or a skeleton, fading away.
    Remnant(Remnant),
    /// A geologist's sign.
    Sign(Sign),
    /// The fire a burnt-down building leaves.
    Fire(Burning),
    /// What is left when a fire goes out.
    Remains(Burning),
    /// Drawn from the boat archive at partial height (inferred: a ship being built).
    ShipSite(ShipSite),
    /// A static object in a group no routine handles: slot 5 `0xc0`–`0xc3` or `0xe8`–`0xff`.
    /// Holds the whole word.
    Unknown(u16),
}

/// A tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    /// Tree type, 0 to 15.
    pub kind: u8,
    /// Growth stage, 0 to 3; 3 is grown. While falling, how far it has fallen.
    pub stage: u8,
    /// Drawn with the felling bitmaps.
    pub felling: bool,
    /// Offset of its animation phase, 0 to 7.
    pub phase: u8,
}

impl Tree {
    /// The stage of a grown tree.
    pub const GROWN: u8 = 3;

    /// The number of animation phases.
    pub const PHASES: u16 = 8;

    /// A stage 0 sapling of type `kind`.
    #[must_use]
    pub const fn sapling(kind: u8, phase: u8) -> Self {
        Self {
            kind,
            stage: 0,
            felling: false,
            phase,
        }
    }

    /// Whether it is grown and standing (`0x3d814`): what animals and the spawner look for.
    #[must_use]
    pub const fn grown(self) -> bool {
        self.stage == Self::GROWN && !self.felling
    }
}

/// A decoration: slot 4 indexes the table at `0x1fb8f4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Decoration {
    /// Slot 5's low two bits, unused when drawing.
    pub set: u8,
    /// Row of the decoration table.
    pub index: u8,
}

impl Decoration {
    /// Whether it keeps its node clear of flags, buildings and walkers (the table's word `+5`
    /// non-zero). Rows past the table (50) are not valid objects and count as harmless.
    #[must_use]
    pub const fn blocks(self) -> bool {
        matches!(self.index, 11 | 21..=33 | 43..=50)
    }

    /// Whether it goes when a road is built over it (the table's word `+7` non-zero; the same in
    /// Gold).
    #[must_use]
    pub const fn cleared_by_roads(self) -> bool {
        matches!(self.index, 4..=20)
    }
}

/// Granite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Granite {
    /// Rock type, 0 to 3; maps use 0 and 1.
    pub kind: u8,
    /// What is left, 1 to 6 in maps; 0 reads as 7 once taken.
    pub size: u8,
    /// Bits 3 to 7, zero in maps; kept so the word reads back unchanged.
    pub spare: u8,
}

impl Granite {
    /// One piece taken (`0x3d8b0`): the size drops by one, wrapping within its three bits, and the
    /// last piece takes the granite away.
    #[must_use]
    pub const fn take_one(self) -> Option<Self> {
        if self.size == 1 {
            return None;
        }
        Some(Self {
            size: self.size.wrapping_sub(1) % (AMOUNT.max() + 1),
            ..self
        })
    }
}

/// A farmer's field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Field {
    /// Field type, 0 to 3; farmers sow 0 and 1.
    pub kind: u8,
    /// How far it has grown.
    pub stage: FieldStage,
}

/// How far a field has grown: one stage per visit of the ageing pass's third part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FieldStage {
    /// Stage 0, just sown.
    Sown,
    /// Stage 1.
    Sprouting,
    /// Stage 2.
    Growing,
    /// Stage 3, where a farmer harvests and growth stops.
    Ripe,
}

impl FieldStage {
    /// The stage's number, 0 to 3, which also picks its bitmap.
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Stage `stage`. Map data can hold 4 to 7, which `S2.EXE` grows on to 7, wraps to 0 and
    /// grows again before a farmer takes it; those read as ripe here.
    #[must_use]
    pub const fn of(stage: u8) -> Self {
        match stage {
            0 => Self::Sown,
            1 => Self::Sprouting,
            2 => Self::Growing,
            _ => Self::Ripe,
        }
    }

    /// The next stage, or `None` once ripe.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Sown => Some(Self::Sprouting),
            Self::Sprouting => Some(Self::Growing),
            Self::Growing => Some(Self::Ripe),
            Self::Ripe => None,
        }
    }
}

impl Field {
    /// A newly sown field of type `kind`.
    #[must_use]
    pub const fn sown(kind: u8) -> Self {
        Self {
            kind,
            stage: FieldStage::Sown,
        }
    }

    /// Whether a farmer can harvest it.
    #[must_use]
    pub const fn ripe(self) -> bool {
        matches!(self.stage, FieldStage::Ripe)
    }

    /// The stubble harvesting leaves (`0x3d928`), which fades like a stump.
    #[must_use]
    pub const fn harvested(self) -> Remnant {
        Remnant {
            age: Remnant::STUBBLE_AGE,
            look: RemnantLook::Stubble(self.kind),
        }
    }
}

/// A stump, stubble or a skeleton: something that fades away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Remnant {
    /// Visits of the ageing pass left before it changes, 0 to 7.
    pub age: u8,
    /// How it is drawn.
    pub look: RemnantLook,
}

impl Remnant {
    /// The highest age.
    pub const FRESH: u8 = 7;

    /// The age stubble starts at.
    pub const STUBBLE_AGE: u8 = 5;

    /// A felled tree's stump.
    pub const STUMP: Self = Self {
        age: Self::FRESH,
        look: RemnantLook::Stump,
    };

    /// A lost settler's skeleton.
    pub const SKELETON: Self = Self {
        age: Self::FRESH,
        look: RemnantLook::Skeleton,
    };
}

/// How a [`Remnant`] is drawn: item `500 + look` of the map archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RemnantLook {
    /// Look 31.
    Stump,
    /// Look `36 + 5 × kind` for field type `kind`.
    Stubble(u8),
    /// Look 47, a fresh skeleton.
    Skeleton,
    /// Look 48, the skeleton once it has aged out the first time.
    OldSkeleton,
    /// Any other look, 0 to 127.
    Other(u8),
}

impl RemnantLook {
    const STUMP: u8 = 31;
    const STUBBLE: u8 = 36;
    const STUBBLE_STEP: u8 = 5;
    const STUBBLE_KINDS: u8 = 4;
    const SKELETON: u8 = 47;
    const OLD_SKELETON: u8 = 48;

    /// The look's number.
    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::Stump => Self::STUMP,
            Self::Stubble(kind) => Self::STUBBLE + Self::STUBBLE_STEP * kind,
            Self::Skeleton => Self::SKELETON,
            Self::OldSkeleton => Self::OLD_SKELETON,
            Self::Other(look) => look,
        }
    }

    const fn of(look: u8) -> Self {
        let stubble = look.wrapping_sub(Self::STUBBLE);
        match look {
            Self::STUMP => Self::Stump,
            Self::SKELETON => Self::Skeleton,
            Self::OLD_SKELETON => Self::OldSkeleton,
            _ if look >= Self::STUBBLE
                && stubble % Self::STUBBLE_STEP == 0
                && stubble / Self::STUBBLE_STEP < Self::STUBBLE_KINDS =>
            {
                Self::Stubble(stubble / Self::STUBBLE_STEP)
            }
            _ => Self::Other(look),
        }
    }
}

/// A geologist's sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Sign {
    /// What was found, 0 to 7.
    pub found: u8,
    /// How much, 0 to 3.
    pub amount: u8,
    /// Visits of the ageing pass left, 0 to 31.
    pub life: u8,
}

impl Sign {
    /// What a water sign has found.
    pub const WATER: u8 = 4;

    /// What an empty sign has found.
    pub const NOTHING: u8 = 5;

    /// The ageing pass visits a new sign lasts.
    pub const LIFE: u8 = 12;

    /// The largest size a sign shows.
    pub const LARGEST: u8 = 2;

    /// The sign a geologist puts down for find `found` of size `size` (`0x3d9dc`). The size is
    /// capped at [`Self::LARGEST`] and is 0 for water.
    #[must_use]
    pub fn placed(found: u8, size: u8) -> Self {
        let amount = if found == Self::WATER {
            0
        } else {
            size.min(Self::LARGEST)
        };
        Self {
            found,
            amount,
            life: Self::LIFE,
        }
    }
}

/// A fire or its remains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Burning {
    /// Slot 5's low two bits: bit 0 is set for a medium or large building.
    pub set: u8,
    /// Visits of the ageing pass left, 0 to 31.
    pub count: u8,
    /// Bits 5 to 7, zero in the game; kept so the word reads back unchanged.
    pub spare: u8,
}

impl Burning {
    /// The count a fire starts at.
    const FIRE_COUNT: u8 = 5;

    /// The count remains start at.
    const REMAINS_COUNT: u8 = 2;

    /// Slot 5's bit for a medium or large building.
    const LARGE: u8 = 1;

    /// The fire a finished building leaves when it goes.
    #[must_use]
    pub const fn fire(large: bool) -> Self {
        Self {
            set: if large { Self::LARGE } else { 0 },
            count: Self::FIRE_COUNT,
            spare: 0,
        }
    }

    /// The remains this fire leaves when it goes out. Only the size bit carries over.
    #[must_use]
    pub const fn remains(self) -> Self {
        Self {
            set: self.set % (Self::LARGE + 1),
            count: Self::REMAINS_COUNT,
            spare: 0,
        }
    }
}

/// A ship being built: three stages of four parts, one part per shipwright's trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ShipSite {
    /// Slot 5's low two bits.
    pub set: u8,
    /// Parts laid in this stage less one, 0 to 3 in play: the bitmap is drawn (part + 1) / 4 of
    /// its height.
    pub part: u8,
    /// The stage, 0 to 2 in play, which picks the bitmap.
    pub stage: u8,
}

impl ShipSite {
    /// A site with the first part of the first stage laid (`0x4027c`).
    pub const NEW: Self = Self {
        set: 0,
        part: 0,
        stage: 0,
    };

    /// Parts per stage.
    const PARTS: u8 = 4;

    /// Stages before the ship floats.
    const STAGES: u8 = 3;

    /// The site after one more shipwright's trip (`0x402b4`), or `None` when that trip finishes
    /// the ship. The set bits are written as 0.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        let (mut part, mut stage) = (self.part + 1, self.stage);
        if part == Self::PARTS {
            part = 0;
            stage += 1;
        }
        if stage == Self::STAGES {
            return None;
        }
        Some(Self { set: 0, part, stage })
    }
}

/// What one visit of the ageing pass does to an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ageing {
    /// Nothing.
    Unchanged,
    /// It becomes this.
    Becomes(MapObject),
    /// It goes. With `refresh_sites`, the node and its ring are recomputed twice.
    Gone {
        /// Whether the sites around it change.
        refresh_sites: bool,
    },
}

impl MapObject {
    /// What `MET_SET_MAP_ELEMENT` writes for `element` (`0x31d92`): the decoration word with
    /// `element` OR-ed in, so an element past one byte reaches into slot 5.
    #[must_use]
    pub fn element(element: u16) -> Self {
        let ObjectWord(base) = ObjectWord::from(Self::Decoration(Decoration { set: 0, index: 0 }));
        Self::from(ObjectWord(base | element))
    }

    /// The building record, if this is a building.
    #[must_use]
    pub const fn building(self) -> Option<u16> {
        match self {
            Self::Building(record) => Some(record),
            _ => None,
        }
    }

    /// The flag record, if this is a flag.
    #[must_use]
    pub const fn flag(self) -> Option<u16> {
        match self {
            Self::Flag(record) => Some(record),
            _ => None,
        }
    }

    /// Whether this is a flag.
    #[must_use]
    pub const fn is_flag(self) -> bool {
        matches!(self, Self::Flag(_))
    }

    /// The tree, if this is one.
    #[must_use]
    pub const fn tree(self) -> Option<Tree> {
        match self {
            Self::Tree(tree) => Some(tree),
            _ => None,
        }
    }

    /// Whether this is a grown, standing tree.
    #[must_use]
    pub const fn grown_tree(self) -> bool {
        matches!(self, Self::Tree(tree) if tree.grown())
    }

    /// The first part of the ageing pass (`0x3dae8`): a fire counts down, then becomes remains;
    /// remains count down, then go and change the sites around them. A remnant ages, a skeleton
    /// takes its second look and ages again, and then it goes.
    #[must_use]
    pub const fn age(self) -> Ageing {
        match self {
            Self::Fire(fire) | Self::Remains(fire) if fire.count > 0 => {
                let fire = Burning {
                    count: fire.count - 1,
                    ..fire
                };
                Ageing::Becomes(match self {
                    Self::Fire(_) => Self::Fire(fire),
                    _ => Self::Remains(fire),
                })
            }
            Self::Fire(fire) => Ageing::Becomes(Self::Remains(fire.remains())),
            Self::Remains(_) => Ageing::Gone { refresh_sites: true },
            Self::Remnant(remnant) if remnant.age > 0 => Ageing::Becomes(Self::Remnant(Remnant {
                age: remnant.age - 1,
                ..remnant
            })),
            Self::Remnant(Remnant {
                look: RemnantLook::Skeleton,
                ..
            }) => Ageing::Becomes(Self::Remnant(Remnant {
                age: Remnant::FRESH,
                look: RemnantLook::OldSkeleton,
            })),
            Self::Remnant(_) => Ageing::Gone { refresh_sites: false },
            _ => Ageing::Unchanged,
        }
    }

    /// The second part of the ageing pass: a tree below grown that is not falling grows one
    /// stage.
    #[must_use]
    pub const fn grow(self) -> Option<Self> {
        match self {
            Self::Tree(tree) if !tree.felling && tree.stage != Tree::GROWN => Some(Self::Tree(Tree {
                stage: tree.stage + 1,
                ..tree
            })),
            _ => None,
        }
    }

    /// The third part of the ageing pass: a field below ripe ripens one stage, and a sign loses
    /// one of its life, going at 0.
    #[must_use]
    pub const fn ripen_and_fade(self) -> Option<Self> {
        match self {
            Self::Field(field) => match field.stage.next() {
                Some(stage) => Some(Self::Field(Field { stage, ..field })),
                None => None,
            },
            Self::Sign(sign) if sign.life == 0 => Some(Self::Nothing),
            Self::Sign(sign) => Some(Self::Sign(Sign {
                life: sign.life - 1,
                ..sign
            })),
            _ => None,
        }
    }
}

impl Hash for MapObject {
    /// Hashes the word, so state hashes match those of the packed form.
    fn hash<H: Hasher>(&self, state: &mut H) {
        ObjectWord::from(*self).hash(state);
    }
}

impl From<ObjectWord> for MapObject {
    fn from(ObjectWord(word): ObjectWord) -> Self {
        let set = SET.byte(word);
        match CLASS.read(word) {
            PLAIN_CLASS if word == 0 => Self::Nothing,
            PLAIN_CLASS => Self::Plain(word),
            BUILDING_CLASS => Self::Building(RECORD.read(word)),
            FLAG_CLASS => Self::Flag(RECORD.read(word)),
            _ => match GROUP.read(word) {
                TREE_GROUP => Self::Tree(Tree {
                    kind: TREE_KIND.byte(word),
                    stage: TREE_STAGE.byte(word),
                    felling: TREE_FELLING.read(word) == 1,
                    phase: TREE_PHASE.byte(word),
                }),
                DECORATION_GROUP => Self::Decoration(Decoration {
                    set,
                    index: DECORATION_INDEX.byte(word),
                }),
                GRANITE_GROUP => Self::Granite(Granite {
                    kind: set,
                    size: AMOUNT.byte(word),
                    spare: AMOUNT_SPARE.byte(word),
                }),
                FIELD_GROUP => Self::Field(Field {
                    kind: set,
                    stage: FieldStage::of(AMOUNT.byte(word)),
                }),
                REMNANT_GROUP => Self::Remnant(Remnant {
                    age: REMNANT_AGE.byte(word),
                    look: RemnantLook::of(REMNANT_LOOK.byte(word)),
                }),
                SIGN_GROUP => Self::Sign(Sign {
                    found: SIGN_FOUND.byte(word),
                    amount: SIGN_AMOUNT.byte(word),
                    life: SIGN_LIFE.byte(word),
                }),
                FIRE_GROUP | REMAINS_GROUP => {
                    let burning = Burning {
                        set,
                        count: BURNING_COUNT.byte(word),
                        spare: BURNING_SPARE.byte(word),
                    };
                    if GROUP.read(word) == FIRE_GROUP {
                        Self::Fire(burning)
                    } else {
                        Self::Remains(burning)
                    }
                }
                SHIP_GROUP => Self::ShipSite(ShipSite {
                    set,
                    part: SHIP_PART.byte(word),
                    stage: SHIP_STAGE.byte(word),
                }),
                _ => Self::Unknown(word),
            },
        }
    }
}

impl From<MapObject> for ObjectWord {
    fn from(object: MapObject) -> Self {
        let static_object = |group, detail| CLASS.write(STATIC_CLASS) | GROUP.write(group) | detail;
        Self(match object {
            MapObject::Nothing => 0,
            MapObject::Plain(word) | MapObject::Unknown(word) => word,
            MapObject::Building(record) => CLASS.write(BUILDING_CLASS) | RECORD.write(record),
            MapObject::Flag(record) => CLASS.write(FLAG_CLASS) | RECORD.write(record),
            MapObject::Tree(tree) => static_object(
                TREE_GROUP,
                TREE_KIND.put(tree.kind)
                    | TREE_STAGE.put(tree.stage)
                    | TREE_FELLING.write(u16::from(tree.felling))
                    | TREE_PHASE.put(tree.phase),
            ),
            MapObject::Decoration(decoration) => static_object(
                DECORATION_GROUP,
                SET.put(decoration.set) | DECORATION_INDEX.put(decoration.index),
            ),
            MapObject::Granite(granite) => static_object(
                GRANITE_GROUP,
                SET.put(granite.kind) | AMOUNT_SPARE.put(granite.spare) | AMOUNT.put(granite.size),
            ),
            MapObject::Field(field) => {
                static_object(FIELD_GROUP, SET.put(field.kind) | AMOUNT.put(field.stage.index()))
            }
            MapObject::Remnant(remnant) => static_object(
                REMNANT_GROUP,
                REMNANT_AGE.put(remnant.age) | REMNANT_LOOK.put(remnant.look.index()),
            ),
            MapObject::Sign(sign) => static_object(
                SIGN_GROUP,
                SIGN_FOUND.put(sign.found) | SIGN_AMOUNT.put(sign.amount) | SIGN_LIFE.put(sign.life),
            ),
            MapObject::Fire(burning) | MapObject::Remains(burning) => {
                let group = if matches!(object, MapObject::Fire(_)) {
                    FIRE_GROUP
                } else {
                    REMAINS_GROUP
                };
                static_object(
                    group,
                    SET.put(burning.set)
                        | BURNING_SPARE.put(burning.spare)
                        | BURNING_COUNT.put(burning.count),
                )
            }
            MapObject::ShipSite(ship) => static_object(
                SHIP_GROUP,
                SET.put(ship.set) | SHIP_PART.put(ship.part) | SHIP_STAGE.put(ship.stage),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The word `word` reads back as, once a field's unused bits are dropped and its stage is
    /// capped at ripe.
    fn tidied(word: u16) -> u16 {
        if GROUP.read(word) != FIELD_GROUP || CLASS.read(word) != STATIC_CLASS {
            return word;
        }
        let stage = AMOUNT.read(word).min(u16::from(FieldStage::Ripe.index()));
        word - AMOUNT.write(AMOUNT.read(word)) - AMOUNT_SPARE.write(AMOUNT_SPARE.read(word)) + stage
    }

    #[test]
    fn every_word_reads_back_unchanged_but_fields_past_ripe() {
        for word in 0..=u16::MAX {
            let object = MapObject::from(ObjectWord(word));
            assert_eq!(ObjectWord::from(object).0, tidied(word), "{object:?}");
        }
    }

    #[test]
    fn words_from_the_game_decode_to_their_objects() {
        let object = |word| MapObject::from(ObjectWord(word));
        assert_eq!(object(0xd79f), MapObject::Remnant(Remnant::STUMP));
        assert_eq!(object(0xd7af), MapObject::Remnant(Remnant::SKELETON));
        assert_eq!(object(0xdc05), MapObject::Fire(Burning::fire(false)));
        assert_eq!(object(0xdd05), MapObject::Fire(Burning::fire(true)));
        assert_eq!(object(0xe002), MapObject::Remains(Burning::fire(false).remains()));
        assert_eq!(object(0xd6a9), MapObject::Remnant(Field::sown(1).harvested()));
        assert_eq!(object(0x4007), MapObject::Building(7));
        assert_eq!(object(0x8003), MapObject::Flag(3));
        assert_eq!(
            object(0xc432),
            MapObject::Tree(Tree {
                kind: 0,
                stage: 3,
                felling: false,
                phase: 2
            })
        );
    }

    #[test]
    fn a_field_past_stage_three_loads_ripe_and_stays_ripe() {
        let field = MapObject::from(ObjectWord(0xd107));
        let ripe = Field {
            kind: 1,
            stage: FieldStage::Ripe,
        };
        assert_eq!(field, MapObject::Field(ripe));
        assert_eq!(field.ripen_and_fade(), None);
    }

    #[test]
    fn a_skeleton_takes_its_second_look_then_goes() {
        let mut skeleton = MapObject::Remnant(Remnant {
            age: 0,
            ..Remnant::SKELETON
        });
        let Ageing::Becomes(old) = skeleton.age() else {
            panic!("the skeleton went straight away");
        };
        assert_eq!(ObjectWord::from(old), ObjectWord(0xd7b0));
        skeleton = MapObject::Remnant(Remnant {
            age: 0,
            ..Remnant::STUMP
        });
        assert_eq!(skeleton.age(), Ageing::Gone { refresh_sites: false });
    }
}
