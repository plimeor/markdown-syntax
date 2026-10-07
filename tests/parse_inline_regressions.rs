//! Inline-parsing regression coverage: emphasis/strong delimiter resolution,
//! the inline delimiter stack (asterisk/underscore/tilde/underline), and the
//! Unicode-awareness fixes for flanking and reference-label folding.
//!
//! Each former regression file is preserved verbatim inside its own `mod` so
//! that helper functions and test names cannot collide across the merged
//! sources.

mod emphasis {
    //! Regression coverage for CommonMark `*`/`_` emphasis resolution.
    //!
    //! These cases exercise the delimiter-stack matcher in `parse_inlines`,
    //! particularly partial matches where an opener run is longer than the closer
    //! consumes (or vice versa): the leftover delimiters must stay outside the
    //! emphasis, and closers must bind to the nearest preceding compatible opener.

    use markdown_syntax::{parse, Block, Inline};

    /// Parses `input` as CommonMark and returns the inlines of the first paragraph.
    fn paragraph_inlines(input: &str) -> Vec<Inline> {
        let output = parse(input);
        match output.document.children.into_iter().next() {
            Some(Block::Paragraph(paragraph)) => paragraph.children,
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    fn text(node: &Inline) -> &str {
        match node {
            Inline::Text(value) => &value.value,
            other => panic!("expected text, got {other:?}"),
        }
    }

    fn emphasis(node: &Inline) -> &[Inline] {
        match node {
            Inline::Emphasis(value) => &value.children,
            other => panic!("expected emphasis, got {other:?}"),
        }
    }

    fn strong(node: &Inline) -> &[Inline] {
        match node {
            Inline::Strong(value) => &value.children,
            other => panic!("expected strong, got {other:?}"),
        }
    }

    /// `*foo*` -> emphasis "foo".
    #[test]
    fn single_star_is_emphasis() {
        let nodes = paragraph_inlines("*foo*");
        assert_eq!(nodes.len(), 1);
        assert_eq!(text(&emphasis(&nodes[0])[0]), "foo");
    }

    /// `**foo**` -> strong "foo".
    #[test]
    fn double_star_is_strong() {
        let nodes = paragraph_inlines("**foo**");
        assert_eq!(nodes.len(), 1);
        assert_eq!(text(&strong(&nodes[0])[0]), "foo");
    }

    /// `***foo***` -> emphasis wrapping strong "foo".
    #[test]
    fn triple_star_is_emphasis_around_strong() {
        let nodes = paragraph_inlines("***foo***");
        assert_eq!(nodes.len(), 1);
        let inner = emphasis(&nodes[0]);
        assert_eq!(inner.len(), 1);
        assert_eq!(text(&strong(&inner[0])[0]), "foo");
    }

    /// `**foo*` -> leftover `*` stays to the LEFT of the emphasis it could not strengthen.
    #[test]
    fn double_open_single_close_leaves_star_left() {
        let nodes = paragraph_inlines("**foo*");
        assert_eq!(nodes.len(), 2);
        assert_eq!(text(&nodes[0]), "*");
        assert_eq!(text(&emphasis(&nodes[1])[0]), "foo");
    }

    /// `*foo**` -> leftover `*` stays to the RIGHT of the emphasis.
    #[test]
    fn single_open_double_close_leaves_star_right() {
        let nodes = paragraph_inlines("*foo**");
        assert_eq!(nodes.len(), 2);
        assert_eq!(text(&emphasis(&nodes[0])[0]), "foo");
        assert_eq!(text(&nodes[1]), "*");
    }

    /// `***foo*` -> leftover `**` stays to the LEFT.
    #[test]
    fn triple_open_single_close_leaves_double_star_left() {
        let nodes = paragraph_inlines("***foo*");
        assert_eq!(nodes.len(), 2);
        assert_eq!(text(&nodes[0]), "**");
        assert_eq!(text(&emphasis(&nodes[1])[0]), "foo");
    }

    /// `*foo***` -> leftover `**` stays to the RIGHT.
    #[test]
    fn single_open_triple_close_leaves_double_star_right() {
        let nodes = paragraph_inlines("*foo***");
        assert_eq!(nodes.len(), 2);
        assert_eq!(text(&emphasis(&nodes[0])[0]), "foo");
        assert_eq!(text(&nodes[1]), "**");
    }

    /// `**foo*bar*` -> the unmatched `**` merges with `foo`; `*bar*` is emphasis.
    #[test]
    fn unmatched_double_open_merges_with_following_text() {
        let nodes = paragraph_inlines("**foo*bar*");
        assert_eq!(nodes.len(), 2);
        assert_eq!(text(&nodes[0]), "**foo");
        assert_eq!(text(&emphasis(&nodes[1])[0]), "bar");
    }

    /// `*foo**bar*` -> the inner `**` cannot strengthen, so it stays literal inside.
    #[test]
    fn interior_double_star_stays_literal_inside_emphasis() {
        let nodes = paragraph_inlines("*foo**bar*");
        assert_eq!(nodes.len(), 1);
        let inner = emphasis(&nodes[0]);
        assert_eq!(inner.len(), 1);
        assert_eq!(text(&inner[0]), "foo**bar");
    }

    /// `**bold*****bold+italic***` -> strong "bold" then emphasis-around-strong.
    #[test]
    fn adjacent_runs_split_into_strong_then_emphasis_strong() {
        let nodes = paragraph_inlines("**bold*****bold+italic***");
        assert_eq!(nodes.len(), 2);
        assert_eq!(text(&strong(&nodes[0])[0]), "bold");

        let outer = emphasis(&nodes[1]);
        assert_eq!(outer.len(), 1);
        assert_eq!(text(&strong(&outer[0])[0]), "bold+italic");
    }

    /// `****foo****` -> nested strong (strong "foo").
    #[test]
    fn quadruple_star_is_nested_strong() {
        let nodes = paragraph_inlines("****foo****");
        assert_eq!(nodes.len(), 1);
        let outer = strong(&nodes[0]);
        assert_eq!(outer.len(), 1);
        assert_eq!(text(&strong(&outer[0])[0]), "foo");
    }

    /// `*foo* **bar**` -> emphasis, space, strong.
    #[test]
    fn separate_runs_resolve_independently() {
        let nodes = paragraph_inlines("*foo* **bar**");
        assert_eq!(nodes.len(), 3);
        assert_eq!(text(&emphasis(&nodes[0])[0]), "foo");
        assert_eq!(text(&nodes[1]), " ");
        assert_eq!(text(&strong(&nodes[2])[0]), "bar");
    }

    /// `a*b*c` -> intraword `*` still opens/closes emphasis.
    #[test]
    fn intraword_star_emphasis_resolves() {
        let nodes = paragraph_inlines("a*b*c");
        assert_eq!(nodes.len(), 3);
        assert_eq!(text(&nodes[0]), "a");
        assert_eq!(text(&emphasis(&nodes[1])[0]), "b");
        assert_eq!(text(&nodes[2]), "c");
    }
}

mod inline_delimiter {
    use markdown_syntax::{parse, Block, Inline};

    #[test]
    fn asterisk_mixed_runs_nest_emphasis_and_strong() {
        let inlines = paragraph("**foo *bar***\n");
        let [Inline::Strong(strong)] = inlines.as_slice() else {
            panic!("expected outer strong");
        };
        let [Inline::Text(prefix), Inline::Emphasis(emphasis)] = strong.children.as_slice() else {
            panic!("expected text followed by inner emphasis");
        };
        assert_eq!(prefix.value, "foo ");
        assert_text(emphasis.children.as_slice(), "bar");

        let inlines = paragraph("*foo **bar***\n");
        let [Inline::Emphasis(emphasis)] = inlines.as_slice() else {
            panic!("expected outer emphasis");
        };
        let [Inline::Text(prefix), Inline::Strong(strong)] = emphasis.children.as_slice() else {
            panic!("expected text followed by inner strong");
        };
        assert_eq!(prefix.value, "foo ");
        assert_text(strong.children.as_slice(), "bar");
    }

    #[test]
    fn underscore_triple_and_mixed_runs_nest_emphasis_and_strong() {
        let inlines = paragraph("___foo___\n");
        let [Inline::Emphasis(emphasis)] = inlines.as_slice() else {
            panic!("expected outer emphasis");
        };
        let [Inline::Strong(strong)] = emphasis.children.as_slice() else {
            panic!("expected inner strong");
        };
        assert_text(strong.children.as_slice(), "foo");

        let inlines = paragraph("__foo _bar___\n");
        let [Inline::Strong(strong)] = inlines.as_slice() else {
            panic!("expected outer strong");
        };
        let [Inline::Text(prefix), Inline::Emphasis(emphasis)] = strong.children.as_slice() else {
            panic!("expected text followed by inner emphasis");
        };
        assert_eq!(prefix.value, "foo ");
        assert_text(emphasis.children.as_slice(), "bar");
    }

    #[test]
    fn intraword_underscore_stays_text() {
        let inlines = paragraph("foo_bar_baz\n");
        assert_text(inlines.as_slice(), "foo_bar_baz");
    }

    #[test]
    fn strikethrough_coexists_with_attention_when_gfm_is_enabled() {
        let inlines = paragraph("~~two *emphasis* two~~\n");
        let [Inline::Delete(delete)] = inlines.as_slice() else {
            panic!("expected delete");
        };
        let [Inline::Text(prefix), Inline::Emphasis(emphasis), Inline::Text(suffix)] =
            delete.children.as_slice()
        else {
            panic!("expected delete containing emphasis");
        };
        assert_eq!(prefix.value, "two ");
        assert_text(emphasis.children.as_slice(), "emphasis");
        assert_eq!(suffix.value, " two");

        let inlines = paragraph("***~~xxx~~***\n");
        let [Inline::Emphasis(emphasis)] = inlines.as_slice() else {
            panic!("expected outer emphasis");
        };
        let [Inline::Strong(strong)] = emphasis.children.as_slice() else {
            panic!("expected inner strong");
        };
        let [Inline::Delete(delete)] = strong.children.as_slice() else {
            panic!("expected delete inside strong");
        };
        assert_text(delete.children.as_slice(), "xxx");
    }

    #[test]
    fn strikethrough_takes_two_tildes() {
        let inlines = paragraph("a ~one~ b and ~~two~~ c\n");
        let [Inline::Text(prefix), Inline::Delete(two), Inline::Text(suffix)] = inlines.as_slice()
        else {
            panic!("expected one double tilde delete node: {inlines:?}");
        };
        assert_eq!(prefix.value, "a ~one~ b and ");
        assert_text(two.children.as_slice(), "two");
        assert_eq!(suffix.value, " c");

        let inlines = paragraph("a ~one~ b\n");
        assert_text(inlines.as_slice(), "a ~one~ b");

        let inlines = paragraph("H~2~O and ~gone~\n");
        assert_text(inlines.as_slice(), "H~2~O and ~gone~");
    }

    #[test]
    fn double_underscore_inside_a_triple_run_is_strong() {
        let inlines = paragraph("___foo___\n");
        let [Inline::Emphasis(emphasis)] = inlines.as_slice() else {
            panic!("expected outer emphasis");
        };
        let [Inline::Strong(strong)] = emphasis.children.as_slice() else {
            panic!("expected inner strong");
        };
        assert_text(strong.children.as_slice(), "foo");
    }

    fn paragraph(source: &str) -> Vec<Inline> {
        let output = parse(source);
        assert!(
            output.diagnostics.is_empty(),
            "expected no parse diagnostics: {:?}",
            output.diagnostics
        );
        let [Block::Paragraph(paragraph)] = output.document.children.as_slice() else {
            panic!("expected a single paragraph");
        };
        paragraph.children.clone()
    }

    fn assert_text(inlines: &[Inline], expected: &str) {
        let [Inline::Text(text)] = inlines else {
            panic!("expected a single text inline");
        };
        assert_eq!(text.value, expected);
    }
}

mod review_inline {
    use markdown_syntax::{parse, Block, Inline, LineBreakKind, LinkDestinationKind};

    fn parse_blocks(input: &str) -> Vec<Block> {
        parse(input).document.children
    }

    fn only_paragraph(input: &str) -> Vec<Inline> {
        let blocks = parse_blocks(input);
        let [Block::Paragraph(paragraph)] = blocks.as_slice() else {
            panic!("expected a single paragraph, got {blocks:?}");
        };
        paragraph.children.clone()
    }

    #[test]
    fn h1_bang_declaration_is_an_html_block() {
        let blocks = parse_blocks("<!a>\nbar\n");
        let Some(Block::HtmlBlock(html)) = blocks.first() else {
            panic!("expected an HTML declaration block, got {blocks:?}");
        };
        assert_eq!(html.value, "<!a>");
        assert!(matches!(blocks.get(1), Some(Block::Paragraph(_))));
    }

    #[test]
    fn h1_bang_declaration_is_inline_html() {
        let inlines = only_paragraph("a <!b\nc>\n");
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Text(text), Inline::Html(html)]
                if text.value == "a " && html.value == "<!b\nc>"
        ));
    }

    #[test]
    fn i2_tab_after_trailing_spaces_is_a_soft_break() {
        let inlines = only_paragraph("aaa  \t\nbb\n");
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Text(a), Inline::SoftBreak(_), Inline::Text(b)]
                if a.value == "aaa" && b.value == "bb"
        ));
    }

    #[test]
    fn i2_pure_double_space_remains_a_hard_break() {
        let inlines = only_paragraph("aaa  \nbb\n");
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Text(a), Inline::LineBreak(br), Inline::Text(b)]
                if a.value == "aaa" && b.value == "bb" && br.kind == LineBreakKind::Spaces
        ));
    }

    #[test]
    fn l1_bracketed_reference_label_is_literal() {
        let blocks = parse_blocks("[ref[bar]]: /uri\n\n[foo][ref[bar]]\n");
        assert_eq!(blocks.len(), 2);
        assert!(
            blocks
                .iter()
                .all(|block| matches!(block, Block::Paragraph(_))),
            "expected two literal paragraphs, got {blocks:?}"
        );
    }

    #[test]
    fn l1_inline_link_text_still_nests_brackets() {
        let inlines = only_paragraph("[a[b]](u)\n");
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Link(link)] if link.destination == "u"
        ));
    }

    #[test]
    fn l5_blank_definition_label_is_literal() {
        let blocks = parse_blocks("[ ]: /uri\n");
        assert!(matches!(blocks.as_slice(), [Block::Paragraph(_)]));
    }

    #[test]
    fn l4_unicode_space_is_part_of_bare_destination() {
        let inlines = only_paragraph("[a](/url\u{00A0}\"title\")\n");
        let [Inline::Link(link)] = inlines.as_slice() else {
            panic!("expected a single link, got {inlines:?}");
        };
        assert_eq!(link.destination, "/url\u{00A0}\"title\"");
        assert_eq!(link.destination_kind, LinkDestinationKind::Bare);
        assert!(link.title.is_none());
    }

    #[test]
    fn l2_dotless_email_autolink_is_valid() {
        let inlines = only_paragraph(
            "<asd@012345678901234567890123456789012345678901234567890123456789012>\n",
        );
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Autolink(autolink)]
                if autolink.destination().as_deref()
                    == Some("mailto:asd@012345678901234567890123456789012345678901234567890123456789012")
        ));
    }

    #[test]
    fn g1_delimiter_cells_reject_interior_colons_and_spaces() {
        for source in ["|a|\n|-:-|\n", "|a|\n|- -|\n", "|a|\n|-::|\n"] {
            let blocks = parse_blocks(source);
            assert!(
                blocks
                    .iter()
                    .all(|block| matches!(block, Block::Paragraph(_))),
                "{source:?} should not form a table, got {blocks:?}"
            );
        }
    }

    #[test]
    fn g1_valid_delimiter_cells_still_form_a_table() {
        for source in [
            "|a|\n|:-:|\n",
            "|a|\n|---|\n",
            "|a|\n|:--|\n",
            "|a|\n|--:|\n",
        ] {
            let blocks = parse_blocks(source);
            assert!(
                matches!(blocks.as_slice(), [Block::Table(_)]),
                "{source:?} should still form a table, got {blocks:?}"
            );
        }
    }

    #[test]
    fn g2_www_autolink_rejects_underscore_in_last_two_segments() {
        let inlines = only_paragraph("www.aaa.bbb.ccc_ccc\n");
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Text(text)] if text.value == "www.aaa.bbb.ccc_ccc"
        ));
    }

    #[test]
    fn g2_www_autolink_allows_underscore_before_last_two_segments() {
        let inlines = only_paragraph("www.aaa.bbb_bbb.ccc.ddd\n");
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Autolink(autolink)] if autolink.destination().as_deref() == Some("http://www.aaa.bbb_bbb.ccc.ddd")
        ));
    }

    #[test]
    fn g3_literal_email_allows_underscore_in_domain() {
        let inlines = only_paragraph("a@a_b.c\n");
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Autolink(autolink)] if autolink.destination().as_deref() == Some("mailto:a@a_b.c")
        ));
    }

    #[test]
    fn g3_literal_email_rejects_trailing_underscore_period() {
        let inlines = only_paragraph("aaa@a.b_.\n");
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Text(text)] if text.value == "aaa@a.b_."
        ));
    }

    #[test]
    fn hg2_literal_link_excludes_trailing_entity_run() {
        let inlines = only_paragraph("www.example.com&xxx;.\n");
        let [Inline::Autolink(autolink), Inline::Text(rest)] = inlines.as_slice() else {
            panic!("expected an autolink followed by literal text, got {inlines:?}");
        };
        assert_eq!(
            autolink.destination().as_deref(),
            Some("http://www.example.com")
        );
        assert_eq!(rest.value, "&xxx;.");
    }

    #[test]
    fn hg2_literal_link_keeps_entity_run_without_semicolon() {
        let inlines = only_paragraph("www.example.com&xxx\n");
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Autolink(autolink)] if autolink.destination().as_deref() == Some("http://www.example.com&xxx")
        ));
    }

    #[test]
    fn g4_table_body_stops_at_a_block_start() {
        let blocks = parse_blocks("| a |\n| - |\n> b | c\n");
        assert!(
            matches!(blocks.first(), Some(Block::Table(_))),
            "expected a table first, got {blocks:?}"
        );
        assert!(
            matches!(blocks.get(1), Some(Block::BlockQuote(_))),
            "expected the blockquote line to start its own block, got {blocks:?}"
        );
    }
}

mod review_unicode {
    //! Regression coverage for the Unicode-awareness fixes (matching the
    //! CommonMark reference): emphasis flanking treats the Unicode `P*`/`S*`
    //! categories as punctuation (not only ASCII), and reference-label matching
    //! uses a Unicode case fold (not ASCII lowercasing).

    use markdown_syntax::{parse, Block, Inline};

    fn paragraph_inlines(input: &str) -> Vec<Inline> {
        let output = parse(input);
        match output.document.children.into_iter().next() {
            Some(Block::Paragraph(paragraph)) => paragraph.children,
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    /// `foo*…*bar` — the `*` is preceded by a letter and followed by U+2026
    /// (Unicode punctuation), so it is NOT left-flanking and cannot open emphasis.
    /// With ASCII-only punctuation classification this wrongly produced
    /// `foo<em>…</em>bar`.
    #[test]
    fn emphasis_does_not_open_before_unicode_punctuation() {
        let nodes = paragraph_inlines("foo*\u{2026}*bar\n");
        assert_eq!(
            nodes.len(),
            1,
            "expected a single literal text node: {nodes:?}"
        );
        match &nodes[0] {
            Inline::Text(text) => assert_eq!(text.value, "foo*\u{2026}*bar"),
            other => panic!("expected literal text, got {other:?}"),
        }
    }

    /// `a*。*b` — CJK full stop U+3002 is Unicode punctuation; same rule, no emphasis.
    #[test]
    fn emphasis_does_not_open_before_cjk_punctuation() {
        let nodes = paragraph_inlines("a*\u{3002}*b\n");
        assert!(
            !nodes.iter().any(|node| matches!(node, Inline::Emphasis(_))),
            "no emphasis should form around CJK punctuation: {nodes:?}"
        );
    }

    /// Control: a `*` between ASCII letters still opens/closes emphasis — the
    /// Unicode-punctuation change must not over-restrict intraword emphasis.
    #[test]
    fn emphasis_still_opens_between_ascii_letters() {
        let nodes = paragraph_inlines("foo*x*bar\n");
        assert!(
            nodes.iter().any(|node| matches!(node, Inline::Emphasis(_))),
            "intraword emphasis must still form: {nodes:?}"
        );
    }

    /// Reference labels match case-insensitively via a Unicode case fold: `[Ä]`
    /// resolves against `[ä]: /url`. ASCII lowercasing would leave `Ä` unchanged and
    /// fail to match.
    #[test]
    fn reference_label_matches_with_unicode_case_fold() {
        let nodes = paragraph_inlines("[\u{00C4}]\n\n[\u{00E4}]: /url\n");
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Inline::LinkReference(reference) => assert_eq!(reference.identifier, "\u{00E4}"),
            other => panic!("expected a resolved link reference, got {other:?}"),
        }
    }

    /// ASCII reference-label folding is unchanged by the switch to Unicode folding.
    #[test]
    fn ascii_reference_label_folding_is_unchanged() {
        let nodes = paragraph_inlines("[Ref]\n\n[ref]: /url\n");
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Inline::LinkReference(reference) => assert_eq!(reference.identifier, "ref"),
            other => panic!("expected a resolved link reference, got {other:?}"),
        }
    }
}

mod escapes_and_references {
    //! Backslash escapes and character references are always nodes of their
    //! own; inside raw-text constructs they stay part of the raw text.

    use markdown_syntax::prelude::*;

    /// Inlines as tokens: text quoted, `Escape(c)`, `Ref(value)`, and other
    /// nodes by kind.
    fn shape(inlines: &[Inline]) -> Vec<String> {
        inlines
            .iter()
            .map(|inline| match inline {
                Inline::Text(text) => format!("{:?}", text.value),
                Inline::Escape(escape) => format!("Escape({})", escape.value),
                Inline::CharacterReference(reference) => {
                    format!("Ref({})", reference.value().unwrap_or_default())
                }
                Inline::Code(code) => format!("Code({})", code.value),
                Inline::Html(html) => format!("Html({})", html.value),
                Inline::Math(math) => format!("Math({})", math.value),
                other => format!("{other:?}"),
            })
            .collect()
    }

    fn paragraph(source: &str) -> Vec<String> {
        match parse(source).document.children.as_slice() {
            [Block::Paragraph(paragraph)] => shape(&paragraph.children),
            other => panic!("{source:?}: {other:?}"),
        }
    }

    fn first_cell(source: &str) -> Vec<String> {
        match parse(source).document.children.as_slice() {
            [Block::Table(table)] => shape(&table.rows[0].cells[0].children),
            other => panic!("{source:?}: {other:?}"),
        }
    }

    #[test]
    fn escaped_punctuation_is_an_escape_node() {
        assert_eq!(
            paragraph("\\*not em\\* and \\#tag"),
            [
                "Escape(*)",
                "\"not em\"",
                "Escape(*)",
                "\" and \"",
                "Escape(#)",
                "\"tag\""
            ]
        );
    }

    #[test]
    fn a_numeric_character_reference_is_a_reference_node() {
        assert_eq!(paragraph("&#35;tag"), ["Ref(#)", "\"tag\""]);
    }

    #[test]
    fn a_backslash_inside_a_code_span_stays_raw() {
        assert_eq!(paragraph("`\\*`"), ["Code(\\*)"]);
    }

    #[test]
    fn an_escaped_pipe_in_a_cell_is_an_escape_in_text_and_a_pipe_in_raw_text() {
        assert_eq!(
            first_cell("| a\\|b |\n|-|"),
            ["\"a\"", "Escape(|)", "\"b\""]
        );
        assert_eq!(
            first_cell("| <a b=\"x\\|y\"> |\n|-|"),
            ["Html(<a b=\"x|y\">)"]
        );
        assert_eq!(first_cell("$\\|$||\n-|-"), ["Math(|)"]);
    }
}

mod wiki_embeds {
    //! A `!` directly before a wikilink's `[[` makes it an embed.

    use markdown_syntax::prelude::*;

    fn paragraph(source: &str) -> Vec<Inline> {
        match parse(source).document.children.as_slice() {
            [Block::Paragraph(paragraph)] => paragraph.children.clone(),
            other => panic!("{source:?}: {other:?}"),
        }
    }

    #[test]
    fn an_embed_forms_past_the_bracket_nesting_limit() {
        let source = "[x ".repeat(40) + "![[a]]";
        let inlines = paragraph(&source);
        let last = inlines.last().expect("inlines");
        assert!(
            matches!(last, Inline::WikiLink(link) if link.embed && link.meta.span == Some(Span::new(120, 126))),
            "{last:?}"
        );
    }

    #[test]
    fn a_bang_before_a_wikilink_makes_an_embed_spanning_from_it() {
        let inlines = paragraph("see ![[x.png]]");
        assert!(
            matches!(inlines.as_slice(), [Inline::Text(text), Inline::WikiLink(link)]
                if text.value == "see "
                    && link.target == "x.png"
                    && link.embed
                    && link.meta.span == Some(Span::new(4, 14))),
            "{inlines:?}"
        );
    }

    #[test]
    fn an_escaped_bang_leaves_a_plain_wikilink() {
        let inlines = paragraph("\\![[x.png]]");
        assert!(
            matches!(inlines.as_slice(), [Inline::Escape(bang), Inline::WikiLink(link)]
                if bang.value == '!' && link.target == "x.png" && !link.embed),
            "{inlines:?}"
        );
    }

    #[test]
    fn a_bang_before_brackets_that_form_no_wikilink_stays_text() {
        let inlines = paragraph("![[x]");
        assert!(
            !inlines
                .iter()
                .any(|inline| matches!(inline, Inline::WikiLink(_))),
            "{inlines:?}"
        );
    }

    #[test]
    fn an_embed_wins_over_an_image_with_a_destination() {
        let inlines = paragraph("![[a]](u)");
        assert!(
            matches!(inlines.as_slice(), [Inline::WikiLink(link), Inline::Text(rest)]
                if link.embed && link.target == "a" && rest.value == "(u)"),
            "{inlines:?}"
        );
    }
}

mod autolinks_as_links {
    //! Literal and angle-bracket autolinks are `Autolink` nodes holding the
    //! URL as written, from which their destination derives.

    use markdown_syntax::prelude::*;

    /// The destination and text of the paragraph's one link or autolink.
    fn only_link(source: &str) -> (String, String) {
        let document = parse(source).document;
        let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
            panic!("{source:?}: {document:?}");
        };
        let links: Vec<_> = paragraph
            .children
            .iter()
            .filter_map(|inline| match inline {
                Inline::Link(link) => {
                    assert!(link.title.is_none());
                    assert_eq!(link.destination_kind, LinkDestinationKind::Bare);
                    let [Inline::Text(text)] = link.children.as_slice() else {
                        panic!("{source:?}: {link:?}");
                    };
                    Some((link.destination.clone(), text.value.clone()))
                }
                Inline::Autolink(autolink) => Some((
                    autolink
                        .destination()
                        .expect("a parsed autolink has a destination"),
                    autolink.text.clone(),
                )),
                _ => None,
            })
            .collect();
        let [link] = links.as_slice() else {
            panic!("{source:?}: {paragraph:?}");
        };
        link.clone()
    }

    #[test]
    fn a_bare_url_is_a_link_whose_text_is_the_url() {
        assert_eq!(
            only_link("see https://example.com"),
            ("https://example.com".into(), "https://example.com".into())
        );
        assert_eq!(
            only_link("www.example.com"),
            ("http://www.example.com".into(), "www.example.com".into())
        );
    }

    #[test]
    fn an_angle_bracket_autolink_is_a_link_whose_text_is_the_uri() {
        assert_eq!(
            only_link("<http://a\u{a0}b>"),
            ("http://a\u{a0}b".into(), "http://a\u{a0}b".into())
        );
        assert_eq!(
            only_link("<a@b.c>"),
            ("mailto:a@b.c".into(), "a@b.c".into())
        );
    }

    #[test]
    fn an_autolink_in_link_text_is_text() {
        assert_eq!(
            only_link("[http://a.b](u)"),
            ("u".into(), "http://a.b".into())
        );
    }
}

mod literal_autolink_boundaries {
    //! A literal autolink ends before Unicode whitespace, `<`, a non-ASCII
    //! char in CommonMark's Unicode punctuation set, and, with wikilinks
    //! enabled, `[[`; every boundary check reads whitespace as Unicode
    //! whitespace, on char boundaries.

    use markdown_syntax::prelude::*;

    /// The paragraph's inlines, each as its kind and its text or target.
    fn shape(source: &str) -> Vec<String> {
        let document = parse(source).document;
        let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
            panic!("{source:?}: {document:?}");
        };
        paragraph
            .children
            .iter()
            .map(|inline| match inline {
                Inline::Text(text) => format!("text {}", text.value),
                Inline::Autolink(autolink) => {
                    format!("link {}", autolink.destination().unwrap_or_default())
                }
                Inline::WikiLink(wiki) => format!("wiki {}", wiki.target),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_literal_autolink_ends_where_the_spec_says() {
        for (source, expected) in [
            (
                "见 https://example.com/page，然后 [[笔记]]",
                &[
                    "text 见 ",
                    "link https://example.com/page",
                    "text ，然后 ",
                    "wiki 笔记",
                ][..],
            ),
            (
                "www.example.com。下一句",
                &["link http://www.example.com", "text 。下一句"],
            ),
            (
                "see https://example.com/a[[b]] end",
                &[
                    "text see ",
                    "link https://example.com/a",
                    "wiki b",
                    "text  end",
                ],
            ),
            (
                "https://example.com/page#section、[[笔记#小节]]、",
                &[
                    "link https://example.com/page#section",
                    "text 、",
                    "wiki 笔记#小节",
                    "text 、",
                ],
            ),
            (
                // Only `http(s)://`, `www.`, and email literals link.
                "见 smb://host/share，然后",
                &["text 见 smb://host/share，然后"],
            ),
        ] {
            assert_eq!(shape(source), expected, "{source:?}");
        }
        assert_eq!(
            shape("https://zh.wikipedia.org/wiki/中文 x"),
            ["link https://zh.wikipedia.org/wiki/中文", "text  x"]
        );
        assert_eq!(
            shape("see https://example.com"),
            ["text see ", "link https://example.com"]
        );
    }

    #[test]
    fn a_no_break_space_before_an_email_like_run_is_text() {
        assert_eq!(shape("\u{a0}e+@"), ["text \u{a0}e+@"]);
    }

    /// A small deterministic xorshift generator, so failures reproduce.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, bound: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % bound as u64) as usize
        }
    }

    #[test]
    fn generated_whitespace_and_autolink_pieces_never_panic() {
        const PIECES: &[&str] = &[
            " ", "\t", "\n", "\u{a0}", "\u{85}", "\u{1680}", "\u{2000}", "\u{2007}", "\u{200a}",
            "\u{2028}", "\u{2029}", "\u{202f}", "\u{205f}", "\u{3000}", "\u{b}", "\u{c}", "www.",
            "://", "http", "https://", "mailto:", "@", ".", "+", "_", "-", "a", "b", "x", "，",
            "。", "、", "：", "[[", "]]", "<", ">", "(", ")",
        ];
        // Recorded seed of this generator; the four rounds keep the inputs
        // of the four former dialects.
        let mut rng = Rng(0x0a17_0115);
        for _ in 0..4 {
            for _ in 0..4_000 {
                let count = 1 + rng.below(10);
                let input: String = (0..count)
                    .map(|_| PIECES[rng.below(PIECES.len())])
                    .collect();
                let document = parse(&input).document;
                let _ = document.to_markdown();
            }
        }
    }
}

mod gemoji_shortcodes {
    //! A shortcode names an entry of the pinned gemoji table, with no letter
    //! or digit as the source char directly outside either colon.

    use markdown_syntax::prelude::*;

    fn inlines(source: &str) -> Vec<Inline> {
        let document = parse(source).document;
        let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
            panic!("{source:?}: {document:?}");
        };
        paragraph.children.clone()
    }

    fn shortcodes(source: &str) -> Vec<String> {
        inlines(source)
            .iter()
            .filter_map(|inline| match inline {
                Inline::Shortcode(node) => Some(node.name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_name_followed_by_a_colon_opens_no_text_directive() {
        // As micromark reads it: the name of a text directive cannot run
        // into a colon, so an unknown or blocked `:word:` stays text.
        for source in ["a :not_an_emoji_name: b", "x :smile:b", ":smile:中"] {
            let inlines = inlines(source);
            assert!(
                matches!(inlines.as_slice(), [Inline::Text(text)] if text.value == source),
                "{source:?}: {inlines:?}"
            );
        }
    }

    #[test]
    fn a_gemoji_name_between_colons_is_a_shortcode() {
        assert!(
            matches!(&inlines("a :tada: b")[1], Inline::Shortcode(node) if node.name == "tada")
        );
        assert!(
            matches!(&inlines("score :100: today")[1], Inline::Shortcode(node) if node.name == "100")
        );
    }

    #[test]
    fn a_letter_or_digit_beside_a_colon_or_an_unknown_name_is_no_shortcode() {
        for source in [
            "meet at 10:30:45 today",
            "时间:10:30",
            "a:smile:b",
            "a :not_an_emoji_name: b",
        ] {
            assert_eq!(shortcodes(source), Vec::<String>::new(), "{source:?}");
        }
    }

    #[test]
    fn a_character_reference_before_a_shortcode_leaves_it_a_shortcode() {
        let inlines = inlines("&#97;:smile:");
        assert!(
            matches!(
                inlines.as_slice(),
                [Inline::CharacterReference(reference), Inline::Shortcode(shortcode)]
                    if reference.value().as_deref() == Some("a") && shortcode.name == "smile"
            ),
            "{inlines:?}"
        );
    }

    #[test]
    fn a_shortcode_gives_its_glyph() {
        let inlines = inlines(":tada:");
        let [Inline::Shortcode(tada)] = inlines.as_slice() else {
            panic!("{inlines:?}");
        };
        assert_eq!(tada.glyph(), Some("\u{1F389}"));
        let unknown = Shortcode {
            meta: NodeMeta::default(),
            name: "not_an_emoji_name".into(),
        };
        assert_eq!(unknown.glyph(), None);
    }
}

mod underscore_beside_tilde {
    //! A `_` run gets no strikethrough bonus beside a `~`.

    use markdown_syntax::prelude::*;

    #[test]
    fn underscores_around_a_tilde_stay_text() {
        let document = parse("d_~_").document;
        let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
            panic!("{document:?}");
        };
        assert!(
            matches!(paragraph.children.as_slice(), [Inline::Text(text)] if text.value == "d_~_"),
            "{paragraph:?}"
        );
    }
}

mod autolinks_inside_link_text {
    //! An autolink keeps no open bracket from forming a link around it; one
    //! in the label of a text directive inside link text is text, as in any
    //! link text.

    use markdown_syntax::prelude::*;

    #[test]
    fn an_autolink_in_a_directive_label_keeps_the_link_around_it() {
        for source in ["[:abbr[<http://a>]](u)", "[:abbr[https://a.com]](u)"] {
            let document = parse(source).document;
            let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
                panic!("{source:?}: {document:?}");
            };
            let [Inline::Link(link)] = paragraph.children.as_slice() else {
                panic!("{source:?}: {:?}", paragraph.children);
            };
            assert_eq!(link.destination, "u");
            let [Inline::TextDirective(directive)] = link.children.as_slice() else {
                panic!("{source:?}: {:?}", link.children);
            };
            assert!(
                matches!(directive.label.as_slice(), [Inline::Text(_)]),
                "{source:?}: {:?}",
                directive.label
            );
        }
    }

    #[test]
    fn a_backslash_before_punctuation_ends_a_literal_autolink() {
        // cmark-gfm keeps `\*x` inside the URL. Here it ends the URL, so the
        // escapes the serializer writes after an autolink read back as text.
        let document = parse("www.a.com\\*x").document;
        let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
            panic!("{document:?}");
        };
        assert!(
            matches!(paragraph.children.as_slice(), [Inline::Autolink(link), Inline::Escape(escape), Inline::Text(_)]
                if link.destination().as_deref() == Some("http://www.a.com") && escape.value == '*'),
            "{:?}",
            paragraph.children
        );
    }
}

mod wikilinks_as_written {
    //! A wiki link's target and label keep their escapes and character
    //! references as written, and are written back as they are.

    use markdown_syntax::prelude::*;

    fn wikilink(source: &str) -> WikiLink {
        let document = parse(source).document;
        let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
            panic!("{source:?}: {document:?}");
        };
        let [Inline::WikiLink(link)] = paragraph.children.as_slice() else {
            panic!("{source:?}: {:?}", paragraph.children);
        };
        link.clone()
    }

    #[test]
    fn escapes_and_references_stay_as_written() {
        let link = wikilink("[[a\\$b]]");
        assert_eq!(
            (link.target.as_str(), link.label.as_str()),
            ("a\\$b", "a\\$b")
        );
        let link = wikilink("[[a &amp; b|x\\]y]]");
        assert_eq!(
            (link.target.as_str(), link.label.as_str()),
            ("a &amp; b", "x\\]y")
        );
        for source in ["[[a\\$b]]", "[[a &amp; b|x\\]y]]", "d$![[\\$]]"] {
            assert_eq!(
                parse(source).document.to_markdown().unwrap(),
                format!("{source}\n")
            );
        }
    }

    #[test]
    fn decoding_reads_escapes_and_references() {
        let link = wikilink("[[a\\|b &amp; c|x &#65; \\y]]");
        assert_eq!(
            (link.target.as_str(), link.label.as_str()),
            ("a\\|b &amp; c", "x &#65; \\y")
        );
        assert_eq!(
            (link.decoded_target(), link.decoded_label()),
            ("a|b & c".to_string(), "x A \\y".to_string())
        );
        // An unlabeled link's label is its target.
        let link = wikilink("[[a\\$b]]");
        assert_eq!(link.label, link.target);
        assert_eq!(link.decoded_label(), "a$b");
    }

    #[test]
    fn a_decoded_target_compares_with_a_link_destination() {
        let document = parse("[[a&amp;b]] [x](a&amp;b)").document;
        let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
            panic!("{document:?}");
        };
        let [Inline::WikiLink(wiki), Inline::Text(_), Inline::Link(link)] =
            paragraph.children.as_slice()
        else {
            panic!("{:?}", paragraph.children);
        };
        assert_eq!(wiki.target, "a&amp;b");
        assert_eq!(wiki.decoded_target(), link.destination);
        assert_eq!(link.destination, "a&b");
    }
}

mod literal_autolink_trailing_references {
    //! A trailing `;` after a numeric character reference is trimmed alone,
    //! as cmark-gfm trims it, whether the reference is decimal or hex.

    use markdown_syntax::prelude::*;

    #[test]
    fn a_numeric_reference_keeps_its_digits_in_the_url() {
        for (source, destination) in [
            ("www.a.b&#x41;", "http://www.a.b&#x41"),
            ("www.a.b&#35;", "http://www.a.b&#35"),
            ("http://a.b/c&#x41;", "http://a.b/c&#x41"),
        ] {
            let document = parse(source).document;
            let [Block::Paragraph(paragraph)] = document.children.as_slice() else {
                panic!("{source:?}: {document:?}");
            };
            assert!(
                matches!(
                    paragraph.children.as_slice(),
                    [Inline::Autolink(link), Inline::Text(text)]
                        if link.destination().as_deref() == Some(destination) && text.value == ";"
                ),
                "{source:?}: {:?}",
                paragraph.children
            );
        }
    }
}
