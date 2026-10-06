//! Inputs shaped to make a parser slow or exhaust its stack: deep nesting,
//! long runs of unclosed openers in one paragraph, and long runs of container
//! markers on one line. Each must parse within a generous time limit on a small
//! thread stack, and nesting past the documented limits must stay literal
//! text.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use markdown_syntax::{parse, Block, Document, Inline, SyntaxOptions};

/// Generous for an unoptimized build; every input below parses in
/// milliseconds when parsing is linear.
const TIME_LIMIT: Duration = Duration::from_secs(10);

/// Held by each test for its whole run: the tests here time their work, and
/// a test run beside another of them on the same cores times both.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Thread stack for each parse: the Rust default for spawned threads. Enough
/// for the nesting limits even in an unoptimized build, far too small for
/// recursion that follows input depth.
const STACK_BYTES: usize = 2 << 20;

/// Parses `input` on a small-stack thread, serializes and validates the
/// result there, and returns the document.
fn parse_bounded(name: &str, input: String) -> Document {
    parse_bounded_with(name, input, SyntaxOptions::default())
}

/// `parse_bounded` under `options`.
fn parse_bounded_with(name: &str, input: String, options: SyntaxOptions) -> Document {
    let thread_name = name.to_owned();
    let document = std::thread::Builder::new()
        .name(thread_name)
        .stack_size(STACK_BYTES)
        .spawn(move || {
            let started = Instant::now();
            let document = options.parse(&input).document;
            let _ = document.to_markdown();
            let _ = document.validate();
            (document, started.elapsed())
        })
        .expect("spawn parse thread")
        .join();
    let (document, elapsed) = document.unwrap_or_else(|_| panic!("{name}: parse panicked"));
    assert!(
        elapsed < TIME_LIMIT,
        "{name}: took {elapsed:?}, limit {TIME_LIMIT:?}"
    );
    document
}

fn first_paragraph(document: &Document) -> &[Inline] {
    match document.children.first() {
        Some(Block::Paragraph(paragraph)) => &paragraph.children,
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

/// The deepest chain of nested inline containers, counting each container.
fn inline_depth(nodes: &[Inline]) -> usize {
    let mut deepest = 0;
    let mut pending = vec![(nodes, 0)];
    while let Some((nodes, depth)) = pending.pop() {
        for node in nodes {
            // Every container counts, empty ones included.
            let container = !matches!(
                node,
                Inline::Text(_)
                    | Inline::Escape(_)
                    | Inline::CharacterReference(_)
                    | Inline::Shortcode(_)
                    | Inline::Code(_)
                    | Inline::Html(_)
                    | Inline::SoftBreak(_)
                    | Inline::LineBreak(_)
                    | Inline::Math(_)
                    | Inline::FootnoteReference(_)
                    | Inline::WikiLink(_)
                    | Inline::MdxExpression(_)
                    | Inline::MdxJsx(_)
            );
            if container {
                deepest = deepest.max(depth + 1);
                pending.push((node.children(), depth + 1));
            }
        }
    }
    deepest
}

fn emphasis_depth(nodes: &[Inline]) -> usize {
    let mut deepest = 0;
    let mut pending = vec![(nodes, 0)];
    while let Some((nodes, depth)) = pending.pop() {
        for node in nodes {
            if let Inline::Emphasis(_) | Inline::Strong(_) = node {
                deepest = deepest.max(depth + 1);
                pending.push((node.children(), depth + 1));
            }
        }
    }
    deepest
}

fn block_quote_depth(document: &Document) -> usize {
    let mut depth = 0;
    let mut blocks = &document.children;
    while let Some(Block::BlockQuote(quote)) = blocks.first() {
        depth += 1;
        blocks = &quote.children;
    }
    depth
}

fn contains_link(nodes: &[Inline]) -> bool {
    let mut pending = vec![nodes];
    while let Some(nodes) = pending.pop() {
        for node in nodes {
            if let Inline::Link(_) = node {
                return true;
            }
            pending.push(node.children());
        }
    }
    false
}

#[test]
fn nested_link_labels_parse_in_bounded_time() {
    let _serial = one_at_a_time();
    for depth in [30, 1_000] {
        let input = "[".repeat(depth) + "x" + &"](u)".repeat(depth);
        let document = parse_bounded("nested link labels", input);
        assert!(
            contains_link(first_paragraph(&document)),
            "depth {depth}: the innermost label still forms a link"
        );
    }
}

#[test]
fn runs_of_unclosed_openers_parse_in_bounded_time() {
    let _serial = one_at_a_time();
    let openers = [
        "[",
        "[[",
        "![",
        "^[",
        "[^",
        ":a[",
        ":a{",
        "[a][",
        "[a](<",
        "<",
        "<a ",
        "<a b=\"",
        "</",
        "<!--",
        "<?",
        "<![CDATA[",
        "<!X",
        "`",
        "$`",
        "==a ",
        "++a ",
        "__a ",
        "~a ",
        "*_",
        "a@",
        "a.",
        "www.a ",
        "http://a ",
        "&a ",
        "&#",
        "[\\",
        "[`",
        "[<",
        "{",
    ];
    for opener in openers {
        let input = opener.repeat(60_000 / opener.len());
        parse_bounded(opener, input);
        // The same openers sharing one closer at the very end.
        let input = opener.repeat(60_000 / opener.len()) + "]]>}`$)";
        parse_bounded(opener, input);
    }
}

#[test]
fn realistic_large_paragraphs_parse_in_bounded_time() {
    let _serial = one_at_a_time();
    let log: String = (0..10_000)
        .map(|line| {
            format!(
                "[INFO] [worker-{}] id=[{line}] took *{line}ms* path=/a_b [retry]\n",
                line % 8
            )
        })
        .collect();
    parse_bounded("log lines", log);
    let code: String = (0..20_000)
        .map(|line| format!("if (a_{line} < b) {{ x[{line}] == y_{line} * z; }}\n"))
        .collect();
    parse_bounded("unfenced code", code);
}

#[test]
fn many_definitions_parse_in_bounded_time() {
    let _serial = one_at_a_time();
    let mut input: String = (0..60_000).map(|label| format!("[r{label}] ")).collect();
    input.push_str("\n\n");
    input.extend((0..60_000).map(|label| format!("[r{label}]: /u{label}\n")));
    parse_bounded("definitions", input);
}

#[test]
fn block_nesting_stops_at_the_limit() {
    let _serial = one_at_a_time();
    let document = parse_bounded("nested quotes", ">".repeat(100_000) + " x");
    assert_eq!(block_quote_depth(&document), 32);

    for (name, input) in [
        ("nested list markers", "- ".repeat(100_000) + "x"),
        ("nested ordered markers", "1. ".repeat(100_000) + "x"),
        ("quote then list markers", "> - ".repeat(50_000) + "x"),
        (
            "nested directives",
            ":::a\n".repeat(10_000) + "x\n" + &":::\n".repeat(10_000),
        ),
        (
            "nested details",
            "<details>\n\n".repeat(10_000) + "x\n\n" + &"</details>\n\n".repeat(10_000),
        ),
        ("unclosed directives", ":::note\n".repeat(20_000)),
        ("unclosed details", "<details>\n\n".repeat(20_000)),
    ] {
        parse_bounded(name, input);
    }
}

#[test]
fn long_marker_runs_on_a_quote_continuation_line_are_bounded() {
    let _serial = one_at_a_time();
    for markers in ["> ", "- ", "> - ", "1. ", "-\t"] {
        parse_bounded(markers, format!("> a\n{}x\n", markers.repeat(100_000)));
    }
}

#[test]
fn inline_nesting_stops_at_the_limit() {
    let _serial = one_at_a_time();
    let input = "*a ".repeat(5_000) + &" a*".repeat(5_000);
    let document = parse_bounded("nested emphasis", input);
    assert_eq!(emphasis_depth(first_paragraph(&document)), 16);

    let input = "**a ".repeat(5_000) + &" a**".repeat(5_000);
    let document = parse_bounded("nested strong", input);
    assert_eq!(emphasis_depth(first_paragraph(&document)), 16);

    for (name, open, close) in [
        ("nested images", "![", "](u)"),
        ("nested inline footnotes", "^[", "]"),
        ("nested text directives", ":a[", "]"),
        ("nested highlights", "==a ", " a=="),
        ("nested spoilers", "||a ", " a||"),
    ] {
        let input = open.repeat(5_000) + "x" + &close.repeat(5_000);
        let document = parse_bounded(name, input);
        assert!(
            inline_depth(first_paragraph(&document)) <= 32,
            "{name}: inline nesting stops at the limit"
        );
    }
}

#[test]
fn mdx_jsx_and_expressions_parse_in_bounded_time() {
    let _serial = one_at_a_time();
    let distinct = |close: bool| -> String {
        let mut input: String = (0..20_000).map(|tag| format!("<A{tag}>\n")).collect();
        if close {
            input.extend((0..20_000).rev().map(|tag| format!("</A{tag}>\n")));
        }
        input
    };
    for (name, input) in [
        ("distinct unclosed tags", distinct(false)),
        ("distinct tags closed in reverse", distinct(true)),
        ("same-name unclosed tags", "<A>".repeat(20_000)),
        (
            "same-name nested tags",
            "<A>".repeat(20_000) + &"</A>".repeat(20_000),
        ),
        (
            "unclosed attribute strings",
            "<A b=\"".repeat(20_000) + "\">",
        ),
        (
            "unclosed attribute expressions",
            "<A b={".repeat(20_000) + "}>",
        ),
        (
            "escaped quotes across lines",
            "<A b=\"\\\n".repeat(20_000) + "\">",
        ),
        ("unclosed expression blocks", "{\n".repeat(20_000) + "}"),
        (
            "expressions with strings and comments",
            "{'}' \"{\" `}` /*}*/\n".repeat(10_000),
        ),
        ("declarations without raw HTML", "<!X".repeat(30_000) + ">"),
    ] {
        parse_bounded_with(name, input, SyntaxOptions::mdx());
    }
}

/// `open`/`close` around `x`, nested `levels` deep through `mid_open` /
/// `mid_close`.
fn nest(levels: usize, open: &str, mid_open: &str, mid_close: &str, close: &str) -> String {
    let mut input = String::from("x");
    for _ in 0..levels {
        input = format!(
            "{}{mid_open}{input}{mid_close}{}",
            open.repeat(30),
            close.repeat(30)
        );
    }
    input
}

#[test]
fn nesting_through_directive_labels_stops_at_the_limit() {
    let _serial = one_at_a_time();
    for (name, open, close) in [
        ("images around directives", "![", "](u)"),
        ("footnotes around directives", "^[a ", "]"),
        ("marks around directives", "==a ", " b=="),
    ] {
        let document = parse_bounded(name, nest(31, open, ":d[", "]", close));
        assert!(
            inline_depth(first_paragraph(&document)) <= 32,
            "{name}: inline nesting stops at the limit"
        );
    }
    for leaf in ["![](u)", "[](u)", ":e[]"] {
        let input =
            String::from("*a :d[") + &"==a ".repeat(40) + leaf + &" b==".repeat(40) + "] b*";
        let document = parse_bounded("empty container under directive marks", input);
        assert!(
            inline_depth(first_paragraph(&document)) <= 32,
            "{leaf}: inline nesting stops at the limit"
        );
    }
    parse_bounded(
        "emphasis and marks around directives",
        nest(31, "*a ==b ", ":d[", "]", " c== d*"),
    );
}

#[test]
fn emphasis_inside_marks_shares_the_inline_limit() {
    let _serial = one_at_a_time();
    let open = "*a ".repeat(16) + "==b ";
    let close = String::from(" c==") + &" d*".repeat(16);
    let input = open.repeat(32) + "x" + &close.repeat(32);
    let document = parse_bounded("emphasis inside marks", input);
    assert!(inline_depth(first_paragraph(&document)) <= 32);
}

#[test]
fn tilde_closers_with_subscripts_parse_in_bounded_time() {
    let _serial = one_at_a_time();
    let input = "*a ".repeat(50_000) + &" ~b~~ ".repeat(50_000);
    parse_bounded("tilde closers", input);
    let input = String::from("~~x ") + &"*a ".repeat(50_000) + &" ~b~~ ".repeat(50_000);
    parse_bounded("tilde closers after a strikethrough opener", input);
}

#[test]
fn long_nested_containers_and_tables_parse_in_bounded_time() {
    let _serial = one_at_a_time();
    // Every line of a deep container is derived through each level's source
    // map; lookups must not grow with the lines before them.
    let quote_prefix = "> ".repeat(30);
    let quoted: String = (0..3_000)
        .map(|line| format!("{quote_prefix}line {line} *a* [b](u)\r\n"))
        .collect();
    parse_bounded("long nested block quote", quoted);

    let mut listed = String::new();
    for level in 0..15 {
        listed.push_str(&"  ".repeat(level));
        listed.push_str("- item\n");
    }
    let continuation = "  ".repeat(15);
    for line in 0..5_000 {
        listed.push_str(&format!("{continuation}\tline {line} *a*\n"));
    }
    parse_bounded("long nested list item", listed);

    let mut table = String::from("| a | b | c |\n|---|---|---|\n");
    for row in 0..5_000 {
        table.push_str(&format!("| {row} \\| x | *b* | `c` |\n"));
    }
    parse_bounded("long table", table);

    let escaped_pipes = String::from("| a |\n|---|\n| ") + &"x\\|".repeat(50_000) + " |\n";
    parse_bounded("cell of escaped pipes", escaped_pipes);
}

/// Asserts that the work `run` times grows about linearly: four times the
/// input may take at most eight times as long, where a quadratic cost takes
/// sixteen. The best of three runs counts, and a small slack absorbs noise.
fn assert_linear_growth(name: &str, n: usize, run: impl Fn(usize) -> Duration) {
    let best = |n| (0..3).map(|_| run(n)).min().expect("three runs");
    let (small, large) = (best(n), best(4 * n));
    assert!(
        large <= small * 8 + Duration::from_millis(20),
        "{name}: {small:?} at {n}, {large:?} at {}",
        4 * n
    );
}

fn time_parse(input: &str, options: SyntaxOptions) -> Duration {
    let started = Instant::now();
    let _ = options.parse(input);
    started.elapsed()
}

fn time_serialize(document: &Document) -> Duration {
    let started = Instant::now();
    let _ = document.to_markdown();
    started.elapsed()
}

#[test]
fn parsing_grows_linearly_with_lines_and_diagnostics() {
    let _serial = one_at_a_time();
    // Each malformed opener's diagnostic runs to the end of the paragraph;
    // translating it must not walk the lines after it.
    for line in [" x :a{\n", "> x :a{ \n"] {
        assert_linear_growth(line, 8_000, |n| {
            time_parse(&line.repeat(n), SyntaxOptions::default())
        });
    }
    let quote_prefix = "> ".repeat(30);
    assert_linear_growth("long nested block quote", 500, |n| {
        let quoted: String = (0..n)
            .map(|line| format!("{quote_prefix}line {line} *a* [b](u)\r\n"))
            .collect();
        time_parse(&quoted, SyntaxOptions::default())
    });
}

#[test]
fn block_containers_grow_linearly_with_their_lines() {
    let _serial = one_at_a_time();
    // A description's details continue with lazy lines.
    assert_linear_growth("long description details", 2_000, |n| {
        time_parse(
            &(String::from("a\n: b\n") + &"c\n".repeat(n)),
            SyntaxOptions::default(),
        )
    });
    // Each line passes the same nested containers.
    let prefix = "> - ".repeat(8);
    assert_linear_growth("long nested containers", 500, |n| {
        let mut input = format!("{prefix}x\n");
        let continuation = format!("> {}", "  ".repeat(8));
        for line in 0..n {
            input.push_str(&format!("{continuation}line {line} *a*\n"));
        }
        let started = Instant::now();
        let document = parse(&input).document;
        assert!(document.to_markdown().is_ok());
        started.elapsed()
    });
}

#[test]
fn open_definitions_and_terms_grow_linearly() {
    let _serial = one_at_a_time();
    // A title left open runs to the paragraph's end; each line that could
    // close it must not reparse the title so far.
    for name in ["open title", "open title before pipes"] {
        assert_linear_growth(name, 2_000, |n| {
            let line = if name == "open title" { "\nx" } else { "\n|x" };
            time_parse(
                &(String::from("[a]: /u \"") + &line.repeat(n)),
                SyntaxOptions::gfm(),
            )
        });
    }
    // An unclosed label before lines a table delimiter row could follow.
    assert_linear_growth("open label before pipes", 2_000, |n| {
        time_parse(
            &(format!("[{}", "a".repeat(85)) + &"\n|x".repeat(n)),
            SyntaxOptions::gfm(),
        )
    });
    // Description markers after a long paragraph that is no term.
    assert_linear_growth("markers after a long paragraph", 2_000, |n| {
        time_parse(
            &("a\n".repeat(n) + "    b\n" + &"~ x\n".repeat(n)),
            SyntaxOptions::default(),
        )
    });
    // Quoted MDX expression openers before list items.
    assert_linear_growth("quoted expression openers", 1_000, |n| {
        time_parse(&"> {\n- a\n".repeat(n), SyntaxOptions::mdx())
    });
}

#[test]
fn serialization_grows_linearly_with_definitions_and_layouts() {
    let _serial = one_at_a_time();
    // Every block reads back with the labels the document knows.
    assert_linear_growth("definitions before paragraphs", 500, |n| {
        let mut input: String = (0..n).map(|i| format!("[a{i}]: /u{i}\n")).collect();
        input.push('\n');
        input.push_str(&"plain words here\n\n".repeat(n));
        time_serialize(&parse(&input).document)
    });
    // Each list needs a layout alternative of its own.
    assert_linear_growth("lists that need a layout", 100, |n| {
        time_serialize(&parse(&"-\n  ---\n\nx\n\n".repeat(n)).document)
    });
    assert_linear_growth("items that need a layout", 100, |n| {
        time_serialize(&parse(&"-\n  ---\n".repeat(n)).document)
    });
    // Groups of abutting runs, each needing a delimiter switch.
    assert_linear_growth("abutting run groups", 250, |n| {
        time_serialize(&parse(&"***b_*_b_* ".repeat(n)).document)
    });
}

#[test]
fn serialization_grows_linearly_with_runs() {
    let _serial = one_at_a_time();
    assert_linear_growth("tilde run", 20_000, |n| {
        time_serialize(&parse(&format!("a {} b", "~".repeat(n))).document)
    });
    assert_linear_growth("nested emphasis paragraphs", 20, |n| {
        let paragraph = format!("{}bc{}\n\n", "*a ".repeat(16), "c*".repeat(16));
        time_serialize(&parse(&paragraph.repeat(n)).document)
    });
}

#[test]
fn deeply_nested_emphasis_serializes_in_time_linear_in_its_depth() {
    let _serial = one_at_a_time();
    // Sixteen levels of nested emphasis, a mark around further nested
    // emphasis, and a tail that abuts the runs.
    let paragraph = |depth: usize| {
        format!(
            "{}==b {}x{} c=={}~~",
            "*a ".repeat(depth),
            "*a ".repeat(6),
            " a*".repeat(6),
            " d*".repeat(depth)
        )
    };
    let document = parse(&paragraph(16)).document;
    let started = Instant::now();
    assert!(document.to_markdown().is_ok());
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );

    // Forty such paragraphs, so that the time measured is well past the
    // slack the bound allows.
    let best = |depth| {
        let document = parse(&format!("{}\n\n", paragraph(depth)).repeat(40)).document;
        assert!(document.to_markdown().is_ok());
        (0..3)
            .map(|_| time_serialize(&document))
            .min()
            .expect("three runs")
    };
    let (four, eight, sixteen) = (best(4), best(8), best(16));
    assert!(
        eight <= four * 3 + Duration::from_millis(5)
            && sixteen <= eight * 3 + Duration::from_millis(5),
        "{four:?} at 4, {eight:?} at 8, {sixteen:?} at 16"
    );
}

#[test]
fn nested_quote_depth_grows_time_linearly() {
    let _serial = one_at_a_time();
    // The same lines inside 4, 8, and 16 nested block quotes: each doubling
    // doubles the input, and must at most roughly double the time to parse
    // and serialize it.
    let quoted = |depth: usize| -> String {
        (0..2_000)
            .map(|line| format!("{}line {line} *a*\n", "> ".repeat(depth)))
            .collect()
    };
    let started = Instant::now();
    let document = parse(&quoted(16)).document;
    assert!(document.to_markdown().is_ok());
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );

    let best = |depth| {
        let input = quoted(depth);
        (0..3)
            .map(|_| {
                let started = Instant::now();
                let document = parse(&input).document;
                assert!(document.to_markdown().is_ok());
                started.elapsed()
            })
            .min()
            .expect("three runs")
    };
    let (four, eight, sixteen) = (best(4), best(8), best(16));
    assert!(
        eight <= four * 3 + Duration::from_millis(5)
            && sixteen <= eight * 3 + Duration::from_millis(5),
        "{four:?} at 4, {eight:?} at 8, {sixteen:?} at 16"
    );
}

#[test]
fn nested_list_depth_grows_time_linearly() {
    let _serial = one_at_a_time();
    // The same lines inside 8, 16, and 32 nested list items: each doubling
    // about doubles the input, and must at most roughly double the time to
    // parse and serialize it.
    let listed = |depth: usize| -> String {
        let mut input: String = (0..depth)
            .map(|level| format!("{}- i\n", "  ".repeat(level)))
            .collect();
        for _ in 0..1_200 {
            input.push_str(&format!("{}x\n", "  ".repeat(depth)));
        }
        input
    };
    let best = |depth| {
        let input = listed(depth);
        (0..5)
            .map(|_| {
                let started = Instant::now();
                let document = parse(&input).document;
                assert!(document.to_markdown().is_ok());
                started.elapsed()
            })
            .min()
            .expect("five runs")
    };
    let (eight, sixteen, thirty_two) = (best(8), best(16), best(32));
    assert!(thirty_two < Duration::from_secs(1), "{thirty_two:?}");
    assert!(
        sixteen.as_secs_f64() <= eight.as_secs_f64() * 2.75 + 0.005
            && thirty_two.as_secs_f64() <= sixteen.as_secs_f64() * 2.75 + 0.005,
        "{eight:?} at 8, {sixteen:?} at 16, {thirty_two:?} at 32"
    );
}

#[test]
fn nested_emphasis_that_does_not_read_back_serializes_in_bounded_time() {
    let _serial = one_at_a_time();
    // A paragraph that reads back only after its delimiter choices and edge
    // encodings are tried, nested sixteen deep, serializes within the
    // scenario's bound rather than the shared limit.
    let input = format!(
        "{}==b {}x{} c=={} ://~&mp;~",
        "*a ".repeat(16),
        "*a ".repeat(6),
        " a*".repeat(6),
        " d*".repeat(16)
    );
    let document = parse_bounded("nested emphasis with every delimiter choice", input);
    let started = Instant::now();
    let _ = document.to_markdown();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}
