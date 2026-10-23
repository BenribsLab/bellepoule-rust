//! Console de mise au point (port de `common/network/console/`).
//!
//! Un serveur TCP (`telnet hôte port`) accepte des clients ; chacun reçoit le
//! logo, l'invite `BellePoule> ` et peut taper :
//! - `help` (ou `h`) : aide ;
//! - `retained` (ou `r`) : résumé des objets retenus (fonction fournie par
//!   l'application via [`ConsoleServer::set_retained_dumper`]) ;
//! - `vars` (ou `v`) : liste des variables exposées ;
//! - `toggle <var>` (ou `t <var>`) : bascule une variable (préfixe unique).
//!
//! Les variables (`ExposeVariable` / `VariableIsSet`) sont globales au
//! processus et utilisables même sans serveur (ex. `heartbeat` de l'anneau,
//! `fix-affinities` du Marshaller).

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::netutil::color::{BLUE, ESC, GREEN, RED};

const LOGO: &str = concat!(
    "                              \n",
    "             .dkkOOOOkk:.     \n",
    "             .O00KXXKx,       \n",
    "             .0KK0Kx,         \n",
    "             .0K0xol:.        \n",
    "             .00kkOOOOo.      \n",
    "             .O00K0K000Ol.    \n",
    "           .lxddkKK000000'    \n",
    "         'd0KOdollx00OO00'    \n",
    "       'oO000Oxddd:,cxOOO'    \n",
    "     'lkOO000Okxc.    ,dO'    \n",
    "   ;d000OOOOOOc.        ,.    \n",
    "   ...........                \n",
    "                              \n",
);

const PROMPT: &str = "ManageComp> ";

/// Variables exposées : nom → valeur (ordre d'insertion conservé).
static VARIABLES: Mutex<Option<Vec<(String, u32)>>> = Mutex::new(None);

type Dumper = Box<dyn Fn() -> String + Send + Sync>;
static RETAINED_DUMPER: Mutex<Option<Dumper>> = Mutex::new(None);

/// Serveur de console : un thread d'acceptation, un thread par client.
pub struct ConsoleServer {
    port: u16,
    stop: Arc<AtomicBool>,
    clients: Arc<Mutex<Vec<TcpStream>>>,
    thread: Option<JoinHandle<()>>,
}

impl ConsoleServer {
    /// `ConsoleServer (port)` ; `port = 0` choisit un port libre.
    pub fn new(port: u16) -> std::io::Result<ConsoleServer> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let clients: Arc<Mutex<Vec<TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
        let thread = {
            let stop = stop.clone();
            let clients = clients.clone();
            std::thread::Builder::new().name("ConsoleServer".into()).spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = stream.set_nonblocking(false);
                            if let Ok(clone) = stream.try_clone() {
                                clients.lock().unwrap().push(clone);
                            }
                            let _ = std::thread::Builder::new()
                                .name("Console".into())
                                .spawn(move || Console::run(stream));
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        Err(e) => {
                            log::warn!("ConsoleServer: accept: {e}");
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            })?
        };
        Ok(ConsoleServer { port, stop, clients, thread: Some(thread) })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// `ExposeVariable` : déclare (ou réinitialise) une variable.
    pub fn expose_variable(variable: &str, initial_value: u32) {
        let mut vars = VARIABLES.lock().unwrap();
        let vars = vars.get_or_insert_with(Vec::new);
        match vars.iter_mut().find(|(k, _)| k == variable) {
            Some(v) => v.1 = initial_value,
            None => vars.push((variable.to_string(), initial_value)),
        }
    }

    /// `VariableIsSet` : vrai si la variable vaut autre chose que 0. Tant
    /// qu'aucune variable n'est exposée, toutes sont considérées comme actives.
    pub fn variable_is_set(variable: &str) -> bool {
        match &*VARIABLES.lock().unwrap() {
            Some(vars) => vars.iter().find(|(k, _)| k == variable).is_some_and(|(_, v)| *v != 0),
            None => true,
        }
    }

    /// Change la valeur d'une variable (équivalent programmatique de `toggle`).
    pub fn set_variable(variable: &str, value: u32) {
        Self::expose_variable(variable, value);
    }

    /// Fonction appelée par la commande `retained` (`Object::DumpList`).
    pub fn set_retained_dumper(dumper: Option<Box<dyn Fn() -> String + Send + Sync>>) {
        *RETAINED_DUMPER.lock().unwrap() = dumper;
    }

    /// Exécute une commande et renvoie le texte à afficher (`OnCommand`).
    pub fn execute(command: &str) -> String {
        let mut out = String::new();
        let args: Vec<&str> = command.trim().split(' ').collect();
        let first = args.first().copied().unwrap_or("");
        if command_is(first, "help") {
            out.push_str(&format!(
                "{BLUE} r{ESC}etained       dump a summary of the retained objects.\n\
                 {BLUE} v{ESC}ars           list all the variables.\n\
                 {BLUE} t{ESC}oggle var     toggle the given variable.\n"
            ));
        } else if command_is(first, "retained") {
            if let Some(dumper) = &*RETAINED_DUMPER.lock().unwrap() {
                out.push_str(&dumper());
            }
        } else if command_is(first, "vars") {
            if let Some(vars) = &*VARIABLES.lock().unwrap() {
                let width = vars.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
                for (k, v) in vars {
                    out.push_str(&echo_variable(k, *v, width));
                }
            }
        } else if command_is(first, "toggle") {
            if let Some(arg) = args.get(1) {
                let mut guard = VARIABLES.lock().unwrap();
                if let Some(vars) = guard.as_mut() {
                    let width = vars.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
                    let candidates: Vec<usize> =
                        vars.iter().enumerate().filter(|(_, (k, _))| k.starts_with(arg)).map(|(i, _)| i).collect();
                    if candidates.len() == 1 {
                        let var = &mut vars[candidates[0]];
                        var.1 = if var.1 != 0 { 0 } else { 1 };
                        out.push_str(&echo_variable(&var.0, var.1, width));
                    } else {
                        out.push_str("Unknown variable\n");
                    }
                }
            }
        }
        out
    }
}

impl Drop for ConsoleServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for client in self.clients.lock().unwrap().drain(..) {
            let _ = client.shutdown(Shutdown::Both);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Un client connecté (`Console`).
struct Console;

impl Console {
    fn run(mut stream: TcpStream) {
        if Self::echo(&mut stream, LOGO).is_err() || Self::echo(&mut stream, PROMPT).is_err() {
            return;
        }
        let mut command = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    for &c in &buffer[..n] {
                        if c == b'\n' {
                            let text = String::from_utf8_lossy(&command).into_owned();
                            command.clear();
                            let answer = ConsoleServer::execute(&text);
                            if !answer.is_empty() && Self::echo(&mut stream, &answer).is_err() {
                                return;
                            }
                            if Self::echo(&mut stream, PROMPT).is_err() {
                                return;
                            }
                        } else {
                            command.push(c);
                        }
                    }
                }
            }
        }
    }

    /// `Echo` : le C++ envoie aussi le zéro terminal de la chaîne.
    fn echo(stream: &mut TcpStream, message: &str) -> std::io::Result<()> {
        stream.write_all(message.as_bytes())?;
        stream.write_all(&[0])
    }
}

/// `CommandIs` : la commande complète ou sa seule initiale.
fn command_is(command: &str, name: &str) -> bool {
    if command.is_empty() || command.as_bytes()[0] != name.as_bytes()[0] {
        return false;
    }
    command.len() == 1 || command == name
}

fn echo_variable(name: &str, value: u32, width: usize) -> String {
    let flag = if value != 0 { format!(" [{GREEN}*{ESC}]") } else { format!(" [{RED}-{ESC}]") };
    format!("{name:<width$}{flag}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        assert!(command_is("h", "help"));
        assert!(command_is("help", "help"));
        assert!(!command_is("he", "help"));
        ConsoleServer::expose_variable("console-test-variable", 1);
        assert!(ConsoleServer::variable_is_set("console-test-variable"));
        let out = ConsoleServer::execute("t console-test-v");
        assert!(out.starts_with("console-test-variable"));
        assert!(!ConsoleServer::variable_is_set("console-test-variable"));
        assert!(ConsoleServer::execute("vars").contains("console-test-variable"));
    }

    #[test]
    fn telnet() {
        let server = ConsoleServer::new(0).unwrap();
        let mut s = TcpStream::connect(("127.0.0.1", server.port())).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(b"help\n").unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 1024];
        while !String::from_utf8_lossy(&got).contains("toggle the given variable") {
            let n = s.read(&mut buf).unwrap();
            assert!(n > 0);
            got.extend_from_slice(&buf[..n]);
        }
    }
}
