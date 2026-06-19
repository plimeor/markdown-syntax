# Comrak Fixture Texts

Files under `upstream/`, `upstream-tests/`, and `upstream-fuzz/` are copied
from `kivikakk/comrak` at local comparison commit `d2da7a0`.

They are included as compatibility-oriented upstream audit sources. They are
not treated as HTML-rendering expectations because `markdown-syntax` does not
implement an HTML renderer.

`../derived-cases/comrak/` contains legacy string-literal artifacts derived from
these files. Those artifacts are audited for source and count stability but are
not executable semantic tests.

`../derived-cases/semantic-inputs/comrak/` contains executable Markdown input
cases extracted from recognized parser-facing upstream calls and macros. The
case profile records whether the local fixture runner uses GFM, math,
frontmatter, wikilink, or comrak-extra constructs for that input. Deferred
dynamic, render-only, sourcepos, and helper groups are listed in
`../derived-cases/semantic-inputs/MANIFEST.md`.

The upstream license text is retained in `COPYING.comrak`.
