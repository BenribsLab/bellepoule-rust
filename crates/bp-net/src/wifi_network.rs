//! Réseau Wi-Fi annoncé aux smartphones (port de `common/network/wifi_network.cpp`).

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WifiNetwork {
    ssid: Option<String>,
    passphrase: Option<String>,
    encryption: Option<String>,
}

impl WifiNetwork {
    pub fn new() -> Self {
        Self::default()
    }

    /// `IsValid` : un SSID non vide est renseigné.
    pub fn is_valid(&self) -> bool {
        self.ssid.as_deref().is_some_and(|s| !s.is_empty())
    }

    pub fn ssid(&self) -> Option<&str> {
        self.ssid.as_deref()
    }

    pub fn passphrase(&self) -> Option<&str> {
        self.passphrase.as_deref()
    }

    pub fn encryption(&self) -> Option<&str> {
        self.encryption.as_deref()
    }

    pub fn set_ssid(&mut self, ssid: Option<&str>) {
        self.ssid = ssid.map(str::to_string);
    }

    pub fn set_passphrase(&mut self, passphrase: Option<&str>) {
        self.passphrase = passphrase.map(str::to_string);
    }

    pub fn set_encryption(&mut self, encryption: Option<&str>) {
        self.encryption = encryption.map(str::to_string);
    }
}
