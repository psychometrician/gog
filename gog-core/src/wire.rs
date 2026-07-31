//! The request envelope — the JSON a binding sends, decoded into engine types.
//!
//! `ir.rs` owns the *spec* contract. This module owns what wraps it on the way
//! in: the spec plus the column-oriented data tables, and the policy for what
//! happens to a row that is missing a value.
//!
//! It lives here rather than in a bridge because there is more than one bridge.
//! `gog-cli` speaks this format over stdin; the WebAssembly build speaks it over
//! a pointer into linear memory. Two decoders would be two chances to disagree
//! about which rows get dropped, and a disagreement there is not a crash — it is
//! the browser quietly drawing a different dataset than the command line. That
//! is the failure a deleted `png.rs` already taught this project once, when
//! duplicated layout code drifted until one renderer drew untransformed data.
//!
//! Missing values are the whole of the policy, and it has three parts. A row is
//! dropped only when a column the plot *maps* has no value in it, because real
//! tables carry gaps in columns a given plot never reads and shrinking `n` for
//! those would be a silent lie. The drop is **counted and named**, never
//! silent. And a `None` in an unmapped column becomes `NaN`, which is inert
//! because nothing reads it.

use crate::data::DataFrame;
use crate::ir::Figure;
use crate::time::TimeUnit;
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};

/// One table, as a binding sends it: columns rather than rows.
#[derive(Deserialize, Default)]
pub struct DataFrameJson {
    /// A `None` is a missing value. R's `NA` crosses the wire as JSON `null`
    /// (`na = "null"` in the binding's `toJSON`), not as the string `"NA"`,
    /// which would fail the `f64` parse.
    #[serde(default)]
    pub floats: HashMap<String, Vec<Option<f64>>>,
    #[serde(default)]
    pub strings: HashMap<String, Vec<Option<String>>>,
    /// Declared category order for a text column — an R factor's levels.
    /// Absent for a plain character column, which has no declared order.
    #[serde(default)]
    pub levels: HashMap<String, Vec<String>>,
    /// Declared resolution for a temporal column — `"day"` for an R `Date`,
    /// `"second"` for a `POSIXct`. The values live in `floats` as epoch
    /// seconds; this marker is what keeps them being dates.
    #[serde(default)]
    pub dates: HashMap<String, TimeUnit>,
}

/// One plot, or a page of them, plus the tables it reads.
#[derive(Deserialize)]
pub struct RenderRequest {
    /// The two shapes are told apart by their own required fields, so every
    /// spec ever written still parses and no binding needed a flag day to gain
    /// composition.
    pub spec: Figure,
    #[serde(default)]
    pub data: HashMap<String, DataFrameJson>,
}

/// Turn the wire's tables into engine tables, dropping rows a missing value has
/// made unplottable.
///
/// Returns the frames and one message per table that lost rows. The messages are
/// **returned rather than printed** because only one of the callers has a stderr
/// to print to: a WebAssembly build has none, and a decoder that reported by
/// `eprintln!` would drop its diagnostics on the floor in the browser. Deciding
/// where words go belongs to the bridge, not to the decoding.
pub fn decode(request: RenderRequest) -> (HashMap<String, DataFrame>, Vec<String>) {
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
    let mut remarks: Vec<String> = Vec::new();

    for (name, df_json) in request.data {
        // Frames are rectangular, so every column is the same length; take the
        // longest defensively.
        let n = df_json
            .floats
            .values()
            .map(Vec::len)
            .chain(df_json.strings.values().map(Vec::len))
            .max()
            .unwrap_or(0);

        let mut keep = vec![true; n];
        let mut culprits: BTreeSet<String> = BTreeSet::new();
        {
            let mut veto = |col: &str, missing: &[bool]| {
                if !mapped.contains(col) {
                    return;
                }
                let mut hit = false;
                for (i, &is_missing) in missing.iter().enumerate() {
                    if is_missing {
                        keep[i] = false;
                        hit = true;
                    }
                }
                if hit {
                    culprits.insert(col.to_string());
                }
            };
            for (col, vals) in &df_json.floats {
                veto(col, &vals.iter().map(Option::is_none).collect::<Vec<_>>());
            }
            for (col, vals) in &df_json.strings {
                veto(col, &vals.iter().map(Option::is_none).collect::<Vec<_>>());
            }
        }
        let dropped = keep.iter().filter(|&&k| !k).count();

        let mut df = DataFrame::new();
        for (col, vals) in df_json.floats {
            // A dropped row's `None` never survives to be unwrapped; a `None` in
            // an unmapped column (row kept) becomes NaN, inert because nothing
            // reads it.
            let clean: Vec<f64> = vals
                .into_iter()
                .zip(&keep)
                .filter(|(_, &k)| k)
                .map(|(v, _)| v.unwrap_or(f64::NAN))
                .collect();
            df = match df_json.dates.get(&col) {
                Some(unit) => df.with_time(col, clean, *unit),
                None => df.with_float(col, clean),
            };
        }
        for (col, vals) in df_json.strings {
            let clean: Vec<String> = vals
                .into_iter()
                .zip(&keep)
                .filter(|(_, &k)| k)
                .map(|(v, _)| v.unwrap_or_default())
                .collect();
            df = match df_json.levels.get(&col) {
                Some(levels) => df.with_levels(col, clean, levels.clone()),
                None => df.with_str(col, clean),
            };
        }

        if dropped > 0 {
            let cols = culprits
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(", ");
            remarks.push(format!(
                "gog: dropped {dropped} row{} of `{name}` with a missing value in {cols} — \
                 a row with no value in a column the plot maps cannot be placed, so it is \
                 left out (the same as other plotting tools drop NA).",
                if dropped == 1 { "" } else { "s" },
            ));
        }
        data.insert(name, df);
    }

    (data, remarks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(json: &str) -> RenderRequest {
        serde_json::from_str(json).expect("wire JSON should parse")
    }

    /// The policy's first half: a gap in a column the plot maps costs that row.
    #[test]
    fn a_missing_value_in_a_mapped_column_drops_its_row_and_says_so() {
        let (data, remarks) = decode(req(r#"{
            "spec": {"data":"t","layers":[{"mark":"point","encodings":{
                "x":{"field":"a"},"y":{"field":"b"}},"transforms":[]}]},
            "data": {"t": {"floats": {"a": [1.0, null, 3.0], "b": [1.0, 2.0, 3.0]}}}
        }"#));
        assert_eq!(data["t"].len(), 2, "the row with the gap is gone");
        assert_eq!(remarks.len(), 1, "and the drop is reported, never silent");
        assert!(remarks[0].contains("dropped 1 row"), "{}", remarks[0]);
        assert!(remarks[0].contains("`a`"), "names the column: {}", remarks[0]);
    }

    /// The policy's second half, and the reason it is not simply "drop any NA":
    /// a real table carries gaps in columns a given plot never reads, and
    /// shrinking `n` for those would misreport how much data was plotted.
    #[test]
    fn a_missing_value_in_an_unmapped_column_keeps_its_row_and_stays_quiet() {
        let (data, remarks) = decode(req(r#"{
            "spec": {"data":"t","layers":[{"mark":"point","encodings":{
                "x":{"field":"a"},"y":{"field":"b"}},"transforms":[]}]},
            "data": {"t": {"floats": {"a": [1.0,2.0,3.0], "b": [1.0,2.0,3.0], "unread": [1.0,null,3.0]}}}
        }"#));
        assert_eq!(data["t"].len(), 3, "no row is lost for a column nothing reads");
        assert!(remarks.is_empty(), "and nothing is reported: {remarks:?}");
    }
}
