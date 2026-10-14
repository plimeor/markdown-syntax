//! Checks of the tokens of an inline parse: they tile the text, each lies in
//! its node and outside that node's children, the children of each container
//! tile its content, and each part reads back as the field it names.

#![allow(dead_code)]

use markdown_syntax::prelude::*;

/// The problems with the tokens `parse_inline` returns for `text`.
pub(crate) fn token_problems(text: &str, definitions: &Definitions) -> Vec<String> {
    let output = parse_inline(text, definitions);
    let nodes = output.nodes();
    let tokens = &output.tokens;
    let mut problems = Vec::new();
    let mut at = 0;
    for (index, token) in tokens.iter().enumerate() {
        if token.span.start != at || token.span.end <= token.span.start {
            problems.push(format!("token {index} {token:?} does not follow {at}"));
        }
        at = token.span.end;
        if index > 0 && tokens[index - 1].node == token.node && tokens[index - 1].role == token.role
        {
            problems.push(format!("token {index} {token:?} repeats the one before"));
        }
        let Some(node) = nodes.get(token.node) else {
            problems.push(format!("token {index} {token:?} names no node"));
            continue;
        };
        let span = node.span().expect("a parsed inline has a span");
        if token.span.start < span.start || token.span.end > span.end {
            problems.push(format!("token {index} {token:?} lies outside {span:?}"));
        }
        if node.children().iter().any(|child| {
            child
                .span()
                .is_some_and(|child| child.start < token.span.end && token.span.start < child.end)
        }) {
            problems.push(format!(
                "token {index} {token:?} overlaps a child of its node"
            ));
        }
    }
    if at != text.len() {
        problems.push(format!("the tokens end at {at}, not {}", text.len()));
    }
    tiling(&output.inlines, 0, text.len(), "the text", &mut problems);
    for node in &nodes {
        let children = node.children();
        if let (Some(first), Some(last)) = (children.first(), children.last()) {
            let (first, last) = (first.span().unwrap(), last.span().unwrap());
            tiling(
                children,
                first.start,
                last.end,
                "a container",
                &mut problems,
            );
        }
    }
    if !text.contains('\0') {
        for (index, node) in nodes.iter().enumerate() {
            if let Some(problem) = part_problem(text, node, index, tokens) {
                problems.push(format!("node {index}: {problem}"));
            }
        }
    }
    problems
}

/// Whether `inlines` tile `start..end`.
fn tiling(inlines: &[Inline], start: usize, end: usize, what: &str, problems: &mut Vec<String>) {
    let mut at = start;
    for inline in inlines {
        let span = inline.span().expect("a parsed inline has a span");
        if span.start != at {
            problems.push(format!("{what}: {span:?} does not follow {at}"));
        }
        at = span.end;
    }
    if !inlines.is_empty() && at != end {
        problems.push(format!("{what}: the inlines end at {at}, not {end}"));
    }
}

/// The bytes of `text` that the tokens of `node` with `role` hold, with line
/// endings read as `\n`.
fn part(text: &str, tokens: &[InlineToken], node: usize, role: TokenRole) -> Option<String> {
    let mut found: Option<String> = None;
    for token in tokens
        .iter()
        .filter(|token| token.node == node && token.role == role)
    {
        found
            .get_or_insert_with(String::new)
            .push_str(&text[token.span.start..token.span.end]);
    }
    found.map(|part| part.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Whether a part holds what decoding could change.
fn encoded(part: &str) -> bool {
    part.contains(['\\', '&'])
}

/// A part of `node` that does not read back as the field it names.
fn part_problem(text: &str, node: &Inline, index: usize, tokens: &[InlineToken]) -> Option<String> {
    let get = |role| part(text, tokens, index, role);
    let differs = |role: TokenRole, read: Option<String>, field: &str| {
        let read = read.unwrap_or_default();
        (read != field && !encoded(&read)).then(|| format!("{role:?} {read:?} is not {field:?}"))
    };
    match node {
        Inline::Code(code) => {
            let read = get(TokenRole::Text).unwrap_or_default().replace('\n', " ");
            (read != code.value).then(|| format!("code {read:?} is not {:?}", code.value))
        }
        Inline::Math(math) => {
            let read = get(TokenRole::Text).unwrap_or_default();
            let read = match math.kind {
                MathInlineKind::Dollar { dollars: 1 } => read.replace('\n', " "),
                _ => read,
            };
            (read != math.value).then(|| format!("math {read:?} is not {:?}", math.value))
        }
        Inline::Link(link) => differs(
            TokenRole::Destination,
            get(TokenRole::Destination),
            &link.destination,
        )
        .or_else(|| {
            let title = link.title.as_ref().map_or("", |title| title.value.as_str());
            differs(TokenRole::Title, get(TokenRole::Title), title)
        }),
        Inline::Image(image) => differs(
            TokenRole::Destination,
            get(TokenRole::Destination),
            &image.destination,
        )
        .or_else(|| {
            let title = image
                .title
                .as_ref()
                .map_or("", |title| title.value.as_str());
            differs(TokenRole::Title, get(TokenRole::Title), title)
        }),
        Inline::Autolink(autolink) => differs(
            TokenRole::Destination,
            get(TokenRole::Destination),
            &autolink.text,
        ),
        Inline::LinkReference(reference) if reference.kind == ReferenceKind::Full => {
            differs(TokenRole::Label, get(TokenRole::Label), &reference.label)
        }
        Inline::ImageReference(reference) if reference.kind == ReferenceKind::Full => {
            differs(TokenRole::Label, get(TokenRole::Label), &reference.label)
        }
        Inline::FootnoteReference(reference) => {
            differs(TokenRole::Label, get(TokenRole::Label), &reference.label)
        }
        Inline::WikiLink(wikilink) => {
            differs(TokenRole::Target, get(TokenRole::Target), &wikilink.target).or_else(|| {
                match get(TokenRole::Label) {
                    Some(label) => differs(TokenRole::Label, Some(label), &wikilink.label),
                    None => (wikilink.label != wikilink.target)
                        .then(|| format!("no label for {:?}", wikilink.label)),
                }
            })
        }
        Inline::TextDirective(directive) => {
            differs(TokenRole::Name, get(TokenRole::Name), &directive.name)
        }
        Inline::Shortcode(shortcode) => {
            differs(TokenRole::Name, get(TokenRole::Name), &shortcode.name)
        }
        Inline::Tag(tag) => differs(TokenRole::Name, get(TokenRole::Name), &tag.name),
        _ => None,
    }
}
