//! Shared, dependency-free types for the AST→HTML conformance bench.
//!
//! These are the FROZEN interface between the extractor and runner/report. The
//! extractor produces [`OracleTuple`]s; the runner maps each tuple's captured
//! option tokens to parse [`markdown_syntax::SyntaxOptions`] plus public
//! [`markdown_syntax::HtmlOptions`].
//!
//! The bench renders ONE convention (GFM). The only spec-layer split that
//! survives is the suite [`Category`] (commonmark vs gfm), kept purely so a
//! handful of category-divergent oracle conventions (raw-HTML safe-mode form,
//! task-list checkbox attribute order) and the per-suite report grouping stay
//! faithful. It does NOT switch the rendering convention: math is always GFM.

/// Which suite directory a tuple came from (`suite/commonmark/` vs
/// `suite/gfm/`). A spec-layer grouping label for reporting, plus the two
/// oracle conventions whose expected HTML legitimately differs by suite layer
/// (safe-mode raw HTML, task-list checkbox attribute order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    CommonMark,
    Gfm,
}

/// Routing flags captured by the extractor so the runner can exclude tuples it
/// cannot fairly run, instead of silently dropping them (zero-omission intent).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TupleFlags {
    /// MDX case needing an external SWC parser hook the crate does not embed.
    pub mdx_needs_swc: bool,
    /// GFM `html_opts_i(.., |opts| { .. })` — options set by a Rust closure
    /// (header-id-prefix string, url-rewriter Arc) not portable.
    pub gfm_closure: bool,
    /// GFM math case whose expected HTML the extractor already rebuilt by
    /// replaying the per-fn `.replace("<math>", ..)` chain.
    pub math_transform: bool,
    /// Tuple was expanded from a GFM `for example in examples` array loop.
    pub array_expanded: bool,
    /// Expected HTML uses a form this renderer deliberately does not emit
    /// (e.g. the CommonMark `user-content-`/`aria-describedby` footnote shape).
    pub form_divergent: bool,
    /// Construct the crate's parser cannot produce (GFM greentext, subtext,
    /// cjk_friendly_emphasis, phoenix_heex, smart punctuation, multiline block
    /// quotes, escaped-char-spans render mode, sourcepos attributes, …).
    pub parser_unsupported: bool,
}

impl TupleFlags {
    /// True when the tuple cannot be run+compared and must be recorded as
    /// `skipped`, excluded from the headline denominator.
    pub fn is_unrunnable(&self) -> bool {
        self.mdx_needs_swc || self.gfm_closure || self.form_divergent || self.parser_unsupported
    }
}

/// One extracted `(markdown input → expected HTML)` oracle case.
#[derive(Clone, Debug)]
pub struct OracleTuple {
    /// Suite-relative path of the `.cases` file this came from (for reporting).
    pub source_file: &'static str,
    /// Suite-layer category, derived from the fixture subdirectory.
    pub category: Category,
    /// Human label (CommonMark 3rd assert arg, or synthesized `fn`+index).
    pub label: Option<String>,
    /// The markdown source fed to the parser.
    pub input: String,
    /// The expected HTML from the upstream oracle.
    pub expected_html: String,
    /// Raw option identifiers captured at the call site, e.g.
    /// `"allow_dangerous_html"`, `"Options::gfm"`, `"extension.table"`,
    /// `"render.unsafe_"`, `"ParseOptions::mdx"`, `"closure"`, `"math"`.
    /// The runner interprets these into parse options + [`markdown_syntax::HtmlOptions`].
    pub option_tokens: Vec<String>,
    /// Routing flags (see [`TupleFlags`]).
    pub flags: TupleFlags,
}
