mod support;

use std::path::Path;

use markdown_syntax::{
    parse_strict_with_options, parse_with_options, to_markdown, validate_document, Block,
    Constructs, DiagnosticCode, DiagnosticSeverity, Document, Inline, LineIndex, ParseOptions,
    ParseStrictError, SerializeError, Span, SyntaxConfigError, SyntaxOptions,
};

use support::fixtures::{
    assert_case_file_stable, assert_fixture, assert_parse_serialize_stable,
    assert_required_profiles, assert_semantic_input_corpus_stable, profile_options,
    snapshot_document,
};

#[test]
fn core_fixture_snapshots_and_roundtrips() {
    assert_fixture(
        "tests/fixtures/roundtrip/core/heading_emphasis",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/core/list",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/core/code_html",
        SyntaxOptions::commonmark(),
    );
}

#[test]
fn commonmark_spec_fixture_snapshots_and_roundtrips() {
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_blocks",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_blockquotes",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_inlines",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_attention",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_autolinks",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_code_spans",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_hard_breaks",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_references_html",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_html_inlines",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_html_blocks",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_html_raw_blocks",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_tabs",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_lists",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_character_references",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_character_escapes",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_link_resources",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_link_resource_edges",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_link_nesting",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_inline_precedence",
        SyntaxOptions::commonmark(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/spec/commonmark_reference_labels",
        SyntaxOptions::commonmark(),
    );
}

#[test]
fn extension_fixture_snapshots_and_roundtrips() {
    let mut constructs = Constructs::gfm();
    constructs.math_block = true;
    constructs.math_inline = true;
    constructs.directive_text = true;
    constructs.directive_leaf = true;
    constructs.directive_container = true;
    constructs.frontmatter = true;
    constructs.gfm_alert = true;
    constructs.underline = true;
    constructs.insert = true;
    constructs.highlight = true;
    constructs.subscript = true;
    constructs.superscript = true;
    constructs.spoiler = true;
    constructs.shortcode = true;
    constructs.description_list = true;
    constructs.inline_footnote = true;
    let options = SyntaxOptions::custom(
        constructs,
        ParseOptions {
            single_tilde_strikethrough: true,
            preserve_character_escapes: false,
            preserve_character_references: false,
        },
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/table_math_directive",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/math_edges",
        options.clone(),
    );
    let escape_options = SyntaxOptions::custom(
        Constructs::commonmark(),
        ParseOptions {
            preserve_character_escapes: true,
            ..ParseOptions::default()
        },
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/character_escapes_preserved",
        escape_options,
    );
    let reference_options = SyntaxOptions::custom(
        Constructs::commonmark(),
        ParseOptions {
            preserve_character_references: true,
            ..ParseOptions::default()
        },
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/character_references_preserved",
        reference_options,
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/gfm_table_edges",
        SyntaxOptions::gfm(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/gfm_table_cells",
        SyntaxOptions::gfm(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/gfm_table_invalid",
        SyntaxOptions::gfm(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/gfm_table_containers",
        SyntaxOptions::gfm(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/gfm_footnotes",
        SyntaxOptions::gfm(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/gfm_footnote_edges",
        SyntaxOptions::gfm(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/inline_footnotes",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/gfm_autolinks",
        SyntaxOptions::gfm(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/gfm_task_list",
        SyntaxOptions::gfm(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/gfm_alerts",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/inline_markup_extras",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/insert_highlight",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/shortcodes",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/description_lists_core",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/description_lists_edges",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/description_lists_blocks",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/directive_attributes",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/frontmatter_yaml",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/frontmatter_toml",
        options.clone(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/directive_nested",
        options,
    );
}

#[test]
fn mdx_fixture_snapshot_and_roundtrip() {
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/mdx",
        SyntaxOptions::mdx(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/mdx_multiline",
        SyntaxOptions::mdx(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/mdx_jsx_flow",
        SyntaxOptions::mdx(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/mdx_esm",
        SyntaxOptions::mdx(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/mdx_inline",
        SyntaxOptions::mdx(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/mdx_jsx_inline",
        SyntaxOptions::mdx(),
    );
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/mdx_html_like",
        SyntaxOptions::mdx(),
    );
}

#[test]
fn wikilink_fixture_snapshots_and_roundtrips() {
    let mut after_constructs = Constructs::gfm();
    after_constructs.wikilink_title_after_pipe = true;
    let after_options = SyntaxOptions::custom(after_constructs, ParseOptions::default());
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/wikilinks_after_pipe",
        after_options,
    );

    let mut before_constructs = Constructs::gfm();
    before_constructs.wikilink_title_before_pipe = true;
    let before_options = SyntaxOptions::custom(before_constructs, ParseOptions::default());
    assert_fixture(
        "tests/fixtures/roundtrip/extensions/wikilinks_before_pipe",
        before_options,
    );
}

#[test]
fn stability_fixture_texts_roundtrip_stably() {
    const FIXTURES: &[&str] = &[
        "alerts",
        "description_lists",
        "math_code",
        "math_dollars",
        "multiline_alerts",
        "multiline_blockquote",
        "wikilinks_title_after_pipe",
        "wikilinks_title_before_pipe",
    ];

    for fixture in FIXTURES {
        let options = match *fixture {
            "alerts" | "description_lists" | "multiline_alerts" => profile_options("extras"),
            "math_code" | "math_dollars" => profile_options("math"),
            "wikilinks_title_after_pipe" => profile_options("wikilink-after"),
            "wikilinks_title_before_pipe" => profile_options("wikilink-before"),
            _ => profile_options("commonmark"),
        };
        assert_parse_serialize_stable(
            &format!("tests/fixtures/roundtrip/stability/{fixture}.md"),
            &options,
        );
    }
}

#[test]
fn derived_case_corpus_roundtrips_stably() {
    let cases_root = Path::new("tests/fixtures/roundtrip/cases");
    let semantic = assert_semantic_input_corpus_stable(cases_root);

    assert!(
        semantic.commonmark_cases > 1_400,
        "expected substantial CommonMark-dialect semantic input corpus, got {}",
        semantic.commonmark_cases
    );
    assert!(
        semantic.gfm_cases > 500,
        "expected substantial GFM-dialect semantic input corpus, got {}",
        semantic.gfm_cases
    );
    assert_required_profiles(&semantic.profiles);
}

#[test]
fn commonmark_example_inputs_roundtrip_stably() {
    let count = assert_case_file_stable(
        Path::new("tests/fixtures/roundtrip/examples/official-stable-inputs.cases"),
        &SyntaxOptions::commonmark(),
    );

    assert_eq!(
        count, 8,
        "CommonMark selected input stability corpus drifted"
    );
}

#[test]
fn html_syntax_nodes_are_preserved() {
    let input = concat!(
        "<script>\n",
        "const value = '<tag>';\n",
        "\n",
        "</script>\n",
        "\n",
        "Text <span data-x=\"1\">ok</span> and <!-- inline -->.\n"
    );
    let output = parse_with_options(input, &SyntaxOptions::commonmark()).unwrap();
    assert_eq!(output.diagnostics, Vec::new());
    assert!(matches!(
        output.document.children.first(),
        Some(Block::HtmlBlock(_))
    ));
    assert!(snapshot_document(&output.document).contains("HtmlInline \"<span data-x=\\\"1\\\">\""));
    assert!(snapshot_document(&output.document).contains("HtmlInline \"<!-- inline -->\""));

    let markdown = to_markdown(&output.document).unwrap();
    let reparsed = parse_with_options(&markdown, &SyntaxOptions::commonmark()).unwrap();
    assert_eq!(
        snapshot_document(&reparsed.document),
        snapshot_document(&output.document)
    );
}

#[test]
fn gfm_footnote_label_length_limit_is_enforced() {
    let valid = "x".repeat(999);
    let invalid = "x".repeat(1000);

    let valid_output = parse_with_options(
        &format!("[^{valid}].\n\n[^{valid}]: ok\n"),
        &SyntaxOptions::gfm(),
    )
    .unwrap();
    assert!(matches!(
        valid_output.document.children.first(),
        Some(Block::Paragraph(_))
    ));
    assert!(valid_output
        .document
        .children
        .iter()
        .any(|block| matches!(block, Block::FootnoteDefinition(_))));

    let invalid_output = parse_with_options(
        &format!("[^{invalid}].\n\n[^{invalid}]: nope\n"),
        &SyntaxOptions::gfm(),
    )
    .unwrap();
    assert!(!invalid_output
        .document
        .children
        .iter()
        .any(|block| matches!(block, Block::FootnoteDefinition(_))));
    assert!(!snapshot_document(&invalid_output.document).contains("FootnoteReference"));
}

#[test]
fn wikilink_label_length_limit_is_enforced() {
    let mut constructs = Constructs::commonmark();
    constructs.wikilink_title_after_pipe = true;
    let options = SyntaxOptions::custom(constructs, ParseOptions::default());
    let valid = "x".repeat(999);
    let invalid = "x".repeat(1000);

    let valid_output =
        parse_with_options(&format!("[[{valid}]]\n"), &options).expect("valid wikilink parse");
    assert!(snapshot_document(&valid_output.document).contains("WikiLink"));

    let invalid_output =
        parse_with_options(&format!("[[{invalid}]]\n"), &options).expect("valid fallback parse");
    assert!(!snapshot_document(&invalid_output.document).contains("WikiLink"));
}

#[test]
fn gfm_alerts_allow_empty_and_nested_blockquote_positions() {
    let mut constructs = Constructs::commonmark();
    constructs.gfm_alert = true;
    let options = SyntaxOptions::custom(constructs, ParseOptions::default());

    let empty = parse_with_options("> [!note]\n", &options).expect("valid alert parse");
    let empty_snapshot = snapshot_document(&empty.document);
    assert!(empty_snapshot.contains("Alert kind=note title=none"));
    assert!(!empty_snapshot.contains("Paragraph"));

    let nested = parse_with_options("- item one\n\n  > [!note]\n  > Pay attention\n", &options)
        .expect("valid nested alert parse");
    let nested_snapshot = snapshot_document(&nested.document);
    assert!(nested_snapshot.contains("Alert kind=note title=none"));
    assert!(nested_snapshot.contains("Text \"Pay attention\""));
}

#[test]
fn subscript_does_not_imply_strikethrough() {
    let mut constructs = Constructs::commonmark();
    constructs.subscript = true;
    let options = SyntaxOptions::custom(constructs, ParseOptions::default());
    let output = parse_with_options("~~H~2~O~~\n", &options).expect("valid subscript parse");
    let snapshot = snapshot_document(&output.document);

    assert!(snapshot.contains("Subscript"));
    assert!(!snapshot.contains("Delete"));
}

#[test]
fn strikethrough_can_contain_subscript() {
    let mut constructs = Constructs::commonmark();
    constructs.gfm_strikethrough = true;
    constructs.subscript = true;
    let options = SyntaxOptions::custom(
        constructs,
        ParseOptions {
            single_tilde_strikethrough: false,
            preserve_character_escapes: false,
            preserve_character_references: false,
        },
    );
    let output = parse_with_options("~~H~2~O~~\n", &options).expect("valid strikethrough parse");
    let snapshot = snapshot_document(&output.document);

    assert!(snapshot.contains("Delete"));
    assert!(snapshot.contains("Subscript"));
}

#[test]
fn description_list_markers_need_a_term_and_content() {
    let mut constructs = Constructs::commonmark();
    constructs.description_list = true;
    let options = SyntaxOptions::custom(constructs, ParseOptions::default());

    let marker_only = parse_with_options(": foo\n", &options).expect("valid fallback parse");
    assert!(!snapshot_document(&marker_only.document).contains("DescriptionList"));

    let empty_details = parse_with_options("a\n:\n", &options).expect("valid fallback parse");
    assert!(!snapshot_document(&empty_details.document).contains("DescriptionList"));
}

#[test]
fn frontmatter_is_extension_only_and_document_start_only() {
    let commonmark = parse_with_options("---\ntitle: Jupyter\n---\n", &SyntaxOptions::commonmark())
        .expect("valid CommonMark parse");
    assert!(!snapshot_document(&commonmark.document).contains("Frontmatter"));

    let mut constructs = Constructs::commonmark();
    constructs.frontmatter = true;
    let options = SyntaxOptions::custom(constructs, ParseOptions::default());
    let after_content = parse_with_options("## Neptune\n---\n---\n", &options)
        .expect("valid frontmatter-extension parse");
    assert!(!snapshot_document(&after_content.document).contains("Frontmatter"));

    let in_container =
        parse_with_options("> ---\n> ---\n", &options).expect("valid frontmatter-extension parse");
    assert!(!snapshot_document(&in_container.document).contains("Frontmatter"));
}

#[test]
fn invalid_options_fail_closed() {
    let mut options = SyntaxOptions::mdx();
    options.constructs.html_inline = true;
    assert_eq!(
        parse_with_options("<X />", &options).unwrap_err(),
        SyntaxConfigError::MdxHtmlConflict
    );

    let mut options = SyntaxOptions::commonmark();
    options.constructs.wikilink_title_after_pipe = true;
    options.constructs.wikilink_title_before_pipe = true;
    assert_eq!(
        parse_with_options("[[target]]", &options).unwrap_err(),
        SyntaxConfigError::WikilinkTitleOrderConflict
    );
}

#[test]
fn strict_parse_promotes_extension_diagnostics() {
    let mut constructs = Constructs::commonmark();
    constructs.directive_leaf = true;
    let options = SyntaxOptions::custom(constructs, ParseOptions::default());
    let err = parse_strict_with_options("::1bad\n", &options).unwrap_err();
    match err {
        ParseStrictError::Diagnostic(diagnostic) => {
            assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
            assert_eq!(diagnostic.code, DiagnosticCode::InvalidDirectiveName);
        }
        ParseStrictError::Config(_) => panic!("expected diagnostic strict failure"),
    }
}

#[test]
fn validation_and_serializer_reject_invalid_ast() {
    let mut document = Document::default();
    document
        .children
        .push(Block::Heading(markdown_syntax::Heading {
            meta: markdown_syntax::NodeMeta::default(),
            depth: 9,
            kind: markdown_syntax::HeadingKind::Atx,
            children: vec![Inline::Text(markdown_syntax::Text {
                meta: markdown_syntax::NodeMeta::default(),
                value: "bad".into(),
            })],
        }));

    let diagnostics = validate_document(&document);
    assert_eq!(diagnostics.len(), 1);
    assert!(matches!(
        to_markdown(&document).unwrap_err(),
        SerializeError::InvalidDocument(_)
    ));
}

#[test]
fn line_index_uses_half_open_byte_offsets() {
    let source = "a\né\r\nb";
    let index = LineIndex::new(source);
    assert_eq!(index.position(0).line, 1);
    assert_eq!(index.position(2).line, 2);
    assert_eq!(index.position(2).column, 1);
    assert_eq!(index.position(4).column, 3);
    assert_eq!(index.position(source.len()).line, 3);
    assert_eq!(Span::new(0, 1).len(), 1);
}
