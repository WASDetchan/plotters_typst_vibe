/*!
The Typst drawing backend for plotters
*/

use plotters_backend::{
    text_anchor::{HPos, VPos},
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
    FontStyle, FontTransform,
};

use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufWriter, Error, Write};
use std::path::Path;

struct Rgb(u8, u8, u8);

fn make_typst_color(color: BackendColor) -> String {
    let Rgb(r, g, b) = Rgb(color.rgb.0, color.rgb.1, color.rgb.2);
    if color.alpha < 1.0 {
        format!(
            "rgb({}, {}, {}, {}%)",
            r,
            g,
            b,
            (color.alpha * 100.0) as u32
        )
    } else {
        format!("rgb({}, {}, {})", r, g, b)
    }
}

enum Target<'a> {
    File(String, &'a Path),
    Buffer(&'a mut String),
}

impl Target<'_> {
    fn get_mut(&mut self) -> &mut String {
        match self {
            Target::File(ref mut buf, _) => buf,
            Target::Buffer(buf) => buf,
        }
    }
}

/// The Typst drawing backend
pub struct TypstBackend<'a> {
    target: Target<'a>,
    size: (u32, u32),
    saved: bool,
}

impl<'a> TypstBackend<'a> {
    fn escape_text(text: &str) -> String {
        text.replace('\\', r"\\")
            .replace('"', r#"\""#)
            .replace('#', r"\#")
            .replace('$', r"\$")
    }

    fn write_command(&mut self, command: &str) {
        let buf = self.target.get_mut();
        buf.push_str(command);
        buf.push('\n');
    }

    fn init_canvas(&mut self, size: (u32, u32)) {
        let buf = self.target.get_mut();
        // Create a box with absolute positioning and clipping for the canvas
        writeln!(
            buf,
            "#box(width: {}pt, height: {}pt, clip: true)[",
            size.0, size.1
        )
        .unwrap();
    }

    /// Create a new Typst drawing backend
    pub fn new<T: AsRef<Path> + ?Sized>(path: &'a T, size: (u32, u32)) -> Self {
        let mut ret = Self {
            target: Target::File(String::default(), path.as_ref()),
            size,
            saved: false,
        };

        ret.init_canvas(size);
        ret
    }

    /// Create a new Typst drawing backend and store the document into a String buffer
    pub fn with_string(buf: &'a mut String, size: (u32, u32)) -> Self {
        let mut ret = Self {
            target: Target::Buffer(buf),
            size,
            saved: false,
        };

        ret.init_canvas(size);
        ret
    }
}

impl<'a> DrawingBackend for TypstBackend<'a> {
    type ErrorType = Error;

    fn get_size(&self) -> (u32, u32) {
        self.size
    }

    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        Ok(())
    }

    fn present(&mut self) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if !self.saved {
            // Close the box
            self.write_command("]");

            match self.target {
                Target::File(ref buf, path) => {
                    let outfile = File::create(path).map_err(DrawingErrorKind::DrawingError)?;
                    let mut outfile = BufWriter::new(outfile);
                    outfile
                        .write_all(buf.as_ref())
                        .map_err(DrawingErrorKind::DrawingError)?;
                }
                Target::Buffer(_) => {}
            }
            self.saved = true;
        }
        Ok(())
    }

    fn draw_pixel(
        &mut self,
        point: BackendCoord,
        color: BackendColor,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if color.alpha == 0.0 {
            return Ok(());
        }

        let cmd =
            format!(
            "  #place(dx: {}pt, dy: {}pt, rect(width: 1pt, height: 1pt, fill: {}, stroke: none))",
            point.0, point.1, make_typst_color(color)
        );
        self.write_command(&cmd);
        Ok(())
    }

    fn draw_line<S: BackendStyle>(
        &mut self,
        from: BackendCoord,
        to: BackendCoord,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }

        let color = make_typst_color(style.color());
        let stroke_width = style.stroke_width();

        let dx = (to.0 - from.0) as f64;
        let dy = (to.1 - from.1) as f64;
        let length = (dx * dx + dy * dy).sqrt();
        let angle = dy.atan2(dx).to_degrees();

        let cmd = format!(
            "  #place(dx: {}pt, dy: {}pt, line(length: {}pt, angle: {}deg, stroke: {}pt + {}))",
            from.0, from.1, length, angle, stroke_width, color
        );
        self.write_command(&cmd);
        Ok(())
    }

    fn draw_rect<S: BackendStyle>(
        &mut self,
        upper_left: BackendCoord,
        bottom_right: BackendCoord,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }

        let color = make_typst_color(style.color());
        let width = bottom_right.0 - upper_left.0;
        let height = bottom_right.1 - upper_left.1;

        let (fill_attr, stroke_attr) = if fill {
            (format!("fill: {}", color), "stroke: none".to_string())
        } else {
            (
                "fill: none".to_string(),
                format!("stroke: {}pt + {}", style.stroke_width(), color),
            )
        };

        let cmd = format!(
            "  #place(dx: {}pt, dy: {}pt, rect(width: {}pt, height: {}pt, {}, {}))",
            upper_left.0, upper_left.1, width, height, fill_attr, stroke_attr
        );
        self.write_command(&cmd);
        Ok(())
    }

    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        path: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }

        let points: Vec<_> = path.into_iter().collect();
        if points.len() < 2 {
            return Ok(());
        }

        // Draw as individual line segments to avoid auto-closing
        for window in points.windows(2) {
            let from = window[0];
            let to = window[1];
            self.draw_line(from, to, style)?;
        }

        Ok(())
    }

    fn fill_polygon<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        path: I,
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }

        let points: Vec<_> = path.into_iter().collect();
        if points.is_empty() {
            return Ok(());
        }

        let color = make_typst_color(style.color());

        let points_str = points
            .iter()
            .map(|(x, y)| format!("({}pt, {}pt)", x, y))
            .collect::<Vec<_>>()
            .join(", ");

        let cmd = format!(
            "  #place(polygon(fill: {}, stroke: none, {}))",
            color, points_str
        );
        self.write_command(&cmd);
        Ok(())
    }

    fn draw_circle<S: BackendStyle>(
        &mut self,
        center: BackendCoord,
        radius: u32,
        style: &S,
        fill: bool,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if style.color().alpha == 0.0 {
            return Ok(());
        }

        let color = make_typst_color(style.color());
        let (fill_attr, stroke_attr) = if fill {
            (format!("fill: {}", color), "stroke: none".to_string())
        } else {
            (
                "fill: none".to_string(),
                format!("stroke: {}pt + {}", style.stroke_width(), color),
            )
        };

        // Typst circle is positioned by center minus radius to get top-left
        let cmd = format!(
            "  #place(dx: {}pt, dy: {}pt, circle(radius: {}pt, {}, {}))",
            center.0 - radius as i32,
            center.1 - radius as i32,
            radius,
            fill_attr,
            stroke_attr
        );
        self.write_command(&cmd);
        Ok(())
    }

    fn draw_text<S: BackendTextStyle>(
        &mut self,
        text: &str,
        style: &S,
        pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let color = style.color();
        if color.alpha == 0.0 {
            return Ok(());
        }

        let (x0, y0) = pos;
        let text_color = make_typst_color(color);
        let font_size = style.size() / 1.24; // Similar adjustment as SVG backend
        let escaped_text = Self::escape_text(text);

        // Map generic font families to Typst fonts
        let family_str = style.family();
        let font_family = match family_str.as_str() {
            "sans-serif" => "Liberation Sans",
            "serif" => "Liberation Serif",
            "monospace" => "Liberation Mono",
            other => other,
        };

        // For vertical alignment, we use top-edge and bottom-edge
        // top-edge accepts: "ascender", "cap-height", "x-height", "baseline", "bounds", or length
        // bottom-edge accepts: "baseline", "descender", "bounds", or length
        let (top_edge, bottom_edge) = match style.anchor().v_pos {
            VPos::Top => ("\"bounds\"", "\"bounds\""),
            VPos::Center => ("\"cap-height\"", "\"baseline\""),
            VPos::Bottom => ("\"baseline\"", "\"baseline\""),
        };

        // Handle font style
        let font_weight = match style.style() {
            FontStyle::Bold => "\"bold\"",
            _ => "\"regular\"",
        };

        let font_style_attr = match style.style() {
            FontStyle::Italic | FontStyle::Oblique => "\"italic\"",
            _ => "\"normal\"",
        };

        // Handle rotation
        let rotation_attr = match style.transform() {
            FontTransform::Rotate90 => "rotate(90deg, ",
            FontTransform::Rotate180 => "rotate(180deg, ",
            FontTransform::Rotate270 => "rotate(270deg, ",
            _ => "",
        };

        let rotation_close = if rotation_attr.is_empty() { "" } else { ")" };

        // Use a simple approach: text in a box with manual horizontal alignment
        let aligned_text = match style.anchor().h_pos {
            HPos::Left => escaped_text.clone(),
            HPos::Right => {
                // Right align: measure and shift
                format!(
                    "#context {{ let m = measure([{}]); h(-m.width); [{}] }}",
                    escaped_text, escaped_text
                )
            }
            HPos::Center => {
                // Center align: measure and shift by half
                format!(
                    "#context {{ let m = measure([{}]); h(-m.width / 2); [{}] }}",
                    escaped_text, escaped_text
                )
            }
        };

        let cmd = format!(
            "  #place(dx: {}pt, dy: {}pt, {}box[#set text(size: {}pt, fill: {}, weight: {}, style: {}, font: \"{}\", top-edge: {}, bottom-edge: {}); {}]{})",
            x0,
            y0,
            rotation_attr,
            font_size,
            text_color,
            font_weight,
            font_style_attr,
            font_family,
            top_edge,
            bottom_edge,
            aligned_text,
            rotation_close
        );
        self.write_command(&cmd);
        Ok(())
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "image"))]
    fn blit_bitmap(
        &mut self,
        pos: BackendCoord,
        (w, h): (u32, u32),
        src: &[u8],
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        use image::codecs::png::PngEncoder;
        use image::ImageEncoder;
        use std::io::Cursor;

        let mut data = vec![];

        {
            let cursor = Cursor::new(&mut data);
            let encoder = PngEncoder::new(cursor);
            let color = image::ColorType::Rgb8;

            encoder.write_image(src, w, h, color).map_err(|e| {
                DrawingErrorKind::DrawingError(Error::new(
                    std::io::ErrorKind::Other,
                    format!("Image error: {}", e),
                ))
            })?;
        }

        // Convert to base64
        let base64_data = base64_encode(&data);

        let cmd = format!(
            "  #place(dx: {}pt, dy: {}pt, image.decode(\"data:image/png;base64,{}\", width: {}pt, height: {}pt))",
            pos.0, pos.1, base64_data, w, h
        );
        self.write_command(&cmd);
        Ok(())
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "image"))]
fn base64_encode(data: &[u8]) -> String {
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i + 2 < data.len() {
        let b1 = data[i];
        let b2 = data[i + 1];
        let b3 = data[i + 2];

        result.push(BASE64_CHARS[(b1 >> 2) as usize] as char);
        result.push(BASE64_CHARS[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize] as char);
        result.push(BASE64_CHARS[(((b2 & 0x0F) << 2) | (b3 >> 6)) as usize] as char);
        result.push(BASE64_CHARS[(b3 & 0x3F) as usize] as char);

        i += 3;
    }

    // Handle remaining bytes
    if i < data.len() {
        let b1 = data[i];
        result.push(BASE64_CHARS[(b1 >> 2) as usize] as char);

        if i + 1 < data.len() {
            let b2 = data[i + 1];
            result.push(BASE64_CHARS[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize] as char);
            result.push(BASE64_CHARS[((b2 & 0x0F) << 2) as usize] as char);
            result.push('=');
        } else {
            result.push(BASE64_CHARS[((b1 & 0x03) << 4) as usize] as char);
            result.push_str("==");
        }
    }

    result
}

impl Drop for TypstBackend<'_> {
    fn drop(&mut self) {
        if !self.saved {
            // drop should not panic, so we ignore a failed present
            let _ = self.present();
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use plotters::prelude::*;
    use plotters::style::text_anchor::{HPos, Pos, VPos};
    use std::fs;

    static DST_DIR: &str = "target/test/typst";

    fn checked_save_file(name: &str, content: &str) {
        /*
          Please use the Typst file to manually verify the results.
        */
        assert!(!content.is_empty());
        fs::create_dir_all(DST_DIR).unwrap();
        let file_name = format!("{}.typ", name);
        let file_path = std::path::Path::new(DST_DIR).join(file_name);
        println!("{:?} created", file_path);
        fs::write(file_path, &content).unwrap();
    }

    fn draw_mesh_with_custom_ticks(tick_size: i32, test_name: &str) {
        let mut content: String = Default::default();
        {
            let root = TypstBackend::with_string(&mut content, (500, 500)).into_drawing_area();

            let mut chart = ChartBuilder::on(&root)
                .caption("This is a test", ("sans-serif", 20u32))
                .set_all_label_area_size(40u32)
                .build_cartesian_2d(0..10, 0..10)
                .unwrap();

            chart
                .configure_mesh()
                .set_all_tick_mark_size(tick_size)
                .draw()
                .unwrap();
        }

        checked_save_file(test_name, &content);

        assert!(content.contains("This is a test"));
    }

    #[test]
    fn test_draw_mesh_no_ticks() {
        draw_mesh_with_custom_ticks(0, "test_draw_mesh_no_ticks");
    }

    #[test]
    fn test_draw_mesh_negative_ticks() {
        draw_mesh_with_custom_ticks(-10, "test_draw_mesh_negative_ticks");
    }

    #[test]
    fn test_text_alignments() {
        let mut content: String = Default::default();
        {
            let mut root = TypstBackend::with_string(&mut content, (500, 500));

            let style = TextStyle::from(("sans-serif", 20).into_font())
                .pos(Pos::new(HPos::Right, VPos::Top));
            root.draw_text("right-align", &style, (150, 50)).unwrap();

            let style = style.pos(Pos::new(HPos::Center, VPos::Top));
            root.draw_text("center-align", &style, (150, 150)).unwrap();

            let style = style.pos(Pos::new(HPos::Left, VPos::Top));
            root.draw_text("left-align", &style, (150, 200)).unwrap();
        }

        checked_save_file("test_text_alignments", &content);

        assert!(content.contains("right-align"));
        assert!(content.contains("center-align"));
        assert!(content.contains("left-align"));
        // Right and center aligned text will have measure() calls
        assert!(content.contains("measure("));
    }

    #[test]
    fn test_text_draw() {
        let mut content: String = Default::default();
        {
            let root = TypstBackend::with_string(&mut content, (1500, 800)).into_drawing_area();
            let root = root
                .titled("Image Title", ("sans-serif", 60).into_font())
                .unwrap();

            let mut chart = ChartBuilder::on(&root)
                .caption("All anchor point positions", ("sans-serif", 20u32))
                .set_all_label_area_size(40u32)
                .build_cartesian_2d(0..100i32, 0..50i32)
                .unwrap();

            chart
                .configure_mesh()
                .disable_x_mesh()
                .disable_y_mesh()
                .x_desc("X Axis")
                .y_desc("Y Axis")
                .draw()
                .unwrap();

            let ((x1, y1), (x2, y2), (x3, y3)) = ((-30, 30), (0, -30), (30, 30));

            for (dy, trans) in [
                FontTransform::None,
                FontTransform::Rotate90,
                FontTransform::Rotate180,
                FontTransform::Rotate270,
            ]
            .iter()
            .enumerate()
            {
                for (dx1, h_pos) in [HPos::Left, HPos::Right, HPos::Center].iter().enumerate() {
                    for (dx2, v_pos) in [VPos::Top, VPos::Center, VPos::Bottom].iter().enumerate() {
                        let x = 150_i32 + (dx1 as i32 * 3 + dx2 as i32) * 150;
                        let y = 120 + dy as i32 * 150;
                        let draw = |x, y, text| {
                            root.draw(&Circle::new((x, y), 3, &BLACK.mix(0.5))).unwrap();
                            let style = TextStyle::from(("sans-serif", 20).into_font())
                                .pos(Pos::new(*h_pos, *v_pos))
                                .transform(trans.clone());
                            root.draw_text(text, &style, (x, y)).unwrap();
                        };
                        draw(x + x1, y + y1, "dood");
                        draw(x + x2, y + y2, "dog");
                        draw(x + x3, y + y3, "goog");
                    }
                }
            }
        }

        checked_save_file("test_text_draw", &content);

        // Text appears twice for center/right aligned text (once in measure, once displayed)
        // So we expect more than 36 occurrences
        assert!(content.matches("dog").count() >= 36);
        assert!(content.matches("dood").count() >= 36);
        assert!(content.matches("goog").count() >= 36);
    }

    #[test]
    fn test_text_clipping() {
        let mut content: String = Default::default();
        {
            let (width, height) = (500_i32, 500_i32);
            let root = TypstBackend::with_string(&mut content, (width as u32, height as u32))
                .into_drawing_area();

            let style = TextStyle::from(("sans-serif", 20).into_font())
                .pos(Pos::new(HPos::Center, VPos::Center));
            root.draw_text("TOP LEFT", &style, (0, 0)).unwrap();
            root.draw_text("TOP CENTER", &style, (width / 2, 0))
                .unwrap();
            root.draw_text("TOP RIGHT", &style, (width, 0)).unwrap();

            root.draw_text("MIDDLE LEFT", &style, (0, height / 2))
                .unwrap();
            root.draw_text("MIDDLE RIGHT", &style, (width, height / 2))
                .unwrap();

            root.draw_text("BOTTOM LEFT", &style, (0, height)).unwrap();
            root.draw_text("BOTTOM CENTER", &style, (width / 2, height))
                .unwrap();
            root.draw_text("BOTTOM RIGHT", &style, (width, height))
                .unwrap();
        }

        checked_save_file("test_text_clipping", &content);
    }

    #[test]
    fn test_series_labels() {
        let mut content = String::default();
        {
            let (width, height) = (500, 500);
            let root = TypstBackend::with_string(&mut content, (width, height)).into_drawing_area();

            let mut chart = ChartBuilder::on(&root)
                .caption("All series label positions", ("sans-serif", 20u32))
                .set_all_label_area_size(40u32)
                .build_cartesian_2d(0..50i32, 0..50i32)
                .unwrap();

            chart
                .configure_mesh()
                .disable_x_mesh()
                .disable_y_mesh()
                .draw()
                .unwrap();

            chart
                .draw_series(std::iter::once(Circle::new((5, 15), 5u32, &RED)))
                .expect("Drawing error")
                .label("Series 1")
                .legend(|(x, y)| Circle::new((x, y), 3u32, RED.filled()));

            chart
                .draw_series(std::iter::once(Circle::new((5, 15), 10u32, &BLUE)))
                .expect("Drawing error")
                .label("Series 2")
                .legend(|(x, y)| Circle::new((x, y), 3u32, BLUE.filled()));

            for pos in vec![
                SeriesLabelPosition::UpperLeft,
                SeriesLabelPosition::MiddleLeft,
                SeriesLabelPosition::LowerLeft,
                SeriesLabelPosition::UpperMiddle,
                SeriesLabelPosition::MiddleMiddle,
                SeriesLabelPosition::LowerMiddle,
                SeriesLabelPosition::UpperRight,
                SeriesLabelPosition::MiddleRight,
                SeriesLabelPosition::LowerRight,
                SeriesLabelPosition::Coordinate(70, 70),
            ]
            .into_iter()
            {
                chart
                    .configure_series_labels()
                    .border_style(&BLACK.mix(0.5))
                    .position(pos)
                    .draw()
                    .expect("Drawing error");
            }
        }

        checked_save_file("test_series_labels", &content);
    }

    #[test]
    fn test_draw_pixel_alphas() {
        let mut content = String::default();
        {
            let (width, height) = (100_i32, 100_i32);
            let root = TypstBackend::with_string(&mut content, (width as u32, height as u32))
                .into_drawing_area();
            root.fill(&WHITE).unwrap();

            for i in -20..20 {
                let alpha = i as f64 * 0.1;
                root.draw_pixel((50 + i, 50 + i), &BLACK.mix(alpha))
                    .unwrap();
            }
        }

        checked_save_file("test_draw_pixel_alphas", &content);
    }

    #[test]
    fn test_simple_drawing() {
        let mut content: String = Default::default();
        {
            let mut backend = TypstBackend::with_string(&mut content, (500, 500));

            // Draw a simple rectangle
            backend
                .draw_rect((10, 10), (100, 100), &RGBColor(255, 0, 0), true)
                .unwrap();

            backend.present().unwrap();
        }

        checked_save_file("test_simple_drawing", &content);
        assert!(content.contains("rect"));
        assert!(content.contains("rgb(255, 0, 0)"));
    }

    #[test]
    fn test_draw_line() {
        let mut content = String::default();
        {
            let mut backend = TypstBackend::with_string(&mut content, (300, 300));

            backend
                .draw_line((10, 10), (100, 100), &RGBColor(0, 255, 0))
                .unwrap();

            backend.present().unwrap();
        }

        checked_save_file("test_draw_line", &content);
        assert!(content.contains("line"));
        assert!(content.contains("rgb(0, 255, 0)"));
    }

    #[test]
    fn test_draw_circle() {
        let mut content = String::default();
        {
            let mut backend = TypstBackend::with_string(&mut content, (300, 300));

            // Filled circle
            backend
                .draw_circle((150, 150), 50, &RGBColor(0, 0, 255), true)
                .unwrap();

            backend.present().unwrap();
        }

        checked_save_file("test_draw_circle", &content);
        assert!(content.contains("circle"));
        assert!(content.contains("rgb(0, 0, 255)"));
    }

    #[test]
    fn test_draw_polygon() {
        let mut content = String::default();
        {
            let mut backend = TypstBackend::with_string(&mut content, (300, 300));

            let points = vec![(50, 50), (100, 50), (75, 100)];
            backend
                .fill_polygon(points, &RGBColor(255, 128, 0))
                .unwrap();

            backend.present().unwrap();
        }

        checked_save_file("test_draw_polygon", &content);
        assert!(content.contains("polygon"));
        assert!(content.contains("rgb(255, 128, 0)"));
    }
}

