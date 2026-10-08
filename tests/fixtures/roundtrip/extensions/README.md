# Extension Fixtures

Golden fixtures for syntax beyond CommonMark: `<name>.md` is the input,
`<name>.ast` the tree it parses to, and `<name>.canonical.md` the Markdown that
tree serializes to. `tests/fixtures.rs` finds every fixture here by itself.

## Fixtures named for syntax the crate does not parse

Some fixtures keep the name of a syntax that is not part of the crate's one
syntax. They pin that such input reads as the syntax the crate has, mostly
paragraph text:

- `mdx`, `mdx_esm`, `mdx_html_like`, `mdx_inline`, `mdx_jsx_flow`,
  `mdx_jsx_inline`, `mdx_multiline`: MDX ESM and expressions read as paragraph
  text, and JSX tags as raw HTML where they fit its rules.
- `description_lists_core`, `description_lists_edges`,
  `description_lists_blocks`, and `../stability/description_lists.md`: a `:`
  definition line reads as paragraph text.
- `insert_highlight`: `++insert++` reads as text; `==highlight==` is a `Mark`.
- `inline_markup_extras`: subscript, superscript, and `||spoiler||` markers
  read as text, and a spoiler's bars split table cells.
- `wikilinks_before_pipe` and `../stability/wikilinks_title_before_pipe.md`:
  the title-before-pipe order is not read; the text before the pipe is the
  target, as in `wikilinks_after_pipe`.

The names stay so the goldens keep their paths; a note inside a fixture's `.md`
would change what it parses to.
