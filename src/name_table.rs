//! Lookup tables of names, each with a text, built at compile time from a
//! `const` list of `(name, text)` pairs sorted by name, with no pointer per
//! entry, which keeps the WebAssembly binding small.
//!
//! A table writes its entries one after another as records, in blocks of up
//! to `BLOCK` entries whose names start with the same byte. A record is the
//! length of the prefix its name shares with the name before it in the
//! block, the length of the rest of the name, the length of the text, the
//! rest of the name, and the text. Each length is a byte below `0x80` and
//! every name is ASCII, so the records are UTF-8 and a text is a slice of
//! them. A lookup searches the blocks of its name's first byte by their first
//! names, which share nothing, and walks the one block that may hold the
//! name.
//!
//! `name_table!` reads the list twice: `sizes` checks it and counts the
//! bytes of the records and the blocks, and `parts` writes them. The build
//! interprets both, and a call there, even to `len`, costs as much as many
//! other steps, so their loops read each length once.

/// The most entries a block holds.
const BLOCK: usize = 8;

/// Names sorted by their bytes, distinct, each with a text; built by
/// `name_table!`.
pub(crate) struct NameTable {
    /// The records.
    records: &'static str,
    /// Where each block's first record starts.
    block_starts: &'static [u16],
    /// For each byte, the index of the first block whose names start with it
    /// or a later byte; the last slot is the number of blocks.
    first_byte_starts: &'static [u16; 257],
}

impl NameTable {
    /// The table `parts` holds.
    pub(crate) const fn new<const RECORDS: usize, const BLOCKS: usize>(
        parts: &'static Parts<RECORDS, BLOCKS>,
    ) -> Self {
        let records = match core::str::from_utf8(&parts.records) {
            Ok(records) => records,
            Err(_) => panic!("records of ASCII names and UTF-8 texts are UTF-8"),
        };
        NameTable {
            records,
            block_starts: &parts.block_starts,
            first_byte_starts: &parts.first_byte_starts,
        }
    }

    /// The text of `name`.
    pub(crate) fn get(&self, name: &str) -> Option<&'static str> {
        let name = name.as_bytes();
        let first = usize::from(*name.first()?);
        let mut block = usize::from(self.first_byte_starts[first]);
        let mut blocks = usize::from(self.first_byte_starts[first + 1]) - block;
        if blocks == 0 {
            return None;
        }
        // The last of the blocks whose first name is at most `name`, or the
        // first block; every name there starts with the byte `name` does.
        while blocks > 1 {
            let half = blocks / 2;
            if at_most(&self.first_name(block + half)[1..], &name[1..]) {
                block += half;
            }
            blocks -= half;
        }
        self.find_in_block(block, name)
    }

    /// The name of the first record of `block`, which shares nothing.
    fn first_name(&self, block: usize) -> &'static [u8] {
        let records = self.records.as_bytes();
        let at = usize::from(self.block_starts[block]);
        &records[at + 3..at + 3 + usize::from(records[at + 1])]
    }

    /// The text of `name` in `block`. Each record's name is compared from
    /// the bytes `matched` that the name before it shares with `name`: a name
    /// that shares more with the one before is smaller than `name` as that
    /// one is, and one that shares less is larger.
    fn find_in_block(&self, block: usize, name: &[u8]) -> Option<&'static str> {
        let records = self.records.as_bytes();
        let mut at = usize::from(self.block_starts[block]);
        let end = self
            .block_starts
            .get(block + 1)
            .map_or(records.len(), |end| usize::from(*end));
        let mut matched = 0;
        while at < end {
            let shared = usize::from(records[at]);
            let rest = at + 3..at + 3 + usize::from(records[at + 1]);
            let text = rest.end..rest.end + usize::from(records[at + 2]);
            if shared < matched {
                return None;
            }
            if shared == matched {
                let rest = &records[rest];
                let wanted = &name[matched..];
                let mut common = 0;
                for (byte, wanted) in rest.iter().zip(wanted) {
                    if byte != wanted {
                        if byte > wanted {
                            return None;
                        }
                        break;
                    }
                    common += 1;
                }
                if common == wanted.len() {
                    return (common == rest.len()).then(|| &self.records[text]);
                }
                matched += common;
            }
            at = text.end;
        }
        None
    }
}

/// Whether `one` orders at or before `other`, compared a byte at a time:
/// names are too short for a call to pay.
#[inline]
fn at_most(one: &[u8], other: &[u8]) -> bool {
    for (one, other) in one.iter().zip(other) {
        if one != other {
            return one < other;
        }
    }
    one.len() <= other.len()
}

/// The length of the prefix `name` shares with `before`.
const fn common_prefix_len(before: &[u8], name: &[u8]) -> usize {
    let (before_len, name_len) = (before.len(), name.len());
    let max = if before_len < name_len {
        before_len
    } else {
        name_len
    };
    let mut len = 0;
    while len < max && before[len] == name[len] {
        len += 1;
    }
    len
}

/// Whether a name that shares `common` bytes with the name before it, which
/// is the last of `in_block` entries of its block, starts a block: the first
/// name and one with another first byte than the name before share nothing
/// with it, and a block holds `BLOCK` entries at most.
const fn starts_block(common: usize, in_block: usize) -> bool {
    common == 0 || in_block == BLOCK
}

/// The sizes of the parts of a table, as `sizes` counts them.
pub(crate) struct Sizes {
    /// The length of the records.
    pub(crate) records: usize,
    /// The number of blocks.
    pub(crate) blocks: usize,
}

/// The sizes of the parts of the table of `entries`; the build fails for
/// entries that are not sorted and distinct, a name that is empty or not
/// ASCII, a name or text longer than `0x7f` bytes, or a block start or index
/// past what a `u16` holds.
pub(crate) const fn sizes(entries: &[(&str, &str)]) -> Sizes {
    let mut records = 0;
    let mut blocks = 0;
    let mut in_block = 0;
    let mut before: &[u8] = &[];
    let mut index = 0;
    let len = entries.len();
    while index < len {
        let name = entries[index].0.as_bytes();
        let (name_len, text_len) = (name.len(), entries[index].1.len());
        assert!(name_len > 0, "no name is empty");
        assert!(name_len < 0x80 && text_len < 0x80, "lengths fit below 0x80");
        let common = common_prefix_len(before, name);
        assert!(
            index == 0
                || (common < name_len && (common == before.len() || before[common] < name[common])),
            "names are sorted and distinct"
        );
        // The bytes `name` shares with `before` were checked with it.
        let mut at = common;
        while at < name_len {
            assert!(name[at] < 0x80, "names are ASCII");
            at += 1;
        }
        let shared = if starts_block(common, in_block) {
            blocks += 1;
            in_block = 0;
            0
        } else {
            common
        };
        in_block += 1;
        records += 3 + name_len - shared + text_len;
        before = name;
        index += 1;
    }
    assert!(
        records <= u16::MAX as usize,
        "every block start fits in a u16"
    );
    assert!(
        blocks <= u16::MAX as usize,
        "every block index fits in a u16"
    );
    Sizes { records, blocks }
}

/// The parts of a table, `RECORDS` bytes of records and `BLOCKS` blocks;
/// [`NameTable::new`] reads them.
pub(crate) struct Parts<const RECORDS: usize, const BLOCKS: usize> {
    /// The records.
    records: [u8; RECORDS],
    /// Where each block's first record starts.
    block_starts: [u16; BLOCKS],
    /// For each byte, the index of the first block whose names start with it
    /// or a later byte; the last slot is the number of blocks.
    first_byte_starts: [u16; 257],
}

/// The parts of the table of `entries`, which `sizes` checks, of the sizes it
/// counts.
pub(crate) const fn parts<const RECORDS: usize, const BLOCKS: usize>(
    entries: &[(&str, &str)],
) -> Parts<RECORDS, BLOCKS> {
    let mut parts = Parts {
        records: [0; RECORDS],
        block_starts: [0; BLOCKS],
        first_byte_starts: [0; 257],
    };
    let mut at = 0;
    let mut blocks = 0;
    let mut in_block = 0;
    // The first byte of `first_byte_starts` not yet written.
    let mut byte = 0;
    let mut before: &[u8] = &[];
    let mut index = 0;
    let len = entries.len();
    while index < len {
        let name = entries[index].0.as_bytes();
        let text = entries[index].1.as_bytes();
        let (name_len, text_len) = (name.len(), text.len());
        let common = common_prefix_len(before, name);
        let shared = if starts_block(common, in_block) {
            parts.block_starts[blocks] = at as u16;
            while byte <= name[0] as usize {
                parts.first_byte_starts[byte] = blocks as u16;
                byte += 1;
            }
            blocks += 1;
            in_block = 0;
            0
        } else {
            common
        };
        in_block += 1;
        parts.records[at] = shared as u8;
        parts.records[at + 1] = (name_len - shared) as u8;
        parts.records[at + 2] = text_len as u8;
        at += 3;
        let mut offset = shared;
        while offset < name_len {
            parts.records[at] = name[offset];
            at += 1;
            offset += 1;
        }
        offset = 0;
        while offset < text_len {
            parts.records[at] = text[offset];
            at += 1;
            offset += 1;
        }
        before = name;
        index += 1;
    }
    while byte < 257 {
        parts.first_byte_starts[byte] = blocks as u16;
        byte += 1;
    }
    assert!(
        at == RECORDS && blocks == BLOCKS,
        "the parts are as large as `sizes` counts"
    );
    parts
}

/// A [`NameTable`] of the `const` list `$entries` of `(name, text)` pairs,
/// sorted by name, built at compile time; the list itself is not kept.
macro_rules! name_table {
    ($entries:path) => {{
        use $crate::name_table::{parts, sizes, NameTable, Parts, Sizes};
        const SIZES: Sizes = sizes(&$entries);
        const PARTS: Parts<{ SIZES.records }, { SIZES.blocks }> = parts(&$entries);
        NameTable::new(&PARTS)
    }};
}

pub(crate) use name_table;

/// Checks that `get`, which reads the table of `entries`, finds each name's
/// text, and for a name one byte longer or shorter, or with its first,
/// middle, or last byte changed, what a search of `entries` finds.
#[cfg(test)]
pub(crate) fn assert_finds_what_the_list_holds(
    entries: &[(&str, &str)],
    get: fn(&str) -> Option<&'static str>,
) {
    use alloc::{format, string::String, vec, vec::Vec};

    assert!(entries.windows(2).all(|pair| pair[0].0 < pair[1].0));
    let find = |name: &str| {
        entries
            .iter()
            .find(|(entry, _)| *entry == name)
            .map(|(_, text)| *text)
    };
    for &(name, text) in entries {
        assert_eq!(get(name), Some(text), "{name}");
        let mut near: Vec<String> = vec![
            format!("{name}a"),
            format!("{name}\u{e9}"),
            format!("a{name}"),
            String::from(&name[..name.len() - 1]),
            String::from(&name[1..]),
        ];
        for byte in [b'+', b'0', b'Z', b'_', b'z', 0x7f] {
            for at in [0, name.len() / 2, name.len() - 1] {
                let mut bytes = name.as_bytes().to_vec();
                bytes[at] = byte;
                near.push(String::from_utf8(bytes).expect("ASCII names"));
            }
        }
        for near in near {
            assert_eq!(get(&near), find(&near), "{near:?}");
        }
    }
    assert_eq!(get(""), None);
}
