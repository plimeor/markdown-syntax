//! The encoding a tree crosses the WebAssembly boundary in, both ways. It is
//! private to this crate and `js/index.js`, which mirror each other.
//!
//! A message is a header of two `u32`, the word count and the string count,
//! then the words, then a `(start, length)` pair of `u32` for each string,
//! then the strings themselves as one run of UTF-8, where each string's start
//! and length count UTF-16 code units, as a JavaScript string's do; the run
//! may hold text between the strings, which no string covers. A node is its
//! kind, its span as two UTF-16 offsets (`NO_SPAN` and 0 for none), and then
//! its fields in the order of its Rust struct; a list of nodes is its length
//! followed by the nodes. A character reference is followed by its decoded
//! value and a shortcode by its glyph, each an optional string: written
//! towards JavaScript, and read past, as none, on the way back. The top-level
//! blocks of a document written against its parse are a list whose entries
//! may each be, in place of a block, `kind::BASE_BLOCK` and the index of a
//! top-level block of the parse.
//!
//! A message is written with its strings in the order its words name them,
//! and a [`Reader`] finds each string in the text from where the string read
//! before it ends, so that reading a message takes time linear in it. A
//! string named before the last one read is found again from the start of
//! the text: a forged message that names its strings out of order may take
//! time quadratic in its length, never more.

use std::cell::Cell;
use std::fmt::{self, Write as _};

use markdown_syntax::prelude::*;
use markdown_syntax::{
    AlertKind, AutolinkForm, CodeBlockKind, DirectiveAttribute, EmphasisDelimiter, FenceMarker,
    FrontmatterKind, HtmlContainerContent, HtmlTag, LineBreakKind, LinkDestinationKind,
    LinkTitleKind, ListDelimiter, MathInlineKind, ReferenceKind, TableAlignment,
    ThematicBreakMarker, Title,
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
    /// Not a node: in the blocks of a message `Reader::document_against`
    /// reads, it and the index that follows it stand for the top-level
    /// block of the parse at that index. It lies outside the range of every
    /// node kind, each of which fits in a byte; `js/index.js` holds the same
    /// word.
    pub const BASE_BLOCK: u32 = 0xFFFF_FFFE;
}

/// For each byte offset of `text`, its UTF-16 offset; `text.len()` included.
pub fn utf16_offsets(text: &str) -> Vec<u32> {
    let mut offsets = Vec::with_capacity(text.len() + 1);
    // The bytes of a char share the offset of its first; a char of four bytes
    // takes two units.
    let (mut at, mut next) = (0, 0);
    offsets.extend(text.bytes().map(|byte| {
        if byte & 0xc0 != 0x80 {
            at = next;
            next += 1 + u32::from(byte >= 0xf0);
        }
        at
    }));
    offsets.push(next);
    offsets
}

/// The length of `text` in UTF-16 code units: a char takes a unit for the
/// byte that starts it, and a char of four bytes one more.
fn utf16_len(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.bytes()
            .map(|byte| usize::from(byte & 0xc0 != 0x80) + usize::from(byte >= 0xf0))
            .sum()
    }
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

/// Where the offsets of a text fall in the unit spans count in on the other
/// side, as a table of `utf16_offsets` or `byte_offsets` gives them.
#[derive(Clone, Copy)]
pub enum Offsets<'t> {
    /// An ASCII text of this length, whose offsets fall on themselves.
    Same(usize),
    /// The offset each offset falls on.
    Table(&'t [u32]),
    /// Where the first offset of each block of `BLOCK` falls, and how far
    /// past that each offset falls, or `GAP` for `NO_SPAN`.
    Blocks(&'t [u32], &'t [u8]),
    /// Blocks of `BLOCK` offsets, of which only those that do not step as
    /// ASCII does keep their steps.
    Sparse {
        /// The last offset.
        len: usize,
        /// For each block, where its first offset falls when each falls one
        /// past the one before, or else `STEPPED` and the index of its steps.
        blocks: &'t [u32],
        /// For each block with steps, where its first offset falls.
        bases: &'t [u32],
        /// For each block with steps, its `BLOCK` steps as `Blocks` holds
        /// them.
        steps: &'t [u8],
    },
}

const BLOCK: usize = 64;
const GAP: u8 = u8::MAX;
/// Marks a block of `Offsets::Sparse` as one with steps.
const STEPPED: u32 = 1 << 31;

impl Offsets<'_> {
    // Inlined into each read and write of a span, as a few instructions each
    // variant takes.
    #[inline(always)]
    pub fn get(self, offset: usize) -> Option<u32> {
        match self {
            Offsets::Same(len) => (offset <= len).then_some(offset as u32),
            Offsets::Table(table) => table.get(offset).copied(),
            Offsets::Blocks(blocks, steps) => Some(match *steps.get(offset)? {
                GAP => NO_SPAN,
                step => blocks[offset / BLOCK] + u32::from(step),
            }),
            Offsets::Sparse {
                len,
                blocks,
                bases,
                steps,
            } => {
                if offset > len {
                    return None;
                }
                let block = blocks[offset / BLOCK];
                if block & STEPPED == 0 {
                    return Some(block + (offset % BLOCK) as u32);
                }
                let index = (block & !STEPPED) as usize;
                Some(match steps[index * BLOCK + offset % BLOCK] {
                    GAP => NO_SPAN,
                    step => bases[index] + u32::from(step),
                })
            }
        }
    }
}

/// What `Offsets::Sparse` reads, kept with a document in four bytes a block
/// of offsets and a little over a byte an offset of each block that does not
/// step as ASCII does; or, where at least half the blocks do not, what
/// `Offsets::Blocks` reads, a little over a byte an offset, which reads an
/// offset in one read less. The threshold of half favors that read: at it
/// the dense table takes about 1.8 times the memory the sparse one would,
/// and less in proportion the more blocks step. An ASCII text needs none.
pub struct OffsetTable {
    /// The last offset.
    len: usize,
    /// For each block, where its first offset falls, or for a sparse table
    /// where it falls when each offset in it falls one past the one before,
    /// or else `STEPPED` and the index of its steps.
    blocks: Vec<u32>,
    /// For each block with steps of a sparse table, where its first offset
    /// falls; empty for any other table.
    bases: Vec<u32>,
    /// How far past where its block's first offset falls each offset falls,
    /// or `GAP` for `NO_SPAN`: for each offset, or for a sparse table, for
    /// each offset of each block with steps.
    steps: Vec<u8>,
}

/// The steps of a block of ASCII, each offset one past the one before.
const ASCENDING: [u8; BLOCK] = {
    let mut steps = [0; BLOCK];
    let mut step = 0;
    while step < BLOCK {
        steps[step] = step as u8;
        step += 1;
    }
    steps
};

/// The block `OffsetTable::to_bytes` is reading: its steps, how many it
/// holds, and the byte offset its first offset falls at.
struct Stepping {
    steps: [u8; BLOCK],
    filled: usize,
    base: usize,
}

impl Stepping {
    /// Adds the step of the next offset, which falls at `byte` or, inside a
    /// surrogate pair, at none, and adds the block to `table` once full. The
    /// next char starts at `at`, and so does a block that starts inside a
    /// surrogate pair.
    fn step(&mut self, table: &mut OffsetTable, at: usize, byte: Option<usize>) {
        if self.filled == 0 {
            self.base = at;
        }
        self.steps[self.filled] =
            byte.map_or(GAP, |byte| OffsetTable::step((byte - self.base) as u32));
        self.filled += 1;
        if self.filled == BLOCK {
            table.push_steps(self.base as u32, &self.steps);
            self.filled = 0;
        }
    }
}

impl OffsetTable {
    /// The UTF-16 offset of each byte offset of `text`, as `utf16_offsets`
    /// gives them, read a block at a time: a block of ASCII steps one unit a
    /// byte.
    pub fn to_utf16(text: &str) -> Self {
        let mut table = Self::empty(text.len());
        if text.is_ascii() {
            return table;
        }
        // The offset of the char the byte before belongs to, and of the next
        // char.
        let (mut at, mut next) = (0, 0);
        for block in text.as_bytes().chunks(BLOCK) {
            if block.is_ascii() {
                table.push_ascii(next);
                next += block.len() as u32;
                at = next - 1;
                continue;
            }
            // The bytes of a char share the offset of its first; a char of
            // four bytes takes two units.
            let starts_char = |byte: u8| byte & 0xc0 != 0x80;
            let base = if starts_char(block[0]) { next } else { at };
            // Within the block an offset moves at most a unit a byte; the
            // end of the text, when it falls in the block, follows its last
            // byte.
            let mut steps = [0; BLOCK];
            for (step, &byte) in steps.iter_mut().zip(block) {
                if starts_char(byte) {
                    at = next;
                    next += 1 + u32::from(byte >= 0xf0);
                }
                *step = (at - base) as u8;
            }
            if let Some(end) = steps.get_mut(block.len()) {
                *end = (next - base) as u8;
            }
            table.push_steps(base, &steps);
        }
        // The end of the text, which may start a block of its own.
        if text.len() % BLOCK == 0 {
            table.push_ascii(next);
        }
        table.finish()
    }

    /// The byte offset of each UTF-16 offset of `text`, as `byte_offsets`
    /// gives them, read a char at a time and a block of ASCII at once.
    pub fn to_bytes(text: &str) -> Self {
        let mut table = Self::empty(text.len());
        if text.is_ascii() {
            return table;
        }
        let bytes = text.as_bytes();
        let mut block = Stepping {
            steps: [0; BLOCK],
            filled: 0,
            base: 0,
        };
        // The byte offset of the next char, and how many UTF-16 offsets come
        // before it.
        let (mut at, mut units) = (0, 0);
        while let Some(&lead) = bytes.get(at) {
            if block.filled == 0 && bytes.get(at..at + BLOCK).is_some_and(<[u8]>::is_ascii) {
                // A block of ASCII steps a byte an offset.
                table.push_ascii(at as u32);
                at += BLOCK;
                units += BLOCK;
                continue;
            }
            let width = match lead {
                0..=0x7f => 1,
                0x80..=0xdf => 2,
                0xe0..=0xef => 3,
                _ => 4,
            };
            block.step(&mut table, at, Some(at));
            units += 1;
            if width == 4 {
                // The offset between the two units of the char.
                block.step(&mut table, at, None);
                units += 1;
            }
            at += width;
        }
        block.step(&mut table, at, Some(at));
        if block.filled > 0 {
            table.push_steps(block.base as u32, &block.steps);
        }
        table.len = units;
        table.finish()
    }

    fn empty(len: usize) -> Self {
        Self {
            len,
            blocks: Vec::new(),
            bases: Vec::new(),
            steps: Vec::new(),
        }
    }

    /// Adds a block whose first offset falls at `base` and each other one
    /// past the one before, as over ASCII.
    fn push_ascii(&mut self, base: u32) {
        if base < STEPPED {
            self.blocks.push(base);
        } else {
            self.push_steps(base, &ASCENDING);
        }
    }

    /// Adds a block whose first offset falls at `base` and the rest `steps`
    /// past it, of which those past the last offset are never read.
    fn push_steps(&mut self, base: u32, steps: &[u8; BLOCK]) {
        self.blocks.push(STEPPED | self.bases.len() as u32);
        self.bases.push(base);
        self.steps.extend_from_slice(steps);
    }

    /// The table, sparse where fewer than half its blocks have steps, and
    /// else with the steps of every offset.
    fn finish(mut self) -> Self {
        if 2 * self.bases.len() < self.blocks.len() {
            self.blocks.shrink_to_fit();
            self.bases.shrink_to_fit();
            self.steps.shrink_to_fit();
            return self;
        }
        let mut steps = Vec::with_capacity(self.len + 1);
        for (block, entry) in self.blocks.iter_mut().enumerate() {
            let length = (self.len + 1 - block * BLOCK).min(BLOCK);
            if *entry & STEPPED == 0 {
                steps.extend_from_slice(&ASCENDING[..length]);
            } else {
                let index = (*entry & !STEPPED) as usize;
                steps.extend_from_slice(&self.steps[index * BLOCK..][..length]);
                *entry = self.bases[index];
            }
        }
        self.blocks.shrink_to_fit();
        Self {
            len: self.len,
            blocks: self.blocks,
            bases: Vec::new(),
            steps,
        }
    }

    /// How far past its block's first an offset falls: within a block of 64
    /// offsets, a byte offset moves at most 3 bytes a unit and a UTF-16
    /// offset at most a unit a byte, plus one.
    fn step(step: u32) -> u8 {
        u8::try_from(step)
            .ok()
            .filter(|&step| step != GAP)
            .expect("a block spans fewer than 255 offsets")
    }

    pub fn offsets(&self) -> Offsets<'_> {
        if self.blocks.is_empty() {
            Offsets::Same(self.len)
        } else if self.bases.is_empty() {
            Offsets::Blocks(&self.blocks, &self.steps)
        } else {
            Offsets::Sparse {
                len: self.len,
                blocks: &self.blocks,
                bases: &self.bases,
                steps: &self.steps,
            }
        }
    }
}

/// Builds a message, each part as the bytes it is sent as.
pub struct Writer<'t> {
    /// The header, which `finish` fills in, and the words.
    head: Vec<u8>,
    /// The start and length of each string.
    strings: Vec<u8>,
    /// The strings, one after another.
    text: String,
    /// The length of `text` in UTF-16 code units.
    units: usize,
    /// The UTF-16 offset of each byte offset spans count in.
    offsets: Offsets<'t>,
}

impl<'t> Writer<'t> {
    pub fn new(offsets: &'t [u32]) -> Self {
        Self::with_offsets(Offsets::Table(offsets))
    }

    pub fn with_offsets(offsets: Offsets<'t>) -> Self {
        Self {
            head: vec![0; 8],
            strings: Vec::new(),
            text: String::new(),
            units: 0,
            offsets,
        }
    }

    pub fn word(&mut self, word: u32) {
        self.head.extend_from_slice(&word.to_le_bytes());
    }

    /// Adds a word for `set_word` to fill in, and returns where it is.
    pub fn reserve_word(&mut self) -> usize {
        self.word(0);
        self.head.len() - 4
    }

    /// Sets the word `reserve_word` added at `at`.
    pub fn set_word(&mut self, at: usize, word: u32) {
        self.head[at..at + 4].copy_from_slice(&word.to_le_bytes());
    }

    pub fn words(&mut self, words: &[u32]) {
        for &word in words {
            self.word(word);
        }
    }

    pub fn bool(&mut self, value: bool) {
        self.word(u32::from(value));
    }

    /// Adds `value` to the strings, and returns its index.
    fn string(&mut self, value: &str) -> u32 {
        let at = self.text.len();
        self.text.push_str(value);
        self.end_string(at)
    }

    /// Makes the text from byte `at` on the next string, and returns its
    /// index.
    fn end_string(&mut self, at: usize) -> u32 {
        let units = utf16_len(&self.text[at..]);
        let index = self.strings.len() / 8;
        for word in [self.units, units] {
            self.strings.extend_from_slice(&(word as u32).to_le_bytes());
        }
        self.units += units;
        index as u32
    }

    pub fn str(&mut self, value: &str) {
        let index = self.string(value);
        self.word(index);
    }

    /// Writes the `Debug` text of `value`, as a Rust name is written, as a
    /// string.
    fn name(&mut self, value: &impl fmt::Debug) {
        let at = self.text.len();
        write!(self.text, "{value:?}").expect("a String takes any text");
        let index = self.end_string(at);
        self.word(index);
    }

    /// 0 for `None`, else the string's index plus one.
    pub fn opt_str(&mut self, value: Option<&str>) {
        match value {
            None => self.word(0),
            Some(value) => {
                let index = self.string(value);
                self.word(index + 1);
            }
        }
    }

    pub fn span(&mut self, span: Option<Span>) {
        let mapped = span.and_then(|span| {
            let start = self.offsets.get(span.start)?;
            let end = self.offsets.get(span.end)?;
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
            Inline::CharacterReference(node) => {
                self.head(kind::CHARACTER_REFERENCE, &node.meta);
                self.str(&node.reference);
                self.opt_str(node.value().as_deref());
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
            Inline::Shortcode(node) => {
                self.head(kind::SHORTCODE, &node.meta);
                self.str(&node.name);
                self.opt_str(node.glyph());
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
            self.name(&token.role);
        }
    }

    /// Diagnostics, with their severity and code as their Rust names, and
    /// whether a path follows, then its indices.
    pub fn diagnostics(&mut self, diagnostics: &[Diagnostic]) {
        self.word(diagnostics.len() as u32);
        for diagnostic in diagnostics {
            self.name(&diagnostic.severity);
            self.name(&diagnostic.code);
            self.span(diagnostic.span);
            self.bool(diagnostic.path.is_some());
            if let Some(path) = &diagnostic.path {
                self.word(path.0.len() as u32);
                for &index in &path.0 {
                    self.word(index as u32);
                }
            }
            self.str(&diagnostic.message);
        }
    }

    pub fn finish(self) -> Vec<u8> {
        let mut bytes = self.head;
        let word_count = (bytes.len() - 8) / 4;
        let string_count = self.strings.len() / 8;
        bytes[..4].copy_from_slice(&(word_count as u32).to_le_bytes());
        bytes[4..8].copy_from_slice(&(string_count as u32).to_le_bytes());
        bytes.reserve_exact(self.strings.len() + self.text.len());
        bytes.extend_from_slice(&self.strings);
        bytes.extend_from_slice(self.text.as_bytes());
        // The message outlives the call as its result.
        bytes.shrink_to_fit();
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

/// Reads a message, in place.
pub struct Reader<'m> {
    /// The words, as their bytes.
    words: &'m [u8],
    at: usize,
    /// The start and length of each string, as their bytes.
    strings: &'m [u8],
    text: &'m str,
    /// Where the string read last ends in `text`, in UTF-16 code units and in
    /// bytes: strings are read mostly in order, and the next is found from
    /// there.
    read: Cell<(usize, usize)>,
    /// The byte offset of each UTF-16 offset spans count in.
    offsets: Offsets<'m>,
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

/// The most a list reserves before reading its items: the lists open at once
/// in a tree of `MAX_DEPTH` levels reserve a few MiB, however many items a
/// forged message counts.
const LIST_RESERVE_BYTES: usize = 16 << 10;

fn le_word(bytes: &[u8], at: usize) -> Read<u32> {
    match bytes.get(at..at + 4) {
        Some(word) => Ok(u32::from_le_bytes([word[0], word[1], word[2], word[3]])),
        None => malformed("the message ends early"),
    }
}

/// The little-endian word at byte `at` of `bytes`, which holds it: one
/// bounds check for its four bytes.
#[inline]
fn le(bytes: &[u8], at: usize) -> u32 {
    let word: [u8; 4] = bytes[at..at + 4].try_into().expect("four bytes");
    u32::from_le_bytes(word)
}

/// The byte offset of UTF-16 offset `to` in `text`, found on from `from`, a
/// UTF-16 offset and its byte offset; none past the end of `text`, before
/// `from`, or inside a char.
fn byte_offset(text: &str, (mut units, mut at): (usize, usize), to: usize) -> Option<usize> {
    let ahead = to.checked_sub(units)?;
    let rest = text.get(at..)?;
    // In ASCII a byte is a unit.
    if rest.as_bytes().get(..ahead).is_some_and(<[u8]>::is_ascii) {
        return Some(at + ahead);
    }
    for char in rest.chars() {
        if units >= to {
            break;
        }
        units += char.len_utf16();
        at += char.len_utf8();
    }
    (units == to).then_some(at)
}

impl<'m> Reader<'m> {
    pub fn new(message: &'m [u8], offsets: &'m [u32]) -> Read<Self> {
        Self::with_offsets(message, Offsets::Table(offsets))
    }

    pub fn with_offsets(message: &'m [u8], offsets: Offsets<'m>) -> Read<Self> {
        let word_count = le_word(message, 0)? as usize;
        let string_count = le_word(message, 4)? as usize;
        // A forged count may overflow these ends; no message holds what it
        // counts then, so the message ends early.
        let ends = word_count
            .checked_mul(4)
            .and_then(|words| words.checked_add(8))
            .and_then(|words_end| {
                let strings_end = string_count.checked_mul(8)?.checked_add(words_end)?;
                Some((words_end, strings_end))
            });
        let Some((words_end, strings_end)) = ends.filter(|&(_, end)| end <= message.len()) else {
            return malformed("the message ends early");
        };
        let Ok(text) = std::str::from_utf8(&message[strings_end..]) else {
            return malformed("the strings are not UTF-8");
        };
        Ok(Self {
            words: &message[8..words_end],
            at: 0,
            strings: &message[words_end..strings_end],
            text,
            read: Cell::new((0, 0)),
            offsets,
            depth: 0,
            sourced: false,
        })
    }

    pub fn word(&mut self) -> Read<u32> {
        let word = self.peek()?;
        self.at += 1;
        Ok(word)
    }

    /// The next word, left to read.
    fn peek(&self) -> Read<u32> {
        if self.at == self.word_count() {
            return malformed("the tree ends early");
        }
        Ok(le(self.words, 4 * self.at))
    }

    fn word_count(&self) -> usize {
        self.words.len() / 4
    }

    fn bool(&mut self) -> Read<bool> {
        match self.word()? {
            0 => Ok(false),
            1 => Ok(true),
            other => malformed(format!("{other} is not a boolean")),
        }
    }

    fn string(&self, index: usize) -> Read<String> {
        if index >= self.strings.len() / 8 {
            return malformed(format!("no string {index}"));
        }
        let start = le(self.strings, 8 * index) as usize;
        let length = le(self.strings, 8 * index + 4) as usize;
        let Some(end) = start.checked_add(length) else {
            return malformed(format!("string {index} lies outside the message"));
        };
        let read = match self.read.get() {
            read @ (units, _) if units <= start => read,
            _ => (0, 0),
        };
        let range = byte_offset(self.text, read, start)
            .and_then(|from| Some((from, byte_offset(self.text, (start, from), end)?)));
        let Some((from, to)) = range else {
            return malformed(format!("string {index} lies outside the message"));
        };
        self.read.set((end, to));
        Ok(self.text[from..to].to_owned())
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
        if count > self.word_count() - self.at {
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
        self.finish(document)
    }

    /// The document whose top-level blocks the message lists, each a block or
    /// `kind::BASE_BLOCK` and the index of a top-level block of `parsed`,
    /// which stands for that block, and whose span is `parsed`'s: the
    /// document `document` reads from a message that holds each such block
    /// in place of its index.
    pub fn document_against(&mut self, parsed: &Document) -> Read<Document> {
        let blocks = self.nested(|reader| {
            reader.list(|reader| {
                if reader.peek()? != kind::BASE_BLOCK {
                    return reader.block();
                }
                reader.at += 1;
                let index = reader.word()?;
                match parsed.children.get(index as usize) {
                    Some(block) => Ok(block.clone()),
                    None => malformed(format!("the parse holds no block {index}")),
                }
            })
        })?;
        let mut document = Document::new(blocks);
        document.meta = parsed.meta.clone();
        self.finish(document)
    }

    /// `document`, read whole from the message.
    fn finish(&self, mut document: Document) -> Read<Document> {
        if self.at != self.word_count() {
            return malformed("words follow the tree");
        }
        // A leaf's source reads with the definitions of the whole tree.
        if self.sourced && Definitions::of(&document) != Definitions::default() {
            document.reread_kept_sources();
        }
        Ok(document)
    }

    pub fn blocks(&mut self) -> Read<Vec<Block>> {
        self.nested(|reader| reader.list(Self::block))
    }

    pub fn inlines(&mut self) -> Read<Vec<Inline>> {
        self.nested(|reader| reader.list(Self::inline))
    }

    /// A count, then as many items, each read by `read`, into a vector sized
    /// for them up to `LIST_RESERVE_BYTES`: a forged message counts more
    /// items than it holds, at every level of a deep tree, and the vector
    /// grows past the hint only as items are read.
    fn list<T>(&mut self, mut read: impl FnMut(&mut Self) -> Read<T>) -> Read<Vec<T>> {
        let count = self.count()?;
        let hint = count.min(LIST_RESERVE_BYTES / std::mem::size_of::<T>().max(1));
        let mut items = Vec::with_capacity(hint);
        for _ in 0..count {
            items.push(read(self)?);
        }
        Ok(items)
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
        self.list(|reader| {
            Ok(DirectiveAttribute {
                name: reader.str()?,
                value: reader.opt_str()?,
            })
        })
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
                let mut node = match self.leaf()? {
                    Leaf::Source(text) => {
                        Heading::with_source(depth, &text, &Definitions::default()).0
                    }
                    Leaf::Inlines(children) => Heading::new(depth, children),
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
                let children = self.list(|reader| {
                    reader.expect_kind(kind::LIST_ITEM, "list item")?;
                    Ok(ListItem {
                        meta: reader.meta()?,
                        checked: match reader.word()? {
                            0 => None,
                            1 => Some(false),
                            2 => Some(true),
                            other => return malformed(format!("{other} is not a task state")),
                        },
                        number: match reader.word()? {
                            0 => None,
                            1 => {
                                let low = u64::from(reader.word()?);
                                let high = u64::from(reader.word()?);
                                Some(high << 32 | low)
                            }
                            other => return malformed(format!("{other} is not an item number")),
                        },
                        children: reader.blocks()?,
                    })
                })?;
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
                let alignments = self.list(|reader| {
                    reader.enumerated(
                        &[
                            TableAlignment::None,
                            TableAlignment::Left,
                            TableAlignment::Center,
                            TableAlignment::Right,
                        ],
                        "table alignment",
                    )
                })?;
                let rows = self.list(|reader| {
                    reader.expect_kind(kind::TABLE_ROW, "table row")?;
                    let meta = reader.meta()?;
                    let cells = reader.list(|reader| {
                        reader.expect_kind(kind::TABLE_CELL, "table cell")?;
                        let meta = reader.meta()?;
                        let mut cell = match reader.leaf()? {
                            Leaf::Source(text) => {
                                TableCell::with_source(&text, &Definitions::default()).0
                            }
                            Leaf::Inlines(children) => TableCell::new(children),
                        };
                        cell.meta = meta;
                        Ok(cell)
                    })?;
                    Ok(TableRow { meta, cells })
                })?;
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
            kind::CHARACTER_REFERENCE => {
                let reference = self.str()?;
                // The decoded value is the reference's to give, not the tree's.
                self.opt_str()?;
                Inline::CharacterReference(CharacterReference { meta, reference })
            }
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
            kind::SHORTCODE => {
                let name = self.str()?;
                // The glyph is the name's to give, not the tree's.
                self.opt_str()?;
                Inline::Shortcode(Shortcode { meta, name })
            }
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
            "#  \n\n",
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
    fn references_and_shortcodes_carry_what_they_stand_for() {
        let inlines: Vec<Inline> = vec![
            CharacterReference::new("&amp;").into(),
            CharacterReference::new("&nope;").into(),
            Shortcode {
                meta: NodeMeta::default(),
                name: "smile".into(),
            }
            .into(),
            Shortcode {
                meta: NodeMeta::default(),
                name: "nope".into(),
            }
            .into(),
        ];
        let mut writer = Writer::new(&[0]);
        writer.inlines(&inlines);
        let message = writer.finish();
        let words: Vec<u32> = message[8..]
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
            .collect();
        let reader = Reader::new(&message, &[0]).unwrap();
        // The count, then per node its kind, span, string, and the value or
        // glyph: none, or a string's index plus one.
        let carried: Vec<(u32, Option<String>)> = (0..words[0] as usize)
            .map(|index| {
                let at = 1 + 5 * index;
                let value = match words[at + 4] {
                    0 => None,
                    string => Some(reader.string(string as usize - 1).unwrap()),
                };
                (words[at], value)
            })
            .collect();
        assert_eq!(
            carried,
            [
                (kind::CHARACTER_REFERENCE, Some("&".into())),
                (kind::CHARACTER_REFERENCE, None),
                (kind::SHORTCODE, Some("😄".into())),
                (kind::SHORTCODE, None),
            ]
        );
        // On the way back the values are read past: the nodes are their
        // reference and name alone.
        let mut reader = Reader::new(&message, &[0]).unwrap();
        assert_eq!(reader.inlines().unwrap(), inlines);
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
    fn base_blocks_read_as_the_blocks_of_the_parse() {
        let source = "a\n\n> b\n\n- c\n";
        let parsed = parse(source).document;
        let to_utf16 = utf16_offsets(source);
        let to_bytes = byte_offsets(source);
        let new = parse("*d*\n").document.children;
        let read = |entries: &[Option<u32>], nested: bool| {
            let mut writer = Writer::new(&to_utf16);
            writer.word(entries.len() as u32);
            for entry in entries {
                match entry {
                    Some(index) => writer.words(&[kind::BASE_BLOCK, *index]),
                    None if nested => {
                        // A quote holding a base block, which reads as a
                        // node of no kind with the index as its span.
                        writer.word(kind::BLOCK_QUOTE);
                        writer.span(None);
                        writer.words(&[1, kind::BASE_BLOCK, 0, 0]);
                    }
                    None => writer.block(&new[0]),
                }
            }
            let message = writer.finish();
            Reader::new(&message, &to_bytes).and_then(|mut reader| reader.document_against(&parsed))
        };
        let document = read(&[Some(2), None, Some(0), Some(2)], false).unwrap();
        let children = &parsed.children;
        let expected = [&children[2], &new[0], &children[0], &children[2]];
        assert_eq!(document.children.iter().collect::<Vec<_>>(), expected);
        assert_eq!(document.meta, parsed.meta);
        assert_eq!(
            read(&[Some(0), Some(3)], false).unwrap_err(),
            Malformed("the parse holds no block 3".into())
        );
        assert_eq!(
            read(&[None], true).unwrap_err(),
            Malformed(format!("kind {} where a block belongs", kind::BASE_BLOCK))
        );
    }

    #[test]
    fn no_node_kind_is_the_base_block() {
        assert!(kind::BASE_BLOCK > u32::from(u8::MAX));
        let mut writer = Writer::new(&[0]);
        writer.words(&[kind::BASE_BLOCK, NO_SPAN, 0]);
        let message = writer.finish();
        let read = |what: fn(&mut Reader<'_>) -> Read<()>| {
            Reader::new(&message, &[0]).and_then(|mut reader| what(&mut reader))
        };
        assert_eq!(
            read(|reader| reader.block().map(drop)).unwrap_err(),
            Malformed(format!("kind {} where a block belongs", kind::BASE_BLOCK))
        );
        assert_eq!(
            read(|reader| reader.inline().map(drop)).unwrap_err(),
            Malformed(format!("kind {} where an inline belongs", kind::BASE_BLOCK))
        );
    }

    /// A message of `strings`, each a `(start, length)` pair in UTF-16 code
    /// units, over `text`, with no words.
    fn strings_message(strings: &[(u32, u32)], text: &str) -> Vec<u8> {
        let mut message = Vec::new();
        for word in [0, strings.len() as u32] {
            message.extend(word.to_le_bytes());
        }
        for &(start, length) in strings {
            message.extend(start.to_le_bytes());
            message.extend(length.to_le_bytes());
        }
        message.extend(text.as_bytes());
        message
    }

    #[test]
    fn strings_are_read_across_the_text_between_them() {
        // A lone surrogate written by JavaScript arrives as U+FFFD, kept from
        // the next string's by the `\n` between them; the text holds a gap
        // no string covers, an astral char of two units, and a BOM.
        let text = "a\u{FFFD}\n\u{FFFD}b\n--gap--\u{1F600}x\n\u{FEFF}\n";
        let strings = [(0, 2), (3, 2), (13, 3), (17, 1), (3, 0), (0, 0)];
        let message = strings_message(&strings, text);
        let reader = Reader::new(&message, &[]).unwrap();
        let expected = ["a\u{FFFD}", "\u{FFFD}b", "\u{1F600}x", "\u{FEFF}", "", ""];
        for index in [0, 1, 2, 3, 4, 5, 3, 0, 2, 1, 5] {
            assert_eq!(
                reader.string(index).unwrap(),
                expected[index],
                "string {index}"
            );
        }
    }

    #[test]
    fn strings_outside_the_text_are_refused() {
        let text = "\u{1F600}ab";
        // Past the end, inside the astral char, ending inside it, and a
        // range whose end overflows.
        for strings in [
            [(0, 5)],
            [(5, 0)],
            [(1, 1)],
            [(0, 1)],
            [(u32::MAX, u32::MAX)],
        ] {
            let message = strings_message(&strings, text);
            let reader = Reader::new(&message, &[]).unwrap();
            assert_eq!(
                reader.string(0),
                Err(Malformed("string 0 lies outside the message".into())),
                "{strings:?}"
            );
        }
    }

    #[test]
    fn counts_past_the_message_are_refused() {
        for (words, strings) in [(u32::MAX, 0), (0, u32::MAX), (u32::MAX, u32::MAX), (1, 0)] {
            let mut message = Vec::new();
            message.extend(words.to_le_bytes());
            message.extend(strings.to_le_bytes());
            assert!(
                matches!(Reader::new(&message, &[]), Err(Malformed(error)) if error == "the message ends early"),
                "{words} words, {strings} strings"
            );
        }
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

    /// The system allocator, recording the largest request each thread makes.
    struct Recording;

    std::thread_local! {
        static LARGEST_REQUEST: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    unsafe impl std::alloc::GlobalAlloc for Recording {
        unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
            let _ =
                LARGEST_REQUEST.try_with(|largest| largest.set(largest.get().max(layout.size())));
            std::alloc::System.alloc(layout)
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
            std::alloc::System.dealloc(pointer, layout)
        }
    }

    #[global_allocator]
    static ALLOCATOR: Recording = Recording;

    #[test]
    fn forged_counts_in_a_deep_tree_reserve_little() {
        // A block quote at every level counts every word after it as one of
        // its blocks, and the words left over hold no block.
        const WORDS: usize = 1 << 16;
        let mut words = vec![kind::DOCUMENT, NO_SPAN, 0];
        for _ in 0..MAX_DEPTH - 2 {
            words.push((WORDS - words.len() - 1) as u32);
            words.extend([kind::BLOCK_QUOTE, NO_SPAN, 0]);
        }
        words.push((WORDS - words.len() - 1) as u32);
        words.resize(WORDS, 0);
        let mut message = Vec::new();
        message.extend((WORDS as u32).to_le_bytes());
        message.extend(0u32.to_le_bytes());
        message.extend(words.iter().flat_map(|word| word.to_le_bytes()));

        LARGEST_REQUEST.with(|largest| largest.set(0));
        let read = Reader::new(&message, &[]).and_then(|mut reader| reader.document());
        let largest = LARGEST_REQUEST.with(std::cell::Cell::get);
        assert_eq!(
            read.unwrap_err(),
            Malformed(format!("kind {} where a block belongs", kind::DOCUMENT))
        );
        assert!(
            largest <= LIST_RESERVE_BYTES,
            "a request of {largest} bytes"
        );
    }

    #[test]
    fn a_span_inside_a_surrogate_pair_reads_as_none() {
        let offsets = byte_offsets("😀a");
        assert_eq!(offsets, vec![0, NO_SPAN, 4, 5]);
    }

    /// Checks that the tables of each of `texts` read every offset, and the
    /// two past the end, as `utf16_offsets` and `byte_offsets` give them.
    fn check_offset_tables(texts: &[String]) {
        for text in texts {
            for (table, expected) in [
                (OffsetTable::to_utf16(text), utf16_offsets(text)),
                (OffsetTable::to_bytes(text), byte_offsets(text)),
            ] {
                for offset in 0..expected.len() + 2 {
                    assert_eq!(
                        table.offsets().get(offset),
                        expected.get(offset).copied(),
                        "{text:?} at {offset}"
                    );
                }
            }
        }
    }

    #[test]
    fn offset_tables_read_as_the_offsets() {
        check_offset_tables(&[
            String::new(),
            "ascii only".repeat(30),
            "é中😀a".repeat(100),
            "😀".repeat(150),
            "中".repeat(150),
            format!("a{}", "😀".repeat(63)),
            format!("{}😀b", "x".repeat(63)),
        ]);
    }

    #[test]
    fn offset_tables_read_as_the_offsets_across_blocks_of_ascii() {
        // Runs of ASCII of every length around a block's, between chars of
        // two, three, and four bytes, so that blocks start at, inside, and
        // right after each, and blocks of ASCII fall between blocks with
        // steps.
        let mut texts = Vec::new();
        for run in [0, 1, 2, 31, 62, 63, 64, 65, 127, 128, 129, 200] {
            for char in ["é", "中", "😀"] {
                let ascii = "x".repeat(run);
                texts.push(format!("{ascii}{char}"));
                texts.push(format!("{char}{ascii}"));
                texts.push(format!("{char}{ascii}{char}{ascii}{char}"));
                texts.push(format!("{ascii}{char}{ascii}{char}{ascii}"));
                texts.push(format!("{}{ascii}", char.repeat(run % 7 + 1)));
                texts.push(format!("{ascii}{}", char.repeat(run)));
            }
        }
        check_offset_tables(&texts);
    }

    #[test]
    fn offset_tables_read_as_the_offsets_of_mixed_texts() {
        // Seeded texts of chars of every width in runs of every length, up to
        // a few thousand bytes each, so that blocks fall across chars and
        // runs every way.
        let long = "q".repeat(70);
        let pieces = ["a", "é", "中", "😀", "xyz ", "中文", "😀😀", "\n", &long];
        let mut seed = 7_u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (seed >> 16) as usize
        };
        let mut texts = Vec::new();
        for _ in 0..200 {
            let length = next() % 5000;
            let mut text = String::new();
            while text.len() < length {
                text.push_str(pieces[next() % pieces.len()]);
            }
            texts.push(text);
        }
        check_offset_tables(&texts);
    }

    #[test]
    fn strings_read_back_by_their_utf16_offsets() {
        let strings = ["", "a", "😀", "é中", "x😀y", "\u{feff}z", ""];
        let mut writer = Writer::new(&[]);
        for string in strings {
            writer.str(string);
        }
        let message = writer.finish();
        let reader = Reader::new(&message, &[]).unwrap();
        for index in (0..strings.len()).chain([3, 1, 5]) {
            assert_eq!(reader.string(index).unwrap(), strings[index]);
        }
        assert_eq!(
            reader.string(strings.len()),
            Err(Malformed(format!("no string {}", strings.len())))
        );
    }
}
