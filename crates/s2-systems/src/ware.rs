//! A ware type: byte `+0x00` of a ware record, and the index of the ware table at `0x1fa8e0`.

/// The number of transport groups a ware type can fall in.
pub const TRANSPORT_GROUPS: usize = 14;

/// The name each transport group goes by in mods, by group: the wares in it, or what they have
/// in common.
pub const TRANSPORT_GROUP_NAMES: [&str; TRANSPORT_GROUPS] = [
    "wood", "boards", "stones", "food", "water", "beer", "coal", "iron_ore", "gold", "iron", "coins",
    "tools", "weapons", "boats",
];

/// A ware type, 0 to 30. Tools run from tongs to bow.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct WareKind(pub u8);

impl WareKind {
    /// Wood.
    pub const WOOD: Self = Self(0);
    /// Boards.
    pub const BOARDS: Self = Self(1);
    /// Stones.
    pub const STONES: Self = Self(2);
    /// Ham, from the pig farm.
    pub const HAM: Self = Self(3);
    /// Grain.
    pub const GRAIN: Self = Self(4);
    /// Flour.
    pub const FLOUR: Self = Self(5);
    /// Fish.
    pub const FISH: Self = Self(6);
    /// Meat.
    pub const MEAT: Self = Self(7);
    /// Bread.
    pub const BREAD: Self = Self(8);
    /// Water.
    pub const WATER: Self = Self(9);
    /// Beer.
    pub const BEER: Self = Self(10);
    /// Coal.
    pub const COAL: Self = Self(11);
    /// Iron ore.
    pub const IRON_ORE: Self = Self(12);
    /// Gold.
    pub const GOLD: Self = Self(13);
    /// Iron.
    pub const IRON: Self = Self(14);
    /// Coins.
    pub const COINS: Self = Self(15);
    /// Tongs.
    pub const TONGS: Self = Self(16);
    /// Axe.
    pub const AXE: Self = Self(17);
    /// Saw.
    pub const SAW: Self = Self(18);
    /// Pick-axe.
    pub const PICK_AXE: Self = Self(19);
    /// Hammer.
    pub const HAMMER: Self = Self(20);
    /// Shovel.
    pub const SHOVEL: Self = Self(21);
    /// Crucible.
    pub const CRUCIBLE: Self = Self(22);
    /// Rod and line.
    pub const ROD_AND_LINE: Self = Self(23);
    /// Scythe.
    pub const SCYTHE: Self = Self(24);
    /// Cleaver.
    pub const CLEAVER: Self = Self(25);
    /// Rolling pin.
    pub const ROLLING_PIN: Self = Self(26);
    /// Bow.
    pub const BOW: Self = Self(27);
    /// Sword.
    pub const SWORD: Self = Self(28);
    /// Shield.
    pub const SHIELD: Self = Self(29);
    /// Boat.
    pub const BOAT: Self = Self(30);

    /// The construction wares, boards then stones.
    pub const CONSTRUCTION: [Self; 2] = [Self::BOARDS, Self::STONES];

    /// The first tool.
    pub const FIRST_TOOL: Self = Self::TONGS;

    /// The number of tools, tongs to bow.
    pub const TOOLS: usize = 12;

    /// The name each ware type goes by in mods, by type.
    pub const NAMES: [&'static str; Self::BOAT.index() + 1] = [
        "wood",
        "boards",
        "stones",
        "ham",
        "grain",
        "flour",
        "fish",
        "meat",
        "bread",
        "water",
        "beer",
        "coal",
        "iron_ore",
        "gold",
        "iron",
        "coins",
        "tongs",
        "axe",
        "saw",
        "pick_axe",
        "hammer",
        "shovel",
        "crucible",
        "rod_and_line",
        "scythe",
        "cleaver",
        "rolling_pin",
        "bow",
        "sword",
        "shield",
        "boat",
    ];

    /// The ware type named `name` in mods.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let index = Self::NAMES.iter().position(|&known| known == name)?;
        u8::try_from(index).ok().map(Self)
    }

    /// The name the ware type goes by in mods, or `None` for one without a name.
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        Self::NAMES.get(self.index()).copied()
    }

    /// The tool's place from tongs, 0 to 11, if this is a tool.
    #[must_use]
    pub const fn tool(self) -> Option<usize> {
        if self.0 >= Self::TONGS.0 && self.0 <= Self::BOW.0 {
            Some((self.0 - Self::TONGS.0) as usize)
        } else {
            None
        }
    }

    /// The type's row in tables indexed by ware type.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
