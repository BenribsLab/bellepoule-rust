//! Réseau de LivePoule (port de `common/network/http_server.cpp` et
//! `cryptor.cpp`, tels qu'utilisés par `LivePoule/screen.cpp`).
//!
//! Le réseau est isolé derrière le trait [`Network`] afin de pouvoir brancher
//! plus tard la couche `Ring` de `bp-net` (portée en parallèle) sans toucher
//! à l'écran. L'implémentation fournie, [`HttpNetwork`], est autonome : un
//! serveur HTTP qui reçoit les messages postés par le smartphone de
//! l'arbitre (SmartPoule).
//!
//! # Protocole
//!
//! Le smartphone envoie une requête `POST` (ou `PUT`) dont le corps est un
//! [`bp_net::Message`] sérialisé (`Message::parcel`). Si l'en-tête HTTP `IV`
//! est présent, le corps est chiffré : base64 d'un AES-256-CBC (bourrage
//! PKCS#7) du texte suivi de son zéro terminal ; la clé est la clé de 32
//! caractères affichée dans le QR code ([`crate::wifi_code`]) et `IV` est le
//! vecteur d'initialisation en base64. La réponse est toujours un `200` vide
//! (ou `405` pour une autre méthode).
//!
//! # Messages reçus
//!
//! | Nom (`[Header] name`)        | Champs du corps (`[Body]`)                                   | Effet |
//! |------------------------------|--------------------------------------------------------------|-------|
//! | `SmartPoule::RefereeHandshake` | aucun                                                      | masque le QR code, remet l'écran à zéro, chrono à 03:00, scores à 0 |
//! | `SmartPoule::Score`          | `competition-color` (couleur CSS/GDK, ex. `#93a24c`), `competition-name`, `red-fencer`, `red-score`, `green-fencer`, `green-score` (chaînes ; un champ absent laisse l'affichage inchangé) | met à jour le bandeau, les noms et les scores |
//! | `SmartPoule::Timer`          | `value` (entier, secondes ; 0 = inchangé), `state` (`RUNNING` démarre le décompte, toute autre valeur l'arrête) | pilote le chronomètre |
//!
//! # Messages émis
//!
//! Aucun : comme dans le C++, le boîtier est passif. Les scores sont saisis
//! par l'arbitre sur son smartphone, qui les transmet lui-même à BellePoule ;
//! LivePoule ne fait que les afficher.


use std::net::IpAddr;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use aes::Aes256;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use bp_net::Message;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};

/// Nom du message de prise en main par l'arbitre.
pub const REFEREE_HANDSHAKE: &str = "SmartPoule::RefereeHandshake";
/// Nom du message de score.
pub const SCORE: &str = "SmartPoule::Score";
/// Nom du message de chronomètre.
pub const TIMER: &str = "SmartPoule::Timer";

/// Accès réseau de l'écran.
pub trait Network {
    /// Port TCP d'écoute (inscrit dans le QR code).
    fn port(&self) -> u16;

    /// Adresse IPv4 du boîtier (inscrite dans le QR code ; `Ring::GetIpV4Address`).
    fn ip_v4_address(&self) -> String;

    /// Prochain message reçu, sans bloquer.
    fn try_recv(&mut self) -> Option<Message>;
}

/// Fournisseur de clé secrète (`HttpServer::Listener::GetSecretKey`) :
/// reçoit l'URL de la requête (schéma d'authentification).
pub type SecretKeyProvider = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// `Cryptor::Decrypt` : déchiffre `data_b64` ; le texte s'arrête au premier
/// zéro (le C++ chiffre la chaîne avec son zéro terminal).
pub fn decrypt(data_b64: &str, iv_b64: &str, key: &str) -> Option<String> {
    let cipher = STANDARD.decode(data_b64.trim()).ok()?;
    let iv = STANDARD.decode(iv_b64.trim()).ok()?;
    let key = key.as_bytes();
    if key.len() < 32 || iv.len() < 16 {
        return None;
    }
    let decryptor = cbc::Decryptor::<Aes256>::new_from_slices(&key[..32], &iv[..16]).ok()?;
    let mut plain = decryptor.decrypt_padded_vec_mut::<Pkcs7>(&cipher).ok()?;
    if let Some(nul) = plain.iter().position(|b| *b == 0) {
        plain.truncate(nul);
    }
    Some(String::from_utf8_lossy(&plain).into_owned())
}

/// `Cryptor::Encrypt` : renvoie (texte chiffré base64, IV base64).
/// `iv` aléatoire si `None` (`GenerateIv`).
pub fn encrypt(text: &str, key: &str, iv: Option<[u8; 16]>) -> Option<(String, String)> {
    let iv = iv.unwrap_or_else(rand::random);
    let key = key.as_bytes();
    if key.len() < 32 {
        return None;
    }
    let mut data = text.as_bytes().to_vec();
    data.push(0);
    let encryptor = cbc::Encryptor::<Aes256>::new_from_slices(&key[..32], &iv).ok()?;
    let cipher = encryptor.encrypt_padded_vec_mut::<Pkcs7>(&data);
    Some((STANDARD.encode(cipher), STANDARD.encode(iv)))
}

/// Traitement d'un corps de requête `POST`/`PUT` (`OnRequestReceived` puis
/// `DeferedPost`). Renvoie le message s'il est valide.
pub fn process_post(body: &[u8], iv: Option<&str>, url: &str, secret: &SecretKeyProvider) -> Option<Message> {
    let mut content = {
        let end = body.iter().position(|b| *b == 0).unwrap_or(body.len());
        String::from_utf8_lossy(&body[..end]).into_owned()
    };
    if let Some(iv) = iv {
        if let Some(key) = secret(url) {
            match decrypt(&content, iv, &key) {
                Some(plain) => content = plain,
                None => {
                    log::warn!("HttpServer : déchiffrement impossible");
                    content.clear();
                }
            }
        }
    }
    let message = Message::parse(&content);
    if message.is_valid() && message.name().is_some() {
        Some(message)
    } else {
        log::warn!("HttpServer::DeferedPost");
        None
    }
}

/// Première adresse IPv4 hors boucle locale, sinon `127.0.0.1`
/// (`Ring::GuessIpV4Addresses`).
pub fn local_ipv4_address() -> String {
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for iface in interfaces {
            if iface.is_loopback() {
                continue;
            }
            if let IpAddr::V4(ip) = iface.ip() {
                if !ip.is_unspecified() && !ip.is_link_local() {
                    return ip.to_string();
                }
            }
        }
    }
    "127.0.0.1".to_string()
}

/// Serveur HTTP de réception des messages (`Net::HttpServer`).
pub struct HttpNetwork {
    port: u16,
    ip: String,
    rx: Receiver<Message>,
    server: Arc<tiny_http::Server>,
    thread: Option<JoinHandle<()>>,
}

impl HttpNetwork {
    /// Démarre le serveur sur `port` (0 : port choisi par le système).
    pub fn start(port: u16, secret: SecretKeyProvider) -> std::io::Result<Self> {
        let server = tiny_http::Server::http(("0.0.0.0", port)).map_err(std::io::Error::other)?;
        let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(port);
        let server = Arc::new(server);
        let (tx, rx) = channel();
        let thread = {
            let server = server.clone();
            std::thread::Builder::new()
                .name("livepoule-http".into())
                .spawn(move || serve(&server, &tx, &secret))?
        };
        log::info!("Serveur HTTP sur le port {port}");
        Ok(HttpNetwork { port, ip: local_ipv4_address(), rx, server, thread: Some(thread) })
    }
}

fn serve(server: &tiny_http::Server, tx: &Sender<Message>, secret: &SecretKeyProvider) {
    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        if matches!(method, tiny_http::Method::Post | tiny_http::Method::Put) {
            let iv = request
                .headers()
                .iter()
                .find(|h| h.field.as_str().as_str() == "IV")
                .map(|h| h.value.as_str().to_string());
            let url = request.url().to_string();
            let mut body = Vec::new();
            if let Err(e) = request.as_reader().read_to_end(&mut body) {
                log::warn!("HttpServer : {e}");
            }
            let _ = request.respond(tiny_http::Response::empty(200));
            if let Some(message) = process_post(&body, iv.as_deref(), &url, secret) {
                if tx.send(message).is_err() {
                    break;
                }
            }
        } else {
            let _ = request.respond(tiny_http::Response::empty(405));
        }
    }
}

impl Network for HttpNetwork {
    fn port(&self) -> u16 {
        self.port
    }

    fn ip_v4_address(&self) -> String {
        self.ip.clone()
    }

    fn try_recv(&mut self) -> Option<Message> {
        self.rx.try_recv().ok()
    }
}

impl Drop for HttpNetwork {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Réseau en mémoire : les messages sont injectés par le code (tests, mode
/// de démonstration).
#[derive(Default)]
pub struct LocalNetwork {
    queue: std::collections::VecDeque<Message>,
}

impl LocalNetwork {
    pub fn push(&mut self, message: Message) {
        self.queue.push_back(message);
    }
}

impl Network for LocalNetwork {
    fn port(&self) -> u16 {
        0
    }

    fn ip_v4_address(&self) -> String {
        "127.0.0.1".to_string()
    }

    fn try_recv(&mut self) -> Option<Message> {
        self.queue.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    const KEY: &str = "ABCDEFGHIJKLMNOPQRSTUVWXabcdefgh";

    fn provider() -> SecretKeyProvider {
        Arc::new(|_| Some(KEY.to_string()))
    }

    #[test]
    fn crypto_roundtrip() {
        let (data, iv) = encrypt("[Header]\nname=SmartPoule::Timer\n", KEY, Some([7; 16])).unwrap();
        assert_eq!(decrypt(&data, &iv, KEY).unwrap(), "[Header]\nname=SmartPoule::Timer\n");
        assert!(decrypt(&data, &iv, "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ").is_none_or(|t| !t.starts_with("[Header]")));
    }

    #[test]
    fn post_plain_and_encrypted() {
        let mut m = Message::new(SCORE);
        m.set_str("red-score", "4");
        let text = m.parcel();
        let got = process_post(text.as_bytes(), None, "/", &provider()).unwrap();
        assert!(got.is(SCORE));
        assert_eq!(got.get_str("red-score").as_deref(), Some("4"));

        let (data, iv) = encrypt(&text, KEY, None).unwrap();
        let got = process_post(data.as_bytes(), Some(&iv), "/", &provider()).unwrap();
        assert_eq!(got.get_str("red-score").as_deref(), Some("4"));

        assert!(process_post(b"n'importe quoi", None, "/", &provider()).is_none());
    }

    #[test]
    fn http_server_receives_messages() {
        let mut net = HttpNetwork::start(0, provider()).unwrap();
        assert_ne!(net.port(), 0);
        let mut m = Message::new(TIMER);
        m.set_int("value", 42);
        m.set_str("state", "RUNNING");
        let (data, iv) = encrypt(&m.parcel(), KEY, None).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", net.port())).unwrap();
        write!(
            stream,
            "POST / HTTP/1.1\r\nHost: localhost\r\nIV: {iv}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",
            data.len()
        )
        .unwrap();
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        assert!(answer.starts_with("HTTP/1.1 200"));
        let start = Instant::now();
        let received = loop {
            if let Some(m) = net.try_recv() {
                break m;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(received.is(TIMER));
        assert_eq!(received.get_int("value"), 42);

        let mut stream = TcpStream::connect(("127.0.0.1", net.port())).unwrap();
        write!(stream, "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        assert!(answer.starts_with("HTTP/1.1 405"));
    }
}
