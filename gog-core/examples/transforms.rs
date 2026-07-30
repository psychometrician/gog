/// Example: transforms — histogram (bin) and smooth.
///
/// Demonstrates:
///   - `bar * bin`    → histogram of normally-distributed data
///   - `line * smooth`→ LOESS curve through noisy sine data
use std::collections::HashMap;

use gog_core::{
    data::DataFrame,
    ir::{Layer, Mark, PlotSpec, Transform},
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
    std::fs::create_dir_all("output").expect("failed to create output/");

    // -----------------------------------------------------------------------
    // 1. Histogram: bar * bin
    //    Simulated heights (cm) for 200 people, normally distributed ~170 ± 10
    // -----------------------------------------------------------------------
    let mut rng = SimpleRng::new(42);
    let heights: Vec<f64> = (0..200)
        .map(|_| {
            // Box–Muller transform
            let u1 = rng.next_f64();
            let u2 = rng.next_f64();
            let z  = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            170.0 + 10.0 * z
        })
        .collect();

    let hist_df = DataFrame::new().with_float("height", heights);
    let hist_spec = PlotSpec::new()
        .data("heights")
        .title("Distribution of Heights")
        .x("height")
        .y("count")
        .x_label("Height (cm)")
        .y_label("Count")
        .layer(Mark::Bar * Transform::Bin);

    let mut data = HashMap::new();
    data.insert("heights".to_string(), hist_df);

    let svg = draw(&hist_spec, &data);
    std::fs::write("output/histogram.svg", &svg).expect("failed to write histogram.svg");
    println!("Rendered {} bytes → output/histogram.svg", svg.len());

    // -----------------------------------------------------------------------
    // 2. Smooth: line * smooth (LOESS curve over noisy sine data)
    // -----------------------------------------------------------------------
    let mut rng2 = SimpleRng::new(99);
    let n = 80_usize;
    let xs: Vec<f64> = (0..n).map(|i| i as f64 / (n - 1) as f64 * 4.0 * std::f64::consts::PI).collect();
    let ys: Vec<f64> = xs.iter()
        .map(|&x| x.sin() + (rng2.next_f64() - 0.5) * 1.2)
        .collect();

    let noisy_df = DataFrame::new().with_float("x", xs).with_float("y", ys.clone());
    let smooth_spec = PlotSpec::new()
        .data("noisy")
        .title("LOESS Smooth")
        .x("x")
        .y("y")
        .x_label("x")
        .y_label("y")
        .layer(Layer::new(Mark::Point))
        .layer(Mark::Line * Transform::Smooth);

    let mut data2 = HashMap::new();
    data2.insert("noisy".to_string(), noisy_df);

    let svg2 = draw(&smooth_spec, &data2);
    std::fs::write("output/smooth.svg", &svg2).expect("failed to write smooth.svg");
    println!("Rendered {} bytes → output/smooth.svg", svg2.len());

    // -----------------------------------------------------------------------
    // 3. Density: line * density (KDE of the same height data)
    // -----------------------------------------------------------------------
    let mut rng3 = SimpleRng::new(42);
    let heights2: Vec<f64> = (0..200)
        .map(|_| {
            let u1 = rng3.next_f64();
            let u2 = rng3.next_f64();
            let z  = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            170.0 + 10.0 * z
        })
        .collect();

    let dens_df = DataFrame::new().with_float("height", heights2);
    let dens_spec = PlotSpec::new()
        .data("heights")
        .title("Kernel Density of Heights")
        .x("height")
        .y("density")
        .x_label("Height (cm)")
        .y_label("Density")
        .layer(Mark::Line * Transform::Density);

    let mut data3 = HashMap::new();
    data3.insert("heights".to_string(), dens_df);

    let svg3 = draw(&dens_spec, &data3);
    std::fs::write("output/density.svg", &svg3).expect("failed to write density.svg");
    println!("Rendered {} bytes → output/density.svg", svg3.len());
}

// ---------------------------------------------------------------------------
// Minimal LCG random number generator (no external rand crate needed)
// ---------------------------------------------------------------------------
struct SimpleRng { state: u64 }
impl SimpleRng {
    fn new(seed: u64) -> Self { Self { state: seed } }
    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}
