//! Block-parsing regression coverage: setext/list/code-block defects from the
//! review pass plus the broader parser regressions (reference definitions,
//! HTML blocks, character references, link resources, source spans).
//!
//! Each former regression file is preserved verbatim inside its own `mod` so
//! that helper functions and test names cannot collide across the merged
//! sources.

#[path = "support/normalize.rs"]
mod normalize;

mod review_block {
    //! Regression tests for the block-level parser defects fixed in the 2026-06-18
    //! review. Each asserts the CommonMark-correct
    //! AST shape against the live parser.

    use markdown_syntax::{parse, Block, CodeBlockKind, HeadingKind, Inline, ListDelimiter};

    /// B1: a multi-line paragraph followed by a setext underline is a setext
    /// heading whose content spans every paragraph line, not one flat paragraph.
    #[test]
    fn setext_heading_absorbs_multiline_paragraph() {
        let output = parse("Foo\nbar\n===\n");

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
        let output = parse("Foo\n---\n");
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
        let output = parse("Foo\n# heading\n===\n");
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
        let output = parse("- foo\n - bar\n  - baz\n   - boo\n");

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
        let output = parse("- foo\n  - bar\n");

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
        let output = parse("- a\n+ b\n");
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
        let output = parse("foo\n*\n");
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
        let output = parse("*\n");
        let [Block::List(list)] = output.document.children.as_slice() else {
            panic!("expected an empty list: {:?}", output.document.children);
        };
        assert_eq!(list.children.len(), 1);
        assert!(list.children[0].children.is_empty(), "empty item: {list:?}");
    }

    /// B3 guard: a non-empty list item still interrupts a paragraph.
    #[test]
    fn non_empty_list_item_still_interrupts_paragraph() {
        let output = parse("foo\n- bar\n");
        assert!(matches!(
            output.document.children.as_slice(),
            [Block::Paragraph(_), Block::List(_)]
        ));
    }

    /// B4: a fenced code block indented N spaces strips up to N leading spaces from
    /// each content line.
    #[test]
    fn fenced_code_strips_opening_indent_from_content() {
        let output = parse(" ```\n aaa\naaa\n```\n");
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
        let output = parse("   ```\n   aaa\n    aaa\n  aaa\n   ```\n");
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
        let output = parse("    foo\n    \n");
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
        let output = parse("    foo\n\n    bar\n");
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
        parse, Block, HtmlContainerContent, Inline, LinkDestinationKind, LinkTitleKind,
        ReferenceKind, Span, Title,
    };

    #[test]
    fn reference_definitions_are_collected_from_real_blocks_only() {
        let output = parse("```\n[foo]: /url\n```\n\n[foo]\n");

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
        let output = parse("[foo]:\n /url\n\n[foo]\n");

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
        let interrupted = parse("a\n2. b\n");
        assert_eq!(interrupted.document.children.len(), 1);
        assert!(matches!(
            interrupted.document.children.as_slice(),
            [Block::Paragraph(_)]
        ));

        let too_many_digits = parse("1234567890. not ok\n");
        assert!(matches!(
            too_many_digits.document.children.as_slice(),
            [Block::Paragraph(_)]
        ));
    }

    #[test]
    fn html_block_starts_interrupt_paragraphs_when_commonmark_allows() {
        let output = parse("foo\n<div>\nbar\n");

        assert!(matches!(
            output.document.children.as_slice(),
            [Block::Paragraph(_), Block::HtmlBlock(_)]
        ));
    }

    #[test]
    fn raw_html_block_close_requires_matching_raw_tag_name() {
        let source = "<script>\nnot closed by </scripture>\nstill raw\n</script>\n";
        let output = parse(source);

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
        let output = parse(source);

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
        let output = parse(source);

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
    fn details_on_their_own_lines_form_an_html_container() {
        let source = "<details>\n\nbody\n\n</details>\n";
        let output = parse(source);

        assert!(matches!(
            output.document.children.as_slice(),
            [Block::HtmlContainer(_)]
        ));
    }

    #[test]
    fn html_container_construct_can_interrupt_without_html_block() {
        let output = parse("before\n<details>\n\nbody\n\n</details>\n");

        assert!(matches!(
            output.document.children.as_slice(),
            [Block::Paragraph(_), Block::HtmlContainer(_)]
        ));
    }

    #[test]
    fn unclosed_details_falls_back_to_raw_html_block() {
        let source = "<details>\n<summary>Open</summary>\n";
        let output = parse(source);

        assert!(matches!(
            output.document.children.as_slice(),
            [Block::HtmlBlock(_)]
        ));
    }

    #[test]
    fn tab_indented_details_in_an_item_measure_columns_from_the_item() {
        // The item takes two of the tab's columns; the rest is indentation
        // of at most three columns, so these are details containers.
        for source in [
            "- a\n\t<details>\n\n\tx\n\t</details>",
            "- <details>\n\n  x\n  \t</details>",
            "- a\n\t<details>\n\t<summary>\n\t\t\tExample\n\t</summary>\n\n\tb\n\t</details>\n",
        ] {
            let document = parse(source).document;
            let [Block::List(list)] = document.children.as_slice() else {
                panic!("{source:?}: expected one list, got {:?}", document.children);
            };
            assert!(
                list.children[0]
                    .children
                    .iter()
                    .any(|block| matches!(block, Block::HtmlContainer(_))),
                "{source:?}: {:?}",
                list.children[0].children
            );
            let markdown = document.to_markdown().expect("document serializes");
            let reparsed = parse(&markdown).document;
            assert_eq!(
                format!("{:?}", crate::normalize::normalized(&reparsed.children)),
                format!("{:?}", crate::normalize::normalized(&document.children)),
                "{source:?} -> {markdown:?}"
            );
        }
    }

    #[test]
    fn details_container_scan_ignores_closing_tag_inside_fenced_code() {
        let source = "<details>\n<summary>Log</summary>\n\n```\n</details>\n```\n\n</details>\n";
        let output = parse(source);

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
        let output = parse(&source);

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
        let definition_output = parse(&definition_source);

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
        let reference_output = parse(&reference_source);

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
    fn a_former_mdx_jsx_block_is_raw_html() {
        let output = parse("<A>\n");

        assert!(
            matches!(output.document.children.as_slice(), [Block::HtmlBlock(_)]),
            "{:?}",
            output.document.children
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    }

    #[test]
    fn directive_openers_scan_escaped_labels_and_quoted_attributes() {
        let output = parse(":note[has \\] bracket]{title=\"x } y\"}\n");

        let Some(Block::Paragraph(paragraph)) = output.document.children.first() else {
            panic!("expected paragraph");
        };
        let [Inline::TextDirective(directive)] = paragraph.children.as_slice() else {
            panic!("expected text directive");
        };
        assert!(matches!(
            directive.label.as_slice(),
            [Inline::Text(before), Inline::Escape(escape), Inline::Text(after)]
                if before.value == "has " && escape.value == ']' && after.value == " bracket"
        ));
        assert_eq!(directive.attributes.len(), 1);
        assert_eq!(directive.attributes[0].name, "title");
        assert_eq!(directive.attributes[0].value.as_deref(), Some("x } y"));
    }

    #[test]
    fn named_character_references_cover_common_html5_entities() {
        let source =
            "&semi; &trade; &NotEqualTilde; &CounterClockwiseContourIntegral; &acE; &nGg; &fjlig; &AMP;\n";
        let output = parse(source);

        let Some(Block::Paragraph(paragraph)) = output.document.children.first() else {
            panic!("expected paragraph");
        };
        let references = paragraph
            .children
            .iter()
            .filter_map(|inline| match inline {
                Inline::CharacterReference(reference) => Some((
                    reference.reference.clone(),
                    reference.value().expect("decodes"),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        let references = references
            .iter()
            .map(|(reference, value)| (reference.as_str(), value.as_str()))
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
    }

    #[test]
    fn asterisk_runs_open_emphasis_only_as_whole_left_flanking_runs() {
        // CommonMark example 397: a `**` run followed by whitespace is not
        // left-flanking, so no single `*` may be peeled off to open emphasis.
        let space = parse("** foo bar**\n").document;
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
        let punctuation = parse("a**\"foo\"**\n").document;
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
        let strong = parse("**foo bar**\n").document;
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
        let output = parse(source);

        let Some(Block::Paragraph(paragraph)) = output.document.children.first() else {
            panic!("expected paragraph");
        };
        let references = paragraph
            .children
            .iter()
            .filter_map(|inline| match inline {
                Inline::CharacterReference(reference) => Some((
                    reference.reference.clone(),
                    reference.value().expect("decodes"),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        let references = references
            .iter()
            .map(|(reference, value)| (reference.as_str(), value.as_str()))
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
        let output = parse("[foo]: <my url> 'title'\n\n[angle](<foo bar> 'single') [paren](url (paren title)) [empty]( \"title\")\n");

        let Some(Block::Definition(definition)) = output.document.children.first() else {
            panic!("expected definition");
        };
        assert_eq!(definition.destination, "my url");
        assert_eq!(definition.destination_kind, LinkDestinationKind::Angle);
        assert_eq!(
            definition.title,
            Some(Title::new("title", LinkTitleKind::SingleQuote))
        );

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
                && angle.title == Some(Title::new("single", LinkTitleKind::SingleQuote))
                && paren.destination == "url"
                && paren.destination_kind == LinkDestinationKind::Bare
                && paren.title == Some(Title::new("paren title", LinkTitleKind::Paren))
                && empty.destination.is_empty()
                && empty.destination_kind == LinkDestinationKind::Omitted
                && empty.title == Some(Title::new("title", LinkTitleKind::DoubleQuote))
        ));
    }

    #[test]
    fn localized_source_spans_track_trimmed_markers() {
        let heading = parse("# foo #\n");
        let Some(Block::Heading(node)) = heading.document.children.first() else {
            panic!("expected heading");
        };
        assert!(matches!(
            node.children.as_slice(),
            [Inline::Text(text)] if text.meta.span == Some(Span::new(2, 5))
        ));

        let blockquote = parse("> **a**\n");
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

        let directive = parse(":note[*x*]\n");
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
        let output = parse(source);
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
                        Inline::Escape(escape) => format!("\\{}", escape.value),
                        Inline::Code(code) => format!("Code({})", code.value),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn table_rows_split_at_unescaped_pipes() {
        // Every unescaped pipe delimits a cell, inside a `||` run or a code
        // span too; a row keeps as many cells as the header has.
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n| ||a | b|| | c |"),
            ["", ""]
        );
        assert_eq!(
            body_cells("| w | x | y | z |\n|-|-|-|-|\n| ||a `||` | b |"),
            ["", "", "a `", ""]
        );
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n| ||a `||` b|| | c |"),
            ["", ""]
        );
        assert_eq!(body_cells("| x | y |\n|---|---|\n||a|| | d |"), ["", "a"]);
        // Escaped pipes stay literal text in their own cells.
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n| a \\|\\| b | or, like c \\|\\| d |"),
            ["a \\|\\| b", "or, like c \\|\\| d"]
        );
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n|\\| a | b \\||"),
            ["\\| a", "b \\|"]
        );
        assert_eq!(
            body_cells("| x | y |\n|---|---|\n| \\`||a\\` | b|| |"),
            ["\\`", ""]
        );
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

    use markdown_syntax::{parse, Block, Inline};

    fn blocks(source: &str) -> Vec<Block> {
        parse(source).document.children
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

    use markdown_syntax::{parse, Block, Inline, ListItem};

    fn blocks(source: &str) -> Vec<Block> {
        parse(source).document.children
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
        let math = parse("> $$\n> x\na").document.children;
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

    use markdown_syntax::{parse, Block, CodeBlock};

    fn blocks(source: &str) -> Vec<Block> {
        parse(source).document.children
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
    fn a_list_that_cannot_interrupt_a_paragraph_continues_a_definitions_paragraph() {
        for source in ["[foo]: /url\n2) a", "[foo]: /url\n-"] {
            let blocks = blocks(source);
            assert!(
                matches!(
                    blocks.as_slice(),
                    [Block::Definition(_), Block::Paragraph(_)]
                ),
                "{source:?}: {blocks:?}"
            );
        }
        let blocks = blocks("[foo]: /url\n- a");
        assert!(
            matches!(blocks.as_slice(), [Block::Definition(_), Block::List(_)]),
            "{blocks:?}"
        );
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
            let blocks = parse(source).document.children;
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
        let blocks = parse("a\n    |b\n----").document.children;
        assert!(
            matches!(blocks.as_slice(), [Block::Heading(_)]),
            "{blocks:?}"
        );
    }

    #[test]
    fn a_directive_attribute_without_a_valid_name_is_dropped() {
        let document = parse(":b{<} :c{a <=1 d}").document;
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

mod unicode_whitespace {
    //! Block structure reads only spaces and tabs as whitespace: a line holding
    //! another whitespace char, such as a no-break space or a form feed, is
    //! neither blank nor indented, and such a char ends no marker or fence.

    use markdown_syntax::{parse, Block, Inline};

    fn blocks(source: &str) -> Vec<Block> {
        parse(source).document.children
    }

    #[test]
    fn a_line_with_other_whitespace_is_text() {
        for source in [
            "***\u{a0}",
            "\u{a0}***",
            "a\n---\u{a0}",
            "a\n===\u{3000}",
            "a\n\u{a0}\nb",
            "a\n\u{c}---",
            "<a>\u{a0}\nx",
        ] {
            let blocks = blocks(source);
            assert!(
                matches!(blocks.as_slice(), [Block::Paragraph(_)]),
                "{source:?}: {blocks:?}"
            );
        }
        // The dashes read as list markers, as no thematic break forms.
        let nested = blocks("- - -\u{c}");
        assert!(matches!(nested.as_slice(), [Block::List(_)]), "{nested:?}");
        for source in ["| a |\n| - |\u{a0}", "\u{a0}| a |\n| - |"] {
            let blocks = blocks(source);
            assert!(
                matches!(blocks.as_slice(), [Block::Paragraph(_)]),
                "{source:?}: {blocks:?}"
            );
        }
    }

    #[test]
    fn a_no_break_space_before_a_tag_keeps_it_inline() {
        let blocks = blocks("\u{a0} <p/>");
        assert!(
            matches!(blocks.as_slice(), [Block::Paragraph(_)]),
            "{blocks:?}"
        );
    }

    #[test]
    fn a_form_feed_ending_a_paragraph_stays_its_text() {
        let blocks = blocks("a\u{c}");
        let [Block::Paragraph(paragraph)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        assert!(
            matches!(paragraph.children.as_slice(), [Inline::Text(text)] if text.value == "a\u{c}"),
            "{blocks:?}"
        );
    }

    #[test]
    fn labels_collapse_only_spaces_tabs_and_line_endings() {
        let debug = format!("{:?}", blocks("[a\u{a0}b]\n\n[a b]: /u"));
        assert!(!debug.contains("LinkReference"), "{debug}");
        let debug = format!("{:?}", blocks("[^a\u{a0}b]\n\n[^a\u{a0}b]: x"));
        assert!(debug.contains("FootnoteReference"), "{debug}");
    }
}

mod footnotes_and_directives {
    use markdown_syntax::{parse, Block, Inline};

    #[test]
    fn a_footnote_definitions_first_line_keeps_a_hard_break() {
        let blocks = parse("[^1]: a  \nb").document.children;
        let [Block::FootnoteDefinition(definition)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        let [Block::Paragraph(paragraph)] = definition.children.as_slice() else {
            panic!("{blocks:?}");
        };
        assert!(
            matches!(
                paragraph.children.as_slice(),
                [Inline::Text(_), Inline::LineBreak(_), Inline::Text(_)]
            ),
            "{blocks:?}"
        );
    }

    #[test]
    fn a_directive_opener_inside_fenced_code_is_code() {
        let blocks = parse(":::t\n```\n:::e\n```\n:::").document.children;
        let [Block::ContainerDirective(directive)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        assert!(
            matches!(directive.children.as_slice(), [Block::CodeBlock(code)] if code.value == ":::e\n"),
            "{blocks:?}"
        );
    }

    #[test]
    fn a_fence_left_open_in_a_nested_directive_ends_with_it() {
        let output = parse(":::outer\n:::inner\n```\n:::\n:::other\nx\n:::\n:::\nafter");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let blocks = output.document.children;
        let [Block::ContainerDirective(outer), Block::Paragraph(after)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        assert_eq!(after.children.len(), 1, "{blocks:?}");
        assert!(
            matches!(
                outer.children.as_slice(),
                [Block::ContainerDirective(inner), Block::ContainerDirective(inner2)]
                    if inner.name == "inner" && inner2.name == "other"
            ),
            "{blocks:?}"
        );
    }
}

mod gfm_tables {
    use markdown_syntax::{parse, Block};

    fn blocks(source: &str) -> Vec<Block> {
        parse(source).document.children
    }

    #[test]
    fn a_setext_underline_wins_over_a_delimiter_row_without_pipes() {
        for source in ["| --- |\n-- ", "|a|\n---"] {
            let blocks = blocks(source);
            assert!(
                matches!(blocks.as_slice(), [Block::Heading(_)]),
                "{source:?}: {blocks:?}"
            );
        }
        assert!(matches!(blocks("a\n|---").as_slice(), [Block::Table(_)]));
        // The paragraph's earlier lines join the heading too.
        for source in ["a\n|b\n---", "a\n| b |\n---"] {
            let blocks = blocks(source);
            assert!(
                matches!(blocks.as_slice(), [Block::Heading(heading)] if heading.children.len() == 3),
                "{source:?}: {blocks:?}"
            );
        }
    }

    #[test]
    fn a_table_that_ends_a_paragraph_starts_on_its_header_row() {
        let table = blocks("a\n+\n|-");
        assert!(
            matches!(table.as_slice(), [Block::Paragraph(_), Block::Table(_)]),
            "{table:?}"
        );
        // Below a lazy line no table forms, so the paragraph goes on.
        let lazy = blocks("- $\n  +\n|-");
        let [Block::List(list)] = lazy.as_slice() else {
            panic!("{lazy:?}");
        };
        assert!(
            matches!(list.children[0].children.as_slice(), [Block::Paragraph(_)]),
            "{lazy:?}"
        );
    }

    #[test]
    fn a_lazy_line_is_no_delimiter_row() {
        let blocks = blocks("1. ---(\n:-:");
        let [Block::List(list)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        assert!(
            matches!(list.children[0].children.as_slice(), [Block::Paragraph(_)]),
            "{blocks:?}"
        );
    }
}

mod list_items_in_containers {
    use markdown_syntax::{parse, Block};

    fn blocks(source: &str) -> Vec<Block> {
        parse(source).document.children
    }

    #[test]
    fn a_quoted_line_short_of_the_item_is_lazy_for_its_paragraph() {
        // The list marker ends the item's paragraph and the quote's content,
        // as on any lazy line, so the next line is outside the quote.
        for source in ["> - a\n> 2.\nz", "> - > a\n>   2.\nz"] {
            let blocks = blocks(source);
            assert!(
                matches!(
                    blocks.as_slice(),
                    [Block::BlockQuote(_), Block::Paragraph(_)]
                ),
                "{source:?}: {blocks:?}"
            );
        }
        // A setext-like line short of the item continues its paragraph.
        let blocks = blocks("> 1. a\n> ===\nb");
        let [Block::BlockQuote(quote)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        let [Block::List(list)] = quote.children.as_slice() else {
            panic!("{blocks:?}");
        };
        assert!(
            matches!(list.children[0].children.as_slice(), [Block::Paragraph(paragraph)] if paragraph.children.len() == 5),
            "{blocks:?}"
        );
    }

    #[test]
    fn a_lazy_fence_like_line_opens_no_fence_in_an_item() {
        let blocks = blocks("1.   a\n    ```\n\nb");
        assert!(
            matches!(blocks.as_slice(), [Block::List(_), Block::Paragraph(_)]),
            "{blocks:?}"
        );
    }

    #[test]
    fn a_block_open_in_a_nested_item_ends_with_the_item() {
        // The sibling item ends the fence, so the blank line loosens the list.
        let loose = blocks("- - ```\n  - a\n\n- b");
        let [Block::List(list)] = loose.as_slice() else {
            panic!("{loose:?}");
        };
        assert!(!list.tight, "{loose:?}");
        // A line below the nested item's content continues the outer item,
        // and the line after it is lazy.
        let lazy = blocks("- a\n  - ```\n  b\nc");
        let [Block::List(list)] = lazy.as_slice() else {
            panic!("{lazy:?}");
        };
        assert!(
            matches!(list.children[0].children.as_slice(), [Block::Paragraph(_), Block::List(_), Block::Paragraph(paragraph)] if paragraph.children.len() == 3),
            "{lazy:?}"
        );
    }
}
#[cfg(feature = "html")]
mod nested_containers {
    //! The nested-container parse cases of plimeor/markdown-syntax#11, each
    //! with the HTML commonmark.js renders for it. Where micromark renders
    //! something else, the case says so; commonmark.js is the reference.

    use markdown_syntax::{parse, HtmlOptions};

    struct Case {
        name: &'static str,
        source: &'static str,
        commonmark_js: &'static str,
        /// micromark's rendering where it differs from commonmark.js's.
        micromark: Option<&'static str>,
    }

    const CASES: &[Case] = &[
        Case {
            name: "item indentation measured from the quote's content",
            source: "  > - a\n>   ===\nb",
            commonmark_js: "<blockquote>\n<ul>\n<li>\n<h1>a</h1>\n</li>\n</ul>\n</blockquote>\n<p>b</p>",
            micromark: None,
        },
        Case {
            name: "quote marker indented on a later line",
            source: ">- >a\n > >\nb",
            commonmark_js: "<blockquote>\n<ul>\n<li>\n<blockquote>\n<p>a</p>\n</blockquote>\n</li>\n</ul>\n<blockquote>\n</blockquote>\n</blockquote>\n<p>b</p>",
            micromark: None,
        },
        Case {
            name: "setext-like line past a quoted item",
            source: "   > q\n  > - c\n>    ===\n    :::d",
            commonmark_js: "<blockquote>\n<p>q</p>\n<ul>\n<li>\n<h1>c</h1>\n</li>\n</ul>\n</blockquote>\n<pre><code>:::d\n</code></pre>",
            micromark: None,
        },
        Case {
            name: "fence in nested items behind a quote",
            source: "   > - - ```\n>     code\n> c",
            commonmark_js: "<blockquote>\n<ul>\n<li>\n<ul>\n<li>\n<pre><code>code\n</code></pre>\n</li>\n</ul>\n</li>\n</ul>\n<p>c</p>\n</blockquote>",
            micromark: None,
        },
        Case {
            name: "nested item behind an inner quote",
            source: "> - > - a\n>   > 2.\nz",
            commonmark_js: "<blockquote>\n<ul>\n<li>\n<blockquote>\n<ul>\n<li>a</li>\n</ul>\n<ol start=\"2\">\n<li></li>\n</ol>\n</blockquote>\n</li>\n</ul>\n</blockquote>\n<p>z</p>",
            micromark: None,
        },
        Case {
            name: "setext-like line in an inner quote",
            source: "> - - > a\n>   > ===\nc",
            commonmark_js: "<blockquote>\n<ul>\n<li>\n<ul>\n<li>\n<blockquote>\n<p>a</p>\n</blockquote>\n</li>\n</ul>\n<blockquote>\n<p>===\nc</p>\n</blockquote>\n</li>\n</ul>\n</blockquote>",
            micromark: None,
        },
        Case {
            name: "a quote marker four columns in is text",
            source: "- - >=\n\t\t>```\n=",
            commonmark_js: "<ul>\n<li>\n<ul>\n<li>\n<blockquote>\n<p>=\n&gt;```\n=</p>\n</blockquote>\n</li>\n</ul>\n</li>\n</ul>",
            micromark: None,
        },
        Case {
            name: "item continuation indented four columns more",
            source: ">- >a\n>     >- =\n=",
            commonmark_js: "<blockquote>\n<ul>\n<li>\n<blockquote>\n<p>a</p>\n<ul>\n<li>=\n=</li>\n</ul>\n</blockquote>\n</li>\n</ul>\n</blockquote>",
            micromark: None,
        },
        Case {
            name: "item continuation indented four columns more, spaced",
            source: "> - >a\n>     >- =\n=",
            commonmark_js: "<blockquote>\n<ul>\n<li>\n<blockquote>\n<p>a</p>\n<ul>\n<li>=\n=</li>\n</ul>\n</blockquote>\n</li>\n</ul>\n</blockquote>",
            micromark: None,
        },
        Case {
            name: "tab-indented continuation of a nested quote",
            source: "- - >a\n\t  >1. c\n->",
            commonmark_js: "<ul>\n<li>\n<ul>\n<li>\n<blockquote>\n<p>a</p>\n<ol>\n<li>c\n-&gt;</li>\n</ol>\n</blockquote>\n</li>\n</ul>\n</li>\n</ul>",
            micromark: None,
        },
        Case {
            name: "open fence in a nested item is not trusted after it closes",
            source: "- 1. c\n  2. ```\n  \tx",
            commonmark_js: "<ul>\n<li>\n<ol>\n<li>c</li>\n<li>\n<pre><code></code></pre>\n</li>\n</ol>\nx</li>\n</ul>",
            micromark: None,
        },
        Case {
            name: "open tilde fence in a nested item",
            source: "2) 2. c\n   0. ~~~\n   \ty",
            commonmark_js: "<ol start=\"2\">\n<li>\n<ol start=\"2\">\n<li>c</li>\n<li>\n<pre><code></code></pre>\n</li>\n</ol>\ny</li>\n</ol>",
            micromark: None,
        },
        Case {
            name: "setext and fence checks inside an item",
            source: "1. 0. ```\n   - a\n       -\n->",
            commonmark_js: "<ol>\n<li>\n<ol start=\"0\">\n<li>\n<pre><code></code></pre>\n</li>\n</ol>\n<ul>\n<li>\n<h2>a</h2>\n</li>\n</ul>\n</li>\n</ol>\n<p>-&gt;</p>",
            micromark: None,
        },
        Case {
            name: "three tabs after a quote marker",
            source: ">\t\t\tfoo",
            commonmark_js: "<blockquote>\n<pre><code>  \tfoo\n</code></pre>\n</blockquote>",
            micromark: None,
        },
        Case {
            name: "tabs inside a quoted fence",
            source: "> ```\n>\t\tcode\n> ```",
            commonmark_js: "<blockquote>\n<pre><code>  \tcode\n</code></pre>\n</blockquote>",
            micromark: None,
        },
        Case {
            name: "tabs inside an indented fence",
            source: "  ```\n\t\tx\n  ```",
            commonmark_js: "<pre><code>  \tx\n</code></pre>",
            micromark: None,
        },
        Case {
            name: "tabs after an item marker",
            source: "-\t\t\tcode",
            commonmark_js: "<ul>\n<li>\n<pre><code>  \tcode\n</code></pre>\n</li>\n</ul>",
            micromark: None,
        },
        Case {
            name: "blank line inside a nested fence",
            source: "- a\n  - ```\n\n    x\n\n- b",
            commonmark_js: "<ul>\n<li>a\n<ul>\n<li>\n<pre><code>\nx\n\n</code></pre>\n</li>\n</ul>\n</li>\n<li>b</li>\n</ul>",
            micromark: Some("<ul>\n<li>a\n<ul>\n<li>\n<pre><code>x\n</code></pre>\n</li>\n</ul>\n</li>\n<li>b</li>\n</ul>"),
        },
        Case {
            name: "blank line inside an HTML comment in an item",
            source: "\n\n- <!--\n\n- text",
            commonmark_js: "<ul>\n<li>\n<!--\n\n</li>\n<li>text</li>\n</ul>",
            micromark: None,
        },
        Case {
            name: "blank lines of an unclosed fence before a paragraph",
            source: "- ```\n\n\nb",
            commonmark_js: "<ul>\n<li>\n<pre><code>\n\n</code></pre>\n</li>\n</ul>\n<p>b</p>",
            micromark: None,
        },
        Case {
            name: "trailing blank line of an unclosed fence",
            source: "- ```\n  x\n\n- b",
            commonmark_js: "<ul>\n<li>\n<pre><code>x\n\n</code></pre>\n</li>\n<li>b</li>\n</ul>",
            micromark: Some("<ul>\n<li>\n<pre><code>x\n\n\n</code></pre>\n</li>\n<li>b</li>\n</ul>"),
        },
        Case {
            name: "unclosed fence in a quote before a lazy line",
            source: "> ```\n> a\n>\nb",
            commonmark_js: "<blockquote>\n<pre><code>a\n\n</code></pre>\n</blockquote>\n<p>b</p>",
            micromark: None,
        },
        Case {
            name: "lazy line from an outer quote does not enter a fence",
            source: "> - ```\n>   x\n  y",
            commonmark_js: "<blockquote>\n<ul>\n<li>\n<pre><code>x\n</code></pre>\n</li>\n</ul>\n</blockquote>\n<p>y</p>",
            micromark: None,
        },
        Case {
            name: "dedented lazy line opens no block",
            source: "   - > q\n    1. a\n1.  ```",
            commonmark_js: "<ul>\n<li>\n<blockquote>\n<p>q\n1. a</p>\n</blockquote>\n</li>\n</ul>\n<ol>\n<li>\n<pre><code></code></pre>\n</li>\n</ol>",
            micromark: None,
        },
        Case {
            name: "HTML block on an item's continuation line in a quote",
            source: "> - <div>\n>   <!--\n> - x\n>   y\nz",
            commonmark_js: "<blockquote>\n<ul>\n<li>\n<div>\n<!--\n</li>\n<li>x\ny\nz</li>\n</ul>\n</blockquote>",
            micromark: None,
        },
        Case {
            name: "sibling item ends a fence behind a quote",
            source: "- > - ```\n  > - b\nc",
            commonmark_js: "<ul>\n<li>\n<blockquote>\n<ul>\n<li>\n<pre><code></code></pre>\n</li>\n<li>b\nc</li>\n</ul>\n</blockquote>\n</li>\n</ul>",
            micromark: None,
        },
    ];

    /// Tables are part of the syntax, so a delimiter row in an item makes its
    /// line a table header, where commonmark.js, without tables, reads
    /// `"<ul>\n<li>a\n|-|\nx</li>\n</ul>"`.
    #[test]
    fn a_delimiter_row_in_an_item_forms_a_table() {
        let html = parse("- a\n  |-|\nx").document.to_html().unwrap();
        assert_eq!(
            html.trim_end(),
            "<ul>\n<li>\n<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n</li>\n</ul>\n<p>x</p>"
        );
        // commonmark.js reads `"<blockquote>\n<ul>\n<li>z\n|-|\n$$</li>\n</ul>\n</blockquote>"`.
        let html = parse("> - z\n>   |-|\n>$$").document.to_html().unwrap();
        assert_eq!(
            html.trim_end(),
            concat!(
                "<blockquote>\n<ul>\n<li>\n<table>\n<thead>\n<tr>\n<th>z</th>\n</tr>\n</thead>\n</table>\n</li>\n</ul>\n",
                "<pre><code class=\"language-math\" data-math-style=\"display\"></code></pre>\n</blockquote>"
            )
        );
    }

    #[test]
    fn nested_containers_match_the_reference() {
        let mut options = HtmlOptions::default();
        options.allow_dangerous_html = true;
        for case in CASES {
            let document = parse(case.source).document;
            let html = document
                .to_html_with(&options)
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
            assert_eq!(
                html.trim_end(),
                case.commonmark_js,
                "{} ({:?}); micromark: {:?}",
                case.name,
                case.source,
                case.micromark
            );
        }
    }
}

mod one_pass_over_open_blocks {
    //! Block structure read in one pass over one stack of open blocks: the
    //! block-syntax "One pass over open blocks", indentation, tab, and blank
    //! line scenarios.

    use markdown_syntax::{parse, Block, CodeBlockKind, HeadingKind, Inline, List, ListItem};

    fn blocks_of(source: &str) -> Vec<Block> {
        parse(source).document.children
    }

    fn code_value(block: &Block) -> &str {
        match block {
            Block::CodeBlock(code) => &code.value,
            other => panic!("expected a code block, got {other:?}"),
        }
    }

    #[test]
    fn an_unclosed_fence_keeps_an_empty_last_line_without_a_line_ending() {
        // commonmark.js gives `a\n\n` for each.
        assert_eq!(code_value(&blocks_of("  ```\na\n  ")[0]), "a\n\n");
        assert_eq!(
            code_value(&only_item(&blocks_of("- ```\n  a\n  ")[0]).children[0]),
            "a\n\n"
        );
        assert_eq!(code_value(&blocks_of("```\r\na\r\n")[0]), "a\r\n");
    }

    #[test]
    fn the_task_checkbox_follows_cmark_gfm_and_micromark() {
        let gfm = |source: &str| parse(source).document.children;
        // A setext heading is no paragraph, so `[x]` stays its text.
        let blocks = gfm("- [x] a\n  ===");
        let item = only_item(&blocks[0]);
        assert_eq!(item.checked, None);
        assert!(
            matches!(&item.children[0], Block::Heading(heading) if heading.kind == HeadingKind::Setext)
        );
        // The checkbox starts the paragraph left after its definitions.
        let blocks = gfm("- [a]: /u\n  [x] b");
        let item = only_item(&blocks[0]);
        assert_eq!(item.checked, Some(true));
        assert!(matches!(
            item.children.as_slice(),
            [Block::Definition(_), Block::Paragraph(_)]
        ));
        // The whitespace after the checkbox's space stays text.
        let blocks = gfm("- [x]   b");
        assert_eq!(paragraph(&only_item(&blocks[0]).children[0]), ["  b"]);
    }

    #[test]
    fn a_definition_title_drops_the_indentation_of_its_lines() {
        let blocks = blocks_of("[a]: /u \"x\n   y\"\n\n[a]");
        let Block::Definition(definition) = &blocks[0] else {
            panic!("expected a definition, got {blocks:?}");
        };
        assert_eq!(
            definition.title.as_ref().map(|title| title.value.as_str()),
            Some("x\ny")
        );
    }

    #[test]
    fn indented_code_does_not_interrupt_an_alert_marker_line() {
        for source in ["> [!NOTE]\n    code", "> [!NOTE]\n>     code"] {
            let blocks = parse(source).document.children;
            let [Block::Alert(alert)] = blocks.as_slice() else {
                panic!("{source:?}: expected one alert, got {blocks:?}");
            };
            assert_eq!(paragraph(&alert.children[0]), ["code"], "{source:?}");
        }
    }

    fn texts(inlines: &[Inline]) -> Vec<String> {
        inlines
            .iter()
            .map(|inline| match inline {
                Inline::Text(text) => text.value.clone(),
                Inline::SoftBreak(_) => "\n".into(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    fn list(block: &Block) -> &List {
        match block {
            Block::List(list) => list,
            other => panic!("expected a list, got {other:?}"),
        }
    }

    fn quote(block: &Block) -> &[Block] {
        match block {
            Block::BlockQuote(quote) => &quote.children,
            other => panic!("expected a block quote, got {other:?}"),
        }
    }

    fn only_item(block: &Block) -> &ListItem {
        let list = list(block);
        assert_eq!(list.children.len(), 1, "{list:?}");
        &list.children[0]
    }

    fn paragraph(block: &Block) -> Vec<String> {
        match block {
            Block::Paragraph(paragraph) => texts(&paragraph.children),
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    fn code(block: &Block) -> (&CodeBlockKind, &str) {
        match block {
            Block::CodeBlock(code) => (&code.kind, code.value.as_str()),
            other => panic!("expected a code block, got {other:?}"),
        }
    }

    #[test]
    fn item_indentation_is_measured_from_the_quotes_content() {
        let blocks = blocks_of("  > - a\n>   ===\nb");
        let [quoted, after] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        let item = only_item(&quote(quoted)[0]);
        let [Block::Heading(heading)] = item.children.as_slice() else {
            panic!("{item:?}");
        };
        assert_eq!(
            (heading.depth, heading.kind, texts(&heading.children)),
            (1, HeadingKind::Setext, vec!["a".to_owned()])
        );
        assert_eq!(paragraph(after), ["b"]);
    }

    #[test]
    fn a_nested_item_behind_an_inner_quote_ends_with_the_outer_quote() {
        let blocks = blocks_of("> - > - a\n>   > 2.\nz");
        assert!(matches!(blocks[0], Block::BlockQuote(_)), "{blocks:?}");
        assert_eq!(paragraph(&blocks[1]), ["z"]);
    }

    #[test]
    fn a_quote_marker_four_columns_in_is_text() {
        let blocks = blocks_of("- - >=\n\t\t>```\n=");
        let outer = only_item(&blocks[0]);
        let inner = only_item(&outer.children[0]);
        let inner_quote = quote(&inner.children[0]);
        assert_eq!(paragraph(&inner_quote[0]), ["=", "\n", ">```", "\n", "="]);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
    }

    #[test]
    fn item_continuation_indented_four_columns_more_continues_the_paragraph() {
        let blocks = blocks_of("> - >a\n>     >- =\n=");
        let item = only_item(&quote(&blocks[0])[0]);
        let inner_quote = quote(&item.children[0]);
        let innermost = only_item(&inner_quote[1]);
        assert_eq!(paragraph(&innermost.children[0]), ["=", "\n", "="]);
    }

    #[test]
    fn an_open_fence_in_a_nested_item_is_not_trusted_after_it_closes() {
        let blocks = blocks_of("- 1. c\n  2. ```\n  \tx");
        let item = only_item(&blocks[0]);
        assert!(matches!(item.children[0], Block::List(_)), "{item:?}");
        assert_eq!(paragraph(&item.children[1]), ["x"]);
        assert!(
            !format!("{blocks:?}").contains("Indented"),
            "no indented code: {blocks:?}"
        );
    }

    #[test]
    fn a_lazy_line_from_an_outer_quote_does_not_enter_a_fence() {
        let blocks = blocks_of("> - ```\n>   x\n  y");
        let item = only_item(&quote(&blocks[0])[0]);
        assert!(matches!(
            code(&item.children[0]),
            (CodeBlockKind::Fenced { .. }, "x\n")
        ));
        assert_eq!(paragraph(&blocks[1]), ["y"]);
    }

    #[test]
    fn a_dedented_lazy_line_opens_no_block() {
        let blocks = blocks_of("   - > q\n    1. a\n1.  ```");
        let item = only_item(&blocks[0]);
        assert_eq!(paragraph(&quote(&item.children[0])[0]), ["q", "\n", "1. a"]);
    }

    #[test]
    fn an_html_block_on_a_quoted_items_continuation_line_ends_at_the_next_item() {
        let blocks = blocks_of("> - <div>\n>   <!--\n> - x\n>   y\nz");
        let items = &list(&quote(&blocks[0])[0]).children;
        assert_eq!(
            paragraph(&items[1].children[0]),
            ["x", "\n", "y", "\n", "z"]
        );
    }

    #[test]
    fn a_sibling_item_ends_a_fence_behind_a_quote() {
        let blocks = blocks_of("- > - ```\n  > - b\nc");
        let item = only_item(&blocks[0]);
        let nested = &list(&quote(&item.children[0])[0]).children;
        assert_eq!(paragraph(&nested[1].children[0]), ["b", "\n", "c"]);
    }

    #[test]
    fn block_extensions_indent_at_most_three_columns() {
        for (source, value) in [("    [^1]: x", "[^1]: x\n"), ("    ::name", "::name\n")] {
            let blocks = parse(source).document.children;
            let [block] = blocks.as_slice() else {
                panic!("{source:?}: {blocks:?}");
            };
            assert_eq!(code(block), (&CodeBlockKind::Indented, value), "{source:?}");
        }
    }

    #[test]
    fn tabs_after_a_split_tab_keep_their_columns() {
        let blocks = blocks_of(">\t\t\tfoo");
        assert_eq!(
            code(&quote(&blocks[0])[0]),
            (&CodeBlockKind::Indented, "  \tfoo\n")
        );
        let blocks = blocks_of("> ```\n>\t\tcode\n> ```");
        assert_eq!(code(&quote(&blocks[0])[0]).1, "  \tcode\n");
    }

    #[test]
    fn blank_lines_inside_open_leaf_blocks_loosen_no_list() {
        for source in ["- a\n  - ```\n\n    x\n\n- b", "\n\n- <!--\n\n- text"] {
            assert!(list(&blocks_of(source)[0]).tight, "{source:?}");
        }
        assert!(list(&parse("- $$\n\n- b").document.children[0]).tight);
    }

    #[test]
    fn an_unclosed_fence_keeps_its_trailing_blank_lines() {
        let blocks = blocks_of("- ```\n  x\n\n- b");
        assert_eq!(code(&list(&blocks[0]).children[0].children[0]).1, "x\n\n");
        let blocks = blocks_of("> ```\n> a\n>\nb");
        assert_eq!(code(&quote(&blocks[0])[0]).1, "a\n\n");
    }

    #[test]
    fn container_lines_follow_the_open_blocks() {
        let blocks = blocks_of("2. a\n   1. ```\n\n2. b");
        assert!(blocks.len() == 1 && list(&blocks[0]).tight, "{blocks:?}");

        let blocks = blocks_of("> - a\n> 2.\nz");
        let inner = quote(&blocks[0]);
        assert!(
            matches!(inner, [Block::List(_), Block::List(_)]),
            "{inner:?}"
        );
        assert_eq!(paragraph(&blocks[1]), ["z"]);

        let blocks = blocks_of("> 1. a\n> ===\nb");
        let item = only_item(&quote(&blocks[0])[0]);
        assert_eq!(paragraph(&item.children[0]), ["a", "\n", "===", "\n", "b"]);

        assert!(!list(&blocks_of("- - ```\n  - a\n\n- b")[0]).tight);

        let blocks = blocks_of("1.   a\n    ```\n\nb");
        assert!(matches!(blocks[0], Block::List(_)), "{blocks:?}");
        assert_eq!(paragraph(&blocks[1]), ["b"]);

        // A delimiter row makes the item's line a table header, and the
        // line after the table is no lazy continuation.
        let blocks = blocks_of("- a\n  |-|\nx");
        assert!(
            matches!(only_item(&blocks[0]).children.as_slice(), [Block::Table(_)]),
            "{blocks:?}"
        );
        assert_eq!(paragraph(&blocks[1]), ["x"]);
    }

    #[test]
    fn an_alerts_paragraph_takes_lazy_lines() {
        let blocks = parse("> [!NOTE]\n> a\n===").document.children;
        let [Block::Alert(alert)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        let [only] = alert.children.as_slice() else {
            panic!("{alert:?}");
        };
        assert_eq!(paragraph(only), ["a", "\n", "==="]);
    }

    #[test]
    fn a_quoted_footnote_definition_takes_lazy_lines() {
        let blocks = parse("> [^1]: a\nb\n\nx[^1]").document.children;
        let [Block::FootnoteDefinition(definition)] = quote(&blocks[0]) else {
            panic!("{blocks:?}");
        };
        assert_eq!(paragraph(&definition.children[0]), ["a", "\n", "b"]);
    }
}

mod directive_containers {
    //! Directive scenarios of the block-syntax spec: leaf directives stand
    //! alone, and a closing fence closes the innermost directive it can.

    use markdown_syntax::{parse, Block, DiagnosticCode, Inline};

    fn texts(inlines: &[Inline]) -> Vec<String> {
        inlines
            .iter()
            .map(|inline| match inline {
                Inline::Text(text) => text.value.clone(),
                Inline::SoftBreak(_) => "\n".into(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn text_after_a_leaf_directive_makes_the_line_paragraph_text() {
        let blocks = parse("x\n::a b").document.children;
        let [Block::Paragraph(paragraph)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        assert_eq!(texts(&paragraph.children), ["x", "\n", "::a b"]);
    }

    #[test]
    fn nested_directives_close_innermost_first() {
        let blocks = parse(":::outer\n:::inner\nx\n:::\n:::\nafter")
            .document
            .children;
        let [Block::ContainerDirective(outer), Block::Paragraph(after)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        let [Block::ContainerDirective(inner)] = outer.children.as_slice() else {
            panic!("{outer:?}");
        };
        assert_eq!(
            (outer.name.as_str(), inner.name.as_str()),
            ("outer", "inner")
        );
        assert!(matches!(inner.children.as_slice(), [Block::Paragraph(_)]));
        assert_eq!(texts(&after.children), ["after"]);
    }

    #[test]
    fn a_directive_opener_inside_an_html_block_is_its_content() {
        let output = parse(":::e\n<y>\n:::e");
        let [Block::ContainerDirective(directive)] = output.document.children.as_slice() else {
            panic!("{:?}", output.document);
        };
        let [Block::HtmlBlock(html)] = directive.children.as_slice() else {
            panic!("{directive:?}");
        };
        assert_eq!(html.value, "<y>\n:::e");
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::UnclosedDirectiveContainer));
    }

    #[test]
    fn a_closing_fence_like_line_inside_an_items_fence_is_code() {
        let blocks = parse(":::t\n- ```\n  :::e\n  ```\n:::\nafter")
            .document
            .children;
        let [Block::ContainerDirective(directive), Block::Paragraph(after)] = blocks.as_slice()
        else {
            panic!("{blocks:?}");
        };
        let [Block::List(list)] = directive.children.as_slice() else {
            panic!("{directive:?}");
        };
        let [Block::CodeBlock(code)] = list.children[0].children.as_slice() else {
            panic!("{list:?}");
        };
        assert_eq!(code.value, ":::e\n");
        assert_eq!(texts(&after.children), ["after"]);
    }

    #[test]
    fn a_leaf_directive_in_an_item_leaves_later_lines_lazy() {
        let blocks = parse("- ::name[x]\n  foo\nbar").document.children;
        let [Block::List(list)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        let [Block::LeafDirective(_), Block::Paragraph(paragraph)] =
            list.children[0].children.as_slice()
        else {
            panic!("{list:?}");
        };
        assert_eq!(texts(&paragraph.children), ["foo", "\n", "bar"]);
    }
}

mod directive_attributes {
    //! An attribute without a valid name is dropped and reported.

    use markdown_syntax::{parse, Block, DiagnosticCode, DiagnosticSeverity, Span};

    fn dropped(source: &str) -> Vec<Span> {
        parse(source)
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::InvalidDirectiveAttribute)
            .map(|diagnostic| {
                assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
                diagnostic.span.expect("a parser diagnostic has a span")
            })
            .collect()
    }

    #[test]
    fn a_text_directives_nameless_attributes_are_reported() {
        let source = ":b{<} :c{a <=1 d}";
        let document = parse(source).document;
        assert_eq!(document.to_markdown().unwrap(), ":b :c{a d}\n");
        let spans = dropped(source);
        let texts: Vec<&str> = spans
            .iter()
            .map(|span| &source[span.start..span.end])
            .collect();
        assert_eq!(texts, ["<", "<=1"]);
    }

    #[test]
    fn a_leaf_directives_dotted_and_colon_led_names_are_reported() {
        let source = "::a{x.y=1 :b=2 data-x=3}";
        let blocks = parse(source).document.children;
        let [Block::LeafDirective(directive)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        let names: Vec<&str> = directive
            .attributes
            .iter()
            .map(|attribute| attribute.name.as_str())
            .collect();
        assert_eq!(names, ["data-x"]);
        let spans = dropped(source);
        let texts: Vec<&str> = spans
            .iter()
            .map(|span| &source[span.start..span.end])
            .collect();
        assert_eq!(texts, ["x.y=1", ":b=2"]);
    }

    #[test]
    fn a_container_directives_nameless_attribute_is_reported() {
        let source = ":::a{=x}\nbody\n:::";
        let spans = dropped(source);
        let texts: Vec<&str> = spans
            .iter()
            .map(|span| &source[span.start..span.end])
            .collect();
        assert_eq!(texts, ["=x"]);
    }
}

mod serialized_spellings {
    //! The values that the spellings the serializer writes parse to: titles
    //! and their kinds, labels and identifiers as written, and values whose
    //! whitespace is written as character references.

    use markdown_syntax::prelude::*;

    #[test]
    fn empty_resource_titles_keep_their_kind() {
        let input = concat!(
            "[a](/u \"\")\n\n",
            "[b](/u '')\n\n",
            "[c](/u ())\n\n",
            "[](<> \"\")\n\n",
            "[d]: /u \"\"\n",
        );
        let document = parse(input).document;
        let link_titles = document
            .children
            .iter()
            .filter_map(|block| match block {
                Block::Paragraph(Paragraph { children, .. }) => match &children[..] {
                    [Inline::Link(link)] => link.title.clone(),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            link_titles,
            vec![
                Title::new("", LinkTitleKind::DoubleQuote),
                Title::new("", LinkTitleKind::SingleQuote),
                Title::new("", LinkTitleKind::Paren),
                Title::new("", LinkTitleKind::DoubleQuote),
            ]
        );
    }

    #[test]
    fn definition_labels_are_read_as_written() {
        for (markdown, label, identifier) in [
            ("[line\nbreak]: /u\n", "line\nbreak", "line break"),
            ("[a\\]b\\\\c\\[d]: /u\n", "a\\]b\\\\c\\[d", "a\\]b\\\\c\\[d"),
        ] {
            match &parse(markdown).document.children[..] {
                [Block::Definition(definition)] => {
                    assert_eq!(definition.label, label);
                    assert_eq!(definition.identifier, identifier);
                }
                other => panic!("unexpected document shape: {other:?}"),
            }
        }
    }

    #[test]
    fn footnote_identifiers_are_the_raw_label() {
        // A footnote reference and its definition fold identically, and the
        // identifier keeps the escaped or entity-encoded spelling.
        let input = "See [^a\\]b\\\\c\\[d] and [^white&#x20;space]\n\n[^a\\]b\\\\c\\[d]: bracket\n\n[^white&#x20;space]: space\n";
        let identifiers: Vec<String> = parse(input)
            .document
            .children
            .iter()
            .filter_map(|block| match block {
                Block::FootnoteDefinition(definition) => Some(definition.identifier.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(identifiers, ["a\\]b\\\\c\\[d", "white&#x20;space"]);
    }

    #[test]
    fn references_at_the_ends_of_an_info_string_are_its_whitespace() {
        let document = parse("``` &#x20;a&#x9;\nb\n```\n").document;
        let [Block::CodeBlock(code)] = document.children.as_slice() else {
            panic!("{document:?}");
        };
        assert_eq!(code.info.as_deref(), Some(" a\t"));
    }

    #[test]
    fn a_referenced_space_is_a_bare_destination() {
        let document = parse("[o]: &#x20;\n").document;
        let [Block::Definition(definition)] = document.children.as_slice() else {
            panic!("{document:?}");
        };
        assert_eq!(definition.destination, " ");
        assert_eq!(definition.destination_kind, LinkDestinationKind::Bare);
    }

    #[test]
    fn an_html_block_value_ends_at_its_last_line() {
        let document = parse("<!--\n\n").document;
        let [Block::HtmlBlock(html)] = document.children.as_slice() else {
            panic!("{document:?}");
        };
        assert_eq!(html.value, "<!--\n");
    }
}
