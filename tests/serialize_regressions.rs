//! Serializer regression coverage: value encodings (fences, destinations,
//! titles, labels, and pipes in table cells), the recorded spellings the
//! serializer writes, and the parsed inputs of earlier serializer defects,
//! each of which reads back or is listed with the reason it does not.
//!
//! The serializer only renders: text, escapes, and character references are
//! written as recorded, and a hand-built tree whose text reads as syntax is
//! written as it is.

#[path = "support/normalize.rs"]
mod normalize;

use markdown_syntax::prelude::*;

fn text(value: &str) -> Inline {
    Text::from(value).into()
}

fn escape(value: char) -> Inline {
    Inline::Escape(Escape {
        meta: NodeMeta::default(),
        value,
    })
}

fn paragraph(children: Vec<Inline>) -> Block {
    Paragraph::new(children).into()
}

fn document(children: Vec<Block>) -> Document {
    Document {
        meta: NodeMeta::default(),
        children,
    }
}

fn paragraph_document(children: Vec<Inline>) -> Document {
    document(vec![paragraph(children)])
}

/// The normalized blocks of `document`, for comparing trees.
fn normalized(document: &Document) -> String {
    format!("{:?}", normalize::normalized(&document.children))
}

/// Serializes `document` and checks that the output reads back as it, and
/// serializes back to itself.
fn assert_reads_back(document: &Document) -> String {
    let markdown = document.to_markdown().expect("document serializes");
    let reparsed = parse(&markdown).document;
    assert_eq!(normalized(&reparsed), normalized(document), "{markdown:?}");
    assert_eq!(
        reparsed
            .to_markdown()
            .expect("reparsed document serializes"),
        markdown,
        "{markdown:?}"
    );
    markdown
}

/// Serializes `document` and checks that the output reads back as a
/// different tree.
fn assert_reads_back_otherwise(document: &Document) -> String {
    let markdown = document.to_markdown().expect("document serializes");
    let reparsed = parse(&markdown).document;
    assert_ne!(normalized(&reparsed), normalized(document), "{markdown:?}");
    markdown
}

/// Parsed inputs whose Markdown reads back as a different tree, with the
/// reason. Each awaits a decision (plan one-normative-syntax, Risks): the
/// difference is whitespace, a blank line, a lazy line, or a spelling the AST
/// does not record.
const NOT_READING_BACK: &[(&str, &str)] = &[
    (
        "[o]:u\n\t$$\na$$",
        "a paragraph continuing a definition's line, whose `$$` line keeps from opening a math block, is written apart from it",
    ),
    (
        "[o]:u\n\t<div>",
        "a paragraph continuing a definition's line, whose `<div>` line keeps from opening an HTML block, is written apart from it",
    ),
    (
        "[o]:u\n<a>\n-",
        "a paragraph continuing a definition's line, whose `<a>` line keeps from opening an HTML block, is written apart from it",
    ),
    (
        "[o]: u\n<a>",
        "a paragraph continuing a definition's line, whose `<a>` line keeps from opening an HTML block, is written apart from it",
    ),
    (
        "~ \n\n>  > \t<!--[x] 2) *   :::e",
        "the tab after the nested quote marker, which keeps `<!--` from opening an HTML block, is not recorded",
    ),
    (
        ">* \t[x] : # <div>a\n> |-|",
        "the tab between the bullet and `[x]`, which keeps the item from being a task, is not recorded",
    ),
    (
        "-$$\n    $$",
        "the indentation of the continuation line, which keeps `$$` from closing math, is not recorded",
    ),
    (
        "-\t(\n  <v>",
        "the tab after the bullet, which sets the item's content column, is not recorded",
    ),
    (
        "*\t<a>\n  <v>",
        "the tab after the bullet, which sets the item's content column, is not recorded",
    ),
    (
        "[\n~ _\n    ~~~",
        "the indentation of the continuation line, which keeps `~~~` from opening a fence, is not recorded",
    ),
    (
        "- > a\n  >\n  b\n  ---",
        "the quote's empty last line, which keeps `b` from continuing its paragraph, is not recorded",
    ),
    (
        "- > a\n  >\n  | b |\n  | - |",
        "the quote's empty last line, which keeps `| b |` from continuing its paragraph, is not recorded",
    ),
    (
        "- *  (\n    <a>",
        "the spaces after the nested bullet, which set the item's content column, are not recorded",
    ),
    (
        ">\n>[!NOTE]:>",
        "the quote's empty first line, which keeps `[!NOTE]` from opening an alert, is not recorded",
    ),
    (
        "(\n    <div>",
        "the indentation of the continuation line, which keeps `<div>` from opening an HTML block, is not recorded",
    ),
    (
        "- a\n\n  <!--\n- b",
        "the blank line written between list items is taken by the HTML block that runs to its item's end",
    ),
];

/// Parses `source`, and checks that its Markdown reads back as the parsed
/// tree, or, for a listed source, that it reads back as a different one.
/// Returns the Markdown.
fn written(source: &str) -> String {
    let document = parse(source).document;
    let markdown = document
        .to_markdown()
        .unwrap_or_else(|error| panic!("{source:?}: {error:?}"));
    let reparsed = parse(&markdown).document;
    let reads_back = normalized(&reparsed) == normalized(&document)
        && reparsed.to_markdown().as_deref() == Ok(markdown.as_str());
    let listed = NOT_READING_BACK.iter().any(|(listed, _)| *listed == source);
    assert!(
        reads_back != listed,
        "{source:?} -> {markdown:?}: {}",
        if listed {
            "listed, but reads back"
        } else {
            "does not read back"
        }
    );
    markdown
}

mod value_encodings {
    use super::*;

    #[test]
    fn list_markers_preserve_by_default_and_yield_when_overridden() {
        let input = "- a\n\n+ b\n\n* c\n";
        let document = parse(input).document;
        assert_eq!(written(input), input);

        let mut options = SerializeOptions::default();
        options.bullet = Some(BulletMarker::Plus);
        let overridden = document
            .to_markdown_with(&options)
            .expect("document serializes with options");
        // The override yields where two adjacent lists would read as one.
        assert_eq!(overridden, "+ a\n\n- b\n\n+ c\n");
        assert_eq!(parse(&overridden).document.children.len(), 3);

        let mut options = SerializeOptions::default();
        options.ordered_delimiter = Some(OrderedDelimiter::Period);
        let input = "1) a\n\n1. b\n";
        let overridden = parse(input)
            .document
            .to_markdown_with(&options)
            .expect("document serializes with options");
        assert_eq!(overridden, "1. a\n\n1) b\n");
        assert_eq!(parse(&overridden).document.children.len(), 2);
    }

    #[test]
    fn some_marker_replaces_every_recorded_marker_and_none_keeps_it() {
        let input = "* a\n\n2) b\n\n~~~\ncode\n~~~\n\n```\nmore\n```\n";
        let document = parse(input).document;

        let defaults = SerializeOptions::default();
        assert_eq!(defaults.bullet, None);
        assert_eq!(defaults.ordered_delimiter, None);
        assert_eq!(defaults.fence_marker, None);
        assert_eq!(document.to_markdown_with(&defaults).unwrap(), input);

        // `Some` holding the marker a default build would pick still replaces.
        let mut options = SerializeOptions::default();
        options.bullet = Some(BulletMarker::Dash);
        options.ordered_delimiter = Some(OrderedDelimiter::Period);
        options.fence_marker = Some(FenceMarker::Backtick);
        assert_eq!(
            document.to_markdown_with(&options).unwrap(),
            "- a\n\n2. b\n\n```\ncode\n```\n\n```\nmore\n```\n"
        );

        let mut options = SerializeOptions::default();
        options.fence_marker = Some(FenceMarker::Tilde);
        assert_eq!(
            document.to_markdown_with(&options).unwrap(),
            "* a\n\n2) b\n\n~~~\ncode\n~~~\n\n~~~\nmore\n~~~\n"
        );
    }

    #[test]
    fn math_serialization_uses_parseable_fences() {
        let document = document(vec![
            Block::MathBlock(MathBlock {
                meta: NodeMeta::default(),
                value: "$$\n$$$\na $$ b".into(),
            }),
            paragraph(vec![Inline::Math(MathInline {
                meta: NodeMeta::default(),
                value: "a $$ b".into(),
                kind: MathInlineKind::Code,
            })]),
        ]);

        let markdown = document.to_markdown().expect("document serializes");
        assert!(markdown.contains("$$$$\n$$\n$$$\na $$ b\n$$$$"));
        assert!(markdown.contains("$`a $$ b`$"));

        let reparsed = parse(&markdown).document;
        match &reparsed.children[..] {
            [Block::MathBlock(block), Block::Paragraph(paragraph)] => {
                assert_eq!(block.value, "$$\n$$$\na $$ b\n");
                assert!(matches!(
                    &paragraph.children[..],
                    [Inline::Math(MathInline { value, .. })] if value == "a $$ b"
                ));
            }
            other => panic!("unexpected document shape: {other:?}"),
        }
    }

    #[test]
    fn inline_math_that_code_math_cannot_represent_fails() {
        let document = paragraph_document(vec![Inline::Math(MathInline {
            meta: NodeMeta::default(),
            value: "a $$ b `$ c".into(),
            kind: MathInlineKind::Code,
        })]);

        assert!(matches!(
            document.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));
    }

    #[test]
    fn container_directive_fence_exceeds_serialized_code_colons() {
        let document = document(vec![Block::ContainerDirective(ContainerDirective {
            meta: NodeMeta::default(),
            name: "note".into(),
            label: Vec::new(),
            attributes: Vec::new(),
            children: vec![Block::CodeBlock(CodeBlock {
                meta: NodeMeta::default(),
                kind: CodeBlockKind::Fenced {
                    marker: FenceMarker::Backtick,
                    length: 3,
                },
                info: None,
                value: "before\n:::\n::::\nafter".into(),
            })],
        })]);

        let markdown = document.to_markdown().expect("document serializes");
        assert!(markdown.starts_with(":::::note\n"));

        let reparsed = parse(&markdown).document;
        match &reparsed.children[..] {
            [Block::ContainerDirective(container)] => match &container.children[..] {
                [Block::CodeBlock(code)] => {
                    assert_eq!(code.value, "before\n:::\n::::\nafter\n");
                }
                other => panic!("unexpected directive children: {other:?}"),
            },
            other => panic!("unexpected document shape: {other:?}"),
        }
    }

    fn cell(children: Vec<Inline>) -> TableCell {
        TableCell {
            meta: NodeMeta::default(),
            children,
        }
    }

    fn table(header: Vec<&str>, row: Vec<Vec<Inline>>) -> Block {
        Block::Table(Table {
            meta: NodeMeta::default(),
            alignments: vec![TableAlignment::None; header.len()],
            rows: vec![
                TableRow {
                    meta: NodeMeta::default(),
                    cells: header.into_iter().map(|h| cell(vec![text(h)])).collect(),
                },
                TableRow {
                    meta: NodeMeta::default(),
                    cells: row.into_iter().map(cell).collect(),
                },
            ],
        })
    }

    #[test]
    fn table_cells_escape_resource_pipes() {
        let document = document(vec![table(
            vec!["Link", "Image"],
            vec![
                vec![Inline::Link(Link {
                    meta: NodeMeta::default(),
                    destination: "b|c".into(),
                    destination_kind: LinkDestinationKind::Bare,
                    title: Some("t|u".into()),
                    title_kind: Some(LinkTitleKind::DoubleQuote),
                    children: vec![text("a")],
                })],
                vec![Inline::Image(Image {
                    meta: NodeMeta::default(),
                    destination: "y|z".into(),
                    destination_kind: LinkDestinationKind::Bare,
                    title: Some("i|j".into()),
                    title_kind: Some(LinkTitleKind::DoubleQuote),
                    alt: vec![text("x")],
                })],
            ],
        )]);

        let markdown = assert_reads_back(&document);
        assert!(markdown.contains(r#"b\|c "t\|u""#));
        assert!(markdown.contains(r#"y\|z "i\|j""#));
    }

    #[test]
    fn table_cell_values_escape_their_pipes_and_text_holds_escapes() {
        // Literal text pipes are built as escapes; the values of code, math,
        // labels, and attributes are encoded by the serializer.
        let document = document(vec![
            table(
                vec![
                    "Text",
                    "Code",
                    "Math",
                    "Link",
                    "Image",
                    "Reference",
                    "Directive",
                ],
                vec![
                    vec![text("a"), escape('|'), text("b")],
                    vec![Inline::Code(CodeInline::new("c|d"))],
                    vec![Inline::Math(MathInline {
                        meta: NodeMeta::default(),
                        value: "x|y".into(),
                        kind: MathInlineKind::Dollar { dollars: 1 },
                    })],
                    vec![Inline::Link(Link::new(
                        "/link",
                        [text("link"), escape('|'), text("label")],
                    ))],
                    vec![Inline::Image(Image {
                        meta: NodeMeta::default(),
                        destination: "/img".into(),
                        destination_kind: LinkDestinationKind::Bare,
                        title: None,
                        title_kind: None,
                        alt: vec![text("img"), escape('|'), text("alt")],
                    })],
                    vec![Inline::LinkReference(LinkReference {
                        meta: NodeMeta::default(),
                        identifier: "pipe|id".into(),
                        label: "pipe|id".into(),
                        kind: ReferenceKind::Full,
                        children: vec![text("ref"), escape('|'), text("text")],
                    })],
                    vec![Inline::TextDirective(TextDirective {
                        meta: NodeMeta::default(),
                        name: "note".into(),
                        label: vec![text("label"), escape('|'), text("text")],
                        attributes: vec![DirectiveAttribute {
                            name: "data".into(),
                            value: Some("value|pipe".into()),
                        }],
                    })],
                ],
            ),
            Block::Definition(Definition {
                meta: NodeMeta::default(),
                label: "pipe|id".into(),
                identifier: "pipe|id".into(),
                destination: "/dest".into(),
                destination_kind: LinkDestinationKind::Bare,
                title: None,
                title_kind: None,
            }),
        ]);

        let markdown = assert_reads_back(&document);
        assert!(markdown.contains(r"a\|b"));
        assert!(markdown.contains(r"`c\|d`"));
        assert!(markdown.contains(r"$x\|y$"));
        assert!(markdown.contains(r"[link\|label](/link)"));
        assert!(markdown.contains(r"![img\|alt](/img)"));
        assert!(markdown.contains(r"[ref\|text][pipe\|id]"));
        assert!(markdown.contains(r#":note[label\|text]{data="value\|pipe"}"#));
    }

    #[test]
    fn a_text_pipe_in_a_cell_is_written_as_recorded() {
        let document = document(vec![table(vec!["Text"], vec![vec![text("a|b")]])]);
        let markdown = assert_reads_back_otherwise(&document);
        assert_eq!(markdown, "| Text |\n| --- |\n| a|b |\n");
    }

    #[test]
    fn bars_in_a_cell_split_it() {
        // Former spoiler bars delimit cells, as every unescaped pipe does.
        let source = "| Result |\n| --- |\n| ||visible|| |\n";
        let reparsed = parse(source).document;
        let [Block::Table(table)] = reparsed.children.as_slice() else {
            panic!("{reparsed:?}");
        };
        assert!(table.rows[1].cells[0].children.is_empty());
        assert_eq!(written(source), "| Result |\n| --- |\n|  |\n");
    }

    #[test]
    fn serialize_options_apply_to_lists_and_code_fences_without_overflow() {
        let item = |value: &str| ListItem {
            meta: NodeMeta::default(),
            checked: None,
            children: vec![paragraph(vec![text(value)])],
        };
        let document = document(vec![
            Block::List(List {
                meta: NodeMeta::default(),
                ordered: false,
                start: None,
                delimiter: ListDelimiter::Dash,
                tight: true,
                children: vec![item("bullet")],
            }),
            Block::List(List {
                meta: NodeMeta::default(),
                ordered: true,
                start: Some(999_999_998),
                delimiter: ListDelimiter::Period,
                tight: true,
                children: vec![item("one"), item("two")],
            }),
            Block::CodeBlock(CodeBlock {
                meta: NodeMeta::default(),
                kind: CodeBlockKind::Fenced {
                    marker: FenceMarker::Backtick,
                    length: 3,
                },
                info: None,
                value: "code".into(),
            }),
        ]);
        let mut options = SerializeOptions::default();
        options.bullet = Some(BulletMarker::Plus);
        options.ordered_delimiter = Some(OrderedDelimiter::Paren);
        options.fence_marker = Some(FenceMarker::Tilde);

        let markdown = document
            .to_markdown_with(&options)
            .expect("document serializes");
        assert_eq!(
            markdown,
            concat!(
                "+ bullet\n\n",
                "999999998) one\n",
                "999999999) two\n\n",
                "~~~\n",
                "code\n",
                "~~~\n"
            )
        );
    }

    #[test]
    fn resource_destination_and_title_kinds_are_ast_owned() {
        let document = document(vec![
            Block::Definition(Definition {
                meta: NodeMeta::default(),
                label: "foo".into(),
                identifier: "foo".into(),
                destination: "my url".into(),
                destination_kind: LinkDestinationKind::Angle,
                title: Some("single title".into()),
                title_kind: Some(LinkTitleKind::SingleQuote),
            }),
            paragraph(vec![
                Inline::Link(Link {
                    meta: NodeMeta::default(),
                    destination: "foo bar".into(),
                    destination_kind: LinkDestinationKind::Angle,
                    title: Some("paren title".into()),
                    title_kind: Some(LinkTitleKind::Paren),
                    children: vec![text("angle")],
                }),
                text(" "),
                Inline::Image(Image {
                    meta: NodeMeta::default(),
                    destination: String::new(),
                    destination_kind: LinkDestinationKind::Omitted,
                    title: Some("empty title".into()),
                    title_kind: Some(LinkTitleKind::DoubleQuote),
                    alt: vec![text("empty")],
                }),
            ]),
        ]);

        let markdown = assert_reads_back(&document);
        assert_eq!(
            markdown,
            "[foo]: <my url> 'single title'\n\n[angle](<foo bar> (paren title)) ![empty]( \"empty title\")\n"
        );
    }

    #[test]
    fn empty_resource_titles_are_preserved_with_their_kind() {
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
                    [Inline::Link(link)] => Some((link.title.clone(), link.title_kind)),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            link_titles,
            vec![
                (Some(String::new()), Some(LinkTitleKind::DoubleQuote)),
                (Some(String::new()), Some(LinkTitleKind::SingleQuote)),
                (Some(String::new()), Some(LinkTitleKind::Paren)),
                (Some(String::new()), Some(LinkTitleKind::DoubleQuote)),
            ]
        );
        assert_eq!(written(input), input);
    }

    #[test]
    fn ordinary_at_text_stays_text() {
        let markdown = assert_reads_back(&paragraph_document(vec![text("This@that.")]));
        assert_eq!(markdown, "This@that.\n");
    }

    fn definition(label: &str, identifier: &str) -> Document {
        document(vec![Block::Definition(Definition {
            meta: NodeMeta::default(),
            label: label.into(),
            identifier: identifier.into(),
            destination: "/u".into(),
            destination_kind: LinkDestinationKind::Bare,
            title: None,
            title_kind: None,
        })])
    }

    #[test]
    fn definition_labels_are_written_as_recorded() {
        // A label is matched raw, so it is written as the AST holds it; one
        // holding an unescaped bracket reads back as something else.
        assert_eq!(
            assert_reads_back_otherwise(&definition("a]b\\c[d", "a]b\\c[d")),
            "[a]b\\c[d]: /u\n"
        );

        // A label may span lines, and its escapes stay as written.
        for (label, identifier, expected) in [
            ("line\nbreak", "line break", "[line\nbreak]: /u\n"),
            ("a\\]b\\\\c\\[d", "a\\]b\\\\c\\[d", "[a\\]b\\\\c\\[d]: /u\n"),
        ] {
            let document = definition(label, identifier);
            let markdown = document.to_markdown().expect("document serializes");
            assert_eq!(markdown, expected);
            match &parse(&markdown).document.children[..] {
                [Block::Definition(definition)] => {
                    assert_eq!(definition.label, label);
                    assert_eq!(definition.identifier, identifier);
                }
                other => panic!("unexpected document shape: {other:?}"),
            }
        }
    }

    #[test]
    fn footnote_labels_are_written_as_recorded() {
        let footnote = |label: &str| {
            let reference = Inline::FootnoteReference(FootnoteReference {
                meta: NodeMeta::default(),
                label: label.into(),
                identifier: label.into(),
            });
            document(vec![
                paragraph(vec![text("See "), reference]),
                Block::FootnoteDefinition(FootnoteDefinition {
                    meta: NodeMeta::default(),
                    label: label.into(),
                    identifier: label.into(),
                    children: vec![paragraph(vec![text("note")])],
                }),
            ])
        };
        assert_eq!(
            assert_reads_back_otherwise(&footnote("a]b\\c[d")),
            "See [^a]b\\c[d]\n\n[^a]b\\c[d]: note\n"
        );
        assert_eq!(
            assert_reads_back_otherwise(&footnote("white space")),
            "See [^white space]\n\n[^white space]: note\n"
        );

        // Raw-label matching keeps the escaped/entity-encoded spelling: a
        // footnote ref and its definition fold identically, and the
        // identifier is the raw source, which reads back.
        let input = "See [^a\\]b\\\\c\\[d] and [^white&#x20;space]\n\n[^a\\]b\\\\c\\[d]: bracket\n\n[^white&#x20;space]: space\n";
        assert_eq!(written(input), input);
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
    fn whitespace_at_the_ends_of_an_info_string_is_written_as_references() {
        assert_eq!(
            written("```&#x20;a&#9;\nb\n```"),
            "``` &#x20;a&#x9;\nb\n```\n"
        );
        let document = parse(&written("```&#x20;a&#9;\nb\n```")).document;
        let [Block::CodeBlock(code)] = document.children.as_slice() else {
            panic!("{document:?}");
        };
        assert_eq!(code.info.as_deref(), Some(" a\t"));
    }

    #[test]
    fn an_empty_fenced_code_block_writes_no_content_line() {
        assert_eq!(written("```\n```"), "```\n```\n");
    }

    #[test]
    fn a_fence_grows_only_past_lines_that_would_close_it() {
        assert_eq!(written("```\n```*"), "```\n```*\n```\n");
        assert_eq!(written("````\n```\n````"), "````\n```\n````\n");
        assert_eq!(written(" ~~~\n    ~~~"), " ~~~\n    ~~~\n ~~~\n");
    }

    #[test]
    fn indented_code_keeps_a_carriage_return_ending_its_last_line() {
        assert_eq!(written("\ta\r\tb"), "    a\r    b\r");
        for source in ["    a\r\n    b\r\n\r\nc", "    a\r    b\r\rc"] {
            written(source);
        }
        let mut crlf = SerializeOptions::default();
        crlf.line_ending = LineEnding::CrLf;
        let document = parse("```\r\na\r\n```\r\nb").document;
        let markdown = document
            .to_markdown_with(&crlf)
            .expect("document serializes");
        assert_eq!(markdown, "```\r\na\r\n```\r\n\r\nb\r\n");
    }

    #[test]
    fn a_bare_destination_writes_a_space_as_a_reference() {
        assert_eq!(written("[o]:&#x20;"), "[o]: &#x20;\n");
        let document = parse("[o]: &#x20;").document;
        let [Block::Definition(definition)] = document.children.as_slice() else {
            panic!("{document:?}");
        };
        assert_eq!(definition.destination, " ");
        assert_eq!(definition.destination_kind, LinkDestinationKind::Bare);
    }

    #[test]
    fn an_html_block_value_is_written_verbatim() {
        written("<!--\n\n");
        written("<div>\n  a  \n</div>");
        let document = parse(&written("<!--\n\n")).document;
        let [Block::HtmlBlock(html)] = document.children.as_slice() else {
            panic!("{document:?}");
        };
        assert_eq!(html.value, "<!--\n");
    }
}

mod recorded_spellings {
    use super::*;

    fn emphasis(delimiter: EmphasisDelimiter, children: Vec<Inline>) -> Inline {
        Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            delimiter,
            children,
        })
    }

    fn strong(delimiter: EmphasisDelimiter, children: Vec<Inline>) -> Inline {
        Inline::Strong(Strong {
            meta: NodeMeta::default(),
            delimiter,
            children,
        })
    }

    const STAR: EmphasisDelimiter = EmphasisDelimiter::Asterisk;
    const UNDERSCORE: EmphasisDelimiter = EmphasisDelimiter::Underscore;

    #[test]
    fn delimiters_the_ast_records_are_written() {
        assert_eq!(written("_a_ __b__"), "_a_ __b__\n");
        assert_eq!(written("__*a*__"), "__*a*__\n");
        assert_eq!(written("__#$***~**b~**__|#"), "__#$***~**b~**__|#\n");
        assert_eq!(written("foo-_(bar)_"), "foo-_(bar)_\n");
    }

    #[test]
    fn strong_around_emphasis_reads_back_with_distinct_delimiters() {
        let built =
            |inner| paragraph_document(vec![strong(STAR, vec![emphasis(inner, vec![text("em")])])]);
        assert_eq!(assert_reads_back(&built(UNDERSCORE)), "**_em_**\n");
        // With one delimiter the runs merge and read back the other way round.
        assert_eq!(assert_reads_back_otherwise(&built(STAR)), "***em***\n");
    }

    #[test]
    fn adjacent_emphasis_reads_back_with_distinct_delimiters() {
        let built = |second| {
            paragraph_document(vec![
                emphasis(STAR, vec![text("a")]),
                emphasis(second, vec![text("b")]),
            ])
        };
        assert_eq!(assert_reads_back(&built(UNDERSCORE)), "*a*_b_\n");
        assert_eq!(assert_reads_back_otherwise(&built(STAR)), "*a**b*\n");
    }

    #[test]
    fn literal_delimiters_after_emphasis_are_built_as_escapes() {
        let built = |after: Vec<Inline>| {
            let mut children = vec![emphasis(STAR, vec![text("a")])];
            children.extend(after);
            paragraph_document(children)
        };
        assert_eq!(
            assert_reads_back(&built(vec![escape('*'), text("b")])),
            "*a*\\*b\n"
        );
        assert_eq!(
            assert_reads_back_otherwise(&built(vec![text("*b")])),
            "*a**b\n"
        );

        let nested = |after: Vec<Inline>| {
            let mut children = vec![emphasis(STAR, vec![strong(STAR, vec![text("(a b)_.")])])];
            children.extend(after);
            paragraph_document(children)
        };
        assert_eq!(
            assert_reads_back(&nested(vec![escape('*'), text("#")])),
            "***(a b)_.***\\*#\n"
        );
        // `****` there is no closer, so this one reads back too.
        assert_eq!(
            assert_reads_back(&nested(vec![text("*#")])),
            "***(a b)_.****#\n"
        );
    }

    #[test]
    fn a_trailing_hash_in_an_atx_heading_is_built_as_an_escape() {
        let heading = |children| {
            document(vec![Block::Heading(Heading {
                meta: NodeMeta::default(),
                depth: 1,
                kind: HeadingKind::Atx,
                children,
            })])
        };
        assert_eq!(
            assert_reads_back(&heading(vec![text("foo "), escape('#')])),
            "# foo \\#\n"
        );
        assert_eq!(
            assert_reads_back_otherwise(&heading(vec![text("foo #")])),
            "# foo #\n"
        );
    }

    #[test]
    fn link_forms_the_ast_records_are_written() {
        assert_eq!(written("see http://a.b"), "see http://a.b\n");
        assert_eq!(written("www.a.b"), "www.a.b\n");
        assert_eq!(written("a@b.c"), "a@b.c\n");
        assert_eq!(written("<a@b.c> <http://a.b>"), "<a@b.c> <http://a.b>\n");
        assert_eq!(
            written("[http://a.b](http://a.b)"),
            "[http://a.b](http://a.b)\n"
        );
        assert_eq!(written("[x](<http://a.b>)"), "[x](<http://a.b>)\n");
        // Other schemes are no literal autolink.
        assert_eq!(written("a://x"), "a://x\n");
        let constructed = paragraph_document(vec![Inline::Link(Link::new("u", [text("a")]))]);
        assert_eq!(assert_reads_back(&constructed), "[a](u)\n");
    }

    #[test]
    fn references_keep_their_kind_and_label() {
        assert_eq!(
            written("[f&#246;o]\n\n[f&#246;o]: /url\n"),
            "[f&#246;o]\n\n[f&#246;o]: /url\n"
        );
        assert_eq!(
            written("[text][Ref]\n\n[ref]: /url\n"),
            "[text][Ref]\n\n[ref]: /url\n"
        );
        let markdown =
            written("Use [text][Foo\\]] and [t][A &amp; B].\n\n[Foo\\]]: /a\n\n[A &amp; B]: /b\n");
        assert!(markdown.contains("[text][Foo\\]]"));
        assert!(markdown.contains("[t][A &amp; B]"));
    }

    #[test]
    fn a_reference_without_its_definition_is_written_as_a_reference() {
        for kind in [
            ReferenceKind::Shortcut,
            ReferenceKind::Collapsed,
            ReferenceKind::Full,
        ] {
            let reference = Inline::LinkReference(LinkReference {
                meta: NodeMeta::default(),
                identifier: "foo".into(),
                label: "foo".into(),
                kind,
                children: vec![text("foo")],
            });
            let expected = match kind {
                ReferenceKind::Shortcut => "[foo]\n",
                ReferenceKind::Collapsed => "[foo][]\n",
                ReferenceKind::Full => "[foo][foo]\n",
            };
            assert_eq!(
                paragraph_document(vec![reference]).to_markdown().unwrap(),
                expected
            );
        }
    }

    #[test]
    fn references_before_literal_text_read_back_when_the_text_is_built_with_escapes() {
        let reference = || {
            Inline::LinkReference(LinkReference {
                meta: NodeMeta::default(),
                identifier: "foo".into(),
                label: "foo".into(),
                kind: ReferenceKind::Shortcut,
                children: vec![text("foo")],
            })
        };
        let defined = |inlines: Vec<Inline>| {
            document(vec![
                paragraph(inlines),
                Block::Definition(Definition {
                    meta: NodeMeta::default(),
                    label: "foo".into(),
                    identifier: "foo".into(),
                    destination: "/u".into(),
                    destination_kind: LinkDestinationKind::Bare,
                    title: None,
                    title_kind: None,
                }),
            ])
        };
        assert_reads_back(&defined(vec![reference(), escape('('), text("a)")]));
        assert_reads_back(&defined(vec![reference(), escape(':'), text(" /x")]));
        assert_reads_back_otherwise(&defined(vec![reference(), text("(a)")]));
        let image = Inline::ImageReference(ImageReference {
            meta: NodeMeta::default(),
            identifier: "foo".into(),
            label: "foo".into(),
            kind: ReferenceKind::Shortcut,
            alt: vec![text("foo")],
        });
        assert_reads_back(&defined(vec![image, escape('('), text("a)")]));

        // Brackets in text read as a reference where a definition matches.
        let bracketed = defined(vec![text("[x]")]);
        let mut with_x = bracketed.clone();
        if let Block::Definition(definition) = &mut with_x.children[1] {
            definition.label = "x".into();
            definition.identifier = "x".into();
        }
        assert_eq!(assert_reads_back_otherwise(&with_x), "[x]\n\n[x]: /u\n");
        assert_eq!(assert_reads_back(&bracketed), "[x]\n\n[foo]: /u\n");
    }

    #[test]
    fn thematic_breaks_keep_their_marker() {
        let rule = |marker| {
            Block::ThematicBreak(ThematicBreak {
                meta: NodeMeta::default(),
                marker,
            })
        };
        let after_paragraph = document(vec![
            paragraph(vec![text("intro")]),
            rule(ThematicBreakMarker::Dash),
        ]);
        assert_eq!(assert_reads_back(&after_paragraph), "intro\n\n---\n");
        // A contiguous `---` at the document start would open frontmatter.
        assert_eq!(
            assert_reads_back(&document(vec![rule(ThematicBreakMarker::Dash)])),
            "- - -\n"
        );
        // `* ***` would read as one longer break, so the item starts on the
        // line after its marker.
        let list = Block::List(List {
            meta: NodeMeta::default(),
            ordered: false,
            start: None,
            delimiter: ListDelimiter::Asterisk,
            tight: true,
            children: vec![ListItem {
                meta: NodeMeta::default(),
                checked: None,
                children: vec![rule(ThematicBreakMarker::Asterisk)],
            }],
        });
        assert_eq!(assert_reads_back(&document(vec![list])), "*\n  ***\n");
        assert_eq!(written("- a\n  - ---"), "- a\n  - - -\n");
    }

    #[test]
    fn a_nested_bullet_is_written_as_a_list() {
        let list = |children| {
            Block::List(List {
                meta: NodeMeta::default(),
                ordered: false,
                start: None,
                delimiter: ListDelimiter::Asterisk,
                tight: true,
                children: vec![ListItem {
                    meta: NodeMeta::default(),
                    checked: None,
                    children,
                }],
            })
        };
        let markdown = document(vec![list(vec![list(Vec::new())])])
            .to_markdown()
            .unwrap();
        assert_eq!(markdown, "* * \n");
    }

    #[test]
    fn fenced_code_honors_its_marker() {
        let document = document(vec![Block::CodeBlock(CodeBlock {
            meta: NodeMeta::default(),
            kind: CodeBlockKind::Fenced {
                marker: FenceMarker::Tilde,
                length: 3,
            },
            info: None,
            value: "code\n".into(),
        })]);
        assert_eq!(assert_reads_back(&document), "~~~\ncode\n~~~\n");
    }

    #[test]
    fn heading_soft_breaks_are_written_as_spaces() {
        let soft_break = || {
            Inline::SoftBreak(SoftBreak {
                meta: NodeMeta::default(),
            })
        };
        let heading = |depth, kind| {
            document(vec![Block::Heading(Heading {
                meta: NodeMeta::default(),
                depth,
                kind,
                children: vec![text("foo"), soft_break(), text("bar")],
            })])
        };
        assert_eq!(
            assert_reads_back(&heading(1, HeadingKind::Setext)),
            "foo bar\n=======\n"
        );
        assert_eq!(
            assert_reads_back(&heading(1, HeadingKind::Atx)),
            "# foo bar\n"
        );
        // A setext underline writes depth 1 or 2 only.
        assert_eq!(
            document(vec![Block::Heading(Heading {
                meta: NodeMeta::default(),
                depth: 3,
                kind: HeadingKind::Setext,
                children: vec![text("foo")],
            })])
            .to_markdown()
            .unwrap(),
            "### foo\n"
        );
        assert_eq!(written("a\nb\n==="), "a b\n===\n");
        assert_eq!(
            written("Foo *bar\nbaz*\n===="),
            "Foo *bar baz*\n=============\n"
        );
    }
}

mod text_as_recorded {
    use super::*;

    #[test]
    fn hand_built_text_is_written_as_it_is() {
        for value in [
            "x_y_ a*b x^2 ~5",
            "a+b = c, #tag, wow!, a | b, a < b, C++ and x^2 ~ y & z",
            "Invalid &unknown; &copy and &#x; stay text.",
            "b ``a`",
        ] {
            let markdown = assert_reads_back(&paragraph_document(vec![text(value)]));
            assert_eq!(markdown, format!("{value}\n"));
        }
        for (value, expected) in [("==a==", "==a==\n"), ("*not emphasis*", "*not emphasis*\n")] {
            let markdown = assert_reads_back_otherwise(&paragraph_document(vec![text(value)]));
            assert_eq!(markdown, expected);
        }
        let split = paragraph_document(vec![text("a"), text("b")]);
        assert_eq!(assert_reads_back(&split), "ab\n");
    }

    #[test]
    fn escapes_and_references_are_written_as_recorded() {
        assert_eq!(written("a\\.b \\#tag"), "a\\.b \\#tag\n");
        assert_eq!(written("&#35;tag &amp; x"), "&#35;tag &amp; x\n");
        assert_eq!(
            written("Test \\`hello world` here."),
            "Test \\`hello world` here.\n"
        );
        assert_eq!(written("x_y_ a*b x^2 ~5"), "x_y_ a*b x^2 ~5\n");
    }

    #[test]
    fn neighbours_that_read_as_syntax_are_written_as_they_are() {
        let shortcode = Inline::Shortcode(Shortcode {
            meta: NodeMeta::default(),
            name: "smile".into(),
        });
        assert_eq!(
            assert_reads_back_otherwise(&paragraph_document(vec![text("a"), shortcode])),
            "a:smile:\n"
        );
        let wikilink = Inline::WikiLink(WikiLink {
            meta: NodeMeta::default(),
            target: "x".into(),
            label: "x".into(),
            embed: false,
        });
        assert_eq!(
            assert_reads_back_otherwise(&paragraph_document(vec![text("a!"), wikilink.clone()])),
            "a![[x]]\n"
        );
        assert_eq!(
            assert_reads_back(&paragraph_document(vec![text("a"), escape('!'), wikilink])),
            "a\\![[x]]\n"
        );
        assert_eq!(written("see ![[x.png]]"), "see ![[x.png]]\n");
    }

    #[test]
    fn invalid_shapes_are_rejected_before_rendering() {
        let link = |children: Vec<Inline>| Inline::Link(Link::new("/u", children));
        let nested = paragraph_document(vec![link(vec![link(vec![text("a")])])]);
        assert!(matches!(
            nested.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));
        let spaced = paragraph_document(vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            delimiter: EmphasisDelimiter::Asterisk,
            children: vec![text("a ")],
        })]);
        assert!(matches!(
            spaced.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));
    }
}

mod former_tilde_scenarios {
    use super::*;

    #[test]
    fn single_tildes_stay_text() {
        assert_eq!(
            written("This ~text~~~~ is ~~~~curious~.\n"),
            "This ~text~~~~ is ~~~~curious~.\n"
        );
        assert_eq!(written("a ~~two/one~ b\n"), "a ~~two/one~ b\n");
    }

    #[test]
    fn a_delete_is_written_with_two_tildes() {
        let delete = |value: &str| {
            paragraph_document(vec![Inline::Delete(Delete {
                meta: NodeMeta::default(),
                children: vec![text(value)],
            })])
        };
        assert_eq!(delete("text").to_markdown().unwrap(), "~~text~~\n");
        // Text ending in tildes lengthens the closing run.
        assert_eq!(
            assert_reads_back_otherwise(&delete("text~~~")),
            "~~text~~~~~\n"
        );
        assert_eq!(
            assert_reads_back(&delete("text~~~~ is ~~~~curious")),
            "~~text~~~~ is ~~~~curious~~\n"
        );
    }
}

mod parsed_inputs {
    //! Parsed inputs of earlier serializer defects. Each reads back, or is
    //! listed in `NOT_READING_BACK` with its reason.

    use super::*;

    #[test]
    fn the_spelling_of_parsed_inputs_is_kept() {
        for (source, expected) in [
            ("[foo]\\(a)\n\n[foo]: /u", "[foo]\\(a)\n\n[foo]: /u\n"),
            ("![foo]\\(a)\n\n[foo]: /u", "![foo]\\(a)\n\n[foo]: /u\n"),
            ("[foo]\\: /x\n\n[foo]: /u", "[foo]\\: /x\n\n[foo]: /u\n"),
            ("a |\n-", "a |\n---\n"),
            ("&#x20;\na", "&#x20;\na\n"),
            ("&#x20; \na", "&#x20;\na\n"),
            ("a\n&#x20;\nb", "a\n&#x20;\nb\n"),
            ("[\nfoo](u)", "[\nfoo](u)\n"),
            ("`x`<div", "`x`<div\n"),
            ("a *b*::c", "a *b*::c\n"),
            ("++a:++ b:", "++a:++ b:\n"),
            ("# Title\n\nHello *world*.", "# Title\n\nHello *world*.\n"),
            ("+ a", "+ a\n"),
            ("y***b***", "y***b***\n"),
            ("***y*b", "***y*b\n"),
            ("[^`]``", "[^`]``\n"),
            ("![[$[]]a$>", "![[$[]]a$>\n"),
            ("-\n   <v>", "-\n   <v>\n"),
            ("<a>&#x20;\n;", "<a>&#x20;\n;\n"),
            ("a**~**", "a**~**\n"),
            ("b*~*", "b*~*\n"),
            ("-\n  ---", "-\n  ---\n"),
            ("==a\\== b==", "==a\\== b==\n"),
            ("++a\\++ b++", "++a\\++ b++\n"),
            ("[o]:u\n\t$$\na$$", "[o]: u\n\n$$\na$$\n"),
            ("- a\n  - b\n   <div>", "- a\n  - b\n   <div>\n"),
            ("^://y ^", "^://y ^\n"),
            ("^://. ^", "^://. ^\n"),
            (":e!://}", ":e!://}\n"),
            (
                "[^1]: **^]]| a |- `a[^1]: ^://<",
                "[^1]: **^]]| a |- `a[^1]: ^://<\n",
            ),
            (
                "**Note:** use snake_case: here",
                "**Note:** use snake_case: here\n",
            ),
            ("*Warning:* set MY_VAR: 1", "*Warning:* set MY_VAR: 1\n"),
            ("a +\nb + c", "a +\nb + c\n"),
            ("a =\nb = c", "a =\nb = c\n"),
            ("if a == b\nthen c== d", "if a == b\nthen c== d\n"),
            (
                "**See [docs] and `cfg`** then use \\` quote\n\n[docs]: /u",
                "**See [docs] and `cfg`** then use \\` quote\n\n[docs]: /u\n",
            ),
        ] {
            assert_eq!(written(source), expected, "{source:?}");
        }
        for source in ["=```\n    ```", "(\n    <div>", "- a\n\n  <!--\n- b"] {
            written(source);
        }
        // A code span is written from its value, whose line endings are
        // spaces, so its continuation lines cannot open a fence.
        let fenced = parse(&format!("a `{}`", "\n    ~~~".repeat(40))).document;
        assert_eq!(
            assert_reads_back(&fenced),
            format!("a `{}`\n", " ~~~".repeat(40))
        );
        for source in [
            "-\n  ---\n-\n  ---",
            "- a\n\n-\n  ---",
            "> - a\n> -\n>   ---",
            "- - a\n  -\n    ---",
        ] {
            assert_eq!(written(source), format!("{source}\n"));
        }
        written(&"-\n  ---\n\nx\n\n".repeat(40));
        written(&"-\n  ---\n".repeat(40));
    }

    #[test]
    fn a_parsed_code_span_is_written_from_its_value() {
        for (source, expected) in [
            ("``\nfoo\nbar\n``", "`foo bar`\n"),
            ("`` a`b ``", "``a`b``\n"),
            ("```a``b```", "`a``b`\n"),
            ("``  a  ``", "`  a  `\n"),
            ("`` `a ``", "`` `a ``\n"),
            // A fence never closes a backtick run written before it.
            ("`foo``bar``", "`foo``bar``\n"),
            ("x` and ``y`` and ``z``", "x` and ``y`` and ``z``\n"),
            ("\\`` `a`", "\\`` `a`\n"),
            ("\\` `a`", "\\` `a`\n"),
            ("$`a`$ <b c='`'> `d`", "$`a`$ <b c='`'> `d`\n"),
        ] {
            assert_eq!(written(source), expected, "{source:?}");
        }
    }

    #[test]
    fn parsed_documents_read_back() {
        for source in [
            // Delimiter runs, escapes, and references.
            "*a****a*a*v",
            "_)***&b***a_",
            "&(_.b***b***._",
            "_,_[www.x.com![{",
            "&___ab**~&**__~#b",
            "*****$___&___*_*(***",
            "***___(_~***a",
            "**\t*$",
            "+*(**\0",
            "__***-*",
            "(*~\n**)",
            "**:\n**:",
            "($$]$=",
            "[\\||>||)||",
            "__**)**&__",
            "**:__$__**",
            "****(*+***",
            "***_|_***",
            "__***/***__",
            "**#****]***_**",
            "***_\\**#*",
            "**__\u{0}___**",
            "***b_*_b_*",
            "__<__y_`__",
            "_# _*#***___",
            "_^*^*_c__",
            "y*x***a_ b**",
            "***)__\\_#__*b***",
            "**~***|_a* ",
            "**__a__~~**b",
            "[__**)**&__](u)",
            "![__**)**&__](u)",
            "==__***/***__==",
            "*******b_*_c~_y",
            "__**a*********___",
            "y__**__**y****___",
            "*a***b**",
            "*c*__&__",
            // Literal autolinks and the text around them.
            "://[\t:e",
            "(:w!://[",
            "{:e>://[",
            "a@b.c://x\t:e",
            "^://]^",
            "b]]__目[a@b.c://{:e",
            "www.x.com-->^[  ://![\t:e<!--#",
            "^://*| a |[*```^\t[x] <div>~",
            "&#x20;://y",
            "://\\:://y",
            "://\\::p:",
            "://\t[",
            "://&amp;",
            "\"://&amp;\"",
            "www.}",
            "a.b@c.d&#x5f;",
            "a\\-://`",
            "ab&#99;://x",
            "*://*&mp;",
            "**://**&mp;",
            "://^&mp;",
            "://~&mp;~",
            "://__&mp;__",
            "://~&mp;&p;~",
            "://&#x0;&mp;",
            "www.\\[]_(",
            "**a *b*www.x.com**",
            "**a *b*x@y.com**",
            "**x@y.com***x@y.com*",
            "**\\*www.x.com**",
            "\\\\&#33;[a](b)",
            "://~ #~",
            "_&#x20;://_",
            "||://y\t||",
            "==&#x20;://<==",
            "^http://x ^",
            "*&#x20;http://x*",
            "://\\~||>||",
            "://\\)||#||",
            "__://| a |a@b.c_[[",
            "_www.x.com__http://x<!-- `[a]: /u[^1]: [x] ",
            "[foo]:`\n[foo]^://y\t^",
            ":e!://www.x.com# [[~[x] >> !^~~  ",
            // Containers, items, and blocks.
            "~ \n\n>  > \t<!--[x] 2) *   :::e",
            ">* \t[x] : # <div>a\n> |-|",
            "-$$\n    $$",
            "-\t(\n  <v>",
            "*\t<a>",
            "*\t<a>\n  <v>",
            "- >**\n`",
            "1. >)\n~",
            "><!--\n>```",
            "`\n|`\n-",
            "---\n \n\n---",
            "[o]:u\n\t<div>",
            "[o]:u\n<a>\n-",
            "<a>&#x20;\n[\n-",
            "[o]: u\n<a>",
            "a\n   : `",
            "~\n: ]",
            "``\n   ~\t``",
            "[\n~ _\n    ~~~",
            "a\n\\<div>",
            "a\n\\<!-- b",
            "a\n\\::b",
            "- > a\n  >\n  b\n  ---",
            "- > a\n  >\n  | b |\n  | - |",
            // Extension constructs and text that would open one.
            ":b[",
            "\\:p",
            "( \\:a",
            "$\\<!--\n-->",
            "`\\$[<a>[$>",
            "\\:p://",
            ":b\\:",
            ":\\+:",
            "[[\\&mp;]]",
            "[^:](\\)",
            "a\\://",
            "^:// ^",
            "||://\t||",
            "&#x20; :b",
            "==)\\==^==",
            "||*\\||:||",
            "://y \\\n|",
            "++&#x20;\nd++",
            "_&#x20;\n=_",
            "d_~_",
            "b*~~~***",
            "~~a~~~",
            "~~~a",
            "://\\||[\n-|-",
            "|<!--\\|-->\n--",
            "$\\|$||\n-|-",
            "[a`]``\n\n[a`]: x",
            "[^`]: x\n\n[^`]``",
            "://`\\`",
            "b**~\n~**",
            "b_~~_~",
            "~\nb*~***",
            "a__~>__~",
            "t_~>___~",
            "~~目*~***",
            "a*~ **&*",
            ":\\^:^[|]",
            "|\\<!--[^-->]",
            "*\\$#$>$",
            "~\\$#://$",
            "b\\-p://",
            ")||||||\t||",
            "**(__)__$**",
            "*{_~_目*",
            "`\\:++:++",
            "|\\$*`$`",
            "\u{c}",
            "a\u{c}",
            "\u{c}:a",
            "[^\u{c}]",
            "://y\u{c}c",
            "1. \u{c}",
            "==&#x20; \n-==",
            "[;\u{c}]:[",
            "[\u{c}]:\u{a0}",
            "d$![[\\$]]",
            "++@b.c",
            ">[!NOTE]\u{c}",
            ">[!NOTE]+\t*",
            ":e\\{",
            ":e{}1",
            ":e[]www.+",
            ":e{}a@b.c",
            ":e{}[^1]",
            "]\\-a@b.c",
            "~\\+a@b.c",
            "\\+@b.p://",
            "1++1://u 1++",
            "]\n: :::e",
            "[^1]:| &#x20;\n:",
            ":::t\n```\n:::e",
            "_`*www._",
            "# _*www._",
            "**://\\***[^1]",
            "^[www.[ ]",
            "1. >[!NOTE]\n~[^1]",
            "+ [x]  :e",
            "- [ ] &#x20;",
            "***:*:**",
            "++*++_@b.c",
            "://\\::+1:",
            "| <a b=\"x\\\\\\|y\"> |\n| --- |",
            "| x |\n| --- |\n| $a\\\\\\|b$ |",
            "| <http://x\\\\\\|y> |\n|-|",
            "[foo`bar] *&#96;*\n\n[foo`bar]: /u",
            "*[foo`bar]* &#96;\n\n[foo`bar]: /u",
            "[^a`b] *&#96;*\n\n[^a`b]: x",
            "-[^\\`]://\\`",
            "++:++\\:",
            "&#x20;://>|>\n-|-",
            "- *  (\n    <a>",
            "~~:~ :e~",
            "~\t:e~",
            "++\\+>++",
            "==&#61;==",
            "++&#43;++",
            ">\n>[!NOTE]:>",
            "| h |\n| - |\n| **a**__b__ |",
            "::d[**a**__b__]",
            "Term **a**__b__\n: def",
            "Term\n: **a**__b__",
            "**=* ++@b.c*",
            "~&#x20;://~",
            // Former MDX inputs.
            " import -",
            " export x",
            " import *\n-",
            "<!--@b>",
            "\\{[]()}",
            "\u{a0}&#x20;<p/>",
            "{}&#x20;\n\\",
            "{}&#x20; \n\\",
        ] {
            written(source);
        }
    }
}
