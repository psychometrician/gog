//! A page of composed plots: the checks across its cells, on the axes and facets
//! they share.

use super::*;

/// Check a figure — one plot, or a page of them (spec §11).
///
/// A page is checked plot by plot, because that is what a page *is*: separate
/// plots, each grammatical on its own terms. What is left for this level is the
/// one thing no plot can see from inside itself — whether the sizes the plots
/// asked for fit on the page they were arranged onto.
pub fn check_figure(figure: &Figure, data: &HashMap<String, DataFrame>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for spec in figure.plots() {
        out.extend(check(spec, data));
    }
    check_page_fits(&mut out, figure, crate::render::svg::CANVAS, None);
    check_page_orders(&mut out, figure, data);
    check_page_axes(&mut out, figure);
    check_page_facets(&mut out, figure);
    check_page_formats(&mut out, figure);
    out
}

/// One column on one axis of a page is one axis, so two plots cannot say opposite
/// things about whether it is drawn.
///
/// Asked only of plots that share the axis, which name the column and read it the
/// same way ([`AxisReading`]). The page
/// draws a shared axis once, by the plot nearest the edge it lives on, so one plot
/// saying `axis = FALSE` leaves it out for all of them (`render::page::share`), as
/// one binding does inside a plot. A plot saying `axis = TRUE` beside it is the
/// contradiction [`check_axis`] refuses inside one plot, and which of the two won
/// would depend on where the plots sat. Only flat plots share an axis this way.
pub(super) fn check_page_axes(out: &mut Vec<Diagnostic>, figure: &Figure) {
    if !figure.is_page() {
        return;
    }
    // What a plot wrote about one axis: `Some(false)` if any binding leaves it out,
    // `Some(true)` if one draws it and none leaves it out, `None` if none said.
    let stated = |spec: &PlotSpec, ch: &Channel| -> Option<bool> {
        let said: Vec<bool> = spec.position(ch).into_iter()
            .chain(spec.channels.get(ch))
            .chain(spec.layers.iter().filter_map(|l| l.encodings.get(ch)))
            .filter_map(|d| d.axis)
            .collect();
        if said.contains(&false) { Some(false) } else if said.contains(&true) { Some(true) } else { None }
    };
    let mut seen: Vec<(Channel, String, AxisReading, bool)> = Vec::new();
    let mut said: Vec<(Channel, String, AxisReading)> = Vec::new();
    for spec in figure.plots() {
        if space_of(spec) != SpaceKind::Flat {
            continue;
        }
        for ch in [Channel::X, Channel::Y] {
            let Some(def) = spec.axis_def(&ch) else { continue };
            let Some(mine) = stated(spec, &ch) else { continue };
            let reading = AxisReading::of(def);
            let Some((_, _, _, first)) = seen.iter()
                .find(|(c, f, r, _)| *c == ch && *f == def.field && *r == reading) else {
                seen.push((ch, def.field.clone(), reading, mine));
                continue;
            };
            let key = (ch.clone(), def.field.clone(), reading);
            if *first == mine || said.contains(&key) {
                continue;
            }
            said.push(key);
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{f}` is on the {c} axis of two plots in this arrangement, and one leaves \
                     the axis out (its `axis` set to false) while the other draws it (set to \
                     true). An arrangement draws a shared axis once, so the two cannot both hold. Keep one of \
                     them, or give the column another name in one of the plots.",
                    f = def.field, c = channel_name(&ch),
                ),
            });
        }
    }
}

/// **A shared axis writes its numbers one way** (2026-10-15). The page draws a
/// shared axis once, by the plot nearest its edge (`render::page::share`), with
/// that plot's `decimals`, `short` and `percent`; a different statement in the
/// other plot would be left out without a word. So the plots sharing an axis
/// state the same, or none of them states any.
pub(super) fn check_page_formats(out: &mut Vec<Diagnostic>, figure: &Figure) {
    if !figure.is_page() {
        return;
    }
    type Format = (Option<u32>, bool, bool);
    let mut seen: Vec<(Channel, String, AxisReading, Format)> = Vec::new();
    let mut said: Vec<(Channel, String, AxisReading)> = Vec::new();
    for spec in figure.plots() {
        if space_of(spec) != SpaceKind::Flat {
            continue;
        }
        for ch in [Channel::X, Channel::Y] {
            let Some(def) = spec.axis_def(&ch) else { continue };
            let mine: Format = (def.decimals, def.short == Some(true), def.percent == Some(true));
            let reading = AxisReading::of(def);
            let Some((_, _, _, first)) = seen.iter()
                .find(|(c, f, r, _)| *c == ch && *f == def.field && *r == reading) else {
                seen.push((ch, def.field.clone(), reading, mine));
                continue;
            };
            let key = (ch.clone(), def.field.clone(), reading);
            if *first == mine || said.contains(&key) {
                continue;
            }
            said.push(key);
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{f}` is on the {c} axis of two plots in this arrangement, and they \
                     write its numbers differently (`decimals`, `short` or `percent`). An \
                     arrangement draws a shared axis once, so state the same on both plots, \
                     or give the column another name in one of them.",
                    f = def.field, c = channel_name(&ch),
                ),
            });
        }
    }
}

/// A shared axis lines up across a page, so the plots sharing it have to be
/// split into panels the same way along it.
///
/// The page shares an axis by column and lines it up by each plot's panel box,
/// and a faceted plot's box is its whole grid. So a histogram stacked over a
/// scatter faceted by continent spanned all five panels, lost its tick labels and
/// lined up with none of them, in silence; a wrapped facet under a plot did the
/// same with a third of the page left empty. Refused, and pointed at the sentence
/// that draws: both plots faceted by the same column, which lines up one panel
/// over each. Along `x` the plots must agree on their panel columns (a column
/// facet and its wrap); along `y` on their panel rows (a row facet, or a column
/// facet that wraps into rows).
pub(super) fn check_page_facets(out: &mut Vec<Diagnostic>, figure: &Figure) {
    if !figure.is_page() {
        return;
    }
    type Split = Option<(String, Option<usize>)>;
    let along = |spec: &PlotSpec, ch: &Channel| -> Split {
        let f = spec.facet.as_ref()?;
        match ch {
            Channel::X => f.col.clone().map(|c| (c, f.wrap)),
            _ => match (&f.row, &f.col, f.wrap) {
                (Some(r), _, _) => Some((r.clone(), None)),
                (None, Some(c), Some(w)) => Some((c.clone(), Some(w))),
                _ => None,
            },
        }
    };
    let describe = |split: &Split| -> String {
        match split {
            None => "not split into panels".to_string(),
            Some((c, None)) => format!("split into panels by `{c}`"),
            Some((c, Some(w))) => format!("split into panels by `{c}`, wrapped at {w}"),
        }
    };
    let mut seen: Vec<(Channel, String, AxisReading, Split)> = Vec::new();
    let mut said: Vec<(Channel, String, AxisReading)> = Vec::new();
    for spec in figure.plots() {
        if space_of(spec) != SpaceKind::Flat {
            continue;
        }
        for ch in [Channel::X, Channel::Y] {
            let Some(def) = spec.axis_def(&ch) else { continue };
            let split = along(spec, &ch);
            let reading = AxisReading::of(def);
            let Some((_, _, _, first)) = seen.iter()
                .find(|(c, f, r, _)| *c == ch && *f == def.field && *r == reading) else {
                seen.push((ch, def.field.clone(), reading, split));
                continue;
            };
            let key = (ch.clone(), def.field.clone(), reading);
            if *first == split || said.contains(&key) {
                continue;
            }
            said.push(key);
            let by = first.as_ref().or(split.as_ref()).map_or("<column>".to_string(), |(c, _)| c.clone());
            let each = match ch {
                Channel::X => format!("`| facet({by})`"),
                _ => format!("`/ facet({by})`"),
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{f}` is on the {c} axis of two plots in this arrangement, and they are \
                     split into panels differently: one is {a}, the other {b}. An arrangement \
                     lines a shared axis up across its plots, and one panel cannot line up with a \
                     row of them. Split both plots the same way, with {each} on each, or give \
                     the column another name in one of them.",
                    f = def.field, c = channel_name(&ch), a = describe(first), b = describe(&split),
                ),
            });
        }
    }
}

/// How one plot reads a column on an axis: the stated options that change where a
/// value lands. Two flat plots on a page share an axis when they name the same
/// column on it **and** read it the same way (`render::page::groups`), so this is
/// the second half of the test the whole of composition rests on.
///
/// The page once shared by name alone. Two plots that read `gdp` differently were
/// then refused, because sharing them had drawn one axis from 10⁵⁸³¹ with every
/// point at its left edge, in silence. Ruled on 2026-10-08: the reading the
/// sentence states decides instead. A linear histogram of `gdp` above or beside a
/// log scatter of `gdp` is the usual way to show what a log scale does, and both
/// readings are written out, so each plot keeps its own scale and draws its own
/// axis. Compared: the scale (log, with its base, or category, against the
/// column's own reading) and the limits. `linear` and `time` stated outright are
/// the column's own reading, so they match a binding that says nothing.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct AxisReading {
    log_base: Option<f64>,
    category: bool,
    limits: Option<[Option<f64>; 2]>,
}

impl AxisReading {
    pub(crate) fn of(def: &ChannelDef) -> Self {
        AxisReading {
            log_base: (def.scale == Some(ScaleType::Log)).then(|| def.base.unwrap_or(10.0)),
            category: def.scale == Some(ScaleType::Category),
            limits: def.limits.filter(|l| l[0].is_some() || l[1].is_some()),
        }
    }
}

/// Two composed panels each deriving an order for one shared categorical axis,
/// and disagreeing — one axis cannot hold two orders, and quietly seating one
/// panel's slots in the other's sequence would misplace every mark of the
/// loser. Same inputs agree by determinism, so this fires only on a genuine
/// contradiction. Checked here, where a refusal can still refuse; the page
/// itself, reached only under `GOG_STRICT=0`, falls back to own slots.
pub(super) fn check_page_orders(
    out: &mut Vec<Diagnostic>, figure: &Figure, data: &HashMap<String, DataFrame>,
) {
    if !figure.is_page() {
        return;
    }
    // (channel, column) → the distinct leaf orders derived for it.
    let mut derived: Vec<(Channel, String, Vec<Vec<String>>)> = Vec::new();
    for spec in figure.plots() {
        let resolved = resolve_scopes(spec);
        for layer in &resolved.layers {
            if !layer.transforms.contains(&Transform::Cluster) {
                continue;
            }
            let cl = layer.cluster.clone().unwrap_or_default();
            let over = cl.over.clone();
            let Some(df) = layer.data.as_ref().or(resolved.data.as_ref())
                .and_then(|n| data.get(n)) else { continue };
            for ch in [Channel::X, Channel::Y] {
                let Some(def) = resolved.position_for(layer, &ch) else { continue };
                let leaf = def.field.clone();
                if over.as_deref() == Some(leaf.as_str()) {
                    continue;
                }
                let value = cl.value.clone()
                    .or_else(|| layer.encodings.get(&Channel::Color)
                        .map(|d| d.field.clone()))
                    .unwrap_or_default();
                let Some(order) = crate::transform::cluster_order(
                    df, &leaf, &value, over.as_deref()) else { continue };
                match derived.iter_mut().find(|(c, f, _)| *c == ch && *f == leaf) {
                    Some((_, _, orders)) => {
                        if !orders.contains(&order) {
                            orders.push(order);
                        }
                    }
                    None => derived.push((ch, leaf, vec![order])),
                }
            }
        }
    }
    for (ch, field, orders) in derived {
        if orders.len() > 1 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: two composed panels each derive an order for `{field}` on \
                     {ax} and they disagree — one axis cannot hold two orders. \
                     Cluster it in one panel, or give both the same value and \
                     profile columns.",
                    ax = channel_name(&ch),
                ),
            });
        }
    }
}

/// A page whose cells ask for more room than the page has.
///
/// Checked against the **whole** canvas rather than against each node's share of
/// it, deliberately: a nested page has less room than that, so this can only
/// ever miss an over-full page, never refuse one that fits. A refusal that fires
/// on a legal sentence is a much worse failure than a squeeze that draws.
///
/// A page that states its own size is measured against *that*, and hands it down
/// to the pages nested inside it. Otherwise the refusal would quote the canvas at
/// someone who had already said the figure was smaller, and name a number that
/// appears nowhere in what they wrote.
pub(super) fn check_page_fits(
    out: &mut Vec<Diagnostic>, figure: &Figure, canvas: (f64, f64), inherited: Option<f64>,
) {
    let Figure::Page(page) = figure else { return };
    // The gap this page's plots stand apart by: its own, else the page it sits on.
    let stated_gap = page.theme.gap.or(inherited);
    check_page_theme(out, page);

    // **An ask is pixels, and a page honors each one or refuses** (ruled
    // 2026-09-28). The page measures what it states, else what its cells give
    // it (`Figure::ask`, gaps included), else the canvas it was handed. So a page
    // whose plots all state a size is exactly as big as they are, and this check
    // is left with what a page cannot do: hold more than it has, leave a plot
    // that asked for nothing no room at all, be two sizes at once, or give two
    // plots beside each other two heights. The gaps between cells used to go
    // uncounted, so two 300 px plots passed a 600 px page and were drawn at 290.
    let canvas = (
        figure.ask_within(true, inherited).unwrap_or(canvas.0),
        figure.ask_within(false, inherited).unwrap_or(canvas.1),
    );
    let horizontal = page.arrange == crate::ir::Arrange::Beside;
    let (limit, dimension, across, word) = if horizontal {
        (canvas.0, "width", "height", "beside")
    } else {
        (canvas.1, "height", "width", "below")
    };
    let asks: Vec<Option<f64>> = page.cells.iter()
        .map(|c| c.ask_within(horizontal, stated_gap)).collect();
    let claimed: f64 = asks.iter().flatten().sum();
    let gaps = stated_gap.unwrap_or(crate::ir::PAGE_GAP)
        * page.cells.len().saturating_sub(1) as f64;
    let unasked = asks.iter().filter(|a| a.is_none()).count();
    let stated = if horizontal { page.theme.width } else { page.theme.height };
    let whole = claimed + gaps;
    let message = if whole > limit + 0.5 {
        // Only a stated page can be too small for plots that all ask, since an
        // unstated one takes what they ask; then the page is what to change.
        let direction = if unasked > 0 {
            format!(
                "A `theme({dimension} = )` on a composed plot is how much of the arrangement \
                 that plot takes, so the ones that state it must leave room for the ones that do not."
            )
        } else {
            format!(
                "An arrangement whose plots all state a size is as big as they are; drop the \
                 arrangement's `theme({dimension} = )`, or make a plot's smaller."
            )
        };
        Some(format!(
            "gog: the plots {word} each other ask for {claimed:.0}px of {dimension}, and the \
             {gaps:.0}px between them makes {whole:.0}px; the arrangement has {limit:.0}. {direction}"
        ))
    } else if unasked > 0 && whole >= limit - 0.5 {
        let who = if unasked == 1 { "the plot that asks" } else { "the plots that ask" };
        Some(format!(
            "gog: the plots {word} each other ask for {claimed:.0}px of {dimension}, and with \
             the {gaps:.0}px between them that is all {limit:.0}px the arrangement has, which \
             leaves no room for {who} for no size. Make a stated size smaller, or the \
             arrangement larger: \
             `(…) + theme({dimension} = )`."
        ))
    } else if let (0, Some(s)) = (unasked, stated) {
        ((whole - s).abs() > 0.5).then(|| format!(
            "gog: the plots {word} each other ask for {claimed:.0}px of {dimension}, \
             {whole:.0}px with the space between them, and the arrangement states {s:.0}. An \
             arrangement whose plots all state a size is as big as they are; drop the \
             arrangement's \
             `theme({dimension} = )`, or change a plot's."
        ))
    } else {
        None
    };
    if let Some(message) = message {
        out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
    }
    // Across the way the cells run, they share the page's one extent: plots side by
    // side are as tall as the page. Two cells asking for two, or one asking for
    // other than what the page states, is a page asked to be two sizes at once.
    let mut crossing: Vec<f64> = page.cells.iter()
        .filter_map(|c| c.ask_within(!horizontal, stated_gap)).collect();
    let stated_across = if horizontal { page.theme.height } else { page.theme.width };
    crossing.extend(stated_across);
    crossing.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    crossing.dedup_by(|a, b| (*a - *b).abs() <= 0.5);
    if crossing.len() > 1 {
        let sizes: Vec<String> = crossing.iter().map(|v| format!("{v:.0}px")).collect();
        let where_ = if stated_across.is_some() { " between them and the arrangement" } else { "" };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: plots {word} each other share one {across}, and {} are asked for{where_}. \
                 Give them one {across}, or state it once on the arrangement: \
                 `(…) + theme({across} = )`.",
                code_free_list(&sizes)
            ),
        });
    }
    for cell in &page.cells {
        check_page_fits(out, cell, canvas, stated_gap);
    }
}

/// What a page's `theme()` may say: how big the figure is, and nothing else.
///
/// Every other theme property describes a **panel** — the grid drawn on it, the
/// frame around it, its background, the band above it, the angle its tick labels
/// are read at, the type they are set in. A page has no panel; it is plots
/// arranged, each with a panel of its own. So a property landing here has no
/// subject to describe, and taking it in silence would be the accept-and-drop
/// §12 forbids. The direction is the same for every one of them, because the
/// place it does mean something is one level down.
///
/// Size is the exception because size is the one theme property whose subject is
/// the *figure*: a plot states how much room it wants, a page states how much
/// room it wants, and composed, the first is a cell of the second. A `theme()`
/// that cascaded its appearance to every cell is a real thing to want and a
/// second feature; one atom whose properties scattered to two different scopes
/// is the double meaning §13 exists to catch.
pub(super) fn check_page_theme(out: &mut Vec<Diagnostic>, page: &PageSpec) {
    let t = &page.theme;
    let panel_properties: [(&str, bool); 11] = [
        ("preset", t.preset.is_some()),
        ("grid", t.grid.is_some()),
        ("ratio", t.ratio.is_some()),
        ("tick_angle", t.tick_angle.is_some()),
        ("font_size", t.font_size.is_some()),
        ("background", t.background.is_some()),
        ("strip", t.strip.is_some()),
        ("strip_text", t.strip_text.is_some()),
        ("frame", t.frame.is_some()),
        ("axis_label", t.axis_label.is_some()),
        // A page draws no legend of its own: each plot keeps its keys.
        ("legend", t.legend.is_some()),
    ];
    for (name, stated) in panel_properties {
        if !stated {
            continue;
        }
        // Quoted back the way it was written, so the refusal names the reader's
        // own sentence: a preset is the first argument and has no name.
        let written = match (name, t.preset.as_deref()) {
            ("preset", Some(preset)) => format!("theme(\"{preset}\")"),
            _ => format!("theme({name} = )"),
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: `{written}` on an arrangement is valid grammar, but this engine does \
                 not draw it: it describes a panel, and an arrangement is plots placed \
                 together rather than a panel of its own. On an arrangement, `theme()` \
                 states how big the figure is and \
                 how far apart its plots are — `theme(width = )`, `theme(height = )` and \
                 `theme(gap = )` — and nothing else. Write \
                 this into the plot it describes, before composing: \
                 `(plot + {written}) | other_plot`."
            ),
        });
    }

    check_theme_size(
        out,
        t,
        "the arrangement",
        "It is the whole figure's size, which the plots composed onto it then divide \
         between them.",
    );
    // **The space between the plots is the page's too** (ruled 2026-10-09): its
    // subject is the figure, as the size's is, so it is the second thing a page's
    // `theme()` says. Pixels, as the size is, and none is a legal answer.
    if let Some(gap) = t.gap {
        if !gap.is_finite() || gap < 0.0 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(gap = {gap})` is not a space. It is the pixels between the \
                     plots of an arrangement, so give 0 or more; 20 is the default."
                ),
            });
        }
    }
}

/// `theme(width =, height =)` is a size wherever it is written, so one function
/// checks it for a plot and for a page.
///
/// Both are checked by one closure over the two names for the reason the pair is
/// checked by one function over the two figures: a rule stated twice is a rule
/// that drifts, and the floor is the part that would drift first.
pub(super) fn check_theme_size(out: &mut Vec<Diagnostic>, theme: &ThemeSpec, asker: &str, then: &str) {
    for (name, value) in [("width", theme.width), ("height", theme.height)] {
        if let Some(v) = value {
            if !(v.is_finite() && v >= MIN_PLOT_SIZE) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `theme({name} = {v})` is not a size — it is how many pixels \
                         {asker} asks for, so it must be at least {MIN_PLOT_SIZE}. {then}"
                    ),
                });
            }
        }
    }
}
