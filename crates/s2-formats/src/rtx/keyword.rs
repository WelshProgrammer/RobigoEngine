//! The 41 RTX keywords as S2.EXE knows them.

macro_rules! keywords {
    ($($variant:ident = $id:literal, $name:literal, $arity:literal;)*) => {
        /// An RTX keyword. Discriminants are the ids S2.EXE's keyword table assigns (1 to 41).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Keyword {
            $(
                #[doc = concat!("`!", $name, "`, ", stringify!($arity), " argument(s).")]
                $variant = $id,
            )*
        }

        impl Keyword {
            /// Every keyword, in id order.
            pub const ALL: [Self; 41] = [$(Self::$variant),*];

            /// The keyword as written after `!`.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            /// How many numbers the game reads after the keyword.
            #[must_use]
            pub const fn arity(self) -> usize {
                match self {
                    $(Self::$variant => $arity,)*
                }
            }

            /// Looks a keyword up by its exact, case-sensitive name.
            #[must_use]
            pub fn from_name(name: &[u8]) -> Option<Self> {
                match name {
                    $(n if n == $name.as_bytes() => Some(Self::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

keywords! {
    LoadMissionTexts = 1, "LOAD_MISSION_TEXTS", 1;
    SetActPlayer = 2, "SET_ACT_PLAYER", 1;
    SetHouse = 3, "SET_HOUSE", 3;
    AddWare = 4, "ADD_WARE", 2;
    AddPeople = 5, "ADD_PEOPLE", 2;
    EnableHouse = 6, "ENABLE_HOUSE", 1;
    EnableAllHouses = 7, "ENABLE_ALL_HOUSES", 0;
    DisableHouse = 8, "DISABLE_HOUSE", 1;
    AddAnimal = 9, "ADD_ANIMAL", 4;
    GlobalAddComputerPlayer = 10, "GLOBAL_ADD_COMPUTER_PLAYER", 2;
    GlobalSetMap = 11, "GLOBAL_SET_MAP", 1;
    GlobalSetBobs = 12, "GLOBAL_SET_BOBS", 1;
    GlobalSetPattern = 13, "GLOBAL_SET_PATTERN", 1;
    GlobalSetPalette = 14, "GLOBAL_SET_PALETTE", 1;
    GlobalSetMapbobs = 15, "GLOBAL_SET_MAPBOBS", 1;
    GlobalSetHqBobs = 16, "GLOBAL_SET_HQ_BOBS", 3;
    GlobalSetComputerBarrier = 17, "GLOBAL_SET_COMPUTER_BARRIER", 3;
    GlobalSetComputerAlliance = 18, "GLOBAL_SET_COMPUTER_ALLIANCE", 2;
    FmMaxPlayer = 19, "FM_MAX_PLAYER", 1;
    FmMaxHuman = 20, "FM_MAX_HUMAN", 1;
    FmHqPos = 21, "FM_HQ_POS", 2;
    FmCharacter = 22, "FM_CHARACTER", 1;
    MetPositionOccupied = 23, "MET_POSITION_OCCUPIED", 7;
    MetPositionExplored = 24, "MET_POSITION_EXPLORED", 7;
    MetClearActEvents = 25, "MET_CLEAR_ACT_EVENTS", 0;
    MetAddAsActEvent = 26, "MET_ADD_AS_ACT_EVENT", 1;
    MetEndEvent = 27, "MET_END_EVENT", 9;
    MetHouseAmount = 28, "MET_HOUSE_AMOUNT", 6;
    MetPeopleAmount = 29, "MET_PEOPLE_AMOUNT", 6;
    MetWareAmount = 30, "MET_WARE_AMOUNT", 6;
    MetLandSize = 31, "MET_LAND_SIZE", 5;
    MetContactToPlayer = 32, "MET_CONTACT_TO_PLAYER", 5;
    MetFoundGold = 33, "MET_FOUND_GOLD", 4;
    MetFoundIron = 34, "MET_FOUND_IRON", 4;
    MetFoundCoal = 35, "MET_FOUND_COAL", 4;
    MetFoundGranite = 36, "MET_FOUND_GRANITE", 4;
    MetFoundWater = 37, "MET_FOUND_WATER", 4;
    MetHouseEnabling = 38, "MET_HOUSE_ENABLING", 7;
    MetDirectEvent = 39, "MET_DIRECT_EVENT", 4;
    MetSetFinalEvent = 40, "MET_SET_FINAL_EVENT", 4;
    MetSetMapElement = 41, "MET_SET_MAP_ELEMENT", 4;
}

/// The most arguments any keyword takes.
pub const MAX_ARITY: usize = 9;

impl Keyword {
    /// The id S2.EXE's keyword table gives this keyword.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }
}

impl std::fmt::Display for Keyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_run_from_one_to_forty_one() {
        for (i, k) in Keyword::ALL.iter().enumerate() {
            assert_eq!(usize::from(k.id()), i + 1);
            assert_eq!(Keyword::from_name(k.name().as_bytes()), Some(*k));
            assert!(k.arity() <= MAX_ARITY);
        }
    }

    #[test]
    fn lookup_is_exact() {
        assert_eq!(Keyword::from_name(b"set_house"), None);
        assert_eq!(Keyword::from_name(b"SET_HOUSES"), None);
        assert_eq!(Keyword::from_name(b""), None);
    }
}
