//! Courtier USB (port de `common/network/usb_broker.cpp`).
//!
//! Lance le processus `bp-usb-backend` et lit sa sortie d'erreur. Le
//! protocole est une suite d'objets JSON, chacun terminé par une ligne
//! commençant par `.` :
//!
//! ```text
//! {
//!   "Event":        "PLUG",
//!   "Slot":         "…",
//!   "Manufacturer": "…",
//!   "Product":      "…",
//!   "SerialNumber": "…"
//! }
//! .
//! ```
//!
//! `Event` vaut `PLUG`, `UNPLUG` (avec `Slot`) ou `ERROR` (avec `Message`).
//! L'entrée standard du processus fils reste ouverte : la fermer (à la
//! destruction du courtier) lui demande de s'arrêter.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::usb_drive::UsbDrive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbEvent {
    StickPlugged,
    StickUnplugged,
}

/// Rappel des événements (`UsbBroker::Listener::OnUsbEvent`), appelé depuis
/// le thread de lecture.
pub type UsbListener = Arc<dyn Fn(UsbEvent, &UsbDrive) + Send + Sync>;

/// Décodeur du flux du processus fils (utilisable sans processus).
#[derive(Default)]
pub struct UsbEventParser {
    json: String,
    drives: HashMap<String, UsbDrive>,
}

impl UsbEventParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Traite une ligne ; renvoie l'événement complet éventuel.
    pub fn feed_line(&mut self, line: &str) -> Option<(UsbEvent, UsbDrive)> {
        if line.starts_with('.') {
            let json = std::mem::take(&mut self.json);
            self.on_event(&json)
        } else {
            self.json.push_str(line);
            None
        }
    }

    /// Clés actuellement branchées.
    pub fn drives(&self) -> impl Iterator<Item = &UsbDrive> {
        self.drives.values()
    }

    fn on_event(&mut self, json: &str) -> Option<(UsbEvent, UsbDrive)> {
        let value: serde_json::Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("json_parser_load_from_data: {e}");
                return None;
            }
        };
        let field = |name: &str| value.get(name).and_then(|v| v.as_str()).map(str::to_string);
        match field("Event").as_deref() {
            Some("ERROR") => {
                log::warn!("{}", field("Message").unwrap_or_default());
                None
            }
            Some("PLUG") => {
                let drive = UsbDrive::new(
                    field("Slot").as_deref(),
                    field("Product").as_deref(),
                    field("Manufacturer").as_deref(),
                    field("SerialNumber").as_deref(),
                );
                self.drives.insert(drive.slot().unwrap_or("").to_string(), drive.clone());
                Some((UsbEvent::StickPlugged, drive))
            }
            Some("UNPLUG") => {
                let slot = field("Slot").unwrap_or_default();
                self.drives.remove(&slot).map(|d| (UsbEvent::StickUnplugged, d))
            }
            _ => None,
        }
    }
}

pub struct UsbBroker {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
}

impl UsbBroker {
    /// Nom du programme de détection (à côté de l'exécutable courant).
    pub fn default_backend_path() -> PathBuf {
        let name = if cfg!(windows) { "bp-usb-backend.exe" } else { "bp-usb-backend" };
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_default()
            .join(name)
    }

    /// `UsbBroker (listener)` : lance `backend` et lit ses événements.
    pub fn new(backend: &Path, listener: UsbListener) -> UsbBroker {
        let spawned = Command::new(backend).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn();
        match spawned {
            Ok(mut child) => {
                let stdin = child.stdin.take();
                if let Some(stderr) = child.stderr.take() {
                    let _ = std::thread::Builder::new().name("UsbBroker".into()).spawn(move || {
                        Self::read_events(BufReader::new(stderr), listener);
                    });
                }
                UsbBroker { child: Some(child), stdin }
            }
            Err(e) => {
                log::warn!("g_spawn_async_with_pipes: {}: {e}", backend.display());
                UsbBroker { child: None, stdin: None }
            }
        }
    }

    /// Lit un flux d'événements jusqu'à sa fin (exposé pour les tests).
    pub fn read_events(reader: impl BufRead, listener: UsbListener) {
        let mut parser = UsbEventParser::new();
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if let Some((event, drive)) = parser.feed_line(&line) {
                        listener(event, &drive);
                    }
                }
                Err(e) => {
                    log::warn!("g_io_channel_read_line: {e}");
                    break;
                }
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }
}

impl Drop for UsbBroker {
    fn drop(&mut self) {
        // fermer l'entrée standard demande l'arrêt du processus fils
        self.stdin.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stream() {
        let stream = "{\n  \"Event\":        \"PLUG\",\n  \"Slot\":         \"S1\",\n  \"Manufacturer\": \"ACME\",\n  \"Product\":      \"Stick\",\n  \"SerialNumber\": \"123456\"\n}\n.\n\
                      {\n  \"Event\":   \"ERROR\",\n  \"Message\": \"oops\"\n}\n.\n\
                      {\n  \"Event\": \"UNPLUG\",\n  \"Slot\":  \"S1\"\n}\n.\n\
                      {\n  \"Event\": \"UNPLUG\",\n  \"Slot\":  \"S2\"\n}\n.\n";
        let events = Arc::new(Mutex::new(Vec::new()));
        let e = events.clone();
        UsbBroker::read_events(
            BufReader::new(stream.as_bytes()),
            Arc::new(move |ev, d: &UsbDrive| e.lock().unwrap().push((ev, d.serial_number().to_string()))),
        );
        let events = events.lock().unwrap();
        assert_eq!(
            *events,
            vec![(UsbEvent::StickPlugged, "123456".to_string()), (UsbEvent::StickUnplugged, "123456".to_string())]
        );
    }
}
