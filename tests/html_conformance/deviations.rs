//! The conformance cases whose expected HTML differs from this crate's, each
//! with its reason.
//!
//! An entry names its case by content, not by its number in the file, so it
//! keeps naming the same case when the vendored oracle files gain, lose, or
//! reorder cases: the `.cases` file, the case's option tokens as its header
//! writes them (`-` for none), and the FNV-1a hash of its Markdown input. The
//! excerpt is the start of that input, kept so a reader can find the case; it
//! must match too.
//!
//! `exception_lists_are_current` fails on an entry that names no case or whose
//! case now passes. A failing case no entry names does not fail it; the report
//! prints that case with an entry ready to fill in.

/// One listed case and why it differs from its oracle.
pub struct Listed {
    pub file: &'static str,
    pub options: &'static str,
    pub input_hash: u64,
    pub excerpt: &'static str,
    pub reason: &'static str,
}

const fn case(
    file: &'static str,
    options: &'static str,
    input_hash: u64,
    excerpt: &'static str,
    reason: &'static str,
) -> Listed {
    Listed {
        file,
        options,
        input_hash,
        excerpt,
        reason,
    }
}

impl Listed {
    /// Whether this entry names the case `input` of `file` under `options`.
    pub fn names(&self, file: &str, options: &str, input: &str) -> bool {
        self.file == file
            && self.options == options
            && self.input_hash == input_hash(input)
            && input.starts_with(self.excerpt)
    }
}

/// FNV-1a (64-bit) of the input's bytes: stable across platforms and Rust
/// versions, so a listed hash never moves unless the input does.
pub fn input_hash(input: &str) -> u64 {
    input.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The start of the input an entry quotes: its first 40 chars.
pub fn excerpt(input: &str) -> &str {
    match input.char_indices().nth(40) {
        Some((end, _)) => &input[..end],
        None => input,
    }
}

/// Cases that differ from their oracle by design. An oracle case whose
/// difference follows a decision of this syntax (a construct the syntax drops,
/// one the oracle lacks or turns off, or a rule this crate shares with another
/// reference renderer) is removed from the suite instead, and its input, with
/// HTML verified against a reference renderer or the decision, is checked by
/// `tests/syntax_decisions.rs`. An entry here waits for that move.
pub const DEVIATIONS: &[Listed] = &[];

/// Cases that fail as parser defects, not by design. A case belongs here only
/// when its Markdown, read as its author wrote it, means what the oracle
/// renders; a different result alone is not a defect.
pub const KNOWN_DEFECTS: &[Listed] = &[];
