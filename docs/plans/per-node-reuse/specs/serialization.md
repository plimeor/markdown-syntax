# Serialization — spec changes

## MODIFIED Requirements

### Requirement: Unchanged source is kept
For a document with a source, `to_markdown` and `to_markdown_with` SHALL treat
a node that carries a span as the successor of the node of the same kind at
that span in the document `parse` returns for the source, when one exists and
no node earlier in document order has claimed it, and SHALL write:

- a successor that corresponds to its base node, whose ancestors all keep
  their own syntax and which follows the sibling it followed in the base, as
  that span's source text;
- any other successor that corresponds to its base node as its source lines
  without the base's container prefixes, under the prefixes it is written
  with;
- a successor whose own fields equal its base node's, with the source text of
  its own syntax (markers, fences, delimiters, underlines, the table's
  delimiter row, and everything else written once per node that is not a
  child or a gap between children), and its children by these rules;
- every other node as canonical output, with its children by these rules.

A node corresponds to a base node of its kind when their spans and their own
fields are equal and their children correspond pairwise; a leaf with its own
source (`with_source`) whose inlines are the ones it was built from
corresponds when that source equals the base leaf's inline source. A leaf
with its own source SHALL be written likewise against that source, its
inlines being the children. The writer SHALL keep the source between two
siblings written from the source that are adjacent in the base, and the
source before the document's first block and after its last when that block
is written from the source there; every other boundary, and every line
prefix not inside kept source, SHALL be canonical. A node SHALL be written as
canonical output even so where its source would read differently in its
place; which placements make a node's source read differently is
unspecified. Options SHALL apply only to the text written as canonical
output. Parsing the output SHALL yield a document equal to the written one
as "Round-trip stability" compares them wherever parsing the canonical
output of the same document does.

#### Scenario: One edited item
- **WHEN** the paragraph of the second item of the document parsed from `"-   a\n-   b\n-   c\n"` is replaced by a hand-built paragraph `Text("x")` and the document is written
- **THEN** it returns `"-   a\n-   x\n-   c\n"`: the item keeps its marker and gap, and the other items are kept

#### Scenario: One edited link
- **WHEN** the target of the `WikiLink` in the document parsed from `"see  [[A]] and `` b ``"` is changed to `B` and the document is written
- **THEN** it returns `"see  [[B]] and `` b ``"`
