//! Configuration Wi-Fi via `wpa_supplicant` (port de `LivePoule/wpa.cpp`).
//!
//! Sur le boîtier, l'organisateur dépose un fichier `bellepoule.wifi` à la
//! racine d'une clé USB (montée sur `/media/usb1`). Il s'agit d'un fichier
//! clé/valeur au format GKeyFile :
//!
//! ```ini
//! [wpa_supplicant]
//! ssid="MonReseau"
//! psk="secret"
//! ```
//!
//! Quand l'arbitre demande le QR code, LivePoule relit ce fichier ; s'il a
//! changé, il reconfigure `wpa_supplicant` par sa socket de contrôle Unix
//! (`/var/run/wpa_supplicant/wlan0`) : suppression des réseaux existants,
//! `ADD_NETWORK`, un `SET_NETWORK` par clé, `ENABLE_NETWORK`,
//! `SELECT_NETWORK` puis `SAVE_CONFIG`.
//!
//! Hors Raspberry Pi (comme le C++ compilé sans `WIRING_PI`), la lecture de
//! la clé USB est remplacée par une attente d'une seconde.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Fichier de configuration Wi-Fi sur la clé USB.
pub const WIFI_CONFIG_PATH: &str = "/media/usb1/bellepoule.wifi";
/// Socket de contrôle de `wpa_supplicant`.
pub const WPA_CTRL_PATH: &str = "/var/run/wpa_supplicant/wlan0";
/// Groupe lu dans le fichier de configuration.
pub const WPA_GROUP: &str = "wpa_supplicant";

const ESC: &str = "\x1b[0m";
const YELLOW: &str = "\x1b[1;33m";
const RED: &str = "\x1b[1;31m";

/// Canal de dialogue avec `wpa_supplicant`.
pub trait WpaTransport: Send {
    /// Envoie une requête et renvoie la réponse (`None` en cas d'erreur).
    fn exchange(&mut self, request: &str) -> Option<String>;
}

/// `atoi` : entier en tête de chaîne, 0 sinon.
fn atoi(s: &str) -> i64 {
    let s = s.trim_start();
    let (sign, digits) = match s.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, s.strip_prefix('+').unwrap_or(s)),
    };
    let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
    sign * digits[..end].parse::<i64>().unwrap_or(0)
}

/// `g_key_file_get_string` : déséchappement des valeurs.
fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Lit les clés (dans l'ordre du fichier) d'un groupe GKeyFile.
pub fn key_file_group(text: &str, group: &str) -> Result<Vec<(String, String)>, String> {
    let mut current: Option<String> = None;
    let mut found = false;
    let mut keys: Vec<(String, String)> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('[') {
            match rest.find(']') {
                Some(end) => {
                    let name = rest[..end].to_string();
                    found |= name == group;
                    current = Some(name);
                    continue;
                }
                None => return Err(format!("Key file contains line “{line}” which is not a key-value pair, group, or comment")),
            }
        }
        let Some(eq) = line.find('=') else {
            return Err(format!("Key file contains line “{line}” which is not a key-value pair, group, or comment"));
        };
        if current.is_none() {
            return Err(format!("Key file does not start with a group (ligne {})", n + 1));
        }
        if current.as_deref() == Some(group) {
            let key = line[..eq].trim().to_string();
            let value = unescape(line[eq + 1..].trim_start());
            match keys.iter_mut().find(|(k, _)| *k == key) {
                Some(e) => e.1 = value,
                None => keys.push((key, value)),
            }
        }
    }
    if found {
        Ok(keys)
    } else {
        Err(format!("Key file does not have group “{group}”"))
    }
}

/// `Wpa::Send` sur un transport éventuel, avec la trace console du C++.
fn send(transport: &mut Option<Box<dyn WpaTransport>>, request: &str) -> Option<String> {
    print!("{YELLOW}{request} {ESC}");
    let answer = match transport.as_mut() {
        Some(t) => t.exchange(request),
        None => {
            log::warn!("wpa_supplicant : pas de socket de contrôle");
            None
        }
    };
    match &answer {
        Some(a) => print!("{RED}{a}{ESC}"),
        None => println!(),
    }
    answer
}

/// `Wpa::AddNetwork` : identifiant du nouveau réseau (sans fin de ligne).
fn add_network(transport: &mut Option<Box<dyn WpaTransport>>) -> Option<String> {
    send(transport, "ADD_NETWORK").map(|mut id| {
        if let Some(nl) = id.find('\n') {
            id.truncate(nl);
        }
        id
    })
}

/// Séquence de reconfiguration (`ConfigurationThread`) pour les clés données.
pub fn apply_configuration(transport: &mut Option<Box<dyn WpaTransport>>, keys: &[(String, String)]) {
    {
        let last_network = add_network(transport);
        let last = atoi(last_network.as_deref().unwrap_or(""));
        for i in 1..=last {
            send(transport, &format!("REMOVE_NETWORK {i}"));
        }
    }

    let network_id = add_network(transport).unwrap_or_else(|| "(null)".to_string());

    for (key, value) in keys {
        send(transport, &format!("SET_NETWORK {network_id} {key} {value}"));
    }

    send(transport, &format!("ENABLE_NETWORK {network_id}"));
    send(transport, &format!("SELECT_NETWORK {network_id}"));
    send(transport, "SAVE_CONFIG");
}

#[cfg(unix)]
mod unix_transport {
    use super::WpaTransport;
    use std::os::unix::net::UnixDatagram;

    /// Socket datagramme Unix vers `wpa_supplicant` (`Wpa::OpenSocket`).
    pub struct UnixWpa {
        socket: UnixDatagram,
        path: String,
    }

    impl UnixWpa {
        pub fn open(ctrl_path: &str) -> Option<Self> {
            let path = format!("/tmp/wpa_ctrl_{}-1", std::process::id());
            let socket = match UnixDatagram::bind(&path) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("{e}");
                    match UnixDatagram::unbound() {
                        Ok(s) => s,
                        Err(e) => {
                            log::warn!("{e}");
                            return None;
                        }
                    }
                }
            };
            if let Err(e) = socket.connect(ctrl_path) {
                log::warn!("{ctrl_path}: {e}");
            }
            Some(UnixWpa { socket, path })
        }
    }

    impl WpaTransport for UnixWpa {
        fn exchange(&mut self, request: &str) -> Option<String> {
            // Le C++ envoie la chaîne avec son zéro terminal
            let mut data = request.as_bytes().to_vec();
            data.push(0);
            if let Err(e) = self.socket.send(&data) {
                log::warn!("{e}");
                return None;
            }
            let mut answer = [0u8; 100];
            match self.socket.recv(&mut answer[..99]) {
                Ok(n) => {
                    let n = answer[..n].iter().position(|b| *b == 0).unwrap_or(n);
                    Some(String::from_utf8_lossy(&answer[..n]).into_owned())
                }
                Err(e) => {
                    log::warn!("{e}");
                    None
                }
            }
        }
    }

    impl Drop for UnixWpa {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Ouvre la socket de contrôle de `wpa_supplicant`.
fn open_transport() -> Option<Box<dyn WpaTransport>> {
    #[cfg(unix)]
    {
        unix_transport::UnixWpa::open(WPA_CTRL_PATH).map(|t| Box::new(t) as Box<dyn WpaTransport>)
    }
    #[cfg(not(unix))]
    {
        log::warn!("{WPA_CTRL_PATH}: sockets Unix indisponibles sur cette plateforme");
        None
    }
}

struct WpaState {
    transport: Option<Box<dyn WpaTransport>>,
    network_config: Option<String>,
}

/// Gestionnaire Wi-Fi (`Wpa`).
#[derive(Clone)]
pub struct Wpa {
    state: Arc<Mutex<WpaState>>,
    read_usb_key: bool,
}

impl Wpa {
    /// `Wpa::Wpa` : ouvre la socket de contrôle. La configuration n'est lue
    /// sur la clé USB que sur le boîtier (feature `raspberry`).
    pub fn new() -> Self {
        Self::with_transport(open_transport(), crate::RASPBERRY)
    }

    /// Constructeur paramétrable (tests).
    pub fn with_transport(transport: Option<Box<dyn WpaTransport>>, read_usb_key: bool) -> Self {
        Wpa { state: Arc::new(Mutex::new(WpaState { transport, network_config: None })), read_usb_key }
    }

    /// `Wpa::GetNetworkConfig` : nouveau contenu de la configuration, s'il a changé.
    fn network_config(&self, previous: Option<&str>) -> Option<String> {
        if self.read_usb_key {
            match std::fs::read_to_string(WIFI_CONFIG_PATH) {
                Ok(config) => {
                    if previous != Some(config.as_str()) {
                        return Some(config);
                    }
                }
                Err(e) => log::warn!("{WIFI_CONFIG_PATH}: {e}"),
            }
        } else {
            std::thread::sleep(Duration::from_secs(1));
        }
        None
    }

    /// Applique une configuration (contenu du fichier `bellepoule.wifi`).
    pub fn configure_from(&self, new_config: String) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match key_file_group(&new_config, WPA_GROUP) {
            Err(e) => log::warn!("{e}"),
            Ok(keys) => {
                apply_configuration(&mut state.transport, &keys);
                state.network_config = Some(new_config);
            }
        }
    }

    /// Corps du `ConfigurationThread` (bloquant).
    pub fn configure_blocking(&self) {
        let previous = self.state.lock().unwrap_or_else(|e| e.into_inner()).network_config.clone();
        if let Some(new_config) = self.network_config(previous.as_deref()) {
            self.configure_from(new_config);
        }
    }

    /// `Wpa::ConfigureNetwork` : configuration dans un fil d'exécution
    /// séparé ; `done` est notifié à la fin (succès ou non).
    pub fn configure_network(&self, done: Sender<()>) {
        let wpa = self.clone();
        std::thread::spawn(move || {
            wpa.configure_blocking();
            let _ = done.send(());
        });
    }
}

impl Default for Wpa {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mock {
        log: Arc<Mutex<Vec<String>>>,
        next_id: u32,
    }

    impl WpaTransport for Mock {
        fn exchange(&mut self, request: &str) -> Option<String> {
            self.log.lock().unwrap().push(request.to_string());
            if request == "ADD_NETWORK" {
                let id = self.next_id;
                self.next_id += 1;
                Some(format!("{id}\n"))
            } else {
                Some("OK\n".to_string())
            }
        }
    }

    #[test]
    fn key_file() {
        let text = "# commentaire\n[autre]\nx=1\n[wpa_supplicant]\nssid=\"Salle\"\npsk=\\s\"a b\"\n";
        let keys = key_file_group(text, WPA_GROUP).unwrap();
        assert_eq!(keys, vec![("ssid".into(), "\"Salle\"".into()), ("psk".into(), " \"a b\"".into())]);
        assert!(key_file_group("[x]\na=1\n", WPA_GROUP).is_err());
        assert!(key_file_group("bad line\n", WPA_GROUP).is_err());
    }

    #[test]
    fn configuration_sequence() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mock = Mock { log: log.clone(), next_id: 2 };
        let wpa = Wpa::with_transport(Some(Box::new(mock)), false);
        wpa.configure_from("[wpa_supplicant]\nssid=\"S\"\nkey_mgmt=WPA-PSK\n".to_string());
        let log = log.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "ADD_NETWORK",
                "REMOVE_NETWORK 1",
                "REMOVE_NETWORK 2",
                "ADD_NETWORK",
                "SET_NETWORK 3 ssid \"S\"",
                "SET_NETWORK 3 key_mgmt WPA-PSK",
                "ENABLE_NETWORK 3",
                "SELECT_NETWORK 3",
                "SAVE_CONFIG",
            ]
        );
        assert!(wpa.state.lock().unwrap().network_config.is_some());
    }

    #[test]
    fn missing_group_does_nothing() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let wpa = Wpa::with_transport(Some(Box::new(Mock { log: log.clone(), next_id: 0 })), false);
        wpa.configure_from("[autre]\na=b\n".to_string());
        assert!(log.lock().unwrap().is_empty());
        assert!(wpa.state.lock().unwrap().network_config.is_none());
    }

    #[test]
    fn atoi_like_c() {
        assert_eq!(atoi("12\n"), 12);
        assert_eq!(atoi("FAIL"), 0);
        assert_eq!(atoi(""), 0);
        assert_eq!(atoi(" -3x"), -3);
    }
}
