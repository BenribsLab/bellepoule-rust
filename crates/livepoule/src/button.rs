//! Boutons poussoirs (port de `LivePoule/button.cpp`).
//!
//! Un bouton est une broche GPIO dont les fronts sont filtrés (anti-rebond) :
//! le premier front lance une temporisation de 50 ms, les fronts suivants
//! sont ignorés jusqu'à son échéance ; à l'échéance, l'action du bouton est
//! exécutée si la broche est alors au niveau bas (bouton enfoncé).

use std::time::Duration;

use crate::gpio::PinId;

/// Durée de l'anti-rebond (`g_timeout_add (50, ...)`).
pub const DEBOUNCE_DELAY: Duration = Duration::from_millis(50);

/// Action associée à un bouton de l'écran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    /// Affiche/masque le QR code de connexion (`OnQrCodeButton`, broche 6).
    QrCode,
    /// Piste suivante (`OnStripPlusPin`, broche 7).
    StripPlus,
    /// Piste précédente (`OnStripMinusPin`, broche 8).
    StripMinus,
}

/// Un bouton et son anti-rebond.
#[derive(Debug, Clone)]
pub struct Button {
    pin: PinId,
    action: ButtonAction,
    debounce_started: bool,
}

impl Button {
    pub fn new(pin: PinId, action: ButtonAction) -> Self {
        Button { pin, action, debounce_started: false }
    }

    pub fn pin(&self) -> PinId {
        self.pin
    }

    pub fn action(&self) -> ButtonAction {
        self.action
    }

    /// `OnButtonEvent` : renvoie `true` s'il faut lancer la temporisation.
    pub fn on_event(&mut self) -> bool {
        if self.debounce_started {
            false
        } else {
            self.debounce_started = true;
            true
        }
    }

    /// `OnDebounceStop` : fin de temporisation ; renvoie l'action à exécuter
    /// si la broche est au niveau bas (`GetVoltageState () == 0`).
    pub fn on_debounce_stop(&mut self, voltage: u8) -> Option<ButtonAction> {
        self.debounce_started = false;
        (voltage == 0).then_some(self.action)
    }
}

/// Boutons de l'écran (`Screen::Screen`).
pub fn screen_buttons() -> Vec<Button> {
    vec![
        Button::new(6, ButtonAction::QrCode),
        Button::new(7, ButtonAction::StripPlus),
        Button::new(8, ButtonAction::StripMinus),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce() {
        let mut b = Button::new(7, ButtonAction::StripPlus);
        assert!(b.on_event());
        // rebonds ignorés
        assert!(!b.on_event());
        assert!(!b.on_event());
        assert_eq!(b.on_debounce_stop(0), Some(ButtonAction::StripPlus));
        // nouveau cycle : relâché à l'échéance -> pas d'action
        assert!(b.on_event());
        assert_eq!(b.on_debounce_stop(1), None);
    }
}
