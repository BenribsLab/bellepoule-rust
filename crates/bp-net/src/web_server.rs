//! Site web local de consultation des compétitions (port de `web_server.cpp`
//! et de l'installation du site faite dans `supervisor/main.cpp`).
//!
//! Le site (`resources/www.zip`, pages PHP « cot2html ») est extrait dans
//! `<config>/BellePoule/www/` ; les fichiers `.cotcot` publiés sont copiés
//! dans `www/cotcot/` (`Global::_www`). Comme le C++ sous Linux, le site est
//! servi par l'interpréteur PHP (`php -S ip:port`, dossier `www`). Si aucun
//! PHP n'est installé, un serveur intégré prend le relais : il sert les
//! fichiers statiques du site et une page de sélection des compétitions
//! (équivalent de `selcotpage.php`) donnant accès aux fichiers `.cotcot`.

use std::io::Read;
use std::net::SocketAddrV4;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::http_server::serve_file;
use crate::netutil::{uri_escape, user_config_dir};

/// Port du site : 80 sous Windows (suffixe vide), 8080 ailleurs (`_www_port`).
pub fn www_port() -> u16 {
    if cfg!(windows) {
        80
    } else {
        8080
    }
}

/// Suffixe d'URL du port (`Global::_www_port`).
pub fn www_port_suffix() -> &'static str {
    if cfg!(windows) {
        ""
    } else {
        ":8080"
    }
}

/// Adresse du site (`http://ip[:8080]`), affichée et mise en QR code.
pub fn www_url(ip: &str) -> String {
    format!("http://{ip}{}", www_port_suffix())
}

/// Extrait `www.zip` dans `dest` (les entrées commencent par `www/`).
pub fn extract_www_zip(zip_path: &Path, dest: &Path) -> std::io::Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(std::io::Error::other)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(std::io::Error::other)?;
        let Some(name) = entry.enclosed_name() else { continue };
        let out = dest.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        if let Err(e) = std::fs::write(&out, &data) {
            log::warn!("g_file_set_contents: {}: {e}", out.display());
        }
    }
    Ok(())
}

/// Prépare le site et renvoie le dossier des `.cotcot` (`Global::_www`).
///
/// Un site installé avec l'application (`<share>/webserver/LightTPD/www/cotcot`)
/// est utilisé tel quel ; sinon `www.zip` est extrait dans le dossier de
/// configuration de l'utilisateur.
pub fn prepare_www(share_dir: &Path) -> PathBuf {
    let bundled = share_dir.join("webserver").join("LightTPD").join("www").join("cotcot");
    if bundled.is_dir() {
        return bundled;
    }
    let base = user_config_dir().join("BellePoule");
    let cotcot = base.join("www").join("cotcot");
    let zip_path = share_dir.join("resources").join("www.zip");
    if let Err(e) = extract_www_zip(&zip_path, &base) {
        log::warn!("zip_open ({}): {e}", zip_path.display());
    }
    if let Err(e) = std::fs::create_dir_all(&cotcot) {
        log::warn!("g_mkdir_with_parents: {}: {e}", cotcot.display());
    }
    cotcot
}

/// Rappel d'état (`WebServer::Listener::OnWebServerState (in_progress, on)`),
/// appelé depuis un thread de surveillance.
pub type WebServerListener = Arc<dyn Fn(bool, bool) + Send + Sync>;

/// Mode de service effectivement utilisé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebServerMode {
    Php,
    BuiltIn,
}

pub struct WebServer {
    cotcot_dir: PathBuf,
    listener: WebServerListener,
    child: Arc<Mutex<Option<Child>>>,
    builtin: Option<BuiltIn>,
    php_candidates: Vec<String>,
    port: u16,
}

struct BuiltIn {
    server: Arc<tiny_http::Server>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for BuiltIn {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl WebServer {
    /// `cotcot_dir` : dossier renvoyé par [`prepare_www`].
    pub fn new(cotcot_dir: &Path, listener: WebServerListener) -> Self {
        let mut php_candidates = Vec::new();
        if let Ok(p) = std::env::var("BELLEPOULE_PHP") {
            php_candidates.push(p);
        }
        php_candidates.extend(["php", "php7.4", "php8.3", "php8.2", "php8.1"].map(String::from));
        WebServer {
            cotcot_dir: cotcot_dir.to_path_buf(),
            listener,
            child: Arc::new(Mutex::new(None)),
            builtin: None,
            php_candidates,
            port: www_port(),
        }
    }

    /// Change le port (tests, port 80 indisponible…).
    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }

    /// Interpréteurs PHP essayés dans l'ordre (vide = serveur intégré seul).
    pub fn set_php_candidates(&mut self, candidates: Vec<String>) {
        self.php_candidates = candidates;
    }

    /// Dossier racine du site (parent de `cotcot`).
    pub fn www_dir(&self) -> PathBuf {
        self.cotcot_dir.parent().map(Path::to_path_buf).unwrap_or_else(|| self.cotcot_dir.clone())
    }

    /// `Start` : (re)lance le service sur `ip`.
    pub fn start(&mut self, ip: &str) -> Option<WebServerMode> {
        self.stop();
        let www = self.www_dir();
        for php in &self.php_candidates {
            let spawned = Command::new(php)
                .arg("-S")
                .arg(format!("{ip}:{}", self.port))
                .current_dir(&www)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            match spawned {
                Ok(child) => {
                    *self.child.lock().unwrap() = Some(child);
                    self.watch_child();
                    (self.listener)(false, true);
                    return Some(WebServerMode::Php);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => log::warn!("g_spawn_async: {php}: {e}"),
            }
        }
        match self.start_builtin(ip) {
            Ok(()) => {
                (self.listener)(false, true);
                Some(WebServerMode::BuiltIn)
            }
            Err(e) => {
                log::warn!("WebServer: {e}");
                (self.listener)(false, false);
                None
            }
        }
    }

    /// `Stop`.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if self.builtin.take().is_some() {
            (self.listener)(false, false);
        }
    }

    /// `OnChildStatus` : signale l'arrêt du processus PHP.
    fn watch_child(&self) {
        let child = self.child.clone();
        let listener = self.listener.clone();
        let _ = std::thread::Builder::new().name("WebServer".into()).spawn(move || loop {
            std::thread::sleep(Duration::from_millis(250));
            let mut guard = child.lock().unwrap();
            match guard.as_mut() {
                None => {
                    drop(guard);
                    listener(false, false);
                    break;
                }
                Some(c) => {
                    if let Ok(Some(_)) = c.try_wait() {
                        *guard = None;
                        drop(guard);
                        listener(false, false);
                        break;
                    }
                }
            }
        });
    }

    fn start_builtin(&mut self, ip: &str) -> std::io::Result<()> {
        let addr: std::net::Ipv4Addr = ip.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
        let server = tiny_http::Server::http(SocketAddrV4::new(addr, self.port)).map_err(std::io::Error::other)?;
        let server = Arc::new(server);
        let www = self.www_dir();
        let cotcot = self.cotcot_dir.clone();
        let s = server.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new().name("WebServer::BuiltIn".into()).spawn(move || {
            for request in s.incoming_requests() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let path = request.url().split('?').next().unwrap_or("/").to_string();
                let response = match path.as_str() {
                    "/" | "/index.php" | "/selcotpage.php" => {
                        let header = tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap();
                        tiny_http::Response::from_string(selection_page(&cotcot)).with_header(header).boxed()
                    }
                    p if p.ends_with(".php") => tiny_http::Response::empty(404).boxed(),
                    _ => serve_file(&www, request.url()),
                };
                let _ = request.respond(response);
            }
        })?;
        self.builtin = Some(BuiltIn { server, thread: Some(thread) });
        Ok(())
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Attributs de l'élément racine d'un `.cotcot` (`scancot`).
fn root_attributes(path: &Path) -> Option<[String; 4]> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let start = text.find("<CompetitionIndividuelle").or_else(|| text.find("<CompetitionParEquipes"))?;
    let end = text[start..].find('>')? + start;
    let tag = &text[start..end];
    let attr = |name: &str| -> String {
        let pat = format!(" {name}=\"");
        tag.find(&pat)
            .and_then(|i| {
                let v = &tag[i + pat.len()..];
                v.find('"').map(|j| v[..j].to_string())
            })
            .unwrap_or_default()
    };
    Some([attr("TitreLong"), attr("Sexe"), attr("Categorie"), attr("Arme")])
}

/// Page de sélection (`selcotpage.php` + `selcot_table`).
pub fn selection_page(cotcot_dir: &Path) -> String {
    let mut files: Vec<PathBuf> = std::fs::read_dir(cotcot_dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect())
        .unwrap_or_default();
    files.sort();
    let mut tbl = String::from(
        "<table class=\"liste_cots\"><tr><th>COTCOT</th><th>Titre</th><th>Sexe</th><th>Catégorie</th><th>Arme</th></tr>",
    );
    for f in files {
        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let Some([titre, sexe, cat, arme]) = root_attributes(&f) else { continue };
        let href = format!("cotcot/{}", uri_escape(&name));
        tbl.push_str(&format!(
            "<tr onclick=\"document.location='{href}';\"><td>cotcot/{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            html_escape(&name),
            html_escape(&titre),
            html_escape(&sexe),
            html_escape(&cat),
            html_escape(&arme)
        ));
    }
    tbl.push_str("</table>");
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"UTF-8\">\n<title>SELECTION</title>\n<style>\n\
         table {{ border-collapse: collapse; }}\ntable td {{ border : 1px solid black; }}\n\
         table tr:hover {{ background-color : #AAA; }}\n</style>\n</head>\n<body>\n<h1>Selection cotcot</h1>\n{tbl}\n</body>\n</html>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_server_and_selection() {
        let base = std::env::temp_dir().join(format!("bp-net-web-{}", rand::random::<u32>()));
        let cotcot = base.join("www").join("cotcot");
        std::fs::create_dir_all(&cotcot).unwrap();
        std::fs::write(
            cotcot.join("test.cotcot"),
            "<?xml version=\"1.0\"?>\n<CompetitionIndividuelle TitreLong=\"Coupe &amp; co\" Sexe=\"M\" Categorie=\"S\" Arme=\"E\">\n</CompetitionIndividuelle>\n",
        )
        .unwrap();
        std::fs::write(base.join("www").join("const.css"), "body{}").unwrap();
        let page = selection_page(&cotcot);
        assert!(page.contains("cotcot/test.cotcot"));

        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let states = Arc::new(Mutex::new(Vec::new()));
        let st = states.clone();
        let mut ws = WebServer::new(&cotcot, Arc::new(move |p, on| st.lock().unwrap().push((p, on))));
        ws.set_port(port);
        ws.set_php_candidates(vec![]);
        assert_eq!(ws.start("127.0.0.1"), Some(WebServerMode::BuiltIn));
        let body = ureq::get(&format!("http://127.0.0.1:{port}/")).call().unwrap().into_string().unwrap();
        assert!(body.contains("Selection cotcot"));
        let css = ureq::get(&format!("http://127.0.0.1:{port}/const.css")).call().unwrap().into_string().unwrap();
        assert_eq!(css, "body{}");
        ws.stop();
        assert_eq!(*states.lock().unwrap(), vec![(false, true), (false, false)]);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn www_zip_extraction() {
        let zip = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/www.zip");
        if !zip.exists() {
            return;
        }
        let dest = std::env::temp_dir().join(format!("bp-net-zip-{}", rand::random::<u32>()));
        extract_www_zip(&zip, &dest).unwrap();
        assert!(dest.join("www").join("index.php").is_file());
        let _ = std::fs::remove_dir_all(dest);
    }
}
