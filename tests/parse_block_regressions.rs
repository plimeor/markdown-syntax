//! Block-parsing regression coverage: setext/list/code-block defects from the
//! review pass plus the broader parser regressions (reference definitions,
//! HTML blocks, character references, link resources, source spans).
//!
//! Each former regression file is preserved verbatim inside its own `mod` so
//! that helper functions and test names cannot collide across the merged
//! sources.

mod review_block {
    //! Regression tests for the block-level parser defects fixed in the 2026-06-18
    //! review. Each asserts the CommonMark-correct
    //! AST shape against the live parser.

    use markdown_syntax::{
        parse_with_options, Block, CodeBlockKind, HeadingKind, Inline, ListDelimiter, SyntaxOptions,
    };

    /// B1: a multi-line paragraph followed by a setext underline is a setext
    /// heading whose content spans every paragraph line, not one flat paragraph.
    #[test]
    fn setext_heading_absorbs_multiline_paragraph() {
        let output = parse_with_options("Foo\nbar\n===\n", &SyntaxOptions::commonmark())
            .expect("valid parse");

        let [Block::Heading(heading)] = output.document.children.as_slice() else {
            panic!(
                "expected a single setext heading: {:?}",
                output.document.children
            );
        };
        assert_eq!(heading.depth, 1);
        assert_eq!(heading.kind, HeadingKind::Setext);
        // The two paragraph lines are joined with a soft line break before the
        // underline applies, so the heading text is `Foo` then `bar`.
        assert!(matches!(
            heading.children.as_slice(),
            [Inline::Text(first), Inline::SoftBreak(_), Inline::Text(second)]
                if first.value == "Foo" && second.value == "bar"
        ));
    }

    /// B1 guard: a single-line setext heading still parses (the multi-line scan must
    /// not break the original one-line case).
    #[test]
    fn setext_heading_single_line_still_parses() {
        let output =
            parse_with_options("Foo\n---\n", &SyntaxOptions::commonmark()).expect("valid parse");
        let [Block::Heading(heading)] = output.document.children.as_slice() else {
            panic!(
                "expected a single setext heading: {:?}",
                output.document.children
            );
        };
        assert_eq!(heading.depth, 2);
        assert_eq!(heading.kind, HeadingKind::Setext);
        assert!(matches!(heading.children.as_slice(), [Inline::Text(text)] if text.value == "Foo"));
    }

    /// B1 guard: a block start between the paragraph lines and the underline stops
    /// the setext heading from forming.
    #[test]
    fn setext_heading_rejected_when_continuation_is_block_start() {
        let output = parse_with_options("Foo\n# heading\n===\n", &SyntaxOptions::commonmark())
            .expect("valid parse");
        assert!(
            !output.document.children.iter().any(
                |block| matches!(block, Block::Heading(heading) if heading.kind == HeadingKind::Setext)
            ),
            "no setext heading should form across a block start: {:?}",
            output.document.children
        );
    }

    /// B2: bullet markers at 0/1/2/3 leading spaces form ONE list with four sibling
    /// items, not three separate lists or a single item.
    #[test]
    fn bullets_with_too_few_spaces_are_siblings_not_sublists() {
        let output = parse_with_options(
            "- foo\n - bar\n  - baz\n   - boo\n",
            &SyntaxOptions::commonmark(),
        )
        .expect("valid parse");

        let [Block::List(list)] = output.document.children.as_slice() else {
            panic!("expected one bullet list: {:?}", output.document.children);
        };
        assert!(!list.ordered);
        assert_eq!(list.delimiter, ListDelimiter::Dash);
        assert_eq!(
            list.children.len(),
            4,
            "expected four sibling items: {list:?}"
        );
        // No item should contain a nested list.
        for item in &list.children {
            assert!(
                !item
                    .children
                    .iter()
                    .any(|block| matches!(block, Block::List(_))),
                "items must be flat siblings, not nested: {item:?}"
            );
        }
    }

    /// B2 guard: markers indented to the parent's content column still nest as
    /// sublists (the indent threshold must keep real nesting working).
    #[test]
    fn bullets_with_enough_spaces_still_nest() {
        let output = parse_with_options("- foo\n  - bar\n", &SyntaxOptions::commonmark())
            .expect("valid parse");

        let [Block::List(list)] = output.document.children.as_slice() else {
            panic!("expected one bullet list: {:?}", output.document.children);
        };
        assert_eq!(
            list.children.len(),
            1,
            "outer list should have one item: {list:?}"
        );
        let nested = list.children[0]
            .children
            .iter()
            .any(|block| matches!(block, Block::List(_)));
        assert!(
            nested,
            "`  - bar` must nest under `- foo`: {:?}",
            list.children[0]
        );
    }

    /// B2 guard: a delimiter change still splits one list into two.
    #[test]
    fn delimiter_change_still_splits_lists() {
        let output =
            parse_with_options("- a\n+ b\n", &SyntaxOptions::commonmark()).expect("valid parse");
        let lists = output
            .document
            .children
            .iter()
            .filter(|block| matches!(block, Block::List(_)))
            .count();
        assert_eq!(
            lists, 2,
            "different bullets are different lists: {:?}",
            output.document.children
        );
    }

    /// B3: an empty list item does not interrupt a paragraph; `foo\n*` is a single
    /// paragraph, not a paragraph plus an empty list.
    #[test]
    fn empty_list_item_does_not_interrupt_paragraph() {
        let output =
            parse_with_options("foo\n*\n", &SyntaxOptions::commonmark()).expect("valid parse");
        let [Block::Paragraph(paragraph)] = output.document.children.as_slice() else {
            panic!(
                "expected a single paragraph: {:?}",
                output.document.children
            );
        };
        assert!(matches!(
            paragraph.children.as_slice(),
            [Inline::Text(first), Inline::SoftBreak(_), Inline::Text(second)]
                if first.value == "foo" && second.value == "*"
        ));
    }

    /// B3 guard: a bare `*` at block start (not interrupting) is still an empty
    /// list.
    #[test]
    fn empty_list_at_block_start_still_parses() {
        let output = parse_with_options("*\n", &SyntaxOptions::commonmark()).expect("valid parse");
        let [Block::List(list)] = output.document.children.as_slice() else {
            panic!("expected an empty list: {:?}", output.document.children);
        };
        assert_eq!(list.children.len(), 1);
        assert!(list.children[0].children.is_empty(), "empty item: {list:?}");
    }

    /// B3 guard: a non-empty list item still interrupts a paragraph.
    #[test]
    fn non_empty_list_item_still_interrupts_paragraph() {
        let output =
            parse_with_options("foo\n- bar\n", &SyntaxOptions::commonmark()).expect("valid parse");
        assert!(matches!(
            output.document.children.as_slice(),
            [Block::Paragraph(_), Block::List(_)]
        ));
    }

    /// B4: a fenced code block indented N spaces strips up to N leading spaces from
    /// each content line.
    #[test]
    fn fenced_code_strips_opening_indent_from_content() {
        let output = parse_with_options(" ```\n aaa\naaa\n```\n", &SyntaxOptions::commonmark())
            .expect("valid parse");
        let [Block::CodeBlock(code)] = output.document.children.as_slice() else {
            panic!(
                "expected one fenced code block: {:?}",
                output.document.children
            );
        };
        assert!(matches!(code.kind, CodeBlockKind::Fenced { .. }));
        assert_eq!(code.value, "aaa\naaa\n");
    }

    /// B4 guard: only up to N spaces are removed; deeper indentation is preserved.
    #[test]
    fn fenced_code_keeps_indent_beyond_opening() {
        let output = parse_with_options(
            "   ```\n   aaa\n    aaa\n  aaa\n   ```\n",
            &SyntaxOptions::commonmark(),
        )
        .expect("valid parse");
        let [Block::CodeBlock(code)] = output.document.children.as_slice() else {
            panic!(
                "expected one fenced code block: {:?}",
                output.document.children
            );
        };
        // Three-space opening fence: `   aaa` loses 3, `    aaa` keeps 1, `  aaa`
        // loses only the two it has.
        assert_eq!(code.value, "aaa\n aaa\naaa\n");
    }

    /// B5: leading/trailing blank lines are not part of an indented code block;
    /// interior blanks and the final content line ending stay.
    #[test]
    fn indented_code_trims_trailing_blank_lines() {
        let output = parse_with_options("    foo\n    \n", &SyntaxOptions::commonmark())
            .expect("valid parse");
        let [Block::CodeBlock(code)] = output.document.children.as_slice() else {
            panic!(
                "expected one indented code block: {:?}",
                output.document.children
            );
        };
        assert_eq!(code.kind, CodeBlockKind::Indented);
        assert_eq!(code.value, "foo\n");
    }

    /// B5 guard: interior blank lines are preserved.
    #[test]
    fn indented_code_keeps_interior_blank_lines() {
        let output = parse_with_options("    foo\n\n    bar\n", &SyntaxOptions::commonmark())
            .expect("valid parse");
        let [Block::CodeBlock(code)] = output.document.children.as_slice() else {
            panic!(
                "expected one indented code block: {:?}",
                output.document.children
            );
        };
        assert_eq!(code.value, "foo\n\nbar\n");
    }
}

mod parser {
    use markdown_syntax::{
        parse_strict_with_options, parse_with_options, Block, Constructs, DiagnosticCode, Inline,
        LinkDestinationKind, LinkTitleKind, ParseOptions, ParseStrictError, ReferenceKind, Span,
        SyntaxOptions,
    };

    #[test]
    fn reference_definitions_are_collected_from_real_blocks_only() {
        let output = parse_with_options(
            "```\n[foo]: /url\n```\n\n[foo]\n",
            &SyntaxOptions::commonmark(),
        )
        .expect("valid CommonMark parse");

        assert!(matches!(
            output.document.children.first(),
            Some(Block::CodeBlock(_))
        ));
        let Some(Block::Paragraph(paragraph)) = output.document.children.get(1) else {
            panic!("expected paragraph after fenced code");
        };
        assert!(
            matches!(paragraph.children.as_slice(), [Inline::Text(text)] if text.value == "[foo]")
        );
    }

    #[test]
    fn reference_definitions_support_multiline_destination() {
        let output = parse_with_options("[foo]:\n /url\n\n[foo]\n", &SyntaxOptions::commonmark())
            .expect("valid CommonMark parse");

        let Some(Block::Definition(definition)) = output.document.children.first() else {
            panic!("expected link reference definition");
        };
        assert_eq!(definition.identifier, "foo");
        assert_eq!(definition.destination, "/url");

        let Some(Block::Paragraph(paragraph)) = output.document.children.get(1) else {
            panic!("expected reference paragraph");
        };
        assert!(matches!(
            paragraph.children.as_slice(),
            [Inline::LinkReference(reference)] if reference.identifier == "foo"
        ));
    }

    #[test]
    fn ordered_list_markers_follow_commonmark_interrupt_rules() {
        let interrupted =
            parse_with_options("a\n2. b\n", &SyntaxOptions::commonmark()).expect("valid parse");
        assert_eq!(interrupted.document.children.len(), 1);
        assert!(matches!(
            interrupted.document.children.as_slice(),
            [Block::Paragraph(_)]
        ));

        let too_many_digits =
            parse_with_options("1234567890. not ok\n", &SyntaxOptions::commonmark())
                .expect("valid parse");
        assert!(matches!(
            too_many_digits.document.children.as_slice(),
            [Block::Paragraph(_)]
        ));
    }

    #[test]
    fn html_block_starts_interrupt_paragraphs_when_commonmark_allows() {
        let output = parse_with_options("foo\n<div>\nbar\n", &SyntaxOptions::commonmark())
            .expect("valid parse");

        assert!(matches!(
            output.document.children.as_slice(),
            [Block::Paragraph(_), Block::HtmlBlock(_)]
        ));
    }

    #[test]
    fn raw_html_block_close_requires_matching_raw_tag_name() {
        let source = "<script>\nnot closed by </scripture>\nstill raw\n</script>\n";
        let output = parse_with_options(source, &SyntaxOptions::commonmark()).expect("valid parse");

        let [Block::HtmlBlock(block)] = output.document.children.as_slice() else {
            panic!("expected one raw HTML block");
        };
        assert_eq!(
            block.value,
            "<script>\nnot closed by </scripture>\nstill raw\n</script>"
        );
    }

    #[test]
    fn reference_labels_allow_999_characters() {
        let label = "x".repeat(999);
        let source = format!("[{label}]: /url\n\n[full][{label}]\n[{label}][]\n[{label}]\n");
        let output =
            parse_with_options(&source, &SyntaxOptions::commonmark()).expect("valid parse");

        let Some(Block::Definition(definition)) = output.document.children.first() else {
            panic!("expected max-length definition");
        };
        assert_eq!(&definition.label, &label);

        let Some(Block::Paragraph(paragraph)) = output.document.children.get(1) else {
            panic!("expected reference paragraph");
        };
        let references = paragraph
            .children
            .iter()
            .filter_map(|inline| match inline {
                Inline::LinkReference(reference) => Some(reference),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(references.len(), 3);
        assert_eq!(references[0].kind, ReferenceKind::Full);
        assert_eq!(references[1].kind, ReferenceKind::Collapsed);
        assert_eq!(references[2].kind, ReferenceKind::Shortcut);
        assert!(references
            .iter()
            .all(|reference| reference.label == label && reference.identifier == label));
    }

    #[test]
    fn reference_labels_reject_1000_character_labels() {
        let overlong = format!("x{}", " ".repeat(999));
        let definition_source = format!("[{overlong}]: /url\n\n[x]\n");
        let definition_output =
            parse_with_options(&definition_source, &SyntaxOptions::commonmark())
                .expect("valid parse");

        assert!(definition_output
            .document
            .children
            .iter()
            .all(|block| !matches!(block, Block::Definition(_))));
        assert!(definition_output.document.children.iter().all(|block| {
            let Block::Paragraph(paragraph) = block else {
                return true;
            };
            paragraph
                .children
                .iter()
                .all(|inline| !matches!(inline, Inline::LinkReference(_)))
        }));

        let reference_source =
            format!("[x]: /url\n\n[full][{overlong}]\n[{overlong}][]\n[{overlong}]\n");
        let reference_output = parse_with_options(&reference_source, &SyntaxOptions::commonmark())
            .expect("valid parse");

        assert!(matches!(
            reference_output.document.children.first(),
            Some(Block::Definition(_))
        ));
        let Some(Block::Paragraph(paragraph)) = reference_output.document.children.get(1) else {
            panic!("expected fallback paragraph");
        };
        assert!(paragraph
            .children
            .iter()
            .all(|inline| !matches!(inline, Inline::LinkReference(_))));
    }

    #[test]
    fn triple_asterisk_parses_as_nested_emphasis_and_strong() {
        let output = parse_with_options("***foo***\n", &SyntaxOptions::commonmark())
            .expect("valid CommonMark parse");
        let Some(Block::Paragraph(paragraph)) = output.document.children.first() else {
            panic!("expected paragraph");
        };

        let [Inline::Emphasis(emphasis)] = paragraph.children.as_slice() else {
            panic!("expected outer emphasis");
        };
        let [Inline::Strong(strong)] = emphasis.children.as_slice() else {
            panic!("expected inner strong");
        };
        assert!(matches!(strong.children.as_slice(), [Inline::Text(text)] if text.value == "foo"));
    }

    #[test]
    fn strict_mdx_reports_unclosed_jsx_blocks() {
        let err = parse_strict_with_options("<A>\n", &SyntaxOptions::mdx()).unwrap_err();

        let ParseStrictError::Diagnostic(diagnostic) = err else {
            panic!("expected strict parse diagnostic");
        };
        assert_eq!(diagnostic.code, DiagnosticCode::InvalidMdx);
    }

    #[test]
    fn directive_openers_scan_escaped_labels_and_quoted_attributes() {
        let mut constructs = Constructs::commonmark();
        constructs.directive_text = true;
        let options = SyntaxOptions::custom(constructs, ParseOptions::default());
        let output = parse_with_options(":note[has \\] bracket]{title=\"x } y\"}\n", &options)
            .expect("valid directive parse");

        let Some(Block::Paragraph(paragraph)) = output.document.children.first() else {
            panic!("expected paragraph");
        };
        let [Inline::TextDirective(directive)] = paragraph.children.as_slice() else {
            panic!("expected text directive");
        };
        assert!(
            matches!(directive.label.as_slice(), [Inline::Text(text)] if text.value == "has ] bracket")
        );
        assert_eq!(directive.attributes.len(), 1);
        assert_eq!(directive.attributes[0].name, "title");
        assert_eq!(directive.attributes[0].value.as_deref(), Some("x } y"));
    }

    #[test]
    fn named_character_references_cover_common_html5_entities() {
        let source =
            "&semi; &trade; &NotEqualTilde; &CounterClockwiseContourIntegral; &acE; &nGg; &fjlig; &AMP;\n";
        let options = SyntaxOptions::custom(
            Constructs::commonmark(),
            ParseOptions {
                preserve_character_references: true,
                ..ParseOptions::default()
            },
        );
        let output = parse_with_options(source, &options).expect("valid parse");

        let Some(Block::Paragraph(paragraph)) = output.document.children.first() else {
            panic!("expected paragraph");
        };
        let references = paragraph
            .children
            .iter()
            .filter_map(|inline| match inline {
                Inline::CharacterReference(reference) => {
                    Some((reference.reference.as_str(), reference.value.as_str()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            references,
            vec![
                ("&semi;", ";"),
                ("&trade;", "\u{2122}"),
                ("&NotEqualTilde;", "\u{2242}\u{0338}"),
                ("&CounterClockwiseContourIntegral;", "\u{2233}"),
                ("&acE;", "\u{223E}\u{0333}"),
                ("&nGg;", "\u{22D9}\u{0338}"),
                ("&fjlig;", "fj"),
                ("&AMP;", "&"),
            ]
        );

        let resolved =
            parse_with_options(source, &SyntaxOptions::commonmark()).expect("valid resolved parse");
        let Some(Block::Paragraph(paragraph)) = resolved.document.children.first() else {
            panic!("expected resolved paragraph");
        };
        assert!(matches!(
            paragraph.children.as_slice(),
            [Inline::Text(text)] if text.value == ";\u{20}\u{2122}\u{20}\u{2242}\u{0338}\u{20}\u{2233}\u{20}\u{223E}\u{0333}\u{20}\u{22D9}\u{0338}\u{20}fj\u{20}&"
        ));
    }

    #[test]
    fn asterisk_runs_open_emphasis_only_as_whole_left_flanking_runs() {
        // CommonMark example 397: a `**` run followed by whitespace is not
        // left-flanking, so no single `*` may be peeled off to open emphasis.
        let space = parse_with_options("** foo bar**\n", &SyntaxOptions::commonmark())
            .expect("valid parse")
            .document;
        assert!(
            matches!(
                space.children.as_slice(),
                [Block::Paragraph(paragraph)]
                    if matches!(paragraph.children.as_slice(), [Inline::Text(text)] if text.value == "** foo bar**")
            ),
            "`** foo bar**` must stay literal text: {space:?}"
        );

        // CommonMark example 399: an interior `**` run is not left-flanking next to
        // punctuation, and the second asterisk of a run can never open on its own.
        let punctuation = parse_with_options("a**\"foo\"**\n", &SyntaxOptions::commonmark())
            .expect("valid parse")
            .document;
        assert!(
            matches!(
                punctuation.children.as_slice(),
                [Block::Paragraph(paragraph)]
                    if matches!(paragraph.children.as_slice(), [Inline::Text(text)] if text.value == "a**\"foo\"**")
            ),
            "`a**\"foo\"**` must stay literal text: {punctuation:?}"
        );

        // Guard against over-restriction: a genuinely left-flanking `**` run still
        // opens strong emphasis.
        let strong = parse_with_options("**foo bar**\n", &SyntaxOptions::commonmark())
            .expect("valid parse")
            .document;
        assert!(
            matches!(
                strong.children.as_slice(),
                [Block::Paragraph(paragraph)]
                    if matches!(
                        paragraph.children.as_slice(),
                        [Inline::Strong(node)]
                            if matches!(node.children.as_slice(), [Inline::Text(text)] if text.value == "foo bar")
                    )
            ),
            "`**foo bar**` must still be strong: {strong:?}"
        );
    }

    #[test]
    fn numeric_character_references_decode_with_commonmark_replacement() {
        // CommonMark reference behavior: only U+0000, surrogates, and
        // out-of-range codepoints decode to U+FFFD. C0/C1 controls, DEL, and Unicode
        // noncharacters keep their literal scalar (no HTML5 Windows-1252 remapping),
        // which is also what lets the serializer round-trip `&#xNN;`-escaped control
        // characters.
        let source =
            "&#x41; &#9; &#10; &#0; &#1; &#127; &#128; &#xFDD0; &#xFFFE; &#xD800; &#x110000;\n";
        let options = SyntaxOptions::custom(
            Constructs::commonmark(),
            ParseOptions {
                preserve_character_references: true,
                ..ParseOptions::default()
            },
        );
        let output = parse_with_options(source, &options).expect("valid parse");

        let Some(Block::Paragraph(paragraph)) = output.document.children.first() else {
            panic!("expected paragraph");
        };
        let references = paragraph
            .children
            .iter()
            .filter_map(|inline| match inline {
                Inline::CharacterReference(reference) => {
                    Some((reference.reference.as_str(), reference.value.as_str()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            references,
            vec![
                ("&#x41;", "A"),
                ("&#9;", "\t"),
                ("&#10;", "\n"),
                ("&#0;", "\u{FFFD}"),
                ("&#1;", "\u{1}"),
                ("&#127;", "\u{7F}"),
                ("&#128;", "\u{80}"),
                ("&#xFDD0;", "\u{FDD0}"),
                ("&#xFFFE;", "\u{FFFE}"),
                ("&#xD800;", "\u{FFFD}"),
                ("&#x110000;", "\u{FFFD}"),
            ]
        );
    }

    #[test]
    fn link_resources_preserve_destination_and_title_kinds_in_ast() {
        let output = parse_with_options(
            "[foo]: <my url> 'title'\n\n[angle](<foo bar> 'single') [paren](url (paren title)) [empty]( \"title\")\n",
            &SyntaxOptions::commonmark(),
        )
        .expect("valid CommonMark parse");

        let Some(Block::Definition(definition)) = output.document.children.first() else {
            panic!("expected definition");
        };
        assert_eq!(definition.destination, "my url");
        assert_eq!(definition.destination_kind, LinkDestinationKind::Angle);
        assert_eq!(definition.title.as_deref(), Some("title"));
        assert_eq!(definition.title_kind, Some(LinkTitleKind::SingleQuote));

        let Some(Block::Paragraph(paragraph)) = output.document.children.get(1) else {
            panic!("expected paragraph");
        };
        assert!(matches!(
            paragraph.children.as_slice(),
            [
                Inline::Link(angle),
                Inline::Text(_),
                Inline::Link(paren),
                Inline::Text(_),
                Inline::Link(empty)
            ] if angle.destination == "foo bar"
                && angle.destination_kind == LinkDestinationKind::Angle
                && angle.title.as_deref() == Some("single")
                && angle.title_kind == Some(LinkTitleKind::SingleQuote)
                && paren.destination == "url"
                && paren.destination_kind == LinkDestinationKind::Bare
                && paren.title.as_deref() == Some("paren title")
                && paren.title_kind == Some(LinkTitleKind::Paren)
                && empty.destination.is_empty()
                && empty.destination_kind == LinkDestinationKind::Omitted
                && empty.title.as_deref() == Some("title")
                && empty.title_kind == Some(LinkTitleKind::DoubleQuote)
        ));
    }

    #[test]
    fn localized_source_spans_track_trimmed_markers() {
        let heading =
            parse_with_options("# foo #\n", &SyntaxOptions::commonmark()).expect("valid heading");
        let Some(Block::Heading(node)) = heading.document.children.first() else {
            panic!("expected heading");
        };
        assert!(matches!(
            node.children.as_slice(),
            [Inline::Text(text)] if text.meta.span == Some(Span::new(2, 5))
        ));

        let blockquote =
            parse_with_options("> **a**\n", &SyntaxOptions::commonmark()).expect("valid quote");
        let Some(Block::BlockQuote(quote)) = blockquote.document.children.first() else {
            panic!("expected blockquote");
        };
        let Some(Block::Paragraph(paragraph)) = quote.children.first() else {
            panic!("expected quote paragraph");
        };
        assert!(matches!(
            paragraph.children.as_slice(),
            [Inline::Strong(strong)] if strong.meta.span == Some(Span::new(2, 7))
        ));

        let mut constructs = Constructs::commonmark();
        constructs.directive_text = true;
        let options = SyntaxOptions::custom(constructs, ParseOptions::default());
        let directive = parse_with_options(":note[*x*]\n", &options).expect("valid directive");
        let Some(Block::Paragraph(paragraph)) = directive.document.children.first() else {
            panic!("expected directive paragraph");
        };
        let [Inline::TextDirective(node)] = paragraph.children.as_slice() else {
            panic!("expected text directive");
        };
        assert!(matches!(
            node.label.as_slice(),
            [Inline::Emphasis(emphasis)] if emphasis.meta.span == Some(Span::new(6, 9))
        ));
    }
}
