//! Chronomètre de la piste (port de `LivePoule/timer.cpp`).
//!
//! Le modèle est indépendant de GTK : l'écran l'interroge pour afficher le
//! texte `MM:SS` et démarre/arrête un rafraîchissement d'une seconde selon
//! la commande renvoyée par [`Timer::set_state`].

use std::time::{SystemTime, UNIX_EPOCH};

/// Heure murale en microsecondes (`g_get_real_time`).
pub fn real_time_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0)
}

/// Action à effectuer par l'affichage après un changement d'état.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerCommand {
    /// Démarrer le rafraîchissement périodique (toutes les secondes).
    StartTicking,
    /// Arrêter le rafraîchissement périodique.
    StopTicking,
    /// Rien à faire.
    None,
}

/// Chronomètre décompté en microsecondes.
#[derive(Debug, Clone, Default)]
pub struct Timer {
    remaining: i64,
    start_time: i64,
    remaining_at_start: i64,
    running: bool,
}

impl Timer {
    /// `Timer::Timer` : chrono à zéro, arrêté.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Timer::Set` : une valeur nulle est ignorée (comportement du C++).
    /// Renvoie `true` si l'affichage doit être rafraîchi.
    pub fn set(&mut self, sec: i32) -> bool {
        if sec != 0 {
            self.remaining = sec as i64 * 1_000_000;
            true
        } else {
            false
        }
    }

    /// `Timer::SetState` : `"RUNNING"` démarre le décompte à l'instant `now`
    /// (µs), tout autre état l'arrête. `None` (champ absent) ne change rien.
    ///
    /// Écart volontaire : si le chrono tourne déjà, le C++ empilait un second
    /// `g_timeout` qui ne pouvait plus jamais être arrêté ; ici on redémarre
    /// simplement le décompte depuis la valeur courante.
    pub fn set_state(&mut self, state: Option<&str>, now: i64) -> TimerCommand {
        match state {
            None => TimerCommand::None,
            Some("RUNNING") => {
                self.start_time = now;
                self.remaining_at_start = self.remaining;
                self.running = true;
                TimerCommand::StartTicking
            }
            Some(_) => {
                if self.running {
                    self.running = false;
                    TimerCommand::StopTicking
                } else {
                    TimerCommand::None
                }
            }
        }
    }

    /// `Timer::OnTimeout` : recalcule le temps restant à l'instant `now` (µs).
    pub fn tick(&mut self, now: i64) {
        self.remaining = self.remaining_at_start - (now - self.start_time);
        if self.remaining < 0 {
            self.remaining = 0;
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Temps restant en microsecondes.
    pub fn remaining_us(&self) -> i64 {
        self.remaining
    }

    /// `Timer::Refresh` : texte `%02d:%02d` (minutes:secondes, troncature).
    pub fn text(&self) -> String {
        let sec = (self.remaining / 1_000_000) as i32;
        format!("{:02}:{:02}", sec / 60, sec % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_zero_is_ignored() {
        let mut t = Timer::new();
        assert!(t.set(180));
        assert_eq!(t.text(), "03:00");
        assert!(!t.set(0));
        assert_eq!(t.text(), "03:00");
    }

    #[test]
    fn countdown_and_stop() {
        let mut t = Timer::new();
        t.set(65);
        assert_eq!(t.set_state(Some("RUNNING"), 1_000_000), TimerCommand::StartTicking);
        assert!(t.is_running());
        t.tick(1_000_000 + 1_500_000);
        // 63,5 s restantes -> troncature à 63 s
        assert_eq!(t.text(), "01:03");
        assert_eq!(t.set_state(Some("STOPPED"), 0), TimerCommand::StopTicking);
        assert!(!t.is_running());
        assert_eq!(t.set_state(Some("STOPPED"), 0), TimerCommand::None);
        assert_eq!(t.set_state(None, 0), TimerCommand::None);
    }

    #[test]
    fn never_negative() {
        let mut t = Timer::new();
        t.set(2);
        t.set_state(Some("RUNNING"), 0);
        t.tick(10_000_000);
        assert_eq!(t.remaining_us(), 0);
        assert_eq!(t.text(), "00:00");
    }

    #[test]
    fn running_restart_uses_current_value() {
        let mut t = Timer::new();
        t.set(100);
        t.set_state(Some("RUNNING"), 0);
        t.tick(10_000_000);
        assert_eq!(t.text(), "01:30");
        // nouveau RUNNING : repart de la valeur courante
        t.set_state(Some("RUNNING"), 10_000_000);
        t.tick(20_000_000);
        assert_eq!(t.text(), "01:20");
    }

    #[test]
    fn set_while_running_is_overridden_by_tick() {
        // Comportement du C++ : Set() pendant le décompte n'affecte que
        // l'affichage immédiat ; le tick suivant repart de la valeur de départ.
        let mut t = Timer::new();
        t.set(60);
        t.set_state(Some("RUNNING"), 0);
        t.set(10);
        assert_eq!(t.text(), "00:10");
        t.tick(1_000_000);
        assert_eq!(t.text(), "00:59");
    }
}
