//! QR code d'appairage des smartphones (port de `common/util/wifi_code.cpp`).
//!
//! Le texte encodé indique au smartphone le réseau Wi-Fi à rejoindre, l'adresse
//! du serveur HTTP de l'application, l'identité de l'utilisateur (arbitre,
//! administrateur, piste) et la clé qui chiffre ses messages :
//! `[SSID-chiffrement-phrase]ip:port-nom-réf-clé`.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;

use crate::flash_code::{get_key256, FlashCode};
use crate::wifi_network::WifiNetwork;

/// Premier port candidat (`ClaimIpPort`) ; c'est aussi le port d'annonce UDP.
pub const FIRST_IP_PORT: u16 = 35830;
/// Borne haute (exclue) des ports candidats.
pub const LAST_IP_PORT: u16 = 35840;

static WIFI_NETWORK: Mutex<Option<WifiNetwork>> = Mutex::new(None);
static PORT: AtomicU16 = AtomicU16::new(0);

/// Identité du joueur (arbitre) associé à un QR code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerIdentity {
    pub name: String,
    pub reference: u32,
}

#[derive(Debug, Default)]
pub struct WifiCode {
    user_name: Option<String>,
    player: Mutex<Option<PlayerIdentity>>,
    key: Mutex<Option<String>>,
}

impl WifiCode {
    /// `WifiCode (user_name)` : ex. « Administrator », « Piste ».
    pub fn new(user_name: &str) -> Self {
        WifiCode { user_name: Some(user_name.to_string()), ..Default::default() }
    }

    /// `WifiCode (Player *)` : QR code d'un arbitre.
    pub fn for_player(name: &str, reference: u32) -> Self {
        WifiCode {
            user_name: None,
            player: Mutex::new(Some(PlayerIdentity { name: name.to_string(), reference })),
            key: Mutex::new(None),
        }
    }

    /// Met à jour le nom et la référence de l'arbitre (le C++ les relit à
    /// chaque `GetText`).
    pub fn set_player(&self, name: &str, reference: u32) {
        *self.player.lock().unwrap() = Some(PlayerIdentity { name: name.to_string(), reference });
    }

    pub fn is_player_code(&self) -> bool {
        self.player.lock().unwrap().is_some()
    }

    /// `SetWifiNetwork` : réseau partagé par tous les QR codes (statique en C++).
    pub fn set_wifi_network(network: Option<WifiNetwork>) {
        *WIFI_NETWORK.lock().unwrap() = network;
    }

    pub fn wifi_network() -> Option<WifiNetwork> {
        WIFI_NETWORK.lock().unwrap().clone()
    }

    /// `ClaimIpPort` : premier port TCP libre entre 35830 et 35839, retenu
    /// pour le serveur HTTP de l'application (0 si aucun n'est libre).
    pub fn claim_ip_port() -> u16 {
        let mut port = 0;
        for candidate in FIRST_IP_PORT..LAST_IP_PORT {
            match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, candidate)) {
                Ok(_) => {
                    port = candidate;
                    break;
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::AddrInUse {
                        log::warn!("WifiCode::ClaimIpPort: {candidate}: {e}");
                    }
                }
            }
        }
        PORT.store(port, Ordering::SeqCst);
        port
    }

    /// Port réservé par le dernier [`WifiCode::claim_ip_port`].
    pub fn ip_port() -> u16 {
        PORT.load(Ordering::SeqCst)
    }

    /// Impose le port annoncé (ex. port effectivement ouvert par le serveur HTTP).
    pub fn set_ip_port(port: u16) {
        PORT.store(port, Ordering::SeqCst);
    }

    /// `GetKey` : clé du QR code, tirée au premier appel. Pour un arbitre, le
    /// C++ recopie alors la clé dans l'attribut `password` : l'appelant peut
    /// le faire en testant [`WifiCode::has_key`] avant l'appel.
    pub fn key(&self) -> String {
        let mut key = self.key.lock().unwrap();
        key.get_or_insert_with(get_key256).clone()
    }

    pub fn has_key(&self) -> bool {
        self.key.lock().unwrap().is_some()
    }

    /// Impose la clé (ex. mot de passe d'arbitre relu dans un fichier).
    pub fn set_key(&self, key: Option<&str>) {
        *self.key.lock().unwrap() = key.map(str::to_string);
    }

    /// `ResetKey` : une nouvelle clé sera tirée au prochain [`WifiCode::key`].
    pub fn reset_key(&self) {
        *self.key.lock().unwrap() = None;
    }

    /// `GetNetwork` : `[SSID-chiffrement-phrase]` ou chaîne vide.
    fn network_text() -> String {
        match &*WIFI_NETWORK.lock().unwrap() {
            Some(n) if n.is_valid() => format!(
                "[{}-{}-{}]",
                n.ssid().unwrap_or("(null)"),
                n.encryption().unwrap_or("(null)"),
                n.passphrase().unwrap_or("(null)")
            ),
            _ => String::new(),
        }
    }
}

impl FlashCode for WifiCode {
    fn text(&self) -> String {
        let network = Self::network_text();
        let ip = match crate::ring::Ring::broker() {
            Some(ring) => ring.ipv4_address(),
            None => crate::netutil::guess_ipv4().unicast.to_string(),
        };
        let port = Self::ip_port();
        let key = self.key();
        match &*self.player.lock().unwrap() {
            Some(p) => format!("{network}{ip}:{port}-{}-{}-{key}", p.name, p.reference),
            None => format!("{network}{ip}:{port}-{}-0-{key}", self.user_name.as_deref().unwrap_or("(null)")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_layout() {
        let code = WifiCode::new("Piste");
        code.set_key(Some("KEY"));
        let text = code.text();
        assert!(text.ends_with("-Piste-0-KEY"), "{text}");
        let referee = WifiCode::for_player("DUPONT Jean", 42);
        assert!(!referee.has_key());
        let k = referee.key();
        assert_eq!(k.len(), 32);
        assert!(referee.text().ends_with(&format!("-DUPONT Jean-42-{k}")));
    }
}
