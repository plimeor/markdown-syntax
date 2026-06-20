use alloc::string::String;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constructs {
    pub html_block: bool,
    pub html_inline: bool,
    pub indented_code: bool,
    pub gfm_table: bool,
    pub gfm_task_list_item: bool,
    pub gfm_strikethrough: bool,
    pub gfm_autolink_literal: bool,
    /// cmark-gfm "relaxed" URL autolinks: bare `scheme://` URLs (and a bare
    /// leading `://`) are auto-linkified without angle brackets, e.g. `smb://`,
    /// `irc://`, `rdar://`. This is a cmark extension beyond the GFM spec (which
    /// defines only `http(s)://`/`www.`/email); on by default in `gfm()` for
    /// GitHub/cmark-gfm parity. The angle form `<scheme:…>` works regardless.
    pub relaxed_autolinks: bool,
    pub gfm_alert: bool,
    pub underline: bool,
    pub insert: bool,
    pub highlight: bool,
    pub subscript: bool,
    pub superscript: bool,
    pub spoiler: bool,
    pub shortcode: bool,
    pub description_list: bool,
    pub footnote_definition: bool,
    pub footnote_reference: bool,
    pub inline_footnote: bool,
    pub math_block: bool,
    pub math_inline: bool,
    pub frontmatter: bool,
    pub wikilink_title_after_pipe: bool,
    pub wikilink_title_before_pipe: bool,
    pub mdx_esm: bool,
    pub mdx_expression_block: bool,
    pub mdx_expression_inline: bool,
    pub mdx_jsx_block: bool,
    pub mdx_jsx_inline: bool,
    pub directive_text: bool,
    pub directive_leaf: bool,
    pub directive_container: bool,
}

impl Constructs {
    pub const fn commonmark() -> Self {
        Self {
            html_block: true,
            html_inline: true,
            indented_code: true,
            gfm_table: false,
            gfm_task_list_item: false,
            gfm_strikethrough: false,
            gfm_autolink_literal: false,
            relaxed_autolinks: false,
            gfm_alert: false,
            underline: false,
            insert: false,
            highlight: false,
            subscript: false,
            superscript: false,
            spoiler: false,
            shortcode: false,
            description_list: false,
            footnote_definition: false,
            footnote_reference: false,
            inline_footnote: false,
            math_block: false,
            math_inline: false,
            frontmatter: false,
            wikilink_title_after_pipe: false,
            wikilink_title_before_pipe: false,
            mdx_esm: false,
            mdx_expression_block: false,
            mdx_expression_inline: false,
            mdx_jsx_block: false,
            mdx_jsx_inline: false,
            directive_text: false,
            directive_leaf: false,
            directive_container: false,
        }
    }

    pub const fn gfm() -> Self {
        let mut constructs = Self::commonmark();
        constructs.gfm_table = true;
        constructs.gfm_task_list_item = true;
        constructs.gfm_strikethrough = true;
        constructs.gfm_autolink_literal = true;
        constructs.relaxed_autolinks = true;
        constructs.footnote_definition = true;
        constructs.footnote_reference = true;
        constructs
    }

    pub const fn mdx() -> Self {
        let mut constructs = Self::commonmark();
        constructs.html_block = false;
        constructs.html_inline = false;
        constructs.indented_code = false;
        constructs.mdx_esm = true;
        constructs.mdx_expression_block = true;
        constructs.mdx_expression_inline = true;
        constructs.mdx_jsx_block = true;
        constructs.mdx_jsx_inline = true;
        constructs
    }

    /// The maximal non-MDX construct set, and the default dialect: every
    /// construct that does not reinterpret a core CommonMark delimiter. MDX is
    /// off (it conflicts with raw HTML and reinterprets `{…}`/`<…>`), and
    /// `underline` is off because it would parse `__bold__` as underline,
    /// overriding CommonMark strong. The wikilink title order is after-pipe.
    pub const fn max() -> Self {
        Self {
            html_block: true,
            html_inline: true,
            indented_code: true,
            gfm_table: true,
            gfm_task_list_item: true,
            gfm_strikethrough: true,
            gfm_autolink_literal: true,
            relaxed_autolinks: true,
            gfm_alert: true,
            underline: false,
            insert: true,
            highlight: true,
            subscript: true,
            superscript: true,
            spoiler: true,
            shortcode: true,
            description_list: true,
            footnote_definition: true,
            footnote_reference: true,
            inline_footnote: true,
            math_block: true,
            math_inline: true,
            frontmatter: true,
            wikilink_title_after_pipe: true,
            wikilink_title_before_pipe: false,
            mdx_esm: false,
            mdx_expression_block: false,
            mdx_expression_inline: false,
            mdx_jsx_block: false,
            mdx_jsx_inline: false,
            directive_text: true,
            directive_leaf: true,
            directive_container: true,
        }
    }
}

impl Default for Constructs {
    fn default() -> Self {
        Self::max()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParseOptions {
    pub single_tilde_strikethrough: bool,
    pub preserve_character_escapes: bool,
    pub preserve_character_references: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxOptions {
    pub constructs: Constructs,
    pub parse: ParseOptions,
}

impl SyntaxOptions {
    pub fn commonmark() -> Self {
        Self {
            constructs: Constructs::commonmark(),
            parse: ParseOptions::default(),
        }
    }

    pub fn gfm() -> Self {
        Self {
            constructs: Constructs::gfm(),
            parse: ParseOptions {
                single_tilde_strikethrough: true,
                preserve_character_escapes: false,
                preserve_character_references: false,
            },
        }
    }

    pub fn mdx() -> Self {
        Self {
            constructs: Constructs::mdx(),
            parse: ParseOptions::default(),
        }
    }

    /// Enable a [`Construct`] on top of these options, returning the modified
    /// options for chaining. Grouped constructs (footnotes, math, directives, …)
    /// flip every flag in the group so no member is left silently inert.
    pub fn enable(mut self, construct: Construct) -> Self {
        construct.apply(&mut self.constructs, true);
        self
    }

    /// Disable a [`Construct`], the inverse of [`SyntaxOptions::enable`].
    pub fn disable(mut self, construct: Construct) -> Self {
        construct.apply(&mut self.constructs, false);
        self
    }

    pub fn validate(&self) -> Result<(), SyntaxConfigError> {
        if (self.constructs.mdx_jsx_block || self.constructs.mdx_jsx_inline)
            && (self.constructs.html_block || self.constructs.html_inline)
        {
            return Err(SyntaxConfigError::MdxHtmlConflict);
        }
        if self.constructs.wikilink_title_after_pipe && self.constructs.wikilink_title_before_pipe {
            return Err(SyntaxConfigError::WikilinkTitleOrderConflict);
        }

        Ok(())
    }
}

impl Default for SyntaxOptions {
    fn default() -> Self {
        Self {
            constructs: Constructs::max(),
            parse: ParseOptions::default(),
        }
    }
}

/// Where a wikilink's display title sits relative to the `|` separator. The two
/// orders are mutually exclusive ([`SyntaxConfigError::WikilinkTitleOrderConflict`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WikiLinkOrder {
    /// `[[target|title]]` — the Obsidian convention, and the maximal default.
    TitleAfterPipe,
    /// `[[title|target]]`.
    TitleBeforePipe,
}

/// A discoverable, typo-proof front door for toggling a syntax feature via
/// [`SyntaxOptions::enable`] / [`SyntaxOptions::disable`]. Each variant maps to
/// one conceptual feature; grouped features flip every underlying [`Constructs`]
/// flag together. The raw [`Constructs`] struct remains the exhaustive escape
/// hatch for fine-grained control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Construct {
    Table,
    TaskList,
    Strikethrough,
    /// GFM literal autolinks plus the cmark relaxed `scheme://` extension.
    Autolink,
    Alert,
    /// Footnote definitions, references, and inline footnotes.
    Footnotes,
    /// Inline and block math.
    Math,
    Frontmatter,
    Underline,
    Insert,
    Highlight,
    Subscript,
    Superscript,
    Spoiler,
    Shortcode,
    DescriptionList,
    Wikilinks(WikiLinkOrder),
    /// MDX JSX (block and inline). Conflicts with raw HTML; pair with
    /// `disable`-ing HTML or start from [`SyntaxOptions::mdx`].
    MdxJsx,
    /// MDX `{…}` expressions (block and inline).
    MdxExpressions,
    /// MDX ESM `import`/`export` lines.
    MdxEsm,
    /// The `:name` / `::name` / `:::name` directive family.
    Directives,
}

impl Construct {
    fn apply(self, c: &mut Constructs, on: bool) {
        match self {
            Construct::Table => c.gfm_table = on,
            Construct::TaskList => c.gfm_task_list_item = on,
            Construct::Strikethrough => c.gfm_strikethrough = on,
            Construct::Autolink => {
                c.gfm_autolink_literal = on;
                c.relaxed_autolinks = on;
            }
            Construct::Alert => c.gfm_alert = on,
            Construct::Footnotes => {
                c.footnote_definition = on;
                c.footnote_reference = on;
                c.inline_footnote = on;
            }
            Construct::Math => {
                c.math_block = on;
                c.math_inline = on;
            }
            Construct::Frontmatter => c.frontmatter = on,
            Construct::Underline => c.underline = on,
            Construct::Insert => c.insert = on,
            Construct::Highlight => c.highlight = on,
            Construct::Subscript => c.subscript = on,
            Construct::Superscript => c.superscript = on,
            Construct::Spoiler => c.spoiler = on,
            Construct::Shortcode => c.shortcode = on,
            Construct::DescriptionList => c.description_list = on,
            Construct::Wikilinks(order) => {
                c.wikilink_title_after_pipe = on && matches!(order, WikiLinkOrder::TitleAfterPipe);
                c.wikilink_title_before_pipe =
                    on && matches!(order, WikiLinkOrder::TitleBeforePipe);
            }
            Construct::MdxJsx => {
                c.mdx_jsx_block = on;
                c.mdx_jsx_inline = on;
            }
            Construct::MdxExpressions => {
                c.mdx_expression_block = on;
                c.mdx_expression_inline = on;
            }
            Construct::MdxEsm => c.mdx_esm = on,
            Construct::Directives => {
                c.directive_text = on;
                c.directive_leaf = on;
                c.directive_container = on;
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntaxConfigError {
    MdxHtmlConflict,
    WikilinkTitleOrderConflict,
}

impl SyntaxConfigError {
    pub fn message(&self) -> String {
        match self {
            Self::MdxHtmlConflict => "MDX JSX and raw HTML syntax cannot both be enabled".into(),
            Self::WikilinkTitleOrderConflict => {
                "wikilink title-before-pipe and title-after-pipe cannot both be enabled".into()
            }
        }
    }
}
