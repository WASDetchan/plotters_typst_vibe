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
        // Create a box with absolute positioning for the canvas
        writeln!(buf, "#box(width: {}pt, height: {}pt)[", size.0, size.1).unwrap();
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

        // Determine text alignment
        let h_align = match style.anchor().h_pos {
            HPos::Left => "left",
            HPos::Right => "right",
            HPos::Center => "center",
        };

        // Typst's baseline handling - adjust y position based on vertical alignment
        let v_offset = match style.anchor().v_pos {
            VPos::Top => font_size * 0.76,
            VPos::Center => font_size * 0.35,
            VPos::Bottom => 0.0,
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

        let cmd = format!(
            "  #place(dx: {}pt, dy: {}pt, {}text(size: {}pt, fill: {}, weight: {}, style: {}, font: \"{}\")[#align({})[\n    {}\n  ]]{})",
            x0,
            y0 - v_offset as i32,
            rotation_attr,
            font_size,
            text_color,
            font_weight,
            font_style_attr,
            font_family,
            h_align,
            escaped_text,
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

