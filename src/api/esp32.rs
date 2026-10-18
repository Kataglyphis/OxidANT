//! flutter_rust_bridge surface for the ESP32-S3 companion. The real work is in
//! `kataglyphis_esp32`; this module only maps its types for Dart and holds the one live link.

use crate::frb_generated::StreamSink;

#[cfg(desktop)]
use kataglyphis_esp32::kataglyphis_esp32_proto::{
    Chip, DeviceInfo, Telemetry, TELEMETRY_BUTTON_DOWN,
};
#[cfg(desktop)]
use kataglyphis_esp32::{Link, LinkEvent};

pub struct Esp32Device {
    pub path: String,
    pub serial_number: Option<String>,
    pub product: Option<String>,
    pub manufacturer: Option<String>,
    pub vendor_id: u32,
    pub product_id: u32,
    pub espressif: bool,
}

/// A flattened link event: `kind` selects which of the optional fields are set, so the
/// Dart side needs no Rust enum support.
pub struct Esp32Event {
    pub kind: String,
    /// ESP32-S3, a disconnect reason, ...
    pub text: Option<String>,
    /// Pong id, acknowledged command, button count or error code.
    pub value: Option<u32>,
    /// Button state for `button` and `telemetry` events.
    pub flag: Option<bool>,
    /// Best score in per mille for `camera` events.
    pub score_permille: Option<u32>,
    pub firmware: Option<String>,
    pub protocol: Option<u32>,
    pub mac: Option<String>,
    pub uptime_ms: Option<u32>,
    pub free_heap: Option<u32>,
    pub temperature_centi_c: Option<i32>,
    pub detection_count: Option<u32>,
}

impl Esp32Event {
    fn empty(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            text: None,
            value: None,
            flag: None,
            score_permille: None,
            firmware: None,
            protocol: None,
            mac: None,
            uptime_ms: None,
            free_heap: None,
            temperature_centi_c: None,
            detection_count: None,
        }
    }
}

#[cfg(desktop)]
impl From<LinkEvent> for Esp32Event {
    fn from(event: LinkEvent) -> Self {
        match event {
            LinkEvent::Hello(info) => hello(info),
            LinkEvent::Telemetry(telemetry) => telemetry_event(telemetry),
            LinkEvent::Pong { id } => {
                let mut event = Esp32Event::empty("pong");
                event.value = Some(id);
                event
            }
            LinkEvent::Ack { command } => {
                let mut event = Esp32Event::empty("ack");
                event.value = Some(command as u32);
                event
            }
            LinkEvent::Button { pressed, count } => {
                let mut event = Esp32Event::empty("button");
                event.flag = Some(pressed);
                event.value = Some(count);
                event
            }
            LinkEvent::Error { code } => {
                let mut event = Esp32Event::empty("error");
                event.value = Some(code as u32);
                event
            }
            LinkEvent::CameraDetection {
                count,
                score_permille,
            } => {
                let mut event = Esp32Event::empty("camera");
                event.value = Some(count as u32);
                event.score_permille = Some(score_permille as u32);
                event
            }
            LinkEvent::Disconnected(reason) => {
                let mut event = Esp32Event::empty("disconnected");
                event.text = Some(reason);
                event
            }
        }
    }
}

#[cfg(desktop)]
fn hello(info: DeviceInfo) -> Esp32Event {
    let mut event = Esp32Event::empty("hello");
    event.text = Some(
        match info.chip {
            Chip::Esp32S3 => "ESP32-S3",
            Chip::Other => "ESP32",
        }
        .into(),
    );
    event.firmware = Some(format!(
        "{}.{}.{}",
        info.firmware_major, info.firmware_minor, info.firmware_patch
    ));
    event.protocol = Some(info.protocol as u32);
    event.mac = Some(
        info.mac
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
    );
    event.uptime_ms = Some(info.uptime_ms);
    event
}

#[cfg(desktop)]
fn telemetry_event(telemetry: Telemetry) -> Esp32Event {
    let mut event = Esp32Event::empty("telemetry");
    event.uptime_ms = Some(telemetry.uptime_ms);
    event.free_heap = Some(telemetry.free_heap);
    event.temperature_centi_c = Some(telemetry.temperature_centi_c as i32);
    event.flag = Some(telemetry.flags & TELEMETRY_BUTTON_DOWN != 0);
    event.detection_count = Some(telemetry.detection_count as u32);
    event
}

/// Whether this build can talk to the board: false on web and Android.
#[flutter_rust_bridge::frb(sync)]
pub fn esp32_available() -> bool {
    cfg!(desktop)
}

/// Lists USB serial ports, Espressif boards first. Other ports are included so a
/// relabelled board can still be picked by hand.
pub fn esp32_list_devices() -> Result<Vec<Esp32Device>, String> {
    #[cfg(desktop)]
    {
        enabled::list_devices()
    }
    #[cfg(not(desktop))]
    {
        Err("the ESP32 bridge needs a desktop build".into())
    }
}

/// Opens the port and streams events until [`esp32_disconnect`]. Fails when a link is
/// already open.
pub fn esp32_connect(path: String, sink: StreamSink<Esp32Event>) -> Result<(), String> {
    #[cfg(desktop)]
    {
        enabled::connect(path, sink)
    }
    #[cfg(not(desktop))]
    {
        let _ = (path, sink);
        Err("the ESP32 bridge needs a desktop build".into())
    }
}

/// Stops the link and drains its threads.
pub fn esp32_disconnect() {
    #[cfg(desktop)]
    enabled::disconnect();
}

/// Whether the board answered the handshake on the live link.
#[flutter_rust_bridge::frb(sync)]
pub fn esp32_connected() -> bool {
    #[cfg(desktop)]
    {
        enabled::connected()
    }
    #[cfg(not(desktop))]
    {
        false
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn esp32_set_led(red: u8, green: u8, blue: u8, blink_ms: u16) -> Result<(), String> {
    #[cfg(desktop)]
    {
        enabled::with_link(|link| link.set_led(red, green, blue, blink_ms))
    }
    #[cfg(not(desktop))]
    {
        let _ = (red, green, blue, blink_ms);
        Err("the ESP32 bridge needs a desktop build".into())
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn esp32_ping() -> Result<(), String> {
    #[cfg(desktop)]
    {
        enabled::ping()
    }
    #[cfg(not(desktop))]
    {
        Err("the ESP32 bridge needs a desktop build".into())
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn esp32_set_telemetry_period(ms: u16) -> Result<(), String> {
    #[cfg(desktop)]
    {
        enabled::with_link(|link| link.set_telemetry_period(ms))
    }
    #[cfg(not(desktop))]
    {
        let _ = ms;
        Err("the ESP32 bridge needs a desktop build".into())
    }
}

/// Forwards an externally counted detection batch (the ObservANT web page can call
/// this); the webcam pipeline reports its own detections from Rust without Dart.
#[flutter_rust_bridge::frb(sync)]
pub fn esp32_notify_detection(count: u16, score: f32) -> Result<(), String> {
    #[cfg(desktop)]
    {
        enabled::with_link(|link| {
            link.notify_detection(count, score);
            true
        })
    }
    #[cfg(not(desktop))]
    {
        let _ = (count, score);
        Err("the ESP32 bridge needs a desktop build".into())
    }
}

/// Called by the webcam engine for every inference result; throttling lives in the link.
#[cfg(all(feature = "gstreamer", onnx))]
pub(crate) fn notify_detections(detections: &[crate::Detection]) {
    #[cfg(desktop)]
    enabled::notify_detections(detections);
    #[cfg(not(desktop))]
    let _ = detections;
}

#[cfg(desktop)]
mod enabled {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Mutex, OnceLock};

    use super::*;

    static LINK: OnceLock<Mutex<Option<Link>>> = OnceLock::new();
    static NEXT_PING: AtomicU32 = AtomicU32::new(1);

    fn slot() -> &'static Mutex<Option<Link>> {
        LINK.get_or_init(|| Mutex::new(None))
    }

    fn lock() -> std::sync::MutexGuard<'static, Option<Link>> {
        slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn list_devices() -> Result<Vec<Esp32Device>, String> {
        kataglyphis_esp32::list_devices()
            .map(|devices| {
                devices
                    .into_iter()
                    .map(|device| Esp32Device {
                        path: device.path,
                        serial_number: device.serial_number,
                        product: device.product,
                        manufacturer: device.manufacturer,
                        vendor_id: device.vendor_id as u32,
                        product_id: device.product_id as u32,
                        espressif: device.espressif,
                    })
                    .collect()
            })
            .map_err(|error| format!("listing serial ports failed: {error:#}"))
    }

    pub(super) fn connect(path: String, sink: StreamSink<Esp32Event>) -> Result<(), String> {
        if lock().is_some() {
            return Err("the ESP32 link is already open; disconnect it first".into());
        }

        let mut slot = lock();
        let link = Link::open(&path, move |event| {
            let _ = sink.add(event.into());
        })
        .map_err(|error| format!("opening {path} failed: {error:#}"))?;
        *slot = Some(link);
        Ok(())
    }

    pub(super) fn disconnect() {
        let link = lock().take();
        if let Some(mut link) = link {
            link.close();
        }
    }

    pub(super) fn connected() -> bool {
        lock().as_ref().is_some_and(|link| link.is_connected())
    }

    pub(super) fn ping() -> Result<(), String> {
        let id = NEXT_PING.fetch_add(1, Ordering::Relaxed);
        with_link(|link| link.ping(id))
    }

    pub(super) fn with_link<F>(action: F) -> Result<(), String>
    where
        F: FnOnce(&Link) -> bool,
    {
        let guard = lock();
        let Some(link) = guard.as_ref() else {
            return Err("no ESP32 link is open".into());
        };
        if action(link) {
            Ok(())
        } else {
            Err("the ESP32 writer has stopped; reconnect the board".into())
        }
    }

    #[cfg(all(feature = "gstreamer", onnx))]
    pub(super) fn notify_detections(detections: &[crate::Detection]) {
        let count = detections.len().min(u16::MAX as usize) as u16;
        let score = detections
            .iter()
            .map(|detection| detection.score)
            .fold(0.0f32, f32::max);
        let guard = lock();
        if let Some(link) = guard.as_ref() {
            link.notify_detection(count, score);
        }
    }
}
