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

/// Cases that differ by design: the oracle reads them under another dialect.
pub const DEVIATIONS: &[Listed] = &[
    case("commonmark/autolink.cases", "-", 0x8d336d2ce0091b58, "<http://foo.bar/baz bim>", "a literal autolink forms inside the `<…>` the oracle reads as text"),
    case("commonmark/autolink.cases", "-", 0x47c41072fc131820, "< http://foo.bar >", "a literal autolink forms inside the `<…>` the oracle reads as text"),
    case("commonmark/autolink.cases", "-", 0xf6e395b64dad9ae4, "http://example.com", "literal autolinks are part of the syntax; the oracle reads this without them"),
    case("commonmark/autolink.cases", "-", 0x503d30b802c63bc9, "foo@bar.example.com", "literal autolinks are part of the syntax; the oracle reads this without them"),
    case("commonmark/autolink.cases", "-", 0xd76ab36872096640, "<asd@01234567890123456789012345678901234", "a literal autolink forms inside the `<…>` the oracle reads as text"),
    case("commonmark/autolink.cases", "-", 0x7b67e1e142d5bbcb, "<asd@-example.com>", "a literal autolink forms inside the `<…>` the oracle reads as text"),
    case("commonmark/autolink.cases", "-", 0xb708fdc951b8af95, "<asd@example-.com>", "a literal autolink forms inside the `<…>` the oracle reads as text"),
    case("commonmark/code_indented.cases", "code_indented_off", 0x3ac7e87e17e3cf3c, "    a", "indented code is part of the syntax; the oracle turns it off"),
    case("commonmark/code_indented.cases", "code_indented_off", 0xc24b9c53c8f8b8f8, "```\na\n    ```", "indented code is part of the syntax; the oracle turns it off"),
    case("commonmark/commonmark.cases", "allow_dangerous_html,allow_dangerous_protocol", 0x85a2b91000f9635a, "---\nFoo\n---\nBar\n---\nBaz\n", "frontmatter is part of the syntax; the oracle reads the leading `---` lines as breaks and a heading"),
    case("commonmark/commonmark.cases", "allow_dangerous_html,allow_dangerous_protocol", 0x3702e9bd361ac825, "---\n---\n", "frontmatter is part of the syntax; the oracle reads the leading `---` lines as breaks and a heading"),
    case("commonmark/commonmark.cases", "allow_dangerous_html,allow_dangerous_protocol", 0x3a87d9bc9cc4d729, "[[[foo]]]\n\n[[[foo]]]: /url\n", "wiki links are part of the syntax and win over link and reference syntax"),
    case("commonmark/commonmark.cases", "allow_dangerous_html,allow_dangerous_protocol", 0x4a83e42041208161, "[[*foo* bar]]\n\n[*foo* bar]: /url \"title\"", "wiki links are part of the syntax and win over link and reference syntax"),
    case("commonmark/commonmark.cases", "allow_dangerous_html,allow_dangerous_protocol", 0xce9484f01a55309c, "![[foo]]\n\n[[foo]]: /url \"title\"\n", "wiki links are part of the syntax and win over link and reference syntax"),
    case("commonmark/commonmark.cases", "allow_dangerous_html,allow_dangerous_protocol", 0xe395b72ebf329d31, "<https://foo.bar/baz bim>\n", "a literal autolink forms inside the `<…>` the oracle reads as text"),
    case("commonmark/commonmark.cases", "allow_dangerous_html,allow_dangerous_protocol", 0xab5ec990ca592a3f, "< https://foo.bar >\n", "a literal autolink forms inside the `<…>` the oracle reads as text"),
    case("commonmark/commonmark.cases", "allow_dangerous_html,allow_dangerous_protocol", 0x0c8b5ecfdcb3fa5b, "https://example.com\n", "literal autolinks are part of the syntax; the oracle reads this without them"),
    case("commonmark/commonmark.cases", "allow_dangerous_html,allow_dangerous_protocol", 0x1e358bacb6d78c59, "foo@bar.example.com\n", "literal autolinks are part of the syntax; the oracle reads this without them"),
    case("commonmark/frontmatter.cases", "-", 0xc7b84559ee61d622, "---\ntitle: Jupyter\n---", "frontmatter is part of the syntax; the oracle reads the leading `---` lines as breaks and a heading"),
    case("commonmark/gfm_autolink_literal.cases", "-", 0x837b2b5793a240b3, "https://example.com", "literal autolinks are part of the syntax; the oracle reads this without them"),
    case("commonmark/gfm_autolink_literal.cases", "-", 0xacc7e7b8b7a0236b, "www.example.com", "literal autolinks are part of the syntax; the oracle reads this without them"),
    case("commonmark/gfm_autolink_literal.cases", "-", 0xb8169be981f3cadb, "user@example.com", "literal autolinks are part of the syntax; the oracle reads this without them"),
    case("commonmark/gfm_strikethrough.cases", "gfm", 0x17e06f08e8819c4e, "a ~b~", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("commonmark/gfm_strikethrough.cases", "gfm", 0x3045a67a3c63747c, "a ~-1~ b", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("commonmark/gfm_strikethrough.cases", "gfm", 0xc389e56d08b39af9, "a ~b.~ c", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("commonmark/gfm_strikethrough.cases", "gfm", 0xd5122feb9a7ea46d, "~b.~.", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("commonmark/gfm_strikethrough.cases", "gfm", 0xf025d0d09b470054, "\n# Balanced\n\na ~one~ b\n\na ~~two~~ b\n\na ~", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("commonmark/gfm_strikethrough.cases", "gfm", 0xf6ceb2044ed1d70a, "\n# Flank\n\na oneRight~ b oneRight~ c oneR", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("commonmark/gfm_strikethrough.cases", "gfm", 0xcd946eb22a63b608, "\n# Interlpay\n\n## Interleave with attenti", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("commonmark/gfm_strikethrough.cases", "gfm", 0xf64e64466781b805, "a*~b~*c\n\na*.b.*c", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("commonmark/gfm_strikethrough.cases", "-", 0xf64e64466781b805, "a*~b~*c\n\na*.b.*c", "`~~` strikethrough is part of the syntax; the CommonMark oracle reads it as text"),
    case("commonmark/gfm_strikethrough.cases", "gfm", 0x779e60667a9ffca3, "a ~b~ ~~c~~ d", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("commonmark/gfm_table.cases", "-", 0x344b3ce2752a24d5, "| a |\n| - |\n| b |", "tables are part of the syntax; the oracle reads the rows as text"),
    case("commonmark/gfm_table.cases", "gfm,code_indented_off", 0xcfaf038193c872fd, "| a |\n    | - |", "indented code is part of the syntax; the oracle turns it off"),
    case("commonmark/gfm_table.cases", "gfm,code_indented_off", 0xcad0aaca934cedd8, "    | a |\n\t| - |\n    | b |", "indented code is part of the syntax; the oracle turns it off"),
    case("commonmark/gfm_tagfilter.cases", "allow_dangerous_html,gfm_tagfilter", 0xf96faea142502e88, "\n<title>\n\n<div title=\"<title>\"></div>\n\n<", "literal autolinks are part of the syntax; the oracle reads this without them"),
    case("commonmark/gfm_task_list_item.cases", "-", 0xa3f7f4ff52863012, "* [x] y.", "task list items are part of the syntax; the oracle reads `[x]` as text"),
    case("commonmark/heading_setext.cases", "-", 0x977989679d221e34, "---\nFoo\n---\nBar\n---\nBaz", "frontmatter is part of the syntax; the oracle reads the leading `---` lines as breaks and a heading"),
    case("commonmark/heading_setext.cases", "-", 0xe1934f8733e9b2cd, "---\n---", "frontmatter is part of the syntax; the oracle reads the leading `---` lines as breaks and a heading"),
    case("commonmark/html_flow.cases", "html_flow_off", 0x6fc38c182fd4dc45, "<x>", "raw HTML blocks are part of the syntax; the oracle turns them off"),
    case("commonmark/image.cases", "-", 0x0ae6d075a41b2254, "[[foo]]: /url \"title\"\n\n![[foo]]", "wiki links are part of the syntax and win over link and reference syntax"),
    case("commonmark/link_reference.cases", "-", 0xcabd7820a75b4f39, "[[[foo]]]: /url\n\n[[[foo]]]", "wiki links are part of the syntax and win over link and reference syntax"),
    case("commonmark/link_reference.cases", "-", 0x7136c3f6ddadc171, "[*foo* bar]: /url \"title\"\n\n[[*foo* bar]]", "wiki links are part of the syntax and win over link and reference syntax"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x57b1699985b000e9, "[[https://foo.com]]", "wiki links are part of the syntax and win over link and reference syntax"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0xd50e442a846c8be9, "[[Foo|https://foo.com]]", "wiki links are part of the syntax and win over link and reference syntax"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x6b15bc44646aa89b, "{https://foo.com}", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x2331e3230526030c, "http://example.com/[abc]]...\n", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x202f1cdfa22ce76c, "http://example.com/{abc}}...\n", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x43fa842e6f0c1f56, "smb:///Volumes/shared/foo.pdf", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x1f19b402fc427483, "irc://irc.freenode.net/git", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x450d69e6e6fc6e08, "rdar://localhost.com/blah", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x186b524cc5405412, "://-", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x1ea57c680166f6b0, "we://w", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,extension.wikilinks_title_before_pipe", 0x1e74335cd4075d28, "[[Check www.example.com|http://example.c", "wiki links take the title after the pipe; the oracle takes it before"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x1bf3834602afe524, "hi: nex://[fe80::1ff:fe23:4567:890a%25et", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x9683d1343ac0c762, "foo www. foo", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/autolink.cases", "extension.autolink,parse.relaxed_autolinks", 0x43d484fd15066323, "foo http:// foo", "literal autolinks take the strict GFM extents; the oracle uses comrak's relaxed autolinks"),
    case("gfm/footnotes.cases", "extension.superscript,extension.footnotes", 0x3e304646c5d3f314, "Here is a footnote reference.[^1]\n\nHere ", "superscript is not part of the syntax; the oracle reads `^2^` as superscript"),
    case("gfm/inline_footnotes.cases", "extension.inline_footnotes", 0x23538404ce8325cd, "Text^[note] should not parse.\n", "inline footnotes are part of the syntax; the oracle runs without footnotes"),
    case("gfm/math.cases", "extension.math_dollars", 0xdf5d221a68b26090, "test $$\n2+2\n$$", "`$$` opens a math block; the GFM oracle reads it as inline display math"),
    case("gfm/math.cases", "extension.math_dollars", 0x2d7905f74cfccf51, "$$\n2+2\n4+4\n$$", "`$$` opens a math block; the GFM oracle reads it as inline display math"),
    case("gfm/math.cases", "extension.math_dollars,extension.math_code", 0xa118ba17bb1eb6c3, "$$$", "`$$` opens a math block; the GFM oracle reads it as inline display math"),
    case("gfm/strikethrough.cases", "extension.strikethrough", 0xd8dadd7319068944, "This is ~strikethrough~.\n\nAs is ~~this, ", "strikethrough takes two tildes; the oracle reads one as strikethrough"),
    case("gfm/wikilinks.cases", "extension.wikilinks_title_before_pipe", 0x2a0637484a418a0b, "This is [[a &lt;link|&lt;script&gt;alert", "wiki links take the title after the pipe; the oracle takes it before"),
    case("gfm/wikilinks.cases", "extension.wikilinks_title_before_pipe", 0xf05bd3ae39a75119, "[[a|http:'\"injected=attribute&gt;&lt;img", "wiki links take the title after the pipe; the oracle takes it before"),
    case("gfm/wikilinks.cases", "extension.wikilinks_title_before_pipe", 0x0f63f2981df76fe1, "<i>[[a|'\"&gt;&lt;svg&gt;&lt;i/class=gl-s", "wiki links take the title after the pipe; the oracle takes it before"),
];

/// Cases that fail as parser defects, not by design.
pub const KNOWN_DEFECTS: &[Listed] = &[
    case("commonmark/code_fenced.cases", "-", 0x8be7f99eb116458d, "  ```\n ", "an unclosed fence whose last line holds only whitespace and no line ending renders that line as an empty content line; the oracle renders the fence empty (commonmark.js renders it as this crate does)"),
    case("commonmark/fuzz.cases", "-", 0x9165948c43745101, "> ```\n", "a fence in a block quote that the end of input closes right after its opening line renders empty; the oracle renders one empty content line (commonmark.js renders it as this crate does)"),
    case("commonmark/fuzz.cases", "-", 0x5b4ae38f093b60d6, "- ```\n", "a fence in a list item that the end of input closes right after its opening line renders empty; the oracle renders one empty content line (commonmark.js renders it as this crate does)"),
    case("commonmark/list.cases", "-", 0x57ecef8531311db8, "- ```\n   \n  ```", "a whitespace-only line in a fence in a list item keeps no whitespace past the item's indentation; the oracle keeps it (commonmark.js renders it as this crate does)"),
    case("commonmark/list.cases", "-", 0x68dbf208da8c60bc, "- ```\n    \n  ```", "a whitespace-only line in a fence in a list item keeps no whitespace past the item's indentation; the oracle keeps it (commonmark.js renders it as this crate does)"),
    case("commonmark/list.cases", "-", 0xcfbe10e540f5a44f, "- ```\n\t\n  ```", "a whitespace-only line in a fence in a list item keeps no whitespace past the item's indentation; the oracle keeps it (commonmark.js renders it as this crate does)"),
    case("gfm/autolink.cases", "extension.autolink", 0xeeec9d08384f38db, "[https://foo.com]", "a literal autolink forms inside `[…]`; the oracle leaves a URL in brackets unlinked unless relaxed autolinks are on"),
    case("gfm/autolink.cases", "extension.autolink", 0x57b1699985b000e9, "[[https://foo.com]]", "`[[…]]` around a URL reads as a wiki link; the oracle, without wiki links, reads it as text"),
    case("gfm/autolink.cases", "extension.autolink", 0xd50e442a846c8be9, "[[Foo|https://foo.com]]", "`[[…]]` around a URL reads as a wiki link; the oracle, without wiki links, reads it as text"),
];
