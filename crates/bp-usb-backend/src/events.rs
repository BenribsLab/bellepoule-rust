//! Format des événements écrits sur stderr (identique au C++).
//!
//! Chaque événement est un objet JSON multiligne suivi d'une ligne `.`,
//! que BellePoule (`UsbBroker`) découpe et analyse.

use std::io::Write;

/// `ReportError`.
pub fn error(title: &str, message: &str) -> String {
    format!("{{\n  \"Event\":   \"ERROR\",\n  \"Message\": \"{title}: {message}\"\n}}\n.\n")
}

/// Emplacement libusb : `%04x:%04x` (bus, port).
pub fn libusb_slot(bus: u8, port: u8) -> String {
    format!("{bus:04x}:{port:04x}")
}

/// Branchement d'une clé (version libusb, `main.cpp`).
pub fn libusb_plug(slot: &str, product: &str, manufacturer: &str, serial_number: &str) -> String {
    format!(
        "{{\n  \"Event\":        \"PLUG\",\n  \"Slot\":         \"{slot}\",\n  \"Product\":      \"{product}\",\n  \"Manufacturer\": \"{manufacturer}\",\n  \"SerialNumber\": \"{serial_number}\"\n}}\n.\n"
    )
}

/// Débranchement (même format pour libusb et Windows).
pub fn unplug(slot: &str) -> String {
    format!("{{\n  \"Event\": \"UNPLUG\",\n  \"Slot\":  \"{slot}\"\n}}\n.\n")
}

/// Branchement (version Windows, `main-win.cpp`) : l'emplacement est le
/// numéro de série, le fabricant et le produit sont les champs `VID_xxxx`
/// et `PID_xxxx` du chemin du périphérique.
pub fn windows_plug(serial_number: &str, vendor: &str, product: &str) -> String {
    format!(
        "{{\n  \"Event\":        \"PLUG\",\n  \"Slot\":         \"{serial_number}\",\n  \"Manufacturer\": \"{vendor}\",\n  \"Product\":      \"{product}\",\n  \"SerialNumber\": \"{serial_number}\"\n}}\n.\n"
    )
}

/// Écrit un événement sur stderr (`fprintf (stderr, ...); fflush (stderr)`).
pub fn emit(event: &str) {
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(event.as_bytes());
    let _ = err.flush();
}

/// Champs extraits du nom d'interface Windows (`dbcc_name`), par exemple
/// `\\?\USB#VID_0781&PID_5567#4C530001231108102174#{a5dcbf10-...}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceName {
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub serial_number: String,
}

/// Découpage façon `strtok_r` (les séparateurs consécutifs sont fusionnés).
fn tokens(s: &str, sep: char) -> impl Iterator<Item = &str> {
    s.split(sep).filter(|t| !t.is_empty())
}

/// Analyse de `dbcc_name` (`OnWindowMessage`). `None` s'il n'y a pas de
/// troisième champ (numéro de série).
pub fn parse_device_name(name: &str) -> Option<DeviceName> {
    let mut vendor = None;
    let mut product = None;
    for (i, token) in tokens(name, '#').enumerate() {
        if i == 1 {
            let mut factory = tokens(token, '&');
            vendor = factory.next().map(str::to_string);
            product = factory.next().map(str::to_string);
        } else if i == 2 {
            return Some(DeviceName { vendor, product, serial_number: token.to_string() });
        }
    }
    None
}

/// `printf ("%s", NULL)` affiche `(null)`.
pub fn or_null(s: &Option<String>) -> &str {
    s.as_deref().unwrap_or("(null)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_format() {
        assert_eq!(
            error("libusb_open", "LIBUSB_ERROR_ACCESS"),
            "{\n  \"Event\":   \"ERROR\",\n  \"Message\": \"libusb_open: LIBUSB_ERROR_ACCESS\"\n}\n.\n"
        );
    }

    #[test]
    fn libusb_formats() {
        let slot = libusb_slot(1, 0x1a);
        assert_eq!(slot, "0001:001a");
        assert_eq!(
            libusb_plug(&slot, "Cruzer", "SanDisk", "123"),
            "{\n  \"Event\":        \"PLUG\",\n  \"Slot\":         \"0001:001a\",\n  \"Product\":      \"Cruzer\",\n  \"Manufacturer\": \"SanDisk\",\n  \"SerialNumber\": \"123\"\n}\n.\n"
        );
        assert_eq!(unplug(&slot), "{\n  \"Event\": \"UNPLUG\",\n  \"Slot\":  \"0001:001a\"\n}\n.\n");
    }

    #[test]
    fn windows_formats() {
        let name = r"\\?\USB#VID_0781&PID_5567#4C530001231108102174#{a5dcbf10-6530-11d2-901f-00c04fb951ed}";
        let d = parse_device_name(name).unwrap();
        assert_eq!(d.vendor.as_deref(), Some("VID_0781"));
        assert_eq!(d.product.as_deref(), Some("PID_5567"));
        assert_eq!(d.serial_number, "4C530001231108102174");
        assert_eq!(
            windows_plug(&d.serial_number, or_null(&d.vendor), or_null(&d.product)),
            "{\n  \"Event\":        \"PLUG\",\n  \"Slot\":         \"4C530001231108102174\",\n  \"Manufacturer\": \"VID_0781\",\n  \"Product\":      \"PID_5567\",\n  \"SerialNumber\": \"4C530001231108102174\"\n}\n.\n"
        );
    }

    #[test]
    fn windows_name_edge_cases() {
        assert_eq!(parse_device_name(r"\\?\USB#VID_0781"), None);
        let d = parse_device_name("##A##B#C").unwrap();
        assert_eq!((d.vendor.as_deref(), d.product, d.serial_number.as_str()), (Some("B"), None, "C"));
    }
}
