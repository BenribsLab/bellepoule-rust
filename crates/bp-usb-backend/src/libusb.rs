//! Détection à chaud avec libusb (port de `backend/main.cpp`), utilisée
//! sous Linux et macOS. Sous Windows, libusb ne gère pas le branchement à
//! chaud : c'est la détection native ([`crate::windows`]) qui est employée,
//! comme dans le C++ (`main-win.cpp`).

use rusb::{Context, Device, Hotplug, HotplugBuilder, UsbContext};

use crate::events;

/// Classe USB « par interface » (`LIBUSB_CLASS_PER_INTERFACE`).
const CLASS_PER_INTERFACE: u8 = 0x00;
/// Classe USB « stockage de masse » (`LIBUSB_CLASS_MASS_STORAGE`).
const CLASS_MASS_STORAGE: u8 = 0x08;

/// `libusb_error_name`.
pub fn error_name(e: &rusb::Error) -> &'static str {
    match e {
        rusb::Error::Io => "LIBUSB_ERROR_IO",
        rusb::Error::InvalidParam => "LIBUSB_ERROR_INVALID_PARAM",
        rusb::Error::Access => "LIBUSB_ERROR_ACCESS",
        rusb::Error::NoDevice => "LIBUSB_ERROR_NO_DEVICE",
        rusb::Error::NotFound => "LIBUSB_ERROR_NOT_FOUND",
        rusb::Error::Busy => "LIBUSB_ERROR_BUSY",
        rusb::Error::Timeout => "LIBUSB_ERROR_TIMEOUT",
        rusb::Error::Overflow => "LIBUSB_ERROR_OVERFLOW",
        rusb::Error::Pipe => "LIBUSB_ERROR_PIPE",
        rusb::Error::Interrupted => "LIBUSB_ERROR_INTERRUPTED",
        rusb::Error::NoMem => "LIBUSB_ERROR_NO_MEM",
        rusb::Error::NotSupported => "LIBUSB_ERROR_NOT_SUPPORTED",
        _ => "LIBUSB_ERROR_OTHER",
    }
}

/// `DeviceHasClass (device, LIBUSB_CLASS_MASS_STORAGE)`.
fn is_mass_storage<T: UsbContext>(device: &Device<T>) -> bool {
    let desc = match device.device_descriptor() {
        Ok(d) => d,
        Err(e) => {
            events::emit(&events::error("libusb_get_device_descriptor:", error_name(&e)));
            return false;
        }
    };
    match desc.class_code() {
        CLASS_PER_INTERFACE => match device.active_config_descriptor() {
            Ok(config) => config
                .interfaces()
                .any(|i| i.descriptors().any(|alt| alt.class_code() == CLASS_MASS_STORAGE)),
            Err(_) => false,
        },
        CLASS_MASS_STORAGE => true,
        _ => false,
    }
}

struct Handler;

impl<T: UsbContext> Hotplug<T> for Handler {
    fn device_arrived(&mut self, device: Device<T>) {
        if !is_mass_storage(&device) {
            return;
        }
        let Ok(desc) = device.device_descriptor() else { return };
        match device.open() {
            Err(e) => events::emit(&events::error("libusb_open", error_name(&e))),
            Ok(handle) => {
                // GetField : chaîne vide si le descripteur est absent ou illisible
                let field = |index: Option<u8>| {
                    index.and_then(|i| handle.read_string_descriptor_ascii(i).ok()).unwrap_or_default()
                };
                let product = field(desc.product_string_index());
                let manufacturer = field(desc.manufacturer_string_index());
                let serial = field(desc.serial_number_string_index());
                let slot = events::libusb_slot(device.bus_number(), device.port_number());
                events::emit(&events::libusb_plug(&slot, &product, &manufacturer, &serial));
            }
        }
    }

    fn device_left(&mut self, device: Device<T>) {
        let slot = events::libusb_slot(device.bus_number(), device.port_number());
        events::emit(&events::unplug(&slot));
    }
}

/// `main` : surveille les branchements jusqu'à la lecture d'un caractère
/// (ou la fermeture) de stdin.
pub fn run() {
    let context = match Context::new() {
        Ok(c) => c,
        Err(e) => {
            events::emit(&events::error("libusb_init", error_name(&e)));
            return;
        }
    };

    let registration = if rusb::has_hotplug() {
        HotplugBuilder::new().register(context.clone(), Box::new(Handler))
    } else {
        Err(rusb::Error::NotSupported)
    };

    match registration {
        Err(e) => events::emit(&events::error("libusb_hotplug_register_callback", error_name(&e))),
        Ok(_registration) => {
            // Looper
            let looper = context.clone();
            let spawned = std::thread::Builder::new().name("looper".into()).spawn(move || loop {
                let _ = looper.handle_events(None);
            });
            if let Err(e) = spawned {
                events::emit(&events::error("pthread_create", &e.to_string()));
            }

            // getc (stdin)
            let mut byte = [0u8; 1];
            let _ = std::io::Read::read(&mut std::io::stdin(), &mut byte);
        }
    }
}
