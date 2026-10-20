//! Compétition (port de `supervisor/contest.cpp`) — portage en cours.

pub mod grand;
pub mod html;
pub mod players;
pub mod teams;

/// Décode un document XML : UTF-8, ou ISO-8859-1 si déclaré ou invalide.
pub fn decode_xml_bytes(bytes: &[u8]) -> String {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_ascii_lowercase();
    if !head.contains("iso-8859-1") {
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.to_string();
        }
    }
    bytes.iter().map(|b| *b as char).collect()
}
