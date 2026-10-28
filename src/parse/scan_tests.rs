//! Checks every memoized or bounded scan in `parse` against a plain forward
//! scan that defines its answer: at every start position of many generated
//! inputs, queried both in shuffled and in ascending order (the inline pass
//! asks in ascending order; memo answers must not depend on order).

use alloc::vec::Vec;

use super::*;
use crate::decode::parse_character_reference;
use crate::test_support::{
    boundaries, fixture_inputs, generated_inputs, labels, query_orders, Rng,
};

/// Plain forward scans that define what each memoized or bounded lookup must
/// answer. Each runs from its start position to its answer with no shared
/// state.
#[allow(clippy::all)]
mod reference {
    use super::super::*;
    use crate::{decode::character_reference_value, entities::named_character_reference};

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
                    // A run that opens no code span is literal text whole.
                    cursor = parse_code_span(input, cursor).map_or_else(
                        || {
                            cursor
                                + input[cursor..]
                                    .bytes()
                                    .take_while(|byte| *byte == b'`')
                                    .count()
                        },
                        |(end, _)| end,
                    );
                    continue;
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
        // The `?>` closing a processing instruction follows its `<?`.
        if rest.starts_with("<?") {
            let end = rest[2..].find("?>")? + 4;
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
                        decode_escapes_and_references(&input[index + 1..cursor]),
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
                decode_escapes_and_references(&input[index..cursor]),
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
        if !email_left_boundary_ok(input, index, auto_mailto, |end| {
            prefix_ends_with_gfm_email(input, end)
        }) {
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

    pub(super) fn parse_math_inline(
        input: &str,
        index: usize,
    ) -> Option<(usize, String, MathInlineKind)> {
        if let Some((end, value)) = parse_math_code_inline(input, index) {
            return Some((end, value, MathInlineKind::Code));
        }

        let bytes = input.as_bytes();
        let open_dollars = bytes[index..]
            .iter()
            .take_while(|byte| **byte == b'$')
            .count();
        if open_dollars == 0 || open_dollars > 2 {
            return None;
        }

        let content_start = index + open_dollars;
        let close = scan_to_closing_dollar(input, content_start, open_dollars)?;
        let content_end = close - open_dollars;
        if content_end <= content_start {
            return None;
        }

        let raw = &input[content_start..content_end];
        let value = if open_dollars == 1 {
            normalize_math_text(raw)
        } else {
            raw.into()
        };
        let dollars = u8::try_from(open_dollars).unwrap_or(u8::MAX);
        Some((close, value, MathInlineKind::Dollar { dollars }))
    }

    fn scan_to_closing_dollar(input: &str, start: usize, open_dollars: usize) -> Option<usize> {
        let bytes = input.as_bytes();
        if open_dollars == 1 && bytes.get(start).is_some_and(|byte| is_math_space(*byte)) {
            return None;
        }

        let mut cursor = start;
        loop {
            while cursor < bytes.len() && bytes[cursor] != b'$' {
                cursor += 1;
            }
            if cursor >= bytes.len() {
                return None;
            }
            let prev = bytes[cursor - 1];
            if open_dollars == 1 && is_math_space(prev) {
                return None;
            }
            if open_dollars == 1 && prev == b'\\' {
                cursor += 1;
                continue;
            }
            let run = bytes[cursor..]
                .iter()
                .take(open_dollars)
                .take_while(|byte| **byte == b'$')
                .count();
            if open_dollars == 1 && bytes.get(cursor + run).is_some_and(u8::is_ascii_digit) {
                return None;
            }
            if run == open_dollars {
                return Some(cursor + run);
            }
            cursor += run;
        }
    }

    /// cmark-gfm `check_domain`, walking the whole host: the loop over the
    /// bytes of `data` as cmark holds them (a NUL replaced by U+FFFD), each
    /// read by `is_valid_hostchar`, which decodes the char starting at the byte
    /// and finds none at a continuation byte. How a decoded non-ASCII char is
    /// classified never changes the verdict: the byte after it ends the walk.
    /// A vertical tab ends the host here, as it ends a GFM spec domain, where
    /// cmark-gfm reads on.
    pub(super) fn check_domain(data: &str, allow_short: bool) -> bool {
        let data = data.replace('\0', "\u{FFFD}");
        let bytes = data.as_bytes();
        let size = bytes.len();
        let is_valid_hostchar = |index: usize| {
            data.is_char_boundary(index)
                && data[index..].chars().next().is_some_and(|char| {
                    let space = matches!(char, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ')
                        || (!char.is_ascii() && char.is_whitespace());
                    !space && !crate::unicode_punctuation::is_unicode_punctuation(char)
                })
        };
        let mut np = 0usize;
        let mut uscore1 = 0usize;
        let mut uscore2 = 0usize;

        let mut i = 1;
        while i + 1 < size {
            if bytes[i] == b'\\' && i + 2 < size {
                i += 1;
            }
            if bytes[i] == b'_' {
                uscore2 += 1;
            } else if bytes[i] == b'.' {
                uscore1 = uscore2;
                uscore2 = 0;
                np += 1;
            } else if !is_valid_hostchar(i) && bytes[i] != b'-' {
                break;
            }
            i += 1;
        }

        if (uscore1 > 0 || uscore2 > 0) && np <= 10 {
            return false;
        }
        allow_short || np > 0
    }

    /// The start of a run of `marker` bytes at or before `index`.
    pub(super) fn delimiter_run_start(input: &str, index: usize, marker: u8) -> usize {
        let bytes = input.as_bytes();
        let mut start = index;
        while start > 0 && bytes[start - 1] == marker && !is_escaped_at(input, start - 1) {
            start -= 1;
        }
        start
    }

    /// `definition_label`, finding the label's end afresh in the text of each
    /// line count.
    pub(super) fn definition_label(
        lines: &[Line<'_>],
        index: usize,
    ) -> Option<(String, usize, usize)> {
        let text = trim_ascii_start(lines[index].text);
        if !text.starts_with('[') {
            return None;
        }
        let mut accumulated = String::from(text);
        let mut label_end_line = index;
        let close = loop {
            if let Some(close) = find_reference_label_end(&accumulated, 0) {
                if accumulated.as_bytes().get(close + 1) == Some(&b':') {
                    break close;
                }
                return None;
            }
            let next = label_end_line + 1;
            if next >= lines.len() || accumulated.len() > 4 * REFERENCE_LABEL_MAX_CHARS + 2 {
                return None;
            }
            accumulated.push('\n');
            accumulated.push_str(lines[next].text);
            label_end_line = next;
        };
        Some((accumulated, close, label_end_line))
    }

    /// `is_reference_label`, walking `label` as it is written between
    /// brackets and normalizing it whole.
    pub(super) fn is_reference_label(label: &str) -> bool {
        let written = alloc::format!("[{label}]");
        path_walk(1, |cursor| reference_label_step(&written, cursor)) == Some(written.len() - 1)
            && reference_label_is_within_limit(label)
            && !normalize_label(label).is_empty()
            && !holds_blank_line(label)
    }

    /// `is_written_footnote_label`, reading the `]` written after `label`.
    pub(super) fn is_written_footnote_label(label: &str) -> bool {
        is_footnote_label(label) && !is_escaped_at(&alloc::format!("{label}]"), label.len())
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

/// Inline HTML whose closing string could overlap its opening one, which the
/// generated inputs rarely hold.
#[test]
fn overlapping_html_closes_match_the_reference_scan() {
    for input in [
        "<?>", "<?> b ?>", "<??>", "<?x?>", "<?", "<?>?>", "a <?> b", "<!-->", "<!--->", "<!-- -->",
    ] {
        for index in boundaries(input) {
            if input.as_bytes().get(index) != Some(&b'<') {
                continue;
            }
            let expected = reference::parse_html_inline(input, index).map(|(end, _)| end);
            let mut scan = InlineScan::new(input);
            assert_eq!(
                html_inline_end(&mut scan.lookups, input, index),
                expected,
                "{input:?} at {index}"
            );
        }
    }
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
fn inline_math_matches_the_reference_scan() {
    for_each_scan(18, |input, scan, index| {
        if input.as_bytes().get(index) != Some(&b'$') {
            return;
        }
        assert_eq!(
            parse_math_inline(&mut scan.lookups, input, index),
            reference::parse_math_inline(input, index),
            "{input:?} at {index}"
        );
        assert_eq!(
            parse_math_inline(&mut DirectLookups { input }, input, index),
            reference::parse_math_inline(input, index),
            "{input:?} at {index}"
        );
    });
}

/// One set of tables given from each input's scans to the next input's, as
/// the passes over a document's leaves give them, through inputs ordered
/// long, short, long, so that each input finds tables an input of another
/// length filled and cleared: every memo answers at every start as the
/// reference scan does, and every lookup as the unmemoized one.
#[test]
fn scan_tables_given_from_input_to_input_answer_as_the_reference_scan() {
    let mut inputs = generated_inputs(160, 60, 21);
    inputs.sort_by_key(String::len);
    let mut ordered = Vec::with_capacity(inputs.len());
    while let Some(long) = inputs.pop() {
        ordered.push(long);
        if !inputs.is_empty() {
            ordered.push(inputs.remove(0));
        }
    }
    let mut tables = ScanTables::default();
    for input in &ordered {
        let input = input.as_str();
        let mut scan = InlineScan::with_tables(input, tables);
        for index in boundaries(input) {
            assert_eq!(
                scan.link_label_end(index),
                reference::find_link_label_end(input, index),
                "{input:?} at {index}"
            );
            assert_eq!(
                scan.reference_label_end(index),
                reference::find_reference_label_end(input, index),
                "{input:?} at {index}"
            );
            assert_eq!(
                scan.wikilink_close(index),
                reference::find_wikilink_close(input, index),
                "{input:?} at {index}"
            );
            assert_eq!(
                scan.directive_attributes_close(index),
                reference::find_directive_attributes_close(input, index),
                "{input:?} at {index}"
            );
            // The lookups answer as the same constructs read without tables.
            let (lookups, direct) = (&mut scan.lookups, &mut DirectLookups { input });
            match input.as_bytes().get(index) {
                Some(b'`') => assert_eq!(
                    parse_code_span_with(lookups, input, index),
                    parse_code_span_with(direct, input, index),
                    "{input:?} at {index}"
                ),
                Some(b'<') => {
                    assert_eq!(
                        autolink_end(lookups, input, index),
                        autolink_end(direct, input, index),
                        "{input:?} at {index}"
                    );
                    assert_eq!(
                        html_inline_end(lookups, input, index),
                        html_inline_end(direct, input, index),
                        "{input:?} at {index}"
                    );
                }
                Some(b'$') => assert_eq!(
                    parse_math_inline(lookups, input, index),
                    parse_math_inline(direct, input, index),
                    "{input:?} at {index}"
                ),
                _ => {}
            }
            assert_eq!(
                parse_link_destination(lookups, input, index),
                parse_link_destination(direct, input, index),
                "{input:?} at {index}"
            );
            assert_eq!(
                parse_math_code_inline(lookups, input, index),
                parse_math_code_inline(direct, input, index),
                "{input:?} at {index}"
            );
        }
        tables = scan.into_tables();
        assert!(tables.memos.is_some(), "{input:?}");
        assert!(tables.is_clear(), "{input:?}");
    }
}

/// A pass gives its tables to the next one only from an input of at most
/// `KEPT_TABLES_INPUT_LEN` bytes.
#[test]
fn scan_tables_are_kept_only_from_inputs_up_to_the_kept_length() {
    for (len, kept) in [
        (KEPT_TABLES_INPUT_LEN, true),
        (KEPT_TABLES_INPUT_LEN + 1, false),
    ] {
        let input = "[`".repeat(len / 2) + &"[".repeat(len % 2);
        let mut scan = InlineScan::new(&input);
        assert_eq!(scan.link_label_end(0), None);
        assert_eq!(scan.lookups.code_span_close(1, 1), Some(1));
        let tables = scan.into_tables();
        assert_eq!(tables.lookups.is_some(), kept, "{len}");
        assert_eq!(tables.memos.is_some(), kept, "{len}");
    }
}

/// Inputs dense in host chars, dots, underscores and escapes, so that hosts
/// run past the eleven dots that settle `check_domain`.
fn host_inputs(count: usize, seed: u64) -> Vec<String> {
    const HOST_PIECES: &[&str] = &[
        "www.", "a", "b", ".", "..", "_", "-", "\\", "\\ ", "\\.", "\\\n", " ", "\n", "/", ":",
        "中", "é", "。", "\u{a0}", "😀", "\u{b}", "\0", "\u{fffd}", "http://",
    ];
    let mut rng = Rng(seed);
    (0..count)
        .map(|_| {
            let pieces = rng.below(81);
            (0..pieces)
                .map(|_| HOST_PIECES[rng.below(HOST_PIECES.len())])
                .collect()
        })
        .collect()
}

#[test]
fn domain_checks_match_the_reference_walk() {
    let mut inputs = host_inputs(3000, 19);
    inputs.extend(generated_inputs(700, 40, 19));
    for input in inputs {
        for index in boundaries(&input) {
            for allow_short in [false, true] {
                assert_eq!(
                    check_domain(&input[index..], allow_short),
                    reference::check_domain(&input[index..], allow_short),
                    "{input:?} at {index}, allow_short {allow_short}"
                );
            }
        }
    }
}

#[test]
fn delimiter_run_starts_match_the_reference_walk() {
    for input in generated_inputs(700, 40, 20) {
        for (index, marker) in input.bytes().enumerate() {
            if matches!(marker, b'*' | b'_' | b'~') {
                assert_eq!(
                    starts_delimiter_run(&input, index, marker),
                    reference::delimiter_run_start(&input, index, marker) == index,
                    "{input:?} at {index}"
                );
            }
        }
    }
}

/// Lines of label text, with label lengths around the limit.
fn label_inputs(count: usize, seed: u64) -> Vec<String> {
    let long = "a".repeat(240);
    let wide = "中".repeat(120);
    let pieces: [&str; 16] = [
        "[", "]", "]:", ":", "\\", "\\[", "\\]", "a", " ", "\n", "\n\n", "\\\n", "\r\n", "é",
        &long, &wide,
    ];
    let mut rng = Rng(seed);
    (0..count)
        .map(|_| {
            let count = rng.below(61);
            (0..count)
                .map(|_| pieces[rng.below(pieces.len())])
                .collect()
        })
        .collect()
}

#[test]
fn definition_labels_match_the_reference_scan() {
    let mut inputs = label_inputs(3000, 21);
    inputs.extend(generated_inputs(700, 40, 21));
    for input in inputs {
        let map = SourceMap::verbatim(input.len(), 0);
        let lines = collect_lines(&input, &map);
        for index in 0..lines.len() {
            assert_eq!(
                definition_label(&lines, index),
                reference::definition_label(&lines, index),
                "{input:?} at line {index}"
            );
        }
    }
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
    let check = |input: &str, scan: &mut InlineScan, index| {
        let found_email = parse_literal_email(input, index, &mut scan.literal_autolinks)
            .map(|(end, prefix)| (end, alloc::format!("{prefix}{}", &input[index..end])));
        assert_eq!(
            found_email,
            reference::parse_literal_email(input, index),
            "{input:?} at {index}"
        );
    };
    for_each_scan(15, check);
    // Email-dense inputs: runs of local-part bytes, `@`, schemes, and `+`
    // after an email, which the cached facts of one `@` serve.
    const PIECES: &[&str] = &[
        "a", "b1", ".", "-", "_", "+", "@", ":", "/", " ", "mailto:", "xmpp:", "x@y.z", "é", "..",
    ];
    let mut rng = Rng(0x5eed_e3a1);
    let inputs = (0..1500)
        .map(|_| {
            (0..rng.below(24))
                .map(|_| PIECES[rng.below(PIECES.len())])
                .collect()
        })
        .collect();
    check_scans(inputs, 18, check);
}

#[test]
fn every_literal_autolink_the_parser_reads_has_its_destination() {
    let mut inputs = generated_inputs(700, 40, 16);
    inputs.extend(host_inputs(3000, 16));
    // Escapes, spaces, and `_` around `www.` candidates, repeated past the
    // eleven dots that settle `check_domain`.
    for piece in [
        " www.a_\\",
        "\\ www._",
        "WWW._\\ é",
        "\\ www..._b",
        "_www.a\\_",
    ] {
        inputs.extend((1..=13).map(|count| piece.repeat(count)));
    }
    for input in inputs {
        for index in boundaries(&input) {
            let Some((end, prefix)) =
                parse_literal_autolink(&input, index, &mut LiteralAutolinkScan::default(), true)
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

/// `parse` and `parse_inline` of `input` as `Debug` text.
fn parsed(input: &str) -> String {
    let output = crate::parse(input);
    let inline = crate::parse_inline(input, &Definitions::of(&output.document));
    alloc::format!("{output:?}\n{inline:?}")
}

/// The inline pass reads a run of text, past the bytes `TEXT_STOPS` reads on
/// at and the positions `literal_autolink_may_start` rules out, as it reads
/// the run a char at a time with `literal_email_may_start` holding at every
/// char, so a literal autolink is tried at each `http://`, `https://`, `www.`,
/// and email local-part or scheme byte: on every fixture input, its CRLF
/// variant, and generated inputs, some of them dense in literal autolinks.
/// `text_runs_end_where_a_literal_autolink_may_start` shows that
/// `literal_autolink_may_start` holds wherever a literal autolink forms.
#[test]
fn text_runs_parse_as_their_chars_one_at_a_time() {
    const AUTOLINK_PIECES: &[&str] = &[
        "http://", "https://", "HTTP://", "hTtps://", "www.", "WWW.", "wWw.", "x.y", "a", "B", "1",
        "_", "-", "+", ".", "@", "a@b.c", "mailto:", "MAILTO:", "xmpp:", "/", " ", "\n", "(", ")",
        "*", "~", "<", "é", "中",
    ];
    let mut rng = Rng(0x7e47);
    let autolinks = (0..3_000).map(|_| {
        (0..rng.below(16))
            .map(|_| AUTOLINK_PIECES[rng.below(AUTOLINK_PIECES.len())])
            .collect::<String>()
    });
    let fixtures = fixture_inputs();
    let crlf = fixtures.iter().map(|input| input.replace('\n', "\r\n"));
    let inputs = fixtures
        .iter()
        .cloned()
        .chain(crlf)
        .chain(generated_inputs(3_000, 40, 29))
        .chain(autolinks);
    for input in inputs {
        let in_runs = parsed(&input);
        CHAR_BY_CHAR.with(|on| on.set(true));
        let by_char = parsed(&input);
        CHAR_BY_CHAR.with(|on| on.set(false));
        assert_eq!(in_runs, by_char, "{input:?}");
    }
}

#[test]
fn ascii_labels_normalize_as_the_casefold_does() {
    for label in labels() {
        assert_eq!(normalize_label(&label), casefold_label(&label), "{label:?}");
    }
}

/// Inputs dense in literal autolinks and in what nearly forms one.
fn literal_autolink_inputs(count: usize, seed: u64) -> Vec<String> {
    const PIECES: &[&str] = &[
        "http://", "https://", "HTTP://", "http:/", "https", "www.", "WWW.", "wWw.", "ww", "w",
        "h", "x.y", "a", "B", "1", "_", "-", "+", ".", ":", "@", "@@", "a@b.c", "mailto:", "xmpp:",
        "+x@y.z", "/", " ", "\n", "(", ")", "*", "`", "[", "]", "<", "é", "中",
    ];
    let mut rng = Rng(seed);
    (0..count)
        .map(|_| {
            (0..rng.below(20))
                .map(|_| PIECES[rng.below(PIECES.len())])
                .collect()
        })
        .collect()
}

/// `EmailStarts` answers as the walk to the next position whose run of
/// local-part and scheme bytes ends at an `@` and that starts the content,
/// is a `+`, or follows a byte that is no ASCII letter or digit, for
/// queries in any order.
#[test]
fn email_starts_match_the_reference_walk() {
    let reference = |input: &str, from: usize| {
        let bytes = input.as_bytes();
        (from..bytes.len())
            .find(|&position| {
                let run = bytes[position..]
                    .iter()
                    .take_while(|byte| is_email_local_or_scheme_byte(**byte))
                    .count();
                run > 0
                    && bytes.get(position + run) == Some(&b'@')
                    && (position == 0
                        || bytes[position] == b'+'
                        || !bytes[position - 1].is_ascii_alphanumeric())
            })
            .unwrap_or(bytes.len())
    };
    let inputs = literal_autolink_inputs(1500, 0x5eed_e3a2)
        .into_iter()
        .chain(generated_inputs(300, 40, 30));
    check_scans(inputs.collect(), 31, |input, scan, from| {
        assert_eq!(
            scan.literal_autolinks
                .email_starts
                .first_at_or_after(input, from),
            reference(input, from),
            "{input:?} from {from}"
        );
    });
}

/// A run of text that `plain_text_end` reads from any position ends at or
/// before every later position at which a literal autolink forms, inside an
/// open `[` or not, where `literal_autolink_may_start` holds.
#[test]
fn text_runs_end_where_a_literal_autolink_may_start() {
    let inputs = literal_autolink_inputs(1500, 0x5eed_e3a3)
        .into_iter()
        .chain(generated_inputs(300, 40, 32));
    for input in inputs {
        let positions = boundaries(&input);
        let starts: Vec<usize> = positions
            .iter()
            .copied()
            .filter(|&index| {
                [true, false].into_iter().any(|urls| {
                    parse_literal_autolink(&input, index, &mut LiteralAutolinkScan::default(), urls)
                        .is_some()
                })
            })
            .collect();
        let mut scan = LiteralAutolinkScan::default();
        for &index in &starts {
            assert!(
                literal_autolink_may_start(&input, index, &mut scan),
                "{input:?} at {index}"
            );
        }
        let mut scan = LiteralAutolinkScan::default();
        for &from in positions.iter().filter(|&&from| from > 0) {
            let next = starts
                .iter()
                .copied()
                .find(|&start| start >= from)
                .unwrap_or(input.len());
            let end = plain_text_end(&input, from, &mut scan);
            assert!(end <= next, "{input:?} from {from}: {end} past {next}");
        }
    }
}

/// Labels over the chars the walk to a label's `]` and label normalization
/// treat specially: every string of up to five of them, and `labels()`.
fn walk_labels() -> Vec<String> {
    let chars = ['\\', '[', ']', 'a', 'B', ' ', '\n', '\r', 'é', '\0'];
    let mut labels = labels();
    let mut grown = Vec::from([String::new()]);
    for _ in 0..5 {
        grown = grown
            .iter()
            .flat_map(|label| {
                chars
                    .iter()
                    .map(move |char| alloc::format!("{label}{char}"))
            })
            .collect();
        labels.extend(grown.iter().cloned());
    }
    labels
}

#[test]
fn reference_labels_match_the_written_walk() {
    for label in walk_labels() {
        assert_eq!(
            is_reference_label(&label),
            reference::is_reference_label(&label),
            "{label:?}"
        );
        assert_eq!(
            is_written_footnote_label(&label),
            reference::is_written_footnote_label(&label),
            "{label:?}"
        );
    }
}

/// `normalizes_to` answers as comparing `normalize_label`'s result does: for
/// each label's own identifier, that identifier one char shorter or longer
/// or with a char changed, another label's identifier, and the label.
#[test]
fn labels_normalize_to_their_identifier_alone() {
    let labels = walk_labels();
    for (index, label) in labels.iter().enumerate() {
        let identifier = normalize_label(label);
        let mut candidates = Vec::from([
            identifier.clone(),
            alloc::format!("{identifier}a"),
            normalize_label(&labels[(index * 7 + 3) % labels.len()]),
            label.clone(),
        ]);
        if let Some((last, _)) = identifier.char_indices().last() {
            candidates.push(String::from(&identifier[..last]));
            candidates.push(alloc::format!("{}a", &identifier[..last]));
        }
        for candidate in candidates {
            assert_eq!(
                normalizes_to(label, &candidate),
                normalize_label(label) == candidate,
                "{label:?} to {candidate:?}"
            );
        }
    }
}

/// Definitions hold their identifiers in the order of their bytes, and a
/// lookup finds an identifier exactly when they hold it, whatever bytes the
/// identifiers share: among strings of up to five of a few chars, U+0000
/// and the eight-byte prefixes a lookup compares first among them, each
/// held or not.
#[test]
fn definitions_find_exactly_the_identifiers_they_hold() {
    let chars = ['\0', 'a', 'b', 'é'];
    let mut identifiers = Vec::from([String::new()]);
    let mut grown = Vec::from([String::new()]);
    for _ in 0..5 {
        grown = grown
            .iter()
            .flat_map(|text| chars.iter().map(move |char| alloc::format!("{text}{char}")))
            .collect();
        identifiers.extend(grown.iter().cloned());
    }
    identifiers
        .extend(["aaaaaaaa", "aaaaaaaab", "aaaaaaaa\0", "aaaaaaa", "aaaaaaab"].map(String::from));
    for held_every in [1, 2, 3, 7] {
        let held: Vec<&String> = identifiers.iter().step_by(held_every).collect();
        let definitions = Definitions::sorted(
            held.iter()
                .map(|text| Identifier::new(String::from(text.as_str())))
                .collect(),
        );
        // In the order of their bytes, as a sorted list of the texts is.
        let mut texts: Vec<&String> = held.clone();
        texts.sort_unstable();
        texts.dedup();
        assert!(definitions
            .identifiers
            .iter()
            .map(|identifier| &identifier.text)
            .eq(texts));
        let labels = definitions.labels();
        for identifier in &identifiers {
            let found = labels.find(identifier.as_bytes());
            assert_eq!(
                found.is_some(),
                held.contains(&identifier),
                "{identifier:?}"
            );
            if let Some(index) = found {
                assert_eq!(&labels.own[index].text, identifier);
            }
        }
    }
}

#[test]
fn definitions_read_as_their_identifiers() {
    let definitions = Definitions::new(["Foo  Bar", "\u{1E9E}"]);
    assert_eq!(
        alloc::format!("{definitions:?}"),
        r#"Definitions { identifiers: ["foo bar", "ss"] }"#
    );
}
