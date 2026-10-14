//! The token pass: splits the text of one inline parse into runs, each the
//! innermost inline that holds it and the part of that inline it is.
//!
//! It runs after the parse, over the inlines and the input they were read
//! from, in that input's coordinates. A parsed container's children tile its
//! content between its own syntax, so a container's own bytes are what its
//! children leave; only a link's tail and a wiki link's separator are scanned,
//! by the scanners the parse read them with.

use alloc::vec::Vec;

use crate::ast::{AutolinkForm, Inline, MathInlineKind, ReferenceKind};
use crate::span::Span;

use super::{code_padding, find_wikilink_separator, parse_link_resource, DirectLookups};
use super::{InlineToken, TokenRole};

/// The tokens of `inlines`, read from `input`, tiling `input`.
pub(super) fn tokens(input: &str, inlines: &[Inline]) -> Vec<InlineToken> {
    let mut tokens = Tokens {
        tokens: Vec::new(),
        at: 0,
    };
    // Pre-order with an explicit stack: a node is numbered when it is taken,
    // and its own runs and children are pushed in reverse.
    let mut work: Vec<Work<'_>> = inlines.iter().rev().map(Work::Node).collect();
    let mut next = 0;
    let mut last = None;
    while let Some(item) = work.pop() {
        match item {
            Work::Run(end, node, role) => tokens.push(end, node, role),
            Work::Node(inline) => {
                let node = next;
                next += 1;
                last = Some(node);
                let Some(span) = inline.span() else {
                    continue;
                };
                // Bytes before a node that no node holds are syntax of the
                // node after them.
                tokens.push(span.start, node, TokenRole::Syntax);
                let start = work.len();
                pieces(input, inline, span, node, &mut work);
                work[start..].reverse();
            }
        }
    }
    if let Some(node) = last {
        tokens.push(input.len(), node, TokenRole::Syntax);
    }
    tokens.tokens
}

/// One step of the walk: the run up to an offset, as a node's part, or a node
/// to take.
enum Work<'a> {
    Run(usize, usize, TokenRole),
    Node(&'a Inline),
}

struct Tokens {
    tokens: Vec<InlineToken>,
    /// Where the next token starts.
    at: usize,
}

impl Tokens {
    /// Adds the run from where the last one ended to `end`, merged into the
    /// last one when it has the same node and role; nothing when empty.
    fn push(&mut self, end: usize, node: usize, role: TokenRole) {
        if end <= self.at {
            return;
        }
        match self.tokens.last_mut() {
            Some(last) if last.node == node && last.role == role => last.span.end = end,
            _ => self.tokens.push(InlineToken {
                span: Span::new(self.at, end),
                node,
                role,
            }),
        }
        self.at = end;
    }
}

/// Pushes, in order, the runs and children of `inline`, node `node` at `span`.
fn pieces<'a>(input: &str, inline: &'a Inline, span: Span, node: usize, work: &mut Vec<Work<'a>>) {
    let (start, end) = (span.start, span.end);
    let run = |work: &mut Vec<Work<'a>>, to: usize, role| work.push(Work::Run(to, node, role));
    // The children, each after the syntax before it; returns where the last
    // one ends, or `from` without children.
    let children = |work: &mut Vec<Work<'a>>, children: &'a [Inline], from: usize| {
        let mut at = from;
        for child in children {
            if let Some(child_span) = child.span() {
                work.push(Work::Run(child_span.start, node, TokenRole::Syntax));
                at = child_span.end;
            }
            work.push(Work::Node(child));
        }
        at
    };
    match inline {
        Inline::Text(_) | Inline::Html(_) | Inline::SoftBreak(_) => run(work, end, TokenRole::Text),
        Inline::LineBreak(_) => {
            let ending = input[start..end].rfind('\n').map_or(end, |at| start + at);
            run(work, ending, TokenRole::Syntax);
            run(work, end, TokenRole::Text);
        }
        Inline::Escape(_) => {
            run(work, (start + 1).min(end), TokenRole::Syntax);
            run(work, end, TokenRole::Text);
        }
        Inline::CharacterReference(_) => {
            let closed = input[start..end].ends_with(';');
            run(work, (start + 1).min(end), TokenRole::Syntax);
            run(work, if closed { end - 1 } else { end }, TokenRole::Text);
            run(work, end, TokenRole::Syntax);
        }
        Inline::Emphasis(_) | Inline::Strong(_) | Inline::Delete(_) | Inline::Mark(_) => {
            children(work, inline.children(), start);
            run(work, end, TokenRole::Syntax);
        }
        Inline::InlineFootnote(_) => {
            children(work, inline.children(), start);
            run(work, end, TokenRole::Syntax);
        }
        Inline::Code(_) => {
            let fence = input[start..end]
                .bytes()
                .take_while(|byte| *byte == b'`')
                .count();
            delimited(input, start + fence, end - fence, true, node, work);
            run(work, end, TokenRole::Syntax);
        }
        Inline::Math(math) => {
            let (open, close, padded) = match math.kind {
                MathInlineKind::Code => (2, 2, false),
                MathInlineKind::Dollar { dollars } => {
                    (usize::from(dollars), usize::from(dollars), dollars == 1)
                }
            };
            delimited(input, start + open, end - close, padded, node, work);
            run(work, end, TokenRole::Syntax);
        }
        Inline::Link(_) | Inline::Image(_) => {
            let open = start + usize::from(matches!(inline, Inline::Image(_)));
            // The children tile the text up to its `]`.
            let close = children(work, inline.children(), open + 1);
            run(work, close + 1, TokenRole::Syntax);
            let tail = &input[..end];
            if let Some((_, _, ranges)) =
                parse_link_resource(&mut DirectLookups { input: tail }, tail, close + 1)
            {
                run(work, ranges.destination.0, TokenRole::Syntax);
                run(work, ranges.destination.1, TokenRole::Destination);
                if let Some((title_start, title_end)) = ranges.title {
                    run(work, title_start, TokenRole::Syntax);
                    run(work, title_end, TokenRole::Title);
                }
            }
            run(work, end, TokenRole::Syntax);
        }
        Inline::LinkReference(_) | Inline::ImageReference(_) => {
            let open = start + usize::from(matches!(inline, Inline::ImageReference(_)));
            let close = children(work, inline.children(), open + 1);
            let kind = match inline {
                Inline::LinkReference(node) => node.kind,
                Inline::ImageReference(node) => node.kind,
                _ => ReferenceKind::Shortcut,
            };
            if kind == ReferenceKind::Full {
                run(work, close + 2, TokenRole::Syntax);
                run(work, end - 1, TokenRole::Label);
            }
            run(work, end, TokenRole::Syntax);
        }
        Inline::Autolink(autolink) => match autolink.form {
            AutolinkForm::Angle => {
                run(work, start + 1, TokenRole::Syntax);
                run(work, end - 1, TokenRole::Destination);
                run(work, end, TokenRole::Syntax);
            }
            AutolinkForm::Literal => run(work, end, TokenRole::Destination),
        },
        Inline::FootnoteReference(_) => {
            run(work, (start + 2).min(end), TokenRole::Syntax);
            run(work, end - 1, TokenRole::Label);
            run(work, end, TokenRole::Syntax);
        }
        Inline::WikiLink(wikilink) => {
            let open = start + usize::from(wikilink.embed) + 2;
            let content = &input[open..end - 2];
            run(work, open, TokenRole::Syntax);
            match find_wikilink_separator(content) {
                Some(separator) => {
                    run(work, open + separator, TokenRole::Target);
                    run(work, open + separator + 1, TokenRole::Syntax);
                    run(work, end - 2, TokenRole::Label);
                }
                None => run(work, end - 2, TokenRole::Target),
            }
            run(work, end, TokenRole::Syntax);
        }
        Inline::TextDirective(_) => {
            let name_end = start
                + 1
                + input[start + 1..end]
                    .bytes()
                    .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                    .count();
            run(work, start + 1, TokenRole::Syntax);
            run(work, name_end, TokenRole::Name);
            let mut at = name_end;
            if input.as_bytes().get(at) == Some(&b'[') {
                at = children(work, inline.children(), at + 1) + 1;
                run(work, at, TokenRole::Syntax);
            }
            if at < end && input.as_bytes().get(at) == Some(&b'{') {
                run(work, at + 1, TokenRole::Syntax);
                run(work, end - 1, TokenRole::Attributes);
            }
            run(work, end, TokenRole::Syntax);
        }
        Inline::Shortcode(_) => {
            run(work, start + 1, TokenRole::Syntax);
            run(work, end - 1, TokenRole::Name);
            run(work, end, TokenRole::Syntax);
        }
        Inline::Tag(_) => {
            run(work, start + 1, TokenRole::Syntax);
            run(work, end, TokenRole::Name);
        }
    }
}

/// The content of code or math between `from` and `to`, with the padding its
/// value drops as syntax when `padded`.
fn delimited(
    input: &str,
    from: usize,
    to: usize,
    padded: bool,
    node: usize,
    work: &mut Vec<Work<'_>>,
) {
    let (before, after) = if padded {
        code_padding(&input[from..to])
    } else {
        (0, 0)
    };
    work.push(Work::Run(from + before, node, TokenRole::Syntax));
    work.push(Work::Run(to - after, node, TokenRole::Text));
}
