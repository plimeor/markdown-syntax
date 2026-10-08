//! Seeded round-trip generators: documents built from pieces of inline,
//! block, and emphasis syntax are parsed, serialized, and parsed again. Each
//! generated document reads back as the parsed one, or is listed with the
//! reason it does not.

#[path = "support/normalize.rs"]
mod normalize;

use markdown_syntax::parse;

/// A small deterministic xorshift generator, so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

const INLINE_PIECES: &[&str] = &[
    "a",
    "b",
    "目",
    " ",
    "  ",
    "\t",
    "\n",
    "*",
    "_",
    "**",
    "__",
    "~",
    "~~",
    "=",
    "==",
    "+",
    "++",
    "^",
    "|",
    "||",
    "$",
    "$$",
    "`",
    "``",
    "[",
    "]",
    "(",
    ")",
    "<",
    ">",
    "!",
    "\\",
    "&amp;",
    "&#x20;",
    "&#42;",
    ":",
    ":e",
    ":smile:",
    "{",
    "}",
    "http://x",
    "www.x.com",
    "a@b.c",
    "://",
    "[^1]",
    "^[",
    "[[",
    "]]",
    "![",
    "<a>",
    "</a>",
    "<!--",
    "-->",
    "#",
    "-",
    ".",
    ",",
];

const BLOCK_PIECES: &[&str] = &[
    "a",
    "b c",
    "*a*",
    " ",
    "\t",
    "\n",
    "\n\n",
    "- ",
    "* ",
    "+ ",
    "1. ",
    "2) ",
    "> ",
    ">",
    "# ",
    "## ",
    "    ",
    "  ",
    "---",
    "***",
    "===",
    "```",
    "~~~",
    "$$",
    ":::e",
    "::e",
    ":::",
    "<div>",
    "</div>",
    "<details>",
    "</details>",
    "<!--",
    "-->",
    "[a]: /u",
    "[^1]: ",
    "| a |",
    "|-|",
    "| - |",
    ": ",
    "~ ",
    ">[!NOTE]",
    "[x] ",
    "[ ] ",
    "{",
    "}",
    "import x",
];

const EMPHASIS_PIECES: &[&str] = &[
    "*", "_", "**", "__", "***", "___", "a", "b", " ", "(", ")", ".", "#", "&", "~", "$", "|",
];

/// The generators, each with its pieces, the most pieces in an input, and
/// its recorded seed.
const GENERATORS: &[(&str, &[&str], usize, u64)] = &[
    ("inline", INLINE_PIECES, 10, 0x1d1e_5eed),
    ("block", BLOCK_PIECES, 12, 0xb10c_5eed),
    ("emphasis", EMPHASIS_PIECES, 12, 0xe3fa_5eed),
];

const INPUTS_PER_GENERATOR: usize = 2_000;

/// Generated inputs whose Markdown reads back as a different tree, with the
/// reason.
///
/// Each difference is whitespace or a blank line that the AST does not
/// record, as decision 0008 (Consequences) accepts.
const NOT_READING_BACK: &[(&str, &str)] = &[
    (
        "__$$++\t\\\t\n}$==",
        "the tab between a final backslash and the line ending, which keeps the \
         backslash from making a hard break, is not recorded",
    ),
    (
        ">\n>[!NOTE]: }",
        "the empty first line of the quote, which keeps `[!NOTE]` from opening an \
         alert, is not recorded",
    ),
    (
        "-->\n\t$$[^1]: -->```:::- ",
        "the tab indenting the paragraph's continuation line, which keeps `$$` \
         from opening a math block, is not recorded",
    ),
];

fn generated(pieces: &[&str], max_pieces: usize, seed: u64) -> Vec<String> {
    let mut rng = Rng(seed);
    (0..INPUTS_PER_GENERATOR)
        .map(|_| {
            let count = 1 + rng.below(max_pieces);
            (0..count)
                .map(|_| pieces[rng.below(pieces.len())])
                .collect()
        })
        .collect()
}

/// Whether `input` reads back: its Markdown parses as the same tree, and
/// serializes to the same Markdown again. Panics if it does not serialize.
fn reads_back(input: &str) -> Result<(), String> {
    let document = parse(input).document;
    let markdown = document
        .to_markdown()
        .unwrap_or_else(|error| panic!("{input:?}: {error:?}"));
    let reparsed = parse(&markdown).document;
    if normalize::normalized(&reparsed.children) != normalize::normalized(&document.children) {
        return Err(format!("{input:?} -> {markdown:?}"));
    }
    match reparsed.to_markdown() {
        Ok(again) if again == markdown => Ok(()),
        other => Err(format!("{input:?} -> {markdown:?} -> {other:?}")),
    }
}

#[test]
fn generated_documents_round_trip() {
    let mut failures = Vec::new();
    let mut listed_seen = Vec::new();
    for &(generator, pieces, max_pieces, seed) in GENERATORS {
        for input in generated(pieces, max_pieces, seed) {
            let listed = NOT_READING_BACK.iter().any(|(listed, _)| *listed == input);
            match (reads_back(&input), listed) {
                (Ok(()), false) => {}
                (Err(_), true) => listed_seen.push(input),
                (Err(failure), false) => failures.push(format!("{generator} {failure}")),
                (Ok(()), true) => {
                    failures.push(format!("{generator} {input:?}: listed, but reads back"))
                }
            }
        }
    }
    for (listed, _) in NOT_READING_BACK {
        if !listed_seen.iter().any(|seen| seen == listed) {
            failures.push(format!("{listed:?}: listed, but not generated"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
