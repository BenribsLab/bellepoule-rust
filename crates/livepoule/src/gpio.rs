//! Broches GPIO (port de `LivePoule/gpio.cpp`).
//!
//! Le C++ utilisait WiringPi (`-DWIRING_PI`) : numérotation WiringPi 0 à 8,
//! entrée avec rappel au niveau haut (`gpio -g mode N in/up`) et interruption
//! sur les deux fronts (`wiringPiISR (INT_EDGE_BOTH)`). Chaque front est
//! remonté à la boucle principale (`g_idle_add`).
//!
//! Ici, deux implémentations de [`GpioBackend`] :
//! - [`SimulatedGpio`] (par défaut, toutes plateformes) : les tensions sont
//!   pilotées au clavier par l'écran (touches `0` à `8` : appui = niveau bas,
//!   relâchement = niveau haut) ;
//! - `RaspberryGpio` (feature `raspberry`, cible Linux ARM) : `rppal`.
//!
//! Les fronts sont transmis par un canal `mpsc` que l'écran relève
//! périodiquement depuis la boucle GTK (équivalent de `g_idle_add`).

use std::sync::mpsc::{channel, Receiver, Sender};

/// Numéro de broche WiringPi.
pub type PinId = u8;

/// Nombre de broches câblées (`Gpio::WIRED_PINS`).
pub const WIRED_PINS: u8 = 9;

/// Conversion WiringPi -> BCM (`wpiPinToGpio`, cartes révision 2 et suivantes).
pub fn wpi_pin_to_bcm(pin: PinId) -> Option<u8> {
    const TABLE: [u8; WIRED_PINS as usize] = [17, 18, 27, 22, 23, 24, 25, 4, 2];
    TABLE.get(pin as usize).copied()
}

/// Accès matériel (ou simulé) aux broches.
pub trait GpioBackend {
    /// Nom du pilote, pour les traces.
    fn name(&self) -> &'static str;

    /// Configure `pin` en entrée avec rappel au niveau haut et signale chaque
    /// front (montant ou descendant) en envoyant `pin` sur `events`.
    fn setup_pin(&mut self, pin: PinId, events: Sender<PinId>);

    /// `Gpio::GetVoltageState` : 0 (niveau bas) ou 1 (niveau haut).
    fn voltage(&self, pin: PinId) -> u8;

    /// Force la tension d'une broche (GPIO simulé uniquement).
    /// Renvoie `true` si la simulation est prise en charge.
    fn simulate(&mut self, _pin: PinId, _voltage: u8) -> bool {
        false
    }
}

/// GPIO simulé : toutes les broches sont au repos (niveau haut, comme avec
/// le rappel au niveau haut du matériel) tant qu'aucune touche n'est enfoncée.
#[derive(Default)]
pub struct SimulatedGpio {
    voltages: [u8; WIRED_PINS as usize],
    senders: Vec<(PinId, Sender<PinId>)>,
}

impl SimulatedGpio {
    pub fn new() -> Self {
        SimulatedGpio { voltages: [1; WIRED_PINS as usize], senders: Vec::new() }
    }
}

impl GpioBackend for SimulatedGpio {
    fn name(&self) -> &'static str {
        "simulé (clavier)"
    }

    fn setup_pin(&mut self, pin: PinId, events: Sender<PinId>) {
        self.senders.push((pin, events));
    }

    fn voltage(&self, pin: PinId) -> u8 {
        self.voltages.get(pin as usize).copied().unwrap_or(1)
    }

    fn simulate(&mut self, pin: PinId, voltage: u8) -> bool {
        if let Some(v) = self.voltages.get_mut(pin as usize) {
            let voltage = u8::from(voltage != 0);
            if *v != voltage {
                *v = voltage;
                for (p, tx) in &self.senders {
                    if *p == pin {
                        let _ = tx.send(pin);
                    }
                }
            }
        }
        true
    }
}

#[cfg(all(feature = "raspberry", target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
mod raspberry {
    use super::{wpi_pin_to_bcm, GpioBackend, PinId};
    use rppal::gpio::{Gpio, InputPin, Trigger};
    use std::collections::HashMap;
    use std::sync::mpsc::Sender;

    /// GPIO matériel du Raspberry Pi.
    pub struct RaspberryGpio {
        gpio: Option<Gpio>,
        pins: HashMap<PinId, InputPin>,
    }

    impl RaspberryGpio {
        /// `Gpio::Init` : si la carte n'est pas reconnue, toutes les lectures
        /// renvoient 1 (`_raspberry_not_supported`).
        pub fn new() -> Self {
            let gpio = match Gpio::new() {
                Ok(g) => Some(g),
                Err(e) => {
                    log::warn!("GPIO non pris en charge : {e}");
                    None
                }
            };
            RaspberryGpio { gpio, pins: HashMap::new() }
        }
    }

    impl GpioBackend for RaspberryGpio {
        fn name(&self) -> &'static str {
            "Raspberry Pi (rppal)"
        }

        fn setup_pin(&mut self, pin: PinId, events: Sender<PinId>) {
            let (Some(gpio), Some(bcm)) = (self.gpio.as_ref(), wpi_pin_to_bcm(pin)) else {
                return;
            };
            match gpio.get(bcm) {
                Ok(p) => {
                    // Entrée avec rappel au niveau haut (gpio -g mode N in / up)
                    let mut input = p.into_input_pullup();
                    if let Err(e) = input.set_async_interrupt(Trigger::Both, None, move |_| {
                        let _ = events.send(pin);
                    }) {
                        log::warn!("OnSignal[{bcm}] registration error: {e}");
                    }
                    self.pins.insert(pin, input);
                }
                Err(e) => log::warn!("Gpio BCM {bcm}: {e}"),
            }
        }

        fn voltage(&self, pin: PinId) -> u8 {
            match self.pins.get(&pin) {
                Some(p) if self.gpio.is_some() => u8::from(p.is_high()),
                _ => 1,
            }
        }
    }
}

/// Pilote GPIO par défaut de la plateforme.
pub fn default_backend() -> Box<dyn GpioBackend> {
    #[cfg(all(feature = "raspberry", target_os = "linux", any(target_arch = "arm", target_arch = "aarch64")))]
    {
        Box::new(raspberry::RaspberryGpio::new())
    }
    #[cfg(not(all(feature = "raspberry", target_os = "linux", any(target_arch = "arm", target_arch = "aarch64"))))]
    {
        Box::new(SimulatedGpio::new())
    }
}

/// Ensemble des broches utilisées par LivePoule (équivalent du tableau
/// statique `Gpio::_wired_pins` et de `Gpio::Init`).
pub struct GpioHub {
    backend: Box<dyn GpioBackend>,
    tx: Sender<PinId>,
    rx: Receiver<PinId>,
    registered: Vec<PinId>,
}

impl GpioHub {
    pub fn new(backend: Box<dyn GpioBackend>) -> Self {
        let (tx, rx) = channel();
        log::info!("GPIO : pilote {}", backend.name());
        GpioHub { backend, tx, rx, registered: Vec::new() }
    }

    /// `Gpio::Gpio` : enregistre une broche. Renvoie `false` si elle n'est
    /// pas câblée.
    pub fn register(&mut self, pin: PinId) -> bool {
        if pin >= WIRED_PINS {
            log::warn!("Gpio pin #{pin} not wired");
            return false;
        }
        if !self.registered.contains(&pin) {
            self.registered.push(pin);
            self.backend.setup_pin(pin, self.tx.clone());
        }
        true
    }

    pub fn is_registered(&self, pin: PinId) -> bool {
        self.registered.contains(&pin)
    }

    pub fn voltage(&self, pin: PinId) -> u8 {
        self.backend.voltage(pin)
    }

    /// Simulation clavier ; renvoie `true` si le pilote la gère.
    pub fn simulate(&mut self, pin: PinId, voltage: u8) -> bool {
        self.backend.simulate(pin, voltage)
    }

    /// Fronts survenus depuis le dernier appel.
    pub fn pending_events(&self) -> Vec<PinId> {
        self.rx.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_edges() {
        let mut hub = GpioHub::new(Box::new(SimulatedGpio::new()));
        assert!(hub.register(6));
        assert!(!hub.register(9));
        assert_eq!(hub.voltage(6), 1);
        assert!(hub.simulate(6, 0));
        assert_eq!(hub.voltage(6), 0);
        // même tension : pas de nouveau front
        hub.simulate(6, 0);
        // broche non enregistrée : tension changée mais aucun front signalé
        hub.simulate(2, 0);
        assert_eq!(hub.pending_events(), vec![6]);
        hub.simulate(6, 1);
        assert_eq!(hub.pending_events(), vec![6]);
        assert!(hub.pending_events().is_empty());
    }

    #[test]
    fn wiring_pi_mapping() {
        assert_eq!(wpi_pin_to_bcm(0), Some(17));
        assert_eq!(wpi_pin_to_bcm(8), Some(2));
        assert_eq!(wpi_pin_to_bcm(9), None);
    }
}
