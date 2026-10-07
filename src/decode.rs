//! Decoding of the two source forms CommonMark reads as other characters:
//! backslash escapes and character references. The parser decodes link
//! destinations, titles, info strings, and attribute values with it; the
//! public decoding methods on AST nodes and the HTML renderer use it too.

use alloc::string::String;

use crate::entities::named_character_reference;

/// `input` with each backslash escape of an ASCII punctuation character
/// replaced by that character, and each character reference replaced by the
/// text it names. A backslash before any other character stays as written.
pub(crate) fn decode_escapes_and_references(input: &str) -> String {
    decode_selected_escapes_and_references(input, |char| char.is_ascii_punctuation())
}

/// `input` with each character reference decoded, and each backslash escape
/// of a character `escapable` accepts replaced by that character. A backslash
/// before any other character stays as written, and that character is not
/// read as the start of a reference.
pub(crate) fn decode_selected_escapes_and_references(
    input: &str,
    escapable: impl Fn(char) -> bool,
) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(char) = input[cursor..].chars().next() {
        if char == '&' {
            if let Some((end, value)) = parse_character_reference(input, cursor) {
                output.push_str(&value);
                cursor = end;
                continue;
            }
        }
        cursor += char.len_utf8();
        if char == '\\' {
            if let Some(escaped) = input[cursor..].chars().next() {
                if !escapable(escaped) {
                    output.push(char);
                }
                output.push(escaped);
                cursor += escaped.len_utf8();
                continue;
            }
        }
        output.push(char);
    }
    output
}

/// The character reference starting at the `&` at `index`: the byte offset
/// just past its `;` and the text it names, or `None` when no valid reference
/// starts there.
/// The character `reference` decodes to, when it is exactly one character
/// reference.
pub(crate) fn decode_character_reference(reference: &str) -> Option<String> {
    parse_character_reference(reference, 0)
        .filter(|(end, _)| *end == reference.len())
        .map(|(_, value)| value)
}

pub(crate) fn parse_character_reference(input: &str, index: usize) -> Option<(usize, String)> {
    let rest = input.get(index..)?;
    if let Some(rest) = rest
        .strip_prefix("&#x")
        .or_else(|| rest.strip_prefix("&#X"))
    {
        let digits = find_reference_terminator(rest, 6)?;
        if digits == 0 || !rest[..digits].bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(&rest[..digits], 16).ok()?;
        return Some((
            index + 3 + digits + 1,
            character_reference_value(value).into(),
        ));
    }
    if let Some(rest) = rest.strip_prefix("&#") {
        let digits = find_reference_terminator(rest, 7)?;
        if digits == 0 || !rest[..digits].bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let value = rest[..digits].parse::<u32>().ok()?;
        return Some((
            index + 2 + digits + 1,
            character_reference_value(value).into(),
        ));
    }

    let name_end = find_reference_terminator(rest, 32)?;
    if name_end == 0 {
        return None;
    }
    let name = &rest[1..name_end];
    named_character_reference(name).map(|value| (index + name_end + 1, value.into()))
}

/// Finds the `;` that ends a character reference body, looking no further than
/// `max_len` bytes: a longer body is never a reference, so the scan stays
/// bounded instead of running to the end of the input.
fn find_reference_terminator(rest: &str, max_len: usize) -> Option<usize> {
    rest.as_bytes()
        .iter()
        .take(max_len + 1)
        .position(|byte| *byte == b';')
}

/// Decode a numeric character reference codepoint to its scalar value.
///
/// This follows the CommonMark reference behavior: `U+0000`, the UTF-16
/// surrogate range, and codepoints beyond the Unicode scalar range decode to
/// `U+FFFD`; every other codepoint decodes to itself.
///
/// Two deliberate non-behaviors:
/// - We do NOT apply the HTML5 Windows-1252 remapping of C1 bytes; `&#128;`
///   decodes to `U+0080`, not the Euro sign. The CommonMark reference does not
///   perform that remapping.
/// - We do NOT extend replacement to the C0/C1 controls, DEL, or the Unicode
///   noncharacters the way some HTML-oriented decoders do. Keeping those as
///   their literal scalar is what makes the serializer's `&#xNN;` escaping of
///   control characters round-trip through a re-parse. The roundtrip corpus
///   only pins `{0 -> FFFD, 9 -> tab, 10 -> line feed, surrogate -> FFFD,
///   out-of-range -> FFFD}`, all of which this matches.
pub(crate) fn character_reference_value(value: u32) -> char {
    if value == 0 {
        '\u{FFFD}'
    } else {
        char::from_u32(value).unwrap_or('\u{FFFD}')
    }
}
