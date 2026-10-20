//! Détecteur de clés USB de BellePoule (port de `BellePoule/backend`).
//!
//! Processus séparé lancé par BellePoule : chaque branchement/débranchement
//! est écrit sur stderr sous forme d'objet JSON suivi d'une ligne `.`
//! (voir [`events`]).
//!
//! - Linux/macOS : libusb (`rusb`), branchement à chaud, clés de stockage
//!   de masse uniquement ; le programme s'arrête dès qu'un caractère est lu
//!   sur stdin (ou à sa fermeture).
//! - Windows : libusb ne gère pas le branchement à chaud sous Windows ; comme
//!   `main-win.cpp`, on utilise une fenêtre cachée abonnée aux notifications
//!   `WM_DEVICECHANGE` (sous-système « windows », sans console). Le
//!   programme tourne jusqu'à ce que BellePoule le termine.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod events;
// Compilé sur toutes les plateformes (vérifié aussi sous Windows), mais
// utilisé uniquement hors Windows.
#[cfg_attr(windows, allow(dead_code))]
mod libusb;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
fn main() {
    std::process::exit(windows::run());
}

#[cfg(not(windows))]
fn main() {
    libusb::run();
}
