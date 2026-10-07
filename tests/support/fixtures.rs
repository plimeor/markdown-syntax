use std::{
    fs,
    path::{Path, PathBuf},
};

use markdown_syntax::{Block, Document, Inline};

/// The fixture corpus: goldens, stability inputs, and `.cases` files.
pub(crate) const ROUNDTRIP_ROOT: &str = "tests/fixtures/roundtrip";

/// Every file under `root` with `extension`, sorted.
pub(crate) fn files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files(root, extension, &mut files);
    files.sort();
    files
}

/// Checks the derived corpus under `root`, its metadata, its promoted
/// sources, and its manifest total, and counts its cases.
pub(crate) fn derived_corpus_stats(root: &Path) -> DerivedCorpusStats {
    let mut stats = DerivedCorpusStats::default();
    for file in files_with_extension(root, "cases") {
        let metadata = read_derived_metadata(&file);
        assert_eq!(
            metadata.role.as_deref(),
            Some("upstream-input"),
            "{}: executable corpus files must declare role: upstream-input",
            file.display()
        );
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
        stats.total_cases += cases.len();
    }

    assert_promoted_semantic_sources(root);
    assert_semantic_manifest_matches(root, &stats);
    stats
}

#[derive(Default)]
pub(crate) struct DerivedCorpusStats {
    pub(crate) total_cases: usize,
    pub(crate) commonmark_cases: usize,
    pub(crate) gfm_cases: usize,
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
}

pub(crate) struct DerivedMetadata {
    pub(crate) origin: String,
    pub(crate) role: Option<String>,
    pub(crate) count: usize,
}

pub(crate) fn read_derived_metadata(path: &Path) -> DerivedMetadata {
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

/// One case of a derived `.cases` file: its number and its input.
pub(crate) struct DerivedCase {
    pub(crate) index: usize,
    pub(crate) input: String,
}

pub(crate) fn read_derived_cases(path: &Path) -> Vec<DerivedCase> {
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
        let (index, byte_len) = parse_case_header(path, header);
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
            input: source[body_start..body_end].to_string(),
        });
        cursor = body_end + end_marker.len();
    }

    cases
}

fn parse_case_header(path: &Path, header: &str) -> (usize, usize) {
    let parts = header.split_whitespace().collect::<Vec<_>>();
    assert!(
        parts.len() == 5 && parts[0] == "---" && parts[1] == "case" && parts[3] == "bytes",
        "{}: invalid case header: {header}",
        path.display()
    );
    let index = parts[2].parse::<usize>().unwrap_or_else(|error| {
        panic!(
            "{}: invalid case index in {header}: {error}",
            path.display()
        )
    });
    let byte_len = parts[4].parse::<usize>().unwrap_or_else(|error| {
        panic!(
            "{}: invalid case byte length in {header}: {error}",
            path.display()
        )
    });
    (index, byte_len)
}

pub(crate) fn collect_files(root: &Path, extension: &str, output: &mut Vec<PathBuf>) {
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

pub(crate) fn read_fixture(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

pub(crate) fn trim_final_newline(input: &str) -> &str {
    input.trim_end_matches('\n')
}

pub(crate) fn normalize_expected_markdown(input: &str) -> String {
    let mut output = input.trim_end_matches('\n').to_string();
    output.push('\n');
    output
}

pub(crate) fn snapshot_document(document: &Document) -> String {
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
        Block::HtmlContainer(node) => {
            push(
                lines,
                indent,
                format!(
                    "HtmlContainer tag={} open={} close={}",
                    node.opening.name,
                    quote(&node.opening.raw),
                    quote(&node.closing.raw)
                ),
            );
            match &node.content {
                markdown_syntax::HtmlContainerContent::Blocks(children) => {
                    for child in children {
                        snapshot_block(child, indent + 1, lines);
                    }
                }
                markdown_syntax::HtmlContainerContent::Inlines(children) => {
                    snapshot_inlines(children, indent + 1, lines);
                }
            }
        }
        Block::Definition(node) => push(
            lines,
            indent,
            format!(
                "Definition label={} destination={} title={}",
                node.label,
                node.destination,
                snapshot_link_title(node.title.as_ref())
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

fn snapshot_delimiter(delimiter: markdown_syntax::EmphasisDelimiter) -> &'static str {
    match delimiter {
        markdown_syntax::EmphasisDelimiter::Asterisk => "*",
        markdown_syntax::EmphasisDelimiter::Underscore => "_",
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
                    quote(&node.value().unwrap_or_default())
                ),
            ),
            Inline::Emphasis(node) => {
                push(
                    lines,
                    indent,
                    format!("Emphasis delimiter={}", snapshot_delimiter(node.delimiter)),
                );
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Strong(node) => {
                push(
                    lines,
                    indent,
                    format!("Strong delimiter={}", snapshot_delimiter(node.delimiter)),
                );
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Delete(node) => {
                push(lines, indent, "Delete");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Mark(node) => {
                push(lines, indent, "Mark");
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Shortcode(node) => {
                push(lines, indent, format!("Shortcode {}", quote(&node.name)));
            }
            Inline::Code(node) => push(lines, indent, format!("Code value={}", quote(&node.value))),
            Inline::Link(node) => {
                push(
                    lines,
                    indent,
                    format!(
                        "Link destination={} title={}",
                        node.destination,
                        snapshot_link_title(node.title.as_ref())
                    ),
                );
                snapshot_inlines(&node.children, indent + 1, lines);
            }
            Inline::Autolink(node) => push(
                lines,
                indent,
                format!(
                    "Autolink form={} text={} destination={}",
                    match node.form {
                        markdown_syntax::AutolinkForm::Angle => "angle",
                        markdown_syntax::AutolinkForm::Literal => "literal",
                    },
                    quote(&node.text),
                    node.destination().unwrap_or_default()
                ),
            ),
            Inline::Image(node) => {
                push(
                    lines,
                    indent,
                    format!(
                        "Image destination={} title={}",
                        node.destination,
                        snapshot_link_title(node.title.as_ref())
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
                    "WikiLink target={} label={}{}",
                    quote(&node.target),
                    quote(&node.label),
                    if node.embed { " embed" } else { "" }
                ),
            ),
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

/// A link title's value; the quotes it is written in are not snapshotted.
fn snapshot_link_title(title: Option<&markdown_syntax::Title>) -> String {
    title.map_or_else(|| "none".into(), |title| quote(&title.value))
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
