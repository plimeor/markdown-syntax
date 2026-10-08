//! Deterministic generated inputs shared by the scan equivalence tests.

use alloc::{string::String, vec::Vec};

/// A small deterministic xorshift generator, so failures reproduce.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// Pieces the generated inputs are made of: every byte or token some scan
/// treats specially, plus ordinary and multi-byte text.
pub(crate) const PIECES: &[&str] = &[
    "[",
    "]",
    "[[",
    "]]",
    "\\",
    "\\[",
    "\\]",
    "`",
    "``",
    "```",
    "<",
    ">",
    "<a>",
    "</a>",
    "<a/>",
    "<a b=\"",
    "<a b='",
    "\"",
    "'",
    "<!--",
    "-->",
    "<?",
    "?>",
    "<![CDATA[",
    "]]>",
    "<!X",
    "<x:y>",
    "<http://x>",
    "<a@b.c>",
    "{",
    "}",
    "/",
    "//",
    "/*",
    "*/",
    "*",
    "(",
    ")",
    "](",
    "][",
    "^[",
    "[^",
    "^",
    "~",
    "~~",
    ":",
    ":a[",
    ":a{",
    "$",
    "$`",
    "`$",
    "==",
    "++",
    "__",
    "___",
    "_",
    "&",
    "&amp;",
    "&#x41;",
    "&#65;",
    "&a",
    ";",
    "@",
    ".",
    "a@b.c",
    "mailto:",
    "xmpp:",
    "x+",
    "http://",
    "smb://",
    "www.",
    "a",
    "b",
    "x",
    " ",
    "  ",
    "\t",
    "\n",
    "\r",
    "\r\n",
    "|",
    "!",
    "中",
    "é",
    "<details>",
    "</details>",
    "```\n",
    "~~~\n",
    "<A>",
    "</A>",
    "<A/>",
    "<B>",
    "</B>",
    "<A b=\"x\">",
    "<A b={1}>",
    "<>",
    "</>",
    "<A\n",
    "{'",
    "\\\n",
    "\n\n",
    "<a.b:c>",
    "</a.b:c>",
    " />",
];

pub(crate) fn generated_inputs(count: usize, max_pieces: usize, seed: u64) -> Vec<String> {
    let mut rng = Rng(seed);
    (0..count)
        .map(|_| {
            let pieces = rng.below(max_pieces + 1);
            (0..pieces)
                .map(|_| PIECES[rng.below(PIECES.len())])
                .collect()
        })
        .collect()
}

/// Every char boundary of `input`, including its end.
pub(crate) fn boundaries(input: &str) -> Vec<usize> {
    input
        .char_indices()
        .map(|(index, _)| index)
        .chain(core::iter::once(input.len()))
        .collect()
}

/// `positions` in a seeded shuffled order, then in ascending order.
pub(crate) fn query_orders(positions: &[usize], rng: &mut Rng) -> [Vec<usize>; 2] {
    let mut shuffled = positions.to_vec();
    for index in (1..shuffled.len()).rev() {
        shuffled.swap(index, rng.below(index + 1));
    }
    [shuffled, positions.to_vec()]
}
