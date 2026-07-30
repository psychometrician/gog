/// Example: render a bar chart to `output/bar.svg`.
///
/// Spec (following the design document):
///
///   data(medals)
///     + bar
///     + x(year) + y(gold)
///     + color(country)
use std::collections::HashMap;

use gog_core::{
    data::DataFrame,
    ir::{Channel, Layer, Mark, PlotSpec},
};

/// Draw, or report the refusal and stop — what every Rust caller does.
///
/// `plot::render` is the only way into the engine: it checks the spec against
/// the grammar first, so an illegal plot is refused with direction rather than
/// drawn in silence. Reaching for `SvgRenderer` directly does not compile.
fn draw(spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> String {
    match gog_core::plot::render(spec, data) {
        Ok(drawing) => {
            for d in &drawing.diagnostics {
                eprintln!("{}", d.message);
            }
            drawing.svg
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

fn main() {
    // -----------------------------------------------------------------------
    // 1. Gold medal counts at Summer Olympics, grouped by year
    // -----------------------------------------------------------------------
    let medals = DataFrame::new()
        .with_float(
            "year",
            vec![
                1996.0, 2000.0, 2004.0, 2008.0, 2012.0, 2016.0, 2020.0,
            ],
        )
        .with_float(
            "gold",
            vec![44.0, 36.0, 36.0, 36.0, 46.0, 46.0, 39.0],
        )
        .with_str(
            "country",
            vec!["USA", "USA", "USA", "USA", "USA", "USA", "USA"]
                .into_iter()
                .map(String::from)
                .collect(),
        );

    // -----------------------------------------------------------------------
    // 2. Build the spec
    //    Mirrors:  data(medals) + bar + x(year) + y(gold) + color(country)
    // -----------------------------------------------------------------------
    let spec = PlotSpec::new()
        .data("medals")
        .title("USA Gold Medals — Summer Olympics")
        .x("year")
        .y("gold")
        .x_label("Year")
        .y_label("Gold medals")
        .layer(
            Layer::new(Mark::Bar)
                .encode(Channel::Color, "country"),
        );

    // -----------------------------------------------------------------------
    // 3. Render to SVG
    // -----------------------------------------------------------------------
    let mut data = HashMap::new();
    data.insert("medals".to_string(), medals);

    std::fs::create_dir_all("output").expect("failed to create output/");

    let svg_str = draw(&spec, &data);
    std::fs::write("output/bar.svg", &svg_str).expect("failed to write bar.svg");
    println!("Rendered {} bytes → output/bar.svg", svg_str.len());
}
