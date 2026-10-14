//! `parse_inline`, Markdown → inlines: the requirement "Inline parse"
//! (`docs/specs/public-api.md`). Its cost is checked in `linear_growth.rs`
//! and `pathological_inputs.rs`; its agreement with the inlines of every
//! parsed leaf block in `inline_source.rs`.

use markdown_syntax::prelude::*;

fn none() -> Definitions {
    Definitions::new::<_, &str>([])
}

/// The inlines of the one paragraph `source` parses to.
fn paragraph_inlines(source: &str) -> Vec<Inline> {
    match parse(source).document.children.as_slice() {
        [Block::Paragraph(paragraph)] => paragraph.children.clone(),
        other => panic!("{source:?}: {other:?}"),
    }
}

mod inline_parse {
    use super::*;

    #[test]
    fn block_syntax_stays_inline() {
        let output = parse_inline("# a\n===", &none());
        assert!(output.diagnostics.is_empty());
        let [Inline::Text(first), Inline::SoftBreak(_), Inline::Text(second)] =
            output.inlines.as_slice()
        else {
            panic!("{:?}", output.inlines);
        };
        assert_eq!(
            (first.value.as_str(), second.value.as_str()),
            ("# a", "===")
        );
        for text in ["- a", "> a", "    a", "```", "| a |\n| - |", "[r]: /u"] {
            let output = parse_inline(text, &none());
            assert!(
                !output
                    .inlines
                    .iter()
                    .any(|inline| matches!(inline, Inline::Link(_))),
                "{text:?}: {:?}",
                output.inlines
            );
        }
    }

    #[test]
    fn definitions_from_the_document() {
        let document = parse("[r]: /u").document;
        let output = parse_inline("[x][r]", &Definitions::of(&document));
        let [Inline::LinkReference(link)] = output.inlines.as_slice() else {
            panic!("{:?}", output.inlines);
        };
        assert_eq!(link.identifier, "r");
        assert_eq!(link.meta.span, Some(Span::new(0, 6)));
    }

    #[test]
    fn a_reference_forms_only_for_a_held_label() {
        let output = parse_inline("[x][r] [r]", &none());
        assert!(output
            .inlines
            .iter()
            .all(|inline| !matches!(inline, Inline::LinkReference(_))));
        let output = parse_inline("[x][r] [R]", &Definitions::new(["  r "]));
        let references = output
            .inlines
            .iter()
            .filter(|inline| matches!(inline, Inline::LinkReference(_)))
            .count();
        assert_eq!(references, 2, "{:?}", output.inlines);
    }

    #[test]
    fn nested_definitions_count() {
        let document = parse("> - [a]: /u\n\n[^n]: [b]: /v\n\n:::d\n[c]: /w\n:::").document;
        let output = parse_inline("[a] [b] [c]", &Definitions::of(&document));
        let references = output
            .inlines
            .iter()
            .filter(|inline| matches!(inline, Inline::LinkReference(_)))
            .count();
        assert_eq!(references, 3, "{:?}", output.inlines);
    }

    #[test]
    fn inlines_match_a_paragraphs() {
        for source in [
            "a *b* `c` [d](/e) ![[f]] :g[h]{i=j} ==k== ~~l~~ ^[m] <n@o.p> www.q.r",
            "a  \nb\\\nc\nd",
            "a\r\nb  \r\nc\rd",
            "[x] [y][]\n\n[x]: /u\n[y]: /v",
        ] {
            let document = parse(source).document;
            let Some(Block::Paragraph(paragraph)) = document.children.first() else {
                panic!("{source:?}");
            };
            let end = paragraph.meta.span.unwrap().end;
            let text = source[..end].trim_end_matches(['\n', '\r']);
            assert_eq!(
                parse_inline(text, &Definitions::of(&document)).inlines,
                paragraph.children,
                "{source:?}"
            );
        }
    }

    #[test]
    fn spans_are_byte_ranges_into_the_text() {
        let output = parse_inline("é\r\n*a*", &none());
        let [Inline::Text(text), Inline::SoftBreak(soft), Inline::Emphasis(emphasis)] =
            output.inlines.as_slice()
        else {
            panic!("{:?}", output.inlines);
        };
        assert_eq!(text.meta.span, Some(Span::new(0, 2)));
        assert_eq!(soft.meta.span, Some(Span::new(2, 4)));
        assert_eq!(emphasis.meta.span, Some(Span::new(4, 7)));
    }

    #[test]
    fn nul_reads_as_the_replacement_character_and_a_bom_is_content() {
        assert_eq!(
            parse_inline("a\0b", &none()).inlines,
            paragraph_inlines("a\0b")
        );
        let output = parse_inline("[\0]", &Definitions::of(&parse("[\0]: /u").document));
        assert!(
            matches!(output.inlines.as_slice(), [Inline::LinkReference(_)]),
            "{:?}",
            output.inlines
        );
        let output = parse_inline("\u{feff}a", &none());
        let [Inline::Text(text)] = output.inlines.as_slice() else {
            panic!("{:?}", output.inlines);
        };
        assert_eq!(text.value, "\u{feff}a");
    }

    #[test]
    fn every_string_parses() {
        let pieces = [
            "", "[", "]", "(", ")", "*", "_", "`", "<", ">", "\\", "&", "\n", "\r", "\0",
            "\u{feff}", ":", "{", "}", "!", "^", "~", "=", "|", "$", "é", " ", "#", "-",
        ];
        for first in pieces {
            for second in pieces {
                for third in pieces {
                    let text = format!("{first}{second}{third}{first}");
                    let output = parse_inline(&text, &Definitions::new(["a"]));
                    let _ = output.inlines;
                }
            }
        }
    }
}
