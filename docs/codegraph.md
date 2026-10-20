# CodeGraph

CodeGraph gives agents local code navigation and context. The CLI comes from
the official standalone installer, which bundles its own runtime; the project
index lives in `.codegraph/`, which Git ignores.

Install on macOS / Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/colbymchenry/codegraph/main/install.sh | sh
codegraph telemetry off
```

The installer links the CLI to `~/.local/bin/codegraph`, which must be on the
`PATH` the client starts with. Upgrade with `codegraph upgrade`. Initialize and
query from the repository root:

```sh
codegraph init --yes
codegraph explore "src/serialize/reuse.rs"
codegraph status
```

Each checkout is initialized on its own. While the MCP server runs it watches
the sources; changes made while it is down sync on the next connection. When
using only the CLI, run `codegraph sync` before querying.

## Clients

- Codex CLI reads the project's `.codex/config.toml`; the project must be
  trusted. `codex mcp get codegraph` shows whether it was loaded.
- Claude Code reads the project's `.mcp.json`; approve the project's CodeGraph
  server on first connection, and check it with `/mcp`.

Both start `codegraph serve --mcp` from `PATH` with `CODEGRAPH_TELEMETRY=0`,
which turns off anonymous usage statistics for the MCP server; the CLI uses the
`codegraph telemetry off` setting from installation. The MCP tool is the
default `codegraph_explore`; the CLI also has `query`, `callers`, `callees`,
`impact`, and `affected`.

## Index scope

The repository has no `codegraph.json`: the defaults index the root crate, the
`wasm/` binding and its JS wrapper, `tools/gemoji`, and the tests, and skip
`target/` and what `.gitignore` lists. The fixture corpora under
`tests/fixtures/` are not code and are not indexed; read them directly. The
generated tables (`src/entities.rs`, `src/gemoji.rs`, `src/unicode_*.rs`) stay
at normal priority because a question about CJK flanking or tag names should
return `is_cjk` or `is_tag_name_char`.

## Limits

The call graph is for navigation, not a complete dependency proof. Under 1.6.2,
calls through a re-export in `src/lib.rs` get no caller edge: a test or
`wasm/src/lib.rs` calling `markdown_syntax::parse` or `parse_inline` does not
show up in `callers parse` or `impact parse`, while method calls such as
`Document::to_markdown` do. `affected` works per file and does list the tests
that import a changed file. Confirm with a source search before calling
something unused, and never let `impact` or `affected` replace the test
commands in `AGENTS.md`.

Specs, plans, and decisions are read from `docs/` directly; the code graph does
not decide whether a document is in force.
