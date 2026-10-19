//! The live link: a reader thread parses frames and a writer thread owns the port, so
//! the inference thread can report a detection without ever blocking on USB.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use kataglyphis_esp32_proto::{
    encode, Decoder, DeviceInfo, Message, Telemetry, MAX_FRAME, PROTOCOL_VERSION,
};

const READ_TIMEOUT: Duration = Duration::from_millis(200);
const WRITE_TIMEOUT: Duration = Duration::from_millis(200);
const HELLO_RETRY: Duration = Duration::from_millis(500);
const CONNECTION_IDLE: Duration = Duration::from_secs(6);

#[derive(Clone, Debug, PartialEq)]
pub enum LinkEvent {
    Hello(DeviceInfo),
    Telemetry(Telemetry),
    Pong { id: u32 },
    Ack { command: u8 },
    Button { pressed: bool, count: u32 },
    Error { code: u8 },
    CameraDetection { count: u16, score_permille: u16 },
    Disconnected(String),
}

/// An open ESP32-S3 USB-serial link. Dropping it stops both threads.
pub struct Link {
    path: String,
    tx: mpsc::Sender<Message>,
    stop: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    throttle: Mutex<DetectionThrottle>,
}

impl Link {
    /// Opens `path` (e.g. `/dev/ttyACM0`, `COM5`) and starts the bridge. Events are
    /// delivered on the reader thread.
    pub fn open<F>(path: &str, on_event: F) -> Result<Self>
    where
        F: Fn(LinkEvent) + Send + 'static,
    {
        let port = serialport::new(path, 115_200)
            .timeout(READ_TIMEOUT)
            .open()
            .with_context(|| format!("failed to open ESP32 port {path}"))?;
        let reader = port
            .try_clone()
            .context("failed to clone the ESP32 serial port for reading")?;
        Self::start(path, reader, port, on_event)
    }

    /// Starts the bridge over any byte stream, a test double included. `reader_port` must
    /// time out (`TimedOut`) like a serial port, or [`Link::close`] waits for its next byte.
    pub fn start<R, W, F>(path: &str, mut reader_port: R, mut port: W, on_event: F) -> Result<Self>
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
        F: Fn(LinkEvent) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Message>();
        let stop = Arc::new(AtomicBool::new(false));
        let connected = Arc::new(AtomicBool::new(false));
        let failure: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let emit = Emitter::new(on_event);

        let writer = {
            let stop = stop.clone();
            let failure = failure.clone();
            let name = format!("esp32-writer ({path})");
            thread::Builder::new()
                .name(name)
                .spawn(move || {
                    write_loop(&mut port, &rx, &stop, &failure);
                })
                .context("failed to spawn the ESP32 writer")?
        };

        let reader = {
            let stop = stop.clone();
            let connected = connected.clone();
            let failure = failure.clone();
            let tx = tx.clone();
            let name = format!("esp32-reader ({path})");
            thread::Builder::new()
                .name(name)
                .spawn(move || {
                    read_loop(&mut reader_port, &tx, &stop, &connected, &failure, &emit);
                })
                .context("failed to spawn the ESP32 reader")?
        };

        Ok(Self {
            path: path.to_string(),
            tx,
            stop,
            connected,
            reader: Some(reader),
            writer: Some(writer),
            throttle: Mutex::new(DetectionThrottle::new()),
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    /// Queues a message; false when the writer is already gone.
    pub fn send(&self, message: Message) -> bool {
        self.tx.send(message).is_ok()
    }

    pub fn set_led(&self, red: u8, green: u8, blue: u8, blink_ms: u16) -> bool {
        self.send(Message::SetLed {
            red,
            green,
            blue,
            blink_ms,
        })
    }

    pub fn ping(&self, id: u32) -> bool {
        self.send(Message::Ping { id })
    }

    pub fn set_telemetry_period(&self, ms: u16) -> bool {
        self.send(Message::SetTelemetryPeriod { ms })
    }

    /// Forwards the detector's latest frame, throttled so USB bandwidth and the board's
    /// blink state machine are never flooded at camera framerate.
    pub fn notify_detection(&self, count: u16, score: f32) {
        let now = Instant::now();
        let mut throttle = self
            .throttle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !throttle.should_send(count, now) {
            return;
        }
        drop(throttle);

        let score_permille = (score.clamp(0.0, 1.0) * 1000.0).round() as u16;
        self.send(Message::Detection {
            count,
            score_permille,
        });
    }

    /// Stops both threads. Idempotent.
    pub fn close(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.connected.store(false, Ordering::Release);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.close();
    }
}

fn write_loop(
    port: &mut impl Write,
    rx: &mpsc::Receiver<Message>,
    stop: &AtomicBool,
    failure: &Mutex<Option<String>>,
) {
    let mut sequence = 0u16;
    while !stop.load(Ordering::Acquire) {
        let message = match rx.recv_timeout(WRITE_TIMEOUT) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        let mut frame = [0u8; MAX_FRAME];
        let Ok(len) = encode(sequence, &message, &mut frame) else {
            log::warn!("esp32: a message did not encode; dropped");
            continue;
        };
        if let Err(error) = port.write_all(&frame[..len]).and_then(|()| port.flush()) {
            *failure.lock().unwrap_or_else(|p| p.into_inner()) =
                Some(format!("write failed: {error}"));
            break;
        }
        sequence = sequence.wrapping_add(1);
    }
}

fn read_loop<F>(
    port: &mut impl Read,
    tx: &mpsc::Sender<Message>,
    stop: &AtomicBool,
    connected: &AtomicBool,
    failure: &Mutex<Option<String>>,
    emit: &Emitter<F>,
) where
    F: Fn(LinkEvent),
{
    let mut decoder = Decoder::new();
    let mut buf = [0u8; 256];
    let mut last_hello = Instant::now() - HELLO_RETRY;
    let mut last_frame = Instant::now();

    while !stop.load(Ordering::Acquire) {
        match port.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                if !decoder.push(&buf[..n]) {
                    log::warn!("esp32: frame buffer overflow; resynchronising");
                    decoder = Decoder::new();
                }
                while let Some(frame) = decoder.next_frame() {
                    last_frame = Instant::now();
                    match frame.message {
                        Message::Hello(info) => {
                            connected.store(true, Ordering::Release);
                            emit.emit(LinkEvent::Hello(info));
                        }
                        Message::Telemetry(telemetry) => {
                            emit.emit(LinkEvent::Telemetry(telemetry));
                        }
                        Message::Pong { id } => emit.emit(LinkEvent::Pong { id }),
                        Message::Ack { command } => emit.emit(LinkEvent::Ack { command }),
                        Message::Button { pressed, count } => {
                            emit.emit(LinkEvent::Button { pressed, count });
                        }
                        Message::Error { code } => emit.emit(LinkEvent::Error { code }),
                        Message::CameraDetection {
                            count,
                            score_permille,
                        } => emit.emit(LinkEvent::CameraDetection {
                            count,
                            score_permille,
                        }),
                        // Host-bound messages echoed by a confused device are ignored.
                        Message::HostHello { .. }
                        | Message::Ping { .. }
                        | Message::SetLed { .. }
                        | Message::Detection { .. }
                        | Message::SetTelemetryPeriod { .. } => {}
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => {
                connected.store(false, Ordering::Release);
                emit.emit(LinkEvent::Disconnected(format!("read failed: {error}")));
                return;
            }
        }

        if let Some(error) = failure.lock().unwrap_or_else(|p| p.into_inner()).take() {
            connected.store(false, Ordering::Release);
            emit.emit(LinkEvent::Disconnected(error));
            return;
        }

        if !connected.load(Ordering::Acquire) && last_hello.elapsed() >= HELLO_RETRY {
            let _ = tx.send(Message::HostHello {
                version: PROTOCOL_VERSION,
            });
            last_hello = Instant::now();
        }

        if connected.load(Ordering::Acquire) && last_frame.elapsed() >= CONNECTION_IDLE {
            connected.store(false, Ordering::Release);
            emit.emit(LinkEvent::Disconnected(
                "no frames for 6 s; is the board still running?".into(),
            ));
            return;
        }
    }
}

struct Emitter<F>(Arc<Mutex<F>>);

impl<F> Emitter<F> {
    fn new(handler: F) -> Self {
        Self(Arc::new(Mutex::new(handler)))
    }

    fn emit(&self, event: LinkEvent)
    where
        F: Fn(LinkEvent),
    {
        if let Ok(handler) = self.0.lock() {
            handler(event);
        }
    }
}

struct DetectionThrottle {
    last_sent: Option<Instant>,
    last_count: u16,
}

impl DetectionThrottle {
    fn new() -> Self {
        Self {
            last_sent: None,
            last_count: 0,
        }
    }

    fn should_send(&mut self, count: u16, now: Instant) -> bool {
        let minimum_gap = Duration::from_millis(100);
        if self
            .last_sent
            .is_some_and(|last| now.duration_since(last) < minimum_gap)
        {
            return false;
        }

        let interval = if count == 0 {
            Duration::from_millis(1000)
        } else {
            Duration::from_millis(250)
        };
        let due = self
            .last_sent
            .is_none_or(|last| now.duration_since(last) >= interval);
        if !due && count == self.last_count {
            return false;
        }

        self.last_sent = Some(now);
        self.last_count = count;
        true
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use kataglyphis_esp32_proto::{Chip, Frame, COMMAND_SET_LED};

    use super::*;

    const WAIT: Duration = Duration::from_secs(5);

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
                        return Err(io::Error::new(io::ErrorKind::BrokenPipe, "unplugged"));
                    }
                }
            }
            let n = buf.len().min(self.pending.len());
            buf[..n].copy_from_slice(&self.pending[..n]);
            self.pending.drain(..n);
            Ok(n)
        }
    }

    struct HostWriter {
        to_board: mpsc::Sender<Vec<u8>>,
        cable_pulled: Arc<AtomicBool>,
    }

    impl Write for HostWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.cable_pulled.load(Ordering::Acquire) {
                return Err(io::Error::other("cable pulled"));
            }
            self.to_board
                .send(buf.to_vec())
                .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// The board's end: frames out to the host, the host's frames decoded.
    struct Board {
        to_host: mpsc::Sender<Vec<u8>>,
        from_host: mpsc::Receiver<Vec<u8>>,
        cable_pulled: Arc<AtomicBool>,
        decoder: Decoder,
        sequence: u16,
    }

    impl Board {
        fn send(&mut self, message: Message) {
            let mut frame = [0u8; MAX_FRAME];
            let len = encode(self.sequence, &message, &mut frame).unwrap();
            self.sequence = self.sequence.wrapping_add(1);
            self.to_host.send(frame[..len].to_vec()).unwrap();
        }

        fn frame(&mut self) -> Frame {
            let deadline = Instant::now() + WAIT;
            loop {
                if let Some(frame) = self.decoder.next_frame() {
                    return frame;
                }
                let left = deadline.saturating_duration_since(Instant::now());
                let bytes = self
                    .from_host
                    .recv_timeout(left)
                    .expect("no frame from the host");
                assert!(self.decoder.push(&bytes));
            }
        }

        /// The next frame that is not the greeting the host repeats until it hears Hello.
        fn command(&mut self) -> Frame {
            loop {
                let frame = self.frame();
                if !matches!(frame.message, Message::HostHello { .. }) {
                    return frame;
                }
            }
        }
    }

    fn info() -> DeviceInfo {
        DeviceInfo {
            protocol: PROTOCOL_VERSION,
            firmware_major: 0,
            firmware_minor: 2,
            firmware_patch: 1,
            chip: Chip::Esp32S3,
            mac: [0xE8, 0x06, 0x90, 0x9D, 0x3E, 0x78],
            uptime_ms: 1500,
        }
    }

    fn started() -> (Link, Board, mpsc::Receiver<LinkEvent>) {
        let (to_host, from_board) = mpsc::channel();
        let (to_board, from_host) = mpsc::channel();
        let (events_tx, events) = mpsc::channel();
        let cable_pulled = Arc::new(AtomicBool::new(false));
        let link = Link::start(
            "fake",
            HostReader {
                from_board,
                pending: Vec::new(),
            },
            HostWriter {
                to_board,
                cable_pulled: cable_pulled.clone(),
            },
            move |event| {
                let _ = events_tx.send(event);
            },
        )
        .unwrap();
        let board = Board {
            to_host,
            from_host,
            cable_pulled,
            decoder: Decoder::new(),
            sequence: 0,
        };
        (link, board, events)
    }

    /// Past the handshake, so the host has stopped repeating HostHello.
    fn connected() -> (Link, Board, mpsc::Receiver<LinkEvent>) {
        let (link, mut board, events) = started();
        assert_eq!(
            board.frame().message,
            Message::HostHello {
                version: PROTOCOL_VERSION
            }
        );
        board.send(Message::Hello(info()));
        assert_eq!(events.recv_timeout(WAIT).unwrap(), LinkEvent::Hello(info()));
        (link, board, events)
    }

    #[test]
    fn the_host_greets_until_the_board_says_hello() {
        let (link, mut board, events) = started();
        assert_eq!(link.path(), "fake");
        let first = board.frame();
        let second = board.frame();
        let hello = Message::HostHello {
            version: PROTOCOL_VERSION,
        };
        assert_eq!((first.sequence, first.message), (0, hello));
        assert_eq!((second.sequence, second.message), (1, hello));
        assert!(!link.is_connected());

        board.send(Message::Hello(info()));
        assert_eq!(events.recv_timeout(WAIT).unwrap(), LinkEvent::Hello(info()));
        assert!(link.is_connected());
    }

    #[test]
    fn board_messages_arrive_as_events_in_order() {
        let (_link, mut board, events) = connected();
        let telemetry = Telemetry {
            uptime_ms: 2000,
            free_heap: 4096,
            temperature_centi_c: -150,
            flags: 1,
            detection_count: 2,
        };
        board.send(Message::Telemetry(telemetry));
        board.send(Message::Pong { id: 7 });
        board.send(Message::Ack {
            command: COMMAND_SET_LED,
        });
        board.send(Message::Button {
            pressed: true,
            count: 3,
        });
        board.send(Message::Error { code: 9 });
        board.send(Message::CameraDetection {
            count: 2,
            score_permille: 640,
        });

        let received: Vec<LinkEvent> = (0..6).map(|_| events.recv_timeout(WAIT).unwrap()).collect();
        assert_eq!(
            received,
            [
                LinkEvent::Telemetry(telemetry),
                LinkEvent::Pong { id: 7 },
                LinkEvent::Ack {
                    command: COMMAND_SET_LED
                },
                LinkEvent::Button {
                    pressed: true,
                    count: 3
                },
                LinkEvent::Error { code: 9 },
                LinkEvent::CameraDetection {
                    count: 2,
                    score_permille: 640
                },
            ]
        );
    }

    #[test]
    fn host_bound_messages_echoed_by_the_board_raise_no_event() {
        let (_link, mut board, events) = connected();
        board.send(Message::HostHello { version: 1 });
        board.send(Message::Ping { id: 1 });
        board.send(Message::SetLed {
            red: 1,
            green: 2,
            blue: 3,
            blink_ms: 0,
        });
        board.send(Message::Detection {
            count: 1,
            score_permille: 500,
        });
        board.send(Message::SetTelemetryPeriod { ms: 100 });
        board.send(Message::Pong { id: 2 });
        assert_eq!(
            events.recv_timeout(WAIT).unwrap(),
            LinkEvent::Pong { id: 2 }
        );
    }

    #[test]
    fn commands_reach_the_board_in_order_with_rising_sequence_numbers() {
        let (link, mut board, _events) = connected();
        assert!(link.set_led(255, 128, 0, 250));
        assert!(link.ping(41));
        assert!(link.set_telemetry_period(1000));
        assert!(link.send(Message::Ping { id: 42 }));

        let frames: Vec<Frame> = (0..4).map(|_| board.command()).collect();
        assert_eq!(
            frames.iter().map(|frame| frame.message).collect::<Vec<_>>(),
            [
                Message::SetLed {
                    red: 255,
                    green: 128,
                    blue: 0,
                    blink_ms: 250
                },
                Message::Ping { id: 41 },
                Message::SetTelemetryPeriod { ms: 1000 },
                Message::Ping { id: 42 },
            ]
        );
        assert!(frames
            .windows(2)
            .all(|pair| pair[1].sequence == pair[0].sequence.wrapping_add(1)));
    }

    #[test]
    fn a_detection_goes_out_in_per_mille_clamped_and_throttled() {
        let (link, mut board, _events) = connected();
        link.notify_detection(3, 0.9124);
        link.notify_detection(3, 0.95);
        assert!(link.ping(1));
        assert_eq!(
            board.command().message,
            Message::Detection {
                count: 3,
                score_permille: 912
            }
        );
        assert_eq!(board.command().message, Message::Ping { id: 1 });

        thread::sleep(Duration::from_millis(150));
        link.notify_detection(4, 1.7);
        thread::sleep(Duration::from_millis(150));
        link.notify_detection(0, -0.3);
        assert_eq!(
            board.command().message,
            Message::Detection {
                count: 4,
                score_permille: 1000
            }
        );
        assert_eq!(
            board.command().message,
            Message::Detection {
                count: 0,
                score_permille: 0
            }
        );
    }

    #[test]
    fn a_failed_write_disconnects_with_its_reason_and_stops_the_writer() {
        let (link, board, events) = connected();
        board.cable_pulled.store(true, Ordering::Release);
        assert!(link.ping(1));
        assert_eq!(
            events.recv_timeout(WAIT).unwrap(),
            LinkEvent::Disconnected("write failed: cable pulled".into())
        );
        assert!(!link.is_connected());

        let deadline = Instant::now() + WAIT;
        while link.ping(2) {
            assert!(Instant::now() < deadline, "the writer outlived its failure");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn an_unplugged_board_disconnects_with_the_read_error() {
        let (link, board, events) = connected();
        drop(board);
        assert_eq!(
            events.recv_timeout(WAIT).unwrap(),
            LinkEvent::Disconnected("read failed: unplugged".into())
        );
        assert!(!link.is_connected());
    }

    #[test]
    fn a_board_silent_for_six_seconds_is_disconnected() {
        let (link, _board, events) = connected();
        assert_eq!(
            events.recv_timeout(CONNECTION_IDLE - Duration::from_secs(1)),
            Err(RecvTimeoutError::Timeout)
        );
        assert_eq!(
            events.recv_timeout(WAIT).unwrap(),
            LinkEvent::Disconnected("no frames for 6 s; is the board still running?".into())
        );
        assert!(!link.is_connected());
    }

    #[test]
    fn close_stops_both_threads_and_can_repeat() {
        let (mut link, _board, events) = connected();
        link.close();
        assert!(!link.is_connected());
        assert!(!link.ping(1), "the writer outlived close()");
        assert_eq!(
            events.recv_timeout(WAIT),
            Err(RecvTimeoutError::Disconnected),
            "the reader outlived close()"
        );
        link.close();
    }

    #[test]
    fn dropping_the_link_stops_its_reader() {
        let (link, _board, events) = connected();
        drop(link);
        assert_eq!(
            events.recv_timeout(WAIT),
            Err(RecvTimeoutError::Disconnected)
        );
    }

    #[test]
    fn a_missing_port_names_itself() {
        let error = Link::open("/nonexistent/ttyESP32", |_| {})
            .err()
            .expect("a missing port opened");
        assert!(
            format!("{error:#}").contains("failed to open ESP32 port /nonexistent/ttyESP32"),
            "{error:#}"
        );
    }

    #[test]
    fn first_detection_frame_is_sent() {
        let mut throttle = DetectionThrottle::new();
        assert!(throttle.should_send(1, Instant::now()));
    }

    #[test]
    fn a_steady_count_is_repeated_only_at_the_interval() {
        let start = Instant::now();
        let mut throttle = DetectionThrottle::new();
        assert!(throttle.should_send(1, start));
        assert!(!throttle.should_send(1, start + Duration::from_millis(150)));
        assert!(throttle.should_send(1, start + Duration::from_millis(260)));
    }

    #[test]
    fn a_change_is_sent_but_never_faster_than_the_minimum_gap() {
        let start = Instant::now();
        let mut throttle = DetectionThrottle::new();
        assert!(throttle.should_send(0, start));
        assert!(!throttle.should_send(2, start + Duration::from_millis(50)));
        assert!(throttle.should_send(2, start + Duration::from_millis(120)));
    }

    #[test]
    fn zero_detections_are_repeated_once_a_second() {
        let start = Instant::now();
        let mut throttle = DetectionThrottle::new();
        assert!(throttle.should_send(0, start));
        assert!(!throttle.should_send(0, start + Duration::from_millis(500)));
        assert!(throttle.should_send(0, start + Duration::from_millis(1100)));
    }
}
