//! Anneau des partenaires (port de `common/network/ring.cpp`).
//!
//! # Protocole
//!
//! Chaque application de l'anneau (BellePoule = [`Role::ResourceUser`],
//! Marshaller = [`Role::ResourceManager`]) :
//! - ouvre un serveur HTTP sur un port TCP de 35830 à 35839
//!   ([`WifiCode::claim_ip_port`]) ; les partenaires y déposent leurs
//!   messages (`PUT /ring`, chiffrés avec la phrase secrète commune, IV dans
//!   l'en-tête `IV`) ;
//! - écoute en UDP le port d'annonce [`ANNOUNCE_PORT`] (35830) et diffuse sur
//!   l'adresse de diffusion de son sous-réseau un message `Ring::Announcement`
//!   (texte GKeyFile suivi d'un zéro) contenant `role`, `ip_address`,
//!   `unicast_port`, `partner` (identifiant aléatoire) et un défi :
//!   [`SECRET`] chiffré avec la phrase secrète (`challenge`/`iv`),
//!   éventuellement avec la phrase de l'application partenaire du même poste
//!   (`backup_challenge`/`backup_iv`) ou avec le numéro de série d'une clé
//!   USB (`usb_challenge`/`usb_iv`).
//!
//! À la réception d'une annonce d'un rôle différent, le récepteur répond par
//! `Ring::Handshake` (champ `response` = [`HandshakeResult`]) :
//! - défi relevé : il ajoute l'annonceur à ses partenaires, lui envoie les
//!   messages diffusés, puis répond `CHALLENGE_PASSED` avec ses propres
//!   coordonnées ; l'annonceur l'ajoute à son tour ;
//! - défi de secours relevé : `BACKUP_CHALLENGE_PASSED` ; l'annonceur adopte
//!   la phrase de l'application partenaire et s'annonce à nouveau ;
//! - défi USB : mémorisé jusqu'au branchement de la clé correspondante, puis
//!   `USB_TRUSTED` avec la phrase secrète (`password`), chiffré par le numéro
//!   de série ;
//! - sinon `AUTHENTICATION_FAILED`, ou `ROLE_REJECTED` si un second
//!   gestionnaire de ressources tente de rejoindre l'anneau.
//!
//! Ensuite : `Ring::Heartbeat` toutes les 5 s (variable de console
//! `heartbeat`), `Ring::Farewell` au départ. Les smartphones peuvent aussi
//! envoyer en UDP un `SmartPoule::ScoreSheetCall` (`sender`, `IV`, `cypher`)
//! qui contient un message chiffré par la clé de l'expéditeur.
//!
//! # Utilisation
//!
//! [`Ring::join`] renvoie l'anneau et un canal de [`RingEvent`] que
//! l'application relit depuis son thread graphique (ex. `glib` idle). Aucune
//! dépendance GTK : tout tourne dans des threads.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use crate::config::{PassphraseStore, UserConfigStore};
use crate::console::ConsoleServer;
use crate::credentials::Credentials;
use crate::cryptor::Cryptor;
use crate::flash_code::get_key256;
use crate::http_server::{HttpListener, HttpServer};
use crate::message::Message;
use crate::message_uploader::{PeerStatus, StatusListener};
use crate::netutil::guess_ipv4;
use crate::partner::{Partner, PartnerInfo};
use crate::usb_broker::{UsbBroker, UsbEvent};
use crate::usb_challenge::UsbChallenges;
use crate::usb_drive::UsbDrive;
use crate::wifi_code::WifiCode;

/// Port UDP des annonces.
pub const ANNOUNCE_PORT: u16 = 35830;
/// Texte chiffré dans les défis.
pub const SECRET: &str = "tagada soinsoin";
/// Période du battement de cœur.
pub const HEARTBEAT_PERIOD: Duration = Duration::from_secs(5);
/// Taille du tampon de réception des annonces.
const BUFFER_SIZE: usize = 2048;

/// Rôle dans l'anneau (valeur transmise dans le champ `role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Role {
    ResourceManager = 0,
    ResourceUser = 1,
}

impl Role {
    pub fn from_u32(v: u32) -> Option<Role> {
        match v {
            0 => Some(Role::ResourceManager),
            1 => Some(Role::ResourceUser),
            _ => None,
        }
    }

    /// `GetRoleImage`.
    pub fn image(self) -> &'static str {
        match self {
            Role::ResourceManager => "RESOURCE_MANAGER",
            Role::ResourceUser => "RESOURCE_USER",
        }
    }
}

/// Résultat d'une poignée de main (champ `response`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum HandshakeResult {
    NotApplicable = 0,
    ChallengePassed = 1,
    BackupChallengePassed = 2,
    AuthenticationFailed = 3,
    UsbTrusted = 4,
    RoleRejected = 5,
}

impl HandshakeResult {
    pub fn from_u32(v: u32) -> HandshakeResult {
        match v {
            1 => HandshakeResult::ChallengePassed,
            2 => HandshakeResult::BackupChallengePassed,
            3 => HandshakeResult::AuthenticationFailed,
            4 => HandshakeResult::UsbTrusted,
            5 => HandshakeResult::RoleRejected,
            _ => HandshakeResult::NotApplicable,
        }
    }
}

/// Événements livrés à l'application (`Ring::Listener`, `PartnerListener`).
#[derive(Debug, Clone)]
pub enum RingEvent {
    /// Message applicatif reçu (`OnMessage`).
    Message(Message),
    /// Fin d'une poignée de main (`OnHanshakeResult`).
    HandshakeResult(HandshakeResult),
    /// Clé USB branchée ou retirée (`OnUsbEvent`).
    Usb(UsbEvent, UsbDrive),
    /// Arrivée (`true`) ou départ (`false`) d'un partenaire ; l'indicateur
    /// réseau doit être rafraîchi ([`Ring::indicator_tooltip`]).
    PartnerJoined(PartnerInfo, bool),
    /// Départ terminé après [`Ring::leave`] (le C++ quittait alors GTK).
    Left,
}

/// Notification d'un écouteur de partenaires (`OnPartnerJoined`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartnerEvent {
    pub partner: PartnerInfo,
    pub joined: bool,
}

/// Fournit les clés des expéditeurs autres que l'anneau
/// (`Ring::Listener::GetSecretKey`) : `/sender/<réf>` pour un arbitre…
/// Appelé depuis les threads réseau.
pub trait SecretKeyProvider: Send + Sync {
    fn secret_key(&self, authentication_scheme: &str) -> Option<String>;
}

impl<F> SecretKeyProvider for F
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    fn secret_key(&self, authentication_scheme: &str) -> Option<String> {
        self(authentication_scheme)
    }
}

/// Paramètres de l'anneau.
pub struct RingConfig {
    pub role: Role,
    /// Port UDP d'écoute des annonces.
    pub announce_port: u16,
    /// Destination des annonces ; par défaut, adresse de diffusion du
    /// sous-réseau sur `announce_port`.
    pub announce_target: Option<SocketAddr>,
    /// Port du serveur HTTP ; `None` = [`WifiCode::claim_ip_port`], `Some(0)` = port libre.
    pub unicast_port: Option<u16>,
    /// Stockage de la phrase secrète.
    pub store: Arc<dyn PassphraseStore>,
    /// Programme de détection USB ; `None` désactive la détection.
    pub usb_backend: Option<PathBuf>,
    /// Port de la console de mise au point (1234 / 4321 en debug dans le C++).
    pub console_port: Option<u16>,
    pub heartbeat_period: Duration,
    /// Impose l'adresse IPv4 annoncée (sinon détectée).
    pub ipv4_address: Option<Ipv4Addr>,
}

impl RingConfig {
    /// Configuration standard d'une application.
    pub fn new(role: Role) -> Self {
        RingConfig {
            role,
            announce_port: ANNOUNCE_PORT,
            announce_target: None,
            unicast_port: None,
            store: Arc::new(UserConfigStore::for_role(role)),
            usb_backend: Some(UsbBroker::default_backend_path()),
            console_port: if cfg!(debug_assertions) {
                Some(if role == Role::ResourceManager { 1234 } else { 4321 })
            } else {
                None
            },
            heartbeat_period: HEARTBEAT_PERIOD,
            ipv4_address: None,
        }
    }
}

static BROKER: Mutex<Option<Ring>> = Mutex::new(None);

/// Anneau (poignée partagée, clonable).
#[derive(Clone)]
pub struct Ring {
    inner: Arc<Inner>,
}

struct Inner {
    role: Role,
    partner_id: u32,
    unicast_address: String,
    unicast_port: u16,
    announce_address: SocketAddr,
    key_provider: Arc<dyn SecretKeyProvider>,
    store: Arc<dyn PassphraseStore>,
    events: Mutex<Sender<RingEvent>>,
    state: Mutex<State>,
    stop: AtomicBool,
    services: Mutex<Services>,
}

#[derive(Default)]
struct Services {
    http_server: Option<HttpServer>,
    usb_broker: Option<UsbBroker>,
    console: Option<ConsoleServer>,
    heartbeat_stop: Option<Sender<()>>,
}

struct State {
    credentials: Credentials,
    partners: Vec<Partner>,
    messages: Vec<Message>,
    partner_listeners: Vec<(u64, Sender<PartnerEvent>)>,
    next_listener_id: u64,
    handshake_result: HandshakeResult,
    trusted_drive: Option<UsbDrive>,
    challenges: UsbChallenges<PartnerInfo>,
    quit_countdown: i32,
    leaving: HashSet<u32>,
    left: bool,
}

/// Identité d'un message dans la liste des messages diffusés.
fn same_message(a: &Message, b: &Message) -> bool {
    a.net_id() == b.net_id() && a.name() == b.name()
}

impl Ring {
    /// `Ring::Join` : crée l'anneau, l'enregistre comme courtier global
    /// ([`Ring::broker`]) et annonce sa présence.
    pub fn join(config: RingConfig, key_provider: Arc<dyn SecretKeyProvider>) -> (Ring, Receiver<RingEvent>) {
        let (ring, rx) = Self::start(config, key_provider);
        {
            let mut broker = BROKER.lock().unwrap();
            if broker.is_some() {
                log::warn!("Ring instanciated more than once");
            }
            *broker = Some(ring.clone());
        }
        (ring, rx)
    }

    /// Crée et annonce un anneau sans l'enregistrer comme courtier global.
    pub fn start(config: RingConfig, key_provider: Arc<dyn SecretKeyProvider>) -> (Ring, Receiver<RingEvent>) {
        let (tx, rx) = channel();
        let guess = guess_ipv4();
        let unicast_ip = config.ipv4_address.unwrap_or(guess.unicast);
        let broadcast = if config.ipv4_address.is_some() && config.ipv4_address != Some(guess.unicast) {
            config.ipv4_address.unwrap()
        } else {
            guess.broadcast
        };
        let announce_address = config
            .announce_target
            .unwrap_or_else(|| SocketAddr::V4(SocketAddrV4::new(broadcast, config.announce_port)));

        ConsoleServer::expose_variable("heartbeat", 1);

        // serveur HTTP (écoute unicast)
        let requested_port = config.unicast_port.unwrap_or_else(WifiCode::claim_ip_port);
        let http_listener = Arc::new(RingHttp { inner: OnceLock::new() });
        let http_server = match HttpServer::new(http_listener.clone(), requested_port) {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("HttpServer: port {requested_port}: {e}");
                None
            }
        };
        let unicast_port = http_server.as_ref().map(|s| s.port()).unwrap_or(requested_port);
        if config.unicast_port.is_some() {
            WifiCode::set_ip_port(unicast_port);
        }

        let unicast_address = unicast_ip.to_string();
        let passphrase = config.store.load().unwrap_or_else(get_key256);
        let credentials = Credentials::new(config.role.image(), Some(&unicast_address), unicast_port, &passphrase);

        let inner = Arc::new(Inner {
            role: config.role,
            partner_id: rand::random(),
            unicast_address,
            unicast_port,
            announce_address,
            key_provider,
            store: config.store.clone(),
            events: Mutex::new(tx),
            state: Mutex::new(State {
                credentials,
                partners: Vec::new(),
                messages: Vec::new(),
                partner_listeners: Vec::new(),
                next_listener_id: 1,
                handshake_result: HandshakeResult::NotApplicable,
                trusted_drive: None,
                challenges: UsbChallenges::default(),
                quit_countdown: -1,
                leaving: HashSet::new(),
                left: false,
            }),
            stop: AtomicBool::new(false),
            services: Mutex::new(Services::default()),
        });
        let _ = http_listener.inner.set(Arc::downgrade(&inner));
        inner.change_passphrase(&passphrase);

        let mut services = Services { http_server, ..Default::default() };

        // console de mise au point
        if let Some(port) = config.console_port {
            match ConsoleServer::new(port) {
                Ok(c) => services.console = Some(c),
                Err(e) => log::warn!("g_socket_bind: console {port}: {e}"),
            }
        }

        // détection USB
        if let Some(backend) = &config.usb_backend {
            let weak = Arc::downgrade(&inner);
            services.usb_broker = Some(UsbBroker::new(
                backend,
                Arc::new(move |event, drive: &UsbDrive| {
                    if let Some(inner) = weak.upgrade() {
                        inner.on_usb_event(event, drive.clone());
                    }
                }),
            ));
        }

        // écoute des annonces
        match bind_announce_socket(config.announce_port) {
            Ok(socket) => {
                let weak = Arc::downgrade(&inner);
                let _ = std::thread::Builder::new().name("Ring::OnMulticast".into()).spawn(move || {
                    let mut buffer = vec![0u8; BUFFER_SIZE];
                    loop {
                        let Some(inner) = weak.upgrade() else { break };
                        if inner.stop.load(Ordering::SeqCst) {
                            break;
                        }
                        drop(inner);
                        match socket.recv_from(&mut buffer) {
                            Ok((n, _)) => {
                                if let Some(inner) = weak.upgrade() {
                                    inner.on_datagram(&buffer[..n]);
                                }
                            }
                            Err(e)
                                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                            Err(e) => {
                                // ex. WSAECONNRESET sous Windows : sans conséquence
                                log::debug!("g_socket_receive: {e}");
                            }
                        }
                    }
                });
            }
            Err(e) => log::warn!("g_socket_bind: {e}"),
        }

        // battement de cœur
        {
            let (stop_tx, stop_rx) = channel::<()>();
            services.heartbeat_stop = Some(stop_tx);
            let weak = Arc::downgrade(&inner);
            let period = config.heartbeat_period;
            let _ = std::thread::Builder::new().name("Ring::SendHeartbeat".into()).spawn(move || loop {
                match stop_rx.recv_timeout(period) {
                    Err(RecvTimeoutError::Timeout) => match weak.upgrade() {
                        Some(inner) => inner.send_heartbeat(),
                        None => break,
                    },
                    _ => break,
                }
            });
        }

        *inner.services.lock().unwrap() = services;

        let ring = Ring { inner };
        ring.announce_availability();
        (ring, rx)
    }

    /// Courtier global (`Ring::_broker`).
    pub fn broker() -> Option<Ring> {
        BROKER.lock().unwrap().clone()
    }

    pub fn role(&self) -> Role {
        self.inner.role
    }

    pub fn partner_id(&self) -> u32 {
        self.inner.partner_id
    }

    /// `Leave` : adieu aux partenaires ; [`RingEvent::Left`] suit quand tous
    /// les envois sont terminés.
    pub fn leave(&self) {
        self.inner.leave();
    }

    /// `InjectMessage` : insère `message` juste après `after` dans la liste
    /// des messages diffusés (envoyé aux futurs partenaires seulement).
    pub fn inject_message(&self, message: &mut Message, after: &Message) {
        let mut st = self.inner.state.lock().unwrap();
        if let Some(i) = st.messages.iter().position(|m| same_message(m, after)) {
            message.set_fitness(1);
            st.messages.retain(|m| !same_message(m, message));
            let i = i.min(st.messages.len());
            let at = st.messages.iter().position(|m| same_message(m, after)).map(|j| j + 1).unwrap_or(i);
            st.messages.insert(at, message.clone());
        }
    }

    /// `SpreadMessage` : mémorise et envoie le message à tous les partenaires.
    pub fn spread_message(&self, message: &mut Message) {
        message.set_fitness(1);
        message.mark_spread(true);
        let st = &mut *self.inner.state.lock().unwrap();
        match st.messages.iter_mut().find(|m| same_message(m, message)) {
            Some(stored) => *stored = message.clone(),
            None => st.messages.push(message.clone()),
        }
        for p in &st.partners {
            p.send_message(message);
        }
    }

    /// `RecallMessage` : retire le message ; s'il avait été diffusé, sa
    /// version « fitness = 0 » est envoyée pour l'annuler chez les partenaires.
    pub fn recall_message(&self, message: &mut Message) {
        let st = &mut *self.inner.state.lock().unwrap();
        st.messages.retain(|m| !same_message(m, message));
        if message.fitness() != 0 {
            message.set_fitness(0);
            for p in &st.partners {
                p.send_message(message);
            }
        }
        message.mark_spread(false);
    }

    /// Oublie un message sans prévenir les partenaires (objet détruit).
    pub fn forget_message(&self, message: &Message) {
        self.inner.state.lock().unwrap().messages.retain(|m| !same_message(m, message));
    }

    /// Messages diffusés, du plus ancien au plus récent.
    pub fn spread_messages(&self) -> Vec<Message> {
        self.inner.state.lock().unwrap().messages.clone()
    }

    /// `GetCryptorKey` : phrase secrète courante.
    pub fn cryptor_key(&self) -> String {
        self.inner.state.lock().unwrap().credentials.key().to_string()
    }

    /// `RegisterPartnerListener` : les partenaires présents sont notifiés
    /// immédiatement. Renvoie l'identifiant de l'abonnement.
    pub fn register_partner_listener(&self, listener: Sender<PartnerEvent>) -> u64 {
        let st = &mut *self.inner.state.lock().unwrap();
        let id = st.next_listener_id;
        st.next_listener_id += 1;
        for p in &st.partners {
            let _ = listener.send(PartnerEvent { partner: p.info().clone(), joined: true });
        }
        st.partner_listeners.insert(0, (id, listener));
        id
    }

    pub fn unregister_partner_listener(&self, id: u64) {
        self.inner.state.lock().unwrap().partner_listeners.retain(|(i, _)| *i != id);
    }

    /// `GetPartner` : partenaire le plus récent.
    pub fn partner(&self) -> Option<PartnerInfo> {
        self.inner.state.lock().unwrap().partners.first().map(|p| p.info().clone())
    }

    /// Liste des partenaires (du plus récent au plus ancien).
    pub fn partners(&self) -> Vec<PartnerInfo> {
        self.inner.state.lock().unwrap().partners.iter().map(|p| p.info().clone()).collect()
    }

    /// Infobulle de l'indicateur réseau (`DisplayIndicator`) : `None` sans
    /// partenaire (icône `network-ko.png`), sinon une ligne `adresse:<b>port</b>`
    /// par partenaire (icône `network.png`).
    pub fn indicator_tooltip(&self) -> Option<String> {
        let st = self.inner.state.lock().unwrap();
        if st.partners.is_empty() {
            return None;
        }
        Some(
            st.partners
                .iter()
                .map(|p| format!("{}:<b>{}</b>", p.address().unwrap_or("(null)"), p.port()))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    /// `GetFlashCode` : identifiants à présenter en QR code.
    pub fn flash_code(&self) -> Credentials {
        self.inner.state.lock().unwrap().credentials.clone()
    }

    /// `GetIpV4Address`.
    pub fn ipv4_address(&self) -> String {
        self.inner.unicast_address.clone()
    }

    pub fn unicast_port(&self) -> u16 {
        self.inner.unicast_port
    }

    pub fn announce_address(&self) -> SocketAddr {
        self.inner.announce_address
    }

    pub fn handshake_result(&self) -> HandshakeResult {
        self.inner.state.lock().unwrap().handshake_result
    }

    /// `AnnounceAvailability`.
    pub fn announce_availability(&self) {
        self.inner.announce_availability();
    }

    /// `ChangePassphrase` : mémorise la phrase et renouvelle les identifiants.
    pub fn change_passphrase(&self, passphrase: &str) {
        self.inner.change_passphrase(passphrase);
    }

    /// `RetreiveBackupCredentials` : identifiants construits avec la phrase
    /// de l'application partenaire du même poste.
    pub fn retrieve_backup_credentials(&self) -> Option<Credentials> {
        self.inner.retrieve_backup_credentials()
    }

    /// `StampSender` : champ `address` = `http://ip:port` (adresse de réponse).
    pub fn stamp_sender(&self, message: &mut Message) {
        message.set_str("address", &format!("http://{}:{}", self.inner.unicast_address, self.inner.unicast_port));
    }

    /// Clé de déchiffrement d'un schéma d'authentification (`GetSecretKey`).
    pub fn secret_key(&self, authentication_scheme: &str) -> Option<String> {
        self.inner.get_secret_key(authentication_scheme)
    }
}

/// Écouteur HTTP de l'anneau.
struct RingHttp {
    inner: OnceLock<Weak<Inner>>,
}

impl HttpListener for RingHttp {
    fn secret_key(&self, scheme: &str) -> Option<String> {
        self.inner.get()?.upgrade()?.get_secret_key(scheme)
    }

    fn on_message(&self, message: Message) {
        if let Some(inner) = self.inner.get().and_then(Weak::upgrade) {
            inner.on_message(message);
        }
    }
}

fn bind_announce_socket(port: u16) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    socket.bind(&SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).into())?;
    let socket: UdpSocket = socket.into();
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;
    Ok(socket)
}

impl Inner {
    fn emit(&self, event: RingEvent) {
        let _ = self.events.lock().unwrap().send(event);
    }

    fn status_listener(self: &Arc<Self>, partner_id: u32) -> StatusListener {
        let weak = Arc::downgrade(self);
        Arc::new(move |status| {
            if let Some(inner) = weak.upgrade() {
                match status {
                    PeerStatus::ConnError => inner.on_partner_killed(partner_id),
                    PeerStatus::ConnClosed => inner.on_partner_leaved(partner_id),
                    PeerStatus::ConnOk => {}
                }
            }
        })
    }

    fn change_passphrase(&self, passphrase: &str) {
        self.store.save(passphrase);
        self.state.lock().unwrap().credentials =
            Credentials::new(self.role.image(), Some(&self.unicast_address), self.unicast_port, passphrase);
    }

    fn retrieve_backup_credentials(&self) -> Option<Credentials> {
        self.store
            .load_partner()
            .map(|pass| Credentials::new(self.role.image(), Some(&self.unicast_address), self.unicast_port, &pass))
    }

    fn announce_availability(&self) {
        let mut message = Message::new("Ring::Announcement");
        message.set_uint("role", self.role as u32);
        message.set_str("ip_address", &self.unicast_address);
        message.set_uint("unicast_port", self.unicast_port as u32);
        message.set_uint("partner", self.partner_id);

        let cryptor = Cryptor::new();
        let (result, key, trusted) = {
            let st = self.state.lock().unwrap();
            (st.handshake_result, st.credentials.key().to_string(), st.trusted_drive.clone())
        };
        match result {
            HandshakeResult::NotApplicable | HandshakeResult::BackupChallengePassed | HandshakeResult::UsbTrusted => {
                let (challenge, iv) = cryptor.encrypt(SECRET, &key);
                message.set_str("challenge", &challenge);
                message.set_str("iv", &iv);
                if let Some(backup) = self.retrieve_backup_credentials() {
                    let (challenge, iv) = cryptor.encrypt(SECRET, backup.key());
                    message.set_str("backup_challenge", &challenge);
                    message.set_str("backup_iv", &iv);
                }
            }
            HandshakeResult::AuthenticationFailed => {
                if let Some(drive) = trusted {
                    let (challenge, iv) = cryptor.encrypt(SECRET, drive.serial_number());
                    message.set_str("usb_challenge", &challenge);
                    message.set_str("usb_iv", &iv);
                }
            }
            _ => {}
        }
        self.multicast(&message);
    }

    /// `Multicast` : diffusion UDP du texte suivi de son zéro terminal.
    fn multicast(&self, message: &Message) {
        let socket = match UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("g_socket_new: {e}");
                return;
            }
        };
        let _ = socket.set_broadcast(true);
        let mut parcel = message.parcel().into_bytes();
        parcel.push(0);
        if let Err(e) = socket.send_to(&parcel, self.announce_address) {
            log::warn!("sendto: {e}");
        }
    }

    /// `OnMulticast`.
    fn on_datagram(self: &Arc<Self>, data: &[u8]) {
        let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
        let text = String::from_utf8_lossy(&data[..end]);
        let message = Message::parse(&text);
        if !message.is_valid() {
            return;
        }
        if message.is("Ring::Announcement") {
            let id = message.get_uint("partner");
            if id == self.partner_id {
                return;
            }
            let info = PartnerInfo::from_message(&message);
            let Some(role) = Role::from_u32(info.role) else { return };
            if self.role_is_acceptable(role, id) {
                if role != self.role {
                    let key = self.state.lock().unwrap().credentials.key().to_string();
                    if decrypt_secret(&message, None, &key) {
                        let partner = Partner::new(&message, Some(self.status_listener(id)));
                        self.add(partner);
                        self.send_handshake_to(id, &message, HandshakeResult::ChallengePassed);
                    } else if decrypt_secret(&message, Some("backup"), &key) {
                        let partner = Partner::new(&message, None);
                        self.send_handshake(&partner, HandshakeResult::BackupChallengePassed);
                    } else if message.has_field("usb_challenge") {
                        let cypher = message.get_str("usb_challenge").unwrap_or_default();
                        let iv = message.get_str("usb_iv").unwrap_or_default();
                        self.state.lock().unwrap().challenges.monitor(info, &cypher, &iv);
                    } else {
                        let partner = Partner::new(&message, None);
                        self.send_handshake(&partner, HandshakeResult::AuthenticationFailed);
                    }
                }
            } else {
                let partner = Partner::new(&message, None);
                self.send_handshake(&partner, HandshakeResult::RoleRejected);
            }
        } else if message.is("SmartPoule::ScoreSheetCall") {
            let iv = message.get_str("IV").unwrap_or_default();
            let cypher = message.get_str("cypher").unwrap_or_default();
            let sender = message.get_str("sender").unwrap_or_default();
            let Some(key) = self.key_provider.secret_key(&sender) else {
                log::warn!("SmartPoule::ScoreSheetCall: clé inconnue pour {sender}");
                return;
            };
            match Cryptor::decrypt(&cypher, &iv, &key) {
                Some(hidden) => {
                    let hidden = Message::parse(&hidden);
                    if hidden.is_valid() {
                        self.emit(RingEvent::Message(hidden));
                    }
                }
                None => log::warn!("SmartPoule::ScoreSheetCall: déchiffrement impossible"),
            }
        }
    }

    /// `RoleIsAcceptable` : un seul gestionnaire de ressources par anneau
    /// (le partenaire déjà connu sous le même identifiant reste accepté).
    fn role_is_acceptable(&self, partner_role: Role, partner_id: u32) -> bool {
        if partner_role == Role::ResourceManager {
            if self.role == Role::ResourceManager {
                return false;
            }
            let st = self.state.lock().unwrap();
            if st.partners.iter().any(|p| p.has_role(Role::ResourceManager as u32) && !p.is(partner_id)) {
                return false;
            }
        }
        true
    }

    fn handshake_message(&self, result: HandshakeResult) -> Message {
        let mut message = Message::new("Ring::Handshake");
        if matches!(result, HandshakeResult::ChallengePassed | HandshakeResult::BackupChallengePassed) {
            message.set_uint("role", self.role as u32);
            message.set_str("ip_address", &self.unicast_address);
            message.set_uint("unicast_port", self.unicast_port as u32);
            message.set_uint("partner", self.partner_id);
        }
        message.set_uint("response", result as u32);
        message.set_fitness(1);
        message
    }

    /// `SendHandshake` vers un partenaire non enregistré (message en clair).
    fn send_handshake(&self, partner: &Partner, result: HandshakeResult) {
        partner.send_message(&self.handshake_message(result));
    }

    /// `SendHandshake` vers un partenaire enregistré (chiffré par sa phrase).
    fn send_handshake_to(&self, partner_id: u32, announce: &Message, result: HandshakeResult) {
        let message = self.handshake_message(result);
        let st = self.state.lock().unwrap();
        match st.partners.iter().find(|p| p.is(partner_id)) {
            Some(p) => p.send_message(&message),
            None => Partner::new(announce, None).send_message(&message),
        }
    }

    fn notify_partner_status(st: &mut State, info: &PartnerInfo, joined: bool) {
        st.partner_listeners
            .retain(|(_, l)| l.send(PartnerEvent { partner: info.clone(), joined }).is_ok());
    }

    /// `Add` : enregistre le partenaire et lui envoie les messages diffusés.
    fn add(&self, mut partner: Partner) {
        let info = partner.info().clone();
        {
            let st = &mut *self.state.lock().unwrap();
            if st.partners.iter().any(|p| p.is(info.partner_id)) {
                return;
            }
            partner.set_passphrase256(Some(st.credentials.key()));
            Self::notify_partner_status(st, &info, true);
            // `Synchronize` : du plus ancien au plus récent
            for m in &st.messages {
                partner.send_message(m);
            }
            st.partners.insert(0, partner);
        }
        self.emit(RingEvent::PartnerJoined(info, true));
    }

    /// `Remove`.
    fn remove(&self, partner_id: u32) -> bool {
        let info = {
            let st = &mut *self.state.lock().unwrap();
            let Some(i) = st.partners.iter().position(|p| p.is(partner_id)) else { return false };
            let mut partner = st.partners.remove(i);
            let info = partner.info().clone();
            Self::notify_partner_status(st, &info, false);
            partner.leave();
            info
        };
        self.emit(RingEvent::PartnerJoined(info, false));
        true
    }

    fn on_partner_killed(&self, partner_id: u32) {
        self.remove(partner_id);
    }

    /// `OnPartnerLeaved` : décompte des départs pendant [`Ring::leave`].
    fn on_partner_leaved(&self, partner_id: u32) {
        let done = {
            let st = &mut *self.state.lock().unwrap();
            if st.quit_countdown != -1 && st.leaving.remove(&partner_id) {
                st.quit_countdown -= 1;
                st.quit_countdown == 0
            } else {
                false
            }
        };
        if done {
            self.release();
        }
    }

    fn leave(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(stop) = self.services.lock().unwrap().heartbeat_stop.take() {
            let _ = stop.send(());
        }
        let mut farewell = Message::new("Ring::Farewell");
        farewell.set_uint("partner", self.partner_id);
        farewell.set_uint("role", self.role as u32);

        let ids: Vec<u32> = {
            let st = &mut *self.state.lock().unwrap();
            st.quit_countdown = 0;
            st.partners.iter().map(|p| p.info().partner_id).collect()
        };
        for id in ids {
            {
                let st = &mut *self.state.lock().unwrap();
                if let Some(p) = st.partners.iter().find(|p| p.is(id)) {
                    p.send_message(&farewell);
                }
                st.leaving.insert(id);
                st.quit_countdown += 1;
            }
            self.remove(id);
        }
        let done = self.state.lock().unwrap().quit_countdown == 0;
        if done {
            self.release();
        }
    }

    /// Fin de vie de l'anneau (destructeur du C++).
    fn release(&self) {
        {
            let mut st = self.state.lock().unwrap();
            if st.left {
                return;
            }
            st.left = true;
        }
        let services = std::mem::take(&mut *self.services.lock().unwrap());
        drop(services);
        {
            let mut broker = BROKER.lock().unwrap();
            if broker.as_ref().is_some_and(|r| std::ptr::eq(Arc::as_ptr(&r.inner), self)) {
                *broker = None;
            }
        }
        self.emit(RingEvent::Left);
    }

    /// `SendHeartbeat`.
    fn send_heartbeat(&self) {
        if ConsoleServer::variable_is_set("heartbeat") {
            let mut heartbeat = Message::new("Ring::Heartbeat");
            heartbeat.set_uint("partner", self.partner_id);
            let st = self.state.lock().unwrap();
            for p in &st.partners {
                p.send_message(&heartbeat);
            }
        }
    }

    fn get_secret_key(&self, scheme: &str) -> Option<String> {
        if scheme == "/ring" || scheme == "/sender/0" {
            let st = self.state.lock().unwrap();
            match &st.trusted_drive {
                Some(d) => Some(d.serial_number().to_string()),
                None => Some(st.credentials.key().to_string()),
            }
        } else {
            self.key_provider.secret_key(scheme)
        }
    }

    /// `OnMessage` (messages reçus par le serveur HTTP).
    fn on_message(self: &Arc<Self>, message: Message) {
        if message.is("Ring::Handshake") {
            let result = HandshakeResult::from_u32(message.get_uint("response"));
            self.state.lock().unwrap().handshake_result = result;
            if result == HandshakeResult::BackupChallengePassed {
                if let Some(credentials) = self.retrieve_backup_credentials() {
                    self.change_passphrase(credentials.key());
                    self.announce_availability();
                }
            } else {
                if result == HandshakeResult::ChallengePassed {
                    let id = message.get_uint("partner");
                    self.add(Partner::new(&message, Some(self.status_listener(id))));
                } else if result == HandshakeResult::UsbTrusted {
                    let password = message.get_str("password").unwrap_or_default();
                    self.change_passphrase(&password);
                    self.state.lock().unwrap().trusted_drive = None;
                    self.announce_availability();
                }
                self.emit(RingEvent::HandshakeResult(result));
            }
        } else if message.is("Ring::Farewell") {
            let id = message.get_uint("partner");
            if id != self.partner_id {
                self.remove(id);
            }
        } else {
            self.emit(RingEvent::Message(message));
        }
    }

    /// `OnUsbEvent`.
    fn on_usb_event(self: &Arc<Self>, event: UsbEvent, drive: UsbDrive) {
        match event {
            UsbEvent::StickPlugged => {
                let trusted = self.state.lock().unwrap().challenges.check_key(drive.serial_number(), SECRET);
                for info in trusted {
                    self.on_usb_partner_trusted(info, drive.serial_number());
                }
            }
            UsbEvent::StickUnplugged => {
                let failed = {
                    let mut st = self.state.lock().unwrap();
                    if st.handshake_result == HandshakeResult::AuthenticationFailed {
                        st.trusted_drive = Some(drive.clone());
                        true
                    } else {
                        false
                    }
                };
                if failed {
                    self.announce_availability();
                }
            }
        }
        self.emit(RingEvent::Usb(event, drive));
    }

    /// `OnUsbPartnerTrusted` : transmet la phrase secrète, chiffrée par le
    /// numéro de série de la clé.
    fn on_usb_partner_trusted(&self, info: PartnerInfo, key: &str) {
        let mut message = Message::new("Ring::Handshake");
        message.set_uint("response", HandshakeResult::UsbTrusted as u32);
        message.set_str("password", self.state.lock().unwrap().credentials.key());
        message.set_fitness(1);
        message.set_passphrase256(Some(key));
        Partner::from_info(info, None).send_message(&message);
    }
}

/// `DecryptSecret` : le champ `[type_]challenge` déchiffré avec `key`
/// redonne-t-il [`SECRET`] ?
fn decrypt_secret(message: &Message, kind: Option<&str>, key: &str) -> bool {
    let (challenge_field, iv_field) = match kind {
        Some(t) => (format!("{t}_challenge"), format!("{t}_iv")),
        None => ("challenge".to_string(), "iv".to_string()),
    };
    match (message.get_str(&challenge_field), message.get_str(&iv_field)) {
        (Some(c), Some(iv)) => Cryptor::decrypt(&c, &iv, key).is_some_and(|s| s.eq_ignore_ascii_case(SECRET)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MemoryStore;

    fn free_udp_port() -> u16 {
        UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    fn config(role: Role, listen: u16, target: u16, store: Arc<MemoryStore>) -> RingConfig {
        RingConfig {
            role,
            announce_port: listen,
            announce_target: Some(SocketAddr::from(([127, 0, 0, 1], target))),
            unicast_port: Some(0),
            store,
            usb_backend: None,
            console_port: None,
            heartbeat_period: Duration::from_millis(200),
            ipv4_address: Some(Ipv4Addr::LOCALHOST),
        }
    }

    fn no_keys() -> Arc<dyn SecretKeyProvider> {
        Arc::new(|_: &str| None)
    }

    /// Attend un événement satisfaisant `pred`.
    fn wait_for(rx: &Receiver<RingEvent>, pred: impl Fn(&RingEvent) -> bool) -> RingEvent {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let ev = rx.recv_timeout(left).expect("événement attendu");
            if pred(&ev) {
                return ev;
            }
        }
    }

    const KEY: &str = "Ab0Cd1Ef2Gh3Ij4Kl5Mn6Op7Qr8St0Uv";

    #[test]
    fn two_rings_discover_each_other() {
        let (pa, pb) = (free_udp_port(), free_udp_port());
        let (a, rx_a) = Ring::start(config(Role::ResourceManager, pa, pb, Arc::new(MemoryStore::new(Some(KEY), None))), no_keys());
        // message diffusé avant l'arrivée du partenaire : synchronisé à l'ajout
        let mut early = Message::new("BellePoule::Job");
        early.set_str("xml", "<Job/>");
        a.spread_message(&mut early);
        assert!(early.is_spread());

        let (b, rx_b) = Ring::start(config(Role::ResourceUser, pb, pa, Arc::new(MemoryStore::new(Some(KEY), None))), no_keys());

        wait_for(&rx_b, |e| matches!(e, RingEvent::HandshakeResult(HandshakeResult::ChallengePassed)));
        wait_for(&rx_a, |e| matches!(e, RingEvent::PartnerJoined(_, true)));
        assert_eq!(a.partners().len(), 1);
        assert_eq!(b.partners().len(), 1);
        assert_eq!(a.partner().unwrap().partner_id, b.partner_id());
        assert!(a.indicator_tooltip().unwrap().starts_with("127.0.0.1:<b>"));

        let got = wait_for(&rx_b, |e| matches!(e, RingEvent::Message(m) if m.is("BellePoule::Job")));
        if let RingEvent::Message(m) = got {
            assert_eq!(m.get_str("xml").as_deref(), Some("<Job/>"));
            assert_eq!(m.fitness(), 1);
        }

        // diffusion puis rappel
        let mut score = Message::new("BellePoule::Score");
        score.set_uint("piste", 4);
        b.spread_message(&mut score);
        wait_for(&rx_a, |e| matches!(e, RingEvent::Message(m) if m.is("BellePoule::Score") && m.fitness() == 1));
        b.recall_message(&mut score);
        wait_for(&rx_a, |e| matches!(e, RingEvent::Message(m) if m.is("BellePoule::Score") && m.fitness() == 0));
        assert!(b.spread_messages().is_empty());

        // battement de cœur
        wait_for(&rx_a, |e| matches!(e, RingEvent::Message(m) if m.is("Ring::Heartbeat")));

        // départ
        b.leave();
        wait_for(&rx_a, |e| matches!(e, RingEvent::PartnerJoined(_, false)));
        wait_for(&rx_b, |e| matches!(e, RingEvent::Left));
        assert!(a.partners().is_empty());
        a.leave();
        wait_for(&rx_a, |e| matches!(e, RingEvent::Left));
    }

    #[test]
    fn wrong_passphrase_then_backup_credentials() {
        let (pa, pb) = (free_udp_port(), free_udp_port());
        const OTHER: &str = "ZyXwVuTsRqPoNmLkJiHgFeDcBa876543";
        // A connaît la phrase de l'application partenaire du poste (secours)
        let (a, rx_a) =
            Ring::start(config(Role::ResourceManager, pa, pb, Arc::new(MemoryStore::new(Some(KEY), Some(OTHER)))), no_keys());
        let (b, rx_b) = Ring::start(config(Role::ResourceUser, pb, pa, Arc::new(MemoryStore::new(Some(OTHER), None))), no_keys());
        wait_for(&rx_b, |e| matches!(e, RingEvent::HandshakeResult(HandshakeResult::AuthenticationFailed)));
        assert_eq!(b.handshake_result(), HandshakeResult::AuthenticationFailed);

        // nouvelle annonce de A : B relève le défi de secours, A adopte la
        // phrase de B et s'annonce de nouveau
        a.announce_availability();
        wait_for(&rx_a, |e| matches!(e, RingEvent::HandshakeResult(HandshakeResult::ChallengePassed)));
        assert_eq!(a.cryptor_key(), OTHER);
        assert_eq!(a.partners().len(), 1);
        a.leave();
        b.leave();
    }

    #[test]
    fn second_manager_is_rejected() {
        let (pa, pb) = (free_udp_port(), free_udp_port());
        let (_a, _rx_a) = Ring::start(config(Role::ResourceManager, pa, pb, Arc::new(MemoryStore::new(Some(KEY), None))), no_keys());
        // un utilisateur qui connaît déjà un gestionnaire refuse le second
        let (b, rx_b) = Ring::start(config(Role::ResourceUser, pb, pa, Arc::new(MemoryStore::new(Some(KEY), None))), no_keys());
        wait_for(&rx_b, |e| matches!(e, RingEvent::PartnerJoined(_, true)));
        let pc = free_udp_port();
        let (_c, rx_c) = Ring::start(config(Role::ResourceManager, pc, pb, Arc::new(MemoryStore::new(Some(KEY), None))), no_keys());
        wait_for(&rx_c, |e| matches!(e, RingEvent::HandshakeResult(HandshakeResult::RoleRejected)));
        assert_eq!(b.partners().len(), 1);
    }

    #[test]
    fn usb_challenge_flow() {
        // logique sans réseau : déchiffrement des défis
        let (challenge, iv) = Cryptor::new().encrypt(SECRET, KEY);
        let mut m = Message::new("Ring::Announcement");
        m.set_str("backup_challenge", &challenge);
        m.set_str("backup_iv", &iv);
        assert!(!decrypt_secret(&m, None, KEY));
        assert!(decrypt_secret(&m, Some("backup"), KEY));
        assert!(!decrypt_secret(&m, Some("backup"), "mauvaise"));
    }

    #[test]
    fn score_sheet_call_over_udp() {
        let (pa, pb) = (free_udp_port(), free_udp_port());
        let referee_key = "RefereeKeyRefereeKeyRefereeKey12".to_string();
        let k = referee_key.clone();
        let keys: Arc<dyn SecretKeyProvider> = Arc::new(move |s: &str| (s == "/sender/12").then(|| k.clone()));
        let (a, rx) = Ring::start(config(Role::ResourceUser, pa, pb, Arc::new(MemoryStore::new(Some(KEY), None))), keys);
        let mut hidden = Message::new("SmartPoule::Score");
        hidden.set_uint("bout", 3);
        let (cypher, iv) = Cryptor::new().encrypt(&hidden.parcel(), &referee_key);
        let mut call = Message::new("SmartPoule::ScoreSheetCall");
        call.set_str("sender", "/sender/12");
        call.set_str("IV", &iv);
        call.set_str("cypher", &cypher);
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut data = call.parcel().into_bytes();
        data.push(0);
        s.send_to(&data, ("127.0.0.1", pa)).unwrap();
        let ev = wait_for(&rx, |e| matches!(e, RingEvent::Message(m) if m.is("SmartPoule::Score")));
        if let RingEvent::Message(m) = ev {
            assert_eq!(m.get_uint("bout"), 3);
        }
        let mut stamped = Message::new("X");
        a.stamp_sender(&mut stamped);
        assert_eq!(stamped.get_str("address").unwrap(), format!("http://127.0.0.1:{}", a.unicast_port()));
        a.leave();
    }
}
