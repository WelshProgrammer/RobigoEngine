//! Gouraud shading tables (`GOU*.DAT`).
//!
//! Each table is 256 rows of 256 bytes. The row is a light level and the column a palette index;
//! the byte is the palette index to draw for that colour under that light. Row
//! [`NEUTRAL_LEVEL`] is the identity, lower rows darken and higher rows brighten. In every
//! shipped table rows 0 to [`MAX_LEVEL`] are distinct and the rest repeat one filler colour.
//! See `docs/formats/gouraud.md`.

/// Size of a shading table file in bytes.
pub const TABLE_SIZE: usize = 256 * 256;

/// The light level whose row maps every index to itself.
pub const NEUTRAL_LEVEL: u8 = 64;

/// The brightest level with its own row in the shipped tables.
pub const MAX_LEVEL: u8 = 129;

/// Why a shading table could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The input is not exactly [`TABLE_SIZE`] bytes.
    #[error("shading table is {0} bytes, expected {TABLE_SIZE}")]
    WrongSize(usize),
}

/// A palette shading lookup table.
#[derive(Clone, PartialEq, Eq)]
pub struct ShadingTable(Box<[[u8; 256]; 256]>);

impl ShadingTable {
    /// Reads a table from the bytes of a `GOU*.DAT` file.
    ///
    /// # Errors
    ///
    /// [`Error::WrongSize`] unless `data` is exactly [`TABLE_SIZE`] bytes long.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        if data.len() != TABLE_SIZE {
            return Err(Error::WrongSize(data.len()));
        }
        let (rows, _) = data.as_chunks::<256>();
        Box::<[[u8; 256]]>::from(rows)
            .try_into()
            .map(Self)
            .map_err(|_| Error::WrongSize(data.len()))
    }

    /// The palette index to draw for `index` under light `level`.
    #[must_use]
    pub fn shade(&self, level: u8, index: u8) -> u8 {
        self.row(level)[usize::from(index)]
    }

    /// Every shaded index for light `level`, by original palette index.
    #[must_use]
    pub fn row(&self, level: u8) -> &[u8; 256] {
        &self.0[usize::from(level)]
    }
}

impl std::fmt::Debug for ShadingTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShadingTable").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_light_levels_and_columns_are_indices() {
        let mut data = vec![0; TABLE_SIZE];
        for (i, byte) in data.iter_mut().enumerate() {
            let (level, index) = (i / 256, i % 256);
            *byte = u8::try_from((index + level) % 256).unwrap();
        }
        let table = ShadingTable::parse(&data).unwrap();
        assert_eq!(table.shade(0, 7), 7);
        assert_eq!(table.shade(2, 255), 1);
        assert_eq!(table.row(255)[1], 0);
    }

    #[test]
    fn size_must_be_exact() {
        assert_eq!(
            ShadingTable::parse(&vec![0; TABLE_SIZE - 1]),
            Err(Error::WrongSize(TABLE_SIZE - 1))
        );
        assert_eq!(ShadingTable::parse(&[]), Err(Error::WrongSize(0)));
        assert!(ShadingTable::parse(&vec![0; TABLE_SIZE + 1]).is_err());
    }
}
