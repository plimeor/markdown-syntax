//! AST to canonical Markdown. The verbs live on [`Document`]
//! ([`to_markdown`](Document::to_markdown) /
//! [`to_markdown_with`](Document::to_markdown_with)); [`SerializeOptions`] tunes
//! the output style. The document is validated first, so serialization can fail
//! with a [`SerializeError`].

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::ops::Deref;

use crate::{
    ast::*,
    compare::layout_normalized_blocks,
    diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity},
    options::SyntaxOptions,
    parse::parse_with_definitions,
    validate::validate_document,
};

mod inline;
mod layout;
mod read_back;

use layout::{Alternative, Layout, Node};
use read_back::{write_reading_back, Extracted, Part, ReadBack};

/// The newline style emitted by the serializer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    /// Unix `\n`.
    Lf,
    /// Windows `\r\n`.
    CrLf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

/// Output-style options for serialization. Defaults: LF, trailing newline, `-`
/// bullets, `.` ordered markers, and backtick code fences.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SerializeOptions {
    /// Newline style to emit.
    pub line_ending: LineEnding,
    /// Whether to end the output with a trailing newline.
    pub final_newline: bool,
    /// The bullet marker for unordered lists.
    pub bullet: ListDelimiter,
    /// The delimiter for ordered-list markers (e.g. `.` → `1.`).
    pub ordered_delimiter: ListDelimiter,
    /// The fence character for fenced code blocks.
    pub fence_marker: FenceMarker,
    /// The dialect the output is read back under: escapes and delimiter
    /// choices are made so that parsing the output with these options yields
    /// the serialized tree. Defaults to the maximal dialect,
    /// [`SyntaxOptions::default`].
    pub syntax: SyntaxOptions,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            line_ending: LineEnding::Lf,
            final_newline: true,
            bullet: ListDelimiter::Dash,
            ordered_delimiter: ListDelimiter::Period,
            fence_marker: FenceMarker::Backtick,
            syntax: SyntaxOptions::default(),
        }
    }
}

/// Why serialization failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SerializeError {
    /// The AST failed validation; carries the validation diagnostics.
    InvalidDocument(Vec<Diagnostic>),
    /// A node kind that the serializer does not support was encountered.
    UnsupportedNode(&'static str),
    /// No Markdown the serializer can write reads back, under
    /// [`SerializeOptions::syntax`], as the same tree; the diagnostic names
    /// the first node that reads back differently.
    Unrepresentable(Diagnostic),
}

/// The options a document is serialized with, and what its output is read
/// back under.
struct Cx<'o> {
    options: &'o SerializeOptions,
    read_back: ReadBack<'o>,
    /// The layout alternatives the document's read-back called for.
    layout: Layout,
}

impl Deref for Cx<'_> {
    type Target = SerializeOptions;

    fn deref(&self) -> &SerializeOptions {
        self.options
    }
}

impl Document {
    /// Serialize this document to canonical Markdown with default options.
    pub fn to_markdown(&self) -> Result<String, SerializeError> {
        self.to_markdown_with(&SerializeOptions::default())
    }

    /// Serialize this document to canonical Markdown with explicit options.
    pub fn to_markdown_with(&self, options: &SerializeOptions) -> Result<String, SerializeError> {
        let diagnostics = validate_document(self);
        if !diagnostics.is_empty() {
            return Err(SerializeError::InvalidDocument(diagnostics));
        }

        serialize_document_body(self, options)
    }
}

fn serialize_document_body(
    document: &Document,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    // The output is read as if each label the document defines or uses were
    // defined, as the document resolves its references.
    let mut known = Vec::new();
    for block in &document.children {
        known_labels_in_block(block, &mut known);
    }
    known.sort_unstable();
    known.dedup();
    let mut cx = Cx {
        options,
        read_back: ReadBack {
            syntax: &options.syntax,
            known,
        },
        layout: Layout::default(),
    };
    // The document is written with the default layout and read back; where
    // a written block reads back as another one, a layout alternative applies
    // there and the document is written again.
    let ours = layout_normalized_blocks(&document.children);
    let mut tried: Vec<layout::Choice> = Vec::new();
    let mut last: Option<(layout::Choice, Option<usize>)> = None;
    let mut output = loop {
        let output = serialize_blocks_at_start(&document.children, &cx, true)?;
        // Read as written with its final line ending.
        let mut written = output.clone();
        if !written.is_empty() && !ends_with_carriage_return_ending(&written) {
            written.push('\n');
        }
        let reparsed = parse_with_definitions(&written, &options.syntax, &cx.read_back.known)
            .document
            .children;
        let theirs = layout_normalized_blocks(&reparsed);
        if ours == theirs {
            break output;
        }
        let path = layout::divergence(&document.children, &ours, &theirs);
        let key = layout::key(&path);
        // An alternative that left the difference where it was is withdrawn.
        if let Some((choice, before)) = last.take() {
            if before == key {
                cx.layout.remove(&choice);
            }
        }
        let next = layout::candidates(&path)
            .into_iter()
            .find(|choice| !tried.contains(choice) && !cx.layout.holds(choice));
        match next {
            Some(choice) if tried.len() < LAYOUT_ROUNDS => {
                cx.layout.add(choice);
                tried.push(choice);
                last = Some((choice, key));
            }
            _ => return Err(unrepresentable_block(layout::blamed(&path))),
        }
    };
    if options.line_ending == LineEnding::CrLf {
        output = lf_to_crlf(&output);
    }
    // Blocks end without a line ending, except an HTML block whose last line
    // is empty (the final newline ends that line too) and an indented code
    // block that keeps its value's `\r` or `\r\n` ending.
    if options.final_newline && !output.is_empty() && !ends_with_carriage_return_ending(&output) {
        output.push_str(options.line_ending.as_str());
    }
    Ok(output)
}

/// Layout alternatives a document's read-back tries at most.
const LAYOUT_ROUNDS: usize = 32;

fn unrepresentable_block(node: Option<Node<'_>>) -> SerializeError {
    let (span, name) = match node {
        Some(Node::Block(block)) => (block.span(), block_kind(block)),
        Some(Node::Item(item)) => (item.meta.span, "a ListItem"),
        None => (None, "the document"),
    };
    SerializeError::Unrepresentable(Diagnostic {
        severity: DiagnosticSeverity::Error,
        code: DiagnosticCode::Unrepresentable,
        span,
        message: format!("{name} has no Markdown that reads back as the same tree"),
    })
}

fn block_kind(block: &Block) -> &'static str {
    match block {
        Block::Paragraph(_) => "a Paragraph",
        Block::Heading(_) => "a Heading",
        Block::ThematicBreak(_) => "a ThematicBreak",
        Block::BlockQuote(_) => "a BlockQuote",
        Block::Alert(_) => "an Alert",
        Block::List(_) => "a List",
        Block::DescriptionList(_) => "a DescriptionList",
        Block::CodeBlock(_) => "a CodeBlock",
        Block::HtmlBlock(_) => "an HtmlBlock",
        Block::HtmlContainer(_) => "an HtmlContainer",
        Block::Definition(_) => "a Definition",
        Block::FootnoteDefinition(_) => "a FootnoteDefinition",
        Block::Table(_) => "a Table",
        Block::MathBlock(_) => "a MathBlock",
        Block::Frontmatter(_) => "a Frontmatter",
        Block::MdxEsm(_) => "an MdxEsm",
        Block::MdxExpression(_) => "an MdxExpression",
        Block::MdxJsx(_) => "an MdxJsx",
        Block::LeafDirective(_) => "a LeafDirective",
        Block::ContainerDirective(_) => "a ContainerDirective",
    }
}

/// Rewrites each bare `\n` as `\r\n`; existing `\r\n` and `\r` endings that
/// verbatim values carry stay as they are.
fn lf_to_crlf(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut previous = '\0';
    for ch in input.chars() {
        if ch == '\n' && previous != '\r' {
            output.push('\r');
        }
        output.push(ch);
        previous = ch;
    }
    output
}

fn ends_with_carriage_return_ending(input: &str) -> bool {
    input.ends_with('\r') || input.ends_with("\r\n")
}

/// Separates two blocks with one blank line. A block that already ends its
/// last line with `\r` or `\r\n` gets only the blank line, written so that it
/// cannot merge into that ending.
fn push_block_gap(output: &mut String) {
    if output.ends_with('\r') {
        output.push('\r');
    } else if output.ends_with("\r\n") {
        output.push('\n');
    } else {
        output.push_str("\n\n");
    }
}

/// Serialize a block sequence. `document_start` is true only for the top-level
/// document body, where the first block sits at byte 0 and a contiguous `---`
/// would open frontmatter; that one position emits a spaced dash thematic break
/// instead. Nested sequences (blockquotes, list items, ...) are never at byte 0.
fn serialize_blocks_at_start(
    blocks: &[Block],
    options: &Cx<'_>,
    document_start: bool,
) -> Result<String, SerializeError> {
    // Written last to first: a list reads the indentation of the block after
    // it, which would join its last item unless the items' content starts
    // further in.
    let mut outputs: Vec<(String, bool)> = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate().rev() {
        let at_document_start = document_start && index == 0;
        let next = outputs
            .last()
            .map(|(next, _): &(String, bool)| next.as_str());
        let written = match block {
            Block::List(list) => (serialize_list_before(block, list, options, next)?, false),
            // A paragraph or setext heading right after a definition may read
            // back only as the continuation of the paragraph the definition
            // was read from.
            Block::Paragraph(paragraph) => {
                serialize_paragraph(paragraph, options, definition_before(blocks, index))?
            }
            Block::Heading(heading) if writes_setext(heading) => {
                serialize_setext_heading(heading, options, definition_before(blocks, index))?
            }
            _ => (serialize_block(block, options, at_document_start)?, false),
        };
        outputs.push(written);
    }
    let mut output = String::new();
    for (index, (written, continues)) in outputs.iter().rev().enumerate() {
        if index > 0 {
            if *continues {
                output.push('\n');
            } else {
                push_block_gap(&mut output);
            }
        }
        output.push_str(written);
    }
    Ok(output)
}

fn definition_before(blocks: &[Block], index: usize) -> Option<&Definition> {
    match index.checked_sub(1).map(|before| &blocks[before]) {
        Some(Block::Definition(definition)) => Some(definition),
        _ => None,
    }
}

/// A paragraph's Markdown, and whether it is written right after the
/// definition before it, as the continuation of the paragraph that
/// definition was read from.
fn serialize_paragraph(
    node: &Paragraph,
    cx: &Cx<'_>,
    after: Option<&Definition>,
) -> Result<(String, bool), SerializeError> {
    let parts = [Part::Inlines(&node.children, Place::Block)];
    let error = match write_reading_back(
        &parts,
        |blocks| one_block(blocks, paragraph_content),
        &cx.read_back,
    ) {
        Ok(mut written) => return Ok((written.remove(0), false)),
        Err(error) => error,
    };
    let Some(definition) = after else {
        return Err(error);
    };
    let parts = [
        Part::Literal(serialize_definition(definition) + "\n"),
        Part::Inlines(&node.children, Place::Continuation),
    ];
    let extract = extractor(|blocks| match blocks {
        [Block::Definition(_), rest @ ..] => one_block(rest, paragraph_content),
        _ => Extracted::Mismatch,
    });
    match write_reading_back(&parts, extract, &cx.read_back) {
        Ok(mut written) => Ok((written.remove(0), true)),
        Err(_) => Err(error),
    }
}

/// `content`, typed as reading inline content from any block it is given.
fn content_of<F>(content: F) -> F
where
    F: for<'d> Fn(&'d Block) -> Option<&'d [Inline]>,
{
    content
}

/// `extract`, typed as reading any blocks it is given.
fn extractor<F>(extract: F) -> F
where
    F: for<'d> Fn(&'d [Block]) -> Extracted<'d>,
{
    extract
}

fn paragraph_content(block: &Block) -> Option<&[Inline]> {
    match block {
        Block::Paragraph(node) => Some(&node.children),
        _ => None,
    }
}

/// The inline content `content` reads from the one block `blocks` should
/// hold, or where a line landed outside it.
fn one_block<'d>(
    blocks: &'d [Block],
    content: impl for<'b> Fn(&'b Block) -> Option<&'b [Inline]>,
) -> Extracted<'d> {
    match blocks {
        [only] => match content(only) {
            Some(inlines) => Extracted::Lists(vec![inlines]),
            None => misplaced_within(only),
        },
        [first, second, ..] => match (content(first), second.span()) {
            (Some(_), Some(span)) => Extracted::Misplaced(span.start),
            _ => match first.span() {
                Some(span) => Extracted::Misplaced(span.start),
                None => Extracted::Mismatch,
            },
        },
        [] => Extracted::Mismatch,
    }
}

/// Where a line landed in `block`, read where one block of another kind was
/// written: the first block it holds that starts after it does, a setext
/// heading's underline, or else its first line.
fn misplaced_within(block: &Block) -> Extracted<'_> {
    let Some(span) = block.span() else {
        return Extracted::Mismatch;
    };
    let mut inner = None;
    let mut visit = |child: &Block| {
        if let Some(child) = child.span().filter(|child| child.start > span.start) {
            inner = Some(inner.map_or(child.start, |start: usize| start.min(child.start)));
        }
    };
    match block {
        Block::BlockQuote(node) => node.children.iter().for_each(&mut visit),
        Block::Alert(node) => node.children.iter().for_each(&mut visit),
        Block::List(node) => node
            .children
            .iter()
            .flat_map(|item| &item.children)
            .for_each(&mut visit),
        Block::DescriptionList(node) => node
            .children
            .iter()
            .flat_map(|item| &item.details)
            .flat_map(|details| &details.children)
            .for_each(&mut visit),
        Block::FootnoteDefinition(node) => node.children.iter().for_each(&mut visit),
        Block::ContainerDirective(node) => node.children.iter().for_each(&mut visit),
        Block::HtmlContainer(node) => {
            if let HtmlContainerContent::Blocks(children) = &node.content {
                children.iter().for_each(&mut visit);
            }
        }
        _ => {}
    }
    match (inner, block) {
        (Some(start), _) => Extracted::Misplaced(start),
        (None, Block::Heading(heading)) if heading.kind == HeadingKind::Setext => {
            Extracted::Misplaced(span.end.saturating_sub(1).max(span.start))
        }
        (None, _) => Extracted::Misplaced(span.start),
    }
}

/// A task item's first paragraph, read back after its checkbox, which keeps
/// the whitespace after it as text and holds the paragraph off the line's
/// start.
fn serialize_task_paragraph(
    node: &Paragraph,
    cx: &Cx<'_>,
    checked: bool,
) -> Result<String, SerializeError> {
    let checkbox = if checked { "- [x] " } else { "- [ ] " };
    let parts = [
        Part::Literal(checkbox.into()),
        Part::Inlines(&node.children, Place::Within),
    ];
    let extract = extractor(|blocks| match blocks {
        [Block::List(list)] => match list.children.as_slice() {
            [item] if item.checked == Some(checked) => one_block(&item.children, paragraph_content),
            [_, second, ..] => match second.meta.span {
                Some(span) => Extracted::Misplaced(span.start),
                None => Extracted::Mismatch,
            },
            _ => Extracted::Mismatch,
        },
        [Block::List(_), second, ..] => match second.span() {
            Some(span) => Extracted::Misplaced(span.start),
            None => Extracted::Mismatch,
        },
        _ => Extracted::Mismatch,
    });
    let mut written = write_reading_back(&parts, extract, &cx.read_back)?;
    Ok(written.remove(0))
}

/// Whether `node` is written as a setext heading: a setext underline can only
/// express depth 1 (`=`) or 2 (`-`); any other depth falls back to ATX,
/// otherwise the depth is lost.
fn writes_setext(node: &Heading) -> bool {
    node.kind == HeadingKind::Setext && matches!(node.depth, 1 | 2)
}

fn heading_content(node: &Heading) -> impl for<'d> Fn(&'d Block) -> Option<&'d [Inline]> {
    let setext = writes_setext(node);
    let depth = node.depth;
    content_of(move |block| match block {
        Block::Heading(heading)
            if heading.depth == depth && (heading.kind == HeadingKind::Setext) == setext =>
        {
            Some(&heading.children)
        }
        _ => None,
    })
}

fn serialize_heading(node: &Heading, cx: &Cx<'_>) -> Result<String, SerializeError> {
    if writes_setext(node) {
        return Ok(serialize_setext_heading(node, cx, None)?.0);
    }
    let hashes = "#".repeat(usize::from(node.depth));
    if node.children.is_empty() {
        return Ok(hashes);
    }
    let content = heading_content(node);
    let parts = [
        Part::Literal(format!("{hashes} ")),
        Part::Inlines(&node.children, Place::Block),
    ];
    let mut written =
        write_reading_back(&parts, |blocks| one_block(blocks, &content), &cx.read_back)?;
    Ok(format!("{hashes} {}", written.remove(0)))
}

/// A setext heading's Markdown, and whether it is written right after the
/// definition before it, as the continuation of the paragraph that
/// definition was read from.
fn serialize_setext_heading(
    node: &Heading,
    cx: &Cx<'_>,
    after: Option<&Definition>,
) -> Result<(String, bool), SerializeError> {
    let content = heading_content(node);
    let marker = if node.depth == 1 { "=" } else { "-" };
    let underline = |content: &str| marker.repeat(content.len().max(3));
    let parts = [
        Part::Inlines(&node.children, Place::Block),
        Part::Literal(format!("\n{}", marker.repeat(3))),
    ];
    let error =
        match write_reading_back(&parts, |blocks| one_block(blocks, &content), &cx.read_back) {
            Ok(mut written) => {
                let content = written.remove(0);
                return Ok((format!("{content}\n{}", underline(&content)), false));
            }
            Err(error) => error,
        };
    let Some(definition) = after else {
        return Err(error);
    };
    let parts = [
        Part::Literal(serialize_definition(definition) + "\n"),
        Part::Inlines(&node.children, Place::Continuation),
        Part::Literal(format!("\n{}", marker.repeat(3))),
    ];
    let extract = extractor(|blocks| match blocks {
        [Block::Definition(_), rest @ ..] => one_block(rest, &content),
        _ => Extracted::Mismatch,
    });
    match write_reading_back(&parts, extract, &cx.read_back) {
        Ok(mut written) => {
            let content = written.remove(0);
            Ok((format!("{content}\n{}", underline(&content)), true))
        }
        Err(_) => Err(error),
    }
}

/// A directive's `[label]`, which reads back in `probe`, the directive
/// written with its label marked by `(start, end)`.
fn serialize_directive_label(
    label: &[Inline],
    cx: &Cx<'_>,
    open: String,
    close: String,
    content: impl for<'d> Fn(&'d Block) -> Option<&'d [Inline]>,
) -> Result<String, SerializeError> {
    if label.is_empty() {
        return Ok(String::new());
    }
    let parts = [
        Part::Literal(open),
        Part::Inlines(label, Place::Within),
        Part::Literal(close),
    ];
    let mut written =
        write_reading_back(&parts, |blocks| one_block(blocks, &content), &cx.read_back)?;
    Ok(format!("[{}]", written.remove(0)))
}

fn serialize_definition(node: &Definition) -> String {
    let destination = serialize_destination_kind(
        &node.destination,
        node.destination_kind,
        InlineSerializeContext::block_content(),
    );
    // The label is matched as written, so it is written as the AST holds it;
    // one that does not read back that way is unrepresentable.
    let label = escape_definition_label_source(&node.label);
    let mut output = format!("[{}]: {}", label, destination);
    if let (Some(title), Some(title_kind)) = (&node.title, node.title_kind) {
        output.push(' ');
        output.push_str(&serialize_title_kind(
            title,
            title_kind,
            InlineSerializeContext::block_content(),
        ));
    }
    output
}

/// The identifiers of the definitions `block` holds and of the references it
/// uses, at any depth.
fn known_labels_in_block(block: &Block, known: &mut Vec<String>) {
    let blocks = |children: &[Block], known: &mut Vec<String>| {
        for child in children {
            known_labels_in_block(child, known);
        }
    };
    match block {
        Block::Definition(node) => known.push(node.identifier.clone()),
        Block::Paragraph(node) => known_labels_in_inlines(&node.children, known),
        Block::Heading(node) => known_labels_in_inlines(&node.children, known),
        Block::BlockQuote(node) => blocks(&node.children, known),
        Block::Alert(node) => blocks(&node.children, known),
        Block::List(node) => {
            for item in &node.children {
                blocks(&item.children, known);
            }
        }
        Block::DescriptionList(node) => {
            for item in &node.children {
                known_labels_in_inlines(&item.term, known);
                for details in &item.details {
                    blocks(&details.children, known);
                }
            }
        }
        Block::HtmlContainer(node) => match &node.content {
            HtmlContainerContent::Blocks(children) => blocks(children, known),
            HtmlContainerContent::Inlines(children) => known_labels_in_inlines(children, known),
        },
        Block::FootnoteDefinition(node) => blocks(&node.children, known),
        Block::Table(node) => {
            for row in &node.rows {
                for cell in &row.cells {
                    known_labels_in_inlines(&cell.children, known);
                }
            }
        }
        Block::LeafDirective(node) => known_labels_in_inlines(&node.label, known),
        Block::ContainerDirective(node) => {
            known_labels_in_inlines(&node.label, known);
            blocks(&node.children, known);
        }
        _ => {}
    }
}

fn known_labels_in_inlines(inlines: &[Inline], known: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::LinkReference(node) => known.push(node.identifier.clone()),
            Inline::ImageReference(node) => known.push(node.identifier.clone()),
            _ => {}
        }
        if let Some(children) = inline::inline_children(inline) {
            known_labels_in_inlines(children, known);
        }
    }
}

fn serialize_block(
    block: &Block,
    options: &Cx<'_>,
    at_document_start: bool,
) -> Result<String, SerializeError> {
    match block {
        Block::Paragraph(node) => Ok(serialize_paragraph(node, options, None)?.0),
        Block::Heading(node) => serialize_heading(node, options),
        Block::ThematicBreak(node) => Ok(match node.marker {
            // A Dash break is normally written contiguous (`---`) — the form
            // that survives after a `-` bullet list, where the spaced `- - -`
            // would be re-read as nested list items. The one exception is the
            // document start, where a contiguous `---` opens frontmatter, so the
            // spaced form (which is not a frontmatter fence) is used there.
            ThematicBreakMarker::Dash if at_document_start => "- - -".into(),
            ThematicBreakMarker::Dash => "---".into(),
            ThematicBreakMarker::Asterisk => "***".into(),
            ThematicBreakMarker::Underscore => "___".into(),
        }),
        Block::BlockQuote(node) => {
            let inner = serialize_blocks_at_start(&node.children, options, false)?;
            let mut output = if inner.is_empty() {
                ">".into()
            } else if options.layout.has(block, Alternative::QuoteOpensEmpty) {
                format!(">\n{}", prefix_lines(&inner, "> "))
            } else {
                prefix_lines(&inner, "> ")
            };
            if options.layout.has(block, Alternative::QuoteEndsEmpty) {
                output.push_str("\n>");
            }
            Ok(output)
        }
        Block::Alert(node) => {
            let mut output = serialize_alert(node, options)?;
            if options.layout.has(block, Alternative::QuoteEndsEmpty) {
                output.push_str("\n>");
            }
            Ok(output)
        }
        Block::List(node) => serialize_list(node, options),
        Block::DescriptionList(node) => serialize_description_list(node, options),
        Block::CodeBlock(node) => serialize_code_block(node, options),
        // An HTML block's lines are joined with `\n`, so a value ending in one
        // ends with an empty line that belongs to the block (an unclosed
        // comment, say); it is written as it is.
        Block::HtmlBlock(node) => Ok(node.value.clone()),
        Block::HtmlContainer(node) => serialize_html_container(node, options),
        Block::Definition(node) => Ok(serialize_definition(node)),
        Block::FootnoteDefinition(node) => {
            let inner = serialize_blocks_at_start(&node.children, options, false)?;
            let label = if node.meta.span.is_some() {
                escape_footnote_label_source(&node.label)
            } else {
                escape_footnote_label_semantic(&node.label)
            };
            Ok(format!("[^{}]: {}", label, indent_continuation(&inner)))
        }
        Block::Table(node) => serialize_table(node, options),
        Block::MathBlock(node) => {
            let fence = block_math_fence(&node.value);
            Ok(fenced_body(&fence, &node.value, &fence))
        }
        Block::Frontmatter(node) => {
            let fence = match node.kind {
                FrontmatterKind::Yaml => "---",
                FrontmatterKind::Toml => "+++",
            };
            // The value holds the lines between the fences joined with `\n`,
            // so a final `\n` is an empty last line.
            Ok(format!("{fence}\n{}\n{fence}", node.value))
        }
        Block::MdxEsm(node) => Ok(node.value.clone()),
        Block::MdxExpression(node) => Ok(format!("{{{}}}", node.value)),
        Block::MdxJsx(node) => Ok(node.value.clone()),
        Block::LeafDirective(node) => {
            let attributes = serialize_attributes(&node.attributes);
            let label = serialize_directive_label(
                &node.label,
                options,
                format!("::{}[", node.name),
                format!("]{attributes}"),
                content_of(|block| match block {
                    Block::LeafDirective(directive) => Some(&directive.label[..]),
                    _ => None,
                }),
            )?;
            Ok(format!("::{}{label}{attributes}", node.name))
        }
        Block::ContainerDirective(node) => {
            let mut inner = serialize_blocks_at_start(&node.children, options, false)?;
            let fence = directive_fence(&inner);
            // Content ends with a line ending before the closing fence; an
            // empty directive takes no blank line, which would loosen a list
            // holding it.
            if !inner.is_empty() {
                inner.push('\n');
            }
            let attributes = serialize_attributes(&node.attributes);
            let label = serialize_directive_label(
                &node.label,
                options,
                format!(":::{}[", node.name),
                format!("]{attributes}\n:::"),
                content_of(|block| match block {
                    Block::ContainerDirective(directive) => Some(&directive.label[..]),
                    _ => None,
                }),
            )?;
            Ok(format!(
                "{fence}{}{label}{attributes}\n{inner}{fence}",
                node.name
            ))
        }
    }
}

fn serialize_html_container(
    node: &HtmlContainer,
    options: &Cx<'_>,
) -> Result<String, SerializeError> {
    match &node.content {
        HtmlContainerContent::Blocks(children) => {
            let inner = serialize_blocks_at_start(children, options, false)?;
            if inner.is_empty() {
                Ok(format!("{}\n{}", node.opening.raw, node.closing.raw))
            } else {
                Ok(format!(
                    "{}\n{}\n\n{}",
                    node.opening.raw, inner, node.closing.raw
                ))
            }
        }
        HtmlContainerContent::Inlines(children) => {
            // A summary reads back as the first line of a `<details>`.
            let parts = [
                Part::Literal(format!("<details>\n{}", node.opening.raw)),
                Part::Inlines(children, Place::Within),
                Part::Literal(format!("{}\n</details>", node.closing.raw)),
            ];
            let summary = content_of(|block| {
                let Block::HtmlContainer(details) = block else {
                    return None;
                };
                match &details.content {
                    HtmlContainerContent::Blocks(blocks) => match blocks.first() {
                        Some(Block::HtmlContainer(HtmlContainer {
                            content: HtmlContainerContent::Inlines(children),
                            ..
                        })) => Some(children),
                        _ => None,
                    },
                    HtmlContainerContent::Inlines(_) => None,
                }
            });
            let mut written = write_reading_back(
                &parts,
                |blocks| one_block(blocks, summary),
                &options.read_back,
            )?;
            Ok(format!(
                "{}{}{}",
                node.opening.raw,
                written.remove(0),
                node.closing.raw
            ))
        }
    }
}

fn serialize_alert(node: &Alert, options: &Cx<'_>) -> Result<String, SerializeError> {
    let mut output = String::from("> [!");
    output.push_str(alert_kind_name(node.kind));
    output.push(']');
    if let Some(title) = &node.title {
        if !title.is_empty() {
            output.push(' ');
            output.push_str(&escape_alert_title(title));
        }
    }
    let inner = serialize_blocks_at_start(&node.children, options, false)?;
    if !inner.is_empty() {
        output.push('\n');
        output.push_str(&prefix_lines(&inner, "> "));
    }
    Ok(output)
}

fn alert_kind_name(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Note => "NOTE",
        AlertKind::Tip => "TIP",
        AlertKind::Important => "IMPORTANT",
        AlertKind::Warning => "WARNING",
        AlertKind::Caution => "CAUTION",
    }
}

/// An alert title is kept as written, so only a line ending, which would end
/// its line, is written as a space.
fn escape_alert_title(input: &str) -> String {
    input.replace(['\n', '\r'], " ")
}

fn serialize_list(node: &List, options: &Cx<'_>) -> Result<String, SerializeError> {
    serialize_list_with_marker_spacing(node, options, "", " ", None)
}

/// A list written before `next`, the Markdown of the block after it. A list
/// the read-back keeps past the next block has its markers indented past
/// that block's indentation, and one it keeps apart from the next list takes
/// a marker other than the one that list starts with.
fn serialize_list_before(
    block: &Block,
    node: &List,
    options: &Cx<'_>,
    next: Option<&str>,
) -> Result<String, SerializeError> {
    let avoid = next
        .filter(|_| options.layout.has(block, Alternative::ListApartFromNext))
        .and_then(|next| {
            let marker = next.trim_start_matches(' ');
            let digits = marker.bytes().take_while(u8::is_ascii_digit).count();
            marker[digits..].chars().next()
        });
    let indent = next
        .filter(|_| options.layout.has(block, Alternative::ListPastNext))
        .map(|next| next.len() - next.trim_start_matches(' ').len());
    match indent {
        Some(indent @ 1..=3) => {
            serialize_list_with_marker_spacing(node, options, &" ".repeat(indent), " ", avoid)
        }
        Some(4..) => serialize_list_with_marker_spacing(node, options, " ", "    ", avoid),
        _ => serialize_list_with_marker_spacing(node, options, "", " ", avoid),
    }
}

fn serialize_list_with_marker_spacing(
    node: &List,
    options: &Cx<'_>,
    marker_prefix: &str,
    marker_padding: &str,
    avoid: Option<char>,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    for (index, item) in node.children.iter().enumerate() {
        if index > 0 {
            if node.tight {
                output.push('\n');
            } else {
                output.push_str("\n\n");
            }
        }
        let mut list_delimiter = if node.ordered {
            if options.ordered_delimiter == SerializeOptions::default().ordered_delimiter {
                node.delimiter
            } else {
                options.ordered_delimiter
            }
        } else if options.bullet == SerializeOptions::default().bullet {
            node.delimiter
        } else {
            options.bullet
        };
        let marker_char = |delimiter| {
            if node.ordered {
                ordered_list_marker(delimiter)
            } else {
                unordered_list_marker(delimiter)
            }
        };
        if avoid == Some(marker_char(list_delimiter)) {
            let others: &[ListDelimiter] = if node.ordered {
                &[ListDelimiter::Period, ListDelimiter::Paren]
            } else {
                &[
                    ListDelimiter::Dash,
                    ListDelimiter::Asterisk,
                    ListDelimiter::Plus,
                ]
            };
            if let Some(other) = others
                .iter()
                .find(|other| Some(marker_char(**other)) != avoid)
            {
                list_delimiter = *other;
            }
        }
        let marker = if node.ordered {
            let start = node.start.unwrap_or(1).saturating_add(index as u64);
            let delimiter = ordered_list_marker(list_delimiter);
            format!("{marker_prefix}{start}{delimiter}{marker_padding}")
        } else {
            format!(
                "{marker_prefix}{}{marker_padding}",
                unordered_list_marker(list_delimiter)
            )
        };
        let mut inner = serialize_item_blocks(&item.children, options, node.tight, item.checked)?;
        if let Some(checked) = item.checked {
            if let Some(rest) = inner.strip_prefix("- ") {
                inner = rest.into();
            }
            let checkbox = if checked { "[x] " } else { "[ ] " };
            inner = format!("{checkbox}{inner}");
        }
        if !node.tight
            && node.children.len() == 1
            && matches!(item.children.as_slice(), [Block::Paragraph(_)])
            && !inner.is_empty()
        {
            output.push_str(marker.trim_end());
            output.push_str("\n\n");
            output.push_str(&prefix_lines(&inner, &" ".repeat(marker.len())));
            continue;
        }
        if options.layout.has(item, Alternative::ItemOnNextLine) {
            // An item that starts blank has its content one column past the
            // marker, whatever padding the other items use.
            let content_indent = marker.trim_end().len() + 1;
            output.push_str(marker.trim_end());
            output.push('\n');
            output.push_str(&prefix_lines(&inner, &" ".repeat(content_indent)));
            continue;
        }
        output.push_str(&marker);
        output.push_str(&indent_after_first_line(&inner, marker.len()));
    }
    Ok(output)
}

/// An item's blocks; a task item's, `task` holding whether it is checked,
/// have their first paragraph read back after the checkbox.
fn serialize_item_blocks(
    blocks: &[Block],
    options: &Cx<'_>,
    tight: bool,
    task: Option<bool>,
) -> Result<String, SerializeError> {
    // Written last to first, as at the top level: a list reads the
    // indentation of the block after it.
    let mut written: Vec<String> = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate().rev() {
        let next = written.last().map(String::as_str);
        written.push(match (block, task) {
            (Block::List(list), _) => serialize_list_before(block, list, options, next)?,
            (Block::Paragraph(paragraph), Some(checked)) if index == 0 => {
                serialize_task_paragraph(paragraph, options, checked)?
            }
            _ => serialize_block(block, options, false)?,
        });
    }
    let mut output = String::new();
    for (index, written) in written.iter().rev().enumerate() {
        if index > 0 {
            if tight {
                output.push('\n');
            } else {
                output.push_str("\n\n");
            }
        }
        output.push_str(written);
    }
    Ok(output)
}

fn serialize_description_list(
    node: &DescriptionList,
    options: &Cx<'_>,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    for (item_index, item) in node.children.iter().enumerate() {
        if item_index > 0 {
            output.push_str(if node.tight { "\n" } else { "\n\n" });
        }
        output.push_str(&serialize_term(&item.term, options)?);
        for (detail_index, detail) in item.details.iter().enumerate() {
            if node.tight && detail.children.len() == 1 {
                if let Block::Paragraph(paragraph) = &detail.children[0] {
                    output.push('\n');
                    output.push_str(": ");
                    output.push_str(&serialize_details_line(&paragraph.children, options)?);
                    continue;
                }
            }
            // A loose list is re-parsed as loose only through an intra-item blank;
            // the parser treats blanks BETWEEN items as tight-preserving group
            // separators. Encode the looseness with a blank line before the term's
            // first definition marker (a `blank_after_term`), so the round trip
            // keeps `tight=false`.
            if !node.tight && detail_index == 0 {
                output.push('\n');
            }
            output.push_str("\n:");
            let inner = if node.tight {
                serialize_item_blocks(&detail.children, options, true, None)?
            } else {
                serialize_blocks_at_start(&detail.children, options, false)?
            };
            if !inner.is_empty() {
                output.push('\n');
                output.push_str(&indent_lines(&inner, 4));
            }
        }
    }
    Ok(output)
}

/// A description term, which reads back above a details marker.
fn serialize_term(term: &[Inline], cx: &Cx<'_>) -> Result<String, SerializeError> {
    let parts = [
        Part::Inlines(term, Place::Block),
        Part::Literal("\n: x".into()),
    ];
    let content = content_of(|block| match block {
        Block::DescriptionList(list) => list.children.first().map(|item| &item.term[..]),
        _ => None,
    });
    let mut written =
        write_reading_back(&parts, |blocks| one_block(blocks, content), &cx.read_back)?;
    Ok(written.remove(0))
}

/// The paragraph of tight description details, on the marker's line.
fn serialize_details_line(inlines: &[Inline], cx: &Cx<'_>) -> Result<String, SerializeError> {
    let parts = [
        Part::Literal("t\n: ".into()),
        Part::Inlines(inlines, Place::Within),
    ];
    let content = content_of(|block| {
        let Block::DescriptionList(list) = block else {
            return None;
        };
        let details = list.children.first()?.details.first()?;
        match details.children.as_slice() {
            [Block::Paragraph(paragraph)] => Some(&paragraph.children[..]),
            _ => None,
        }
    });
    let mut written =
        write_reading_back(&parts, |blocks| one_block(blocks, content), &cx.read_back)?;
    Ok(written.remove(0))
}

fn serialize_code_block(
    node: &CodeBlock,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    match node.kind {
        CodeBlockKind::Indented => {
            // Each value line ends with a line ending; the block gap or the
            // final newline writes a last `\n`, so only `\r` and `\r\n` stay.
            let body = trim_trailing_newline(&node.value);
            let mut output = prefix_lines(body, "    ");
            let ending = &node.value[body.len()..];
            if matches!(ending, "\r" | "\r\n") {
                output.push_str(ending);
            }
            Ok(output)
        }
        CodeBlockKind::Fenced { marker, length } => {
            let marker = code_block_fence_marker(node, marker, options);
            let (fence, indent) = code_block_fence(&node.value, marker, length.max(3));
            let mut opener = fence.clone();
            if let Some(info) = &node.info {
                opener.push(' ');
                opener.push_str(&escape_code_info(info));
            }
            let body = fenced_body(&opener, &node.value, &fence);
            Ok(if indent == 0 {
                body
            } else {
                prefix_lines(&body, &" ".repeat(indent))
            })
        }
    }
}

/// A fenced block: `opener`, then `value` (whose lines each keep their line
/// ending, the last one optionally), then `closer`. An empty value writes no
/// line between the fences.
fn fenced_body(opener: &str, value: &str, closer: &str) -> String {
    let mut output = String::with_capacity(opener.len() + value.len() + closer.len() + 2);
    output.push_str(opener);
    output.push('\n');
    output.push_str(value);
    if !value.is_empty() && !ends_with_line_ending(value) {
        output.push('\n');
    }
    output.push_str(closer);
    output
}

fn code_block_fence_marker(
    node: &CodeBlock,
    marker: FenceMarker,
    options: &SerializeOptions,
) -> FenceMarker {
    if node.info.as_deref().is_some_and(|info| info.contains('`')) {
        return FenceMarker::Tilde;
    }
    if options.fence_marker == SerializeOptions::default().fence_marker {
        marker
    } else {
        options.fence_marker
    }
}

fn escape_code_info(input: &str) -> String {
    let mut output = String::new();
    // The parser trims the info string, so whitespace at either end is
    // written as a character reference.
    let inner_start = input.len() - input.trim_start_matches([' ', '\t']).len();
    let inner_end = input.trim_end_matches([' ', '\t']).len().max(inner_start);
    for (offset, char) in input.char_indices() {
        match char {
            ' ' | '\t' if offset < inner_start || offset >= inner_end => {
                output.push_str(&format!("&#x{:X};", char as u32));
            }
            '\n' => output.push_str("&#xA;"),
            '\r' => output.push_str("&#xD;"),
            '\t' => output.push(char),
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '\\' | '&' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn serialize_table(node: &Table, cx: &Cx<'_>) -> Result<String, SerializeError> {
    let delimiter_row = node
        .alignments
        .iter()
        .map(|alignment| match alignment {
            TableAlignment::None => "---",
            TableAlignment::Left => ":---",
            TableAlignment::Center => ":---:",
            TableAlignment::Right => "---:",
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let mut parts = Vec::new();
    for (row_index, row) in node.rows.iter().enumerate() {
        match row_index {
            0 => parts.push(Part::Literal("| ".into())),
            1 => parts.push(Part::Literal(format!(" |\n| {delimiter_row} |\n| "))),
            _ => parts.push(Part::Literal(" |\n| ".into())),
        }
        for (cell_index, cell) in row.cells.iter().enumerate() {
            if cell_index > 0 {
                parts.push(Part::Literal(" | ".into()));
            }
            parts.push(Part::Inlines(&cell.children, Place::Cell));
        }
    }
    parts.push(Part::Literal(if node.rows.len() == 1 {
        format!(" |\n| {delimiter_row} |")
    } else {
        " |".into()
    }));
    let shape: Vec<usize> = node.rows.iter().map(|row| row.cells.len()).collect();
    let extract = extractor(|blocks| {
        let [Block::Table(table)] = blocks else {
            return Extracted::Mismatch;
        };
        let read: Vec<usize> = table.rows.iter().map(|row| row.cells.len()).collect();
        if read != shape {
            return Extracted::Mismatch;
        }
        Extracted::Lists(
            table
                .rows
                .iter()
                .flat_map(|row| row.cells.iter().map(|cell| &cell.children[..]))
                .collect(),
        )
    });
    let cells = write_reading_back(&parts, extract, &cx.read_back)?;
    let mut cells = cells.into_iter();
    let mut output = String::new();
    for (row_index, row) in node.rows.iter().enumerate() {
        if row_index > 0 {
            output.push('\n');
        }
        let written: Vec<String> = cells.by_ref().take(row.cells.len()).collect();
        output.push_str(&format!("| {} |", written.join(" | ")));
        if row_index == 0 {
            output.push_str(&format!("\n| {delimiter_row} |"));
        }
    }
    Ok(output)
}

/// Where inline content is written, as the value encodings of its nodes
/// read it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct InlineSerializeContext {
    /// In a table cell, which a raw `|` would split.
    table_cell: bool,
}

impl InlineSerializeContext {
    const fn table_cell() -> Self {
        Self { table_cell: true }
    }

    const fn block_content() -> Self {
        Self { table_cell: false }
    }
}

/// Where a block's inline content sits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Place {
    /// It opens a block, whose first line cannot move.
    Block,
    /// It continues a paragraph, so its first line can be indented too.
    Continuation,
    /// It follows other syntax on its block's first line.
    Within,
    /// It is a table cell.
    Cell,
}

/// The URI `link` writes as an angle-bracket autolink: a link with no title
/// whose one child is a text the autolink `<text>` reads back to the link's
/// destination from.
fn autolink_uri(link: &Link) -> Option<&str> {
    let [Inline::Text(text)] = link.children.as_slice() else {
        return None;
    };
    (link.title.is_none()
        && link.destination_kind == LinkDestinationKind::Bare
        && crate::parse::angle_autolink_destination(&text.value).as_deref()
            == Some(link.destination.as_str()))
    .then_some(text.value.as_str())
}

fn serialize_attributes(attributes: &[DirectiveAttribute]) -> String {
    serialize_attributes_with_context(attributes, InlineSerializeContext::default())
}

fn serialize_attributes_with_context(
    attributes: &[DirectiveAttribute],
    context: InlineSerializeContext,
) -> String {
    if attributes.is_empty() {
        return String::new();
    }
    let mut output = String::from("{");
    for (index, attribute) in attributes.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        match (&*attribute.name, &attribute.value) {
            ("id", Some(value)) if is_directive_shorthand_value(value) => {
                output.push('#');
                output.push_str(value);
            }
            ("class", Some(value)) if is_directive_shorthand_value(value) => {
                output.push('.');
                output.push_str(value);
            }
            (_, Some(value)) => {
                output.push_str(&attribute.name);
                output.push('=');
                output.push('"');
                output.push_str(&escape_title_with_context(
                    value,
                    LinkTitleKind::DoubleQuote,
                    context,
                ));
                output.push('"');
            }
            (_, None) => output.push_str(&attribute.name),
        }
    }
    output.push('}');
    output
}

fn is_directive_shorthand_value(input: &str) -> bool {
    !input.is_empty()
        && input
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '-'))
}

fn escape_destination_with_pipe(input: &str, escape_pipe: bool) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            // A space would end the destination, and `\ ` is no escape.
            char if char.is_control() || char == ' ' => {
                output.push_str(&format!("&#x{:X};", char as u32));
            }
            '|' if escape_pipe => {
                output.push('\\');
                output.push(char);
            }
            '(' | ')' | '\\' | '<' | '>' | '&' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

/// Normalize a serialized reference label exactly the way the parser matches
/// reference labels: collapse internal whitespace and Unicode case-fold the RAW
/// text (no backslash/entity unescape). Delegating to the parser's
/// `normalize_label` keeps this in lockstep so the Shortcut/Collapsed arms
/// decide correctly whether the rendered children already reproduce the
/// definition identifier.
fn normalize_reference_label(input: &str) -> String {
    crate::parse::normalize_label(input)
}

/// Emit the bracketed body of a link/image reference (`[text]`, `[text][]`, or
/// `[text][label]`) given the already-serialized `rendered` children and the
/// escaped raw `label`.
///
/// A Shortcut/Collapsed reference normally re-uses the rendered children as the
/// matching label, so it is only whole if those children fold back to the
/// definition identifier. Under RAW label matching the children can re-escape
/// in a fold-breaking way (e.g. a leading `^` becomes `\^`), so when the
/// children no longer reproduce the identifier we substitute the escaped raw
/// label as the bracket body — keeping the Shortcut/Collapsed kind (and its
/// re-parse) intact instead of degrading it into a Full reference.
fn push_reference_body(
    output: &mut String,
    kind: ReferenceKind,
    rendered: &str,
    children_match_identifier: bool,
    escaped_label: &str,
) {
    // For a Shortcut/Collapsed reference the bracket body must fold back to the
    // identifier on its own. Substitute the escaped raw label when the rendered
    // children would not (keeping the reference kind), but a Full reference
    // always keeps its rendered text since its explicit label does the matching.
    let use_label_body = !children_match_identifier && !matches!(kind, ReferenceKind::Full);
    let body = if use_label_body {
        escaped_label
    } else {
        rendered
    };

    output.push('[');
    output.push_str(body);
    output.push(']');

    match kind {
        ReferenceKind::Shortcut => {}
        ReferenceKind::Collapsed => output.push_str("[]"),
        ReferenceKind::Full => {
            output.push('[');
            output.push_str(escaped_label);
            output.push(']');
        }
    }
}

/// Escape the explicit label of a link/image reference. The original `label`
/// (not the normalized identifier) is used so case and entity spelling survive
/// the round-trip. A parsed label (`span.is_some()`) is already source text, so
/// only control characters are escaped; a hand-built label is semantic text and
/// is escaped like any reference label.
fn reference_explicit_label(
    from_source: bool,
    label: &str,
    context: InlineSerializeContext,
) -> String {
    if from_source {
        escape_reference_label_source(label, context.table_cell)
    } else {
        escape_reference_label_with_pipe(label, context.table_cell)
    }
}

/// Escapes a parsed definition label for re-emission. A definition label may
/// span several physical lines (CommonMark §4.7), and the parser stores those
/// interior newlines verbatim in `label`. Emitting them as literal line breaks
/// (rather than `&#xA;`) lets the multi-line label re-parse to the same raw
/// label, keeping the round trip stable; other control characters are still
/// numeric-escaped, and tabs pass through as in `escape_reference_label_source`.
fn escape_definition_label_source(input: &str) -> String {
    escape_reference_label_source(input, false)
}

fn escape_reference_label_source(input: &str, escape_pipe: bool) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            // A reference label may span several physical lines, and the parser
            // matches the RAW label (whitespace collapsed, no entity decode), so
            // every control char, line endings and tabs among them, is written
            // as itself rather than as a reference such as `&#xA;`. This keeps
            // a whitespace-bearing label re-parsing as the same reference —
            // crucially, a `^`-prefixed label with a literal space stays a link
            // reference instead of becoming a footnote.
            char if char.is_control() => output.push(char),
            '|' if escape_pipe => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn escape_reference_label_with_pipe(input: &str, escape_pipe: bool) -> String {
    escape_label_syntax(input, escape_pipe, false)
}

/// A footnote label is matched as written, so its chars are.
fn escape_footnote_label_source(input: &str) -> String {
    input.into()
}

fn escape_footnote_label_semantic(input: &str) -> String {
    escape_label_syntax(input, false, true)
}

fn escape_label_syntax(input: &str, escape_pipe: bool, escape_whitespace: bool) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            char if char.is_whitespace() && escape_whitespace => {
                output.push_str(&format!("&#x{:X};", char as u32));
            }
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '|' if escape_pipe => {
                output.push('\\');
                output.push(char);
            }
            '\\' | '[' | ']' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn escape_wikilink_part(input: &str) -> String {
    let mut output = String::new();
    for (offset, char) in input.char_indices() {
        match char {
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '&' if crate::parse::parse_character_reference(input, offset).is_some() => {
                output.push('\\');
                output.push(char);
            }
            '\\' | '[' | ']' | '|' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn serialize_destination_kind(
    input: &str,
    kind: LinkDestinationKind,
    context: InlineSerializeContext,
) -> String {
    match kind {
        LinkDestinationKind::Omitted if input.is_empty() => String::new(),
        LinkDestinationKind::Angle => {
            let mut output = String::from("<");
            output.push_str(&escape_angle_destination_with_context(input, context));
            output.push('>');
            output
        }
        LinkDestinationKind::Bare | LinkDestinationKind::Omitted => {
            if input.is_empty() {
                "<>".into()
            } else {
                escape_destination_with_pipe(input, context.table_cell)
            }
        }
    }
}

fn escape_angle_destination_with_context(input: &str, context: InlineSerializeContext) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '|' if context.table_cell => {
                output.push('\\');
                output.push(char);
            }
            '\\' | '<' | '>' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn serialize_title_kind(
    input: &str,
    kind: LinkTitleKind,
    context: InlineSerializeContext,
) -> String {
    let (open, close) = match kind {
        LinkTitleKind::DoubleQuote => ('"', '"'),
        LinkTitleKind::SingleQuote => ('\'', '\''),
        LinkTitleKind::Paren => ('(', ')'),
    };
    let mut output = String::new();
    output.push(open);
    output.push_str(&escape_title_with_context(input, kind, context));
    output.push(close);
    output
}

fn escape_title_with_context(
    input: &str,
    kind: LinkTitleKind,
    context: InlineSerializeContext,
) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            char if char.is_control() => output.push_str(&format!("&#x{:X};", char as u32)),
            '|' if context.table_cell => {
                output.push('\\');
                output.push(char);
            }
            '\\' | '&' => {
                output.push('\\');
                output.push(char);
            }
            '"' if kind == LinkTitleKind::DoubleQuote => {
                output.push('\\');
                output.push(char);
            }
            '\'' if kind == LinkTitleKind::SingleQuote => {
                output.push('\\');
                output.push(char);
            }
            '(' | ')' if kind == LinkTitleKind::Paren => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

fn unordered_list_marker(delimiter: ListDelimiter) -> char {
    match delimiter {
        ListDelimiter::Dash => '-',
        ListDelimiter::Asterisk => '*',
        ListDelimiter::Plus => '+',
        ListDelimiter::Period | ListDelimiter::Paren => '-',
    }
}

fn ordered_list_marker(delimiter: ListDelimiter) -> char {
    match delimiter {
        ListDelimiter::Paren => ')',
        ListDelimiter::Dash
        | ListDelimiter::Asterisk
        | ListDelimiter::Plus
        | ListDelimiter::Period => '.',
    }
}

fn prefix_lines(input: &str, prefix: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    let bytes = input.as_bytes();
    let mut output = String::new();
    let mut line_start = 0;
    let mut cursor = 0;
    while cursor < input.len() {
        let eol_end = match bytes[cursor] {
            b'\n' => Some(cursor + 1),
            b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => Some(cursor + 2),
            b'\r' => Some(cursor + 1),
            _ => None,
        };
        if let Some(end) = eol_end {
            output.push_str(prefix);
            output.push_str(&input[line_start..end]);
            cursor = end;
            line_start = cursor;
        } else {
            cursor += 1;
        }
    }
    if line_start < input.len() {
        output.push_str(prefix);
        output.push_str(&input[line_start..]);
    }
    output
}

fn indent_after_first_line(input: &str, width: usize) -> String {
    let indent = " ".repeat(width);
    input
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                line.into()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn indent_lines(input: &str, width: usize) -> String {
    let indent = " ".repeat(width);
    input
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn indent_continuation(input: &str) -> String {
    input
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                line.into()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn trim_trailing_newline(input: &str) -> &str {
    input.trim_end_matches('\n').trim_end_matches('\r')
}

fn ends_with_line_ending(input: &str) -> bool {
    input.ends_with('\n') || input.ends_with('\r')
}

fn fence_for(input: &str, marker: FenceMarker, min_len: usize) -> String {
    let char = match marker {
        FenceMarker::Backtick => '`',
        FenceMarker::Tilde => '~',
    };
    let longest = longest_char_streak(input, char);
    char.to_string().repeat(min_len.max(longest + 1))
}

/// The fence for a code block's `value` and the columns the block is indented
/// by, so that no line of `value` closes it. A closing fence is up to three
/// spaces, at least as many marker chars, and nothing else but spaces and
/// tabs. The fence keeps `min_len` chars when indenting the block (which the
/// value's lines lose again) moves every closing-like line past three spaces;
/// otherwise it grows past the longest of them.
fn code_block_fence(value: &str, marker: FenceMarker, min_len: usize) -> (String, usize) {
    let char = match marker {
        FenceMarker::Backtick => '`',
        FenceMarker::Tilde => '~',
    };
    let closing_like = |line: &str, length: usize| {
        let indent = line.len() - line.trim_start_matches(' ').len();
        let rest = &line[indent..];
        let run = rest.len() - rest.trim_start_matches(char).len();
        (indent <= 3 && run >= length && rest[run..].trim_matches([' ', '\t']).is_empty())
            .then_some((indent, run))
    };
    let lines = || value.split(['\n', '\r']);
    let least_indent = lines()
        .filter_map(|line| closing_like(line, min_len))
        .map(|(indent, _)| indent)
        .min();
    match least_indent {
        None => (char.to_string().repeat(min_len), 0),
        Some(indent) if indent > 0 => (char.to_string().repeat(min_len), 4 - indent),
        Some(_) => {
            let mut length = min_len;
            for line in lines() {
                if let Some((_, run)) = closing_like(line, length) {
                    length = run + 1;
                }
            }
            (char.to_string().repeat(length), 0)
        }
    }
}

fn inline_code_fence(input: &str) -> String {
    fence_for(input, FenceMarker::Backtick, 1)
}

fn code_span_needs_padding(input: &str) -> bool {
    input.starts_with('`')
        || input.ends_with('`')
        || (input.starts_with(' ') && input.ends_with(' ') && input.chars().any(|char| char != ' '))
}

fn table_cell_escape_code_pipes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for char in input.chars() {
        if char == '|' {
            output.push('\\');
        }
        output.push(char);
    }
    output
}

fn block_math_fence(input: &str) -> String {
    let mut length = 2;
    for line in trim_trailing_newline(input).lines() {
        let trimmed = line.trim();
        if trimmed.len() >= 2 && trimmed.chars().all(|char| char == '$') {
            length = length.max(trimmed.len() + 1);
        }
    }
    "$".repeat(length)
}

fn serialize_inline_math(node: &MathInline) -> Result<String, SerializeError> {
    let input = node.value.as_str();
    match node.kind {
        MathInlineKind::Code => {
            if input.contains("`$") {
                return Err(SerializeError::UnsupportedNode(
                    "inline math (code-math form) containing a `$` close",
                ));
            }
            Ok(format!("$`{input}`$"))
        }
        // Dollar math is emitted verbatim behind an exact-length fence: no
        // padding strip and no fence widening. A single-`$` value can only
        // contain a `$` that is backslash-escaped (`\$`), which the flanking
        // parser skips, so an exact `$`…`$` fence round-trips; a `$$` display
        // value is verbatim including any edge spaces or newlines.
        MathInlineKind::Dollar { dollars } => {
            let fence = "$".repeat(usize::from(dollars));
            Ok(format!("{fence}{input}{fence}"))
        }
    }
}

fn longest_char_streak(input: &str, needle: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for char in input.chars() {
        if char == needle {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn directive_fence(inner: &str) -> String {
    ":".repeat(directive_fence_len(inner))
}

fn directive_fence_len(inner: &str) -> usize {
    let mut max = 3;
    for line in inner.lines() {
        if let Some(length) = directive_closing_fence_len(line) {
            max = max.max(length + 1);
        }
    }
    max
}

fn directive_closing_fence_len(line: &str) -> Option<usize> {
    let trimmed = trim_up_to_three_indent_columns(line)?;
    let length = trimmed
        .as_bytes()
        .iter()
        .take_while(|byte| **byte == b':')
        .count();
    if length >= 3 && trimmed[length..].trim().is_empty() {
        Some(length)
    } else {
        None
    }
}

fn trim_up_to_three_indent_columns(input: &str) -> Option<&str> {
    let mut columns = 0usize;
    let mut bytes = 0usize;
    for byte in input.as_bytes() {
        match *byte {
            b' ' => columns += 1,
            b'\t' => columns += 4 - (columns % 4),
            _ => break,
        }
        bytes += 1;
    }
    (columns <= 3).then_some(&input[bytes..])
}
