//! CJK characters and variation selectors for CJK-friendly emphasis flanking.
//!
//! The CommonMark CJK-friendly amendments
//! (<https://github.com/tats-u/markdown-cjk-friendly>) define a CJK character
//! by East Asian Width, emoji presentation, and the Hangul script. The set is
//! copied from the amendments' `ranges.md` at commit `5b17173` (Unicode 18),
//! which upstream prints with `node --run print-ranges`; a new Unicode version
//! is taken from a newer `ranges.md` the same way. `no_std`, no allocation, no
//! dependency.

/// Inclusive code-point ranges of CJK characters, sorted ascending and
/// non-overlapping.
static CJK_RANGES: [(char, char); 64] = [
    ('\u{1100}', '\u{11FF}'),
    ('\u{20A9}', '\u{20A9}'),
    ('\u{2329}', '\u{232A}'),
    ('\u{2630}', '\u{2637}'),
    ('\u{268A}', '\u{268F}'),
    ('\u{2E80}', '\u{2E99}'),
    ('\u{2E9B}', '\u{2EF3}'),
    ('\u{2F00}', '\u{2FD5}'),
    ('\u{2FF0}', '\u{303E}'),
    ('\u{3041}', '\u{3096}'),
    ('\u{3099}', '\u{30FF}'),
    ('\u{3105}', '\u{312F}'),
    ('\u{3131}', '\u{318E}'),
    ('\u{3190}', '\u{31E5}'),
    ('\u{31EF}', '\u{321E}'),
    ('\u{3220}', '\u{3247}'),
    ('\u{3250}', '\u{A48C}'),
    ('\u{A490}', '\u{A4C6}'),
    ('\u{A960}', '\u{A97C}'),
    ('\u{AC00}', '\u{D7A3}'),
    ('\u{D7B0}', '\u{D7C6}'),
    ('\u{D7CB}', '\u{D7FB}'),
    ('\u{F900}', '\u{FAFF}'),
    ('\u{FE10}', '\u{FE19}'),
    ('\u{FE30}', '\u{FE52}'),
    ('\u{FE54}', '\u{FE66}'),
    ('\u{FE68}', '\u{FE6B}'),
    ('\u{FF01}', '\u{FFBE}'),
    ('\u{FFC2}', '\u{FFC7}'),
    ('\u{FFCA}', '\u{FFCF}'),
    ('\u{FFD2}', '\u{FFD7}'),
    ('\u{FFDA}', '\u{FFDC}'),
    ('\u{FFE0}', '\u{FFE6}'),
    ('\u{FFE8}', '\u{FFEE}'),
    ('\u{16FE0}', '\u{16FE4}'),
    ('\u{16FF0}', '\u{16FF6}'),
    ('\u{17000}', '\u{18CDA}'),
    ('\u{18CFF}', '\u{18D20}'),
    ('\u{18D80}', '\u{18DF2}'),
    ('\u{18E00}', '\u{19191}'),
    ('\u{191A0}', '\u{191D2}'),
    ('\u{1AFF0}', '\u{1AFF3}'),
    ('\u{1AFF5}', '\u{1AFFB}'),
    ('\u{1AFFD}', '\u{1AFFE}'),
    ('\u{1B000}', '\u{1B128}'),
    ('\u{1B132}', '\u{1B132}'),
    ('\u{1B150}', '\u{1B152}'),
    ('\u{1B155}', '\u{1B155}'),
    ('\u{1B164}', '\u{1B168}'),
    ('\u{1B170}', '\u{1B2FB}'),
    ('\u{1D300}', '\u{1D356}'),
    ('\u{1D360}', '\u{1D376}'),
    ('\u{1F1AE}', '\u{1F1AE}'),
    ('\u{1F200}', '\u{1F200}'),
    ('\u{1F202}', '\u{1F202}'),
    ('\u{1F210}', '\u{1F219}'),
    ('\u{1F21B}', '\u{1F22E}'),
    ('\u{1F230}', '\u{1F231}'),
    ('\u{1F237}', '\u{1F237}'),
    ('\u{1F23B}', '\u{1F23B}'),
    ('\u{1F240}', '\u{1F248}'),
    ('\u{1F260}', '\u{1F265}'),
    ('\u{1F7DA}', '\u{1F7DA}'),
    ('\u{20000}', '\u{3FFFD}'),
];

/// Whether `value` is a CJK character as the amendments define it.
pub(crate) fn is_cjk(value: char) -> bool {
    value >= '\u{1100}'
        && CJK_RANGES
            .binary_search_by(|&(start, end)| {
                if value < start {
                    core::cmp::Ordering::Greater
                } else if value > end {
                    core::cmp::Ordering::Less
                } else {
                    core::cmp::Ordering::Equal
                }
            })
            .is_ok()
}

/// A non-emoji general-use variation selector: U+FE00–U+FE0E, the Variation
/// Selectors block without the emoji presentation selector U+FE0F.
pub(crate) fn is_general_use_variation_selector(value: char) -> bool {
    ('\u{FE00}'..='\u{FE0E}').contains(&value)
}

/// An ideographic variation selector: the Variation Selectors Supplement block.
pub(crate) fn is_ideographic_variation_selector(value: char) -> bool {
    ('\u{E0100}'..='\u{E01EF}').contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_sorted_and_disjoint() {
        assert!(CJK_RANGES.windows(2).all(|pair| pair[0].1 < pair[1].0));
        assert!(CJK_RANGES.iter().all(|&(start, end)| start <= end));
    }

    #[test]
    fn cjk_edges() {
        for cjk in [
            '\u{1100}',
            '\u{11FF}',
            '重',
            '。',
            '」',
            '！',
            '가',
            '\u{3FFFD}',
            '\u{1F202}',
        ] {
            assert!(is_cjk(cjk), "{cjk:?}");
        }
        for other in [
            'a',
            '.',
            '\u{10FF}',
            '\u{1200}',
            '\u{3040}',
            '\u{1F600}',
            '\u{3FFFE}',
            '“',
        ] {
            assert!(!is_cjk(other), "{other:?}");
        }
    }

    #[test]
    fn variation_selectors() {
        assert!(is_general_use_variation_selector('\u{FE00}'));
        assert!(is_general_use_variation_selector('\u{FE0E}'));
        assert!(!is_general_use_variation_selector('\u{FE0F}'));
        assert!(is_ideographic_variation_selector('\u{E0100}'));
        assert!(is_ideographic_variation_selector('\u{E01EF}'));
        assert!(!is_ideographic_variation_selector('\u{E01F0}'));
    }
}
