//! The native half of the browser module's contract check: ModelANT's `inference-v1.json` through this crate's
//! exports, each result hashed, printed as JSON for `scripts/linux/vision-core-web-contract.mjs` to compare with wasm.
//!
//! `cargo run -p kataglyphis_vision_core_web --example contract_report -- <inference-v1.json>`

use kataglyphis_vision_core_web::{class_names, decode, Mapping, Preprocessor, Refusal};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: contract_report <inference-v1.json>");
    let text = std::fs::read_to_string(&path).expect("the contract file");
    let contract: Value = serde_json::from_str(&text).expect("the contract's JSON");
    let mut report = Map::new();
    tensor_cases(&contract, &mut report);
    digest_cases(&contract, &mut report);
    decode_cases(&contract, &mut report);
    refusals(&mut report);
    let names = class_names().join("\n");
    report.insert("classNames".into(), json!(hex(Sha256::digest(names))));
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("the report")
    );
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// The SHA-256 of the values' little-endian bytes, as the browser hashes a typed array.
fn sha256<const N: usize, T: Copy>(values: &[T], bytes: fn(T) -> [u8; N]) -> String {
    let mut digest = Sha256::new();
    for &value in values {
        digest.update(bytes(value));
    }
    hex(digest.finalize())
}

fn size(item: &Value, key: &str) -> usize {
    item[key].as_u64().expect("a size") as usize
}

fn mapping_bits(m: &Mapping) -> Value {
    json!([
        m.src_width,
        m.src_height,
        m.scale_x.to_bits(),
        m.scale_y.to_bits(),
        m.pad_x.to_bits(),
        m.pad_y.to_bits()
    ])
}

/// One case through `Preprocessor`: the tensor's hash and length and the mapping's bits.
fn preprocessed(item: &Value, data: &[u8], format: &str) -> Value {
    let (input_w, input_h) = (size(item, "input_width"), size(item, "input_height"));
    let mode = item["mode"].as_str().expect("a mode");
    let mut pre = Preprocessor::new(input_w as u32, input_h as u32, mode).expect("a preprocessor");
    let (width, height) = (size(item, "width") as u32, size(item, "height") as u32);
    let (stride, offset) = (
        size(item, "row_stride_bytes"),
        size(item, "plane_offset_bytes"),
    );
    let mapping = pre
        .run(data, format, width, height, stride, offset)
        .expect("a tensor");
    let tensor = pre.tensor();
    json!({"tensor": sha256(&tensor, f32::to_le_bytes), "length": tensor.len(), "mapping": mapping_bits(&mapping)})
}

fn tensor_cases(contract: &Value, report: &mut Map<String, Value>) {
    for item in contract["cases"].as_array().expect("cases") {
        let data: Vec<u8> = item["data"]
            .as_array()
            .expect("data")
            .iter()
            .map(|b| b.as_u64().expect("a byte") as u8)
            .collect();
        let format = item["pixel_format"].as_str().expect("a format");
        let name = item["name"].as_str().expect("a name");
        report.insert(format!("tensor/{name}"), preprocessed(item, &data, format));
    }
}

/// digest_cases.frame: each byte from x, y and the channel, 171 in every padding and offset byte.
fn digest_frame(item: &Value, channels: usize) -> Vec<u8> {
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

fn digest_cases(contract: &Value, report: &mut Map<String, Value>) {
    for item in contract["digest_cases"]["cases"]
        .as_array()
        .expect("digest cases")
    {
        let format = item["pixel_format"].as_str().expect("a format");
        let name = item["name"].as_str().expect("a name");
        let mut data = digest_frame(item, format.len());
        report.insert(format!("digest/{name}"), preprocessed(item, &data, format));
        if format != "RGB" {
            continue;
        }
        // The same frame with red and blue swapped, read as BGR.
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
        report.insert(
            format!("digest/{name}/bgr"),
            preprocessed(item, &data, "BGR"),
        );
    }
}

/// A row's number; JSON has no NaN or infinity, so the contract spells them as strings.
fn number(value: &Value) -> f32 {
    match value.as_str() {
        Some(text) => text.parse().expect("a number's spelling"),
        None => value.as_f64().expect("a number") as f32,
    }
}

fn decode_cases(contract: &Value, report: &mut Map<String, Value>) {
    let range = &contract["output"]["class_range"];
    let class = |i: usize| range[i].as_u64().expect("a class");
    let class_count = (class(1) - class(0) + 1) as u32;
    for case in contract["decode"]["cases"]
        .as_array()
        .expect("decode cases")
    {
        let rows = case["rows"].as_array().expect("rows");
        let data: Vec<f32> = rows
            .iter()
            .flat_map(|row| row.as_array().expect("a row").iter().map(number))
            .collect();
        let map = |key: &str| case["mapping"][key].as_f64().expect("a mapping value") as f32;
        let (width, height) = (size(case, "width") as u32, size(case, "height") as u32);
        let mapping = Mapping::new(
            width,
            height,
            map("scale_x"),
            map("scale_y"),
            map("pad_x"),
            map("pad_y"),
        );
        let threshold = case["score_threshold"].as_f64().expect("a threshold") as f32;
        let shape = [1, rows.len() as u32, 6];
        let found =
            decode(&shape, &data, threshold, &mapping, false, class_count).expect("detections");
        let name = case["name"].as_str().expect("a name");
        report.insert(
            format!("decode/{name}"),
            json!({"rows": sha256(&found, f64::to_le_bytes), "count": found.len() / 6}),
        );
    }
}

fn refused(result: Result<impl Sized, Refusal>) -> Value {
    match result {
        Ok(_) => json!(null),
        Err(refusal) => json!({"kind": refusal.kind, "message": refusal.message}),
    }
}

/// The same refusals `vision-core-web-contract.mjs` provokes, so both sides name and word them alike.
fn refusals(report: &mut Map<String, Value>) {
    let frame = [7_u8; 64];
    let mut pre = Preprocessor::new(4, 4, "stretch").expect("a preprocessor");
    let mut zero_input = Preprocessor::new(0, 4, "stretch").expect("a preprocessor");
    let mapping = Mapping::new(100, 100, 1.0, 1.0, 0.0, 0.0);
    let cases = [
        (
            "buffer-too-short",
            refused(pre.run(&frame, "RGBA", 4, 2, 20, 29)),
        ),
        (
            "stride-shorter-than-row",
            refused(pre.run(&frame, "RGB", 4, 2, 11, 0)),
        ),
        ("zero-size-frame", refused(pre.run(&[], "RGB", 0, 2, 0, 0))),
        (
            "zero-size-input",
            refused(zero_input.run(&frame, "RGB", 4, 2, 0, 0)),
        ),
        (
            "unknown-pixel-format",
            refused(pre.run(&frame, "YUV", 4, 2, 0, 0)),
        ),
        (
            "unknown-resize-mode",
            refused(Preprocessor::new(4, 4, "fit")),
        ),
        (
            "too-few-columns",
            refused(decode(&[1, 4], &[0.0; 4], 0.5, &mapping, false, 80)),
        ),
        (
            "batch-not-one",
            refused(decode(&[2, 1, 6], &[0.0; 12], 0.5, &mapping, false, 80)),
        ),
        (
            "unsupported-rank",
            refused(decode(&[6], &[0.0; 6], 0.5, &mapping, false, 80)),
        ),
        (
            "shorter-than-shape",
            refused(decode(&[2, 6], &[0.0; 6], 0.5, &mapping, false, 80)),
        ),
    ];
    for (name, value) in cases {
        report.insert(format!("refusal/{name}"), value);
    }
}
