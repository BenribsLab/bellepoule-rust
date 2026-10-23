//! Écran de piste relié au serveur BellePoule Web : le téléphone de
//! l'arbitre (SmartPoule) y dépose le score et le chrono du match en cours,
//! LivePoule les relit chaque seconde pour son numéro de piste. Aucun réseau
//! commun n'est nécessaire entre le téléphone et l'écran.
//!
//! Adresse de liaison (affichée par BellePoule, menu BellePoule2D ›
//! SmartPoule…) : `https://serveur/live/?c=<club>`. Elle est donnée par
//! `livepoule --live <adresse>` et mémorisée dans `livepoule.ini`.
//!
//! L'état reçu (`GET /api/live?c=<club>&p=<piste>`) est converti en messages
//! `SmartPoule::Score` et `SmartPoule::Timer`, traités par l'écran comme ceux
//! du réseau local.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

use bp_net::Message;
use serde_json::Value;

use crate::network::{SCORE, TIMER};

/// Serveur et club lus dans l'adresse de liaison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAddress {
    pub server: String,
    pub club: String,
}

impl LiveAddress {
    /// `https://serveur[/préfixe]/live/?c=<club>` (ou `…/api/live?c=…`).
    pub fn parse(text: &str) -> Option<LiveAddress> {
        let text = text.trim();
        if !(text.starts_with("http://") || text.starts_with("https://")) {
            return None;
        }
        let (base, query) = text.split_once('?')?;
        let club = query.split('&').find_map(|kv| kv.strip_prefix("c="))?.to_string();
        if club.is_empty() {
            return None;
        }
        let base = base.trim_end_matches('/');
        let server = base.strip_suffix("/api/live").or_else(|| base.strip_suffix("/live")).unwrap_or(base).trim_end_matches('/').to_string();
        Some(LiveAddress { server, club })
    }

    pub fn url(&self) -> String {
        format!("{}/live/?c={}", self.server, self.club)
    }
}

/// Fichier de réglages de LivePoule.
pub fn config_path() -> std::path::PathBuf {
    glib::user_config_dir().join("BellePoule").join("livepoule.ini")
}

/// Réglages mémorisés : adresse de liaison et numéro de piste de l'écran.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub address: Option<LiveAddress>,
    pub piste: Option<u32>,
}

impl Settings {
    pub fn load() -> Settings {
        let text = std::fs::read_to_string(config_path()).unwrap_or_default();
        Settings {
            address: text.lines().find_map(|l| l.strip_prefix("live=")).and_then(LiveAddress::parse),
            piste: text.lines().find_map(|l| l.strip_prefix("piste=")).and_then(|p| p.trim().parse().ok()).filter(|p| *p > 0),
        }
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut text = String::from("[LivePoule]\n");
        if let Some(a) = &self.address {
            text.push_str(&format!("live={}\n", a.url()));
        }
        if let Some(p) = self.piste {
            text.push_str(&format!("piste={p}\n"));
        }
        let _ = std::fs::write(path, text);
    }
}

/// Ce qui a déjà été affiché, pour n'envoyer que les changements.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Shown {
    score: Option<Vec<String>>,
    timer: Option<(bool, u32, i32)>,
    /// Secondes affichées au dernier recalage du chrono en marche.
    timer_seconds: i32,
    active: bool,
}

fn text(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for k in path {
        cur = match cur.get(k) {
            Some(x) => x,
            None => return String::new(),
        };
    }
    match cur {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// Messages à traiter pour passer de `shown` à l'état `state` du serveur.
pub fn messages_for(state: &Value, shown: &mut Shown) -> Vec<Message> {
    let mut out = Vec::new();
    let active = state.get("active").and_then(Value::as_bool).unwrap_or(false);
    if !active {
        if shown.active {
            // Plus de match sur la piste : écran vide, chrono arrêté.
            let mut m = Message::new(SCORE);
            for k in ["competition-name", "red-fencer", "red-score", "green-fencer", "green-score"] {
                m.set_str(k, "");
            }
            out.push(m);
            let mut t = Message::new(TIMER);
            t.set_str("state", "STOPPED");
            out.push(t);
        }
        *shown = Shown::default();
        return out;
    }
    shown.active = true;
    let mut competition = text(state, &["competition"]);
    let bout = text(state, &["bout"]);
    if !bout.is_empty() {
        competition = if competition.is_empty() { bout } else { format!("{competition} · {bout}") };
    }
    let score = vec![
        text(state, &["color"]),
        competition,
        text(state, &["red", "name"]),
        text(state, &["red", "score"]),
        text(state, &["green", "name"]),
        text(state, &["green", "score"]),
    ];
    if shown.score.as_ref() != Some(&score) {
        let mut m = Message::new(SCORE);
        if !score[0].is_empty() {
            m.set_str("competition-color", &score[0]);
        }
        for (k, v) in ["competition-name", "red-fencer", "red-score", "green-fencer", "green-score"].iter().zip(score.iter().skip(1)) {
            m.set_str(k, v);
        }
        out.push(m);
        shown.score = Some(score);
    }
    let running = state.get("running").and_then(Value::as_bool).unwrap_or(false) && !state.get("over").and_then(Value::as_bool).unwrap_or(false);
    let remaining = state.get("remaining_ms").and_then(Value::as_i64).unwrap_or(0).max(0);
    let seconds = ((remaining + 999) / 1000) as i32;
    let period = state.get("period").and_then(Value::as_u64).unwrap_or(1) as u32;
    // Recalage : changement de marche ou de manche, temps changé à l'arrêt,
    // ou dérive de plus de 2 s en marche.
    let key = (running, period, if running { 0 } else { seconds });
    let drift = running && (shown.timer_seconds - seconds).abs() > 2;
    if shown.timer != Some(key) || drift {
        let mut t = Message::new(TIMER);
        // 0 laisse le chrono inchangé : on n'envoie une valeur que si elle compte.
        if seconds > 0 {
            t.set_int("value", seconds);
        }
        t.set_str("state", if running && seconds > 0 { "RUNNING" } else { "STOPPED" });
        out.push(t);
        shown.timer = Some(key);
        shown.timer_seconds = seconds;
    } else if running {
        // Le chrono de l'écran décompte tout seul ; on suit sa valeur attendue.
        shown.timer_seconds = seconds;
    }
    out
}

/// Interrogation du serveur dans un fil séparé.
pub struct WebLink {
    rx: Receiver<Message>,
    stop: Arc<AtomicBool>,
}

impl WebLink {
    /// `strip` : numéro de piste courant de l'écran (modifiable à tout moment).
    pub fn start(address: LiveAddress, strip: Arc<AtomicU32>) -> WebLink {
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let halt = stop.clone();
        std::thread::spawn(move || {
            let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(5)).build();
            let mut shown = Shown::default();
            let mut piste = 0;
            while !halt.load(Ordering::Relaxed) {
                let current = strip.load(Ordering::Relaxed);
                if current != piste {
                    piste = current;
                    shown = Shown { active: true, ..Shown::default() };
                }
                let url = format!("{}/api/live?c={}&p={}", address.server, address.club, piste);
                match agent.get(&url).call().ok().and_then(|r| r.into_json::<Value>().ok()) {
                    Some(state) => {
                        for m in messages_for(&state, &mut shown) {
                            if tx.send(m).is_err() {
                                return;
                            }
                        }
                    }
                    None => log::debug!("LivePoule : serveur injoignable ({url})"),
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        WebLink { rx, stop }
    }

    pub fn try_recv(&self) -> Option<Message> {
        self.rx.try_recv().ok()
    }
}

impl Drop for WebLink {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn address_is_read_from_the_link() {
        let a = LiveAddress::parse("http://efs91.fr:8092/live/?c=abc123").unwrap();
        assert_eq!(a, LiveAddress { server: "http://efs91.fr:8092".into(), club: "abc123".into() });
        assert_eq!(LiveAddress::parse(&a.url()), Some(a));
        assert!(LiveAddress::parse("efs91.fr/live").is_none());
    }

    #[test]
    fn state_becomes_score_and_timer_messages_only_when_it_changes() {
        let mut shown = Shown::default();
        let state = json!({
            "active": true, "competition": "Fleuret M11", "bout": "M3", "color": "#93a24c",
            "red": { "name": "MARTIN Lina", "score": "2" }, "green": { "name": "DURAND Nathan", "score": "1" },
            "remaining_ms": 125_000, "running": true, "period": 1
        });
        let first = messages_for(&state, &mut shown);
        assert_eq!(first.len(), 2);
        assert!(first[0].is(SCORE));
        assert_eq!(first[0].get_str("red-fencer").as_deref(), Some("MARTIN Lina"));
        assert_eq!(first[0].get_str("competition-name").as_deref(), Some("Fleuret M11 · M3"));
        assert!(first[1].is(TIMER));
        assert_eq!(first[1].get_int("value"), 125);
        assert_eq!(first[1].get_str("state").as_deref(), Some("RUNNING"));
        // Une seconde plus tard, rien n'a changé : pas de message.
        let mut later = state.clone();
        later["remaining_ms"] = json!(124_000);
        assert!(messages_for(&later, &mut shown).is_empty());
        // Touche : seul le score part.
        later["green"]["score"] = json!("2");
        let touch = messages_for(&later, &mut shown);
        assert_eq!(touch.len(), 1);
        assert!(touch[0].is(SCORE));
        // Pause : le chrono s'arrête sur la bonne valeur.
        later["running"] = json!(false);
        let pause = messages_for(&later, &mut shown);
        assert_eq!(pause.len(), 1);
        assert_eq!(pause[0].get_str("state").as_deref(), Some("STOPPED"));
        // Fin du match sur la piste : écran vidé.
        let gone = messages_for(&json!({ "active": false }), &mut shown);
        assert_eq!(gone.len(), 2);
        assert_eq!(gone[0].get_str("red-fencer").as_deref(), Some(""));
    }
}
