//! Seeded round-trip generators: documents built from pieces of inline,
//! block, and emphasis syntax are parsed, serialized under the same dialect,
//! and parsed again. Each generated document reads back as the parsed one,
//! or is listed as unrepresentable with its reason.

#[path = "support/normalize.rs"]
mod normalize;

use markdown_syntax::{SerializeError, SerializeOptions, SyntaxOptions};

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

/// Generated inputs whose parse no Markdown the serializer writes reads
/// back as, with the reason.
const UNREPRESENTABLE: &[(&str, &str)] = &[];

fn dialects() -> [(&'static str, SyntaxOptions); 4] {
    [
        ("commonmark", SyntaxOptions::commonmark()),
        ("gfm", SyntaxOptions::gfm()),
        ("default", SyntaxOptions::default()),
        ("mdx", SyntaxOptions::mdx()),
    ]
}

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

#[test]
fn generated_documents_round_trip_in_each_dialect() {
    let mut failures = Vec::new();
    for (dialect, options) in dialects() {
        let mut serialize = SerializeOptions::default();
        serialize.syntax = options.clone();
        for &(generator, pieces, max_pieces, seed) in GENERATORS {
            for input in generated(pieces, max_pieces, seed) {
                let document = options.parse(&input).document;
                let markdown = match document.to_markdown_with(&serialize) {
                    Ok(markdown) => markdown,
                    // A parsed document has its source as one spelling, so
                    // it is unrepresentable only where listed with a reason.
                    Err(SerializeError::Unrepresentable(_))
                        if UNREPRESENTABLE.iter().any(|(listed, _)| *listed == input) =>
                    {
                        continue
                    }
                    Err(error) => {
                        failures.push(format!("{dialect} {generator} {input:?}: {error:?}"));
                        continue;
                    }
                };
                let reparsed = options.parse(&markdown).document;
                if normalize::normalized(&reparsed.children)
                    != normalize::normalized(&document.children)
                {
                    failures.push(format!("{dialect} {generator} {input:?} -> {markdown:?}"));
                    continue;
                }
                match reparsed.to_markdown_with(&serialize) {
                    Ok(again) if again == markdown => {}
                    other => failures.push(format!(
                        "{dialect} {generator} {input:?} -> {markdown:?} -> {other:?}"
                    )),
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
