//! Serializer regression coverage, AST → Markdown: hand-built trees and the
//! exact Markdown each is written as, for value encodings (fences,
//! destinations, titles, labels, and pipes in table cells) and the recorded
//! spellings the serializer writes.
//!
//! The serializer only renders: text, escapes, and character references are
//! written as recorded, and a hand-built tree whose text reads as syntax is
//! written as it is. Parsed inputs, and whether Markdown reads back, are
//! checked in `fixtures.rs`.

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

/// The Markdown `document` serializes to.
fn rendered(document: &Document) -> String {
    document.to_markdown().expect("document serializes")
}

// The table-cell cases in `value_encodings` still parse, and check read-back,
// with the helpers below; every other case asserts the exact output.

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

/// The Markdown `source` parses and serializes to.
fn written(source: &str) -> String {
    parse(source)
        .document
        .to_markdown()
        .unwrap_or_else(|error| panic!("{source:?}: {error:?}"))
}

mod value_encodings {
    use super::*;

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

        assert_eq!(
            rendered(&document),
            "$$$$\n$$\n$$$\na $$ b\n$$$$\n\n$`a $$ b`$\n"
        );
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

        assert_eq!(
            rendered(&document),
            ":::::note\n```\nbefore\n:::\n::::\nafter\n```\n:::::\n"
        );
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

        assert_eq!(
            rendered(&document),
            "[foo]: <my url> 'single title'\n\n[angle](<foo bar> (paren title)) ![empty]( \"empty title\")\n"
        );
    }

    #[test]
    fn ordinary_at_text_stays_text() {
        assert_eq!(
            rendered(&paragraph_document(vec![text("This@that.")])),
            "This@that.\n"
        );
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
        // holding an unescaped bracket reads back as something else. A label
        // may span lines, and its escapes stay as written.
        for (label, identifier, expected) in [
            ("a]b\\c[d", "a]b\\c[d", "[a]b\\c[d]: /u\n"),
            ("line\nbreak", "line break", "[line\nbreak]: /u\n"),
            ("a\\]b\\\\c\\[d", "a\\]b\\\\c\\[d", "[a\\]b\\\\c\\[d]: /u\n"),
        ] {
            assert_eq!(rendered(&definition(label, identifier)), expected);
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
        // A label holding an unescaped bracket or whitespace reads back as
        // something else.
        assert_eq!(
            rendered(&footnote("a]b\\c[d")),
            "See [^a]b\\c[d]\n\n[^a]b\\c[d]: note\n"
        );
        assert_eq!(
            rendered(&footnote("white space")),
            "See [^white space]\n\n[^white space]: note\n"
        );
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
    fn strong_around_emphasis_keeps_distinct_delimiters() {
        let built =
            |inner| paragraph_document(vec![strong(STAR, vec![emphasis(inner, vec![text("em")])])]);
        assert_eq!(rendered(&built(UNDERSCORE)), "**_em_**\n");
        // With one delimiter the runs merge and read back the other way round.
        assert_eq!(rendered(&built(STAR)), "***em***\n");
    }

    #[test]
    fn adjacent_emphasis_keeps_distinct_delimiters() {
        let built = |second| {
            paragraph_document(vec![
                emphasis(STAR, vec![text("a")]),
                emphasis(second, vec![text("b")]),
            ])
        };
        assert_eq!(rendered(&built(UNDERSCORE)), "*a*_b_\n");
        assert_eq!(rendered(&built(STAR)), "*a**b*\n");
    }

    #[test]
    fn literal_delimiters_after_emphasis_are_built_as_escapes() {
        let built = |after: Vec<Inline>| {
            let mut children = vec![emphasis(STAR, vec![text("a")])];
            children.extend(after);
            paragraph_document(children)
        };
        assert_eq!(rendered(&built(vec![escape('*'), text("b")])), "*a*\\*b\n");
        assert_eq!(rendered(&built(vec![text("*b")])), "*a**b\n");

        let nested = |after: Vec<Inline>| {
            let mut children = vec![emphasis(STAR, vec![strong(STAR, vec![text("(a b)_.")])])];
            children.extend(after);
            paragraph_document(children)
        };
        assert_eq!(
            rendered(&nested(vec![escape('*'), text("#")])),
            "***(a b)_.***\\*#\n"
        );
        // `****` there is no closer, so this one reads back too.
        assert_eq!(rendered(&nested(vec![text("*#")])), "***(a b)_.****#\n");
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
            rendered(&heading(vec![text("foo "), escape('#')])),
            "# foo \\#\n"
        );
        assert_eq!(rendered(&heading(vec![text("foo #")])), "# foo #\n");
    }

    #[test]
    fn a_constructed_link_is_written_inline() {
        let constructed = paragraph_document(vec![Inline::Link(Link::new("u", [text("a")]))]);
        assert_eq!(rendered(&constructed), "[a](u)\n");
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
    fn references_before_literal_text_are_followed_by_its_escapes() {
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
        assert_eq!(
            rendered(&defined(vec![reference(), escape('('), text("a)")])),
            "[foo]\\(a)\n\n[foo]: /u\n"
        );
        assert_eq!(
            rendered(&defined(vec![reference(), escape(':'), text(" /x")])),
            "[foo]\\: /x\n\n[foo]: /u\n"
        );
        // Without the escape, the text reads as the reference's destination.
        assert_eq!(
            rendered(&defined(vec![reference(), text("(a)")])),
            "[foo](a)\n\n[foo]: /u\n"
        );
        let image = Inline::ImageReference(ImageReference {
            meta: NodeMeta::default(),
            identifier: "foo".into(),
            label: "foo".into(),
            kind: ReferenceKind::Shortcut,
            alt: vec![text("foo")],
        });
        assert_eq!(
            rendered(&defined(vec![image, escape('('), text("a)")])),
            "![foo]\\(a)\n\n[foo]: /u\n"
        );

        // Brackets in text read as a reference where a definition matches.
        let bracketed = defined(vec![text("[x]")]);
        let mut with_x = bracketed.clone();
        if let Block::Definition(definition) = &mut with_x.children[1] {
            definition.label = "x".into();
            definition.identifier = "x".into();
        }
        assert_eq!(rendered(&with_x), "[x]\n\n[x]: /u\n");
        assert_eq!(rendered(&bracketed), "[x]\n\n[foo]: /u\n");
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
        assert_eq!(rendered(&after_paragraph), "intro\n\n---\n");
        // A contiguous `---` at the document start would open frontmatter.
        assert_eq!(
            rendered(&document(vec![rule(ThematicBreakMarker::Dash)])),
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
        assert_eq!(rendered(&document(vec![list])), "*\n  ***\n");
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
        assert_eq!(rendered(&document), "~~~\ncode\n~~~\n");
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
            rendered(&heading(1, HeadingKind::Setext)),
            "foo bar\n=======\n"
        );
        assert_eq!(rendered(&heading(1, HeadingKind::Atx)), "# foo bar\n");
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
            assert_eq!(
                rendered(&paragraph_document(vec![text(value)])),
                format!("{value}\n")
            );
        }
        // These read back as a `Mark` and an `Emphasis`.
        for (value, expected) in [("==a==", "==a==\n"), ("*not emphasis*", "*not emphasis*\n")] {
            assert_eq!(rendered(&paragraph_document(vec![text(value)])), expected);
        }
        let split = paragraph_document(vec![text("a"), text("b")]);
        assert_eq!(rendered(&split), "ab\n");
    }

    #[test]
    fn neighbours_that_read_as_syntax_are_written_as_they_are() {
        let shortcode = Inline::Shortcode(Shortcode {
            meta: NodeMeta::default(),
            name: "smile".into(),
        });
        assert_eq!(
            rendered(&paragraph_document(vec![text("a"), shortcode])),
            "a:smile:\n"
        );
        let wikilink = Inline::WikiLink(WikiLink {
            meta: NodeMeta::default(),
            target: "x".into(),
            label: "x".into(),
            embed: false,
        });
        assert_eq!(
            rendered(&paragraph_document(vec![text("a!"), wikilink.clone()])),
            "a![[x]]\n"
        );
        assert_eq!(
            rendered(&paragraph_document(vec![text("a"), escape('!'), wikilink])),
            "a\\![[x]]\n"
        );
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
    fn a_delete_is_written_with_two_tildes() {
        let delete = |value: &str| {
            paragraph_document(vec![Inline::Delete(Delete {
                meta: NodeMeta::default(),
                children: vec![text(value)],
            })])
        };
        assert_eq!(delete("text").to_markdown().unwrap(), "~~text~~\n");
        // Text ending in tildes lengthens the closing run.
        assert_eq!(rendered(&delete("text~~~")), "~~text~~~~~\n");
        assert_eq!(
            rendered(&delete("text~~~~ is ~~~~curious")),
            "~~text~~~~ is ~~~~curious~~\n"
        );
    }
}
