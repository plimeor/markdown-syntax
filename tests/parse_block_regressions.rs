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
        Block, CodeBlockKind, HeadingKind, Inline, ListDelimiter, SyntaxOptions,
    };

    /// B1: a multi-line paragraph followed by a setext underline is a setext
    /// heading whose content spans every paragraph line, not one flat paragraph.
    #[test]
    fn setext_heading_absorbs_multiline_paragraph() {
        let output = SyntaxOptions::commonmark().parse("Foo\nbar\n===\n");

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
        let output = SyntaxOptions::commonmark().parse("Foo\n---\n");
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
        let output = SyntaxOptions::commonmark().parse("Foo\n# heading\n===\n");
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
        let output = SyntaxOptions::commonmark().parse("- foo\n - bar\n  - baz\n   - boo\n");

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
        let output = SyntaxOptions::commonmark().parse("- foo\n  - bar\n");

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
        let output = SyntaxOptions::commonmark().parse("- a\n+ b\n");
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
        let output = SyntaxOptions::commonmark().parse("foo\n*\n");
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
        let output = SyntaxOptions::commonmark().parse("*\n");
        let [Block::List(list)] = output.document.children.as_slice() else {
            panic!("expected an empty list: {:?}", output.document.children);
        };
        assert_eq!(list.children.len(), 1);
        assert!(list.children[0].children.is_empty(), "empty item: {list:?}");
    }

    /// B3 guard: a non-empty list item still interrupts a paragraph.
    #[test]
    fn non_empty_list_item_still_interrupts_paragraph() {
        let output = SyntaxOptions::commonmark().parse("foo\n- bar\n");
        assert!(matches!(
            output.document.children.as_slice(),
            [Block::Paragraph(_), Block::List(_)]
        ));
    }

    /// B4: a fenced code block indented N spaces strips up to N leading spaces from
    /// each content line.
    #[test]
    fn fenced_code_strips_opening_indent_from_content() {
        let output = SyntaxOptions::commonmark().parse(" ```\n aaa\naaa\n```\n");
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
        let output = SyntaxOptions::commonmark().parse("   ```\n   aaa\n    aaa\n  aaa\n   ```\n");
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
        let output = SyntaxOptions::commonmark().parse("    foo\n    \n");
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
        let output = SyntaxOptions::commonmark().parse("    foo\n\n    bar\n");
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
        Block, Constructs, DiagnosticCode, HtmlContainerContent, Inline, LinkDestinationKind,
        LinkTitleKind, ParseOptions, ParseStrictError, ReferenceKind, Span, SyntaxOptions,
    };

    #[test]
    fn reference_definitions_are_collected_from_real_blocks_only() {
        let output = SyntaxOptions::commonmark().parse("```\n[foo]: /url\n```\n\n[foo]\n");

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
        let output = SyntaxOptions::commonmark().parse("[foo]:\n /url\n\n[foo]\n");

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
        let interrupted = SyntaxOptions::commonmark().parse("a\n2. b\n");
        assert_eq!(interrupted.document.children.len(), 1);
        assert!(matches!(
            interrupted.document.children.as_slice(),
            [Block::Paragraph(_)]
        ));

        let too_many_digits = SyntaxOptions::commonmark().parse("1234567890. not ok\n");
        assert!(matches!(
            too_many_digits.document.children.as_slice(),
            [Block::Paragraph(_)]
        ));
    }

    #[test]
    fn html_block_starts_interrupt_paragraphs_when_commonmark_allows() {
        let output = SyntaxOptions::commonmark().parse("foo\n<div>\nbar\n");

        assert!(matches!(
            output.document.children.as_slice(),
            [Block::Paragraph(_), Block::HtmlBlock(_)]
        ));
    }

    #[test]
    fn raw_html_block_close_requires_matching_raw_tag_name() {
        let source = "<script>\nnot closed by </scripture>\nstill raw\n</script>\n";
        let output = SyntaxOptions::commonmark().parse(source);

        let [Block::HtmlBlock(block)] = output.document.children.as_slice() else {
            panic!("expected one raw HTML block");
        };
        assert_eq!(
            block.value,
            "<script>\nnot closed by </scripture>\nstill raw\n</script>"
        );
    }

    #[test]
    fn default_parses_details_summary_as_html_containers() {
        let source = "<details>\n<summary>Install</summary>\n\nRun `cargo test`.\n\n</details>\n";
        let output = SyntaxOptions::default().parse(source);

        let [Block::HtmlContainer(details)] = output.document.children.as_slice() else {
            panic!(
                "expected one details container: {:?}",
                output.document.children
            );
        };
        assert_eq!(details.opening.name, "details");
        let HtmlContainerContent::Blocks(children) = &details.content else {
            panic!("details should contain block content");
        };
        let [Block::HtmlContainer(summary), Block::Paragraph(_)] = children.as_slice() else {
            panic!("expected summary plus paragraph: {children:?}");
        };
        assert_eq!(summary.opening.name, "summary");
        assert!(matches!(
            &summary.content,
            HtmlContainerContent::Inlines(inlines)
                if matches!(inlines.as_slice(), [Inline::Text(text)] if text.value == "Install")
        ));
    }

    #[test]
    fn default_parses_compact_details_summary_line_as_html_container() {
        let source = "<details><summary>Compact</summary>\n\nbody\n\n</details>\n";
        let output = SyntaxOptions::default().parse(source);

        let [Block::HtmlContainer(details)] = output.document.children.as_slice() else {
            panic!(
                "expected one details container: {:?}",
                output.document.children
            );
        };
        let HtmlContainerContent::Blocks(children) = &details.content else {
            panic!("details should contain block content");
        };
        let [Block::HtmlContainer(summary), Block::Paragraph(_)] = children.as_slice() else {
            panic!("expected summary plus paragraph: {children:?}");
        };
        assert!(matches!(
            &summary.content,
            HtmlContainerContent::Inlines(inlines)
                if matches!(inlines.as_slice(), [Inline::Text(text)] if text.value == "Compact")
        ));
    }

    #[test]
    fn commonmark_keeps_details_as_raw_html_blocks() {
        let source = "<details>\n\nbody\n\n</details>\n";
        let output = SyntaxOptions::commonmark().parse(source);

        assert!(matches!(
            output.document.children.as_slice(),
            [
                Block::HtmlBlock(_),
                Block::Paragraph(_),
                Block::HtmlBlock(_)
            ]
        ));
    }

    #[test]
    fn html_container_construct_can_interrupt_without_html_block() {
        let mut constructs = Constructs::commonmark();
        constructs.html_block = false;
        constructs.html_inline = false;
        constructs.html_container = true;
        let output = SyntaxOptions {
            constructs,
            parse: Default::default(),
        }
        .parse("before\n<details>\n\nbody\n\n</details>\n");

        assert!(matches!(
            output.document.children.as_slice(),
            [Block::Paragraph(_), Block::HtmlContainer(_)]
        ));
    }

    #[test]
    fn unclosed_details_falls_back_to_raw_html_block() {
        let source = "<details>\n<summary>Open</summary>\n";
        let output = SyntaxOptions::default().parse(source);

        assert!(matches!(
            output.document.children.as_slice(),
            [Block::HtmlBlock(_)]
        ));
    }

    #[test]
    fn details_container_scan_ignores_closing_tag_inside_fenced_code() {
        let source = "<details>\n<summary>Log</summary>\n\n```\n</details>\n```\n\n</details>\n";
        let output = SyntaxOptions::default().parse(source);

        let [Block::HtmlContainer(details)] = output.document.children.as_slice() else {
            panic!(
                "expected one details container: {:?}",
                output.document.children
            );
        };
        let HtmlContainerContent::Blocks(children) = &details.content else {
            panic!("details should contain block content");
        };
        assert!(children
            .iter()
            .any(|block| matches!(block, Block::CodeBlock(_))));
    }

    #[test]
    fn reference_labels_allow_999_characters() {
        let label = "x".repeat(999);
        let source = format!("[{label}]: /url\n\n[full][{label}]\n[{label}][]\n[{label}]\n");
        let output = SyntaxOptions::commonmark().parse(&source);

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
        let definition_output = SyntaxOptions::commonmark().parse(&definition_source);

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
        let reference_output = SyntaxOptions::commonmark().parse(&reference_source);

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
    fn strict_mdx_reports_unclosed_jsx_blocks() {
        let err = SyntaxOptions::mdx().parse_strict("<A>\n").unwrap_err();

        let ParseStrictError::Diagnostic(diagnostic) = err else {
            panic!("expected strict parse diagnostic");
        };
        assert_eq!(diagnostic.code, DiagnosticCode::InvalidMdx);
    }

    #[test]
    fn directive_openers_scan_escaped_labels_and_quoted_attributes() {
        let mut constructs = Constructs::commonmark();
        constructs.directive_text = true;
        let options = SyntaxOptions {
            constructs: constructs,
            parse: ParseOptions::default(),
        };
        let output = options.parse(":note[has \\] bracket]{title=\"x } y\"}\n");

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
        let options = SyntaxOptions {
            constructs: Constructs::commonmark(),
            parse: ParseOptions {
                preserve_character_references: true,
                ..ParseOptions::default()
            },
        };
        let output = options.parse(source);

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

        let resolved = SyntaxOptions::commonmark().parse(source);
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
        let space = SyntaxOptions::commonmark().parse("** foo bar**\n").document;
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
        let punctuation = SyntaxOptions::commonmark().parse("a**\"foo\"**\n").document;
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
        let strong = SyntaxOptions::commonmark().parse("**foo bar**\n").document;
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
        let options = SyntaxOptions {
            constructs: Constructs::commonmark(),
            parse: ParseOptions {
                preserve_character_references: true,
                ..ParseOptions::default()
            },
        };
        let output = options.parse(source);

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
        let output = SyntaxOptions::commonmark().parse("[foo]: <my url> 'title'\n\n[angle](<foo bar> 'single') [paren](url (paren title)) [empty]( \"title\")\n");

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
        let heading = SyntaxOptions::commonmark().parse("# foo #\n");
        let Some(Block::Heading(node)) = heading.document.children.first() else {
            panic!("expected heading");
        };
        assert!(matches!(
            node.children.as_slice(),
            [Inline::Text(text)] if text.meta.span == Some(Span::new(2, 5))
        ));

        let blockquote = SyntaxOptions::commonmark().parse("> **a**\n");
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
        let options = SyntaxOptions {
            constructs: constructs,
            parse: ParseOptions::default(),
        };
        let directive = options.parse(":note[*x*]\n");
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

    /// The inline content of each body-row cell of the table `source` parses
    /// to, written compactly.
    fn body_cells(source: &str) -> Vec<String> {
        let output = SyntaxOptions::default().parse(source);
        let Some(Block::Table(table)) = output.document.children.first() else {
            panic!("{source:?}: expected a table");
        };
        table.rows[1]
            .cells
            .iter()
            .map(|cell| {
                cell.children
                    .iter()
                    .map(|inline| match inline {
                        Inline::Text(text) => text.value.clone(),
                        Inline::Code(code) => format!("Code({})", code.value),
                        Inline::Spoiler(spoiler) => format!(
                            "Spoiler({})",
                            spoiler
                                .children
                                .iter()
                                .map(|inline| match inline {
                                    Inline::Text(text) => text.value.clone(),
                                    Inline::Code(code) => format!("Code({})", code.value),
                                    other => format!("{other:?}"),
                                })
                                .collect::<String>()
                        ),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn table_spoilers_pair_as_the_inline_parser_pairs_them() {
        // A spoiler keeps the pipes between its bars in its cell.
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n| ||a | b|| | c |"),
            ["Spoiler(a | b)", "c"]
        );
        // A `||` inside a code span closes no spoiler, so the opener's bars
        // delimit.
        assert_eq!(
            body_cells("| w | x | y | z |\n|-|-|-|-|\n| ||a `||` | b |"),
            ["", "", "a Code(||)", "b"]
        );
        // The closer is the next `||` outside code spans.
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n| ||a `||` b|| | c |"),
            ["Spoiler(a Code(||) b)", "c"]
        );
        // A row may start with a spoiler instead of a border pipe.
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n||a|| | d |"),
            ["Spoiler(a)", "d"]
        );
        // Escaped pipes stay literal text in their own cells: a pair that uses
        // one never holds a pipe that delimits.
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n| a \\|\\| b | or, like c \\|\\| d |"),
            ["a || b", "or, like c || d"]
        );
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n|\\| a | b \\||"),
            ["| a", "b |"]
        );
        // An escaped backtick opens no code span, so the `||` after it opens a
        // spoiler that holds the pipe.
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n| \\`||a\\` | b|| |"),
            ["`Spoiler(a` | b)", ""]
        );
        // Bars with no closer are delimiters around an empty cell.
        assert_eq!(
            body_cells("| x | y | z |\n|---|---|---|\n|a||b|"),
            ["a", "", "b"]
        );
    }
}

mod lazy_lines_and_final_whitespace {
    //! A lazy line that opens a list ends the container it would otherwise
    //! continue, and a paragraph or setext heading drops the final whitespace
    //! of its content, as CommonMark specifies.

    use markdown_syntax::{Block, Inline, SyntaxOptions};

    fn blocks(source: &str) -> Vec<Block> {
        SyntaxOptions::commonmark().parse(source).document.children
    }

    fn texts(inlines: &[Inline]) -> Vec<String> {
        inlines
            .iter()
            .map(|inline| match inline {
                Inline::Text(text) => text.value.clone(),
                Inline::LineBreak(_) => "<br>".into(),
                Inline::SoftBreak(_) => "/".into(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn an_empty_list_item_on_a_lazy_line_ends_the_block_quote() {
        for source in ["> a\n- ", "> a\n-", "> a\n1. "] {
            let blocks = blocks(source);
            let [Block::BlockQuote(quote), Block::List(list)] = blocks.as_slice() else {
                panic!("{source:?}: expected a block quote and a list, got {blocks:?}");
            };
            let [Block::Paragraph(paragraph)] = quote.children.as_slice() else {
                panic!("{source:?}: expected one paragraph in the quote");
            };
            assert_eq!(texts(&paragraph.children), ["a"], "{source:?}");
            assert!(
                matches!(list.children.as_slice(), [item] if item.children.is_empty()),
                "{source:?}: {list:?}"
            );
        }
    }

    #[test]
    fn an_ordered_item_not_starting_at_one_on_a_lazy_line_ends_the_block_quote() {
        let blocks = blocks("> > a\n2. b");
        let [Block::BlockQuote(_), Block::List(list)] = blocks.as_slice() else {
            panic!("expected a block quote and a list, got {blocks:?}");
        };
        assert!(list.ordered);
        assert_eq!(list.start, Some(2));
    }

    #[test]
    fn a_lazy_list_marker_does_not_join_a_list_inside_the_quote() {
        let blocks = blocks("> - a\n- ");
        let [Block::BlockQuote(quote), Block::List(outer)] = blocks.as_slice() else {
            panic!("expected a block quote and a list, got {blocks:?}");
        };
        assert!(
            matches!(quote.children.as_slice(), [Block::List(inner)] if inner.children.len() == 1)
        );
        assert_eq!(outer.children.len(), 1);
    }

    #[test]
    fn a_paragraph_drops_the_final_whitespace_of_its_content() {
        for (source, expected) in [
            ("foo  ", vec!["foo"]),
            ("foo \t\n", vec!["foo"]),
            ("aaa     \nbbb     ", vec!["aaa", "<br>", "bbb"]),
            ("> a  ", vec!["a"]),
        ] {
            let blocks = blocks(source);
            let paragraph = match blocks.as_slice() {
                [Block::Paragraph(paragraph)] => paragraph,
                [Block::BlockQuote(quote)] => match quote.children.as_slice() {
                    [Block::Paragraph(paragraph)] => paragraph,
                    other => panic!("{source:?}: {other:?}"),
                },
                other => panic!("{source:?}: {other:?}"),
            };
            assert_eq!(texts(&paragraph.children), expected, "{source:?}");
        }
        let blocks = blocks("foo  ");
        let [Block::Paragraph(paragraph)] = blocks.as_slice() else {
            panic!("expected a paragraph");
        };
        assert_eq!(paragraph.children[0].span().map(|span| span.end), Some(3));
    }

    #[test]
    fn a_setext_heading_drops_the_final_whitespace_of_its_content() {
        for source in ["Foo  \n-----", "Foo\t\n==="] {
            let blocks = blocks(source);
            let [Block::Heading(heading)] = blocks.as_slice() else {
                panic!("{source:?}: expected a heading, got {blocks:?}");
            };
            assert_eq!(texts(&heading.children), ["Foo"], "{source:?}");
        }
    }
}

mod container_laziness {
    //! Lazy paragraph continuation inside lists and block quotes, as cmark,
    //! commonmark.js, and micromark read it, and the blank lines and blank
    //! item separators that end containers or loosen lists.

    use markdown_syntax::{Block, Inline, ListItem, SyntaxOptions};

    fn blocks(source: &str) -> Vec<Block> {
        SyntaxOptions::commonmark().parse(source).document.children
    }

    fn texts(inlines: &[Inline]) -> Vec<String> {
        inlines
            .iter()
            .map(|inline| match inline {
                Inline::Text(text) => text.value.clone(),
                Inline::SoftBreak(_) => "/".into(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    fn only_item(block: &Block) -> &ListItem {
        match block {
            Block::List(list) if list.children.len() == 1 => &list.children[0],
            other => panic!("expected a one-item list, got {other:?}"),
        }
    }

    fn paragraph_texts(block: &Block) -> Vec<String> {
        match block {
            Block::Paragraph(paragraph) => texts(&paragraph.children),
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    #[test]
    fn a_lazy_line_continues_a_paragraph_in_an_item_that_started_blank() {
        let blocks = blocks("- \n  a\nb");
        let [list] = blocks.as_slice() else {
            panic!("expected one list, got {blocks:?}");
        };
        assert_eq!(
            paragraph_texts(&only_item(list).children[0]),
            ["a", "/", "b"]
        );

        let blocks = self::blocks("* \n  1. a\na");
        let [list] = blocks.as_slice() else {
            panic!("expected one list, got {blocks:?}");
        };
        let inner = only_item(&only_item(list).children[0]);
        assert_eq!(paragraph_texts(&inner.children[0]), ["a", "/", "a"]);
    }

    #[test]
    fn a_lazy_line_continues_a_paragraph_after_a_thematic_break_in_an_item() {
        let blocks = blocks("2. ---\n   > b c\nb c");
        let [list] = blocks.as_slice() else {
            panic!("expected one list, got {blocks:?}");
        };
        let item = only_item(list);
        let [Block::ThematicBreak(_), Block::BlockQuote(quote)] = item.children.as_slice() else {
            panic!("expected a break and a quote, got {:?}", item.children);
        };
        assert_eq!(paragraph_texts(&quote.children[0]), ["b c", "/", "b c"]);
    }

    #[test]
    fn an_empty_item_marker_that_cannot_interrupt_continues_the_paragraph() {
        let blocks = blocks("- a\n    * \nb c");
        let [list] = blocks.as_slice() else {
            panic!("expected one list, got {blocks:?}");
        };
        assert_eq!(
            paragraph_texts(&only_item(list).children[0]),
            ["a", "/", "*", "/", "b c"]
        );
    }

    #[test]
    fn a_lazy_line_of_a_block_quote_stays_lazy_inside_its_list() {
        let blocks = blocks("> - a\n    - ");
        let [Block::BlockQuote(quote)] = blocks.as_slice() else {
            panic!("expected one quote, got {blocks:?}");
        };
        assert_eq!(
            paragraph_texts(&only_item(&quote.children[0]).children[0]),
            ["a", "/", "-"]
        );
    }

    #[test]
    fn a_line_short_of_the_paragraphs_quote_level_continues_it_only_lazily() {
        let blocks = blocks("> > a\n> 1. ");
        let [Block::BlockQuote(quote)] = blocks.as_slice() else {
            panic!("expected one quote, got {blocks:?}");
        };
        assert!(
            matches!(
                quote.children.as_slice(),
                [Block::BlockQuote(_), Block::List(_)]
            ),
            "{:?}",
            quote.children
        );

        let blocks = self::blocks("> > a\n> > 1. ");
        let [Block::BlockQuote(outer)] = blocks.as_slice() else {
            panic!("expected one quote, got {blocks:?}");
        };
        let [Block::BlockQuote(inner)] = outer.children.as_slice() else {
            panic!("expected a nested quote, got {:?}", outer.children);
        };
        assert_eq!(paragraph_texts(&inner.children[0]), ["a", "/", "1."]);
    }

    #[test]
    fn a_blank_line_indented_four_columns_ends_a_block_quote() {
        let blocks = blocks("> a\n    \n> b");
        assert!(
            matches!(
                blocks.as_slice(),
                [Block::BlockQuote(_), Block::BlockQuote(_)]
            ),
            "{blocks:?}"
        );
    }

    #[test]
    fn a_blank_line_between_an_empty_item_and_the_next_loosens_the_list() {
        let blocks = blocks("* \n\n  * b c");
        let [Block::List(list)] = blocks.as_slice() else {
            panic!("expected one list, got {blocks:?}");
        };
        assert_eq!(list.children.len(), 2);
        assert!(!list.tight);
    }

    #[test]
    fn a_blank_line_before_a_thematic_break_leaves_the_list_tight() {
        let blocks = blocks("- a\n\n- ---");
        let [Block::List(list), Block::ThematicBreak(_)] = blocks.as_slice() else {
            panic!("expected a list and a break, got {blocks:?}");
        };
        assert!(list.tight);
    }

    #[test]
    fn a_complete_html_tag_on_a_lazy_line_ends_a_list_item() {
        // cmark-gfm and micromark start a type-7 HTML block here; upstream cmark
        // and commonmark.js keep the tag in the paragraph.
        let blocks = blocks("- a\n<a>");
        assert!(
            matches!(blocks.as_slice(), [Block::List(_), Block::HtmlBlock(_)]),
            "{blocks:?}"
        );
    }

    fn quote_children(block: &Block) -> &[Block] {
        match block {
            Block::BlockQuote(quote) => &quote.children,
            other => panic!("expected a block quote, got {other:?}"),
        }
    }

    #[test]
    fn a_line_after_an_unclosed_fence_in_a_block_quote_is_not_lazy() {
        for source in ["> ```\n> x\na", "> > ```\n> > x\na", "> ```\n> \n> x\na"] {
            let blocks = blocks(source);
            let [quote, paragraph] = blocks.as_slice() else {
                panic!("{source:?}: expected a quote and a paragraph, got {blocks:?}");
            };
            assert!(
                !format!("{quote:?}").contains("\"a"),
                "{source:?}: {quote:?}"
            );
            assert_eq!(paragraph_texts(paragraph), ["a"], "{source:?}");
        }
    }

    #[test]
    fn a_line_after_an_html_or_math_block_in_a_block_quote_is_not_lazy() {
        let math = SyntaxOptions::default()
            .parse("> $$\n> x\na")
            .document
            .children;
        for blocks in [blocks("> <div>\n> x\na"), blocks("> <!--\n> x\na"), math] {
            let [quote, paragraph] = blocks.as_slice() else {
                panic!("expected a quote and a paragraph, got {blocks:?}");
            };
            assert_eq!(quote_children(quote).len(), 1, "{quote:?}");
            assert_eq!(paragraph_texts(paragraph), ["a"]);
        }
    }

    #[test]
    fn a_quoted_paragraph_opening_with_backticks_takes_lazy_lines() {
        // The fence-like rule ends a quote at a lazy line, not at a quoted one.
        let blocks = blocks("> ``a\nb");
        let [quote] = blocks.as_slice() else {
            panic!("expected one quote, got {blocks:?}");
        };
        let [paragraph] = quote_children(quote) else {
            panic!("expected one paragraph, got {quote:?}");
        };
        assert_eq!(
            paragraph_texts(paragraph).last().map(String::as_str),
            Some("b")
        );
    }

    #[test]
    fn a_blank_line_inside_a_nested_items_open_fence_leaves_the_list_tight() {
        let blocks = blocks("2. a\n   1. ```\n\n2. b");
        let [Block::List(list)] = blocks.as_slice() else {
            panic!("expected one list, got {blocks:?}");
        };
        assert!(list.tight, "{list:?}");
    }

    #[test]
    fn a_closed_fence_leaves_the_next_paragraph_open_to_lazy_lines() {
        let blocks = blocks("> ```\n> x\n> ```\n> y\na");
        let [quote] = blocks.as_slice() else {
            panic!("expected one quote, got {blocks:?}");
        };
        let [Block::CodeBlock(_), paragraph] = quote_children(quote) else {
            panic!("expected code and a paragraph, got {quote:?}");
        };
        assert_eq!(paragraph_texts(paragraph), ["y", "/", "a"]);
    }

    #[test]
    fn a_fence_inside_a_quoted_list_item_does_not_hold_the_quote_open() {
        let blocks = blocks("> - ```\n> x\na");
        let [quote] = blocks.as_slice() else {
            panic!("expected one quote, got {blocks:?}");
        };
        let [Block::List(_), paragraph] = quote_children(quote) else {
            panic!("expected a list and a paragraph, got {quote:?}");
        };
        assert_eq!(paragraph_texts(paragraph), ["x", "/", "a"]);
    }
}

mod paragraph_interruption {
    //! Only a line that opens the block it looks like interrupts a paragraph.

    use markdown_syntax::{Block, CodeBlock, SyntaxOptions};

    fn blocks(source: &str) -> Vec<Block> {
        SyntaxOptions::commonmark().parse(source).document.children
    }

    #[test]
    fn a_hash_run_that_is_not_a_heading_continues_the_paragraph() {
        for source in ["a\n#)", "a\n#b", "a\n#######"] {
            let blocks = blocks(source);
            assert!(
                matches!(blocks.as_slice(), [Block::Paragraph(_)]),
                "{source:?}: {blocks:?}"
            );
        }
    }

    #[test]
    fn a_backtick_run_with_a_backtick_in_its_info_continues_the_paragraph() {
        let blocks = blocks("a\n``` `` ```\n```b`");
        assert!(
            matches!(blocks.as_slice(), [Block::Paragraph(_)]),
            "{blocks:?}"
        );
    }

    #[test]
    fn a_malformed_directive_line_continues_the_paragraph() {
        for source in ["a\n::", "a\n::1bad", "a\n:::", "a\n::: x"] {
            let blocks = SyntaxOptions::default().parse(source).document.children;
            assert!(
                matches!(blocks.as_slice(), [Block::Paragraph(_)]),
                "{source:?}: {blocks:?}"
            );
        }
    }

    #[test]
    fn a_container_ends_its_last_line_with_a_line_feed() {
        let blocks = blocks("- ```\n  ~\r");
        let [Block::List(list)] = blocks.as_slice() else {
            panic!("expected one list, got {blocks:?}");
        };
        let [Block::CodeBlock(CodeBlock { value, .. })] = list.children[0].children.as_slice()
        else {
            panic!("expected one code block, got {list:?}");
        };
        assert_eq!(value, "~\n");
    }

    #[test]
    fn a_complete_tag_after_a_definition_continues_its_paragraph() {
        let tag = blocks("[o]: u\n<a>");
        assert!(
            matches!(tag.as_slice(), [Block::Definition(_), Block::Paragraph(_)]),
            "{tag:?}"
        );
        let div = blocks("[o]: u\n<div>");
        assert!(
            matches!(div.as_slice(), [Block::Definition(_), Block::HtmlBlock(_)]),
            "{div:?}"
        );
    }

    #[test]
    fn a_header_row_indented_four_columns_starts_no_table() {
        let blocks = SyntaxOptions::default()
            .parse("a\n    |b\n----")
            .document
            .children;
        assert!(
            matches!(blocks.as_slice(), [Block::Heading(_)]),
            "{blocks:?}"
        );
    }

    #[test]
    fn a_directive_attribute_without_a_valid_name_is_dropped() {
        let document = SyntaxOptions::default().parse(":b{<} :c{a <=1 d}").document;
        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, ":b :c{a d}\n");
    }

    #[test]
    fn a_tab_after_a_top_level_containers_marker_spans_its_source_columns() {
        // `> ` ends at column 2, so the tab reaches column 4: two columns, a
        // paragraph's indentation, not an indented code block.
        let quote = blocks("> \tcode");
        let [Block::BlockQuote(inner)] = quote.as_slice() else {
            panic!("expected one quote, got {quote:?}");
        };
        assert!(
            matches!(inner.children.as_slice(), [Block::Paragraph(_)]),
            "{inner:?}"
        );
        let list = blocks("- item\n\n  \tcode");
        let [Block::List(list)] = list.as_slice() else {
            panic!("expected one list, got {list:?}");
        };
        assert!(
            matches!(
                list.children[0].children.as_slice(),
                [Block::Paragraph(_), Block::Paragraph(_)]
            ),
            "{list:?}"
        );
        // A tab past the indentation an indented code block takes stays a
        // tab of its value, and so does one inside a fence.
        let code = blocks("- a\n\n      \tb");
        let [Block::List(code)] = code.as_slice() else {
            panic!("expected one list, got {code:?}");
        };
        assert!(
            matches!(code.children[0].children.as_slice(),
                [Block::Paragraph(_), Block::CodeBlock(CodeBlock { value, .. })] if value == "\tb\n"),
            "{code:?}"
        );
        let fence = blocks("> ```\n> \tb\n> ```");
        assert!(
            format!("{fence:?}").contains("value: \"\\tb\\n\""),
            "{fence:?}"
        );
    }

    #[test]
    fn a_tab_inside_nested_containers_spans_its_source_columns() {
        // Each tab reaches its stop from its column in the source line, not from
        // the start of the content a container read it in.
        let shape = |source: &str| format!("{:?}", blocks(source));
        // `> > ` ends at column 4: the tab spans four columns, code.
        assert!(
            shape("> > \ta").contains("CodeBlock"),
            "{}",
            shape("> > \ta")
        );
        // `* - ` ends at column 4: one space and a four-column tab, code.
        assert!(
            shape("* - \tb c").contains("CodeBlock"),
            "{}",
            shape("* - \tb c")
        );
        // `>1. \t` holds code, so the next line is not its lazy continuation.
        let lazy = blocks(">1. \ta\n    \t1. ===");
        assert!(
            matches!(lazy.as_slice(), [Block::BlockQuote(_), Block::CodeBlock(_)]),
            "{lazy:?}"
        );
    }

    #[test]
    fn indented_code_keeps_the_first_line_ending_for_its_last_line() {
        for (source, value) in [("\ta\r\tb", "a\rb\r"), ("    a\r\n    b", "a\r\nb\r\n")] {
            let blocks = blocks(source);
            let [Block::CodeBlock(CodeBlock { value: actual, .. })] = blocks.as_slice() else {
                panic!("{source:?}: expected one code block, got {blocks:?}");
            };
            assert_eq!(actual, value, "{source:?}");
        }
    }
}
