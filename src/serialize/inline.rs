//! Inline content written as Markdown, with the choices the read-back makes:
//! how each text char is written and which delimiter each emphasis and strong
//! takes. The writer records where each text char and each node lands in the
//! output, so a parse of the output can say which written chars it read as
//! syntax.

use alloc::{format, string::String, vec::Vec};

use super::{
    autolink_uri, escape_footnote_label_semantic, escape_footnote_label_source,
    escape_wikilink_part, inline_code_fence, normalize_reference_label, push_reference_body,
    reference_explicit_label, serialize_attributes_with_context, serialize_destination_kind,
    serialize_title_kind, table_cell_escape_code_pipes, InlineSerializeContext, SerializeError,
};
use crate::ast::*;

/// How a text char is written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Form {
    Raw,
    /// After a backslash; only an ASCII punctuation char is written so.
    Backslash,
    /// As a character reference.
    Reference,
}

impl Form {
    /// The form a text char takes before the read-back escapes anything: a
    /// control char that the parser would not keep, line endings among them,
    /// is a reference, and a backtick, which could open a code span that an
    /// escaped one closes, is always escaped.
    fn initial(char: char) -> Self {
        if written_as_reference(char) {
            Self::Reference
        } else if char == '`' {
            Self::Backslash
        } else {
            Self::Raw
        }
    }
}

/// Whether text writes `char` as a character reference whatever the parse
/// reads: a control char other than the tab, line tabulation, form feed, and
/// next line, which the reparse keeps as written and reads as whitespace, as
/// the source did.
fn written_as_reference(char: char) -> bool {
    char.is_control() && !matches!(char, '\t' | '\u{b}' | '\u{c}' | '\u{85}')
}

/// A character reference for `char`: hexadecimal for whitespace and control
/// chars, decimal otherwise.
pub(super) fn char_reference(char: char) -> String {
    if char.is_whitespace() || char.is_control() {
        format!("&#x{:X};", char as u32)
    } else {
        format!("&#{};", char as u32)
    }
}

/// The writing choices for one block's inline content. Text chars and nodes
/// are numbered in the order the writer visits them.
#[derive(Clone, Debug, Default)]
pub(super) struct Choices {
    /// The forms of the text chars, by number; a char past the end takes its
    /// initial form.
    forms: Vec<Option<Form>>,
    /// Whether each emphasis and strong node is written with `_`, by number.
    underscore: Vec<bool>,
    /// A hash of the nodes written with `_`, which a switch updates in
    /// constant time, so that the delimiter choices tried are told apart
    /// without comparing them whole.
    underscore_hash: u64,
    /// Whether each link with a literal autolink's shape is written bare, as
    /// its text, by number.
    bare: Vec<bool>,
    /// How far each text directive is closed, by number: with an empty
    /// label (1), and with empty attributes too (2).
    closed: Vec<u8>,
}

/// A hash of node `id` for `Choices::underscore_hash`, which combines the
/// hashes of the nodes written with `_` by exclusive or.
fn node_hash(id: usize) -> u64 {
    // SplitMix64's finalizer.
    let mut z = (id as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Choices {
    pub(super) fn form(&self, index: usize, char: char) -> Form {
        self.forms
            .get(index)
            .copied()
            .flatten()
            .unwrap_or_else(|| Form::initial(char))
    }

    pub(super) fn set_form(&mut self, index: usize, form: Form) {
        if self.forms.len() <= index {
            self.forms.resize(index + 1, None);
        }
        self.forms[index] = Some(form);
    }

    /// The hash of the delimiter choices: equal choices hash alike.
    pub(super) fn underscore_hash(&self) -> u64 {
        self.underscore_hash
    }

    /// The hash the delimiter choices would have with `node` switched.
    pub(super) fn underscore_hash_switched(&self, node: usize) -> u64 {
        self.underscore_hash ^ node_hash(node)
    }

    pub(super) fn closed(&self, node: usize) -> u8 {
        self.closed.get(node).copied().unwrap_or(0)
    }

    /// Closes text directive `node` further, when it can be. Whether it was.
    pub(super) fn close_further(&mut self, node: usize) -> bool {
        if self.closed(node) >= 2 {
            return false;
        }
        if self.closed.len() <= node {
            self.closed.resize(node + 1, 0);
        }
        self.closed[node] += 1;
        true
    }

    pub(super) fn any_closed(&self) -> bool {
        self.closed.iter().any(|&closed| closed > 0)
    }

    pub(super) fn clear_closed(&mut self) {
        self.closed.clear();
    }

    pub(super) fn bare(&self, node: usize) -> bool {
        self.bare.get(node).copied().unwrap_or(false)
    }

    pub(super) fn set_bare(&mut self, node: usize) {
        if self.bare.len() <= node {
            self.bare.resize(node + 1, false);
        }
        self.bare[node] = true;
    }

    pub(super) fn any_bare(&self) -> bool {
        self.bare.contains(&true)
    }

    pub(super) fn clear_bare(&mut self) {
        self.bare.clear();
    }

    pub(super) fn any_underscored(&self) -> bool {
        self.underscore.contains(&true)
    }

    pub(super) fn clear_underscores(&mut self) {
        self.underscore.clear();
        self.underscore_hash = 0;
    }

    pub(super) fn underscored(&self, node: usize) -> bool {
        self.underscore.get(node).copied().unwrap_or(false)
    }

    pub(super) fn set_underscore(&mut self, node: usize, underscore: bool) {
        if self.underscored(node) == underscore {
            return;
        }
        if self.underscore.len() <= node {
            self.underscore.resize(node + 1, false);
        }
        self.underscore[node] = underscore;
        self.underscore_hash ^= node_hash(node);
    }
}

/// A text char as written.
#[derive(Clone, Copy, Debug)]
pub(super) struct WrittenChar {
    pub(super) index: usize,
    pub(super) char: char,
    pub(super) form: Form,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// A node as written: where it starts and ends, and where the content of a
/// span such as an emphasis does.
#[derive(Clone, Copy, Debug)]
pub(super) struct WrittenNode {
    pub(super) id: usize,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) content: Option<(usize, usize)>,
    /// The node is an emphasis (1) or strong (2), whose delimiters the
    /// read-back may write with `_`.
    pub(super) run: u8,
    /// The node is a link with a literal autolink's shape, which the
    /// read-back may write bare.
    pub(super) bare_shape: bool,
    /// The node is a text directive, which the read-back may close.
    pub(super) directive: bool,
}

/// The child inlines a node holds, in the order the writer visits them.
pub(super) fn inline_children(inline: &Inline) -> Option<&[Inline]> {
    Some(match inline {
        Inline::Emphasis(node) => &node.children,
        Inline::Strong(node) => &node.children,
        Inline::Underline(node) => &node.children,
        Inline::Delete(node) => &node.children,
        Inline::Insert(node) => &node.children,
        Inline::Mark(node) => &node.children,
        Inline::Subscript(node) => &node.children,
        Inline::Superscript(node) => &node.children,
        Inline::Spoiler(node) => &node.children,
        Inline::InlineFootnote(node) => &node.children,
        Inline::Link(node) => &node.children,
        Inline::Image(node) => &node.alt,
        Inline::LinkReference(node) => &node.children,
        Inline::ImageReference(node) => &node.alt,
        Inline::TextDirective(node) => &node.label,
        _ => return None,
    })
}

/// Writes inline content into `out`, recording where its text chars and
/// nodes land.
pub(super) struct Writer<'c> {
    pub(super) out: String,
    pub(super) chars: Vec<WrittenChar>,
    pub(super) nodes: Vec<WrittenNode>,
    choices: &'c Choices,
    next_char: usize,
    next_node: usize,
    /// Nesting of segments whose output is dropped, which record nothing.
    dropping: usize,
}

impl<'c> Writer<'c> {
    pub(super) fn new(choices: &'c Choices) -> Self {
        Writer {
            out: String::new(),
            chars: Vec::new(),
            nodes: Vec::new(),
            choices,
            next_char: 0,
            next_node: 0,
            dropping: 0,
        }
    }

    pub(super) fn push_str(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// Indents by `width` spaces each line of the output from `start` on whose
    /// number, counted from that line, `lines` holds, moving the records
    /// after each indentation along.
    pub(super) fn indent_lines(&mut self, start: usize, lines: &[usize], width: usize) {
        let mut points = Vec::new();
        if lines.first() == Some(&0) {
            points.push(start);
        }
        let mut line = 0;
        for (offset, byte) in self.out.as_bytes()[start..].iter().enumerate() {
            if *byte == b'\n' {
                line += 1;
                if lines.binary_search(&line).is_ok() {
                    points.push(start + offset + 1);
                }
            }
        }
        if points.is_empty() {
            return;
        }
        let mut out = String::with_capacity(self.out.len() + width * points.len());
        let mut copied = 0;
        for &point in &points {
            out.push_str(&self.out[copied..point]);
            out.extend(core::iter::repeat_n(' ', width));
            copied = point;
        }
        out.push_str(&self.out[copied..]);
        self.out = out;
        // A position at an indented line's start moves past the indentation
        // when something starts there, and stays when something ends there.
        let starts =
            |position: usize| position + width * points.partition_point(|point| *point <= position);
        let ends =
            |position: usize| position + width * points.partition_point(|point| *point < position);
        for char in &mut self.chars {
            char.start = starts(char.start);
            char.end = ends(char.end);
        }
        for node in &mut self.nodes {
            node.start = starts(node.start);
            node.end = ends(node.end);
            if let Some((content_start, content_end)) = node.content {
                node.content = Some((starts(content_start), ends(content_end)));
            }
        }
    }

    /// Writes `inlines`, in a table cell when `cell` is set.
    pub(super) fn write(&mut self, inlines: &[Inline], cell: bool) -> Result<(), SerializeError> {
        let context = if cell {
            InlineSerializeContext::table_cell()
        } else {
            InlineSerializeContext::block_content()
        };
        for inline in inlines {
            self.write_inline(inline, context)?;
        }
        Ok(())
    }

    fn write_children(
        &mut self,
        children: &[Inline],
        context: InlineSerializeContext,
    ) -> Result<(), SerializeError> {
        for inline in children {
            self.write_inline(inline, context)?;
        }
        Ok(())
    }

    /// Writes `children`, visiting them as when written, and returns what they
    /// wrote, which the output does not keep.
    fn render_dropped(
        &mut self,
        children: &[Inline],
        context: InlineSerializeContext,
    ) -> Result<String, SerializeError> {
        let start = self.out.len();
        let (chars, nodes) = (self.chars.len(), self.nodes.len());
        self.dropping += 1;
        self.write_children(children, context)?;
        self.dropping -= 1;
        self.chars.truncate(chars);
        self.nodes.truncate(nodes);
        Ok(self.out.split_off(start))
    }

    fn write_text(&mut self, value: &str) {
        for char in value.chars() {
            let index = self.next_char;
            self.next_char += 1;
            let mut form = self.choices.form(index, char);
            if form == Form::Backslash && !char.is_ascii_punctuation() {
                form = Form::Reference;
            }
            let start = self.out.len();
            match form {
                Form::Raw => self.out.push(char),
                Form::Backslash => {
                    self.out.push('\\');
                    self.out.push(char);
                }
                Form::Reference => self.out.push_str(&char_reference(char)),
            }
            if self.dropping == 0 {
                self.chars.push(WrittenChar {
                    index,
                    char,
                    form,
                    start,
                    end: self.out.len(),
                });
            }
        }
    }

    /// Writes a span: `open`, the children, and `close`.
    fn write_span(
        &mut self,
        id: usize,
        (open, close): (&str, &str),
        children: &[Inline],
        context: InlineSerializeContext,
        run: u8,
    ) -> Result<(), SerializeError> {
        let start = self.out.len();
        self.out.push_str(open);
        let content_start = self.out.len();
        self.write_children(children, context)?;
        let content_end = self.out.len();
        self.out.push_str(close);
        self.record(id, start, Some((content_start, content_end)), run);
        Ok(())
    }

    fn record(&mut self, id: usize, start: usize, content: Option<(usize, usize)>, run: u8) {
        if self.dropping == 0 {
            self.nodes.push(WrittenNode {
                id,
                start,
                end: self.out.len(),
                content,
                run,
                bare_shape: false,
                directive: false,
            });
        }
    }

    fn write_inline(
        &mut self,
        inline: &Inline,
        context: InlineSerializeContext,
    ) -> Result<(), SerializeError> {
        let id = self.next_node;
        self.next_node += 1;
        let start = self.out.len();
        match inline {
            Inline::Text(node) => {
                self.write_text(&node.value);
                self.record(id, start, None, 0);
            }
            Inline::Escape(node) => {
                self.out.push('\\');
                self.out.push(node.value);
                self.record(id, start, None, 0);
            }
            Inline::CharacterReference(node) => {
                self.out.push_str(&node.reference);
                self.record(id, start, None, 0);
            }
            Inline::Emphasis(node) => {
                let delimiter = if self.choices.underscored(id) {
                    "_"
                } else {
                    "*"
                };
                self.write_span(id, (delimiter, delimiter), &node.children, context, 1)?;
            }
            Inline::Strong(node) => {
                let delimiter = if self.choices.underscored(id) {
                    "__"
                } else {
                    "**"
                };
                self.write_span(id, (delimiter, delimiter), &node.children, context, 2)?;
            }
            Inline::Underline(node) => {
                self.write_span(id, ("__", "__"), &node.children, context, 0)?;
            }
            Inline::Delete(node) => {
                let marker = match node.marker {
                    DeleteMarker::SingleTilde => "~",
                    DeleteMarker::DoubleTilde => "~~",
                };
                self.write_span(id, (marker, marker), &node.children, context, 0)?;
            }
            Inline::Insert(node) => {
                self.write_span(id, ("++", "++"), &node.children, context, 0)?;
            }
            Inline::Mark(node) => {
                self.write_span(id, ("==", "=="), &node.children, context, 0)?;
            }
            Inline::Subscript(node) => {
                self.write_span(id, ("~", "~"), &node.children, context, 0)?;
            }
            Inline::Superscript(node) => {
                self.write_span(id, ("^", "^"), &node.children, context, 0)?;
            }
            Inline::Spoiler(node) => {
                self.write_span(id, ("||", "||"), &node.children, context, 0)?;
            }
            Inline::InlineFootnote(node) => {
                self.write_span(id, ("^[", "]"), &node.children, context, 0)?;
            }
            Inline::Shortcode(node) => {
                self.out.push(':');
                self.out.push_str(&node.name);
                self.out.push(':');
                self.record(id, start, None, 0);
            }
            Inline::Code(node) => {
                if node.fence_length > 0 && !node.raw.is_empty() {
                    let fence = "`".repeat(node.fence_length);
                    self.out.push_str(&fence);
                    if context.table_cell {
                        self.out.push_str(&table_cell_escape_code_pipes(&node.raw));
                    } else {
                        self.out.push_str(&node.raw);
                    }
                    self.out.push_str(&fence);
                } else if node.value.is_empty() {
                    self.out.push_str("`` ``");
                } else {
                    let value = if context.table_cell {
                        table_cell_escape_code_pipes(&node.value)
                    } else {
                        node.value.clone()
                    };
                    let fence = inline_code_fence(&value);
                    self.out.push_str(&fence);
                    if super::code_span_needs_padding(&value) {
                        self.out.push(' ');
                        self.out.push_str(&value);
                        self.out.push(' ');
                    } else {
                        self.out.push_str(&value);
                    }
                    self.out.push_str(&fence);
                }
                self.record(id, start, None, 0);
            }
            Inline::Link(node) => {
                let bare_text = bare_link_text(node);
                if let (Some(text), true) = (bare_text, self.choices.bare(id)) {
                    // Its text, which the parser reads as a literal autolink.
                    self.next_char += text.chars().count();
                    self.next_node += node.children.len();
                    self.push_verbatim(text, context);
                    self.record(id, start, None, 0);
                } else if let Some(uri) = autolink_uri(node) {
                    // A link that an angle-bracket autolink writes is written
                    // as one; its text is no text the read-back escapes.
                    self.next_char += node.children.iter().map(text_chars).sum::<usize>();
                    self.next_node += node.children.len();
                    self.out.push('<');
                    self.push_verbatim(uri, context);
                    self.out.push('>');
                    self.record(id, start, None, 0);
                } else {
                    self.out.push('[');
                    let content_start = self.out.len();
                    self.write_children(&node.children, context)?;
                    let content_end = self.out.len();
                    self.out.push_str("](");
                    self.out.push_str(&serialize_destination_kind(
                        &node.destination,
                        node.destination_kind,
                        context,
                    ));
                    if let (Some(title), Some(title_kind)) = (&node.title, node.title_kind) {
                        self.out.push(' ');
                        self.out
                            .push_str(&serialize_title_kind(title, title_kind, context));
                    }
                    self.out.push(')');
                    self.record(id, start, Some((content_start, content_end)), 0);
                }
                if bare_text.is_some() && self.dropping == 0 {
                    if let Some(last) = self.nodes.last_mut().filter(|last| last.id == id) {
                        last.bare_shape = true;
                    }
                }
            }
            Inline::Image(node) => {
                self.out.push_str("![");
                let content_start = self.out.len();
                self.write_children(&node.alt, context)?;
                let content_end = self.out.len();
                self.out.push_str("](");
                self.out.push_str(&serialize_destination_kind(
                    &node.destination,
                    node.destination_kind,
                    context,
                ));
                if let (Some(title), Some(title_kind)) = (&node.title, node.title_kind) {
                    self.out.push(' ');
                    self.out
                        .push_str(&serialize_title_kind(title, title_kind, context));
                }
                self.out.push(')');
                self.record(id, start, Some((content_start, content_end)), 0);
            }
            Inline::LinkReference(node) => {
                let label =
                    reference_explicit_label(node.meta.span.is_some(), &node.label, context);
                self.write_reference(
                    id,
                    "",
                    &node.children,
                    node.kind,
                    &node.identifier,
                    &label,
                    context,
                )?;
            }
            Inline::ImageReference(node) => {
                let label =
                    reference_explicit_label(node.meta.span.is_some(), &node.label, context);
                self.write_reference(
                    id,
                    "!",
                    &node.alt,
                    node.kind,
                    &node.identifier,
                    &label,
                    context,
                )?;
            }
            Inline::Html(node) => {
                self.push_verbatim(&node.value, context);
                self.record(id, start, None, 0);
            }
            Inline::SoftBreak(_) => {
                self.out.push('\n');
                self.record(id, start, None, 0);
            }
            Inline::LineBreak(node) => {
                match node.kind {
                    LineBreakKind::Backslash => self.out.push_str("\\\n"),
                    LineBreakKind::Spaces => self.out.push_str("  \n"),
                }
                self.record(id, start, None, 0);
            }
            Inline::Math(node) => {
                let written = super::serialize_inline_math(node)?;
                self.push_verbatim(&written, context);
                self.record(id, start, None, 0);
            }
            Inline::FootnoteReference(node) => {
                self.out.push_str("[^");
                if node.meta.span.is_some() {
                    self.out
                        .push_str(&escape_footnote_label_source(&node.label));
                } else {
                    self.out
                        .push_str(&escape_footnote_label_semantic(&node.label));
                }
                self.out.push(']');
                self.record(id, start, None, 0);
            }
            Inline::WikiLink(node) => {
                if node.embed {
                    self.out.push('!');
                }
                self.out.push_str("[[");
                let target = escape_wikilink_part(&node.target);
                let label = escape_wikilink_part(&node.label);
                let separator = if context.table_cell { "\\|" } else { "|" };
                if node.target == node.label {
                    self.out.push_str(&target);
                } else {
                    let (first, second) = match node.label_order {
                        WikiLinkLabelOrder::AfterPipe => (&target, &label),
                        WikiLinkLabelOrder::BeforePipe => (&label, &target),
                    };
                    self.out.push_str(first);
                    self.out.push_str(separator);
                    self.out.push_str(second);
                }
                self.out.push_str("]]");
                self.record(id, start, None, 0);
            }
            Inline::MdxExpression(node) => {
                self.out.push('{');
                self.push_verbatim(&node.value, context);
                self.out.push('}');
                self.record(id, start, None, 0);
            }
            Inline::MdxJsx(node) => {
                self.push_verbatim(&node.value, context);
                self.record(id, start, None, 0);
            }
            Inline::TextDirective(node) => {
                self.out.push(':');
                self.out.push_str(&node.name);
                let mut content = None;
                if !node.label.is_empty() {
                    self.out.push('[');
                    let content_start = self.out.len();
                    self.write_children(&node.label, context)?;
                    content = Some((content_start, self.out.len()));
                    self.out.push(']');
                }
                self.out.push_str(&serialize_attributes_with_context(
                    &node.attributes,
                    context,
                ));
                // What follows can go on with the directive's name, label, or
                // attributes; the read-back closes it where it does.
                if node.attributes.is_empty() {
                    let closed = self.choices.closed(id);
                    if closed >= 1 && node.label.is_empty() {
                        self.out.push_str("[]");
                    }
                    if closed >= 2 || (closed >= 1 && !node.label.is_empty()) {
                        self.out.push_str("{}");
                    }
                }
                self.record(id, start, content, 0);
                if self.dropping == 0 {
                    if let Some(last) = self.nodes.last_mut() {
                        last.directive = true;
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_reference(
        &mut self,
        id: usize,
        bang: &str,
        children: &[Inline],
        kind: ReferenceKind,
        identifier: &str,
        label: &str,
        context: InlineSerializeContext,
    ) -> Result<(), SerializeError> {
        let start = self.out.len();
        self.out.push_str(bang);
        let (chars, nodes) = (self.chars.len(), self.nodes.len());
        let (next_char, next_node) = (self.next_char, self.next_node);
        // The children are written first; a shortcut or collapsed reference
        // whose rendered children do not fold to its identifier writes its
        // label as its text instead.
        let body_start = self.out.len() + 1;
        self.out.push('[');
        self.write_children(children, context)?;
        let rendered = String::from(&self.out[body_start..]);
        let matches = normalize_reference_label(&rendered) == identifier;
        if !matches && !matches!(kind, ReferenceKind::Full) {
            self.out.truncate(body_start - 1);
            self.chars.truncate(chars);
            self.nodes.truncate(nodes);
            self.next_char = next_char;
            self.next_node = next_node;
            let _ = self.render_dropped(children, context)?;
            push_reference_body(&mut self.out, kind, &rendered, false, label);
            self.record(id, start, None, 0);
            return Ok(());
        }
        let content_end = self.out.len();
        self.out.truncate(body_start - 1);
        let mut body = String::new();
        push_reference_body(&mut body, kind, &rendered, true, label);
        // `body` opens with `[`, the rendered children, and `]`.
        self.out.push_str(&body);
        self.record(id, start, Some((body_start, content_end)), 0);
        Ok(())
    }

    /// Writes verbatim content, with the pipes a table cell would split at
    /// escaped: the cell reads each `\|` as `|` inside such content.
    fn push_verbatim(&mut self, text: &str, context: InlineSerializeContext) {
        if context.table_cell {
            self.out.push_str(&table_cell_escape_code_pipes(text));
        } else {
            self.out.push_str(text);
        }
    }
}

/// The text of `link` when it has the shape of a literal autolink: no
/// title, one text child, and a destination that is the text, or the text
/// after `http://` or `mailto:`, as a `www.` or email autolink reads.
fn bare_link_text(link: &Link) -> Option<&str> {
    let [Inline::Text(text)] = link.children.as_slice() else {
        return None;
    };
    let destination = link.destination.as_str();
    let text = text.value.as_str();
    (link.title.is_none()
        && link.destination_kind == LinkDestinationKind::Bare
        && (destination == text
            || destination.strip_prefix("http://") == Some(text)
            || destination.strip_prefix("mailto:") == Some(text)))
    .then_some(text)
}

/// The text chars `inline` holds at any depth, as the writer numbers them.
fn text_chars(inline: &Inline) -> usize {
    match inline {
        Inline::Text(node) => node.value.chars().count(),
        other => inline_children(other)
            .map(|children| children.iter().map(text_chars).sum())
            .unwrap_or(0),
    }
}
