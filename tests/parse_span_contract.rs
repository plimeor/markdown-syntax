use markdown_syntax::{
    parse, Block, HtmlContainerContent, Inline, ListItem, Span, TableCell, TableRow,
};
use std::path::{Path, PathBuf};

#[path = "support/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;

#[test]
fn top_level_block_spans_slice_the_original_source() {
    for (name, source) in [
        (
            "ordinary_markdown",
            "# Title\n\nparagraph with *emphasis*\n\n- one\n- two\n",
        ),
        ("crlf_line_endings", "# Title\r\n\r\nparagraph\r\n"),
        ("leading_bom", "\u{feff}# title\n\nparagraph\n"),
        ("embedded_nul", "alpha \u{0} beta\n\nparagraph\n"),
        ("unterminated_fence", "```rust\nlet value = 1;\n"),
    ] {
        assert_original_source_tiling(name, source);
    }
}

fn assert_original_source_tiling(name: &str, source: &str) {
    let output = parse(source);
    // A leading byte order mark is not content, so it may precede the first block.
    let mut cursor = if source.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };

    for (index, block) in output.document.children.iter().enumerate() {
        let span = block
            .span()
            .unwrap_or_else(|| panic!("{name}: top-level block {index} has no span"));

        assert!(
            span.start >= cursor,
            "{name}: top-level block {index} overlaps the previous span: {span:?}"
        );
        assert!(
            span.end <= source.len(),
            "{name}: top-level block {index} span exceeds source length: {span:?}, len={}",
            source.len()
        );
        assert!(
            source.is_char_boundary(span.start) && source.is_char_boundary(span.end),
            "{name}: top-level block {index} span is not on UTF-8 boundaries: {span:?}"
        );

        assert!(
            source[cursor..span.start].chars().all(char::is_whitespace),
            "{name}: non-trivia bytes before top-level block {index}: {:?}",
            &source[cursor..span.start]
        );

        let _slice = &source[span.start..span.end];
        cursor = span.end;
    }

    assert!(
        source[cursor..].chars().all(char::is_whitespace),
        "{name}: non-trivia bytes after final top-level block: {:?}",
        &source[cursor..]
    );
}

#[test]
fn inline_container_spans_cover_their_delimiters_and_content() {
    for source in [
        "***a***",
        "**a*",
        "x ***a* b**",
        "***a** b*",
        "*a **b** c*",
        "_a __b__ c_",
        "~~a *b* c~~",
        "==a *b* c==",
        "++a ^b^ c++",
        "||a ~b~ c||",
        "[*a* b](u)",
        "  a *b*",
        "x\r\n  ***a* b**",
        "> x\n> ***a* b**",
        "- x\n  ***a* b**",
        "| ***a* b** | ==c *d*== |\n|-|-|",
    ] {
        let output = parse(source);
        let mut paragraphs = Vec::new();
        collect_inline_parents(&output.document.children, &mut paragraphs);
        assert!(
            !paragraphs.is_empty(),
            "{source:?}: expected inline content"
        );
        for (span, inlines) in paragraphs {
            assert_inline_spans(source, span, inlines);
        }
    }
}

/// The span and inlines of every paragraph and table cell, at any depth.
fn collect_inline_parents<'a>(blocks: &'a [Block], out: &mut Vec<(Span, &'a [Inline])>) {
    for block in blocks {
        match block {
            Block::Paragraph(paragraph) => out.push((
                paragraph.meta.span.expect("parsed paragraph has a span"),
                &paragraph.children,
            )),
            Block::BlockQuote(quote) => collect_inline_parents(&quote.children, out),
            Block::List(list) => list
                .children
                .iter()
                .for_each(|item| collect_inline_parents(&item.children, out)),
            Block::Table(table) => {
                for cell in table.rows.iter().flat_map(|row| &row.cells) {
                    out.push((
                        cell.meta.span.expect("parsed cell has a span"),
                        &cell.children,
                    ));
                }
            }
            _ => {}
        }
    }
}

/// Each inline lies inside its parent and after its previous sibling, and an
/// emphasis-like container starts and ends on its own delimiters.
fn assert_inline_spans(source: &str, parent: Span, inlines: &[Inline]) {
    let mut cursor = parent.start;
    for inline in inlines {
        let span = inline
            .span()
            .unwrap_or_else(|| panic!("{source:?}: {inline:?} has no span"));
        assert!(
            cursor <= span.start && span.end <= parent.end,
            "{source:?}: {inline:?} at {span:?} leaves {parent:?} or overlaps its previous sibling"
        );
        let marker = match inline {
            Inline::Emphasis(_) | Inline::Strong(_) => Some(['*', '_']),
            Inline::Delete(_) => Some(['~', '~']),
            Inline::Mark(_) => Some(['=', '=']),
            _ => None,
        };
        if let Some(marker) = marker {
            let slice = &source[span.start..span.end];
            assert!(
                slice.starts_with(marker) && slice.ends_with(marker),
                "{source:?}: {inline:?} spans {slice:?}"
            );
        }
        assert_inline_spans(source, span, inline.children());
        cursor = span.end;
    }
}

#[test]
fn emphasis_like_spans_start_and_end_on_their_own_delimiters() {
    let spans = |source: &str| {
        let output = parse(source);
        let [Block::Paragraph(paragraph)] = output.document.children.as_slice() else {
            panic!("{source:?}: expected one paragraph");
        };
        let mut found = Vec::new();
        let mut stack: Vec<&Inline> = paragraph.children.iter().rev().collect();
        while let Some(inline) = stack.pop() {
            let kind = match inline {
                Inline::Text(_) => "Text",
                Inline::Emphasis(_) => "Emphasis",
                Inline::Strong(_) => "Strong",
                _ => "Other",
            };
            let span = inline.span().expect("parsed inline has a span");
            found.push((kind, span.start, span.end));
            stack.extend(inline.children().iter().rev());
        }
        found
    };
    assert_eq!(
        spans("***a***"),
        [("Emphasis", 0, 7), ("Strong", 1, 6), ("Text", 3, 4)]
    );
    assert_eq!(
        spans("x ***a* b**"),
        [
            ("Text", 0, 2),
            ("Strong", 2, 11),
            ("Emphasis", 4, 7),
            ("Text", 5, 6),
            ("Text", 7, 9)
        ]
    );
    assert_eq!(
        spans("**a*"),
        [("Text", 0, 1), ("Emphasis", 1, 4), ("Text", 2, 3)]
    );
}

/// (name, source, literals expected at each collected node, in document order)
/// from issue plimeor/markdown-syntax#6.
const CONTAINER_CASES: &[(&str, &str, &[&str])] = &[
    (
        "C01 top-level paragraph",
        "# 标题\n\n见 [[Rust 笔记]] 和 [[标题#小节|显示]]。\n",
        &["[[Rust 笔记]]", "[[标题#小节|显示]]"],
    ),
    (
        "C02 bullet list, one level",
        "- 项目 [[Rust 笔记]]\n- 第二项 [[B]]\n",
        &["[[Rust 笔记]]", "[[B]]"],
    ),
    (
        "C03 nested bullet, 2-space",
        "- 项目\n  - 嵌套 [[library/工作/买菜]]\n",
        &["[[library/工作/买菜]]"],
    ),
    (
        "C04 nested bullet, 4-space",
        "- 项目\n    - 嵌套 [[A]]\n",
        &["[[A]]"],
    ),
    (
        "C05 nested ordered list",
        "1. 第一\n   1. 子项 [[A]]\n",
        &["[[A]]"],
    ),
    (
        "C06 task list",
        "- [ ] 买菜 [[library/工作/买菜]]\n- [x] 完成 [[B]]\n",
        &["[[library/工作/买菜]]", "[[B]]"],
    ),
    (
        "C07 nested task list",
        "- [ ] 父任务\n  - [ ] 子任务 [[A]]\n",
        &["[[A]]"],
    ),
    (
        "C08 list item, wrapped line",
        "- 第一行\n  第二行 [[A]]\n",
        &["[[A]]"],
    ),
    (
        "C09 list item, second paragraph",
        "- 第一段\n\n  第二段 [[A]]\n",
        &["[[A]]"],
    ),
    ("C10 blockquote, first line", "> 引用 [[A]]\n", &["[[A]]"]),
    (
        "C11 blockquote, second line",
        "> 第一行\n> 第二行 [[B]]\n",
        &["[[B]]"],
    ),
    (
        "C12 nested blockquote",
        "> 外层\n> > 内层 [[A]]\n",
        &["[[A]]"],
    ),
    (
        "C13 blockquote inside list",
        "- 项目\n  > 引用 [[A]]\n",
        &["[[A]]"],
    ),
    (
        "C14 list inside blockquote",
        "> - 项目 [[A]]\n> - 第二项 [[B]]\n",
        &["[[A]]", "[[B]]"],
    ),
    ("C15 alert body", "> [!NOTE]\n> 见 [[A]]\n", &["[[A]]"]),
    (
        "C16 lazy blockquote continuation",
        "> 第一行\n第二行 [[A]]\n",
        &["[[A]]"],
    ),
    (
        "C17 paragraph continuation, leading spaces",
        "第一行\n   第二行 [[A]]\n",
        &["[[A]]"],
    ),
    (
        "C18 tab-indented nested list",
        "- 项目\n\t- 嵌套 [[A]]\n",
        &["[[A]]"],
    ),
    (
        "C19 CRLF nested list",
        "- 项目\r\n  - 嵌套 [[A]]\r\n",
        &["[[A]]"],
    ),
    (
        "C20 emoji/CJK before link, nested",
        "- 项目\n  - 🍎 苹果 [[水果]]\n",
        &["[[水果]]"],
    ),
    (
        "C21 section + alias, nested",
        "- 项目\n  - 见 [[标题#小节|显示文字]]\n",
        &["[[标题#小节|显示文字]]"],
    ),
    (
        "C22 image, nested",
        "- 发票\n  - [](assets/9f2c3a1b7e4d5c60-invoice.png)\n",
        &["[](assets/9f2c3a1b7e4d5c60-invoice.png)"],
    ),
    (
        "C23 relative link, nested",
        "- 附件\n  - [发票](assets/9f2c3a1b7e4d5c60-invoice.pdf)\n",
        &["[发票](assets/9f2c3a1b7e4d5c60-invoice.pdf)"],
    ),
    (
        "C24 marked-up wikilinks, nested",
        "- 项目\n  - **[[A]]** 和 ==[[B]]==\n",
        &["[[A]]", "[[B]]"],
    ),
    (
        "C25 hashtag text, nested",
        "- 项目\n  - 讨论 #pkm\n",
        &["讨论 #pkm"],
    ),
    (
        "C26 hashtag text, blockquote line 2",
        "> 第一行\n> 讨论 #pkm\n",
        &["讨论 #pkm"],
    ),
    (
        "C27 table cells",
        "| a | b |\n| --- | --- |\n| [[A]] | [[B]] |\n",
        &["[[A]]", "[[B]]"],
    ),
    (
        "C28 footnote definition continuation",
        "正文[^1]\n\n[^1]: 见 [[A]]\n    续 [[B]]\n",
        &["[[A]]", "[[B]]"],
    ),
    (
        "C29 details container",
        "<details>\n<summary>更多</summary>\n\n- 项目\n  - [[A]]\n\n</details>\n",
        &["[[A]]"],
    ),
    (
        "C30 container directive",
        ":::note\n- 项目\n  - [[A]]\n:::\n",
        &["[[A]]"],
    ),
    (
        "C31 frontmatter + nested list",
        "---\nid: 01J\n---\n# T\n\n- 项目\n  - 嵌套 [[A]]\n",
        &["[[A]]"],
    ),
];

fn collect_addressed_inlines(inlines: &[Inline], out: &mut Vec<Inline>) {
    for inline in inlines {
        match inline {
            Inline::WikiLink(_) | Inline::Link(_) | Inline::Image(_) => out.push(inline.clone()),
            Inline::Text(text) if text.value.contains('#') => out.push(inline.clone()),
            _ => {}
        }
        collect_addressed_inlines(inline.children(), out);
    }
}

fn collect_addressed_blocks(blocks: &[Block], out: &mut Vec<Inline>) {
    for block in blocks {
        match block {
            Block::Paragraph(node) => collect_addressed_inlines(&node.children, out),
            Block::Heading(node) => collect_addressed_inlines(&node.children, out),
            Block::List(node) => node
                .children
                .iter()
                .for_each(|item| collect_addressed_blocks(&item.children, out)),
            Block::BlockQuote(node) => collect_addressed_blocks(&node.children, out),
            Block::Alert(node) => collect_addressed_blocks(&node.children, out),
            Block::FootnoteDefinition(node) => collect_addressed_blocks(&node.children, out),
            Block::ContainerDirective(node) => collect_addressed_blocks(&node.children, out),
            Block::Table(node) => node
                .rows
                .iter()
                .flat_map(|row| &row.cells)
                .for_each(|cell| collect_addressed_inlines(&cell.children, out)),
            Block::HtmlContainer(node) => {
                if let HtmlContainerContent::Blocks(blocks) = &node.content {
                    collect_addressed_blocks(blocks, out)
                }
            }
            _ => {}
        }
    }
}

#[test]
fn inline_spans_address_source_inside_containers() {
    let mut failures = Vec::new();
    for (name, src, expected) in CONTAINER_CASES {
        let mut found = Vec::new();
        collect_addressed_blocks(&parse(src).document.children, &mut found);
        let actual: Vec<_> = found
            .iter()
            .map(|node| {
                node.span()
                    .map(|span| (span.start, span.end, src.get(span.start..span.end)))
            })
            .collect();
        let want: Vec<_> = expected
            .iter()
            .map(|literal| {
                src.find(literal)
                    .map(|start| (start, start + literal.len(), Some(*literal)))
            })
            .collect();
        if actual != want {
            failures.push(format!(
                "{name}\n    expected {want:?}\n    actual   {actual:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n{}",
        failures.len(),
        CONTAINER_CASES.len(),
        failures.join("\n")
    );
}

/// Any parsed node, for walking the whole tree generically.
#[derive(Clone, Copy)]
enum Node<'a> {
    Block(&'a Block),
    Inline(&'a Inline),
    ListItem(&'a ListItem),
    TableRow(&'a TableRow),
    TableCell(&'a TableCell),
}

impl<'a> Node<'a> {
    fn span(self) -> Option<Span> {
        match self {
            Node::Block(node) => node.span(),
            Node::Inline(node) => node.span(),
            Node::ListItem(node) => node.meta.span,
            Node::TableRow(node) => node.meta.span,
            Node::TableCell(node) => node.meta.span,
        }
    }

    fn children(self) -> Vec<Node<'a>> {
        let blocks = |blocks: &'a [Block]| blocks.iter().map(Node::Block).collect::<Vec<_>>();
        let inlines = |inlines: &'a [Inline]| inlines.iter().map(Node::Inline).collect::<Vec<_>>();
        match self {
            Node::Block(block) => match block {
                Block::Paragraph(node) => inlines(&node.children),
                Block::Heading(node) => inlines(&node.children),
                Block::BlockQuote(node) => blocks(&node.children),
                Block::Alert(node) => blocks(&node.children),
                Block::List(node) => node.children.iter().map(Node::ListItem).collect(),
                Block::HtmlContainer(node) => match &node.content {
                    HtmlContainerContent::Blocks(children) => blocks(children),
                    HtmlContainerContent::Inlines(children) => inlines(children),
                },
                Block::FootnoteDefinition(node) => blocks(&node.children),
                Block::Table(node) => node.rows.iter().map(Node::TableRow).collect(),
                Block::LeafDirective(node) => inlines(&node.label),
                Block::ContainerDirective(node) => {
                    let mut children = inlines(&node.label);
                    children.extend(blocks(&node.children));
                    children
                }
                _ => Vec::new(),
            },
            Node::Inline(inline) => inlines(inline.children()),
            Node::ListItem(item) => blocks(&item.children),
            Node::TableRow(row) => row.cells.iter().map(Node::TableCell).collect(),
            Node::TableCell(cell) => inlines(&cell.children),
        }
    }
}

/// Every node's span lies on character boundaries within `source` and within
/// its parent's span, and siblings are in source order without overlapping.
fn assert_spans_nest(label: &str, source: &str, parent: Span, children: Vec<Node<'_>>) {
    let mut cursor = parent.start;
    for child in children {
        let Some(span) = child.span() else {
            panic!("{label}: {source:?}: a node inside {parent:?} has no span");
        };
        assert!(
            span.start <= span.end
                && span.end <= source.len()
                && source.is_char_boundary(span.start)
                && source.is_char_boundary(span.end),
            "{label}: {source:?}: {span:?} is not a character range of the input"
        );
        assert!(
            cursor <= span.start && span.end <= parent.end,
            "{label}: {source:?}: {span:?} leaves {parent:?} or overlaps its previous sibling (which ends at {cursor})"
        );
        assert_spans_nest(label, source, span, child.children());
        cursor = span.end;
    }
}

fn assert_document_spans_nest(label: &str, source: &str) {
    let document = parse(source).document;
    let whole = Span {
        start: 0,
        end: source.len(),
    };
    assert_spans_nest(
        label,
        source,
        whole,
        document.children.iter().map(Node::Block).collect(),
    );
}

fn corpus_inputs() -> Vec<(String, String)> {
    let mut files = Vec::new();
    let mut directories = vec![PathBuf::from("tests/fixtures")];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).expect("fixture directory reads") {
            let path = entry.expect("fixture entry reads").path();
            if path.is_dir() {
                directories.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut inputs = Vec::new();
    for path in files {
        let name = path.display().to_string();
        if !name.ends_with(".cases") && !name.ends_with(".md") {
            continue;
        }
        let text = std::fs::read_to_string(Path::new(&path))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        if name.ends_with(".cases") {
            let mut rest = text.as_str();
            let mut index = 0;
            while let Some(start) = rest.find("\n--- input\n") {
                let after = &rest[start + "\n--- input\n".len()..];
                let end = after.find("\n--- expected\n").unwrap_or(after.len());
                index += 1;
                inputs.push((format!("{name} case {index}"), after[..end].to_string()));
                rest = &after[end..];
            }
        } else if name.ends_with(".md") && !name.ends_with(".canonical.md") {
            inputs.push((name, text));
        }
    }
    inputs
}

/// Pieces that exercise every way a line's text is derived from its source:
/// container markers, indentation, tabs, CRLF, lazy lines, table cells and
/// escaped pipes, alongside inline delimiters and multi-byte text.
const PIECES: &[&str] = &[
    "> ",
    ">",
    "- ",
    "1. ",
    "* ",
    "  ",
    "    ",
    "\t",
    "\n",
    "\r\n",
    "\n\n",
    "| ",
    " | ",
    "|",
    "\\|",
    "\n|-|-|\n",
    "[^1]: ",
    ":::note\n",
    "\n:::\n",
    "<details>\n\n",
    "\n</details>\n",
    "```\n",
    "*",
    "**",
    "_",
    "==",
    "~",
    "[",
    "](u)",
    "[[A]]",
    "`",
    "\\",
    "a",
    "b c",
    "项目",
    "🍎",
    "#",
    "- [ ] ",
    "> [!NOTE]\n",
];

/// The recorded seed of the generated inputs.
const GENERATED_SEED: u64 = 0x2545_f491_4f6c_dd1d;

fn generated_inputs(count: usize) -> Vec<String> {
    let mut state = GENERATED_SEED;
    let mut next = move |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % bound as u64) as usize
    };
    (0..count)
        .map(|_| {
            let mut input = String::new();
            for _ in 0..1 + next(24) {
                input.push_str(PIECES[next(PIECES.len())]);
            }
            input
        })
        .collect()
}

#[test]
fn spans_nest_in_the_fixture_corpus() {
    for (name, source) in corpus_inputs() {
        assert_document_spans_nest(&name, &source);
    }
    let mut cases = 0;
    for path in derived_case_files() {
        for case in fixtures::read_derived_cases(&path) {
            let label = format!("{} case {}", path.display(), case.index);
            assert_document_spans_nest(&label, &case.input);
            cases += 1;
        }
    }
    assert!(cases > 0, "no derived cases read");
}

/// The `.cases` files of the fixture corpus that hold round-trip cases,
/// `--- case N bytes B` headers each, rather than the AST->HTML
/// conformance suites.
fn derived_case_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut directories = vec![PathBuf::from("tests/fixtures")];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).expect("fixture directory reads") {
            let path = entry.expect("fixture entry reads").path();
            if path.is_dir() {
                directories.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "cases")
            {
                let text = std::fs::read(&path).expect("case file reads");
                let conformance =
                    text.starts_with(b"# markdown-syntax AST->HTML conformance suite");
                if !conformance && text.windows(9).any(|window| window == b"--- case ") {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

#[test]
fn spans_nest_in_generated_inputs() {
    for (index, source) in generated_inputs(20_000).iter().enumerate() {
        let label = format!("generated input {index}");
        assert_document_spans_nest(&label, source);
    }
}

impl Node<'_> {
    fn kind(self) -> String {
        let debug = match self {
            Node::Block(node) => format!("{node:?}"),
            Node::Inline(node) => format!("{node:?}"),
            Node::ListItem(_) => return "ListItem".into(),
            Node::TableRow(_) => return "TableRow".into(),
            Node::TableCell(_) => return "TableCell".into(),
        };
        debug.split('(').next().unwrap_or_default().to_string()
    }
}

/// Every node of `source`'s parse in preorder, as (kind, start, end).
fn preorder(source: &str) -> Vec<(String, usize, usize)> {
    let document = parse(source).document;
    let mut out = Vec::new();
    let mut stack: Vec<Node<'_>> = document.children.iter().rev().map(Node::Block).collect();
    while let Some(node) = stack.pop() {
        let span = node.span().expect("parsed node has a span");
        out.push((node.kind(), span.start, span.end));
        stack.extend(node.children().into_iter().rev());
    }
    out
}

/// The span of the `nth` node of `kind` in `source`'s parse, in preorder.
fn nth_span(source: &str, kind: &str, nth: usize) -> (usize, usize) {
    preorder(source)
        .into_iter()
        .filter(|(found, _, _)| found == kind)
        .nth(nth)
        .map(|(_, start, end)| (start, end))
        .unwrap_or_else(|| panic!("{source:?}: no {kind} #{nth}"))
}

#[test]
fn spans_map_stripped_lines_back_to_the_source() {
    // Leading whitespace on a paragraph line.
    assert_eq!(nth_span("  a *b*", "Text", 0), (2, 4));
    assert_eq!(nth_span("  a *b*", "Emphasis", 0), (4, 7));
    // Block quote continuation line.
    let quote = "> a\n> b *c*";
    assert_eq!(nth_span(quote, "Paragraph", 0), (2, 11));
    assert_eq!(nth_span(quote, "Text", 1), (6, 8));
    assert_eq!(nth_span(quote, "Emphasis", 0), (8, 11));
    // List item continuation line.
    let item = "- a\n  b *c*";
    assert_eq!(nth_span(item, "Paragraph", 0), (2, 11));
    assert_eq!(nth_span(item, "Emphasis", 0), (8, 11));
    // A later block inside a block quote.
    assert_eq!(nth_span("> a\n>\n> b", "Paragraph", 1), (8, 9));
    // Nested containers and container kinds.
    assert_eq!(
        nth_span("- 项目\n  - 嵌套 [[library/工作/买菜]]\n", "WikiLink", 0),
        (20, 45)
    );
    assert_eq!(
        nth_span("> 外层\n> > 内层 [[A]]\n", "WikiLink", 0),
        (20, 25)
    );
    assert_eq!(nth_span("> [!NOTE]\n> 见 [[A]]\n", "WikiLink", 0), (16, 21));
    assert_eq!(
        nth_span("正文[^1]\n\n[^1]: 见 [[A]]\n    续 [[B]]\n", "WikiLink", 1),
        (36, 41)
    );
    assert_eq!(
        nth_span(
            "<details>\n<summary>更多</summary>\n\n- 项目\n  - [[A]]\n\n</details>\n",
            "WikiLink",
            0
        ),
        (50, 55)
    );
    assert_eq!(
        nth_span(":::note\n- 项目\n  - [[A]]\n:::\n", "WikiLink", 0),
        (21, 26)
    );
    assert_eq!(
        nth_span("- 项目\n\t- 嵌套 [[A]]\n", "WikiLink", 0),
        (19, 24)
    );
    assert_eq!(
        nth_span("- 项目\r\n  - 嵌套 [[A]]\r\n", "WikiLink", 0),
        (21, 26)
    );
    // CRLF soft break.
    assert_eq!(nth_span("a\r\nb", "SoftBreak", 0), (1, 3));
    assert_eq!(nth_span("a\r\nb", "Text", 1), (3, 4));
    // Table cell content and an escaped pipe.
    let cell = "| a *b* |\n|-|";
    assert_eq!(nth_span(cell, "TableCell", 0), (2, 7));
    assert_eq!(nth_span(cell, "Text", 0), (2, 4));
    assert_eq!(nth_span(cell, "Emphasis", 0), (4, 7));
    let escaped_pipe = "| a\\|b |\n|-|";
    assert_eq!(nth_span(escaped_pipe, "TableCell", 0), (2, 6));
    assert_eq!(nth_span(escaped_pipe, "Text", 0), (2, 3));
    assert_eq!(nth_span(escaped_pipe, "Escape", 0), (3, 5));
    assert_eq!(nth_span(escaped_pipe, "Text", 1), (5, 6));
    // A split tab.
    assert_eq!(nth_span(">\t\tfoo", "CodeBlock", 0), (1, 6));
}

#[test]
fn table_cells_carry_spans() {
    assert_eq!(nth_span("| a | |\n|-|-|", "TableCell", 1), (6, 6));
    assert_eq!(nth_span("| a | b |\n|-|-|\n| c", "TableCell", 3), (19, 19));
}

#[test]
fn an_escaped_cell_pipe_spans_its_backslash() {
    let spans = |source: &str| -> Vec<(usize, usize)> {
        let document = parse(source).document;
        let Some(Block::Table(table)) = document.children.first() else {
            panic!("{source:?}");
        };
        table.rows[0].cells[0]
            .children
            .iter()
            .map(|inline| {
                let span = inline.meta().span.expect("parsed span");
                (span.start, span.end)
            })
            .collect()
    };
    assert_eq!(spans("| \\|a |\n|-|"), [(2, 4), (4, 5)]);
    assert_eq!(spans("| `a`\\|`b` |\n|-|"), [(2, 5), (5, 7), (7, 10)]);
}

#[test]
fn emphasis_on_a_block_quote_continuation_line_covers_its_delimiters() {
    assert_eq!(nth_span("> a\n> *b*", "Emphasis", 0), (6, 9));
}

#[test]
fn a_task_checkbox_is_part_of_the_items_marker() {
    assert_eq!(nth_span("- [ ] task", "Paragraph", 0), (6, 10));
    assert_eq!(nth_span("- [ ] task", "Text", 0), (6, 10));
    let markup = "- [x] done *x*";
    assert_eq!(nth_span(markup, "Paragraph", 0), (6, 14));
    assert_eq!(nth_span(markup, "Text", 0), (6, 11));
    assert_eq!(nth_span(markup, "Emphasis", 0), (11, 14));
    assert_eq!(nth_span("1. [ ] step", "Paragraph", 0), (7, 11));
    assert_eq!(nth_span("1. [ ] step", "Text", 0), (7, 11));
}

#[test]
fn a_container_directives_last_child_ends_after_its_line_ending() {
    assert_eq!(
        nth_span(":::note\n```\nx\n```\n:::\n", "CodeBlock", 0),
        (8, 18)
    );
}
