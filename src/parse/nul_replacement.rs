//! Rewrites U+0000 to U+FFFD in every string of a parsed document. The parser
//! recognizes structure on the original input (reading `\0` as U+FFFD where the
//! two classify differently), so spans stay in source coordinates; this pass
//! makes node values match what CommonMark's replacement would have produced.
//! The matches are exhaustive so a new node kind cannot skip the rewrite.

use alloc::string::String;

use crate::ast::*;

pub(super) fn replace_in_document(document: &mut Document) {
    blocks(&mut document.children);
}

fn string(value: &mut String) {
    if value.contains('\0') {
        *value = value.replace('\0', "\u{FFFD}");
    }
}

fn optional(value: &mut Option<String>) {
    if let Some(value) = value {
        string(value);
    }
}

fn title(title: &mut Option<Title>) {
    if let Some(title) = title {
        string(&mut title.value);
    }
}

fn attributes(attributes: &mut [DirectiveAttribute]) {
    for attribute in attributes {
        string(&mut attribute.name);
        optional(&mut attribute.value);
    }
}

fn tag(tag: &mut HtmlTag) {
    string(&mut tag.name);
    string(&mut tag.raw);
}

fn blocks(blocks: &mut [Block]) {
    for block in blocks {
        match block {
            Block::Paragraph(node) => inlines(&mut node.children),
            Block::Heading(node) => inlines(&mut node.children),
            Block::ThematicBreak(_) => {}
            Block::BlockQuote(node) => self::blocks(&mut node.children),
            Block::Alert(node) => {
                optional(&mut node.title);
                self::blocks(&mut node.children);
            }
            Block::List(node) => {
                for item in &mut node.children {
                    self::blocks(&mut item.children);
                }
            }
            Block::CodeBlock(node) => {
                optional(&mut node.info);
                string(&mut node.value);
            }
            Block::HtmlBlock(node) => string(&mut node.value),
            Block::HtmlContainer(node) => {
                tag(&mut node.opening);
                tag(&mut node.closing);
                match &mut node.content {
                    HtmlContainerContent::Blocks(children) => self::blocks(children),
                    HtmlContainerContent::Inlines(children) => inlines(children),
                }
            }
            Block::Definition(node) => {
                string(&mut node.label);
                string(&mut node.identifier);
                string(&mut node.destination);
                title(&mut node.title);
            }
            Block::FootnoteDefinition(node) => {
                string(&mut node.label);
                string(&mut node.identifier);
                self::blocks(&mut node.children);
            }
            Block::Table(node) => {
                for row in &mut node.rows {
                    for cell in &mut row.cells {
                        inlines(&mut cell.children);
                    }
                }
            }
            Block::MathBlock(node) => string(&mut node.value),
            Block::Frontmatter(node) => string(&mut node.value),
            Block::LeafDirective(node) => {
                string(&mut node.name);
                inlines(&mut node.label);
                attributes(&mut node.attributes);
            }
            Block::ContainerDirective(node) => {
                string(&mut node.name);
                inlines(&mut node.label);
                attributes(&mut node.attributes);
                self::blocks(&mut node.children);
            }
        }
    }
}

fn inlines(inlines: &mut [Inline]) {
    for inline in inlines {
        match inline {
            Inline::Text(node) => string(&mut node.value),
            Inline::Escape(_) | Inline::SoftBreak(_) | Inline::LineBreak(_) => {}
            Inline::CharacterReference(node) => string(&mut node.reference),
            Inline::Emphasis(node) => self::inlines(&mut node.children),
            Inline::Strong(node) => self::inlines(&mut node.children),
            Inline::Delete(node) => self::inlines(&mut node.children),
            Inline::Mark(node) => self::inlines(&mut node.children),
            Inline::InlineFootnote(node) => self::inlines(&mut node.children),
            Inline::Shortcode(node) => string(&mut node.name),
            Inline::Code(node) => string(&mut node.value),
            Inline::Link(node) => {
                string(&mut node.destination);
                title(&mut node.title);
                self::inlines(&mut node.children);
            }
            Inline::Autolink(node) => string(&mut node.text),
            Inline::Image(node) => {
                string(&mut node.destination);
                title(&mut node.title);
                self::inlines(&mut node.alt);
            }
            Inline::LinkReference(node) => {
                string(&mut node.identifier);
                string(&mut node.label);
                self::inlines(&mut node.children);
            }
            Inline::ImageReference(node) => {
                string(&mut node.identifier);
                string(&mut node.label);
                self::inlines(&mut node.alt);
            }
            Inline::Html(node) => string(&mut node.value),
            Inline::Math(node) => string(&mut node.value),
            Inline::FootnoteReference(node) => {
                string(&mut node.label);
                string(&mut node.identifier);
            }
            Inline::WikiLink(node) => {
                string(&mut node.target);
                string(&mut node.label);
            }
            Inline::TextDirective(node) => {
                string(&mut node.name);
                self::inlines(&mut node.label);
                attributes(&mut node.attributes);
            }
        }
    }
}
