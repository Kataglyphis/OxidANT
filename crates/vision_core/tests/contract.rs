//! ModelANT's `contracts/inference-v1.json`, the cases every detector of the family replays, on their raw bytes.

use kataglyphis_vision_core::preprocess::LETTERBOX_FILL_VALUE;
use kataglyphis_vision_core::{
    parse_yolo_like_detections, preprocess_frame_into, Frame, ImageMapping, PixelFormat, ResizeMode,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn contract() -> Value {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../third_party/ModelANT/contracts/inference-v1.json"
    ));
    let contract: Value = serde_json::from_str(text).unwrap();
    assert_eq!(contract["schema_version"], 1);
    contract
}

fn size(item: &Value, key: &str) -> usize {
    item[key].as_u64().unwrap() as usize
}

fn format_of(item: &Value) -> PixelFormat {
    match item["pixel_format"].as_str().unwrap() {
        "RGB" => PixelFormat::Rgb,
        "BGR" => PixelFormat::Bgr,
        "RGBA" => PixelFormat::Rgba,
        other => panic!("{}: no such pixel format {other}", item["name"]),
    }
}

/// The case's bytes as the camera hands them over, stride and offset included, through the production entry point.
fn preprocessed(item: &Value, data: &[u8], format: PixelFormat) -> (Vec<f32>, ImageMapping) {
    let frame = Frame {
        data,
        format,
        width: size(item, "width") as u32,
        height: size(item, "height") as u32,
        row_stride_bytes: size(item, "row_stride_bytes"),
        plane_offset_bytes: size(item, "plane_offset_bytes"),
    };
    let mode = if item["mode"] == "letterbox" {
        ResizeMode::Letterbox
    } else {
        ResizeMode::Stretch
    };
    let mut output = Vec::new();
    let (input_w, input_h) = (
        size(item, "input_width") as u32,
        size(item, "input_height") as u32,
    );
    let mapping = preprocess_frame_into(&frame, input_w, input_h, mode, &mut output).unwrap();
    (output, mapping)
}

fn assert_mapping(item: &Value, mapping: ImageMapping) {
    for (actual, key) in [
        (mapping.scale_x, "scale_x"),
        (mapping.scale_y, "scale_y"),
        (mapping.pad_x, "pad_x"),
        (mapping.pad_y, "pad_y"),
    ] {
        let expected = item["mapping"][key].as_f64().unwrap() as f32;
        assert!((actual - expected).abs() < 1e-6, "{} {key}", item["name"]);
    }
}

#[test]
fn the_tensor_cases_replay_on_their_raw_bytes() {
    let contract = contract();
    let fill = contract["letterbox_fill"].as_f64().unwrap() as f32;
    assert_eq!(fill.to_bits(), LETTERBOX_FILL_VALUE.to_bits());
    let cases = contract["cases"].as_array().unwrap();
    assert!(cases.len() >= 4, "the tensor cases shrank: {}", cases.len());
    for item in cases {
        let data: Vec<u8> = item["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_u64().unwrap() as u8)
            .collect();
        let (output, mapping) = preprocessed(item, &data, format_of(item));
        let expected = item["expected_nchw"].as_array().unwrap();
        assert_eq!(output.len(), expected.len(), "{}", item["name"]);
        for (actual, expected) in output.iter().zip(expected) {
            let expected = expected.as_f64().unwrap() as f32;
            assert!((actual - expected).abs() < 1e-6, "{}", item["name"]);
        }
        assert_mapping(item, mapping);
    }
}

/// digest_cases.frame: each byte from x, y and the channel, 171 in every padding and offset byte.
fn digest_frame(item: &Value) -> Vec<u8> {
    let channels = item["pixel_format"].as_str().unwrap().len();
    let (offset, stride) = (
        size(item, "plane_offset_bytes"),
        size(item, "row_stride_bytes"),
    );
    let mut data = vec![171_u8; offset + stride * size(item, "height")];
    for y in 0..size(item, "height") {
        for x in 0..size(item, "width") {
            for c in 0..channels {
                data[offset + y * stride + x * channels + c] =
                    ((7 * x + 13 * y + 61 * c) % 256) as u8;
            }
        }
    }
    data
}

fn sha256(output: &[f32]) -> String {
    let mut digest = Sha256::new();
    for value in output {
        digest.update(value.to_le_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn assert_digest(item: &Value, output: &[f32], label: &str) {
    let (width, height) = (size(item, "input_width"), size(item, "input_height"));
    for sample in item["samples"].as_array().unwrap() {
        let at = |i: usize| sample[i].as_u64().unwrap() as usize;
        let value = output[at(0) * width * height + at(1) * width + at(2)];
        let expected = sample[3].as_f64().unwrap() as f32;
        assert_eq!(
            value.to_bits(),
            expected.to_bits(),
            "{} {label} {sample}",
            item["name"]
        );
    }
    assert_eq!(
        sha256(output),
        item["expected_sha256"].as_str().unwrap(),
        "{} {label}",
        item["name"]
    );
}

/// ModelANT's full-size frames (hub CON288 (e)): the tensor bit for bit, by its samples and its SHA-256.
#[test]
fn full_size_frames_match_their_digest_bit_for_bit() {
    let contract = contract();
    let cases = contract["digest_cases"]["cases"].as_array().unwrap();
    assert!(cases.len() >= 4, "the digest cases shrank: {}", cases.len());
    for item in cases {
        let (output, mapping) = preprocessed(item, &digest_frame(item), format_of(item));
        assert_digest(item, &output, "as written");
        assert_mapping(item, mapping);
    }
}

/// The RGB frames with red and blue swapped in every pixel, read as BGR: the same tensor, bit for bit.
#[test]
fn a_bgr_frame_gives_the_digest_of_its_rgb_twin() {
    let contract = contract();
    for item in contract["digest_cases"]["cases"].as_array().unwrap() {
        if format_of(item) != PixelFormat::Rgb {
            continue;
        }
        let mut data = digest_frame(item);
        let (offset, stride) = (
            size(item, "plane_offset_bytes"),
            size(item, "row_stride_bytes"),
        );
        for y in 0..size(item, "height") {
            for x in 0..size(item, "width") {
                let at = offset + y * stride + x * 3;
                data.swap(at, at + 2);
            }
        }
        let (output, _) = preprocessed(item, &data, PixelFormat::Bgr);
        assert_digest(item, &output, "as BGR");
    }
}

/// A row's number; JSON has no NaN or infinity, so the contract spells them as strings that `f32` parses.
fn number(value: &Value) -> f32 {
    match value.as_str() {
        Some(text) => text.parse().unwrap(),
        None => value.as_f64().unwrap() as f32,
    }
}

fn case_mapping(case: &Value) -> ImageMapping {
    let map = |key: &str| case["mapping"][key].as_f64().unwrap() as f32;
    ImageMapping {
        src_w: size(case, "width") as u32,
        src_h: size(case, "height") as u32,
        scale_x: map("scale_x"),
        scale_y: map("scale_y"),
        pad_x: map("pad_x"),
        pad_y: map("pad_y"),
    }
}

#[test]
fn the_decode_cases_replay_with_the_contracts_class_count() {
    let contract = contract();
    let decode = &contract["decode"];
    assert_eq!(decode["layout"], "xyxy_score_class");
    assert_eq!(decode["order"], "row");
    let range = &contract["output"]["class_range"];
    let class_count = (range[1].as_u64().unwrap() - range[0].as_u64().unwrap() + 1) as usize;
    let score_tolerance = decode["score_tolerance"].as_f64().unwrap() as f32;
    let coordinate_tolerance = decode["coordinate_tolerance"].as_f64().unwrap() as f32;
    let cases = decode["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let name = &case["name"];
        let rows = case["rows"].as_array().unwrap();
        let data: Vec<f32> = rows
            .iter()
            .flat_map(|row| row.as_array().unwrap().iter().map(number))
            .collect();
        let threshold = case["score_threshold"].as_f64().unwrap() as f32;
        let shape = [1, rows.len(), 6];
        let found = parse_yolo_like_detections(
            &shape,
            &data,
            threshold,
            case_mapping(case),
            false,
            class_count,
        )
        .unwrap();
        let expected = case["expected"].as_array().unwrap();
        assert_eq!(found.len(), expected.len(), "{name}");
        // In row order: the contract compares the list entry by entry.
        for (found, expected) in found.iter().zip(expected) {
            assert_eq!(
                found.class_id,
                expected["class"].as_i64().unwrap(),
                "{name}"
            );
            let score = expected["score"].as_f64().unwrap() as f32;
            assert!((found.score - score).abs() <= score_tolerance, "{name}");
            let corners = [found.x1, found.y1, found.x2, found.y2];
            for (corner, expected) in corners.iter().zip(expected["box"].as_array().unwrap()) {
                let expected = expected.as_f64().unwrap() as f32;
                assert!(
                    (corner - expected).abs() <= coordinate_tolerance,
                    "{name}: {corners:?}"
                );
            }
        }
    }
}
