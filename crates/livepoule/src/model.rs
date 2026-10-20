//! État de l'écran de piste, indépendant de GTK (logique de
//! `LivePoule/screen.cpp`). L'affichage relit ce modèle après chaque action.

use bp_net::Message;

use crate::network::{REFEREE_HANDSHAKE, SCORE, TIMER};
use crate::timer::{Timer, TimerCommand};

/// Couleur du bandeau au repos (`ResetDisplay`).
pub const DEFAULT_COLOR: &str = "#93a24c";

/// Durée d'un match de poule annoncée à la prise en main (3 minutes).
pub const POOL_BOUT_DURATION: i32 = 3 * 60;

/// Couleur d'un tireur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Red,
    Green,
}

impl Side {
    pub fn name(self) -> &'static str {
        match self {
            Side::Red => "red",
            Side::Green => "green",
        }
    }
}

/// Résultat du traitement d'un message (`Screen::OnMessage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageOutcome {
    /// Valeur de retour du C++ (message consommé).
    pub handled: bool,
    /// Commande pour le rafraîchissement du chronomètre.
    pub timer: TimerCommand,
}

/// Contenu de l'écran.
#[derive(Debug, Clone)]
pub struct ScreenModel {
    pub strip_id: u32,
    /// Couleur de fond du bandeau (`title_box`).
    pub color: String,
    pub title: String,
    pub red_fencer: String,
    pub red_score: String,
    pub green_fencer: String,
    pub green_score: String,
    pub timer: Timer,
    /// QR code de connexion visible.
    pub code_visible: bool,
    /// Configuration Wi-Fi en cours (roue d'attente visible).
    pub spinner_visible: bool,
}

impl Default for ScreenModel {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenModel {
    /// Valeurs initiales de l'interface, puis `ResetDisplay`.
    pub fn new() -> Self {
        let mut m = ScreenModel {
            strip_id: 1,
            color: DEFAULT_COLOR.to_string(),
            title: "Title".to_string(),
            red_fencer: "Red".to_string(),
            red_score: "0".to_string(),
            green_fencer: "Green".to_string(),
            green_score: "0".to_string(),
            timer: Timer::new(),
            code_visible: false,
            spinner_visible: false,
        };
        m.reset_display();
        m
    }

    /// `Screen::ResetDisplay` (le chrono n'est pas remis à zéro : `Set (0)`
    /// est sans effet dans le C++).
    pub fn reset_display(&mut self) {
        self.set_color(Some(DEFAULT_COLOR));
        self.set_title(Some(""));
        self.timer.set(0);
        self.set_fencer(Side::Red, "fencer", Some(""));
        self.set_fencer(Side::Red, "score", Some(""));
        self.set_fencer(Side::Green, "fencer", Some(""));
        self.set_fencer(Side::Green, "score", Some(""));
    }

    /// Texte du numéro de piste (`RefreshStripId`).
    pub fn strip_text(&self) -> String {
        format!("Piste {:02}", self.strip_id)
    }

    /// `Screen::ChangeStripId` : jamais en dessous de 1.
    pub fn change_strip_id(&mut self, step: i32) {
        if step < 0 && self.strip_id <= step.unsigned_abs() {
            self.strip_id = 1;
        } else {
            self.strip_id = self.strip_id.saturating_add_signed(step);
        }
    }

    pub fn set_color(&mut self, color: Option<&str>) {
        if let Some(c) = color {
            self.color = c.to_string();
        }
    }

    pub fn set_title(&mut self, title: Option<&str>) {
        if let Some(t) = title {
            self.title = t.to_string();
        }
    }

    /// `Screen::SetFencer` : `data` vaut `"fencer"` ou `"score"`.
    pub fn set_fencer(&mut self, side: Side, data: &str, name: Option<&str>) {
        let Some(name) = name else { return };
        let field = match (side, data) {
            (Side::Red, "fencer") => &mut self.red_fencer,
            (Side::Red, "score") => &mut self.red_score,
            (Side::Green, "fencer") => &mut self.green_fencer,
            (Side::Green, "score") => &mut self.green_score,
            _ => return,
        };
        *field = name.to_string();
    }

    /// `Screen::SetScore`.
    fn set_score(&mut self, side: Side, message: &Message) {
        let name = message.get_str(&format!("{}-fencer", side.name()));
        self.set_fencer(side, "fencer", name.as_deref());
        let score = message.get_str(&format!("{}-score", side.name()));
        self.set_fencer(side, "score", score.as_deref());
    }

    /// `Screen::OnMessage` ; `now` est l'heure courante en microseconds.
    pub fn on_message(&mut self, message: &Message, now: i64) -> MessageOutcome {
        let mut outcome = MessageOutcome { handled: false, timer: TimerCommand::None };
        if message.is(REFEREE_HANDSHAKE) {
            self.code_visible = false;
            self.reset_display();
            self.timer.set(POOL_BOUT_DURATION);
            self.set_fencer(Side::Red, "score", Some("0"));
            self.set_fencer(Side::Green, "score", Some("0"));
            outcome.handled = true;
        } else if message.is(SCORE) {
            // SetCompetition
            self.set_color(message.get_str("competition-color").as_deref());
            self.set_title(message.get_str("competition-name").as_deref());
            self.set_score(Side::Red, message);
            self.set_score(Side::Green, message);
        } else if message.is(TIMER) {
            self.timer.set(message.get_int("value"));
            outcome.timer = self.timer.set_state(message.get_str("state").as_deref(), now);
            outcome.handled = true;
        }
        outcome
    }

    /// `Screen::ToggleWifiCode` : renvoie `true` s'il faut lancer la
    /// configuration Wi-Fi (la roue d'attente vient d'apparaître).
    pub fn toggle_wifi_code(&mut self) -> bool {
        if self.code_visible {
            self.code_visible = false;
            false
        } else if !self.spinner_visible {
            self.spinner_visible = true;
            true
        } else {
            false
        }
    }

    /// `Screen::OnNetworkConfigured` (partie modèle ; la clé est régénérée
    /// et le QR code redessiné par l'affichage).
    pub fn network_configured(&mut self) {
        self.spinner_visible = false;
        self.code_visible = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state() {
        let m = ScreenModel::new();
        assert_eq!(m.strip_text(), "Piste 01");
        assert_eq!(m.color, DEFAULT_COLOR);
        assert_eq!(m.title, "");
        assert_eq!(m.red_score, "");
        assert_eq!(m.timer.text(), "00:00");
    }

    #[test]
    fn strip_id_bounds() {
        let mut m = ScreenModel::new();
        m.change_strip_id(-1);
        assert_eq!(m.strip_id, 1);
        m.change_strip_id(1);
        m.change_strip_id(1);
        assert_eq!(m.strip_text(), "Piste 03");
        m.change_strip_id(-1);
        assert_eq!(m.strip_id, 2);
        m.change_strip_id(-5);
        assert_eq!(m.strip_id, 1);
        for _ in 0..99 {
            m.change_strip_id(1);
        }
        assert_eq!(m.strip_text(), "Piste 100");
    }

    #[test]
    fn handshake_score_timer_sequence() {
        let mut m = ScreenModel::new();
        m.code_visible = true;
        let out = m.on_message(&Message::new(REFEREE_HANDSHAKE), 0);
        assert!(out.handled);
        assert!(!m.code_visible);
        assert_eq!(m.timer.text(), "03:00");
        assert_eq!((m.red_score.as_str(), m.green_score.as_str()), ("0", "0"));

        let mut s = Message::new(SCORE);
        s.set_str("competition-color", "#ff0000");
        s.set_str("competition-name", "Fleuret H");
        s.set_str("red-fencer", "DUPONT");
        s.set_str("red-score", "3");
        s.set_str("green-score", "1");
        let out = m.on_message(&s, 0);
        assert!(!out.handled);
        assert_eq!(m.color, "#ff0000");
        assert_eq!(m.title, "Fleuret H");
        assert_eq!(m.red_fencer, "DUPONT");
        assert_eq!(m.red_score, "3");
        // champ absent : inchangé
        assert_eq!(m.green_fencer, "");
        assert_eq!(m.green_score, "1");

        let mut t = Message::new(TIMER);
        t.set_int("value", 120);
        t.set_str("state", "RUNNING");
        let out = m.on_message(&t, 1_000_000);
        assert!(out.handled);
        assert_eq!(out.timer, TimerCommand::StartTicking);
        m.timer.tick(31_000_000);
        assert_eq!(m.timer.text(), "01:30");

        let mut t = Message::new(TIMER);
        t.set_int("value", 90);
        t.set_str("state", "STOPPED");
        assert_eq!(m.on_message(&t, 0).timer, TimerCommand::StopTicking);

        assert!(!m.on_message(&Message::new("Autre"), 0).handled);
    }

    #[test]
    fn wifi_toggle() {
        let mut m = ScreenModel::new();
        assert!(m.toggle_wifi_code());
        // configuration en cours : pas de second lancement
        assert!(!m.toggle_wifi_code());
        m.network_configured();
        assert!(m.code_visible && !m.spinner_visible);
        assert!(!m.toggle_wifi_code());
        assert!(!m.code_visible);
    }
}
