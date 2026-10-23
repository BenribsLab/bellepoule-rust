//! Petits utilitaires réseau partagés par les modules de `bp-net` :
//! détection de l'adresse IPv4 locale (`Ring::GuessIpV4Addresses`),
//! échappement d'URI (`g_uri_escape_string`), dossier de configuration
//! utilisateur (`g_get_user_config_dir`) et codes couleur ANSI des traces.

use std::net::Ipv4Addr;
use std::path::PathBuf;

/// Codes couleur utilisés par les traces de la console (`object.hpp`).
pub mod color {
    pub const RED: &str = "\x1b[1;31m";
    pub const GREEN: &str = "\x1b[1;32m";
    pub const YELLOW: &str = "\x1b[1;33m";
    pub const BLUE: &str = "\x1b[1;34m";
    pub const MAGENTA: &str = "\x1b[1;35m";
    pub const CYAN: &str = "\x1b[1;36m";
    pub const ESC: &str = "\x1b[0m";
}

/// Adresses IPv4 retenues pour l'anneau : adresse unicast de la machine
/// et adresse de diffusion du sous-réseau correspondant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ipv4Guess {
    pub unicast: Ipv4Addr,
    pub broadcast: Ipv4Addr,
}

/// Port de `Ring::GuessIpV4Addresses` : première interface IPv4 active qui
/// n'est pas la boucle locale. Sans interface, on se replie sur 127.0.0.1
/// (adresse unicast et adresse de diffusion confondues, comme le C++).
pub fn guess_ipv4() -> Ipv4Guess {
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if let if_addrs::IfAddr::V4(v4) = &iface.addr {
                if v4.ip.is_loopback() || v4.ip.is_unspecified() {
                    continue;
                }
                let broadcast = v4.broadcast.unwrap_or_else(|| {
                    // ip | ~mask (calcul fait par le C++ sous Windows)
                    let ip = u32::from(v4.ip);
                    let mask = u32::from(v4.netmask);
                    Ipv4Addr::from(ip | !mask)
                });
                return Ipv4Guess { unicast: v4.ip, broadcast };
            }
        }
    }
    Ipv4Guess { unicast: Ipv4Addr::LOCALHOST, broadcast: Ipv4Addr::LOCALHOST }
}

/// `g_uri_escape_string (s, NULL, FALSE)` : tout est échappé sauf les
/// caractères non réservés `A-Z a-z 0-9 - . _ ~`.
pub fn uri_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Décodage des séquences `%XX` (chemins d'URL FTP, requêtes HTTP).
pub fn uri_unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `g_path_get_basename` simplifié (séparateurs `/` et `\`).
pub fn basename(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(i) => trimmed[i + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

/// `g_get_user_config_dir` : `%LOCALAPPDATA%` sous Windows,
/// `$XDG_CONFIG_HOME` ou `~/.config` ailleurs.
pub fn user_config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(dir);
        }
        if let Some(dir) = std::env::var_os("APPDATA") {
            return PathBuf::from(dir);
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
            if !dir.is_empty() {
                return PathBuf::from(dir);
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".config");
        }
    }
    std::env::temp_dir()
}

/// Lecture d'un entier non signé comme `g_key_file_get_uint64`
/// (tolère les espaces, renvoie 0 en cas d'erreur).
pub fn parse_u32(s: &str) -> u32 {
    s.trim().parse::<i64>().map(|v| v as u32).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape() {
        assert_eq!(uri_escape("Mon fichier é.cotcot"), "Mon%20fichier%20%C3%A9.cotcot");
        assert_eq!(uri_unescape("Mon%20fichier%20%C3%A9.cotcot"), "Mon fichier é.cotcot");
        assert_eq!(uri_unescape("50%"), "50%");
        assert_eq!(basename("C:\\a\\b\\c.cotcot"), "c.cotcot");
        assert_eq!(basename("/a/b/c.cotcot"), "c.cotcot");
    }
}
