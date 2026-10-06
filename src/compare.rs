//! The tree comparison serialization reads back with. Two trees are the same
//! when they differ only in spans, in text one holds as an `Escape` or a
//! `CharacterReference` and the other as plain text, and in where adjacent
//! `Text` nodes split: the parser cannot tell an author's escape from one the
//! serializer writes, so an escaped text char always reads back as an
//! `Escape`. The matches are exhaustive so a new node kind cannot skip the
//! normalization.

use alloc::string::String;
use alloc::vec::Vec;

use crate::ast::*;

/// `inlines` normalized for comparison: spans cleared, escapes and character
/// references read as text, and adjacent text merged.
pub(crate) fn normalized_inlines(inlines: &[Inline]) -> Vec<Inline> {
    let mut inlines = inlines.to_vec();
    normalize_inlines(&mut inlines);
    inlines
}

/// `blocks` normalized for comparison, as [`normalized_inlines`] does for
/// inline content.
pub fn normalized_blocks(blocks: &[Block]) -> Vec<Block> {
    let mut blocks = blocks.to_vec();
    normalize_blocks(&mut blocks, false);
    blocks
}

/// `blocks` normalized as [`normalized_blocks`] does, and also apart from
/// what the serializer chooses for them: list markers, code fences, the last
/// line ending of a code or math block, heading forms, thematic break markers,
/// and a code span's fence and raw text.
pub(crate) fn layout_normalized_blocks(blocks: &[Block]) -> Vec<Block> {
    let mut blocks = blocks.to_vec();
    normalize_blocks(&mut blocks, true);
    blocks
}

fn clear(meta: &mut NodeMeta) {
    meta.span = None;
}

fn normalize_blocks(blocks: &mut [Block], layout: bool) {
    let inlines = |inlines: &mut Vec<Inline>| {
        normalize_inlines(inlines);
        if layout {
            clear_code_fences(inlines);
        }
    };
    for block in blocks {
        match block {
            Block::Paragraph(node) => {
                clear(&mut node.meta);
                inlines(&mut node.children);
            }
            Block::Heading(node) => {
                clear(&mut node.meta);
                inlines(&mut node.children);
                if layout {
                    node.kind = HeadingKind::Atx;
                }
            }
            Block::ThematicBreak(node) => {
                clear(&mut node.meta);
                if layout {
                    node.marker = ThematicBreakMarker::Dash;
                }
            }
            Block::BlockQuote(node) => {
                clear(&mut node.meta);
                normalize_blocks(&mut node.children, layout);
            }
            Block::Alert(node) => {
                clear(&mut node.meta);
                normalize_blocks(&mut node.children, layout);
            }
            Block::List(node) => {
                clear(&mut node.meta);
                if layout {
                    node.delimiter = ListDelimiter::Dash;
                }
                for item in &mut node.children {
                    clear(&mut item.meta);
                    normalize_blocks(&mut item.children, layout);
                }
            }
            Block::DescriptionList(node) => {
                clear(&mut node.meta);
                for item in &mut node.children {
                    clear(&mut item.meta);
                    inlines(&mut item.term);
                    for details in &mut item.details {
                        clear(&mut details.meta);
                        normalize_blocks(&mut details.children, layout);
                    }
                }
            }
            Block::CodeBlock(node) => {
                clear(&mut node.meta);
                if layout {
                    // A code block's lines end with a line ending, which a
                    // value may leave out.
                    node.kind = CodeBlockKind::Indented;
                    if node.value.ends_with('\n') {
                        node.value.pop();
                    }
                }
            }
            Block::HtmlBlock(node) => clear(&mut node.meta),
            Block::HtmlContainer(node) => {
                clear(&mut node.meta);
                clear(&mut node.opening.meta);
                clear(&mut node.closing.meta);
                match &mut node.content {
                    HtmlContainerContent::Blocks(children) => normalize_blocks(children, layout),
                    HtmlContainerContent::Inlines(children) => inlines(children),
                }
            }
            Block::Definition(node) => clear(&mut node.meta),
            Block::FootnoteDefinition(node) => {
                clear(&mut node.meta);
                normalize_blocks(&mut node.children, layout);
            }
            Block::Table(node) => {
                clear(&mut node.meta);
                for row in &mut node.rows {
                    clear(&mut row.meta);
                    for cell in &mut row.cells {
                        clear(&mut cell.meta);
                        inlines(&mut cell.children);
                    }
                }
            }
            Block::MathBlock(node) => {
                clear(&mut node.meta);
                if layout && node.value.ends_with('\n') {
                    node.value.pop();
                }
            }
            Block::Frontmatter(node) => clear(&mut node.meta),
            Block::MdxEsm(node) => clear(&mut node.meta),
            Block::MdxExpression(node) => clear(&mut node.meta),
            Block::MdxJsx(node) => clear(&mut node.meta),
            Block::LeafDirective(node) => {
                clear(&mut node.meta);
                inlines(&mut node.label);
            }
            Block::ContainerDirective(node) => {
                clear(&mut node.meta);
                inlines(&mut node.label);
                normalize_blocks(&mut node.children, layout);
            }
        }
    }
}

/// Appends the text `inline` reads as when it is text, as the comparison
/// reads a `Text`, an `Escape`, or a `CharacterReference`. Whether it is.
pub(crate) fn push_text(inline: &Inline, out: &mut String) -> bool {
    match inline {
        Inline::Text(node) => out.push_str(&node.value),
        Inline::Escape(node) => out.push(node.value),
        Inline::CharacterReference(node) => out.push_str(&node.value),
        _ => return false,
    }
    true
}

fn normalize_inlines(inlines: &mut Vec<Inline>) {
    let mut normalized = Vec::with_capacity(inlines.len());
    for mut inline in inlines.drain(..) {
        let text = match &mut inline {
            Inline::Text(_) | Inline::Escape(_) | Inline::CharacterReference(_) => {
                let mut value = String::new();
                push_text(&inline, &mut value);
                Some(value)
            }
            Inline::SoftBreak(node) => {
                clear(&mut node.meta);
                None
            }
            Inline::LineBreak(node) => {
                clear(&mut node.meta);
                None
            }
            Inline::Emphasis(node) => children(&mut node.meta, &mut node.children),
            Inline::Strong(node) => children(&mut node.meta, &mut node.children),
            Inline::Underline(node) => children(&mut node.meta, &mut node.children),
            Inline::Delete(node) => children(&mut node.meta, &mut node.children),
            Inline::Insert(node) => children(&mut node.meta, &mut node.children),
            Inline::Mark(node) => children(&mut node.meta, &mut node.children),
            Inline::Subscript(node) => children(&mut node.meta, &mut node.children),
            Inline::Superscript(node) => children(&mut node.meta, &mut node.children),
            Inline::Spoiler(node) => children(&mut node.meta, &mut node.children),
            Inline::InlineFootnote(node) => children(&mut node.meta, &mut node.children),
            Inline::Link(node) => children(&mut node.meta, &mut node.children),
            Inline::Image(node) => children(&mut node.meta, &mut node.alt),
            Inline::LinkReference(node) => children(&mut node.meta, &mut node.children),
            Inline::ImageReference(node) => children(&mut node.meta, &mut node.alt),
            Inline::TextDirective(node) => children(&mut node.meta, &mut node.label),
            Inline::Shortcode(node) => leaf(&mut node.meta),
            Inline::Code(node) => leaf(&mut node.meta),
            Inline::Html(node) => leaf(&mut node.meta),
            Inline::Math(node) => leaf(&mut node.meta),
            Inline::FootnoteReference(node) => leaf(&mut node.meta),
            Inline::WikiLink(node) => leaf(&mut node.meta),
            Inline::MdxExpression(node) => leaf(&mut node.meta),
            Inline::MdxJsx(node) => leaf(&mut node.meta),
        };
        match text {
            Some(value) => match normalized.last_mut() {
                Some(Inline::Text(last)) => last.value.push_str(&value),
                _ => normalized.push(Inline::Text(Text {
                    meta: NodeMeta::default(),
                    value,
                })),
            },
            None => normalized.push(inline),
        }
    }
    *inlines = normalized;
}

/// Whether two nodes are the same kind with the same values, apart from
/// spans and children: what the serializer's read-back blames a node for.
/// A code span without its source fence compares by value, since it is
/// written from its value.
pub(crate) fn same_node(a: &Inline, b: &Inline) -> bool {
    match (a, b) {
        (Inline::Emphasis(_), Inline::Emphasis(_))
        | (Inline::Strong(_), Inline::Strong(_))
        | (Inline::Underline(_), Inline::Underline(_))
        | (Inline::Insert(_), Inline::Insert(_))
        | (Inline::Mark(_), Inline::Mark(_))
        | (Inline::Subscript(_), Inline::Subscript(_))
        | (Inline::Superscript(_), Inline::Superscript(_))
        | (Inline::Spoiler(_), Inline::Spoiler(_))
        | (Inline::InlineFootnote(_), Inline::InlineFootnote(_)) => true,
        (Inline::Delete(a), Inline::Delete(b)) => a.marker == b.marker,
        (Inline::Link(a), Inline::Link(b)) => {
            a.destination == b.destination
                && a.destination_kind == b.destination_kind
                && a.title == b.title
                && a.title_kind == b.title_kind
        }
        (Inline::Image(a), Inline::Image(b)) => {
            a.destination == b.destination
                && a.destination_kind == b.destination_kind
                && a.title == b.title
                && a.title_kind == b.title_kind
        }
        (Inline::LinkReference(a), Inline::LinkReference(b)) => {
            a.kind == b.kind && a.identifier == b.identifier && a.label == b.label
        }
        (Inline::ImageReference(a), Inline::ImageReference(b)) => {
            a.kind == b.kind && a.identifier == b.identifier && a.label == b.label
        }
        (Inline::TextDirective(a), Inline::TextDirective(b)) => {
            a.name == b.name && a.attributes == b.attributes
        }
        // A code span without its source fence is written from its value.
        (Inline::Code(a), Inline::Code(b)) => {
            a.value == b.value
                && (a.fence_length == 0
                    || a.raw.is_empty()
                    || (a.raw == b.raw && a.fence_length == b.fence_length))
        }
        // Nodes without children compare whole.
        (a, b) if is_leaf(a) => {
            core::mem::discriminant(a) == core::mem::discriminant(b)
                && normalized_inlines(core::slice::from_ref(a))
                    == normalized_inlines(core::slice::from_ref(b))
        }
        _ => false,
    }
}

/// Whether `inline` holds no inline children.
fn is_leaf(inline: &Inline) -> bool {
    matches!(
        inline,
        Inline::Text(_)
            | Inline::Escape(_)
            | Inline::CharacterReference(_)
            | Inline::SoftBreak(_)
            | Inline::LineBreak(_)
            | Inline::Shortcode(_)
            | Inline::Code(_)
            | Inline::Html(_)
            | Inline::Math(_)
            | Inline::FootnoteReference(_)
            | Inline::WikiLink(_)
            | Inline::MdxExpression(_)
            | Inline::MdxJsx(_)
    )
}

/// Clears what a code span's writing chooses, so that it compares by value.
fn clear_code_fences(inlines: &mut [Inline]) {
    for inline in inlines {
        match inline {
            Inline::Code(node) => {
                node.raw.clear();
                node.fence_length = 0;
            }
            Inline::Emphasis(node) => clear_code_fences(&mut node.children),
            Inline::Strong(node) => clear_code_fences(&mut node.children),
            Inline::Underline(node) => clear_code_fences(&mut node.children),
            Inline::Delete(node) => clear_code_fences(&mut node.children),
            Inline::Insert(node) => clear_code_fences(&mut node.children),
            Inline::Mark(node) => clear_code_fences(&mut node.children),
            Inline::Subscript(node) => clear_code_fences(&mut node.children),
            Inline::Superscript(node) => clear_code_fences(&mut node.children),
            Inline::Spoiler(node) => clear_code_fences(&mut node.children),
            Inline::InlineFootnote(node) => clear_code_fences(&mut node.children),
            Inline::Link(node) => clear_code_fences(&mut node.children),
            Inline::Image(node) => clear_code_fences(&mut node.alt),
            Inline::LinkReference(node) => clear_code_fences(&mut node.children),
            Inline::ImageReference(node) => clear_code_fences(&mut node.alt),
            Inline::TextDirective(node) => clear_code_fences(&mut node.label),
            Inline::Text(_)
            | Inline::Escape(_)
            | Inline::CharacterReference(_)
            | Inline::SoftBreak(_)
            | Inline::LineBreak(_)
            | Inline::Shortcode(_)
            | Inline::Html(_)
            | Inline::Math(_)
            | Inline::FootnoteReference(_)
            | Inline::WikiLink(_)
            | Inline::MdxExpression(_)
            | Inline::MdxJsx(_) => {}
        }
    }
}

fn children(meta: &mut NodeMeta, children: &mut Vec<Inline>) -> Option<String> {
    clear(meta);
    normalize_inlines(children);
    None
}

fn leaf(meta: &mut NodeMeta) -> Option<String> {
    clear(meta);
    None
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn text(value: &str) -> Inline {
        Text::from(value).into()
    }

    #[test]
    fn escapes_and_references_read_as_text() {
        let escaped = vec![
            Inline::Escape(Escape {
                meta: NodeMeta::new(Some(crate::Span::new(0, 2))),
                value: '*',
            }),
            text("a"),
            Inline::CharacterReference(CharacterReference {
                meta: NodeMeta::default(),
                reference: "#42;".into(),
                value: "*".into(),
            }),
        ];
        assert_eq!(normalized_inlines(&escaped), vec![text("*a*")]);
    }

    #[test]
    fn adjacent_text_merges_inside_containers() {
        let split = vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::new(Some(crate::Span::new(0, 4))),
            children: vec![text("a"), text("b")],
        })];
        let whole = vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            children: vec![text("ab")],
        })];
        assert_eq!(normalized_inlines(&split), whole);
    }

    #[test]
    fn block_spans_are_cleared() {
        let parsed = crate::parse("> a\\*b").document.children;
        let built: Block = BlockQuote {
            meta: NodeMeta::default(),
            children: vec![Paragraph::new([Text::from("a*b")]).into()],
        }
        .into();
        assert_eq!(normalized_blocks(&parsed), normalized_blocks(&[built]));
    }
}
