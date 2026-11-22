//! A settler's job: the figure byte `+0x01`, an index into the job table at `0x1fad97`.

/// A settler's job, 0 to 71. Jobs 0 to 29 are the people a storehouse counts and orders; the
/// entries above them are the same people with another look, such as a woodcutter carrying wood,
/// which the game switches to and back as they work.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct Job(pub u8);

impl Job {
    /// Carries wares on a road.
    pub const HELPER: Self = Self(0);
    /// Fells trees.
    pub const WOODCUTTER: Self = Self(1);
    /// Fishes from the shore; the only job whose paths may leave its building's area.
    pub const FISHER: Self = Self(2);
    /// Plants trees.
    pub const FORESTER: Self = Self(3);
    /// Works the sawmill.
    pub const CARPENTER: Self = Self(4);
    /// Cuts stone from granite.
    pub const STONEMASON: Self = Self(5);
    /// Hunts animals.
    pub const HUNTER: Self = Self(6);
    /// Sows and harvests fields.
    pub const FARMER: Self = Self(7);
    /// Works the mill.
    pub const MILLER: Self = Self(8);
    /// Works the bakery.
    pub const BAKER: Self = Self(9);
    /// Works the slaughterhouse.
    pub const BUTCHER: Self = Self(10);
    /// Works a mine.
    pub const MINER: Self = Self(11);
    /// Works the brewery.
    pub const BREWER: Self = Self(12);
    /// Works the pig farm.
    pub const PIG_BREEDER: Self = Self(13);
    /// Works the donkey breeder.
    pub const DONKEY_BREEDER: Self = Self(14);
    /// Works the iron smelter (15 or 16; which was not checked).
    pub const IRON_FOUNDER: Self = Self(15);
    /// Works the mint.
    pub const MINTER: Self = Self(16);
    /// Works the metalworks.
    pub const METALWORKER: Self = Self(17);
    /// Works the armoury.
    pub const ARMOURER: Self = Self(18);
    /// Builds on a construction site.
    pub const BUILDER: Self = Self(19);
    /// Levels a construction site's ground.
    pub const PLANER: Self = Self(20);
    /// Looks for minerals.
    pub const GEOLOGIST: Self = Self(21);
    /// The lowest soldier rank.
    pub const PRIVATE: Self = Self(22);
    /// The second soldier rank.
    pub const PRIVATE_FIRST_CLASS: Self = Self(23);
    /// The third soldier rank.
    pub const SERGEANT: Self = Self(24);
    /// The fourth soldier rank.
    pub const OFFICER: Self = Self(25);
    /// The highest soldier rank.
    pub const GENERAL: Self = Self(26);
    /// Explores.
    pub const SCOUT: Self = Self(27);
    /// Builds ships.
    pub const SHIPWRIGHT: Self = Self(28);
    /// A road's second carrier on a busy road.
    pub const PACK_DONKEY: Self = Self(29);

    /// A farmer out to sow (`0x2f075`).
    pub const SOWER: Self = Self(30);
    /// A woodcutter carrying wood home.
    pub const CARRYING_WOOD: Self = Self(38);
    /// A stonemason carrying a stone home.
    pub const CARRYING_STONE: Self = Self(40);
    /// A fisher carrying fish home.
    pub const CARRYING_FISH: Self = Self(47);
    /// A farmer carrying grain home.
    pub const CARRYING_GRAIN: Self = Self(48);
    /// A hunter carrying meat home.
    pub const CARRYING_MEAT: Self = Self(67);
    /// A shipwright walking out to its ship site (`0x2fa1a`). How it looks is not known.
    pub const SHIPWRIGHT_OUT: Self = Self(69);
    /// What a water road orders in place of a helper (`0x25b15`); not identified.
    pub const WATER_ROAD: Self = Self(70);
    /// A well's helper carrying water out to the flag (`0x2d6de`), drawn from `CARRIER.BOB`.
    pub const CARRYING_WATER: Self = Self(71);

    /// The soldier ranks, lowest first.
    pub const RANKS: [Self; 5] = [
        Self::PRIVATE,
        Self::PRIVATE_FIRST_CLASS,
        Self::SERGEANT,
        Self::OFFICER,
        Self::GENERAL,
    ];

    /// The number of jobs a storehouse counts.
    pub const PEOPLE: usize = 30;

    /// The name each job a storehouse counts goes by in mods, by job.
    pub const NAMES: [&'static str; Self::PEOPLE] = [
        "helper",
        "woodcutter",
        "fisher",
        "forester",
        "carpenter",
        "stonemason",
        "hunter",
        "farmer",
        "miller",
        "baker",
        "butcher",
        "miner",
        "brewer",
        "pig_breeder",
        "donkey_breeder",
        "iron_founder",
        "minter",
        "metalworker",
        "armourer",
        "builder",
        "planer",
        "geologist",
        "private",
        "private_first_class",
        "sergeant",
        "officer",
        "general",
        "scout",
        "shipwright",
        "pack_donkey",
    ];

    /// The job named `name` in mods.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let index = Self::NAMES.iter().position(|&known| known == name)?;
        u8::try_from(index).ok().map(Self)
    }

    /// The name the job goes by in mods, or `None` for one without a name.
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        Self::NAMES.get(self.index()).copied()
    }

    /// Whether this is one of the jobs a storehouse counts, 0 to 29.
    #[must_use]
    pub const fn is_person(self) -> bool {
        (self.0 as usize) < Self::PEOPLE
    }

    /// Whether this is one of the five soldier ranks.
    #[must_use]
    pub const fn is_soldier(self) -> bool {
        self.0 >= Self::PRIVATE.0 && self.0 <= Self::GENERAL.0
    }

    /// The job's row in tables indexed by job.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
