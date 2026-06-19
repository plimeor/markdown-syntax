//! Faithful AST → HTML reference renderer for the conformance bench.
//!
//! The renderer consumes a parsed [`markdown_syntax::Document`] plus a
//! [`crate::types::RenderConfig`] and emits HTML byte-comparable (after the
//! normalizer's trailing-newline trim) against the suite oracles. It derives
//! its OWN link-definition map and footnote document state from the document;
//! the runner only supplies a `RenderConfig`.
//!
//! Math and footnotes always render the GFM form. The only behaviour keyed on
//! the suite [`crate::types::Category`] is the two category-divergent oracle
//! conventions (safe-mode raw HTML form, task-list checkbox attribute order).

mod blocks;
mod escape;
mod footnotes;
mod inlines;
mod refs;
mod tables;

use markdown_syntax::Document;

use crate::types::{Category, RenderConfig};

use self::footnotes::FootnoteContext;
use self::refs::DefMap;

/// Render-time context threaded through the block/inline renderers. Holds the
/// resolved definition map, the footnote document state, and the config flags.
pub struct Ctx<'a> {
    pub defs: &'a DefMap,
    pub footnotes: &'a FootnoteContext,
    pub allow_dangerous_html: bool,
    pub allow_dangerous_protocol: bool,
    pub allow_any_img_src: bool,
    pub gfm_tagfilter: bool,
    pub tasklist_checkable: bool,
    /// Suite-layer category, used only for the two category-divergent oracle
    /// conventions (safe-mode raw HTML form, task-list checkbox attribute
    /// order). Math/footnotes do not consult it.
    pub category: Category,
}

impl<'a> Ctx<'a> {
    /// GFM (cmark-gfm) suites use a URL-scheme denylist (keep everything but
    /// `javascript:`/`vbscript:`/`file:`/`data:`); CommonMark (micromark)
    /// suites use an allowlist. The two oracles genuinely disagree on unknown
    /// schemes (cmark-gfm keeps `smb:`; micromark blanks `made-up-scheme:`), so
    /// the link-href policy is category-keyed like the other divergences.
    pub fn gfm_url_denylist(&self) -> bool {
        matches!(self.category, Category::Gfm)
    }
}

/// Render a whole document to HTML.
///
/// Top-level blocks are rendered, empty-string results are filtered, survivors
/// are joined with a single `\n` (no leading/trailing newline), then the
/// document-end footnote section is appended when any footnote was referenced.
pub fn render_document(doc: &Document, cfg: &RenderConfig) -> String {
    let defs = DefMap::build(&doc.children);
    let footnote_ctx = footnotes::build(&doc.children);

    let ctx = Ctx {
        defs: &defs,
        footnotes: &footnote_ctx,
        allow_dangerous_html: cfg.allow_dangerous_html,
        allow_dangerous_protocol: cfg.allow_dangerous_protocol,
        allow_any_img_src: cfg.allow_any_img_src,
        gfm_tagfilter: cfg.gfm_tagfilter,
        tasklist_checkable: cfg.tasklist_checkable,
        category: cfg.category,
    };

    let mut out = blocks::render_blocks_joined(&doc.children, &ctx);

    let section = footnotes::emit_footnote_section(&footnote_ctx, |body| {
        blocks::render_blocks_joined(body, &ctx)
    });
    if !section.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&section);
    }

    out
}
