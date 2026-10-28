//! `Document::inline_source`: the requirement "Inline source of a leaf
//! block" (`docs/specs/public-api.md`). Every leaf of the read-back corpus
//! and of the seeded generators is checked in `fixtures.rs` and
//! `serialize_roundtrip_fuzz.rs`.

#[path = "support/leaves.rs"]
mod leaves;

use markdown_syntax::prelude::*;

fn inline_source(source: &str, path: &[usize]) -> Option<InlineSource> {
    parse(source).document.inline_source(&NodePath::from(path))
}

mod inline_source_of_a_leaf_block {
    use super::*;

    #[test]
    fn paragraph_in_a_block_quote() {
        let source = inline_source("> a *b*\n> c", &[0, 0]).unwrap();
        assert_eq!(source.text, "a *b*\nc");
        assert_eq!(source.to_source(6), 10);
    }

    #[test]
    fn atx_heading() {
        let source = inline_source("## T *x* ##", &[0]).unwrap();
        assert_eq!(source.text, "T *x*");
        assert_eq!(source.to_source(0), 3);
        assert_eq!(source.to_source(source.text.len()), 8);
    }

    #[test]
    fn paragraph_in_a_list_item() {
        let source = inline_source("- a\n  *b*", &[0, 0, 0]).unwrap();
        assert_eq!(source.text, "a\n*b*");
        assert_eq!(source.to_source(2), 6);
        assert_eq!(source.span_to_source(Span::new(2, 5)), Span::new(6, 9));
    }

    #[test]
    fn heading_and_later_blocks() {
        let input = "\u{feff}a\n\n# T\r\n\n> - x\n>   y";
        let source = inline_source(input, &[1]).unwrap();
        assert_eq!(source.text, "T");
        assert_eq!(source.to_source(0), 8);
        let source = inline_source(input, &[2, 0, 0, 0]).unwrap();
        assert_eq!(source.text, "x\ny");
        assert_eq!(source.to_source(2), input.len() - 1);
        assert_eq!(leaves::leaves_reproduce(input), Ok(3));
    }

    #[test]
    fn a_paragraph_opened_by_definitions_reads_its_continuation_lines() {
        let input = "[a]: /u\n\t b [a]";
        let source = inline_source(input, &[1]).unwrap();
        assert_eq!(source.text, "b [a]");
        assert_eq!(leaves::leaves_reproduce(input), Ok(1));
    }

    #[test]
    fn a_table_header_row_that_continues_a_paragraph() {
        // Each header row is a line the paragraph or definitions before it
        // read as a continuation, where it cannot open a block; at the start
        // of a document it would open an item, a definition, or HTML.
        for (input, path, header, at) in [
            ("a\n2) b | c\n--|--", &[1, 0, 0][..], "2) b", 2),
            ("a\n10. b | c\n--|--", &[1, 0, 0], "10. b", 2),
            ("--2. \n3) \n|-", &[1, 0, 0], "3)", 6),
            ("a\n- \n|-", &[1, 0, 0], "-", 2),
            ("a\n* \n|-", &[1, 0, 0], "*", 2),
            ("a\n+ \n|-", &[1, 0, 0], "+", 2),
            ("a\n1. \n|-", &[1, 0, 0], "1.", 2),
            ("a\n[x]: /u|c\n-|-", &[1, 0, 0], "[x]: /u", 2),
            ("a\n<x>\n|-", &[1, 0, 0], "<x>", 2),
            ("a\r\n2) b | c\r\n--|--", &[1, 0, 0], "2) b", 3),
            ("a\r2) b | c\r--|--", &[1, 0, 0], "2) b", 2),
            ("[a]: /u\n2) c | d\n--|--", &[1, 0, 0], "2) c", 8),
            // The paragraph is a continuation of the definitions before it.
            ("[a]: /u\nb\n2) c | d\n--|--", &[2, 0, 0], "2) c", 10),
            ("[a]: /u\n    b\n2) c | d\n--|--", &[2, 0, 0], "2) c", 14),
        ] {
            let source = inline_source(input, path)
                .unwrap_or_else(|| panic!("{input:?} {path:?}: no inline source"));
            assert_eq!((source.text.as_str(), source.to_source(0)), (header, at));
            assert!(leaves::leaves_reproduce(input).is_ok(), "{input:?}");
        }
        // After a blank line the row opens a list, as it does read alone.
        let input = "a\n\n2) b | c\n--|--";
        assert_eq!(
            inline_source(input, &[1, 0, 0]).unwrap().text,
            "b | c\n--|--"
        );
        assert_eq!(leaves::leaves_reproduce(input), Ok(2));
    }

    /// Every document of up to four lines drawn from lines that open a
    /// paragraph or definition, lines that only continue one, and table
    /// delimiter rows reads each leaf's inline source as `leaf_sources` does.
    #[test]
    fn leaves_after_paragraphs_and_definitions_reproduce() {
        const LINES: &[&str] = &[
            "a",
            "[a]: /u",
            "[b]:",
            "/v",
            "    b",
            "b | c",
            "2) b | c",
            "10. b",
            "- ",
            "1. ",
            "<x>",
            "[x]: /u|c",
            "> a",
            "- a",
            "# h",
            "```",
            "===",
            "",
            "|-",
            "-|-",
        ];
        let mut inputs: Vec<String> = LINES.iter().map(|line| (*line).to_string()).collect();
        let mut failures = Vec::new();
        for lines in 1..=4 {
            if lines > 1 {
                inputs = inputs
                    .iter()
                    .flat_map(|input| LINES.iter().map(move |line| format!("{input}\n{line}")))
                    .collect();
            }
            for input in &inputs {
                if let Err(failure) = leaves::leaves_reproduce(input) {
                    failures.push(failure);
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} failures:\n{}",
            failures.len(),
            failures[..failures.len().min(40)].join("\n")
        );
    }

    /// A frontmatter fence opens frontmatter only on the document's first
    /// line; a block read again from a later line reads its fence as the
    /// whole document does.
    #[test]
    fn a_fence_after_the_first_line_is_not_frontmatter() {
        for (input, path, text) in [
            ("x\n\n+++\na\n+++", [1], "+++\na\n+++"),
            ("x\n\n+++\na\n+++\n", [1], "+++\na\n+++"),
            ("\u{feff}x\n\n+++\na\n+++", [1], "+++\na\n+++"),
            ("+++\na\n+++\n\nb", [1], "b"),
            ("\u{feff}+++\na\n+++\n\nb", [1], "b"),
        ] {
            assert_eq!(
                inline_source(input, &path).map(|source| source.text),
                Some(String::from(text)),
                "{input:?}"
            );
            assert!(leaves::leaves_reproduce(input).is_ok(), "{input:?}");
        }
    }

    #[test]
    fn table_cell() {
        let input = "| h |\n| - |\n| a\\|b *c* |";
        let source = inline_source(input, &[0, 1, 0]).unwrap();
        assert_eq!(source.text, "a|b *c*");
        assert_eq!(source.span_to_source(Span::new(1, 2)), Span::new(15, 17));
        assert_eq!(source.to_source(3), 18);
        assert_eq!(leaves::leaves_reproduce(input), Ok(2));
    }

    #[test]
    fn directive_labels() {
        let input = "::leaf[a *b*]

:::box[c]
d
:::";
        assert_eq!(inline_source(input, &[0]).unwrap().text, "a *b*");
        let label = inline_source(input, &[1]).unwrap();
        assert_eq!(label.text, "c");
        assert_eq!(label.to_source(0), 22);
        assert_eq!(inline_source(input, &[1, 0]).unwrap().text, "d");
        assert_eq!(leaves::leaves_reproduce(input), Ok(3));
    }

    #[test]
    fn none_where_there_is_no_leaf_or_source() {
        assert!(inline_source("- a", &[0]).is_none());
        assert!(inline_source("- a", &[0, 1, 0]).is_none());
        assert!(inline_source("a", &[1]).is_none());
        assert!(inline_source("a", &[]).is_none());
        assert!(inline_source("| a |\n| - |", &[0, 0]).is_none());
        assert!(inline_source("| a |\n| - |", &[0, 0, 0, 0]).is_none());
        assert!(inline_source(":::d\nx\n:::", &[0]).is_none());
        assert!(parse("a")
            .document
            .without_source()
            .inline_source(&NodePath::from([0]))
            .is_none());
        let document = Document::new(vec![Paragraph::new([Text::from("a")]).into()]);
        assert!(document.inline_source(&NodePath::from([0])).is_none());
    }

    #[test]
    fn leaves_with_references_and_nul_reproduce() {
        for input in [
            "[a]\n\n[a]: /u",
            "> [b][]\n>\n> [b]: /v",
            "a\0*b*\n\n# \0",
            "- a\n\n      code\n  b *c*\n",
            ":::d\n# h\n:::",
            "| a | b |\n|-|-|\n| `x\\|y` | [c] |\n\n[c]: /u",
            "> | a |\n> |---|\n> | b\n\n- ::x[*y*]",
            "[^n]: x *y*\n    z",
        ] {
            assert!(
                leaves::leaves_reproduce(input).is_ok_and(|count| count > 0),
                "{input:?}: {:?}",
                leaves::leaves_reproduce(input)
            );
        }
    }

    /// The top-level list of `document`.
    fn list(document: &mut Document, index: usize) -> &mut List {
        match &mut document.children[index] {
            Block::List(list) => list,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_leaf_of_an_edited_list_reads_from_the_whole_list() {
        // A paragraph moved into another item, which does not hold its lines.
        let mut moved = parse("- a\n- b *c*").document;
        let paragraph = list(&mut moved, 0).children[1].children.remove(0);
        list(&mut moved, 0).children[0].children.push(paragraph);
        let source = moved.inline_source(&NodePath::from([0, 0, 1])).unwrap();
        assert_eq!(source.text, "b *c*");
        assert_eq!((source.to_source(0), source.to_source(2)), (6, 8));

        // An item whose span is gone.
        let mut cleared = parse("- a\n- b *c*").document;
        list(&mut cleared, 0).children[1].meta.span = None;
        let source = cleared.inline_source(&NodePath::from([0, 1, 0])).unwrap();
        assert_eq!(source.text, "b *c*");
        assert_eq!((source.to_source(0), source.to_source(2)), (6, 8));

        // Items in another order each still hold their own lines.
        let mut swapped = parse("- a *x*\n- b *c*\n  d").document;
        list(&mut swapped, 0).children.swap(0, 1);
        let source = swapped.inline_source(&NodePath::from([0, 0, 0])).unwrap();
        assert_eq!(
            (source.text.as_str(), source.to_source(0)),
            ("b *c*\nd", 10)
        );
        let source = swapped.inline_source(&NodePath::from([0, 1, 0])).unwrap();
        assert_eq!((source.text.as_str(), source.to_source(0)), ("a *x*", 2));

        // An item moved from another list, whose lines the list does not hold.
        let mut joined = parse("- a\n\nx\n\n- b *c*").document;
        let item = list(&mut joined, 2).children.remove(0);
        list(&mut joined, 0).children.push(item);
        assert!(joined.inline_source(&NodePath::from([0, 1, 0])).is_none());
    }

    /// The table of `document` at `index`.
    fn table(document: &mut Document, index: usize) -> &mut Table {
        match &mut document.children[index] {
            Block::Table(table) => table,
            other => panic!("{other:?}"),
        }
    }

    /// Asserts that the leaf at `path` of `edited` reads as the leaf at
    /// `parsed_path` of `parse(input)` does in `leaf_sources`.
    fn reads_as_parsed(edited: &Document, path: &[usize], input: &str, parsed_path: &[usize]) {
        let parsed = parse(input).document.leaf_sources();
        let (_, expected) = parsed
            .iter()
            .find(|(at, _)| at.0 == parsed_path)
            .unwrap_or_else(|| panic!("{input:?} holds no leaf at {parsed_path:?}"));
        let read = edited
            .inline_source(&NodePath::from(path))
            .unwrap_or_else(|| panic!("{input:?} {path:?}: no inline source"));
        assert!(
            leaves::same_source(&read, expected),
            "{input:?} {path:?}: {read:?}, not {expected:?}"
        );
    }

    #[test]
    fn a_leaf_reads_as_parsed_whatever_the_other_top_level_blocks_become() {
        // The table's header row continues the paragraph the definition
        // opened, so the table is read from the definition's line, as it was
        // parsed, wherever the definition and the paragraph go.
        let input = "[d]: /u\n    a\n2) b | c\n--|--";
        let parsed = parse(input).document;
        let check = |edited: &Document, table_at: usize| {
            reads_as_parsed(edited, &[table_at, 0, 0], input, &[2, 0, 0]);
            reads_as_parsed(edited, &[table_at, 0, 1], input, &[2, 0, 1]);
        };

        let mut removed = parsed.clone();
        removed.children.remove(0);
        check(&removed, 1);
        reads_as_parsed(&removed, &[0], input, &[1]);

        let mut replaced = parsed.clone();
        replaced.children[0] = Paragraph::new([Text::from("x")]).into();
        check(&replaced, 2);
        assert!(replaced.inline_source(&NodePath::from([0])).is_none());

        let mut reordered = parsed.clone();
        reordered.children.reverse();
        check(&reordered, 0);
        reads_as_parsed(&reordered, &[1], input, &[1]);

        let mut inserted = parsed.clone();
        inserted
            .children
            .insert(2, Heading::new(1, [Text::from("h")]).into());
        inserted.children.insert(2, BlankLine::new().into());
        let paragraph = parse("e").document.children.remove(0);
        inserted.children.insert(0, paragraph);
        check(&inserted, 5);

        let mut alone = parsed.clone();
        alone.children.drain(..2);
        check(&alone, 0);
    }

    #[test]
    fn none_where_the_source_holds_no_such_leaf_in_that_block() {
        let input = "a\n2) b | c\n--|--\n\n> d";
        let parsed = parse(input).document;

        // A top-level block whose span no parsed top-level block has: read
        // alone, the table's header row would be a paragraph at the cell's
        // span holding `a\\|b`, where the cell holds `a|b`.
        let mut narrowed = parse("a\\|b\n|-").document;
        assert_eq!(
            narrowed
                .inline_source(&NodePath::from([0, 0, 0]))
                .unwrap()
                .text,
            "a|b"
        );
        table(&mut narrowed, 0).meta.span = Some(Span::new(0, 4));
        assert_eq!(
            table(&mut narrowed, 0).rows[0].cells[0].meta.span,
            Some(Span::new(0, 4))
        );
        assert!(narrowed.inline_source(&NodePath::from([0, 0, 0])).is_none());

        // A hand-built top-level block around a parsed leaf.
        let mut wrapped = parsed.clone();
        let paragraph = wrapped.children.remove(0);
        wrapped.children[0] = BlockQuote {
            meta: NodeMeta::default(),
            children: vec![paragraph],
        }
        .into();
        assert!(wrapped.inline_source(&NodePath::from([0, 0])).is_none());

        // A parsed leaf moved into another parsed top-level block.
        let mut moved = parsed.clone();
        let paragraph = moved.children.remove(0);
        let Block::BlockQuote(quote) = &mut moved.children[1] else {
            panic!("{:?}", moved.children);
        };
        quote.children.push(paragraph);
        assert!(moved.inline_source(&NodePath::from([1, 1])).is_none());
        reads_as_parsed(&moved, &[1, 0], input, &[2, 0]);
    }

    /// Every document of up to three lines drawn from the lines of
    /// `leaves_after_paragraphs_and_definitions_reproduce`, with its
    /// top-level blocks reversed and with each one alone, reads each leaf as
    /// `leaf_sources` reads it in the document as parsed, and as
    /// `leaf_sources` reads the edited document.
    #[test]
    fn leaves_of_reordered_and_lone_top_level_blocks_read_as_parsed() {
        const LINES: &[&str] = &[
            "a",
            "[a]: /u",
            "[b]:",
            "/v",
            "    b",
            "b | c",
            "2) b | c",
            "10. b",
            "- ",
            "1. ",
            "<x>",
            "[x]: /u|c",
            "> a",
            "- a",
            "# h",
            "```",
            "===",
            "",
            "|-",
            "-|-",
        ];
        let mut inputs: Vec<String> = LINES.iter().map(|line| (*line).to_string()).collect();
        let mut failures = Vec::new();
        for lines in 1..=3 {
            if lines > 1 {
                inputs = inputs
                    .iter()
                    .flat_map(|input| LINES.iter().map(move |line| format!("{input}\n{line}")))
                    .collect();
            }
            for input in &inputs {
                let parsed = parse(input).document;
                let leaves = parsed.leaf_sources();
                let count = parsed.children.len();
                let mut reversed = parsed.clone();
                reversed.children.reverse();
                let mut edits = vec![(reversed, (0..count).rev().collect::<Vec<_>>())];
                for index in 0..count {
                    let mut alone = parsed.clone();
                    alone.children = vec![alone.children.swap_remove(index)];
                    edits.push((alone, vec![index]));
                }
                for (edited, from) in &edits {
                    let mut read = 0;
                    for (path, expected) in &leaves {
                        let Some(at) = from.iter().position(|&index| index == path.0[0]) else {
                            continue;
                        };
                        let mut edited_path = path.clone();
                        edited_path.0[0] = at;
                        match edited.inline_source(&edited_path) {
                            Some(found) if leaves::same_source(&found, expected) => read += 1,
                            found => failures.push(format!(
                                "{input:?} {path:?} at {edited_path:?}: {found:?}, not {expected:?}"
                            )),
                        }
                    }
                    // `leaf_sources` reads the edited document as
                    // `inline_source` does.
                    let edited_leaves = edited.leaf_sources();
                    let agrees = edited_leaves.len() == read
                        && edited_leaves.iter().all(|(path, source)| {
                            edited
                                .inline_source(path)
                                .is_some_and(|found| leaves::same_source(&found, source))
                        });
                    if !agrees {
                        failures.push(format!(
                            "{input:?} as {from:?}: leaf_sources gives {edited_leaves:?}"
                        ));
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} failures:\n{}",
            failures.len(),
            failures[..failures.len().min(40)].join("\n")
        );
    }

    #[test]
    fn debug_shows_the_text_and_each_segment() {
        let one = inline_source("a *b*", &[0]).unwrap();
        assert_eq!(
            format!("{one:?}"),
            "InlineSource { text: \"a *b*\", map: SourceMap { segments: [Segment { \
             text_start: 0, text_end: 5, source_start: 0, source_end: 5 }] } }"
        );
        assert_eq!(
            format!("{one:#?}"),
            r#"InlineSource {
    text: "a *b*",
    map: SourceMap {
        segments: [
            Segment {
                text_start: 0,
                text_end: 5,
                source_start: 0,
                source_end: 5,
            },
        ],
    },
}"#
        );
        let lines = inline_source("> a *b*\n> c\n> d", &[0, 0]).unwrap();
        assert_eq!(
            format!("{lines:?}"),
            "InlineSource { text: \"a *b*\\nc\\nd\", map: SourceMap { segments: [\
             Segment { text_start: 0, text_end: 6, source_start: 2, source_end: 8 }, \
             Segment { text_start: 6, text_end: 8, source_start: 10, source_end: 12 }, \
             Segment { text_start: 8, text_end: 9, source_start: 14, source_end: 15 }] } }"
        );
        assert_eq!(
            format!("{lines:#?}"),
            r#"InlineSource {
    text: "a *b*\nc\nd",
    map: SourceMap {
        segments: [
            Segment {
                text_start: 0,
                text_end: 6,
                source_start: 2,
                source_end: 8,
            },
            Segment {
                text_start: 6,
                text_end: 8,
                source_start: 10,
                source_end: 12,
            },
            Segment {
                text_start: 8,
                text_end: 9,
                source_start: 14,
                source_end: 15,
            },
        ],
    },
}"#
        );
    }
}

mod leaves_built_from_their_source {
    use super::*;

    // public-api: Leaves built from their source
    #[test]
    fn each_kind_keeps_its_text_and_reads_it_as_parse_inline_does() {
        let definitions = Definitions::new(["r"]);
        let text = "a *b* [c][r] [d]";
        let expected = parse_inline(text, &definitions);
        let (paragraph, diagnostics) = Paragraph::with_source(text, &definitions);
        assert_eq!(
            (paragraph.source(), &paragraph.children, diagnostics),
            (Some(text), &expected.inlines, expected.diagnostics.clone())
        );
        let (heading, _) = Heading::with_source(2, text, &definitions);
        assert_eq!(heading.depth, 2);
        assert_eq!(
            (heading.source(), &heading.children),
            (Some(text), &expected.inlines)
        );
        let (cell, _) = TableCell::with_source(text, &definitions);
        assert_eq!(
            (cell.source(), &cell.children),
            (Some(text), &expected.inlines)
        );
        let (leaf, _) = LeafDirective::with_source("x", text, &definitions);
        assert_eq!(
            (leaf.label_source(), &leaf.label),
            (Some(text), &expected.inlines)
        );
        let (container, _) =
            ContainerDirective::with_source("y", text, vec![BlankLine::new().into()], &definitions);
        assert_eq!(
            (
                container.label_source(),
                &container.label,
                container.children.len()
            ),
            (Some(text), &expected.inlines, 1)
        );
        // Spans are offsets in the text.
        assert_eq!(paragraph.children[1].meta().span, Some(Span::new(2, 5)));
    }

    // public-api: Leaves built from their source
    #[test]
    fn a_leaf_built_before_its_definition_reads_its_text_again() {
        let mut document = parse("[a]: /u").document.without_source();
        let (paragraph, _) = Paragraph::with_source("[a] [l]( x )", &Definitions::default());
        document.children.push(paragraph.into());
        let stale = document.to_markdown().unwrap();
        // Written from its inlines: the link loses its padding.
        assert!(stale.ends_with("\n\n[a] [l](x)\n"), "{stale:?}");
        document.reread_kept_sources();
        let Block::Paragraph(paragraph) = &document.children[1] else {
            panic!("{:?}", document.children);
        };
        assert!(matches!(
            paragraph.children.as_slice(),
            [Inline::LinkReference(_), Inline::Text(_), Inline::Link(_)]
        ));
        assert_eq!(paragraph.source(), Some("[a] [l]( x )"));
        let written = document.to_markdown().unwrap();
        assert!(written.ends_with("\n\n[a] [l]( x )\n"), "{written:?}");
    }

    #[test]
    fn constructors_and_parses_keep_no_text() {
        assert_eq!(Paragraph::new([Text::from("a")]).source(), None);
        let document = parse("a\n\n| b |\n|-|\n\n::c[d]").document;
        let [Block::Paragraph(paragraph), Block::Table(table), Block::LeafDirective(leaf)] =
            document.children.as_slice()
        else {
            panic!("{:?}", document.children);
        };
        assert_eq!(paragraph.source(), None);
        assert_eq!(table.rows[0].cells[0].source(), None);
        assert_eq!(leaf.label_source(), None);
    }
}
