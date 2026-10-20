//! Clé USB détectée (port de `common/network/usb_drive.cpp`).

/// Longueur maximale conservée du numéro de série (tampon de 256 + 1 octets).
pub const SERIAL_MAX_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbDrive {
    slot: Option<String>,
    product: Option<String>,
    manufacturer: Option<String>,
    serial_number: String,
}

impl UsbDrive {
    pub fn new(slot: Option<&str>, product: Option<&str>, manufacturer: Option<&str>, serial_number: Option<&str>) -> Self {
        let mut serial = serial_number.unwrap_or("").to_string();
        if serial.len() > SERIAL_MAX_LEN {
            let mut cut = SERIAL_MAX_LEN;
            while !serial.is_char_boundary(cut) {
                cut -= 1;
            }
            serial.truncate(cut);
        }
        UsbDrive {
            slot: slot.map(str::to_string),
            product: product.map(str::to_string),
            manufacturer: manufacturer.map(str::to_string),
            serial_number: serial,
        }
    }

    pub fn slot(&self) -> Option<&str> {
        self.slot.as_deref()
    }

    pub fn product(&self) -> Option<&str> {
        self.product.as_deref()
    }

    pub fn manufacturer(&self) -> Option<&str> {
        self.manufacturer.as_deref()
    }

    /// Numéro de série : sert de clé de chiffrement pour le défi USB.
    pub fn serial_number(&self) -> &str {
        &self.serial_number
    }
}
