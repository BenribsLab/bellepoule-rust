//! Couleurs du canevas (`fill-color`, `stroke-color` de GooCanvas).

use gtk4 as gtk;

/// Couleur RVBA, composantes entre 0 et 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub fn rgb(r: f64, g: f64, b: f64) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    /// `goo_canvas` : `0xRRGGBBAA`.
    pub fn from_rgba(c: u32) -> Color {
        Color {
            r: ((c >> 24) & 0xff) as f64 / 255.0,
            g: ((c >> 16) & 0xff) as f64 / 255.0,
            b: ((c >> 8) & 0xff) as f64 / 255.0,
            a: (c & 0xff) as f64 / 255.0,
        }
    }

    /// Nom (`red`, `dark grey`…), `#rgb`, `#rrggbb`, `#rrrrggggbbbb` ou
    /// `rgb(…)`. Une couleur illisible est noire, comme `gdk_color_parse`
    /// dont le résultat n'était pas vérifié.
    pub fn parse(s: &str) -> Color {
        let t = s.trim();
        let lowered = t.to_ascii_lowercase().replace(' ', "");
        gtk::gdk::RGBA::parse(&lowered)
            .or_else(|_| gtk::gdk::RGBA::parse(t))
            .map(|c| Color { r: c.red() as f64, g: c.green() as f64, b: c.blue() as f64, a: c.alpha() as f64 })
            .unwrap_or(Color::BLACK)
    }

    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", (self.r * 255.0).round() as u8, (self.g * 255.0).round() as u8, (self.b * 255.0).round() as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_and_hex() {
        let c = Color::from_rgba(0xff8000ff);
        assert!((c.r - 1.0).abs() < 1e-9 && (c.g - 128.0 / 255.0).abs() < 1e-9 && c.b == 0.0 && c.a == 1.0);
        assert_eq!(c.to_hex(), "#ff8000");
    }

    #[test]
    fn bad_names_are_black() {
        assert_eq!(Color::parse("pas une couleur"), Color::BLACK);
    }
}
