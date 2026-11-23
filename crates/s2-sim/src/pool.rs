//! Record pools as `S2.EXE` keeps them: an array where a new record takes the lowest free slot
//! (`0x281cc` for flags, `0x293ac` for roads). Record numbers end up in object words and savegames,
//! so the slot a record gets is part of the game state.

/// Records of one kind, numbered from 0, each slot in use or free.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Pool<T> {
    records: Vec<Option<T>>,
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Pool<T> {
    /// A pool with no records.
    #[must_use]
    pub const fn new() -> Self {
        Self { records: Vec::new() }
    }

    /// Puts `value` in the lowest free slot and returns its number; `None` when `limit` records
    /// are already in use.
    pub fn insert(&mut self, value: T, limit: usize) -> Option<u16> {
        if self.len() >= limit {
            return None;
        }
        let slot = self
            .records
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.records.len());
        let record = u16::try_from(slot).ok()?;
        if slot == self.records.len() {
            self.records.push(Some(value));
        } else {
            self.records[slot] = Some(value);
        }
        Some(record)
    }

    /// Frees `record` and returns what it held.
    pub fn remove(&mut self, record: u16) -> Option<T> {
        let value = self.records.get_mut(usize::from(record))?.take();
        while self.records.last().is_some_and(Option::is_none) {
            self.records.pop();
        }
        value
    }

    /// Record `record`, if it is in use.
    #[must_use]
    pub fn get(&self, record: u16) -> Option<&T> {
        self.records.get(usize::from(record))?.as_ref()
    }

    /// Record `record` to change, if it is in use.
    pub fn get_mut(&mut self, record: u16) -> Option<&mut T> {
        self.records.get_mut(usize::from(record))?.as_mut()
    }

    /// How many records are in use.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.iter().flatten().count()
    }

    /// One past the highest slot in use: the high-water mark plus one, 0 when the pool is empty.
    /// Freeing the top record lowers it past the free slots below.
    #[must_use]
    pub fn slots(&self) -> usize {
        self.records.len()
    }

    /// Whether no record is in use.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The records in use to change, in record order, with their numbers.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u16, &mut T)> {
        (0_u16..)
            .zip(&mut self.records)
            .filter_map(|(record, value)| Some((record, value.as_mut()?)))
    }

    /// The records in use, in record order, with their numbers.
    pub fn iter(&self) -> impl Iterator<Item = (u16, &T)> {
        (0_u16..)
            .zip(&self.records)
            .filter_map(|(record, value)| Some((record, value.as_ref()?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_record_takes_the_lowest_free_slot() {
        let mut pool = Pool::new();
        for value in 0..4 {
            assert_eq!(pool.insert(value, 10), Some(value));
        }
        assert_eq!(pool.remove(2), Some(2));
        assert_eq!(pool.remove(0), Some(0));
        assert_eq!(pool.insert(10, 10), Some(0));
        assert_eq!(pool.insert(11, 10), Some(2));
        assert_eq!(pool.insert(12, 10), Some(4));
        assert_eq!(
            pool.iter().map(|(_, &v)| v).collect::<Vec<_>>(),
            [10, 1, 11, 3, 12]
        );
    }

    #[test]
    fn a_full_pool_refuses() {
        let mut pool = Pool::new();
        assert_eq!(pool.insert('a', 1), Some(0));
        assert_eq!(pool.insert('b', 1), None);
        pool.remove(0);
        assert!(pool.is_empty());
        assert_eq!(pool.insert('b', 1), Some(0));
    }

    #[test]
    fn freeing_the_top_records_leaves_the_same_pool_as_never_using_them() {
        let mut used = Pool::new();
        used.insert(1, 10);
        used.insert(2, 10);
        used.insert(3, 10);
        used.remove(2);
        used.remove(1);
        let mut fresh = Pool::new();
        fresh.insert(1, 10);
        assert_eq!(used, fresh);
    }
}
