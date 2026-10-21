//! Serveur HTTP de réception des messages (port de `http_server.cpp`).
//!
//! - `POST` ou `PUT` : le corps est un [`Message`] (texte GKeyFile). Si la
//!   requête porte un en-tête `IV`, le corps est chiffré : la clé est
//!   demandée à l'écouteur selon l'URL (`/ring`, `/sender/<réf>`…) puis le
//!   message déchiffré. Réponse `200` vide ; le message valide est remis à
//!   l'écouteur.
//! - `GET`/`HEAD` : si une racine documentaire est configurée
//!   ([`HttpServer::set_document_root`]), les fichiers en sont servis ;
//!   sinon `405 Method Not Allowed` comme le C++.
//!
//! L'écouteur est appelé depuis le thread du serveur.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::cryptor::Cryptor;
use crate::message::Message;

/// Requête adressée à un service web de l'application (ex. `/smartpoule/…`).
#[derive(Debug, Clone)]
pub struct WebRequest {
    pub method: String,
    /// Chemin et paramètres (`/smartpoule/v1/jobs?ref=12&key=…`).
    pub url: String,
    pub body: Vec<u8>,
}

/// Réponse d'un service web.
#[derive(Debug, Clone)]
pub struct WebResponse {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl WebResponse {
    pub fn json(status: u16, body: String) -> Self {
        WebResponse { status, content_type: "application/json; charset=utf-8".into(), body: body.into_bytes() }
    }
}

/// Service web : appelé (dans le thread du serveur) pour les URL qui
/// commencent par son préfixe ; `None` laisse le traitement habituel.
pub type WebHandler = Arc<dyn Fn(WebRequest) -> Option<WebResponse> + Send + Sync>;

/// Écouteur du serveur (`HttpServer::Listener`).
pub trait HttpListener: Send + Sync {
    /// Clé de déchiffrement pour un schéma d'authentification (l'URL).
    fn secret_key(&self, authentication_scheme: &str) -> Option<String>;
    /// Message reçu (déjà déchiffré et validé).
    fn on_message(&self, message: Message);
}

pub struct HttpServer {
    port: u16,
    server: Arc<tiny_http::Server>,
    document_root: Arc<Mutex<Option<PathBuf>>>,
    web: Arc<Mutex<Vec<(String, WebHandler)>>>,
    thread: Option<JoinHandle<()>>,
}

impl HttpServer {
    /// `HttpServer (listener, port)` ; `port = 0` choisit un port libre.
    pub fn new(listener: Arc<dyn HttpListener>, port: u16) -> std::io::Result<HttpServer> {
        let server = tiny_http::Server::http(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(port);
        let server = Arc::new(server);
        let document_root: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
        let web: Arc<Mutex<Vec<(String, WebHandler)>>> = Arc::new(Mutex::new(Vec::new()));
        let thread = {
            let server = server.clone();
            let root = document_root.clone();
            let web = web.clone();
            std::thread::Builder::new().name("HttpServer".into()).spawn(move || {
                for request in server.incoming_requests() {
                    let handler = {
                        let url = request.url().to_string();
                        web.lock().unwrap().iter().find(|(prefix, _)| url.starts_with(prefix.as_str())).map(|(_, h)| h.clone())
                    };
                    let request = match handler {
                        Some(h) => match Self::on_web_request(request, &h) {
                            Some(r) => r,
                            None => continue,
                        },
                        None => request,
                    };
                    let root = root.lock().unwrap().clone();
                    Self::on_request(request, listener.as_ref(), root.as_deref());
                }
            })?
        };
        Ok(HttpServer { port, server, document_root, web, thread: Some(thread) })
    }

    /// Serveur dont les messages arrivent dans un canal.
    pub fn with_channel(
        port: u16,
        key_provider: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> std::io::Result<(HttpServer, Receiver<Message>)> {
        struct ChannelListener<F> {
            keys: F,
            tx: Mutex<std::sync::mpsc::Sender<Message>>,
        }
        impl<F: Fn(&str) -> Option<String> + Send + Sync> HttpListener for ChannelListener<F> {
            fn secret_key(&self, scheme: &str) -> Option<String> {
                (self.keys)(scheme)
            }
            fn on_message(&self, message: Message) {
                let _ = self.tx.lock().unwrap().send(message);
            }
        }
        let (tx, rx) = channel();
        let server = HttpServer::new(Arc::new(ChannelListener { keys: key_provider, tx: Mutex::new(tx) }), port)?;
        Ok((server, rx))
    }

    /// `GetPort`.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Installe un service web pour les URL qui commencent par `prefix`.
    pub fn set_web_handler(&self, prefix: &str, handler: WebHandler) {
        let mut web = self.web.lock().unwrap();
        web.retain(|(p, _)| p != prefix);
        web.push((prefix.to_string(), handler));
    }

    /// Requête d'un service web : réponse (avec en-têtes CORS pour les
    /// pages servies ailleurs), ou la requête rendue si le service l'ignore.
    fn on_web_request(mut request: tiny_http::Request, handler: &WebHandler) -> Option<tiny_http::Request> {
        let cors = |r: tiny_http::ResponseBox| {
            r.with_header(tiny_http::Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
                .with_header(tiny_http::Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap())
                .with_header(tiny_http::Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap())
        };
        if *request.method() == tiny_http::Method::Options {
            let _ = request.respond(cors(tiny_http::Response::empty(204).boxed()));
            return None;
        }
        let mut body = Vec::new();
        let _ = request.as_reader().read_to_end(&mut body);
        let web = WebRequest { method: request.method().as_str().to_string(), url: request.url().to_string(), body };
        match handler(web) {
            Some(r) => {
                let header = tiny_http::Header::from_bytes("Content-Type", r.content_type.as_bytes()).unwrap();
                let response = tiny_http::Response::from_data(r.body).with_status_code(r.status).with_header(header).boxed();
                let _ = request.respond(cors(response));
                None
            }
            None => {
                let _ = request.respond(cors(tiny_http::Response::empty(404).boxed()));
                None
            }
        }
    }

    /// Dossier servi en `GET` (pages web, fichiers publiés).
    pub fn set_document_root(&self, root: Option<PathBuf>) {
        *self.document_root.lock().unwrap() = root;
    }

    fn on_request(mut request: tiny_http::Request, listener: &dyn HttpListener, root: Option<&Path>) {
        use tiny_http::Method;
        match request.method() {
            Method::Post | Method::Put => {
                let url = request.url().to_string();
                let iv = request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("IV"))
                    .map(|h| h.value.as_str().to_string());
                let mut body = Vec::new();
                let _ = request.as_reader().read_to_end(&mut body);
                let _ = request.respond(tiny_http::Response::empty(200));

                let mut content = Some(String::from_utf8_lossy(&body).into_owned());
                if let Some(iv) = iv {
                    if let Some(key) = listener.secret_key(&url) {
                        content = Cryptor::decrypt(content.as_deref().unwrap_or(""), &iv, &key);
                    }
                }
                match content {
                    Some(text) => {
                        // le texte s'arrête au premier zéro (chaîne C)
                        let text = text.split('\0').next().unwrap_or("");
                        let message = Message::parse(text);
                        if message.is_valid() {
                            listener.on_message(message);
                        } else {
                            log::warn!("HttpServer::DeferedPost");
                        }
                    }
                    None => log::warn!("HttpServer: déchiffrement impossible ({url})"),
                }
            }
            Method::Get | Method::Head if root.is_some() => {
                let response = serve_file(root.unwrap(), request.url());
                let _ = request.respond(response);
            }
            _ => {
                let _ = request.respond(tiny_http::Response::empty(405));
            }
        }
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Type MIME selon l'extension.
pub fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("json") => "application/json",
        Some("xml") | Some("cotcot") => "application/xml",
        Some("txt") | Some("md") => "text/plain; charset=utf-8",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Résout une URL dans `root` sans permettre d'en sortir.
pub fn resolve_path(root: &Path, url: &str) -> Option<PathBuf> {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let path = crate::netutil::uri_unescape(path);
    let mut out = root.to_path_buf();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            p if p.contains(':') => return None,
            p => out.push(p),
        }
    }
    if out.is_dir() {
        for index in ["index.html", "index.htm"] {
            if out.join(index).is_file() {
                return Some(out.join(index));
            }
        }
        return None;
    }
    out.is_file().then_some(out)
}

/// Réponse `GET` d'un fichier statique.
pub fn serve_file(root: &Path, url: &str) -> tiny_http::ResponseBox {
    match resolve_path(root, url).and_then(|p| std::fs::read(&p).ok().map(|d| (p, d))) {
        Some((path, data)) => {
            let header = tiny_http::Header::from_bytes("Content-Type", content_type(&path)).unwrap();
            tiny_http::Response::from_data(data).with_header(header).boxed()
        }
        None => tiny_http::Response::empty(404).boxed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_uploader::{MessageUploader, PeerStatus};
    use std::time::Duration;

    const KEY: &str = "AbCdEfGhIjKlMnOpQrStUvWxYz012345";

    #[test]
    fn receive_clear_and_crypted_messages() {
        let (server, rx) = HttpServer::with_channel(0, |scheme| (scheme == "/ring").then(|| KEY.to_string())).unwrap();
        let port = server.port();
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let st = statuses.clone();
        let mut up = MessageUploader::new(
            &format!("http://127.0.0.1:{port}/ring"),
            Some(Arc::new(move |s: PeerStatus| st.lock().unwrap().push(s))),
        );

        let mut clear = Message::new("Test::Clear");
        clear.set_str("texte", "Bonjour\nle monde");
        up.push_message(&clear);

        let mut crypted = Message::new("Test::Crypted");
        crypted.set_uint("piste", 7);
        crypted.set_passphrase256(Some(KEY));
        up.push_message(&crypted);
        up.stop();

        let a = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(a.is("Test::Clear"));
        assert_eq!(a.get_str("texte").as_deref(), Some("Bonjour\nle monde"));
        let b = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(b.is("Test::Crypted"));
        assert_eq!(b.get_uint("piste"), 7);
        assert_eq!(b.net_id(), crypted.net_id());

        // attendre la fermeture de la file d'envoi
        for _ in 0..100 {
            if statuses.lock().unwrap().contains(&PeerStatus::ConnClosed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(*statuses.lock().unwrap(), vec![PeerStatus::ConnClosed]);
    }

    #[test]
    fn wrong_key_is_dropped() {
        let (server, rx) = HttpServer::with_channel(0, |_| Some("autre clé autre clé autre clé !!".into())).unwrap();
        let mut up = MessageUploader::new(&format!("http://127.0.0.1:{}/ring", server.port()), None);
        let mut m = Message::new("Test::Secret");
        m.set_passphrase256(Some(KEY));
        up.push_message(&m);
        up.stop();
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_err());
    }

    #[test]
    fn connection_error_is_reported() {
        // port fermé : on réserve un port puis on le libère
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let (tx, rx) = channel();
        let tx = Mutex::new(tx);
        let up = MessageUploader::new(
            &format!("http://127.0.0.1:{port}/ring"),
            Some(Arc::new(move |s| {
                let _ = tx.lock().unwrap().send(s);
            })),
        );
        up.push_message(&Message::new("Test"));
        assert_eq!(rx.recv_timeout(Duration::from_secs(20)).unwrap(), PeerStatus::ConnError);
    }

    #[test]
    fn get_serves_documents_or_405() {
        let (server, _rx) = HttpServer::with_channel(0, |_| None).unwrap();
        let url = format!("http://127.0.0.1:{}/page.html", server.port());
        match ureq::get(&url).call() {
            Err(ureq::Error::Status(405, _)) => {}
            other => panic!("{other:?}"),
        }
        let dir = std::env::temp_dir().join(format!("bp-net-www-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("page.html"), "<p>ok</p>").unwrap();
        server.set_document_root(Some(dir.clone()));
        let body = ureq::get(&url).call().unwrap().into_string().unwrap();
        assert_eq!(body, "<p>ok</p>");
        let bad = format!("http://127.0.0.1:{}/../secret", server.port());
        assert!(ureq::get(&bad).call().is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
