//! The encoding a tree crosses the WebAssembly boundary in, both ways. It is
//! private to this crate and `js/index.js`, which mirror each other.
//!
//! A message is a header of two `u32`, the word count and the string count,
//! then the words, then a `(start, length)` pair of `u32` for each string,
//! then the strings themselves as one run of UTF-16 code units. A node is its
//! kind, its span as two UTF-16 offsets (`NO_SPAN` and 0 for none), and then
//! its fields in the order of its Rust struct; a list of nodes is its length
//! followed by the nodes.

use markdown_syntax::prelude::*;
use markdown_syntax::{
    AlertKind, AutolinkForm, CodeBlockKind, DirectiveAttribute, EmphasisDelimiter, FenceMarker,
    FrontmatterKind, HeadingKind, HtmlContainerContent, HtmlTag, LineBreakKind,
    LinkDestinationKind, LinkTitleKind, ListDelimiter, MathInlineKind, ReferenceKind,
    TableAlignment, ThematicBreakMarker, Title,
};

/// The start word of a node without a span.
pub const NO_SPAN: u32 = u32::MAX;

pub mod kind {
    pub const DOCUMENT: u32 = 0;
    pub const PARAGRAPH: u32 = 1;
    pub const HEADING: u32 = 2;
    pub const THEMATIC_BREAK: u32 = 3;
    pub const BLOCK_QUOTE: u32 = 4;
    pub const ALERT: u32 = 5;
    pub const LIST: u32 = 6;
    pub const CODE_BLOCK: u32 = 7;
    pub const HTML_BLOCK: u32 = 8;
    pub const HTML_CONTAINER: u32 = 9;
    pub const DEFINITION: u32 = 10;
    pub const FOOTNOTE_DEFINITION: u32 = 11;
    pub const TABLE: u32 = 12;
    pub const MATH_BLOCK: u32 = 13;
    pub const FRONTMATTER: u32 = 14;
    pub const LEAF_DIRECTIVE: u32 = 15;
    pub const CONTAINER_DIRECTIVE: u32 = 16;
    pub const BLANK_LINE: u32 = 17;
    pub const LIST_ITEM: u32 = 30;
    pub const TABLE_ROW: u32 = 31;
    pub const TABLE_CELL: u32 = 32;
    pub const TEXT: u32 = 40;
    pub const ESCAPE: u32 = 41;
    pub const CHARACTER_REFERENCE: u32 = 42;
    pub const EMPHASIS: u32 = 43;
    pub const STRONG: u32 = 44;
    pub const DELETE: u32 = 45;
    pub const MARK: u32 = 46;
    pub const SHORTCODE: u32 = 47;
    pub const CODE: u32 = 48;
    pub const LINK: u32 = 49;
    pub const AUTOLINK: u32 = 50;
    pub const IMAGE: u32 = 51;
    pub const LINK_REFERENCE: u32 = 52;
    pub const IMAGE_REFERENCE: u32 = 53;
    pub const HTML: u32 = 54;
    pub const SOFT_BREAK: u32 = 55;
    pub const LINE_BREAK: u32 = 56;
    pub const MATH: u32 = 57;
    pub const FOOTNOTE_REFERENCE: u32 = 58;
    pub const INLINE_FOOTNOTE: u32 = 59;
    pub const WIKI_LINK: u32 = 60;
    pub const TEXT_DIRECTIVE: u32 = 61;
    pub const TAG: u32 = 62;
}

/// For each byte offset of `text`, its UTF-16 offset; `text.len()` included.
pub fn utf16_offsets(text: &str) -> Vec<u32> {
    let mut offsets = vec![0; text.len() + 1];
    let mut at = 0u32;
    for (index, char) in text.char_indices() {
        for offset in &mut offsets[index..index + char.len_utf8()] {
            *offset = at;
        }
        at += char.len_utf16() as u32;
    }
    offsets[text.len()] = at;
    offsets
}

/// For each UTF-16 offset of `text`, its byte offset, or `NO_SPAN` between
/// the two halves of a surrogate pair; the end included.
pub fn byte_offsets(text: &str) -> Vec<u32> {
    let mut offsets = Vec::with_capacity(text.len() + 1);
    for (index, char) in text.char_indices() {
        offsets.push(index as u32);
        if char.len_utf16() == 2 {
            offsets.push(NO_SPAN);
        }
    }
    offsets.push(text.len() as u32);
    offsets
}

/// Builds a message.
pub struct Writer<'t> {
    words: Vec<u32>,
    strings: Vec<(u32, u32)>,
    units: Vec<u16>,
    /// The UTF-16 offset of each byte offset spans count in.
    offsets: &'t [u32],
}

impl<'t> Writer<'t> {
    pub fn new(offsets: &'t [u32]) -> Self {
        Self {
            words: Vec::new(),
            strings: Vec::new(),
            units: Vec::new(),
            offsets,
        }
    }

    pub fn word(&mut self, word: u32) {
        self.words.push(word);
    }

    pub fn bool(&mut self, value: bool) {
        self.word(u32::from(value));
    }

    pub fn str(&mut self, value: &str) {
        let start = self.units.len() as u32;
        self.units.extend(value.encode_utf16());
        let length = self.units.len() as u32 - start;
        self.word(self.strings.len() as u32);
        self.strings.push((start, length));
    }

    /// 0 for `None`, else the string's index plus one.
    pub fn opt_str(&mut self, value: Option<&str>) {
        match value {
            None => self.word(0),
            Some(value) => {
                self.str(value);
                *self.words.last_mut().expect("just pushed") += 1;
            }
        }
    }

    pub fn span(&mut self, span: Option<Span>) {
        let mapped = span.and_then(|span| {
            let start = *self.offsets.get(span.start)?;
            let end = *self.offsets.get(span.end)?;
            Some((start, end))
        });
        match mapped {
            Some((start, end)) => {
                self.word(start);
                self.word(end);
            }
            None => {
                self.word(NO_SPAN);
                self.word(0);
            }
        }
    }

    fn head(&mut self, kind: u32, meta: &NodeMeta) {
        self.word(kind);
        self.span(meta.span);
    }

    pub fn document(&mut self, document: &Document) {
        self.head(kind::DOCUMENT, &document.meta);
        self.blocks(&document.children);
    }

    pub fn blocks(&mut self, blocks: &[Block]) {
        self.word(blocks.len() as u32);
        for block in blocks {
            self.block(block);
        }
    }

    pub fn inlines(&mut self, inlines: &[Inline]) {
        self.word(inlines.len() as u32);
        for inline in inlines {
            self.inline(inline);
        }
    }

    /// A leaf's inline content: the source it was built from, if any, and
    /// its inlines.
    fn leaf(&mut self, source: Option<&str>, inlines: &[Inline]) {
        self.opt_str(source);
        self.inlines(inlines);
    }

    fn title(&mut self, title: &Option<Title>) {
        match title {
            None => self.word(0),
            Some(Title { value, kind }) => {
                self.word(1);
                self.str(value);
                self.word(*kind as u32);
            }
        }
    }

    fn tag(&mut self, tag: &HtmlTag) {
        let HtmlTag { meta, name, raw } = tag;
        self.span(meta.span);
        self.str(name);
        self.str(raw);
    }

    fn attributes(&mut self, attributes: &[DirectiveAttribute]) {
        self.word(attributes.len() as u32);
        for DirectiveAttribute { name, value } in attributes {
            self.str(name);
            self.opt_str(value.as_deref());
        }
    }

    fn block(&mut self, block: &Block) {
        match block {
            Block::Paragraph(node) => {
                self.head(kind::PARAGRAPH, &node.meta);
                self.leaf(node.source(), &node.children);
            }
            Block::Heading(node) => {
                self.head(kind::HEADING, &node.meta);
                self.word(u32::from(node.depth));
                self.word(node.kind as u32);
                self.leaf(node.source(), &node.children);
            }
            Block::ThematicBreak(ThematicBreak { meta, marker }) => {
                self.head(kind::THEMATIC_BREAK, meta);
                self.word(*marker as u32);
            }
            Block::BlockQuote(BlockQuote { meta, children }) => {
                self.head(kind::BLOCK_QUOTE, meta);
                self.blocks(children);
            }
            Block::Alert(Alert {
                meta,
                kind,
                title,
                children,
            }) => {
                self.head(kind::ALERT, meta);
                self.word(*kind as u32);
                self.opt_str(title.as_deref());
                self.blocks(children);
            }
            Block::List(List {
                meta,
                ordered,
                delimiter,
                tight,
                children,
            }) => {
                self.head(kind::LIST, meta);
                self.bool(*ordered);
                self.word(*delimiter as u32);
                self.bool(*tight);
                self.word(children.len() as u32);
                for ListItem {
                    meta,
                    checked,
                    number,
                    children,
                } in children
                {
                    self.head(kind::LIST_ITEM, meta);
                    self.word(match checked {
                        None => 0,
                        Some(false) => 1,
                        Some(true) => 2,
                    });
                    match number {
                        None => self.word(0),
                        Some(number) => {
                            self.word(1);
                            self.word(*number as u32);
                            self.word((*number >> 32) as u32);
                        }
                    }
                    self.blocks(children);
                }
            }
            Block::CodeBlock(CodeBlock {
                meta,
                kind,
                info,
                value,
            }) => {
                self.head(kind::CODE_BLOCK, meta);
                match kind {
                    CodeBlockKind::Indented => self.word(0),
                    CodeBlockKind::Fenced { marker, length } => {
                        self.word(1);
                        self.word(*marker as u32);
                        self.word(*length as u32);
                    }
                }
                self.opt_str(info.as_deref());
                self.str(value);
            }
            Block::HtmlBlock(HtmlBlock { meta, value }) => {
                self.head(kind::HTML_BLOCK, meta);
                self.str(value);
            }
            Block::HtmlContainer(HtmlContainer {
                meta,
                opening,
                content,
                closing,
            }) => {
                self.head(kind::HTML_CONTAINER, meta);
                self.tag(opening);
                match content {
                    HtmlContainerContent::Blocks(children) => {
                        self.word(0);
                        self.blocks(children);
                    }
                    HtmlContainerContent::Inlines(children) => {
                        self.word(1);
                        self.inlines(children);
                    }
                }
                self.tag(closing);
            }
            Block::Definition(Definition {
                meta,
                label,
                identifier,
                destination,
                destination_kind,
                title,
            }) => {
                self.head(kind::DEFINITION, meta);
                self.str(label);
                self.str(identifier);
                self.str(destination);
                self.word(*destination_kind as u32);
                self.title(title);
            }
            Block::FootnoteDefinition(FootnoteDefinition {
                meta,
                label,
                identifier,
                children,
            }) => {
                self.head(kind::FOOTNOTE_DEFINITION, meta);
                self.str(label);
                self.str(identifier);
                self.blocks(children);
            }
            Block::Table(Table {
                meta,
                alignments,
                rows,
            }) => {
                self.head(kind::TABLE, meta);
                self.word(alignments.len() as u32);
                for alignment in alignments {
                    self.word(*alignment as u32);
                }
                self.word(rows.len() as u32);
                for TableRow { meta, cells } in rows {
                    self.head(kind::TABLE_ROW, meta);
                    self.word(cells.len() as u32);
                    for cell in cells {
                        self.head(kind::TABLE_CELL, &cell.meta);
                        self.leaf(cell.source(), &cell.children);
                    }
                }
            }
            Block::MathBlock(MathBlock { meta, value }) => {
                self.head(kind::MATH_BLOCK, meta);
                self.str(value);
            }
            Block::Frontmatter(Frontmatter { meta, kind, value }) => {
                self.head(kind::FRONTMATTER, meta);
                self.word(*kind as u32);
                self.str(value);
            }
            Block::LeafDirective(node) => {
                self.head(kind::LEAF_DIRECTIVE, &node.meta);
                self.str(&node.name);
                self.leaf(node.label_source(), &node.label);
                self.attributes(&node.attributes);
            }
            Block::ContainerDirective(node) => {
                self.head(kind::CONTAINER_DIRECTIVE, &node.meta);
                self.str(&node.name);
                self.leaf(node.label_source(), &node.label);
                self.attributes(&node.attributes);
                self.blocks(&node.children);
            }
            Block::BlankLine(BlankLine { meta }) => self.head(kind::BLANK_LINE, meta),
        }
    }

    fn inline(&mut self, inline: &Inline) {
        match inline {
            Inline::Text(Text { meta, value }) => {
                self.head(kind::TEXT, meta);
                self.str(value);
            }
            Inline::Escape(Escape { meta, value }) => {
                self.head(kind::ESCAPE, meta);
                self.word(u32::from(*value));
            }
            Inline::CharacterReference(CharacterReference { meta, reference }) => {
                self.head(kind::CHARACTER_REFERENCE, meta);
                self.str(reference);
            }
            Inline::Emphasis(Emphasis {
                meta,
                delimiter,
                children,
            }) => {
                self.head(kind::EMPHASIS, meta);
                self.word(*delimiter as u32);
                self.inlines(children);
            }
            Inline::Strong(Strong {
                meta,
                delimiter,
                children,
            }) => {
                self.head(kind::STRONG, meta);
                self.word(*delimiter as u32);
                self.inlines(children);
            }
            Inline::Delete(Delete { meta, children }) => {
                self.head(kind::DELETE, meta);
                self.inlines(children);
            }
            Inline::Mark(Mark { meta, children }) => {
                self.head(kind::MARK, meta);
                self.inlines(children);
            }
            Inline::Shortcode(Shortcode { meta, name }) => {
                self.head(kind::SHORTCODE, meta);
                self.str(name);
            }
            Inline::Tag(Tag { meta, name }) => {
                self.head(kind::TAG, meta);
                self.str(name);
            }
            Inline::Code(CodeInline { meta, value }) => {
                self.head(kind::CODE, meta);
                self.str(value);
            }
            Inline::Link(Link {
                meta,
                destination,
                destination_kind,
                title,
                children,
            }) => {
                self.head(kind::LINK, meta);
                self.str(destination);
                self.word(*destination_kind as u32);
                self.title(title);
                self.inlines(children);
            }
            Inline::Autolink(Autolink { meta, form, text }) => {
                self.head(kind::AUTOLINK, meta);
                self.word(*form as u32);
                self.str(text);
            }
            Inline::Image(Image {
                meta,
                destination,
                destination_kind,
                title,
                alt,
            }) => {
                self.head(kind::IMAGE, meta);
                self.str(destination);
                self.word(*destination_kind as u32);
                self.title(title);
                self.inlines(alt);
            }
            Inline::LinkReference(LinkReference {
                meta,
                identifier,
                label,
                kind,
                children,
            }) => {
                self.head(kind::LINK_REFERENCE, meta);
                self.str(identifier);
                self.str(label);
                self.word(*kind as u32);
                self.inlines(children);
            }
            Inline::ImageReference(ImageReference {
                meta,
                identifier,
                label,
                kind,
                alt,
            }) => {
                self.head(kind::IMAGE_REFERENCE, meta);
                self.str(identifier);
                self.str(label);
                self.word(*kind as u32);
                self.inlines(alt);
            }
            Inline::Html(HtmlInline { meta, value }) => {
                self.head(kind::HTML, meta);
                self.str(value);
            }
            Inline::SoftBreak(SoftBreak { meta }) => self.head(kind::SOFT_BREAK, meta),
            Inline::LineBreak(LineBreak { meta, kind }) => {
                self.head(kind::LINE_BREAK, meta);
                self.word(*kind as u32);
            }
            Inline::Math(MathInline { meta, value, kind }) => {
                self.head(kind::MATH, meta);
                self.str(value);
                match kind {
                    MathInlineKind::Code => self.word(0),
                    MathInlineKind::Dollar { dollars } => {
                        self.word(1);
                        self.word(u32::from(*dollars));
                    }
                }
            }
            Inline::FootnoteReference(FootnoteReference {
                meta,
                label,
                identifier,
            }) => {
                self.head(kind::FOOTNOTE_REFERENCE, meta);
                self.str(label);
                self.str(identifier);
            }
            Inline::InlineFootnote(InlineFootnote { meta, children }) => {
                self.head(kind::INLINE_FOOTNOTE, meta);
                self.inlines(children);
            }
            Inline::WikiLink(WikiLink {
                meta,
                target,
                label,
                embed,
            }) => {
                self.head(kind::WIKI_LINK, meta);
                self.str(target);
                self.str(label);
                self.bool(*embed);
            }
            Inline::TextDirective(TextDirective {
                meta,
                name,
                label,
                attributes,
            }) => {
                self.head(kind::TEXT_DIRECTIVE, meta);
                self.str(name);
                self.inlines(label);
                self.attributes(attributes);
            }
        }
    }

    /// The tokens of an inline parse: each one's span, the pre-order index of
    /// its node, and its role as its Rust name.
    pub fn tokens(&mut self, tokens: &[InlineToken]) {
        self.word(tokens.len() as u32);
        for token in tokens {
            self.span(Some(token.span));
            self.word(token.node as u32);
            self.str(&format!("{:?}", token.role));
        }
    }

    /// Diagnostics, with their severity and code as their Rust names.
    pub fn diagnostics(&mut self, diagnostics: &[Diagnostic]) {
        self.word(diagnostics.len() as u32);
        for diagnostic in diagnostics {
            self.str(&format!("{:?}", diagnostic.severity));
            self.str(&format!("{:?}", diagnostic.code));
            self.span(diagnostic.span);
            self.str(&diagnostic.message);
        }
    }

    pub fn finish(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(
            8 + 4 * self.words.len() + 8 * self.strings.len() + 2 * self.units.len(),
        );
        bytes.extend((self.words.len() as u32).to_le_bytes());
        bytes.extend((self.strings.len() as u32).to_le_bytes());
        for word in &self.words {
            bytes.extend(word.to_le_bytes());
        }
        for (start, length) in &self.strings {
            bytes.extend(start.to_le_bytes());
            bytes.extend(length.to_le_bytes());
        }
        for unit in &self.units {
            bytes.extend(unit.to_le_bytes());
        }
        bytes
    }
}

/// Why a message does not hold a tree.
#[derive(Debug, Eq, PartialEq)]
pub struct Malformed(pub String);

type Read<T> = Result<T, Malformed>;

fn malformed<T>(message: impl Into<String>) -> Read<T> {
    Err(Malformed(message.into()))
}

/// Reads a message.
pub struct Reader<'m> {
    words: Vec<u32>,
    at: usize,
    strings: Vec<(u32, u32)>,
    units: &'m [u8],
    /// The byte offset of each UTF-16 offset spans count in.
    offsets: &'m [u32],
    /// How many lists of nodes enclose the one being read.
    depth: usize,
    /// Whether a leaf was built from its source, with no definitions yet.
    sourced: bool,
}

/// A leaf's inline content as the message gives it.
enum Leaf {
    Source(String),
    Inlines(Vec<Inline>),
}

/// The deepest nesting of node lists a message may hold: far above what a
/// parse produces, and shallow enough that decoding and writing the tree stay
/// within the WebAssembly stack.
pub const MAX_DEPTH: usize = 128;

fn le_word(bytes: &[u8], at: usize) -> Read<u32> {
    match bytes.get(at..at + 4) {
        Some(word) => Ok(u32::from_le_bytes([word[0], word[1], word[2], word[3]])),
        None => malformed("the message ends early"),
    }
}

impl<'m> Reader<'m> {
    pub fn new(message: &'m [u8], offsets: &'m [u32]) -> Read<Self> {
        let word_count = le_word(message, 0)? as usize;
        let string_count = le_word(message, 4)? as usize;
        let words_end = 8 + 4 * word_count;
        let strings_end = words_end + 8 * string_count;
        if message.len() < strings_end {
            return malformed("the message ends early");
        }
        let words = (0..word_count)
            .map(|index| le_word(message, 8 + 4 * index))
            .collect::<Read<_>>()?;
        let strings = (0..string_count)
            .map(|index| {
                let at = words_end + 8 * index;
                Ok((le_word(message, at)?, le_word(message, at + 4)?))
            })
            .collect::<Read<_>>()?;
        Ok(Self {
            words,
            at: 0,
            strings,
            units: &message[strings_end..],
            offsets,
            depth: 0,
            sourced: false,
        })
    }

    pub fn word(&mut self) -> Read<u32> {
        let word = *self
            .words
            .get(self.at)
            .ok_or_else(|| Malformed("the tree ends early".into()))?;
        self.at += 1;
        Ok(word)
    }

    fn bool(&mut self) -> Read<bool> {
        match self.word()? {
            0 => Ok(false),
            1 => Ok(true),
            other => malformed(format!("{other} is not a boolean")),
        }
    }

    fn string(&self, index: usize) -> Read<String> {
        let &(start, length) = self
            .strings
            .get(index)
            .ok_or_else(|| Malformed(format!("no string {index}")))?;
        let (start, end) = (2 * start as usize, 2 * (start as usize + length as usize));
        let bytes = self
            .units
            .get(start..end)
            .ok_or_else(|| Malformed(format!("string {index} lies outside the message")))?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }

    fn str(&mut self) -> Read<String> {
        let index = self.word()? as usize;
        self.string(index)
    }

    /// A string, as a list of labels holds each.
    pub fn label(&mut self) -> Read<String> {
        self.str()
    }

    /// A leaf's inline content: the source it is built from, or else its
    /// inlines.
    fn leaf(&mut self) -> Read<Leaf> {
        match self.opt_str()? {
            Some(text) => {
                if self.count()? != 0 {
                    return malformed("a leaf with a source holds inlines");
                }
                self.sourced = true;
                Ok(Leaf::Source(text))
            }
            None => self.inlines().map(Leaf::Inlines),
        }
    }

    fn opt_str(&mut self) -> Read<Option<String>> {
        match self.word()? {
            0 => Ok(None),
            index => self.string(index as usize - 1).map(Some),
        }
    }

    fn count(&mut self) -> Read<usize> {
        let count = self.word()? as usize;
        // Every counted item takes at least a word.
        if count > self.words.len() - self.at {
            return malformed(format!("a count of {count} exceeds the message"));
        }
        Ok(count)
    }

    /// A span; one that no UTF-16 offset of the source maps reads as none.
    fn span(&mut self) -> Read<Option<Span>> {
        let start = self.word()?;
        let end = self.word()?;
        if start == NO_SPAN {
            return Ok(None);
        }
        let byte = |offset: u32| {
            self.offsets
                .get(offset as usize)
                .copied()
                .filter(|byte| *byte != NO_SPAN)
        };
        Ok(match (byte(start), byte(end)) {
            (Some(start), Some(end)) if start <= end => {
                Some(Span::new(start as usize, end as usize))
            }
            _ => None,
        })
    }

    fn meta(&mut self) -> Read<NodeMeta> {
        Ok(NodeMeta::new(self.span()?))
    }

    fn enumerated<T: Copy>(&mut self, values: &[T], what: &str) -> Read<T> {
        let index = self.word()? as usize;
        match values.get(index) {
            Some(value) => Ok(*value),
            None => malformed(format!("{index} is not a {what}")),
        }
    }

    fn expect_kind(&mut self, expected: u32, what: &str) -> Read<()> {
        let kind = self.word()?;
        if kind != expected {
            return malformed(format!("kind {kind} where a {what} belongs"));
        }
        Ok(())
    }

    pub fn document(&mut self) -> Read<Document> {
        self.expect_kind(kind::DOCUMENT, "document")?;
        let meta = self.meta()?;
        let mut document = Document::new(self.blocks()?);
        document.meta = meta;
        if self.at != self.words.len() {
            return malformed("words follow the tree");
        }
        // A leaf's source reads with the definitions of the whole tree.
        if self.sourced && Definitions::of(&document) != Definitions::default() {
            document.reread_kept_sources();
        }
        Ok(document)
    }

    pub fn blocks(&mut self) -> Read<Vec<Block>> {
        self.nested(|reader| {
            let count = reader.count()?;
            (0..count).map(|_| reader.block()).collect()
        })
    }

    pub fn inlines(&mut self) -> Read<Vec<Inline>> {
        self.nested(|reader| {
            let count = reader.count()?;
            (0..count).map(|_| reader.inline()).collect()
        })
    }

    /// Reads one more level of nodes, refusing one past `MAX_DEPTH`.
    fn nested<T>(&mut self, read: impl FnOnce(&mut Self) -> Read<T>) -> Read<T> {
        if self.depth == MAX_DEPTH {
            return malformed(format!("the tree nests deeper than {MAX_DEPTH} levels"));
        }
        self.depth += 1;
        let value = read(self);
        self.depth -= 1;
        value
    }

    fn title(&mut self) -> Read<Option<Title>> {
        Ok(match self.word()? {
            0 => None,
            1 => Some(Title {
                value: self.str()?,
                kind: self.enumerated(
                    &[
                        LinkTitleKind::DoubleQuote,
                        LinkTitleKind::SingleQuote,
                        LinkTitleKind::Paren,
                    ],
                    "title kind",
                )?,
            }),
            other => return malformed(format!("{other} is not a title")),
        })
    }

    fn tag(&mut self) -> Read<HtmlTag> {
        Ok(HtmlTag {
            meta: self.meta()?,
            name: self.str()?,
            raw: self.str()?,
        })
    }

    fn attributes(&mut self) -> Read<Vec<DirectiveAttribute>> {
        let count = self.count()?;
        (0..count)
            .map(|_| {
                Ok(DirectiveAttribute {
                    name: self.str()?,
                    value: self.opt_str()?,
                })
            })
            .collect()
    }

    fn destination_kind(&mut self) -> Read<LinkDestinationKind> {
        self.enumerated(
            &[
                LinkDestinationKind::Bare,
                LinkDestinationKind::Angle,
                LinkDestinationKind::Omitted,
            ],
            "destination kind",
        )
    }

    fn reference_kind(&mut self) -> Read<ReferenceKind> {
        self.enumerated(
            &[
                ReferenceKind::Full,
                ReferenceKind::Collapsed,
                ReferenceKind::Shortcut,
            ],
            "reference kind",
        )
    }

    fn emphasis_delimiter(&mut self) -> Read<EmphasisDelimiter> {
        self.enumerated(
            &[EmphasisDelimiter::Asterisk, EmphasisDelimiter::Underscore],
            "emphasis delimiter",
        )
    }

    fn block(&mut self) -> Read<Block> {
        let kind = self.word()?;
        let meta = self.meta()?;
        Ok(match kind {
            kind::PARAGRAPH => {
                let mut node = match self.leaf()? {
                    Leaf::Source(text) => Paragraph::with_source(&text, &Definitions::default()).0,
                    Leaf::Inlines(children) => Paragraph::new(children),
                };
                node.meta = meta;
                Block::Paragraph(node)
            }
            kind::HEADING => {
                let depth = match self.word()? {
                    depth @ 1..=6 => depth as u8,
                    other => return malformed(format!("{other} is not a heading depth")),
                };
                let kind =
                    self.enumerated(&[HeadingKind::Atx, HeadingKind::Setext], "heading kind")?;
                let mut node = match self.leaf()? {
                    Leaf::Source(text) => {
                        Heading::with_source(depth, kind, &text, &Definitions::default()).0
                    }
                    Leaf::Inlines(children) => {
                        let mut node = Heading::new(depth, children);
                        node.kind = kind;
                        node
                    }
                };
                node.meta = meta;
                Block::Heading(node)
            }
            kind::THEMATIC_BREAK => Block::ThematicBreak(ThematicBreak {
                meta,
                marker: self.enumerated(
                    &[
                        ThematicBreakMarker::Dash,
                        ThematicBreakMarker::Asterisk,
                        ThematicBreakMarker::Underscore,
                    ],
                    "thematic break marker",
                )?,
            }),
            kind::BLOCK_QUOTE => Block::BlockQuote(BlockQuote {
                meta,
                children: self.blocks()?,
            }),
            kind::ALERT => Block::Alert(Alert {
                meta,
                kind: self.enumerated(
                    &[
                        AlertKind::Note,
                        AlertKind::Tip,
                        AlertKind::Important,
                        AlertKind::Warning,
                        AlertKind::Caution,
                    ],
                    "alert kind",
                )?,
                title: self.opt_str()?,
                children: self.blocks()?,
            }),
            kind::LIST => {
                let ordered = self.bool()?;
                let delimiter = self.enumerated(
                    &[
                        ListDelimiter::Dash,
                        ListDelimiter::Asterisk,
                        ListDelimiter::Plus,
                        ListDelimiter::Period,
                        ListDelimiter::Paren,
                    ],
                    "list delimiter",
                )?;
                let tight = self.bool()?;
                let count = self.count()?;
                let children = (0..count)
                    .map(|_| {
                        self.expect_kind(kind::LIST_ITEM, "list item")?;
                        Ok(ListItem {
                            meta: self.meta()?,
                            checked: match self.word()? {
                                0 => None,
                                1 => Some(false),
                                2 => Some(true),
                                other => return malformed(format!("{other} is not a task state")),
                            },
                            number: match self.word()? {
                                0 => None,
                                1 => {
                                    let low = u64::from(self.word()?);
                                    let high = u64::from(self.word()?);
                                    Some(high << 32 | low)
                                }
                                other => {
                                    return malformed(format!("{other} is not an item number"))
                                }
                            },
                            children: self.blocks()?,
                        })
                    })
                    .collect::<Read<_>>()?;
                Block::List(List {
                    meta,
                    ordered,
                    delimiter,
                    tight,
                    children,
                })
            }
            kind::CODE_BLOCK => Block::CodeBlock(CodeBlock {
                meta,
                kind: match self.word()? {
                    0 => CodeBlockKind::Indented,
                    1 => CodeBlockKind::Fenced {
                        marker: self.enumerated(
                            &[FenceMarker::Backtick, FenceMarker::Tilde],
                            "fence marker",
                        )?,
                        length: self.word()? as usize,
                    },
                    other => return malformed(format!("{other} is not a code block kind")),
                },
                info: self.opt_str()?,
                value: self.str()?,
            }),
            kind::HTML_BLOCK => Block::HtmlBlock(HtmlBlock {
                meta,
                value: self.str()?,
            }),
            kind::HTML_CONTAINER => Block::HtmlContainer(HtmlContainer {
                meta,
                opening: self.tag()?,
                content: match self.word()? {
                    0 => HtmlContainerContent::Blocks(self.blocks()?),
                    1 => HtmlContainerContent::Inlines(self.inlines()?),
                    other => return malformed(format!("{other} is not an HTML content kind")),
                },
                closing: self.tag()?,
            }),
            kind::DEFINITION => Block::Definition(Definition {
                meta,
                label: self.str()?,
                identifier: self.str()?,
                destination: self.str()?,
                destination_kind: self.destination_kind()?,
                title: self.title()?,
            }),
            kind::FOOTNOTE_DEFINITION => Block::FootnoteDefinition(FootnoteDefinition {
                meta,
                label: self.str()?,
                identifier: self.str()?,
                children: self.blocks()?,
            }),
            kind::TABLE => {
                let count = self.count()?;
                let alignments = (0..count)
                    .map(|_| {
                        self.enumerated(
                            &[
                                TableAlignment::None,
                                TableAlignment::Left,
                                TableAlignment::Center,
                                TableAlignment::Right,
                            ],
                            "table alignment",
                        )
                    })
                    .collect::<Read<_>>()?;
                let count = self.count()?;
                let rows = (0..count)
                    .map(|_| {
                        self.expect_kind(kind::TABLE_ROW, "table row")?;
                        let meta = self.meta()?;
                        let count = self.count()?;
                        let cells = (0..count)
                            .map(|_| {
                                self.expect_kind(kind::TABLE_CELL, "table cell")?;
                                let meta = self.meta()?;
                                let mut cell = match self.leaf()? {
                                    Leaf::Source(text) => {
                                        TableCell::with_source(&text, &Definitions::default()).0
                                    }
                                    Leaf::Inlines(children) => TableCell::new(children),
                                };
                                cell.meta = meta;
                                Ok(cell)
                            })
                            .collect::<Read<_>>()?;
                        Ok(TableRow { meta, cells })
                    })
                    .collect::<Read<_>>()?;
                Block::Table(Table {
                    meta,
                    alignments,
                    rows,
                })
            }
            kind::MATH_BLOCK => Block::MathBlock(MathBlock {
                meta,
                value: self.str()?,
            }),
            kind::FRONTMATTER => Block::Frontmatter(Frontmatter {
                meta,
                kind: self.enumerated(
                    &[FrontmatterKind::Yaml, FrontmatterKind::Toml],
                    "frontmatter kind",
                )?,
                value: self.str()?,
            }),
            kind::LEAF_DIRECTIVE => {
                let name = self.str()?;
                let mut node = match self.leaf()? {
                    Leaf::Source(text) => {
                        LeafDirective::with_source(name, &text, &Definitions::default()).0
                    }
                    Leaf::Inlines(label) => {
                        let mut node = LeafDirective::new(name);
                        node.label = label;
                        node
                    }
                };
                node.meta = meta;
                node.attributes = self.attributes()?;
                Block::LeafDirective(node)
            }
            kind::CONTAINER_DIRECTIVE => {
                let name = self.str()?;
                let label = self.leaf()?;
                let attributes = self.attributes()?;
                let children = self.blocks()?;
                let mut node = match label {
                    Leaf::Source(text) => {
                        ContainerDirective::with_source(
                            name,
                            &text,
                            children,
                            &Definitions::default(),
                        )
                        .0
                    }
                    Leaf::Inlines(label) => {
                        let mut node = ContainerDirective::new(name, children);
                        node.label = label;
                        node
                    }
                };
                node.meta = meta;
                node.attributes = attributes;
                Block::ContainerDirective(node)
            }
            kind::BLANK_LINE => Block::BlankLine(BlankLine { meta }),
            other => return malformed(format!("kind {other} where a block belongs")),
        })
    }

    fn inline(&mut self) -> Read<Inline> {
        let kind = self.word()?;
        let meta = self.meta()?;
        Ok(match kind {
            kind::TEXT => Inline::Text(Text {
                meta,
                value: self.str()?,
            }),
            kind::ESCAPE => Inline::Escape(Escape {
                meta,
                value: match char::from_u32(self.word()?) {
                    Some(value) => value,
                    None => return malformed("an escape holds no char"),
                },
            }),
            kind::CHARACTER_REFERENCE => Inline::CharacterReference(CharacterReference {
                meta,
                reference: self.str()?,
            }),
            kind::EMPHASIS => Inline::Emphasis(Emphasis {
                meta,
                delimiter: self.emphasis_delimiter()?,
                children: self.inlines()?,
            }),
            kind::STRONG => Inline::Strong(Strong {
                meta,
                delimiter: self.emphasis_delimiter()?,
                children: self.inlines()?,
            }),
            kind::DELETE => Inline::Delete(Delete {
                meta,
                children: self.inlines()?,
            }),
            kind::MARK => Inline::Mark(Mark {
                meta,
                children: self.inlines()?,
            }),
            kind::SHORTCODE => Inline::Shortcode(Shortcode {
                meta,
                name: self.str()?,
            }),
            kind::TAG => Inline::Tag(Tag {
                meta,
                name: self.str()?,
            }),
            kind::CODE => Inline::Code(CodeInline {
                meta,
                value: self.str()?,
            }),
            kind::LINK => Inline::Link(Link {
                meta,
                destination: self.str()?,
                destination_kind: self.destination_kind()?,
                title: self.title()?,
                children: self.inlines()?,
            }),
            kind::AUTOLINK => Inline::Autolink(Autolink {
                meta,
                form: self.enumerated(
                    &[AutolinkForm::Angle, AutolinkForm::Literal],
                    "autolink form",
                )?,
                text: self.str()?,
            }),
            kind::IMAGE => Inline::Image(Image {
                meta,
                destination: self.str()?,
                destination_kind: self.destination_kind()?,
                title: self.title()?,
                alt: self.inlines()?,
            }),
            kind::LINK_REFERENCE => Inline::LinkReference(LinkReference {
                meta,
                identifier: self.str()?,
                label: self.str()?,
                kind: self.reference_kind()?,
                children: self.inlines()?,
            }),
            kind::IMAGE_REFERENCE => Inline::ImageReference(ImageReference {
                meta,
                identifier: self.str()?,
                label: self.str()?,
                kind: self.reference_kind()?,
                alt: self.inlines()?,
            }),
            kind::HTML => Inline::Html(HtmlInline {
                meta,
                value: self.str()?,
            }),
            kind::SOFT_BREAK => Inline::SoftBreak(SoftBreak { meta }),
            kind::LINE_BREAK => Inline::LineBreak(LineBreak {
                meta,
                kind: self.enumerated(
                    &[LineBreakKind::Backslash, LineBreakKind::Spaces],
                    "line break kind",
                )?,
            }),
            kind::MATH => Inline::Math(MathInline {
                meta,
                value: self.str()?,
                kind: match self.word()? {
                    0 => MathInlineKind::Code,
                    1 => MathInlineKind::Dollar {
                        dollars: match u8::try_from(self.word()?) {
                            Ok(dollars) => dollars,
                            Err(_) => return malformed("too many dollars"),
                        },
                    },
                    other => return malformed(format!("{other} is not a math kind")),
                },
            }),
            kind::FOOTNOTE_REFERENCE => Inline::FootnoteReference(FootnoteReference {
                meta,
                label: self.str()?,
                identifier: self.str()?,
            }),
            kind::INLINE_FOOTNOTE => Inline::InlineFootnote(InlineFootnote {
                meta,
                children: self.inlines()?,
            }),
            kind::WIKI_LINK => Inline::WikiLink(WikiLink {
                meta,
                target: self.str()?,
                label: self.str()?,
                embed: self.bool()?,
            }),
            kind::TEXT_DIRECTIVE => Inline::TextDirective(TextDirective {
                meta,
                name: self.str()?,
                label: self.inlines()?,
                attributes: self.attributes()?,
            }),
            other => return malformed(format!("kind {other} where an inline belongs")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(source: &str) {
        let document = parse(source).document;
        let to_utf16 = utf16_offsets(source);
        let mut writer = Writer::new(&to_utf16);
        writer.document(&document);
        let message = writer.finish();
        let to_bytes = byte_offsets(source);
        let read = Reader::new(&message, &to_bytes)
            .and_then(|mut reader| reader.document())
            .unwrap_or_else(|error| panic!("{source:?}: {error:?}"));
        assert_eq!(read.children, document.children, "{source:?}");
        assert_eq!(read.meta, document.meta, "{source:?}");
    }

    #[test]
    fn every_node_kind_reads_back() {
        round_trip(concat!(
            "---\na: b\n---\n\n",
            "# H *e* **s** __u__ ~~d~~ ==m== :smile: `c` \\* &amp; <b>\n\n",
            "Setext\n===\n\n",
            "***\n\n",
            "> [!NOTE] title\n> a\n\n",
            "> q\n\n",
            "1) a\n2) [ ] b\n3) [x] c\n\n",
            "- a\n\n  b\n\n",
            "```rust\nfn\n```\n\n",
            "    indented\n\n",
            "<div>\nraw\n</div>\n\n",
            "<details>\n<summary>s</summary>\n\nx\n\n</details>\n\n",
            "[d]: </u u> 'title'\n\n",
            "[^f]: note\n\n",
            "| a | b |\n|:-|-:|\n| c | d |\n\n",
            "$$\nx\n$$\n\n",
            "::leaf[l]{a=b c}\n\n",
            ":::container[l]{#i}\nx\n:::\n\n",
            "[l](/u \"t\") <http://a> www.b.c ![i](/j) [d][] ![d] <i> a  \nb\\\nc $x$ $`y`$ ",
            "[^f] ^[inline] [[w|l]] ![[e]] :t[l]{k} 😀\n",
        ));
    }

    #[test]
    fn spans_count_utf16_units() {
        let source = "😀 *a*";
        let offsets = utf16_offsets(source);
        let mut writer = Writer::new(&offsets);
        writer.document(&parse(source).document);
        let message = writer.finish();
        // Header, then document, its span, its block count, then the
        // paragraph and its span, its source (none), its inline count, the
        // text and its span and string, and the emphasis and its span.
        let words: Vec<u32> = message[8..]
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
            .collect();
        assert_eq!(&words[..3], &[kind::DOCUMENT, 0, 6]);
        assert_eq!(&words[4..8], &[kind::PARAGRAPH, 0, 6, 0]);
        assert_eq!(&words[9..12], &[kind::TEXT, 0, 3]);
        assert_eq!(&words[13..16], &[kind::EMPHASIS, 3, 6]);
    }

    #[test]
    fn tokens_carry_utf16_spans_nodes_and_roles() {
        let source = "😀[a](u)";
        let offsets = utf16_offsets(source);
        let mut writer = Writer::new(&offsets);
        writer.tokens(&parse_inline(source, &Definitions::default()).tokens);
        let message = writer.finish();
        let words: Vec<u32> = message[8..]
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
            .collect();
        // The count, then per token its span, node, and role string: the text
        // (node 0), the link's `[` (node 1), its label (node 2), and `](`,
        // `u` and `)` of the link.
        let spans: Vec<(u32, u32, u32)> = (0..words[0] as usize)
            .map(|index| {
                let at = 1 + 4 * index;
                (words[at], words[at + 1], words[at + 2])
            })
            .collect();
        assert_eq!(
            spans,
            [
                (0, 2, 0),
                (2, 3, 1),
                (3, 4, 2),
                (4, 6, 1),
                (6, 7, 1),
                (7, 8, 1)
            ]
        );
        let reader = Reader::new(&message, &[]).unwrap();
        let roles: Vec<String> = (0..spans.len())
            .map(|index| reader.string(words[4 + 4 * index] as usize).unwrap())
            .collect();
        assert_eq!(
            roles,
            ["Text", "Syntax", "Text", "Syntax", "Destination", "Syntax"]
        );
    }

    #[test]
    fn malformed_messages_are_refused() {
        let to_bytes = byte_offsets("");
        assert!(Reader::new(&[1, 0], &to_bytes).is_err());
        let mut writer = Writer::new(&[0]);
        writer.word(kind::DOCUMENT);
        writer.span(None);
        writer.word(1);
        writer.word(kind::TEXT);
        writer.span(None);
        let message = writer.finish();
        let error = Reader::new(&message, &to_bytes)
            .and_then(|mut reader| reader.document())
            .unwrap_err();
        assert_eq!(error, Malformed("kind 40 where a block belongs".into()));
    }

    #[test]
    fn a_tree_deeper_than_the_limit_is_refused() {
        let nested = |depth: usize| {
            let mut inline = Inline::from(Text::from("x"));
            // The document's blocks and the paragraph's inlines take two
            // levels, each emphasis one more.
            for _ in 2..depth {
                inline = Emphasis {
                    meta: NodeMeta::default(),
                    delimiter: EmphasisDelimiter::Asterisk,
                    children: vec![inline],
                }
                .into();
            }
            let document = Document::new(vec![Paragraph::new([inline]).into()]);
            let mut writer = Writer::new(&[0]);
            writer.document(&document);
            let message = writer.finish();
            Reader::new(&message, &[0]).and_then(|mut reader| reader.document())
        };
        assert!(nested(MAX_DEPTH).is_ok());
        assert_eq!(
            nested(MAX_DEPTH + 1).unwrap_err(),
            Malformed(format!("the tree nests deeper than {MAX_DEPTH} levels"))
        );
    }

    #[test]
    fn a_span_inside_a_surrogate_pair_reads_as_none() {
        let offsets = byte_offsets("😀a");
        assert_eq!(offsets, vec![0, NO_SPAN, 4, 5]);
    }
}
