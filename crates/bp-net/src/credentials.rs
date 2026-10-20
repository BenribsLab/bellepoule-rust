//! Identifiants de l'anneau (port de `common/network/credentials.cpp`).
//!
//! Le QR code produit (`nom:ip:port:clé`) permet à un partenaire (Marshaller,
//! smartphone) de récupérer la phrase secrète du poste.

use crate::flash_code::FlashCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    name: String,
    ip: Option<String>,
    port: u16,
    key: String,
}

impl Credentials {
    pub fn new(name: &str, ip_address: Option<&str>, port: u16, key: &str) -> Self {
        Credentials { name: name.to_string(), ip: ip_address.map(str::to_string), port, key: key.to_string() }
    }

    /// `GetKey` : phrase secrète (clé AES-256 brute).
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ip_address(&self) -> Option<&str> {
        self.ip.as_deref()
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn set_ip_address(&mut self, address: Option<&str>) {
        self.ip = address.map(str::to_string);
    }
}

impl FlashCode for Credentials {
    /// `GetText` : `"%s:%s:%d:%s"` (nom du rôle, adresse IP, port, clé).
    fn text(&self) -> String {
        format!("{}:{}:{}:{}", self.name, self.ip.as_deref().unwrap_or("(null)"), self.port, self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text() {
        let c = Credentials::new("RESOURCE_USER", Some("10.0.0.2"), 35831, "abc");
        assert_eq!(c.text(), "RESOURCE_USER:10.0.0.2:35831:abc");
    }
}
