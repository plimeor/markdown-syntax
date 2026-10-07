//! Checks every memoized or bounded scan in `parse` against a plain forward
//! scan that defines its answer: at every start position of many generated
//! inputs, queried both in shuffled and in ascending order (the inline pass
//! asks in ascending order; memo answers must not depend on order).

use alloc::vec::Vec;

use super::*;
use crate::test_support::{boundaries, generated_inputs, query_orders, Rng};

/// Plain forward scans that define what each memoized or bounded lookup must
/// answer. Each runs from its start position to its answer with no shared
/// state.
#[allow(clippy::all)]
mod reference {
    use super::super::*;

    pub(super) fn find_link_label_end(input: &str, open: usize) -> Option<usize> {
        if input.as_bytes().get(open) != Some(&b'[') {
            return None;
        }

        let mut depth = 1usize;
        let mut cursor = open + 1;
        while cursor < input.len() {
            let (next, char) = next_char(input, cursor)?;
            match char {
                '\\' => {
                    cursor = next_char(input, next)
                        .map(|(after_escape, _)| after_escape)
                        .unwrap_or(next);
                    continue;
                }
                '`' => {
                    if let Some((end, _)) = parse_code_span(input, cursor) {
                        cursor = end;
                        continue;
                    }
                }
                '<' => {
                    if let Some(end) = parse_autolink_end(input, cursor) {
                        let raw = &input[cursor..end];
                        if is_autolink(raw) {
                            cursor = end;
                            continue;
                        }
                    }
                    if let Some((end, _)) = parse_html_inline(input, cursor) {
                        cursor = end;
                        continue;
                    }
                }
                '[' => depth += 1,
                ']' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(cursor);
                    }
                }
                _ => {}
            }
            cursor = next;
        }
        None
    }

    pub(super) fn parse_code_span(input: &str, index: usize) -> Option<(usize, String)> {
        let len = input[index..]
            .as_bytes()
            .iter()
            .take_while(|byte| **byte == b'`')
            .count();
        let search_start = index + len;
        let close = find_code_span_close(input, search_start, len)?;
        Some((
            close + len,
            normalize_code_span(&input[search_start..close]),
        ))
    }

    pub(super) fn parse_autolink_end(input: &str, index: usize) -> Option<usize> {
        input[index..].find('>').map(|end| index + end + 1)
    }

    pub(super) fn parse_html_inline(input: &str, index: usize) -> Option<(usize, String)> {
        let rest = &input[index..];
        if rest.starts_with("<!--") {
            let end = rest.find("-->")? + 3;
            return Some((index + end, rest[..end].into()));
        }
        if rest.starts_with("<?") {
            let end = rest.find("?>")? + 2;
            return Some((index + end, rest[..end].into()));
        }
        if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>")? + 3;
            return Some((index + end, rest[..end].into()));
        }
        if is_declaration_start(rest) {
            let end = rest.find('>')? + 1;
            return Some((index + end, rest[..end].into()));
        }

        let (end, _) = parse_html_tag(input, index)?;
        Some((end, input[index..end].into()))
    }

    pub(super) fn parse_html_tag(input: &str, index: usize) -> Option<(usize, &str)> {
        let bytes = input.as_bytes();
        if bytes.get(index) != Some(&b'<') {
            return None;
        }

        let closing = bytes.get(index + 1) == Some(&b'/');
        let name_start = index + if closing { 2 } else { 1 };
        let first = *bytes.get(name_start)?;
        if !first.is_ascii_alphabetic() {
            return None;
        }

        let mut cursor = name_start + 1;
        while bytes.get(cursor).is_some_and(|byte| html_name_byte(*byte)) {
            cursor += 1;
        }
        let name = &input[name_start..cursor];

        if closing {
            cursor = skip_spaces(input, cursor);
            if bytes.get(cursor) == Some(&b'>') {
                return Some((cursor + 1, name));
            }
            return None;
        }

        let mut needs_space = false;
        loop {
            let before_spaces = cursor;
            cursor = skip_spaces(input, cursor);
            let had_space = cursor > before_spaces;
            match bytes.get(cursor) {
                Some(b'>') => return Some((cursor + 1, name)),
                Some(b'/') if bytes.get(cursor + 1) == Some(&b'>') => {
                    return Some((cursor + 2, name))
                }
                Some(byte) if had_space && html_attribute_name_start(*byte) => {
                    cursor += 1;
                    while bytes
                        .get(cursor)
                        .is_some_and(|byte| html_attribute_name_byte(*byte))
                    {
                        cursor += 1;
                    }
                    let after_name = cursor;
                    let after_spaces = skip_spaces(input, cursor);
                    if bytes.get(after_spaces) == Some(&b'=') {
                        cursor = skip_spaces(input, after_spaces + 1);
                        cursor = parse_html_attribute_value(input, cursor)?;
                    } else {
                        cursor = after_name;
                    }
                    needs_space = true;
                }
                Some(_) if needs_space => return None,
                _ => return None,
            }
        }
    }

    pub(super) fn parse_html_attribute_value(input: &str, index: usize) -> Option<usize> {
        let bytes = input.as_bytes();
        match bytes.get(index)? {
            b'"' | b'\'' => {
                let quote = bytes[index];
                let mut cursor = index + 1;
                while cursor < bytes.len() {
                    if bytes[cursor] == quote {
                        return Some(cursor + 1);
                    }
                    cursor += 1;
                }
                None
            }
            b'=' | b'<' | b'>' | b'`' => None,
            _ => {
                let mut cursor = index;
                while bytes.get(cursor).is_some_and(|byte| {
                    !byte.is_ascii_whitespace()
                        && !matches!(*byte, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
                }) {
                    cursor += 1;
                }
                if cursor == index {
                    None
                } else {
                    Some(cursor)
                }
            }
        }
    }

    pub(super) fn is_autolink(input: &str) -> bool {
        let inner = &input[1..input.len() - 1];
        is_uri_autolink(inner) || is_email_autolink(inner)
    }

    pub(super) fn is_uri_autolink(input: &str) -> bool {
        let Some(colon) = input.find(':') else {
            return false;
        };
        let scheme = &input[..colon];
        if scheme.len() < 2 || scheme.len() > 32 {
            return false;
        }
        let mut bytes = scheme.bytes();
        if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic()) {
            return false;
        }
        if !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-')) {
            return false;
        }
        input[colon + 1..]
            .chars()
            .all(|char| !matches!(char, '<' | '>') && !char.is_control() && !char.is_whitespace())
    }

    pub(super) fn is_email_autolink(input: &str) -> bool {
        if input.chars().any(char::is_whitespace) {
            return false;
        }
        let Some(at) = input.find('@') else {
            return false;
        };
        if at == 0 || at + 1 >= input.len() {
            return false;
        }
        // Angle-bracket `<email>` autolinks use the strict CommonMark domain
        // grammar but, unlike the GFM bare form, allow a single (dotless) label.
        is_email_local_part(&input[..at]) && is_email_domain(&input[at + 1..], 1)
    }

    pub(super) fn find_reference_label_end(input: &str, open: usize) -> Option<usize> {
        // A reference/definition link label does not nest: it ends at the first
        // unescaped `]`, and an unescaped interior `[` disqualifies it.
        if input.as_bytes().get(open) != Some(&b'[') {
            return None;
        }

        let mut cursor = open + 1;
        while cursor < input.len() {
            let (next, char) = next_char(input, cursor)?;
            match char {
                '\\' => {
                    cursor = next_char(input, next)
                        .map(|(after_escape, _)| after_escape)
                        .unwrap_or(next);
                    continue;
                }
                '[' => return None,
                ']' => {
                    return reference_label_is_within_limit(&input[open + 1..cursor])
                        .then_some(cursor);
                }
                _ => {}
            }
            cursor = next;
        }
        None
    }

    pub(super) fn find_wikilink_close(input: &str, start: usize) -> Option<usize> {
        let bytes = input.as_bytes();
        let mut cursor = start;
        while cursor < input.len() {
            match bytes[cursor] {
                b'\\' => {
                    cursor += 1;
                    if cursor < input.len() {
                        cursor = next_char(input, cursor)?.0;
                    }
                }
                b'\n' | b'\r' | b'[' => return None,
                b']' if bytes.get(cursor + 1) == Some(&b']') => return Some(cursor),
                b']' => return None,
                _ => cursor = next_char(input, cursor)?.0,
            }
        }
        None
    }

    pub(super) fn find_directive_attributes_close(input: &str, open: usize) -> Option<usize> {
        if input.as_bytes().get(open) != Some(&b'{') {
            return None;
        }

        let bytes = input.as_bytes();
        let mut cursor = open + 1;
        let mut quote = None;
        let mut escaped = false;
        while cursor < input.len() {
            let byte = bytes[cursor];
            if escaped {
                escaped = false;
                cursor += 1;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                cursor += 1;
                continue;
            }
            if let Some(delimiter) = quote {
                if byte == delimiter {
                    quote = None;
                }
                cursor += 1;
                continue;
            }
            match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'}' => return Some(cursor),
                _ => {}
            }
            cursor += 1;
        }
        None
    }

    pub(super) fn has_unclosed_link_label_opener(input: &str, index: usize) -> bool {
        let line_start = input[..index]
            .rfind(['\n', '\r'])
            .map_or(0, |offset| offset + 1);
        let mut depth = 0usize;
        let mut cursor = line_start;
        while cursor < index {
            let Some((next, char)) = next_char(input, cursor) else {
                break;
            };
            match char {
                '\\' => {
                    cursor = next_char(input, next)
                        .map(|(after_escape, _)| after_escape)
                        .unwrap_or(next);
                    continue;
                }
                '[' => depth += 1,
                ']' => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
            cursor = next;
        }
        depth > 0
    }

    pub(super) fn find_link_resource_tail_close(input: &str, start: usize) -> Option<usize> {
        let mut cursor = start;
        while cursor < input.len() {
            let (next, char) = next_char(input, cursor)?;
            match char {
                '\\' => {
                    cursor = next_char(input, next)
                        .map(|(after_escape, _)| after_escape)
                        .unwrap_or(next);
                    continue;
                }
                '\n' | '\r' => return None,
                ')' => return Some(cursor),
                _ => {}
            }
            cursor = next;
        }
        None
    }

    pub(super) fn find_html_container_close(
        lines: &[Line<'_>],
        mut cursor: usize,
        tag: &str,
    ) -> Option<usize> {
        let mut depth = 1usize;
        let mut fence = None;

        while cursor < lines.len() {
            let line = &lines[cursor];
            if let Some(open_fence) = fence {
                if html_container_fence_closes(line, open_fence) {
                    fence = None;
                }
                cursor += 1;
                continue;
            }

            if let Some(open_fence) = html_container_fence_opens(line) {
                fence = Some(open_fence);
                cursor += 1;
                continue;
            }

            if parse_html_container_tag_line(lines[cursor], tag, HtmlContainerTag::Closing)
                .is_some()
            {
                depth -= 1;
                if depth == 0 {
                    return Some(cursor);
                }
            } else if parse_html_container_opening_line(lines[cursor], tag).is_some() {
                depth += 1;
            }

            cursor += 1;
        }

        None
    }

    pub(super) fn parse_link_destination(
        input: &str,
        index: usize,
    ) -> Option<(String, LinkDestinationKind, usize)> {
        if input.as_bytes().get(index) == Some(&b'<') {
            let mut cursor = index + 1;
            while cursor < input.len() {
                let (next, char) = next_char(input, cursor)?;
                if char == '>' && !is_escaped_at(input, cursor) {
                    return Some((
                        unescape_ascii_punctuation(&input[index + 1..cursor]),
                        LinkDestinationKind::Angle,
                        next,
                    ));
                }
                if (char == '<' && !is_escaped_at(input, cursor)) || char == '\n' || char == '\r' {
                    return None;
                }
                cursor = next;
            }
            return None;
        }

        let mut cursor = index;
        let mut depth = 0usize;
        while cursor < input.len() {
            let (next, char) = next_char(input, cursor)?;
            // A bare destination terminates on ASCII space or an ASCII control
            // character; Unicode whitespace (e.g. U+00A0) is ordinary. A backslash
            // before a space is NOT an escape (only ASCII punctuation is escapable),
            // so `\ ` still terminates the destination → `[a](\ b)` is not a link.
            if char == ' ' || char.is_ascii_control() {
                break;
            }
            if char == '(' && !is_escaped_at(input, cursor) {
                depth += 1;
                // CommonMark caps balanced parens in a bare destination at depth 32.
                if depth > 32 {
                    return None;
                }
            } else if char == ')' && !is_escaped_at(input, cursor) {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            cursor = next;
        }

        if cursor == index || depth > 0 {
            None
        } else {
            Some((
                unescape_ascii_punctuation(&input[index..cursor]),
                LinkDestinationKind::Bare,
                cursor,
            ))
        }
    }

    pub(super) fn parse_math_code_inline(input: &str, index: usize) -> Option<(usize, String)> {
        if !input[index..].starts_with("$`") {
            return None;
        }

        let search_start = index + 2;
        let close = input[search_start..]
            .find("`$")
            .map(|offset| search_start + offset)?;
        if close == search_start {
            return None;
        }

        Some((close + 2, input[search_start..close].into()))
    }

    pub(super) fn parse_character_reference(input: &str, index: usize) -> Option<(usize, String)> {
        let rest = input.get(index..)?;
        if let Some(rest) = rest
            .strip_prefix("&#x")
            .or_else(|| rest.strip_prefix("&#X"))
        {
            let digits = rest.find(';')?;
            if digits == 0
                || digits > 6
                || !rest[..digits].bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return None;
            }
            let value = u32::from_str_radix(&rest[..digits], 16).ok()?;
            return Some((
                index + 3 + digits + 1,
                character_reference_value(value).into(),
            ));
        }
        if let Some(rest) = rest.strip_prefix("&#") {
            let digits = rest.find(';')?;
            if digits == 0
                || digits > 7
                || !rest[..digits].bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            let value = rest[..digits].parse::<u32>().ok()?;
            return Some((
                index + 2 + digits + 1,
                character_reference_value(value).into(),
            ));
        }

        let name_end = rest.find(';')?;
        if name_end == 0 || name_end > 32 {
            return None;
        }
        let name = &rest[1..name_end];
        named_character_reference(name).map(|value| (index + name_end + 1, value.into()))
    }

    pub(super) fn parse_literal_email(input: &str, index: usize) -> Option<(usize, String)> {
        let rest = &input[index..];
        let at = rest.find('@')?;
        if at == 0 {
            return None;
        }
        let local = &rest[..at];

        // Determine whether this `@` is preceded by an extended protocol scheme
        // (`mailto:` / `xmpp:`), which both relaxes the href synthesis and (xmpp)
        // allows `/` in the domain.
        let (auto_mailto, is_xmpp) = classify_email_local(local);

        // Left-boundary guard (autolink-1): the char before `index` must not be a
        // local-part continuation char, otherwise the true link starts earlier and
        // this position is interior. After a recognized scheme, the scheme's own
        // preceding-char rule is what matters.
        if !email_left_boundary_ok(input, index, auto_mailto) {
            return None;
        }

        if !email_local_is_valid(local, auto_mailto) {
            return None;
        }

        let domain_start = index + at + 1;
        let domain_end = literal_email_domain_end(input, domain_start, is_xmpp)?;
        let trimmed = autolink_delim(input, domain_start, domain_end);
        if trimmed <= domain_start {
            return None;
        }

        let domain = &input[domain_start..trimmed];
        if !is_gfm_email_domain(domain, is_xmpp) {
            return None;
        }

        let mut destination = String::new();
        if auto_mailto {
            destination.push_str("mailto:");
        }
        destination.push_str(&input[index..trimmed]);
        Some((trimmed, destination))
    }
}

/// Runs `check` once per input and query order, with a fresh `InlineScan` each
/// time.
fn check_scans(
    inputs: Vec<String>,
    seed: u64,
    mut check: impl FnMut(&str, &mut InlineScan, usize),
) {
    let mut rng = Rng(seed ^ 0x9E37_79B9_7F4A_7C15);
    for input in inputs {
        let positions = boundaries(&input);
        for order in query_orders(&positions, &mut rng) {
            let mut scan = InlineScan::new(&input);
            for &position in &order {
                check(&input, &mut scan, position);
            }
        }
    }
}

/// `check_scans` over the generated Markdown-ish inputs.
fn for_each_scan(seed: u64, check: impl FnMut(&str, &mut InlineScan, usize)) {
    check_scans(generated_inputs(700, 40, seed), seed, check);
}

#[test]
fn link_label_ends_match_the_reference_scan() {
    for_each_scan(1, |input, scan, open| {
        let expected = reference::find_link_label_end(input, open);
        assert_eq!(scan.link_label_end(open), expected, "{input:?} at {open}");
        assert_eq!(
            find_link_label_end(input, open),
            expected,
            "{input:?} at {open}"
        );
    });
}

#[test]
fn reference_label_ends_match_the_reference_scan() {
    for_each_scan(2, |input, scan, open| {
        let expected = reference::find_reference_label_end(input, open);
        assert_eq!(
            scan.reference_label_end(open),
            expected,
            "{input:?} at {open}"
        );
        assert_eq!(
            find_reference_label_end(input, open),
            expected,
            "{input:?} at {open}"
        );
    });
}

#[test]
fn wikilink_closes_match_the_reference_scan() {
    for_each_scan(3, |input, scan, start| {
        let expected = reference::find_wikilink_close(input, start);
        assert_eq!(scan.wikilink_close(start), expected, "{input:?} at {start}");
    });
}

#[test]
fn directive_attribute_closes_match_the_reference_scan() {
    for_each_scan(5, |input, scan, open| {
        let expected = reference::find_directive_attributes_close(input, open);
        assert_eq!(
            scan.directive_attributes_close(open),
            expected,
            "{input:?} at {open}"
        );
        assert_eq!(
            find_directive_attributes_close(input, open),
            expected,
            "{input:?} at {open}"
        );
    });
}

#[test]
fn code_span_ends_match_the_reference_scan() {
    for_each_scan(8, |input, scan, index| {
        if input.as_bytes().get(index) != Some(&b'`') {
            return;
        }
        let expected = reference::parse_code_span(input, index);
        let found = parse_code_span_with(&mut scan.lookups, input, index);
        assert_eq!(found, expected, "{input:?} at {index}");
        let end = code_span_end(&mut scan.lookups, input, index);
        assert_eq!(end, expected.map(|(end, _)| end), "{input:?} at {index}");
    });
}

#[test]
fn angle_constructs_match_the_reference_scan() {
    for_each_scan(9, |input, scan, index| {
        if input.as_bytes().get(index) != Some(&b'<') {
            return;
        }
        let expected_autolink = reference::parse_autolink_end(input, index)
            .filter(|end| reference::is_autolink(&input[index..*end]));
        assert_eq!(
            autolink_end(&mut scan.lookups, input, index),
            expected_autolink,
            "{input:?} at {index}"
        );
        let expected_html = reference::parse_html_inline(input, index).map(|(end, _)| end);
        assert_eq!(
            html_inline_end(&mut scan.lookups, input, index),
            expected_html,
            "{input:?} at {index}"
        );
        assert_eq!(
            parse_html_tag(input, index),
            reference::parse_html_tag(input, index),
            "{input:?} at {index}"
        );
    });
}

#[test]
fn autolink_validity_matches_the_reference_checks() {
    for input in generated_inputs(5000, 12, 10) {
        assert_eq!(
            is_uri_autolink(&input),
            reference::is_uri_autolink(&input),
            "{input:?}"
        );
        assert_eq!(
            is_email_autolink(&input),
            reference::is_email_autolink(&input),
            "{input:?}"
        );
    }
}

#[test]
fn link_destinations_match_the_reference_scan() {
    for_each_scan(11, |input, scan, index| {
        assert_eq!(
            parse_link_destination(&mut scan.lookups, input, index),
            reference::parse_link_destination(input, index),
            "{input:?} at {index}"
        );
    });
}

#[test]
fn math_code_spans_match_the_reference_scan() {
    for_each_scan(13, |input, scan, index| {
        assert_eq!(
            parse_math_code_inline(&mut scan.lookups, input, index),
            reference::parse_math_code_inline(input, index),
            "{input:?} at {index}"
        );
    });
}

#[test]
fn character_references_match_the_reference_scan() {
    for input in generated_inputs(700, 40, 14) {
        for index in boundaries(&input) {
            if input.as_bytes().get(index) != Some(&b'&') {
                continue;
            }
            assert_eq!(
                parse_character_reference(&input, index),
                reference::parse_character_reference(&input, index),
                "{input:?} at {index}"
            );
        }
    }
}

#[test]
fn literal_autolink_scans_match_the_reference_scan() {
    for_each_scan(15, |input, scan, index| {
        let found_email =
            parse_literal_email(input, index, &mut scan.literal_autolinks.email_local)
                .map(|(end, prefix)| (end, alloc::format!("{prefix}{}", &input[index..end])));
        assert_eq!(
            found_email,
            reference::parse_literal_email(input, index),
            "{input:?} at {index}"
        );
        assert_eq!(
            scan.literal_autolinks.resource_tail_close(input, index),
            reference::find_link_resource_tail_close(input, index),
            "{input:?} at {index}"
        );
        assert_eq!(
            scan.literal_autolinks
                .label_openers
                .has_unclosed_opener(input, index),
            reference::has_unclosed_link_label_opener(input, index),
            "{input:?} at {index}"
        );
    });
}

#[test]
fn every_literal_autolink_the_parser_reads_has_its_destination() {
    for input in generated_inputs(700, 40, 16) {
        for index in boundaries(&input) {
            let Some((end, prefix)) =
                parse_literal_autolink(&input, index, &mut LiteralAutolinkScan::default())
            else {
                continue;
            };
            let text = &input[index..end];
            assert_eq!(
                literal_autolink_destination(text),
                Some(alloc::format!("{prefix}{text}")),
                "{input:?} at {index}"
            );
        }
    }
}

#[test]
fn html_container_closes_match_the_reference_scan() {
    let mut rng = Rng(17);
    for input in generated_inputs(700, 40, 17) {
        let map = SourceMap::verbatim(input.len(), 0);
        let lines = collect_lines(&input, &map);
        let starts: Vec<usize> = (0..=lines.len()).collect();
        for order in query_orders(&starts, &mut rng) {
            let mut closes = BracketMemo::default();
            for start in order {
                let found = closes.resolve(lines.len() + 1, start, |cursor| {
                    html_container_close_step(&lines, cursor, "details")
                });
                assert_eq!(
                    found,
                    reference::find_html_container_close(&lines, start, "details"),
                    "{input:?} at line {start}"
                );
            }
        }
    }
}
