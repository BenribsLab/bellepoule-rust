//! QR code de connexion de l'arbitre (port de `common/util/wifi_code.cpp` et
//! `flash_code.cpp`, partie compilée avec `-DLIVE_POOL`).
//!
//! Le smartphone de l'arbitre scanne ce code pour connaître l'adresse du
//! boîtier et la clé AES-256 qui chiffre les messages qu'il lui envoie.
//! Texte encodé : `{réseau}{ip}:{port}-Piste-0-{clé}` où `{réseau}` est vide
//! (LivePoule ne publie pas de réseau Wi-Fi dans le code).

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::{Arc, Mutex};

use qrcode::{Color, EcLevel, QrCode};
use rand::Rng;

/// Premier port essayé par `ClaimIpPort`.
pub const FIRST_PORT: u16 = 35830;
/// Port de fin (exclu).
pub const LAST_PORT: u16 = 35840;

/// `WifiCode::ClaimIpPort` : premier port TCP libre de 35830 à 35839
/// (0 si aucun).
pub fn claim_ip_port() -> u16 {
    use socket2::{Domain, Socket, Type};

    for port in FIRST_PORT..LAST_PORT {
        let Ok(socket) = Socket::new(Domain::IPV4, Type::STREAM, None) else {
            break;
        };
        // g_socket_bind (allow_reuse = TRUE) : SO_REUSEADDR sous Unix
        // uniquement (sous Windows, SO_REUSEADDR autoriserait un port occupé).
        #[cfg(unix)]
        let _ = socket.set_reuse_address(true);
        let address = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
        match socket.bind(&address.into()) {
            Ok(()) => return port,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::AddrInUse {
                    log::warn!("g_socket_bind: {e}");
                }
            }
        }
    }
    0
}

/// `FlashCode::GetKey256` : 32 caractères pris dans `0-8`, `A-Y` ou `a-y`
/// (`g_rand_int_range` exclut la borne haute, d'où l'absence de `9`, `Z`, `z`).
pub fn generate_key256() -> String {
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let (low, high) = match rng.gen_range(0..3) {
                0 => (b'0', b'9'),
                1 => (b'A', b'Z'),
                _ => (b'a', b'z'),
            };
            rng.gen_range(low..high) as char
        })
        .collect()
}

/// Clé secrète partagée entre l'écran (QR code) et le serveur HTTP
/// (`GetSecretKey`). Générée à la première demande.
#[derive(Clone, Default)]
pub struct SecretKey(Arc<Mutex<Option<String>>>);

impl SecretKey {
    /// `WifiCode::GetKey`.
    pub fn get(&self) -> String {
        let mut key = self.0.lock().unwrap_or_else(|e| e.into_inner());
        key.get_or_insert_with(generate_key256).clone()
    }

    /// `WifiCode::ResetKey`.
    pub fn reset(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Code de connexion (`WifiCode ("Piste")`).
#[derive(Clone)]
pub struct WifiCode {
    user_name: String,
    key: SecretKey,
}

impl WifiCode {
    pub fn new(user_name: &str) -> Self {
        WifiCode { user_name: user_name.to_string(), key: SecretKey::default() }
    }

    pub fn secret_key(&self) -> SecretKey {
        self.key.clone()
    }

    pub fn reset_key(&self) {
        self.key.reset();
    }

    pub fn key(&self) -> String {
        self.key.get()
    }

    /// `WifiCode::GetText`.
    pub fn text(&self, ip: &str, port: u16) -> String {
        let network = "";
        format!("{network}{ip}:{port}-{}-0-{}", self.user_name, self.key())
    }
}

/// Image RVB d'un QR code (`FlashCode::GetPixbuf`).
#[derive(Debug, Clone)]
pub struct QrImage {
    /// Largeur = hauteur, en pixels.
    pub size: usize,
    /// Pixels RVB, `size * 3` octets par ligne.
    pub rgb: Vec<u8>,
}

/// Encode `text` (niveau de correction L) avec une marge de 4 modules,
/// chaque module faisant `pixel_size` pixels.
pub fn render_qr(text: &str, pixel_size: usize) -> Option<QrImage> {
    let code = match QrCode::with_error_correction_level(text.as_bytes(), EcLevel::L) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("QR code : {e}");
            return None;
        }
    };
    const MARGIN: usize = 4;
    let width = code.width();
    let colors = code.to_colors();
    let modules = width + 2 * MARGIN;
    let size = modules * pixel_size;
    let mut rgb = vec![0xFFu8; size * size * 3];
    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == Color::Dark {
                for dy in 0..pixel_size {
                    let row = (y + MARGIN) * pixel_size + dy;
                    let start = (row * size + (x + MARGIN) * pixel_size) * 3;
                    rgb[start..start + pixel_size * 3].fill(0);
                }
            }
        }
    }
    Some(QrImage { size, rgb })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_alphabet() {
        for _ in 0..50 {
            let k = generate_key256();
            assert_eq!(k.len(), 32);
            assert!(k.chars().all(|c| matches!(c, '0'..='8' | 'A'..='Y' | 'a'..='y')));
        }
    }

    #[test]
    fn key_is_stable_until_reset() {
        let code = WifiCode::new("Piste");
        let k1 = code.key();
        assert_eq!(code.key(), k1);
        assert_eq!(code.secret_key().get(), k1);
        code.reset_key();
        assert_ne!(code.key(), k1);
    }

    #[test]
    fn text_format() {
        let code = WifiCode::new("Piste");
        let key = code.key();
        assert_eq!(code.text("192.168.1.20", 35830), format!("192.168.1.20:35830-Piste-0-{key}"));
    }

    #[test]
    fn qr_image() {
        let img = render_qr("192.168.1.20:35830-Piste-0-ABC", 3).unwrap();
        assert_eq!(img.rgb.len(), img.size * img.size * 3);
        assert_eq!(img.size % 3, 0);
        // la marge est blanche, le coin du motif de repérage est noir
        assert_eq!(img.rgb[0], 0xFF);
        let corner = ((4 * 3) * img.size + 4 * 3) * 3;
        assert_eq!(img.rgb[corner], 0);
    }
}
