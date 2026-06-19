use alloc::string::String;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxProfile {
    CommonMark,
    Gfm,
    Mdx,
    Custom,
}

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
}

impl Default for Constructs {
    fn default() -> Self {
        Self::commonmark()
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
    pub profile: SyntaxProfile,
    pub constructs: Constructs,
    pub parse: ParseOptions,
}

impl SyntaxOptions {
    pub fn commonmark() -> Self {
        Self {
            profile: SyntaxProfile::CommonMark,
            constructs: Constructs::commonmark(),
            parse: ParseOptions::default(),
        }
    }

    pub fn gfm() -> Self {
        Self {
            profile: SyntaxProfile::Gfm,
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
            profile: SyntaxProfile::Mdx,
            constructs: Constructs::mdx(),
            parse: ParseOptions::default(),
        }
    }

    pub fn custom(constructs: Constructs, parse: ParseOptions) -> Self {
        Self {
            profile: SyntaxProfile::Custom,
            constructs,
            parse,
        }
    }

    pub fn resolve(&self) -> Result<ResolvedSyntaxOptions, SyntaxConfigError> {
        if (self.constructs.mdx_jsx_block || self.constructs.mdx_jsx_inline)
            && (self.constructs.html_block || self.constructs.html_inline)
        {
            return Err(SyntaxConfigError::MdxHtmlConflict);
        }
        if self.constructs.wikilink_title_after_pipe && self.constructs.wikilink_title_before_pipe {
            return Err(SyntaxConfigError::WikilinkTitleOrderConflict);
        }

        Ok(ResolvedSyntaxOptions {
            profile: self.profile,
            constructs: self.constructs.clone(),
            parse: self.parse.clone(),
        })
    }
}

impl Default for SyntaxOptions {
    fn default() -> Self {
        Self::commonmark()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedSyntaxOptions {
    pub profile: SyntaxProfile,
    pub constructs: Constructs,
    pub parse: ParseOptions,
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
