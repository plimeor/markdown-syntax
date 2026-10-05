//! A leading byte order mark is ignored and U+0000 reads as U+FFFD, as
//! CommonMark and cmark/comrak/micromark do, while spans stay in the
//! coordinates of the original input.

use markdown_syntax::prelude::*;

fn paragraph_inlines(document: &Document) -> &[Inline] {
    match document.children.as_slice() {
        [Block::Paragraph(paragraph)] => &paragraph.children,
        other => panic!("expected one paragraph, got {other:?}"),
    }
}

fn text_of(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(text) => text.value.clone(),
            other => panic!("expected text, got {other:?}"),
        })
        .collect()
}

#[test]
fn leading_bom_is_ignored_and_spans_stay_in_source_coordinates() {
    let document = parse("\u{feff}# title").document;
    let [Block::Heading(heading)] = document.children.as_slice() else {
        panic!("expected one heading, got {:?}", document.children);
    };
    assert_eq!(text_of(&heading.children), "title");
    assert_eq!(heading.meta.span.map(|span| span.start), Some(3));
    assert_eq!(
        document.meta.span,
        Some(Span::new(0, "\u{feff}# title".len()))
    );
}

#[test]
fn bom_only_input_is_an_empty_document() {
    assert!(parse("\u{feff}").document.children.is_empty());
}

#[test]
fn bom_inside_a_line_is_kept() {
    let document = parse("# hea\u{feff}ding").document;
    let [Block::Heading(heading)] = document.children.as_slice() else {
        panic!("expected one heading, got {:?}", document.children);
    };
    assert_eq!(text_of(&heading.children), "hea\u{feff}ding");
}

#[test]
fn nul_in_text_reads_as_replacement_character() {
    let document = parse("a\u{0}b").document;
    let inlines = paragraph_inlines(&document);
    assert_eq!(text_of(inlines), "a\u{FFFD}b");
    assert_eq!(inlines[0].span(), Some(Span::new(0, 3)));
}

#[test]
fn nul_in_link_destination_reads_as_replacement_character() {
    let document = parse("[a](\u{0})").document;
    let [Inline::Link(link)] = paragraph_inlines(&document) else {
        panic!("expected a link, got {:?}", document.children);
    };
    assert_eq!(link.destination, "\u{FFFD}");
}

#[test]
fn nul_in_image_destination_reads_as_replacement_character() {
    let document = parse("![](\\#\u{0})").document;
    let [Inline::Image(image)] = paragraph_inlines(&document) else {
        panic!("expected an image, got {:?}", document.children);
    };
    assert_eq!(image.destination, "#\u{FFFD}");
}

#[test]
fn nul_is_punctuation_for_emphasis_flanking() {
    // U+FFFD is a Unicode symbol, so it counts as punctuation for flanking:
    // `a*\u{FFFD}*b` has a closer-only `*` followed by an opener-only `*`.
    let document = parse("a*\u{0}*b").document;
    assert_eq!(text_of(paragraph_inlines(&document)), "a*\u{FFFD}*b");
}

#[test]
fn nul_in_uri_autolink_reads_as_replacement_character() {
    let document = SyntaxOptions::commonmark().parse("<ab:c\u{0}d>").document;
    let [Inline::Autolink(autolink)] = paragraph_inlines(&document) else {
        panic!("expected an autolink, got {:?}", document.children);
    };
    assert_eq!(autolink.destination, "ab:c\u{FFFD}d");
}

#[test]
fn nul_is_replaced_in_every_value() {
    let source = concat!(
        "---\nk: \u{0}\n---\n\n",
        "```\u{0}\n\u{0}\n```\n\n",
        "`\u{0}` <b\u{0}> [x](/u \"\u{0}\")\n\n",
        "[\u{0}]: /d\u{0}\n",
    );
    let output = parse(source);
    let debug = format!("{:?}", output.document);
    assert!(!debug.contains("\\0"), "NUL survived in {debug}");
    assert!(debug.contains('\u{FFFD}'));
}

#[test]
fn nul_in_a_literal_autolink_host_reads_as_replacement_character() {
    let with_nul = parse("www.a\u{0}b_c.d_e").document;
    let with_replacement = parse("www.a\u{FFFD}b_c.d_e").document;
    let is_autolink = |document: &Document| {
        paragraph_inlines(document)
            .iter()
            .any(|inline| matches!(inline, Inline::Autolink(_)))
    };
    assert_eq!(is_autolink(&with_nul), is_autolink(&with_replacement));
}
