//! QR codes (port de `common/util/flash_code.cpp`).
//!
//! Le C++ encode le texte avec libqrencode (`QRcode_encodeString`, version
//! automatique, correction `L`, mode 8 bits), ajoute une marge blanche de
//! 4 modules puis agrandit l'image (`gdk_pixbuf_scale_simple`, plus proche
//! voisin). Ici on produit la même image en mémoire (RGB 8 bits) : l'interface
//! GTK la convertit en `GdkTexture`, ou l'on peut l'exporter en PNG.

use qrcode::bits::Bits;
use qrcode::{Color, EcLevel, QrCode, Version};

/// Marge blanche autour du code, en modules.
pub const MARGIN: usize = 4;
/// Taille d'un module en pixels par défaut (`GetPixbuf (guint pixel_size = 3)`).
pub const DEFAULT_PIXEL_SIZE: u32 = 3;

/// Matrice d'un QR code : `width × width` modules, `true` = module noir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrMatrix {
    pub width: usize,
    pub modules: Vec<bool>,
}

impl QrMatrix {
    /// Encode `text` en mode octets, correction `L`, plus petite version possible.
    pub fn encode(text: &str) -> Option<QrMatrix> {
        let data = text.as_bytes();
        for v in 1..=40 {
            let mut bits = Bits::new(Version::Normal(v));
            if bits.push_byte_data(data).is_err() {
                continue;
            }
            if bits.push_terminator(EcLevel::L).is_err() {
                continue;
            }
            if let Ok(code) = QrCode::with_bits(bits, EcLevel::L) {
                let width = code.width();
                let modules = code.to_colors().into_iter().map(|c| c == Color::Dark).collect();
                return Some(QrMatrix { width, modules });
            }
        }
        None
    }

    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.width + x]
    }

    /// Image RGB avec la marge, agrandie de `pixel_size` (`GetPixbuf`).
    pub fn to_image(&self, pixel_size: u32) -> QrImage {
        let pixel_size = pixel_size.max(1) as usize;
        let small = self.width + 2 * MARGIN;
        let size = small * pixel_size;
        let mut rgb = vec![0xFFu8; size * size * 3];
        for y in 0..size {
            let my = y / pixel_size;
            for x in 0..size {
                let mx = x / pixel_size;
                let dark = mx >= MARGIN
                    && my >= MARGIN
                    && mx < MARGIN + self.width
                    && my < MARGIN + self.width
                    && self.is_dark(mx - MARGIN, my - MARGIN);
                if dark {
                    let o = (y * size + x) * 3;
                    rgb[o] = 0;
                    rgb[o + 1] = 0;
                    rgb[o + 2] = 0;
                }
            }
        }
        QrImage { width: size as u32, height: size as u32, rgb }
    }
}

/// Image RGB 8 bits sans canal alpha (équivalent du `GdkPixbuf` du C++).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrImage {
    pub width: u32,
    pub height: u32,
    /// Pixels RGB ligne par ligne, `stride = width * 3`.
    pub rgb: Vec<u8>,
}

impl QrImage {
    pub fn stride(&self) -> usize {
        self.width as usize * 3
    }

    /// Encodage PNG de l'image.
    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            if let Ok(mut writer) = encoder.write_header() {
                if let Err(e) = writer.write_image_data(&self.rgb) {
                    log::warn!("QrImage::to_png: {e}");
                }
            }
        }
        out
    }
}

/// Classe de base `FlashCode` : tout objet qui sait produire le texte de son
/// QR code. `Credentials` et `WifiCode` redéfinissent [`FlashCode::text`].
pub trait FlashCode {
    /// `GetText` : texte encodé dans le QR code.
    fn text(&self) -> String;

    /// Matrice du QR code.
    fn qr_matrix(&self) -> Option<QrMatrix> {
        QrMatrix::encode(&self.text())
    }

    /// `GetPixbuf (pixel_size)` : image RGB prête à afficher.
    fn image(&self, pixel_size: u32) -> Option<QrImage> {
        self.qr_matrix().map(|m| m.to_image(pixel_size))
    }

    /// Image encodée en PNG.
    fn png(&self, pixel_size: u32) -> Option<Vec<u8>> {
        self.image(pixel_size).map(|i| i.to_png())
    }
}

/// `FlashCode (text)` : QR code d'un texte fixe (ex. URL du site web).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextFlashCode {
    pub text: Option<String>,
}

impl TextFlashCode {
    pub fn new(text: Option<&str>) -> Self {
        TextFlashCode { text: text.map(str::to_string) }
    }
}

impl FlashCode for TextFlashCode {
    fn text(&self) -> String {
        self.text.clone().unwrap_or_default()
    }
}

/// `FlashCode::GetKey256` : 32 caractères tirés au hasard parmi les chiffres,
/// majuscules et minuscules. Comme `g_rand_int_range` exclut la borne haute,
/// les caractères `9`, `Z` et `z` ne sont jamais produits (fidèle au C++).
pub fn get_key256() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..crate::cryptor::KEY_LEN)
        .map(|_| match rng.gen_range(0..3) {
            0 => rng.gen_range(b'0'..b'9') as char,
            1 => rng.gen_range(b'A'..b'Z') as char,
            _ => rng.gen_range(b'a'..b'z') as char,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key256() {
        let k = get_key256();
        assert_eq!(k.len(), 32);
        assert!(k.chars().all(|c| c.is_ascii_alphanumeric() && c != '9' && c != 'Z' && c != 'z'));
    }

    #[test]
    fn qr_image() {
        let code = TextFlashCode::new(Some("http://192.168.1.10:8080"));
        let m = code.qr_matrix().unwrap();
        assert_eq!(m.width, 25); // 24 octets en mode 8 bits, niveau L → version 2
        let img = m.to_image(3);
        assert_eq!(img.width as usize, (25 + 8) * 3);
        assert_eq!(img.rgb.len(), img.stride() * img.height as usize);
        // la marge est blanche, le coin du motif de repérage est noir
        assert_eq!(&img.rgb[0..3], &[0xFF, 0xFF, 0xFF]);
        let o = (MARGIN * 3 * img.width as usize + MARGIN * 3) * 3;
        assert_eq!(&img.rgb[o..o + 3], &[0, 0, 0]);
        let png = img.to_png();
        assert_eq!(&png[1..4], b"PNG");
    }
}
