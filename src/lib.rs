#![doc = include_str!("../README.md")]
#![no_std]
#![deny(missing_docs)]

extern crate alloc;
// The unit tests read the fixture corpus and keep per-thread switches.
#[cfg(test)]
extern crate std;

mod compare;
mod decode;
mod entities;
mod gemoji;
mod leaves;
mod memo;
mod name_table;
mod swar;
#[cfg(test)]
mod test_support;
mod unicode_cjk;
mod unicode_letters;
mod unicode_punctuation;

pub mod ast;
pub mod diagnostic;
#[cfg(feature = "html")]
pub mod html;
pub mod parse;
pub mod serialize;
pub mod span;
pub mod validate;

pub use ast::*;
pub use diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
#[cfg(feature = "html")]
pub use html::{HtmlError, HtmlOptions, SafeRawHtmlForm, TasklistAttrOrder};
pub use parse::{
    parse, parse_inline, Definitions, InlineOutput, InlineSource, InlineToken, ParseOutput,
    TokenRole,
};
pub use serialize::{BulletMarker, LineEnding, OrderedDelimiter, SerializeError, SerializeOptions};
pub use span::{LineIndex, LinePosition, Span};

/// Not part of the public API: entry points for the crate's own integration
/// tests, which compare trees as round-trip checks do, and for the WebAssembly
/// binding in this workspace, which reads leaf sources with a parse and keeps
/// that parse to write against. Neither the items nor their behavior are
/// covered by semantic versioning.
#[doc(hidden)]
pub mod __private {
    pub use crate::compare::normalized_blocks;
    pub use crate::parse::inline_spans_to_source;

    /// `parse(input)`, and what `read` makes of it and of the inline content
    /// of each of its document's leaves, read in the same pass over the
    /// source, for the wasm binding, which returns both:
    /// [`LeafContents::each`] lends each leaf's inline source where the parse
    /// keeps it.
    pub fn parse_with_leaf_contents<R>(
        input: &str,
        read: impl FnOnce(&crate::ParseOutput, LeafContents<'_>) -> R,
    ) -> (crate::ParseOutput, R) {
        crate::parse::parse_with_leaf_contents(input, |output, contents| {
            read(output, LeafContents(contents))
        })
    }

    /// The inline content of each leaf of a document, read with its parse.
    pub struct LeafContents<'a>(crate::parse::LeafContents<'a>);

    impl LeafContents<'_> {
        /// Calls `found` with the path and inline source of every leaf of the
        /// document, in document order, each as `leaf_sources()` returns it.
        pub fn each(self, mut found: impl FnMut(&[usize], LeafSource<'_>)) {
            self.0
                .each(|path, text, map| found(path, LeafSource { text, map }));
        }

        /// Calls `found` as [`LeafContents::each`] does, and keeps every
        /// inline source for writes against the parse they were read with
        /// ([`to_markdown_against_with`]).
        pub fn each_keeping(self, mut found: impl FnMut(&[usize], LeafSource<'_>)) -> KeptSources {
            let kept = self
                .0
                .each_keeping(|path, text, map| found(path, LeafSource { text, map }));
            KeptSources(crate::serialize::KeptSources::new(kept))
        }
    }

    /// The inline source of each leaf of a parsed document, kept from its
    /// parse by [`LeafContents::each_keeping`].
    #[derive(Debug)]
    pub struct KeptSources(crate::serialize::KeptSources);

    /// An inline source, borrowed: its text and where each run of it was
    /// read from.
    #[derive(Clone, Copy)]
    pub struct LeafSource<'a> {
        /// The text, as [`crate::InlineSource::text`].
        pub text: &'a str,
        map: &'a crate::parse::SourceMap,
    }

    impl<'a> LeafSource<'a> {
        /// The inline source `source` holds.
        pub fn of(source: &'a crate::InlineSource) -> Self {
            Self {
                text: &source.text,
                map: source.map(),
            }
        }

        /// The runs of the text, in order, each a range of the text and the
        /// source range it was read from, which
        /// [`crate::InlineSource::span_to_source`] maps through as
        /// `SourceMap::runs` (`src/parse/source_map.rs`) describes.
        pub fn runs(
            &self,
        ) -> impl Iterator<Item = (core::ops::Range<usize>, core::ops::Range<usize>)> + 'a {
            self.map.runs(self.text.len())
        }
    }

    /// How many top-level blocks `to_markdown` writes canonically because,
    /// written partly from the source, they would read back as something
    /// else.
    pub fn rewritten_blocks(document: &crate::Document) -> usize {
        document
            .write(&crate::SerializeOptions::default())
            .map_or(0, |(_, rewritten)| rewritten)
    }

    /// What `to_markdown_with` writes for `tree` given the source of
    /// `parsed`, read as the parse of that source in place of parsing it
    /// again: the WebAssembly binding keeps that parse for every write.
    ///
    /// `parsed` must be a document `parse` returned, unchanged since, so that
    /// it equals `parse(source)` for its source; with another document the
    /// output is unspecified (debug builds check it). A `parsed` without a
    /// source gives `tree`'s canonical Markdown.
    pub fn to_markdown_against(
        tree: &crate::Document,
        parsed: &crate::Document,
        options: &crate::SerializeOptions,
    ) -> Result<alloc::string::String, crate::SerializeError> {
        tree.write_against(parsed, None, crate::serialize::KeptTexts::Check, options)
    }

    /// [`to_markdown_against`], with the inline sources of the leaves of
    /// `parsed` taken from `sources` in place of reading them again from its
    /// source, and the text each leaf of `tree` keeps (`with_source`) taken
    /// as reading as that leaf's inlines in place of reading it again.
    ///
    /// `sources` must be what [`LeafContents::each_keeping`] returned for the
    /// parse that returned `parsed`, both unchanged since, and every leaf of
    /// `tree` that keeps its text must hold the inlines that text reads as
    /// with the definitions of `tree`, as `Document::reread_kept_sources`
    /// leaves them or `with_source` builds them given those definitions;
    /// otherwise the output is unspecified (debug builds check both).
    pub fn to_markdown_against_with(
        tree: &crate::Document,
        parsed: &crate::Document,
        sources: &KeptSources,
        options: &crate::SerializeOptions,
    ) -> Result<alloc::string::String, crate::SerializeError> {
        tree.write_against(
            parsed,
            Some(&sources.0),
            crate::serialize::KeptTexts::Trusted,
            options,
        )
    }
}

/// Common imports for working with `markdown-syntax`: `use
/// markdown_syntax::prelude::*;` brings the AST, diagnostics, the parse
/// entry points, and serialize/span types (plus the HTML renderer under the
/// `html` feature) into scope.
pub mod prelude {
    pub use crate::ast::*;
    pub use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
    #[cfg(feature = "html")]
    pub use crate::html::{HtmlError, HtmlOptions, SafeRawHtmlForm, TasklistAttrOrder};
    pub use crate::parse::{
        parse, parse_inline, Definitions, InlineOutput, InlineSource, InlineToken, ParseOutput,
        TokenRole,
    };
    pub use crate::serialize::{
        BulletMarker, LineEnding, OrderedDelimiter, SerializeError, SerializeOptions,
    };
    pub use crate::span::{LineIndex, LinePosition, Span};
}
