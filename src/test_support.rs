//! Deterministic generated inputs shared by the scan equivalence tests, and
//! the fixture corpus the crate's integration tests read.

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

/// Reference labels: every ASCII string of up to two chars, every string of
/// up to four chars over a few that label normalization treats specially,
/// and seeded random strings over both.
pub(crate) fn labels() -> Vec<String> {
    let ascii: Vec<char> = (0..=127u8).map(char::from).collect();
    let special = [
        ' ', '\t', '\n', '\x0b', '\x0c', '\r', '\0', 'a', 'Z', '!', 'é', 'ẞ', '\u{a0}',
    ];
    let mut labels = Vec::from([String::new()]);
    for &one in &ascii {
        labels.push(String::from(one));
        for &two in &ascii {
            labels.push(String::from_iter([one, two]));
        }
    }
    let mut grown = Vec::from([String::new()]);
    for _ in 0..4 {
        grown = grown
            .iter()
            .flat_map(|label| {
                special
                    .iter()
                    .map(move |char| alloc::format!("{label}{char}"))
            })
            .collect();
        labels.extend(grown.iter().cloned());
    }
    let mut rng = Rng(0x1abe1);
    for _ in 0..20_000 {
        let pool: &[char] = if rng.below(2) == 0 { &ascii } else { &special };
        labels.push(
            (0..rng.below(12))
                .map(|_| pool[rng.below(pool.len())])
                .collect(),
        );
    }
    labels
}

/// Every input of the fixture corpus under `tests/fixtures`: each `.md`
/// file, and the input of each case of each `.cases` file, both those that
/// hold only inputs (`--- case <i> bytes <n>`) and those that hold expected
/// HTML too (`--- case <i> options <o> label-bytes <l> input-bytes <n>
/// expected-bytes <e>`), in path order.
pub(crate) fn fixture_inputs() -> Vec<String> {
    fn collect(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("the fixture directory reads") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                collect(&path, files);
            } else {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    collect(std::path::Path::new("tests/fixtures"), &mut files);
    files.sort();
    let mut inputs = Vec::new();
    for path in files {
        let read = || std::fs::read_to_string(&path).expect("a fixture reads as UTF-8");
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("md") => inputs.push(read()),
            Some("cases") => inputs.extend(case_inputs(&read())),
            _ => {}
        }
    }
    assert!(inputs.len() > 3_000, "only {} fixture inputs", inputs.len());
    inputs
}

/// The input of each case of a `.cases` file, each read by its byte count.
fn case_inputs(file: &str) -> Vec<String> {
    let mut inputs = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = file[cursor..].find("--- case ") {
        let header_start = cursor + offset;
        let header_end = header_start + file[header_start..].find('\n').expect("a header line");
        let parts: Vec<&str> = file[header_start..header_end].split_whitespace().collect();
        let count = |at: usize| parts[at].parse::<usize>().expect("a byte count");
        let (start, len, rest) = match parts.len() {
            5 => (header_end + 1, count(4), 0),
            11 => (
                header_end + 1 + count(6) + "\n--- input\n".len(),
                count(8),
                "\n--- expected\n".len() + count(10),
            ),
            _ => panic!("a case header: {parts:?}"),
        };
        inputs.push(String::from(&file[start..start + len]));
        cursor = start + len + rest;
    }
    inputs
}
