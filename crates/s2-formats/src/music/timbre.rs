//! Miles Global Timbre Library (`.AD`, `.OPL`) banks of FM instruments.

use super::Error;
use super::xmidi::TimbreRef;
use crate::bytes::{Reader, slice, to_usize};

/// Record length, prefix included, of a two-operator OPL2 timbre.
const OPL2_RECORD_LEN: usize = 14;

/// The bank byte that ends the directory.
const DIRECTORY_END: u8 = u8::MAX;

/// A Global Timbre Library: instruments addressed by bank and patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimbreBank {
    /// Timbres in directory order.
    pub timbres: Vec<Timbre>,
}

/// One instrument in a [`TimbreBank`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timbre {
    /// Patch number; in bank 127, the percussion key that selects it.
    pub patch: u8,
    /// Bank number.
    pub bank: u8,
    /// The instrument definition.
    pub data: TimbreData,
}

/// The body of a timbre record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimbreData {
    /// A two-operator OPL2 instrument (a 14-byte record).
    Opl2(Opl2Timbre),
    /// Any other record size, such as a four-operator OPL3 voice; the bytes after the length.
    Other(Vec<u8>),
}

/// A two-operator OPL2 instrument, as raw register values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Opl2Timbre {
    /// For melodic banks, a signed semitone offset applied to every note (0 throughout the
    /// Settlers II bank). For percussion (bank 127) Miles plays the timbre at this fixed key.
    pub transpose: u8,
    /// Operator 1, which modulates the carrier in FM mode.
    pub modulator: Opl2Operator,
    /// Register `0xC0`: feedback in bits 1..=3, connection in bit 0.
    pub feedback_connection: u8,
    /// Operator 2, the one heard in FM mode.
    pub carrier: Opl2Operator,
}

/// One OPL2 operator's register values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Opl2Operator {
    /// Register `0x20`: tremolo, vibrato, sustain (envelope type), key scale rate, multiplier.
    pub characteristic: u8,
    /// Register `0x40`: key scale level in bits 6..=7, total level (attenuation) in bits 0..=5.
    pub level: u8,
    /// Register `0x60`: attack rate high nibble, decay rate low nibble.
    pub attack_decay: u8,
    /// Register `0x80`: sustain level high nibble, release rate low nibble.
    pub sustain_release: u8,
    /// Register `0xE0`: waveform select.
    pub waveform: u8,
}

impl Opl2Operator {
    fn from_bytes(b: [u8; 5]) -> Self {
        let [characteristic, level, attack_decay, sustain_release, waveform] = b;
        Self {
            characteristic,
            level,
            attack_decay,
            sustain_release,
            waveform,
        }
    }

    /// Frequency multiplier code (register `0x20` bits 0..=3).
    #[must_use]
    pub fn multiplier(self) -> u8 {
        self.characteristic % 16
    }

    /// Attenuation in 0.75 dB steps (register `0x40` bits 0..=5).
    #[must_use]
    pub fn total_level(self) -> u8 {
        self.level % 64
    }
}

impl Opl2Timbre {
    fn from_bytes(b: [u8; 12]) -> Self {
        let [t, m0, m1, m2, m3, m4, fc, c0, c1, c2, c3, c4] = b;
        Self {
            transpose: t,
            modulator: Opl2Operator::from_bytes([m0, m1, m2, m3, m4]),
            feedback_connection: fc,
            carrier: Opl2Operator::from_bytes([c0, c1, c2, c3, c4]),
        }
    }

    /// Modulator feedback strength, 0..=7.
    #[must_use]
    pub fn feedback(self) -> u8 {
        (self.feedback_connection >> 1) % 8
    }

    /// Whether both operators sound directly (additive synthesis) rather than FM.
    #[must_use]
    pub fn additive(self) -> bool {
        self.feedback_connection & 1 == 1
    }
}

impl TimbreBank {
    /// Parses a Global Timbre Library.
    ///
    /// The file opens with a directory of six-byte entries (patch, bank, little-endian `u32`
    /// offset) ended by a bank byte of 255. Each offset points at a record: a little-endian
    /// `u16` length that counts itself, then the body.
    ///
    /// # Errors
    ///
    /// [`Error::UnterminatedDirectory`] when the directory runs off the end,
    /// [`Error::BadTimbreLength`] for a length below 2, [`Error::Truncated`] when a record
    /// lies outside the file, and [`Error::TimbreOverlap`] when the records together claim
    /// more bytes than the file holds.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(data);
        let mut directory = Vec::new();
        loop {
            let [patch, bank] = r.array().map_err(|_| Error::UnterminatedDirectory)?;
            if bank == DIRECTORY_END {
                break;
            }
            let offset = to_usize(r.u32().map_err(|_| Error::UnterminatedDirectory)?);
            directory.push((patch, bank, offset));
        }
        let mut budget = data.len();
        let mut timbres = Vec::with_capacity(directory.len());
        for (patch, bank, offset) in directory {
            let len = Reader::new(slice(data, offset, 2)?).u16()?;
            let body_len = usize::from(len)
                .checked_sub(2)
                .ok_or(Error::BadTimbreLength { offset, len })?;
            budget = budget.checked_sub(usize::from(len)).ok_or(Error::TimbreOverlap)?;
            let body = slice(data, offset + 2, body_len)?;
            let data = match body.try_into() {
                Ok(bytes) if usize::from(len) == OPL2_RECORD_LEN => {
                    TimbreData::Opl2(Opl2Timbre::from_bytes(bytes))
                }
                _ => TimbreData::Other(body.to_vec()),
            };
            timbres.push(Timbre { patch, bank, data });
        }
        Ok(Self { timbres })
    }

    /// Looks up the timbre for a bank and patch.
    #[must_use]
    pub fn get(&self, which: TimbreRef) -> Option<&Timbre> {
        self.timbres
            .iter()
            .find(|t| t.bank == which.bank && t.patch == which.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIANO: [u8; 12] = [
        0, 0x01, 0x8f, 0xf2, 0xf4, 0x00, 0x08, 0x01, 0x06, 0xf2, 0xf7, 0x00,
    ];

    fn bank(entries: &[(u8, u8, &[u8])]) -> Vec<u8> {
        let mut dir = Vec::new();
        let mut records = Vec::new();
        let dir_len = entries.len() * 6 + 2;
        for &(patch, bank, body) in entries {
            let offset = u32::try_from(dir_len + records.len()).unwrap();
            dir.extend([patch, bank]);
            dir.extend(offset.to_le_bytes());
            records.extend(u16::try_from(body.len() + 2).unwrap().to_le_bytes());
            records.extend_from_slice(body);
        }
        dir.extend([0xff, 0xff]);
        dir.extend(records);
        dir
    }

    #[test]
    fn parses_opl2_and_other_records() {
        let mut drum = PIANO;
        drum[0] = 60;
        let data = bank(&[(0, 0, &PIANO), (35, 127, &drum), (1, 0, &[1, 2, 3])]);
        let parsed = TimbreBank::parse(&data).unwrap();
        assert_eq!(parsed.timbres.len(), 3);
        let TimbreData::Opl2(piano) = parsed.timbres[0].data else {
            panic!("expected OPL2");
        };
        assert_eq!(piano.modulator.level, PIANO[2]);
        assert_eq!(piano.modulator.total_level(), 15);
        assert_eq!(piano.carrier.sustain_release, PIANO[10]);
        assert_eq!(piano.carrier.multiplier(), 1);
        assert_eq!(piano.feedback(), 4);
        assert!(!piano.additive());
        let drum = parsed.get(TimbreRef { patch: 35, bank: 127 });
        assert!(matches!(drum.map(|t| &t.data), Some(TimbreData::Opl2(t)) if t.transpose == 60));
        assert_eq!(parsed.timbres[2].data, TimbreData::Other(vec![1, 2, 3]));
        assert!(parsed.get(TimbreRef { patch: 2, bank: 0 }).is_none());
    }

    #[test]
    fn rejects_malformed_banks() {
        assert_eq!(
            TimbreBank::parse(&[0, 0, 8, 0, 0, 0]),
            Err(Error::UnterminatedDirectory)
        );
        assert_eq!(
            TimbreBank::parse(&[0, 0, 8, 0, 0, 0, 0xff, 0xff, 1, 0]),
            Err(Error::BadTimbreLength { offset: 8, len: 1 })
        );
        assert!(matches!(
            TimbreBank::parse(&[0, 0, 8, 0, 0, 0, 0xff, 0xff, 9, 0]),
            Err(Error::Truncated(_))
        ));
        let mut aliased = vec![];
        for _ in 0..3 {
            aliased.extend([0, 0, 20, 0, 0, 0]);
        }
        aliased.extend([0xff, 0xff, 4, 0, 0, 0]);
        assert_eq!(TimbreBank::parse(&aliased).map(|b| b.timbres.len()), Ok(3));
        let mut many = vec![];
        for _ in 0..5 {
            many.extend([0, 0, 32, 0, 0, 0]);
        }
        many.extend([0xff, 0xff]);
        many.extend([30, 0]);
        many.resize(62, 0);
        assert_eq!(TimbreBank::parse(&many), Err(Error::TimbreOverlap));
    }

    #[test]
    fn never_panics_on_prefixes_or_corruption() {
        let data = bank(&[(0, 0, &PIANO), (35, 127, &PIANO)]);
        for len in 0..data.len() {
            let _ = TimbreBank::parse(&data[..len]);
        }
        for i in 0..data.len() {
            let mut bad = data.clone();
            bad[i] = !bad[i];
            let _ = TimbreBank::parse(&bad);
        }
    }
}
