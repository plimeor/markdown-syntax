#![cfg(feature = "html")]

use markdown_syntax::{
    parse, Block, Document, Heading, HeadingKind, HtmlError, HtmlOptions, NodeMeta,
    SafeRawHtmlForm, TasklistAttrOrder,
};

fn parse_render(markdown: &str, html: &HtmlOptions) -> String {
    let output = parse(markdown);
    assert_eq!(output.diagnostics, Vec::new());
    output
        .document
        .to_html_with(html)
        .expect("document renders")
}

fn gfm_html_options() -> HtmlOptions {
    let mut options = HtmlOptions::default();
    options.safe_raw_html_form = SafeRawHtmlForm::OmitPlaceholder;
    options.tasklist_attr_order = TasklistAttrOrder::CheckedFirst;
    options
}

#[test]
fn commonmark_blocks_and_core_inlines_render() {
    let markdown = concat!(
        "# H *em*\n",
        "\n",
        "> quote\n",
        "\n",
        "- one\n",
        "- two\n",
        "\n",
        "---\n",
        "\n",
        "```rust\n",
        "fn main() {}\n",
        "```\n",
        "\n",
        "<div>raw</div>\n",
        "\n",
        "[ref]: https://example.com \"Title\"\n",
        "\n",
        "[link][ref] ![alt *x*][ref] <a@b.c> `code` &amp; \\*  \n",
        "next\n",
    );

    let actual = parse_render(markdown, &HtmlOptions::default());

    assert_eq!(
        actual,
        concat!(
            "<h1>H <em>em</em></h1>\n",
            "<blockquote>\n",
            "<p>quote</p>\n",
            "</blockquote>\n",
            "<ul>\n",
            "<li>one</li>\n",
            "<li>two</li>\n",
            "</ul>\n",
            "<hr />\n",
            "<pre><code class=\"language-rust\">fn main() {}\n",
            "</code></pre>\n",
            "&lt;div&gt;raw&lt;/div&gt;\n",
            "<p><a href=\"https://example.com\" title=\"Title\">link</a> ",
            "<img src=\"https://example.com\" alt=\"alt x\" title=\"Title\" /> ",
            "<a href=\"mailto:a@b.c\">a@b.c</a> <code>code</code> &amp; *<br />\n",
            "next</p>",
        )
    );
}

#[test]
fn extension_blocks_and_inlines_render() {
    let markdown = concat!(
        "> [!NOTE]\n",
        "> Heads up\n",
        "\n",
        "- [x] done\n",
        "- [ ] todo\n",
        "\n",
        "Term\n",
        ": Detail\n",
        "\n",
        "| A | B |\n",
        "| :- | -: |\n",
        "| *x* | ~~gone~~ |\n",
        "\n",
        "++ins++ ==mark== __under__ ~sub~ ^sup^ ||hide|| :rocket: $x$ $$y$$ [[Page|Label]] ^[inline note]\n",
        "\n",
        "```math\n",
        "z\n",
        "```\n",
        "\n",
        "foot[^a]\n",
        "\n",
        "[^a]: note\n",
    );

    let actual = parse_render(markdown, &gfm_html_options());

    assert_eq!(
        actual,
        concat!(
            "<div class=\"markdown-alert markdown-alert-note\">\n",
            "<p class=\"markdown-alert-title\">Note</p>\n",
            "<p>Heads up</p>\n",
            "</div>\n",
            "<ul>\n",
            "<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> done</li>\n",
            "<li><input type=\"checkbox\" disabled=\"\" /> todo</li>\n",
            "</ul>\n",
            "<p>Term\n: Detail</p>\n",
            "<table>\n",
            "<thead>\n",
            "<tr>\n",
            "<th align=\"left\">A</th>\n",
            "<th align=\"right\">B</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr>\n",
            "<td align=\"left\"><em>x</em></td>\n",
            "<td align=\"right\"><del>gone</del></td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n",
            "<p>++ins++ <mark>mark</mark> <strong>under</strong> ~sub~ ",
            "^sup^ ||hide|| \u{1F680} ",
            "<span data-math-style=\"inline\">x</span> ",
            "<span data-math-style=\"display\">y</span> ",
            "<a href=\"Page\" data-wikilink=\"true\">Label</a> ",
            "<sup class=\"footnote-ref\"><a href=\"#fn-__inline_1\" id=\"fnref-__inline_1\" data-footnote-ref>1</a></sup></p>\n",
            "<pre><code class=\"language-math\" data-math-style=\"display\">z\n",
            "</code></pre>\n",
            "<p>foot<sup class=\"footnote-ref\"><a href=\"#fn-a\" id=\"fnref-a\" data-footnote-ref>2</a></sup></p>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-__inline_1\">\n",
            "<p>inline note <a href=\"#fnref-__inline_1\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">\u{21a9}</a></p>\n",
            "</li>\n",
            "<li id=\"fn-a\">\n",
            "<p>note <a href=\"#fnref-a\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"2\" aria-label=\"Back to reference 2\">\u{21a9}</a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>",
        )
    );
}

#[test]
fn directives_render_through_public_options() {
    let markdown = concat!(
        "::leaf[Label]{key=\"v\"}\n",
        "\n",
        ":::note[Title]{class=\"callout\"}\n",
        "Body :abbr[HTML]{title=\"Hyper\"}\n",
        ":::\n",
    );

    let actual = parse_render(markdown, &HtmlOptions::default());

    assert_eq!(
        actual,
        concat!(
            "<div class=\"directive directive-leaf\" data-directive-name=\"leaf\" data-key=\"v\">Label</div>\n",
            "<div class=\"note\" data-class=\"callout\" data-directive-label=\"Title\">\n",
            "<p>Body <span class=\"directive directive-text\" data-directive-name=\"abbr\" data-title=\"Hyper\">HTML</span></p>\n",
            "</div>",
        )
    );
}

#[test]
fn former_mdx_syntax_renders_as_text_and_raw_html() {
    let markdown = concat!(
        "import X from './x'\n",
        "\n",
        "{value}\n",
        "\n",
        "<X />\n",
        "\n",
        "inline {x} <Y /> end\n",
    );

    let actual = parse_render(markdown, &HtmlOptions::default());

    assert_eq!(
        actual,
        concat!(
            "<p>import X from './x'</p>\n",
            "<p>{value}</p>\n",
            "&lt;X /&gt;\n",
            "<p>inline {x} &lt;Y /&gt; end</p>",
        )
    );
}

#[test]
fn html_options_and_validate_first_are_public_contract() {
    let markdown = concat!(
        "<div>x</div>\n",
        "\n",
        "- [x] done\n",
        "\n",
        "smb:///share <smb:///share>\n"
    );

    let default_html = parse_render(markdown, &HtmlOptions::default());
    assert_eq!(
        default_html,
        concat!(
            "&lt;div&gt;x&lt;/div&gt;\n",
            "<ul>\n",
            "<li><input type=\"checkbox\" disabled=\"\" checked=\"\" /> done</li>\n",
            "</ul>\n",
            "<p>smb:///share <a href=\"\">smb:///share</a></p>",
        )
    );

    let gfm_form = parse_render(markdown, &gfm_html_options());
    assert_eq!(
        gfm_form,
        concat!(
            "<!-- raw HTML omitted -->\n",
            "<ul>\n",
            "<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> done</li>\n",
            "</ul>\n",
            "<p>smb:///share <a href=\"smb:///share\">smb:///share</a></p>",
        )
    );

    let invalid = Document {
        meta: NodeMeta::default(),
        children: vec![Block::Heading(Heading {
            meta: NodeMeta::default(),
            depth: 0,
            kind: HeadingKind::Atx,
            children: Vec::new(),
        })],
    };
    assert!(matches!(
        invalid.to_html(),
        Err(HtmlError::InvalidDocument(diagnostics))
            if diagnostics.iter().any(|d| d.message.contains("heading depth"))
    ));
}

#[test]
fn dangerous_protocol_matrix_matches_public_options() {
    assert_eq!(
        parse_render("<javascript:alert(1)>\n", &HtmlOptions::default()),
        "<p><a href=\"\">javascript:alert(1)</a></p>",
    );
    assert_eq!(
        parse_render("[x](javascript:alert(1))\n", &HtmlOptions::default()),
        "<p><a href=\"\">x</a></p>",
    );
    assert_eq!(
        parse_render("![x](javascript:alert(1))\n", &HtmlOptions::default()),
        "<p><img src=\"\" alt=\"x\" /></p>",
    );
    assert_eq!(
        parse_render(
            "[x](irc:///help) ![x](irc:///help)\n",
            &HtmlOptions::default()
        ),
        "<p><a href=\"irc:///help\">x</a> <img src=\"\" alt=\"x\" /></p>",
    );
    assert_eq!(
        parse_render(
            "![x](data:image/png;base64,abc) ![x](data:text/html,abc)\n",
            &HtmlOptions::default()
        ),
        "<p><img src=\"data:image/png;base64,abc\" alt=\"x\" /> <img src=\"\" alt=\"x\" /></p>",
    );
    // A wikilink target is blanked for a dangerous scheme, decoded or not, and
    // kept for a note name that reads as another scheme.
    for source in [
        "[[javascript:alert(1)|x]]\n",
        "[[JavaScript&#58;alert(1)|x]]\n",
        "[[javascript\\:alert(1)|x]]\n",
    ] {
        assert_eq!(
            parse_render(source, &HtmlOptions::default()),
            "<p><a href=\"\" data-wikilink=\"true\">x</a></p>",
            "{source:?}"
        );
    }
    assert_eq!(
        parse_render("![[javascript:alert(1)|x]]\n", &HtmlOptions::default()),
        "<p><a href=\"\" data-wikilink=\"true\" data-wikilink-embed=\"true\">x</a></p>",
    );
    assert_eq!(
        parse_render("[[Note: x]]\n", &HtmlOptions::default()),
        "<p><a href=\"Note:%20x\" data-wikilink=\"true\">Note: x</a></p>",
    );
    // The target is encoded once, as a link destination is.
    assert_eq!(
        parse_render("[[a&b]] [x](a&b)\n", &HtmlOptions::default()),
        "<p><a href=\"a&amp;b\" data-wikilink=\"true\">a&amp;b</a> <a href=\"a&amp;b\">x</a></p>",
    );

    let mut dangerous = HtmlOptions::default();
    dangerous.allow_dangerous_protocol = true;
    assert_eq!(
        parse_render("[[javascript:alert(1)|x]]\n", &dangerous),
        "<p><a href=\"javascript:alert(1)\" data-wikilink=\"true\">x</a></p>",
    );
    assert_eq!(
        parse_render(
            "[x](javascript:alert(1)) ![x](javascript:alert(1))\n",
            &dangerous
        ),
        "<p><a href=\"javascript:alert(1)\">x</a> <img src=\"javascript:alert(1)\" alt=\"x\" /></p>",
    );

    let mut any_img = HtmlOptions::default();
    any_img.allow_any_img_src = true;
    assert_eq!(
        parse_render(
            "[x](javascript:alert(1)) ![x](javascript:alert(1))\n",
            &any_img
        ),
        "<p><a href=\"\">x</a> <img src=\"javascript:alert(1)\" alt=\"x\" /></p>",
    );
}

#[test]
fn raw_html_and_tagfilter_options_are_independent() {
    assert_eq!(
        parse_render("<iframe>\n", &HtmlOptions::default()),
        "&lt;iframe&gt;",
    );

    let mut dangerous = HtmlOptions::default();
    dangerous.allow_dangerous_html = true;
    assert_eq!(parse_render("<iframe>\n", &dangerous), "<iframe>");

    let mut tagfilter_only = HtmlOptions::default();
    tagfilter_only.gfm_tagfilter = true;
    assert_eq!(
        parse_render("<iframe>\n", &tagfilter_only),
        "&lt;iframe&gt;",
    );

    let mut filtered = HtmlOptions::default();
    filtered.allow_dangerous_html = true;
    filtered.gfm_tagfilter = true;
    assert_eq!(
        parse_render("<iframe>\n\n<div>\n", &filtered),
        "&lt;iframe>\n<div>",
    );
    assert_eq!(
        parse_render("a <iframe>\n", &filtered),
        "<p>a &lt;iframe></p>",
    );
}

#[test]
fn a_wiki_embed_is_marked_and_its_target_left_unresolved() {
    assert_eq!(
        parse_render("![[x.png]]", &HtmlOptions::default()),
        "<p><a href=\"x.png\" data-wikilink=\"true\" data-wikilink-embed=\"true\">x.png</a></p>"
    );
}

#[test]
fn shortcodes_render_their_gemoji_glyph() {
    let html = parse_render(":sparkles:", &HtmlOptions::default());
    assert_eq!(html.trim_end(), "<p>\u{2728}</p>");
}

#[test]
fn a_shortcode_in_image_alt_text_reads_as_its_glyph() {
    let html = parse_render("![:tada: x](i.png)", &HtmlOptions::default());
    assert_eq!(
        html.trim_end(),
        "<p><img src=\"i.png\" alt=\"\u{1F389} x\" /></p>"
    );
}
