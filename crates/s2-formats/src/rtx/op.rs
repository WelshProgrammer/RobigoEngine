//! Typed RTX commands.
//!
//! Every argument keeps the 32-bit value the game's number reader produces. The game narrows
//! most of them to 8 or 16 bits when it uses them. Field names follow the argument's role where
//! the shipped scripts, S2.EXE or rtx2lua (MIT, by kubaau) make it clear. What each command does
//! at run time is not covered here.
//!
//! In the scripts, event ids run 0 to 99, with 255 meaning "none". Text ids index the slots of
//! the `DATA/TXT/MISS_nnn.ENG` table chosen by `!LOAD_MISSION_TEXTS`, with -1 meaning "none".

use super::keyword::{Keyword, MAX_ARITY};

/// A map position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pos {
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
}

/// Arguments shared by `!MET_POSITION_OCCUPIED` and `!MET_POSITION_EXPLORED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PositionEvent {
    /// This event's id.
    pub event: i32,
    /// Event to end when this one fires, or 255.
    pub next: i32,
    /// The watched map position.
    pub pos: Pos,
    /// Text shown when the event fires, or -1.
    pub text: i32,
    /// A second word for the letter, or -1, which S2 keeps on the diary and never reads.
    pub text2: i32,
    /// Seventh argument; 0 or 1 in the shipped scripts, role not yet known.
    pub flag: i32,
}

/// Arguments shared by `!MET_HOUSE_AMOUNT`, `!MET_PEOPLE_AMOUNT` and `!MET_WARE_AMOUNT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AmountEvent {
    /// This event's id.
    pub event: i32,
    /// Event to end when this one fires, or 255.
    pub next: i32,
    /// Building, job or ware type.
    pub kind: i32,
    /// How many are needed.
    pub amount: i32,
    /// Text shown when the event fires, or -1.
    pub text: i32,
    /// A second word for the letter, or -1, which S2 keeps on the diary and never reads.
    pub text2: i32,
}

/// Arguments shared by the `!MET_FOUND_*` events and `!MET_DIRECT_EVENT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimpleEvent {
    /// This event's id.
    pub event: i32,
    /// Event to end when this one fires, or 255.
    pub next: i32,
    /// Text shown when the event fires, or -1.
    pub text: i32,
    /// A second word for the letter, or -1, which S2 keeps on the diary and never reads.
    pub text2: i32,
}

/// A parsed RTX command: one variant per keyword, with its arguments in file order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    /// `!LOAD_MISSION_TEXTS n`: use `DATA/TXT/MISS_nnn.ENG` for this mission's texts.
    LoadMissionTexts {
        /// The `nnn` in the file name.
        number: i32,
    },
    /// `!SET_ACT_PLAYER player`: later setup commands apply to this player.
    SetActPlayer {
        /// Player slot.
        player: i32,
    },
    /// `!SET_HOUSE building x y`.
    SetHouse {
        /// Building type.
        building: i32,
        /// Where it goes.
        pos: Pos,
    },
    /// `!ADD_WARE ware amount`.
    AddWare {
        /// Ware type.
        ware: i32,
        /// How many.
        amount: i32,
    },
    /// `!ADD_PEOPLE job amount`.
    AddPeople {
        /// Job type.
        job: i32,
        /// How many.
        amount: i32,
    },
    /// `!ENABLE_HOUSE building`.
    EnableHouse {
        /// Building type.
        building: i32,
    },
    /// `!ENABLE_ALL_HOUSES`.
    EnableAllHouses,
    /// `!DISABLE_HOUSE building`.
    DisableHouse {
        /// Building type.
        building: i32,
    },
    /// `!ADD_ANIMAL species amount x y`.
    AddAnimal {
        /// Animal type.
        species: i32,
        /// How many.
        amount: i32,
        /// Where they start.
        pos: Pos,
    },
    /// `!GLOBAL_ADD_COMPUTER_PLAYER player character`.
    GlobalAddComputerPlayer {
        /// Player slot.
        player: i32,
        /// AI character (leader portrait).
        character: i32,
    },
    /// `!GLOBAL_SET_MAP map`: which map file to load.
    GlobalSetMap {
        /// Map number.
        map: i32,
    },
    /// `!GLOBAL_SET_BOBS set`: which map object library to use.
    GlobalSetBobs {
        /// Library number.
        set: i32,
    },
    /// `!GLOBAL_SET_PATTERN pattern`.
    GlobalSetPattern {
        /// Pattern number.
        pattern: i32,
    },
    /// `!GLOBAL_SET_PALETTE palette`.
    GlobalSetPalette {
        /// Palette number.
        palette: i32,
    },
    /// `!GLOBAL_SET_MAPBOBS set`.
    GlobalSetMapbobs {
        /// Library number.
        set: i32,
    },
    /// `!GLOBAL_SET_HQ_BOBS a b c`: three numbers whose roles are not yet known.
    GlobalSetHqBobs {
        /// The three arguments in order.
        values: [i32; 3],
    },
    /// `!GLOBAL_SET_COMPUTER_BARRIER radius x y`.
    GlobalSetComputerBarrier {
        /// Radius around the position.
        radius: i32,
        /// Centre.
        pos: Pos,
    },
    /// `!GLOBAL_SET_COMPUTER_ALLIANCE player ally`.
    GlobalSetComputerAlliance {
        /// Player slot.
        player: i32,
        /// The player it allies with.
        ally: i32,
    },
    /// `!FM_MAX_PLAYER count` (free play).
    FmMaxPlayer {
        /// Player count.
        count: i32,
    },
    /// `!FM_MAX_HUMAN count` (free play).
    FmMaxHuman {
        /// Human player count.
        count: i32,
    },
    /// `!FM_HQ_POS x y` (free play): the next headquarters position.
    FmHqPos {
        /// Headquarters position.
        pos: Pos,
    },
    /// `!FM_CHARACTER character` (free play): the next AI character.
    FmCharacter {
        /// AI character.
        character: i32,
    },
    /// `!MET_POSITION_OCCUPIED event next x y text text2 flag`.
    MetPositionOccupied(PositionEvent),
    /// `!MET_POSITION_EXPLORED event next x y text text2 flag`.
    MetPositionExplored(PositionEvent),
    /// `!MET_CLEAR_ACT_EVENTS`.
    MetClearActEvents,
    /// `!MET_ADD_AS_ACT_EVENT event`: makes an event active at the start.
    MetAddAsActEvent {
        /// Event id.
        event: i32,
    },
    /// `!MET_END_EVENT event next times a b c d text text2`.
    MetEndEvent {
        /// This event's id.
        event: i32,
        /// Event to end when this one fires, or 255.
        next: i32,
        /// How many times the event must be ended before it fires.
        times: i32,
        /// Events to activate, 255 for an unused entry.
        activate: [i32; 4],
        /// Text shown when the event fires, or -1.
        text: i32,
        /// A second word for the letter, or -1, which S2 keeps on the diary and never reads.
        text2: i32,
    },
    /// `!MET_HOUSE_AMOUNT event next building amount text text2`.
    MetHouseAmount(AmountEvent),
    /// `!MET_PEOPLE_AMOUNT event next job amount text text2`.
    MetPeopleAmount(AmountEvent),
    /// `!MET_WARE_AMOUNT event next ware amount text text2`.
    MetWareAmount(AmountEvent),
    /// `!MET_LAND_SIZE event next size text text2`.
    MetLandSize {
        /// This event's id.
        event: i32,
        /// Event to end when this one fires, or 255.
        next: i32,
        /// Land size to reach.
        size: i32,
        /// Text shown when the event fires, or -1.
        text: i32,
        /// A second word for the letter, or -1, which S2 keeps on the diary and never reads.
        text2: i32,
    },
    /// `!MET_CONTACT_TO_PLAYER event next player text text2`.
    MetContactToPlayer {
        /// This event's id.
        event: i32,
        /// Event to end when this one fires, or 255.
        next: i32,
        /// The player to meet.
        player: i32,
        /// Text shown when the event fires, or -1.
        text: i32,
        /// A second word for the letter, or -1, which S2 keeps on the diary and never reads.
        text2: i32,
    },
    /// `!MET_FOUND_GOLD event next text text2`.
    MetFoundGold(SimpleEvent),
    /// `!MET_FOUND_IRON event next text text2`.
    MetFoundIron(SimpleEvent),
    /// `!MET_FOUND_COAL event next text text2`.
    MetFoundCoal(SimpleEvent),
    /// `!MET_FOUND_GRANITE event next text text2`.
    MetFoundGranite(SimpleEvent),
    /// `!MET_FOUND_WATER event next text text2`.
    MetFoundWater(SimpleEvent),
    /// `!MET_HOUSE_ENABLING event b1 b2 b3 b4 b5 b6`.
    MetHouseEnabling {
        /// The event that enables them.
        event: i32,
        /// Building types; values above 32 (255 in the scripts) leave an entry unused.
        buildings: [i32; 6],
    },
    /// `!MET_DIRECT_EVENT event next text text2`.
    MetDirectEvent(SimpleEvent),
    /// `!MET_SET_FINAL_EVENT event chapter a b`.
    MetSetFinalEvent {
        /// The event that ends the mission.
        event: i32,
        /// Campaign chapter number, or 255.
        chapter: i32,
        /// The last two arguments; 255 in every shipped script.
        rest: [i32; 2],
    },
    /// `!MET_SET_MAP_ELEMENT event x y element`.
    MetSetMapElement {
        /// The event that triggers the change.
        event: i32,
        /// Where.
        pos: Pos,
        /// Map object to place.
        element: i32,
    },
}

impl Op {
    /// Builds the command for `keyword` from its arguments; entries past its arity are unused.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per keyword; splitting it would only hide the mapping"
    )]
    pub(super) fn from_args(keyword: Keyword, args: [i32; MAX_ARITY]) -> Self {
        use Keyword as K;
        let [p0, p1, p2, p3, p4, p5, p6, p7, p8] = args;
        let position = PositionEvent {
            event: p0,
            next: p1,
            pos: Pos { x: p2, y: p3 },
            text: p4,
            text2: p5,
            flag: p6,
        };
        let amount = AmountEvent {
            event: p0,
            next: p1,
            kind: p2,
            amount: p3,
            text: p4,
            text2: p5,
        };
        let simple = SimpleEvent {
            event: p0,
            next: p1,
            text: p2,
            text2: p3,
        };
        match keyword {
            K::LoadMissionTexts => Self::LoadMissionTexts { number: p0 },
            K::SetActPlayer => Self::SetActPlayer { player: p0 },
            K::SetHouse => Self::SetHouse {
                building: p0,
                pos: Pos { x: p1, y: p2 },
            },
            K::AddWare => Self::AddWare { ware: p0, amount: p1 },
            K::AddPeople => Self::AddPeople { job: p0, amount: p1 },
            K::EnableHouse => Self::EnableHouse { building: p0 },
            K::EnableAllHouses => Self::EnableAllHouses,
            K::DisableHouse => Self::DisableHouse { building: p0 },
            K::AddAnimal => Self::AddAnimal {
                species: p0,
                amount: p1,
                pos: Pos { x: p2, y: p3 },
            },
            K::GlobalAddComputerPlayer => Self::GlobalAddComputerPlayer {
                player: p0,
                character: p1,
            },
            K::GlobalSetMap => Self::GlobalSetMap { map: p0 },
            K::GlobalSetBobs => Self::GlobalSetBobs { set: p0 },
            K::GlobalSetPattern => Self::GlobalSetPattern { pattern: p0 },
            K::GlobalSetPalette => Self::GlobalSetPalette { palette: p0 },
            K::GlobalSetMapbobs => Self::GlobalSetMapbobs { set: p0 },
            K::GlobalSetHqBobs => Self::GlobalSetHqBobs { values: [p0, p1, p2] },
            K::GlobalSetComputerBarrier => Self::GlobalSetComputerBarrier {
                radius: p0,
                pos: Pos { x: p1, y: p2 },
            },
            K::GlobalSetComputerAlliance => Self::GlobalSetComputerAlliance { player: p0, ally: p1 },
            K::FmMaxPlayer => Self::FmMaxPlayer { count: p0 },
            K::FmMaxHuman => Self::FmMaxHuman { count: p0 },
            K::FmHqPos => Self::FmHqPos {
                pos: Pos { x: p0, y: p1 },
            },
            K::FmCharacter => Self::FmCharacter { character: p0 },
            K::MetPositionOccupied => Self::MetPositionOccupied(position),
            K::MetPositionExplored => Self::MetPositionExplored(position),
            K::MetClearActEvents => Self::MetClearActEvents,
            K::MetAddAsActEvent => Self::MetAddAsActEvent { event: p0 },
            K::MetEndEvent => Self::MetEndEvent {
                event: p0,
                next: p1,
                times: p2,
                activate: [p3, p4, p5, p6],
                text: p7,
                text2: p8,
            },
            K::MetHouseAmount => Self::MetHouseAmount(amount),
            K::MetPeopleAmount => Self::MetPeopleAmount(amount),
            K::MetWareAmount => Self::MetWareAmount(amount),
            K::MetLandSize => Self::MetLandSize {
                event: p0,
                next: p1,
                size: p2,
                text: p3,
                text2: p4,
            },
            K::MetContactToPlayer => Self::MetContactToPlayer {
                event: p0,
                next: p1,
                player: p2,
                text: p3,
                text2: p4,
            },
            K::MetFoundGold => Self::MetFoundGold(simple),
            K::MetFoundIron => Self::MetFoundIron(simple),
            K::MetFoundCoal => Self::MetFoundCoal(simple),
            K::MetFoundGranite => Self::MetFoundGranite(simple),
            K::MetFoundWater => Self::MetFoundWater(simple),
            K::MetHouseEnabling => Self::MetHouseEnabling {
                event: p0,
                buildings: [p1, p2, p3, p4, p5, p6],
            },
            K::MetDirectEvent => Self::MetDirectEvent(simple),
            K::MetSetFinalEvent => Self::MetSetFinalEvent {
                event: p0,
                chapter: p1,
                rest: [p2, p3],
            },
            K::MetSetMapElement => Self::MetSetMapElement {
                event: p0,
                pos: Pos { x: p1, y: p2 },
                element: p3,
            },
        }
    }

    /// The keyword this command was written with.
    #[must_use]
    pub const fn keyword(&self) -> Keyword {
        use Keyword as K;
        match self {
            Self::LoadMissionTexts { .. } => K::LoadMissionTexts,
            Self::SetActPlayer { .. } => K::SetActPlayer,
            Self::SetHouse { .. } => K::SetHouse,
            Self::AddWare { .. } => K::AddWare,
            Self::AddPeople { .. } => K::AddPeople,
            Self::EnableHouse { .. } => K::EnableHouse,
            Self::EnableAllHouses => K::EnableAllHouses,
            Self::DisableHouse { .. } => K::DisableHouse,
            Self::AddAnimal { .. } => K::AddAnimal,
            Self::GlobalAddComputerPlayer { .. } => K::GlobalAddComputerPlayer,
            Self::GlobalSetMap { .. } => K::GlobalSetMap,
            Self::GlobalSetBobs { .. } => K::GlobalSetBobs,
            Self::GlobalSetPattern { .. } => K::GlobalSetPattern,
            Self::GlobalSetPalette { .. } => K::GlobalSetPalette,
            Self::GlobalSetMapbobs { .. } => K::GlobalSetMapbobs,
            Self::GlobalSetHqBobs { .. } => K::GlobalSetHqBobs,
            Self::GlobalSetComputerBarrier { .. } => K::GlobalSetComputerBarrier,
            Self::GlobalSetComputerAlliance { .. } => K::GlobalSetComputerAlliance,
            Self::FmMaxPlayer { .. } => K::FmMaxPlayer,
            Self::FmMaxHuman { .. } => K::FmMaxHuman,
            Self::FmHqPos { .. } => K::FmHqPos,
            Self::FmCharacter { .. } => K::FmCharacter,
            Self::MetPositionOccupied(_) => K::MetPositionOccupied,
            Self::MetPositionExplored(_) => K::MetPositionExplored,
            Self::MetClearActEvents => K::MetClearActEvents,
            Self::MetAddAsActEvent { .. } => K::MetAddAsActEvent,
            Self::MetEndEvent { .. } => K::MetEndEvent,
            Self::MetHouseAmount(_) => K::MetHouseAmount,
            Self::MetPeopleAmount(_) => K::MetPeopleAmount,
            Self::MetWareAmount(_) => K::MetWareAmount,
            Self::MetLandSize { .. } => K::MetLandSize,
            Self::MetContactToPlayer { .. } => K::MetContactToPlayer,
            Self::MetFoundGold(_) => K::MetFoundGold,
            Self::MetFoundIron(_) => K::MetFoundIron,
            Self::MetFoundCoal(_) => K::MetFoundCoal,
            Self::MetFoundGranite(_) => K::MetFoundGranite,
            Self::MetFoundWater(_) => K::MetFoundWater,
            Self::MetHouseEnabling { .. } => K::MetHouseEnabling,
            Self::MetDirectEvent(_) => K::MetDirectEvent,
            Self::MetSetFinalEvent { .. } => K::MetSetFinalEvent,
            Self::MetSetMapElement { .. } => K::MetSetMapElement,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_keyword_round_trips() {
        for k in Keyword::ALL {
            assert_eq!(Op::from_args(k, [0; MAX_ARITY]).keyword(), k);
        }
    }

    #[test]
    fn end_event_takes_nine_arguments_in_order() {
        let op = Op::from_args(Keyword::MetEndEvent, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(
            op,
            Op::MetEndEvent {
                event: 1,
                next: 2,
                times: 3,
                activate: [4, 5, 6, 7],
                text: 8,
                text2: 9,
            }
        );
    }
}
