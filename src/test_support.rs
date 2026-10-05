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

/// Generated MDX JSX: nested elements with quoted and expression attributes
/// spread over lines, self-closing tags, fragments, and stray or missing
/// closing tags.
pub(crate) fn generated_jsx(count: usize, seed: u64) -> Vec<String> {
    let mut rng = Rng(seed);
    (0..count)
        .map(|_| {
            let mut out = String::new();
            for _ in 0..1 + rng.below(3) {
                jsx_element(&mut rng, 0, &mut out);
                out.push_str(["", "\n", " ", "\n\n"][rng.below(4)]);
            }
            out
        })
        .collect()
}

fn jsx_element(rng: &mut Rng, depth: usize, out: &mut String) {
    const NAMES: [&str; 3] = ["A", "B", "a.b"];
    if rng.below(8) == 0 {
        out.push_str("<>");
        jsx_children(rng, depth, out);
        if rng.below(6) != 0 {
            out.push_str("</>");
        }
        return;
    }
    let name = NAMES[rng.below(NAMES.len())];
    out.push('<');
    out.push_str(name);
    for _ in 0..rng.below(3) {
        out.push_str([" ", "\n", " \n  "][rng.below(3)]);
        match rng.below(4) {
            0 => {
                out.push_str("x=\"");
                jsx_text(
                    rng,
                    out,
                    &["a", ">", "'", "{", "\\\"", "\\\n", "\\", "\n", "<A>"],
                );
                out.push('"');
            }
            1 => {
                out.push_str("y='");
                jsx_text(rng, out, &["a", ">", "\"", "}", "\\'", "\\\n", "\n"]);
                out.push('\'');
            }
            2 => {
                out.push_str("z={");
                jsx_text(
                    rng,
                    out,
                    &[
                        "1", "{2}", "'}'", "\"}\"", "`>`", "//}\n", "/*}*/", ">", "\n", "'\\\n'",
                    ],
                );
                out.push('}');
            }
            _ => out.push('w'),
        }
    }
    if rng.below(5) == 0 {
        out.push_str([" />", "/>", "\n/>", "/ >", "/\n>", " / \n >"][rng.below(6)]);
        return;
    }
    out.push('>');
    jsx_children(rng, depth, out);
    match rng.below(10) {
        0 => {}
        1 => out.push_str(&alloc::format!("</{}>", NAMES[rng.below(NAMES.len())])),
        _ => out.push_str(&alloc::format!("</{name}>")),
    }
}

fn jsx_children(rng: &mut Rng, depth: usize, out: &mut String) {
    for _ in 0..rng.below(4) {
        match rng.below(9) {
            0 | 1 if depth < 4 => jsx_element(rng, depth + 1, out),
            2 => out.push('\n'),
            3 => out.push_str(["<A>", "</A>", "<B/>", "</a.b>", "<>", "</>"][rng.below(6)]),
            4 => out.push_str("{1}"),
            5 => out.push_str(["<", "<!-- x -->", ">"][rng.below(3)]),
            _ => out.push('t'),
        }
    }
}

fn jsx_text(rng: &mut Rng, out: &mut String, pieces: &[&str]) {
    for _ in 0..rng.below(4) {
        out.push_str(pieces[rng.below(pieces.len())]);
    }
}

/// Generated MDX expression blocks: lines opening with `{`, holding strings,
/// comments, nested braces, and escapes at line ends.
pub(crate) fn generated_expression_blocks(count: usize, seed: u64) -> Vec<String> {
    const PIECES: [&str; 16] = [
        "'", "\"", "`", "\\", "\\\n", "\n", "\n\n", "{", "}", "a", " ", "//", "/*", "*/", "}\n",
        "\n{",
    ];
    let mut rng = Rng(seed);
    (0..count)
        .map(|_| {
            let mut out = String::from(["{", "  {", "{\n"][rng.below(3)]);
            for _ in 0..rng.below(24) {
                out.push_str(PIECES[rng.below(PIECES.len())]);
            }
            out
        })
        .collect()
}
