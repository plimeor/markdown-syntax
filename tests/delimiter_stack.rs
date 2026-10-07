//! Emphasis-like marks (`*`, `_`, `~~`, `==`) pair on one delimiter stack:
//! closers are taken in source order, and marks that do not cross nest as
//! written. Inputs written for the removed marks (`~`, `^`, `++`, `||`,
//! underline `__`) pin that those runs stay text.

use markdown_syntax::prelude::*;

/// A compact rendering of inline structure: text in quotes, a soft break as
/// `/`, an escape as `\c`, code spans as `Code(...)`, containers as
/// `Kind[...]`.
fn shape(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        let (kind, children): (&str, &[Inline]) = match inline {
            Inline::Text(text) => {
                out.push_str(&format!("{:?}", text.value));
                continue;
            }
            Inline::SoftBreak(_) => {
                out.push('/');
                continue;
            }
            Inline::Escape(escape) => {
                out.push('\\');
                out.push(escape.value);
                continue;
            }
            Inline::Code(code) => {
                out.push_str(&format!("Code({:?})", code.value));
                continue;
            }
            Inline::Emphasis(node) => ("Emphasis", &node.children),
            Inline::Strong(node) => ("Strong", &node.children),
            Inline::Delete(node) => ("Delete", &node.children),
            Inline::Mark(node) => ("Mark", &node.children),
            Inline::Link(node) => ("Link", &node.children),
            Inline::Image(node) => ("Image", &node.alt),
            Inline::InlineFootnote(node) => ("InlineFootnote", &node.children),
            other => panic!("unexpected inline {other:?}"),
        };
        out.push_str(kind);
        out.push('[');
        out.push_str(&shape(children));
        out.push(']');
    }
    out
}

fn parsed(markdown: &str) -> String {
    match parse(markdown).document.children.as_slice() {
        [Block::Paragraph(paragraph)] => shape(&paragraph.children),
        other => panic!("expected one paragraph, got {other:?}"),
    }
}

#[test]
fn strong_closes_before_a_highlight() {
    assert_eq!(parsed("**a ==b** c=="), r#"Strong["a ==b"]" c==""#);
}

#[test]
fn emphasis_closes_before_an_insert() {
    assert_eq!(parsed("*a ++b* c++"), r#"Emphasis["a ++b"]" c++""#);
}

#[test]
fn highlight_closes_before_strong() {
    assert_eq!(parsed("==a **b== c**"), r#"Mark["a **b"]" c**""#);
}

#[test]
fn marks_that_do_not_cross_nest_as_written() {
    assert_eq!(parsed("==a *b* c=="), r#"Mark["a "Emphasis["b"]" c"]"#);
    assert_eq!(
        parsed("*a ||b ~~c ^d^ e~~ f|| ++g ~h~ i++ j*"),
        r#"Emphasis["a ||b "Delete["c ^d^ e"]" f|| ++g ~h~ i++ j"]"#
    );
}

#[test]
fn emphasis_right_inside_a_mark_stays_inside_it() {
    // The `*` touching `~~` could also close the outer `*`; inside the mark
    // it opens, as in the mark's content read on its own.
    assert_eq!(
        parsed("*a ==~~*b*~~== c*"),
        r#"Emphasis["a "Mark[Delete[Emphasis["b"]]]" c"]"#
    );
    // Without a mark around it, the `*` after `~~` closes the outer `*`.
    assert_eq!(
        parsed("*a ||~~*b*~~|| c*"),
        r#"Emphasis["a ||~~"]"b"Emphasis["~~|| c"]"#
    );
    assert_eq!(parsed("~ **a*~"), r#""~ **a*~""#);
}

#[test]
fn double_underscore_is_strong() {
    assert_eq!(parsed("a __b__ c"), r#""a "Strong["b"]" c""#);
    assert_eq!(parsed("___a___"), r#"Emphasis[Strong["a"]]"#);
    assert_eq!(
        parsed("__a ~~__b__~~ c__"),
        r#"Strong["a "Delete[Strong["b"]]" c"]"#
    );
}

#[test]
fn single_tildes_and_carets_stay_text() {
    assert_eq!(parsed("~a ~b~"), r#""~a ~b~""#);
    assert_eq!(parsed("^^a^"), r#""^^a^""#);
    assert_eq!(parsed("^a\nb^"), r#""^a"/"b^""#);
}

#[test]
fn deeply_nested_highlights_stop_at_32_levels() {
    let depth = 40;
    let markdown = format!("{}x{}", "==a ".repeat(depth), " b==".repeat(depth));
    let shape = parsed(&markdown);
    assert_eq!(shape.matches("Mark[").count(), 32, "{shape}");
}

#[test]
fn deep_emphasis_stops_at_16_levels() {
    let depth = 20;
    let markdown = format!("{}x{}", "*a ".repeat(depth), " b*".repeat(depth));
    let shape = parsed(&markdown);
    assert_eq!(shape.matches("Emphasis[").count(), 16, "{shape}");
}

#[test]
fn code_spans_bind_tighter_than_marks() {
    assert_eq!(parsed("^a `^` b^"), r#""^a "Code("^")" b^""#);
    assert_eq!(parsed("==a `b== c` d=="), r#"Mark["a "Code("b== c")" d"]"#);
}

#[test]
fn a_mark_opened_inside_a_link_stays_inside_it() {
    assert_eq!(parsed("[a ==b](u) c=="), r#"Link["a ==b"]" c==""#);
    assert_eq!(parsed("*[foo*](/u)"), r#""*"Link["foo*"]"#);
}

#[test]
fn deeply_nested_images_stop_at_32_levels() {
    let depth = 40;
    let markdown = format!("{}x{}", "![".repeat(depth), "](u)".repeat(depth));
    let document = parse(&markdown).document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    let mut levels = 0;
    let mut current = paragraph.children.as_slice();
    while let Some(Inline::Image(image)) = current.first() {
        levels += 1;
        current = &image.alt;
    }
    assert_eq!(levels, 32);
}

#[test]
fn links_resolve_at_the_innermost_bracket() {
    assert_eq!(parsed("[a [b](u) c](v)"), r#""[a "Link["b"]" c](v)""#);
    assert_eq!(parsed("![a [b](u) c](v)"), r#"Image["a "Link["b"]" c"]"#);
}

#[test]
fn an_unclosed_inline_footnote_stays_text() {
    assert_eq!(parsed("^a ^[b"), r#""^a ^[b""#);
}

#[test]
fn a_caret_before_a_bracket_opens_an_inline_footnote() {
    assert_eq!(
        parsed("a^b^[link](u)"),
        r#""a^b"InlineFootnote["link"]"(u)""#
    );
}

#[test]
fn a_caret_inside_emphasis_stays_text() {
    assert_eq!(
        parsed("*a ^b* ^[x]"),
        r#"Emphasis["a ^b"]" "InlineFootnote["x"]"#
    );
}

#[test]
fn a_caret_right_after_a_caret_opens_an_inline_footnote() {
    let document = parse("a ^^[x] b").document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    assert!(
        paragraph
            .children
            .iter()
            .any(|inline| matches!(inline, Inline::InlineFootnote(_))),
        "{:?}",
        paragraph.children
    );
}

#[test]
fn an_unclosed_reference_label_leaves_a_shortcut_reference() {
    let document = parse("[foo][bar\n\n[foo]: /u").document;
    let Some(Block::Paragraph(paragraph)) = document.children.first() else {
        panic!("expected a paragraph");
    };
    assert!(
        matches!(paragraph.children.as_slice(), [Inline::LinkReference(link), Inline::Text(rest)]
            if link.kind == ReferenceKind::Shortcut && link.identifier == "foo" && rest.value == "[bar"),
        "{:?}",
        paragraph.children
    );
}

#[test]
fn a_long_closing_run_reopens_with_what_it_has_left() {
    assert_eq!(parsed("*x ==a====* b=="), r#""*x "Mark["a"]Mark["* b"]"#);
    assert_eq!(parsed("*x ++a++++* b++"), r#"Emphasis["x ++a++++"]" b++""#);
    assert_eq!(parsed("*x ||a||||* b||"), r#"Emphasis["x ||a||||"]" b||""#);
}

#[test]
fn underscore_spans_cover_their_own_delimiters() {
    let document = parse("___a_ b__").document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    let [Inline::Strong(node)] = paragraph.children.as_slice() else {
        panic!("expected one strong, got {:?}", paragraph.children);
    };
    assert_eq!(node.meta.span, Some(Span::new(0, 9)));
    assert_eq!(node.children[0].span(), Some(Span::new(2, 5)));
}

#[test]
fn brackets_past_the_limit_close_as_text() {
    let depth = 40;
    let markdown = format!("{}x{}", "![".repeat(depth), "](u)".repeat(depth));
    let document = parse(&markdown).document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    let [Inline::Image(outer)] = paragraph.children.as_slice() else {
        panic!("expected one outermost image, got {:?}", paragraph.children);
    };
    assert_eq!(outer.meta.span, Some(Span::new(0, markdown.len())));
}

#[test]
fn a_wikilink_after_a_bang_is_an_embed_and_wins_over_the_image() {
    let document = parse("![[a\\]b]]").document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    assert!(
        matches!(paragraph.children.as_slice(), [Inline::WikiLink(link)]
            if link.embed && link.target == "a\\]b" && link.meta.span == Some(Span::new(0, 9))),
        "{:?}",
        paragraph.children
    );
    // An unescaped bracket in the content leaves no wikilink.
    assert_eq!(parsed("![[a]b]]"), r#""![[a]b]]""#);
}

#[test]
fn a_link_inside_a_directive_label_inside_a_mark_keeps_links_from_nesting() {
    let document = parse("[x :d[==[b](u)==] y](v)").document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    assert!(
        !paragraph
            .children
            .iter()
            .any(|inline| matches!(inline, Inline::Link(_))),
        "{:?}",
        paragraph.children
    );
}

#[test]
fn an_escaped_dot_after_a_bracket_is_an_escape() {
    assert_eq!(parsed("[www. \\. x"), r#""[www. "\." x""#);
}

#[test]
fn delimiters_past_the_limit_stay_in_the_output() {
    let input = String::from("++") + &"==a ".repeat(32) + "x" + &" b==".repeat(32) + "++++c++";
    let markdown = parse(&input).document.to_markdown().unwrap();
    assert_eq!(markdown.matches('+').count(), input.matches('+').count());
    let input = String::from("___") + &"*".repeat(32) + "x" + &"*".repeat(32) + "_ b__";
    let markdown = parse(&input).document.to_markdown().unwrap();
    assert_eq!(markdown.matches('_').count(), input.matches('_').count());
}

#[test]
fn a_bracket_inside_an_embed_leaves_text() {
    assert_eq!(parsed("![[a[b]]"), r#""![[a[b]]""#);
}

#[test]
fn the_rule_of_three_counts_whole_delimiter_runs() {
    assert_eq!(parsed("*a***b*"), r#"Emphasis["a"]"*"Emphasis["b"]"#);
    assert_eq!(parsed("***a*a*a"), r#""*"Emphasis[Emphasis["a"]"a"]"a""#);
}

#[test]
fn an_image_whose_resource_is_invalid_falls_back_to_a_shortcut_reference() {
    let document = parse("![foo](a b)\n\n[foo]: /u").document;
    let Some(Block::Paragraph(paragraph)) = document.children.first() else {
        panic!("expected a paragraph");
    };
    assert!(
        matches!(paragraph.children.as_slice(), [Inline::ImageReference(image), Inline::Text(rest)]
            if image.kind == ReferenceKind::Shortcut && image.identifier == "foo" && rest.value == "(a b)"),
        "{:?}",
        paragraph.children
    );
}

#[test]
fn an_underscore_after_unicode_punctuation_opens_as_after_ascii_punctuation() {
    for source in ["\u{ab}_**]**_", "\u{20ac}_**]**_", "\0_**]**_"] {
        let shape = parsed(source);
        assert!(
            shape.ends_with(r#"Emphasis[Strong["]"]]"#),
            "{source:?}: {shape}"
        );
    }
}

#[test]
fn an_escaped_backslash_before_a_line_ending_is_no_hard_break() {
    assert_eq!(parsed("a\\\\\nb"), r#""a"\\/"b""#);
}

#[test]
fn a_bare_destination_ends_at_a_space_inside_parentheses() {
    assert_eq!(parsed("[a](( ))"), r#""[a](( ))""#);
    let blocks = parse("[o]:(a b)\n\n[o]").document.children;
    assert!(
        !blocks
            .iter()
            .any(|block| matches!(block, Block::Definition(_))),
        "{blocks:?}"
    );
}

#[test]
fn a_footnote_label_holds_no_unescaped_bracket() {
    for source in ["^*[^[^]]", "[^a[b]", "[^a[b]: x\n\n[^a[b]"] {
        let debug = format!("{:?}", parse(source).document.children);
        assert!(!debug.contains("FootnoteReference"), "{source:?}: {debug}");
        assert!(!debug.contains("FootnoteDefinition"), "{source:?}: {debug}");
    }
    let debug = format!("{:?}", parse("[^a\\[b]").document.children);
    assert!(debug.contains("FootnoteReference"), "{debug}");
}

#[test]
fn an_angle_autolink_holds_whitespace_other_than_a_space() {
    let document = parse("<http://a\u{a0}b>").document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("{:?}", document.children);
    };
    let [Inline::Autolink(autolink)] = paragraph.children.as_slice() else {
        panic!("{paragraph:?}");
    };
    assert_eq!(autolink.destination().as_deref(), Some("http://a\u{a0}b"));
    assert!(document.validate().is_empty());
    assert_eq!(document.to_markdown().unwrap(), "<http://a\u{a0}b>\n");
}

#[test]
fn a_referenced_space_makes_no_hard_break() {
    let blocks = parse("a&#x20; \nb").document.children;
    let [Block::Paragraph(paragraph)] = blocks.as_slice() else {
        panic!("{blocks:?}");
    };
    assert!(
        matches!(
            paragraph.children.as_slice(),
            [Inline::Text(text), Inline::CharacterReference(space), Inline::SoftBreak(_), Inline::Text(_)]
                if text.value == "a" && space.value().as_deref() == Some(" ")
        ),
        "{blocks:?}"
    );
}

#[test]
fn a_processing_instruction_closes_after_its_opener() {
    let blocks = parse("a<?> b").document.children;
    let debug = format!("{blocks:?}");
    assert!(!debug.contains("Html"), "{debug}");
}
