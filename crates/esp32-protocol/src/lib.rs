//! Framed, versioned USB-serial protocol between OmniAccelerANT and the ESP32-S3 companion.
//!
//! One frame is `A5 5A | version | seq (LE) | len (LE) | postcard payload | CRC-16/CCITT (LE)`.
//! The CRC covers version, sequence, length and payload. It is shared verbatim by the
//! firmware (`firmware/esp32-s3`), which is why this crate is `no_std`.

#![no_std]

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAGIC: [u8; 2] = [0xA5, 0x5A];
pub const HEADER_LEN: usize = 7;
pub const MAX_PAYLOAD: usize = 240;
pub const MAX_FRAME: usize = HEADER_LEN + MAX_PAYLOAD + 2;

/// Messages the host and the device exchange. Variants may be appended; changing one
/// or removing one is a protocol-version bump.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    /// Host -> device: the host is listening and speaks `version`.
    HostHello { version: u16 },
    /// Device -> host: identity and uptime, sent on boot until the host greets it.
    Hello(DeviceInfo),
    /// Host -> device.
    Ping { id: u32 },
    /// Device -> host.
    Pong { id: u32 },
    /// Device -> host, periodically after the handshake.
    Telemetry(Telemetry),
    /// Host -> device; `blink_ms` 0 keeps the colour until the next command.
    SetLed {
        red: u8,
        green: u8,
        blue: u8,
        blink_ms: u16,
    },
    /// Host -> device: the detector's latest frame. Sent at most a few times a second.
    Detection { count: u16, score_permille: u16 },
    /// Device -> host: the on-device camera detector's latest result.
    CameraDetection { count: u16, score_permille: u16 },
    /// Host -> device: telemetry period in milliseconds; 0 stops periodic telemetry.
    SetTelemetryPeriod { ms: u16 },
    /// Device -> host: the BOOT button changed.
    Button { pressed: bool, count: u32 },
    /// Device -> host: command accepted; `command` is one of the `COMMAND_*` values.
    Ack { command: u8 },
    /// Device -> host: command rejected.
    Error { code: u8 },
}

pub const COMMAND_HOST_HELLO: u8 = 1;
pub const COMMAND_PING: u8 = 2;
pub const COMMAND_SET_LED: u8 = 3;
pub const COMMAND_DETECTION: u8 = 4;
pub const COMMAND_SET_TELEMETRY_PERIOD: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Chip {
    Esp32S3,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub protocol: u16,
    pub firmware_major: u8,
    pub firmware_minor: u8,
    pub firmware_patch: u8,
    pub chip: Chip,
    /// Station MAC, so a UI can tell two boards apart.
    pub mac: [u8; 6],
    pub uptime_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Telemetry {
    pub uptime_ms: u32,
    /// Bytes still available in the firmware's heap; 0 when it uses none.
    pub free_heap: u32,
    /// Internal temperature sensor, hundredths of a degree Celsius.
    pub temperature_centi_c: i16,
    pub flags: u8,
    /// Number of detections in the last `Detection` the device saw.
    pub detection_count: u16,
}

/// Bit 0 of [`Telemetry::flags`]: the BOOT button is held down.
pub const TELEMETRY_BUTTON_DOWN: u8 = 1 << 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub sequence: u16,
    pub message: Message,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    Serialize,
    PayloadTooLarge,
    BufferTooSmall,
}

/// Encodes one frame into `out`, returning its length.
pub fn encode(sequence: u16, message: &Message, out: &mut [u8]) -> Result<usize, EncodeError> {
    let mut payload = [0u8; MAX_PAYLOAD];
    let written = postcard::to_slice(message, &mut payload)
        .map_err(|_| EncodeError::Serialize)?
        .len();
    if written > MAX_PAYLOAD {
        return Err(EncodeError::PayloadTooLarge);
    }

    let total = HEADER_LEN + written + 2;
    if out.len() < total {
        return Err(EncodeError::BufferTooSmall);
    }

    out[0..2].copy_from_slice(&MAGIC);
    out[2] = PROTOCOL_VERSION as u8;
    out[3..5].copy_from_slice(&sequence.to_le_bytes());
    out[5..7].copy_from_slice(&(written as u16).to_le_bytes());
    out[HEADER_LEN..HEADER_LEN + written].copy_from_slice(&payload[..written]);
    let crc = crc16(&out[2..HEADER_LEN + written]);
    out[HEADER_LEN + written..total].copy_from_slice(&crc.to_le_bytes());
    Ok(total)
}

/// CRC-16/CCITT-FALSE: poly 0x1021, init 0xFFFF, no reflection, xorout 0.
pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &byte in bytes {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Incremental frame parser: push bytes, poll for whole frames. Noise (panic output,
/// boot logs, stale bytes) is discarded until the next magic pair.
pub struct Decoder {
    buf: [u8; 1024],
    len: usize,
}

impl Decoder {
    pub const fn new() -> Self {
        Self {
            buf: [0; 1024],
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Appends bytes. Returns false when the buffer cannot hold them all; drain with
    /// [`Decoder::next`] first.
    pub fn push(&mut self, bytes: &[u8]) -> bool {
        if bytes.len() > self.buf.len() - self.len {
            return false;
        }
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        true
    }

    /// Returns the next complete, valid frame, or `None` while more bytes are needed.
    pub fn next_frame(&mut self) -> Option<Frame> {
        loop {
            let start = self.find_magic();
            let Some(start) = start else {
                self.keep_possible_magic();
                return None;
            };
            if start > 0 {
                self.discard(start);
            }
            if self.len < HEADER_LEN {
                return None;
            }

            if self.buf[2] as u16 != PROTOCOL_VERSION {
                self.discard(1);
                continue;
            }
            let payload_len = u16::from_le_bytes([self.buf[5], self.buf[6]]) as usize;
            if payload_len > MAX_PAYLOAD {
                self.discard(1);
                continue;
            }
            let total = HEADER_LEN + payload_len + 2;
            if self.len < total {
                return None;
            }

            let crc = u16::from_le_bytes([self.buf[total - 2], self.buf[total - 1]]);
            if crc != crc16(&self.buf[2..HEADER_LEN + payload_len]) {
                self.discard(1);
                continue;
            }

            match postcard::from_bytes::<Message>(&self.buf[HEADER_LEN..HEADER_LEN + payload_len]) {
                Ok(message) => {
                    let sequence = u16::from_le_bytes([self.buf[3], self.buf[4]]);
                    self.discard(total);
                    return Some(Frame { sequence, message });
                }
                Err(_) => self.discard(1),
            }
        }
    }

    fn find_magic(&self) -> Option<usize> {
        if self.len < 2 {
            return None;
        }
        (0..self.len - 1).find(|&i| self.buf[i] == MAGIC[0] && self.buf[i + 1] == MAGIC[1])
    }

    fn keep_possible_magic(&mut self) {
        if self.len > 0 && self.buf[self.len - 1] == MAGIC[0] {
            self.buf[0] = MAGIC[0];
            self.len = 1;
        } else {
            self.len = 0;
        }
    }

    fn discard(&mut self, count: usize) {
        let count = count.min(self.len);
        self.buf.copy_within(count..self.len, 0);
        self.len -= count;
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> DeviceInfo {
        DeviceInfo {
            protocol: PROTOCOL_VERSION,
            firmware_major: 0,
            firmware_minor: 1,
            firmware_patch: 0,
            chip: Chip::Esp32S3,
            mac: [0xE8, 0x06, 0x90, 0x9D, 0x3E, 0x78],
            uptime_ms: 42,
        }
    }

    fn all_messages() -> [Message; 9] {
        [
            Message::HostHello {
                version: PROTOCOL_VERSION,
            },
            Message::Hello(info()),
            Message::Ping { id: 7 },
            Message::Pong { id: 7 },
            Message::Telemetry(Telemetry {
                uptime_ms: 1234,
                free_heap: 4096,
                temperature_centi_c: 4210,
                flags: TELEMETRY_BUTTON_DOWN,
                detection_count: 2,
            }),
            Message::SetLed {
                red: 1,
                green: 2,
                blue: 3,
                blink_ms: 500,
            },
            Message::Detection {
                count: 3,
                score_permille: 912,
            },
            Message::CameraDetection {
                count: 1,
                score_permille: 870,
            },
            Message::Button {
                pressed: true,
                count: 4,
            },
        ]
    }

    #[test]
    fn crc_vector_matches_the_ccitt_false_check_value() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    #[test]
    fn every_message_round_trips() {
        for message in all_messages() {
            let mut buf = [0u8; MAX_FRAME];
            let len = encode(9, &message, &mut buf).unwrap();
            let mut decoder = Decoder::new();
            assert!(decoder.push(&buf[..len]));
            assert_eq!(
                decoder.next_frame(),
                Some(Frame {
                    sequence: 9,
                    message
                })
            );
            assert!(decoder.is_empty());
        }
    }

    #[test]
    fn noise_before_a_frame_is_discarded() {
        let mut buf = [0u8; MAX_FRAME];
        let len = encode(1, &Message::Ping { id: 3 }, &mut buf).unwrap();
        let mut decoder = Decoder::new();
        assert!(decoder.push(b"boot log\r\nA5"));
        assert_eq!(decoder.next_frame(), None);
        assert!(decoder.push(&buf[..len]));
        assert_eq!(
            decoder.next_frame(),
            Some(Frame {
                sequence: 1,
                message: Message::Ping { id: 3 }
            })
        );
    }

    #[test]
    fn a_truncated_frame_waits_for_the_rest() {
        let mut buf = [0u8; MAX_FRAME];
        let len = encode(2, &Message::Pong { id: 9 }, &mut buf).unwrap();
        let mut decoder = Decoder::new();
        assert!(decoder.push(&buf[..len - 1]));
        assert_eq!(decoder.next_frame(), None);
        assert!(decoder.push(&buf[len - 1..len]));
        assert!(decoder.next_frame().is_some());
    }

    #[test]
    fn a_corrupt_frame_is_skipped_and_the_next_survives() {
        let mut first = [0u8; MAX_FRAME];
        let first_len = encode(3, &Message::Ping { id: 1 }, &mut first).unwrap();
        first[HEADER_LEN] ^= 0xFF;
        let mut second = [0u8; MAX_FRAME];
        let second_len = encode(4, &Message::Pong { id: 2 }, &mut second).unwrap();

        let mut decoder = Decoder::new();
        assert!(decoder.push(&first[..first_len]));
        assert!(decoder.push(&second[..second_len]));
        assert_eq!(
            decoder.next_frame(),
            Some(Frame {
                sequence: 4,
                message: Message::Pong { id: 2 }
            })
        );
    }

    #[test]
    fn a_payload_larger_than_the_buffer_is_refused() {
        let mut small = [0u8; 8];
        assert_eq!(
            encode(0, &Message::Ping { id: 1 }, &mut small),
            Err(EncodeError::BufferTooSmall)
        );
    }
}
