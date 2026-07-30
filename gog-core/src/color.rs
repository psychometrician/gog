//! Color vocabulary — which colors exist, and whether a string names one.
//!
//! Deliberately knows nothing about plots, rules, or SVG. Two very different
//! callers need the same answers and must not disagree:
//!
//! - `legality` asks *is this a color?* to refuse `style(color = "stelblue")`
//!   before anything is drawn.
//! - `render::palette` asks *what color is this?* to interpolate ramp stops,
//!   because `palette(c("white", "navy"))` has to be mixed numerically.
//!
//! The second is why the table carries RGB values and not just names. It used
//! to live inside `legality`, which meant the renderer reached into the
//! rule-checker for a color value — a dependency pointing the wrong way.

/// The 148 CSS Color Level 4 named colors, with their RGB values.
///
/// The values are here, not just the names, because a ramp stop has to be
/// *interpolated*: `palette(c("white", "navy"))` needs numbers. Storing only
/// names would make color names work everywhere except as ramp stops, which
/// is the kind of exception the grammar refuses.
///
/// Sorted by name — `colors_are_sorted` guards the binary search.
pub const CSS_COLORS: &[(&str, u32)] = &[
    ("aliceblue", 0xF0F8FF), ("antiquewhite", 0xFAEBD7), ("aqua", 0x00FFFF),
    ("aquamarine", 0x7FFFD4), ("azure", 0xF0FFFF), ("beige", 0xF5F5DC),
    ("bisque", 0xFFE4C4), ("black", 0x000000), ("blanchedalmond", 0xFFEBCD),
    ("blue", 0x0000FF), ("blueviolet", 0x8A2BE2), ("brown", 0xA52A2A),
    ("burlywood", 0xDEB887), ("cadetblue", 0x5F9EA0), ("chartreuse", 0x7FFF00),
    ("chocolate", 0xD2691E), ("coral", 0xFF7F50), ("cornflowerblue", 0x6495ED),
    ("cornsilk", 0xFFF8DC), ("crimson", 0xDC143C), ("cyan", 0x00FFFF),
    ("darkblue", 0x00008B), ("darkcyan", 0x008B8B), ("darkgoldenrod", 0xB8860B),
    ("darkgray", 0xA9A9A9), ("darkgreen", 0x006400), ("darkgrey", 0xA9A9A9),
    ("darkkhaki", 0xBDB76B), ("darkmagenta", 0x8B008B), ("darkolivegreen", 0x556B2F),
    ("darkorange", 0xFF8C00), ("darkorchid", 0x9932CC), ("darkred", 0x8B0000),
    ("darksalmon", 0xE9967A), ("darkseagreen", 0x8FBC8F), ("darkslateblue", 0x483D8B),
    ("darkslategray", 0x2F4F4F), ("darkslategrey", 0x2F4F4F), ("darkturquoise", 0x00CED1),
    ("darkviolet", 0x9400D3), ("deeppink", 0xFF1493), ("deepskyblue", 0x00BFFF),
    ("dimgray", 0x696969), ("dimgrey", 0x696969), ("dodgerblue", 0x1E90FF),
    ("firebrick", 0xB22222), ("floralwhite", 0xFFFAF0), ("forestgreen", 0x228B22),
    ("fuchsia", 0xFF00FF), ("gainsboro", 0xDCDCDC), ("ghostwhite", 0xF8F8FF),
    ("gold", 0xFFD700), ("goldenrod", 0xDAA520), ("gray", 0xBEBEBE),
    ("green", 0x00FF00), ("greenyellow", 0xADFF2F), ("grey", 0xBEBEBE),
    ("honeydew", 0xF0FFF0), ("hotpink", 0xFF69B4), ("indianred", 0xCD5C5C),
    ("indigo", 0x4B0082), ("ivory", 0xFFFFF0), ("khaki", 0xF0E68C),
    ("lavender", 0xE6E6FA), ("lavenderblush", 0xFFF0F5), ("lawngreen", 0x7CFC00),
    ("lemonchiffon", 0xFFFACD), ("lightblue", 0xADD8E6), ("lightcoral", 0xF08080),
    ("lightcyan", 0xE0FFFF), ("lightgoldenrodyellow", 0xFAFAD2), ("lightgray", 0xD3D3D3),
    ("lightgreen", 0x90EE90), ("lightgrey", 0xD3D3D3), ("lightpink", 0xFFB6C1),
    ("lightsalmon", 0xFFA07A), ("lightseagreen", 0x20B2AA), ("lightskyblue", 0x87CEFA),
    ("lightslategray", 0x778899), ("lightslategrey", 0x778899), ("lightsteelblue", 0xB0C4DE),
    ("lightyellow", 0xFFFFE0), ("lime", 0x00FF00), ("limegreen", 0x32CD32),
    ("linen", 0xFAF0E6), ("magenta", 0xFF00FF), ("maroon", 0xB03060),
    ("mediumaquamarine", 0x66CDAA), ("mediumblue", 0x0000CD), ("mediumorchid", 0xBA55D3),
    ("mediumpurple", 0x9370DB), ("mediumseagreen", 0x3CB371), ("mediumslateblue", 0x7B68EE),
    ("mediumspringgreen", 0x00FA9A), ("mediumturquoise", 0x48D1CC), ("mediumvioletred", 0xC71585),
    ("midnightblue", 0x191970), ("mintcream", 0xF5FFFA), ("mistyrose", 0xFFE4E1),
    ("moccasin", 0xFFE4B5), ("navajowhite", 0xFFDEAD), ("navy", 0x000080),
    ("oldlace", 0xFDF5E6), ("olive", 0x808000), ("olivedrab", 0x6B8E23),
    ("orange", 0xFFA500), ("orangered", 0xFF4500), ("orchid", 0xDA70D6),
    ("palegoldenrod", 0xEEE8AA), ("palegreen", 0x98FB98), ("paleturquoise", 0xAFEEEE),
    ("palevioletred", 0xDB7093), ("papayawhip", 0xFFEFD5), ("peachpuff", 0xFFDAB9),
    ("peru", 0xCD853F), ("pink", 0xFFC0CB), ("plum", 0xDDA0DD),
    ("powderblue", 0xB0E0E6), ("purple", 0xA020F0), ("rebeccapurple", 0x663399),
    ("red", 0xFF0000), ("rosybrown", 0xBC8F8F), ("royalblue", 0x4169E1),
    ("saddlebrown", 0x8B4513), ("salmon", 0xFA8072), ("sandybrown", 0xF4A460),
    ("seagreen", 0x2E8B57), ("seashell", 0xFFF5EE), ("sienna", 0xA0522D),
    ("silver", 0xC0C0C0), ("skyblue", 0x87CEEB), ("slateblue", 0x6A5ACD),
    ("slategray", 0x708090), ("slategrey", 0x708090), ("snow", 0xFFFAFA),
    ("springgreen", 0x00FF7F), ("steelblue", 0x4682B4), ("tan", 0xD2B48C),
    ("teal", 0x008080), ("thistle", 0xD8BFD8), ("tomato", 0xFF6347),
    ("turquoise", 0x40E0D0), ("violet", 0xEE82EE), ("wheat", 0xF5DEB3),
    ("white", 0xFFFFFF), ("whitesmoke", 0xF5F5F5), ("yellow", 0xFFFF00),
    ("yellowgreen", 0x9ACD32),
];

/// Is `s` a color SVG will actually paint?
///
/// Accepts the three forms a user can reasonably write: a CSS color name, a
/// hex literal, or a functional notation. Anything else is a typo.
pub fn is_valid_color(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }

    if let Some(hex) = t.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }

    let lower = t.to_ascii_lowercase();
    if matches!(lower.as_str(), "none" | "transparent" | "currentcolor") {
        return true;
    }
    // rgb() / rgba() / hsl() / hsla() — the parenthesized forms. Checking the
    // head and the closing paren is enough to catch a typo without
    // reimplementing a CSS parser.
    for f in ["rgb(", "rgba(", "hsl(", "hsla("] {
        if lower.starts_with(f) && lower.ends_with(')') {
            return true;
        }
    }
    css_rgb(&lower).is_some()
}

/// The RGB value of a CSS color name, for interpolating ramp stops.
pub fn css_rgb(name: &str) -> Option<u32> {
    CSS_COLORS
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| CSS_COLORS[i].1)
}

/// The RGB behind any color a caller can write, or `None` when there is none to
/// have.
///
/// Names and hex resolve; `rgb()`/`hsl()` do not, because reading them would be
/// a CSS parser this crate has deliberately never grown, and `transparent`/`none`
/// have no color at all — they show whatever is behind them. Every caller of this
/// therefore has to have an answer for `None`, which is the point of returning it
/// rather than guessing a gray.
pub fn parse_rgb(s: &str) -> Option<u32> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix('#') {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        // The alpha forms drop their alpha: this answers *what color is it*, and
        // a translucent band's readability depends on what is behind it, which
        // nothing here can know.
        let rgb: String = match hex.len() {
            3 | 4 => hex[..3].chars().flat_map(|c| [c, c]).collect(),
            6 | 8 => hex[..6].to_string(),
            _ => return None,
        };
        return u32::from_str_radix(&rgb, 16).ok();
    }
    css_rgb(&t.to_ascii_lowercase())
}

/// Relative luminance, the WCAG definition — 0.0 for black, 1.0 for white.
fn relative_luminance(rgb: u32) -> f64 {
    let channel = |c: u32| {
        let v = c as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel((rgb >> 16) & 0xff)
        + 0.7152 * channel((rgb >> 8) & 0xff)
        + 0.0722 * channel(rgb & 0xff)
}

/// The WCAG contrast ratio between two colors, from 1.0 (identical) to 21.0.
fn contrast(a: u32, b: u32) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Which of two inks reads better on `background`, or `None` when the background
/// is not a color whose luminance can be computed.
///
/// **It picks rather than thresholds.** A cutoff ("is this darker than 50%?") is a
/// number someone has to defend at every edge; asking which of the two candidates
/// actually contrasts more is the same question with no constant in it, and it
/// stays right if either ink is ever changed.
///
/// Its one caller is the facet strip, where `theme(strip = "black")` would
/// otherwise paint the default near-black label on a near-black band and print a
/// panel name nobody can read — a silently empty guide, which §12 forbids.
pub fn better_ink<'a>(background: &str, a: &'a str, b: &'a str) -> Option<&'a str> {
    let bg = parse_rgb(background)?;
    let (ra, rb) = (parse_rgb(a)?, parse_rgb(b)?);
    Some(if contrast(bg, ra) >= contrast(bg, rb) { a } else { b })
}

/// Suggest the closest known color name, for a directional error message.
///
/// Deliberately conservative: a wrong suggestion is worse than none, so this
/// only fires on a near-miss (within two edits).
pub fn nearest_color(s: &str) -> Option<&'static str> {
    let lower = s.trim().to_ascii_lowercase();
    CSS_COLORS
        .iter()
        .map(|(c, _)| (*c, edit_distance(&lower, c)))
        .filter(|(c, d)| *d <= 2 && *d < c.len())
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}
/// Levenshtein distance, two-row variant.
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let sub = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + sub);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}
/// A numbered shade like `grey80` or `gray50`.
///
/// These are R color names, not CSS ones, and an R user reaches for them by
/// habit — this was the first thing to trip while writing the manual. gog keeps
/// the color vocabulary to CSS on purpose: it is what SVG actually accepts, and
/// it stays the same for the Python and Julia bindings. So the name is refused,
/// but the message has to explain *which* vocabulary is in force rather than
/// suggesting a plain `grey` that is nothing like the shade asked for.
pub fn numbered_shade(s: &str) -> Option<&'static str> {
    let lower = s.trim().to_ascii_lowercase();
    let stem = lower.trim_end_matches(|c: char| c.is_ascii_digit());
    if stem.len() == lower.len() || stem.is_empty() {
        return None;
    }
    CSS_COLORS.binary_search_by(|(n, _)| (*n).cmp(stem)).ok().map(|i| CSS_COLORS[i].0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_colors_cover_the_forms_a_user_would_write() {
        for good in [
            "red", "STEELBLUE", "#fff", "#4e79a7", "#4e79a7cc",
            "rgb(1,2,3)", "hsl(10, 50%, 50%)", "transparent",
        ] {
            assert!(is_valid_color(good), "{good} should be valid");
        }
        for bad in ["", "  ", "stelblue", "#12345", "#nothex", "reddish", "rgb(1,2,3"] {
            assert!(!is_valid_color(bad), "{bad} should be invalid");
        }
    }

    #[test]
    fn rgb_is_read_from_a_name_or_a_hex_and_from_nothing_else() {
        assert_eq!(parse_rgb("white"), Some(0xffffff));
        assert_eq!(parse_rgb("BLACK"), Some(0x000000));
        assert_eq!(parse_rgb("#4e79a7"), Some(0x4e79a7));
        assert_eq!(parse_rgb("#fff"), Some(0xffffff), "the short form expands");
        assert_eq!(parse_rgb("#4e79a7cc"), Some(0x4e79a7), "alpha is dropped");
        // The forms with no color to read. Every one of these has to return None
        // rather than a guessed gray, because the caller's fallback is the
        // *right* answer for them: a transparent band shows the page.
        for none in ["transparent", "none", "rgb(1,2,3)", "hsl(10, 50%, 50%)", "nosuch"] {
            assert_eq!(parse_rgb(none), None, "{none} has no RGB to read");
        }
    }

    #[test]
    fn the_ink_that_reads_is_chosen_rather_than_thresholded() {
        const DARK: &str = "#3c3c46";
        const LIGHT: &str = "#ffffff";

        // The two that must not move: today's band and the journal preset's.
        assert_eq!(better_ink("#e4e4ec", DARK, LIGHT), Some(DARK));
        assert_eq!(better_ink("white", DARK, LIGHT), Some(DARK));

        // The case this exists for — `theme(strip = "black")` must not print a
        // near-black label on a near-black band.
        assert_eq!(better_ink("black", DARK, LIGHT), Some(LIGHT));
        assert_eq!(better_ink("navy", DARK, LIGHT), Some(LIGHT));
        assert_eq!(better_ink("#1a1a2e", DARK, LIGHT), Some(LIGHT));

        // No luminance to read → no opinion, and the caller keeps its default.
        assert_eq!(better_ink("transparent", DARK, LIGHT), None);
    }

    #[test]
    fn colors_are_sorted() {
        // `is_valid_color` binary-searches this table.
        let names: Vec<&str> = CSS_COLORS.iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert_eq!(CSS_COLORS.len(), 148);
    }
}
