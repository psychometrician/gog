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

/// The name of the column a table keeps when the spec names none of its own: all
/// zeros, so the table still has as many rows as it had (a bare `bar * count`
/// counts them) and discloses nothing.
pub const ROW_KEEPER: &str = "gog.rows";

/// **The request cut down to the columns the plot reads**, for a page to carry.
///
/// A plot that needs the engine in the browser (a brush, a cube) carries its
/// request into the page, and a binding used to send every column of every table,
/// so a published page held columns its sentence never mapped, an email address
/// beside the two numbers drawn. This keeps a column only if some plot on the page
/// reads it ([`crate::ir::PlotSpec::columns_read`]), which is asked of the *typed*
/// spec: a first version kept any column whose name appeared anywhere in the spec's
/// JSON, and so kept a column called `title` or `label` because the spec uses those
/// words as keys. The engine decides it, in one place, and the bindings ask
/// (`gog-cli --prune`) rather than each keeping a list that grows every time an atom
/// names a column. A spec that does not parse is returned whole: it cannot draw, and
/// a prune that guessed could make it draw wrong.
///
/// A table none of whose columns is read keeps [`ROW_KEEPER`] instead, so its row
/// count survives. Rows are never dropped: what is mapped is what is published.
pub fn prune(request: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    let Some(figure) = request.get("spec")
        .and_then(|s| serde_json::from_value::<Figure>(s.clone()).ok())
    else {
        return request.clone();
    };
    let named: BTreeSet<String> = figure.plots().iter()
        .flat_map(|p| p.columns_read())
        .collect();
    let mut out = request.clone();
    let Some(tables) = out.get_mut("data").and_then(Value::as_object_mut) else {
        return out;
    };
    for table in tables.values_mut() {
        let Some(table) = table.as_object_mut() else { continue };
        let rows = ["floats", "strings"].iter()
            .filter_map(|kind| table.get(*kind).and_then(Value::as_object))
            .flat_map(|cols| cols.values())
            .filter_map(Value::as_array)
            .map(Vec::len)
            .max()
            .unwrap_or(0);
        for kind in ["floats", "strings", "levels", "dates"] {
            if let Some(cols) = table.get_mut(kind).and_then(Value::as_object_mut) {
                cols.retain(|col, _| named.contains(col));
            }
        }
        let kept = ["floats", "strings"].iter()
            .filter_map(|kind| table.get(*kind).and_then(Value::as_object))
            .any(|cols| !cols.is_empty());
        if !kept && rows > 0 {
            let zeros = Value::Array(vec![Value::from(0.0); rows]);
            match table.get_mut("floats").and_then(Value::as_object_mut) {
                Some(floats) => {
                    floats.insert(ROW_KEEPER.to_string(), zeros);
                }
                None => {
                    table.insert("floats".to_string(),
                        serde_json::json!({ ROW_KEEPER: zeros }));
                }
            }
        }
    }
    out
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

    /// A column whose name is a word the spec uses for something else, a key such
    /// as `title` or `label`, a mark's name, a table's name, is not taken for a
    /// column the plot reads.
    #[test]
    fn prune_reads_the_spec_not_its_words() {
        let request: serde_json::Value = serde_json::from_str(r#"{
            "spec": {"data":"t","title":"white","layers":[{"mark":"point","encodings":{
                "x":{"field":"a"},"label":{"field":"b"}},"transforms":[]}]},
            "data": {"t": {"floats": {"a": [1.0], "x": [2.0]},
                           "strings": {"b": ["p"], "title": ["Dr."], "label": ["HIV+"],
                                       "point": ["q"], "white": ["w"], "t": ["s"]}}}
        }"#).unwrap();
        let out = super::prune(&request);
        let t = &out["data"]["t"];
        assert_eq!(t["floats"].as_object().unwrap().keys().collect::<Vec<_>>(), vec!["a"]);
        assert_eq!(t["strings"].as_object().unwrap().keys().collect::<Vec<_>>(), vec!["b"]);
    }

    /// A page carries the columns its plot names and no others, with each kept
    /// column's declarations, and a table whose columns are all unnamed keeps its
    /// row count in a column of zeros.
    #[test]
    fn prune_keeps_the_named_columns_and_the_row_count() {
        let request: serde_json::Value = serde_json::from_str(r#"{
            "spec": {"data":"t","layers":[{"mark":"point","encodings":{
                "x":{"field":"a"},"color":{"field":"k"}},"transforms":[]},
                {"mark":"bar","data":"u","encodings":{},"transforms":["count"]}]},
            "data": {
                "t": {"floats": {"a": [1.0, 2.0], "email_score": [9.0, 8.0]},
                      "strings": {"k": ["p", "q"], "email": ["x@y", "z@w"]},
                      "levels": {"k": ["q", "p"], "email": ["x@y", "z@w"]},
                      "dates": {"when": "day"}},
                "u": {"strings": {"secret": ["s", "t", "u"]}}
            }
        }"#).unwrap();
        let out = super::prune(&request);
        let t = &out["data"]["t"];
        assert_eq!(t["floats"].as_object().unwrap().keys().collect::<Vec<_>>(), vec!["a"]);
        assert_eq!(t["strings"].as_object().unwrap().keys().collect::<Vec<_>>(), vec!["k"]);
        assert_eq!(t["levels"].as_object().unwrap().keys().collect::<Vec<_>>(), vec!["k"]);
        assert!(t["dates"].as_object().unwrap().is_empty());
        let u = &out["data"]["u"];
        assert!(u["strings"].as_object().unwrap().is_empty(), "the unnamed column is gone");
        assert_eq!(u["floats"][super::ROW_KEEPER].as_array().map(Vec::len), Some(3),
            "and the three rows are still counted");
        assert_eq!(out["spec"], request["spec"], "the spec is untouched");
    }
}
