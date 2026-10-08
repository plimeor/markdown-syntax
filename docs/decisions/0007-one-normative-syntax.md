# 0007: One normative syntax

Status: Accepted
Date: 2026-10-07
Supersedes: 0005

## Context

The crate parsed four dialects through `SyntaxOptions` presets, and the 34
`Constructs` flags could be combined 2^34 ways. The union of the supported
constructs contradicts itself: `~x~` is subscript or strikethrough, `__x__` is
strong or underline, raw HTML and MDX JSX both claim `<`. The maximal default
misread ordinary prose such as `~/.bashrc`, `^12.0.0`, and `[[Prototype]]`
(plimeor/markdown-syntax#14). The serializer had to read its output back under
every configuration, and the tests ran each case under several profiles.

Every caller that used the crate's default already read one syntax. The
parser never builds a modified options value internally: labels, cells, and
headings are re-parsed under the caller's options, and nesting context lives
in other state. A fixed syntax therefore expresses every parse the crate
performs.

## Decision

`parse(input)` is the only parse entry point, and it reads one fixed syntax:

- CommonMark, with raw HTML and indented code;
- `<details>` HTML containers;
- GFM tables, task list items, literal autolinks (the strict GFM extents,
  plus `mailto:` and `xmpp:`; as in cmark-gfm, a `www.`, `http://`, or
  `https://` literal does not form after a link or image `[` that no `]` has
  closed yet, while an email address still links there), and `~~`
  strikethrough;
- footnote definitions, references, and inline footnotes `^[…]`, whose `^[`
  opens no link text, so a URL inside one links;
- GFM alerts, frontmatter, shortcodes, `==` highlight;
- wiki links with the title after the pipe, whose content holds no unescaped
  `[` or `]`, and which win over a defined reference label;
- inline and block math;
- directives, whose names are runs of ASCII letters joined by single `-`, and
  whose text form is followed by whitespace or the end of the content, or by
  ASCII punctuation when it has a non-empty label or non-blank attribute
  braces.

Link text holds no link: an autolink inside link text, image alt included,
reads as its text, an angle-bracket one without its brackets, and a wiki link,
also inside a text directive's label, keeps the brackets around it from forming
a link.

Subscript, superscript, insert, spoiler, underline, description lists,
single-tilde strikethrough, relaxed literal autolinks, and MDX are not part of
the syntax, and their AST nodes do not exist. The syntax is hard-coded: no
public or private configuration value selects constructs.

## Considered options

- Keep the presets and add a normative one as the default: the configuration
  space, the profile-by-profile tests, and the serializer's per-syntax read
  back stay.
- Keep a crate-private constant `Constructs`: dead branches stay alive for a
  configuration that no longer exists.
- Keep relaxed literal-autolink extents for `http(s)://` and `www.`: they
  match only comrak's relaxed mode.
- Block a bare text directive only before `@` or `.letter`: narrower, and it
  keeps `(:note)` and `:h1` as directives.
- Let a defined reference label beat a wiki link: a reading would depend on
  definitions elsewhere in the document.
- Nest an autolink's link inside link text, as cmark-gfm, commonmark.js, and
  micromark do for `[this <http://and.com> that](url)`: link text would hold
  a link, which the AST, validation, and the serializer otherwise rule out.

## Consequences

- A caller who relied on a removed construct reads it as text.
- Tests assert one result per input; fixture profiles and preset loops go.
- An oracle case that this syntax reads differently by decision leaves the
  conformance bench; its input is checked exactly in
  `tests/fixtures/syntax_decisions/`, with HTML verified against a reference
  renderer or the decision.
