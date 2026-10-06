//! Writing a block's inline content so that it reads back: the serializer
//! renders the content with its text raw, parses the rendering, and escapes
//! the text chars that parse read as syntax. The parser is the only source of
//! syntax rules; the serializer copies none of them.
//!
//! The steps, each bounded so that the number of parses per block is
//! constant:
//! 1. Render with text raw (backticks escaped), `Escape` and
//!    `CharacterReference` nodes as recorded, emphasis with `*`.
//! 2. Parse the rendering and escape the text chars the parse read as syntax,
//!    a delimiter run counting whole. Repeat, three rounds in all; escapes
//!    that have not settled by then go to step 5.
//! 3. Verify with the tree comparison. A line that landed outside the block
//!    is indented past a block start. Emphasis and strong runs the parse did
//!    not read where they were written switch between `*` and `_`, in each
//!    group of abutting runs one at a time, then the run the comparison
//!    blames; failing that, the text chars touching the delimiters of the
//!    node it blames are written as character references.
//! 4. A block that still does not read back has its abutting emphasis and
//!    strong runs switched with its text raw, since a text char can share a
//!    delimiter run that the parser leaves it literal in.
//! 5. A block that still does not read back has every ASCII punctuation char
//!    of its text escaped; failing that, it is unrepresentable.

use alloc::{format, string::String, vec, vec::Vec};

use super::inline::{inline_children, Choices, Form, Writer, WrittenChar, WrittenNode};
use super::{Place, SerializeError};
use crate::{
    ast::*,
    compare::normalized_inlines,
    diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity},
    options::SyntaxOptions,
    parse::{parse_with_definitions, underscore_run_flanks},
};

/// One piece of a block's Markdown: literal syntax, or inline content.
pub(super) enum Part<'n> {
    Literal(String),
    /// Inline content, and where it sits.
    Inlines(&'n [Inline], Place),
}

/// What a parse of a block's rendering holds where its inline parts were
/// written.
pub(super) enum Extracted<'d> {
    /// The reparsed inline lists, one per inline part.
    Lists(Vec<&'d [Inline]>),
    /// The line starting at this output offset landed outside the block.
    Misplaced(usize),
    /// The block reads back as another block.
    Mismatch,
}

/// What the read-back parses under.
pub(super) struct ReadBack<'o> {
    pub(super) syntax: &'o SyntaxOptions,
    /// The identifiers of the definitions the document holds and of the
    /// references it uses, sorted: the output is read as if each were
    /// defined.
    pub(super) known: Vec<String>,
}

/// Escape rounds before the rendering is verified.
const ESCAPE_ROUNDS: usize = 3;

/// Rounds that switch the delimiters of abutting emphasis and strong runs
/// with the text raw.
const SWITCH_ROUNDS: usize = 8;

/// Rounds that fix a node that does not read back.
const FIX_ROUNDS: usize = 8;

/// The rendering of a block's parts.
struct Rendering {
    text: String,
    chars: Vec<WrittenChar>,
    nodes: Vec<WrittenNode>,
    /// The output range of each inline part.
    segments: Vec<(usize, usize)>,
}

/// Renders `parts`, indenting the lines of each inline part that `indented`
/// lists, by number within the part, past a block start: a paragraph or
/// heading drops a continuation line's indentation.
fn render(
    parts: &[Part<'_>],
    choices: &Choices,
    indented: &[Vec<usize>],
) -> Result<Rendering, SerializeError> {
    let mut writer = Writer::new(choices);
    let mut segments = Vec::new();
    for part in parts {
        match part {
            Part::Literal(text) => writer.push_str(text),
            Part::Inlines(inlines, place) => {
                let start = writer.out.len();
                writer.write(inlines, *place == Place::Cell)?;
                if let Some(lines) = indented
                    .get(segments.len())
                    .filter(|lines| !lines.is_empty())
                {
                    writer.indent_lines(start, lines);
                }
                segments.push((start, writer.out.len()));
            }
        }
    }
    Ok(Rendering {
        text: writer.out,
        chars: writer.chars,
        nodes: writer.nodes,
        segments,
    })
}

/// The inline part and the line within it that output `offset` falls on.
fn line_of(rendering: &Rendering, offset: usize) -> Option<(usize, usize)> {
    let part = rendering
        .segments
        .iter()
        .position(|&(start, end)| start <= offset && offset <= end)?;
    let (start, _) = rendering.segments[part];
    let line = rendering.text[start..offset].matches('\n').count();
    Some((part, line))
}

/// The output ranges a parse read as literal text, as escapes, and as
/// character references.
#[derive(Default)]
struct Trace {
    texts: Vec<(usize, usize)>,
    escapes: Vec<(usize, usize)>,
    references: Vec<(usize, usize)>,
    /// The output ranges of the emphasis (1) and strong (2) nodes read.
    runs: Vec<(usize, usize, u8)>,
    /// The output ranges the parse dropped, which it read neither as text
    /// nor as syntax.
    dropped: Vec<(usize, usize)>,
}

impl Trace {
    fn of(blocks: &[Block]) -> Self {
        let mut trace = Trace::default();
        for block in blocks {
            trace.block(block);
        }
        trace.texts.sort_unstable();
        trace.escapes.sort_unstable();
        trace.references.sort_unstable();
        trace.runs.sort_unstable();
        trace.dropped.sort_unstable();
        trace
    }

    fn block(&mut self, block: &Block) {
        match block {
            Block::Paragraph(node) => self.inlines(&node.children),
            Block::Heading(node) => self.inlines(&node.children),
            Block::BlockQuote(node) => node.children.iter().for_each(|child| self.block(child)),
            Block::Alert(node) => node.children.iter().for_each(|child| self.block(child)),
            Block::List(node) => {
                for item in &node.children {
                    item.children.iter().for_each(|child| self.block(child));
                }
            }
            Block::DescriptionList(node) => {
                for item in &node.children {
                    self.inlines(&item.term);
                    for details in &item.details {
                        details.children.iter().for_each(|child| self.block(child));
                    }
                }
            }
            Block::HtmlContainer(node) => match &node.content {
                HtmlContainerContent::Blocks(children) => {
                    children.iter().for_each(|child| self.block(child))
                }
                HtmlContainerContent::Inlines(children) => self.inlines(children),
            },
            Block::FootnoteDefinition(node) => {
                node.children.iter().for_each(|child| self.block(child))
            }
            Block::Table(node) => {
                for row in &node.rows {
                    for cell in &row.cells {
                        self.inlines(&cell.children);
                    }
                    // The cells past the header's count, which the table
                    // drops.
                    let last = row.cells.iter().rev().find_map(|cell| cell.meta.span);
                    if let (Some(last), Some(row)) = (last, row.meta.span) {
                        self.dropped.push((last.end, row.end));
                    }
                }
            }
            Block::LeafDirective(node) => self.inlines(&node.label),
            Block::ContainerDirective(node) => {
                self.inlines(&node.label);
                node.children.iter().for_each(|child| self.block(child));
            }
            _ => {}
        }
    }

    fn inlines(&mut self, inlines: &[Inline]) {
        for inline in inlines {
            let span = inline.meta().span.map(|span| (span.start, span.end));
            match inline {
                Inline::Text(_) => self.texts.extend(span),
                Inline::Escape(_) => self.escapes.extend(span),
                Inline::CharacterReference(_) => self.references.extend(span),
                other => {
                    let run = match other {
                        Inline::Emphasis(_) => 1,
                        Inline::Strong(_) => 2,
                        _ => 0,
                    };
                    if let Some((start, end)) = span.filter(|_| run != 0) {
                        self.runs.push((start, end, run));
                    }
                    if let Some(children) = inline_children(other) {
                        self.inlines(children);
                    }
                }
            }
        }
    }

    /// Whether the parse read `node`, an emphasis or strong, where it was
    /// written.
    fn read_run(&self, node: &WrittenNode) -> bool {
        self.runs
            .binary_search(&(node.start, node.end, node.run))
            .is_ok()
    }

    /// Whether the parse read the char written at `char` as written, or
    /// dropped it.
    fn literal(&self, char: &WrittenChar) -> bool {
        let range = (char.start, char.end);
        // A table cell reads an escaped pipe as text.
        let in_text = || {
            let after = self.texts.partition_point(|text| text.0 <= char.start);
            after > 0 && self.texts[after - 1].1 >= char.end
        };
        // A pipe in the dropped cells still split them.
        let dropped = || {
            let after = self
                .dropped
                .partition_point(|dropped| dropped.0 <= char.start);
            char.char != '|' && after > 0 && self.dropped[after - 1].1 >= char.end
        };
        match char.form {
            Form::Raw => in_text() || dropped(),
            Form::Backslash => self.escapes.binary_search(&range).is_ok() || in_text() || dropped(),
            Form::Reference => self.references.binary_search(&range).is_ok() || dropped(),
        }
    }
}

/// The chars that delimit runs the parser pairs, which a run counts whole.
fn is_run_char(char: char) -> bool {
    matches!(char, '*' | '_' | '~' | '=' | '+' | '^' | '|' | '$')
}

/// Writes `parts` so that their inline content reads back under `read_back`,
/// `extract` reading the reparsed blocks. Returns the Markdown of each inline
/// part.
pub(super) fn write_reading_back<'n>(
    parts: &[Part<'n>],
    extract: impl for<'d> Fn(&'d [Block]) -> Extracted<'d>,
    read_back: &ReadBack<'_>,
) -> Result<Vec<String>, SerializeError> {
    let originals: Vec<&[Inline]> = parts
        .iter()
        .filter_map(|part| match part {
            Part::Inlines(inlines, _) => Some(*inlines),
            Part::Literal(_) => None,
        })
        .collect();
    let places: Vec<Place> = parts
        .iter()
        .filter_map(|part| match part {
            Part::Inlines(_, place) => Some(*place),
            Part::Literal(_) => None,
        })
        .collect();
    let mut indented = vec![Vec::new(); originals.len()];
    let mut choices = Choices::default();
    let mut fixed = Fixed::default();

    for _ in 0..FIX_ROUNDS + 2 {
        let (rendering, blocks, settled) =
            escape_rounds(parts, &mut choices, &indented, read_back)?;
        let culprits = match extract(&blocks) {
            Extracted::Lists(reparsed) if reparsed.len() == originals.len() => {
                match first_culprit(&originals, &reparsed) {
                    None => return Ok(segments(&rendering)),
                    Some(culprits) => culprits,
                }
            }
            // Escapes that have not settled in three rounds give way to
            // escaping every ASCII punctuation char.
            _ if !settled => break,
            Extracted::Misplaced(offset) => {
                // A continuation line that lands outside the block is
                // indented past a block start.
                match line_of(&rendering, offset) {
                    Some((part, line))
                        if (line > 0 || places[part] == Place::Continuation)
                            && !indented[part].contains(&line) =>
                    {
                        indented[part].push(line);
                        indented[part].sort_unstable();
                        continue;
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        };
        let trace = Trace::of(&blocks);
        if !fix(&culprits, &rendering, &trace, &mut choices, &mut fixed) {
            break;
        }
    }
    // Abutting emphasis and strong runs that do not read back switch their
    // delimiters with the text raw, since a text char can share a delimiter
    // run that the parser leaves it literal in.
    let mut raw = Choices::default();
    fixed.tried.clear();
    let has_runs = |inlines: &[Inline]| {
        fn any_run(inlines: &[Inline]) -> bool {
            inlines.iter().any(|inline| {
                matches!(inline, Inline::Emphasis(_) | Inline::Strong(_))
                    || inline_children(inline).is_some_and(any_run)
            })
        }
        any_run(inlines)
    };
    let switches = if originals.iter().any(|inlines| has_runs(inlines)) {
        SWITCH_ROUNDS
    } else {
        0
    };
    for _ in 0..switches {
        let rendering = render(parts, &raw, &indented)?;
        let blocks = parse(&rendering.text, read_back);
        match extract(&blocks) {
            Extracted::Lists(reparsed) if reparsed.len() == originals.len() => {
                if first_culprit(&originals, &reparsed).is_none() {
                    return Ok(segments(&rendering));
                }
            }
            _ => break,
        }
        let trace = Trace::of(&blocks);
        if !switch_runs(&rendering, &trace, &mut raw, &mut fixed.tried) {
            break;
        }
    }

    // Every ASCII punctuation char of the text escaped.
    let rendering = render(parts, &choices, &indented)?;
    for char in &rendering.chars {
        if char.char.is_ascii_punctuation() && char.form == Form::Raw {
            choices.set_form(char.index, Form::Backslash);
        }
    }
    let rendering = render(parts, &choices, &indented)?;
    let blocks = parse(&rendering.text, read_back);
    let culprits = match extract(&blocks) {
        Extracted::Lists(reparsed) if reparsed.len() == originals.len() => {
            match first_culprit(&originals, &reparsed) {
                None => return Ok(segments(&rendering)),
                Some(culprits) => culprits,
            }
        }
        _ => Vec::new(),
    };
    Err(unrepresentable(&originals, &culprits))
}

fn segments(rendering: &Rendering) -> Vec<String> {
    rendering
        .segments
        .iter()
        .map(|&(start, end)| String::from(&rendering.text[start..end]))
        .collect()
}

fn parse(markdown: &str, read_back: &ReadBack<'_>) -> Vec<Block> {
    parse_with_definitions(markdown, read_back.syntax, &read_back.known)
        .document
        .children
}

/// Renders and escapes what the parse reads as syntax, up to the round limit,
/// and returns the last rendering with its parse, and whether the escapes
/// settled: whether the parse of the last rendering left nothing to escape.
fn escape_rounds(
    parts: &[Part<'_>],
    choices: &mut Choices,
    indented: &[Vec<usize>],
    read_back: &ReadBack<'_>,
) -> Result<(Rendering, Vec<Block>, bool), SerializeError> {
    let mut rendering = render(parts, choices, indented)?;
    let mut blocks = parse(&rendering.text, read_back);
    for _ in 0..ESCAPE_ROUNDS {
        let trace = Trace::of(&blocks);
        if !escape_syntax(&rendering, &trace, choices) {
            return Ok((rendering, blocks, true));
        }
        rendering = render(parts, choices, indented)?;
        blocks = parse(&rendering.text, read_back);
    }
    Ok((rendering, blocks, false))
}

/// Escapes the text chars the parse read as syntax: every text char of a
/// delimiter run that the parse reads any char of as syntax, and in each
/// other stretch of chars read as syntax its first ASCII punctuation char, or
/// its first char; a stretch with an escaped char escapes its first char
/// next, and then escalates its escaped char. Whether anything changed.
fn escape_syntax(rendering: &Rendering, trace: &Trace, choices: &mut Choices) -> bool {
    let text = &rendering.text;
    let chars = &rendering.chars;
    let syntax: Vec<bool> = chars.iter().map(|char| !trace.literal(char)).collect();
    let mut changed = false;
    let mut escape = |char: &WrittenChar, choices: &mut Choices| {
        let form = match char.form {
            Form::Raw if char.char.is_ascii_punctuation() => Form::Backslash,
            Form::Raw | Form::Backslash => Form::Reference,
            Form::Reference => return,
        };
        choices.set_form(char.index, form);
        changed = true;
    };

    // Delimiter runs: raw text chars and the delimiters of written spans.
    let bytes = text.as_bytes();
    let mut delimiter = vec![false; bytes.len()];
    let mut in_run = vec![false; bytes.len()];
    for char in chars.iter().filter(|char| char.form == Form::Raw) {
        in_run[char.start..char.end].fill(true);
    }
    for node in &rendering.nodes {
        let Some((content_start, content_end)) = node.content else {
            continue;
        };
        for (start, end) in [(node.start, content_start), (content_end, node.end)] {
            let run = &bytes[start..end];
            if run.first().is_some_and(|first| {
                is_run_char(char::from(*first)) && run.iter().all(|byte| byte == first)
            }) {
                delimiter[start..end].fill(true);
                in_run[start..end].fill(true);
            }
        }
    }
    let mut handled = vec![false; chars.len()];
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if !in_run[at] || !is_run_char(char::from(byte)) {
            at += 1;
            continue;
        }
        let end = at
            + bytes[at..]
                .iter()
                .zip(&in_run[at..])
                .take_while(|(next, in_run)| **next == byte && **in_run)
                .count();
        let first = chars.partition_point(|char| char.start < at);
        let last = chars.partition_point(|char| char.start < end);
        let raw_text = (first..last).filter(|&index| chars[index].form == Form::Raw);
        let text_count = raw_text.clone().count();
        let read_as_syntax =
            delimiter[at..end].contains(&true) || raw_text.clone().any(|index| syntax[index]);
        if read_as_syntax && text_count > 0 {
            for index in raw_text {
                escape(&chars[index], choices);
                handled[index] = true;
            }
        } else {
            for index in raw_text {
                handled[index] = true;
            }
        }
        at = end;
    }

    // Other stretches of chars read as syntax; a stretch holding a run's char
    // is escaped with the run.
    let mut index = 0;
    while index < chars.len() {
        if !syntax[index] {
            index += 1;
            continue;
        }
        let start = index;
        while index + 1 < chars.len()
            && syntax[index + 1]
            && chars[index + 1].start == chars[index].end
        {
            index += 1;
        }
        if handled[start..=index].contains(&true) {
            index += 1;
            continue;
        }
        let stretch = &chars[start..=index];
        let pick = if stretch.iter().all(|char| char.form == Form::Raw) {
            stretch
                .iter()
                .find(|char| char.char.is_ascii_punctuation())
                .or(stretch.first())
        } else if stretch[0].form == Form::Raw {
            // A stretch still read as syntax after one of its chars was
            // escaped escapes its first char, then escalates.
            stretch.first()
        } else {
            stretch
                .iter()
                .find(|char| char.form == Form::Backslash)
                .or_else(|| stretch.iter().find(|char| char.form == Form::Raw))
        };
        if let Some(pick) = pick {
            escape(pick, choices);
        }
        index += 1;
    }
    changed
}

/// What the fix rounds have tried.
#[derive(Default)]
struct Fixed {
    /// The emphasis and strong nodes written with `_`, by choices tried.
    tried: Vec<Vec<usize>>,
    /// The nodes whose edges were written as references.
    encoded: Vec<usize>,
}

/// Fixes what does not read back: emphasis and strong runs the parse did not
/// read where they were written switch their delimiter, then a blamed
/// emphasis or strong does, and failing that the text chars touching the
/// delimiters of each node blamed are written as references. Whether
/// anything changed.
fn fix(
    culprits: &[usize],
    rendering: &Rendering,
    trace: &Trace,
    choices: &mut Choices,
    fixed: &mut Fixed,
) -> bool {
    if switch_runs(rendering, trace, choices, &mut fixed.tried) {
        return true;
    }
    // A blamed emphasis or strong switches its delimiter alone.
    let mut changed = false;
    for &id in culprits {
        let Some(node) = rendering
            .nodes
            .iter()
            .find(|node| node.id == id && node.run != 0)
        else {
            continue;
        };
        changed |= switch_run(&rendering.text, node, choices, &fixed.tried);
    }
    if changed {
        return true;
    }
    for &id in culprits {
        let Some(node) = rendering.nodes.iter().find(|node| node.id == id) else {
            continue;
        };
        if fixed.encoded.contains(&id) {
            continue;
        }
        fixed.encoded.push(id);
        for char in &rendering.chars {
            let touches = char.end == node.start
                || char.start == node.end
                || node
                    .content
                    .is_some_and(|(start, end)| char.start == start || char.end == end);
            if touches && char.form != Form::Reference {
                choices.set_form(char.index, Form::Reference);
                changed = true;
            }
        }
    }
    changed
}

/// The opening and closing delimiters of `node`, an emphasis or strong.
fn delimiters(node: &WrittenNode) -> [(usize, usize); 2] {
    let (content_start, content_end) = node.content.unwrap_or((node.start, node.end));
    [(node.start, content_start), (content_end, node.end)]
}

/// Whether `run`, an emphasis or strong, can switch its delimiter: back to
/// `*`, or to `_` where the parser lets a `_` run open and close there.
fn can_switch(text: &str, run: &WrittenNode, choices: &Choices) -> bool {
    // A `_` run flanks by the chars around it.
    let underscore_flanks = |(start, end): (usize, usize)| {
        let mut window = String::new();
        window.extend(text[..start].chars().next_back());
        let at = window.len();
        window.extend(core::iter::repeat_n('_', end - start));
        window.extend(text[end..].chars().next());
        underscore_run_flanks(&window, at, end - start)
    };
    let [open, close] = delimiters(run);
    choices.underscored(run.id) || (underscore_flanks(open).0 && underscore_flanks(close).1)
}

/// Switches `run`'s delimiter when it can and the switch makes delimiter
/// choices not tried yet. Whether it switched.
fn switch_run(text: &str, run: &WrittenNode, choices: &mut Choices, tried: &[Vec<usize>]) -> bool {
    if !can_switch(text, run, choices) {
        return false;
    }
    let mut state = choices.clone();
    state.set_underscore(run.id, !choices.underscored(run.id));
    if tried.contains(&state.underscored_ids().to_vec()) {
        return false;
    }
    *choices = state;
    true
}

/// Switches the delimiter of emphasis and strong runs the parse did not read
/// where they were written: in each group of two or more abutting runs
/// holding such a run, one run switches between `*` and `_` where the
/// parser's flanking allows it and the switch makes delimiter choices not
/// tried yet, the last run written that was not read where it was written
/// first. Whether anything changed.
fn switch_runs(
    rendering: &Rendering,
    trace: &Trace,
    choices: &mut Choices,
    tried: &mut Vec<Vec<usize>>,
) -> bool {
    tried.push(choices.underscored_ids().to_vec());
    let mut runs: Vec<&WrittenNode> = rendering
        .nodes
        .iter()
        .filter(|node| node.run != 0)
        .collect();
    runs.sort_unstable_by_key(|node| node.id);
    // The runs whose delimiters start at each offset.
    let mut starts: Vec<(usize, usize)> = Vec::new();
    for (index, run) in runs.iter().enumerate() {
        for (start, _) in delimiters(run) {
            starts.push((start, index));
        }
    }
    starts.sort_unstable();
    let mut abutting: Vec<Vec<usize>> = vec![Vec::new(); runs.len()];
    for (index, run) in runs.iter().enumerate() {
        for (_, end) in delimiters(run) {
            let first = starts.partition_point(|&(start, _)| start < end);
            for &(_, other) in starts[first..]
                .iter()
                .take_while(|&&(start, _)| start == end)
            {
                if other != index {
                    abutting[index].push(other);
                    abutting[other].push(index);
                }
            }
        }
    }

    let mut changed = false;
    let mut grouped = vec![false; runs.len()];
    for first in 0..runs.len() {
        if grouped[first] {
            continue;
        }
        let mut group = vec![first];
        grouped[first] = true;
        let mut next = 0;
        while next < group.len() {
            for &other in &abutting[group[next]] {
                if !grouped[other] {
                    grouped[other] = true;
                    group.push(other);
                }
            }
            next += 1;
        }
        if group.len() < 2 {
            continue;
        }
        group.sort_unstable();
        let misread = |index: &&usize| !trace.read_run(runs[**index]);
        if !group.iter().any(|index| misread(&index)) {
            continue;
        }
        let candidates = group
            .iter()
            .rev()
            .filter(misread)
            .chain(group.iter().rev().filter(|index| !misread(index)));
        for &index in candidates {
            if switch_run(&rendering.text, runs[index], choices, tried) {
                changed = true;
                break;
            }
        }
    }
    changed
}

/// The ids of the original nodes to blame where the reparsed inline lists
/// first differ from the original ones, `None` when they read back.
fn first_culprit(originals: &[&[Inline]], reparsed: &[&[Inline]]) -> Option<Vec<usize>> {
    let mut first_id = 0;
    for (original, reparsed) in originals.iter().zip(reparsed) {
        if let Some(culprits) = culprits(original, reparsed, first_id) {
            return Some(culprits);
        }
        first_id += original.iter().map(subtree_size).sum::<usize>();
    }
    None
}

/// The nodes `inline` numbers, itself included.
fn subtree_size(inline: &Inline) -> usize {
    1 + inline_children(inline)
        .map(|children| children.iter().map(subtree_size).sum())
        .unwrap_or(0)
}

/// A run of an inline list as the tree comparison reads it: text, escapes,
/// and references as one text, or another node.
enum Item<'a> {
    Text { value: String },
    Node { inline: &'a Inline, id: usize },
}

fn items(inlines: &[Inline], first_id: usize) -> Vec<Item<'_>> {
    let mut items = Vec::new();
    let mut id = first_id;
    for inline in inlines {
        let text = match inline {
            Inline::Text(node) => Some(node.value.as_str()),
            Inline::CharacterReference(node) => Some(node.value.as_str()),
            Inline::Escape(_) => None,
            _ => None,
        };
        let escaped;
        let text = match inline {
            Inline::Escape(node) => {
                escaped = String::from(node.value);
                Some(escaped.as_str())
            }
            _ => text,
        };
        match (text, items.last_mut()) {
            (Some(text), Some(Item::Text { value })) => value.push_str(text),
            (Some(text), _) => items.push(Item::Text { value: text.into() }),
            (None, _) => items.push(Item::Node { inline, id }),
        }
        id += subtree_size(inline);
    }
    items
}

/// The nodes to blame where `reparsed` first differs from `original`, whose
/// first node has id `first_id`. An empty list blames the node holding them.
fn culprits(original: &[Inline], reparsed: &[Inline], first_id: usize) -> Option<Vec<usize>> {
    let ours = items(original, first_id);
    let theirs = items(reparsed, 0);
    let neighbours = |at: usize| -> Vec<usize> {
        let mut ids = Vec::new();
        for near in [at.checked_sub(1), Some(at + 1)].into_iter().flatten() {
            if let Some(Item::Node { id, .. }) = ours.get(near) {
                ids.push(*id);
            }
        }
        ids
    };
    for at in 0..ours.len().max(theirs.len()) {
        match (ours.get(at), theirs.get(at)) {
            (Some(Item::Text { value: a }), Some(Item::Text { value: b })) if a == b => {}
            (Some(Item::Node { inline: a, id }), Some(Item::Node { inline: b, .. }))
                if same_node(a, b) =>
            {
                if let (Some(ours), Some(theirs)) = (inline_children(a), inline_children(b)) {
                    if let Some(found) = culprits(ours, theirs, id + 1) {
                        return Some(if found.is_empty() { vec![*id] } else { found });
                    }
                }
            }
            (Some(Item::Node { id, .. }), _) => return Some(vec![*id]),
            (Some(Item::Text { .. }), _) => return Some(neighbours(at)),
            (None, _) => return Some(neighbours(ours.len())),
        }
    }
    None
}

/// Whether two nodes are the same kind with the same values, apart from
/// spans and children.
fn same_node(a: &Inline, b: &Inline) -> bool {
    match (a, b) {
        (Inline::Emphasis(_), Inline::Emphasis(_))
        | (Inline::Strong(_), Inline::Strong(_))
        | (Inline::Underline(_), Inline::Underline(_))
        | (Inline::Insert(_), Inline::Insert(_))
        | (Inline::Mark(_), Inline::Mark(_))
        | (Inline::Subscript(_), Inline::Subscript(_))
        | (Inline::Superscript(_), Inline::Superscript(_))
        | (Inline::Spoiler(_), Inline::Spoiler(_))
        | (Inline::InlineFootnote(_), Inline::InlineFootnote(_)) => true,
        (Inline::Delete(a), Inline::Delete(b)) => a.marker == b.marker,
        (Inline::Link(a), Inline::Link(b)) => {
            a.destination == b.destination
                && a.destination_kind == b.destination_kind
                && a.title == b.title
                && a.title_kind == b.title_kind
        }
        (Inline::Image(a), Inline::Image(b)) => {
            a.destination == b.destination
                && a.destination_kind == b.destination_kind
                && a.title == b.title
                && a.title_kind == b.title_kind
        }
        (Inline::LinkReference(a), Inline::LinkReference(b)) => {
            a.kind == b.kind && a.identifier == b.identifier && a.label == b.label
        }
        (Inline::ImageReference(a), Inline::ImageReference(b)) => {
            a.kind == b.kind && a.identifier == b.identifier && a.label == b.label
        }
        (Inline::TextDirective(a), Inline::TextDirective(b)) => {
            a.name == b.name && a.attributes == b.attributes
        }
        // A code span without its source fence is written from its value.
        (Inline::Code(a), Inline::Code(b)) => {
            a.value == b.value
                && (a.fence_length == 0
                    || a.raw.is_empty()
                    || (a.raw == b.raw && a.fence_length == b.fence_length))
        }
        // Nodes without children compare whole.
        (a, b) if inline_children(a).is_none() => {
            core::mem::discriminant(a) == core::mem::discriminant(b)
                && normalized_inlines(core::slice::from_ref(a))
                    == normalized_inlines(core::slice::from_ref(b))
        }
        _ => false,
    }
}

fn unrepresentable(originals: &[&[Inline]], culprits: &[usize]) -> SerializeError {
    let mut node = None;
    if let Some(&id) = culprits.first() {
        let mut first = 0;
        for inlines in originals {
            if let Some(found) = find_node(inlines, &mut first, id) {
                node = Some(found);
                break;
            }
        }
    }
    let (span, name) = match node {
        Some(inline) => (inline.meta().span, inline_kind(inline)),
        None => (None, "the block's content"),
    };
    SerializeError::Unrepresentable(Diagnostic {
        severity: DiagnosticSeverity::Error,
        code: DiagnosticCode::Unrepresentable,
        span,
        message: format!("{name} has no Markdown that reads back as the same tree"),
    })
}

fn find_node<'a>(inlines: &'a [Inline], next: &mut usize, id: usize) -> Option<&'a Inline> {
    for inline in inlines {
        if *next == id {
            return Some(inline);
        }
        *next += 1;
        if let Some(children) = inline_children(inline) {
            if let Some(found) = find_node(children, next, id) {
                return Some(found);
            }
        }
    }
    None
}

fn inline_kind(inline: &Inline) -> &'static str {
    match inline {
        Inline::Text(_) => "a Text",
        Inline::Escape(_) => "an Escape",
        Inline::CharacterReference(_) => "a CharacterReference",
        Inline::SoftBreak(_) => "a SoftBreak",
        Inline::LineBreak(_) => "a LineBreak",
        Inline::Emphasis(_) => "an Emphasis",
        Inline::Strong(_) => "a Strong",
        Inline::Underline(_) => "an Underline",
        Inline::Delete(_) => "a Delete",
        Inline::Insert(_) => "an Insert",
        Inline::Mark(_) => "a Mark",
        Inline::Subscript(_) => "a Subscript",
        Inline::Superscript(_) => "a Superscript",
        Inline::Spoiler(_) => "a Spoiler",
        Inline::Shortcode(_) => "a Shortcode",
        Inline::Code(_) => "a Code",
        Inline::Link(_) => "a Link",
        Inline::Image(_) => "an Image",
        Inline::LinkReference(_) => "a LinkReference",
        Inline::ImageReference(_) => "an ImageReference",
        Inline::Html(_) => "an Html",
        Inline::Math(_) => "a Math",
        Inline::FootnoteReference(_) => "a FootnoteReference",
        Inline::InlineFootnote(_) => "an InlineFootnote",
        Inline::WikiLink(_) => "a WikiLink",
        Inline::MdxExpression(_) => "an MdxExpression",
        Inline::MdxJsx(_) => "an MdxJsx",
        Inline::TextDirective(_) => "a TextDirective",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse::parse_with_definitions, test_support::generated_inputs};

    /// The trace reads the parse through the spans of its text, escapes,
    /// character references, and emphasis: each covers the source it was
    /// read from, a text's holding its value, and none overlaps another.
    #[test]
    fn the_trace_matches_the_parsed_tree() {
        for options in [
            SyntaxOptions::commonmark(),
            SyntaxOptions::gfm(),
            SyntaxOptions::default(),
            SyntaxOptions::mdx(),
        ] {
            for input in generated_inputs(3_000, 12, 0x5eed_7ace) {
                let blocks = parse_with_definitions(&input, &options, &[])
                    .document
                    .children;
                let trace = Trace::of(&blocks);
                let mut ranges: Vec<(usize, usize)> = trace
                    .texts
                    .iter()
                    .chain(&trace.escapes)
                    .chain(&trace.references)
                    .copied()
                    .collect();
                ranges.sort_unstable();
                for pair in ranges.windows(2) {
                    assert!(pair[0].1 <= pair[1].0, "{input:?}: {pair:?} overlap");
                }
                let mut texts = Vec::new();
                collect_texts(&blocks, &mut texts);
                // A text before a line ending covers the spaces and tabs the
                // line drops.
                for (span, value) in texts {
                    let source = &input[span.0..span.1];
                    let source = if input[span.1..].starts_with(['\n', '\r']) {
                        source.trim_end_matches([' ', '\t'])
                    } else {
                        source
                    };
                    assert_eq!(source, value, "{input:?}: text {span:?}");
                }
                for &(start, end) in &trace.escapes {
                    assert!(
                        input[start..end].starts_with('\\') && end - start > 1,
                        "{input:?}: escape {start}..{end}"
                    );
                }
                for &(start, end) in &trace.references {
                    assert!(
                        input[start..end].starts_with('&') && input[start..end].ends_with(';'),
                        "{input:?}: reference {start}..{end}"
                    );
                }
                for &(start, end, run) in &trace.runs {
                    let delimiter = &input[start..start + usize::from(run)];
                    assert!(
                        delimiter == "**"
                            || delimiter == "__"
                            || delimiter == "*"
                            || delimiter == "_",
                        "{input:?}: run {start}..{end}"
                    );
                    assert_eq!(&input[end - usize::from(run)..end], delimiter, "{input:?}");
                }
            }
        }
    }

    fn collect_texts<'a>(blocks: &'a [Block], texts: &mut Vec<((usize, usize), &'a str)>) {
        fn walk<'a>(inlines: &'a [Inline], texts: &mut Vec<((usize, usize), &'a str)>) {
            for inline in inlines {
                match inline {
                    Inline::Text(node) => {
                        if let Some(span) = node.meta.span {
                            texts.push(((span.start, span.end), node.value.as_str()));
                        }
                    }
                    other => {
                        if let Some(children) = inline_children(other) {
                            walk(children, texts);
                        }
                    }
                }
            }
        }
        for block in blocks {
            match block {
                Block::Paragraph(node) => walk(&node.children, texts),
                Block::Heading(node) => walk(&node.children, texts),
                Block::BlockQuote(node) => collect_texts(&node.children, texts),
                Block::List(node) => node
                    .children
                    .iter()
                    .for_each(|item| collect_texts(&item.children, texts)),
                Block::Table(node) => node
                    .rows
                    .iter()
                    .flat_map(|row| &row.cells)
                    .for_each(|cell| walk(&cell.children, texts)),
                _ => {}
            }
        }
    }
}
