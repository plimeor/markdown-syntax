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
                form: LinkForm::Inline,
                destination: destination.into(),
                destination_kind: LinkDestinationKind::Bare,
                title: None,
                title_kind: None,
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

    // SR9 — inline code stored as a raw passthrough whose backtick run is at least
    // as long as its fence would close the span early.
    #[test]
    fn sr9_inline_code_raw_backtick_run_is_invalid() {
        let bad = paragraph(vec![Inline::Code(CodeInline {
            meta: NodeMeta::default(),
            value: "a`b".into(),
            raw: "a`b".into(),
            fence_length: 1,
        })]);
        assert!(!bad.validate().is_empty());

        // A raw run shorter than the fence is safe.
        let shorter = paragraph(vec![Inline::Code(CodeInline {
            meta: NodeMeta::default(),
            value: "a`b".into(),
            raw: "a`b".into(),
            fence_length: 2,
        })]);
        assert!(shorter.validate().is_empty());

        // A raw run LONGER than the fence is also inert (a fence of length N closes
        // only on a run of exactly N) — this is the `` ` `` ` `` code-span shape.
        let longer = paragraph(vec![Inline::Code(CodeInline {
            meta: NodeMeta::default(),
            value: "``".into(),
            raw: " `` ".into(),
            fence_length: 1,
        })]);
        assert!(longer.validate().is_empty());

        // The value path (no raw passthrough) is always safe.
        let value_only = paragraph(vec![Inline::Code(CodeInline {
            meta: NodeMeta::default(),
            value: "a`b".into(),
            raw: String::new(),
            fence_length: 0,
        })]);
        assert!(value_only.validate().is_empty());
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
                title_kind: None,
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
                title_kind: None,
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
        let reference = Inline::CharacterReference(CharacterReference {
            meta: NodeMeta::default(),
            reference: "&#x20;".into(),
            value: " ".into(),
        });
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

        // A link inside image alt text is valid.
        let image = Inline::Image(Image {
            meta: NodeMeta::default(),
            destination: "i".into(),
            destination_kind: LinkDestinationKind::Bare,
            title: None,
            title_kind: None,
            alt: vec![Inline::Link(Link::new("v", [Text::from("b")]))],
        });
        assert!(paragraph(vec![image]).validate().is_empty());
    }

    #[test]
    fn an_autolink_form_that_does_not_fit_its_content_is_invalid() {
        let link = |form, destination: &str, text: &str| {
            let mut link = Link::new(destination, [Text::from(text)]);
            link.form = form;
            paragraph(vec![Inline::Link(link)])
        };
        assert!(invalid(&link(LinkForm::LiteralAutolink, "http://a.b", "x")));
        assert!(invalid(&link(
            LinkForm::LiteralAutolink,
            "http://a.b",
            "a.b"
        )));
        assert!(invalid(&link(
            LinkForm::LiteralAutolink,
            "mailto:a.b",
            "a.b"
        )));
        assert!(invalid(&link(LinkForm::AngleAutolink, "http://a.b", "a b")));
        let mut titled = Link::new("http://a.b", [Text::from("http://a.b")]);
        titled.form = LinkForm::AngleAutolink;
        titled.title = Some("t".into());
        titled.title_kind = Some(LinkTitleKind::DoubleQuote);
        assert!(invalid(&paragraph(vec![Inline::Link(titled)])));

        for (form, destination, text) in [
            (LinkForm::LiteralAutolink, "http://a.b", "http://a.b"),
            (LinkForm::LiteralAutolink, "http://www.a.b", "www.a.b"),
            (LinkForm::LiteralAutolink, "mailto:a@b.c", "a@b.c"),
            (LinkForm::AngleAutolink, "mailto:a@b.c", "a@b.c"),
            (LinkForm::AngleAutolink, "irc://a", "irc://a"),
        ] {
            let document = link(form, destination, text);
            assert!(document.validate().is_empty(), "{destination}");
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
