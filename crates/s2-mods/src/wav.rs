//! The effects' samples as WAV: unsigned 8-bit mono PCM, the form `SOUND.LST` holds them in.

/// The bytes of a RIFF chunk header: its id and its length.
const HEADER_BYTES: u32 = 8;
const CHUNK_HEADER: usize = HEADER_BYTES as usize;

/// The length of the `fmt ` chunk's body for plain PCM.
const FORMAT_LENGTH: u32 = 16;

/// The format tag for uncompressed PCM.
const PCM: u16 = 1;

/// One channel.
const MONO: u16 = 1;

/// Eight bits a sample.
const BITS: u16 = 8;

/// `samples`, unsigned 8-bit mono, as a WAV file at `rate` samples a second.
#[must_use]
pub fn write_wav(rate: u32, samples: &[u8]) -> Vec<u8> {
    let data = u32::try_from(samples.len()).unwrap_or(u32::MAX);
    let riff = (4 + HEADER_BYTES + FORMAT_LENGTH + HEADER_BYTES).saturating_add(data);
    let mut out = Vec::with_capacity(samples.len() + 44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&FORMAT_LENGTH.to_le_bytes());
    out.extend_from_slice(&PCM.to_le_bytes());
    out.extend_from_slice(&MONO.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&MONO.to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    out.extend_from_slice(samples);
    out
}

/// The samples of a WAV file, which must be unsigned 8-bit mono PCM: the form the game mixes.
///
/// # Errors
///
/// The reason, when the file is not a WAV or holds another kind of sample.
pub fn read_wav(bytes: &[u8]) -> Result<&[u8], &'static str> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("not a WAV file");
    }
    let mut at = 12;
    let mut format_ok = false;
    while let Some(header) = bytes.get(at..at + CHUNK_HEADER) {
        let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let length = usize::try_from(length).map_err(|_| "a chunk too long")?;
        let body = bytes
            .get(at + CHUNK_HEADER..at + CHUNK_HEADER + length)
            .ok_or("a chunk runs past the end")?;
        match &header[0..4] {
            b"fmt " => {
                let field = |at: usize| body.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
                if field(0) != Some(PCM) || field(2) != Some(MONO) || field(14) != Some(BITS) {
                    return Err("not unsigned 8-bit mono PCM");
                }
                format_ok = true;
            }
            b"data" if format_ok => return Ok(body),
            b"data" => return Err("the samples come before their format"),
            _ => {}
        }
        at += CHUNK_HEADER + length + length % 2;
    }
    Err("no samples")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_come_back_as_they_went_in() {
        let samples = [128, 129, 127, 0, 255];
        let file = write_wav(11_025, &samples);
        assert_eq!(read_wav(&file), Ok(&samples[..]));
    }

    #[test]
    fn sixteen_bit_samples_are_refused() {
        let mut file = write_wav(11_025, &[0, 0]);
        file[34] = 16;
        assert_eq!(read_wav(&file), Err("not unsigned 8-bit mono PCM"));
        assert_eq!(read_wav(b"RIFX"), Err("not a WAV file"));
    }
}
