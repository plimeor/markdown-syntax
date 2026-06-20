use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use markdown_syntax::{
    parse_strict_with_options, parse_with_options, to_markdown, to_markdown_with_options,
    validate_document, AutolinkKind, Block, Constructs, DiagnosticCode, DiagnosticSeverity,
    Document, Inline, LineIndex, ParseOptions, ParseStrictError, SerializeError, SerializeOptions,
    Span, SyntaxConfigError, SyntaxOptions,
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
    let official = read_derived_metadata(Path::new(
        "tests/fixtures/roundtrip/examples/official-inputs.cases",
    ));
    assert_eq!(
        official.count, 652,
        "CommonMark official full input corpus is not fully accounted for"
    );
    assert_eq!(official.role, None);

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

fn profile_options(profile: &str) -> SyntaxOptions {
    match profile {
        "commonmark" => SyntaxOptions::commonmark(),
        "gfm" => SyntaxOptions::gfm(),
        "mdx" => SyntaxOptions::mdx(),
        "math" => {
            let mut constructs = Constructs::commonmark();
            constructs.math_block = true;
            constructs.math_inline = true;
            SyntaxOptions::custom(constructs, ParseOptions::default())
        }
        "frontmatter" => {
            let mut constructs = Constructs::commonmark();
            constructs.frontmatter = true;
            SyntaxOptions::custom(constructs, ParseOptions::default())
        }
        "preserve-escapes" => SyntaxOptions::custom(
            Constructs::commonmark(),
            ParseOptions {
                preserve_character_escapes: true,
                ..ParseOptions::default()
            },
        ),
        "extras" => SyntaxOptions::custom(extra_constructs(), extra_parse_options()),
        "wikilink-after" => {
            let mut constructs = extra_constructs();
            constructs.wikilink_title_after_pipe = true;
            SyntaxOptions::custom(constructs, extra_parse_options())
        }
        "wikilink-before" => {
            let mut constructs = extra_constructs();
            constructs.wikilink_title_before_pipe = true;
            SyntaxOptions::custom(constructs, extra_parse_options())
        }
        other => panic!("unknown derived corpus profile: {other}"),
    }
}

fn extra_constructs() -> Constructs {
    let mut constructs = Constructs::gfm();
    constructs.math_block = true;
    constructs.math_inline = true;
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
    constructs.directive_text = true;
    constructs.directive_leaf = true;
    constructs.directive_container = true;
    constructs
}

fn extra_parse_options() -> ParseOptions {
    ParseOptions {
        single_tilde_strikethrough: true,
        preserve_character_escapes: false,
        preserve_character_references: false,
    }
}

fn assert_fixture(stem: &str, options: SyntaxOptions) {
    let input = read_fixture(&format!("{stem}.md"));
    let expected_ast = read_fixture(&format!("{stem}.ast"));
    let expected_markdown =
        normalize_expected_markdown(&read_fixture(&format!("{stem}.canonical.md")));

    let output = parse_with_options(&input, &options).expect("valid syntax options");
    assert_eq!(output.diagnostics, Vec::new());
    assert_eq!(
        snapshot_document(&output.document),
        trim_final_newline(&expected_ast)
    );

    let markdown = to_markdown_with_options(&output.document, &SerializeOptions::default())
        .expect("document serializes");
    assert_eq!(markdown, expected_markdown);

    let reparsed = parse_with_options(&markdown, &options).expect("serialized markdown parses");
    assert_eq!(
        snapshot_document(&reparsed.document),
        snapshot_document(&output.document)
    );

    let second = to_markdown(&reparsed.document).expect("reparsed document serializes");
    assert_eq!(second, markdown);
}

fn assert_parse_serialize_stable(path: &str, options: &SyntaxOptions) {
    let input = read_fixture(path);
    let output = parse_with_options(&input, options).expect("valid syntax options");
    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error),
        "{path}: unexpected parse diagnostics: {:?}",
        output.diagnostics
    );

    let markdown = to_markdown(&output.document).expect("document serializes");
    let reparsed = parse_with_options(&markdown, options).expect("serialized markdown parses");
    assert_eq!(
        snapshot_document(&reparsed.document),
        snapshot_document(&output.document),
        "{path}: AST changed after serialize/reparse"
    );

    let second = to_markdown(&reparsed.document).expect("reparsed document serializes");
    assert_eq!(second, markdown, "{path}: serializer is not idempotent");
}

fn assert_case_file_stable(path: &Path, options: &SyntaxOptions) -> usize {
    let metadata = read_derived_metadata(path);
    let cases = read_derived_cases(path);
    assert_eq!(
        cases.len(),
        metadata.count,
        "{}: header count does not match parsed cases",
        path.display()
    );

    for case in &cases {
        assert_source_stable(&case.input, path, case.index, options);
    }

    cases.len()
}

fn assert_semantic_input_corpus_stable(root: &Path) -> DerivedCorpusStats {
    let mut files = Vec::new();
    collect_files(root, "cases", &mut files);
    files.sort();

    let mut stats = DerivedCorpusStats::default();
    for file in files {
        let metadata = read_derived_metadata(&file);
        // Executable cases are keyed on the `role: upstream-input` header; any
        // other (or absent) role is skipped rather than round-tripped.
        if metadata.role.as_deref() != Some("upstream-input") {
            continue;
        }
        let cases = read_derived_cases(&file);
        assert_eq!(
            cases.len(),
            metadata.count,
            "{}: header count does not match parsed cases",
            file.display()
        );

        match metadata.origin.as_str() {
            "commonmark" => stats.commonmark_cases += cases.len(),
            "gfm" => stats.gfm_cases += cases.len(),
            origin => panic!("{}: unexpected origin: {origin}", file.display()),
        }

        for case in cases {
            stats.profiles.insert(case.profile.clone());
            stats.total_cases += 1;
            let options = profile_options(&case.profile);
            assert_source_stable(&case.input, &file, case.index, &options);
        }
    }

    assert_promoted_semantic_sources(root);
    assert_semantic_manifest_matches(root, &stats);
    stats
}

#[derive(Default)]
struct DerivedCorpusStats {
    total_cases: usize,
    commonmark_cases: usize,
    gfm_cases: usize,
    profiles: BTreeSet<String>,
}

fn assert_required_profiles(profiles: &BTreeSet<String>) {
    for profile in [
        "commonmark",
        "gfm",
        "mdx",
        "math",
        "frontmatter",
        "extras",
        "wikilink-after",
        "wikilink-before",
    ] {
        assert!(
            profiles.contains(profile),
            "semantic derived corpus is missing required profile: {profile}"
        );
    }
}

fn assert_promoted_semantic_sources(root: &Path) {
    for relative in [
        "commonmark/attention.cases",
        "commonmark/gfm_strikethrough.cases",
        "commonmark/link_reference.cases",
        "commonmark/list.cases",
        "commonmark/mdx_esm.cases",
    ] {
        let path = root.join(relative);
        assert!(
            path.exists(),
            "{}: promoted semantic input corpus is missing",
            path.display()
        );
    }
}

fn assert_semantic_manifest_matches(root: &Path, stats: &DerivedCorpusStats) {
    let manifest_path = root.join("MANIFEST.md");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("{}: {error}", manifest_path.display()));
    let total = manifest
        .lines()
        .find_map(|line| line.strip_prefix("Total executable input cases: "))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_else(|| {
            panic!(
                "{}: missing Total executable input cases line",
                manifest_path.display()
            )
        });
    assert_eq!(
        total,
        stats.total_cases,
        "{}: manifest total does not match parsed semantic corpus",
        manifest_path.display()
    );

    for profile in &stats.profiles {
        assert!(
            manifest.contains(&format!("`{profile}`")),
            "{}: manifest does not mention executable profile `{profile}`",
            manifest_path.display()
        );
    }
}

struct DerivedMetadata {
    origin: String,
    role: Option<String>,
    count: usize,
}

fn read_derived_metadata(path: &Path) -> DerivedMetadata {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut origin = None;
    let mut source_seen = false;
    let mut role = None;
    let mut count = None;

    for line in source.lines() {
        if line.starts_with("--- case ") {
            break;
        }
        if let Some(value) = line.strip_prefix("origin: ") {
            origin = Some(value.to_string());
        } else if line.strip_prefix("source: ").is_some() {
            source_seen = true;
        } else if let Some(value) = line.strip_prefix("role: ") {
            role = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("count: ") {
            count = Some(value.parse::<usize>().unwrap_or_else(|error| {
                panic!(
                    "{}: invalid metadata count `{value}`: {error}",
                    path.display()
                )
            }));
        }
    }

    if !source_seen {
        panic!("{}: missing source metadata", path.display());
    }

    DerivedMetadata {
        origin: origin.unwrap_or_else(|| panic!("{}: missing origin metadata", path.display())),
        role,
        count: count.unwrap_or_else(|| panic!("{}: missing count metadata", path.display())),
    }
}

struct DerivedCase {
    index: usize,
    profile: String,
    input: String,
}

fn read_derived_cases(path: &Path) -> Vec<DerivedCase> {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut cases = Vec::new();
    let mut cursor = 0;

    while let Some(relative_header_start) = source[cursor..].find("--- case ") {
        let header_start = cursor + relative_header_start;
        let header_end = source[header_start..]
            .find('\n')
            .map(|offset| header_start + offset)
            .unwrap_or(source.len());
        let header = &source[header_start..header_end];
        let (index, profile, byte_len) = parse_case_header(path, header);
        let body_start = header_end.saturating_add(1);
        let body_end = body_start + byte_len;
        assert!(
            source.is_char_boundary(body_start) && source.is_char_boundary(body_end),
            "{}#{index}: case body is not valid UTF-8 boundary",
            path.display()
        );
        assert!(
            body_end <= source.len(),
            "{}#{index}: case body exceeds file length",
            path.display()
        );

        let end_marker = "\n--- end\n";
        assert!(
            source[body_end..].starts_with(end_marker),
            "{}#{index}: missing case end marker",
            path.display()
        );

        cases.push(DerivedCase {
            index,
            profile,
            input: source[body_start..body_end].to_string(),
        });
        cursor = body_end + end_marker.len();
    }

    cases
}

fn parse_case_header(path: &Path, header: &str) -> (usize, String, usize) {
    let parts = header.split_whitespace().collect::<Vec<_>>();
    assert!(
        (parts.len() == 5 && parts[0] == "---" && parts[1] == "case" && parts[3] == "bytes")
            || (parts.len() == 7
                && parts[0] == "---"
                && parts[1] == "case"
                && parts[3] == "profile"
                && parts[5] == "bytes"),
        "{}: invalid case header: {header}",
        path.display()
    );
    let index = parts[2].parse::<usize>().unwrap_or_else(|error| {
        panic!(
            "{}: invalid case index in {header}: {error}",
            path.display()
        )
    });
    let (profile, byte_len_part) = if parts.len() == 7 {
        (parts[4].to_string(), parts[6])
    } else {
        ("commonmark".to_string(), parts[4])
    };
    let byte_len = byte_len_part.parse::<usize>().unwrap_or_else(|error| {
        panic!(
            "{}: invalid case byte length in {header}: {error}",
            path.display()
        )
    });
    (index, profile, byte_len)
}

fn assert_source_stable(source: &str, path: &Path, index: usize, options: &SyntaxOptions) {
    let output = parse_with_options(source, options)
        .unwrap_or_else(|error| panic!("{}#{index}: {}", path.display(), error.message()));

    let markdown = to_markdown(&output.document).unwrap_or_else(|error| {
        panic!("{}#{index}: serialize failed: {:?}", path.display(), error)
    });
    let reparsed = parse_with_options(&markdown, options).unwrap_or_else(|error| {
        panic!(
            "{}#{index}: reparse failed: {}",
            path.display(),
            error.message()
        )
    });
    assert_eq!(
        snapshot_document(&reparsed.document),
        snapshot_document(&output.document),
        "{}#{index}: AST changed after serialize/reparse",
        path.display()
    );
}

fn collect_files(root: &Path, extension: &str, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap_or_else(|error| panic!("{}: {error}", root.display())) {
        let path = entry
            .unwrap_or_else(|error| panic!("{}: {error}", root.display()))
            .path();
        if path.is_dir() {
            collect_files(&path, extension, output);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            output.push(path);
        }
    }
}

fn read_fixture(path: &str) -> String {
    fs::read_to_string(Path::new(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn trim_final_newline(input: &str) -> &str {
    input.trim_end_matches('\n')
}

fn normalize_expected_markdown(input: &str) -> String {
    let mut output = input.trim_end_matches('\n').to_string();
    output.push('\n');
    output
}

fn snapshot_document(document: &Document) -> String {
    let mut lines = vec!["Document".to_string()];
    for block in &document.children {
        snapshot_block(block, 1, &mut lines);
    }
    lines.join("\n")
}

fn snapshot_block(block: &Block, indent: usize, lines: &mut Vec<String>) {
    match block {
        Block::Paragraph(node) => {
            push(lines, indent, "Paragraph");
            snapshot_inlines(&node.children, indent + 1, lines);
        }
        Block::Heading(node) => {
            push(
                lines,
                indent,
                format!(
                    "Heading depth={} kind={}",
                    node.depth,
                    match node.kind {
                        markdown_syntax::HeadingKind::Atx => "atx",
                        markdown_syntax::HeadingKind::Setext => "setext",
                    }
                ),
            );
            snapshot_inlines(&node.children, indent + 1, lines);
        }
        Block::ThematicBreak(_) => push(lines, indent, "ThematicBreak"),
        Block::BlockQuote(node) => {
            push(lines, indent, "BlockQuote");
            for child in &node.children {
                snapshot_block(child, indent + 1, lines);
            }
        }
        Block::Alert(node) => {
            push(
                lines,
                indent,
                format!(
                    "Alert kind={} title={}",
                    match node.kind {
                        markdown_syntax::AlertKind::Note => "note",
                        markdown_syntax::AlertKind::Tip => "tip",
                        markdown_syntax::AlertKind::Important => "important",
                        markdown_syntax::AlertKind::Warning => "warning",
                        markdown_syntax::AlertKind::Caution => "caution",
                    },
                    snapshot_title(&node.title)
                ),
            );
            for child in &node.children {
                snapshot_block(child, indent + 1, lines);
            }
        }
        Block::List(node) => {
            push(
                lines,
                indent,
                format!("List ordered={} tight={}", node.ordered, node.tight),
            );
            for item in &node.children {
                push(
                    lines,
                    indent + 1,
                    format!(
                        "ListItem checked={}",
                        item.checked
                            .map(|checked| checked.to_string())
                            .unwrap_or_else(|| "none".into())
                    ),
                );
                for child in &item.children {
                    snapshot_block(child, indent + 2, lines);
                }
            }
        }
        Block::DescriptionList(node) => {
            push(
                lines,
                indent,
                format!("DescriptionList tight={}", node.tight),
            );
            for item in &node.children {
                push(lines, indent + 1, "Item");
                push(lines, indent + 2, "Term");
                snapshot_inlines(&item.term, indent + 3, lines);
                for details in &item.details {
                    push(lines, indent + 2, "Details");
                    for child in &details.children {
                        snapshot_block(child, indent + 3, lines);
                    }
                }
            }
        }
        Block::CodeBlock(node) => {
            push(
                lines,
                indent,
                format!(
                    "CodeBlock kind={} info={}",
                    match node.kind {
                        markdown_syntax::CodeBlockKind::Fenced { .. } => "fenced",
                        markdown_syntax::CodeBlockKind::Indented => "indented",
                    },
                    node.info
                        .as_ref()
                        .map(|info| quote(info))
                        .unwrap_or_else(|| "none".into())
                ),
            );
            push(
                lines,
                indent + 1,
                format!("Value {}", quote_trimmed(&node.value)),
            );
        }
        Block::HtmlBlock(node) => push(
            lines,
            indent,
            format!("HtmlBlock {}", quote_trimmed(&node.value)),
        ),
        Block::Definition(node) => push(
            lines,
            indent,
            format!(
                "Definition label={} destination={} title={}",
                node.label,
                node.destination,
                snapshot_title(&node.title)
            ),
        ),
        Block::FootnoteDefinition(node) => {
            push(
                lines,
                indent,
                format!("FootnoteDefinition label={}", node.label),
            );
            for child in &node.children {
                snapshot_block(child, indent + 1, lines);
            }
        }
        Block::Table(node) => {
            push(lines, indent, "Table");
            push(
                lines,
                indent + 1,
                format!(
                    "Alignments {}",
                    node.alignments
                        .iter()
                        .map(|alignment| match alignment {
                            markdown_syntax::TableAlignment::None => "none",
                            markdown_syntax::TableAlignment::Left => "left",
                            markdown_syntax::TableAlignment::Center => "center",
                            markdown_syntax::TableAlignment::Right => "right",
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            );
            for row in &node.rows {
                push(lines, indent + 1, "Row");
                for cell in &row.cells {
                    push(lines, indent + 2, "Cell");
                    snapshot_inlines(&cell.children, indent + 3, lines);
                }
            }
        }
        Block::MathBlock(node) => push(
            lines,
            indent,
            format!("MathBlock {}", quote_trimmed(&node.value)),
        ),
        Block::Frontmatter(node) => push(
            lines,
            indent,
            format!("Frontmatter {}", quote_trimmed(&node.value)),
        ),
        Block::MdxEsm(node) => push(
            lines,
            indent,
            format!("MdxEsm {}", quote_trimmed(&node.value)),
        ),
        Block::MdxExpression(node) => push(
            lines,
            indent,
            format!("MdxExpression {}", quote_trimmed(&node.value)),
        ),
        Block::MdxJsx(node) => push(
            lines,
            indent,
            format!("MdxJsx {}", quote_trimmed(&node.value)),
        ),
        Block::LeafDirective(node) => {
            push(
                lines,
                indent,
                format!(
                    "LeafDirective name={} attrs={}",
                    node.name,
                    snapshot_attrs(&node.attributes)
                ),
            );
            if !node.label.is_empty() {
                push(lines, indent + 1, "Label");
                snapshot_inlines(&node.label, indent + 2, lines);
            }
        }
        Block::ContainerDirective(node) => {
            push(
                lines,
                indent,
                format!(
                    "ContainerDirective name={} attrs={}",
                    node.name,
                    snapshot_attrs(&node.attributes)
                ),
            );
            if !node.label.is_empty() {
                push(lines, indent + 1, "Label");
                snapshot_inlines(&node.label, indent + 2, lines);
            }
            for child in &node.children {
                snapshot_block(child, indent + 1, lines);
            }
        }
    }
}

fn snapshot_inlines(inlines: &[Inline], indent: usize, lines: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::Text(node) => push(lines, indent, format!("Text {}", quote(&node.value))),
            Inline::Escape(node) => {
                push(lines, indent, format!("Escape {}", quote_char(node.value)))
            }
            Inline::CharacterReference(node) => push(
                lines,
                indent,
                format!(
                    "CharacterReference reference={} value={}",
                    quote(&node.reference),
                    quote(&node.value)
                ),
            ),
            Inline::Emphasis(node) => {
                push(lines, indent, "Emphasis");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Strong(node) => {
                push(lines, indent, "Strong");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Underline(node) => {
                push(lines, indent, "Underline");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Delete(node) => {
                push(lines, indent, "Delete");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Insert(node) => {
                push(lines, indent, "Insert");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Mark(node) => {
                push(lines, indent, "Mark");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Subscript(node) => {
                push(lines, indent, "Subscript");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Superscript(node) => {
                push(lines, indent, "Superscript");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Spoiler(node) => {
                push(lines, indent, "Spoiler");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Shortcode(node) => {
                push(lines, indent, format!("Shortcode {}", quote(&node.name)));
            }
            Inline::Code(node) => push(
                lines,
                indent,
                format!(
                    "Code value={} raw={} fence={}",
                    quote(&node.value),
                    quote(&node.raw),
                    node.fence_length
                ),
            ),
            Inline::Link(node) => {
                push(
                    lines,
                    indent,
                    format!(
                        "Link destination={} title={}",
                        node.destination,
                        snapshot_title(&node.title)
                    ),
                );
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Image(node) => {
                push(
                    lines,
                    indent,
                    format!(
                        "Image destination={} title={}",
                        node.destination,
                        snapshot_title(&node.title)
                    ),
                );
                snapshot_inlines(&node.alt, indent + 1, lines);
            }
            Inline::LinkReference(node) => {
                push(
                    lines,
                    indent,
                    format!("LinkReference identifier={}", node.identifier),
                );
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::ImageReference(node) => {
                push(
                    lines,
                    indent,
                    format!("ImageReference identifier={}", node.identifier),
                );
                snapshot_inlines(&node.alt, indent + 1, lines);
            }
            Inline::Autolink(node) => {
                let kind = match &node.kind {
                    AutolinkKind::Angle => String::from("angle"),
                    AutolinkKind::GfmLiteral { original } => {
                        format!("gfm-literal original={}", quote(original))
                    }
                };
                push(
                    lines,
                    indent,
                    format!("Autolink {} kind={kind}", quote(&node.destination)),
                );
            }
            Inline::Html(node) => push(lines, indent, format!("HtmlInline {}", quote(&node.value))),
            Inline::SoftBreak(_) => push(lines, indent, "SoftBreak"),
            Inline::LineBreak(node) => push(
                lines,
                indent,
                format!(
                    "LineBreak kind={}",
                    match node.kind {
                        markdown_syntax::LineBreakKind::Backslash => "backslash",
                        markdown_syntax::LineBreakKind::Spaces => "spaces",
                    }
                ),
            ),
            Inline::Math(node) => push(
                lines,
                indent,
                match node.kind {
                    markdown_syntax::MathInlineKind::Dollar { dollars } => {
                        format!("Math {} dollars={}", quote(&node.value), dollars)
                    }
                    markdown_syntax::MathInlineKind::Code => {
                        format!("Math {} code", quote(&node.value))
                    }
                },
            ),
            Inline::FootnoteReference(node) => {
                push(lines, indent, format!("FootnoteReference {}", node.label))
            }
            Inline::InlineFootnote(node) => {
                push(lines, indent, "InlineFootnote");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::WikiLink(node) => push(
                lines,
                indent,
                format!(
                    "WikiLink target={} label={} order={}",
                    quote(&node.target),
                    quote(&node.label),
                    match node.label_order {
                        markdown_syntax::WikiLinkLabelOrder::AfterPipe => "after",
                        markdown_syntax::WikiLinkLabelOrder::BeforePipe => "before",
                    }
                ),
            ),
            Inline::MdxExpression(node) => push(
                lines,
                indent,
                format!("MdxExpression {}", quote(&node.value)),
            ),
            Inline::MdxJsx(node) => push(lines, indent, format!("MdxJsx {}", quote(&node.value))),
            Inline::TextDirective(node) => {
                push(
                    lines,
                    indent,
                    format!(
                        "TextDirective name={} attrs={}",
                        node.name,
                        snapshot_attrs(&node.attributes)
                    ),
                );
                if !node.label.is_empty() {
                    push(lines, indent + 1, "Label");
                    snapshot_inlines(&node.label, indent + 2, lines);
                }
            }
        }
    }
}

fn snapshot_attrs(attributes: &[markdown_syntax::DirectiveAttribute]) -> String {
    if attributes.is_empty() {
        return "none".into();
    }
    attributes
        .iter()
        .map(|attribute| match &attribute.value {
            Some(value) => format!("{}:{}", attribute.name, value),
            None => attribute.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn snapshot_title(title: &Option<String>) -> String {
    title
        .as_ref()
        .map(|title| quote(title))
        .unwrap_or_else(|| "none".into())
}

fn push(lines: &mut Vec<String>, indent: usize, text: impl Into<String>) {
    lines.push(format!("{}{}", "  ".repeat(indent), text.into()));
}

fn quote(input: &str) -> String {
    format!(
        "\"{}\"",
        input
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

fn quote_char(input: char) -> String {
    let mut value = String::new();
    value.push(input);
    quote(&value)
}

fn quote_trimmed(input: &str) -> String {
    quote(input.trim_end_matches('\n'))
}
