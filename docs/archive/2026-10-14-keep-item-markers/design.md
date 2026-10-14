# Keep item markers — design

## Marker text

`reuse::item` writes an item whose own fields are equal with its marker's
source text, `source[item.start .. first_child.start]`:

- on the marker's line, the marker opens the first child's first line
  (`marked`, as before, tabs included);
- on a later line, the marker's text goes through `head_and_prefix`, as a
  footnote definition's does: every line before the first child's is a
  written line with the source's ending, the first opened by the
  containers' own syntax and the rest full; the first child's line keeps
  its source prefix and is full. This path replaces the canonical line
  ending a bare marker line had.

After a checkbox, the first child's line is a lazy one, which only a
paragraph's text continues: it stays only where it is the source's line or
parses alone as a paragraph (`* [ ]\na` edited to `# a` renders). Without a
checkbox the first child's line is indented into the item.

## Tabs

`content_column` gives the column, from the item's start, that lines the
writer indents take: the checkbox's with one (the checkbox is content);
otherwise the column after the marker's whitespace on its line, or the
marker's width plus one where the first child starts on a later line. A
lone tab between the marker and its content is one to four columns wide
depending on where it stands; it counts as four, which keeps those lines in
the item and short of indented code wherever the item is written. Any other
whitespace holding a tab could open indented code at another column, so
the item renders. The `marked` guard still renders an item whose first
child's line opens with whitespace (indented code after the marker).

## Gap probes

`item_marker`, the item's marker in the `keeps_gap` probes, keeps a lone tab
as a tab after a `-` as wide as the marker, so the probe expands it as the
output does where the probe's markers stand as written; it gives `- ` for
any other tab. An empty last item, written as its marker, stands for itself
in `last_leaf`: it takes a line indented past its marker, which the probe
`{markers}{marker}\n{first line}` asks, and always one kept whole after
whitespace on its line, whose indentation the probe does not see. Inside a
container, whose prefix ends with a space, that is nearly every sibling, so
the probe decides at the top level and the boundary is canonical below it.

## Decisions

- **Copy the marker's source text rather than re-encode it** — the marker
  is own syntax (decision 0017), and its text is known from the spans.
  Turned down: normalising tabs to spaces in the marker, which moves bytes.
- **Count a lone tab as its widest** — the writer does not know the output
  column a canonical container prefix gives, and the widest content column
  is right at every column for the lines it indents. Turned down: threading
  each container's canonical column through the writer, more code for the
  rare canonical prefix over a tab, and refusing tabs under every
  container, which keeps the common nested case rendered.
