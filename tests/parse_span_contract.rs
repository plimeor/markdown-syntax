use markdown_syntax::{parse, Block, Inline, Span};

#[test]
fn top_level_block_spans_slice_the_original_source() {
    for (name, source) in [
        (
            "ordinary_markdown",
            "# Title\n\nparagraph with *emphasis*\n\n- one\n- two\n",
        ),
        ("crlf_line_endings", "# Title\r\n\r\nparagraph\r\n"),
        ("leading_bom", "\u{feff}# title\n\nparagraph\n"),
        ("embedded_nul", "alpha \u{0} beta\n\nparagraph\n"),
        ("unterminated_fence", "```rust\nlet value = 1;\n"),
    ] {
        assert_original_source_tiling(name, source);
    }
}

fn assert_original_source_tiling(name: &str, source: &str) {
    let output = parse(source);
    // A leading byte order mark is not content, so it may precede the first block.
    let mut cursor = if source.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };

    for (index, block) in output.document.children.iter().enumerate() {
        let span = block
            .span()
            .unwrap_or_else(|| panic!("{name}: top-level block {index} has no span"));

        assert!(
            span.start >= cursor,
            "{name}: top-level block {index} overlaps the previous span: {span:?}"
        );
        assert!(
            span.end <= source.len(),
            "{name}: top-level block {index} span exceeds source length: {span:?}, len={}",
            source.len()
        );
        assert!(
            source.is_char_boundary(span.start) && source.is_char_boundary(span.end),
            "{name}: top-level block {index} span is not on UTF-8 boundaries: {span:?}"
        );

        assert!(
            source[cursor..span.start].chars().all(char::is_whitespace),
            "{name}: non-trivia bytes before top-level block {index}: {:?}",
            &source[cursor..span.start]
        );

        let _slice = &source[span.start..span.end];
        cursor = span.end;
    }

    assert!(
        source[cursor..].chars().all(char::is_whitespace),
        "{name}: non-trivia bytes after final top-level block: {:?}",
        &source[cursor..]
    );
}

#[test]
fn inline_container_spans_cover_their_delimiters_and_content() {
    for source in [
        "***a***",
        "**a*",
        "x ***a* b**",
        "***a** b*",
        "*a **b** c*",
        "_a __b__ c_",
        "~~a *b* c~~",
        "==a *b* c==",
        "++a ^b^ c++",
        "||a ~b~ c||",
        "[*a* b](u)",
    ] {
        let output = parse(source);
        let [Block::Paragraph(paragraph)] = output.document.children.as_slice() else {
            panic!("{source:?}: expected one paragraph");
        };
        let span = paragraph.meta.span.expect("parsed paragraph has a span");
        assert_inline_spans(source, span, &paragraph.children);
    }
}

/// Each inline lies inside its parent and after its previous sibling, and an
/// emphasis-like container starts and ends on its own delimiters.
fn assert_inline_spans(source: &str, parent: Span, inlines: &[Inline]) {
    let mut cursor = parent.start;
    for inline in inlines {
        let span = inline
            .span()
            .unwrap_or_else(|| panic!("{source:?}: {inline:?} has no span"));
        assert!(
            cursor <= span.start && span.end <= parent.end,
            "{source:?}: {inline:?} at {span:?} leaves {parent:?} or overlaps its previous sibling"
        );
        let marker = match inline {
            Inline::Emphasis(_) | Inline::Strong(_) => Some(['*', '_']),
            Inline::Delete(_) | Inline::Subscript(_) => Some(['~', '~']),
            Inline::Mark(_) => Some(['=', '=']),
            Inline::Insert(_) => Some(['+', '+']),
            Inline::Spoiler(_) => Some(['|', '|']),
            Inline::Superscript(_) => Some(['^', '^']),
            _ => None,
        };
        if let Some(marker) = marker {
            let slice = &source[span.start..span.end];
            assert!(
                slice.starts_with(marker) && slice.ends_with(marker),
                "{source:?}: {inline:?} spans {slice:?}"
            );
        }
        assert_inline_spans(source, span, inline.children());
        cursor = span.end;
    }
}

#[test]
fn emphasis_like_spans_start_and_end_on_their_own_delimiters() {
    let spans = |source: &str| {
        let output = parse(source);
        let [Block::Paragraph(paragraph)] = output.document.children.as_slice() else {
            panic!("{source:?}: expected one paragraph");
        };
        let mut found = Vec::new();
        let mut stack: Vec<&Inline> = paragraph.children.iter().rev().collect();
        while let Some(inline) = stack.pop() {
            let kind = match inline {
                Inline::Text(_) => "Text",
                Inline::Emphasis(_) => "Emphasis",
                Inline::Strong(_) => "Strong",
                _ => "Other",
            };
            let span = inline.span().expect("parsed inline has a span");
            found.push((kind, span.start, span.end));
            stack.extend(inline.children().iter().rev());
        }
        found
    };
    assert_eq!(
        spans("***a***"),
        [("Emphasis", 0, 7), ("Strong", 1, 6), ("Text", 3, 4)]
    );
    assert_eq!(
        spans("x ***a* b**"),
        [
            ("Text", 0, 2),
            ("Strong", 2, 11),
            ("Emphasis", 4, 7),
            ("Text", 5, 6),
            ("Text", 7, 9)
        ]
    );
    assert_eq!(
        spans("**a*"),
        [("Text", 0, 1), ("Emphasis", 1, 4), ("Text", 2, 3)]
    );
}
