//! Checks the memoized lookahead behind `escape_text_with_context` against
//! plain scans that define its answers, at every position of many generated
//! texts, queried both in shuffled and in ascending order.

use alloc::string::String;

use super::*;
use crate::test_support::{boundaries, generated_inputs, query_orders, Rng};

/// Plain scans over the rest of the text that define each answer.
#[allow(clippy::all)]
mod reference {
    use super::super::*;

    pub(super) fn text_attention_delimiter_can_start(
        input: &str,
        offset: usize,
        marker: &str,
        underscore: bool,
    ) -> bool {
        if !input[offset..].starts_with(marker) {
            return false;
        }
        if input[offset + marker.len()..].starts_with(marker)
            || text_char_at_edge(input, offset, marker.len())
        {
            return true;
        }
        if !text_delimiter_can_open(input, offset, marker.len(), underscore) {
            return false;
        }

        let mut cursor = offset + marker.len();
        while let Some(candidate) = input[cursor..].find(marker).map(|index| cursor + index) {
            if !input[candidate + marker.len()..].starts_with(marker)
                && text_delimiter_can_close(input, candidate, marker.len(), underscore)
            {
                return true;
            }
            cursor = candidate + marker.len();
        }
        false
    }

    pub(super) fn text_less_than_can_start_inline(input: &str, offset: usize) -> bool {
        let after = &input[offset + '<'.len_utf8()..];
        if after.contains('>') {
            let next = after.chars().next();
            return next.is_some_and(|char| {
                char.is_ascii_alphabetic() || matches!(char, '/' | '!' | '?' | '_')
            }) || after.starts_with("http://")
                || after.starts_with("https://")
                || after.contains('@');
        }
        false
    }

    pub(super) fn text_spoiler_can_start(input: &str, offset: usize) -> bool {
        input[offset..].starts_with("||")
            && !input[offset + "||".len()..].starts_with('|')
            && input[offset + "||".len()..].contains("||")
    }

    pub(super) fn text_math_can_start(input: &str, offset: usize) -> bool {
        // Mirror the parser's dollar-math start (code-span analogue): an opening run
        // of N dollars starts math when an exact-length-N closing run exists ahead.
        // Edge whitespace no longer blocks it, so a literal `$` adjacent to such a
        // run must be escaped to avoid forming math on the round trip.
        let marker_len = same_char_run_len(input, offset, '$');
        if marker_len == 0 || text_char_at_edge(input, offset, marker_len) {
            return true;
        }
        let after_open = offset + marker_len;
        find_same_char_run(input, after_open, '$', marker_len).is_some()
    }

    pub(super) fn text_tilde_can_start(input: &str, offset: usize) -> bool {
        if input[offset..].starts_with("~~") {
            return text_attention_delimiter_can_start(input, offset, "~~", false)
                || text_simple_delimiter_can_start(input, offset, '~');
        }
        text_simple_delimiter_can_start(input, offset, '~')
    }

    pub(super) fn text_caret_can_start(input: &str, offset: usize) -> bool {
        input[offset + '^'.len_utf8()..].starts_with('[')
            || text_simple_delimiter_can_start(input, offset, '^')
    }

    pub(super) fn text_simple_delimiter_can_start(
        input: &str,
        offset: usize,
        marker: char,
    ) -> bool {
        let marker_len = marker.len_utf8();
        if text_char_at_edge(input, offset, marker_len)
            || input[offset + marker_len..].starts_with(marker)
            || input[..offset].ends_with(marker)
        {
            return true;
        }
        input[offset + marker_len..].contains(marker)
    }

    pub(super) fn find_same_char_run(
        input: &str,
        mut offset: usize,
        needle: char,
        run_len: usize,
    ) -> Option<usize> {
        while offset < input.len() {
            let candidate = input[offset..].find(needle).map(|index| offset + index)?;
            if same_char_run_len(input, candidate, needle) == run_len {
                return Some(candidate);
            }
            offset = candidate + needle.len_utf8();
        }
        None
    }

    pub(super) fn output_line_len(output: &str) -> usize {
        output
            .rsplit_once('\n')
            .map(|(_, line)| line.len())
            .unwrap_or_else(|| output.len())
    }

    pub(super) fn same_char_run_len(input: &str, offset: usize, needle: char) -> usize {
        input[offset..]
            .chars()
            .take_while(|char| *char == needle)
            .map(char::len_utf8)
            .sum()
    }
}

fn for_each_scan(seed: u64, mut check: impl FnMut(&str, &mut TextScan, usize)) {
    let mut rng = Rng(seed ^ 0x2545_F491_4F6C_DD1D);
    for input in generated_inputs(700, 40, seed) {
        let positions = boundaries(&input);
        for order in query_orders(&positions, &mut rng) {
            let mut scan = TextScan::new(&input);
            for offset in order {
                if offset < input.len() {
                    check(&input, &mut scan, offset);
                }
            }
        }
    }
}

#[test]
fn attention_delimiter_checks_match_the_reference_scan() {
    for_each_scan(21, |input, scan, offset| {
        for (marker, underscore) in [
            ("*", false),
            ("_", true),
            ("++", false),
            ("==", false),
            ("~~", false),
        ] {
            assert_eq!(
                text_attention_delimiter_can_start(input, offset, marker, underscore, scan),
                reference::text_attention_delimiter_can_start(input, offset, marker, underscore),
                "{input:?} at {offset} for {marker}"
            );
        }
    });
}

#[test]
fn run_and_lookahead_checks_match_the_reference_scan() {
    for_each_scan(22, |input, scan, offset| {
        let char = input[offset..].chars().next().expect("offset below len");
        match char {
            '$' => assert_eq!(
                text_math_can_start(input, offset, scan),
                reference::text_math_can_start(input, offset),
                "{input:?} at {offset}"
            ),
            '<' => assert_eq!(
                text_less_than_can_start_inline(input, offset, scan),
                reference::text_less_than_can_start_inline(input, offset),
                "{input:?} at {offset}"
            ),
            '|' => assert_eq!(
                text_spoiler_can_start(input, offset, scan),
                reference::text_spoiler_can_start(input, offset),
                "{input:?} at {offset}"
            ),
            '~' => assert_eq!(
                text_tilde_can_start(input, offset, scan),
                reference::text_tilde_can_start(input, offset),
                "{input:?} at {offset}"
            ),
            '^' => assert_eq!(
                text_caret_can_start(input, offset, scan),
                reference::text_caret_can_start(input, offset),
                "{input:?} at {offset}"
            ),
            '{' => assert_eq!(
                scan.occurs_from("}", offset + 1),
                input[offset + 1..].contains('}'),
                "{input:?} at {offset}"
            ),
            _ => {}
        }
    });
}

#[test]
fn tracked_output_line_lengths_match_the_reference_scan() {
    let mut rng = Rng(23);
    for input in generated_inputs(700, 40, 23) {
        let mut output = String::new();
        let mut line = OutputLine::default();
        for piece in input.split_inclusive(|_: char| rng.below(3) == 0) {
            output.push_str(piece);
            assert_eq!(
                line.len(&output),
                reference::output_line_len(&output),
                "{output:?}"
            );
        }
    }
}
