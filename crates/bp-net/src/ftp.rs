//! Client FTP minimal (dépôt d'un fichier en mode passif).
//!
//! Le C++ confie à libcurl les URL `ftp://` saisies dans la fenêtre de
//! publication ; ce module reproduit le dialogue de curl pour un envoi :
//! `USER`/`PASS`, `TYPE I`, un `CWD` par répertoire du chemin, `EPSV` (ou
//! `PASV`), `STOR`, puis `QUIT`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::netutil::uri_unescape;

/// Délai de connexion et de lecture des réponses du serveur.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Éléments d'une URL `ftp://[user[:pass]@]hôte[:port]/chemin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtpUrl {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    /// Répertoires (décodés) menant au fichier.
    pub dirs: Vec<String>,
    /// Nom du fichier distant (décodé), vide pour un répertoire.
    pub file: String,
}

impl FtpUrl {
    pub fn parse(url: &str) -> Option<FtpUrl> {
        let rest = url.strip_prefix("ftp://").or_else(|| url.strip_prefix("FTP://"))?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        };
        let (userinfo, hostport) = match authority.rfind('@') {
            Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
            None => (None, authority),
        };
        let (user, password) = match userinfo {
            Some(ui) => match ui.find(':') {
                Some(i) => (Some(uri_unescape(&ui[..i])), Some(uri_unescape(&ui[i + 1..]))),
                None => (Some(uri_unescape(ui)), None),
            },
            None => (None, None),
        };
        let (host, port) = match hostport.rfind(':') {
            Some(i) if !hostport.ends_with(']') => (hostport[..i].to_string(), hostport[i + 1..].parse().ok()?),
            _ => (hostport.to_string(), 21),
        };
        if host.is_empty() {
            return None;
        }
        let mut parts: Vec<String> = path.split('/').map(uri_unescape).collect();
        let file = parts.pop().unwrap_or_default();
        let dirs = parts.into_iter().filter(|p| !p.is_empty()).collect();
        Some(FtpUrl { host, port, user, password, dirs, file })
    }
}

/// Connexion de contrôle.
struct Control {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    peer: IpAddr,
}

impl Control {
    fn connect(host: &str, port: u16) -> std::io::Result<Control> {
        let addr = (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Couldn't resolve host name"))?;
        let stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        let writer = stream.try_clone()?;
        Ok(Control { reader: BufReader::new(stream), writer, peer: addr.ip() })
    }

    /// Lit une réponse (éventuellement multiligne `123-…` / `123 …`).
    fn response(&mut self) -> std::io::Result<(u32, String)> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "FTP: connexion fermée"));
        }
        let code: u32 = line.get(..3).and_then(|c| c.parse().ok()).unwrap_or(0);
        let mut text = line.clone();
        if line.as_bytes().get(3) == Some(&b'-') {
            let end = format!("{code} ");
            loop {
                let mut next = String::new();
                if self.reader.read_line(&mut next)? == 0 {
                    break;
                }
                text.push_str(&next);
                if next.starts_with(&end) {
                    break;
                }
            }
        }
        Ok((code, text.trim_end().to_string()))
    }

    fn command(&mut self, cmd: &str) -> std::io::Result<(u32, String)> {
        self.writer.write_all(cmd.as_bytes())?;
        self.writer.write_all(b"\r\n")?;
        self.response()
    }

    /// Ouvre la connexion de données (EPSV puis PASV ; l'adresse annoncée
    /// par PASV est ignorée au profit de celle du serveur, comme curl).
    fn data_connection(&mut self) -> std::io::Result<TcpStream> {
        let port = {
            let (code, text) = self.command("EPSV")?;
            if code == 229 {
                parse_epsv(&text)
            } else {
                None
            }
        };
        let port = match port {
            Some(p) => p,
            None => {
                let (code, text) = self.command("PASV")?;
                if code != 227 {
                    return Err(ftp_error(code, &text));
                }
                parse_pasv(&text).ok_or_else(|| ftp_error(code, &text))?
            }
        };
        let stream = TcpStream::connect_timeout(&SocketAddr::new(self.peer, port), TIMEOUT)?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        Ok(stream)
    }
}

fn ftp_error(code: u32, text: &str) -> std::io::Error {
    std::io::Error::other(format!("FTP {code}: {text}"))
}

fn parse_epsv(text: &str) -> Option<u16> {
    // 229 Entering Extended Passive Mode (|||6446|)
    let start = text.find("(|||")? + 4;
    let end = text[start..].find('|')? + start;
    text[start..end].parse().ok()
}

fn parse_pasv(text: &str) -> Option<u16> {
    // 227 Entering Passive Mode (h1,h2,h3,h4,p1,p2)
    let start = text.find('(')? + 1;
    let end = text[start..].find(')')? + start;
    let nums: Vec<u16> = text[start..end].split(',').filter_map(|n| n.trim().parse().ok()).collect();
    if nums.len() != 6 {
        return None;
    }
    Some(nums[4] * 256 + nums[5])
}

/// Dépose `data` à l'emplacement désigné par `url`.
pub fn store(url: &str, user: Option<&str>, password: Option<&str>, data: &[u8]) -> std::io::Result<()> {
    let url = FtpUrl::parse(url)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "URL using bad/illegal format"))?;
    let user = user.map(str::to_string).or(url.user.clone()).unwrap_or_else(|| "anonymous".to_string());
    let password = password.map(str::to_string).or(url.password.clone()).unwrap_or_else(|| "ftp@example.com".to_string());

    let mut ctl = Control::connect(&url.host, url.port)?;
    let (code, text) = ctl.response()?;
    if code != 220 {
        return Err(ftp_error(code, &text));
    }
    let (mut code, mut text) = ctl.command(&format!("USER {user}"))?;
    if code == 331 {
        (code, text) = ctl.command(&format!("PASS {password}"))?;
    }
    if code != 230 && code != 202 {
        return Err(ftp_error(code, &text));
    }
    let (code, text) = ctl.command("TYPE I")?;
    if code != 200 {
        return Err(ftp_error(code, &text));
    }
    for dir in &url.dirs {
        let (code, text) = ctl.command(&format!("CWD {dir}"))?;
        if code != 250 {
            return Err(ftp_error(code, &text));
        }
    }
    let mut data_stream = ctl.data_connection()?;
    let (code, text) = ctl.command(&format!("STOR {}", url.file))?;
    if code != 150 && code != 125 {
        return Err(ftp_error(code, &text));
    }
    data_stream.write_all(data)?;
    drop(data_stream);
    let (code, text) = ctl.response()?;
    if code != 226 && code != 250 {
        return Err(ftp_error(code, &text));
    }
    let _ = ctl.command("QUIT");
    Ok(())
}

/// Lit tout le contenu d'un flux (utilisé par les tests du serveur factice).
#[allow(dead_code)]
pub(crate) fn read_all(mut s: impl Read) -> Vec<u8> {
    let mut v = Vec::new();
    let _ = s.read_to_end(&mut v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn url() {
        let u = FtpUrl::parse("ftp://bob:s%40cret@ftp.example.org:2121/www/cotcot/Mon%20fichier.cotcot").unwrap();
        assert_eq!(u.host, "ftp.example.org");
        assert_eq!(u.port, 2121);
        assert_eq!(u.user.as_deref(), Some("bob"));
        assert_eq!(u.password.as_deref(), Some("s@cret"));
        assert_eq!(u.dirs, vec!["www", "cotcot"]);
        assert_eq!(u.file, "Mon fichier.cotcot");
        assert_eq!(parse_pasv("227 Entering Passive Mode (127,0,0,1,4,1)"), Some(1025));
        assert_eq!(parse_epsv("229 Entering Extended Passive Mode (|||6446|)"), Some(6446));
    }

    /// Serveur FTP factice : vérifie le dialogue complet d'un envoi.
    #[test]
    fn store_on_fake_server() {
        let ctl = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = ctl.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (s, _) = ctl.accept().unwrap();
            let mut w = s.try_clone().unwrap();
            let mut r = BufReader::new(s);
            let data_listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let dport = data_listener.local_addr().unwrap().port();
            w.write_all(b"220 Bienvenue\r\n").unwrap();
            let mut received = Vec::new();
            let mut cmds = Vec::new();
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).unwrap() == 0 {
                    break;
                }
                let line = line.trim_end().to_string();
                cmds.push(line.clone());
                let reply = match line.split(' ').next().unwrap() {
                    "USER" => "331 Password required".to_string(),
                    "PASS" => "230-Logged in\r\n230 OK".to_string(),
                    "TYPE" => "200 OK".to_string(),
                    "CWD" => "250 OK".to_string(),
                    "EPSV" => "500 Unknown".to_string(),
                    "PASV" => format!("227 Entering Passive Mode (10,0,0,1,{},{})", dport / 256, dport % 256),
                    "STOR" => {
                        w.write_all(b"150 Go\r\n").unwrap();
                        let (d, _) = data_listener.accept().unwrap();
                        received = read_all(d);
                        "226 Done".to_string()
                    }
                    "QUIT" => {
                        w.write_all(b"221 Bye\r\n").unwrap();
                        break;
                    }
                    _ => "500 ?".to_string(),
                };
                w.write_all(reply.as_bytes()).unwrap();
                w.write_all(b"\r\n").unwrap();
            }
            (cmds, received)
        });
        store(&format!("ftp://127.0.0.1:{port}/www/a%20b.txt"), Some("u"), Some("p"), b"contenu").unwrap();
        let (cmds, received) = server.join().unwrap();
        assert_eq!(received, b"contenu");
        assert!(cmds.contains(&"CWD www".to_string()));
        assert!(cmds.contains(&"STOR a b.txt".to_string()));
    }
}
