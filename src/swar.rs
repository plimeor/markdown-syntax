//! Finding bytes eight at a time.

/// The position of the first byte of `bytes` from `from` on that is one of
/// `targets`, or `bytes.len()` when there is none. Eight bytes are tested at
/// once: a byte equal to a target leaves a zero byte in the word the target
/// is xored into, and the lowest zero byte of a word is the lowest one whose
/// high bit is set in `(word - 0x01..) & !word & 0x80..` (a higher one may be
/// set without being zero, never a lower one).
#[inline]
pub(crate) fn find_any<const N: usize>(bytes: &[u8], from: usize, targets: [u8; N]) -> usize {
    const ONES: u64 = u64::from_le_bytes([0x01; 8]);
    const HIGHS: u64 = u64::from_le_bytes([0x80; 8]);
    let mut index = from;
    while let Some(chunk) = bytes.get(index..index + 8) {
        let word = u64::from_le_bytes(chunk.try_into().expect("eight bytes"));
        let mut zeros = 0;
        for target in targets {
            let xored = word ^ (ONES * u64::from(target));
            zeros |= xored.wrapping_sub(ONES) & !xored;
        }
        let found = zeros & HIGHS;
        if found != 0 {
            return index + (found.trailing_zeros() / 8) as usize;
        }
        index += 8;
    }
    bytes[index..]
        .iter()
        .position(|byte| targets.contains(byte))
        .map_or(bytes.len(), |offset| index + offset)
}

/// The high bit of each byte of `word` that equals `target`, and no other
/// bit: xored with the target, a byte is zero exactly when its high bit is
/// clear in `((xored & 0x7f..) + 0x7f..) | xored`, as no byte's sum carries
/// into the next.
#[inline]
fn equal_bytes(word: u64, target: u8) -> u64 {
    const ONES: u64 = u64::from_le_bytes([0x01; 8]);
    const LOWS: u64 = u64::from_le_bytes([0x7f; 8]);
    let xored = word ^ (ONES * u64::from(target));
    !(((xored & LOWS) + LOWS) | xored | LOWS)
}

/// Calls `found` with the position of the byte each high bit of `matches`
/// is the high bit of, in a word read at `index`, in ascending order.
#[inline]
fn each_match(index: usize, mut matches: u64, found: &mut impl FnMut(usize)) {
    while matches != 0 {
        found(index + (matches.trailing_zeros() / 8) as usize);
        matches &= matches - 1;
    }
}

/// Calls `found` with the position of every byte of `bytes` that is one of
/// `targets`, in ascending order, testing eight bytes at once.
#[inline]
pub(crate) fn each_any<const N: usize>(
    bytes: &[u8],
    targets: [u8; N],
    mut found: impl FnMut(usize),
) {
    let mut chunks = bytes.chunks_exact(8);
    let mut index = 0;
    for chunk in &mut chunks {
        let word = u64::from_le_bytes(chunk.try_into().expect("eight bytes"));
        let matches = targets
            .iter()
            .fold(0, |matches, target| matches | equal_bytes(word, *target));
        each_match(index, matches, &mut found);
        index += 8;
    }
    for (offset, byte) in chunks.remainder().iter().enumerate() {
        if targets.contains(byte) {
            found(index + offset);
        }
    }
}

/// Calls `found` with every position of `bytes` at which `pair` starts, in
/// ascending order, testing eight positions at once: the words read there and
/// one byte later hold the pair's first and second byte at the same places.
#[inline]
pub(crate) fn each_pair(bytes: &[u8], [first, second]: [u8; 2], mut found: impl FnMut(usize)) {
    let mut index = 0;
    while let Some(window) = bytes.get(index..index + 9) {
        let here = u64::from_le_bytes(window[..8].try_into().expect("eight bytes"));
        let next = u64::from_le_bytes(window[1..].try_into().expect("eight bytes"));
        each_match(
            index,
            equal_bytes(here, first) & equal_bytes(next, second),
            &mut found,
        );
        index += 8;
    }
    for (offset, window) in bytes[index..].windows(2).enumerate() {
        if window == [first, second] {
            found(index + offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{each_any, each_pair, find_any};

    /// Every length up to two words and one byte, each target at every
    /// position alone and before another, among bytes that a word test could
    /// take for a target (zero, a target with its low bit flipped, a high
    /// byte) or among targets, searched from every start, as `position` finds
    /// it, and all found as `filter` finds them.
    fn finds_as_position<const N: usize>(targets: [u8; N]) {
        let mut fillers = Vec::from([b'a', 0, 0x80, 0xff, 0x7f]);
        fillers.extend(targets.iter().map(|target| target ^ 1));
        fillers.extend(targets);
        for len in 0..=17 {
            for &filler in &fillers {
                for &target in &targets {
                    for first in 0..=len {
                        for second in first..=len {
                            let mut bytes = alloc::vec![filler; len];
                            if second < len {
                                bytes[second] = targets[N - 1];
                            }
                            if first < len {
                                bytes[first] = target;
                            }
                            let mut found = Vec::new();
                            each_any(&bytes, targets, |position| found.push(position));
                            let expected: Vec<usize> = (0..len)
                                .filter(|position| targets.contains(&bytes[*position]))
                                .collect();
                            assert_eq!(found, expected, "{bytes:?}");
                            for from in 0..=len {
                                let expected = bytes[from..]
                                    .iter()
                                    .position(|byte| targets.contains(byte))
                                    .map_or(len, |offset| from + offset);
                                assert_eq!(
                                    find_any(&bytes, from, targets),
                                    expected,
                                    "{bytes:?} from {from}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn line_breaks_are_found_as_position_finds_them() {
        finds_as_position([b'\n', b'\r']);
    }

    #[test]
    fn html_specials_are_found_as_position_finds_them() {
        finds_as_position([b'&', b'<', b'>', b'"']);
    }

    /// Every string of up to nineteen bytes over the pair's bytes, a byte
    /// with the low bit of one flipped, and a high byte, as `windows` finds
    /// the pair in it.
    fn finds_pairs_as_windows(pair: [u8; 2]) {
        let alphabet = [pair[0], pair[1], pair[0] ^ 1, 0x80];
        let mut rng = 0x9E37_79B9_7F4A_7C15u64;
        for len in 0..=19 {
            for _ in 0..400 {
                let bytes: Vec<u8> = (0..len)
                    .map(|_| {
                        rng ^= rng << 13;
                        rng ^= rng >> 7;
                        rng ^= rng << 17;
                        alphabet[(rng % 4) as usize]
                    })
                    .collect();
                let mut found = Vec::new();
                each_pair(&bytes, pair, |position| found.push(position));
                let expected: Vec<usize> = bytes
                    .windows(2)
                    .enumerate()
                    .filter(|(_, window)| *window == pair)
                    .map(|(position, _)| position)
                    .collect();
                assert_eq!(found, expected, "{bytes:?}");
            }
        }
    }

    #[test]
    fn pairs_are_found_as_windows_finds_them() {
        for pair in [*b"$$", *b"`$", *b"--", *b"?>", *b"]]", [0, 0xff]] {
            finds_pairs_as_windows(pair);
        }
    }

    #[test]
    fn scan_table_bytes_are_found_as_position_finds_them() {
        for target in [b'$', b'`', b'>', b'-', b'?', b']', b'"', b'\'', b'@'] {
            finds_as_position([target]);
        }
        finds_as_position([b'<', b'>', b'\n', b'\r']);
    }
}
