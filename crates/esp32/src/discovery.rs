//! ESP32-S3 discovery: Espressif's built-in USB Serial/JTAG enumerates as a CDC-ACM
//! port with VID 0x303A and the board's MAC as its serial number.

use anyhow::Result;
use serialport::{SerialPortInfo, SerialPortType};

/// Espressif's USB vendor ID.
pub const ESPRESSIF_VID: u16 = 0x303A;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub path: String,
    pub serial_number: Option<String>,
    pub product: Option<String>,
    pub manufacturer: Option<String>,
    pub vendor_id: u16,
    pub product_id: u16,
    pub espressif: bool,
}

impl DeviceInfo {
    /// `E8:06:90:9D:3E:78`, when the board exposes its MAC as the USB serial.
    pub fn mac(&self) -> Option<String> {
        self.serial_number.as_deref().and_then(|serial| {
            let hex: Vec<&str> = serial
                .split(':')
                .filter(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
                .collect();
            (hex.len() == 6).then(|| hex.join(":").to_ascii_uppercase())
        })
    }
}

/// Lists serial ports, Espressif boards first. Non-USB ports are omitted: the ESP32-S3
/// is always a USB CDC device.
pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let mut devices: Vec<DeviceInfo> = match serialport::available_ports() {
        Ok(ports) => ports.into_iter().filter_map(from_port).collect(),
        Err(error) => {
            // The sysfs fallback can fail in an unusual sandbox; by-id still names the board.
            log::warn!("serial port enumeration failed: {error}; trying /dev/serial/by-id");
            Vec::new()
        }
    };

    #[cfg(target_os = "linux")]
    add_by_id_devices(std::path::Path::new("/dev/serial/by-id"), &mut devices);

    espressif_first(&mut devices);
    Ok(devices)
}

/// Espressif boards first, each group by path, one entry per path.
fn espressif_first(devices: &mut Vec<DeviceInfo>) {
    devices.sort_by(|a, b| {
        b.espressif
            .cmp(&a.espressif)
            .then_with(|| a.path.cmp(&b.path))
    });
    devices.dedup_by(|a, b| a.path == b.path);
}

fn from_port(port: SerialPortInfo) -> Option<DeviceInfo> {
    let SerialPortType::UsbPort(usb) = port.port_type else {
        return None;
    };
    let espressif = is_espressif(
        usb.vid,
        usb.product.as_deref(),
        usb.serial_number.as_deref(),
        usb.manufacturer.as_deref(),
    );
    Some(DeviceInfo {
        path: port.port_name,
        serial_number: usb.serial_number,
        product: usb.product,
        manufacturer: usb.manufacturer,
        vendor_id: usb.vid,
        product_id: usb.pid,
        espressif,
    })
}

fn is_espressif(
    vendor_id: u16,
    product: Option<&str>,
    serial_number: Option<&str>,
    manufacturer: Option<&str>,
) -> bool {
    if vendor_id == ESPRESSIF_VID {
        return true;
    }
    [product, serial_number, manufacturer]
        .into_iter()
        .flatten()
        .any(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("espressif") || value.contains("usb jtag")
        })
}

/// udev's `by-id` links survive a ttyACM renumbering and name the vendor even when
/// the libudev enumeration misses a freshly attached board.
#[cfg(target_os = "linux")]
fn add_by_id_devices(by_id: &std::path::Path, devices: &mut Vec<DeviceInfo>) {
    let Ok(entries) = std::fs::read_dir(by_id) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_ascii_lowercase();
        if !(lower.contains("espressif") || lower.contains("usb_jtag")) {
            continue;
        }
        let Ok(path) = std::fs::canonicalize(entry.path()) else {
            continue;
        };
        let path = path.to_string_lossy().to_string();
        if devices.iter().any(|device| device.path == path) {
            continue;
        }
        // udev names the link `usb-<vendor>_<model>_<serial>-if<interface>`.
        let serial = name
            .split('_')
            .find(|part| part.matches(':').count() == 5)
            .and_then(|part| part.split('-').next())
            .map(str::to_string);
        devices.push(DeviceInfo {
            path,
            serial_number: serial,
            product: Some("USB JTAG/serial debug unit".into()),
            manufacturer: Some("Espressif".into()),
            vendor_id: ESPRESSIF_VID,
            product_id: 0x1001,
            espressif: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use serialport::UsbPortInfo;

    use super::*;

    fn device(path: &str, espressif: bool) -> DeviceInfo {
        DeviceInfo {
            path: path.into(),
            serial_number: None,
            product: None,
            manufacturer: None,
            vendor_id: if espressif { ESPRESSIF_VID } else { 0x0403 },
            product_id: 0x1001,
            espressif,
        }
    }

    fn usb_port(vid: u16, product: &str, manufacturer: &str) -> SerialPortInfo {
        SerialPortInfo {
            port_name: "/dev/ttyACM0".into(),
            port_type: SerialPortType::UsbPort(UsbPortInfo {
                vid,
                pid: 0x1001,
                serial_number: Some("E8:06:90:9D:3E:78".into()),
                manufacturer: Some(manufacturer.into()),
                product: Some(product.into()),
            }),
        }
    }

    #[test]
    fn a_usb_port_keeps_its_descriptors() {
        let device = from_port(usb_port(
            ESPRESSIF_VID,
            "USB JTAG/serial debug unit",
            "Espressif",
        ))
        .unwrap();
        assert_eq!(
            device,
            DeviceInfo {
                path: "/dev/ttyACM0".into(),
                serial_number: Some("E8:06:90:9D:3E:78".into()),
                product: Some("USB JTAG/serial debug unit".into()),
                manufacturer: Some("Espressif".into()),
                vendor_id: ESPRESSIF_VID,
                product_id: 0x1001,
                espressif: true,
            }
        );
        assert_eq!(device.mac().as_deref(), Some("E8:06:90:9D:3E:78"));
    }

    #[test]
    fn an_ftdi_adapter_is_listed_but_not_as_espressif() {
        let device = from_port(usb_port(0x0403, "FT232R USB UART", "FTDI")).unwrap();
        assert!(!device.espressif);
        assert_eq!(device.vendor_id, 0x0403);
    }

    #[test]
    fn ports_that_are_not_usb_are_left_out() {
        for port_type in [
            SerialPortType::PciPort,
            SerialPortType::BluetoothPort,
            SerialPortType::Unknown,
        ] {
            let port = SerialPortInfo {
                port_name: "/dev/ttyS0".into(),
                port_type,
            };
            assert_eq!(from_port(port), None);
        }
    }

    #[test]
    fn espressif_boards_come_first_then_by_path_once_each() {
        let mut devices = vec![
            device("/dev/ttyUSB0", false),
            device("/dev/ttyACM1", true),
            device("/dev/ttyACM0", false),
            device("/dev/ttyACM1", true),
            device("/dev/ttyACM2", true),
        ];
        espressif_first(&mut devices);
        let order: Vec<(&str, bool)> = devices
            .iter()
            .map(|device| (device.path.as_str(), device.espressif))
            .collect();
        assert_eq!(
            order,
            [
                ("/dev/ttyACM1", true),
                ("/dev/ttyACM2", true),
                ("/dev/ttyACM0", false),
                ("/dev/ttyUSB0", false),
            ]
        );
    }

    #[test]
    fn this_hosts_ports_are_listed_espressif_first_once_each() {
        let devices = list_devices().unwrap();
        assert!(devices
            .windows(2)
            .all(|pair| pair[0].espressif >= pair[1].espressif));
        let mut paths: Vec<&str> = devices.iter().map(|device| device.path.as_str()).collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), devices.len());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn by_id_links_add_the_espressif_boards_the_enumeration_missed() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let tty = |name: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, b"").unwrap();
            path
        };
        let (acm0, acm1, usb0) = (tty("ttyACM0"), tty("ttyACM1"), tty("ttyUSB0"));
        let by_id = dir.path().join("by-id");
        std::fs::create_dir(&by_id).unwrap();
        let link =
            |target: &std::path::Path, name: &str| symlink(target, by_id.join(name)).unwrap();
        link(
            &acm0,
            "usb-Espressif_USB_JTAG_serial_debug_unit_E8:06:90:9D:3E:78-if00",
        );
        link(
            &acm1,
            "usb-Espressif_USB_JTAG_serial_debug_unit_AA:BB:CC:DD:EE:FF-if00",
        );
        link(&usb0, "usb-FTDI_FT232R_USB_UART_A12345-if00-port0");
        link(
            &dir.path().join("gone"),
            "usb-Espressif_USB_JTAG_serial_debug_unit_11:22:33:44:55:66-if00",
        );

        let canonical = |path: &std::path::Path| {
            std::fs::canonicalize(path)
                .unwrap()
                .to_string_lossy()
                .to_string()
        };
        let mut devices = vec![device(&canonical(&acm1), true)];
        add_by_id_devices(&by_id, &mut devices);

        assert_eq!(devices.len(), 2, "{devices:?}");
        assert_eq!(devices[0], device(&canonical(&acm1), true));
        let added = &devices[1];
        assert_eq!(added.path, canonical(&acm0));
        assert_eq!(added.serial_number.as_deref(), Some("E8:06:90:9D:3E:78"));
        assert_eq!(added.mac().as_deref(), Some("E8:06:90:9D:3E:78"));
        assert_eq!(added.vendor_id, ESPRESSIF_VID);
        assert!(added.espressif);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_missing_by_id_directory_adds_nothing() {
        let mut devices = vec![device("/dev/ttyUSB0", false)];
        add_by_id_devices(std::path::Path::new("/nonexistent/by-id"), &mut devices);
        assert_eq!(devices, [device("/dev/ttyUSB0", false)]);
    }

    #[test]
    fn recognizes_the_espressif_vid() {
        assert!(is_espressif(0x303A, None, None, None));
    }

    #[test]
    fn recognizes_a_relabelled_board_by_its_descriptors() {
        assert!(is_espressif(
            0x1234,
            Some("USB JTAG/serial debug unit"),
            None,
            None
        ));
        assert!(is_espressif(0x1234, None, Some("Espressif-1234"), None));
        assert!(is_espressif(0x1234, None, None, Some("Espressif")));
    }

    #[test]
    fn leaves_other_usb_serial_adapters_alone() {
        assert!(!is_espressif(
            0x0403,
            Some("FT232R USB UART"),
            Some("A123"),
            Some("FTDI")
        ));
    }

    #[test]
    fn extracts_the_mac_from_the_usb_serial() {
        let device = DeviceInfo {
            path: "/dev/ttyACM0".into(),
            serial_number: Some("e8:06:90:9d:3e:78".into()),
            product: None,
            manufacturer: None,
            vendor_id: ESPRESSIF_VID,
            product_id: 0x1001,
            espressif: true,
        };
        assert_eq!(device.mac().as_deref(), Some("E8:06:90:9D:3E:78"));
    }

    #[test]
    fn a_non_mac_serial_is_not_rendered_as_one() {
        let device = DeviceInfo {
            path: "/dev/ttyUSB0".into(),
            serial_number: Some("A12345".into()),
            product: None,
            manufacturer: None,
            vendor_id: 0x0403,
            product_id: 0x6001,
            espressif: false,
        };
        assert_eq!(device.mac(), None);
    }
}
