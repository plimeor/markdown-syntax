# markdown-rs Test Corpus

Files under `upstream-tests/`, `upstream-mdast-util-to-markdown-tests/`, and
`upstream-fuzz/` are copied from `wooorm/markdown-rs` at local comparison
commit `1506572`.

They are included as upstream audit sources for parser and serializer fixture
work. The upstream test assertions target `markdown-rs` APIs and HTML output in
several places, so this crate does not execute them verbatim.

`../derived-cases/markdown-rs/` contains legacy string-literal artifacts derived
from these files. Those artifacts are audited for source and count stability but
are not executable semantic tests.

`../derived-cases/semantic-inputs/markdown-rs/` contains executable Markdown
input cases extracted from recognized parser-facing upstream calls. Each case
declares the syntax profile used by the local fixture runner. Deferred upstream
source groups are listed in `../derived-cases/semantic-inputs/MANIFEST.md`.

`upstream-tests/commonmark.rs` is a generated CommonMark suite. Its examples
also carry CommonMark specification provenance and `CC-BY-SA-4.0` licensing; see
`../commonmark-examples/NOTICE.md`.

The upstream license text is retained in `LICENSE.markdown-rs`.
