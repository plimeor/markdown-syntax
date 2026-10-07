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

/// Cases that differ by design: the oracle reads them under another dialect,
/// or commonmark.js, which this crate follows, renders them otherwise. A case
/// whose input reads as a construct only one side has (one this syntax drops,
/// or one the oracle lacks, such as literal autolinks, wiki links, or
/// frontmatter under a CommonMark oracle, or math blocks under comrak) is
/// removed from the suite instead:
/// the two outputs cannot be compared. So is a case whose oracle turns off a
/// construct this syntax keeps (indented code, HTML blocks) and that
/// commonmark.js, with the construct on, renders as this crate does.
pub const DEVIATIONS: &[Listed] = &[
    case("commonmark/gfm_table.cases", "-", 0x344b3ce2752a24d5, "| a |\n| - |\n| b |", "tables are part of the syntax; the oracle reads the rows as text"),
    case("commonmark/gfm_task_list_item.cases", "-", 0xa3f7f4ff52863012, "* [x] y.", "task list items are part of the syntax; the oracle reads `[x]` as text"),
    case("gfm/inline_footnotes.cases", "extension.inline_footnotes", 0x23538404ce8325cd, "Text^[note] should not parse.\n", "inline footnotes are part of the syntax; the oracle runs without footnotes"),
    case("commonmark/code_fenced.cases", "-", 0x8be7f99eb116458d, "  ```\n ", "an unclosed fence whose last line holds only whitespace and no line ending renders that line as an empty content line; the oracle renders the fence empty; this crate follows commonmark.js, as block-syntax \"CommonMark oracle cases\" requires"),
    case("commonmark/fuzz.cases", "-", 0x9165948c43745101, "> ```\n", "a fence in a block quote that the end of input closes right after its opening line renders empty; the oracle renders one empty content line; this crate follows commonmark.js, as block-syntax \"CommonMark oracle cases\" requires"),
    case("commonmark/fuzz.cases", "-", 0x5b4ae38f093b60d6, "- ```\n", "a fence in a list item that the end of input closes right after its opening line renders empty; the oracle renders one empty content line; this crate follows commonmark.js, as block-syntax \"CommonMark oracle cases\" requires"),
    case("commonmark/list.cases", "-", 0x57ecef8531311db8, "- ```\n   \n  ```", "a whitespace-only line in a fence in a list item keeps no whitespace past the item's indentation; the oracle keeps it; this crate follows commonmark.js, as block-syntax \"CommonMark oracle cases\" requires"),
    case("commonmark/list.cases", "-", 0x68dbf208da8c60bc, "- ```\n    \n  ```", "a whitespace-only line in a fence in a list item keeps no whitespace past the item's indentation; the oracle keeps it; this crate follows commonmark.js, as block-syntax \"CommonMark oracle cases\" requires"),
    case("commonmark/list.cases", "-", 0xcfbe10e540f5a44f, "- ```\n\t\n  ```", "a whitespace-only line in a fence in a list item keeps no whitespace past the item's indentation; the oracle keeps it; this crate follows commonmark.js, as block-syntax \"CommonMark oracle cases\" requires"),
    case("commonmark/gfm_autolink_literal.cases", "gfm", 0xe753952115ebba0b, "\n[ www.example.com\n\n[ https://example.co", "a URL after a `[` no `]` has closed yet stays text, as in cmark-gfm; the markdown-rs oracle links it"),
    case("commonmark/gfm_autolink_literal.cases", "gfm", 0x9a69188b3123a018, "H0.\n\n[https://a.com&copy;b\n\n[www.a.com&c", "a URL after a `[` no `]` has closed yet stays text, as in cmark-gfm; the markdown-rs oracle links it"),
    case("commonmark/gfm_autolink_literal.cases", "gfm", 0xf37a7f8b146691cf, "Image start.\n\n![https://a.com\n\n![http://", "a URL after a `[` no `]` has closed yet stays text, as in cmark-gfm; the markdown-rs oracle links it"),
    case("commonmark/gfm_autolink_literal.cases", "gfm", 0xf889ed9b7afa71fe, "Link start.\n\n[https://a.com\n\n[http://a.c", "a URL after a `[` no `]` has closed yet stays text, as in cmark-gfm; the markdown-rs oracle links it"),
    case("commonmark/gfm_autolink_literal.cases", "gfm", 0xceb3e904adfbe2a3, "[www.example.com/a&copy;](#)\n\nwww.exampl", "a URL after a `[` no `]` has closed yet stays text, as in cmark-gfm; the markdown-rs oracle links it"),
    case("commonmark/gfm_autolink_literal.cases", "gfm", 0x687e2b89bad6463c, "# HTTP\n\nhttps://a.b can start after EOF\n", "a URL after a `[` no `]` has closed yet stays text, as in cmark-gfm; the markdown-rs oracle links it"),
];

/// Cases that fail as parser defects, not by design. A case belongs here only
/// when its Markdown, read as its author wrote it, means what the oracle
/// renders; a different result alone is not a defect.
pub const KNOWN_DEFECTS: &[Listed] = &[];
