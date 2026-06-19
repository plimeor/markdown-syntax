//! Shared, dependency-free types for the AST→HTML conformance bench.
//!
//! These are the FROZEN interface between the three subsystems (extractor,
//! renderer, runner/report). The extractor produces [`OracleTuple`]s; the
//! runner maps each tuple's captured option tokens to a parse
//! [`markdown_syntax::SyntaxOptions`] plus a [`RenderConfig`]; the renderer
//! consumes `(Document, &RenderConfig)`.
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
    /// The runner interprets these into parse options + a [`RenderConfig`].
    pub option_tokens: Vec<String>,
    /// Routing flags (see [`TupleFlags`]).
    pub flags: TupleFlags,
}

/// Render-time configuration the renderer honors. Built by the runner per
/// tuple from its option tokens + suite category.
#[derive(Clone, Copy, Debug)]
pub struct RenderConfig {
    /// Suite-layer category, carried for the two category-divergent oracle
    /// conventions (safe-mode raw HTML form, task-list checkbox attribute
    /// order). It does NOT switch the rendering convention.
    pub category: Category,
    /// Emit raw HTML blocks/inlines verbatim (true) or text-escaped (false).
    pub allow_dangerous_html: bool,
    /// Keep `javascript:`/`vbscript:`/`file:`/`data:` hrefs (true) or blank them.
    pub allow_dangerous_protocol: bool,
    /// Image `src` bypasses the dangerous-protocol filter.
    pub allow_any_img_src: bool,
    /// Apply the GFM tagfilter to raw inline/flow HTML (`<script>`→`&lt;script>`).
    pub gfm_tagfilter: bool,
    /// Task-list `<input>` omits `disabled=""` (GFM `tasklist_classes` etc.).
    pub tasklist_checkable: bool,
}

impl RenderConfig {
    /// Conservative default for a suite category (no danger). The runner
    /// overrides the flag fields per option tokens.
    pub fn new(category: Category) -> Self {
        Self {
            category,
            allow_dangerous_html: false,
            allow_dangerous_protocol: false,
            allow_any_img_src: false,
            gfm_tagfilter: false,
            tasklist_checkable: false,
        }
    }
}
