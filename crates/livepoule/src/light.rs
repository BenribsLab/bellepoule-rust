//! Lampes de l'écran (port de `LivePoule/light.cpp`).
//!
//! Une lampe est une zone colorée de l'écran (identifiée par le nom du
//! widget). Elle connaît une couleur par état (`"valid"`, `"non-valid"`,
//! `"on"`, éventuellement `"off"`) et une liste de broches GPIO, chacune
//! associée à un état. Au rafraîchissement, la première broche au niveau bas
//! allume la lampe dans l'état correspondant ; sinon la lampe s'éteint.

use crate::gpio::PinId;

/// Résultat d'un rafraîchissement, à appliquer au widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LightOutput {
    /// Fond de la couleur donnée.
    Color(String),
    /// Fond transparent (`SwitchOff` sans couleur `"off"`).
    Transparent,
    /// Aucun changement (état allumé sans couleur définie).
    Unchanged,
}

/// Une lampe.
#[derive(Debug, Clone)]
pub struct Light {
    widget: String,
    colors: Vec<(String, String)>,
    /// Broches câblées, la plus récente en tête (`g_list_prepend`).
    pins: Vec<(PinId, String)>,
}

impl Light {
    /// `Light::Light (w, ..., state, color, ..., NULL)`.
    pub fn new(widget: &str, colors: &[(&str, &str)]) -> Self {
        Light {
            widget: widget.to_string(),
            colors: colors.iter().map(|(s, c)| (s.to_string(), c.to_string())).collect(),
            pins: Vec::new(),
        }
    }

    /// Nom du widget coloré.
    pub fn widget(&self) -> &str {
        &self.widget
    }

    /// `Light::WireGpioPin` : la broche `pin` au niveau bas allume la lampe
    /// dans l'état `state`.
    pub fn wire_gpio_pin(&mut self, state: &str, pin: PinId) {
        self.pins.insert(0, (pin, state.to_string()));
    }

    /// Broches câblées, dans l'ordre de scrutation.
    pub fn pins(&self) -> impl Iterator<Item = PinId> + '_ {
        self.pins.iter().map(|(p, _)| *p)
    }

    fn color(&self, state: &str) -> Option<&str> {
        self.colors.iter().find(|(s, _)| s == state).map(|(_, c)| c.as_str())
    }

    /// `Light::Refresh` : `voltage` donne la tension de chaque broche.
    pub fn refresh(&self, voltage: impl Fn(PinId) -> u8) -> LightOutput {
        for (pin, state) in &self.pins {
            if voltage(*pin) == 0 {
                return self.switch_on(Some(state));
            }
        }
        self.switch_off()
    }

    /// `Light::SwitchOn`.
    fn switch_on(&self, state: Option<&str>) -> LightOutput {
        match self.color(state.unwrap_or("on")) {
            Some(c) => LightOutput::Color(c.to_string()),
            None => LightOutput::Unchanged,
        }
    }

    /// `Light::SwitchOff`.
    fn switch_off(&self) -> LightOutput {
        match self.color("off") {
            Some(c) => LightOutput::Color(c.to_string()),
            None => LightOutput::Transparent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_low_pin_wins() {
        let mut l = Light::new("red_hit_box", &[("valid", "#b01313"), ("non-valid", "#eeeeee")]);
        l.wire_gpio_pin("valid", 1);
        l.wire_gpio_pin("non-valid", 0);
        // aucune broche basse -> éteinte (transparente)
        assert_eq!(l.refresh(|_| 1), LightOutput::Transparent);
        assert_eq!(l.refresh(|p| u8::from(p != 1)), LightOutput::Color("#b01313".into()));
        assert_eq!(l.refresh(|p| u8::from(p != 0)), LightOutput::Color("#eeeeee".into()));
        // les deux basses : la dernière câblée (non-valid) est scrutée en premier
        assert_eq!(l.refresh(|_| 0), LightOutput::Color("#eeeeee".into()));
    }

    #[test]
    fn off_color_and_missing_state() {
        let mut l = Light::new("x", &[("off", "#000000")]);
        l.wire_gpio_pin("on", 3);
        assert_eq!(l.refresh(|_| 1), LightOutput::Color("#000000".into()));
        assert_eq!(l.refresh(|_| 0), LightOutput::Unchanged);
    }
}
