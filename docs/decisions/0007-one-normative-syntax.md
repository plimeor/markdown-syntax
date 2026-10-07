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
  plus `mailto:` and `xmpp:`), and `~~` strikethrough;
- footnote definitions, references, and inline footnotes `^[…]`;
- GFM alerts, frontmatter, shortcodes, `==` highlight;
- wiki links with the title after the pipe, whose content holds no unescaped
  `[` or `]`, and which win over a defined reference label;
- inline and block math;
- directives, whose names are runs of ASCII letters joined by single `-`, and
  whose text form is followed by whitespace or the end of the content, or by
  ASCII punctuation when it has a non-empty label or non-blank attribute
  braces.

Subscript, superscript, insert, spoiler, underline, description lists,
single-tilde strikethrough, relaxed literal autolinks, and MDX are not part of
the syntax, and their AST nodes do not exist. The syntax is hard-coded: no
public or private configuration value selects constructs.

Decision 0005 stays in force except for its parse-side configuration (section
A): output verbs on `Document`, the single `Diagnostic`, the AST accessors and
constructors, and the curated exports and `prelude` remain as it records them.

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

## Consequences

- A caller who relied on a removed construct reads it as text.
- Tests assert one result per input; fixture profiles and preset loops go.
- The conformance bench lists, in code, the cases that differ from the
  CommonMark and GFM oracles by design.
