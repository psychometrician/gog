/// Example: render a 2-D scatter plot to `output/scatter.svg`.
///
/// Spec (following the design document):
///
///   data(gapminder_sample)
///     + point
///     + x(gdp) + y(life)
///     + color(continent)
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
    // 1. Build a small in-memory table (stand-in for gapminder)
    // -----------------------------------------------------------------------
    let gapminder = DataFrame::new()
        .with_float(
            "gdp",
            vec![
                1329.0, 2182.0, 3677.0, 5937.0, 9065.0, 11399.0, 14476.0, 19722.0, 25979.0,
                33849.0, 601.0, 974.0, 1386.0, 1823.0, 2452.0, 35000.0, 38000.0, 42000.0,
                46000.0, 50000.0,
            ],
        )
        .with_float(
            "life",
            vec![
                43.8, 47.2, 51.5, 56.0, 60.3, 63.1, 66.7, 70.0, 72.9, 75.3, 39.0, 41.5, 44.8,
                48.3, 52.0, 76.0, 77.5, 78.9, 80.1, 81.2,
            ],
        )
        .with_str(
            "continent",
            vec![
                "Asia", "Asia", "Asia", "Asia", "Asia", "Asia", "Asia", "Asia", "Asia", "Asia",
                "Africa", "Africa", "Africa", "Africa", "Africa",
                "Europe", "Europe", "Europe", "Europe", "Europe",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );

    // -----------------------------------------------------------------------
    // 2. Build the plot spec
    //    Mirrors:  data(gapminder) + point + x(gdp) + y(life) + color(continent)
    // -----------------------------------------------------------------------
    let spec = PlotSpec::new()
        .data("gapminder")
        .title("GDP vs Life Expectancy")
        .x("gdp")
        .y("life")
        .x_label("GDP per Capita (USD)")
        .y_label("Life Expectancy (years)")
        .layer(
            Layer::new(Mark::Point)
                .encode(Channel::Color, "continent"),
        );

    // -----------------------------------------------------------------------
    // 3. Render to SVG
    // -----------------------------------------------------------------------
    let mut data = HashMap::new();
    data.insert("gapminder".to_string(), gapminder);

    std::fs::create_dir_all("output").expect("failed to create output/");

    let svg_str = draw(&spec, &data);
    std::fs::write("output/scatter.svg", &svg_str).expect("failed to write scatter.svg");
    println!("Rendered {} bytes → output/scatter.svg", svg_str.len());

    // -----------------------------------------------------------------------
    // 4. Round-trip the spec through JSON (proves the IR is serializable)
    // -----------------------------------------------------------------------
    let json = serde_json::to_string_pretty(&spec).expect("serialization failed");
    println!("\nSpec JSON:\n{json}");
}
