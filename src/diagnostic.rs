use alloc::string::String;

use crate::span::Span;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DiagnosticCode {
    InvalidDirectiveName,
    UnclosedDirectiveContainer,
    InvalidMdx,
    StrictParse,
    /// AST validation failure (an invalid or unsupported node shape), the single
    /// code used by `Document::validate` and by serialize/HTML pre-validation.
    InvalidDocument,
}

/// A single diagnostic across every domain — parser, AST validation, and the
/// serialize/HTML pre-validation that wraps it. `span` is optional because a
/// hand-built AST node may carry no source location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    pub span: Option<Span>,
    pub message: String,
}

impl Diagnostic {
    /// A parser diagnostic, which always carries a source span.
    pub fn new(
        severity: DiagnosticSeverity,
        code: DiagnosticCode,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code,
            span: Some(span),
            message: message.into(),
        }
    }

    /// An AST-validation diagnostic (`Error` severity, `InvalidDocument` code),
    /// whose span is optional because a hand-built node may lack one.
    pub fn invalid(span: Option<Span>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::InvalidDocument,
            span,
            message: message.into(),
        }
    }
}
