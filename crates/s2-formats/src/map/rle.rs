//! The escape-byte run-length coding S2.EXE uses for layers in the compact container.
//!
//! A stream starts with its escape byte. After that, `escape, n, value` with `n >= 4` expands to
//! `n` copies of `value`; `escape, n` with `n < 4` is one literal escape byte (the encoder always
//! writes `n = 0`); any other byte is a literal.

/// Why a run-length stream could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RleError {
    /// The stream has no escape byte.
    #[error("RLE stream is empty")]
    Empty,
    /// The stream ends partway through an escape sequence.
    #[error("RLE stream ends inside an escape sequence at offset {offset}")]
    TruncatedRun {
        /// Offset of the escape byte that starts the unfinished sequence.
        offset: usize,
    },
    /// The stream expands past the expected length.
    #[error("RLE output overruns the expected {expected} bytes")]
    Overrun {
        /// The length the caller asked for.
        expected: usize,
    },
    /// The stream ends before producing the expected length.
    #[error("RLE output is {actual} bytes, expected {expected}")]
    Short {
        /// Bytes produced.
        actual: usize,
        /// The length the caller asked for.
        expected: usize,
    },
}

/// The largest output a single input byte can justify: three bytes expand to at most 255.
const MAX_EXPANSION: usize = 85;

/// Shortest run the encoder turns into an escape sequence.
const MIN_RUN: u8 = 4;

/// Decodes `src` into exactly `expected` bytes.
///
/// # Errors
///
/// [`RleError`] when the stream is empty, stops inside an escape sequence, or produces more or
/// fewer than `expected` bytes. Decoding stops at the first byte past `expected`.
pub fn decode(src: &[u8], expected: usize) -> Result<Vec<u8>, RleError> {
    let (&escape, _) = src.split_first().ok_or(RleError::Empty)?;
    let mut out = Vec::with_capacity(expected.min(src.len().saturating_mul(MAX_EXPANSION)));
    let mut put = |value: u8, count: usize| {
        if count > expected - out.len() {
            return Err(RleError::Overrun { expected });
        }
        out.resize(out.len() + count, value);
        Ok(())
    };
    let mut bytes = src.iter().copied().enumerate().skip(1);
    while let Some((offset, byte)) = bytes.next() {
        if byte != escape {
            put(byte, 1)?;
            continue;
        }
        let truncated = RleError::TruncatedRun { offset };
        let (_, count) = bytes.next().ok_or(truncated)?;
        if count < MIN_RUN {
            put(escape, 1)?;
        } else {
            let (_, value) = bytes.next().ok_or(truncated)?;
            put(value, usize::from(count))?;
        }
    }
    if out.len() == expected {
        Ok(out)
    } else {
        Err(RleError::Short {
            actual: out.len(),
            expected,
        })
    }
}

/// Encodes `src` the way S2.EXE does, for round-trip tests.
#[cfg(test)]
pub(crate) fn encode(src: &[u8]) -> Vec<u8> {
    let mut histogram = [0_usize; 256];
    for &b in src {
        histogram[usize::from(b)] += 1;
    }
    let escape = (0..=u8::MAX)
        .min_by_key(|&b| histogram[usize::from(b)])
        .unwrap_or(0);
    let mut out = vec![escape];
    let mut rest = src;
    while let Some(&value) = rest.first() {
        let run = rest.iter().take(255).take_while(|&&b| b == value).count();
        if value == escape {
            for _ in 0..run {
                out.extend([escape, 0]);
            }
        } else if run >= usize::from(MIN_RUN) {
            out.extend([escape, u8::try_from(run).unwrap_or(u8::MAX), value]);
        } else {
            out.extend(&rest[..run]);
        }
        rest = &rest[run..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_runs_and_escaped_literals() {
        assert_eq!(
            decode(&[9, 1, 9, 5, 7, 9, 0, 2], 8),
            Ok(vec![1, 7, 7, 7, 7, 7, 9, 2])
        );
        assert_eq!(decode(&[9, 9, 3], 1), Ok(vec![9]));
    }

    #[test]
    fn round_trips_the_encoder() {
        let mut src: Vec<u8> = (0..=255).collect();
        src.extend([5; 300]);
        src.extend([0, 0, 0, 1, 1, 1, 1]);
        let packed = encode(&src);
        assert!(packed.len() < src.len());
        assert_eq!(decode(&packed, src.len()), Ok(src));
    }

    #[test]
    fn rejects_malformed_streams() {
        assert_eq!(decode(&[], 1), Err(RleError::Empty));
        assert_eq!(decode(&[9, 1, 9], 4), Err(RleError::TruncatedRun { offset: 2 }));
        assert_eq!(decode(&[9, 9, 200], 4), Err(RleError::TruncatedRun { offset: 1 }));
        assert_eq!(decode(&[9, 9, 200, 1], 4), Err(RleError::Overrun { expected: 4 }));
        assert_eq!(
            decode(&[9, 1, 2], 4),
            Err(RleError::Short {
                actual: 2,
                expected: 4
            })
        );
    }

    #[test]
    fn huge_claims_do_not_allocate_up_front() {
        assert_eq!(
            decode(&[9, 1], usize::MAX),
            Err(RleError::Short {
                actual: 1,
                expected: usize::MAX
            })
        );
    }
}
