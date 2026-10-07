mod support;

use std::path::Path;

use markdown_syntax::{
    parse, Block, DiagnosticCode, DiagnosticSeverity, Document, Inline, LineIndex, SerializeError,
    Span,
};

use support::fixtures::{
    assert_case_file_stable, assert_fixture, assert_fixture_not_reading_back,
    assert_parse_serialize_stable, assert_semantic_input_corpus_stable, snapshot_document,
    snapshot_document_normalized,
};

// Awaiting a decision (plan one-normative-syntax, Risks): each input below
// holds a paragraph continuation line whose indentation, or whose reaching
// the paragraph lazily, keeps it from opening a block (a table delimiter row,
// a heading, a setext underline, a thematic break, or a list item). The AST
// records neither, and the serializer writes such a line unindented and with
// its container's full prefix, so it reads back as that block.

/// Fixtures whose Markdown does not read back, pending that decision.
const AWAITING_DECISION_FIXTURES: &[&str] = &[
    // `> | a |` with a lazy `| - |`, and lazy delimiter rows after quotes.
    "tests/fixtures/roundtrip/extensions/gfm_table_containers",
    // `| Literal |` over a delimiter row indented four columns.
    "tests/fixtures/roundtrip/extensions/gfm_table_edges",
];

/// Derived cases whose Markdown does not read back, pending that decision.
const AWAITING_DECISION_CASES: &[(&str, usize)] = &[
    // A lazy `- b` after a quote's paragraph.
    ("commonmark/block_quote.cases", 14),
    // Lazy or indented table delimiter rows.
    ("commonmark/gfm_table.cases", 24),
    ("commonmark/gfm_table.cases", 32),
    ("commonmark/gfm_table.cases", 48),
    ("commonmark/gfm_table.cases", 57),
    // An item's continuation line indented short of a nested list item.
    ("commonmark/gfm_table.cases", 29),
    ("commonmark/gfm_table.cases", 30),
    ("commonmark/list.cases", 64),
    // An indented `# bar`, `=`, `***`, or `-` continuing a paragraph.
    ("commonmark/heading_atx.cases", 17),
    ("commonmark/heading_setext.cases", 14),
    ("commonmark/heading_setext.cases", 15),
    ("commonmark/thematic_break.cases", 11),
    ("gfm/fuzz.cases", 5),
    ("gfm/phoenix_heex.cases", 5),
    // A lazy `===` after a quote's paragraph.
    ("commonmark/heading_setext.cases", 23),
    ("commonmark/heading_setext.cases", 43),
    // A code block after a list, indented short of the item's content.
    ("commonmark/list.cases", 8),
    ("commonmark/list.cases", 65),
];

#[test]
fn core_fixture_snapshots_and_roundtrips() {
    assert_fixture("tests/fixtures/roundtrip/core/heading_emphasis");
    assert_fixture("tests/fixtures/roundtrip/core/list");
    assert_fixture("tests/fixtures/roundtrip/core/code_html");
}

#[test]
fn commonmark_spec_fixture_snapshots_and_roundtrips() {
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_blocks");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_blockquotes");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_inlines");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_attention");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_autolinks");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_code_spans");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_hard_breaks");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_references_html");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_html_inlines");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_html_blocks");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_html_raw_blocks");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_tabs");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_lists");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_character_references");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_character_escapes");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_link_resources");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_link_resource_edges");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_link_nesting");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_inline_precedence");
    assert_fixture("tests/fixtures/roundtrip/spec/commonmark_reference_labels");
}

#[test]
fn extension_fixture_snapshots_and_roundtrips() {
    assert_fixture("tests/fixtures/roundtrip/extensions/table_math_directive");
    assert_fixture("tests/fixtures/roundtrip/extensions/math_edges");
    assert_fixture("tests/fixtures/roundtrip/extensions/character_escapes_preserved");
    assert_fixture("tests/fixtures/roundtrip/extensions/character_references_preserved");
    assert_fixture("tests/fixtures/roundtrip/extensions/gfm_table_cells");
    assert_fixture("tests/fixtures/roundtrip/extensions/gfm_table_invalid");
    assert_fixture("tests/fixtures/roundtrip/extensions/gfm_footnotes");
    assert_fixture("tests/fixtures/roundtrip/extensions/gfm_footnote_edges");
    assert_fixture("tests/fixtures/roundtrip/extensions/inline_footnotes");
    assert_fixture("tests/fixtures/roundtrip/extensions/gfm_autolinks");
    assert_fixture("tests/fixtures/roundtrip/extensions/gfm_task_list");
    assert_fixture("tests/fixtures/roundtrip/extensions/gfm_alerts");
    assert_fixture("tests/fixtures/roundtrip/extensions/inline_markup_extras");
    assert_fixture("tests/fixtures/roundtrip/extensions/html_containers");
    assert_fixture("tests/fixtures/roundtrip/extensions/insert_highlight");
    assert_fixture("tests/fixtures/roundtrip/extensions/shortcodes");
    assert_fixture("tests/fixtures/roundtrip/extensions/description_lists_core");
    assert_fixture("tests/fixtures/roundtrip/extensions/description_lists_edges");
    assert_fixture("tests/fixtures/roundtrip/extensions/description_lists_blocks");
    assert_fixture("tests/fixtures/roundtrip/extensions/directive_attributes");
    assert_fixture("tests/fixtures/roundtrip/extensions/frontmatter_yaml");
    assert_fixture("tests/fixtures/roundtrip/extensions/frontmatter_toml");
    assert_fixture("tests/fixtures/roundtrip/extensions/directive_nested");
}

#[test]
fn mdx_fixture_snapshot_and_roundtrip() {
    assert_fixture("tests/fixtures/roundtrip/extensions/mdx");
    assert_fixture("tests/fixtures/roundtrip/extensions/mdx_multiline");
    assert_fixture("tests/fixtures/roundtrip/extensions/mdx_jsx_flow");
    assert_fixture("tests/fixtures/roundtrip/extensions/mdx_esm");
    assert_fixture("tests/fixtures/roundtrip/extensions/mdx_inline");
    assert_fixture("tests/fixtures/roundtrip/extensions/mdx_jsx_inline");
    assert_fixture("tests/fixtures/roundtrip/extensions/mdx_html_like");
}

#[test]
fn fixtures_awaiting_a_decision_keep_their_goldens() {
    for stem in AWAITING_DECISION_FIXTURES {
        assert_fixture_not_reading_back(stem);
    }
}

#[test]
fn wikilink_fixture_snapshots_and_roundtrips() {
    assert_fixture("tests/fixtures/roundtrip/extensions/wikilinks_after_pipe");
    assert_fixture("tests/fixtures/roundtrip/extensions/wikilinks_before_pipe");
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
        assert_parse_serialize_stable(&format!("tests/fixtures/roundtrip/stability/{fixture}.md"));
    }
}

#[test]
fn derived_case_corpus_roundtrips_stably() {
    let cases_root = Path::new("tests/fixtures/roundtrip/cases");
    let semantic = assert_semantic_input_corpus_stable(cases_root, AWAITING_DECISION_CASES);

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
}

#[test]
fn commonmark_example_inputs_roundtrip_stably() {
    let count = assert_case_file_stable(Path::new(
        "tests/fixtures/roundtrip/examples/official-stable-inputs.cases",
    ));

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
    let output = parse(input);
    assert_eq!(output.diagnostics, Vec::new());
    assert!(matches!(
        output.document.children.first(),
        Some(Block::HtmlBlock(_))
    ));
    assert!(snapshot_document(&output.document).contains("HtmlInline \"<span data-x=\\\"1\\\">\""));
    assert!(snapshot_document(&output.document).contains("HtmlInline \"<!-- inline -->\""));

    let markdown = output.document.to_markdown().unwrap();
    let reparsed = parse(&markdown);
    assert_eq!(
        snapshot_document_normalized(&reparsed.document),
        snapshot_document_normalized(&output.document)
    );
}

#[test]
fn gfm_footnote_label_length_limit_is_enforced() {
    let valid = "x".repeat(999);
    let invalid = "x".repeat(1000);

    let valid_output = parse(&format!("[^{valid}].\n\n[^{valid}]: ok\n"));
    assert!(matches!(
        valid_output.document.children.first(),
        Some(Block::Paragraph(_))
    ));
    assert!(valid_output
        .document
        .children
        .iter()
        .any(|block| matches!(block, Block::FootnoteDefinition(_))));

    let invalid_output = parse(&format!("[^{invalid}].\n\n[^{invalid}]: nope\n"));
    assert!(!invalid_output
        .document
        .children
        .iter()
        .any(|block| matches!(block, Block::FootnoteDefinition(_))));
    assert!(!snapshot_document(&invalid_output.document).contains("FootnoteReference"));
}

#[test]
fn wikilink_label_length_limit_is_enforced() {
    let valid = "x".repeat(999);
    let invalid = "x".repeat(1000);

    let valid_output = parse(&format!("[[{valid}]]\n"));
    assert!(snapshot_document(&valid_output.document).contains("WikiLink"));

    let invalid_output = parse(&format!("[[{invalid}]]\n"));
    assert!(!snapshot_document(&invalid_output.document).contains("WikiLink"));
}

#[test]
fn gfm_alerts_allow_empty_and_nested_blockquote_positions() {
    let empty = parse("> [!note]\n");
    let empty_snapshot = snapshot_document(&empty.document);
    assert!(empty_snapshot.contains("Alert kind=note title=none"));
    assert!(!empty_snapshot.contains("Paragraph"));

    let nested = parse("- item one\n\n  > [!note]\n  > Pay attention\n");
    let nested_snapshot = snapshot_document(&nested.document);
    assert!(nested_snapshot.contains("Alert kind=note title=none"));
    assert!(nested_snapshot.contains("Text \"Pay attention\""));
}

#[test]
fn single_tildes_inside_strikethrough_stay_text() {
    let output = parse("~~H~2~O~~\n");
    let snapshot = snapshot_document(&output.document);

    assert!(snapshot.contains("Delete"), "{snapshot}");
    assert!(snapshot.contains("Text \"H~2~O\""), "{snapshot}");
}

#[test]
fn former_description_markers_are_paragraph_text() {
    let marker_only = snapshot_document(&parse(": foo\n").document);
    assert!(marker_only.contains("Text \": foo\""), "{marker_only}");

    let empty_details = snapshot_document(&parse("a\n:\n").document);
    assert!(empty_details.contains("Paragraph"), "{empty_details}");
    assert!(empty_details.contains("Text \":\""), "{empty_details}");
}

#[test]
fn frontmatter_is_document_start_only() {
    let at_start = parse("---\ntitle: Jupyter\n---\n");
    assert!(snapshot_document(&at_start.document).contains("Frontmatter"));

    let after_content = parse("## Neptune\n---\n---\n");
    assert!(!snapshot_document(&after_content.document).contains("Frontmatter"));

    let in_container = parse("> ---\n> ---\n");
    assert!(!snapshot_document(&in_container.document).contains("Frontmatter"));
}

#[test]
fn malformed_leaf_directive_reports_an_error() {
    let output = parse("::1bad\n");
    assert!(
        output.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == DiagnosticSeverity::Error
                && diagnostic.code == DiagnosticCode::InvalidDirectiveName
        }),
        "{:?}",
        output.diagnostics
    );
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

    let diagnostics = document.validate();
    assert_eq!(diagnostics.len(), 1);
    assert!(matches!(
        document.to_markdown().unwrap_err(),
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
