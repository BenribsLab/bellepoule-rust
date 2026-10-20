//! Téléchargement asynchrone (port de `common/network/downloader.cpp`).
//!
//! Utilisé pour vérifier la dernière version disponible. Le contenu (ou
//! `None` en cas d'échec) est remis au rappel depuis le thread de
//! téléchargement, sauf si [`Downloader::kill`] a été appelé.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::uploader::{Method, Transfer};

/// Délai maximal d'un téléchargement (`CURLOPT_TIMEOUT`).
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// Agent HTTP annoncé.
pub const USER_AGENT: &str = "libcurl-agent/1.0";

pub struct Downloader {
    name: String,
    killed: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Downloader {
    pub fn new(name: &str) -> Self {
        Downloader { name: name.to_string(), killed: Arc::new(AtomicBool::new(false)), thread: None }
    }

    /// `Start` : télécharge `address` puis appelle `on_data`.
    pub fn start<F>(&mut self, address: &str, on_data: F)
    where
        F: FnOnce(Option<String>) + Send + 'static,
    {
        let address = address.to_string();
        let killed = self.killed.clone();
        match std::thread::Builder::new().name(self.name.clone()).spawn(move || {
            let data = Self::fetch(&address);
            if !killed.load(Ordering::SeqCst) {
                on_data(data);
            }
        }) {
            Ok(t) => self.thread = Some(t),
            Err(e) => log::error!("Failed to create Downloader thread: {e}"),
        }
    }

    /// Téléchargement synchrone.
    pub fn fetch(address: &str) -> Option<String> {
        let mut t = Transfer::new(Method::Get, address);
        t.user_agent = Some(USER_AGENT.into());
        t.timeout = Some(TIMEOUT);
        t.perform().ok()
    }

    /// `Kill` : le rappel ne sera pas appelé ; attend la fin du thread.
    pub fn kill(&mut self) {
        self.killed.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
