#![doc = include_str!("../README.md")]
#![no_std]

extern crate alloc;

mod entities;
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
pub use diagnostic::*;
#[cfg(feature = "html")]
pub use html::{
    to_html, to_html_with_options, HtmlError, HtmlOptions, SafeRawHtmlForm, TasklistAttrOrder,
};
pub use options::*;
pub use parse::{
    parse, parse_strict_with_options, parse_with_options, ParseOutput, ParseStrictError,
};
pub use serialize::{
    to_markdown, to_markdown_with_options, LineEnding, SerializeError, SerializeOptions,
};
pub use span::{LineIndex, LinePosition, Span};
pub use validate::{validate_document, ValidationDiagnostic};
