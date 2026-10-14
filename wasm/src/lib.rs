//! The WebAssembly binding of `markdown-syntax`: a C ABI that `js/index.js`
//! wraps. A parsed document stays in WebAssembly behind a handle, with the
//! source it holds, so the source crosses once; every result is written to
//! one buffer that JavaScript reads at `result_ptr()` and `result_len()`.
//!
//! Every export is `unsafe` only in taking pointers JavaScript wrote through
//! `buffer_alloc`; the JavaScript wrapper is the one caller.

pub mod wire;

use std::cell::RefCell;

use markdown_syntax::prelude::*;
use markdown_syntax::FenceMarker;

use wire::{byte_offsets, utf16_offsets, Reader, Writer};

/// A parsed document and the offset tables its spans cross by.
struct Entry {
    document: Document,
    /// The UTF-16 offset of each byte offset of the source.
    to_utf16: Vec<u32>,
    /// The byte offset of each UTF-16 offset of the source, built on the
    /// first write.
    to_bytes: Option<Vec<u32>>,
}

thread_local! {
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static DOCUMENTS: RefCell<Vec<Option<Entry>>> = const { RefCell::new(Vec::new()) };
}

fn set_result(bytes: Vec<u8>) {
    RESULT.with(|result| *result.borrow_mut() = bytes);
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

fn with_entry<T>(handle: u32, work: impl FnOnce(&mut Entry) -> T) -> Option<T> {
    DOCUMENTS.with(|documents| {
        let mut documents = documents.borrow_mut();
        let entry = documents
            .get_mut((handle as usize).checked_sub(1)?)?
            .as_mut()?;
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
    let output = markdown_syntax::parse(source);
    let to_utf16 = utf16_offsets(source);
    let mut writer = Writer::new(&to_utf16);
    writer.document(&output.document);
    writer.diagnostics(&output.diagnostics);
    let leaves = output.document.leaf_sources();
    writer.word(leaves.len() as u32);
    for (path, source) in &leaves {
        writer.word(path.0.len() as u32);
        for &index in &path.0 {
            writer.word(index as u32);
        }
        write_inline_source(&mut writer, source, &to_utf16);
    }
    set_result(writer.finish());
    let entry = Entry {
        document: output.document,
        to_utf16,
        to_bytes: None,
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
/// strings; the result is the inlines and diagnostics.
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
    let labels = match Reader::new(bytes(labels, labels_len), &[]).and_then(|mut reader| {
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
    let output = markdown_syntax::parse_inline(source, &Definitions::new(labels));
    let to_utf16 = utf16_offsets(source);
    let mut writer = Writer::new(&to_utf16);
    writer.inlines(&output.inlines);
    writer.diagnostics(&output.diagnostics);
    set_result(writer.finish());
    0
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
        write_inline_source(&mut writer, &source, &entry.to_utf16);
        set_result(writer.finish());
        1
    })
    .unwrap_or(3)
}

/// Writes an inline source: its text, then where each UTF-16 offset of the
/// text and its end maps in the source, in UTF-16 offsets, as a span's start
/// and then as a span's end. Each table is its runs: a count, then for each
/// run the offset it starts at and what that offset maps to, the offsets
/// after it mapping one further each; an offset inside a surrogate pair is
/// a run of its own that maps to `NO_SPAN`.
fn write_inline_source(writer: &mut Writer, source: &InlineSource, to_utf16: &[u32]) {
    writer.str(&source.text);
    let offsets = byte_offsets(&source.text);
    for at_end in [false, true] {
        let mut runs = Vec::new();
        let mut previous = None;
        for (unit, byte) in offsets.iter().enumerate() {
            let mapped = if *byte == wire::NO_SPAN {
                wire::NO_SPAN
            } else if at_end {
                to_utf16[source.span_to_source(Span::new(0, *byte as usize)).end]
            } else {
                to_utf16[source.to_source(*byte as usize)]
            };
            let continues = matches!(previous, Some(previous) if previous != wire::NO_SPAN
                && mapped != wire::NO_SPAN
                && mapped == previous + 1);
            if !continues {
                runs.push((unit as u32, mapped));
            }
            previous = Some(mapped);
        }
        writer.word(runs.len() as u32);
        for (unit, mapped) in runs {
            writer.word(unit);
            writer.word(mapped);
        }
    }
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
    let message = bytes(tree, tree_len);
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
    with_entry(handle, |entry| {
        let source = entry.document.source().unwrap_or_default();
        let to_bytes = entry.to_bytes.get_or_insert_with(|| byte_offsets(source));
        let decoded = match Reader::new(message, to_bytes).and_then(|mut reader| reader.document())
        {
            Ok(decoded) => decoded,
            Err(error) => {
                set_result(error.0.into_bytes());
                return 2;
            }
        };
        // The kept document lends its source to the decoded tree.
        let document = &mut entry.document;
        let meta = std::mem::replace(&mut document.meta, decoded.meta);
        let children = std::mem::replace(&mut document.children, decoded.children);
        let written = document.to_markdown_with(&options);
        document.meta = meta;
        document.children = children;
        match written {
            Ok(markdown) => {
                set_result(markdown.into_bytes());
                0
            }
            Err(SerializeError::InvalidDocument(diagnostics)) => {
                let mut writer = Writer::new(&entry.to_utf16);
                writer.diagnostics(&diagnostics);
                set_result(writer.finish());
                1
            }
        }
    })
    .unwrap_or(3)
}
