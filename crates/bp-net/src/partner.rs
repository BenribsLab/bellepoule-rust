//! Partenaire de l'anneau (port de `common/network/partner.cpp`).
//!
//! Construit depuis une annonce ou une poignée de main (`role`, `partner`,
//! `ip_address`, `unicast_port`), il envoie les messages vers
//! `http://<ip>:<port>/ring` via un [`MessageUploader`].

use crate::message::Message;
use crate::message_uploader::{MessageUploader, StatusListener};

/// Description d'un partenaire (copiable, transmise à l'application).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartnerInfo {
    /// Rôle du partenaire (valeur de [`crate::ring::Role`]).
    pub role: u32,
    /// Identifiant aléatoire du partenaire dans l'anneau.
    pub partner_id: u32,
    pub address: Option<String>,
    pub port: u16,
}

impl PartnerInfo {
    /// Lit les champs du partenaire dans un message.
    pub fn from_message(message: &Message) -> Self {
        PartnerInfo {
            role: message.get_uint("role"),
            partner_id: message.get_uint("partner"),
            address: message.get_str("ip_address"),
            port: message.get_uint("unicast_port") as u16,
        }
    }

    /// `HasRole`.
    pub fn has_role(&self, role: u32) -> bool {
        self.role == role
    }

    /// `Is`.
    pub fn is(&self, partner_id: u32) -> bool {
        self.partner_id == partner_id
    }

    /// URL de réception des messages de l'anneau.
    pub fn url(&self) -> Option<String> {
        self.address.as_ref().map(|ip| format!("http://{ip}:{}/ring", self.port))
    }
}

pub struct Partner {
    info: PartnerInfo,
    uploader: Option<MessageUploader>,
    passphrase256: Option<String>,
}

impl Partner {
    /// `Partner (message, listener)` : le rappel reçoit les statuts d'envoi.
    pub fn new(message: &Message, listener: Option<StatusListener>) -> Self {
        let info = PartnerInfo::from_message(message);
        let uploader = info.url().map(|url| MessageUploader::new(&url, listener));
        Partner { info, uploader, passphrase256: None }
    }

    /// Partenaire reconstruit depuis sa description.
    pub fn from_info(info: PartnerInfo, listener: Option<StatusListener>) -> Self {
        let uploader = info.url().map(|url| MessageUploader::new(&url, listener));
        Partner { info, uploader, passphrase256: None }
    }

    pub fn info(&self) -> &PartnerInfo {
        &self.info
    }

    pub fn set_passphrase256(&mut self, passphrase256: Option<&str>) {
        self.passphrase256 = passphrase256.map(str::to_string);
    }

    pub fn passphrase256(&self) -> Option<&str> {
        self.passphrase256.as_deref()
    }

    /// `SendMessage` : chiffré avec la phrase du message, ou à défaut celle
    /// du partenaire.
    pub fn send_message(&self, message: &Message) {
        if let Some(uploader) = &self.uploader {
            if message.passphrase256().is_none() && self.passphrase256.is_some() {
                let mut m = message.clone();
                m.set_passphrase256(self.passphrase256.as_deref());
                uploader.push_message(&m);
            } else {
                uploader.push_message(message);
            }
        }
    }

    /// `Leave` : arrêt de l'envoi après les messages en attente.
    pub fn leave(&mut self) {
        if let Some(u) = &mut self.uploader {
            u.stop();
        }
    }

    pub fn is(&self, partner_id: u32) -> bool {
        self.info.is(partner_id)
    }

    pub fn has_role(&self, role: u32) -> bool {
        self.info.has_role(role)
    }

    pub fn address(&self) -> Option<&str> {
        self.info.address.as_deref()
    }

    pub fn port(&self) -> u16 {
        self.info.port
    }
}
