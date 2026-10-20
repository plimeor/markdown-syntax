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
