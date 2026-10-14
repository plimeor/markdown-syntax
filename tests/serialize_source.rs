//! Writing a parsed document with its source, AST → Markdown: the requirement
//! "Unchanged source is kept" (`docs/specs/serialization.md`), and the kept
//! source of "Parsed documents keep their source" (`docs/specs/public-api.md`).
//!
//! The read-back corpus is edited block by block, and written unedited, in
//! `fixtures.rs`.

#[path = "support/leaves.rs"]
mod leaves;

use markdown_syntax::prelude::*;
use markdown_syntax::{BulletMarker, LineEnding, SerializeOptions};

fn paragraph(value: &str) -> Block {
    Paragraph::new([Text::from(value)]).into()
}

fn crlf() -> SerializeOptions {
    let mut options = SerializeOptions::default();
    options.line_ending = LineEnding::CrLf;
    options
}

mod parsed_documents_keep_their_source {
    use super::*;

    #[test]
    fn a_parsed_document_holds_its_input() {
        let input = "\u{feff}# T\r\n\r\na\0b";
        assert_eq!(parse(input).document.source(), Some(input));
    }

    #[test]
    fn a_hand_built_document_holds_none() {
        assert_eq!(Document::new(vec![paragraph("a")]).source(), None);
        assert_eq!(Document::from(vec![paragraph("a")]).source(), None);
        assert_eq!(Document::default().source(), None);
    }

    #[test]
    fn without_source_keeps_the_tree() {
        let document = parse("# T\n\na").document;
        let without = document.clone().without_source();
        assert_eq!(without.source(), None);
        assert_eq!(without.meta, document.meta);
        assert_eq!(without.children, document.children);
        assert_ne!(without, document);
    }
}

mod unchanged_source_is_kept {
    use super::*;

    const SOURCE: &str = "|a|b|\n|-|-|\n\n*  x\n\n\n# T #";

    // public-api: Output verbs on Document
    #[test]
    fn an_unedited_document_writes_its_source() {
        assert_eq!(parse(SOURCE).document.to_markdown().unwrap(), SOURCE);
        assert_eq!(
            parse("# Title\n\nHello *world*.")
                .document
                .to_markdown()
                .unwrap(),
            "# Title\n\nHello *world*."
        );
    }

    #[test]
    fn an_unedited_document_ignores_the_options() {
        let source = "\u{feff}\n\n* a\n\n~~~\nb\n~~~\n\n\n";
        let mut options = crlf();
        options.bullet = Some(BulletMarker::Dash);
        options.final_newline = false;
        assert_eq!(
            parse(source).document.to_markdown_with(&options).unwrap(),
            source
        );
    }

    // serialization: Unchanged source is kept
    #[test]
    fn one_edited_block_is_written_canonically_between_kept_ones() {
        let mut document = parse(SOURCE).document;
        document.children[1] = paragraph("y");
        assert_eq!(
            document.to_markdown().unwrap(),
            "|a|b|\n|-|-|\n\ny\n\n\n# T #"
        );
    }

    #[test]
    fn a_block_with_any_span_changed_is_written_canonically() {
        let mut document = parse("*  x\n\n# T #").document;
        let Block::List(list) = &mut document.children[0] else {
            panic!("{:?}", document.children);
        };
        list.children[0].meta = NodeMeta::default();
        assert_eq!(document.to_markdown().unwrap(), "* x\n\n# T #");
    }

    #[test]
    fn removed_blocks_leave_canonical_gaps() {
        let mut document = parse("a\n\n\nb\n\n\nc").document;
        // `b` and the blank line after it.
        document.children.drain(2..4);
        assert_eq!(document.to_markdown().unwrap(), "a\n\n\nc");
    }

    #[test]
    fn adjacent_kept_blocks_keep_the_gap_between_them() {
        let mut document = parse("a\n\n\nb\n\n\n\nc").document;
        document.children.push(paragraph("d"));
        assert_eq!(document.to_markdown().unwrap(), "a\n\n\nb\n\n\n\nc\n\nd\n");
    }

    #[test]
    fn leading_bytes_are_kept_with_the_first_block() {
        let mut document = parse("\u{feff}\n\n# T\n\na").document;
        // Two blank lines open the document.
        document.children[3] = paragraph("b");
        assert_eq!(document.to_markdown().unwrap(), "\u{feff}\n\n# T\n\nb\n");

        let mut document = parse("\u{feff}\n\n# T\n\na").document;
        document.children.drain(..2);
        document.children[0] = paragraph("b");
        assert_eq!(document.to_markdown().unwrap(), "b\n\na");
    }

    #[test]
    fn trailing_bytes_are_kept_with_the_last_block() {
        let mut document = parse("a\n\nb\n\n\n").document;
        document.children[0] = paragraph("c");
        assert_eq!(document.to_markdown().unwrap(), "c\n\nb\n\n\n");
    }

    #[test]
    fn moved_blocks_keep_their_own_source() {
        let mut document = parse("*  a\n\n# T #").document;
        document.children.reverse();
        assert_eq!(document.to_markdown().unwrap(), "# T #\n\n*  a\n");
    }

    #[test]
    fn options_apply_only_to_written_blocks() {
        let mut document = parse("* a\r\n\r\nb\n").document;
        document.children.push(Block::from(
            parse("+ c").document.without_source().children.remove(0),
        ));
        if let Block::List(list) = &mut document.children[2] {
            list.meta = NodeMeta::default();
            list.children[0].meta = NodeMeta::default();
        }
        let mut options = crlf();
        options.bullet = Some(BulletMarker::Dash);
        assert_eq!(
            document.to_markdown_with(&options).unwrap(),
            "* a\r\n\r\nb\r\n\r\n- c\r\n"
        );
    }

    #[test]
    fn a_left_open_last_block_is_closed_before_new_blocks() {
        for (source, written) in [
            ("```\na", "```\na\n```\n\ny\n"),
            (":::x\na", ":::x\na\n:::\n\ny\n"),
            ("$$\na", "$$\na\n$$\n\ny\n"),
        ] {
            let mut document = parse(source).document;
            document.children.push(paragraph("y"));
            let markdown = document.to_markdown().unwrap();
            assert_eq!(markdown, written, "{source:?}");
            assert_eq!(parse(&markdown).document.children.len(), 2, "{source:?}");
        }
        // A closed last block is kept.
        let mut document = parse("```\na\n```").document;
        document.children.push(paragraph("y"));
        assert_eq!(document.to_markdown().unwrap(), "```\na\n```\n\ny\n");
    }

    #[test]
    fn a_block_its_successor_closed_is_closed_before_another() {
        let mut document = parse("* ```\nparagraph").document;
        document.children[1] = paragraph("y");
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "* ```\n  ```\n\ny\n");
        assert_eq!(parse(&markdown).document.children.len(), 2);
    }

    #[test]
    fn a_kept_block_keeps_its_own_line_ending() {
        let mut document = parse("    a\r\n    b\r\n\r\nc").document;
        document.children[1] = paragraph("y");
        assert_eq!(document.to_markdown().unwrap(), "    a\r\n    b\r\n\ny\n");
        assert_eq!(
            document.to_markdown_with(&crlf()).unwrap(),
            "    a\r\n    b\r\n\r\ny\r\n"
        );
    }

    #[test]
    fn an_indented_block_after_an_edited_list_is_rewritten() {
        let mut document = parse("-   a\n\n  b").document;
        let Block::List(list) = &mut document.children[0] else {
            panic!("{:?}", document.children);
        };
        list.children[0].children[0] = paragraph("c");
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "- c\n\nb\n");
        assert_eq!(parse(&markdown).document.children.len(), 2);
        // After a paragraph, the indentation is kept.
        let mut document = parse("a\n\n   b").document;
        document.children[0] = paragraph("c");
        assert_eq!(document.to_markdown().unwrap(), "c\n\n   b");
    }

    #[test]
    fn a_kept_list_after_a_written_list_with_its_marker_is_rewritten() {
        let mut document = parse("- a\n\n+ b").document;
        document.children[0] = parse("- c").document.without_source().children.remove(0);
        if let Block::List(list) = &mut document.children[0] {
            list.meta = NodeMeta::default();
        }
        let mut options = SerializeOptions::default();
        options.bullet = Some(BulletMarker::Plus);
        let markdown = document.to_markdown_with(&options).unwrap();
        assert_eq!(markdown, "+ c\n\n- b\n");
        assert_eq!(parse(&markdown).document.children.len(), 2);
    }

    #[test]
    fn a_kept_thematic_break_that_comes_to_open_the_document_is_rewritten() {
        let mut document = parse("a\n\n---\n\nb\n\n---\n").document;
        document.children.remove(0);
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "- - -\n\nb\n\n---\n");
        assert!(matches!(
            parse(&markdown).document.children.as_slice(),
            [
                Block::ThematicBreak(_),
                Block::Paragraph(_),
                Block::ThematicBreak(_)
            ]
        ));
    }

    #[test]
    fn a_kept_dash_break_opening_the_document_is_rewritten() {
        let mut document = parse("---\n\nHello\n").document;
        document.children.push(
            ThematicBreak {
                meta: NodeMeta::default(),
                marker: ThematicBreakMarker::Dash,
            }
            .into(),
        );
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "- - -\n\nHello\n\n---\n");
        assert_eq!(parse(&markdown).document.children.len(), 3);
    }

    #[test]
    fn a_code_indented_continuation_after_another_block_is_rewritten() {
        let mut document = parse("[a]: /u\n    text\n").document;
        let Block::Definition(definition) = &mut document.children[0] else {
            panic!("{:?}", document.children);
        };
        definition.destination = "/v".into();
        definition.meta = NodeMeta::default();
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "[a]: /v\n\ntext\n");
        assert!(matches!(
            parse(&markdown).document.children.as_slice(),
            [Block::Definition(_), Block::Paragraph(_)]
        ));
    }

    #[test]
    fn a_kept_last_block_keeps_its_own_line_ending() {
        let mut document = parse("    a\r\n    b\r\n\r\nc").document;
        document.children.remove(1);
        assert_eq!(document.to_markdown().unwrap(), "    a\r\n    b\r\n");
    }

    #[test]
    fn validation_still_comes_first() {
        let mut document = parse("- a\n\np\n\n- b").document;
        document.children.remove(1);
        assert!(document.to_markdown().is_err());
    }
}

mod leaves_given_as_text {
    use super::*;
    use crate::leaves;

    // serialization: Unchanged source is kept
    #[test]
    fn leaves_given_as_text() {
        let source = "- a  *b*\n\n  c\n";
        let parsed = parse(source).document;
        let rebuilt = leaves::rebuilt_from_leaf_sources(&parsed);
        assert_ne!(rebuilt, parsed);
        assert_eq!(rebuilt.to_markdown().unwrap(), source);
    }

    #[test]
    fn an_edited_leaf_is_written_as_its_text_in_a_written_block() {
        let source = "- a  *b*\n\n  c\n\n# T\n";
        let document = parse(source).document;
        let definitions = Definitions::of(&document);
        let mut edited = document.clone();
        leaves::rebuild_at(&mut edited.children, &[0, 0, 0], "a  _b_", &definitions);
        // The list is written canonically, the edited leaf as its text, the
        // unedited one from its inlines, and the heading kept.
        assert_eq!(edited.to_markdown().unwrap(), "- a  _b_\n\n  c\n\n# T\n");
    }

    #[test]
    fn a_leaf_whose_inlines_changed_is_written_from_them() {
        let document = parse("a  *b*\n").document;
        let definitions = Definitions::of(&document);
        let mut edited = document.clone();
        leaves::rebuild_at(&mut edited.children, &[0], "a  *b*", &definitions);
        let Block::Paragraph(paragraph) = &mut edited.children[0] else {
            unreachable!()
        };
        assert_eq!(paragraph.source(), Some("a  *b*"));
        paragraph.children.push(Text::from("!").into());
        assert_eq!(edited.to_markdown().unwrap(), "a  *b*!\n");
    }

    #[test]
    fn cells_labels_and_headings_are_written_as_their_text() {
        let definitions = Definitions::default();
        let cell = |text| TableCell::with_source(text, &definitions).0;
        let table = Block::Table(Table {
            meta: NodeMeta::default(),
            alignments: vec![TableAlignment::None],
            rows: vec![
                TableRow {
                    meta: NodeMeta::default(),
                    cells: vec![cell("h  *i*")],
                },
                TableRow {
                    meta: NodeMeta::default(),
                    cells: vec![cell("a|b")],
                },
            ],
        });
        let heading = Heading::with_source(2, HeadingKind::Atx, "T  *u*", &definitions).0;
        let leaf = LeafDirective::with_source("x", "l  *m*", &definitions).0;
        let document = Document::new(vec![table, heading.into(), leaf.into()]);
        assert_eq!(
            document.to_markdown().unwrap(),
            "| h  *i* |\n| --- |\n| a\\|b |\n\n## T  *u*\n\n::x[l  *m*]\n"
        );
    }
}
