//! Inputs shaped to make a parser slow or exhaust its stack: deep nesting,
//! long runs of unclosed openers in one paragraph, and long runs of container
//! markers on one line. Each must parse within a generous time limit on a small
//! thread stack, and nesting past the documented limits must stay literal
//! text.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use markdown_syntax::{parse, Block, Document, Inline};

/// Generous for an unoptimized build; every input below parses in
/// milliseconds when parsing is linear. It catches a hang; growth is checked
/// by `assert_linear_growth` below and by `tests/linear_growth.rs`.
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
/// recursion that follows input depth. Optimized frames are smaller, so the
/// deep inputs below nest tens of thousands of levels: deeper than any
/// realistic frame size fits in this stack.
const STACK_BYTES: usize = 2 << 20;

/// Parses `input` on a small-stack thread, serializes and validates the
/// result there, and returns the document.
fn parse_bounded(name: &str, input: String) -> Document {
    let thread_name = name.to_owned();
    let document = std::thread::Builder::new()
        .name(thread_name)
        .stack_size(STACK_BYTES)
        .spawn(move || {
            let started = Instant::now();
            let document = parse(&input).document;
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
    for depth in [30, 50_000] {
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
    // Directive openers sharing one closer followed by a char that refuses
    // every one of them.
    for (opener, tail) in [(":a{", "}x"), (":a[", "]x")] {
        let input = opener.repeat(60_000 / opener.len()) + tail;
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
    let input = "*a ".repeat(50_000) + &" a*".repeat(50_000);
    let document = parse_bounded("nested emphasis", input);
    assert_eq!(emphasis_depth(first_paragraph(&document)), 16);

    let input = "**a ".repeat(50_000) + &" a**".repeat(50_000);
    let document = parse_bounded("nested strong", input);
    assert_eq!(emphasis_depth(first_paragraph(&document)), 16);

    for (name, open, close) in [
        ("nested images", "![", "](u)"),
        ("nested inline footnotes", "^[", "]"),
        ("nested text directives", ":a[", "]"),
        ("nested highlights", "==a ", " a=="),
        ("nested spoilers", "||a ", " a||"),
    ] {
        let input = open.repeat(50_000) + "x" + &close.repeat(50_000);
        let document = parse_bounded(name, input);
        assert!(
            inline_depth(first_paragraph(&document)) <= 32,
            "{name}: inline nesting stops at the limit"
        );
    }
}

#[test]
fn jsx_and_expression_shaped_inputs_parse_in_bounded_time() {
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
        parse_bounded(name, input);
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
fn tilde_closers_parse_in_bounded_time() {
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

/// Each timed sample repeats its work until it takes at least this long, so
/// that timer and scheduler noise stay small beside it in an optimized build
/// too, where one run of the work may take microseconds.
const MIN_SAMPLE: Duration = Duration::from_millis(5);

/// The best of three samples of `work` on each of `inputs`, taken in turn so
/// that a change in load falls on all of them. Every sample repeats `work`
/// as often as the first input needs to fill `MIN_SAMPLE`, so the samples
/// compare as single runs would.
fn best_samples<T, const N: usize>(inputs: &[T; N], work: impl Fn(&T)) -> [Duration; N] {
    let sample = |input: &T, repeats: u32| {
        let started = Instant::now();
        for _ in 0..repeats {
            work(input);
        }
        started.elapsed()
    };
    let mut repeats = 1;
    while repeats < 1 << 16 && sample(&inputs[0], repeats) < MIN_SAMPLE {
        repeats *= 2;
    }
    let mut best = [Duration::MAX; N];
    for _ in 0..3 {
        for (input, best) in inputs.iter().zip(&mut best) {
            *best = (*best).min(sample(input, repeats));
        }
    }
    best
}

/// `best_samples`, taken again until they satisfy `linear`, at most three
/// times. A burst of load on the machine can spoil one measurement of a
/// linear cost; a superlinear cost fails every one.
fn growth_samples<T, const N: usize>(
    inputs: &[T; N],
    work: impl Fn(&T),
    linear: impl Fn(&[Duration; N]) -> bool,
) -> [Duration; N] {
    let mut samples = best_samples(inputs, &work);
    for _ in 1..3 {
        if linear(&samples) {
            break;
        }
        samples = best_samples(inputs, &work);
    }
    samples
}

/// Asserts that `work` on `setup(n)` grows about linearly with `n`: four
/// times the input may take at most ten times as long, where a quadratic
/// cost takes sixteen. The margin over four covers the slower memory a
/// larger input and its tree reach.
fn assert_linear_growth<T>(name: &str, n: usize, setup: impl Fn(usize) -> T, work: impl Fn(&T)) {
    let linear = |[small, large]: &[Duration; 2]| *large <= *small * 10;
    let samples = growth_samples(&[setup(n), setup(4 * n)], work, linear);
    let [small, large] = samples;
    assert!(
        linear(&samples),
        "{name}: {small:?} at {n}, {large:?} at {} per sample",
        4 * n
    );
}

/// Asserts that `work` on `setup(depth)` at most roughly doubles in time with
/// each doubling of the depth: each step may take at most three times as
/// long as the one before, where a cost quadratic in the depth takes four.
fn assert_depth_growth<T>(
    name: &str,
    depths: [usize; 3],
    setup: impl Fn(usize) -> T,
    work: impl Fn(&T),
) {
    let linear = |[low, mid, high]: &[Duration; 3]| *mid <= *low * 3 && *high <= *mid * 3;
    let samples = growth_samples(&depths.map(setup), work, linear);
    assert!(
        linear(&samples),
        "{name}: {samples:?} per sample at depths {depths:?}"
    );
}

fn parse_input(input: &impl AsRef<str>) {
    let _ = parse(input.as_ref());
}

fn parse_document(input: String) -> Document {
    parse(&input).document
}

fn serialize(document: &Document) {
    let _ = document.to_markdown();
}

fn parse_and_serialize(input: &impl AsRef<str>) {
    assert!(parse(input.as_ref()).document.to_markdown().is_ok());
}

#[test]
fn parsing_grows_linearly_with_lines_and_diagnostics() {
    let _serial = one_at_a_time();
    // Each malformed opener's diagnostic runs to the end of the paragraph;
    // translating it must not walk the lines after it.
    for line in [" x :a{\n", "> x :a{ \n"] {
        assert_linear_growth(line, 8_000, |n| line.repeat(n), parse_input);
    }
    let quote_prefix = "> ".repeat(30);
    assert_linear_growth(
        "long nested block quote",
        500,
        |n| -> String {
            (0..n)
                .map(|line| format!("{quote_prefix}line {line} *a* [b](u)\r\n"))
                .collect()
        },
        parse_input,
    );
}

#[test]
fn literal_autolinks_in_open_labels_grow_linearly() {
    let _serial = one_at_a_time();
    // Each literal autolink in a label left open looks ahead for the label's
    // resource; the look must not run to the paragraph's end.
    for unit in ["[ www.a](", "[ http://a]("] {
        assert_linear_growth(unit, 500, |n| unit.repeat(n), parse_input);
    }
}

#[test]
fn block_containers_grow_linearly_with_their_lines() {
    let _serial = one_at_a_time();
    // A paragraph after a `:` line continues with lazy lines, as it did when
    // the line opened description details, and so does a footnote
    // definition's paragraph.
    assert_linear_growth(
        "long footnote definition",
        2_000,
        |n| String::from("[^1]: b\n") + &"c\n".repeat(n),
        parse_input,
    );
    assert_linear_growth(
        "long former description details",
        2_000,
        |n| String::from("a\n: b\n") + &"c\n".repeat(n),
        parse_input,
    );
    // Each line passes the same nested containers.
    let prefix = "> - ".repeat(8);
    assert_linear_growth(
        "long nested containers",
        500,
        |n| {
            let mut input = format!("{prefix}x\n");
            let continuation = format!("> {}", "  ".repeat(8));
            for line in 0..n {
                input.push_str(&format!("{continuation}line {line} *a*\n"));
            }
            input
        },
        parse_and_serialize,
    );
}

#[test]
fn open_definitions_and_terms_grow_linearly() {
    let _serial = one_at_a_time();
    // A title left open runs to the paragraph's end; each line that could
    // close it must not reparse the title so far.
    for (name, line) in [("open title", "\nx"), ("open title before pipes", "\n|x")] {
        assert_linear_growth(
            name,
            2_000,
            |n| String::from("[a]: /u \"") + &line.repeat(n),
            parse_input,
        );
    }
    // An unclosed label before lines a table delimiter row could follow.
    assert_linear_growth(
        "open label before pipes",
        2_000,
        |n| format!("[{}", "a".repeat(85)) + &"\n|x".repeat(n),
        parse_input,
    );
    // Former description markers after a long paragraph.
    assert_linear_growth(
        "markers after a long paragraph",
        2_000,
        |n| "a\n".repeat(n) + "    b\n" + &"~ x\n".repeat(n),
        parse_input,
    );
    // Quoted former MDX expression openers before list items.
    assert_linear_growth(
        "quoted expression openers",
        1_000,
        |n| "> {\n- a\n".repeat(n),
        parse_input,
    );
}

#[test]
fn serialization_grows_linearly_with_definitions_and_lists() {
    let _serial = one_at_a_time();
    assert_linear_growth(
        "definitions before paragraphs",
        500,
        |n| {
            let mut input: String = (0..n).map(|i| format!("[a{i}]: /u{i}\n")).collect();
            input.push('\n');
            input.push_str(&"plain words here\n\n".repeat(n));
            parse_document(input)
        },
        serialize,
    );
    assert_linear_growth(
        "lists that need a layout",
        100,
        |n| parse_document("-\n  ---\n\nx\n\n".repeat(n)),
        serialize,
    );
    assert_linear_growth(
        "items that need a layout",
        100,
        |n| parse_document("-\n  ---\n".repeat(n)),
        serialize,
    );
    // Groups of abutting runs.
    assert_linear_growth(
        "abutting run groups",
        250,
        |n| parse_document("***b_*_b_* ".repeat(n)),
        serialize,
    );
}

#[test]
fn serialization_grows_linearly_with_runs() {
    let _serial = one_at_a_time();
    assert_linear_growth(
        "tilde run",
        20_000,
        |n| parse_document(format!("a {} b", "~".repeat(n))),
        serialize,
    );
    assert_linear_growth(
        "nested emphasis paragraphs",
        20,
        |n| {
            let paragraph = format!("{}bc{}\n\n", "*a ".repeat(16), "c*".repeat(16));
            parse_document(paragraph.repeat(n))
        },
        serialize,
    );
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

    assert_depth_growth(
        "nested emphasis",
        [4, 8, 16],
        |depth| parse_document(paragraph(depth)),
        serialize,
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

    assert_depth_growth("nested quotes", [4, 8, 16], quoted, parse_and_serialize);
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
    let started = Instant::now();
    parse_and_serialize(&listed(32));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );

    assert_depth_growth("nested lists", [8, 16, 32], listed, parse_and_serialize);
}

#[test]
fn nested_emphasis_with_abutting_tildes_serializes_in_bounded_time() {
    let _serial = one_at_a_time();
    // Emphasis and a mark nested sixteen deep, with tildes and a reference
    // at its end, serializes within the scenario's bound rather than the
    // shared limit.
    let input = format!(
        "{}==b {}x{} c=={} ://~&mp;~",
        "*a ".repeat(16),
        "*a ".repeat(6),
        " a*".repeat(6),
        " d*".repeat(16)
    );
    let document = parse_bounded("nested emphasis with abutting tildes", input);
    let started = Instant::now();
    let _ = document.to_markdown();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}
