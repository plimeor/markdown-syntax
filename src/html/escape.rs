//! Escaping + URL-encoding helpers shared by the inline/block renderers.
//!
//! Inputs are already-decoded Rust strings; the renderer never re-decodes or
//! re-trims them. Every helper appends to the caller's output buffer.

use alloc::string::String;

/// CommonMark text escaper: replace, IN ORDER, `&`→`&amp;`, `<`→`&lt;`,
/// `>`→`&gt;`, `"`→`&quot;`. The single quote is left literal.
///
/// The escaped characters are ASCII, so the runs between them are copied whole.
pub fn escape_text(out: &mut String, s: &str) {
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = next_text_special(bytes, 0);
    while i < bytes.len() {
        let replacement = match bytes[i] {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            _ => "&quot;",
        };
        out.push_str(&s[start..i]);
        out.push_str(replacement);
        start = i + 1;
        i = next_text_special(bytes, start);
    }
    out.push_str(&s[start..]);
}

/// Index of the first `&`, `<`, `>`, or `"` at or after `from`, or
/// `bytes.len()`.
fn next_text_special(bytes: &[u8], from: usize) -> usize {
    crate::swar::find_any(bytes, from, [b'&', b'<', b'>', b'"'])
}

/// Attribute escaper for the CommonMark / link-title context: identical to
/// [`escape_text`] (`&` `<` `>` `"`), with `'` left literal because the
/// attribute is always emitted double-quoted.
pub fn attr_escape(out: &mut String, s: &str) {
    escape_text(out, s);
}

/// Attribute escaper for the GFM directive / wikilink context: same as
/// [`attr_escape`] but additionally maps `'`→`&#x27;`.
pub fn attr_escape_gfm(out: &mut String, s: &str) {
    let mut start = 0;
    for (i, &b) in s.as_bytes().iter().enumerate() {
        let replacement = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&#x27;",
            _ => continue,
        };
        out.push_str(&s[start..i]);
        out.push_str(replacement);
        start = i + 1;
    }
    out.push_str(&s[start..]);
}

/// True for the cmark "href-safe" byte set kept literal by the houdini
/// encoder: `A-Z a-z 0-9 ! # $ ' ( ) * + , - . / : ; = ? @ _ ~`.
fn is_href_safe(b: u8) -> bool {
    matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
        | b'!' | b'#' | b'$' | b'\'' | b'(' | b')' | b'*' | b'+'
        | b',' | b'-' | b'.' | b'/' | b':' | b';' | b'=' | b'?'
        | b'@' | b'_' | b'~')
}

fn is_ascii_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// cmark houdini href encoder over the UTF-8 bytes of the (already-decoded)
/// destination. `&`→`&amp;`; `%`+2hex preserved else `%25`; href-safe bytes
/// literal; everything else `%XX` uppercase. (See `misc_url.rs:141`.)
///
/// Runs of href-safe bytes are copied whole. They are ASCII, so a non-empty
/// run starts and ends on a char boundary.
pub fn encode_href(out: &mut String, s: &str) {
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if is_href_safe(b) {
            i += 1;
            continue;
        }
        if start < i {
            out.push_str(&s[start..i]);
        }
        if b == b'&' {
            out.push_str("&amp;");
        } else if b == b'%' {
            if i + 2 < bytes.len() && is_ascii_hex(bytes[i + 1]) && is_ascii_hex(bytes[i + 2]) {
                out.push('%');
            } else {
                out.push_str("%25");
            }
        } else {
            push_percent(out, b);
        }
        i += 1;
        start = i;
    }
    if start < i {
        out.push_str(&s[start..i]);
    }
}

fn push_percent(out: &mut String, b: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push('%');
    out.push(HEX[(b >> 4) as usize] as char);
    out.push(HEX[(b & 0x0f) as usize] as char);
}

/// ASCII scheme of `dest`: the chars before the first `:` that occurs before
/// any of `/ ? #`. `None` when there is no such scheme. Schemes compare
/// ASCII-case-insensitively.
fn url_scheme(dest: &str) -> Option<&str> {
    for (i, &b) in dest.as_bytes().iter().enumerate() {
        match b {
            b':' => return Some(&dest[..i]),
            b'/' | b'?' | b'#' => return None,
            _ => {}
        }
    }
    None
}

/// True when `scheme` is one of `names` (lowercase), ignoring ASCII case.
fn scheme_is(scheme: &str, names: &[&str]) -> bool {
    names.iter().any(|name| scheme.eq_ignore_ascii_case(name))
}

/// Link href scheme DENYLIST — cmark-gfm's `scan_dangerous_url` policy (used for
/// the GFM suite, which is cmark-gfm-derived): a URL is dropped only when its
/// scheme is one cmark-gfm treats as dangerous (`javascript`, `vbscript`,
/// `file`, `data`); the four safe `data:image/*` prefixes are carved out
/// separately by `is_allowed_data_image`. Every other scheme (`http`, `ftp`,
/// `smb`, `irc`, `rdar`, …, and the empty scheme from a leading `://`) is kept.
fn is_dangerous_scheme(scheme: &str) -> bool {
    scheme_is(scheme, &["javascript", "vbscript", "file", "data"])
}

/// Link href scheme ALLOWLIST — micromark's policy (used for the CommonMark
/// suite): a URL is kept only when its scheme is one of the safe set (or it is a
/// safe `data:image/*` URI). Unknown schemes (`made-up-scheme:`, `a+b+c:`) are
/// blanked unless `allow_dangerous_protocol` is set. The CommonMark and GFM
/// suites genuinely disagree here (cmark-gfm keeps `smb:`; micromark blanks
/// `made-up-scheme:`), so the policy is selected per suite category.
fn is_allowed_link_scheme(scheme: &str) -> bool {
    scheme_is(scheme, &["http", "https", "irc", "ircs", "mailto", "xmpp"])
}

/// Image src scheme allowlist (only http/https; irc/mailto are NOT allowed for
/// images).
fn is_allowed_img_scheme(scheme: &str) -> bool {
    scheme_is(scheme, &["http", "https"])
}

/// The four `data:image/*` URI prefixes the GFM sanitizer allows, ignoring
/// ASCII case.
fn is_allowed_data_image(dest: &str) -> bool {
    let dest = dest.as_bytes();
    [
        "data:image/png",
        "data:image/gif",
        "data:image/jpeg",
        "data:image/webp",
    ]
    .iter()
    .any(|prefix| {
        dest.len() >= prefix.len() && dest[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    })
}

/// Link href protocol filter: `dest`, or `""` when it is blanked.
/// `gfm_denylist` selects the suite's policy: the
/// GFM (cmark-gfm) suite blanks only dangerous schemes; the CommonMark
/// (micromark) suite blanks any non-allowlisted scheme. `allow_dangerous_protocol`
/// bypasses both (keeps everything). The two oracles genuinely differ (cmark-gfm
/// keeps `smb:`; micromark blanks `made-up-scheme:`), so this is category-keyed
/// alongside the other category-divergent oracle conventions.
pub fn filter_protocol(dest: &str, allow_dangerous_protocol: bool, gfm_denylist: bool) -> &str {
    if allow_dangerous_protocol {
        return dest;
    }
    match url_scheme(dest) {
        None => dest,
        Some(scheme) if gfm_denylist => {
            if is_dangerous_scheme(scheme) && !is_allowed_data_image(dest) {
                ""
            } else {
                dest
            }
        }
        Some(scheme) => {
            if is_allowed_link_scheme(scheme) || is_allowed_data_image(dest) {
                dest
            } else {
                ""
            }
        }
    }
}

/// Image src protocol filter (ALLOWLIST): `dest`, or `""` when it is blanked.
/// `allow_dangerous_protocol` and
/// `allow_any_img_src` both bypass the filter (mirroring the link href path and
/// upstream micromark `allowDangerousProtocol` semantics, which apply to image
/// src as well); otherwise a scheme-less destination passes, a schemed
/// destination passes only for http/https, and the four `data:image/*` URIs are
/// allowed.
pub fn filter_img_protocol(
    dest: &str,
    allow_dangerous_protocol: bool,
    allow_any_img_src: bool,
) -> &str {
    if allow_dangerous_protocol || allow_any_img_src {
        return dest;
    }
    match url_scheme(dest) {
        None => dest,
        Some(scheme) if is_allowed_img_scheme(scheme) => dest,
        Some(_) if is_allowed_data_image(dest) => dest,
        Some(_) => "",
    }
}
