//! The tokens of `parse_inline`: one case per node kind and part, and the
//! invariants every token list keeps.

#[path = "support/tokens.rs"]
mod tokens;

use markdown_syntax::prelude::*;

/// The tokens of `text` as (bytes, node, role).
fn read(text: &str) -> Vec<(String, usize, TokenRole)> {
    let output = parse_inline(text, &Definitions::new(["r"]));
    output
        .tokens
        .iter()
        .map(|token| {
            (
                text[token.span.start..token.span.end].to_string(),
                token.node,
                token.role,
            )
        })
        .collect()
}

fn expect(text: &str, expected: &[(&str, usize, TokenRole)]) {
    let expected: Vec<_> = expected
        .iter()
        .map(|(bytes, node, role)| (bytes.to_string(), *node, *role))
        .collect();
    assert_eq!(read(text), expected, "{text:?}");
    let problems = tokens::token_problems(text, &Definitions::new(["r"]));
    assert!(problems.is_empty(), "{text:?}: {problems:#?}");
}

use TokenRole::{Attributes, Destination, Label, Name, Syntax, Target, Text, Title};

// public-api: Inline syntax tokens
#[test]
fn link_with_a_title() {
    expect(
        "[a](<u> \"t\")",
        &[
            ("[", 0, Syntax),
            ("a", 1, Text),
            ("](<", 0, Syntax),
            ("u", 0, Destination),
            ("> \"", 0, Syntax),
            ("t", 0, Title),
            ("\")", 0, Syntax),
        ],
    );
}

// public-api: Inline syntax tokens
#[test]
fn wiki_link_with_a_label() {
    expect(
        "[[T|L]]",
        &[
            ("[[", 0, Syntax),
            ("T", 0, Target),
            ("|", 0, Syntax),
            ("L", 0, Label),
            ("]]", 0, Syntax),
        ],
    );
    expect(
        "![[T]]",
        &[("![[", 0, Syntax), ("T", 0, Target), ("]]", 0, Syntax)],
    );
}

#[test]
fn text_breaks_escapes_and_references() {
    expect(
        "a  \nb\\\n\\*&amp; <b>\nc",
        &[
            ("a", 0, Text),
            ("  ", 1, Syntax),
            ("\n", 1, Text),
            ("b", 2, Text),
            ("\\", 3, Syntax),
            ("\n", 3, Text),
            ("\\", 4, Syntax),
            ("*", 4, Text),
            ("&", 5, Syntax),
            ("amp", 5, Text),
            (";", 5, Syntax),
            (" ", 6, Text),
            ("<b>", 7, Text),
            ("\n", 8, Text),
            ("c", 9, Text),
        ],
    );
}

#[test]
fn emphasis_like_marks_and_nesting() {
    expect(
        "**a *b*** ~~c~~ ==d==",
        &[
            ("**", 0, Syntax),
            ("a ", 1, Text),
            ("*", 2, Syntax),
            ("b", 3, Text),
            ("*", 2, Syntax),
            ("**", 0, Syntax),
            (" ", 4, Text),
            ("~~", 5, Syntax),
            ("c", 6, Text),
            ("~~", 5, Syntax),
            (" ", 7, Text),
            ("==", 8, Syntax),
            ("d", 9, Text),
            ("==", 8, Syntax),
        ],
    );
}

#[test]
fn code_and_math_with_padding() {
    expect(
        "`` a ``",
        &[("`` ", 0, Syntax), ("a", 0, Text), (" ``", 0, Syntax)],
    );
    expect(
        "`\na\n`",
        &[("`\n", 0, Syntax), ("a", 0, Text), ("\n`", 0, Syntax)],
    );
    expect("` `", &[("`", 0, Syntax), (" ", 0, Text), ("`", 0, Syntax)]);
    expect("$m$", &[("$", 0, Syntax), ("m", 0, Text), ("$", 0, Syntax)]);
    expect(
        "$$ n $$",
        &[("$$", 0, Syntax), (" n ", 0, Text), ("$$", 0, Syntax)],
    );
    expect(
        "$` k `$",
        &[("$`", 0, Syntax), (" k ", 0, Text), ("`$", 0, Syntax)],
    );
}

#[test]
fn links_images_and_references() {
    expect(
        "[a]()",
        &[("[", 0, Syntax), ("a", 1, Text), ("]()", 0, Syntax)],
    );
    expect(
        "![*a*](u (t))",
        &[
            ("![", 0, Syntax),
            ("*", 1, Syntax),
            ("a", 2, Text),
            ("*", 1, Syntax),
            ("](", 0, Syntax),
            ("u", 0, Destination),
            (" (", 0, Syntax),
            ("t", 0, Title),
            ("))", 0, Syntax),
        ],
    );
    expect(
        "[a][r] [r][] ![r]",
        &[
            ("[", 0, Syntax),
            ("a", 1, Text),
            ("][", 0, Syntax),
            ("r", 0, Label),
            ("]", 0, Syntax),
            (" ", 2, Text),
            ("[", 3, Syntax),
            ("r", 4, Text),
            ("][]", 3, Syntax),
            (" ", 5, Text),
            ("![", 6, Syntax),
            ("r", 7, Text),
            ("]", 6, Syntax),
        ],
    );
}

#[test]
fn autolinks_footnotes_and_names() {
    expect(
        "<http://a> www.b.com [^1] ^[f] :smile: #tag/x",
        &[
            ("<", 0, Syntax),
            ("http://a", 0, Destination),
            (">", 0, Syntax),
            (" ", 1, Text),
            ("www.b.com", 2, Destination),
            (" ", 3, Text),
            ("[^", 4, Syntax),
            ("1", 4, Label),
            ("]", 4, Syntax),
            (" ", 5, Text),
            ("^[", 6, Syntax),
            ("f", 7, Text),
            ("]", 6, Syntax),
            (" ", 8, Text),
            (":", 9, Syntax),
            ("smile", 9, Name),
            (":", 9, Syntax),
            (" ", 10, Text),
            ("#", 11, Syntax),
            ("tag/x", 11, Name),
        ],
    );
}

#[test]
fn text_directives() {
    expect(
        ":d[*l*]{#i .c k=v}",
        &[
            (":", 0, Syntax),
            ("d", 0, Name),
            ("[", 0, Syntax),
            ("*", 1, Syntax),
            ("l", 2, Text),
            ("*", 1, Syntax),
            ("]{", 0, Syntax),
            ("#i .c k=v", 0, Attributes),
            ("}", 0, Syntax),
        ],
    );
    expect(
        ":d{} x",
        &[
            (":", 0, Syntax),
            ("d", 0, Name),
            ("{}", 0, Syntax),
            (" x", 1, Text),
        ],
    );
}

#[test]
fn line_endings_and_nul_keep_the_text_tiled() {
    expect(
        "[a](\r\nu \"t\r\n\")",
        &[
            ("[", 0, Syntax),
            ("a", 1, Text),
            ("](\r\n", 0, Syntax),
            ("u", 0, Destination),
            (" \"", 0, Syntax),
            ("t\r\n", 0, Title),
            ("\")", 0, Syntax),
        ],
    );
    expect(
        "[a](u\0v)",
        &[
            ("[", 0, Syntax),
            ("a", 1, Text),
            ("](", 0, Syntax),
            ("u\0v", 0, Destination),
            (")", 0, Syntax),
        ],
    );
}

// public-api: Inline syntax tokens
// inline-syntax: Hard line breaks from spaces
#[test]
fn stripped_spaces_no_text_holds_are_the_soft_break() {
    for (input, span) in [("*a* \nb", Span::new(3, 5)), ("#t\t\nx", Span::new(2, 4))] {
        let document = parse(input).document;
        let Block::Paragraph(paragraph) = &document.children[0] else {
            panic!("{input:?} is a paragraph");
        };
        assert_eq!(paragraph.children[1].span(), Some(span), "{input:?}");
    }
    expect(
        "*a* \nb",
        &[
            ("*", 0, Syntax),
            ("a", 1, Text),
            ("*", 0, Syntax),
            (" \n", 2, Text),
            ("b", 3, Text),
        ],
    );
    expect(
        "#t \nx",
        &[
            ("#", 0, Syntax),
            ("t", 0, Name),
            (" \n", 1, Text),
            ("x", 2, Text),
        ],
    );
    expect(
        "a #t\t\r\nx",
        &[
            ("a ", 0, Text),
            ("#", 1, Syntax),
            ("t", 1, Name),
            ("\t\r\n", 2, Text),
            ("x", 3, Text),
        ],
    );
    expect(
        "*#t \nx*",
        &[
            ("*", 0, Syntax),
            ("#", 1, Syntax),
            ("t", 1, Name),
            (" \n", 2, Text),
            ("x", 3, Text),
            ("*", 0, Syntax),
        ],
    );
    // Text before them holds them.
    expect("a \nb", &[("a ", 0, Text), ("\n", 1, Text), ("b", 2, Text)]);
}

#[test]
fn encoded_parts_read_back_decoded() {
    expect(
        "[a](u\\(v '&amp;')",
        &[
            ("[", 0, Syntax),
            ("a", 1, Text),
            ("](", 0, Syntax),
            ("u\\(v", 0, Destination),
            (" '", 0, Syntax),
            ("&amp;", 0, Title),
            ("')", 0, Syntax),
        ],
    );
    expect(
        ":d{#i .c k=\"v\"}",
        &[
            (":", 0, Syntax),
            ("d", 0, Name),
            ("{", 0, Syntax),
            ("#i .c k=\"v\"", 0, Attributes),
            ("}", 0, Syntax),
        ],
    );
    expect(
        "[[a|]]",
        &[("[[", 0, Syntax), ("a", 0, Target), ("|]]", 0, Syntax)],
    );
}

#[test]
fn nodes_are_numbered_in_pre_order_through_alt_and_labels() {
    let output = parse_inline("x ![a *b*](u) :d[c]", &Definitions::new::<_, &str>([]));
    let kinds: Vec<_> = output
        .nodes()
        .iter()
        .map(|node| match node {
            Inline::Text(text) => text.value.clone(),
            Inline::Image(_) => "image".into(),
            Inline::Emphasis(_) => "emphasis".into(),
            Inline::TextDirective(_) => "directive".into(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        ["x ", "image", "a ", "emphasis", "b", " ", "directive", "c"]
    );
}

#[test]
fn empty_text_has_no_tokens() {
    assert!(read("").is_empty());
}
