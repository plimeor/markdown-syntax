//! AST validation: [`Document::validate`] walks the tree and reports each
//! invalid or unsupported node shape as a [`Diagnostic`]. Serialization and HTML
//! rendering run this first and refuse an invalid document.

use alloc::vec::Vec;

use crate::{
    ast::{
        Alert, Block, CodeBlock, CodeBlockKind, CodeInline, ContainerDirective, Definition,
        DirectiveAttribute, Document, Escape, Frontmatter, Heading, HtmlContainer,
        HtmlContainerContent, Inline, LeafDirective, LineBreakKind, LinkDestinationKind, List,
        ListItem, MathInline, MathInlineKind, ReferenceKind, Table, TextDirective,
    },
    diagnostic::Diagnostic,
    parse::{
        alert_title, frontmatter_fence_kind, interrupts_paragraph, is_blank, is_footnote_label,
        is_reference_label, is_written_footnote_label, normalize_label, MAX_BLOCK_NESTING,
    },
    serialize::{writes_setext, written_marker},
    span::Span,
};

impl Document {
    /// Validate this document's AST shape, returning a diagnostic for each
    /// invalid or unsupported node (empty when the document is well-formed).
    pub fn validate(&self) -> Vec<Diagnostic> {
        validate_document(self)
    }
}

pub(crate) fn validate_document(document: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    validate_blocks(&document.children, 0, &mut diagnostics);
    diagnostics
}

/// A sequence of sibling blocks: each block, and two adjacent lists that one
/// marker would write as one list. `depth` counts the containers around the
/// blocks as the parser's nesting limit counts them: block quotes and alerts,
/// list items, footnote definitions, HTML containers, and container
/// directives.
fn validate_blocks(blocks: &[Block], depth: usize, diagnostics: &mut Vec<Diagnostic>) {
    for (index, block) in blocks.iter().enumerate() {
        if let (Some(Block::List(before)), Block::List(list)) =
            (index.checked_sub(1).map(|before| &blocks[before]), block)
        {
            if written_marker(before) == written_marker(list) {
                diagnostics.push(Diagnostic::invalid(
                    list.meta.span,
                    "adjacent lists cannot use the same marker",
                ));
            }
        }
        if let Block::Definition(definition) = block {
            // A line indented four columns or more continues a paragraph,
            // and starts one on the line after an alert's marker, which may
            // open containers first; neither starts a footnote definition.
            let indented = match index.checked_sub(1) {
                Some(before) => matches!(blocks[before], Block::Definition(_)),
                None => depth > 0,
            };
            validate_definition(definition, depth, indented, diagnostics);
        }
        validate_block(block, depth, diagnostics);
    }
}

fn validate_block(block: &Block, depth: usize, diagnostics: &mut Vec<Diagnostic>) {
    match block {
        Block::Paragraph(paragraph) => validate_inlines(&paragraph.children, diagnostics),
        Block::Heading(heading) => validate_heading(heading, diagnostics),
        Block::BlockQuote(block_quote) => {
            validate_blocks(&block_quote.children, depth + 1, diagnostics);
        }
        Block::Alert(alert) => validate_alert(alert, depth, diagnostics),
        Block::List(list) => validate_list(list, depth, diagnostics),
        Block::Table(table) => validate_table(table, diagnostics),
        Block::FootnoteDefinition(definition) => {
            if definition.identifier.is_empty() {
                diagnostics.push(Diagnostic::invalid(
                    definition.meta.span,
                    "footnote definition identifier cannot be empty",
                ));
            }
            validate_footnote_label(
                definition.meta.span,
                &definition.label,
                &definition.identifier,
                diagnostics,
            );
            validate_blocks(&definition.children, depth + 1, diagnostics);
        }
        // Checked with its siblings in `validate_blocks`.
        Block::Definition(_) => {}
        Block::LeafDirective(directive) => validate_leaf_directive(directive, diagnostics),
        Block::ContainerDirective(directive) => {
            validate_container_directive(directive, depth, diagnostics)
        }
        Block::HtmlContainer(container) => validate_html_container(container, depth, diagnostics),
        Block::CodeBlock(code) => validate_code_block(code, diagnostics),
        Block::Frontmatter(frontmatter) => validate_frontmatter(frontmatter, depth, diagnostics),
        Block::ThematicBreak(_) | Block::HtmlBlock(_) | Block::MathBlock(_) => {}
    }
}

fn validate_html_container(
    container: &HtmlContainer,
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if container.opening.name.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            container.opening.meta.span,
            "HTML container opening tag name cannot be empty",
        ));
    }
    if container.closing.name.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            container.closing.meta.span,
            "HTML container closing tag name cannot be empty",
        ));
    }
    if container.opening.name != container.closing.name {
        diagnostics.push(Diagnostic::invalid(
            container.meta.span,
            "HTML container opening and closing tag names must match",
        ));
    }
    if container.opening.raw.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            container.opening.meta.span,
            "HTML container opening tag source cannot be empty",
        ));
    }
    if container.closing.raw.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            container.closing.meta.span,
            "HTML container closing tag source cannot be empty",
        ));
    }

    match &container.content {
        HtmlContainerContent::Blocks(children) => {
            validate_blocks(children, depth + 1, diagnostics);
        }
        HtmlContainerContent::Inlines(children) => validate_inlines(children, diagnostics),
    }
}

fn validate_heading(heading: &Heading, diagnostics: &mut Vec<Diagnostic>) {
    if heading.depth == 0 || heading.depth > 6 {
        diagnostics.push(Diagnostic::invalid(
            heading.meta.span,
            "heading depth must be in the range 1..=6",
        ));
    }
    validate_inlines(&heading.children, diagnostics);
}

/// An alert's title is what the parser reads from its marker line: never
/// empty, and without spaces or tabs around it.
fn validate_alert(alert: &Alert, depth: usize, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(title) = &alert.title {
        if alert_title(title).as_deref() != Some(title.as_str()) {
            diagnostics.push(Diagnostic::invalid(
                alert.meta.span,
                "alert title cannot be empty or start or end with a space or a tab",
            ));
        }
    }
    validate_blocks(&alert.children, depth + 1, diagnostics);
}

fn validate_list(list: &List, depth: usize, diagnostics: &mut Vec<Diagnostic>) {
    if list.children.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            list.meta.span,
            "list must hold at least one item",
        ));
    }
    validate_list_start(list, diagnostics);
    for item in &list.children {
        validate_task_item(item, diagnostics);
        if list.tight {
            validate_tight_item(item, diagnostics);
        }
        validate_blocks(&item.children, depth + 1, diagnostics);
    }
}

/// A task item's checkbox opens its first paragraph after the definitions it
/// starts with; without one, `[ ]` or `[x]` reads as text.
fn validate_task_item(item: &ListItem, diagnostics: &mut Vec<Diagnostic>) {
    if item.checked.is_none() {
        return;
    }
    let first = item
        .children
        .iter()
        .find(|block| !matches!(block, Block::Definition(_)));
    if !matches!(first, Some(Block::Paragraph(paragraph)) if !paragraph.children.is_empty()) {
        diagnostics.push(Diagnostic::invalid(
            item.meta.span,
            "task list item must hold a non-empty paragraph after its leading definitions",
        ));
    }
}

/// A tight item writes each block on the line after the one before; after a
/// paragraph's line, a block that cannot interrupt the paragraph continues it.
fn validate_tight_item(item: &ListItem, diagnostics: &mut Vec<Diagnostic>) {
    for pair in item.children.windows(2) {
        let [Block::Paragraph(_), block] = pair else {
            continue;
        };
        let interrupts = match block {
            Block::Heading(heading) => !writes_setext(heading),
            _ => interrupts_paragraph(block),
        };
        if !interrupts {
            diagnostics.push(Diagnostic::invalid(
                block.span(),
                "in a tight list item, a block after a paragraph must be able to interrupt it",
            ));
        }
    }
}

fn validate_table(table: &Table, diagnostics: &mut Vec<Diagnostic>) {
    if table.rows.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            table.meta.span,
            "table must contain at least a header row",
        ));
        return;
    }

    let width = table.rows[0].cells.len();
    if width == 0 {
        diagnostics.push(Diagnostic::invalid(
            table.meta.span,
            "table header row must contain at least one cell",
        ));
    }

    if table.alignments.len() != width {
        diagnostics.push(Diagnostic::invalid(
            table.meta.span,
            "table alignment count must match header width",
        ));
    }

    for row in &table.rows {
        if row.cells.len() != width {
            diagnostics.push(Diagnostic::invalid(
                row.meta.span,
                "table row width must match header width",
            ));
        }
        for cell in &row.cells {
            validate_inlines(&cell.children, diagnostics);
            validate_cell_values(&cell.children, diagnostics);
            validate_one_line(
                &cell.children,
                "a table cell cannot hold a line break",
                diagnostics,
            );
        }
    }
}

/// The values a table cell writes as they are. The row splits at each `|`
/// after no backslash or an even run of them, and the cell's inline input
/// reads an odd run before a `|` with one backslash less, so no cell source
/// gives a value that holds a `|` after an odd backslash run.
fn validate_cell_values(inlines: &[Inline], diagnostics: &mut Vec<Diagnostic>) {
    for inline in inlines {
        let unwritable = match inline {
            Inline::Code(node) => has_odd_escaped_pipe(&node.value),
            Inline::Math(node) => has_odd_escaped_pipe(&node.value),
            Inline::Html(node) => has_odd_escaped_pipe(&node.value),
            Inline::Autolink(node) => has_odd_escaped_pipe(&node.text),
            Inline::LinkReference(node) => has_odd_escaped_pipe(&node.label),
            Inline::ImageReference(node) => has_odd_escaped_pipe(&node.label),
            Inline::FootnoteReference(node) => has_odd_escaped_pipe(&node.label),
            Inline::WikiLink(node) => {
                has_odd_escaped_pipe(&node.target) || has_odd_escaped_pipe(&node.label)
            }
            _ => false,
        };
        if unwritable {
            diagnostics.push(Diagnostic::invalid(
                inline.span(),
                "a value in a table cell cannot hold a `|` after an odd run of backslashes",
            ));
        }
        validate_cell_values(inline.children(), diagnostics);
    }
}

/// Whether `value` holds a `|` right after an odd run of backslashes.
fn has_odd_escaped_pipe(value: &str) -> bool {
    let mut backslashes = 0usize;
    for byte in value.bytes() {
        if byte == b'|' && backslashes % 2 == 1 {
            return true;
        }
        backslashes = if byte == b'\\' { backslashes + 1 } else { 0 };
    }
    false
}

/// Inline content written on one line, at any depth: a table cell, or a leaf
/// or container directive's label on its opening line.
fn validate_one_line(inlines: &[Inline], message: &'static str, diagnostics: &mut Vec<Diagnostic>) {
    for inline in inlines {
        if matches!(inline, Inline::SoftBreak(_) | Inline::LineBreak(_)) {
            diagnostics.push(Diagnostic::invalid(inline.span(), message));
        }
        validate_one_line(inline.children(), message, diagnostics);
    }
}

fn validate_leaf_directive(directive: &LeafDirective, diagnostics: &mut Vec<Diagnostic>) {
    validate_directive_name(directive.meta.span, &directive.name, diagnostics);
    validate_directive_attributes(&directive.attributes, diagnostics);
    validate_inlines(&directive.label, diagnostics);
    validate_one_line(
        &directive.label,
        "a leaf directive label cannot hold a line break",
        diagnostics,
    );
}

fn validate_container_directive(
    directive: &ContainerDirective,
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_directive_name(directive.meta.span, &directive.name, diagnostics);
    validate_directive_attributes(&directive.attributes, diagnostics);
    validate_inlines(&directive.label, diagnostics);
    validate_one_line(
        &directive.label,
        "a container directive label cannot hold a line break",
        diagnostics,
    );
    validate_blocks(&directive.children, depth + 1, diagnostics);
}

/// A definition's label is written as a reference label, and its identifier
/// is the parser's normalization of it. A label that is `^` and a footnote
/// label opens a footnote definition instead, unless the definition is past
/// the parser's nesting limit or may come from a line `indented` into a
/// paragraph, which starts no block.
fn validate_definition(
    definition: &Definition,
    depth: usize,
    indented: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if definition
        .identifier
        .trim_matches([' ', '\t', '\n', '\r'])
        .is_empty()
    {
        diagnostics.push(Diagnostic::invalid(
            definition.meta.span,
            "definition identifier cannot be empty",
        ));
    }
    validate_reference_label(
        definition.meta.span,
        &definition.label,
        &definition.identifier,
        true,
        diagnostics,
    );
    let footnote = definition
        .label
        .strip_prefix('^')
        .is_some_and(is_footnote_label);
    if footnote && depth < MAX_BLOCK_NESTING && !indented {
        diagnostics.push(Diagnostic::invalid(
            definition.meta.span,
            "definition label cannot be `^` followed by a footnote label",
        ));
    }
    validate_destination(
        definition.meta.span,
        &definition.destination,
        definition.destination_kind,
        diagnostics,
    );
}

/// A reference's identifier is the parser's normalization of its label, and
/// a label that is `written` (a definition's, or a full reference's) reads
/// back as a reference label.
fn validate_reference_label(
    span: Option<Span>,
    label: &str,
    identifier: &str,
    written: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if normalize_label(label) != identifier {
        diagnostics.push(Diagnostic::invalid(
            span,
            "reference identifier must be the normalized label",
        ));
    }
    if written && !is_reference_label(label) {
        diagnostics.push(Diagnostic::invalid(
            span,
            "reference label must be one the parser reads as a label",
        ));
    }
}

/// A footnote's label reads back as a footnote label, and its identifier is
/// the parser's normalization of it.
fn validate_footnote_label(
    span: Option<Span>,
    label: &str,
    identifier: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if normalize_label(label) != identifier {
        diagnostics.push(Diagnostic::invalid(
            span,
            "footnote identifier must be the normalized label",
        ));
    }
    if !is_written_footnote_label(label) {
        diagnostics.push(Diagnostic::invalid(
            span,
            "footnote label must be one the parser reads as a footnote label",
        ));
    }
}

/// An empty bare destination is written `<>`, which reads as an
/// angle-bracket one.
fn validate_destination(
    span: Option<Span>,
    destination: &str,
    kind: LinkDestinationKind,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if kind == LinkDestinationKind::Bare && destination.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            span,
            "a bare destination cannot be empty",
        ));
    }
}

/// A code block's info string is never empty, and an indented code block's
/// value, which no fence bounds, starts and ends with a line that is not
/// blank.
fn validate_code_block(code: &CodeBlock, diagnostics: &mut Vec<Diagnostic>) {
    if code.info.as_deref() == Some("") {
        diagnostics.push(Diagnostic::invalid(
            code.meta.span,
            "code block info string cannot be empty",
        ));
    }
    if code.kind == CodeBlockKind::Indented {
        // Each line of the value ends with a line ending, the last one
        // optionally.
        let value = code.value.replace("\r\n", "\n");
        let body = value.strip_suffix(['\n', '\r']).unwrap_or(&value);
        let mut lines = body.split(['\n', '\r']);
        let first = lines.next().unwrap_or_default();
        let last = lines.next_back().unwrap_or(first);
        if is_blank(first) || is_blank(last) {
            diagnostics.push(Diagnostic::invalid(
                code.meta.span,
                "indented code cannot be empty or start or end with a blank line",
            ));
        }
    }
}

/// Frontmatter opens only the document, and ends at the first line that is
/// its fence.
fn validate_frontmatter(
    frontmatter: &Frontmatter,
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if depth > 0 {
        diagnostics.push(Diagnostic::invalid(
            frontmatter.meta.span,
            "frontmatter cannot be inside a container",
        ));
    }
    if frontmatter
        .value
        .replace("\r\n", "\n")
        .split(['\n', '\r'])
        .any(|line| frontmatter_fence_kind(line) == Some(frontmatter.kind))
    {
        diagnostics.push(Diagnostic::invalid(
            frontmatter.meta.span,
            "frontmatter value cannot hold its own fence line",
        ));
    }
}

/// Inline content whose container ends where its line does, or with a closing
/// delimiter that a line break before it keeps from closing: a final hard line
/// break would not survive serialization.
fn validate_inlines(inlines: &[Inline], diagnostics: &mut Vec<Diagnostic>) {
    if let Some(Inline::LineBreak(node)) = inlines.last() {
        diagnostics.push(Diagnostic::invalid(
            node.meta.span,
            "hard line break cannot be the final inline of its container",
        ));
    }
    validate_inline_nodes(inlines, diagnostics);
}

/// Inline content closed by a `]`, which closes after a line break as well, so
/// it may end with a hard line break.
fn validate_inline_nodes(inlines: &[Inline], diagnostics: &mut Vec<Diagnostic>) {
    for (index, inline) in inlines.iter().enumerate() {
        if let Some(before) = index.checked_sub(1).map(|before| &inlines[before]) {
            validate_siblings(before, inline, diagnostics);
        }
        match inline {
            Inline::Text(node) => {
                if node.value.contains(['\n', '\r']) {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "text cannot hold a line ending; a line ends with a soft or hard break",
                    ));
                }
            }
            Inline::Emphasis(_) | Inline::Strong(_) | Inline::Delete(_) | Inline::Mark(_) => {
                validate_emphasis_container(inline, diagnostics)
            }
            Inline::Shortcode(node) => {
                if node.glyph().is_none() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "shortcode name must be a gemoji name",
                    ));
                }
            }
            Inline::Link(node) => {
                validate_destination(
                    node.meta.span,
                    &node.destination,
                    node.destination_kind,
                    diagnostics,
                );
                validate_link_text(&node.children, diagnostics);
                validate_inline_nodes(&node.children, diagnostics);
            }
            Inline::Autolink(node) => {
                if node.destination().is_none() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "autolink text must be exactly one autolink of its form",
                    ));
                }
            }
            Inline::Image(node) => {
                validate_destination(
                    node.meta.span,
                    &node.destination,
                    node.destination_kind,
                    diagnostics,
                );
                validate_inline_nodes(&node.alt, diagnostics);
            }
            Inline::LinkReference(node) => {
                if node.identifier.is_empty() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "link reference identifier cannot be empty",
                    ));
                }
                validate_reference_label(
                    node.meta.span,
                    &node.label,
                    &node.identifier,
                    node.kind == ReferenceKind::Full,
                    diagnostics,
                );
                validate_link_text(&node.children, diagnostics);
                validate_inline_nodes(&node.children, diagnostics);
            }
            Inline::ImageReference(node) => {
                if node.identifier.is_empty() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "image reference identifier cannot be empty",
                    ));
                }
                validate_reference_label(
                    node.meta.span,
                    &node.label,
                    &node.identifier,
                    node.kind == ReferenceKind::Full,
                    diagnostics,
                );
                validate_inline_nodes(&node.alt, diagnostics);
            }
            Inline::Escape(node) => validate_escape(node, diagnostics),
            Inline::CharacterReference(node) => {
                if node.value().is_none() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "character reference must be exactly one character reference",
                    ));
                }
            }
            Inline::TextDirective(node) => validate_text_directive(node, diagnostics),
            Inline::FootnoteReference(node) => {
                if node.identifier.is_empty() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "footnote reference identifier cannot be empty",
                    ));
                }
                validate_footnote_label(node.meta.span, &node.label, &node.identifier, diagnostics);
            }
            Inline::InlineFootnote(node) => {
                if node.children.is_empty() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "inline footnote cannot be empty",
                    ));
                }
                validate_inline_nodes(&node.children, diagnostics)
            }
            Inline::WikiLink(node) => {
                if node.target.is_empty() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "wikilink target cannot be empty",
                    ));
                }
            }
            Inline::Code(node) => validate_code_inline(node, diagnostics),
            Inline::Math(node) => validate_math_inline(node, diagnostics),
            Inline::Html(_) | Inline::SoftBreak(_) | Inline::LineBreak(_) => {}
        }
    }
}

/// Two adjacent inlines whose written forms run together: a break after a
/// break leaves an empty line, which ends the paragraph, unless it is a
/// backslash break, which writes its `\` on that line; and two strikethroughs
/// join their delimiters into `~~~~`, which opens and closes none. Whether
/// two emphasis or two strong spans with one delimiter char read back depends
/// on the chars around their joined run (`**:* ~**a**` parses as two
/// emphasis spans), and two marks join into `====`, which reads as two marks
/// again; neither is rejected.
fn validate_siblings(before: &Inline, inline: &Inline, diagnostics: &mut Vec<Diagnostic>) {
    let after_break = matches!(before, Inline::SoftBreak(_) | Inline::LineBreak(_));
    let blank_line = match inline {
        Inline::SoftBreak(_) => true,
        Inline::LineBreak(node) => node.kind == LineBreakKind::Spaces,
        _ => false,
    };
    if after_break && blank_line {
        diagnostics.push(Diagnostic::invalid(
            inline.span(),
            "a soft break or a spaces hard break cannot follow a line break",
        ));
    }
    if matches!((before, inline), (Inline::Delete(_), Inline::Delete(_))) {
        diagnostics.push(Diagnostic::invalid(
            inline.span(),
            "adjacent strikethroughs cannot be written",
        ));
    }
}

/// An emphasis-like container: it holds content without whitespace at its
/// edges, and no only child whose delimiter runs join its own into a run
/// that reads otherwise: `*` around `*a*` writes `**a**`, which is strong;
/// `**` around `*a*` writes `***a***`, emphasis around strong; `~~` around
/// `~~a~~` writes `~~~~a~~~~`, which strikes nothing; and `==` around `==a==`
/// writes `====a====`, one mark between two `==`. `**` around `**a**` reads
/// back as written.
fn validate_emphasis_container(node: &Inline, diagnostics: &mut Vec<Diagnostic>) {
    let children = node.children();
    let span = node.span();
    if children.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            span,
            "emphasis-like inline container cannot have empty children",
        ));
    } else if starts_with_whitespace(children) || ends_with_whitespace(children) {
        diagnostics.push(Diagnostic::invalid(
            span,
            "emphasis-like inline content cannot start or end with whitespace",
        ));
    }
    let joined = match (node, children) {
        (Inline::Emphasis(outer), [Inline::Emphasis(inner)]) => outer.delimiter == inner.delimiter,
        (Inline::Strong(outer), [Inline::Emphasis(inner)]) => outer.delimiter == inner.delimiter,
        (Inline::Delete(_), [Inline::Delete(_)]) | (Inline::Mark(_), [Inline::Mark(_)]) => true,
        _ => false,
    };
    if joined {
        diagnostics.push(Diagnostic::invalid(
            span,
            "emphasis-like span cannot hold only a span written with the same delimiter",
        ));
    }
    validate_inlines(children, diagnostics);
}

/// Whether inline content starts with a space, a tab, or a line break.
fn starts_with_whitespace(inlines: &[Inline]) -> bool {
    match inlines.first() {
        Some(Inline::Text(text)) => text.value.starts_with([' ', '\t']),
        Some(Inline::SoftBreak(_) | Inline::LineBreak(_)) => true,
        _ => false,
    }
}

/// Whether inline content ends with a space, a tab, or a line break.
fn ends_with_whitespace(inlines: &[Inline]) -> bool {
    match inlines.last() {
        Some(Inline::Text(text)) => text.value.ends_with([' ', '\t']),
        Some(Inline::SoftBreak(_) | Inline::LineBreak(_)) => true,
        _ => false,
    }
}

/// Link text holds no link, at any depth.
fn validate_link_text(inlines: &[Inline], diagnostics: &mut Vec<Diagnostic>) {
    for inline in inlines {
        match inline {
            Inline::Link(_)
            | Inline::Autolink(_)
            | Inline::LinkReference(_)
            | Inline::WikiLink(_) => {
                diagnostics.push(Diagnostic::invalid(
                    inline.span(),
                    "link text cannot hold a link",
                ));
            }
            _ => validate_link_text(inline.children(), diagnostics),
        }
    }
}

fn validate_escape(escape: &Escape, diagnostics: &mut Vec<Diagnostic>) {
    if !escape.value.is_ascii_punctuation() {
        diagnostics.push(Diagnostic::invalid(
            escape.meta.span,
            "escaped value must be an ASCII punctuation character",
        ));
    }
}

/// A code span's value is what the parser reads from one: never empty, and
/// with its line endings read as spaces.
fn validate_code_inline(code: &CodeInline, diagnostics: &mut Vec<Diagnostic>) {
    if code.value.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            code.meta.span,
            "inline code value cannot be empty",
        ));
    }
    if code.value.contains(['\n', '\r']) {
        diagnostics.push(Diagnostic::invalid(
            code.meta.span,
            "inline code value cannot hold a line ending",
        ));
    }
}

/// Inline math that its fence closes around: math needs a value (both `$$`
/// and `` $`` `$ `` read back as other text), and code-form math ends at the
/// first `` `$ ``, so its value cannot hold one.
fn validate_math_inline(math: &MathInline, diagnostics: &mut Vec<Diagnostic>) {
    if math.value.is_empty() {
        diagnostics.push(Diagnostic::invalid(
            math.meta.span,
            "inline math value cannot be empty",
        ));
    }
    match math.kind {
        MathInlineKind::Dollar { dollars: 0 } => diagnostics.push(Diagnostic::invalid(
            math.meta.span,
            "dollar-fenced inline math must have a fence length of at least 1",
        )),
        MathInlineKind::Code if math.value.contains("`$") => diagnostics.push(Diagnostic::invalid(
            math.meta.span,
            "code-form inline math value cannot contain a backtick followed by `$`",
        )),
        _ => {}
    }
}

fn validate_list_start(list: &List, diagnostics: &mut Vec<Diagnostic>) {
    if !list.ordered {
        return;
    }
    let Some(start) = list.start else {
        return;
    };
    if start > crate::parse::MAX_ORDERED_NUMBER {
        diagnostics.push(Diagnostic::invalid(
            list.meta.span,
            "ordered list start must be representable in at most 9 digits",
        ));
    }
}

fn validate_text_directive(directive: &TextDirective, diagnostics: &mut Vec<Diagnostic>) {
    validate_directive_name(directive.meta.span, &directive.name, diagnostics);
    validate_directive_attributes(&directive.attributes, diagnostics);
    validate_inline_nodes(&directive.label, diagnostics);
}

fn validate_directive_name(span: Option<Span>, name: &str, diagnostics: &mut Vec<Diagnostic>) {
    if !is_directive_name(name) {
        diagnostics.push(Diagnostic::invalid(
            span,
            "directive name must be runs of ASCII letters joined by single `-`",
        ));
    }
}

fn validate_directive_attributes(
    attributes: &[DirectiveAttribute],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for attribute in attributes {
        if !is_attribute_name(&attribute.name) {
            diagnostics.push(Diagnostic::invalid(
                None,
                "directive attribute name must start with a letter, `_`, or `-`",
            ));
        }
    }
}

/// Whether `name` is one or more runs of ASCII letters joined by single `-`.
pub(crate) fn is_directive_name(name: &str) -> bool {
    name.split('-')
        .all(|run| !run.is_empty() && run.bytes().all(|byte| byte.is_ascii_alphabetic()))
}

pub(crate) fn is_attribute_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '-') {
        return false;
    }
    chars.all(|char| char.is_ascii_alphanumeric() || char == '_' || char == '-' || char == ':')
}
