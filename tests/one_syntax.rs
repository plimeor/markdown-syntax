//! Locks the one syntax `parse` reads (decisions/0007): the constructs it
//! recognizes, the ones it reads as text, and the spellings it records.

use markdown_syntax::prelude::*;

fn first_para(md: &str) -> Vec<Inline> {
    match parse(md).document.children.into_iter().next() {
        Some(Block::Paragraph(p)) => p.children,
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

fn texts_only(inlines: &[Inline]) -> bool {
    inlines
        .iter()
        .all(|inline| matches!(inline, Inline::Text(_)))
}

#[test]
fn underscore_strong_stays_strong() {
    assert!(matches!(
        first_para("a __b__ c").get(1),
        Some(Inline::Strong(_))
    ));
    assert!(matches!(
        first_para("a **b** c").get(1),
        Some(Inline::Strong(_))
    ));
}

#[test]
fn removed_marks_stay_text() {
    let inlines = first_para("H~2~O and x^2^ ++a++ ||b||");
    assert!(texts_only(&inlines), "{inlines:?}");
}

#[test]
fn delimiter_collisions_resolve_to_the_one_syntax() {
    assert!(matches!(
        first_para("~~s~~").first(),
        Some(Inline::Delete(_))
    ));
    assert!(texts_only(&first_para("~s~")));
    assert!(texts_only(&first_para("H~2~O")));
    assert!(texts_only(&first_para("x^2^")));
    assert!(matches!(
        first_para("note^[x] tail").get(1),
        Some(Inline::InlineFootnote(_))
    ));
    assert!(matches!(
        first_para("a :tada: b").get(1),
        Some(Inline::Shortcode(_))
    ));
}

#[test]
fn prose_from_the_issue_stays_text() {
    // plimeor/markdown-syntax#14.
    assert!(texts_only(&first_para("~/.bashrc and ~/.zshrc")));
    assert!(texts_only(&first_para("^12.0.0 or ^13.0.0")));
}

#[test]
fn dollar_amounts_stay_text() {
    // The math parser needs tight delimiters, so `$5 to $10` is not inline math.
    let inlines = first_para("price $5 to $10 today");
    assert!(
        inlines.iter().all(|i| !matches!(i, Inline::Math(_))),
        "unexpected math node: {inlines:?}"
    );
}

#[test]
fn wikilink_title_is_after_the_pipe() {
    let inlines = first_para("see [[target|label]] here");
    let Some(Inline::WikiLink(link)) = inlines.get(1) else {
        panic!("expected a wikilink: {inlines:?}");
    };
    assert_eq!(link.target, "target");
    assert_eq!(link.label, "label");
}

#[test]
fn html_containers_are_recognized() {
    let out = parse("<details>\n<summary>Open</summary>\n\nbody\n\n</details>\n");

    assert!(matches!(
        out.document.children.as_slice(),
        [Block::HtmlContainer(_)]
    ));
}

#[test]
fn mdx_syntax_is_raw_html_or_text() {
    let out = parse("import a from 'b'\n\n{x}\n\n<Foo bar={1} />");
    let [Block::Paragraph(import), Block::Paragraph(expression), Block::HtmlBlock(_)] =
        out.document.children.as_slice()
    else {
        panic!("unexpected blocks: {:?}", out.document.children);
    };
    assert!(texts_only(&import.children));
    assert!(texts_only(&expression.children));
}

#[test]
fn underscore_delimiters_are_recorded() {
    let inlines = first_para("_a_ __b__");
    let [Inline::Emphasis(emphasis), _, Inline::Strong(strong)] = inlines.as_slice() else {
        panic!("unexpected inlines: {inlines:?}");
    };
    assert_eq!(emphasis.delimiter, EmphasisDelimiter::Underscore);
    assert_eq!(strong.delimiter, EmphasisDelimiter::Underscore);
}

#[test]
fn link_forms_are_recorded() {
    let forms: Vec<LinkForm> = first_para("www.a.b <http://c.d> [e](f)")
        .iter()
        .filter_map(|inline| match inline {
            Inline::Link(link) => Some(link.form),
            _ => None,
        })
        .collect();
    assert_eq!(
        forms,
        [
            LinkForm::LiteralAutolink,
            LinkForm::AngleAutolink,
            LinkForm::Inline
        ]
    );
}

#[test]
fn constructed_nodes_take_the_default_spelling() {
    assert_eq!(Link::new("u", [Text::from("a")]).form, LinkForm::Inline);
    assert_eq!(LinkForm::default(), LinkForm::Inline);
    assert_eq!(EmphasisDelimiter::default(), EmphasisDelimiter::Asterisk);
}

#[test]
fn build_layer_round_trips() {
    let document = Document {
        meta: NodeMeta::default(),
        children: vec![
            Heading::new(1, [Text::from("Title")]).into(),
            Paragraph::new([Text::from("hello")]).into(),
        ],
    };
    // Hand-built nodes carry no span.
    assert_eq!(document.children[0].span(), None);
    assert_eq!(document.to_markdown().unwrap(), "# Title\n\nhello\n");
}

#[test]
fn a_text_directive_ends_at_whitespace_or_written_punctuation() {
    let directives = |md: &str| {
        first_para(md)
            .iter()
            .filter(|inline| matches!(inline, Inline::TextDirective(_)))
            .count()
    };
    // Whitespace or the end of the content follows the whole directive, or
    // ASCII punctuation follows a label or attributes.
    for md in [
        ":e",
        "see :call-out here",
        ":e[] text",
        ":e{}",
        "Inside :badge[ok]{flag}.",
        "(:e[x])",
        ":e{a=b},",
        // Braces holding only an attribute without a valid name still count.
        ":e{!}.",
    ] {
        assert_eq!(directives(md), 1, "{md:?}: {:?}", first_para(md));
    }
    // Anything else after it, or punctuation after a directive with nothing
    // written after its name, keeps the whole run text.
    for md in [
        ":e{}x",
        ":e{}1",
        ":e[]www.a.b",
        ":e{}a@b.c",
        ":e{}[^1]",
        ":e[a]b",
        ":e{}.",
        "(:note)",
        "see :note.",
        ":h1[x]",
        ":my_note",
        ":e[a]。",
    ] {
        assert_eq!(directives(md), 0, "{md:?}: {:?}", first_para(md));
        assert_eq!(parse(md).diagnostics, Vec::new(), "{md:?}");
    }
}
