//! AST validation: [`Document::validate`] walks the tree and reports each
//! invalid or unsupported node shape as a [`Diagnostic`]. Serialization and HTML
//! rendering run this first and refuse an invalid document.

use alloc::vec::Vec;

use crate::{
    ast::{
        Block, CodeInline, ContainerDirective, DirectiveAttribute, Document, Escape, Heading,
        HtmlContainer, HtmlContainerContent, Inline, LeafDirective, Link, LinkDestinationKind,
        LinkForm, List, MathInline, MathInlineKind, Table, TextDirective,
    },
    diagnostic::Diagnostic,
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
    validate_blocks(&document.children, &mut diagnostics);
    diagnostics
}

/// A sequence of sibling blocks: each block, and two adjacent lists that one
/// marker would write as one list.
fn validate_blocks(blocks: &[Block], diagnostics: &mut Vec<Diagnostic>) {
    for (index, block) in blocks.iter().enumerate() {
        if let (Some(Block::List(before)), Block::List(list)) =
            (index.checked_sub(1).map(|before| &blocks[before]), block)
        {
            if before.ordered == list.ordered && before.delimiter == list.delimiter {
                diagnostics.push(Diagnostic::invalid(
                    list.meta.span,
                    "adjacent lists cannot use the same marker",
                ));
            }
        }
        validate_block(block, diagnostics);
    }
}

fn validate_block(block: &Block, diagnostics: &mut Vec<Diagnostic>) {
    match block {
        Block::Paragraph(paragraph) => validate_inlines(&paragraph.children, diagnostics),
        Block::Heading(heading) => validate_heading(heading, diagnostics),
        Block::BlockQuote(block_quote) => {
            validate_blocks(&block_quote.children, diagnostics);
        }
        Block::Alert(alert) => {
            validate_blocks(&alert.children, diagnostics);
        }
        Block::List(list) => {
            validate_list_start(list, diagnostics);
            for item in &list.children {
                validate_blocks(&item.children, diagnostics);
            }
        }
        Block::Table(table) => validate_table(table, diagnostics),
        Block::FootnoteDefinition(definition) => {
            if definition.identifier.is_empty() {
                diagnostics.push(Diagnostic::invalid(
                    definition.meta.span,
                    "footnote definition identifier cannot be empty",
                ));
            }
            validate_blocks(&definition.children, diagnostics);
        }
        Block::Definition(definition) => {
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
        }
        Block::LeafDirective(directive) => validate_leaf_directive(directive, diagnostics),
        Block::ContainerDirective(directive) => {
            validate_container_directive(directive, diagnostics)
        }
        Block::HtmlContainer(container) => validate_html_container(container, diagnostics),
        Block::ThematicBreak(_)
        | Block::CodeBlock(_)
        | Block::HtmlBlock(_)
        | Block::MathBlock(_)
        | Block::Frontmatter(_) => {}
    }
}

fn validate_html_container(container: &HtmlContainer, diagnostics: &mut Vec<Diagnostic>) {
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
            validate_blocks(children, diagnostics);
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
        }
    }
}

fn validate_leaf_directive(directive: &LeafDirective, diagnostics: &mut Vec<Diagnostic>) {
    validate_directive_name(directive.meta.span, &directive.name, diagnostics);
    validate_directive_attributes(&directive.attributes, diagnostics);
    validate_inlines(&directive.label, diagnostics);
}

fn validate_container_directive(directive: &ContainerDirective, diagnostics: &mut Vec<Diagnostic>) {
    validate_directive_name(directive.meta.span, &directive.name, diagnostics);
    validate_directive_attributes(&directive.attributes, diagnostics);
    validate_inlines(&directive.label, diagnostics);
    validate_blocks(&directive.children, diagnostics);
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
    for inline in inlines {
        match inline {
            Inline::Emphasis(node) => {
                validate_emphasis_container(&node.children, node.meta.span, diagnostics)
            }
            Inline::Strong(node) => {
                validate_emphasis_container(&node.children, node.meta.span, diagnostics)
            }
            Inline::Delete(node) => {
                validate_emphasis_container(&node.children, node.meta.span, diagnostics)
            }
            Inline::Mark(node) => {
                validate_emphasis_container(&node.children, node.meta.span, diagnostics)
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
                validate_link_form(node, diagnostics);
                validate_link_text(&node.children, diagnostics);
                validate_inline_nodes(&node.children, diagnostics);
            }
            Inline::Image(node) => validate_inline_nodes(&node.alt, diagnostics),
            Inline::LinkReference(node) => {
                if node.identifier.is_empty() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "link reference identifier cannot be empty",
                    ));
                }
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
                validate_inline_nodes(&node.alt, diagnostics);
            }
            Inline::Escape(node) => validate_escape(node, diagnostics),
            Inline::CharacterReference(node) => {
                if node.reference.is_empty() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "character reference source cannot be empty",
                    ));
                }
                if node.value.is_empty() {
                    diagnostics.push(Diagnostic::invalid(
                        node.meta.span,
                        "character reference value cannot be empty",
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
            }
            Inline::InlineFootnote(node) => validate_inline_nodes(&node.children, diagnostics),
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
            Inline::Text(_) | Inline::Html(_) | Inline::SoftBreak(_) | Inline::LineBreak(_) => {}
        }
    }
}

fn validate_emphasis_container(
    children: &[Inline],
    span: Option<Span>,
    diagnostics: &mut Vec<Diagnostic>,
) {
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
            Inline::Link(_) | Inline::LinkReference(_) | Inline::WikiLink(_) => {
                diagnostics.push(Diagnostic::invalid(
                    inline.span(),
                    "link text cannot hold a link",
                ));
            }
            _ => validate_link_text(inline.children(), diagnostics),
        }
    }
}

/// A link recorded as an autolink holds the one text that autolink writes
/// for its destination, and no title.
fn validate_link_form(link: &Link, diagnostics: &mut Vec<Diagnostic>) {
    let fits = match link.form {
        LinkForm::Inline => true,
        LinkForm::AngleAutolink => autolink_text(link).is_some_and(|text| {
            crate::parse::angle_autolink_destination(text).as_deref()
                == Some(link.destination.as_str())
        }),
        LinkForm::LiteralAutolink => {
            autolink_text(link).is_some_and(|text| literal_autolink_writes(text, &link.destination))
        }
    };
    if !fits {
        diagnostics.push(Diagnostic::invalid(
            link.meta.span,
            "an autolink must hold the one text that writes its destination, and no title",
        ));
    }
}

/// Whether a literal autolink written as `text` links to `destination`: a
/// URL or a `mailto:` / `xmpp:` address links to itself, a `www.` domain to
/// it after `http://`, and an email address to it after `mailto:`.
fn literal_autolink_writes(text: &str, destination: &str) -> bool {
    let starts_with = |prefix: &str| {
        text.get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    };
    if ["http://", "https://", "mailto:", "xmpp:"]
        .iter()
        .any(|prefix| starts_with(prefix))
    {
        return destination == text;
    }
    if starts_with("www") {
        return destination.strip_prefix("http://") == Some(text);
    }
    text.contains('@') && destination.strip_prefix("mailto:") == Some(text)
}

/// The one text an autolink holds, when it has no title.
fn autolink_text(link: &Link) -> Option<&str> {
    match link.children.as_slice() {
        [Inline::Text(text)]
            if link.title.is_none() && link.destination_kind == LinkDestinationKind::Bare =>
        {
            Some(&text.value)
        }
        _ => None,
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

fn validate_code_inline(code: &CodeInline, diagnostics: &mut Vec<Diagnostic>) {
    if code.fence_length == 0 {
        return;
    }
    // A code span fence of length N is closed only by a backtick run of exactly
    // length N. A run shorter or longer than the fence is inert, so only an
    // exactly-matching interior run would close the raw passthrough early.
    if raw_has_backtick_run(&code.raw, code.fence_length) {
        diagnostics.push(Diagnostic::invalid(
            code.meta.span,
            "inline code raw passthrough contains a backtick run equal to its fence length",
        ));
    }
}

fn raw_has_backtick_run(input: &str, length: usize) -> bool {
    let mut current = 0;
    for byte in input.bytes() {
        if byte == b'`' {
            current += 1;
        } else {
            if current == length {
                return true;
            }
            current = 0;
        }
    }
    current == length
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
    if start > 999_999_999 {
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
