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
use gog_core::wire::{self, RenderRequest};
use std::io::Read;

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    // `gog-cli --version` prints the version this binary was built from, and
    // exists so a package can be asked whether the engine beside it is its own.
    //
    // Nothing else can answer that. A binary's bytes do not identify it: a
    // correct engine built freshly by `configure` inside an installed package
    // hashes differently from the same sources built in a checkout, because the
    // build path travels in the file. So a hash cannot tell a fresh build from
    // a wrong one, and comparing drawn output cannot either — two engines a
    // release apart agree on every sentence that did not change between them,
    // which is nearly all of them. A source tarball once shipped an engine a
    // whole version behind its own manifest and every check in this repository
    // stayed green, including the harness that draws all 740 sentences in the
    // manual through both.
    //
    // Read no stdin and exit 0, like `--rules` above it.
    if std::env::args().skip(1).any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

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

    // `decode` consumes the tables, so the spec is taken first; it is needed
    // after, to render.
    let spec = request.spec.clone();

    // Decoding — including which rows a missing value costs — is `gog_core::wire`'s,
    // not this bridge's. It moved down when a second caller appeared: the
    // WebAssembly build speaks the same format over a pointer instead of stdin,
    // and two decoders would be two chances to disagree about which rows get
    // dropped. All this binary decides is where the words go.
    let (data, remarks) = wire::decode(request);
    for r in &remarks {
        eprintln!("{r}");
    }

    // Check-then-render, and the `GOG_STRICT` policy with it, are `gog_core`'s —
    // not the bridge's. They used to live here, which meant every other caller of
    // the engine (the examples, any future Rust/WASM/FFI binding) reached the
    // renderer without passing the gate and drew illegal plots in silence. All
    // this binary decides now is where the words go: diagnostics to stderr, SVG
    // to stdout, 2 on a refusal.
    match gog_core::plot::render_figure(&spec, &data) {
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
