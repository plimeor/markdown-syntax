# The syntax layer of a ProseMirror editor — design

## Overview

The source text is the only truth (decision 0013): a parsed `Document`
keeps the text it was parsed from, and writing it back reuses that text for
everything that did not change. The editor holds each paragraph's text as
Markdown source, syntax characters included, and derives its styling by
parsing that text on every change; it loads a file by parsing it once and
saves by writing the edited tree, both through WebAssembly. The core
additions serve Rust callers too (Settle's index can toggle a task and write
the file the same way), and one new crate carries them to JavaScript.

This plan reuses source at the grain of top-level blocks. The interface is
the one the finer grain needs: the second batch reuses source down to list
items and table rows without changing it.

Measured in a spike against v0.4.0, run outside this repository, on
Node 22: a 348-byte paragraph parses in 13–20 µs in wasm, a 124 KB document
in 3–6 ms; the binding builds with no dependency to 154–174 KB gzipped;
splicing re-serialized blocks into the source caused no read-back failure in
4429 simulated edits, and 95% of top-level blocks re-serialize byte for byte
against 40% of whole files.

## Architecture

```
markdown-syntax (root package, no_std + alloc, zero deps)
  parse, parse_inline, Definitions                 parsing
  Document::source, ::without_source, ::new         the kept source
  Document::inline_source, NodePath, InlineSource   leaf source
  Document::to_markdown[_with]                       write, reusing source
        ▲
markdown-syntax-wasm (wasm/, cdylib, zero deps, publish = false)
  extern "C" exports, handle table, two encodings
        ▲
wasm/js/index.js + index.d.ts (no npm dependencies)
  MarkdownSyntax.load(), parse(), parseInline(), ParsedDocument
        ▲
Settle editor (TS, another repository)
```

Dependencies point down only. The wasm crate has the core crate as its one
dependency, by path; the core crate does not know it exists.

### Inline parse

```rust
/// The link reference definitions that decide whether `[a][b]`, `[a][]`,
/// and `[a]` are reference links.
pub struct Definitions { /* sorted normalized identifiers */ }
impl Definitions {
    pub fn of(document: &Document) -> Self;
    pub fn new<I, S>(labels: I) -> Self where I: IntoIterator<Item = S>, S: AsRef<str>;
}

pub struct InlineOutput { pub inlines: Vec<Inline>, pub diagnostics: Vec<Diagnostic> }

/// Parses `text` as the inline content of one leaf block.
pub fn parse_inline(text: &str, definitions: &Definitions) -> InlineOutput;
```

- It runs the parser's existing inline pass (`parse_inlines`) with a verbatim
  source map, so spans are byte ranges into `text`. No block rule applies:
  `# a`, `- a`, and a second line of `===` stay inline content.
- `Definitions::of` walks the document for `Definition` nodes, including
  nested ones, as the block parser collects them today.
- A leading U+FEFF is content here, unlike in `parse`, because `text` is a
  fragment and not a file; U+0000 is still read as U+FFFD.

### Inline source

```rust
/// Child indices from the document down to one node. A list's children are
/// its items, a table's are its rows, a row's are its cells; every other
/// container's are its blocks.
pub struct NodePath(Vec<usize>);

pub struct InlineSource { pub text: String, /* source map */ }
impl InlineSource {
    /// The source offset of `text[offset]`; `offset == text.len()` maps to
    /// the end of the leaf's content.
    pub fn to_source(&self, offset: usize) -> usize;
    pub fn span_to_source(&self, span: Span) -> Span;
}

impl Document {
    /// The inline content the parser read for the paragraph or heading at
    /// `path`. `None` for a document without a source, or a path that names
    /// no paragraph or heading or a node without a span.
    pub fn inline_source(&self, path: &NodePath) -> Option<InlineSource>;
}
```

- `inline_source` re-runs the block parser
  over the source slice of the top-level block that holds `path`, in a mode
  that keeps each leaf's content string and source map instead of dropping
  them after the inline pass, and returns the one at `path`. A top-level
  block parses the same alone as in its document, apart from reference
  definitions, which do not change block structure. The slice starts at the
  start of the block's line and takes in the definitions right before it,
  which may have opened its paragraph and so made a tab-indented line a
  continuation rather than indented code. The cost is linear in that block
  and those definitions.
- The text is what the inline pass read: container markers, indentation, and
  an ATX heading's opening and closing sequences removed, lines joined with
  their line endings, a setext heading without its underline.
- Table cells and directive labels return `None` in this plan. A cell's
  inline pass reads `\|` as `|` through state that `parse_inline` does not
  take, so the reproduction rule would not hold for them; the second batch
  decides the cell model.

### The kept source

```rust
pub struct Document {
    pub meta: NodeMeta,
    pub children: Vec<Block>,
    source: Option<Arc<str>>,   // private: set only by `parse`
}
impl Document {
    pub fn new(children: Vec<Block>) -> Self;      // no source
    pub fn source(&self) -> Option<&str>;
    pub fn without_source(self) -> Self;          // renders canonically
}
```

- `parse` copies its input into an `Arc<str>`; a clone of the document shares
  it. `Arc` comes from `alloc`, so `no_std` and zero dependencies hold.
- The field is private so that a source can only come from the parse that
  produced the tree. `Document { meta, children }` stops compiling; hand
  construction goes through `Document::new` or `From<Vec<Block>>`. This is
  the release's breaking change.
- Nothing else is kept: no copy of the original tree. What changed is decided
  when writing, against a fresh parse of the source (below), so a caller that
  only reads, like Settle's index, pays one copy of its input and nothing
  more.

### Writing

`to_markdown` and `to_markdown_with` keep their signatures and change what
they do for a document with a source.

- **Unchanged** means: the block carries a span, the document parsed from
  `source` has a top-level block with the same span, and the two are equal
  with `==`, spans at every depth included. The base parse runs once per write,
  only when the document has a source. An editor keeps the spans of a block it
  did not touch and drops them, or rebuilds the block without spans, on one it
  edited.
- The serializer's top-level writer runs as it does today, with one
  substitution. An unchanged block is written as its source slice, without
  its trailing line ending, which the writer adds. Between two unchanged
  blocks that are adjacent in the source, the writer copies the original gap
  instead of writing its own separator. Before the first block and after the
  last, when that block is unchanged, the original leading and trailing bytes
  are kept: the BOM, leading blank lines, and the file's final newline or its
  absence.
- A reused block keeps its own line ending; where a block other than its
  successor in the source follows it, one more line ending makes the blank
  line. Where its source would read differently in its new place, the block
  is written canonically instead: one that a blank line and a paragraph after
  it would change (an unclosed fence, directive, or math block, or a fence in
  a list item that the next line closed), before a block other than its
  successor; one whose source opens with indentation after a list or a
  footnote definition, whose content could take it in, or a list after a list
  written with its marker, after a block other than its predecessor; and a
  thematic break that comes to open the document, where `---` would open
  frontmatter. The first check parses the block's own text with a blank line
  and `x` after it, so its cost is linear in the block and runs once per
  block at most.
- Validation runs on the document first, as for every write; the
  substitution never turns a valid document into an error.
- `options` apply to the blocks the writer writes; reused bytes keep their own
  line endings. The caller passes the file's line ending to avoid mixing.
- An unedited parse writes its source byte for byte. A document without a
  source writes as `to_markdown` writes today; `without_source()` is how a
  caller, and every canonical-output test, asks for that.
- The finer grain in the second batch replaces "top-level block" with "node"
  in the first bullet and gives the writer container prefixes for a reused
  node inside a rewritten container; the signatures stay.

### WebAssembly crate

- `wasm/Cargo.toml`: `crate-type = ["cdylib", "rlib"]` (the `rlib` lets Rust
  unit tests run on the host), `publish = false`, `rust-version = "1.82"`, the
  core crate by path with the `html` feature off. It uses `std`; the target
  provides it.
- Exports: `buffer_alloc`, `buffer_free`, `result_ptr`, `result_len`,
  `parse`, `document_free`, `parse_inline`, `inline_source`, `to_markdown`.
  The buffer exports are not named `alloc` and `free`, which would replace
  the C allocator's symbols when the encoding's unit tests link on the host. `parse`
  keeps the `Document`, which holds its source, in a handle table and returns
  the handle; the later calls take it, so the source crosses into wasm once.
  `to_markdown` decodes the tree JS sends and gives it the handle's source.
- Errors cross as a status code and a diagnostics payload in the result
  buffer; a Rust panic aborts (`panic = "abort"`), which the core crate's
  infallibility promises rule out for valid calls.

### Transport

One encoding, private to the crate and the JS wrapper; it is not a promise.

- **One message shape both ways.** A header of two `u32` (word count,
  string count), the words, a `(start, length)` pair per string, then every
  string as one run of UTF-16 code units, which JS decodes with one
  `TextDecoder("utf-16le")` call and slices. A node is its kind, its span as
  two UTF-16 offsets, and its fields in the order of its Rust struct; a list
  is its length and its nodes. Every value travels in the string table, so
  the two directions share one reader and one writer on each side.
- **wasm → JS.** Offsets are converted in wasm from one byte-to-UTF-16 table
  per document, kept with its handle. The wrapper builds plain objects shaped
  like the Rust AST, camelCased.
- **JS → wasm.** The wrapper checks each node against the same field table
  as it encodes it and throws a `TypeError` naming the first malformed
  node's path; wasm decodes into a `Document` and refuses, as a status code,
  a message that is still malformed. A span that falls between the halves of
  a surrogate pair reads as none.
- The spike's span tree for the 124 KB input was 60 KB against 159 KB of
  JSON, and parse plus encode plus read took 3.4–4.1 ms against 7.0 ms.

## Flows

- **Load.** Read the file → `parse(source)` → handle and tree → for each
  paragraph and heading, `inlineSource(path)` gives the text the editor holds
  and the map back to the file.
- **Keystroke in a paragraph.** `parseInline(text, definitions)` → inline tree
  in the paragraph's own offsets → styling. `definitions` comes from the
  handle and is refreshed when a definition is edited.
- **Save.** Build the edited tree, keeping the spans of untouched top-level
  blocks → `toMarkdown(tree, options)` on the handle → text or diagnostics →
  write the file → `parse` the new text for the next base and free the old
  handle.

## Test scope

- Rust, per requirement: unit cases in `tests/parse_inline.rs`,
  `tests/inline_source.rs`, and `tests/serialize_source.rs`.
- Existing canonical-output tests: every `parse(..).document.to_markdown()`
  in `tests/` becomes `parse(..).document.without_source().to_markdown()`, a
  mechanical change; no golden moves.
- Corpus properties over the read-back corpus of `tests/fixtures.rs` and the
  seeded generators of `tests/serialize_roundtrip_fuzz.rs`: the inline-source
  reproduction rule; the identity write; the spike's simulated edits, with
  the edits that change parsing by themselves (text appended before a literal
  autolink) excluded by checking the canonical write first.
- Cost: the linear-growth and pathological suites run the three new
  functions.
- JS: `node --test wasm/js/` on Node 22, against the built `.wasm`, covering
  each `wasm-binding` requirement, CRLF and non-BMP text for the UTF-16
  offsets, and a round trip of every node kind through both encodings.

## Later phases

The second batch replaces the editor-facing block model after a study of
meowdown's source: flat list items, blank lines as content, every block
spelling recorded, `#tag` in the syntax, and source reused per node instead of
per top-level block. This plan stops at top-level blocks because the spans
that make it safe are already promised there, and the finer grain needs that
model first.

## Decisions

- A separate crate in this repository, with a C ABI of its own and no
  wasm-bindgen — because the core crate keeps zero dependencies and the C ABI
  needs none. Turned down: a `wasm` feature in the core crate, because
  wasm-bindgen would be the core crate's first runtime dependency (0009);
  a separate repository, because the binding moves with the core API.
- `parse_inline` as a second entry point reading the same syntax — because the
  editor reads one paragraph per keystroke, and `parse` applies block rules
  to it and resolves no outside definitions. Turned down: re-parsing the
  whole document per keystroke, 3–6 ms at 124 KB and forcing the editor to
  rebuild the file's text first.
- The document keeps its source, and `to_markdown` reuses it — because the
  source is the only truth (0013), so the one write verb is the lossless one,
  and canonical output is what a document without a source gets. Turned down:
  a second verb taking `base` and `source` as arguments, which leaves the
  default write normalizing the bytes the user never touched; a public
  `source` field, which would let a caller pair a tree with text it was not
  parsed from.
- Changes are found against a fresh parse at write time — because reads far
  outnumber writes and a kept copy of the original tree would double every
  parse's memory. Turned down: keeping the original tree beside the source.
- Unchanged is decided by equality with the parsed block at the same span —
  because the editor then reports nothing beyond the tree. Turned down: a
  list of edited indices from the caller, which can disagree with the tree.
- Reusing source lives in the core crate — because whether a written
  block reads back beside its neighbours is decided by the parser's own
  rules, and the corpus and generators already live here. Turned down: the
  same logic in the editor's TS, which would copy those rules.
- Original gaps are kept only between two unchanged blocks — because a kept
  gap beside a rewritten block can merge the two: `# a\nb` with the heading
  edited into a paragraph would read back as one paragraph `a b`. Turned
  down: keeping every gap of a block that keeps its span, which preserves
  more blank lines and breaks that case.
- Text values sliced by JS where they equal their source — because the spike
  halved transfer time against sending every value. Turned down: JSON.
- The JS wrapper is plain JS with a hand-written `.d.ts` — because it keeps the
  repository free of npm dependencies and a build step. Turned down: a
  TypeScript package compiled with `tsc`.

## Risks

- [A re-serialized block reads back differently next to an unchanged one, or
  merges with it] → task 4.2 checks every top-level block of the corpus;
  a failure is fixed in the writer's separator rule, not listed away.
- [The handle table leaks when JS forgets `free`] → `ParsedDocument.free()` is
  documented; the wrapper also registers a `FinalizationRegistry`.
- [UTF-16 conversion off by one around surrogate pairs and CRLF] → JS tests
  with non-BMP text and CRLF at every node kind.
- [`inline_source` and `parse` disagree because the block re-parse sees a
  slice] → task 3.2 compares them over the corpus and the generators.

## Open questions

- Whether the editor needs `inline_source` for table cells before the second
  batch — answered by the Settle editor spike; it changes only whether a cell
  mode is pulled into this plan.
