//! Seeds built from scratch, for formats whose real files are too big to mutate quickly.

/// A 16×16, two-frame Smacker file with empty Huffman trees, one palette change and a raw
/// 8-bit mono audio track. The shipped videos carry about 280 KB of trees, which would
/// otherwise dominate every Smacker seed.
pub fn smacker() -> Vec<u8> {
    const AUDIO_PRESENT: u32 = 1 << 30;
    const SAMPLE_RATE: u32 = 11_025;
    const SAMPLES: [u8; 8] = [128, 144, 160, 144, 128, 112, 96, 112];

    let mut audio = u32::try_from(4 + SAMPLES.len())
        .unwrap_or(0)
        .to_le_bytes()
        .to_vec();
    audio.extend(SAMPLES);
    // A palette chunk of one 4-byte unit: a single new colour, full red.
    let mut first = vec![1, 63, 0, 0];
    first.extend(&audio);
    let frames = [pad(first), pad(audio)];

    let mut out = b"SMK2".to_vec();
    // Width, height, frame count, milliseconds per frame, flags.
    for word in [16, 16, 2, 100, 0] {
        out.extend(u32::to_le_bytes(word));
    }
    let mut max_chunk = [0u32; 7];
    max_chunk[0] = 64;
    out.extend(max_chunk.iter().flat_map(|size| size.to_le_bytes()));
    // A one-byte tree blob, then the four tree buffer sizes the parser skips.
    out.extend(1u32.to_le_bytes());
    out.extend([0; 16]);
    let mut rates = [0u32; 7];
    rates[0] = AUDIO_PRESENT | SAMPLE_RATE;
    out.extend(rates.iter().flat_map(|rate| rate.to_le_bytes()));
    out.extend([0; 4]);
    // Bit 0 of a frame size marks a keyframe.
    for (keyframe, frame) in [1, 0].into_iter().zip(&frames) {
        out.extend((u32::try_from(frame.len()).unwrap_or(0) | keyframe).to_le_bytes());
    }
    // Frame types: bit 0 a palette chunk, bit 1 audio track 0.
    out.extend([0b11, 0b10]);
    // The tree blob: every presence bit clear, so all four trees are absent.
    out.push(0);
    out.extend(frames.concat());
    out
}

fn pad(mut frame: Vec<u8>) -> Vec<u8> {
    frame.resize(frame.len().next_multiple_of(4), 0);
    frame
}
