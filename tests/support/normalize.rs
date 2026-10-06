//! The serializer's tree comparison, as `src/compare.rs` defines it: two
//! trees are the same when they differ only in spans, in text one holds as an
//! `Escape` or a `CharacterReference` and the other as plain text, and in
//! where adjacent `Text` nodes split.

use markdown_syntax::*;

/// `document` normalized as [`normalized`] normalizes its blocks.
#[allow(dead_code)]
pub(crate) fn normalized_document(document: &Document) -> Document {
    Document {
        children: normalized(&document.children),
        ..document.clone()
    }
}

/// `blocks` with spans cleared, escapes and character references read as
/// text, and adjacent text merged.
pub(crate) fn normalized(blocks: &[Block]) -> Vec<Block> {
    markdown_syntax::__private::normalized_blocks(blocks)
}
