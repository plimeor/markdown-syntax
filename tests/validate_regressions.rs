//! `validate_document` regression coverage: the standalone zero-width-table
//! check plus the validator-hardening rejects from the review pass.
//!
//! Each former regression file is preserved verbatim inside its own `mod` so
//! that helper functions and test names cannot collide across the merged
//! sources.

#[path = "support/normalize.rs"]
mod normalize;

mod validation {
    use markdown_syntax::*;

    #[test]
    fn a_shortcode_name_outside_gemoji_is_invalid() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![Block::Paragraph(Paragraph {
                meta: NodeMeta::default(),
                children: vec![Inline::Shortcode(Shortcode {
                    meta: NodeMeta::default(),
                    name: "not_an_emoji_name".into(),
                })],
            })],
        };
        assert!(!document.validate().is_empty());
        assert!(matches!(
            document.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));
    }

    #[test]
    fn empty_paragraph_is_invalid() {
        for children in [vec![], vec![Inline::Text(Text::from(""))]] {
            let document = Document {
                meta: NodeMeta::default(),
                children: vec![Block::Paragraph(Paragraph {
                    meta: NodeMeta::default(),
                    children,
                })],
            };
            let diagnostics = document.validate();
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert_eq!(diagnostics[0].message, "paragraph cannot be empty");
            assert!(matches!(
                document.to_markdown(),
                Err(SerializeError::InvalidDocument(_))
            ));
        }
    }

    #[test]
    fn empty_table_is_invalid() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![Block::Table(Table {
                meta: NodeMeta::default(),
                alignments: vec![],
                rows: vec![],
            })],
        };

        let diagnostics = document.validate();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "table must contain at least a header row"
        );

        assert!(matches!(
            document.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));
    }

    #[test]
    fn zero_width_table_is_invalid() {
        let document = Document {
            meta: NodeMeta::default(),
            children: vec![Block::Table(Table {
                meta: NodeMeta::default(),
                alignments: vec![],
                rows: vec![TableRow {
                    meta: NodeMeta::default(),
                    cells: vec![],
                }],
            })],
        };

        let diagnostics = document.validate();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "table header row must contain at least one cell"
        );

        assert!(matches!(
            document.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));
    }

    #[test]
    fn validation_and_serializer_reject_invalid_ast() {
        let mut document = Document::default();
        document.children.push(Block::Heading(Heading {
            meta: NodeMeta::default(),
            depth: 9,
            kind: HeadingKind::Atx,
            children: vec![Inline::Text(Text {
                meta: NodeMeta::default(),
                value: "bad".into(),
            })],
        }));

        let diagnostics = document.validate();
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            document.to_markdown().unwrap_err(),
            SerializeError::InvalidDocument(_)
        ));
    }
}

mod review_validate {
    //! Regressions for `validate_document` hardening.
    //!
    //! Each item rejects a hand-buildable AST shape that serializes to Markdown the
    //! parser cannot reconstruct. Every case pairs a rejected (bad) shape with a
    //! nearby valid (good) shape that must still pass, so the rejects stay narrow.

    use markdown_syntax::*;

    fn paragraph(children: Vec<Inline>) -> Document {
        Document {
            meta: NodeMeta::default(),
            children: vec![Block::Paragraph(Paragraph {
                meta: NodeMeta::default(),
                children,
            })],
        }
    }

    fn text(value: &str) -> Inline {
        Inline::Text(Text {
            meta: NodeMeta::default(),
            value: value.into(),
        })
    }

    #[test]
    fn zero_dollar_inline_math_is_invalid() {
        let bad = paragraph(vec![Inline::Math(MathInline {
            meta: NodeMeta::default(),
            value: "x".into(),
            kind: MathInlineKind::Dollar { dollars: 0 },
        })]);
        assert!(!bad.validate().is_empty());
        assert!(matches!(
            bad.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));

        let good = paragraph(vec![Inline::Math(MathInline {
            meta: NodeMeta::default(),
            value: "x".into(),
            kind: MathInlineKind::Dollar { dollars: 1 },
        })]);
        assert!(good.validate().is_empty());
    }

    fn math(value: &str, kind: MathInlineKind) -> Document {
        paragraph(vec![Inline::Math(MathInline {
            meta: NodeMeta::default(),
            value: value.into(),
            kind,
        })])
    }

    #[test]
    fn empty_inline_math_is_invalid() {
        // `$$` reads back as text, and `$``$` as dollar math holding "``".
        for kind in [
            MathInlineKind::Dollar { dollars: 1 },
            MathInlineKind::Dollar { dollars: 2 },
            MathInlineKind::Code,
        ] {
            let bad = math("", kind);
            assert_eq!(bad.validate().len(), 1, "{kind:?}");
            assert!(matches!(
                bad.to_markdown(),
                Err(SerializeError::InvalidDocument(_))
            ));
            assert!(math(" ", kind).validate().is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn code_math_holding_its_close_is_invalid() {
        let bad = math("a`$b", MathInlineKind::Code);
        let diagnostics = bad.validate();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::InvalidDocument);
        assert!(matches!(
            bad.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));

        // A `$` or a backtick alone, or the dollar form, may hold them.
        for good in [
            math("a$b`c", MathInlineKind::Code),
            math("$`a", MathInlineKind::Code),
            math("a`$b", MathInlineKind::Dollar { dollars: 2 }),
        ] {
            assert!(good.validate().is_empty(), "{good:?}");
        }
    }

    #[test]
    fn frontmatter_after_a_paragraph_is_left_to_the_builder() {
        let mut document = paragraph(vec![text("a")]);
        document.children.push(Block::Frontmatter(Frontmatter {
            meta: NodeMeta::default(),
            kind: FrontmatterKind::Yaml,
            value: "b: c\n".into(),
        }));
        assert!(document.validate().is_empty());
    }

    #[test]
    fn sr6_rejects_each_emphasis_like_container_when_empty() {
        let empty_containers = [
            Inline::Emphasis(Emphasis {
                meta: NodeMeta::default(),
                delimiter: EmphasisDelimiter::Asterisk,
                children: vec![],
            }),
            Inline::Strong(Strong {
                meta: NodeMeta::default(),
                delimiter: EmphasisDelimiter::Asterisk,
                children: vec![],
            }),
            Inline::Delete(Delete {
                meta: NodeMeta::default(),
                children: vec![],
            }),
            Inline::Mark(Mark {
                meta: NodeMeta::default(),
                children: vec![],
            }),
        ];

        for container in empty_containers {
            let document = paragraph(vec![container]);
            assert!(
                !document.validate().is_empty(),
                "empty container should be rejected"
            );
        }

        let bad = paragraph(vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            delimiter: EmphasisDelimiter::Asterisk,
            children: vec![],
        })]);
        assert!(matches!(
            bad.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));

        let good = paragraph(vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            delimiter: EmphasisDelimiter::Asterisk,
            children: vec![text("a")],
        })]);
        assert!(good.validate().is_empty());
    }

    // SR7 — an escape of a non-ASCII-punctuation char serializes `\x` which the
    // parser keeps literal.
    #[test]
    fn sr7_non_punctuation_escape_is_invalid() {
        let bad = paragraph(vec![Inline::Escape(Escape {
            meta: NodeMeta::default(),
            value: 'a',
        })]);
        assert!(!bad.validate().is_empty());

        // Escaping an ASCII punctuation char is valid.
        let good = paragraph(vec![Inline::Escape(Escape {
            meta: NodeMeta::default(),
            value: '*',
        })]);
        assert!(good.validate().is_empty());
    }

    // SR8 — an inline link whose text is an address is valid and is written
    // as an inline link that reads back as the same link.
    #[test]
    fn sr8_inline_link_whose_text_is_an_address_is_valid() {
        let link = |destination: &str, text: &str| {
            paragraph(vec![Inline::Link(Link {
                meta: NodeMeta::default(),
                destination: destination.into(),
                destination_kind: LinkDestinationKind::Bare,
                title: None,
                children: vec![Text::from(text).into()],
            })])
        };
        let document = link("mailto:a\u{a0}b@c.d", "a\u{a0}b@c.d");
        assert!(document.validate().is_empty());
        let markdown = document.to_markdown().expect("document serializes");
        assert!(
            markdown.starts_with('[') && markdown.ends_with("](mailto:a\u{a0}b@c.d)\n"),
            "{markdown:?}"
        );
        let reparsed = markdown_syntax::parse(&markdown).document;
        assert_eq!(
            format!("{:?}", crate::normalize::normalized(&reparsed.children)),
            format!("{:?}", crate::normalize::normalized(&document.children)),
        );

        // An inline link is written inline even when its text is its URL.
        let inline = link(
            "https://example.com/path?q=1",
            "https://example.com/path?q=1",
        );
        assert!(inline.validate().is_empty());
        assert_eq!(
            inline.to_markdown().expect("document serializes"),
            "[https://example.com/path?q=1](https://example.com/path?q=1)\n"
        );
    }

    // SR9 — a code span holds only a value the parser can read from one; the
    // serializer chooses its fence and padding, so any backticks in the value
    // read back.
    #[test]
    fn sr9_inline_code_value_is_what_a_code_span_reads() {
        for value in ["", "a\nb", "a\rb"] {
            let bad = paragraph(vec![Inline::Code(CodeInline::new(value))]);
            assert!(!bad.validate().is_empty(), "{value:?}");
        }

        for (value, written) in [
            ("a`b", "``a`b``"),
            ("a``b", "`a``b`"),
            ("``", "` `` `"),
            ("`a", "`` `a ``"),
            (" a ", "`  a  `"),
            (" a", "` a`"),
            ("  ", "`  `"),
        ] {
            let document = paragraph(vec![Inline::Code(CodeInline::new(value))]);
            assert!(document.validate().is_empty(), "{value:?}");
            let markdown = document.to_markdown().expect("document serializes");
            assert_eq!(markdown, format!("{written}\n"), "{value:?}");
            let reparsed = parse(&markdown).document;
            assert_eq!(
                format!("{:?}", crate::normalize::normalized(&reparsed.children)),
                format!("{:?}", crate::normalize::normalized(&document.children)),
                "{value:?}"
            );
        }
    }

    // SR4 — an ordered list start beyond the parser's 9-digit marker cap round-trips
    // to a paragraph.
    #[test]
    fn sr4_ordered_list_start_overflow_is_invalid() {
        let bad = Document {
            meta: NodeMeta::default(),
            children: vec![Block::List(List {
                meta: NodeMeta::default(),
                ordered: true,
                start: Some(1_000_000_000),
                delimiter: ListDelimiter::Period,
                tight: true,
                children: vec![ListItem {
                    meta: NodeMeta::default(),
                    checked: None,
                    children: vec![Block::Paragraph(Paragraph {
                        meta: NodeMeta::default(),
                        children: vec![text("foo")],
                    })],
                }],
            })],
        };
        assert!(!bad.validate().is_empty());
        assert!(matches!(
            bad.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));

        // The largest 9-digit start is still representable.
        let good = Document {
            meta: NodeMeta::default(),
            children: vec![Block::List(List {
                meta: NodeMeta::default(),
                ordered: true,
                start: Some(999_999_999),
                delimiter: ListDelimiter::Period,
                tight: true,
                children: vec![ListItem {
                    meta: NodeMeta::default(),
                    checked: None,
                    children: vec![Block::Paragraph(Paragraph {
                        meta: NodeMeta::default(),
                        children: vec![text("foo")],
                    })],
                }],
            })],
        };
        assert!(good.validate().is_empty());
    }

    // SR5 — a hard line break as the final inline of a container serializes to a
    // dangling `\` / trailing spaces the parser cannot reconstruct as a break.
    #[test]
    fn sr5_trailing_hard_line_break_is_invalid() {
        let bad = paragraph(vec![
            text("foo"),
            Inline::LineBreak(LineBreak {
                meta: NodeMeta::default(),
                kind: LineBreakKind::Backslash,
            }),
        ]);
        assert!(!bad.validate().is_empty());

        // A mid-paragraph hard break (followed by content) is fine.
        let good = paragraph(vec![
            text("foo"),
            Inline::LineBreak(LineBreak {
                meta: NodeMeta::default(),
                kind: LineBreakKind::Backslash,
            }),
            text("bar"),
        ]);
        assert!(good.validate().is_empty());
    }

    // L5 (AST side) — a definition with an empty/blank identifier, parity with the
    // existing footnote-definition empty-identifier check.
    #[test]
    fn l5_definition_empty_identifier_is_invalid() {
        let bad = Document {
            meta: NodeMeta::default(),
            children: vec![Block::Definition(Definition {
                meta: NodeMeta::default(),
                label: String::new(),
                identifier: "   ".into(),
                destination: "/uri".into(),
                destination_kind: LinkDestinationKind::Bare,
                title: None,
            })],
        };
        assert!(!bad.validate().is_empty());

        let good = Document {
            meta: NodeMeta::default(),
            children: vec![Block::Definition(Definition {
                meta: NodeMeta::default(),
                label: "ref".into(),
                identifier: "ref".into(),
                destination: "/uri".into(),
                destination_kind: LinkDestinationKind::Bare,
                title: None,
            })],
        };
        assert!(good.validate().is_empty());
    }

    // A `]` closes after a line break, so link text, alt text, an inline
    // footnote, and a text directive label may end with a hard line break.
    #[test]
    fn hard_line_break_may_end_bracketed_content() {
        for source in [
            "[a\\\n](u)\n",
            "[a  \n](u)\n",
            "![a\\\n](u)\n",
            "x^[a\\\n] y\n",
            ":d[a\\\n] y\n",
        ] {
            let document = parse(source).document;
            assert!(document.validate().is_empty(), "{source:?}");
            let markdown = document.to_markdown().expect("document serializes");
            assert_eq!(markdown, source);
            assert_eq!(parse(&markdown).document.to_markdown().unwrap(), markdown);
        }

        // Emphasis closes only with a run that a line break before it does not
        // keep from closing, so it may not end with one.
        let bad = paragraph(vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            delimiter: EmphasisDelimiter::Asterisk,
            children: vec![
                text("foo"),
                Inline::LineBreak(LineBreak {
                    meta: NodeMeta::default(),
                    kind: LineBreakKind::Backslash,
                }),
            ],
        })]);
        assert!(!bad.validate().is_empty());
    }

    fn invalid(document: &Document) -> bool {
        !document.validate().is_empty()
            && matches!(
                document.to_markdown(),
                Err(SerializeError::InvalidDocument(_))
            )
    }

    fn emphasis(children: Vec<Inline>) -> Inline {
        Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            delimiter: EmphasisDelimiter::Asterisk,
            children,
        })
    }

    #[test]
    fn emphasis_like_content_with_whitespace_at_an_edge_is_invalid() {
        let soft_break = || {
            Inline::SoftBreak(SoftBreak {
                meta: NodeMeta::default(),
            })
        };
        for children in [
            vec![text("a ")],
            vec![text(" a")],
            vec![text("\ta")],
            vec![soft_break(), text("a")],
            vec![text("a"), soft_break()],
            // Whitespace as the parser's flanking reads it, past empty text.
            vec![text("\u{a0}a")],
            vec![text("a\u{3000}")],
            vec![text(""), text(" a")],
            vec![text("a "), text("")],
            vec![text("")],
        ] {
            assert!(invalid(&paragraph(vec![emphasis(children.clone())])));
            assert!(invalid(&paragraph(vec![Inline::Mark(Mark {
                meta: NodeMeta::default(),
                children,
            })])));
        }
        // Whitespace inside the content, or written as a reference, is valid.
        assert!(paragraph(vec![emphasis(vec![text("a b")])])
            .validate()
            .is_empty());
        let reference = Inline::CharacterReference(CharacterReference::new("&#x20;"));
        assert!(paragraph(vec![emphasis(vec![text("a"), reference])])
            .validate()
            .is_empty());
    }

    #[test]
    fn a_link_inside_link_text_is_invalid() {
        let inner = Inline::Link(Link::new("v", [Text::from("b")]));
        let nested = Inline::Link(Link::new("u", [text("a "), emphasis(vec![inner])]));
        assert!(invalid(&paragraph(vec![nested])));

        let wikilink = Inline::WikiLink(WikiLink {
            meta: NodeMeta::default(),
            target: "b".into(),
            label: "b".into(),
            embed: false,
        });
        let reference = Inline::LinkReference(LinkReference {
            meta: NodeMeta::default(),
            identifier: "a".into(),
            label: "a".into(),
            kind: ReferenceKind::Full,
            children: vec![wikilink],
        });
        assert!(invalid(&paragraph(vec![reference])));

        let autolink = Inline::Autolink(Autolink::new(AutolinkForm::Literal, "http://a.b"));
        assert!(invalid(&paragraph(vec![Inline::Link(Link::new(
            "u",
            [autolink]
        ))])));

        // A link inside image alt text is valid.
        let image = Inline::Image(Image {
            meta: NodeMeta::default(),
            destination: "i".into(),
            destination_kind: LinkDestinationKind::Bare,
            title: None,
            alt: vec![Inline::Link(Link::new("v", [Text::from("b")]))],
        });
        assert!(paragraph(vec![image]).validate().is_empty());
    }

    #[test]
    fn an_autolink_whose_text_is_not_one_autolink_of_its_form_is_invalid() {
        let autolink =
            |form, text: &str| paragraph(vec![Inline::Autolink(Autolink::new(form, text))]);
        for text in [
            "x",
            "a.b",
            // The parser's `http://` is case-sensitive.
            "HTTP://a.b",
            // The parser trims trailing punctuation off a literal URL.
            "http://a.b.",
            "wwwx.a.b",
            "http://a.b c",
        ] {
            assert!(invalid(&autolink(AutolinkForm::Literal, text)), "{text}");
        }
        for text in ["a b", "http://a.b>", ""] {
            assert!(invalid(&autolink(AutolinkForm::Angle, text)), "{text}");
        }

        for (form, text, destination) in [
            (AutolinkForm::Literal, "http://a.b", "http://a.b"),
            (AutolinkForm::Literal, "www.a.b", "http://www.a.b"),
            (AutolinkForm::Literal, "a@b.c", "mailto:a@b.c"),
            (AutolinkForm::Literal, "mailto:a@b.c", "mailto:a@b.c"),
            (AutolinkForm::Literal, "xmpp:a@b.c/d", "xmpp:a@b.c/d"),
            (AutolinkForm::Angle, "a@b.c", "mailto:a@b.c"),
            (AutolinkForm::Angle, "irc://a", "irc://a"),
            (AutolinkForm::Angle, "HTTP://a.b.", "HTTP://a.b."),
        ] {
            assert_eq!(
                Autolink::new(form, text).destination().as_deref(),
                Some(destination)
            );
            let document = autolink(form, text);
            assert!(document.validate().is_empty(), "{text}");
            let markdown = document.to_markdown().unwrap();
            assert_eq!(
                format!(
                    "{:?}",
                    crate::normalize::normalized(&parse(&markdown).document.children)
                ),
                format!("{:?}", crate::normalize::normalized(&document.children)),
                "{markdown:?}"
            );
        }
    }

    #[test]
    fn a_character_reference_must_be_exactly_one_reference() {
        let reference = |text: &str| {
            paragraph(vec![Inline::CharacterReference(CharacterReference::new(
                text,
            ))])
        };
        for text in ["", "amp", "&amp", "&amp;x", "&amp;&amp;", "&nosuch;", "&#;"] {
            assert!(invalid(&reference(text)), "{text}");
            assert_eq!(CharacterReference::new(text).value(), None, "{text}");
        }
        for (text, value) in [("&amp;", "&"), ("&#x20;", " "), ("&#0;", "\u{FFFD}")] {
            assert!(reference(text).validate().is_empty(), "{text}");
            assert_eq!(
                CharacterReference::new(text).value().as_deref(),
                Some(value)
            );
        }
    }

    #[test]
    fn adjacent_lists_with_one_marker_are_invalid() {
        let item = || ListItem {
            meta: NodeMeta::default(),
            checked: None,
            children: vec![Paragraph::new([Text::from("a")]).into()],
        };
        let list = |ordered: bool, delimiter| {
            Block::List(List {
                meta: NodeMeta::default(),
                ordered,
                start: ordered.then_some(1),
                delimiter,
                tight: true,
                children: vec![item()],
            })
        };
        let document = |first, second| Document {
            meta: NodeMeta::default(),
            children: vec![first, second],
        };
        assert!(invalid(&document(
            list(false, ListDelimiter::Dash),
            list(false, ListDelimiter::Dash)
        )));
        assert!(invalid(&document(
            list(true, ListDelimiter::Paren),
            list(true, ListDelimiter::Paren)
        )));
        let quoted = Document {
            meta: NodeMeta::default(),
            children: vec![Block::BlockQuote(BlockQuote {
                meta: NodeMeta::default(),
                children: vec![
                    list(false, ListDelimiter::Plus),
                    list(false, ListDelimiter::Plus),
                ],
            })],
        };
        assert!(invalid(&quoted));
        assert!(document(
            list(false, ListDelimiter::Dash),
            list(false, ListDelimiter::Plus)
        )
        .validate()
        .is_empty());
        assert!(document(
            list(false, ListDelimiter::Dash),
            list(true, ListDelimiter::Period)
        )
        .validate()
        .is_empty());
    }

    /// A delimiter of the other list kind is written as that kind's first
    /// marker, so the adjacent-list rule compares the written marker chars:
    /// an unordered `Period` list is written `-` and an ordered `Dash` list
    /// `1.`, each reading back as one list with its neighbor.
    #[test]
    fn adjacent_lists_compare_the_marker_they_are_written_with() {
        let item = || ListItem {
            meta: NodeMeta::default(),
            checked: None,
            children: vec![Paragraph::new([Text::from("a")]).into()],
        };
        let list = |ordered: bool, delimiter| {
            Block::List(List {
                meta: NodeMeta::default(),
                ordered,
                start: ordered.then_some(1),
                delimiter,
                tight: true,
                children: vec![item()],
            })
        };
        let document = |first, second| Document {
            meta: NodeMeta::default(),
            children: vec![first, second],
        };
        assert!(invalid(&document(
            list(false, ListDelimiter::Period),
            list(false, ListDelimiter::Dash)
        )));
        assert!(invalid(&document(
            list(true, ListDelimiter::Dash),
            list(true, ListDelimiter::Period)
        )));
        assert!(document(
            list(false, ListDelimiter::Paren),
            list(false, ListDelimiter::Asterisk)
        )
        .validate()
        .is_empty());
        assert!(document(
            list(true, ListDelimiter::Plus),
            list(true, ListDelimiter::Paren)
        )
        .validate()
        .is_empty());
    }

    #[test]
    fn directive_names_are_runs_of_letters_joined_by_dashes() {
        let directive = |name: &str| {
            paragraph(vec![Inline::TextDirective(TextDirective {
                meta: NodeMeta::default(),
                name: name.into(),
                label: vec![],
                attributes: vec![],
            })])
        };
        for name in ["h1", "my_note", "a--b", "-a", "a-", "1a", ""] {
            assert!(invalid(&directive(name)), "{name}");
        }
        for name in ["a", "note", "my-note", "A-b-C"] {
            assert!(directive(name).validate().is_empty(), "{name}");
        }
    }
}

mod written_shapes {
    //! Shapes that validate node by node but are written as Markdown that
    //! reads back as another tree or loses data. Each test pairs the rejected
    //! shapes with nearby shapes that stay valid, and the parsed inputs that
    //! hold the nearby shapes.

    use markdown_syntax::*;

    fn invalid(document: &Document) -> bool {
        !document.validate().is_empty()
            && matches!(
                document.to_markdown(),
                Err(SerializeError::InvalidDocument(_))
            )
    }

    fn valid(document: &Document) -> bool {
        document.validate().is_empty()
    }

    fn document(children: Vec<Block>) -> Document {
        Document {
            meta: NodeMeta::default(),
            children,
        }
    }

    fn paragraph(children: Vec<Inline>) -> Block {
        Block::Paragraph(Paragraph {
            meta: NodeMeta::default(),
            children,
        })
    }

    fn inlines(children: Vec<Inline>) -> Document {
        document(vec![paragraph(children)])
    }

    fn text(value: &str) -> Inline {
        Inline::Text(Text::new(value))
    }

    fn soft_break() -> Inline {
        Inline::SoftBreak(SoftBreak {
            meta: NodeMeta::default(),
        })
    }

    fn line_break(kind: LineBreakKind) -> Inline {
        Inline::LineBreak(LineBreak {
            meta: NodeMeta::default(),
            kind,
        })
    }

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

    fn delete(children: Vec<Inline>) -> Inline {
        Inline::Delete(Delete {
            meta: NodeMeta::default(),
            children,
        })
    }

    fn mark(children: Vec<Inline>) -> Inline {
        Inline::Mark(Mark {
            meta: NodeMeta::default(),
            children,
        })
    }

    fn definition(label: &str, identifier: &str) -> Block {
        Block::Definition(Definition {
            meta: NodeMeta::default(),
            label: label.into(),
            identifier: identifier.into(),
            destination: "/u".into(),
            destination_kind: LinkDestinationKind::Bare,
            title: None,
        })
    }

    fn reference(kind: ReferenceKind, label: &str, identifier: &str) -> Inline {
        Inline::LinkReference(LinkReference {
            meta: NodeMeta::default(),
            identifier: identifier.into(),
            label: label.into(),
            kind,
            children: vec![text("x")],
        })
    }

    fn footnote_reference(label: &str, identifier: &str) -> Inline {
        Inline::FootnoteReference(FootnoteReference {
            meta: NodeMeta::default(),
            label: label.into(),
            identifier: identifier.into(),
        })
    }

    fn item(checked: Option<bool>, children: Vec<Block>) -> ListItem {
        ListItem {
            meta: NodeMeta::default(),
            checked,
            children,
        }
    }

    fn list(tight: bool, items: Vec<ListItem>) -> Block {
        Block::List(List {
            tight,
            ..List::new(items)
        })
    }

    fn ordered(start: u64, items: Vec<ListItem>) -> Block {
        Block::List(List {
            ordered: true,
            start: Some(start),
            delimiter: ListDelimiter::Period,
            ..List::new(items)
        })
    }

    fn indented_code(value: &str) -> Block {
        Block::CodeBlock(CodeBlock {
            meta: NodeMeta::default(),
            kind: CodeBlockKind::Indented,
            info: None,
            value: value.into(),
        })
    }

    fn html_block(value: &str) -> Block {
        Block::HtmlBlock(HtmlBlock {
            meta: NodeMeta::default(),
            value: value.into(),
        })
    }

    fn quoted(children: Vec<Block>) -> Block {
        Block::BlockQuote(BlockQuote {
            meta: NodeMeta::default(),
            children,
        })
    }

    fn frontmatter(kind: FrontmatterKind, value: &str) -> Block {
        Block::Frontmatter(Frontmatter {
            meta: NodeMeta::default(),
            kind,
            value: value.into(),
        })
    }

    fn parsed(source: &str) -> Document {
        parse(source).document
    }

    const STAR: EmphasisDelimiter = EmphasisDelimiter::Asterisk;
    const UNDERSCORE: EmphasisDelimiter = EmphasisDelimiter::Underscore;

    /// `*` around `*a*` is written `**a**`, which is strong; `**` around
    /// `*a*` is `***a***`, emphasis around strong; `~~` or `==` around its
    /// own kind is a run of four, which opens none.
    #[test]
    fn a_span_holding_only_a_span_with_its_delimiter_is_invalid() {
        for delimiter in [STAR, UNDERSCORE] {
            let inner = || emphasis(delimiter, vec![text("a")]);
            assert!(invalid(&inlines(vec![emphasis(delimiter, vec![inner()])])));
            assert!(invalid(&inlines(vec![strong(delimiter, vec![inner()])])));
        }
        assert!(invalid(&inlines(vec![delete(vec![delete(vec![text(
            "a"
        )])])])));
        assert!(invalid(&inlines(vec![mark(vec![mark(vec![text("a")])])])));

        // Another delimiter, another kind, or more content stays valid; so
        // does strong around strong, which `****a****` parses to.
        for good in [
            emphasis(STAR, vec![emphasis(UNDERSCORE, vec![text("a")])]),
            emphasis(STAR, vec![strong(STAR, vec![text("a")])]),
            emphasis(STAR, vec![emphasis(STAR, vec![text("a")]), text(" b")]),
            strong(UNDERSCORE, vec![emphasis(STAR, vec![text("a")])]),
            delete(vec![mark(vec![text("a")])]),
        ] {
            assert!(valid(&inlines(vec![good])));
        }
        assert!(valid(&parsed("****a****")));
        assert!(valid(&parsed("***a***")));
    }

    /// Two strikethroughs side by side are written `~~a~~~~b~~`, one
    /// strikethrough of `a~~~~b`. Two emphasis spans with one delimiter
    /// char are left alone: whether their joined run closes depends on the
    /// chars around it, and `**:* ~**a**` parses to two of them.
    #[test]
    fn adjacent_strikethroughs_are_invalid() {
        let pair = |first: Inline, second: Inline| inlines(vec![first, second]);
        assert!(invalid(&pair(
            delete(vec![text("a")]),
            delete(vec![text("b")])
        )));
        assert!(valid(&pair(mark(vec![text("a")]), mark(vec![text("b")]))));
        assert!(valid(&pair(
            emphasis(STAR, vec![text("a")]),
            emphasis(STAR, vec![text("b")])
        )));
        assert!(valid(&parsed("**:* ~**a**")));
        assert!(valid(&parsed("==a====b==")));
    }

    #[test]
    fn a_reference_identifier_is_its_normalized_label() {
        for kind in [
            ReferenceKind::Full,
            ReferenceKind::Collapsed,
            ReferenceKind::Shortcut,
        ] {
            assert!(invalid(&inlines(vec![reference(kind, "Foo", "Foo")])));
            assert!(invalid(&inlines(vec![reference(kind, "a", "b")])));
            assert!(valid(&inlines(vec![reference(
                kind, "Foo  Bar", "foo bar"
            )])));
        }
        assert!(invalid(&inlines(vec![Inline::ImageReference(
            ImageReference {
                meta: NodeMeta::default(),
                identifier: "A".into(),
                label: "A".into(),
                kind: ReferenceKind::Full,
                alt: vec![text("x")],
            }
        )])));
        assert!(invalid(&document(vec![definition("Foo", "Foo")])));
        assert!(invalid(&inlines(vec![footnote_reference("A", "A")])));
        assert!(valid(&inlines(vec![footnote_reference("A", "a")])));
    }

    /// A written label reads back only in the label grammar: no unescaped
    /// bracket, no escaped close, no blank line, at most 999 chars.
    #[test]
    fn a_written_label_is_one_the_parser_reads() {
        let long = "a".repeat(1000);
        for label in [
            "a]b",
            "a[b",
            "a\\",
            "a\n\nb",
            "a\n \t\nb",
            "a\r\n\r\nb",
            long.as_str(),
        ] {
            let identifier = label.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                invalid(&inlines(vec![reference(
                    ReferenceKind::Full,
                    label,
                    &identifier
                )])),
                "{label:?}"
            );
            assert!(
                invalid(&document(vec![definition(label, &identifier)])),
                "{label:?}"
            );
        }
        // A collapsed or shortcut reference writes its text, not its label.
        assert!(valid(&inlines(vec![reference(
            ReferenceKind::Shortcut,
            "a]b",
            "a]b"
        )])));
        for (label, identifier) in [
            ("a\\]b\\\\", "a\\]b\\\\"),
            ("a\nb", "a b"),
            ("\na\n", "a"),
            (&"a".repeat(999), &"a".repeat(999)),
        ] {
            assert!(
                valid(&inlines(vec![reference(
                    ReferenceKind::Full,
                    label,
                    identifier
                )])),
                "{label:?}"
            );
            assert!(
                valid(&document(vec![definition(label, identifier)])),
                "{label:?}"
            );
        }
    }

    /// `[^a]: x` opens a footnote definition. Past the nesting limit, or on
    /// a line indented into a paragraph, it is a definition, and `[^]: x` and
    /// `[^a b]: x` always are.
    #[test]
    fn a_definition_label_is_not_a_footnote_label() {
        assert!(invalid(&document(vec![definition("^a", "^a")])));
        assert!(invalid(&document(vec![
            paragraph(vec![text("p")]),
            definition("^a", "^a"),
        ])));
        assert!(valid(&document(vec![definition("^", "^")])));
        assert!(valid(&document(vec![definition("^a b", "^a b")])));
        assert!(valid(&parsed("[^]: x")));
        assert!(valid(&parsed("[^a b]: x")));
        for source in [
            "[a]: /u\n    [^a]: x",
            "> [!NOTE]\n    [^a]: x",
            "> [!TIP]\n-   \t[^a]: x",
            &format!("{}[^a]: x", "> ".repeat(32)),
        ] {
            let document = parsed(source);
            assert!(valid(&document), "{source:?}");
            assert!(
                format!("{document:?}").contains("label: \"^a\""),
                "{source:?}"
            );
        }
    }

    #[test]
    fn a_footnote_label_is_one_the_parser_reads() {
        for label in ["a b", "a]b", "a[b", "a\\", "a\nb"] {
            assert!(
                invalid(&inlines(vec![footnote_reference(label, label)])),
                "{label:?}"
            );
            let definition = Block::FootnoteDefinition(FootnoteDefinition {
                meta: NodeMeta::default(),
                label: label.into(),
                identifier: label.into(),
                children: vec![paragraph(vec![text("n")])],
            });
            assert!(invalid(&document(vec![definition])), "{label:?}");
        }
        for label in ["a\\]b", "^x", "a\\\\"] {
            assert!(
                valid(&inlines(vec![footnote_reference(label, label)])),
                "{label:?}"
            );
        }
    }

    #[test]
    fn a_task_item_opens_with_a_paragraph() {
        let task = |children| document(vec![list(true, vec![item(Some(true), children)])]);
        assert!(invalid(&task(vec![])));
        assert!(invalid(&task(vec![paragraph(vec![])])));
        assert!(invalid(&task(vec![indented_code("x\n")])));
        assert!(invalid(&task(vec![
            definition("a", "a"),
            html_block("<div>")
        ])));
        assert!(valid(&task(vec![paragraph(vec![text("a")])])));
        assert!(valid(&task(vec![
            definition("a", "a"),
            paragraph(vec![text("b")]),
        ])));
        assert!(valid(&parsed("- [x] [a]: /u\n  b")));
    }

    #[test]
    fn a_line_ending_is_a_break() {
        assert!(invalid(&inlines(vec![text("a\nb")])));
        assert!(invalid(&inlines(vec![text("a\rb")])));
        // A break right after a break leaves an empty line, unless it is a
        // backslash break, which writes its `\` on that line.
        for (first, second) in [
            (soft_break(), soft_break()),
            (line_break(LineBreakKind::Backslash), soft_break()),
            (soft_break(), line_break(LineBreakKind::Spaces)),
            (
                line_break(LineBreakKind::Spaces),
                line_break(LineBreakKind::Spaces),
            ),
        ] {
            assert!(invalid(&inlines(vec![text("a"), first, second, text("b")])));
        }
        let backslashes = inlines(vec![
            text("a"),
            soft_break(),
            line_break(LineBreakKind::Backslash),
            line_break(LineBreakKind::Backslash),
            text("b"),
        ]);
        assert!(valid(&backslashes));
        assert!(valid(&parsed("a\n\\\n\\\nb")));
    }

    /// A table cell and a leaf or container directive label are written on
    /// one line; a text directive label may span lines.
    #[test]
    fn one_line_content_holds_no_break() {
        let cell = |children| {
            document(vec![Block::Table(Table {
                meta: NodeMeta::default(),
                alignments: vec![TableAlignment::None],
                rows: vec![TableRow {
                    meta: NodeMeta::default(),
                    cells: vec![TableCell {
                        meta: NodeMeta::default(),
                        children,
                    }],
                }],
            })])
        };
        assert!(invalid(&cell(vec![text("a"), soft_break(), text("b")])));
        assert!(invalid(&cell(vec![emphasis(
            STAR,
            vec![text("a"), line_break(LineBreakKind::Backslash), text("b")]
        )])));
        assert!(valid(&cell(vec![text("a b")])));

        let label = vec![text("a"), soft_break(), text("b")];
        assert!(invalid(&document(vec![Block::LeafDirective(
            LeafDirective {
                meta: NodeMeta::default(),
                name: "x".into(),
                label: label.clone(),
                attributes: vec![],
            }
        )])));
        assert!(invalid(&document(vec![Block::ContainerDirective(
            ContainerDirective {
                meta: NodeMeta::default(),
                name: "x".into(),
                label: label.clone(),
                attributes: vec![],
                children: vec![],
            }
        )])));
        let text_directive = |label| {
            inlines(vec![Inline::TextDirective(TextDirective {
                meta: NodeMeta::default(),
                name: "x".into(),
                label,
                attributes: vec![],
            })])
        };
        assert!(valid(&text_directive(label)));
        assert!(invalid(&text_directive(vec![
            text("a"),
            soft_break(),
            soft_break(),
            text("b")
        ])));
        assert!(valid(&parsed(":x[a\nb]")));
    }

    /// A tight item writes a block on the line after a paragraph's, where
    /// only a block that interrupts the paragraph starts.
    #[test]
    fn a_block_after_a_paragraph_in_a_tight_item_interrupts_it() {
        let after = |tight, block| {
            document(vec![list(
                tight,
                vec![item(None, vec![paragraph(vec![text("a")]), block])],
            )])
        };
        let setext = Block::Heading(Heading {
            kind: HeadingKind::Setext,
            ..Heading::new(1, [Text::from("h")])
        });
        let empty_item = list(true, vec![item(None, vec![])]);
        for bad in [
            paragraph(vec![text("b")]),
            definition("x", "x"),
            indented_code("code\n"),
            setext.clone(),
            ordered(2, vec![item(None, vec![paragraph(vec![text("b")])])]),
            empty_item,
            html_block("<span>x</span>"),
            html_block("<span>"),
            // A list whose first item the serializer starts on the line
            // after its bullet: a dash break under a dash bullet, or content
            // opening with a space.
            list(
                true,
                vec![item(
                    None,
                    vec![Block::ThematicBreak(ThematicBreak {
                        meta: NodeMeta::default(),
                        marker: ThematicBreakMarker::Dash,
                    })],
                )],
            ),
            list(true, vec![item(None, vec![html_block(" <v>")])]),
        ] {
            assert!(invalid(&after(true, bad.clone())), "{bad:?}");
            assert!(valid(&after(false, bad)));
        }
        for good in [
            Block::Heading(Heading::new(2, [Text::from("h")])),
            ordered(1, vec![item(None, vec![paragraph(vec![text("b")])])]),
            list(true, vec![item(None, vec![paragraph(vec![text("b")])])]),
            html_block("<div>"),
            quoted(vec![paragraph(vec![text("q")])]),
            Block::ThematicBreak(ThematicBreak {
                meta: NodeMeta::default(),
                marker: ThematicBreakMarker::Dash,
            }),
            Block::CodeBlock(CodeBlock {
                meta: NodeMeta::default(),
                kind: CodeBlockKind::Fenced {
                    marker: FenceMarker::Backtick,
                    length: 3,
                },
                info: None,
                value: "x\n".into(),
            }),
        ] {
            assert!(valid(&after(true, good.clone())), "{good:?}");
        }
        assert!(valid(&parsed("- a\n  | h |\n  | - |")));
        assert!(valid(&parsed("- a\n  # h\n  1. b\n  <div>")));
    }

    #[test]
    fn a_list_holds_an_item() {
        assert!(invalid(&document(vec![list(true, vec![])])));
    }

    /// Blank lines between items or between two blocks of an item make a
    /// list loose, so no source spells a loose list of one item holding at
    /// most one block.
    #[test]
    fn a_loose_list_holds_two_items_or_an_item_of_two_blocks() {
        for items in [
            vec![item(None, vec![])],
            vec![item(None, vec![paragraph(vec![text("a")])])],
        ] {
            assert!(invalid(&document(vec![list(false, items)])));
        }
        assert!(valid(&parsed("- a\n\n- b")));
        assert!(valid(&parsed("- a\n\n  b")));
    }

    #[test]
    fn frontmatter_opens_the_document_and_holds_no_fence_line() {
        assert!(invalid(&document(vec![quoted(vec![frontmatter(
            FrontmatterKind::Yaml,
            "a: 1"
        )])])));
        assert!(invalid(&document(vec![list(
            true,
            vec![item(
                None,
                vec![frontmatter(FrontmatterKind::Toml, "a = 1")]
            )]
        )])));
        assert!(invalid(&document(vec![frontmatter(
            FrontmatterKind::Yaml,
            "a: 1\n---\nb: 2"
        )])));
        assert!(invalid(&document(vec![frontmatter(
            FrontmatterKind::Toml,
            "a = 1\r\n+++  "
        )])));
        assert!(valid(&document(vec![frontmatter(
            FrontmatterKind::Yaml,
            "a: 1\n+++\n----"
        )])));
    }

    #[test]
    fn an_inline_footnote_holds_content() {
        let footnote = |children| {
            inlines(vec![Inline::InlineFootnote(InlineFootnote {
                meta: NodeMeta::default(),
                children,
            })])
        };
        assert!(invalid(&footnote(vec![])));
        assert!(valid(&footnote(vec![text("n")])));
    }

    #[test]
    fn an_alert_title_is_what_its_marker_line_gives() {
        let alert = |title: Option<&str>| {
            document(vec![Block::Alert(Alert {
                meta: NodeMeta::default(),
                kind: AlertKind::Note,
                title: title.map(Into::into),
                children: vec![paragraph(vec![text("a")])],
            })])
        };
        for title in ["", " t", "t\t", "  "] {
            assert!(invalid(&alert(Some(title))), "{title:?}");
        }
        assert!(valid(&alert(Some("t u"))));
        assert!(valid(&alert(None)));
    }

    #[test]
    fn a_code_block_info_string_is_not_empty() {
        let fenced = |info: Option<&str>| {
            document(vec![Block::CodeBlock(CodeBlock {
                meta: NodeMeta::default(),
                kind: CodeBlockKind::Fenced {
                    marker: FenceMarker::Backtick,
                    length: 3,
                },
                info: info.map(Into::into),
                value: "x\n".into(),
            })])
        };
        assert!(invalid(&fenced(Some(""))));
        assert!(valid(&fenced(Some("rust"))));
        assert!(valid(&fenced(None)));
    }

    /// An empty bare destination is written `<>`, an angle-bracket one.
    #[test]
    fn a_bare_destination_is_not_empty() {
        let link = |kind| {
            inlines(vec![Inline::Link(Link {
                destination_kind: kind,
                ..Link::new("", [Text::from("a")])
            })])
        };
        assert!(invalid(&link(LinkDestinationKind::Bare)));
        assert!(valid(&link(LinkDestinationKind::Angle)));
        assert!(valid(&link(LinkDestinationKind::Omitted)));
        let image = Inline::Image(Image {
            meta: NodeMeta::default(),
            destination: String::new(),
            destination_kind: LinkDestinationKind::Bare,
            title: None,
            alt: vec![text("a")],
        });
        assert!(invalid(&inlines(vec![image])));
        let mut definition = definition("a", "a");
        if let Block::Definition(node) = &mut definition {
            node.destination.clear();
        }
        assert!(invalid(&document(vec![definition])));
    }

    /// No fence bounds indented code: its value starts and ends with a line
    /// that is not blank.
    #[test]
    fn indented_code_starts_and_ends_with_content() {
        for value in [
            "",
            "\n",
            "  \t\n",
            "\na\n",
            " \na\n",
            "a\n\n",
            "a\n \n",
            "a\r\n\r\n",
        ] {
            assert!(invalid(&document(vec![indented_code(value)])), "{value:?}");
        }
        for value in ["a", "a\n", "a\n\nb\n", " a\n  \n\tb\r\n"] {
            assert!(valid(&document(vec![indented_code(value)])), "{value:?}");
        }
        assert!(valid(&parsed("    a\n    \n    b\n\n")));
    }
}

mod emphasis_edges {
    use markdown_syntax::*;

    fn emphasis_opening_with(kind: LineBreakKind) -> Document {
        let emphasis = Emphasis {
            meta: NodeMeta::default(),
            delimiter: EmphasisDelimiter::Asterisk,
            children: vec![
                Inline::LineBreak(LineBreak {
                    meta: NodeMeta::default(),
                    kind,
                }),
                Text::from("a").into(),
            ],
        };
        Document {
            meta: NodeMeta::default(),
            children: vec![Paragraph::new([Inline::Emphasis(emphasis)]).into()],
        }
    }

    #[test]
    fn an_emphasis_may_open_with_a_backslash_break() {
        // `*` before `\` is followed by punctuation, which opens it at the
        // start of a paragraph: `*\` + line ending + `a*` parses to this tree.
        let document = emphasis_opening_with(LineBreakKind::Backslash);
        assert_eq!(document.validate(), Vec::new());
        assert_eq!(document.to_markdown().unwrap(), "*\\\na*\n");
    }

    #[test]
    fn an_emphasis_cannot_open_with_a_break_of_trailing_spaces() {
        let document = emphasis_opening_with(LineBreakKind::Spaces);
        assert_eq!(document.validate().len(), 1);
        assert!(matches!(
            document.to_markdown(),
            Err(SerializeError::InvalidDocument(_))
        ));
    }
}
