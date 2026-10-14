# Keep lines around edits — design

## Leaf lines

`reuse::leaf_lines` (formerly `padded`) turns an edited leaf's text into
lines. The parser strips each line's container prefix and leading whitespace
before the leaf's inline source, so the base leaf's `InlineSource` already
says where each of its lines starts: line `k` of the base text starts at
text offset `t_k`, and its source prefix is
`source[line_start(to_source(t_k)) .. to_source(t_k)]`.

Line `k ≥ 1` of the written text, where the base text has a line `k`, is
written as a full line, that prefix and the line, with the source's line
ending before it, where it reads as the base line does there
(`keeps_line_prefix`):

- it is the base line's text;
- or it parses alone as a paragraph, so it opens no block wherever it
  stands, a lazy line included (`> a\nb` edited to `> a\n3) b` would open a
  list on a lazy line, and takes the canonical prefix instead);
- or no line opens a block where the base line stands
  (`opens_no_block_at`): the base line interrupts a paragraph on the line
  after one (`a\n{line}`) but did not there, which only indentation of four
  columns or more past its container does.

The guard in `leaf_text` that the written text reads as one paragraph counts
a line of the third kind as plain text, so `> a\n    - b` edited at its end
stays as it is instead of being written canonically as a list. Lines past
the base's count keep the canonical prefix.

## Own lines

The same idea for the other lines a node writes from the source:

- a setext heading's underline is its whole source line, with the
  whitespace and line ending that end the text's last line;
- a table's delimiter row is its whole source line, and a later row keeps
  its line's prefix where `keeps_line_prefix` holds;
- an item whose marker stands alone on its line keeps the line's trailing
  whitespace;
- `children` writes a `BlankLine` matched to the base's as its source line,
  with the source's ending before it, unless it opens the container, whose
  own syntax opens that line; `quoted` writes a quote's blank first line as
  its source line;
- an alert's or a directive's first child, which starts on a line after the
  container's own, keeps that line's prefix (`open_later_line`).

## After a container

`keeps_gap` keeps the source between a container and its sibling when the
lines around the gap are the source's, and otherwise when the last leaf
before the gap, as written (`last_leaf`, through the last child of each
container), does not take the line after it (`starts_after_leaf`):

- a paragraph or a definition at the same level takes the next line unless
  the sibling interrupts a paragraph as written (#25's rule);
- one inside containers takes it as a lazy line unless that line, as
  written, ends them: a probe of the containers' markers (`- ` sized to the
  last item's content column, `> `, `[^a]: `) with a paragraph, then the
  sibling's first line, must leave that line out of the outermost one. The
  probe follows the parser's own lazy rules (a fence-like line ends a
  quote's paragraph, `3)` starts a list on a lazy line, `<a>` does not
  continue one);
- a table at the same level takes it unless a table probe leaves it out; a
  table inside containers takes no lazy line;
- an alert without blocks takes it as its paragraph's, the same probe;
- a directive, whose lines no probe stands for, takes it;
- anything else (a closed fence, an HTML container, a blank line, an empty
  item) takes nothing.

`last_leaf` and `starts_after_leaf` name every block kind, so a new kind
must be placed in them to compile.

An open list (`Out::OpenList`), whose last item may take indented lines at a
column the source did not give it, keeps only the source's lines around it.

## The opening break

`write_reusing` keeps a `---` break opening the document from the source.
`Document::write` then scans the output: if its first line is a frontmatter
fence and a later line closes it (`frontmatter_fence_kind`), it writes again
with the break canonical. It scans again after the read-back check rewrites
blocks, since a block written canonically can bring in a closing line (a
directive's `- - -` break written as `---`). The scan is linear and runs
only for documents opening with a break.

## Decisions

- **Keep each line's prefix, not the leaf's whole source shape** — the line
  is the unit the parser strips a prefix from, and line `k` of the base is
  known without diffing. Turned down: diffing old and new text to place
  inserted lines, more code for lines the edit added, which the canonical
  prefix serves.
- **Probe the parser instead of restating its lazy rules** — the lazy rules
  differ from the paragraph-interrupt rules (GH-19, `3)`, `<a>`), and a probe
  of one or two lines follows them as the parser has them. Turned down:
  `written_after_paragraph_interrupts` for lazy lines, which keeps gaps the
  parser would close (`> a\n3) b`) and drops ones it keeps (`> a\n<a>`).
- **Ask the deciding question per case, keep the read-back check as the
  backstop** — the check rewrites a whole top-level block canonically, which
  moves more bytes than the gap it guards (#25's design review). Turned
  down: dropping the guards and relying on the check.
