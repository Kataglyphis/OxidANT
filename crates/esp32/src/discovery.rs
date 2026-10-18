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
    add_by_id_devices(&mut devices);

    devices.sort_by(|a, b| {
        b.espressif
            .cmp(&a.espressif)
            .then_with(|| a.path.cmp(&b.path))
    });
    devices.dedup_by(|a, b| a.path == b.path);
    Ok(devices)
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
fn add_by_id_devices(devices: &mut Vec<DeviceInfo>) {
    const BY_ID: &str = "/dev/serial/by-id";
    let Ok(entries) = std::fs::read_dir(BY_ID) else {
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
        let serial = name
            .split('_')
            .find(|part| part.matches(':').count() == 5)
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
    use super::*;

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
