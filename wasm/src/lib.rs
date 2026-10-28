//! The WebAssembly binding of `markdown-syntax`: a C ABI that `js/index.js`
//! wraps. A parsed document stays in WebAssembly behind a handle, with the
//! source it holds, so the source crosses once; every result is written to
//! one buffer that JavaScript reads at `result_ptr()` and `result_len()`.
//!
//! Every export is `unsafe` only in taking pointers JavaScript wrote through
//! `buffer_alloc`; the JavaScript wrapper is the one caller.

pub mod wire;

use std::cell::{OnceCell, RefCell};

use markdown_syntax::__private::{KeptSources, LeafSource};
use markdown_syntax::prelude::*;
use markdown_syntax::FenceMarker;

use wire::{OffsetTable, Offsets, Reader, Writer, NO_SPAN};

/// A parsed document, the inline sources of its leaves, and the offset
/// tables its spans cross by. Calls are lent an entry, never given it to
/// change, so the document and its sources stay as the parse read both.
struct Entry {
    document: Document,
    /// The inline source of each leaf of `document`, read with its parse,
    /// which every write against it takes in place of reading them again.
    sources: KeptSources,
    /// The UTF-16 offset of each byte offset of the source.
    to_utf16: OffsetTable,
    /// The byte offset of each UTF-16 offset of the source, built on the
    /// first write.
    to_bytes: OnceCell<OffsetTable>,
}

thread_local! {
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static DOCUMENTS: RefCell<Vec<Option<Entry>>> = const { RefCell::new(Vec::new()) };
    /// The last message of labels `parse_inline` took, and its definitions:
    /// an editor passes the same labels on every keystroke.
    static LABELS: RefCell<Option<(Vec<u8>, Definitions)>> = const { RefCell::new(None) };
}

fn set_result(bytes: Vec<u8>) {
    RESULT.with(|result| *result.borrow_mut() = bytes);
}

/// Releases the last call's result, which JavaScript has read by the next
/// call, so that its memory serves the work of this one.
fn clear_result() {
    set_result(Vec::new());
}

/// `len` bytes for JavaScript to write an argument into.
#[no_mangle]
pub extern "C" fn buffer_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// Releases what `buffer_alloc` returned.
///
/// # Safety
/// `pointer` and `len` are a pair `buffer_alloc` returned and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn buffer_free(pointer: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(pointer, 0, len));
}

#[no_mangle]
pub extern "C" fn result_ptr() -> *const u8 {
    RESULT.with(|result| result.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn result_len() -> usize {
    RESULT.with(|result| result.borrow().len())
}

/// # Safety
/// `pointer` and `len` hold UTF-8 that JavaScript wrote.
unsafe fn text<'a>(pointer: *const u8, len: usize) -> &'a str {
    if len == 0 {
        return "";
    }
    match std::str::from_utf8(std::slice::from_raw_parts(pointer, len)) {
        Ok(text) => text,
        Err(_) => "",
    }
}

/// # Safety
/// `pointer` and `len` hold bytes that JavaScript wrote.
unsafe fn bytes<'a>(pointer: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        return &[];
    }
    std::slice::from_raw_parts(pointer, len)
}

fn with_entry<T>(handle: u32, work: impl FnOnce(&Entry) -> T) -> Option<T> {
    DOCUMENTS.with(|documents| {
        let documents = documents.borrow();
        let entry = documents.get((handle as usize).checked_sub(1)?)?.as_ref()?;
        Some(work(entry))
    })
}

/// Parses the UTF-8 source at `pointer` and keeps the document; the result
/// is its tree, its diagnostics, and its leaf sources, each a path (its
/// length, then its indices) and an inline source as `inline_source` writes
/// it. Returns the document's handle.
///
/// # Safety
/// `pointer` and `len` hold UTF-8 that JavaScript wrote.
#[no_mangle]
pub unsafe extern "C" fn parse(pointer: *const u8, len: usize) -> u32 {
    let source = text(pointer, len);
    clear_result();
    let to_utf16 = OffsetTable::to_utf16(source);
    let mut writer = Writer::with_offsets(to_utf16.offsets());
    let (output, sources) =
        markdown_syntax::__private::parse_with_leaf_contents(source, |output, leaves| {
            writer.document(&output.document);
            writer.diagnostics(&output.diagnostics);
            let count = writer.reserve_word();
            let mut written = 0;
            let mut runs = Default::default();
            let sources = leaves.each_keeping(|path, source| {
                writer.word(path.len() as u32);
                for &index in path {
                    writer.word(index as u32);
                }
                write_inline_source(&mut writer, source, to_utf16.offsets(), &mut runs);
                written += 1;
            });
            writer.set_word(count, written);
            sources
        });
    set_result(writer.finish());
    let entry = Entry {
        document: output.document,
        sources,
        to_utf16,
        to_bytes: OnceCell::new(),
    };
    DOCUMENTS.with(|documents| {
        let mut documents = documents.borrow_mut();
        match documents.iter().position(Option::is_none) {
            Some(free) => {
                documents[free] = Some(entry);
                free as u32 + 1
            }
            None => {
                documents.push(Some(entry));
                documents.len() as u32
            }
        }
    })
}

/// Releases the document behind `handle`. Returns 0, or 1 for a handle that
/// holds none.
#[no_mangle]
pub extern "C" fn document_free(handle: u32) -> u32 {
    DOCUMENTS.with(|documents| {
        let mut documents = documents.borrow_mut();
        match (handle as usize)
            .checked_sub(1)
            .and_then(|index| documents.get_mut(index))
        {
            Some(entry @ Some(_)) => {
                *entry = None;
                0
            }
            _ => 1,
        }
    })
}

/// Parses the UTF-8 text at `pointer` as one leaf block's inline content,
/// with the definitions whose labels the message at `labels` holds as its
/// strings; the result is the inlines, their tokens, and diagnostics.
///
/// # Safety
/// The pointers and lengths hold what JavaScript wrote.
#[no_mangle]
pub unsafe extern "C" fn parse_inline(
    pointer: *const u8,
    len: usize,
    labels: *const u8,
    labels_len: usize,
) -> u32 {
    let source = text(pointer, len);
    let message = bytes(labels, labels_len);
    LABELS.with(|cached| {
        let mut cached = cached.borrow_mut();
        let definitions = match &*cached {
            Some((labels, definitions)) if labels == message => definitions,
            _ => {
                let labels = match Reader::new(message, &[]).and_then(|mut reader| {
                    let count = reader.word()? as usize;
                    (0..count)
                        .map(|_| reader.label())
                        .collect::<Result<Vec<_>, _>>()
                }) {
                    Ok(labels) => labels,
                    Err(error) => {
                        set_result(error.0.into_bytes());
                        return 2;
                    }
                };
                &cached
                    .insert((message.to_vec(), Definitions::new(labels)))
                    .1
            }
        };
        let output = markdown_syntax::parse_inline(source, definitions);
        let to_utf16 = OffsetTable::to_utf16(source);
        let mut writer = Writer::with_offsets(to_utf16.offsets());
        writer.inlines(&output.inlines);
        writer.tokens(&output.tokens);
        writer.diagnostics(&output.diagnostics);
        set_result(writer.finish());
        0
    })
}

/// The inline source of the leaf at the path whose indices are the
/// little-endian `u32`s in the `path_len` bytes at `path`, as
/// `write_inline_source` writes it. Returns 0 for none, 1 with a result, or 3
/// for a handle that holds no document.
///
/// # Safety
/// `path` and `path_len` hold bytes that JavaScript wrote.
#[no_mangle]
pub unsafe extern "C" fn inline_source(handle: u32, path: *const u8, path_len: usize) -> u32 {
    let path: Vec<usize> = bytes(path, path_len)
        .chunks_exact(4)
        .map(|index| u32::from_le_bytes([index[0], index[1], index[2], index[3]]) as usize)
        .collect();
    with_entry(handle, |entry| {
        let Some(source) = entry.document.inline_source(&NodePath(path)) else {
            return 0;
        };
        let mut writer = Writer::new(&[]);
        write_inline_source(
            &mut writer,
            LeafSource::of(&source),
            entry.to_utf16.offsets(),
            &mut Default::default(),
        );
        set_result(writer.finish());
        1
    })
    .unwrap_or(3)
}

/// One table of `write_inline_source`, built a run of offsets at a time:
/// the start and mapped offset of each run, and what an offset continuing
/// the last run maps to.
#[derive(Default)]
struct Runs {
    words: Vec<u32>,
    next: Option<u32>,
}

impl Runs {
    fn push(&mut self, unit: u32, mapped: u32) {
        if mapped == NO_SPAN || self.next != Some(mapped) {
            self.words.extend([unit, mapped]);
        }
        self.next = (mapped != NO_SPAN).then(|| mapped + 1);
    }

    /// Maps the `count` offsets from `unit` on to `mapped` and one further
    /// each, as pushing each of them does.
    fn linear(&mut self, unit: u32, mapped: u32, count: u32) {
        self.push(unit, mapped);
        self.next = Some(mapped + count);
    }

    fn write(&mut self, writer: &mut Writer) {
        writer.word(self.words.len() as u32 / 2);
        writer.words(&self.words);
        self.words.clear();
        self.next = None;
    }
}

/// Writes an inline source: its text, then where each UTF-16 offset of the
/// text and its end maps in the source, in UTF-16 offsets, as a span's start
/// and then as a span's end. Each table is its runs: a count, then for each
/// run the offset it starts at and what that offset maps to, the offsets
/// after it mapping one further each; an offset inside a surrogate pair is
/// a run of its own that maps to `NO_SPAN`.
fn write_inline_source(
    writer: &mut Writer,
    source: LeafSource<'_>,
    to_utf16: Offsets,
    runs: &mut (Runs, Runs),
) {
    // The tables of an ASCII source map its offsets as they are.
    match to_utf16 {
        Offsets::Same(_) => tabulate(writer, source, |offset| offset as u32, runs),
        offsets => tabulate(
            writer,
            source,
            |offset| offsets.get(offset).expect("an offset of the source"),
            runs,
        ),
    }
}

/// Fills the tables of `source` a run of its text at a time, each offset
/// mapped as `LeafSource::runs` describes: as a span's start, and as the end
/// of the span from 0 to it.
fn tabulate(
    writer: &mut Writer,
    source: LeafSource<'_>,
    utf16: impl Fn(usize) -> u32,
    (starts, ends): &mut (Runs, Runs),
) {
    let text = source.text.as_bytes();
    // Where a span from 0 starts, which no end falls below, and where a span
    // starting at the text's end starts.
    let (mut first, mut last) = (0, 0);
    let mut unit = 0;
    for (index, (within, from)) in source.runs().enumerate() {
        if index == 0 {
            first = if within.is_empty() {
                from.end
            } else {
                from.start
            };
            ends.push(0, utf16(from.start.max(first)));
        }
        last = from.end;
        let verbatim = within.len() == from.len();
        if verbatim && from.start >= first {
            // The text of a run read byte for byte is the source's, so its
            // UTF-16 offsets map one for one from where it starts, but for the
            // one inside each surrogate pair; and no end in it falls below
            // `first`. The starts table takes its offsets from `start` up to
            // `end`, the ends table those after `start` up to and with `end`.
            let (start, mapped) = (unit, utf16(from.start));
            let end = start + (utf16(from.end) - mapped);
            let at = |unit: u32| mapped + (unit - start);
            let (mut starts_from, mut ends_from) = (start, start + 1);
            for (offset, _) in text[within]
                .iter()
                .enumerate()
                .filter(|&(_, &byte)| byte >= 0xf0)
            {
                let pair = start + (utf16(from.start + offset) - mapped);
                starts.linear(starts_from, at(starts_from), pair + 1 - starts_from);
                starts.push(pair + 1, NO_SPAN);
                if ends_from <= pair {
                    ends.linear(ends_from, at(ends_from), pair + 1 - ends_from);
                }
                ends.push(pair + 1, NO_SPAN);
                (starts_from, ends_from) = (pair + 2, pair + 2);
            }
            if starts_from < end {
                starts.linear(starts_from, at(starts_from), end - starts_from);
            }
            if ends_from <= end {
                ends.linear(ends_from, at(ends_from), end + 1 - ends_from);
            }
            unit = end;
            continue;
        }
        // Any other run maps a char at a time. A char of four bytes takes two
        // units, and the offset between them maps to `NO_SPAN`.
        let mut before = within.start;
        while before < within.end {
            let width = match text[before] {
                0..=0x7f => 1,
                0x80..=0xdf => 2,
                0xe0..=0xef => 3,
                _ => 4,
            };
            let at = before + width;
            let (start, end) = if verbatim {
                (
                    from.start + (before - within.start),
                    from.start + (at - within.start),
                )
            } else {
                (from.start, from.end)
            };
            starts.push(unit, utf16(start));
            if width == 4 {
                starts.push(unit + 1, NO_SPAN);
                ends.push(unit + 1, NO_SPAN);
                unit += 2;
            } else {
                unit += 1;
            }
            ends.push(unit, utf16(end.max(first)));
            before = at;
        }
    }
    starts.push(unit, utf16(last));
    // The tables come after the text, and cover its UTF-16 offsets: the runs
    // cover the text from 0 without gaps, and a run read byte for byte is
    // as long in UTF-16 units as its source range.
    debug_assert_eq!(
        unit as usize,
        source.text.encode_utf16().count(),
        "the tables of {:?}",
        source.text
    );
    writer.str(source.text);
    starts.write(writer);
    ends.write(writer);
}

/// Writes the tree in the message at `tree` with the source of the document
/// behind `handle` and the given options. Returns 0 with the UTF-8 Markdown
/// as the result, 1 with the diagnostics that refused the tree, 2 with a
/// UTF-8 message for a malformed tree, or 3 for a handle that holds no
/// document.
///
/// # Safety
/// `tree` and `tree_len` hold a message JavaScript wrote.
#[no_mangle]
pub unsafe extern "C" fn to_markdown(
    handle: u32,
    tree: *const u8,
    tree_len: usize,
    line_ending: u32,
    final_newline: u32,
    bullet: u32,
    ordered_delimiter: u32,
    fence_marker: u32,
) -> u32 {
    let options = options(
        line_ending,
        final_newline,
        bullet,
        ordered_delimiter,
        fence_marker,
    );
    write(handle, bytes(tree, tree_len), &options, |reader, _| {
        reader.document()
    })
}

/// Writes the document whose top-level blocks the message at `blocks` lists,
/// each a block or the index of a top-level block of the document behind
/// `handle`, as `Reader::document_against` reads them, with that document's
/// source and the given options; returns as `to_markdown` does.
///
/// # Safety
/// `blocks` and `blocks_len` hold a message JavaScript wrote.
#[no_mangle]
pub unsafe extern "C" fn to_markdown_blocks(
    handle: u32,
    blocks: *const u8,
    blocks_len: usize,
    line_ending: u32,
    final_newline: u32,
    bullet: u32,
    ordered_delimiter: u32,
    fence_marker: u32,
) -> u32 {
    let options = options(
        line_ending,
        final_newline,
        bullet,
        ordered_delimiter,
        fence_marker,
    );
    write(
        handle,
        bytes(blocks, blocks_len),
        &options,
        |reader, parsed| reader.document_against(parsed),
    )
}

/// The options the words JavaScript passes stand for.
fn options(
    line_ending: u32,
    final_newline: u32,
    bullet: u32,
    ordered_delimiter: u32,
    fence_marker: u32,
) -> SerializeOptions {
    let mut options = SerializeOptions::default();
    options.line_ending = if line_ending == 1 {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    };
    options.final_newline = final_newline != 0;
    options.bullet = match bullet {
        1 => Some(BulletMarker::Dash),
        2 => Some(BulletMarker::Asterisk),
        3 => Some(BulletMarker::Plus),
        _ => None,
    };
    options.ordered_delimiter = match ordered_delimiter {
        1 => Some(OrderedDelimiter::Period),
        2 => Some(OrderedDelimiter::Paren),
        _ => None,
    };
    options.fence_marker = match fence_marker {
        1 => Some(FenceMarker::Backtick),
        2 => Some(FenceMarker::Tilde),
        _ => None,
    };
    options
}

/// Writes the tree `read` reads from `message`, given the kept parse, with
/// the source of the document behind `handle`; returns as `to_markdown`
/// does.
fn write(
    handle: u32,
    message: &[u8],
    options: &SerializeOptions,
    read: fn(&mut Reader<'_>, &Document) -> Result<Document, wire::Malformed>,
) -> u32 {
    clear_result();
    with_entry(handle, |entry| {
        let source = entry.document.source().unwrap_or_default();
        let to_bytes = entry.to_bytes.get_or_init(|| OffsetTable::to_bytes(source));
        let decoded = match Reader::with_offsets(message, to_bytes.offsets())
            .and_then(|mut reader| read(&mut reader, &entry.document))
        {
            Ok(decoded) => decoded,
            Err(error) => {
                set_result(error.0.into_bytes());
                return 2;
            }
        };
        // The kept document is the parse of the source the decoded tree is
        // written with, the kept sources were read with that parse, and each
        // decoded leaf that keeps its text holds the inlines the text reads
        // as with the decoded tree's definitions, as `Reader` leaves every
        // document it reads.
        let written = markdown_syntax::__private::to_markdown_against_with(
            &decoded,
            &entry.document,
            &entry.sources,
            options,
        );
        match written {
            Ok(markdown) => {
                set_result(markdown.into_bytes());
                0
            }
            Err(SerializeError::InvalidDocument(diagnostics)) => {
                let mut writer = Writer::with_offsets(entry.to_utf16.offsets());
                writer.diagnostics(&diagnostics);
                set_result(writer.finish());
                1
            }
        }
    })
    .unwrap_or(3)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use wire::{byte_offsets, utf16_offsets};

    /// The tables `write_inline_source` writes for `source`, by their
    /// definition: where each UTF-16 offset of the text, and its end, maps
    /// as a span's start (`to_source`) and as a span's end (the end of the
    /// span from 0 to it), in UTF-16 offsets of the document's source, as
    /// runs.
    fn defined_tables(source: &InlineSource, to_utf16: &[u32]) -> [Vec<(u32, u32)>; 2] {
        let offsets = byte_offsets(&source.text);
        [false, true].map(|at_end| {
            let mut runs = Vec::new();
            let mut previous = None;
            for (unit, &byte) in offsets.iter().enumerate() {
                let mapped = if byte == NO_SPAN {
                    NO_SPAN
                } else if at_end {
                    to_utf16[source.span_to_source(Span::new(0, byte as usize)).end]
                } else {
                    to_utf16[source.to_source(byte as usize)]
                };
                let continues = matches!(previous, Some(previous) if previous != NO_SPAN
                    && mapped != NO_SPAN
                    && mapped == previous + 1);
                if !continues {
                    runs.push((unit as u32, mapped));
                }
                previous = Some(mapped);
            }
            runs
        })
    }

    /// The tables a message `write_inline_source` wrote holds: its words are
    /// the text's string index, then each table's run count and runs.
    fn written_tables(message: &[u8]) -> [Vec<(u32, u32)>; 2] {
        let word = |index: usize| {
            let at = 8 + 4 * index;
            u32::from_le_bytes(message[at..at + 4].try_into().unwrap())
        };
        let mut at = 1;
        [(), ()].map(|_| {
            let count = word(at) as usize;
            let runs = (0..count)
                .map(|run| (word(at + 1 + 2 * run), word(at + 2 + 2 * run)))
                .collect();
            at += 1 + 2 * count;
            runs
        })
    }

    /// Every `.md` and `.cases` file of the core crate's fixture corpus, each
    /// read whole as one document.
    fn fixture_files() -> Vec<(String, String)> {
        fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    collect(&path, files);
                } else if path
                    .extension()
                    .is_some_and(|extension| extension == "md" || extension == "cases")
                {
                    files.push(path);
                }
            }
        }
        let mut files = Vec::new();
        collect(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures"),
            &mut files,
        );
        files.sort();
        assert!(files.len() > 200, "only {} fixture files", files.len());
        files
            .into_iter()
            .map(|path| {
                let text = std::fs::read_to_string(&path).unwrap();
                (path.display().to_string(), text)
            })
            .collect()
    }

    /// `blocks` with every leaf given as its text, as an editor sends a leaf
    /// it typed in: built by `with_source` with no definitions and its
    /// inlines left out, so that `Writer` sends the text alone, as
    /// `js/index.js` does. `text` gives the text of the leaf at a path.
    fn leaves_as_texts(
        blocks: &mut [Block],
        path: &mut Vec<usize>,
        text: &impl Fn(&[usize]) -> Option<String>,
    ) {
        let none = Definitions::default();
        for (index, block) in blocks.iter_mut().enumerate() {
            path.push(index);
            match block {
                Block::Paragraph(node) => {
                    if let Some(text) = text(path) {
                        let meta = node.meta.clone();
                        *node = Paragraph::with_source(&text, &none).0;
                        node.meta = meta;
                        node.children.clear();
                    }
                }
                Block::Heading(node) => {
                    if let Some(text) = text(path) {
                        let meta = node.meta.clone();
                        *node = Heading::with_source(node.depth, &text, &none).0;
                        node.meta = meta;
                        node.children.clear();
                    }
                }
                Block::LeafDirective(node) => {
                    if let Some(text) = text(path) {
                        let mut new = LeafDirective::with_source(&*node.name, &text, &none).0;
                        new.meta = node.meta.clone();
                        new.attributes = std::mem::take(&mut node.attributes);
                        new.label.clear();
                        *node = new;
                    }
                }
                Block::ContainerDirective(node) => {
                    leaves_as_texts(&mut node.children, path, text);
                    if let Some(text) = text(path) {
                        let children = std::mem::take(&mut node.children);
                        let mut new =
                            ContainerDirective::with_source(&*node.name, &text, children, &none).0;
                        new.meta = node.meta.clone();
                        new.attributes = std::mem::take(&mut node.attributes);
                        new.label.clear();
                        *node = new;
                    }
                }
                Block::Table(node) => {
                    for (row, cells) in node.rows.iter_mut().enumerate() {
                        for (cell, node) in cells.cells.iter_mut().enumerate() {
                            path.extend([row, cell]);
                            if let Some(text) = text(path) {
                                let meta = node.meta.clone();
                                *node = TableCell::with_source(&text, &none).0;
                                node.meta = meta;
                                node.children.clear();
                            }
                            path.truncate(path.len() - 2);
                        }
                    }
                }
                Block::BlockQuote(node) => leaves_as_texts(&mut node.children, path, text),
                Block::Alert(node) => leaves_as_texts(&mut node.children, path, text),
                Block::FootnoteDefinition(node) => {
                    leaves_as_texts(&mut node.children, path, text);
                }
                Block::HtmlContainer(node) => {
                    if let HtmlContainerContent::Blocks(children) = &mut node.content {
                        leaves_as_texts(children, path, text);
                    }
                }
                Block::List(node) => {
                    for (item, node) in node.children.iter_mut().enumerate() {
                        path.push(item);
                        leaves_as_texts(&mut node.children, path, text);
                        path.pop();
                    }
                }
                _ => {}
            }
            path.pop();
        }
    }

    /// The blocks of a container in `blocks`: a quote's, a list item's, or a
    /// container directive's, at `path`.
    fn container_at<'a>(blocks: &'a mut Vec<Block>, path: &[usize]) -> &'a mut Vec<Block> {
        let Some((&index, rest)) = path.split_first() else {
            return blocks;
        };
        match &mut blocks[index] {
            Block::BlockQuote(node) => container_at(&mut node.children, rest),
            Block::ContainerDirective(node) => container_at(&mut node.children, rest),
            Block::List(node) => {
                let (&item, rest) = rest.split_first().unwrap();
                container_at(&mut node.children[item].children, rest)
            }
            other => panic!("no container at {path:?}: {other:?}"),
        }
    }

    /// The last of `blocks`, a definition, taken out.
    fn pop_definition(blocks: &mut Vec<Block>) -> Block {
        let block = blocks.pop();
        assert!(matches!(block, Some(Block::Definition(_))), "{block:?}");
        block.unwrap()
    }

    /// A definition of `label`, built in JavaScript: it has no span.
    fn definition(label: &str) -> Block {
        let parsed = markdown_syntax::parse(&format!("[{label}]: /{label}\n")).document;
        let Some(Block::Definition(mut node)) = parsed.children.into_iter().next() else {
            panic!("no definition of {label}");
        };
        node.meta = NodeMeta::default();
        Block::Definition(node)
    }

    /// The write `to_markdown` makes takes each leaf that keeps its text as
    /// reading as its inlines, which holds only because `Reader::document`
    /// reads the leaves `js/index.js` sends as text again with the tree's
    /// definitions. Leaves sent as text, with definitions at the top level
    /// and in containers added and removed against the parse, write as the
    /// public write writes the decoded tree with the parse's source.
    #[test]
    fn leaves_sent_as_text_write_as_the_public_write_writes_them() {
        let base = "[a] and [b] *x*\n\n> [c] *y* [a]\n>\n> [c]: /c\n\n- [d] [b]\n\n  [d]: /d\n\n\
                    # [a] [c]\n\n| [a] | [b] |\n|-|-|\n| [c] | [d] |\n\n::leaf[[a] l]\n\n\
                    :::box[[b] m]\n[a] [e]\n:::\n\n[a]: /a\n";
        // Each edit of the parse's top-level blocks, named.
        type Edit = fn(&mut Vec<Block>);
        let edits: [(&str, Edit); 8] = [
            ("none", |_| {}),
            ("a top-level definition removed", |blocks| {
                pop_definition(blocks);
            }),
            ("a top-level definition added", |blocks| {
                blocks.push(definition("b"));
            }),
            ("a definition added in the quote", |blocks| {
                container_at(blocks, &[1]).push(definition("b"));
            }),
            ("the quote's definition removed", |blocks| {
                pop_definition(container_at(blocks, &[1]));
            }),
            ("the list item's definition removed", |blocks| {
                pop_definition(container_at(blocks, &[2, 0]));
            }),
            ("a definition added in the directive", |blocks| {
                container_at(blocks, &[6]).insert(0, definition("e"));
            }),
            ("every definition moved into the directive", |blocks| {
                let top = pop_definition(blocks);
                let quote = pop_definition(container_at(blocks, &[1]));
                let item = pop_definition(container_at(blocks, &[2, 0]));
                container_at(blocks, &[6]).extend([top, quote, item]);
            }),
        ];
        let mut inputs = vec![("the base".to_string(), base.to_string())];
        for (name, text) in fixture_files() {
            inputs.push((format!("{name} (CRLF)"), text.replace('\n', "\r\n")));
            inputs.push((name, text));
        }
        let mut crlf = SerializeOptions::default();
        crlf.line_ending = LineEnding::CrLf;
        let options = [SerializeOptions::default(), crlf];
        let mut written = 0;
        for (at, input) in &inputs {
            let (output, kept) =
                markdown_syntax::__private::parse_with_leaf_contents(input, |_, contents| {
                    contents.each_keeping(|_, _| {})
                });
            let parsed = output.document;
            let text = |path: &[usize]| {
                parsed
                    .inline_source(&NodePath(path.to_vec()))
                    .map(|source| source.text)
            };
            let to_utf16 = utf16_offsets(input);
            let to_bytes = byte_offsets(input);
            let edits: &[(&str, Edit)] = if at == "the base" {
                &edits
            } else {
                // A fixture: its leaves as text, with and without its
                // top-level definitions.
                &[
                    ("none", |_| {}),
                    ("its top-level definitions removed", |blocks| {
                        blocks.retain(|block| !matches!(block, Block::Definition(_)));
                    }),
                ]
            };
            for (edit, apply) in edits {
                let mut tree = parsed.clone();
                leaves_as_texts(&mut tree.children, &mut Vec::new(), &text);
                apply(&mut tree.children);
                let mut writer = Writer::new(&to_utf16);
                writer.document(&tree);
                let message = writer.finish();
                let decoded = Reader::new(&message, &to_bytes)
                    .and_then(|mut reader| reader.document())
                    .unwrap();
                // The decoded tree with the parse's source, as the public
                // write takes it.
                let mut public = parsed.clone();
                public.meta = decoded.meta.clone();
                public.children = decoded.children.clone();
                for options in &options {
                    assert_eq!(
                        markdown_syntax::__private::to_markdown_against_with(
                            &decoded, &parsed, &kept, options
                        ),
                        public.to_markdown_with(options),
                        "{at}: {edit}"
                    );
                    written += 1;
                }
            }
        }
        assert!(written > 800, "only {written} writes");
    }

    #[test]
    fn inline_source_tables_are_their_definition() {
        let mut documents = Vec::new();
        for (name, text) in fixture_files() {
            documents.push((format!("{name} (CRLF)"), text.replace('\n', "\r\n")));
            documents.push((
                format!("{name} (quoted with tabs)"),
                text.lines().map(|line| format!("> \t{line}\n")).collect(),
            ));
            documents.push((
                format!("{name} (astral, tabs)"),
                text.replace('a', "\u{1F600}").replace(' ', "\t"),
            ));
            documents.push((
                format!("{name} (BOM, two- and three-byte chars)"),
                format!("\u{feff}{}", text.replace('e', "é中")),
            ));
            documents.push((name, text));
        }
        let mut leaves = 0;
        // One set of tables serves every leaf of a document, as in `parse`.
        let mut runs = Default::default();
        for (name, text) in &documents {
            let to_utf16 = OffsetTable::to_utf16(text);
            let defined = utf16_offsets(text);
            let expected = markdown_syntax::parse(text).document.leaf_sources();
            let mut tables = |source: LeafSource<'_>| {
                let mut writer = Writer::new(&[]);
                write_inline_source(&mut writer, source, to_utf16.offsets(), &mut runs);
                written_tables(&writer.finish())
            };
            // Each leaf as `parse` lends it, and as `inline_source` reads it.
            let mut found = 0;
            markdown_syntax::__private::parse_with_leaf_contents(text, |_, contents| {
                contents.each(|path, source| {
                    let (expected_path, expected) = &expected[found];
                    assert_eq!(path, &expected_path.0[..], "{name}");
                    assert_eq!(source.text, expected.text, "{name} at {path:?}");
                    let defined = defined_tables(expected, &defined);
                    assert_eq!(tables(source), defined, "{name} at {path:?}");
                    assert_eq!(
                        tables(LeafSource::of(expected)),
                        defined,
                        "{name} at {path:?}"
                    );
                    found += 1;
                })
            });
            assert_eq!(found, expected.len(), "{name}");
            leaves += found;
        }
        assert!(leaves > 10_000, "only {leaves} leaves");
    }
}
