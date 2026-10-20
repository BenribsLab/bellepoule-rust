//! Appareils de score (port de `LivePoule/scoring_machine.cpp` et
//! `sg_machine.cpp`).
//!
//! Un appareil de score est relié au boîtier par des broches GPIO : il
//! « câble » chaque lampe de l'écran sur ses sorties. Le seul appareil du C++
//! est le [`SgMachine`] (sorties tout-ou-rien câblées directement sur le GPIO ;
//! le C++ ne comporte aucun protocole série).
//!
//! L'état instantané des broches est représenté par une [`PinFrame`] (un bit
//! par broche WiringPi) ; [`LightBoard::decode`] traduit une trame en état
//! des quatre lampes.

use crate::gpio::{PinId, WIRED_PINS};
use crate::light::{Light, LightOutput};

/// Lampe de touche du tireur rouge (`red_hit_box`).
pub const RED_HIT: &str = "red_hit_box";
/// Lampe « faute/arme » du tireur rouge (`red_failure_light`).
pub const RED_FAILURE: &str = "red_failure_light";
/// Lampe de touche du tireur vert (`green_hit_box`).
pub const GREEN_HIT: &str = "green_hit_box";
/// Lampe « faute/arme » du tireur vert (`green_failure_light`).
pub const GREEN_FAILURE: &str = "green_failure_light";

/// Appareil de score (`ScoringMachine`).
pub trait ScoringMachine {
    /// Nom de la classe (traces).
    fn class_name(&self) -> &'static str;

    /// `Wire` : câble les broches de l'appareil sur la lampe `key`.
    fn wire(&self, key: &str, light: &mut Light);

    /// `ConnectToLights` : câble toutes les lampes.
    fn connect_to_lights(&self, board: &mut LightBoard) {
        for (key, light) in board.lights.iter_mut() {
            self.wire(key, light);
        }
    }
}

/// Appareil Saint-George (SG) : sorties câblées sur les broches WiringPi 0 à 5.
///
/// | Lampe                 | État        | Broche |
/// |-----------------------|-------------|--------|
/// | `red_hit_box`         | `valid`     | 1      |
/// | `red_hit_box`         | `non-valid` | 0      |
/// | `red_failure_light`   | `on`        | 2      |
/// | `green_hit_box`       | `valid`     | 4      |
/// | `green_hit_box`       | `non-valid` | 3      |
/// | `green_failure_light` | `on`        | 5      |
#[derive(Debug, Default, Clone, Copy)]
pub struct SgMachine;

impl ScoringMachine for SgMachine {
    fn class_name(&self) -> &'static str {
        "SgMachine"
    }

    fn wire(&self, key: &str, light: &mut Light) {
        match key {
            RED_HIT => {
                light.wire_gpio_pin("valid", 1);
                light.wire_gpio_pin("non-valid", 0);
            }
            RED_FAILURE => light.wire_gpio_pin("on", 2),
            GREEN_HIT => {
                light.wire_gpio_pin("valid", 4);
                light.wire_gpio_pin("non-valid", 3);
            }
            GREEN_FAILURE => light.wire_gpio_pin("on", 5),
            _ => {}
        }
    }
}

/// Tensions instantanées des broches : bit `n` = tension de la broche `n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinFrame(pub u16);

impl PinFrame {
    /// Toutes les broches au repos (niveau haut).
    pub const IDLE: PinFrame = PinFrame((1 << WIRED_PINS) - 1);

    /// Trame lue sur un pilote.
    pub fn capture(voltage: impl Fn(PinId) -> u8) -> Self {
        let mut bits = 0u16;
        for pin in 0..WIRED_PINS {
            if voltage(pin) != 0 {
                bits |= 1 << pin;
            }
        }
        PinFrame(bits)
    }

    /// Trame où seules les broches listées sont au niveau bas.
    pub fn with_low(pins: &[PinId]) -> Self {
        let mut bits = Self::IDLE.0;
        for p in pins {
            bits &= !(1 << p);
        }
        PinFrame(bits)
    }

    pub fn voltage(&self, pin: PinId) -> u8 {
        u8::from(pin < 16 && self.0 & (1 << pin) != 0)
    }
}

/// Les quatre lampes de l'écran (`Screen::_lights`).
#[derive(Debug, Clone)]
pub struct LightBoard {
    lights: Vec<(String, Light)>,
}

impl Default for LightBoard {
    fn default() -> Self {
        Self::new()
    }
}

impl LightBoard {
    /// Lampes et couleurs de `Screen::Screen`.
    pub fn new() -> Self {
        let lights = vec![
            (RED_HIT, Light::new(RED_HIT, &[("valid", "#b01313"), ("non-valid", "#eeeeee")])),
            (RED_FAILURE, Light::new(RED_FAILURE, &[("on", "#ecdf11")])),
            (GREEN_HIT, Light::new(GREEN_HIT, &[("valid", "#13b013"), ("non-valid", "#eeeeee")])),
            (GREEN_FAILURE, Light::new(GREEN_FAILURE, &[("on", "#ecdf11")])),
        ];
        LightBoard { lights: lights.into_iter().map(|(k, l)| (k.to_string(), l)).collect() }
    }

    /// Toutes les broches câblées sur les lampes.
    pub fn wired_pins(&self) -> Vec<PinId> {
        let mut pins: Vec<PinId> = self.lights.iter().flat_map(|(_, l)| l.pins()).collect();
        pins.sort_unstable();
        pins.dedup();
        pins
    }

    /// `Screen::OnLightEvent` : état de chaque lampe (nom du widget, sortie).
    pub fn refresh(&self, voltage: impl Fn(PinId) -> u8) -> Vec<(String, LightOutput)> {
        self.lights.iter().map(|(_, l)| (l.widget().to_string(), l.refresh(&voltage))).collect()
    }

    /// Analyse d'une trame de l'appareil de score.
    pub fn decode(&self, frame: PinFrame) -> Vec<(String, LightOutput)> {
        self.refresh(|p| frame.voltage(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> LightBoard {
        let mut b = LightBoard::new();
        SgMachine.connect_to_lights(&mut b);
        b
    }

    fn state(out: &[(String, LightOutput)], key: &str) -> LightOutput {
        out.iter().find(|(k, _)| k == key).unwrap().1.clone()
    }

    #[test]
    fn sg_wiring() {
        assert_eq!(board().wired_pins(), vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn idle_frame_switches_everything_off() {
        let out = board().decode(PinFrame::IDLE);
        assert!(out.iter().all(|(_, o)| *o == LightOutput::Transparent));
    }

    #[test]
    fn red_valid_hit() {
        let out = board().decode(PinFrame::with_low(&[1]));
        assert_eq!(state(&out, RED_HIT), LightOutput::Color("#b01313".into()));
        assert_eq!(state(&out, GREEN_HIT), LightOutput::Transparent);
    }

    #[test]
    fn green_non_valid_and_failures() {
        let out = board().decode(PinFrame::with_low(&[3, 2, 5]));
        assert_eq!(state(&out, GREEN_HIT), LightOutput::Color("#eeeeee".into()));
        assert_eq!(state(&out, RED_FAILURE), LightOutput::Color("#ecdf11".into()));
        assert_eq!(state(&out, GREEN_FAILURE), LightOutput::Color("#ecdf11".into()));
        assert_eq!(state(&out, RED_HIT), LightOutput::Transparent);
    }

    #[test]
    fn double_hit_green_valid_red_non_valid() {
        let out = board().decode(PinFrame::with_low(&[0, 4]));
        assert_eq!(state(&out, RED_HIT), LightOutput::Color("#eeeeee".into()));
        assert_eq!(state(&out, GREEN_HIT), LightOutput::Color("#13b013".into()));
    }

    #[test]
    fn frame_capture_roundtrip() {
        let f = PinFrame::with_low(&[0, 7]);
        assert_eq!(PinFrame::capture(|p| f.voltage(p)), f);
        assert_eq!(f.voltage(0), 0);
        assert_eq!(f.voltage(1), 1);
        assert_eq!(f.voltage(15), 0);
    }
}
