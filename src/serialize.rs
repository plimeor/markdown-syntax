//! AST to canonical Markdown. The verbs live on [`Document`]
//! ([`to_markdown`](Document::to_markdown) /
//! [`to_markdown_with`](Document::to_markdown_with)); [`SerializeOptions`] tunes
//! the output style. The document is validated first, so serialization can fail
//! with a [`SerializeError`].
//!
//! The serializer only renders: each node is written in the spelling the AST
//! records for it, or else by one fixed rule. Text, escapes, and character
//! references are written as recorded, and the output is never parsed back.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use crate::{ast::*, diagnostic::Diagnostic, validate::validate_document};

/// The newline style emitted by the serializer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    /// Unix `\n`.
    Lf,
    /// Windows `\r\n`.
    CrLf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

/// The bullet char [`SerializeOptions::bullet`] writes unordered lists with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BulletMarker {
    /// `-`.
    Dash,
    /// `*`.
    Asterisk,
    /// `+`.
    Plus,
}

/// The delimiter [`SerializeOptions::ordered_delimiter`] writes ordered-list
/// markers with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderedDelimiter {
    /// `.`, as in `1.`.
    Period,
    /// `)`, as in `1)`.
    Paren,
}

/// Output-style options for serialization. Defaults: LF, trailing newline, and
/// the list markers and code fences the AST records.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SerializeOptions {
    /// Newline style to emit.
    pub line_ending: LineEnding,
    /// Whether to end the output with a trailing newline.
    pub final_newline: bool,
    /// The bullet for unordered lists: `None`, the default, keeps the marker
    /// each list records; `Some` writes every unordered list with it.
    pub bullet: Option<BulletMarker>,
    /// The delimiter for ordered-list markers: `None`, the default, keeps the
    /// delimiter each list records; `Some` writes every ordered list with it.
    pub ordered_delimiter: Option<OrderedDelimiter>,
    /// The fence char for fenced code blocks: `None`, the default, keeps the
    /// fence each block records; `Some` writes every fenced block with it,
    /// except that an info string holding a backtick always takes tildes.
    pub fence_marker: Option<FenceMarker>,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            line_ending: LineEnding::Lf,
            final_newline: true,
            bullet: None,
            ordered_delimiter: None,
            fence_marker: None,
        }
    }
}

/// Why serialization failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SerializeError {
    /// The AST failed validation; carries the validation diagnostics.
    InvalidDocument(Vec<Diagnostic>),
}

impl Document {
    /// Serialize this document to canonical Markdown with default options.
    pub fn to_markdown(&self) -> Result<String, SerializeError> {
        self.to_markdown_with(&SerializeOptions::default())
    }

    /// Serialize this document to canonical Markdown with explicit options.
    pub fn to_markdown_with(&self, options: &SerializeOptions) -> Result<String, SerializeError> {
        let diagnostics = validate_document(self);
        if !diagnostics.is_empty() {
            return Err(SerializeError::InvalidDocument(diagnostics));
        }

        let mut output = write_blocks(&self.children, options, Join::Gap, true)?;
        if options.line_ending == LineEnding::CrLf {
            output = lf_to_crlf(&output);
        }
        // Blocks end without a line ending, except an HTML block whose last
        // line is empty (the final newline ends that line too) and an indented
        // code block that keeps its value's `\r` or `\r\n` ending.
        if options.final_newline && !output.is_empty() && !ends_with_carriage_return_ending(&output)
        {
            output.push_str(options.line_ending.as_str());
        }
        Ok(output)
    }
}

/// Rewrites each bare `\n` as `\r\n`; existing `\r\n` and `\r` endings that
/// verbatim values carry stay as they are.
fn lf_to_crlf(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut previous = '\0';
    for ch in input.chars() {
        if ch == '\n' && previous != '\r' {
            output.push('\r');
        }
        output.push(ch);
        previous = ch;
    }
    output
}

fn ends_with_carriage_return_ending(input: &str) -> bool {
    input.ends_with('\r') || input.ends_with("\r\n")
}

/// Separates two blocks with one blank line. A block that already ends its
/// last line with `\r` or `\r\n` gets only the blank line, written so that it
/// cannot merge into that ending.
fn push_block_gap(output: &mut String) {
    if output.ends_with('\r') {
        output.push('\r');
    } else if output.ends_with("\r\n") {
        output.push('\n');
    } else {
        output.push_str("\n\n");
    }
}

/// How sibling blocks are separated.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Join {
    /// By a blank line.
    Gap,
    /// By a line ending alone, as the blocks of a tight list item are.
    Line,
}

/// Writes a sequence of sibling blocks. `document_start` is true only for the
/// top-level document body, where the first block sits at byte 0 and a
/// contiguous `---` would open frontmatter.
fn write_blocks(
    blocks: &[Block],
    options: &SerializeOptions,
    join: Join,
    document_start: bool,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    // The marker of the list written right before the current block; ordered
    // and unordered markers never share a char.
    let mut previous_list: Option<char> = None;
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            match join {
                Join::Gap => push_block_gap(&mut output),
                Join::Line => output.push('\n'),
            }
        }
        let after_paragraph_line =
            join == Join::Line && index > 0 && matches!(blocks[index - 1], Block::Paragraph(_));
        let written = match block {
            Block::List(list) => {
                let marker = list_marker(list, options, previous_list);
                previous_list = Some(marker);
                write_list(list, options, marker)?
            }
            Block::ThematicBreak(node) => {
                previous_list = None;
                write_thematic_break(node, document_start && index == 0, after_paragraph_line)
            }
            _ => {
                previous_list = None;
                write_block(block, options)?
            }
        };
        output.push_str(&written);
    }
    Ok(output)
}

fn write_block(block: &Block, options: &SerializeOptions) -> Result<String, SerializeError> {
    match block {
        Block::Paragraph(node) => write_inlines(&node.children, Context::BLOCK),
        Block::Heading(node) => write_heading(node),
        Block::ThematicBreak(node) => Ok(write_thematic_break(node, false, false)),
        Block::BlockQuote(node) => {
            let inner = write_blocks(&node.children, options, Join::Gap, false)?;
            Ok(if inner.is_empty() {
                ">".into()
            } else {
                prefix_lines(&inner, "> ")
            })
        }
        Block::Alert(node) => write_alert(node, options),
        Block::List(node) => write_list(node, options, list_marker(node, options, None)),
        Block::CodeBlock(node) => Ok(write_code_block(node, options)),
        // An HTML block's lines are joined with `\n`, so a value ending in one
        // ends with an empty line that belongs to the block (an unclosed
        // comment, say); it is written as it is.
        Block::HtmlBlock(node) => Ok(node.value.clone()),
        Block::HtmlContainer(node) => write_html_container(node, options),
        Block::Definition(node) => Ok(write_definition(node)),
        Block::FootnoteDefinition(node) => {
            let inner = write_blocks(&node.children, options, Join::Gap, false)?;
            Ok(format!(
                "[^{}]: {}",
                node.label,
                indent_continuation(&inner)
            ))
        }
        Block::Table(node) => write_table(node),
        Block::MathBlock(node) => {
            let fence = block_math_fence(&node.value);
            Ok(fenced_body(&fence, &node.value, &fence))
        }
        Block::Frontmatter(node) => {
            let fence = match node.kind {
                FrontmatterKind::Yaml => "---",
                FrontmatterKind::Toml => "+++",
            };
            // The value holds the lines between the fences joined with `\n`,
            // so a final `\n` is an empty last line.
            Ok(format!("{fence}\n{}\n{fence}", node.value))
        }
        Block::LeafDirective(node) => Ok(format!(
            "::{}{}{}",
            node.name,
            write_directive_label(&node.label)?,
            write_attributes(&node.attributes)
        )),
        Block::ContainerDirective(node) => {
            let mut inner = write_blocks(&node.children, options, Join::Gap, false)?;
            let fence = directive_fence(&inner);
            // Content ends with a line ending before the closing fence; an
            // empty directive takes no blank line, which would loosen a list
            // holding it.
            if !inner.is_empty() {
                inner.push('\n');
            }
            Ok(format!(
                "{fence}{}{}{}\n{inner}{fence}",
                node.name,
                write_directive_label(&node.label)?,
                write_attributes(&node.attributes)
            ))
        }
    }
}

/// A thematic break. A dash break is written spaced where a contiguous `---`
/// would read otherwise: at the document start, where it opens frontmatter,
/// and on the line after a paragraph's, where it underlines a setext heading.
fn write_thematic_break(
    node: &ThematicBreak,
    at_document_start: bool,
    after_paragraph_line: bool,
) -> String {
    match node.marker {
        ThematicBreakMarker::Dash if at_document_start || after_paragraph_line => "- - -".into(),
        ThematicBreakMarker::Dash => "---".into(),
        ThematicBreakMarker::Asterisk => "***".into(),
        ThematicBreakMarker::Underscore => "___".into(),
    }
}

/// Whether `node` is written as a setext heading: a setext underline can only
/// express depth 1 (`=`) or 2 (`-`), and it underlines content.
pub(crate) fn writes_setext(node: &Heading) -> bool {
    node.kind == HeadingKind::Setext && matches!(node.depth, 1 | 2) && !node.children.is_empty()
}

fn write_heading(node: &Heading) -> Result<String, SerializeError> {
    let content = write_inlines(&node.children, Context::HEADING)?;
    if writes_setext(node) {
        let marker = if node.depth == 1 { "=" } else { "-" };
        return Ok(format!(
            "{content}\n{}",
            marker.repeat(content.len().max(3))
        ));
    }
    let hashes = "#".repeat(usize::from(node.depth));
    Ok(if content.is_empty() {
        hashes
    } else {
        format!("{hashes} {content}")
    })
}

fn write_definition(node: &Definition) -> String {
    let destination = write_destination(&node.destination, node.destination_kind);
    let mut output = format!("[{}]: {}", node.label, destination);
    if let Some(title) = &node.title {
        output.push(' ');
        output.push_str(&write_title(title));
    }
    output
}

fn write_html_container(
    node: &HtmlContainer,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    match &node.content {
        HtmlContainerContent::Blocks(children) => {
            let inner = write_blocks(children, options, Join::Gap, false)?;
            Ok(if inner.is_empty() {
                format!("{}\n{}", node.opening.raw, node.closing.raw)
            } else {
                format!("{}\n{}\n\n{}", node.opening.raw, inner, node.closing.raw)
            })
        }
        HtmlContainerContent::Inlines(children) => Ok(format!(
            "{}{}{}",
            node.opening.raw,
            write_inlines(children, Context::BLOCK)?,
            node.closing.raw
        )),
    }
}

fn write_alert(node: &Alert, options: &SerializeOptions) -> Result<String, SerializeError> {
    let mut output = String::from("> [!");
    output.push_str(alert_kind_name(node.kind));
    output.push(']');
    if let Some(title) = &node.title {
        if !title.is_empty() {
            output.push(' ');
            // A line ending would end the marker's line.
            output.push_str(&title.replace(['\n', '\r'], " "));
        }
    }
    let inner = write_blocks(&node.children, options, Join::Gap, false)?;
    if !inner.is_empty() {
        output.push('\n');
        output.push_str(&prefix_lines(&inner, "> "));
    }
    Ok(output)
}

fn alert_kind_name(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Note => "NOTE",
        AlertKind::Tip => "TIP",
        AlertKind::Important => "IMPORTANT",
        AlertKind::Warning => "WARNING",
        AlertKind::Caution => "CAUTION",
    }
}

/// The marker char a list is written with: the one the options replace it
/// with, or else the one the AST records. A marker that the list written right
/// before it in the same container, `previous`, also uses yields to the next
/// one in the order `-`, `*`, `+`, or `.`, `)`; validation keeps two adjacent
/// recorded markers apart, so only a replaced marker can meet this.
fn list_marker(list: &List, options: &SerializeOptions, previous: Option<char>) -> char {
    let marker = if list.ordered {
        options
            .ordered_delimiter
            .map_or_else(|| written_marker(list), ordered_delimiter_char)
    } else {
        options
            .bullet
            .map_or_else(|| written_marker(list), bullet_char)
    };
    if previous == Some(marker) {
        let order: &[char] = if list.ordered {
            &['.', ')']
        } else {
            &['-', '*', '+']
        };
        let at = order.iter().position(|char| *char == marker).unwrap_or(0);
        return order[(at + 1) % order.len()];
    }
    marker
}

/// The marker char the AST records for a list: its bullet, or its ordered
/// delimiter. A delimiter of the other list kind is written `-` or `.`.
/// Serialization and validation's adjacent-list rule both read it.
pub(crate) fn written_marker(list: &List) -> char {
    match (list.ordered, list.delimiter) {
        (false, ListDelimiter::Asterisk) => '*',
        (false, ListDelimiter::Plus) => '+',
        (false, _) => '-',
        (true, ListDelimiter::Paren) => ')',
        (true, _) => '.',
    }
}

fn bullet_char(bullet: BulletMarker) -> char {
    match bullet {
        BulletMarker::Dash => '-',
        BulletMarker::Asterisk => '*',
        BulletMarker::Plus => '+',
    }
}

fn ordered_delimiter_char(delimiter: OrderedDelimiter) -> char {
    match delimiter {
        OrderedDelimiter::Period => '.',
        OrderedDelimiter::Paren => ')',
    }
}

fn write_list(
    node: &List,
    options: &SerializeOptions,
    marker_char: char,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    for (index, item) in node.children.iter().enumerate() {
        if index > 0 {
            output.push_str(if node.tight { "\n" } else { "\n\n" });
        }
        let marker = if node.ordered {
            // Only the first number sets the start, so later items stop
            // counting at the largest number a marker can hold.
            let number = node
                .start
                .unwrap_or(1)
                .saturating_add(index as u64)
                .min(crate::parse::MAX_ORDERED_NUMBER);
            format!("{number}{marker_char} ")
        } else {
            format!("{marker_char} ")
        };
        let inner = write_item_blocks(&item.children, options, node.tight, item.checked)?;
        // A loose list of one item holding one paragraph keeps a blank line
        // inside the item, which is what makes it loose.
        let loose_single = !node.tight
            && node.children.len() == 1
            && matches!(item.children.as_slice(), [Block::Paragraph(_)])
            && !inner.is_empty();
        // A thematic break of the bullet's own char right after the bullet
        // would read as one longer break, so it starts on the next line.
        let break_after_bullet = !node.ordered
            && item.checked.is_none()
            && matches!(
                item.children.first(),
                Some(Block::ThematicBreak(ThematicBreak { marker, .. }))
                    if thematic_break_char(*marker) == marker_char
            );
        // Spaces or a tab opening the item's content would read as padding
        // after the marker, so that content starts on the next line.
        let opens_with_whitespace = inner.starts_with([' ', '\t']);
        if loose_single {
            output.push_str(marker.trim_end());
            output.push_str("\n\n");
            output.push_str(&prefix_lines(&inner, &" ".repeat(marker.len())));
        } else if break_after_bullet || opens_with_whitespace {
            output.push_str(marker.trim_end());
            output.push('\n');
            output.push_str(&prefix_lines(&inner, &" ".repeat(marker.len())));
        } else {
            output.push_str(&marker);
            output.push_str(&indent_after_first_line(&inner, marker.len()));
        }
    }
    Ok(output)
}

fn thematic_break_char(marker: ThematicBreakMarker) -> char {
    match marker {
        ThematicBreakMarker::Dash => '-',
        ThematicBreakMarker::Asterisk => '*',
        ThematicBreakMarker::Underscore => '_',
    }
}

/// An item's blocks; a task item's, `task` holding whether it is checked,
/// open with its checkbox: at the start of the first paragraph after the
/// definitions the item starts with, or else of the item.
fn write_item_blocks(
    blocks: &[Block],
    options: &SerializeOptions,
    tight: bool,
    task: Option<bool>,
) -> Result<String, SerializeError> {
    let join = if tight { Join::Line } else { Join::Gap };
    let Some(checked) = task else {
        return write_blocks(blocks, options, join, false);
    };
    let checkbox = if checked { "[x]" } else { "[ ]" };
    let definitions = blocks
        .iter()
        .take_while(|block| matches!(block, Block::Definition(_)))
        .count();
    if matches!(blocks.get(definitions), Some(Block::Paragraph(_))) {
        let before = write_blocks(&blocks[..definitions], options, join, false)?;
        let rest = write_blocks(&blocks[definitions..], options, join, false)?;
        let mut output = before;
        if !output.is_empty() {
            match join {
                Join::Gap => push_block_gap(&mut output),
                Join::Line => output.push('\n'),
            }
        }
        output.push_str(checkbox);
        output.push(' ');
        output.push_str(&rest);
        return Ok(output);
    }
    let written = write_blocks(blocks, options, join, false)?;
    Ok(if written.is_empty() {
        checkbox.into()
    } else {
        format!("{checkbox} {written}")
    })
}

fn write_code_block(node: &CodeBlock, options: &SerializeOptions) -> String {
    match node.kind {
        CodeBlockKind::Indented => {
            // Each value line ends with a line ending; the block gap or the
            // final newline writes a last `\n`, so only `\r` and `\r\n` stay.
            let body = trim_trailing_newline(&node.value);
            let mut output = prefix_lines(body, "    ");
            let ending = &node.value[body.len()..];
            if matches!(ending, "\r" | "\r\n") {
                output.push_str(ending);
            }
            output
        }
        CodeBlockKind::Fenced { marker, length } => {
            let marker = code_block_fence_marker(node, marker, options);
            let (fence, indent) = code_block_fence(&node.value, marker, length.max(3));
            let mut opener = fence.clone();
            if let Some(info) = &node.info {
                opener.push(' ');
                opener.push_str(&escape_code_info(info));
            }
            let body = fenced_body(&opener, &node.value, &fence);
            if indent == 0 {
                body
            } else {
                prefix_lines(&body, &" ".repeat(indent))
            }
        }
    }
}

/// A fenced block: `opener`, then `value` (whose lines each keep their line
/// ending, the last one optionally), then `closer`. An empty value writes no
/// line between the fences.
fn fenced_body(opener: &str, value: &str, closer: &str) -> String {
    let mut output = String::with_capacity(opener.len() + value.len() + closer.len() + 2);
    output.push_str(opener);
    output.push('\n');
    output.push_str(value);
    if !value.is_empty() && !ends_with_line_ending(value) {
        output.push('\n');
    }
    output.push_str(closer);
    output
}

/// The fence char a code block is written with: tildes when its info string
/// holds a backtick, which a backtick fence's info string cannot, or else the
/// char the AST records or the options replace it with.
fn code_block_fence_marker(
    node: &CodeBlock,
    marker: FenceMarker,
    options: &SerializeOptions,
) -> FenceMarker {
    if node.info.as_deref().is_some_and(|info| info.contains('`')) {
        return FenceMarker::Tilde;
    }
    options.fence_marker.unwrap_or(marker)
}

fn escape_code_info(input: &str) -> String {
    let mut output = String::new();
    // The parser trims the info string, so whitespace at either end is
    // written as a character reference.
    let inner_start = input.len() - input.trim_start_matches([' ', '\t']).len();
    let inner_end = input.trim_end_matches([' ', '\t']).len().max(inner_start);
    for (offset, char) in input.char_indices() {
        match char {
            ' ' | '\t' if offset < inner_start || offset >= inner_end => {
                output.push_str(&char_reference(char));
            }
            '\t' => output.push(char),
            char if char.is_control() => output.push_str(&char_reference(char)),
            '\\' | '&' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

/// A hexadecimal character reference for `char`.
fn char_reference(char: char) -> String {
    format!("&#x{:X};", char as u32)
}

fn write_table(node: &Table) -> Result<String, SerializeError> {
    let delimiter_row = node
        .alignments
        .iter()
        .map(|alignment| match alignment {
            TableAlignment::None => "---",
            TableAlignment::Left => ":---",
            TableAlignment::Center => ":---:",
            TableAlignment::Right => "---:",
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let mut output = String::new();
    for (row_index, row) in node.rows.iter().enumerate() {
        if row_index > 0 {
            output.push('\n');
        }
        let cells = row
            .cells
            .iter()
            .map(|cell| Ok(encode_cell(&write_inlines(&cell.children, Context::BLOCK)?)))
            .collect::<Result<Vec<_>, SerializeError>>()?;
        output.push_str(&format!("| {} |", cells.join(" | ")));
        if row_index == 0 {
            output.push_str(&format!("\n| {delimiter_row} |"));
        }
    }
    Ok(output)
}

/// A cell's inline content, written as anywhere else, encoded as cell
/// source. The parser splits a row at each `|` after no backslash or an even
/// run of them, and reads an odd run before a `|` with one backslash less, so
/// a `\` added before each `|` of the first kind reads back as written. A `|`
/// that the content already puts after an odd run, an escaped `|` in text,
/// reads with one backslash less, still as an escaped `|`; validation rejects
/// the cell values whose meaning that would change.
fn encode_cell(written: &str) -> String {
    let mut output = String::with_capacity(written.len());
    let mut backslashes = 0usize;
    for char in written.chars() {
        if char == '|' && backslashes % 2 == 0 {
            output.push('\\');
        }
        backslashes = if char == '\\' { backslashes + 1 } else { 0 };
        output.push(char);
    }
    output
}

/// Where inline content is written, as the value encodings of its nodes read
/// it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Context {
    /// In a heading, which a line ending would end.
    heading: bool,
}

impl Context {
    const BLOCK: Self = Self { heading: false };
    const HEADING: Self = Self { heading: true };
}

fn write_inlines(inlines: &[Inline], context: Context) -> Result<String, SerializeError> {
    let mut output = InlineOut::default();
    for inline in inlines {
        write_inline(inline, context, &mut output)?;
    }
    Ok(output.text)
}

/// The Markdown written so far for the inline content of one block, cell, or
/// label, which the parser reads in one inline pass, and the backtick runs in
/// it that a code span's fence must not close.
#[derive(Default)]
struct InlineOut {
    text: String,
    /// `open_runs[n]`: a backtick run of length `n` that opens no code span
    /// was written, outside every code span: a later fence of that length
    /// would close it.
    open_runs: Vec<bool>,
    /// How far `text` has been read for `open_runs`.
    scanned: usize,
}

impl InlineOut {
    /// Reads the text written since the last code span for backtick runs. A
    /// backslash escapes the first backtick of a run, which then opens with
    /// the rest of the run.
    fn scan_open_runs(&mut self) {
        let mut escaped = false;
        let mut run = 0;
        let mut run_escaped = false;
        for byte in self.text.as_bytes()[self.scanned..]
            .iter()
            .copied()
            .chain(core::iter::once(b' '))
        {
            if byte == b'`' {
                if run == 0 {
                    run_escaped = escaped;
                }
                run += 1;
                escaped = false;
                continue;
            }
            let opening = run - usize::from(run_escaped && run > 0);
            if opening > 0 {
                if self.open_runs.len() <= opening {
                    self.open_runs.resize(opening + 1, false);
                }
                self.open_runs[opening] = true;
            }
            run = 0;
            escaped = byte == b'\\' && !escaped;
        }
        self.scanned = self.text.len();
    }

    fn is_open_run(&self, length: usize) -> bool {
        self.open_runs.get(length).copied().unwrap_or(false)
    }

    /// Writes, with `write`, content that the parser reads whole from its
    /// first char, so no backtick run in it opens a code span.
    fn push_opaque(&mut self, write: impl FnOnce(&mut String)) {
        self.scan_open_runs();
        write(&mut self.text);
        self.scanned = self.text.len();
    }
}

impl core::ops::Deref for InlineOut {
    type Target = String;

    fn deref(&self) -> &String {
        &self.text
    }
}

impl core::ops::DerefMut for InlineOut {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.text
    }
}

fn write_inline(
    inline: &Inline,
    context: Context,
    out: &mut InlineOut,
) -> Result<(), SerializeError> {
    match inline {
        Inline::Text(node) => out.push_str(&node.value),
        Inline::Escape(node) => {
            out.push('\\');
            out.push(node.value);
        }
        Inline::CharacterReference(node) => out.push_str(&node.reference),
        Inline::Emphasis(node) => {
            let delimiter = emphasis_delimiter(node.delimiter);
            write_span(out, delimiter, &node.children, delimiter, context)?;
        }
        Inline::Strong(node) => {
            let delimiter = emphasis_delimiter(node.delimiter).repeat(2);
            write_span(out, &delimiter, &node.children, &delimiter, context)?;
        }
        Inline::Delete(node) => write_span(out, "~~", &node.children, "~~", context)?,
        Inline::Mark(node) => write_span(out, "==", &node.children, "==", context)?,
        Inline::InlineFootnote(node) => write_span(out, "^[", &node.children, "]", context)?,
        Inline::Shortcode(node) => {
            out.push(':');
            out.push_str(&node.name);
            out.push(':');
        }
        Inline::Code(node) => write_code_span(node, out),
        Inline::Link(node) => {
            write_span(out, "[", &node.children, "](", context)?;
            out.push_opaque(|out| {
                write_resource(
                    out,
                    &node.destination,
                    node.destination_kind,
                    node.title.as_ref(),
                );
            });
        }
        Inline::Autolink(node) => out.push_opaque(|out| match node.form {
            AutolinkForm::Literal => out.push_str(&node.text),
            AutolinkForm::Angle => {
                out.push('<');
                out.push_str(&node.text);
                out.push('>');
            }
        }),
        Inline::Image(node) => {
            write_span(out, "![", &node.alt, "](", context)?;
            out.push_opaque(|out| {
                write_resource(
                    out,
                    &node.destination,
                    node.destination_kind,
                    node.title.as_ref(),
                );
            });
        }
        Inline::LinkReference(node) => {
            write_span(out, "[", &node.children, "]", context)?;
            out.push_opaque(|out| write_reference_kind(out, node.kind, &node.label));
        }
        Inline::ImageReference(node) => {
            write_span(out, "![", &node.alt, "]", context)?;
            out.push_opaque(|out| write_reference_kind(out, node.kind, &node.label));
        }
        Inline::Html(node) => out.push_opaque(|out| out.push_str(&node.value)),
        Inline::SoftBreak(_) if context.heading => out.push(' '),
        Inline::SoftBreak(_) => out.push('\n'),
        Inline::LineBreak(node) => match node.kind {
            LineBreakKind::Backslash => out.push_str("\\\n"),
            LineBreakKind::Spaces => out.push_str("  \n"),
        },
        Inline::Math(node) => {
            let math = write_inline_math(node);
            out.push_opaque(|out| out.push_str(&math));
        }
        Inline::FootnoteReference(node) => {
            out.push_str("[^");
            out.push_str(&node.label);
            out.push(']');
        }
        Inline::WikiLink(node) => out.push_opaque(|out| {
            if node.embed {
                out.push('!');
            }
            out.push_str("[[");
            out.push_str(&node.target);
            if node.target != node.label {
                out.push('|');
                out.push_str(&node.label);
            }
            out.push_str("]]");
        }),
        Inline::TextDirective(node) => {
            // The label is read in an inline pass of its own.
            let label = if node.label.is_empty() {
                String::new()
            } else {
                format!("[{}]", write_inlines(&node.label, context)?)
            };
            out.push_opaque(|out| {
                out.push(':');
                out.push_str(&node.name);
                out.push_str(&label);
                out.push_str(&write_attributes(&node.attributes));
            });
        }
    }
    Ok(())
}

fn emphasis_delimiter(delimiter: EmphasisDelimiter) -> &'static str {
    match delimiter {
        EmphasisDelimiter::Asterisk => "*",
        EmphasisDelimiter::Underscore => "_",
    }
}

/// Writes `open`, `children`, and `close`.
fn write_span(
    out: &mut InlineOut,
    open: &str,
    children: &[Inline],
    close: &str,
    context: Context,
) -> Result<(), SerializeError> {
    out.push_str(open);
    for child in children {
        write_inline(child, context, out)?;
    }
    out.push_str(close);
    Ok(())
}

/// A link's or image's `destination "title")`, after its `](`.
fn write_resource(
    out: &mut String,
    destination: &str,
    kind: LinkDestinationKind,
    title: Option<&Title>,
) {
    out.push_str(&write_destination(destination, kind));
    if let Some(title) = title {
        out.push(' ');
        out.push_str(&write_title(title));
    }
    out.push(')');
}

/// What follows a reference's text: nothing, `[]`, or `[label]`.
fn write_reference_kind(out: &mut String, kind: ReferenceKind, label: &str) {
    match kind {
        ReferenceKind::Shortcut => {}
        ReferenceKind::Collapsed => out.push_str("[]"),
        ReferenceKind::Full => {
            out.push('[');
            // A label is matched as written.
            out.push_str(label);
            out.push(']');
        }
    }
}

/// Writes a code span from its value: fenced by the shortest backtick run
/// that neither the value holds nor a run written before it in the same
/// inline content opens, and padded with a space at each end when the value
/// would otherwise lose an end to the fence or to the space stripping.
fn write_code_span(node: &CodeInline, out: &mut InlineOut) {
    let value = &node.value;
    out.scan_open_runs();
    let held = backtick_runs(value);
    // An escaped backtick written just before joins the opening fence into
    // one longer run, which an earlier open run of that length would take as
    // its close.
    let before = out.len() - out.trim_end_matches('`').len();
    let length = (1..)
        .find(|length| {
            !held.get(*length).copied().unwrap_or(false)
                && !out.is_open_run(*length)
                && (before == 0 || !out.is_open_run(before + *length))
        })
        .unwrap_or(1);
    let fence = "`".repeat(length);
    out.push_str(&fence);
    if code_span_needs_padding(value) {
        out.push(' ');
        out.push_str(value);
        out.push(' ');
    } else {
        out.push_str(value);
    }
    out.push_str(&fence);
    // The span's own runs are matched; what follows is read afresh.
    out.scanned = out.text.len();
}

/// `runs[n]`: whether `value` holds a maximal run of `n` backticks.
fn backtick_runs(value: &str) -> Vec<bool> {
    // A value of `len` bytes holds runs of at most `len` backticks.
    let mut runs = alloc::vec![false; value.len() + 1];
    let mut run = 0;
    for byte in value.bytes() {
        if byte == b'`' {
            run += 1;
        } else {
            runs[run] = true;
            run = 0;
        }
    }
    runs[run] = true;
    runs
}

fn code_span_needs_padding(input: &str) -> bool {
    input.starts_with('`')
        || input.ends_with('`')
        || (input.starts_with(' ') && input.ends_with(' ') && input.chars().any(|char| char != ' '))
}

fn write_inline_math(node: &MathInline) -> String {
    match node.kind {
        MathInlineKind::Code => format!("$`{}`$", node.value),
        // Dollar math is written verbatim behind its exact-length fence.
        MathInlineKind::Dollar { dollars } => {
            let fence = "$".repeat(usize::from(dollars));
            format!("{fence}{}{fence}", node.value)
        }
    }
}

/// A directive's `[label]`, or nothing for an empty label.
fn write_directive_label(label: &[Inline]) -> Result<String, SerializeError> {
    if label.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("[{}]", write_inlines(label, Context::BLOCK)?))
    }
}

fn write_attributes(attributes: &[DirectiveAttribute]) -> String {
    if attributes.is_empty() {
        return String::new();
    }
    let mut output = String::from("{");
    for (index, attribute) in attributes.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        match (&*attribute.name, &attribute.value) {
            ("id", Some(value)) if is_directive_shorthand_value(value) => {
                output.push('#');
                output.push_str(value);
            }
            ("class", Some(value)) if is_directive_shorthand_value(value) => {
                output.push('.');
                output.push_str(value);
            }
            (_, Some(value)) => {
                output.push_str(&attribute.name);
                output.push_str("=\"");
                output.push_str(&escape_title(value, LinkTitleKind::DoubleQuote));
                output.push('"');
            }
            (_, None) => output.push_str(&attribute.name),
        }
    }
    output.push('}');
    output
}

fn is_directive_shorthand_value(input: &str) -> bool {
    !input.is_empty()
        && input
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '-'))
}

fn write_destination(input: &str, kind: LinkDestinationKind) -> String {
    match kind {
        LinkDestinationKind::Omitted if input.is_empty() => String::new(),
        LinkDestinationKind::Angle => {
            let mut output = String::from("<");
            for char in input.chars() {
                match char {
                    char if char.is_control() => output.push_str(&char_reference(char)),
                    '\\' | '<' | '>' => {
                        output.push('\\');
                        output.push(char);
                    }
                    _ => output.push(char),
                }
            }
            output.push('>');
            output
        }
        LinkDestinationKind::Bare | LinkDestinationKind::Omitted if input.is_empty() => "<>".into(),
        LinkDestinationKind::Bare | LinkDestinationKind::Omitted => {
            let mut output = String::new();
            for char in input.chars() {
                match char {
                    // A space would end the destination, and `\ ` is no escape.
                    char if char.is_control() || char == ' ' => {
                        output.push_str(&char_reference(char));
                    }
                    '(' | ')' | '\\' | '<' | '>' | '&' => {
                        output.push('\\');
                        output.push(char);
                    }
                    _ => output.push(char),
                }
            }
            output
        }
    }
}

fn write_title(title: &Title) -> String {
    let (open, close) = match title.kind {
        LinkTitleKind::DoubleQuote => ('"', '"'),
        LinkTitleKind::SingleQuote => ('\'', '\''),
        LinkTitleKind::Paren => ('(', ')'),
    };
    let mut output = String::new();
    output.push(open);
    output.push_str(&escape_title(&title.value, title.kind));
    output.push(close);
    output
}

fn escape_title(input: &str, kind: LinkTitleKind) -> String {
    let mut output = String::new();
    for char in input.chars() {
        match char {
            char if char.is_control() => output.push_str(&char_reference(char)),
            '\\' | '&' => {
                output.push('\\');
                output.push(char);
            }
            '"' if kind == LinkTitleKind::DoubleQuote => output.push_str("\\\""),
            '\'' if kind == LinkTitleKind::SingleQuote => output.push_str("\\'"),
            '(' | ')' if kind == LinkTitleKind::Paren => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

/// Prefixes every line of `input` with `prefix`, keeping each line's ending.
fn prefix_lines(input: &str, prefix: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    let bytes = input.as_bytes();
    let mut output = String::new();
    let mut line_start = 0;
    let mut cursor = 0;
    while cursor < input.len() {
        let eol_end = match bytes[cursor] {
            b'\n' => Some(cursor + 1),
            b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => Some(cursor + 2),
            b'\r' => Some(cursor + 1),
            _ => None,
        };
        if let Some(end) = eol_end {
            output.push_str(prefix);
            output.push_str(&input[line_start..end]);
            cursor = end;
            line_start = cursor;
        } else {
            cursor += 1;
        }
    }
    if line_start < input.len() {
        output.push_str(prefix);
        output.push_str(&input[line_start..]);
    }
    output
}

fn indent_after_first_line(input: &str, width: usize) -> String {
    indent_lines_of(input, width)
}

fn indent_continuation(input: &str) -> String {
    indent_lines_of(input, 4)
}

/// Indents every line of `input` after its first by `width` spaces. Each
/// line keeps its line ending, a final one included.
fn indent_lines_of(input: &str, width: usize) -> String {
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut line_start = 0;
    let mut index = 0;
    let push = |output: &mut String, line: &str, number: usize| {
        if number > 0 {
            output.extend(core::iter::repeat_n(' ', width));
        }
        output.push_str(line);
    };
    let mut number = 0;
    while index < bytes.len() {
        let end = match bytes[index] {
            b'\n' => Some(index + 1),
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => Some(index + 2),
            b'\r' => Some(index + 1),
            _ => None,
        };
        match end {
            Some(end) => {
                push(&mut output, &input[line_start..end], number);
                number += 1;
                index = end;
                line_start = end;
            }
            None => index += 1,
        }
    }
    if line_start < input.len() {
        push(&mut output, &input[line_start..], number);
    }
    output
}

fn trim_trailing_newline(input: &str) -> &str {
    input.trim_end_matches('\n').trim_end_matches('\r')
}

fn ends_with_line_ending(input: &str) -> bool {
    input.ends_with('\n') || input.ends_with('\r')
}

/// The fence for a code block's `value` and the columns the block is indented
/// by, so that no line of `value` closes it. A closing fence is up to three
/// spaces, at least as many marker chars, and nothing else but spaces and
/// tabs. The fence keeps `min_len` chars when indenting the block (which the
/// value's lines lose again) moves every closing-like line past three spaces;
/// otherwise it grows past the longest of them.
fn code_block_fence(value: &str, marker: FenceMarker, min_len: usize) -> (String, usize) {
    let char = match marker {
        FenceMarker::Backtick => '`',
        FenceMarker::Tilde => '~',
    };
    let closing_like = |line: &str, length: usize| {
        let indent = line.len() - line.trim_start_matches(' ').len();
        let rest = &line[indent..];
        let run = rest.len() - rest.trim_start_matches(char).len();
        (indent <= 3 && run >= length && rest[run..].trim_matches([' ', '\t']).is_empty())
            .then_some((indent, run))
    };
    let lines = || value.split(['\n', '\r']);
    let least_indent = lines()
        .filter_map(|line| closing_like(line, min_len))
        .map(|(indent, _)| indent)
        .min();
    match least_indent {
        None => (char.to_string().repeat(min_len), 0),
        Some(indent) if indent > 0 => (char.to_string().repeat(min_len), 4 - indent),
        Some(_) => {
            let mut length = min_len;
            for line in lines() {
                if let Some((_, run)) = closing_like(line, length) {
                    length = run + 1;
                }
            }
            (char.to_string().repeat(length), 0)
        }
    }
}

fn block_math_fence(input: &str) -> String {
    let mut length = 2;
    for line in trim_trailing_newline(input).lines() {
        let trimmed = line.trim();
        if trimmed.len() >= 2 && trimmed.chars().all(|char| char == '$') {
            length = length.max(trimmed.len() + 1);
        }
    }
    "$".repeat(length)
}

fn directive_fence(inner: &str) -> String {
    let mut length = 3;
    for line in inner.lines() {
        if let Some(closing) = directive_closing_fence_len(line) {
            length = length.max(closing + 1);
        }
    }
    ":".repeat(length)
}

fn directive_closing_fence_len(line: &str) -> Option<usize> {
    let trimmed = trim_up_to_three_indent_columns(line)?;
    let length = trimmed
        .as_bytes()
        .iter()
        .take_while(|byte| **byte == b':')
        .count();
    if length >= 3 && trimmed[length..].trim().is_empty() {
        Some(length)
    } else {
        None
    }
}

fn trim_up_to_three_indent_columns(input: &str) -> Option<&str> {
    let mut columns = 0usize;
    let mut bytes = 0usize;
    for byte in input.as_bytes() {
        match *byte {
            b' ' => columns += 1,
            b'\t' => columns += 4 - (columns % 4),
            _ => break,
        }
        bytes += 1;
    }
    (columns <= 3).then_some(&input[bytes..])
}
