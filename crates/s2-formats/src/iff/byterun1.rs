//! `ByteRun1` (`PackBits`) decoding as used by IFF `BODY` chunks.

/// The most output bytes per input byte: a two-byte run (header and value) expands to 128.
pub(super) const MAX_EXPANSION: usize = 64;

/// A streaming `ByteRun1` decoder that fills caller-sized buffers, carrying runs across calls.
#[derive(Debug, Clone)]
pub(super) struct ByteRun1<'a> {
    input: &'a [u8],
    pos: usize,
    pending: usize,
    repeat: Option<u8>,
}

impl<'a> ByteRun1<'a> {
    pub(super) fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            pending: 0,
            repeat: None,
        }
    }

    /// Fills all of `out`; returns `false` if the input ran out first.
    ///
    /// A run that crosses the end of `out` carries over into the next call.
    pub(super) fn fill(&mut self, mut out: &mut [u8]) -> bool {
        while !out.is_empty() {
            if self.pending == 0 && !self.next_run() {
                return false;
            }
            let take = self.pending.min(out.len());
            let Some((dst, rest)) = out.split_at_mut_checked(take) else {
                return false;
            };
            if let Some(value) = self.repeat {
                dst.fill(value);
            } else {
                let Some(src) = self.input.get(self.pos..self.pos + take) else {
                    return false;
                };
                dst.copy_from_slice(src);
                self.pos += take;
            }
            self.pending -= take;
            out = rest;
        }
        true
    }

    fn next_run(&mut self) -> bool {
        loop {
            let Some(&header) = self.input.get(self.pos) else {
                return false;
            };
            self.pos += 1;
            match header {
                0..=127 => {
                    self.pending = usize::from(header) + 1;
                    self.repeat = None;
                }
                128 => continue,
                129..=255 => {
                    let Some(&value) = self.input.get(self.pos) else {
                        return false;
                    };
                    self.pos += 1;
                    self.pending = 257 - usize::from(header);
                    self.repeat = Some(value);
                }
            }
            return true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unpack(input: &[u8], len: usize) -> Option<Vec<u8>> {
        let mut out = vec![0; len];
        ByteRun1::new(input).fill(&mut out).then_some(out)
    }

    #[test]
    fn literals_repeats_and_no_ops() {
        let input = [2, 1, 2, 3, 128, 0xfe, 9, 0x81, 7];
        let mut expected = vec![1, 2, 3, 9, 9, 9];
        expected.extend([7; 128]);
        assert_eq!(unpack(&input, expected.len()), Some(expected));
    }

    #[test]
    fn runs_carry_across_rows() {
        let mut decoder = ByteRun1::new(&[0xfb, 5, 1, 1, 2]);
        let mut row = [0; 4];
        assert!(decoder.fill(&mut row));
        assert_eq!(row, [5; 4]);
        assert!(decoder.fill(&mut row));
        assert_eq!(row, [5, 5, 1, 2]);
        assert!(!decoder.fill(&mut row));
    }

    #[test]
    fn stops_cleanly_when_input_runs_dry() {
        assert_eq!(unpack(&[5, 1, 2], 6), None);
        assert_eq!(unpack(&[0xfe], 3), None);
        assert_eq!(unpack(&[128, 128], 1), None);
        assert_eq!(unpack(&[], 0), Some(vec![]));
    }

    #[test]
    fn overrunning_runs_are_clipped() {
        assert_eq!(unpack(&[0x81, 3], 4), Some(vec![3; 4]));
        assert_eq!(unpack(&[127; 129], 2), Some(vec![127; 2]));
    }
}
