//! Envoi de données vers un serveur (port de `common/network/uploader.cpp`).
//!
//! Le C++ s'appuie sur libcurl : un corps non vide est envoyé en `PUT`
//! (`CURLOPT_UPLOAD`), un formulaire en `POST` multipart, sans proxy. Ici :
//! `ureq` pour HTTP/HTTPS et [`crate::ftp`] pour `ftp://`.
//! Comme curl, un code HTTP d'erreur n'est pas un échec de transfert : seul
//! un problème de connexion l'est.

use std::io::Read;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;

/// Délai d'établissement de connexion.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Échec d'un transfert (équivalent d'un `CURLcode` différent de `CURLE_OK`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadError(pub String);

impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UploadError {}

/// Méthode HTTP employée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Put,
    Post,
}

/// Partie d'un formulaire multipart (`curl_formadd`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormPart {
    /// Champ texte.
    Field { name: String, value: String },
    /// Fichier : nom du champ, nom de fichier annoncé, contenu.
    File { name: String, filename: String, content: Vec<u8> },
}

/// Description d'un transfert.
#[derive(Debug, Clone)]
pub struct Transfer {
    pub method: Method,
    pub url: String,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
    pub credentials: Option<(String, String)>,
    pub user_agent: Option<String>,
    pub form: Vec<FormPart>,
    pub timeout: Option<Duration>,
}

impl Transfer {
    pub fn new(method: Method, url: &str) -> Self {
        Transfer {
            method,
            url: normalize_url(url),
            body: Vec::new(),
            headers: Vec::new(),
            credentials: None,
            user_agent: None,
            form: Vec::new(),
            timeout: None,
        }
    }

    /// Exécute le transfert ; renvoie le corps de la réponse (`GetResponseBody`).
    pub fn perform(&self) -> Result<String, UploadError> {
        let lower = self.url.to_ascii_lowercase();
        if lower.starts_with("ftp://") {
            let (user, pass) = match &self.credentials {
                Some((u, p)) => (Some(u.as_str()), Some(p.as_str())),
                None => (None, None),
            };
            return crate::ftp::store(&self.url, user, pass, &self.body)
                .map(|_| String::new())
                .map_err(|e| UploadError(e.to_string()));
        }
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return Err(UploadError("Unsupported protocol".into()));
        }

        let mut builder = ureq::AgentBuilder::new().timeout_connect(CONNECT_TIMEOUT);
        if let Some(t) = self.timeout {
            builder = builder.timeout(t);
        }
        let agent = builder.build();
        let method = match self.method {
            Method::Get => "GET",
            Method::Put => "PUT",
            Method::Post => "POST",
        };
        let mut req = agent.request(method, &self.url);
        for (k, v) in &self.headers {
            req = req.set(k, v);
        }
        if let Some(ua) = &self.user_agent {
            req = req.set("User-Agent", ua);
        }
        if let Some((u, p)) = &self.credentials {
            req = req.set("Authorization", &format!("Basic {}", B64.encode(format!("{u}:{p}"))));
        }
        let result = if !self.form.is_empty() {
            let (content_type, body) = multipart(&self.form);
            req.set("Content-Type", &content_type).send_bytes(&body)
        } else if self.method == Method::Get {
            req.call()
        } else {
            req.send_bytes(&self.body)
        };
        let response = match result {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(t)) => return Err(UploadError(t.to_string())),
        };
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(64 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|e| UploadError(e.to_string()))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// Ajoute le protocole manquant comme curl : `ftp.` → `ftp://`, sinon `http://`.
pub fn normalize_url(url: &str) -> String {
    if url.contains("://") {
        url.to_string()
    } else if url.to_ascii_lowercase().starts_with("ftp.") {
        format!("ftp://{url}")
    } else {
        format!("http://{url}")
    }
}

/// Construit un corps `multipart/form-data`.
pub fn multipart(parts: &[FormPart]) -> (String, Vec<u8>) {
    let boundary = format!("------------------------{:016x}", rand::random::<u64>());
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match part {
            FormPart::Field { name, value } => {
                body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
                body.extend_from_slice(value.as_bytes());
            }
            FormPart::File { name, filename, content } => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n\
                         Content-Type: application/octet-stream\r\n\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(content);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(normalize_url("ftp.free.fr/www"), "ftp://ftp.free.fr/www");
        assert_eq!(normalize_url("example.org/up"), "http://example.org/up");
        assert_eq!(normalize_url("https://x.org"), "https://x.org");
    }

    #[test]
    fn form() {
        let (ct, body) = multipart(&[
            FormPart::Field { name: "submit".into(), value: "send".into() },
            FormPart::File { name: "ficXML".into(), filename: "a.cotcot".into(), content: b"<x/>".to_vec() },
        ]);
        let boundary = ct.split("boundary=").nth(1).unwrap();
        let text = String::from_utf8(body).unwrap();
        assert!(text.contains("name=\"submit\"\r\n\r\nsend\r\n"));
        assert!(text.contains("filename=\"a.cotcot\""));
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
    }
}
