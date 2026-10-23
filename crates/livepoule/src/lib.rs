//! LivePoule : boîtier de score et d'affichage de piste (port de
//! `sources/LivePoule`).
//!
//! - [`screen`] : écran plein écran GTK4 (`resources/ui/livepoule/LivePoule.ui`) ;
//! - [`model`] : état de l'écran et traitement des messages (sans GTK) ;
//! - [`timer`] : chronomètre ;
//! - [`gpio`], [`button`], [`light`], [`scoring_machine`] : matériel ;
//! - [`wpa`] : configuration Wi-Fi ;
//! - [`wifi_code`] : QR code de connexion de l'arbitre ;
//! - [`network`] : trait [`network::Network`] et serveur HTTP de réception ;
//! - [`web_link`] : scores en direct relus sur le serveur BellePoule Web.

pub mod button;
pub mod gpio;
pub mod light;
pub mod model;
pub mod network;
pub mod scoring_machine;
pub mod screen;
pub mod timer;
pub mod web_link;
pub mod wifi_code;
pub mod wpa;

/// Vrai sur le boîtier (équivalent du `-DWIRING_PI` du C++) : feature
/// `raspberry` activée et cible Linux ARM.
pub const RASPBERRY: bool =
    cfg!(all(feature = "raspberry", target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")));
