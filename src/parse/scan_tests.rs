//! Checks every memoized or bounded scan in `parse` against a plain forward
//! scan that defines its answer: at every start position of many generated
//! inputs, queried both in shuffled and in ascending order (the inline pass
//! asks in ascending order; memo answers must not depend on order).

use alloc::vec::Vec;

use super::*;
use crate::test_support::{
    boundaries, generated_expression_blocks, generated_inputs, generated_jsx, query_orders, Rng,
};

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

    pub(super) fn parse_code_span(input: &str, index: usize) -> Option<(usize, CodeSpanSource)> {
        let len = input[index..]
            .as_bytes()
            .iter()
            .take_while(|byte| **byte == b'`')
            .count();
        let search_start = index + len;
        let close = find_code_span_close(input, search_start, len)?;
        let raw = &input[search_start..close];
        Some((
            close + len,
            CodeSpanSource {
                value: normalize_code_span(raw),
                raw: raw.into(),
                fence_length: len,
            },
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
                b'\n' | b'\r' => return None,
                b']' if bytes.get(cursor + 1) == Some(&b']') => return Some(cursor),
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

    pub(super) fn find_mdx_expression_inline_close(input: &str, open_byte: usize) -> Option<usize> {
        let bytes = input.as_bytes();
        if bytes.get(open_byte) != Some(&b'{') {
            return None;
        }

        let mut depth = 0usize;
        let mut state = MdxBraceState::Normal;
        let mut escaped = false;
        let mut cursor = open_byte;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            match state {
                MdxBraceState::Normal => match byte {
                    b'\'' => state = MdxBraceState::SingleQuoted,
                    b'"' => state = MdxBraceState::DoubleQuoted,
                    b'`' => state = MdxBraceState::Template,
                    b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                        state = MdxBraceState::LineComment;
                        cursor += 1;
                    }
                    b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                        state = MdxBraceState::BlockComment;
                        cursor += 1;
                    }
                    b'{' => depth += 1,
                    b'}' => {
                        depth = depth.checked_sub(1)?;
                        if depth == 0 {
                            return Some(cursor);
                        }
                    }
                    _ => {}
                },
                MdxBraceState::SingleQuoted => {
                    update_mdx_quote_state(byte, b'\'', &mut state, &mut escaped);
                }
                MdxBraceState::DoubleQuoted => {
                    update_mdx_quote_state(byte, b'"', &mut state, &mut escaped);
                }
                MdxBraceState::Template => {
                    update_mdx_quote_state(byte, b'`', &mut state, &mut escaped);
                }
                MdxBraceState::LineComment => {
                    if byte == b'\n' {
                        state = MdxBraceState::Normal;
                    }
                }
                MdxBraceState::BlockComment => {
                    if byte == b'*' && bytes.get(cursor + 1) == Some(&b'/') {
                        state = MdxBraceState::Normal;
                        cursor += 1;
                    }
                }
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

    pub(super) fn find_single_tilde_delete_close(input: &str, start: usize) -> Option<usize> {
        let mut cursor = start;
        while cursor < input.len() {
            let Some(candidate) = input[cursor..].find('~').map(|index| cursor + index) else {
                break;
            };
            if !is_escaped_at(input, candidate) && single_tilde_can_close_delete(input, candidate) {
                return Some(candidate);
            }
            cursor = candidate + 1;
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

    pub(super) fn relaxed_scheme_after_slashes(rest: &str) -> Option<usize> {
        let bytes = rest.as_bytes();
        if bytes.starts_with(b"://") {
            return Some(3);
        }
        let first = bytes.first()?;
        if !first.is_ascii_alphabetic() {
            return None;
        }
        let mut i = 1;
        while i < bytes.len() {
            match bytes[i] {
                b':' => break,
                byte if byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-') => {
                    i += 1
                }
                _ => return None,
            }
        }
        if bytes.get(i..i + 3) == Some(b"://") {
            Some(i + 3)
        } else {
            None
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum MdxBraceState {
        Normal,
        SingleQuoted,
        DoubleQuoted,
        Template,
        LineComment,
        BlockComment,
    }

    pub(super) fn update_mdx_quote_state(
        byte: u8,
        delimiter: u8,
        state: &mut MdxBraceState,
        escaped: &mut bool,
    ) {
        if *escaped {
            *escaped = false;
            return;
        }
        if byte == b'\\' {
            *escaped = true;
            return;
        }
        if byte == delimiter {
            *state = MdxBraceState::Normal;
        }
    }

    pub(super) fn find_mdx_jsx_close<'a>(lines: &'a [Line<'a>], index: usize) -> Option<usize> {
        let line = lines[index];
        let trimmed = line.text.trim_start();
        let start_byte = line.text.len() - trimmed.len();
        let root = mdx_jsx_tag_start(line.text, start_byte)?;
        if root.closing {
            return None;
        }

        let (mut cursor_line, mut cursor_byte, self_closing) =
            find_mdx_jsx_tag_end(lines, index, start_byte)?;
        if self_closing {
            return Some(cursor_line);
        }

        let mut depth = 1usize;
        cursor_byte += 1;
        'scan: while cursor_line < lines.len() {
            let line = lines[cursor_line].text;
            while cursor_byte < line.len() {
                let Some(relative_start) = line[cursor_byte..].find('<') else {
                    break;
                };
                let tag_start_byte = cursor_byte + relative_start;
                let Some(candidate) = mdx_jsx_tag_start(line, tag_start_byte) else {
                    cursor_byte = tag_start_byte + 1;
                    continue;
                };
                let Some((tag_end_line, tag_end_byte, candidate_self_closing)) =
                    find_mdx_jsx_tag_end(lines, cursor_line, tag_start_byte)
                else {
                    return None;
                };

                if mdx_jsx_tag_matches(root.tag, candidate.tag) {
                    if candidate.closing {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return Some(tag_end_line);
                        }
                    } else if !candidate_self_closing {
                        depth += 1;
                    }
                }

                cursor_byte = tag_end_byte + 1;
                if tag_end_line != cursor_line {
                    cursor_line = tag_end_line;
                    continue 'scan;
                }
            }
            cursor_line += 1;
            cursor_byte = 0;
        }
        None
    }

    pub(super) fn parse_mdx_jsx_inline(input: &str, index: usize) -> Option<(usize, String)> {
        let root = mdx_jsx_tag_start(input, index)?;
        if root.closing {
            return None;
        }

        let (mut cursor, self_closing) = find_mdx_jsx_tag_end_in_text(input, index)?;
        if self_closing {
            let end = cursor + 1;
            return Some((end, input[index..end].into()));
        }

        let mut depth = 1usize;
        cursor += 1;
        while cursor < input.len() {
            let Some(relative_start) = input[cursor..].find('<') else {
                return None;
            };
            let tag_start_byte = cursor + relative_start;
            let Some(candidate) = mdx_jsx_tag_start(input, tag_start_byte) else {
                cursor = tag_start_byte + 1;
                continue;
            };
            let Some((tag_end, candidate_self_closing)) =
                find_mdx_jsx_tag_end_in_text(input, tag_start_byte)
            else {
                return None;
            };

            if mdx_jsx_tag_matches(root.tag, candidate.tag) {
                if candidate.closing {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let end = tag_end + 1;
                        return Some((end, input[index..end].into()));
                    }
                } else if !candidate_self_closing {
                    depth += 1;
                }
            }
            cursor = tag_end + 1;
        }
        None
    }

    pub(super) fn mdx_jsx_tag_matches(left: MdxJsxTag<'_>, right: MdxJsxTag<'_>) -> bool {
        match (left, right) {
            (MdxJsxTag::Fragment, MdxJsxTag::Fragment) => true,
            (MdxJsxTag::Named(left), MdxJsxTag::Named(right)) => left == right,
            _ => false,
        }
    }

    pub(super) fn find_mdx_jsx_tag_end(
        lines: &[Line<'_>],
        start_line: usize,
        start_byte: usize,
    ) -> Option<(usize, usize, bool)> {
        let mut line_index = start_line;
        let mut byte_index = start_byte + 1;
        let mut quote = None;
        let mut escaped = false;
        let mut expression_depth = 0usize;
        let mut expression_state = MdxBraceState::Normal;
        let mut expression_escaped = false;

        while line_index < lines.len() {
            let bytes = lines[line_index].text.as_bytes();
            while byte_index < bytes.len() {
                let byte = bytes[byte_index];
                if expression_depth > 0 {
                    if update_mdx_jsx_expression_state(
                        byte,
                        bytes.get(byte_index + 1).copied(),
                        &mut expression_depth,
                        &mut expression_state,
                        &mut expression_escaped,
                    ) {
                        byte_index += 1;
                    }
                    byte_index += 1;
                    continue;
                }

                if let Some(delimiter) = quote {
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == delimiter {
                        quote = None;
                    }
                    byte_index += 1;
                    continue;
                }

                match byte {
                    b'\'' | b'"' => quote = Some(byte),
                    b'{' => {
                        expression_depth = 1;
                        expression_state = MdxBraceState::Normal;
                        expression_escaped = false;
                    }
                    b'>' if expression_depth == 0 => {
                        let self_closing =
                            previous_nonspace_before(lines, line_index, byte_index) == Some(b'/');
                        return Some((line_index, byte_index, self_closing));
                    }
                    _ => {}
                }
                byte_index += 1;
            }
            if expression_state == MdxBraceState::LineComment {
                expression_state = MdxBraceState::Normal;
            }
            line_index += 1;
            byte_index = 0;
        }
        None
    }

    pub(super) fn previous_nonspace_before(
        lines: &[Line<'_>],
        line_index: usize,
        byte_index: usize,
    ) -> Option<u8> {
        let mut cursor_line = line_index;
        let mut cursor_byte = byte_index;

        loop {
            if let Some(byte) = lines[cursor_line].text.as_bytes()[..cursor_byte]
                .iter()
                .rev()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace())
            {
                return Some(byte);
            }
            if cursor_line == 0 {
                return None;
            }
            cursor_line -= 1;
            cursor_byte = lines[cursor_line].text.len();
        }
    }

    pub(super) fn find_mdx_jsx_tag_end_in_text(
        input: &str,
        start_byte: usize,
    ) -> Option<(usize, bool)> {
        let bytes = input.as_bytes();
        let mut byte_index = start_byte + 1;
        let mut quote = None;
        let mut escaped = false;
        let mut expression_depth = 0usize;
        let mut expression_state = MdxBraceState::Normal;
        let mut expression_escaped = false;

        while byte_index < bytes.len() {
            let byte = bytes[byte_index];
            if expression_depth > 0 {
                if update_mdx_jsx_expression_state(
                    byte,
                    bytes.get(byte_index + 1).copied(),
                    &mut expression_depth,
                    &mut expression_state,
                    &mut expression_escaped,
                ) {
                    byte_index += 1;
                }
                byte_index += 1;
                continue;
            }

            if let Some(delimiter) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == delimiter {
                    quote = None;
                }
                byte_index += 1;
                continue;
            }

            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'{' => {
                    expression_depth = 1;
                    expression_state = MdxBraceState::Normal;
                    expression_escaped = false;
                }
                b'>' if expression_depth == 0 => {
                    let self_closing =
                        previous_nonspace_before_text(input, byte_index) == Some(b'/');
                    return Some((byte_index, self_closing));
                }
                _ => {}
            }
            byte_index += 1;
        }
        None
    }

    pub(super) fn update_mdx_jsx_expression_state(
        byte: u8,
        next: Option<u8>,
        depth: &mut usize,
        state: &mut MdxBraceState,
        escaped: &mut bool,
    ) -> bool {
        match *state {
            MdxBraceState::Normal => match byte {
                b'\'' => *state = MdxBraceState::SingleQuoted,
                b'"' => *state = MdxBraceState::DoubleQuoted,
                b'`' => *state = MdxBraceState::Template,
                b'/' if next == Some(b'/') => {
                    *state = MdxBraceState::LineComment;
                    return true;
                }
                b'/' if next == Some(b'*') => {
                    *state = MdxBraceState::BlockComment;
                    return true;
                }
                b'{' => *depth += 1,
                b'}' => {
                    *depth = (*depth).saturating_sub(1);
                    if *depth == 0 {
                        *state = MdxBraceState::Normal;
                        *escaped = false;
                    }
                }
                _ => {}
            },
            MdxBraceState::SingleQuoted => {
                update_mdx_quote_state(byte, b'\'', state, escaped);
            }
            MdxBraceState::DoubleQuoted => {
                update_mdx_quote_state(byte, b'"', state, escaped);
            }
            MdxBraceState::Template => {
                update_mdx_quote_state(byte, b'`', state, escaped);
            }
            MdxBraceState::LineComment => {
                if byte == b'\n' {
                    *state = MdxBraceState::Normal;
                }
            }
            MdxBraceState::BlockComment => {
                if byte == b'*' && next == Some(b'/') {
                    *state = MdxBraceState::Normal;
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn find_mdx_expression_close(
        lines: &[Line<'_>],
        index: usize,
        open_byte: usize,
    ) -> Option<(usize, usize)> {
        let mut depth = 0usize;
        let mut state = MdxBraceState::Normal;
        let mut escaped = false;
        let mut cursor = index;

        while cursor < lines.len() {
            let bytes = lines[cursor].text.as_bytes();
            let mut byte_index = if cursor == index { open_byte } else { 0 };
            while byte_index < bytes.len() {
                let byte = bytes[byte_index];
                match state {
                    MdxBraceState::Normal => match byte {
                        b'\'' => state = MdxBraceState::SingleQuoted,
                        b'"' => state = MdxBraceState::DoubleQuoted,
                        b'`' => state = MdxBraceState::Template,
                        b'/' if bytes.get(byte_index + 1) == Some(&b'/') => {
                            state = MdxBraceState::LineComment;
                            break;
                        }
                        b'/' if bytes.get(byte_index + 1) == Some(&b'*') => {
                            state = MdxBraceState::BlockComment;
                            byte_index += 1;
                        }
                        b'{' => depth += 1,
                        b'}' => {
                            depth = depth.checked_sub(1)?;
                            if depth == 0 {
                                return lines[cursor].text[byte_index + 1..]
                                    .trim()
                                    .is_empty()
                                    .then_some((cursor, byte_index));
                            }
                        }
                        _ => {}
                    },
                    MdxBraceState::SingleQuoted => {
                        update_mdx_quote_state(byte, b'\'', &mut state, &mut escaped);
                    }
                    MdxBraceState::DoubleQuoted => {
                        update_mdx_quote_state(byte, b'"', &mut state, &mut escaped);
                    }
                    MdxBraceState::Template => {
                        update_mdx_quote_state(byte, b'`', &mut state, &mut escaped);
                    }
                    MdxBraceState::LineComment => break,
                    MdxBraceState::BlockComment => {
                        if byte == b'*' && bytes.get(byte_index + 1) == Some(&b'/') {
                            state = MdxBraceState::Normal;
                            byte_index += 1;
                        }
                    }
                }
                byte_index += 1;
            }
            if state == MdxBraceState::LineComment {
                state = MdxBraceState::Normal;
            }
            cursor += 1;
        }

        None
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
fn mdx_expression_closes_match_the_reference_scan() {
    for_each_scan(6, |input, scan, open| {
        let expected = reference::find_mdx_expression_inline_close(input, open);
        assert_eq!(
            scan.mdx_expression_close(open),
            expected,
            "{input:?} at {open}"
        );
    });
}

#[test]
fn single_tilde_delete_closes_match_the_reference_scan() {
    for_each_scan(7, |input, scan, start| {
        let expected = reference::find_single_tilde_delete_close(input, start);
        assert_eq!(
            scan.single_tilde_delete_close(start),
            expected,
            "{input:?} at {start}"
        );
    });
}

#[test]
fn code_span_ends_match_the_reference_scan() {
    for_each_scan(8, |input, scan, index| {
        if input.as_bytes().get(index) != Some(&b'`') {
            return;
        }
        let fields =
            |(end, span): (usize, CodeSpanSource)| (end, span.value, span.raw, span.fence_length);
        let expected = reference::parse_code_span(input, index).map(fields);
        let found = parse_code_span_with(&mut scan.lookups, input, index).map(fields);
        assert_eq!(found, expected, "{input:?} at {index}");
        let end = code_span_end(&mut scan.lookups, input, index);
        assert_eq!(end, expected.map(|(end, ..)| end), "{input:?} at {index}");
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
            parse_literal_email(input, index, &mut scan.literal_autolinks.email_local);
        assert_eq!(
            found_email,
            reference::parse_literal_email(input, index),
            "{input:?} at {index}"
        );
        let found_scheme =
            relaxed_scheme_after_slashes(input, index, &mut scan.literal_autolinks.scheme);
        assert_eq!(
            found_scheme,
            reference::relaxed_scheme_after_slashes(&input[index..]),
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

#[test]
fn inline_jsx_element_ends_match_the_reference_scan() {
    let check = |input: &str, scan: &mut InlineScan, index: usize| {
        if input.as_bytes().get(index) != Some(&b'<') {
            return;
        }
        assert_eq!(
            scan.mdx_jsx_end(index),
            reference::parse_mdx_jsx_inline(input, index).map(|(end, _)| end),
            "{input:?} at {index}"
        );
    };
    for_each_scan(18, check);
    check_scans(generated_jsx(4000, 32), 33, check);
}

/// Dense JSX block material: tags spread over lines, quotes, expressions, and
/// escapes at line ends.
const FLOW_PIECES: &[&str] = &[
    "<A", "<B", ">", "</A>", "</B>", "/>", "<>", "</>", "\"", "'", "`", "\\", "\n", "\n\n", "{",
    "}", " ", "a", "/", "*", "//", "/*", "*/",
];

fn flow_inputs() -> Vec<String> {
    let mut rng = Rng(29);
    let mut inputs = generated_inputs(1500, 40, 19);
    inputs.extend(generated_jsx(4000, 31));
    inputs.extend(generated_expression_blocks(4000, 37));
    inputs.extend((0..4000).map(|_| {
        let pieces = rng.below(30);
        (0..pieces)
            .map(|_| FLOW_PIECES[rng.below(FLOW_PIECES.len())])
            .collect::<String>()
    }));
    inputs
}

#[test]
fn flow_jsx_and_expression_closes_match_the_reference_scan() {
    let mut rng = Rng(19);
    for input in flow_inputs() {
        let map = SourceMap::verbatim(input.len(), 0);
        let lines = collect_lines(&input, &map);
        let starts: Vec<usize> = (0..lines.len()).collect();
        for order in query_orders(&starts, &mut rng) {
            let mut flow = MdxFlowScan::default();
            for index in order {
                let line = lines[index].text;
                let start_byte = line.len() - line.trim_start().len();
                assert_eq!(
                    flow.jsx_close_line(&lines, index, start_byte),
                    reference::find_mdx_jsx_close(&lines, index),
                    "{input:?} at line {index}"
                );
                if mdx_jsx_tag_start(line, start_byte).is_some_and(|root| !root.closing) {
                    assert_eq!(
                        flow.jsx_tag_self_closing(&lines, index, start_byte),
                        reference::find_mdx_jsx_tag_end(&lines, index, start_byte)
                            .map(|(_, _, self_closing)| self_closing),
                        "{input:?} at line {index}"
                    );
                }
                if line[start_byte..].starts_with('{') {
                    assert_eq!(
                        flow.expression_close(&lines, index, start_byte),
                        reference::find_mdx_expression_close(&lines, index, start_byte),
                        "{input:?} at line {index}"
                    );
                }
            }
        }
    }
}

/// Rows of bars, escaped bars, backslashes, and text, and rows of `||`,
/// escaped bars, backticks, escaped backticks, and text: every spoiler the row
/// scan predicts forms in its cell, and no other does. (Where
/// a code span would hold a pipe that delimits, and a spoiler also crosses it,
/// no split agrees with the inline parse, so the generated rows leave that
/// out.)
/// A cell's source with each escaped pipe unescaped, and the offset in it of
/// each `|` read from `\|`, as `table_row_cells` reads them.
fn table_cell_text(source: &str) -> (String, Vec<usize>) {
    let bytes = source.as_bytes();
    let mut cell = String::with_capacity(source.len());
    let mut escaped_pipes = Vec::new();
    let mut copied = 0;
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\\' {
            let pipe = cursor + delimiter_byte_run_len(source, cursor, b'\\');
            if bytes.get(pipe) == Some(&b'|') && (pipe - cursor) % 2 == 1 {
                cell.push_str(&source[copied..pipe - 1]);
                escaped_pipes.push(cell.len());
                copied = pipe;
            }
            cursor = pipe;
        } else {
            cursor += 1;
        }
    }
    cell.push_str(&source[copied..]);
    (cell, escaped_pipes)
}

#[test]
fn table_row_spoilers_form_where_the_row_scan_predicts() {
    const WITHOUT_CODE: &[&str] = &["|", "||", "|||", "\\|", "\\\\|", "\\", "a", " "];
    const WITH_CODE: &[&str] = &["||", "\\|", "`", "``", "\\`", "\\\\`", "a", " "];
    let options = SyntaxOptions::default();
    let mut rng = Rng(0x7ab1e);
    for pieces in [WITHOUT_CODE, WITH_CODE] {
        for _ in 0..20_000 {
            let mut row = String::new();
            for _ in 0..rng.below(12) + 1 {
                row.push_str(pieces[rng.below(pieces.len())]);
            }
            let row = row.trim();
            let (delimiters, spoilers) = scan_table_row(row, true);
            let mut start = 0;
            for &end in delimiters.iter().chain([row.len()].iter()) {
                let predicted = spoilers
                    .iter()
                    .filter(|&&(open, _)| start <= open && open < end)
                    .count();
                let (text, escaped_pipes) = table_cell_text(&row[start..end]);
                let mut diagnostics = Vec::new();
                let content = text.trim_start_matches([' ', '\t']);
                let trimmed = text.len() - content.len();
                let content = content.trim_end_matches([' ', '\t']);
                let escaped_pipes = escaped_pipes
                    .into_iter()
                    .filter_map(|at| at.checked_sub(trimmed))
                    .collect();
                let map = SourceMap::verbatim(content.len(), 0);
                let formed = parse_cell_inlines(
                    content,
                    &map,
                    escaped_pipes,
                    &options,
                    Some(crate::parse::Definitions {
                        own: &[],
                        known: &[],
                    }),
                    &mut diagnostics,
                )
                .iter()
                .filter(|inline| matches!(inline, Inline::Spoiler(_)))
                .count();
                let cell = &row[start..end];
                assert_eq!(formed, predicted, "row {row:?}, cell {cell:?}");
                start = end + 1;
            }
        }
    }
}
