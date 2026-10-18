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
        let mut port = serialport::new(path, 115_200)
            .timeout(READ_TIMEOUT)
            .open()
            .with_context(|| format!("failed to open ESP32 port {path}"))?;
        let mut reader_port = port
            .try_clone()
            .context("failed to clone the ESP32 serial port for reading")?;

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
    port: &mut Box<dyn serialport::SerialPort>,
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
    port: &mut Box<dyn serialport::SerialPort>,
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
    use super::*;

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
