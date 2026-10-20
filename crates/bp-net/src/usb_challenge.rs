//! Défi USB (port de `common/network/usb_challenge.cpp`).
//!
//! Un poste dont l'authentification a échoué propose un défi chiffré avec le
//! numéro de série d'une clé USB qu'on lui a présentée. Les défis reçus sont
//! mémorisés ; quand cette même clé est branchée sur le poste qui détient la
//! bonne phrase secrète, le défi se déchiffre et le partenaire est déclaré
//! de confiance.

use crate::cryptor::Cryptor;

/// Défi en attente (`UsbChallenge`) pour un partenaire de type `P`.
#[derive(Debug, Clone)]
pub struct UsbChallenge<P> {
    pub partner: P,
    pub cypher: String,
    pub iv: String,
}

/// Ensemble des défis en attente (liste statique `_challenges` du C++).
#[derive(Debug, Clone)]
pub struct UsbChallenges<P> {
    challenges: Vec<UsbChallenge<P>>,
}

impl<P> Default for UsbChallenges<P> {
    fn default() -> Self {
        UsbChallenges { challenges: Vec::new() }
    }
}

impl<P> UsbChallenges<P> {
    /// `Monitor` : mémorise un défi.
    pub fn monitor(&mut self, partner: P, cypher: &str, iv: &str) {
        self.challenges.insert(0, UsbChallenge { partner, cypher: cypher.to_string(), iv: iv.to_string() });
    }

    /// `CheckKey` : renvoie (et retire) les partenaires dont le défi se
    /// déchiffre en `secret` avec `key` (comparaison insensible à la casse).
    pub fn check_key(&mut self, key: &str, secret: &str) -> Vec<P> {
        let mut trusted = Vec::new();
        let mut kept = Vec::new();
        for c in self.challenges.drain(..) {
            let ok = Cryptor::decrypt(&c.cypher, &c.iv, key).is_some_and(|m| m.eq_ignore_ascii_case(secret));
            if ok {
                trusted.push(c.partner);
            } else {
                kept.push(c);
            }
        }
        self.challenges = kept;
        trusted
    }

    pub fn len(&self) -> usize {
        self.challenges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.challenges.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check() {
        let mut c = UsbChallenges::default();
        let (cy, iv) = Cryptor::new().encrypt("tagada soinsoin", "SERIAL-1");
        c.monitor(1u32, &cy, &iv);
        c.monitor(2u32, "AAAA", &iv);
        assert!(c.check_key("SERIAL-2", "tagada soinsoin").is_empty());
        assert_eq!(c.check_key("SERIAL-1", "TAGADA SOINSOIN"), vec![1]);
        assert_eq!(c.len(), 1);
    }
}
