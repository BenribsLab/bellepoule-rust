//! Annonceur (port de la partie générique de `common/network/advertiser.cpp`).
//!
//! Dans le C++, `Advertiser` est la base des publications Facebook/Twitter
//! (non portées). On conserve ce qui sert à la publication : titre de la
//! compétition (préfixé de l'escrimeur 🤺), lien public, état
//! activé/désactivé, composition du texte publié (`titre\névénement\nlien`)
//! et le mécanisme `Feeder` qui fournit l'annonce d'un événement. Le canal de
//! publication proprement dit est un [`AdvertiserBackend`].

/// Fournisseur d'annonce (`Advertiser::Feeder`).
pub trait Feeder {
    fn announce(&self) -> String;
}

/// Canal de publication (`PublishMessage`).
pub trait AdvertiserBackend: Send {
    fn publish_message(&mut self, message: &str);
}

/// États de l'annonceur (`Advertiser::State`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvertiserState {
    Warmup,
    Off,
    WaitingForToken,
    On,
}

pub struct Advertiser {
    name: String,
    title: Option<String>,
    link: Option<String>,
    state: AdvertiserState,
    backend: Option<Box<dyn AdvertiserBackend>>,
}

impl Advertiser {
    pub fn new(name: &str, backend: Option<Box<dyn AdvertiserBackend>>) -> Self {
        Advertiser { name: name.to_string(), title: None, link: None, state: AdvertiserState::Warmup, backend }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn state(&self) -> AdvertiserState {
        self.state
    }

    /// `OnToggled`.
    pub fn on_toggled(&mut self, on: bool) {
        self.state = if on { AdvertiserState::On } else { AdvertiserState::Off };
    }

    /// `SetTitle` : titre préfixé de 🤺.
    pub fn set_title(&mut self, title: Option<&str>) {
        self.title = title.map(|t| format!("\u{1F93A} {t}"));
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// `SetLink`.
    pub fn set_link(&mut self, link: Option<&str>) {
        self.link = link.map(str::to_string);
    }

    pub fn link(&self) -> Option<&str> {
        self.link.as_deref()
    }

    /// Texte publié pour un événement (sans condition d'état).
    pub fn compose(&self, event: &str) -> Option<String> {
        let title = self.title.as_deref()?;
        Some(match &self.link {
            Some(link) => format!("{title}\n{event}\n{link}"),
            None => format!("{title}\n{event}"),
        })
    }

    /// `Publish` : publie si l'annonceur est actif et titré.
    pub fn publish(&mut self, event: &str) -> Option<String> {
        if self.state != AdvertiserState::On {
            return None;
        }
        let message = self.compose(event)?;
        if let Some(b) = &mut self.backend {
            b.publish_message(&message);
        }
        Some(message)
    }

    /// `TweetFeeder`.
    pub fn feed(&mut self, feeder: &dyn Feeder) -> Option<String> {
        let event = feeder.announce();
        self.publish(&event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Sink(Arc<Mutex<Vec<String>>>);
    impl AdvertiserBackend for Sink {
        fn publish_message(&mut self, message: &str) {
            self.0.lock().unwrap().push(message.to_string());
        }
    }

    #[test]
    fn publish() {
        let out = Arc::new(Mutex::new(Vec::new()));
        let mut a = Advertiser::new("Web", Some(Box::new(Sink(out.clone()))));
        a.set_title(Some("Coupe"));
        assert_eq!(a.publish("Poules"), None);
        a.on_toggled(true);
        a.set_link(Some("https://x.org"));
        assert_eq!(a.publish("Poules").as_deref(), Some("\u{1F93A} Coupe\nPoules\nhttps://x.org"));
        assert_eq!(out.lock().unwrap().len(), 1);
    }
}
