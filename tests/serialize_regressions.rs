//! Serializer round-trip regression coverage: math/directive/table fences,
//! delete-marker handling, escape rules for labels and pipes, and the
//! serializer defects from the review pass.
//!
//! Each former regression file is preserved verbatim inside its own `mod` so
//! that helper functions and test names cannot collide across the merged
//! sources.

mod serializer {
    use markdown_syntax::*;

    fn text(value: &str) -> Inline {
        Inline::Text(Text {
            meta: NodeMeta::default(),
            value: value.into(),
        })
    }

    fn paragraph(children: Vec<Inline>) -> Block {
        Block::Paragraph(Paragraph {
            meta: NodeMeta::default(),
            children,
        })
    }

    fn math_options() -> SyntaxOptions {
        let mut constructs = Constructs::commonmark();
        constructs.math_block = true;
        constructs.math_inline = true;
        SyntaxOptions {
            constructs: constructs,
            parse: ParseOptions::default(),
        }
    }

    fn directive_options() -> SyntaxOptions {
        let mut constructs = Constructs::commonmark();
        constructs.directive_container = true;
        SyntaxOptions {
            constructs: constructs,
            parse: ParseOptions::default(),
        }
    }

    fn underline_options() -> SyntaxOptions {
        let mut constructs = Constructs::commonmark();
        constructs.underline = true;
        SyntaxOptions {
            constructs: constructs,
            parse: ParseOptions::default(),
        }
    }

    fn parse_document(markdown: &str, options: &SyntaxOptions) -> Document {
        let output = options.parse(markdown);
        assert_eq!(output.diagnostics, Vec::new());
        output.document
    }

    fn assert_single_tilde_delete_with_internal_runs_shape(document: &Document) {
        assert!(
            matches!(
            &document.children[..],
            [Block::Paragraph(Paragraph {
                children,
                ..
            })] if matches!(
                &children[..],
                [
                    Inline::Text(Text { value: before, .. }),
                    Inline::Delete(Delete {
                        children: delete_children,
                        marker: DeleteMarker::SingleTilde,
                        ..
                    }),
                    Inline::Text(Text { value: after, .. }),
                ] if before == "This "
                    && matches!(&delete_children[..], [Inline::Text(Text { value, .. })] if value == "text~~~~ is ~~~~curious")
                    && after == "."
            )
            ),
            "unexpected document shape: {document:#?}"
        );
    }

    #[test]
    fn list_markers_preserve_by_default_avoid_merging_and_can_be_overridden() {
        let input = "- a\n\n+ b\n\n* c\n";
        let document = parse_document(input, &SyntaxOptions::commonmark());

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, input);

        let reparsed = parse_document(&markdown, &SyntaxOptions::commonmark());
        assert_eq!(reparsed.children.len(), 3);
        assert!(reparsed
            .children
            .iter()
            .all(|block| matches!(block, Block::List(_))));

        let mut options = SerializeOptions::default();
        options.bullet = ListDelimiter::Plus;
        let overridden = document
            .to_markdown_with(&options)
            .expect("document serializes with options");
        assert_eq!(overridden, "+ a\n\n+ b\n\n+ c\n");
    }

    #[test]
    fn math_serialization_uses_parseable_fences() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![
                Block::MathBlock(MathBlock {
                    meta: NodeMeta::default(),
                    value: "$$\n$$$\na $$ b".into(),
                }),
                paragraph(vec![Inline::Math(MathInline {
                    meta: NodeMeta::default(),
                    value: "a $$ b".into(),
                    kind: MathInlineKind::Code,
                })]),
            ],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert!(markdown.contains("$$$$\n$$\n$$$\na $$ b\n$$$$"));
        assert!(markdown.contains("$`a $$ b`$"));

        let reparsed = parse_document(&markdown, &math_options());
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
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![paragraph(vec![Inline::Math(MathInline {
                meta: NodeMeta::default(),
                value: "a $$ b `$ c".into(),
                kind: MathInlineKind::Code,
            })])],
        };

        assert!(matches!(
            document.to_markdown(),
            Err(SerializeError::UnsupportedNode(message))
                if message.contains("inline math")
        ));
    }

    #[test]
    fn container_directive_fence_exceeds_serialized_code_colons() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![Block::ContainerDirective(ContainerDirective {
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
            })],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert!(markdown.starts_with(":::::note\n"));

        let reparsed = parse_document(&markdown, &directive_options());
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

    #[test]
    fn table_cells_escape_resource_pipes() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![Block::Table(Table {
                meta: NodeMeta::default(),
                alignments: vec![TableAlignment::None, TableAlignment::None],
                rows: vec![
                    TableRow {
                        meta: NodeMeta::default(),
                        cells: vec![
                            TableCell {
                                meta: NodeMeta::default(),
                                children: vec![text("Link")],
                            },
                            TableCell {
                                meta: NodeMeta::default(),
                                children: vec![text("Image")],
                            },
                        ],
                    },
                    TableRow {
                        meta: NodeMeta::default(),
                        cells: vec![
                            TableCell {
                                meta: NodeMeta::default(),
                                children: vec![Inline::Link(Link {
                                    meta: NodeMeta::default(),
                                    destination: "b|c".into(),
                                    destination_kind: LinkDestinationKind::Bare,
                                    title: Some("t|u".into()),
                                    title_kind: Some(LinkTitleKind::DoubleQuote),
                                    children: vec![text("a")],
                                })],
                            },
                            TableCell {
                                meta: NodeMeta::default(),
                                children: vec![Inline::Image(Image {
                                    meta: NodeMeta::default(),
                                    destination: "y|z".into(),
                                    destination_kind: LinkDestinationKind::Bare,
                                    title: Some("i|j".into()),
                                    title_kind: Some(LinkTitleKind::DoubleQuote),
                                    alt: vec![text("x")],
                                })],
                            },
                        ],
                    },
                ],
            })],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert!(markdown.contains(r#"b\|c "t\|u""#));
        assert!(markdown.contains(r#"y\|z "i\|j""#));

        let reparsed = parse_document(&markdown, &SyntaxOptions::gfm());
        match &reparsed.children[..] {
            [Block::Table(table)] => {
                assert_eq!(table.rows[1].cells.len(), 2);
                assert!(matches!(
                    &table.rows[1].cells[0].children[..],
                    [Inline::Link(Link {
                        destination,
                        title: Some(title),
                        ..
                    })] if destination == "b|c" && title == "t|u"
                ));
                assert!(matches!(
                    &table.rows[1].cells[1].children[..],
                    [Inline::Image(Image {
                        destination,
                        title: Some(title),
                        ..
                    })] if destination == "y|z" && title == "i|j"
                ));
            }
            other => panic!("unexpected document shape: {other:?}"),
        }
    }

    #[test]
    fn strong_uses_star_delimiters_when_underline_enabled() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![paragraph(vec![Inline::Strong(Strong {
                meta: NodeMeta::default(),
                children: vec![Inline::Emphasis(Emphasis {
                    meta: NodeMeta::default(),
                    children: vec![text("em")],
                })],
            })])],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "**_em_**\n");

        let reparsed = parse_document(&markdown, &underline_options());
        assert!(matches!(
            &reparsed.children[..],
            [Block::Paragraph(Paragraph {
                children,
                ..
            })] if matches!(
                &children[..],
                [Inline::Strong(Strong {
                    children: strong_children,
                    ..
                })] if matches!(&strong_children[..], [Inline::Emphasis(_)])
            )
        ));
    }

    #[test]
    fn single_tilde_origin_delete_adjacent_to_tilde_runs_roundtrips() {
        let input = "This ~text~~~~ is ~~~~curious~.\n";
        let document = parse_document(input, &SyntaxOptions::gfm());
        assert_single_tilde_delete_with_internal_runs_shape(&document);

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(
            markdown,
            "This ~text\\~\\~\\~\\~ is \\~\\~\\~\\~curious~.\n"
        );

        let reparsed = parse_document(&markdown, &SyntaxOptions::gfm());
        assert_single_tilde_delete_with_internal_runs_shape(&reparsed);
    }

    #[test]
    fn text_double_tilde_run_with_single_tilde_close_stays_text() {
        let input = "a ~~two/one~ b\n";
        let document = parse_document(input, &SyntaxOptions::gfm());
        assert!(matches!(
            &document.children[..],
            [Block::Paragraph(Paragraph {
                children,
                ..
            })] if matches!(&children[..], [Inline::Text(Text { value, .. })] if value == "a ~~two/one~ b")
        ));

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "a \\~\\~two/one~ b\n");

        let reparsed = parse_document(&markdown, &SyntaxOptions::gfm());
        assert!(matches!(
            &reparsed.children[..],
            [Block::Paragraph(Paragraph {
                children,
                ..
            })] if matches!(&children[..], [Inline::Text(Text { value, .. })] if value == "a ~~two/one~ b")
        ));
    }

    #[test]
    fn delete_without_single_tilde_origin_keeps_double_tilde_marker() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![paragraph(vec![Inline::Delete(Delete {
                meta: NodeMeta::default(),
                marker: DeleteMarker::DoubleTilde,
                children: vec![text("text~~~")],
            })])],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert!(markdown.starts_with("~~"));
        assert!(markdown.trim_end().ends_with("~~"));
    }

    #[test]
    fn single_tilde_delete_marker_is_ast_owned_without_source_span() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![paragraph(vec![Inline::Delete(Delete {
                meta: NodeMeta::default(),
                marker: DeleteMarker::SingleTilde,
                children: vec![text("text~~~~ is ~~~~curious")],
            })])],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "~text\\~\\~\\~\\~ is \\~\\~\\~\\~curious~\n");

        let reparsed = parse_document(&markdown, &SyntaxOptions::gfm());
        assert!(matches!(
            &reparsed.children[..],
            [Block::Paragraph(Paragraph {
                children,
                ..
            })] if matches!(
                &children[..],
                [Inline::Delete(Delete {
                    marker: DeleteMarker::SingleTilde,
                    children: delete_children,
                    ..
                })] if matches!(&delete_children[..], [Inline::Text(Text { value, .. })] if value == "text~~~~ is ~~~~curious")
            )
        ));
    }

    #[test]
    fn serialize_options_apply_to_lists_and_code_fences_without_overflow() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![
                Block::List(List {
                    meta: NodeMeta::default(),
                    ordered: false,
                    start: None,
                    delimiter: ListDelimiter::Dash,
                    tight: true,
                    children: vec![ListItem {
                        meta: NodeMeta::default(),
                        checked: None,
                        children: vec![paragraph(vec![text("bullet")])],
                    }],
                }),
                Block::List(List {
                    meta: NodeMeta::default(),
                    ordered: true,
                    start: Some(999_999_998),
                    delimiter: ListDelimiter::Period,
                    tight: true,
                    children: vec![
                        ListItem {
                            meta: NodeMeta::default(),
                            checked: None,
                            children: vec![paragraph(vec![text("one")])],
                        },
                        ListItem {
                            meta: NodeMeta::default(),
                            checked: None,
                            children: vec![paragraph(vec![text("two")])],
                        },
                    ],
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
            ],
        };
        let mut options = SerializeOptions::default();
        options.bullet = ListDelimiter::Plus;
        options.ordered_delimiter = ListDelimiter::Paren;
        options.fence_marker = FenceMarker::Tilde;

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
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![
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
            ],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(
            markdown,
            "[foo]: <my url> 'single title'\n\n[angle](<foo bar> (paren title)) ![empty]( \"empty title\")\n"
        );

        let reparsed = parse_document(&markdown, &SyntaxOptions::commonmark());
        assert!(matches!(
            &reparsed.children[..],
            [
                Block::Definition(Definition {
                    destination,
                    destination_kind: LinkDestinationKind::Angle,
                    title,
                    title_kind: Some(LinkTitleKind::SingleQuote),
                    ..
                }),
                Block::Paragraph(Paragraph {
                    children,
                    ..
                })
            ] if destination == "my url"
                && title.as_deref() == Some("single title")
                && matches!(
                    &children[..],
                    [
                        Inline::Link(Link {
                            destination: link_destination,
                            destination_kind: LinkDestinationKind::Angle,
                            title: link_title,
                            title_kind: Some(LinkTitleKind::Paren),
                            ..
                        }),
                        Inline::Text(_),
                        Inline::Image(Image {
                            destination: image_destination,
                            destination_kind: LinkDestinationKind::Omitted,
                            title: image_title,
                            title_kind: Some(LinkTitleKind::DoubleQuote),
                            ..
                        })
                    ] if link_destination == "foo bar"
                        && link_title.as_deref() == Some("paren title")
                        && image_destination.is_empty()
                        && image_title.as_deref() == Some("empty title")
                )
        ));
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
        let document = parse_document(input, &SyntaxOptions::commonmark());

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
            ],
            "empty inline titles must survive as Some(\"\") with their original kind"
        );

        let definition = document
            .children
            .iter()
            .find_map(|block| match block {
                Block::Definition(definition) => Some(definition),
                _ => None,
            })
            .expect("definition is present");
        assert_eq!(definition.title.as_deref(), Some(""));
        assert_eq!(definition.title_kind, Some(LinkTitleKind::DoubleQuote));

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, input);

        let reparsed = parse_document(&markdown, &SyntaxOptions::commonmark());
        let second = reparsed
            .to_markdown()
            .expect("reparsed document serializes");
        assert_eq!(second, markdown);
    }

    #[test]
    fn ordinary_at_text_does_not_become_escape_when_preserving_escapes() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![paragraph(vec![text("This@that.")])],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "This@that.\n");

        let options = SyntaxOptions {
            constructs: Constructs::commonmark(),
            parse: ParseOptions {
                preserve_character_escapes: true,
                ..ParseOptions::default()
            },
        };
        let reparsed = parse_document(&markdown, &options);
        assert!(matches!(
            &reparsed.children[..],
            [Block::Paragraph(Paragraph {
                children,
                ..
            })] if matches!(&children[..], [Inline::Text(Text { value, .. })] if value == "This@that.")
        ));
    }

    #[test]
    fn definition_labels_escape_brackets_backslashes_and_newlines() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![
                Block::Definition(Definition {
                    meta: NodeMeta::default(),
                    label: "a]b\\c[d".into(),
                    identifier: "a]b\\c[d".into(),
                    destination: "/bracket".into(),
                    destination_kind: LinkDestinationKind::Bare,
                    title: None,
                    title_kind: None,
                }),
                Block::Definition(Definition {
                    meta: NodeMeta::default(),
                    label: "line\nbreak".into(),
                    identifier: "line break".into(),
                    destination: "/newline".into(),
                    destination_kind: LinkDestinationKind::Bare,
                    title: None,
                    title_kind: None,
                }),
            ],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert!(markdown.contains("[a\\]b\\\\c\\[d]: /bracket"));
        assert!(markdown.contains("[line&#xA;break]: /newline"));

        // CommonMark matches reference labels on their RAW text (no backslash
        // unescape, no entity decode), so a label that must escape `]`/`[`/`\` to
        // serialize re-parses to the escaped raw identifier, and the parsed
        // reference would match it because it folds identically.
        let reparsed = parse_document(&markdown, &SyntaxOptions::commonmark());
        match &reparsed.children[..] {
            [Block::Definition(bracket), Block::Definition(newline)] => {
                assert_eq!(bracket.identifier, "a\\]b\\\\c\\[d");
                assert_eq!(bracket.destination, "/bracket");
                assert_eq!(newline.identifier, "line&#xa;break");
                assert_eq!(newline.destination, "/newline");
            }
            other => panic!("unexpected document shape: {other:?}"),
        }
    }

    #[test]
    fn footnote_labels_escape_brackets_backslashes_and_whitespace() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![
                paragraph(vec![
                    text("See "),
                    Inline::FootnoteReference(FootnoteReference {
                        meta: NodeMeta::default(),
                        label: "a]b\\c[d".into(),
                        identifier: "a]b\\c[d".into(),
                    }),
                    text(" and "),
                    Inline::FootnoteReference(FootnoteReference {
                        meta: NodeMeta::default(),
                        label: "white space".into(),
                        identifier: "white space".into(),
                    }),
                ]),
                Block::FootnoteDefinition(FootnoteDefinition {
                    meta: NodeMeta::default(),
                    label: "a]b\\c[d".into(),
                    identifier: "a]b\\c[d".into(),
                    children: vec![paragraph(vec![text("bracket")])],
                }),
                Block::FootnoteDefinition(FootnoteDefinition {
                    meta: NodeMeta::default(),
                    label: "white space".into(),
                    identifier: "white space".into(),
                    children: vec![paragraph(vec![text("space")])],
                }),
            ],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert!(markdown.contains("[^a\\]b\\\\c\\[d]"));
        assert!(markdown.contains("[^white&#x20;space]"));

        let reparsed = parse_document(&markdown, &SyntaxOptions::gfm());
        assert_eq!(reparsed.children.len(), 3);
        let children = match &reparsed.children[0] {
            Block::Paragraph(Paragraph { children, .. }) => children,
            other => panic!("unexpected first block: {other:?}"),
        };
        let bracket = match &reparsed.children[1] {
            Block::FootnoteDefinition(definition) => definition,
            other => panic!("unexpected second block: {other:?}"),
        };
        let space = match &reparsed.children[2] {
            Block::FootnoteDefinition(definition) => definition,
            other => panic!("unexpected third block: {other:?}"),
        };

        assert!(matches!(
            &children[..],
            [
                Inline::Text(Text { value: before, .. }),
                Inline::FootnoteReference(FootnoteReference {
                    identifier: first,
                    ..
                }),
                Inline::Text(Text { value: between, .. }),
                Inline::FootnoteReference(FootnoteReference {
                    identifier: second,
                    ..
                }),
            ] if before == "See "
                && first == "a\\]b\\\\c\\[d"
                && between == " and "
                && second == "white&#x20;space"
        ));
        // Raw-label matching keeps the escaped/entity-encoded spelling: a footnote
        // ref and its definition fold identically (so they still link), but the
        // identifier is the RAW source rather than the unescaped/decoded form.
        assert_eq!(bracket.identifier, "a\\]b\\\\c\\[d");
        assert_eq!(space.identifier, "white&#x20;space");
    }
}

mod serializer_escape {
    use markdown_syntax::*;

    fn text(value: &str) -> Inline {
        Inline::Text(Text {
            meta: NodeMeta::default(),
            value: value.into(),
        })
    }

    fn paragraph(children: Vec<Inline>) -> Block {
        Block::Paragraph(Paragraph {
            meta: NodeMeta::default(),
            children,
        })
    }

    fn parse_document(markdown: &str, options: &SyntaxOptions) -> Document {
        let output = options.parse(markdown);
        assert_eq!(output.diagnostics, Vec::new());
        output.document
    }

    fn preserve_escape_options(constructs: Constructs) -> SyntaxOptions {
        SyntaxOptions {
            constructs: constructs,
            parse: ParseOptions {
                preserve_character_escapes: true,
                ..ParseOptions::default()
            },
        }
    }

    fn table_extension_options() -> SyntaxOptions {
        let mut constructs = Constructs::gfm();
        constructs.directive_text = true;
        constructs.math_inline = true;
        constructs.spoiler = true;
        SyntaxOptions {
            constructs: constructs,
            parse: ParseOptions::default(),
        }
    }

    fn assert_single_text(document: &Document, expected: &str) {
        assert!(matches!(
            &document.children[..],
            [Block::Paragraph(Paragraph {
                children,
                ..
            })] if matches!(&children[..], [Inline::Text(Text { value, .. })] if value == expected)
        ));
    }

    #[test]
    fn ordinary_punctuation_text_does_not_reparse_as_character_escapes() {
        // A backtick is always escaped, like `[`, `]`, and `\`, so it is not
        // ordinary punctuation here.
        let value = "a+b = c, #tag, wow!, a | b, a < b, C++ and x^2 ~ y & z";
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![paragraph(vec![text(value)])],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, format!("{value}\n"));

        let reparsed = parse_document(
            &markdown,
            &preserve_escape_options(Constructs::commonmark()),
        );
        assert_single_text(&reparsed, value);
    }

    #[test]
    fn invalid_character_reference_like_text_stays_text() {
        let value = "Invalid &unknown; &copy and &#x; stay text.";
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![paragraph(vec![text(value)])],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "Invalid \\&unknown; &copy and &#x; stay text.\n");

        let reparsed = parse_document(&markdown, &SyntaxOptions::commonmark());
        assert_single_text(&reparsed, value);
    }

    #[test]
    fn table_cells_do_not_leak_inline_pipes_that_split_cells() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![
                Block::Table(Table {
                    meta: NodeMeta::default(),
                    alignments: vec![
                        TableAlignment::None,
                        TableAlignment::None,
                        TableAlignment::None,
                        TableAlignment::None,
                        TableAlignment::None,
                        TableAlignment::None,
                        TableAlignment::None,
                    ],
                    rows: vec![
                        TableRow {
                            meta: NodeMeta::default(),
                            cells: vec![
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![text("Text")],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![text("Code")],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![text("Math")],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![text("Link")],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![text("Image")],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![text("Reference")],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![text("Directive")],
                                },
                            ],
                        },
                        TableRow {
                            meta: NodeMeta::default(),
                            cells: vec![
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![text("a|b")],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![Inline::Code(CodeInline {
                                        meta: NodeMeta::default(),
                                        value: "c|d".into(),
                                        raw: String::new(),
                                        fence_length: 0,
                                    })],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![Inline::Math(MathInline {
                                        meta: NodeMeta::default(),
                                        value: "x|y".into(),
                                        kind: MathInlineKind::Dollar { dollars: 1 },
                                    })],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![Inline::Link(Link {
                                        meta: NodeMeta::default(),
                                        destination: "/link".into(),
                                        destination_kind: LinkDestinationKind::Bare,
                                        title: None,
                                        title_kind: None,
                                        children: vec![text("link|label")],
                                    })],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![Inline::Image(Image {
                                        meta: NodeMeta::default(),
                                        destination: "/img".into(),
                                        destination_kind: LinkDestinationKind::Bare,
                                        title: None,
                                        title_kind: None,
                                        alt: vec![text("img|alt")],
                                    })],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![Inline::LinkReference(LinkReference {
                                        meta: NodeMeta::default(),
                                        identifier: "pipe|id".into(),
                                        label: "pipe|id".into(),
                                        kind: ReferenceKind::Full,
                                        children: vec![text("ref|text")],
                                    })],
                                },
                                TableCell {
                                    meta: NodeMeta::default(),
                                    children: vec![Inline::TextDirective(TextDirective {
                                        meta: NodeMeta::default(),
                                        name: "note".into(),
                                        label: vec![text("label|text")],
                                        attributes: vec![DirectiveAttribute {
                                            name: "data".into(),
                                            value: Some("value|pipe".into()),
                                        }],
                                    })],
                                },
                            ],
                        },
                    ],
                }),
                Block::Definition(Definition {
                    meta: NodeMeta::default(),
                    label: "pipe|id".into(),
                    identifier: "pipe|id".into(),
                    destination: "/dest".into(),
                    destination_kind: LinkDestinationKind::Bare,
                    title: None,
                    title_kind: None,
                }),
            ],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert!(markdown.contains("a&#x7C;b"));
        assert!(markdown.contains(r"`c\|d`"));
        assert!(markdown.contains(r"$x\|y$"));
        assert!(markdown.contains("[link&#x7C;label](/link)"));
        assert!(markdown.contains("![img&#x7C;alt](/img)"));
        assert!(markdown.contains("[ref&#x7C;text][pipe\\|id]"));
        assert!(markdown.contains(":note[label&#x7C;text]{data=\"value\\|pipe\"}"));

        let reparsed = parse_document(&markdown, &table_extension_options());
        match &reparsed.children[..] {
            [Block::Table(table), Block::Definition(_)] => {
                assert_eq!(table.rows[1].cells.len(), 7);
                assert!(matches!(
                    &table.rows[1].cells[0].children[..],
                    [Inline::Text(Text { value, .. })] if value == "a|b"
                ));
                assert!(matches!(
                    &table.rows[1].cells[1].children[..],
                    [Inline::Code(CodeInline { value, .. })] if value == "c|d"
                ));
                assert!(matches!(
                    &table.rows[1].cells[2].children[..],
                    [Inline::Math(MathInline { value, kind: MathInlineKind::Dollar { dollars: 1 }, .. })]
                        if value == "x|y"
                ));
                assert!(matches!(
                    &table.rows[1].cells[3].children[..],
                    [Inline::Link(Link { children, .. })]
                        if matches!(&children[..], [Inline::Text(Text { value, .. })] if value == "link|label")
                ));
                assert!(matches!(
                    &table.rows[1].cells[4].children[..],
                    [Inline::Image(Image { alt, .. })]
                        if matches!(&alt[..], [Inline::Text(Text { value, .. })] if value == "img|alt")
                ));
                assert!(matches!(
                    &table.rows[1].cells[5].children[..],
                    [Inline::LinkReference(LinkReference {
                        identifier,
                        children,
                        ..
                    })] if identifier == "pipe|id"
                        && matches!(&children[..], [Inline::Text(Text { value, .. })] if value == "ref|text")
                ));
                assert!(matches!(
                    &table.rows[1].cells[6].children[..],
                    [Inline::TextDirective(TextDirective {
                        label,
                        attributes,
                        ..
                    })] if matches!(&label[..], [Inline::Text(Text { value, .. })] if value == "label|text")
                        && matches!(&attributes[..], [DirectiveAttribute { name, value: Some(value) }]
                            if name == "data" && value == "value|pipe")
                ));
            }
            other => panic!("unexpected document shape: {other:?}"),
        }
    }

    #[test]
    fn spoiler_in_table_cell_roundtrips_without_splitting_columns() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![Block::Table(Table {
                meta: NodeMeta::default(),
                alignments: vec![TableAlignment::None],
                rows: vec![
                    TableRow {
                        meta: NodeMeta::default(),
                        cells: vec![TableCell {
                            meta: NodeMeta::default(),
                            children: vec![text("Result")],
                        }],
                    },
                    TableRow {
                        meta: NodeMeta::default(),
                        cells: vec![TableCell {
                            meta: NodeMeta::default(),
                            children: vec![Inline::Spoiler(Spoiler {
                                meta: NodeMeta::default(),
                                children: vec![text("visible")],
                            })],
                        }],
                    },
                ],
            })],
        };

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "| Result |\n| --- |\n| ||visible|| |\n");

        let reparsed = parse_document(&markdown, &table_extension_options());
        assert!(matches!(
            &reparsed.children[..],
            [Block::Table(Table { rows, .. })]
                if rows.len() == 2
                    && rows[1].cells.len() == 1
                    && matches!(
                        &rows[1].cells[0].children[..],
                        [Inline::Spoiler(Spoiler { children, .. })]
                            if matches!(&children[..], [Inline::Text(Text { value, .. })] if value == "visible")
                    )
        ));
    }
}

mod review_serialize {
    //! Regression coverage for serializer round-trip defects.
    //! Each test builds (or parses) an AST, serializes it, asserts the
    //! exact serialized string, and asserts that re-parsing yields the same AST.

    use markdown_syntax::*;

    fn text(value: &str) -> Inline {
        Inline::Text(Text {
            meta: NodeMeta::default(),
            value: value.into(),
        })
    }

    fn soft_break() -> Inline {
        Inline::SoftBreak(SoftBreak {
            meta: NodeMeta::default(),
        })
    }

    fn emphasis(children: Vec<Inline>) -> Inline {
        Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            children,
        })
    }

    fn paragraph(children: Vec<Inline>) -> Block {
        Block::Paragraph(Paragraph {
            meta: NodeMeta::default(),
            children,
        })
    }

    fn document(children: Vec<Block>) -> Document {
        Document {
            meta: NodeMeta::default(),
            children,
        }
    }

    fn parse(markdown: &str, options: &SyntaxOptions) -> Document {
        let output = options.parse(markdown);
        assert_eq!(output.diagnostics, Vec::new());
        output.document
    }

    /// Assert that re-parsing the serialized markdown lands on the same document.
    /// Spans differ between a hand-built AST and a freshly parsed one, so equality
    /// is checked through the serializer (which is span-agnostic and idempotent):
    /// the reparsed document must serialize back to exactly the same markdown.
    fn assert_round_trip_fixpoint(original_markdown: &str, reparsed: &Document) {
        let reserialized = reparsed
            .to_markdown()
            .expect("reparsed document serializes");
        assert_eq!(reserialized, original_markdown);
    }

    fn preserve_references_options() -> SyntaxOptions {
        let constructs = Constructs::commonmark();
        let parse = ParseOptions {
            preserve_character_references: true,
            ..ParseOptions::default()
        };
        SyntaxOptions {
            constructs: constructs,
            parse: parse,
        }
    }

    // --- SR1: entity-encoded shortcut/collapsed references stay implicit --------

    #[test]
    fn sr1_entity_reference_shortcut_is_not_promoted_to_full() {
        let options = preserve_references_options();
        // CommonMark matches reference labels on their RAW text (Unicode case fold +
        // whitespace collapse only — no entity decode, no backslash unescape). So an
        // entity-spelled shortcut matches a definition with the SAME entity spelling;
        // the label oracle agrees and leaves the reference a Shortcut instead of
        // expanding it to `[f&#246;o][f&#246;o]`.
        let document = parse("[f&#246;o]\n\n[f&#246;o]: /url\n", &options);

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "[f&#246;o]\n\n[f&#246;o]: /url\n");

        let reparsed = parse(&markdown, &options);
        assert_round_trip_fixpoint(&markdown, &reparsed);
    }

    // --- L3: explicit reference labels keep their original case/spelling --------

    #[test]
    fn l3_full_reference_preserves_label_case() {
        let document = parse("[text][Ref]\n\n[ref]: /url\n", &SyntaxOptions::commonmark());

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "[text][Ref]\n\n[ref]: /url\n");

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert_round_trip_fixpoint(&markdown, &reparsed);
    }

    #[test]
    fn l3_explicit_label_is_not_double_escaped() {
        let document = parse(
            "Use [text][Foo\\]] and [t][A &amp; B].\n\n[Foo\\]]: /a\n\n[A &amp; B]: /b\n",
            &SyntaxOptions::commonmark(),
        );

        let markdown = document.to_markdown().expect("document serializes");
        // The parsed (source) label is emitted verbatim — no re-escaping of the
        // `\]` or re-encoding of `&amp;`.
        assert!(markdown.contains("[text][Foo\\]]"));
        assert!(markdown.contains("[t][A &amp; B]"));

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert_round_trip_fixpoint(&markdown, &reparsed);
    }

    // --- S1: dash thematic break re-parses as a dash thematic break -------------

    #[test]
    fn s1_dash_thematic_break_round_trips_after_a_block() {
        let document = document(vec![
            paragraph(vec![text("intro")]),
            Block::ThematicBreak(ThematicBreak {
                meta: NodeMeta::default(),
                marker: ThematicBreakMarker::Dash,
            }),
        ]);

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "intro\n\n---\n");

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert!(matches!(
            reparsed.children.as_slice(),
            [
                Block::Paragraph(_),
                Block::ThematicBreak(ThematicBreak {
                    marker: ThematicBreakMarker::Dash,
                    ..
                })
            ]
        ));
        assert_round_trip_fixpoint(&markdown, &reparsed);
    }

    #[test]
    fn s1_leading_dash_thematic_break_uses_spaced_form() {
        let document = document(vec![Block::ThematicBreak(ThematicBreak {
            meta: NodeMeta::default(),
            marker: ThematicBreakMarker::Dash,
        })]);

        let markdown = document.to_markdown().expect("document serializes");
        // A contiguous `---` at the document start would open frontmatter, so the
        // spaced form is used; it still re-parses as a dash thematic break.
        assert_eq!(markdown, "- - -\n");

        let mut constructs = Constructs::commonmark();
        constructs.frontmatter = true;
        let frontmatter = SyntaxOptions {
            constructs: constructs,
            parse: ParseOptions::default(),
        };
        let reparsed = parse(&markdown, &frontmatter);
        assert!(matches!(
            reparsed.children.as_slice(),
            [Block::ThematicBreak(ThematicBreak {
                marker: ThematicBreakMarker::Dash,
                ..
            })]
        ));
    }

    // --- SR11: fenced code honors the stored fence marker -----------------------

    #[test]
    fn sr11_fenced_code_honors_tilde_marker() {
        let document = document(vec![Block::CodeBlock(CodeBlock {
            meta: NodeMeta::default(),
            kind: CodeBlockKind::Fenced {
                marker: FenceMarker::Tilde,
                length: 3,
            },
            info: None,
            value: "code".into(),
        })]);

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "~~~\ncode\n~~~\n");

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert!(matches!(
            reparsed.children.as_slice(),
            [Block::CodeBlock(CodeBlock {
                kind: CodeBlockKind::Fenced {
                    marker: FenceMarker::Tilde,
                    ..
                },
                ..
            })]
        ));
        assert_round_trip_fixpoint(&markdown, &reparsed);
    }

    // --- S4 / SR3: adjacent same-delimiter emphasis stay two nodes --------------

    #[test]
    fn s4_adjacent_emphasis_does_not_merge() {
        let document = document(vec![paragraph(vec![
            emphasis(vec![text("a")]),
            emphasis(vec![text("b")]),
        ])]);

        let markdown = document.to_markdown().expect("document serializes");
        // The second run switches to `_` so the two runs do not fuse into `*a**b*`.
        assert_eq!(markdown, "*a*_b_\n");

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert!(matches!(
            reparsed.children.as_slice(),
            [Block::Paragraph(Paragraph { children, .. })]
                if matches!(children.as_slice(), [Inline::Emphasis(_), Inline::Emphasis(_)])
        ));
        assert_round_trip_fixpoint(&markdown, &reparsed);
    }

    #[test]
    fn s4_emphasis_then_text_starting_with_delimiter_does_not_merge() {
        let document = document(vec![paragraph(vec![emphasis(vec![text("a")]), text("*b")])]);

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "*a*\\*b\n");

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert!(matches!(
            reparsed.children.as_slice(),
            [Block::Paragraph(Paragraph { children, .. })]
                if matches!(
                    children.as_slice(),
                    [Inline::Emphasis(_), Inline::Text(Text { value, .. })] if value == "*b"
                )
        ));
        assert_round_trip_fixpoint(&markdown, &reparsed);
    }

    // --- S3: trailing `#` in ATX heading content is escaped ---------------------

    #[test]
    fn s3_atx_heading_escapes_trailing_hash() {
        let document = document(vec![Block::Heading(Heading {
            meta: NodeMeta::default(),
            depth: 1,
            kind: HeadingKind::Atx,
            children: vec![text("foo #")],
        })]);

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "# foo \\#\n");

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert!(matches!(
            reparsed.children.as_slice(),
            [Block::Heading(Heading { depth: 1, children, .. })]
                if matches!(children.as_slice(), [Inline::Text(Text { value, .. })] if value == "foo #")
        ));
        assert_round_trip_fixpoint(&markdown, &reparsed);
    }

    // --- S2: setext fallback to ATX for unrepresentable depth -------------------

    #[test]
    fn s2_setext_depth_three_falls_back_to_atx() {
        let document = document(vec![Block::Heading(Heading {
            meta: NodeMeta::default(),
            depth: 3,
            kind: HeadingKind::Setext,
            children: vec![text("foo")],
        })]);

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "### foo\n");

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert!(matches!(
            reparsed.children.as_slice(),
            [Block::Heading(Heading {
                depth: 3,
                kind: HeadingKind::Atx,
                ..
            })]
        ));
    }

    #[test]
    fn s2_multi_line_setext_stays_setext() {
        let document = document(vec![Block::Heading(Heading {
            meta: NodeMeta::default(),
            depth: 1,
            kind: HeadingKind::Setext,
            children: vec![text("foo"), soft_break(), text("bar")],
        })]);

        let markdown = document.to_markdown().expect("document serializes");
        assert_eq!(markdown, "foo\nbar\n=======\n");

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert!(matches!(
            reparsed.children.as_slice(),
            [Block::Heading(Heading { depth: 1, kind: HeadingKind::Setext, children, .. })]
                if children.iter().filter(|i| matches!(i, Inline::SoftBreak(_))).count() == 1
        ));
        assert_round_trip_fixpoint(&markdown, &reparsed);
    }

    // --- S5: asterisk-bullet thematic break stays inside its list ---------------

    #[test]
    fn s5_asterisk_bullet_thematic_break_item_is_disambiguated() {
        let list = Block::List(List {
            meta: NodeMeta::default(),
            ordered: false,
            start: None,
            delimiter: ListDelimiter::Asterisk,
            tight: true,
            children: vec![ListItem {
                meta: NodeMeta::default(),
                checked: None,
                children: vec![Block::ThematicBreak(ThematicBreak {
                    meta: NodeMeta::default(),
                    marker: ThematicBreakMarker::Asterisk,
                })],
            }],
        });
        let document = document(vec![list]);

        let markdown = document.to_markdown().expect("document serializes");
        // `* ***` would escape the list as a top-level thematic break, so the item
        // starts on the line after its marker, keeping the break's marker.
        assert_eq!(markdown, "*\n  ***\n");

        let reparsed = parse(&markdown, &SyntaxOptions::commonmark());
        assert!(matches!(
            reparsed.children.as_slice(),
            [Block::List(List { children, .. })]
                if matches!(
                    children.as_slice(),
                    [ListItem { children, .. }]
                        if matches!(
                            children.as_slice(),
                            [Block::ThematicBreak(ThematicBreak {
                                marker: ThematicBreakMarker::Asterisk,
                                ..
                            })]
                        )
                )
        ));
    }

    #[test]
    fn s5_asterisk_bullet_nested_list_is_left_intact() {
        let inner = Block::List(List {
            meta: NodeMeta::default(),
            ordered: false,
            start: None,
            delimiter: ListDelimiter::Asterisk,
            tight: true,
            children: vec![ListItem {
                meta: NodeMeta::default(),
                checked: None,
                children: Vec::new(),
            }],
        });
        let outer = Block::List(List {
            meta: NodeMeta::default(),
            ordered: false,
            start: None,
            delimiter: ListDelimiter::Asterisk,
            tight: true,
            children: vec![ListItem {
                meta: NodeMeta::default(),
                checked: None,
                children: vec![inner],
            }],
        });
        let document = document(vec![outer]);

        let markdown = document.to_markdown().expect("document serializes");
        // A genuine nested bullet (`* *`) has interior whitespace and must NOT be
        // rewritten into a thematic break.
        assert!(!markdown.contains("---"));
        assert!(markdown.contains('*'));
    }
}

mod literal_text {
    //! Text that the serializer must keep literal when it sits beside
    //! delimiters and references written by other nodes.

    use markdown_syntax::prelude::*;

    fn paragraph_document(children: Vec<Inline>) -> Document {
        Document {
            meta: NodeMeta::default(),
            children: vec![Paragraph::new(children).into()],
        }
    }

    /// Serializes `document`, reparses the output with `options`, and checks
    /// that the reparsed blocks equal the original ones apart from spans.
    fn assert_round_trips(document: &Document, options: &SyntaxOptions) -> String {
        let markdown = document.to_markdown().expect("document serializes");
        let reparsed = options.parse(&markdown).document;
        assert_eq!(
            without_spans(&format!("{:?}", reparsed.children)),
            without_spans(&format!("{:?}", document.children)),
            "{markdown:?}"
        );
        markdown
    }

    /// `debug` with every `Some(Span { .. })` written as `None`.
    fn without_spans(debug: &str) -> String {
        let mut out = String::new();
        let mut rest = debug;
        while let Some(start) = rest.find("Some(Span { ") {
            out.push_str(&rest[..start]);
            out.push_str("None");
            let end = rest[start..].find("})").expect("span ends") + start + 2;
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn an_underscore_that_can_close_stays_inside_underscore_emphasis() {
        let strong = Inline::Strong(Strong {
            meta: NodeMeta::default(),
            children: vec![Text::from("(a b)_.").into()],
        });
        let emphasis = Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            children: vec![strong],
        });
        let document = paragraph_document(vec![emphasis, Text::from("*#").into()]);
        let markdown = assert_round_trips(&document, &SyntaxOptions::commonmark());
        assert_eq!(markdown, "_**(a b)\\_.**_\\*#\n");
    }

    /// Parses `source` with `options`, then checks that the serialized output
    /// reparses to the same blocks.
    fn assert_parsed_round_trips(source: &str, options: &SyntaxOptions) -> String {
        assert_round_trips(&options.parse(source).document, options)
    }

    #[test]
    fn a_parenthesis_after_a_shortcut_reference_stays_text() {
        let markdown =
            assert_parsed_round_trips("[foo]\\(a)\n\n[foo]: /u", &SyntaxOptions::commonmark());
        assert!(markdown.starts_with("[foo]\\(a)\n"), "{markdown:?}");
    }

    #[test]
    fn a_parenthesis_after_a_shortcut_image_reference_stays_text() {
        let markdown =
            assert_parsed_round_trips("![foo]\\(a)\n\n[foo]: /u", &SyntaxOptions::commonmark());
        assert!(markdown.starts_with("![foo]\\(a)\n"), "{markdown:?}");
    }

    #[test]
    fn a_colon_after_a_shortcut_reference_that_starts_a_paragraph_stays_text() {
        let markdown =
            assert_parsed_round_trips("[foo]\\: /x\n\n[foo]: /u", &SyntaxOptions::commonmark());
        assert!(markdown.starts_with("[foo]\\: /x\n"), "{markdown:?}");
    }

    #[test]
    fn every_backtick_in_text_is_escaped() {
        let document = paragraph_document(vec![Text::from("b ``a`").into()]);
        let markdown = assert_round_trips(&document, &SyntaxOptions::default());
        assert_eq!(markdown, "b \\`\\`a\\`\n");
    }

    #[test]
    fn paired_backticks_in_text_are_both_escaped() {
        let markdown = SyntaxOptions::default()
            .parse("Test \\`hello world` here.")
            .document
            .to_markdown()
            .expect("document serializes");
        assert_eq!(markdown, "Test \\`hello world\\` here.\n");
    }

    #[test]
    fn a_pipe_ending_a_level_two_setext_heading_does_not_start_a_table() {
        // A one-dash underline is no table delimiter row, so this is a heading.
        let markdown = assert_parsed_round_trips("a |\n-", &SyntaxOptions::default());
        assert_eq!(markdown, "a \\|\n---\n");
    }

    #[test]
    fn an_empty_fenced_code_block_writes_no_content_line() {
        let markdown = assert_parsed_round_trips("```\n```", &SyntaxOptions::default());
        assert_eq!(markdown, "```\n```\n");
    }

    #[test]
    fn whitespace_at_the_ends_of_an_info_string_is_written_as_references() {
        let markdown =
            assert_parsed_round_trips("```&#x20;a&#9;\nb\n```", &SyntaxOptions::default());
        assert_eq!(markdown, "``` &#x20;a&#x9;\nb\n```\n");
    }

    #[test]
    fn text_after_a_literal_autolink_does_not_extend_it() {
        for source in ["://&amp;", "www.}", "a.b@c.d&#x5f;"] {
            assert_parsed_round_trips(source, &SyntaxOptions::default());
        }
    }

    #[test]
    fn a_paragraph_that_opens_with_a_soft_break_keeps_it() {
        let markdown = assert_parsed_round_trips("&#x20;\na", &SyntaxOptions::default());
        assert_eq!(markdown, "&#x20;\na\n");
    }

    #[test]
    fn a_continuation_line_that_would_open_a_block_stays_text() {
        for source in ["a\n\\<div>", "a\n\\<!-- b", "a\n\\::b"] {
            assert_parsed_round_trips(source, &SyntaxOptions::default());
        }
    }

    #[test]
    fn an_html_block_value_is_written_verbatim() {
        assert_parsed_round_trips("<!--\n\n", &SyntaxOptions::default());
        assert_parsed_round_trips("<div>\n  a  \n</div>", &SyntaxOptions::default());
    }

    #[test]
    fn indented_code_keeps_a_carriage_return_ending_its_last_line() {
        for source in ["\ta\r\tb", "    a\r\n    b\r\n\r\nc", "    a\r    b\r\rc"] {
            assert_parsed_round_trips(source, &SyntaxOptions::default());
        }
        let mut crlf = SerializeOptions::default();
        crlf.line_ending = LineEnding::CrLf;
        let document = SyntaxOptions::default()
            .parse("```\r\na\r\n```\r\nb")
            .document;
        let markdown = document
            .to_markdown_with(&crlf)
            .expect("document serializes");
        assert_eq!(markdown, "```\r\na\r\n```\r\n\r\nb\r\n");
    }
}

mod round_trip_edges {
    //! Parsed documents whose serialized output must reparse to the same
    //! tree under the dialect that parsed them.

    use markdown_syntax::prelude::*;

    /// `debug` with every `Some(Span { .. })` written as `None`.
    fn without_spans(debug: &str) -> String {
        let mut out = String::new();
        let mut rest = debug;
        while let Some(start) = rest.find("Some(Span { ") {
            out.push_str(&rest[..start]);
            out.push_str("None");
            let end = rest[start..].find("})").expect("span ends") + start + 2;
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    }

    /// Checks the round trip of `source` under the CommonMark preset and the
    /// default dialect, and returns the default dialect's output.
    fn assert_round_trips(source: &str) -> String {
        let mut markdown = String::new();
        for options in [SyntaxOptions::commonmark(), SyntaxOptions::default()] {
            let document = options.parse(source).document;
            markdown = document.to_markdown().expect("document serializes");
            let reparsed = options.parse(&markdown).document;
            assert_eq!(
                without_spans(&format!("{:?}", reparsed.children)),
                without_spans(&format!("{:?}", document.children)),
                "{source:?} -> {markdown:?}"
            );
        }
        markdown
    }

    #[test]
    fn a_break_that_opens_a_line_or_a_span_is_written_after_a_reference() {
        assert_eq!(assert_round_trips("&#x20; \na"), "&#x20;\na\n");
        assert_eq!(assert_round_trips("a\n&#x20;\nb"), "a\n&#x20;\nb\n");
        assert_round_trips("++&#x20;\nd++");
        assert_round_trips("_&#x20;\n=_");
        assert_eq!(assert_round_trips("[\nfoo](u)"), "[\nfoo](u)\n");
    }

    #[test]
    fn a_continuation_line_inside_an_inline_that_would_start_a_block_is_indented() {
        assert_eq!(assert_round_trips("=```\n    ```"), "\\=```\n    ```\n");
        assert_round_trips("-$$\n    $$");
        assert_eq!(assert_round_trips("(\n    <div>"), "(\n    <div>\n");
        assert_eq!(assert_round_trips("``\nfoo\nbar\n``"), "``\nfoo\nbar\n``\n");
    }

    #[test]
    fn delimiter_runs_in_text_are_escaped_whole() {
        for source in [
            "**\t*$",
            "+*(**\0",
            "__***-*",
            "(*~\n**)",
            "**:\n**:",
            "($$]$=",
            "[\\||>||)||",
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn emphasis_delimiters_suit_their_neighbours() {
        assert_eq!(assert_round_trips("***y*b"), "\\*\\**y*b\n");
        assert_eq!(assert_round_trips("y***b***"), "y***b***\n");
    }

    #[test]
    fn a_literal_autolink_does_not_run_on_into_what_follows() {
        for source in ["&#x20;://y", "://\\:://y", "://\\::p:", "://\t["] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn a_bare_destination_writes_a_space_as_a_reference() {
        assert_eq!(assert_round_trips("[o]:&#x20;"), "[o]: &#x20;\n");
    }

    #[test]
    fn whitespace_that_opens_a_list_items_first_block_starts_on_the_next_line() {
        assert_eq!(assert_round_trips("-\n   <v>"), "-\n   <v>\n");
        assert_round_trips("*\t<a>");
    }

    #[test]
    fn raw_html_alone_on_a_paragraphs_first_line_keeps_a_trailing_reference() {
        assert_eq!(assert_round_trips("<a>&#x20;\n;"), "<a>&#x20;\n;\n");
    }

    #[test]
    fn a_fence_grows_only_past_lines_that_would_close_it() {
        assert_eq!(assert_round_trips("```\n```*"), "```\n```*\n```\n");
        assert_eq!(assert_round_trips("````\n```\n````"), "````\n```\n````\n");
    }

    #[test]
    fn text_that_would_open_an_extension_construct_stays_text() {
        for source in [
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
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn a_literal_tilde_beside_an_emphasis_run_stays_literal() {
        assert_eq!(assert_round_trips("a**~**"), "a**~**\n");
        assert_eq!(assert_round_trips("b*~*"), "b*~*\n");
        // The default preset's emphasis here is not CommonMark's or GFM's
        // reading, so only the round trip is pinned.
        assert_round_trips("d_~_");
        assert_round_trips("b*~~~***");
        assert_round_trips("~~a~~~");
        assert_round_trips("~~~a");
    }

    #[test]
    fn a_pipe_that_raw_html_an_autolink_or_math_writes_in_a_cell_is_escaped() {
        for source in ["://\\||[\n-|-", "|<!--\\|-->\n--", "$\\|$||\n-|-"] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn container_and_list_edges_round_trip() {
        for source in [
            "-\t(\n  <v>",
            "*\t<a>\n  <v>",
            "- >**\n`",
            "1. >)\n~",
            "-\n  ---",
            "><!--\n>```",
            "`\n|`\n-",
            " ~~~\n    ~~~",
            "---\n \n\n---",
            "*c*__&__",
        ] {
            assert_round_trips(source);
        }
        assert_eq!(assert_round_trips(" ~~~\n    ~~~"), " ~~~\n    ~~~\n ~~~\n");
    }

    #[test]
    fn raw_html_after_a_definition_continues_its_paragraph() {
        for source in [
            "[o]:u\n\t<div>",
            "[o]:u\n<a>\n-",
            "<a>&#x20;\n[\n-",
            "[o]: u\n<a>",
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn a_line_that_would_open_description_details_stays_text() {
        for source in ["a\n   : `", "~\n: ]", "``\n   ~\t``", "[\n~ _\n    ~~~"] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn nested_attention_runs_pick_delimiters_that_reparse_to_them() {
        for source in [
            "__**)**&__",
            "**:__$__**",
            "****(*+***",
            "***_|_***",
            "__***/***__",
            "**#****]***_**",
            "***_\\**#*",
            "**__\u{0}___**",
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn text_around_a_literal_autolink_does_not_move_its_end() {
        for source in [
            "a\\-://`",
            "ab&#99;://x",
            "*://*&mp;",
            "**://**&mp;",
            "://^&mp;",
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn text_after_a_reference_or_before_a_wikilink_keeps_its_parse() {
        assert_eq!(assert_round_trips("[^`]``"), "[^`]&#96;&#96;\n");
        assert_eq!(assert_round_trips("![[$[]]a$>"), "\\![[$\\[]]a$>\n");
        for source in ["[a`]``\n\n[a`]: x", "[^`]: x\n\n[^`]``", "://`\\`"] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn a_tilde_beside_an_attention_run_keeps_the_runs_bonus() {
        for source in [
            "b**~\n~**",
            "b_~~_~",
            "~\nb*~***",
            "a__~>__~",
            "~~目*~***",
            "a*~ **&*",
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn text_before_a_construct_escapes_the_delimiters_it_writes() {
        for source in [
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
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn whitespace_other_than_spaces_and_tabs_round_trips() {
        for source in [
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
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn spaces_and_text_beside_a_literal_autolink_keep_its_end() {
        for source in [
            "^://y ^",
            "://~ #~",
            "_&#x20;://_",
            "||://y\t||",
            "==&#x20;://<==",
            "^http://x ^",
            "*&#x20;http://x*",
            "://\\~||>||",
            "://\\)||#||",
            "[foo]:`\n[foo]^://y\t^",
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn constructs_after_a_directive_email_or_alert_keep_their_parse() {
        for source in [
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
            "^://. ^",
            "]\n: :::e",
            "[^1]:| &#x20;\n:",
            ":::t\n```\n:::e",
        ] {
            assert_round_trips(source);
        }
    }

    /// `assert_round_trips` under `options` alone.
    fn assert_round_trips_under(options: &SyntaxOptions, source: &str) {
        let document = options.parse(source).document;
        let markdown = document.to_markdown().expect("document serializes");
        let reparsed = options.parse(&markdown).document;
        assert_eq!(
            without_spans(&format!("{:?}", reparsed.children)),
            without_spans(&format!("{:?}", document.children)),
            "{source:?} -> {markdown:?}"
        );
    }

    #[test]
    fn a_paragraph_opening_with_an_esm_keyword_stays_a_paragraph_under_mdx() {
        for source in [" import -", " export x", " import *\n-"] {
            assert_round_trips_under(&SyntaxOptions::mdx(), source);
        }
    }

    #[test]
    fn mdx_and_gfm_content_reads_back_under_its_preset() {
        for source in [
            "<!--@b>",
            "\\{[]()}",
            "\u{a0}&#x20;<p/>",
            "{}&#x20;\n\\",
            "{}&#x20; \n\\",
        ] {
            assert_round_trips_under(&SyntaxOptions::mdx(), source);
        }
        for source in ["**=* ++@b.c*", "~&#x20;://~"] {
            assert_round_trips_under(&SyntaxOptions::gfm(), source);
        }
    }

    #[test]
    fn delimiters_around_autolinks_tasks_and_alerts_keep_their_parse() {
        for source in [
            "_`*www._",
            "# _*www._",
            "**://\\***[^1]",
            "^[www.[ ]",
            "1. >[!NOTE]\n~[^1]",
            "+ [x]  :e",
            "- [ ] &#x20;",
            "***:*:**",
            "++*++_@b.c",
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn run_delimiter_choices_reparse_beside_and_inside_other_inlines() {
        for source in [
            "y*x***a_ b**",
            "*a***b**",
            "**__a__~~**b",
            "[__**)**&__](u)",
            "![__**)**&__](u)",
            "==__***/***__==",
            "*******b_*_c~_y",
            "__**a*********___",
            "y__**__**y****___",
        ] {
            assert_round_trips(source);
        }
        // `__` reads back as underline once that construct is enabled.
        assert_round_trips_under(
            &SyntaxOptions::default().enable(Construct::Underline),
            "*a***b**",
        );
    }

    #[test]
    fn a_doubled_delimiter_that_could_close_its_span_is_escaped() {
        assert_eq!(assert_round_trips("==a\\== b=="), "==a\\== b==\n");
        assert_eq!(assert_round_trips("++a\\++ b++"), "++a\\+\\+ b++\n");
    }

    #[test]
    fn math_opening_a_definitions_paragraph_stays_inline() {
        assert_eq!(
            assert_round_trips("[o]:u\n\t$$\na$$"),
            "[o]: u\n    $$\na$$\n"
        );
    }

    #[test]
    fn a_cell_pipe_after_an_escaped_backslash_is_escaped() {
        for source in [
            "| <a b=\"x\\\\\\|y\"> |\n| --- |",
            "| x |\n| --- |\n| $a\\\\\\|b$ |",
            "| <http://x\\\\\\|y> |\n|-|",
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn a_raw_label_backtick_reaches_text_inside_and_after_spans() {
        for source in [
            "[foo`bar] *&#96;*\n\n[foo`bar]: /u",
            "*[foo`bar]* &#96;\n\n[foo`bar]: /u",
            "[^a`b] *&#96;*\n\n[^a`b]: x",
        ] {
            assert_round_trips(source);
        }
    }

    #[test]
    fn spans_cells_and_items_keep_what_borders_them() {
        for source in [
            // An escaped backtick in a label opens no code span.
            "-[^\\`]://\\`",
            // The insert's `++` and the `:`s around it would name a shortcode.
            "++:++\\:",
            // A space opening a cell would be trimmed.
            "&#x20;://>|>\n-|-",
            // The nested item's content column keeps the HTML block out.
            "- *  (\n    <a>",
            // A text directive opens only after raw whitespace.
            "~~:~ :e~",
            "~\t:e~",
            // A `+` or `=` beside the span's delimiter would lengthen it.
            "++\\+>++",
            "==\\=a==",
            // A definition labelled like an alert marker keeps the quote.
            ">\n>[!NOTE]:>",
        ] {
            assert_round_trips(source);
        }
    }
}
