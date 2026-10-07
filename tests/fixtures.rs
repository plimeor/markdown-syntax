//! The fixture corpus, checked one flow at a time, each flow against one
//! oracle:
//!
//! - Markdown → AST: every `.md` under `tests/fixtures/roundtrip/` with a
//!   sibling `.ast` parses, without diagnostics, to the tree that golden
//!   holds.
//! - Markdown → AST → Markdown: every `.md` with a sibling `.canonical.md`,
//!   and every input in `CANONICAL_INPUTS`, serializes to exactly the
//!   Markdown its golden or entry holds.
//! - Source read-back, Markdown → AST → Markdown → AST: every input of one
//!   corpus reads back as the tree it parsed to and serializes to the same
//!   Markdown again, or is listed in `NOT_READING_BACK`.
//!
//! Hand-built trees are serialized in `serialize_regressions.rs`; generated
//! documents read back in `serialize_roundtrip_fuzz.rs`.

mod support;

#[path = "html_conformance/deviations.rs"]
#[allow(dead_code)]
mod deviations;

#[path = "support/normalize.rs"]
mod normalize;

use std::path::{Path, PathBuf};

use markdown_syntax::{
    parse, Block, BulletMarker, DiagnosticCode, DiagnosticSeverity, FenceMarker, LineEnding,
    LineIndex, OrderedDelimiter, SerializeOptions, Span,
};

use deviations::{excerpt, input_hash};
use support::fixtures::{
    derived_corpus_stats, files_with_extension, normalize_expected_markdown, read_derived_cases,
    read_derived_metadata, read_fixture, snapshot_document, trim_final_newline, ROUNDTRIP_ROOT,
};

/// Each golden under the corpus root whose name ends in `suffix`, with the
/// `.md` input beside it. Fails on a golden whose input is missing.
fn goldens(suffix: &str) -> Vec<(PathBuf, PathBuf)> {
    let extension = suffix.rsplit('.').next().expect("suffix has an extension");
    let pairs: Vec<_> = files_with_extension(Path::new(ROUNDTRIP_ROOT), extension)
        .into_iter()
        .filter_map(|golden| {
            let stem = golden.to_str()?.strip_suffix(suffix)?;
            let input = PathBuf::from(format!("{stem}.md"));
            assert!(
                input.exists(),
                "{}: golden without its input",
                golden.display()
            );
            Some((input, golden))
        })
        .collect();
    assert!(
        !pairs.is_empty(),
        "no {suffix} goldens under {ROUNDTRIP_ROOT}"
    );
    pairs
}

// Markdown → AST.

#[test]
fn ast_goldens_match() {
    for (input, golden) in goldens(".ast") {
        let output = parse(&read_fixture(&input));
        assert_eq!(output.diagnostics, Vec::new(), "{}", input.display());
        assert_eq!(
            snapshot_document(&output.document),
            trim_final_newline(&read_fixture(&golden)),
            "{}",
            input.display()
        );
    }
}

const HTML_SYNTAX_NODES: &str = concat!(
    "<script>\n",
    "const value = '<tag>';\n",
    "\n",
    "</script>\n",
    "\n",
    "Text <span data-x=\"1\">ok</span> and <!-- inline -->.\n"
);

#[test]
fn html_syntax_nodes_are_preserved() {
    let output = parse(HTML_SYNTAX_NODES);
    assert_eq!(output.diagnostics, Vec::new());
    assert!(matches!(
        output.document.children.first(),
        Some(Block::HtmlBlock(_))
    ));
    assert!(snapshot_document(&output.document).contains("HtmlInline \"<span data-x=\\\"1\\\">\""));
    assert!(snapshot_document(&output.document).contains("HtmlInline \"<!-- inline -->\""));
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

// Markdown → AST → Markdown.

/// The Markdown `source` parses and serializes to.
fn written(source: &str) -> String {
    parse(source)
        .document
        .to_markdown()
        .unwrap_or_else(|error| panic!("{source:?}: {error:?}"))
}

#[test]
fn canonical_goldens_match() {
    for (input, golden) in goldens(".canonical.md") {
        assert_eq!(
            written(&read_fixture(&input)),
            normalize_expected_markdown(&read_fixture(&golden)),
            "{}",
            input.display()
        );
    }
}

#[test]
fn canonical_inputs_are_written_as_listed() {
    for (source, expected) in CANONICAL_INPUTS {
        assert_eq!(written(source), *expected, "{source:?}");
    }
    // A code span is written from its value, whose line endings are spaces,
    // so its continuation lines cannot open a fence.
    assert_eq!(
        written(&format!("a `{}`", "\n    ~~~".repeat(40))),
        format!("a `{}`\n", " ~~~".repeat(40))
    );
}

#[test]
fn list_markers_preserve_by_default_and_yield_when_overridden() {
    let mut options = SerializeOptions::default();
    options.bullet = Some(BulletMarker::Plus);
    // The override yields where two adjacent lists would read as one.
    assert_eq!(
        parse("- a\n\n+ b\n\n* c\n")
            .document
            .to_markdown_with(&options)
            .expect("document serializes with options"),
        "+ a\n\n- b\n\n+ c\n"
    );

    let mut options = SerializeOptions::default();
    options.ordered_delimiter = Some(OrderedDelimiter::Period);
    assert_eq!(
        parse("1) a\n\n1. b\n")
            .document
            .to_markdown_with(&options)
            .expect("document serializes with options"),
        "1. a\n\n1) b\n"
    );
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
fn crlf_output_keeps_a_values_crlf() {
    let mut crlf = SerializeOptions::default();
    crlf.line_ending = LineEnding::CrLf;
    let document = parse("```\r\na\r\n```\r\nb").document;
    let markdown = document
        .to_markdown_with(&crlf)
        .expect("document serializes");
    assert_eq!(markdown, "```\r\na\r\n```\r\n\r\nb\r\n");
}

// Source read-back.

/// What the AST does not record (decision 0008, Consequences), and so
/// what keeps a listed input from reading back.
#[derive(Clone, Copy, Debug)]
enum Unrecorded {
    /// A line continued a paragraph lazily, without its container's prefix.
    LazyLine,
    /// How far a line is indented, or how wide the space after a container
    /// marker is.
    Indentation,
    /// How many blank lines, or empty container lines, separate two lines,
    /// down to none.
    BlankLines,
}

use Unrecorded::{BlankLines, Indentation, LazyLine};

/// One input of the read-back corpus that reads back as a different tree.
///
/// An entry names its input by content, as the conformance bench's
/// `deviations.rs` does: the file holding it, the FNV-1a hash of the input,
/// and the input's first 40 chars, which must match too.
struct NotReadingBack {
    file: &'static str,
    input_hash: u64,
    excerpt: &'static str,
    unrecorded: Unrecorded,
    reason: &'static str,
}

const fn listed(
    file: &'static str,
    input_hash: u64,
    excerpt: &'static str,
    unrecorded: Unrecorded,
    reason: &'static str,
) -> NotReadingBack {
    NotReadingBack {
        file,
        input_hash,
        excerpt,
        unrecorded,
        reason,
    }
}

impl NotReadingBack {
    fn names(&self, file: &str, input: &str) -> bool {
        self.file == file && self.input_hash == input_hash(input) && input.starts_with(self.excerpt)
    }
}

/// The inputs of the read-back corpus that read back as a different tree,
/// each with what the AST does not record that makes the difference.
///
/// `file` is a path under `tests/fixtures/roundtrip/`, or the name of the
/// list in this file that holds the input.
#[rustfmt::skip]
const NOT_READING_BACK: &[NotReadingBack] = &[
    // Lines that reach a quote's paragraph lazily. Written with the quote's
    // prefix, they open the block they could not open lazily.
    listed("extensions/gfm_table_containers.md", 0xcfbd28b509cf6ed8, "> | a |\n| - |\n\n> a\n> | b |\n|-\n\n| a |\n> |", LazyLine, "lazy delimiter rows continue the quote's paragraph, and a quoted `| - |` under a top-level row is the quote's text; written inside the quote they make tables"),
    listed("cases/commonmark/block_quote.cases", 0x940fa4c97ec4f379, "> a\n    - b", LazyLine, "the lazy `- b` continues the quote's paragraph; written inside the quote it opens a list"),
    listed("cases/commonmark/gfm_table.cases", 0x0515751d02032a6b, "> | a |\n| - |", LazyLine, "the lazy `| - |` continues the quote's paragraph; written inside the quote it makes a table"),
    listed("cases/commonmark/gfm_table.cases", 0x07b885ea9e1e56da, "> | a |\n| - |\n> | c |", LazyLine, "the lazy `| - |` continues the quote's paragraph; written inside the quote it makes a table"),
    listed("cases/commonmark/heading_setext.cases", 0x184c3d69ad4e8889, "> foo\nbar\n===", LazyLine, "the lazy `===` continues the quote's paragraph; written inside the quote it underlines a heading"),
    listed("cases/commonmark/heading_setext.cases", 0xd6c16b68b6febf6d, "> a\n===", LazyLine, "the lazy `===` continues the quote's paragraph; written inside the quote it underlines a heading"),
    // Lines whose indentation keeps them from opening a block, or sets an
    // item's content column. Written at the column the serializer picks,
    // they open that block or move in or out of the item.
    listed("extensions/gfm_table_edges.md", 0xdd4deb82c9a40ff3, "| A\\|B | Code | Link |\n| --- | --- | ---", Indentation, "`| Literal |` over a delimiter row indented four columns is a paragraph; written unindented, the row makes a table"),
    listed("cases/commonmark/gfm_table.cases", 0x7a89901856cce5d0, "   - d\n    - e", Indentation, "`- e`, indented four columns, continues the item's paragraph; written at the item's content column it opens a nested list"),
    listed("cases/commonmark/gfm_table.cases", 0xcfaf038193c872fd, "| a |\n    | - |", Indentation, "the delimiter row indented four columns continues the paragraph; written unindented it makes a table"),
    listed("cases/commonmark/gfm_table.cases", 0xb81d7773080ebe4e, "# Code\n\n## Indented delimiter row\n\na\n   ", Indentation, "delimiter and body rows indented four columns continue a paragraph or end a table; written unindented they make or extend one"),
    listed("cases/commonmark/heading_atx.cases", 0x1d35761f8902ad51, "foo\n    # bar", Indentation, "`# bar`, indented four columns, continues the paragraph; written unindented it opens a heading"),
    listed("cases/commonmark/heading_setext.cases", 0x175cadb2bf7304be, "Foo\n    =", Indentation, "`=`, indented four columns, continues the paragraph; written unindented it underlines a heading"),
    listed("cases/commonmark/heading_setext.cases", 0xc74cbb8694b5a735, "Foo\n\t=", Indentation, "`=`, indented by a tab, continues the paragraph; written unindented it underlines a heading"),
    listed("cases/commonmark/list.cases", 0xc8221aa548233394, " -    one\n\n     two", Indentation, "the spaces after the bullet set a content column that `two` falls short of, so it is indented code after the list; written with one space, the item takes it in"),
    listed("cases/commonmark/list.cases", 0x619786df51da9079, "- a\n - b\n  - c\n   - d\n    - e", Indentation, "`- e`, indented four columns, continues the last item's paragraph; written at the item's content column it opens a nested list"),
    listed("cases/commonmark/list.cases", 0x310035aa719c8c63, "1. a\n\n  2. b\n\n    3. c", Indentation, "`2.`, indented two columns, sets a content column that `3. c` falls short of, so it is indented code after the list; written unindented, the item takes it in"),
    listed("cases/commonmark/thematic_break.cases", 0xce75af4a0dc1af07, "Foo\n    ***", Indentation, "`***`, indented four columns, continues the paragraph; written unindented it is a thematic break"),
    listed("cases/gfm/fuzz.cases", 0xc3010735277f77a1, "\u{2}\n\\\n\t-", Indentation, "`-`, indented by a tab, continues the paragraph; written unindented it underlines a heading"),
    listed("cases/gfm/phoenix_heex.cases", 0x05f78ce45050122f, "<%= foo\n    # |> bar()\n    |> baz() %>\n", Indentation, "`# |> bar()`, indented four columns, continues the paragraph; written unindented it opens a heading"),
    listed("READ_BACK_INPUTS", 0x07a02e5549946319, "~ \n\n>  > \t<!--[x] 2) *   :::e", Indentation, "the tab after the nested quote marker keeps `<!--` from opening an HTML block"),
    listed("READ_BACK_INPUTS", 0xe1010ac996a33d36, ">* \t[x] : # <div>a\n> |-|", Indentation, "the tab between the bullet and `[x]` keeps the item from being a task"),
    listed("READ_BACK_INPUTS", 0x9bf7e5b37f795c2e, "-$$\n    $$", Indentation, "the indentation of the continuation line keeps `$$` from closing math"),
    listed("READ_BACK_INPUTS", 0x60c48ac0b10b1e7f, "-\t(\n  <v>", Indentation, "the tab after the bullet sets the item's content column"),
    listed("READ_BACK_INPUTS", 0x22cd1088c282d619, "*\t<a>\n  <v>", Indentation, "the tab after the bullet sets the item's content column"),
    listed("READ_BACK_INPUTS", 0xffd87a886b214767, "[\n~ _\n    ~~~", Indentation, "the indentation of the continuation line keeps `~~~` from opening a fence"),
    listed("READ_BACK_INPUTS", 0x95d86f92a1781073, "- *  (\n    <a>", Indentation, "the spaces after the nested bullet set the item's content column"),
    listed("READ_BACK_INPUTS", 0x18aded067873d2f6, "(\n    <div>", Indentation, "the indentation of the continuation line keeps `<div>` from opening an HTML block"),
    // Blank lines, or their absence. Written with the serializer's own, a
    // line opens a block or leaves a container it did not.
    listed("CANONICAL_INPUTS", 0x4c51b6c9c2eaf71f, "[o]:u\n\t$$\na$$", BlankLines, "the paragraph continues the definition's lines; written after a blank line, its `$$` opens a math block"),
    listed("READ_BACK_INPUTS", 0xb753e4882c87cc65, "[o]:u\n\t<div>", BlankLines, "the paragraph continues the definition's lines; written after a blank line, its `<div>` opens an HTML block"),
    listed("READ_BACK_INPUTS", 0x250c10eb0427a2eb, "[o]:u\n<a>\n-", BlankLines, "the paragraph continues the definition's lines; written after a blank line, its `<a>` opens an HTML block"),
    listed("READ_BACK_INPUTS", 0x34a71648eeea0c0e, "[o]: u\n<a>", BlankLines, "the paragraph continues the definition's lines; written after a blank line, its `<a>` opens an HTML block"),
    listed("READ_BACK_INPUTS", 0xc8b86f03918aab8a, "- > a\n  >\n  b\n  ---", BlankLines, "the quote's empty last line keeps `b` from continuing its paragraph"),
    listed("READ_BACK_INPUTS", 0xa0ba0dbfcaf02234, "- > a\n  >\n  | b |\n  | - |", BlankLines, "the quote's empty last line keeps `| b |` from continuing its paragraph"),
    listed("READ_BACK_INPUTS", 0x05e9c61b80925f3c, ">\n>[!NOTE]:>", BlankLines, "the quote's empty first line keeps `[!NOTE]` from opening an alert"),
    listed("READ_BACK_INPUTS", 0xe538aaa0c354f66d, "- a\n\n  <!--\n- b", BlankLines, "the blank line written between the items is taken by the HTML block that runs to its item's end"),
];

/// One input of the read-back corpus.
struct Source {
    file: String,
    case: Option<usize>,
    input: String,
}

impl Source {
    fn at(&self) -> String {
        match self.case {
            Some(case) => format!("{}#{case}", self.file),
            None => self.file.clone(),
        }
    }
}

/// The read-back corpus: every input `.md` and every `.cases` file under the
/// corpus root, and the inputs listed in this file.
fn read_back_corpus() -> Vec<Source> {
    let root = Path::new(ROUNDTRIP_ROOT);
    let relative = |path: &Path| {
        path.strip_prefix(root)
            .expect("corpus file is under the root")
            .to_string_lossy()
            .replace('\\', "/")
    };
    let mut sources = Vec::new();
    for path in files_with_extension(root, "md") {
        let file = relative(&path);
        let name = path.file_name().and_then(|name| name.to_str());
        if file.ends_with(".canonical.md")
            || matches!(name, Some("MANIFEST.md" | "NOTICE.md" | "README.md"))
        {
            continue;
        }
        sources.push(Source {
            file,
            case: None,
            input: read_fixture(&path),
        });
    }
    for path in files_with_extension(root, "cases") {
        let metadata = read_derived_metadata(&path);
        let cases = read_derived_cases(&path);
        assert_eq!(
            cases.len(),
            metadata.count,
            "{}: header count does not match parsed cases",
            path.display()
        );
        let file = relative(&path);
        sources.extend(cases.into_iter().map(|case| Source {
            file: file.clone(),
            case: Some(case.index),
            input: case.input,
        }));
    }
    let listed = CANONICAL_INPUTS
        .iter()
        .map(|(source, _)| ("CANONICAL_INPUTS", source.to_string()))
        .chain(
            READ_BACK_INPUTS
                .iter()
                .map(|source| ("READ_BACK_INPUTS", source.to_string())),
        )
        .chain(
            generated_read_back_inputs()
                .into_iter()
                .map(|source| ("READ_BACK_INPUTS", source)),
        );
    sources.extend(listed.map(|(file, input)| Source {
        file: file.into(),
        case: None,
        input,
    }));
    sources
}

/// Whether `source` reads back: its Markdown parses as the same tree, as
/// serialization's tree comparison reads it, and serializes to the same
/// Markdown again. Panics if it does not serialize.
fn reads_back(source: &Source) -> Result<(), String> {
    let document = parse(&source.input).document;
    let markdown = document
        .to_markdown()
        .unwrap_or_else(|error| panic!("{}: serialize failed: {error:?}", source.at()));
    let reparsed = parse(&markdown).document;
    if normalize::normalized(&reparsed.children) != normalize::normalized(&document.children) {
        return Err(format!("{markdown:?} reads back as a different tree"));
    }
    match reparsed.to_markdown() {
        Ok(again) if again == markdown => Ok(()),
        other => Err(format!("{markdown:?} serializes again as {other:?}")),
    }
}

#[test]
fn sources_read_back() {
    let mut failures = Vec::new();
    let mut seen = vec![false; NOT_READING_BACK.len()];
    for source in read_back_corpus() {
        let listed = NOT_READING_BACK
            .iter()
            .position(|entry| entry.names(&source.file, &source.input));
        if let Some(index) = listed {
            seen[index] = true;
        }
        match (reads_back(&source), listed) {
            (Ok(()), None) | (Err(_), Some(_)) => {}
            (Err(failure), None) => failures.push(format!(
                "{}: {:?} does not read back: {failure}\n  listed({:?}, {:#018x}, {:?}, Unrecorded::_, \"\"),",
                source.at(),
                source.input,
                source.file,
                input_hash(&source.input),
                excerpt(&source.input),
            )),
            (Ok(()), Some(index)) => {
                let entry = &NOT_READING_BACK[index];
                failures.push(format!(
                    "{}: listed ({:?}: {}), but reads back",
                    source.at(),
                    entry.unrecorded,
                    entry.reason
                ));
            }
        }
    }
    for (entry, seen) in NOT_READING_BACK.iter().zip(seen) {
        if !seen {
            failures.push(format!(
                "{} {:?}: listed ({:?}: {}), but names no input",
                entry.file, entry.excerpt, entry.unrecorded, entry.reason
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn derived_corpus_matches_its_manifest() {
    let stats = derived_corpus_stats(&Path::new(ROUNDTRIP_ROOT).join("cases"));
    assert!(
        stats.commonmark_cases > 1_400,
        "expected substantial CommonMark-dialect semantic input corpus, got {}",
        stats.commonmark_cases
    );
    assert!(
        stats.gfm_cases > 500,
        "expected substantial GFM-dialect semantic input corpus, got {}",
        stats.gfm_cases
    );
}

#[test]
fn commonmark_example_inputs_are_all_read() {
    let cases = read_derived_cases(Path::new(
        "tests/fixtures/roundtrip/examples/official-stable-inputs.cases",
    ));
    assert_eq!(
        cases.len(),
        8,
        "CommonMark selected input stability corpus drifted"
    );
}

// The inputs listed in code.

/// Parsed inputs, from earlier serializer defects and the serializer's
/// spelling rules, with the Markdown each is written as. Each is also an
/// input of the read-back corpus.
const CANONICAL_INPUTS: &[(&str, &str)] = &[
    // Recorded spellings.
    ("# Title\n\nHello *world*.", "# Title\n\nHello *world*.\n"),
    ("+ a", "+ a\n"),
    ("- a\n\n+ b\n\n* c\n", "- a\n\n+ b\n\n* c\n"),
    ("_a_ __b__", "_a_ __b__\n"),
    ("__*a*__", "__*a*__\n"),
    ("__#$***~**b~**__|#", "__#$***~**b~**__|#\n"),
    ("foo-_(bar)_", "foo-_(bar)_\n"),
    ("y***b***", "y***b***\n"),
    ("***y*b", "***y*b\n"),
    ("a**~**", "a**~**\n"),
    ("b*~*", "b*~*\n"),
    ("see http://a.b", "see http://a.b\n"),
    ("www.a.b", "www.a.b\n"),
    ("a@b.c", "a@b.c\n"),
    ("<a@b.c> <http://a.b>", "<a@b.c> <http://a.b>\n"),
    ("[http://a.b](http://a.b)", "[http://a.b](http://a.b)\n"),
    ("[x](<http://a.b>)", "[x](<http://a.b>)\n"),
    // Other schemes are no literal autolink.
    ("a://x", "a://x\n"),
    (
        "[f&#246;o]\n\n[f&#246;o]: /url\n",
        "[f&#246;o]\n\n[f&#246;o]: /url\n",
    ),
    ("[text][Ref]\n\n[ref]: /url\n", "[text][Ref]\n\n[ref]: /url\n"),
    (
        "Use [text][Foo\\]] and [t][A &amp; B].\n\n[Foo\\]]: /a\n\n[A &amp; B]: /b\n",
        "Use [text][Foo\\]] and [t][A &amp; B].\n\n[Foo\\]]: /a\n\n[A &amp; B]: /b\n",
    ),
    (
        concat!(
            "[a](/u \"\")\n\n",
            "[b](/u '')\n\n",
            "[c](/u ())\n\n",
            "[](<> \"\")\n\n",
            "[d]: /u \"\"\n",
        ),
        concat!(
            "[a](/u \"\")\n\n",
            "[b](/u '')\n\n",
            "[c](/u ())\n\n",
            "[](<> \"\")\n\n",
            "[d]: /u \"\"\n",
        ),
    ),
    (
        "See [^a\\]b\\\\c\\[d] and [^white&#x20;space]\n\n[^a\\]b\\\\c\\[d]: bracket\n\n[^white&#x20;space]: space\n",
        "See [^a\\]b\\\\c\\[d] and [^white&#x20;space]\n\n[^a\\]b\\\\c\\[d]: bracket\n\n[^white&#x20;space]: space\n",
    ),
    ("see ![[x.png]]", "see ![[x.png]]\n"),
    // Text, escapes, and references as written.
    ("a\\.b \\#tag", "a\\.b \\#tag\n"),
    ("&#35;tag &amp; x", "&#35;tag &amp; x\n"),
    (
        "Test \\`hello world` here.",
        "Test \\`hello world` here.\n",
    ),
    ("x_y_ a*b x^2 ~5", "x_y_ a*b x^2 ~5\n"),
    (
        "This ~text~~~~ is ~~~~curious~.\n",
        "This ~text~~~~ is ~~~~curious~.\n",
    ),
    ("a ~~two/one~ b\n", "a ~~two/one~ b\n"),
    ("[foo]\\(a)\n\n[foo]: /u", "[foo]\\(a)\n\n[foo]: /u\n"),
    ("![foo]\\(a)\n\n[foo]: /u", "![foo]\\(a)\n\n[foo]: /u\n"),
    ("[foo]\\: /x\n\n[foo]: /u", "[foo]\\: /x\n\n[foo]: /u\n"),
    ("&#x20;\na", "&#x20;\na\n"),
    ("&#x20; \na", "&#x20;\na\n"),
    ("a\n&#x20;\nb", "a\n&#x20;\nb\n"),
    ("<a>&#x20;\n;", "<a>&#x20;\n;\n"),
    ("[\nfoo](u)", "[\nfoo](u)\n"),
    ("`x`<div", "`x`<div\n"),
    ("a *b*::c", "a *b*::c\n"),
    ("++a:++ b:", "++a:++ b:\n"),
    ("[^`]``", "[^`]``\n"),
    ("![[$[]]a$>", "![[$[]]a$>\n"),
    ("==a\\== b==", "==a\\== b==\n"),
    ("++a\\++ b++", "++a\\++ b++\n"),
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
    // Blocks and their lines.
    ("a |\n-", "a |\n---\n"),
    ("a\nb\n===", "a b\n===\n"),
    (
        "Foo *bar\nbaz*\n====",
        "Foo *bar baz*\n=============\n",
    ),
    ("- a\n  - ---", "- a\n  - - -\n"),
    // Later item numbers stop at the largest a marker can hold.
    ("999999999. a\n1. b", "999999999. a\n999999999. b\n"),
    ("999999998) a\n1) b\n1) c", "999999998) a\n999999999) b\n999999999) c\n"),
    ("-\n   <v>", "-\n   <v>\n"),
    ("-\n  ---", "-\n  ---\n"),
    ("-\n  ---\n-\n  ---", "-\n  ---\n-\n  ---\n"),
    ("- a\n\n-\n  ---", "- a\n\n-\n  ---\n"),
    ("> - a\n> -\n>   ---", "> - a\n> -\n>   ---\n"),
    ("- - a\n  -\n    ---", "- - a\n  -\n    ---\n"),
    ("[o]:u\n\t$$\na$$", "[o]: u\n\n$$\na$$\n"),
    ("- a\n  - b\n   <div>", "- a\n  - b\n   <div>\n"),
    // Values encoded by rule.
    ("```&#x20;a&#9;\nb\n```", "``` &#x20;a&#x9;\nb\n```\n"),
    ("```\n```", "```\n```\n"),
    ("```\n```*", "```\n```*\n```\n"),
    ("````\n```\n````", "````\n```\n````\n"),
    (" ~~~\n    ~~~", " ~~~\n    ~~~\n ~~~\n"),
    ("\ta\r\tb", "    a\r    b\r"),
    ("[o]:&#x20;", "[o]: &#x20;\n"),
    ("<!--\n\n", "<!--\n\n"),
    ("<div>\n  a  \n</div>", "<div>\n  a  \n</div>\n"),
    // Code spans written from their value.
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
];

/// Generated inputs of the read-back corpus, too long to list.
fn generated_read_back_inputs() -> Vec<String> {
    vec![
        format!("a `{}`", "\n    ~~~".repeat(40)),
        "-\n  ---\n\nx\n\n".repeat(40),
        "-\n  ---\n".repeat(40),
    ]
}

/// Parsed inputs of earlier serializer defects, checked by read-back only.
const READ_BACK_INPUTS: &[&str] = &[
    HTML_SYNTAX_NODES,
    "=```\n    ```",
    "(\n    <div>",
    "- a\n\n  <!--\n- b",
    "    a\r\n    b\r\n\r\nc",
    "    a\r    b\r\rc",
    "| Result |\n| --- |\n| ||visible|| |\n",
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
];
