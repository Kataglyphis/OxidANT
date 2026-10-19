//! Properties of the frame codec that the host and the firmware both rely on.

use kataglyphis_esp32_proto::{
    encode, Chip, Decoder, DeviceInfo, Frame, Message, Telemetry, MAGIC, MAX_FRAME,
};
use proptest::prelude::*;
use proptest::sample::Index;

fn device_info() -> impl Strategy<Value = DeviceInfo> {
    (
        any::<u16>(),
        any::<(u8, u8, u8)>(),
        any::<bool>(),
        any::<[u8; 6]>(),
        any::<u32>(),
    )
        .prop_map(
            |(protocol, (major, minor, patch), s3, mac, uptime_ms)| DeviceInfo {
                protocol,
                firmware_major: major,
                firmware_minor: minor,
                firmware_patch: patch,
                chip: if s3 { Chip::Esp32S3 } else { Chip::Other },
                mac,
                uptime_ms,
            },
        )
}

fn telemetry() -> impl Strategy<Value = Telemetry> {
    any::<(u32, u32, i16, u8, u16)>().prop_map(
        |(uptime_ms, free_heap, temperature_centi_c, flags, detection_count)| Telemetry {
            uptime_ms,
            free_heap,
            temperature_centi_c,
            flags,
            detection_count,
        },
    )
}

fn message() -> impl Strategy<Value = Message> {
    prop_oneof![
        any::<u16>().prop_map(|version| Message::HostHello { version }),
        device_info().prop_map(Message::Hello),
        any::<u32>().prop_map(|id| Message::Ping { id }),
        any::<u32>().prop_map(|id| Message::Pong { id }),
        telemetry().prop_map(Message::Telemetry),
        any::<(u8, u8, u8, u16)>().prop_map(|(red, green, blue, blink_ms)| Message::SetLed {
            red,
            green,
            blue,
            blink_ms
        }),
        any::<(u16, u16)>().prop_map(|(count, score_permille)| Message::Detection {
            count,
            score_permille
        }),
        any::<(u16, u16)>().prop_map(|(count, score_permille)| Message::CameraDetection {
            count,
            score_permille
        }),
        any::<u16>().prop_map(|ms| Message::SetTelemetryPeriod { ms }),
        any::<(bool, u32)>().prop_map(|(pressed, count)| Message::Button { pressed, count }),
        any::<u8>().prop_map(|command| Message::Ack { command }),
        any::<u8>().prop_map(|code| Message::Error { code }),
    ]
}

fn encoded(sequence: u16, message: &Message) -> Vec<u8> {
    let mut buf = [0u8; MAX_FRAME];
    let len = encode(sequence, message, &mut buf).expect("every message fits one frame");
    buf[..len].to_vec()
}

proptest! {
    #[test]
    fn any_frame_survives_any_split_into_reads(
        sequence: u16,
        message in message(),
        cuts in prop::collection::vec(any::<Index>(), 0..8),
    ) {
        let bytes = encoded(sequence, &message);
        let mut ends: Vec<usize> = cuts.iter().map(|cut| cut.index(bytes.len())).collect();
        ends.push(bytes.len());
        ends.sort_unstable();

        let mut decoder = Decoder::new();
        let mut frames = Vec::new();
        let mut start = 0;
        for end in ends {
            prop_assert!(decoder.push(&bytes[start..end]));
            frames.extend(decoder.next_frame());
            start = end;
        }
        prop_assert_eq!(frames, vec![Frame { sequence, message }]);
        prop_assert!(decoder.is_empty());
    }

    #[test]
    fn noise_without_the_magic_pair_is_dropped(
        noise in prop::collection::vec(any::<u8>(), 0..512)
            .prop_filter("no magic pair", |noise| !noise.windows(2).any(|pair| pair == MAGIC)),
        sequence: u16,
        message in message(),
    ) {
        let mut decoder = Decoder::new();
        prop_assert!(decoder.push(&noise));
        prop_assert_eq!(decoder.next_frame(), None);
        prop_assert!(decoder.push(&encoded(sequence, &message)));
        prop_assert_eq!(decoder.next_frame(), Some(Frame { sequence, message }));
    }

    // Bytes 5 and 6 (the length) are left out: a shortened frame meets its CRC in other bytes, 1 in 65536.
    #[test]
    fn a_single_bit_error_outside_the_length_is_never_delivered(
        sequence: u16,
        message in message(),
        bit in any::<Index>(),
    ) {
        let mut bytes = encoded(sequence, &message);
        let bits: Vec<usize> = (0..bytes.len() * 8).filter(|bit| !(40..56).contains(bit)).collect();
        let flipped = bits[bit.index(bits.len())];
        bytes[flipped / 8] ^= 1 << (flipped % 8);

        let mut decoder = Decoder::new();
        prop_assert!(decoder.push(&bytes));
        prop_assert_eq!(decoder.next_frame(), None);
    }
}
