#![doc = include_str!("../README.md")]
#![no_std]
#![deny(missing_docs)]

extern crate alloc;

mod compare;
mod entities;
mod gemoji;
mod memo;
#[cfg(test)]
mod test_support;
mod unicode_punctuation;

pub mod ast;
pub mod diagnostic;
#[cfg(feature = "html")]
pub mod html;
pub mod options;
pub mod parse;
pub mod serialize;
pub mod span;
pub mod validate;

pub use ast::*;
pub use diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
#[cfg(feature = "html")]
pub use html::{HtmlError, HtmlOptions, SafeRawHtmlForm, TasklistAttrOrder};
pub use options::{
    Construct, Constructs, ParseOptions, SyntaxConfigError, SyntaxOptions, WikiLinkOrder,
};
pub use parse::{parse, ParseOutput, ParseStrictError};
pub use serialize::{LineEnding, SerializeError, SerializeOptions};
pub use span::{LineIndex, LinePosition, Span};

/// Not part of the public API: the tree comparison serialization reads back
/// with, exported so the crate's integration tests compare trees the same way.
#[doc(hidden)]
pub mod __private {
    pub use crate::compare::normalized_blocks;
}

/// Common imports for working with `markdown-syntax`: `use
/// markdown_syntax::prelude::*;` brings the AST, options, diagnostics, parse
/// entry points, and serialize/span types (plus the HTML renderer under the
/// `html` feature) into scope.
pub mod prelude {
    pub use crate::ast::*;
    pub use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
    #[cfg(feature = "html")]
    pub use crate::html::{HtmlError, HtmlOptions, SafeRawHtmlForm, TasklistAttrOrder};
    pub use crate::options::{
        Construct, Constructs, ParseOptions, SyntaxConfigError, SyntaxOptions, WikiLinkOrder,
    };
    pub use crate::parse::{parse, ParseOutput, ParseStrictError};
    pub use crate::serialize::{LineEnding, SerializeError, SerializeOptions};
    pub use crate::span::{LineIndex, LinePosition, Span};
}
