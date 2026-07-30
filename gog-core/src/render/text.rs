//! Text: escaping, and enough metrics to reserve space for it.
//!
//! SVG lays text out itself, so nothing here has to be exact — it only has to
//! be good enough to size a margin. Two bugs live in this file's history and
//! both are guarded by tests: user data was once interpolated raw (a category
//! called `R&D` produced SVG that no XML parser would accept), and width was
//! measured in *bytes*, which counted every Hangul syllable three times over.

// ---------------------------------------------------------------------------
// Text width estimation
//
// SVG text is rendered by the viewer, so we estimate width for layout only.
// A proportional sans-serif character is roughly 0.58 × font-size wide on
// average. Good enough for margin computation.
// ---------------------------------------------------------------------------

/// Escape a string for embedding in SVG/XML.
///
/// Escapes all five predefined XML entities, which makes the result safe in both
/// character data and double- or single-quoted attribute values. Category names
/// come straight from user data — `R&D` or `<10%` would otherwise produce a
/// document that no parser will accept.
pub(crate) fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Approximate advance width of one character, in em units.
///
/// CJK, Hangul, and emoji are rendered fullwidth — roughly one em — while
/// proportional Latin averages about 0.58 em. Measuring in *bytes* (the previous
/// behavior) counted every Hangul syllable three times over and inflated the
/// margins accordingly.
pub(crate) fn char_width_em(c: char) -> f64 {
    let cp = c as u32;
    let fullwidth = matches!(cp,
        0x1100..=0x115F |   // Hangul Jamo
        0x2E80..=0x303E |   // CJK radicals, Kangxi, CJK symbols & punctuation
        0x3041..=0x33FF |   // Hiragana, Katakana, Bopomofo, Hangul compat jamo
        0x3400..=0x4DBF |   // CJK unified ideographs extension A
        0x4E00..=0x9FFF |   // CJK unified ideographs
        0xA000..=0xA4CF |   // Yi
        0xAC00..=0xD7A3 |   // Hangul syllables
        0xF900..=0xFAFF |   // CJK compatibility ideographs
        0xFE30..=0xFE6F |   // CJK compatibility forms
        0xFF00..=0xFF60 |   // Fullwidth forms
        0xFFE0..=0xFFE6 |
        0x1F300..=0x1F64F | // emoji
        0x1F900..=0x1F9FF |
        0x20000..=0x2FFFD | // CJK extension B and beyond
        0x30000..=0x3FFFD
    );
    if fullwidth { 1.0 } else { 0.58 }
}

/// Estimate rendered text width. SVG text is laid out by the viewer, so this
/// only has to be good enough to reserve margin space.
pub(crate) fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    text.chars().map(char_width_em).sum::<f64>() * font_size
}

pub(crate) fn estimate_cap_height(font_size: f64) -> f64 {
    font_size * 0.72
}

