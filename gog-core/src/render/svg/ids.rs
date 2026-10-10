//! The drawing's names: its accessible name, and the ids that keep many drawings
//! on one page apart.

use super::*;

/// A plot's **accessible name**, the words a screen reader says for the image: its
/// `title()`, or with none, what it draws and what its positions read, in the book's
/// read-aloud words ("Bars derived by count, x is continent"). A written
/// axis name stands for its column there, as it does under the axis.
///
/// Every plot was an anonymous image until 2026-09-24, in the book and in every
/// binding. It rides on an `aria-label` rather than a `<title>` element, because a
/// browser shows a `<title>` as a tooltip whenever the pointer rests on the plot, and
/// a name for a listener should change nothing a reader sees.
pub(super) fn accessible_name(spec: &PlotSpec) -> String {
    if let Some(title) = spec.title.as_deref().filter(|t| !t.trim().is_empty()) {
        return title.to_string();
    }
    let noun = |m: &Mark| match m {
        Mark::Point => "points", Mark::Line => "lines", Mark::Area => "areas",
        Mark::Bar => "bars", Mark::Step => "steps", Mark::Interval => "intervals",
        Mark::Box => "boxes", Mark::Ribbon => "ribbons", Mark::Text => "text",
        Mark::Path => "paths", Mark::Rule => "rules", Mark::Zone => "zones",
        Mark::Surface => "a surface", Mark::Edge => "edges",
    };
    // Each layer as the book reads it aloud: "bars derived by sum and stack". A
    // `box` carries its own summary and says only "boxes". The transforms are named
    // in the order they run, whatever order they were written in (a cut, then what
    // measures, then what rescales, then what moves), which is the order the book
    // asks a reader to write them in, and it keeps the name, like the picture, the
    // same for every spelling of one chain.
    let runs = |t: &Transform| {
        let j = crate::transform::jobs(t, crate::transform::JobContext::default());
        [j.extent, j.measure, j.scale, j.position].iter().position(|b| *b).unwrap_or(4)
    };
    let mut marks: Vec<String> = Vec::new();
    for layer in &spec.layers {
        let mut ordered: Vec<&Transform> = layer.transforms.iter()
            .filter(|t| **t != Transform::Box)
            .collect();
        ordered.sort_by_key(|t| runs(t));
        let derived: Vec<&str> = ordered.into_iter()
            .map(crate::legality::transform_name)
            .collect();
        let n = match derived.is_empty() {
            true => noun(&layer.mark).to_string(),
            false => format!("{} derived by {}", noun(&layer.mark), derived.join(" and ")),
        };
        if !marks.contains(&n) {
            marks.push(n);
        }
    }
    let bound = |ch: Channel| -> Option<String> {
        spec.axis_def(&ch).map(|d| d.field.clone())
            .or_else(|| spec.channels.get(&ch).map(|d| d.field.clone()))
            .or_else(|| spec.layers.iter().find_map(|l| l.encodings.get(&ch).map(|d| d.field.clone())))
    };
    let mut parts = vec![marks.join(" and ")];
    for (c, ch, written) in [
        ("x", Channel::X, &spec.x_axis.label),
        ("y", Channel::Y, &spec.y_axis.label),
        ("z", Channel::Z, &spec.z_axis.label),
    ] {
        let name = written.clone().filter(|w| !w.trim().is_empty()).or_else(|| bound(ch));
        if let Some(n) = name {
            parts.push(format!("{c} is {n}"));
        }
    }
    if let Some(c) = bound(Channel::Color) {
        parts.push(format!("color by {c}"));
    }
    let text = parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::from("A plot"),
    }
}

/// Give every id this render minted a prefix of its own.
///
/// **An id is resolved against the whole document, and a plot is not the whole
/// document.** Ids here are hashed from the *element's* content, on the
/// reasoning that two elements sharing an id are byte-identical and so cannot
/// mislead. That is true of a `clipPath` — a dangling `clip-path` simply does
/// not clip — and it is false of a `<pattern>`, whose dangling
/// `fill="url(#…)"` falls back to **no fill** and empties the shape.
///
/// A notebook is where this bites: every plot is one document, so two plots
/// using the same texture in the same color are handed the same id and the
/// second borrows the first's definition. It draws correctly until a host
/// detaches the owning cell — JupyterLab's default windowing does exactly that
/// to anything scrolled out of view — and then a correct plot renders as
/// nothing. Latent since patterns were built; it could not show in a suite that
/// writes one picture per file.
///
/// **The prefix is hashed from the finished picture**, so it is deterministic
/// (four bindings must agree byte for byte) and two renders share a prefix only
/// when the whole SVG is identical — in which case either copy serves, and
/// whichever survives in the document is the right one.
/// The SVG without its accessible name, which reads the plot aloud and draws
/// nothing: `namespace_ids` hashes ink, so the name stays out of the hash, and
/// naming a plot moves no id and no byte of what it draws. The name's value is
/// escaped, so its closing quote is the first one after it.
pub(super) fn without_accessible_name(svg: &str) -> String {
    const NAME: &str = r#" role="img" aria-label=""#;
    let Some(at) = svg.find(NAME) else { return svg.to_string() };
    let rest = &svg[at + NAME.len()..];
    match rest.find('"') {
        Some(end) => format!("{}{}", &svg[..at], &rest[end + 1..]),
        None => svg.to_string(),
    }
}

pub(super) fn namespace_ids(svg: &str) -> String {
    // Nothing to rename, and the common case: no defs, no references.
    if !svg.contains("id=\"") {
        return svg.to_string();
    }
    // **Hashed from the ink, not from the file.** Anything that draws nothing
    // must not move the prefix, because moving it moves every id and so every
    // byte — and two invariants in this module say exactly that. A brush nobody
    // has dragged adds panel metadata and not one mark
    // (`a_resting_brush_draws_exactly_the_same_ink`), and two moments of one
    // played plot differ only in which is shown
    // (`two_stills_of_one_plot_differ_only_in_which_moment_shows`). Both would
    // fail if the attribute they turn on reached this hash.
    //
    // So this list *is* the definition of "draws nothing", and an attribute of
    // that kind added later belongs in it. A flow's slot outlines, written beside
    // its slots for the page's click, are the second such line.
    let ink = without_accessible_name(svg)
        .replace(r#" display="inline""#, "")
        .replace(r#" display="none""#, "");
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for line in ink.lines().filter(|l| !l.contains("data-gog-panel") && !l.contains("data-gog-slot")) {
        for b in line.bytes() {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        h = (h ^ 0x0a).wrapping_mul(0x0100_0000_01b3);
    }
    let prefix = format!("g{h:016x}-");
    // Only two forms reach the output, and a test pins that: `id="…"` where a
    // definition is minted, and `url(#…)` where one is used.
    svg.replace("id=\"", &format!("id=\"{prefix}"))
        .replace("url(#", &format!("url(#{prefix}"))
}

/// Every id the drawing minted, and every reference to one, with `salt` in front.
///
/// `namespace_ids` makes a drawing's ids its own among *different* drawings; this
/// makes them its own among copies of one drawing, which content alone cannot do.
/// The two forms it rewrites are the two `namespace_ids` writes, so a salted
/// drawing resolves every reference exactly as the unsalted one did. The salt is
/// reduced to letters, digits, `-` and `_`, the characters an id and a URL
/// fragment both take unescaped; one left with none of them salts nothing.
pub(crate) fn salt_ids(svg: &str, salt: &str) -> String {
    let salt: String = salt
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if salt.is_empty() || !svg.contains("id=\"") {
        return svg.to_string();
    }
    svg.replace("id=\"", &format!("id=\"{salt}-"))
        .replace("url(#", &format!("url(#{salt}-"))
}

/// A clip id derived from the rectangle it clips.
///
/// The book inlines many SVGs into one HTML page, where ids are global and the
/// first definition wins — the lesson the gradient legend's id taught. Deriving
/// the id from the geometry means two clips share an id only when they clip the
/// same rectangle, which is the one collision that cannot mislead. (The old
/// fixed `plot-clip` id had exactly that latent defect; faceting, whose panel
/// rectangles genuinely differ from plot to plot, is what made it real.)
pub(super) fn clip_id(l: &Layout) -> String {
    format!(
        "clip-{}-{}-{}-{}",
        (l.x0 * 10.0).round() as i64,
        (l.y0 * 10.0).round() as i64,
        (l.x1 * 10.0).round() as i64,
        (l.y1 * 10.0).round() as i64,
    )
}
