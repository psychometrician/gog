/// gog-cli — the JSON→SVG bridge
///
/// Reads a `RenderRequest` (spec + column-oriented data) as JSON from stdin,
/// renders with the SVG renderer, and writes the SVG string to stdout.
///
/// Language bindings (R, Python, Julia) call this binary via a subprocess and
/// capture stdout.  No native FFI is required.
///
/// Wire format:
/// ```json
/// {
///   "spec":  { ...PlotSpec... },
///   "data": {
///     "table_name": {
///       "floats":  { "col": [1.0, 2.0, ...] },
///       "strings": { "col": ["a", "b", ...] },
///       "levels":  { "col": ["Low", "Medium", "High"] },
///       "dates":   { "col": "day" }
///     }
///   }
/// }
/// ```
use gog_core::{
    data::DataFrame,
    ir::Figure,
    time::TimeUnit,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;

// ---------------------------------------------------------------------------
// Wire types (input-only; no need to re-export from gog-core)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DataFrameJson {
    /// A `None` is a missing value — R's `NA` crosses the wire as JSON `null`
    /// (`na = "null"` in the binding's `toJSON`). A row missing a value in a
    /// column the plot *maps* cannot be placed and is dropped below, with a
    /// count reported; a missing value in an unmapped column rides along.
    #[serde(default)]
    floats: HashMap<String, Vec<Option<f64>>>,
    #[serde(default)]
    strings: HashMap<String, Vec<Option<String>>>,
    /// Declared category order for a text column — an R factor's levels.
    /// Absent for a plain character column, which has no declared order.
    #[serde(default)]
    levels: HashMap<String, Vec<String>>,
    /// Declared resolution for a temporal column — `"day"` for an R `Date`,
    /// `"second"` for a `POSIXct`. The column's values live in `floats` as
    /// epoch seconds; this marker is what keeps them being dates.
    #[serde(default)]
    dates: HashMap<String, TimeUnit>,
}

#[derive(Deserialize)]
struct RenderRequest {
    /// One plot, or a page of them. The two shapes are told apart by their own
    /// required fields (`ir::Figure`), so every spec ever written still parses
    /// and no binding needed a flag day to gain composition.
    spec: Figure,
    #[serde(default)]
    data: HashMap<String, DataFrameJson>,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    // `gog-cli --rules` dumps the Mark × Channel legality matrix as JSON and
    // exits, reading no stdin. The book's Combinations appendix shells out to
    // this and renders the grid live, so the grid is generated from `rule_for`
    // — the one source of truth — and can never drift from what the engine
    // enforces. Checked first, before the blocking stdin read below.
    if std::env::args().skip(1).any(|a| a == "--rules") {
        match serde_json::to_string_pretty(&gog_core::legality::rules_matrix()) {
            Ok(json) => {
                println!("{json}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("gog-cli: failed to serialize rules matrix: {e}");
                std::process::exit(1);
            }
        }
    }

    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("gog-cli: failed to read stdin: {e}");
        std::process::exit(1);
    }

    let request: RenderRequest = match serde_json::from_str(&input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("gog-cli: JSON parse error: {e}");
            std::process::exit(1);
        }
    };

    // Convert JSON data tables → gog-core DataFrames, dropping rows a missing
    // value has made unplottable. The policy — drop a row only when a column the
    // plot *maps* is missing in it, and never in silence — lives here, in the
    // engine, so every binding inherits it; the binding's only job was to send
    // R's `NA` as a wire `null`. Scoped to mapped columns because a real dataset
    // (penguins) carries `NA` in columns a given plot never reads, and shrinking
    // n for those would be a silent lie. `DataFrame` (temporary; Arrow is M14)
    // has no null slot of its own, so the drop happens here where the wire's
    // `Option` still carries the missingness.
    // Every column any plot on the page maps. A page's cells share the data
    // registry, so a row is dropped only if the column it is missing is mapped
    // *somewhere* — the union, not one plot's answer.
    let mapped: std::collections::HashSet<String> = request
        .spec
        .plots()
        .iter()
        .flat_map(|s| s.mapped_fields())
        .collect();
    let mut data: HashMap<String, DataFrame> = HashMap::new();
    for (name, df_json) in request.data {
        // Frames are rectangular, so every column is the same length; take the
        // longest defensively.
        let n = df_json.floats.values().map(Vec::len)
            .chain(df_json.strings.values().map(Vec::len))
            .max().unwrap_or(0);

        let mut keep = vec![true; n];
        let mut culprits: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut veto = |col: &str, missing: &[bool]| {
            if !mapped.contains(col) { return; }
            let mut hit = false;
            for (i, &is_missing) in missing.iter().enumerate() {
                if is_missing { keep[i] = false; hit = true; }
            }
            if hit { culprits.insert(col.to_string()); }
        };
        for (col, vals) in &df_json.floats {
            veto(col, &vals.iter().map(Option::is_none).collect::<Vec<_>>());
        }
        for (col, vals) in &df_json.strings {
            veto(col, &vals.iter().map(Option::is_none).collect::<Vec<_>>());
        }
        let dropped = keep.iter().filter(|&&k| !k).count();

        let mut df = DataFrame::new();
        for (col, vals) in df_json.floats {
            // A dropped row's `None` never survives to be unwrapped; a `None` in
            // an unmapped column (row kept) becomes NaN, inert because nothing
            // reads it.
            let clean: Vec<f64> = vals.into_iter().zip(&keep)
                .filter(|(_, &k)| k).map(|(v, _)| v.unwrap_or(f64::NAN)).collect();
            match df_json.dates.get(&col) {
                Some(unit) => df = df.with_time(col, clean, *unit),
                None => df = df.with_float(col, clean),
            }
        }
        for (col, vals) in df_json.strings {
            let clean: Vec<String> = vals.into_iter().zip(&keep)
                .filter(|(_, &k)| k).map(|(v, _)| v.unwrap_or_default()).collect();
            match df_json.levels.get(&col) {
                Some(levels) => df = df.with_levels(col, clean, levels.clone()),
                None => df = df.with_str(col, clean),
            }
        }

        if dropped > 0 {
            let cols = culprits.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ");
            eprintln!(
                "gog: dropped {dropped} row{} of `{name}` with a missing value in {cols} — \
                 a row with no value in a column the plot maps cannot be placed, so it is \
                 left out (the same as other plotting tools drop NA).",
                if dropped == 1 { "" } else { "s" },
            );
        }
        data.insert(name, df);
    }

    // Check-then-render, and the `GOG_STRICT` policy with it, are `gog_core`'s —
    // not the bridge's. They used to live here, which meant every other caller of
    // the engine (the examples, any future Rust/WASM/FFI binding) reached the
    // renderer without passing the gate and drew illegal plots in silence. All
    // this binary decides now is where the words go: diagnostics to stderr, SVG
    // to stdout, 2 on a refusal.
    match gog_core::plot::render_figure(&request.spec, &data) {
        Ok(drawing) => {
            for d in &drawing.diagnostics {
                eprintln!("{}", d.message);
            }
            print!("{}", drawing.svg);
        }
        Err(diagnostics) => {
            for d in &diagnostics {
                eprintln!("{}", d.message);
            }
            eprintln!("{}", gog_core::plot::REFUSED);
            std::process::exit(2);
        }
    }
}
