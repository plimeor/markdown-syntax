//! Writing a parsed document with its source, AST → Markdown: the requirement
//! "Unchanged source is kept" (`docs/specs/serialization.md`), and the kept
//! source of "Parsed documents keep their source" (`docs/specs/public-api.md`).
//!
//! The read-back corpus is edited block by block, and written unedited, in
//! `fixtures.rs`.

#[path = "support/leaves.rs"]
mod leaves;

use markdown_syntax::__private::normalized_blocks;
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
    fn an_indented_block_after_a_list_written_partly_from_the_source_is_kept() {
        let mut document = parse("-   a\n\n  b").document;
        let Block::List(list) = &mut document.children[0] else {
            panic!("{:?}", document.children);
        };
        list.children[0].children[0] = paragraph("c");
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "-   c\n\n  b");
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

mod source_reused_per_node {
    use super::*;

    fn item_mut(document: &mut Document, list: usize, item: usize) -> &mut ListItem {
        let Block::List(node) = &mut document.children[list] else {
            panic!("not a list");
        };
        &mut node.children[item]
    }

    // serialization: Unchanged source is kept
    #[test]
    fn edited_items() {
        let mut document = parse("-   a\n-   b\n\n    c\n-   d\n").document;
        item_mut(&mut document, 0, 1).children[0] = paragraph("x");
        assert_eq!(
            document.to_markdown().unwrap(),
            "-   a\n-   x\n\n    c\n-   d\n"
        );
        let mut document = parse("aa\n- b\n").document;
        item_mut(&mut document, 1, 0).children[0] = paragraph("x");
        assert_eq!(document.to_markdown().unwrap(), "aa\n- x\n");
        let mut document = parse("aa\n- b\n").document;
        item_mut(&mut document, 1, 0).children.clear();
        assert_eq!(document.to_markdown().unwrap(), "aa\n\n-\n");
        assert_eq!(markdown_syntax::__private::rewritten_blocks(&document), 0);
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_container_after_a_leaf_keeps_the_gap_while_it_still_starts_there() {
        let edited = |source: &str, edit: &dyn Fn(&mut Vec<Block>)| {
            let mut document = parse(source).document;
            let last = document.children.len() - 1;
            match &mut document.children[last] {
                Block::List(list) => edit(&mut list.children[0].children),
                Block::BlockQuote(quote) => edit(&mut quote.children),
                _ => panic!("{source:?} ends with a container"),
            }
            let markdown = document.to_markdown().unwrap();
            reads_back(&markdown, &document);
            assert_eq!(markdown_syntax::__private::rewritten_blocks(&document), 0);
            markdown
        };
        let replace = |children: &mut Vec<Block>| children[0] = paragraph("x");
        let clear = |children: &mut Vec<Block>| children.clear();
        assert_eq!(edited("text\n> q\n", &replace), "text\n> x\n");
        assert_eq!(edited("# h\n- b\n", &replace), "# h\n- x\n");
        assert_eq!(edited("# h\n- b\n", &clear), "# h\n-\n");
        assert_eq!(edited("|a|\n|-|\n- b\n", &replace), "|a|\n|-|\n- x\n");
        assert_eq!(edited("|a|\n|-|\n- b\n", &clear), "|a|\n|-|\n\n-\n");
        assert_eq!(edited("[a]: /u\n- b\n", &clear), "[a]: /u\n\n-\n");
        // Nested in a quote, after a paragraph.
        let mut document = parse("> aa\n> - b\n").document;
        let Block::List(list) = &mut quote_mut(&mut document, 0)[1] else {
            panic!();
        };
        list.children[0].children[0] = paragraph("x");
        assert_eq!(document.to_markdown().unwrap(), "> aa\n> - x\n");
        assert_eq!(markdown_syntax::__private::rewritten_blocks(&document), 0);
    }

    // serialization: Unchanged source is kept
    #[test]
    fn one_edited_link() {
        let mut document = parse("see  [[A]] and `` b ``").document;
        let Block::Paragraph(paragraph) = &mut document.children[0] else {
            panic!("{:?}", document.children);
        };
        let Inline::WikiLink(link) = &mut paragraph.children[1] else {
            panic!("{:?}", paragraph.children);
        };
        link.target = "B".into();
        link.label = "B".into();
        assert_eq!(document.to_markdown().unwrap(), "see  [[B]] and `` b ``");
    }

    // serialization: Unchanged source is kept
    #[test]
    fn an_edited_emphasis_inside_a_link_keeps_the_link() {
        let mut document = parse("> a [_b_ c]( /u  'T' ) d\n").document;
        let Block::BlockQuote(quote) = &mut document.children[0] else {
            panic!("{:?}", document.children);
        };
        let Block::Paragraph(paragraph) = &mut quote.children[0] else {
            panic!("{:?}", quote.children);
        };
        let Inline::Link(link) = &mut paragraph.children[1] else {
            panic!("{:?}", paragraph.children);
        };
        let Inline::Emphasis(emphasis) = &mut link.children[0] else {
            panic!("{:?}", link.children);
        };
        emphasis.children = vec![Text::from("e").into()];
        assert_eq!(
            document.to_markdown().unwrap(),
            "> a [_e_ c]( /u  'T' ) d\n"
        );
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_renumbered_or_ticked_item_keeps_its_column() {
        let source = "1.  a\n\n    > q\n2.  [ ] b\n\n        code\n";
        let mut document = parse(source).document;
        item_mut(&mut document, 0, 0).number = Some(7);
        item_mut(&mut document, 0, 1).checked = Some(true);
        assert_eq!(
            document.to_markdown().unwrap(),
            "7.  a\n\n    > q\n2.  [x] b\n\n        code\n"
        );
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_split_paragraph_and_a_moved_item() {
        let mut document = parse("- a\n- b\n\nc d\n").document;
        // Both halves of a split paragraph carry its span.
        let half = document.children[1].clone();
        document.children.push(half);
        let moved;
        // The second item moves into a new list.
        {
            let Block::List(list) = &mut document.children[0] else {
                panic!();
            };
            moved = list.children.pop().unwrap();
        }
        document.children.push(List::new([moved]).into());
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "- a\n\nc d\n\nc d\n\n- b\n");
        assert_eq!(
            normalized_blocks(&parse(&markdown).document.children),
            normalized_blocks(&document.children)
        );
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_new_item_takes_the_lists_recorded_marker() {
        let mut document = parse("* a\n*   b\n").document;
        let Block::List(list) = &mut document.children[0] else {
            panic!();
        };
        list.children.push(ListItem::new(vec![paragraph("c")]));
        let mut options = SerializeOptions::default();
        options.bullet = Some(BulletMarker::Dash);
        assert_eq!(
            document.to_markdown_with(&options).unwrap(),
            "* a\n*   b\n* c\n"
        );
    }

    fn quote_mut(document: &mut Document, index: usize) -> &mut Vec<Block> {
        let Block::BlockQuote(quote) = &mut document.children[index] else {
            panic!("not a quote");
        };
        &mut quote.children
    }

    /// The edited tree reads back from `markdown`.
    fn reads_back(markdown: &str, document: &Document) {
        assert_eq!(
            normalized_blocks(&parse(markdown).document.children),
            normalized_blocks(&document.children),
            "{markdown:?}"
        );
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_paragraph_after_an_unclosed_fence_closes_it() {
        // The fence ends with the quote; a new paragraph after it in the
        // quote would continue it.
        let mut document = parse("> ```\n> a\n\nb\n").document;
        quote_mut(&mut document, 0).push(paragraph("new"));
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "> ```\n> a\n> ```\n>\n> new\n\nb\n");
        reads_back(&markdown, &document);
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_gap_without_a_blank_line_stays_only_beside_unchanged_lines() {
        // `b` follows the quote's code without a blank line; after a new
        // last paragraph in the quote it would continue that paragraph.
        let mut document = parse(">     code\n    b\n").document;
        quote_mut(&mut document, 0).push(paragraph("new"));
        let markdown = document.to_markdown().unwrap();
        reads_back(&markdown, &document);
        assert!(
            markdown.starts_with(">     code\n>\n> new\n\n"),
            "{markdown:?}"
        );
        // An edited paragraph before a list keeps the gap: the list's first
        // line still interrupts it.
        let mut document = parse("Intro:\n- a\n- b\n").document;
        let Block::Paragraph(intro) = &mut document.children[0] else {
            panic!();
        };
        let Inline::Text(text) = &mut intro.children[0] else {
            panic!();
        };
        text.value = "Intro, edited:".into();
        assert_eq!(
            document.to_markdown().unwrap(),
            "Intro, edited:\n- a\n- b\n"
        );
        // A renumbered list no longer interrupts the paragraph before it.
        let mut document = parse("a\n1. b\n").document;
        item_mut(&mut document, 1, 0).number = Some(2);
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "a\n\n2. b\n");
        reads_back(&markdown, &document);
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_loose_list_keeps_a_blank_line() {
        // The blank line inside the second item made the list loose; with
        // it gone, the list is written whole.
        let mut document = parse("- a\n- b\n\n  c\n- d\n").document;
        item_mut(&mut document, 0, 1).children.remove(0);
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "- a\n\n- c\n\n- d\n");
        reads_back(&markdown, &document);
        // Blank lines at an item's end keep the separator before them.
        let mut document = parse("- a\n\n- b\n\n\n- c\n").document;
        item_mut(&mut document, 0, 1).children[0] = paragraph("x");
        assert_eq!(document.to_markdown().unwrap(), "- a\n\n- x\n\n\n- c\n");
    }

    // serialization: Unchanged source is kept
    #[test]
    fn an_emptied_item_keeps_its_marker_and_column() {
        // `  - c` is a sibling of ` - b` only against b's own column.
        let mut document = parse("- a\n - b\n  - c\n").document;
        item_mut(&mut document, 0, 1).children.clear();
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "- a\n -\n  - c\n");
        reads_back(&markdown, &document);
        // An item whose first block becomes indented code is written whole.
        let mut document = parse("1.  a\n\n        c\n\n    d\n").document;
        item_mut(&mut document, 0, 0).children.remove(0);
        let markdown = document.to_markdown().unwrap();
        reads_back(&markdown, &document);
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_task_items_rendered_blocks_take_its_content_column() {
        // The checkbox is part of the item's content, which starts at two
        // columns.
        let mut document = parse("* [ ] a\n  * [x] b\n").document;
        let Block::List(nested) = &mut item_mut(&mut document, 0, 0).children[1] else {
            panic!();
        };
        nested.children[0].checked = Some(false);
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "* [ ] a\n  * [ ] b\n");
        let mut document = parse("* [ ] a\n\n  b\n").document;
        item_mut(&mut document, 0, 0).children[1] = paragraph("x");
        assert_eq!(document.to_markdown().unwrap(), "* [ ] a\n\n  x\n");
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_nested_block_that_a_list_would_take_in_is_rendered() {
        // Without `b`, the indented `c` would continue the item.
        let mut document = parse("> # T #\n>\n> - a\n>\n> b\n>\n>   c\n").document;
        quote_mut(&mut document, 0).remove(2);
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "> # T #\n>\n> - a\n>\n> c\n");
        reads_back(&markdown, &document);
    }

    // serialization: Unchanged source is kept
    #[test]
    fn containers_keep_their_own_opening_lines() {
        let mut document = parse(">b\n>\n>c\n").document;
        quote_mut(&mut document, 0)[1] = paragraph("x");
        assert_eq!(document.to_markdown().unwrap(), ">b\n>\n> x\n");
        let mut document = parse("[^1]:\n\n    a\n\nb [^1]\n").document;
        let Block::FootnoteDefinition(note) = &mut document.children[0] else {
            panic!();
        };
        note.children[0] = paragraph("x");
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "[^1]:\n\n    x\n\nb [^1]\n");
        reads_back(&markdown, &document);
        let mut document = parse("<details><summary>S</summary>\n\nbody\n\n</details>\n").document;
        let Block::HtmlContainer(container) = &mut document.children[0] else {
            panic!();
        };
        let HtmlContainerContent::Blocks(children) = &mut container.content else {
            panic!();
        };
        children[1] = paragraph("x");
        let markdown = document.to_markdown().unwrap();
        assert_eq!(
            markdown,
            "<details><summary>S</summary>\n\nx\n\n</details>\n"
        );
        reads_back(&markdown, &document);
    }

    // serialization: Unchanged source is kept
    #[test]
    fn a_block_that_reads_differently_is_written_canonically() {
        // A copied underline under content that opens a list.
        let mut document = parse("a\n===\n\nb\n").document;
        let Block::Heading(heading) = &mut document.children[0] else {
            panic!();
        };
        heading.children = vec![Text::from("- x").into()];
        let canonical = document.clone().without_source().to_markdown().unwrap();
        assert_eq!(document.to_markdown().unwrap(), canonical);
        assert_eq!(markdown_syntax::__private::rewritten_blocks(&document), 1);
    }

    fn first_text(blocks: &mut [Block]) -> &mut Text {
        fn in_inlines(inlines: &mut [Inline]) -> Option<&mut Text> {
            inlines.iter_mut().find_map(|inline| match inline {
                Inline::Text(text) => Some(text),
                _ => None,
            })
        }
        blocks
            .iter_mut()
            .find_map(|block| match block {
                Block::Paragraph(node) => in_inlines(&mut node.children),
                Block::Heading(node) => in_inlines(&mut node.children),
                _ => None,
            })
            .expect("a leaf with text")
    }

    // serialization: Unchanged source is kept
    #[test]
    fn the_line_ending_option_leaves_copied_endings() {
        let mut document = parse("x\n\n- a\n- b\n- c\n\ny\n").document;
        first_text(&mut item_mut(&mut document, 1, 1).children).value = "bZ".into();
        assert_eq!(
            document.to_markdown_with(&crate::crlf()).unwrap(),
            "x\n\n- a\n- bZ\n- c\n\ny\n"
        );
    }

    // serialization: Unchanged source is kept
    #[test]
    fn an_edited_leaf_keeps_its_indentation_and_trailing_spaces() {
        for (input, written) in [
            ("x\n\n   b\n", "x\n\n   bZ\n"),
            ("> x\n>\n>   b  \n", "> x\n>\n>   bZ  \n"),
            ("x\n\n  b\n  ===\n", "x\n\n  bZ\n===\n"),
        ] {
            let mut document = parse(input).document;
            let last = document.children.len() - 1;
            let blocks = match &mut document.children[last] {
                Block::BlockQuote(node) => &mut node.children[1..],
                _ => &mut document.children[last..],
            };
            first_text(blocks).value.push('Z');
            let markdown = document.to_markdown().unwrap();
            assert_eq!(markdown, written);
            reads_back(&markdown, &document);
        }
    }

    // serialization: Unchanged source is kept
    #[test]
    fn an_edited_label_or_cell_keeps_the_rest_of_its_line() {
        let mut document = parse(":::box[label]\n\n-   a\n:::\n").document;
        let Block::ContainerDirective(directive) = &mut document.children[0] else {
            panic!();
        };
        directive.label = vec![Text::from("labelZ").into()];
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, ":::box[labelZ]\n\n-   a\n:::\n");
        reads_back(&markdown, &document);
        let mut document = parse("|a|`` b ``|\n|-|-|\n").document;
        let Block::Table(table) = &mut document.children[0] else {
            panic!();
        };
        table.rows[0].cells[0].children = vec![Text::from("aZ").into()];
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "|aZ|`` b ``|\n|-|-|\n");
        reads_back(&markdown, &document);
    }

    // serialization: Unchanged source is kept
    #[test]
    fn items_written_around_an_edit_keep_reading_as_they_did() {
        // A rendered item keeps its trailing blank line.
        let mut document = parse("- foo\n\n- bar\n\n\n- baz").document;
        item_mut(&mut document, 0, 1).checked = Some(false);
        assert_eq!(
            document.to_markdown().unwrap(),
            "- foo\n\n- [ ] bar\n\n\n- baz"
        );
        assert_eq!(markdown_syntax::__private::rewritten_blocks(&document), 0);
        // An indented item no longer after its predecessor is rendered.
        let mut document = parse("1. a\n\n  2. b\n\n   3. c").document;
        let Block::List(list) = &mut document.children[0] else {
            panic!();
        };
        list.children.remove(1);
        assert_eq!(document.to_markdown().unwrap(), "1. a\n\n3. c");
        assert_eq!(markdown_syntax::__private::rewritten_blocks(&document), 0);
        // A break after a new paragraph in a tight item is spaced.
        let mut document = parse("a\n* ---").document;
        item_mut(&mut document, 1, 0)
            .children
            .insert(0, paragraph("new"));
        assert_eq!(document.to_markdown().unwrap(), "a\n* new\n  - - -");
        assert_eq!(markdown_syntax::__private::rewritten_blocks(&document), 0);
        // A new last item takes in no indented paragraph after the list.
        let mut document = parse("-    foo\n\n  bar\n").document;
        let Block::List(list) = &mut document.children[0] else {
            panic!();
        };
        list.children.push(ListItem::new([paragraph("new")]));
        let markdown = document.to_markdown().unwrap();
        assert_eq!(markdown, "-    foo\n- new\n\nbar\n");
        assert_eq!(markdown_syntax::__private::rewritten_blocks(&document), 0);
    }
}
