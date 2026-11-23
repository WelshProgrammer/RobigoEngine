//! Storehouses: the stock of wares and people a headquarters, storehouse or harbour holds.

use std::hash::{Hash, Hasher};

use bitflags::bitflags;

use super::{Building, World};
use crate::commands::Refusal;
use crate::job::Job;
use crate::ware::WareKind;

/// Ware types, 0 to 30.
pub const WARE_TYPES: usize = 31;

/// The tool each job needs to be made from a helper (byte 0 of each row of the job table at
/// `0x1fad97`), `None` for jobs made without one.
pub(crate) const TOOLS: [Option<WareKind>; Job::PEOPLE] = [
    None,
    Some(WareKind::AXE),
    Some(WareKind::ROD_AND_LINE),
    Some(WareKind::SHOVEL),
    Some(WareKind::SAW),
    Some(WareKind::PICK_AXE),
    Some(WareKind::BOW),
    Some(WareKind::SCYTHE),
    None,
    Some(WareKind::ROLLING_PIN),
    Some(WareKind::CLEAVER),
    Some(WareKind::PICK_AXE),
    None,
    None,
    None,
    Some(WareKind::CRUCIBLE),
    Some(WareKind::CRUCIBLE),
    Some(WareKind::TONGS),
    Some(WareKind::HAMMER),
    Some(WareKind::HAMMER),
    Some(WareKind::SHOVEL),
    Some(WareKind::HAMMER),
    None,
    None,
    None,
    None,
    None,
    Some(WareKind::BOW),
    Some(WareKind::HAMMER),
    None,
];

/// What a settler shown in each look from job 30 up counts as when it walks into a storehouse,
/// and the ware it brings in (bytes 3 and 4 of each row of the job table at `0x1fad97`, applied
/// by `0x31603`). A look with no job is not counted; `S2.EXE` would count it past the job array.
const RETURNS: [(Option<Job>, Option<WareKind>); 42] = [
    (None, None),
    (Some(Job::BREWER), Some(WareKind::BEER)),
    (Some(Job::ARMOURER), Some(WareKind::BOW)),
    (Some(Job::ARMOURER), Some(WareKind::SWORD)),
    (Some(Job::ARMOURER), Some(WareKind::SHIELD)),
    (Some(Job::ARMOURER), Some(WareKind::SHIELD)),
    (Some(Job::ARMOURER), Some(WareKind::SHIELD)),
    (Some(Job::ARMOURER), Some(WareKind::SHIELD)),
    (Some(Job::WOODCUTTER), Some(WareKind::WOOD)),
    (Some(Job::CARPENTER), Some(WareKind::BOARDS)),
    (Some(Job::STONEMASON), Some(WareKind::STONES)),
    (Some(Job::MINTER), Some(WareKind::COINS)),
    (Some(Job::MINER), Some(WareKind::GOLD)),
    (Some(Job::MINER), Some(WareKind::IRON_ORE)),
    (Some(Job::MINER), Some(WareKind::COAL)),
    (Some(Job::MINER), Some(WareKind::STONES)),
    (Some(Job::IRON_FOUNDER), Some(WareKind::IRON)),
    (Some(Job::FISHER), Some(WareKind::FISH)),
    (Some(Job::FARMER), Some(WareKind::GRAIN)),
    (Some(Job::PIG_BREEDER), None),
    (Some(Job::PIG_BREEDER), Some(WareKind::HAM)),
    (Some(Job::BUTCHER), Some(WareKind::MEAT)),
    (Some(Job::MILLER), Some(WareKind::FLOUR)),
    (Some(Job::BAKER), Some(WareKind::BREAD)),
    (None, None),
    (None, None),
    (None, None),
    (Some(Job::METALWORKER), Some(WareKind::AXE)),
    (Some(Job::METALWORKER), Some(WareKind::PICK_AXE)),
    (Some(Job::METALWORKER), Some(WareKind::SHOVEL)),
    (Some(Job::METALWORKER), Some(WareKind::CRUCIBLE)),
    (Some(Job::METALWORKER), Some(WareKind::ROD_AND_LINE)),
    (Some(Job::METALWORKER), Some(WareKind::SCYTHE)),
    (Some(Job::METALWORKER), None),
    (Some(Job::METALWORKER), Some(WareKind::CLEAVER)),
    (Some(Job::METALWORKER), Some(WareKind::ROLLING_PIN)),
    (Some(Job::METALWORKER), Some(WareKind::SAW)),
    (Some(Job::HUNTER), Some(WareKind::MEAT)),
    (Some(Job::SHIPWRIGHT), Some(WareKind::BOAT)),
    (Some(Job::SHIPWRIGHT), Some(WareKind::BOARDS)),
    (Some(Job::HELPER), Some(WareKind::BOAT)),
    (Some(Job::HELPER), Some(WareKind::WATER)),
];

/// Bytes in each of a storehouse record's four stop and send sets.
const SET_BYTES: usize = 4;

/// Bytes of the four sets together.
const SETTING_BYTES: usize = 4 * SET_BYTES;

/// The ware a water-road carrier takes from stock with its helper (byte 0 of [`Job::WATER_ROAD`]'s row).
const WATER_ROAD_TOOL: WareKind = WareKind::BOAT;

/// What a new headquarters holds before the start-goods multiplier (`0x1fa392`): wares, then
/// people, in table order.
const HEADQUARTERS_WARES: [(WareKind, u16); 24] = [
    (WareKind::WOOD, 12),
    (WareKind::BOARDS, 22),
    (WareKind::STONES, 34),
    (WareKind::FISH, 2),
    (WareKind::MEAT, 3),
    (WareKind::BREAD, 4),
    (WareKind::BEER, 3),
    (WareKind::COAL, 8),
    (WareKind::IRON_ORE, 8),
    (WareKind::TONGS, 0),
    (WareKind::AXE, 3),
    (WareKind::SAW, 1),
    (WareKind::PICK_AXE, 1),
    (WareKind::HAMMER, 8),
    (WareKind::SHOVEL, 2),
    (WareKind::CRUCIBLE, 2),
    (WareKind::ROD_AND_LINE, 3),
    (WareKind::SCYTHE, 4),
    (WareKind::CLEAVER, 1),
    (WareKind::ROLLING_PIN, 1),
    (WareKind::BOW, 1),
    (WareKind::SWORD, 3),
    (WareKind::SHIELD, 3),
    (WareKind::BOAT, 6),
];

/// People in a new headquarters before the multiplier (`0x1fa392`, after the wares).
const HEADQUARTERS_JOBS: [(Job, u16); 15] = [
    (Job::HELPER, 26),
    (Job::WOODCUTTER, 4),
    (Job::FORESTER, 2),
    (Job::CARPENTER, 2),
    (Job::STONEMASON, 2),
    (Job::HUNTER, 1),
    (Job::MINER, 5),
    (Job::METALWORKER, 1),
    (Job::ARMOURER, 2),
    (Job::BUILDER, 5),
    (Job::PLANER, 3),
    (Job::GEOLOGIST, 3),
    (Job::SCOUT, 1),
    (Job::PACK_DONKEY, 4),
    (Job::PRIVATE, 23),
];

/// How much a new headquarters holds: the free-game set-up's start-goods setting (`0x1f7a3b`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum StartGoods {
    /// Setting 0: the base table.
    Low,
    /// Setting 1, the default: twice the base table.
    #[default]
    Normal,
    /// Setting 2: four times the base table.
    High,
}

impl StartGoods {
    /// What the base table is multiplied by: twice the setting, and 1 for setting 0.
    #[must_use]
    pub const fn multiplier(self) -> u16 {
        match self {
            Self::Low => 1,
            Self::Normal => 2,
            Self::High => 4,
        }
    }
}

bitflags! {
    /// What the player has set for one ware or job in a storehouse's window. The two settings
    /// are independent.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
    pub struct StockSetting: u8 {
        /// Takes no more in (wares `+0x7a`, jobs `+0x7e`).
        const STOP = 1;
        /// Sends its stock out, one at a time, and takes no more wares in (wares `+0x82`,
        /// jobs `+0x86`).
        const SEND = 2;
    }
}

/// What a storehouse gave up to hand a person over, which the owner's counts follow
/// (`0x31583`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// Nobody could be handed over.
    None,
    /// One of the job was already in stock.
    FromStock,
    /// A helper became one, using up the job's tool.
    Converted {
        /// The tool that went with the helper, where the job's row names one.
        tool: Option<WareKind>,
    },
}

impl Taken {
    /// Whether someone was handed over.
    #[must_use]
    pub const fn took(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// A storehouse record (141 bytes in `S2.EXE`, pool at `[0x2016d8]`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Storehouse {
    /// Count of each ware (`+0x00`).
    pub wares: [u16; WARE_TYPES],
    /// Count of each job (`+0x3e`).
    pub jobs: [u16; Job::PEOPLE],
    /// The setting of each ware.
    pub ware_settings: [StockSetting; WARE_TYPES],
    /// The setting of each job.
    pub job_settings: [StockSetting; Job::PEOPLE],
    /// The building it belongs to (`+0x8a`).
    pub building: u16,
}

impl Hash for Storehouse {
    /// Hashes the record's fields in its order, with the settings as its four 32-bit sets.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.wares.hash(state);
        self.jobs.hash(state);
        bit_set(&self.ware_settings, StockSetting::STOP).hash(state);
        bit_set(&self.job_settings, StockSetting::STOP).hash(state);
        bit_set(&self.ware_settings, StockSetting::SEND).hash(state);
        bit_set(&self.job_settings, StockSetting::SEND).hash(state);
        self.building.hash(state);
    }
}

/// The 32-bit set of the items whose setting includes `setting`: bit `i` for item `i`.
fn bit_set(settings: &[StockSetting], setting: StockSetting) -> u32 {
    settings
        .iter()
        .enumerate()
        .filter(|(_, item)| item.contains(setting))
        .fold(0, |set, (index, _)| set | 1 << index)
}

impl Storehouse {
    /// An empty storehouse for `building` (`0x3115b`).
    #[must_use]
    pub const fn new(building: u16) -> Self {
        Self {
            wares: [0; WARE_TYPES],
            jobs: [0; Job::PEOPLE],
            ware_settings: [StockSetting::empty(); WARE_TYPES],
            job_settings: [StockSetting::empty(); Job::PEOPLE],
            building,
        }
    }

    /// The setting of ware `ware`.
    #[must_use]
    pub fn ware_setting(&self, ware: WareKind) -> StockSetting {
        self.ware_settings.get(ware.index()).copied().unwrap_or_default()
    }

    /// The setting of job `job`; jobs past the people have none.
    #[must_use]
    pub fn job_setting(&self, job: Job) -> StockSetting {
        self.job_settings.get(job.index()).copied().unwrap_or_default()
    }

    /// Whether it takes ware `ware` in: neither stopped nor sent out (`0x31294`, `0x31396`).
    #[must_use]
    pub fn takes_ware(&self, ware: WareKind) -> bool {
        self.ware_setting(ware).is_empty()
    }

    /// Whether it takes a settler of job `job` in: not stopped (`0x31263`). Sending a job out
    /// does not stop it coming back.
    #[must_use]
    pub fn takes_job(&self, job: Job) -> bool {
        !self.job_setting(job).contains(StockSetting::STOP)
    }

    /// Whether it can send someone for job `job` (`0x314b1`), and as which job: a soldier rank
    /// with nobody in stock is swapped for another rank. A job it holds nobody for can be made
    /// from a helper and the job's tool, except a pack donkey or a soldier.
    ///
    /// The water-road job is a helper with a boat. Other jobs past the people get `None`.
    #[must_use]
    pub fn supplies(&self, job: Job) -> Option<Job> {
        let count = |job: Job| self.jobs[job.index()];
        if job == Job::WATER_ROAD {
            let boats = self.wares[WATER_ROAD_TOOL.index()];
            return (count(Job::HELPER) > 0 && boats > 0).then_some(job);
        }
        if !job.is_person() {
            return None;
        }
        let job = match job {
            Job::PRIVATE if count(job) == 0 => Job::RANKS[1..].iter().copied().find(|&rank| count(rank) > 0),
            _ if job.is_soldier() && count(job) == 0 => {
                Job::RANKS.iter().rev().copied().find(|&rank| count(rank) > 0)
            }
            _ => None,
        }
        .unwrap_or(job);
        if count(job) > 0 {
            return Some(job);
        }
        if job.is_soldier() || count(Job::HELPER) == 0 || job == Job::PACK_DONKEY {
            return None;
        }
        let has_tool =
            TOOLS[job.index()].is_none_or(|ware| self.wares.get(ware.index()).is_some_and(|&n| n > 0));
        has_tool.then_some(job)
    }

    /// Takes one person with job `job` out of stock, first making one from a helper and the job's
    /// tool when there is none (`0x3155f`, `0x31583`).
    ///
    /// A water-road carrier takes a helper and a boat. `S2.EXE` also adds 1 to a job count past
    /// the end of the record (`0x31583`), which gives the next storehouse record 256 boats; Robigo
    /// leaves that out.
    pub fn take_person(&mut self, job: Job) -> Taken {
        if job == Job::WATER_ROAD {
            if self.supplies(job).is_none() {
                return Taken::None;
            }
            self.jobs[Job::HELPER.index()] -= 1;
            self.wares[WATER_ROAD_TOOL.index()] -= 1;
            return Taken::Converted {
                tool: Some(WATER_ROAD_TOOL),
            };
        }
        if !job.is_person() {
            return Taken::None;
        }
        let index = job.index();
        if self.jobs[index] == 0 {
            if self.jobs[Job::HELPER.index()] == 0 {
                return Taken::None;
            }
            self.jobs[Job::HELPER.index()] -= 1;
            self.jobs[index] += 1;
            let tool = TOOLS[index];
            if let Some(tool) = tool {
                let tools = &mut self.wares[tool.index()];
                *tools = tools.saturating_sub(1);
            }
            self.jobs[index] -= 1;
            return Taken::Converted { tool };
        }
        self.jobs[index] -= 1;
        Taken::FromStock
    }

    /// Counts in a settler of job `job` walking in (`0x31603`): a look from job 30 up counts as
    /// its job and brings its ware with it. A look the table has no row for counts nothing with
    /// `fixes` on, and with them off is counted at its own job number, past the 30 job counts,
    /// as [`Self::count_past_jobs`] describes.
    pub fn take_in(&mut self, job: Job, fixes: crate::game::BugFixes) {
        let (person, ware) = if job.is_person() {
            (Some(job), None)
        } else {
            RETURNS
                .get(job.index() - Job::PEOPLE)
                .copied()
                .unwrap_or_default()
        };
        let Some(person) = person else {
            if !fixes.on() {
                self.count_past_jobs(job);
            }
            return;
        };
        if let Some(count) = self.jobs.get_mut(person.index()) {
            *count = count.saturating_add(1);
        }
        if let Some(stock) = ware.and_then(|ware| self.wares.get_mut(ware.index())) {
            *stock = stock.saturating_add(1);
        }
    }

    /// `S2.EXE` adds one to the word at `+0x3e + 2 · job` whatever the job, so a job from 30 up
    /// writes past the job counts into the stop and send sets that follow them. Job 30 adds one
    /// to the word made of the ware stop bits for wares 0 to 15, which a storehouse with none set
    /// turns into a stop on wood. A job whose word lies past the sets, in the building word or
    /// the next record, changes nothing here.
    fn count_past_jobs(&mut self, job: Job) {
        let Some(at) = job.index().checked_sub(Job::PEOPLE).map(|past| 2 * past) else {
            return;
        };
        let mut bytes = self.setting_bytes();
        let (Some(&low), Some(&high)) = (bytes.get(at), bytes.get(at + 1)) else {
            return;
        };
        let word = u16::from_le_bytes([low, high]).wrapping_add(1).to_le_bytes();
        bytes[at] = word[0];
        bytes[at + 1] = word[1];
        self.set_setting_bytes(&bytes);
    }

    /// The stop and send sets as `S2.EXE` lays them out after the job counts: ware stops, job
    /// stops, ware sends, job sends, each four bytes with ware or job `n` at bit `n % 8` of byte
    /// `n / 8`.
    fn setting_bytes(&self) -> [u8; SETTING_BYTES] {
        let mut bytes = [0; SETTING_BYTES];
        let sets = [
            (0, StockSetting::STOP, &self.ware_settings[..]),
            (SET_BYTES, StockSetting::STOP, &self.job_settings[..]),
            (2 * SET_BYTES, StockSetting::SEND, &self.ware_settings[..]),
            (3 * SET_BYTES, StockSetting::SEND, &self.job_settings[..]),
        ];
        for (start, bit, settings) in sets {
            for (n, setting) in settings.iter().enumerate() {
                if setting.contains(bit) {
                    bytes[start + n / 8] |= 1 << (n % 8);
                }
            }
        }
        bytes
    }

    /// Writes the stop and send sets back from [`Self::setting_bytes`]'s layout.
    fn set_setting_bytes(&mut self, bytes: &[u8; SETTING_BYTES]) {
        let held = |start: usize, n: usize| bytes[start + n / 8] & (1 << (n % 8)) != 0;
        for (n, setting) in self.ware_settings.iter_mut().enumerate() {
            setting.set(StockSetting::STOP, held(0, n));
            setting.set(StockSetting::SEND, held(2 * SET_BYTES, n));
        }
        for (n, setting) in self.job_settings.iter_mut().enumerate() {
            setting.set(StockSetting::STOP, held(SET_BYTES, n));
            setting.set(StockSetting::SEND, held(3 * SET_BYTES, n));
        }
    }

    /// Fills a new headquarters' stock (`0x26221`): every row of the table times `goods`'
    /// multiplier.
    pub fn stock_headquarters(&mut self, goods: StartGoods) {
        let times = goods.multiplier();
        for (ware, count) in HEADQUARTERS_WARES {
            self.wares[ware.index()] += count * times;
        }
        for (job, count) in HEADQUARTERS_JOBS {
            self.jobs[job.index()] += count * times;
        }
    }
}

impl Building {
    /// Whether it is a storehouse type.
    #[must_use]
    pub fn is_storehouse(&self) -> bool {
        self.kind.is_storehouse()
    }
}

impl World {
    /// The storehouse record of building `building`, if it has one.
    #[must_use]
    pub fn storehouse_of(&self, building: u16) -> Option<(u16, &Storehouse)> {
        self.stores.iter().find(|(_, store)| store.building == building)
    }

    /// The storehouse record of the finished storehouse whose flag stands on `flag_node`.
    #[must_use]
    pub fn storehouse_behind(&self, flag_node: crate::grid::Node) -> Option<u16> {
        let node = flag_node.neighbour(crate::grid::Direction::NorthWest, self.width, self.height);
        let super::MapObject::Building(building) = self.object(node) else {
            return None;
        };
        let record = self.building(building)?;
        if !record.is_storehouse() || !record.state.contains(super::BuildingState::FINISHED) {
            return None;
        }
        self.storehouse_of(building).map(|(store, _)| store)
    }

    /// Storehouse record `record`.
    #[must_use]
    pub fn storehouse(&self, record: u16) -> Option<&Storehouse> {
        self.stores.get(record)
    }

    /// The storehouse record of `player`'s finished storehouse on `node`, with the reason when
    /// there is none.
    ///
    /// The storehouse window and [`Command::SetStock`](crate::Command::SetStock) both ask this,
    /// so an icon the window greys out and a command the world refuses give the same reason.
    ///
    /// # Errors
    ///
    /// [`Refusal::NoBuilding`] when nothing of the player's stands there,
    /// [`Refusal::NotYours`] when the building belongs to someone else, and
    /// [`Refusal::NotStorehouse`] when it is not a finished storehouse.
    pub fn player_storehouse(&self, player: super::Player, node: crate::grid::Node) -> Result<u16, Refusal> {
        let super::MapObject::Building(building) = self.object(node) else {
            return Err(Refusal::NoBuilding);
        };
        let record = self.building(building).ok_or(Refusal::NoBuilding)?;
        if record.owner != player {
            return Err(Refusal::NotYours);
        }
        if !record.is_storehouse() || record.is_site() {
            return Err(Refusal::NotStorehouse);
        }
        self.storehouse_of(building)
            .map(|(store, _)| store)
            .ok_or(Refusal::NotStorehouse)
    }

    /// The storehouse records in use, in record order, with their numbers.
    pub fn storehouses(&self) -> impl Iterator<Item = (u16, &Storehouse)> {
        self.stores.iter()
    }

    /// Storehouse record `record` to change.
    pub(crate) fn storehouse_mut(&mut self, record: u16) -> Option<&mut Storehouse> {
        self.stores.get_mut(record)
    }

    /// Counts a helper that storehouse building `building` turned into `job` (`0x31583`): the
    /// helper comes off the owner's people count, the new job goes on, and the tool the
    /// conversion used comes off the owner's ware count.
    pub(crate) fn count_conversion(&mut self, building: u16, job: Job, taken: Taken) {
        let Taken::Converted { tool } = taken else {
            return;
        };
        let Some(owner) = self.building(building).map(|site| site.owner) else {
            return;
        };
        self.drop_people(owner, Job::HELPER, 1);
        self.count_people(owner, job, 1);
        if let Some(tool) = tool {
            self.drop_ware(owner, tool, 1);
        }
    }

    /// Storehouse `store` hands over one person with job `job` (`0x3155f`, `0x31583`), with the
    /// owner's counts following a helper converted for it. Gives back whether someone was handed
    /// over.
    pub(crate) fn hand_over_person(&mut self, store: u16, job: Job) -> bool {
        let Some(building) = self.storehouse(store).map(|stock| stock.building) else {
            return false;
        };
        let Some(stock) = self.storehouse_mut(store) else {
            return false;
        };
        let taken = stock.take_person(job);
        self.count_conversion(building, job, taken);
        taken.took()
    }

    /// Adds the storehouse record `store` in the lowest free slot and returns its number, or
    /// `None` when the pool is full. The building it belongs to is the record's own
    /// [`Storehouse::building`], and the caller fills the record in before handing it over.
    pub fn add_storehouse(&mut self, store: Storehouse) -> Option<u16> {
        self.stores.insert(store, usize::from(u16::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::BugFixes;

    #[test]
    fn a_default_headquarters_holds_52_helpers() {
        let mut store = Storehouse::new(0);
        store.stock_headquarters(StartGoods::default());
        assert_eq!(
            (
                store.jobs[Job::HELPER.index()],
                store.jobs[Job::PRIVATE.index()],
                store.jobs[Job::PACK_DONKEY.index()]
            ),
            (52, 46, 8)
        );
        assert_eq!(
            (
                store.wares[WareKind::BOARDS.index()],
                store.wares[WareKind::STONES.index()],
                store.wares[WareKind::TONGS.index()]
            ),
            (44, 68, 0)
        );
        let mut high = Storehouse::new(0);
        high.stock_headquarters(StartGoods::High);
        assert_eq!(high.jobs[Job::HELPER.index()], 104);
    }

    #[test]
    fn jobs_are_made_from_helpers_and_tools() {
        let mut store = Storehouse::new(0);
        store.jobs[Job::HELPER.index()] = 1;
        assert_eq!(store.supplies(Job::WOODCUTTER), None, "a woodcutter needs an axe");
        store.wares[WareKind::AXE.index()] = 1;
        assert_eq!(store.supplies(Job::WOODCUTTER), Some(Job::WOODCUTTER));
        assert_eq!(
            store.supplies(Job::MILLER),
            Some(Job::MILLER),
            "a miller needs no tool"
        );
        assert_eq!(store.supplies(Job::PACK_DONKEY), None, "donkeys are never made");
        assert!(store.take_person(Job::WOODCUTTER).took());
        assert_eq!(
            (
                store.jobs[Job::HELPER.index()],
                store.jobs[Job::WOODCUTTER.index()],
                store.wares[WareKind::AXE.index()]
            ),
            (0, 0, 0)
        );
        assert!(!store.take_person(Job::HELPER).took());
    }

    #[test]
    fn a_water_road_carrier_is_a_helper_and_a_boat() {
        let mut store = Storehouse::new(0);
        store.jobs[Job::HELPER.index()] = 1;
        assert_eq!(store.supplies(Job::WATER_ROAD), None, "no boat");
        store.wares[WareKind::BOAT.index()] = 2;
        assert_eq!(store.supplies(Job::WATER_ROAD), Some(Job::WATER_ROAD));
        assert!(store.take_person(Job::WATER_ROAD).took());
        assert_eq!(
            (
                store.jobs[Job::HELPER.index()],
                store.wares[WareKind::BOAT.index()]
            ),
            (0, 1)
        );
        assert!(!store.take_person(Job::WATER_ROAD).took(), "no helper left");
    }

    #[test]
    fn a_settler_walking_in_counts_as_its_job_and_brings_its_ware() {
        let mut store = Storehouse::new(0);
        store.take_in(Job::CARRYING_WOOD, BugFixes::Off);
        store.take_in(Job::WATER_ROAD, BugFixes::Off);
        store.take_in(Job::MINER, BugFixes::Off);
        store.take_in(Job(55), BugFixes::Off);
        assert_eq!(store.jobs[Job::WOODCUTTER.index()], 1);
        assert_eq!(store.jobs[Job::HELPER.index()], 1);
        assert_eq!(store.jobs[Job::MINER.index()], 1);
        assert_eq!(store.wares[WareKind::WOOD.index()], 1);
        assert_eq!(store.wares[WareKind::BOAT.index()], 1);
        assert_eq!(
            store.jobs.iter().sum::<u16>() + store.wares.iter().sum::<u16>(),
            5
        );
    }

    #[test]
    fn soldier_ranks_stand_in_for_each_other() {
        let mut store = Storehouse::new(0);
        store.jobs[Job::HELPER.index()] = 5;
        assert_eq!(store.supplies(Job::PRIVATE), None, "soldiers are never made");
        store.jobs[Job::SERGEANT.index()] = 1;
        store.jobs[Job::OFFICER.index()] = 1;
        assert_eq!(
            store.supplies(Job::PRIVATE),
            Some(Job::SERGEANT),
            "the lowest rank above a private"
        );
        assert_eq!(
            store.supplies(Job::PRIVATE_FIRST_CLASS),
            Some(Job::OFFICER),
            "the highest rank there is"
        );
    }

    #[test]
    fn job_30_walking_in_counts_into_the_ware_stop_bits() {
        let mut store = Storehouse::new(0);
        let thirty = Job(u8::try_from(Job::PEOPLE).unwrap());
        store.take_in(thirty, BugFixes::Off);
        assert_eq!(
            store.ware_setting(WareKind::WOOD),
            StockSetting::STOP,
            "the first stops wood"
        );
        assert_eq!(store.jobs, [0; Job::PEOPLE]);
        store.take_in(thirty, BugFixes::Off);
        assert!(
            store.takes_ware(WareKind::WOOD),
            "the second carries into the next bit"
        );
        assert_eq!(store.ware_setting(WareKind(1)), StockSetting::STOP);
    }

    #[test]
    fn with_the_bug_fixes_on_job_30_walking_in_counts_nothing() {
        let mut store = Storehouse::new(0);
        store.take_in(Job(u8::try_from(Job::PEOPLE).unwrap()), BugFixes::On);
        assert_eq!(store, Storehouse::new(0));
    }
}
