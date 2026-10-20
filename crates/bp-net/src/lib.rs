//! Réseau BellePoule (port de `common/network/`, hors Facebook/Twitter/OAuth).
//! Voir ARCHITECTURE.md.
//!
//! Aucune dépendance GTK : les services tournent dans des threads et
//! livrent leurs événements par canaux (`std::sync::mpsc`) ou rappels.
//!
//! - [`ring`] : anneau des partenaires (annonces UDP, poignées de main,
//!   diffusion/rappel des messages, battement de cœur, défi USB) ;
//! - [`http_server`] : réception des messages (`PUT`/`POST`) et pages ;
//! - [`message_uploader`], [`file_uploader`], [`greg_uploader`],
//!   [`downloader`], [`uploader`] : clients HTTP(S)/FTP ;
//! - [`cryptor`], [`credentials`], [`flash_code`], [`wifi_code`] :
//!   chiffrement AES-256-CBC et QR codes ;
//! - [`usb_broker`], [`usb_drive`], [`usb_challenge`] : clés USB ;
//! - [`web_server`] : site web de consultation ; [`console`] : console de
//!   mise au point ; [`advertiser`] : annonces publiées.

pub mod advertiser;
pub mod config;
pub mod console;
pub mod credentials;
pub mod cryptor;
pub mod downloader;
pub mod file_uploader;
pub mod flash_code;
pub mod ftp;
pub mod greg_uploader;
pub mod http_server;
pub mod message;
pub mod message_uploader;
pub mod netutil;
pub mod partner;
pub mod ring;
pub mod uploader;
pub mod usb_broker;
pub mod usb_challenge;
pub mod usb_drive;
pub mod web_server;
pub mod wifi_code;
pub mod wifi_network;

pub use config::{MemoryStore, PassphraseStore, UserConfigStore};
pub use console::ConsoleServer;
pub use credentials::Credentials;
pub use cryptor::Cryptor;
pub use downloader::Downloader;
pub use file_uploader::FileUploader;
pub use flash_code::{FlashCode, QrImage, QrMatrix, TextFlashCode};
pub use greg_uploader::GregUploader;
pub use http_server::{HttpListener, HttpServer};
pub use message::Message;
pub use message_uploader::{MessageUploader, PeerStatus};
pub use partner::{Partner, PartnerInfo};
pub use ring::{HandshakeResult, PartnerEvent, Ring, RingConfig, RingEvent, Role, SecretKeyProvider};
pub use usb_broker::{UsbBroker, UsbEvent};
pub use usb_drive::UsbDrive;
pub use web_server::WebServer;
pub use wifi_code::WifiCode;
pub use wifi_network::WifiNetwork;
