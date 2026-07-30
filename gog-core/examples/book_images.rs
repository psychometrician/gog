/// Generates all static SVG images used in the GOG book.
/// Run with: cargo run --example book_images
/// Output: book/images/*.svg
use std::collections::HashMap;

use gog_core::{
    data::DataFrame,
    ir::{Channel, Layer, Mark, PlotSpec},
};

fn main() {
    std::fs::create_dir_all("book/images").expect("failed to create book/images/");

    scatter_basic();
    scatter_color();
    scatter_shape();
    scatter_size();
    scatter_all_channels();
    bar_chart();
    bar_color();
    line_basic();
    line_chart();
    multi_layer();
    multi_table();

    println!("All book images written to book/images/");
}

// ---------------------------------------------------------------------------
// Shared data helpers
// ---------------------------------------------------------------------------

fn gapminder() -> DataFrame {
    DataFrame::new()
        .with_float("gdp",  vec![
            1329.0, 2182.0, 3677.0, 5937.0, 9065.0, 11399.0, 14476.0, 19722.0, 25979.0, 33849.0,
            601.0,  974.0,  1386.0, 1823.0, 2452.0,
            35000.0, 38000.0, 42000.0, 46000.0, 50000.0,
        ])
        .with_float("life", vec![
            43.8, 47.2, 51.5, 56.0, 60.3, 63.1, 66.7, 70.0, 72.9, 75.3,
            39.0, 41.5, 44.8, 48.3, 52.0,
            76.0, 77.5, 78.9, 80.1, 81.2,
        ])
        .with_float("population", vec![
            120.0, 150.0, 90.0, 200.0, 300.0, 80.0, 60.0, 50.0, 45.0, 100.0,
            30.0,  40.0,  20.0,  15.0,  25.0,
            80.0,  60.0,  70.0,  55.0,  90.0,
        ])
        .with_str("continent", [
            "Asia","Asia","Asia","Asia","Asia","Asia","Asia","Asia","Asia","Asia",
            "Africa","Africa","Africa","Africa","Africa",
            "Europe","Europe","Europe","Europe","Europe",
        ].iter().map(|s| s.to_string()).collect())
}

fn medals() -> DataFrame {
    DataFrame::new()
        .with_float("year", vec![1996.0, 2000.0, 2004.0, 2008.0, 2012.0, 2016.0, 2020.0])
        .with_float("gold", vec![44.0, 36.0, 36.0, 36.0, 46.0, 46.0, 39.0])
        .with_str("country", vec!["USA"; 7].iter().map(|s| s.to_string()).collect())
}

fn sales() -> DataFrame {
    DataFrame::new()
        .with_float("quarter", vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0,
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0,
        ])
        .with_float("revenue", vec![
            120.0, 135.0, 148.0, 160.0, 172.0, 168.0, 185.0, 200.0,
             80.0,  88.0,  95.0, 102.0, 110.0, 108.0, 115.0, 125.0,
        ])
        .with_str("product", [
            "Widget","Widget","Widget","Widget","Widget","Widget","Widget","Widget",
            "Gadget","Gadget","Gadget","Gadget","Gadget","Gadget","Gadget","Gadget",
        ].iter().map(|s| s.to_string()).collect())
}

// ---------------------------------------------------------------------------
// Image generators
// ---------------------------------------------------------------------------

/// Draw, or report the refusal and stop — what every Rust caller does.
///
/// `plot::render` is the only way into the engine: it checks the spec against
/// the grammar first, so an illegal plot is refused with direction rather than
/// drawn in silence. Reaching for `SvgRenderer` directly does not compile.
fn render(spec: &PlotSpec, data: &HashMap<String, DataFrame>, path: &str) {
    let svg = match gog_core::plot::render(spec, data) {
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
    };
    std::fs::write(path, &svg).unwrap_or_else(|_| panic!("failed to write {path}"));
    println!("  → {path}  ({} bytes)", svg.len());
}

fn scatter_color() {
    let spec = PlotSpec::new()
        .data("g").x("gdp").y("life")
        .x_label("GDP per Capita (USD)").y_label("Life Expectancy (years)")
        .title("GDP vs Life Expectancy")
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));

    let mut d = HashMap::new();
    d.insert("g".into(), gapminder());
    render(&spec, &d, "book/images/scatter_color.svg");
}

fn scatter_shape() {
    let spec = PlotSpec::new()
        .data("g").x("gdp").y("life")
        .x_label("GDP per Capita (USD)").y_label("Life Expectancy (years)")
        .title("GDP vs Life Expectancy — Shape by Continent")
        .layer(Layer::new(Mark::Point)
            .encode(Channel::Color, "continent")
            .encode(Channel::Shape, "continent"));

    let mut d = HashMap::new();
    d.insert("g".into(), gapminder());
    render(&spec, &d, "book/images/scatter_shape.svg");
}

fn scatter_size() {
    let spec = PlotSpec::new()
        .data("g").x("gdp").y("life")
        .x_label("GDP per Capita (USD)").y_label("Life Expectancy (years)")
        .title("GDP, Life Expectancy, and Population")
        .layer(Layer::new(Mark::Point)
            .encode(Channel::Color, "continent")
            .encode(Channel::Size,  "population"));

    let mut d = HashMap::new();
    d.insert("g".into(), gapminder());
    render(&spec, &d, "book/images/scatter_size.svg");
}

fn scatter_all_channels() {
    let spec = PlotSpec::new()
        .data("g").x("gdp").y("life")
        .x_label("GDP per Capita (USD)").y_label("Life Expectancy (years)")
        .title("The Health and Wealth of Nations")
        .layer(Layer::new(Mark::Point)
            .encode(Channel::Color, "continent")
            .encode(Channel::Shape, "continent")
            .encode(Channel::Size,  "population"));

    let mut d = HashMap::new();
    d.insert("g".into(), gapminder());
    render(&spec, &d, "book/images/scatter_all.svg");
}

fn scatter_basic() {
    let spec = PlotSpec::new()
        .data("g").x("gdp").y("life")
        .x_label("GDP per Capita (USD)").y_label("Life Expectancy (years)")
        .title("GDP vs Life Expectancy")
        .layer(Layer::new(Mark::Point));

    let mut d = HashMap::new();
    d.insert("g".into(), gapminder());
    render(&spec, &d, "book/images/scatter_basic.svg");
}

fn bar_color() {
    // Same as bar but with explicit color label so legend appears
    let spec = PlotSpec::new()
        .data("m").x("year").y("gold")
        .x_label("Year").y_label("Gold medals")
        .title("USA Gold Medals — Summer Olympics")
        .layer(Layer::new(Mark::Bar).encode(Channel::Color, "country"));

    let mut d = HashMap::new();
    d.insert("m".into(), medals());
    render(&spec, &d, "book/images/bar_color.svg");
}

fn line_basic() {
    // Single series line
    let df = DataFrame::new()
        .with_float("quarter", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])
        .with_float("revenue", vec![120.0, 135.0, 148.0, 160.0, 172.0, 168.0, 185.0, 200.0]);

    let spec = PlotSpec::new()
        .data("s").x("quarter").y("revenue")
        .x_label("Quarter").y_label("Revenue ($M)")
        .title("Quarterly Revenue")
        .layer(Layer::new(Mark::Line));

    let mut d = HashMap::new();
    d.insert("s".into(), df);
    render(&spec, &d, "book/images/line_basic.svg");
}

fn multi_table() {
    // Two different data sources layered
    let actuals = DataFrame::new()
        .with_float("quarter", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .with_float("revenue", vec![120.0, 135.0, 148.0, 160.0, 172.0, 168.0]);

    let forecast = DataFrame::new()
        .with_float("quarter", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])
        .with_float("revenue", vec![118.0, 130.0, 145.0, 162.0, 175.0, 180.0, 190.0, 205.0]);

    let spec = PlotSpec::new()
        .x("quarter").y("revenue")
        .x_label("Quarter").y_label("Revenue ($M)")
        .title("Actuals (line) + Forecast (points)")
        .data("actuals")
        .layer(Layer::new(Mark::Line))
        .layer(Layer::new(Mark::Point).data("forecast"));

    let mut d = HashMap::new();
    d.insert("actuals".into(), actuals);
    d.insert("forecast".into(), forecast);
    render(&spec, &d, "book/images/multi_table.svg");
}

fn bar_chart() {
    let spec = PlotSpec::new()
        .data("m").x("year").y("gold")
        .x_label("Year").y_label("Gold medals")
        .title("USA Gold Medals — Summer Olympics")
        .layer(Layer::new(Mark::Bar).encode(Channel::Color, "country"));

    let mut d = HashMap::new();
    d.insert("m".into(), medals());
    render(&spec, &d, "book/images/bar.svg");
}

fn line_chart() {
    let spec = PlotSpec::new()
        .data("s").x("quarter").y("revenue")
        .x_label("Quarter").y_label("Revenue ($M)")
        .title("Quarterly Revenue by Product")
        .layer(Layer::new(Mark::Line).encode(Channel::Color, "product"));

    let mut d = HashMap::new();
    d.insert("s".into(), sales());
    render(&spec, &d, "book/images/line.svg");
}

fn multi_layer() {
    let spec = PlotSpec::new()
        .data("s").x("quarter").y("revenue")
        .x_label("Quarter").y_label("Revenue ($M)")
        .title("Revenue — Line + Points")
        .layer(Layer::new(Mark::Line))
        .layer(Layer::new(Mark::Point).encode(Channel::Color, "product"));

    let mut d = HashMap::new();
    d.insert("s".into(), sales());
    render(&spec, &d, "book/images/multi_layer.svg");
}
