//! Emphasis-like marks (`*`, `_`, `~~`, `~`, `^`, `++`, `==`, `||`, underline
//! `__`) pair on one delimiter stack: closers are taken in source order, and
//! marks that do not cross nest as written.

use markdown_syntax::prelude::*;

/// A compact rendering of inline structure: text in quotes, a soft break as
/// `/`, code spans as `Code(...)`, containers as `Kind[...]`.
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
            Inline::Code(code) => {
                out.push_str(&format!("Code({:?})", code.value));
                continue;
            }
            Inline::Emphasis(node) => ("Emphasis", &node.children),
            Inline::Strong(node) => ("Strong", &node.children),
            Inline::Underline(node) => ("Underline", &node.children),
            Inline::Delete(node) => ("Delete", &node.children),
            Inline::Insert(node) => ("Insert", &node.children),
            Inline::Mark(node) => ("Mark", &node.children),
            Inline::Spoiler(node) => ("Spoiler", &node.children),
            Inline::Subscript(node) => ("Subscript", &node.children),
            Inline::Superscript(node) => ("Superscript", &node.children),
            Inline::Link(node) => ("Link", &node.children),
            Inline::Image(node) => ("Image", &node.alt),
            other => panic!("unexpected inline {other:?}"),
        };
        out.push_str(kind);
        out.push('[');
        out.push_str(&shape(children));
        out.push(']');
    }
    out
}

fn parsed(options: &SyntaxOptions, markdown: &str) -> String {
    match options.parse(markdown).document.children.as_slice() {
        [Block::Paragraph(paragraph)] => shape(&paragraph.children),
        other => panic!("expected one paragraph, got {other:?}"),
    }
}

#[test]
fn strong_closes_before_a_highlight() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "**a ==b** c=="),
        r#"Strong["a ==b"]" c==""#
    );
}

#[test]
fn emphasis_closes_before_an_insert() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "*a ++b* c++"),
        r#"Emphasis["a ++b"]" c++""#
    );
}

#[test]
fn highlight_closes_before_strong() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "==a **b== c**"),
        r#"Mark["a **b"]" c**""#
    );
}

#[test]
fn marks_that_do_not_cross_nest_as_written() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "==a *b* c=="),
        r#"Mark["a "Emphasis["b"]" c"]"#
    );
    assert_eq!(
        parsed(
            &SyntaxOptions::default(),
            "*a ||b ~~c ^d^ e~~ f|| ++g ~h~ i++ j*"
        ),
        r#"Emphasis["a "Spoiler["b "Delete["c "Superscript["d"]" e"]" f"]" "Insert["g "Subscript["h"]" i"]" j"]"#
    );
}

#[test]
fn emphasis_right_inside_a_mark_stays_inside_it() {
    // The `*` touching `~~` could also close the outer `*`; inside the spoiler
    // it opens, as in the spoiler's content read on its own.
    assert_eq!(
        parsed(&SyntaxOptions::default(), "*a ||~~*b*~~|| c*"),
        r#"Emphasis["a "Spoiler[Delete[Emphasis["b"]]]" c"]"#
    );
    assert_eq!(
        parsed(&SyntaxOptions::default(), "~ **a*~"),
        r#"Subscript[" *"Emphasis["a"]]"#
    );
}

#[test]
fn underline_follows_underscore_strong() {
    let underline = SyntaxOptions::default().enable(Construct::Underline);
    assert_eq!(parsed(&underline, "a __b__ c"), r#""a "Underline["b"]" c""#);
    assert_eq!(parsed(&underline, "___a___"), r#"Emphasis[Underline["a"]]"#);
    assert_eq!(
        parsed(&underline, "__a ~~__b__~~ c__"),
        r#"Underline["a "Delete[Underline["b"]]" c"]"#
    );
}

#[test]
fn subscript_and_superscript_close_at_the_first_marker_on_their_line() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "~a ~b~"),
        r#"Subscript["a "]"b~""#
    );
    assert_eq!(
        parsed(&SyntaxOptions::default(), "^^a^"),
        r#""^"Superscript["a"]"#
    );
    assert_eq!(parsed(&SyntaxOptions::default(), "^a\nb^"), r#""^a"/"b^""#);
}

#[test]
fn deeply_nested_highlights_stop_at_32_levels() {
    let depth = 40;
    let markdown = format!("{}x{}", "==a ".repeat(depth), " b==".repeat(depth));
    let shape = parsed(&SyntaxOptions::default(), &markdown);
    assert_eq!(shape.matches("Mark[").count(), 32, "{shape}");
}

#[test]
fn deep_emphasis_stops_at_16_levels() {
    let depth = 20;
    let markdown = format!("{}x{}", "*a ".repeat(depth), " b*".repeat(depth));
    let shape = parsed(&SyntaxOptions::default(), &markdown);
    assert_eq!(shape.matches("Emphasis[").count(), 16, "{shape}");
}

#[test]
fn code_spans_bind_tighter_than_marks() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "^a `^` b^"),
        r#"Superscript["a "Code("^")" b"]"#
    );
    assert_eq!(
        parsed(&SyntaxOptions::default(), "==a `b== c` d=="),
        r#"Mark["a "Code("b== c")" d"]"#
    );
}

#[test]
fn a_mark_opened_inside_a_link_stays_inside_it() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "[a ==b](u) c=="),
        r#"Link["a ==b"]" c==""#
    );
    assert_eq!(
        parsed(&SyntaxOptions::default(), "*[foo*](/u)"),
        r#""*"Link["foo*"]"#
    );
}

#[test]
fn deeply_nested_images_stop_at_32_levels() {
    let depth = 40;
    let markdown = format!("{}x{}", "![".repeat(depth), "](u)".repeat(depth));
    let document = SyntaxOptions::default().parse(&markdown).document;
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
    assert_eq!(
        parsed(&SyntaxOptions::commonmark(), "[a [b](u) c](v)"),
        r#""[a "Link["b"]" c](v)""#
    );
    assert_eq!(
        parsed(&SyntaxOptions::commonmark(), "![a [b](u) c](v)"),
        r#"Image["a "Link["b"]" c"]"#
    );
}

#[test]
fn an_unclosed_inline_footnote_leaves_its_caret_to_close_a_superscript() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "^a ^[b"),
        r#"Superscript["a "]"[b""#
    );
}

#[test]
fn a_caret_closes_a_waiting_superscript_before_a_bracket() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "a^b^[link](u)"),
        r#""a"Superscript["b"]Link["link"]"#
    );
}

#[test]
fn a_long_closing_run_reopens_with_what_it_has_left() {
    assert_eq!(
        parsed(&SyntaxOptions::default(), "*x ++a++++* b++"),
        r#""*x "Insert["a"]Insert["* b"]"#
    );
    assert_eq!(
        parsed(&SyntaxOptions::default(), "*x ||a||||* b||"),
        r#""*x "Spoiler["a"]Spoiler["* b"]"#
    );
}

#[test]
fn underline_spans_cover_their_own_delimiters() {
    let underline = SyntaxOptions::default().enable(Construct::Underline);
    let document = underline.parse("___a_ b__").document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    let [Inline::Underline(node)] = paragraph.children.as_slice() else {
        panic!("expected one underline, got {:?}", paragraph.children);
    };
    assert_eq!(node.meta.span, Some(Span::new(0, 9)));
}

#[test]
fn brackets_past_the_limit_close_as_text() {
    let depth = 40;
    let markdown = format!("{}x{}", "![".repeat(depth), "](u)".repeat(depth));
    let document = SyntaxOptions::default().parse(&markdown).document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    let [Inline::Image(outer)] = paragraph.children.as_slice() else {
        panic!("expected one outermost image, got {:?}", paragraph.children);
    };
    assert_eq!(outer.meta.span, Some(Span::new(0, markdown.len())));
}

#[test]
fn an_image_that_does_not_form_yields_to_a_wikilink_at_its_bracket() {
    let document = SyntaxOptions::default().parse("![[a]b]]").document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    assert!(
        matches!(paragraph.children.as_slice(), [Inline::Text(bang), Inline::WikiLink(link)]
            if bang.value == "!" && link.target == "a]b"),
        "{:?}",
        paragraph.children
    );
}

#[test]
fn a_link_inside_a_directive_label_inside_a_mark_keeps_links_from_nesting() {
    let document = SyntaxOptions::default()
        .parse("[x :d[==[b](u)==] y](v)")
        .document;
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
fn an_escaped_dot_after_a_bracket_is_a_dot() {
    let mut gfm = SyntaxOptions::gfm();
    gfm.constructs.relaxed_autolinks = false;
    assert_eq!(parsed(&gfm, "[www. \\. x"), r#""[www. . x""#);
}

#[test]
fn delimiters_past_the_limit_stay_in_the_output() {
    let input = String::from("++") + &"==a ".repeat(32) + "x" + &" b==".repeat(32) + "++++c++";
    let markdown = SyntaxOptions::default()
        .parse(&input)
        .document
        .to_markdown()
        .unwrap();
    assert_eq!(markdown.matches('+').count(), input.matches('+').count());

    let underline = SyntaxOptions::default().enable(Construct::Underline);
    let input = String::from("___") + &"*".repeat(32) + "x" + &"*".repeat(32) + "_ b__";
    let markdown = underline.parse(&input).document.to_markdown().unwrap();
    assert_eq!(markdown.matches('_').count(), input.matches('_').count());
}

#[test]
fn an_image_whose_label_cannot_close_yields_to_a_wikilink() {
    let document = SyntaxOptions::default().parse("![[a[b]]").document;
    let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
        panic!("expected one paragraph");
    };
    assert!(
        matches!(paragraph.children.as_slice(), [Inline::Text(bang), Inline::WikiLink(link)]
            if bang.value == "!" && link.target == "a[b"),
        "{:?}",
        paragraph.children
    );
}
