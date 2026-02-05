use plotters::prelude::*;
use plotters_typst::TypstBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a Typst backend
    let root = TypstBackend::new("simple_plot.typ", (400, 700)).into_drawing_area();
    root.fill(&WHITE)?;

    // Build a chart
    let mut chart = ChartBuilder::on(&root)
        .caption("Simple Line Plot", ("sans-serif", 40))
        .margin(10)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(0f32..10f32, 0f32..10f32)?;

    chart
        .configure_mesh()
        .x_desc("X Axis")
        .y_desc("Y Axis")
        .draw()?;

    // Draw some data
    chart
        .draw_series(LineSeries::new(
            (0..=100).map(|x| {
                let x = x as f32 / 10.0;
                (x, x.sin() * 5.0 + 5.0)
            }),
            &RED,
        ))?
        .label("sin(x)")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    chart
        .draw_series(LineSeries::new(
            (0..=100).map(|x| {
                let x = x as f32 / 10.0;
                (x, x.cos() * 5.0 + 5.0)
            }),
            &BLUE,
        ))?
        .label("cos(x)")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;

    println!("Plot saved to simple_plot.typ");
    println!("Include it in your Typst document with: #include \"simple_plot.typ\"");

    Ok(())
}
