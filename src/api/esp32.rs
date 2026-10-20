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

#[cfg(desktop)]
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
#[cfg(desktop)]
pub(crate) fn notify_detections(detections: &[crate::Detection]) {
    enabled::notify_detections(detections);
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
        install(&path, |path| {
            Link::open(path, move |event| {
                let _ = sink.add(event.into());
            })
        })
    }

    /// Holds the slot across the open, so two connects cannot both pass the check.
    pub(super) fn install<O>(path: &str, open: O) -> Result<(), String>
    where
        O: FnOnce(&str) -> anyhow::Result<Link>,
    {
        let mut slot = lock();
        if slot.is_some() {
            return Err("the ESP32 link is already open; disconnect it first".into());
        }
        let link = open(path).map_err(|error| format!("opening {path} failed: {error:#}"))?;
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

#[cfg(all(test, desktop))]
mod tests {
    use std::io::{self, Read, Write};
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::sync::{Mutex, MutexGuard};
    use std::time::{Duration, Instant};

    use kataglyphis_esp32::kataglyphis_esp32_proto::{encode, Decoder, Message, MAX_FRAME};

    use super::*;

    const WAIT: Duration = Duration::from_secs(5);
    const NO_LINK: &str = "no ESP32 link is open";

    /// The bridge is one process-wide slot, so the tests that use it take turns.
    fn serial() -> MutexGuard<'static, ()> {
        static SERIAL: Mutex<()> = Mutex::new(());
        let guard = SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        esp32_disconnect();
        guard
    }

    /// The host's end of an in-memory serial line; a read times out like a real port.
    struct HostReader {
        from_board: mpsc::Receiver<Vec<u8>>,
        pending: Vec<u8>,
    }

    impl Read for HostReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pending.is_empty() {
                match self.from_board.recv_timeout(Duration::from_millis(10)) {
                    Ok(bytes) => self.pending = bytes,
                    Err(RecvTimeoutError::Timeout) => return Err(io::ErrorKind::TimedOut.into()),
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err(io::ErrorKind::BrokenPipe.into());
                    }
                }
            }
            let n = buf.len().min(self.pending.len());
            buf[..n].copy_from_slice(&self.pending[..n]);
            self.pending.drain(..n);
            Ok(n)
        }
    }

    struct HostWriter(mpsc::Sender<Vec<u8>>);

    impl Write for HostWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .send(buf.to_vec())
                .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// The board's end of the line; dropping it unplugs the board.
    struct Board {
        to_host: mpsc::Sender<Vec<u8>>,
        from_host: mpsc::Receiver<Vec<u8>>,
        decoder: Decoder,
    }

    impl Board {
        fn send(&self, message: Message) {
            let mut frame = [0u8; MAX_FRAME];
            let len = encode(0, &message, &mut frame).unwrap();
            self.to_host.send(frame[..len].to_vec()).unwrap();
        }

        /// The next frame the host wrote that is not its repeated HostHello.
        fn command(&mut self) -> Message {
            let deadline = Instant::now() + WAIT;
            loop {
                while let Some(frame) = self.decoder.next_frame() {
                    if !matches!(frame.message, Message::HostHello { .. }) {
                        return frame.message;
                    }
                }
                let left = deadline.saturating_duration_since(Instant::now());
                let bytes = self
                    .from_host
                    .recv_timeout(left)
                    .expect("no command from the host");
                assert!(self.decoder.push(&bytes));
            }
        }
    }

    fn fake_link(path: &str) -> anyhow::Result<(Link, Board)> {
        let (to_host, from_board) = mpsc::channel();
        let (to_board, from_host) = mpsc::channel();
        let reader = HostReader {
            from_board,
            pending: Vec::new(),
        };
        let link = Link::start(path, reader, HostWriter(to_board), |_| {})?;
        let board = Board {
            to_host,
            from_host,
            decoder: Decoder::new(),
        };
        Ok((link, board))
    }

    /// Opens a fake board through the same slot `esp32_connect` fills.
    fn plug_in() -> Board {
        let mut board = None;
        enabled::install("fake", |path| {
            let (link, end) = fake_link(path)?;
            board = Some(end);
            Ok(link)
        })
        .unwrap();
        board.unwrap()
    }

    fn info(chip: Chip) -> DeviceInfo {
        DeviceInfo {
            protocol: 1,
            firmware_major: 0,
            firmware_minor: 3,
            firmware_patch: 12,
            chip,
            mac: [0xE8, 0x06, 0x90, 0x9D, 0x3E, 0x78],
            uptime_ms: 42,
        }
    }

    /// The optional fields an event carries; `kind` promises exactly these.
    fn carried(event: &Esp32Event) -> Vec<&'static str> {
        let fields = [
            ("text", event.text.is_some()),
            ("value", event.value.is_some()),
            ("flag", event.flag.is_some()),
            ("score_permille", event.score_permille.is_some()),
            ("firmware", event.firmware.is_some()),
            ("protocol", event.protocol.is_some()),
            ("mac", event.mac.is_some()),
            ("uptime_ms", event.uptime_ms.is_some()),
            ("free_heap", event.free_heap.is_some()),
            ("temperature_centi_c", event.temperature_centi_c.is_some()),
            ("detection_count", event.detection_count.is_some()),
        ];
        fields
            .into_iter()
            .filter_map(|(name, set)| set.then_some(name))
            .collect()
    }

    #[test]
    fn hello_names_the_chip_and_formats_firmware_and_mac() {
        let event = Esp32Event::from(LinkEvent::Hello(info(Chip::Esp32S3)));
        assert_eq!(event.kind, "hello");
        assert_eq!(
            carried(&event),
            ["text", "firmware", "protocol", "mac", "uptime_ms"]
        );
        assert_eq!(event.text.as_deref(), Some("ESP32-S3"));
        assert_eq!(event.firmware.as_deref(), Some("0.3.12"));
        assert_eq!(event.protocol, Some(1));
        assert_eq!(event.mac.as_deref(), Some("E8:06:90:9D:3E:78"));
        assert_eq!(event.uptime_ms, Some(42));

        let other = Esp32Event::from(LinkEvent::Hello(info(Chip::Other)));
        assert_eq!(other.text.as_deref(), Some("ESP32"));
    }

    #[test]
    fn telemetry_carries_its_readings_and_the_button_bit_as_its_flag() {
        let telemetry = |flags| {
            Esp32Event::from(LinkEvent::Telemetry(Telemetry {
                uptime_ms: 9000,
                free_heap: 123_456,
                temperature_centi_c: -1250,
                flags,
                detection_count: 4,
            }))
        };
        let held = telemetry(TELEMETRY_BUTTON_DOWN);
        assert_eq!(held.kind, "telemetry");
        assert_eq!(
            carried(&held),
            [
                "flag",
                "uptime_ms",
                "free_heap",
                "temperature_centi_c",
                "detection_count"
            ]
        );
        assert_eq!(held.flag, Some(true));
        assert_eq!(held.uptime_ms, Some(9000));
        assert_eq!(held.free_heap, Some(123_456));
        assert_eq!(held.temperature_centi_c, Some(-1250));
        assert_eq!(held.detection_count, Some(4));

        assert_eq!(telemetry(0).flag, Some(false));
        assert_eq!(telemetry(!TELEMETRY_BUTTON_DOWN).flag, Some(false));
    }

    #[test]
    fn the_small_events_carry_one_or_two_fields() {
        let cases = [
            (
                LinkEvent::Pong { id: 7 },
                "pong",
                vec!["value"],
                Some(7),
                None,
            ),
            (
                LinkEvent::Ack { command: 3 },
                "ack",
                vec!["value"],
                Some(3),
                None,
            ),
            (
                LinkEvent::Error { code: 2 },
                "error",
                vec!["value"],
                Some(2),
                None,
            ),
            (
                LinkEvent::Button {
                    pressed: false,
                    count: 5,
                },
                "button",
                vec!["value", "flag"],
                Some(5),
                Some(false),
            ),
        ];
        for (link_event, kind, fields, value, flag) in cases {
            let event = Esp32Event::from(link_event);
            assert_eq!(event.kind, kind);
            assert_eq!(carried(&event), fields, "{kind}");
            assert_eq!((event.value, event.flag), (value, flag), "{kind}");
        }

        let camera = Esp32Event::from(LinkEvent::CameraDetection {
            count: 2,
            score_permille: 875,
        });
        assert_eq!(camera.kind, "camera");
        assert_eq!(carried(&camera), ["value", "score_permille"]);
        assert_eq!((camera.value, camera.score_permille), (Some(2), Some(875)));

        let gone = Esp32Event::from(LinkEvent::Disconnected("read failed".into()));
        assert_eq!(gone.kind, "disconnected");
        assert_eq!(carried(&gone), ["text"]);
        assert_eq!(gone.text.as_deref(), Some("read failed"));
    }

    #[test]
    fn a_desktop_build_has_the_bridge() {
        assert!(esp32_available());
    }

    #[test]
    fn the_device_list_puts_espressif_boards_first() {
        let devices = esp32_list_devices().unwrap();
        assert!(devices
            .windows(2)
            .all(|pair| pair[0].espressif >= pair[1].espressif));
    }

    #[test]
    fn without_a_link_every_command_says_so() {
        let _turn = serial();
        assert!(!esp32_connected());
        assert_eq!(esp32_set_led(1, 2, 3, 0), Err(NO_LINK.into()));
        assert_eq!(esp32_ping(), Err(NO_LINK.into()));
        assert_eq!(esp32_set_telemetry_period(100), Err(NO_LINK.into()));
        assert_eq!(esp32_notify_detection(1, 0.5), Err(NO_LINK.into()));
        esp32_disconnect();
    }

    #[test]
    fn commands_reach_the_board_until_it_is_disconnected() {
        let _turn = serial();
        let mut board = plug_in();
        assert_eq!(esp32_set_led(0, 255, 0, 500), Ok(()));
        assert_eq!(esp32_ping(), Ok(()));
        assert_eq!(esp32_ping(), Ok(()));
        assert_eq!(esp32_set_telemetry_period(250), Ok(()));
        assert_eq!(esp32_notify_detection(2, 0.5), Ok(()));

        assert_eq!(
            board.command(),
            Message::SetLed {
                red: 0,
                green: 255,
                blue: 0,
                blink_ms: 500
            }
        );
        let (Message::Ping { id: first }, Message::Ping { id: second }) =
            (board.command(), board.command())
        else {
            panic!("two pings were sent");
        };
        assert_eq!(second, first.wrapping_add(1));
        assert_eq!(board.command(), Message::SetTelemetryPeriod { ms: 250 });
        assert_eq!(
            board.command(),
            Message::Detection {
                count: 2,
                score_permille: 500
            }
        );

        esp32_disconnect();
        assert_eq!(esp32_ping(), Err(NO_LINK.into()));
    }

    #[test]
    fn connected_follows_the_boards_hello() {
        let _turn = serial();
        let board = plug_in();
        assert!(!esp32_connected());
        board.send(Message::Hello(info(Chip::Esp32S3)));
        let deadline = Instant::now() + WAIT;
        while !esp32_connected() {
            assert!(Instant::now() < deadline, "no handshake after Hello");
            std::thread::sleep(Duration::from_millis(10));
        }
        esp32_disconnect();
        assert!(!esp32_connected());
    }

    #[test]
    fn a_second_connect_is_refused_and_the_first_link_stays() {
        let _turn = serial();
        let mut board = plug_in();
        let mut opened = false;
        let second = enabled::install("other", |path| {
            opened = true;
            fake_link(path).map(|(link, _)| link)
        });
        assert_eq!(
            second,
            Err("the ESP32 link is already open; disconnect it first".into())
        );
        assert!(!opened, "the second port was opened anyway");

        assert_eq!(esp32_ping(), Ok(()));
        assert!(matches!(board.command(), Message::Ping { .. }));
        esp32_disconnect();
    }

    #[test]
    fn a_port_that_fails_to_open_leaves_no_link() {
        let _turn = serial();
        let error =
            enabled::install("/nonexistent/ttyESP32", |path| Link::open(path, |_| {})).unwrap_err();
        let expected =
            "opening /nonexistent/ttyESP32 failed: failed to open ESP32 port /nonexistent/ttyESP32";
        assert!(error.starts_with(expected), "{error}");
        assert_eq!(esp32_ping(), Err(NO_LINK.into()));
    }

    #[test]
    fn an_unplugged_board_asks_for_a_reconnect() {
        let _turn = serial();
        drop(plug_in());
        let deadline = Instant::now() + WAIT;
        let error = loop {
            match esp32_ping() {
                Ok(()) => assert!(Instant::now() < deadline, "the writer survived the unplug"),
                Err(error) => break error,
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(error, "the ESP32 writer has stopped; reconnect the board");
        esp32_disconnect();
    }

    #[test]
    fn the_webcam_hook_forwards_the_count_and_the_best_score() {
        let _turn = serial();
        let mut board = plug_in();
        let detection = |score| crate::Detection {
            score,
            ..Default::default()
        };
        notify_detections(&[detection(0.41), detection(0.83), detection(0.6)]);
        assert_eq!(
            board.command(),
            Message::Detection {
                count: 3,
                score_permille: 830
            }
        );

        std::thread::sleep(Duration::from_millis(150));
        notify_detections(&[]);
        assert_eq!(
            board.command(),
            Message::Detection {
                count: 0,
                score_permille: 0
            }
        );
        esp32_disconnect();
    }
}
