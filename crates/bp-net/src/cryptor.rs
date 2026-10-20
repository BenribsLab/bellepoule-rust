//! Chiffrement symétrique des messages (port de `common/network/cryptor.cpp`).
//!
//! Mêmes paramètres que le C++ (OpenSSL `EVP_aes_256_cbc`) :
//! - AES-256 en mode CBC, remplissage PKCS#7 ;
//! - la clé est la phrase secrète brute : ses 32 premiers octets (complétés
//!   par des zéros si elle est plus courte) ;
//! - le texte clair chiffré inclut le zéro terminal de la chaîne C ;
//! - le vecteur d'initialisation (16 octets aléatoires) et le texte chiffré
//!   circulent encodés en base64 standard.

use aes::Aes256;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;

/// Taille d'une clé AES-256 (et des phrases secrètes `GetKey256`).
pub const KEY_LEN: usize = 256 / 8;
/// Taille d'un bloc AES et du vecteur d'initialisation.
pub const IV_LEN: usize = 16;

/// Chiffreur ; un IV imposé (base64) peut être fourni à la construction,
/// sinon un IV aléatoire est tiré à chaque chiffrement.
#[derive(Debug, Clone, Default)]
pub struct Cryptor {
    iv: Option<String>,
}

impl Cryptor {
    /// `Cryptor ()` : IV aléatoire à chaque appel de [`Cryptor::encrypt`].
    pub fn new() -> Self {
        Cryptor { iv: None }
    }

    /// `Cryptor (iv_b64)` : IV imposé (ex. mots de passe des arbitres).
    pub fn with_iv(iv_b64: Option<&str>) -> Self {
        Cryptor { iv: iv_b64.map(str::to_string) }
    }

    /// Chiffre `text` (zéro terminal inclus) avec `key`.
    /// Renvoie `(texte chiffré base64, IV base64)`.
    pub fn encrypt(&self, text: &str, key: &str) -> (String, String) {
        let iv = match &self.iv {
            Some(iv_b64) => fit::<IV_LEN>(&B64.decode(iv_b64.trim()).unwrap_or_default()),
            None => Self::generate_iv(),
        };
        let key = fit::<KEY_LEN>(key.as_bytes());
        let mut plain = Vec::with_capacity(text.len() + 1);
        plain.extend_from_slice(text.as_bytes());
        plain.push(0);
        let cipher = Aes256CbcEnc::new(&key.into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(&plain);
        (B64.encode(cipher), B64.encode(iv))
    }

    /// Déchiffre et renvoie les octets clairs bruts (zéro terminal compris).
    pub fn decrypt_bytes(data_b64: &str, iv_b64: &str, key: &str) -> Option<Vec<u8>> {
        let cipher = B64.decode(strip_b64(data_b64)).ok()?;
        let iv = fit::<IV_LEN>(&B64.decode(strip_b64(iv_b64)).ok()?);
        let key = fit::<KEY_LEN>(key.as_bytes());
        Aes256CbcDec::new(&key.into(), &iv.into()).decrypt_padded_vec_mut::<Pkcs7>(&cipher).ok()
    }

    /// `Cryptor::Decrypt` : texte clair jusqu'au premier zéro, ou `None`
    /// si le déchiffrement échoue (mauvaise clé, données altérées…).
    pub fn decrypt(data_b64: &str, iv_b64: &str, key: &str) -> Option<String> {
        let bytes = Self::decrypt_bytes(data_b64, iv_b64, key)?;
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    /// `Cryptor::GenerateIv` : 16 octets aléatoires.
    pub fn generate_iv() -> [u8; IV_LEN] {
        rand::random()
    }

    /// IV aléatoire déjà encodé en base64 (champ `_iv` des arbitres).
    pub fn generate_iv_b64() -> String {
        B64.encode(Self::generate_iv())
    }
}

/// Copie `src` dans un tableau de taille fixe, complété par des zéros
/// (le C++ lit directement `N` octets en mémoire).
fn fit<const N: usize>(src: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    let n = src.len().min(N);
    out[..n].copy_from_slice(&src[..n]);
    out
}

/// `g_base64_decode` ignore les caractères hors alphabet (retours à la ligne,
/// zéro terminal…) : on les retire avant décodage.
fn strip_b64(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUV";

    #[test]
    fn roundtrip() {
        let c = Cryptor::new();
        let (cipher, iv) = c.encrypt("tagada soinsoin", KEY);
        assert_eq!(B64.decode(&iv).unwrap().len(), 16);
        // 15 caractères + zéro terminal = 16 octets → 2 blocs avec PKCS#7
        assert_eq!(B64.decode(&cipher).unwrap().len(), 32);
        assert_eq!(Cryptor::decrypt(&cipher, &iv, KEY).as_deref(), Some("tagada soinsoin"));
        let raw = Cryptor::decrypt_bytes(&cipher, &iv, KEY).unwrap();
        assert_eq!(raw.last(), Some(&0));
    }

    #[test]
    fn wrong_key() {
        let (cipher, iv) = Cryptor::new().encrypt("[Header]\nname=Test\n", KEY);
        let other = "abcdefghijklmnopqrstuvwxyz012345";
        let res = Cryptor::decrypt(&cipher, &iv, other);
        assert_ne!(res.as_deref(), Some("[Header]\nname=Test\n"));
    }

    #[test]
    fn fixed_iv_is_deterministic() {
        let iv = Cryptor::generate_iv_b64();
        let c = Cryptor::with_iv(Some(&iv));
        let (a, iv_a) = c.encrypt("secret", KEY);
        let (b, iv_b) = c.encrypt("secret", KEY);
        assert_eq!(a, b);
        assert_eq!(iv_a, iv);
        assert_eq!(iv_b, iv);
        assert_eq!(Cryptor::decrypt(&a, &iv, KEY).as_deref(), Some("secret"));
    }

    #[test]
    fn short_key_is_zero_padded() {
        let (cipher, iv) = Cryptor::new().encrypt("abc", "short");
        let mut padded = String::from("short");
        padded.push_str(&"\0".repeat(27));
        assert_eq!(Cryptor::decrypt(&cipher, &iv, &padded).as_deref(), Some("abc"));
    }
}
