//! File d'envoi de messages vers un pair (port de `message_uploader.cpp`).
//!
//! Un thread dépile les messages et les envoie un par un en `PUT` vers l'URL
//! du pair. Si le message porte une phrase secrète, son texte est chiffré
//! (AES-256-CBC, voir [`crate::cryptor`]) et l'IV part dans l'en-tête HTTP
//! `IV`. [`MessageUploader::stop`] met en file un message d'arrêt : les
//! messages déjà en file partent d'abord, puis le statut
//! [`PeerStatus::ConnClosed`] est signalé.
//!
//! Le rappel de statut s'exécute sur le thread d'envoi (le C++ le différait
//! dans la boucle GTK) : l'application doit le relayer vers son thread
//! graphique si besoin.

use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;

use crate::cryptor::Cryptor;
use crate::message::Message;
use crate::uploader::{Method, Transfer};

/// Nom du message interne qui arrête le thread d'envoi.
pub const STOP_SENDING: &str = "MessageUploader::stop_sending";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerStatus {
    ConnOk,
    ConnError,
    ConnClosed,
}

/// Rappel de statut (`MessageUploader::Listener::OnUploadStatus`), appelé
/// pour tout statut différent de [`PeerStatus::ConnOk`].
pub type StatusListener = Arc<dyn Fn(PeerStatus) + Send + Sync>;

pub struct MessageUploader {
    url: String,
    queue: Option<Sender<Message>>,
    stopped: bool,
}

impl MessageUploader {
    pub fn new(url: &str, listener: Option<StatusListener>) -> Self {
        let (tx, rx) = channel::<Message>();
        let thread_url = url.to_string();
        let spawned = std::thread::Builder::new().name("MessageUploader".into()).spawn(move || {
            while let Ok(message) = rx.recv() {
                if message.is(STOP_SENDING) {
                    if let Some(l) = &listener {
                        l(PeerStatus::ConnClosed);
                    }
                    break;
                }
                let status = match Self::send(&thread_url, &message) {
                    Ok(()) => PeerStatus::ConnOk,
                    Err(e) => {
                        log::warn!("[Uploader Error] {thread_url}: {e}");
                        PeerStatus::ConnError
                    }
                };
                if status != PeerStatus::ConnOk {
                    if let Some(l) = &listener {
                        l(status);
                    }
                }
            }
        });
        let queue = match spawned {
            Ok(_) => Some(tx),
            Err(e) => {
                log::error!("Failed to create Uploader thread: {e}");
                None
            }
        };
        MessageUploader { url: url.to_string(), queue, stopped: false }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// `PushMessage` : une copie du message est mise en file.
    pub fn push_message(&self, message: &Message) {
        if let Some(q) = &self.queue {
            let _ = q.send(message.clone());
        }
    }

    /// `Stop` : arrêt après envoi des messages en attente.
    pub fn stop(&mut self) {
        if !self.stopped {
            self.stopped = true;
            self.push_message(&Message::new(STOP_SENDING));
        }
    }

    /// Prépare le corps et l'en-tête (`PrepareData`), puis envoie.
    fn send(url: &str, message: &Message) -> Result<(), crate::uploader::UploadError> {
        let parcel = message.parcel();
        let mut transfer = Transfer::new(Method::Put, url);
        match message.passphrase256() {
            Some(pass) => {
                let (crypted, iv) = Cryptor::new().encrypt(&parcel, pass);
                transfer.body = crypted.into_bytes();
                transfer.headers.push(("IV".into(), iv));
            }
            None => transfer.body = parcel.into_bytes(),
        }
        transfer.perform().map(|_| ())
    }
}

impl Drop for MessageUploader {
    /// Le thread termine les envois en cours puis s'arrête.
    fn drop(&mut self) {
        self.stop();
    }
}
