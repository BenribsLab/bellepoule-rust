//! Détection native Windows (port de `backend/main-win.cpp`).
//!
//! Une fenêtre cachée s'abonne aux notifications de l'interface
//! `GUID_DEVINTERFACE_USB_DEVICE` (`RegisterDeviceNotification`) ; chaque
//! `WM_DEVICECHANGE` (arrivée ou retrait) est traduit en événement JSON à
//! partir du chemin du périphérique (`VID_xxxx&PID_xxxx` et numéro de série).
//! Comme le C++, tous les périphériques USB sont signalés.

use std::ptr::{null, null_mut};

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{UpdateWindow, COLOR_WINDOW, HBRUSH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, LoadCursorW, LoadIconW,
    PostQuitMessage, RegisterClassExW, RegisterDeviceNotificationW, ShowWindow, TranslateMessage,
    UnregisterDeviceNotification, CS_HREDRAW, CS_OWNDC, CS_VREDRAW, CW_USEDEFAULT, DBT_DEVICEARRIVAL,
    DBT_DEVICEREMOVECOMPLETE, DBT_DEVTYP_DEVICEINTERFACE, DEVICE_NOTIFY_WINDOW_HANDLE, DEV_BROADCAST_DEVICEINTERFACE_W,
    HDEVNOTIFY, IDC_ARROW, IDI_APPLICATION, MSG, SW_HIDE, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DEVICECHANGE,
    WNDCLASSEXW, WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_OVERLAPPEDWINDOW,
};

use crate::events;

/// `GUID_DEVINTERFACE_USB_DEVICE`.
const USB_DEVICE_GUID: GUID = GUID {
    data1: 0xa5dcbf10,
    data2: 0x6530,
    data3: 0x11d2,
    data4: [0x90, 0x1f, 0x00, 0xc0, 0x4f, 0xb9, 0x51, 0xed],
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `ErrorHandler` : « X failed with error N: message ».
fn report_last_error(function: &str) {
    let code = unsafe { GetLastError() };
    let message = std::io::Error::from_raw_os_error(code as i32).to_string();
    events::emit(&events::error("Internal error", &format!("{function} failed with error {code}: {message}")));
}

static mut NOTIFICATION: HDEVNOTIFY = null_mut();

/// Lit le nom (UTF-16 terminé par zéro) d'une interface de périphérique.
unsafe fn device_name(device: *const DEV_BROADCAST_DEVICEINTERFACE_W) -> String {
    let start = std::ptr::addr_of!((*device).dbcc_name) as *const u16;
    let max = ((*device).dbcc_size as usize).saturating_sub(std::mem::offset_of!(DEV_BROADCAST_DEVICEINTERFACE_W, dbcc_name)) / 2;
    let mut len = 0;
    while len < max && *start.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(start, len))
}

/// `OnWindowMessage`.
unsafe extern "system" fn on_window_message(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_CREATE => {
            let mut filter: DEV_BROADCAST_DEVICEINTERFACE_W = std::mem::zeroed();
            filter.dbcc_size = std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32;
            filter.dbcc_devicetype = DBT_DEVTYP_DEVICEINTERFACE;
            filter.dbcc_classguid = USB_DEVICE_GUID;
            NOTIFICATION = RegisterDeviceNotificationW(
                window as _,
                std::ptr::addr_of!(filter) as *const _,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            );
            1
        }
        WM_DEVICECHANGE => {
            let device = lparam as *const DEV_BROADCAST_DEVICEINTERFACE_W;
            if !device.is_null() && (*device).dbcc_devicetype == DBT_DEVTYP_DEVICEINTERFACE {
                let name = device_name(device);
                if let Some(d) = events::parse_device_name(&name) {
                    if wparam as u32 == DBT_DEVICEARRIVAL {
                        events::emit(&events::windows_plug(
                            &d.serial_number,
                            events::or_null(&d.vendor),
                            events::or_null(&d.product),
                        ));
                    } else if wparam as u32 == DBT_DEVICEREMOVECOMPLETE {
                        events::emit(&events::unplug(&d.serial_number));
                    }
                }
            }
            1
        }
        WM_CLOSE => {
            if !NOTIFICATION.is_null() {
                UnregisterDeviceNotification(NOTIFICATION);
                NOTIFICATION = null_mut();
            }
            DestroyWindow(window);
            1
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            1
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

/// `_tWinMain` : renvoie le code de sortie.
pub fn run() -> i32 {
    let class_name = wide("BellePouleWindowClass");
    let title = wide("BellePoule");
    unsafe {
        let instance = GetModuleHandleW(null());
        let window_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_OWNDC | CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(on_window_message),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as usize as HBRUSH,
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: LoadIconW(null_mut(), IDI_APPLICATION),
        };
        if RegisterClassExW(&window_class) == 0 {
            report_last_error("RegisterClassEx");
            return -1;
        }

        let window = CreateWindowExW(
            WS_EX_CLIENTEDGE | WS_EX_APPWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            0,
            640,
            480,
            null_mut(),
            null_mut(),
            null_mut(),
            null(),
        );
        if window.is_null() {
            report_last_error("CreateWindowEx: main appwindow window");
            return -1;
        }
        ShowWindow(window, SW_HIDE);
        UpdateWindow(window);

        let mut msg: MSG = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result == 0 {
                break;
            }
            if result == -1 {
                report_last_error("GetMessage");
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    0
}
