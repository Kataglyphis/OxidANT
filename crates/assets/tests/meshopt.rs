//! EXT_meshopt_compression against gltfpack's originals and meshoptimizer's own streams; provenance: docs/testing.md.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use gltf::json::Value;
use kataglyphis_assets::flat::{load_flat, FlatModel, FlatVertex};
use kataglyphis_assets::meshopt_codec as codec;
use kataglyphis_assets::scene::ChannelValues;
use kataglyphis_assets::{load_gltf, load_gltf_slice, LoadOptions};
use proptest::prelude::*;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets")
        .join(name)
}

/// A packed fixture, its original, and how far each attribute may move: the filter's or the quantization's step.
struct Twin {
    packed: &'static str,
    original: &'static str,
    position: f32,
    normal: f32,
    /// `None` where gltfpack quantized UVs for a material with no texture to carry their transform.
    uv: Option<f32>,
}

const TWINS: [Twin; 3] = [
    // Floats through the attribute and triangle codecs alone: every bit survives.
    Twin {
        packed: "cube_meshopt.glb",
        original: "cube.gltf",
        position: 0.0,
        normal: 0.0,
        uv: Some(0.0),
    },
    // EXPONENTIAL keeps 15 mantissa bits of a position; OCTAHEDRAL an 8-bit normal.
    Twin {
        packed: "cube_meshopt_filters.glb",
        original: "cube.gltf",
        position: 2e-5,
        normal: 1e-2,
        uv: None,
    },
    // 14-bit positions dequantized by the node, bent by the joints at rest; 8-bit OCTAHEDRAL normals, two steps.
    Twin {
        packed: "skinned_bar_meshopt.glb",
        original: "skinned_bar.gltf",
        position: 2e-4,
        normal: 1.5e-2,
        uv: None,
    },
];

fn triangles(model: &FlatModel) -> Vec<[FlatVertex; 3]> {
    model
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| t.map(|index| model.vertices[index as usize]))
        .collect()
}

fn near<const N: usize>(a: [f32; N], b: [f32; N], tolerance: f32) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() <= tolerance)
}

fn distance<const N: usize>(a: [f32; N], b: [f32; N]) -> f32 {
    a.iter()
        .zip(b)
        .fold(0f32, |worst, (a, b)| worst.max((a - b).abs()))
}

fn corner_matches(a: &FlatVertex, b: &FlatVertex, twin: &Twin) -> bool {
    near(a.position, b.position, twin.position)
        && near(a.normal, b.normal, twin.normal)
        && twin
            .uv
            .is_none_or(|tolerance| near(a.uv0, b.uv0, tolerance))
}

/// The same triangle, wound the same way, whichever corner the index codec put first.
fn triangle_matches(a: &[FlatVertex; 3], b: &[FlatVertex; 3], twin: &Twin) -> bool {
    (0..3).any(|turn| (0..3).all(|k| corner_matches(&a[(k + turn) % 3], &b[k], twin)))
}

#[test]
fn each_packed_fixture_draws_its_originals_triangles() {
    for twin in &TWINS {
        let packed = load_flat(fixture(twin.packed), &LoadOptions::default()).unwrap();
        let original = load_flat(fixture(twin.original), &LoadOptions::default()).unwrap();
        let mut unmatched = triangles(&packed);
        let expected = triangles(&original);
        assert_eq!(unmatched.len(), expected.len(), "{}", twin.packed);
        let mut worst = (0f32, 0f32);
        for (t, triangle) in expected.iter().enumerate() {
            let found = unmatched
                .iter()
                .position(|candidate| triangle_matches(candidate, triangle, twin));
            let found = found
                .unwrap_or_else(|| panic!("{}: triangle {t} {triangle:?} is lost", twin.packed));
            for (a, b) in unmatched
                .swap_remove(found)
                .iter()
                .flat_map(|a| triangle.iter().map(move |b| (a, b)))
            {
                if corner_matches(a, b, twin) {
                    worst.0 = worst.0.max(distance(a.position, b.position));
                    worst.1 = worst.1.max(distance(a.normal, b.normal));
                }
            }
        }
        assert_eq!(
            packed.materials.len(),
            original.materials.len(),
            "{}",
            twin.packed
        );
        eprintln!(
            "{}: positions within {:e}, normals within {:e}",
            twin.packed, worst.0, worst.1
        );
    }
}

#[test]
fn quaternion_keyframes_keep_the_originals_rotations() {
    let packed = load_gltf(fixture("skinned_bar_meshopt.glb")).unwrap();
    let original = load_gltf(fixture("skinned_bar.gltf")).unwrap();
    let channel = |scene: &kataglyphis_assets::CpuScene| {
        let channel = scene.animations[0].channels[0].clone();
        let ChannelValues::Rotation(rotations) = channel.values else {
            panic!("the bend is a rotation");
        };
        (channel.times, rotations)
    };
    let (times, rotations) = channel(&packed);
    let (keyframes, expected) = channel(&original);
    assert_eq!(
        times.len(),
        61,
        "gltfpack resamples the two seconds at 30 Hz"
    );
    for rotation in &rotations {
        assert!(
            (rotation.length() - 1.0).abs() < 1e-3,
            "{rotation:?} is not a unit quaternion"
        );
    }
    for (time, expected) in keyframes.iter().zip(expected) {
        let at = times.iter().position(|t| (t - time).abs() < 1e-4);
        let at = at.unwrap_or_else(|| panic!("no keyframe at {time} s"));
        // 12 bits per component, scaled by sqrt(2): about 3.5e-4 at most.
        let rotation = rotations[at];
        assert!(
            near(rotation.to_array(), expected.to_array(), 1e-3),
            "{time} s: {rotation:?} against {expected:?}"
        );
    }
}

/// A GLB's JSON and binary chunk, edited and written back.
fn edited_glb(name: &str, edit: impl FnOnce(&mut Value, &mut Vec<u8>)) -> Vec<u8> {
    let bytes = std::fs::read(fixture(name)).unwrap();
    let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let mut json: Value = gltf::json::deserialize::from_slice(&glb.json).unwrap();
    let mut bin = glb.bin.unwrap().into_owned();
    edit(&mut json, &mut bin);
    let json = gltf::json::serialize::to_vec(&json).unwrap();
    let glb = gltf::binary::Glb {
        header: glb.header,
        json: Cow::Owned(json),
        bin: Some(Cow::Owned(bin)),
    };
    glb.to_vec().unwrap()
}

fn positions(bytes: &[u8]) -> Vec<[f32; 3]> {
    let scene = load_gltf_slice(bytes).unwrap();
    scene.primitives[0]
        .vertices
        .iter()
        .map(|v| v.position)
        .collect()
}

#[test]
fn a_placeholder_may_go_untagged_and_the_buffers_may_be_embedded() {
    let expected = positions(&std::fs::read(fixture("cube_meshopt.glb")).unwrap());
    let untagged = edited_glb("cube_meshopt.glb", |json, _| {
        json["buffers"][1]
            .as_object_mut()
            .unwrap()
            .remove("extensions");
    });
    assert_eq!(
        positions(&untagged),
        expected,
        "an untagged URI-less buffer is the placeholder"
    );

    // The binary chunk as a data URI, the placeholder after it: a .gltf rather than a GLB.
    let glb = std::fs::read(fixture("cube_meshopt.glb")).unwrap();
    let glb = gltf::binary::Glb::from_slice(&glb).unwrap();
    let mut json: Value = gltf::json::deserialize::from_slice(&glb.json).unwrap();
    let bin = glb.bin.unwrap();
    json["buffers"][0]["uri"] = Value::String(format!(
        "data:application/octet-stream;base64,{}",
        base64(&bin)
    ));
    let text = gltf::json::serialize::to_vec(&json).unwrap();
    assert_eq!(positions(&text), expected);
}

fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    while !out.len().is_multiple_of(4) {
        out.push('=');
    }
    out
}

/// Vertices and indices of the first primitive, as `load_gltf_slice` reads them.
fn geometry(bytes: &[u8]) -> (Vec<[f32; 3]>, Vec<u32>) {
    let scene = load_gltf_slice(bytes).unwrap();
    let primitive = &scene.primitives[0];
    (
        primitive.vertices.iter().map(|v| v.position).collect(),
        primitive.indices.clone(),
    )
}

/// A byte count of a view's EXT_meshopt_compression object.
fn view_member(json: &Value, view: usize, member: &str) -> usize {
    json["bufferViews"][view]["extensions"]["EXT_meshopt_compression"][member]
        .as_u64()
        .unwrap() as usize
}

#[test]
fn a_view_may_decode_into_the_buffer_that_holds_its_stream() {
    let expected = geometry(&std::fs::read(fixture("cube_meshopt.glb")).unwrap());
    let shared = edited_glb("cube_meshopt.glb", |json, bin| {
        let start = bin.len();
        let fallback = json["buffers"][1]["byteLength"].as_u64().unwrap() as usize;
        bin.resize(start + fallback, 0);
        json["buffers"][0]["byteLength"] = bin.len().into();
        for view in json["bufferViews"].as_array_mut().unwrap() {
            let offset = view["byteOffset"].as_u64().unwrap() as usize;
            view["buffer"] = 0.into();
            view["byteOffset"] = (start + offset).into();
        }
    });
    assert_eq!(geometry(&shared), expected);
}

#[test]
fn an_index_view_may_be_an_index_sequence() {
    let expected = geometry(&std::fs::read(fixture("cube_meshopt.glb")).unwrap());
    let sequenced = edited_glb("cube_meshopt.glb", |json, bin| {
        let (offset, length) = (
            view_member(json, 3, "byteOffset"),
            view_member(json, 3, "byteLength"),
        );
        let count = view_member(json, 3, "count");
        let mut indices = vec![0u8; count * 2];
        codec::decode_index_buffer(&mut indices, count, 2, &bin[offset..offset + length]).unwrap();
        let indices: Vec<u32> = indices
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&b| u32::from(u16::from_le_bytes(b)))
            .collect();
        let stream = encode_index_sequence(&indices);
        let start = bin.len();
        bin.extend(&stream);
        bin.resize(bin.len().next_multiple_of(4), 0);
        json["buffers"][0]["byteLength"] = bin.len().into();
        set_member(json, 3, "mode", "INDICES".into());
        set_member(json, 3, "byteOffset", start.into());
        set_member(json, 3, "byteLength", stream.len().into());
    });
    assert_eq!(geometry(&sequenced), expected);
}

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u8::from_str_radix(std::str::from_utf8(&pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn meshoptimizers_own_streams_decode_as_its_decoder_decodes_them() {
    let reference = std::fs::read(fixture("meshopt_codec_reference.json")).unwrap();
    let reference: Value = gltf::json::deserialize::from_slice(&reference).unwrap();
    let mut headers = Vec::new();
    for case in reference["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let count = case["count"].as_u64().unwrap() as usize;
        let stride = case["byteStride"].as_u64().unwrap() as usize;
        let stream = unhex(case["stream"].as_str().unwrap());
        let mut decoded = vec![0u8; count * stride];
        match case["mode"].as_str().unwrap() {
            "ATTRIBUTES" => codec::decode_vertex_buffer(&mut decoded, stride, &stream),
            "TRIANGLES" => codec::decode_index_buffer(&mut decoded, count, stride, &stream),
            "INDICES" => codec::decode_index_sequence(&mut decoded, count, stride, &stream),
            mode => panic!("{name}: mode {mode}"),
        }
        .unwrap_or_else(|error| panic!("{name}: {error:#}"));
        assert!(
            decoded == unhex(case["decoded"].as_str().unwrap()),
            "{name} decodes unlike meshoptimizer"
        );
        headers.push(stream[0]);
    }
    for header in [0xa0, 0xe0, 0xe1, 0xd0, 0xd1] {
        assert!(
            headers.contains(&header),
            "no stream with header {header:#04x}"
        );
    }
}

type Edit = fn(&mut Value, &mut Vec<u8>);

fn set_member(json: &mut Value, view: usize, member: &str, value: Value) {
    json["bufferViews"][view]["extensions"]["EXT_meshopt_compression"][member] = value;
}

/// Each breaks one of the extension's rules; the file is refused with an error instead of drawing garbage.
fn broken() -> Vec<(&'static str, Edit)> {
    vec![
        ("an unknown mode", |json, _| {
            set_member(json, 0, "mode", "LINES".into())
        }),
        ("an unknown filter", |json, _| {
            set_member(json, 0, "filter", "COLOR".into())
        }),
        ("OCTAHEDRAL on 12-byte elements", |json, _| {
            set_member(json, 0, "filter", "OCTAHEDRAL".into());
        }),
        ("a stride the bufferView disagrees with", |json, _| {
            set_member(json, 0, "byteStride", 16.into());
        }),
        ("a count that does not fill the bufferView", |json, _| {
            set_member(json, 0, "count", 23.into());
        }),
        ("a filter on triangles", |json, _| {
            set_member(json, 3, "filter", "EXPONENTIAL".into())
        }),
        ("a stream too short for its elements", |json, _| {
            set_member(json, 0, "byteLength", 20.into());
        }),
        ("a stream past its buffer", |json, _| {
            set_member(json, 3, "byteOffset", 330.into())
        }),
        ("a stream in the fallback buffer", |json, _| {
            set_member(json, 0, "buffer", 1.into())
        }),
        ("a placeholder longer than its views", |json, _| {
            json["buffers"][1]["byteLength"] = 1_000_000_000.into();
        }),
        ("an uncompressed view of the fallback", |json, _| {
            let view = gltf::json::deserialize::from_str(r#"{"buffer":1,"byteLength":4}"#).unwrap();
            json["bufferViews"].as_array_mut().unwrap().push(view);
        }),
        ("a vertex codec version 1 header", |_, bin| bin[0] = 0xa1),
    ]
}

#[test]
fn a_view_that_breaks_the_extensions_rules_is_refused() {
    assert!(load_gltf_slice(&edited_glb("cube_meshopt.glb", |_, _| {})).is_ok());
    for (name, edit) in broken() {
        match load_gltf_slice(&edited_glb("cube_meshopt.glb", edit)) {
            Ok(_) => panic!("{name} loaded"),
            Err(error) => eprintln!("{name}: {error:#}"),
        }
    }
}

fn zigzag(delta: u8) -> u8 {
    (delta << 1) ^ 0u8.wrapping_sub(delta >> 7)
}

/// Sixteen deltas at `bits` each, most significant first, with a sentinel for each that does not fit.
fn encode_group(group: &[u8], bits: usize, out: &mut Vec<u8>) {
    match bits {
        0 => {}
        8 => out.extend_from_slice(group),
        _ => {
            let sentinel = (1u8 << bits) - 1;
            let mut packed = vec![0u8; 16 * bits / 8];
            let mut escaped = Vec::new();
            for (i, &delta) in group.iter().enumerate() {
                let value = if delta >= sentinel {
                    escaped.push(delta);
                    sentinel
                } else {
                    delta
                };
                packed[i * bits / 8] |= value << (8 - bits - (i * bits) % 8);
            }
            out.extend(packed);
            out.extend(escaped);
        }
    }
}

/// The attribute codec's version 0, from the first element; `choose(n)` picks each group's width among the `n` valid.
fn encode_vertex_buffer(
    data: &[u8],
    stride: usize,
    choose: &mut impl FnMut(usize) -> usize,
) -> Vec<u8> {
    let baseline: Vec<u8> = data.get(..stride).map_or(vec![0; stride], <[u8]>::to_vec);
    let mut last = baseline.clone();
    let mut out = vec![0xa0];
    for block in data.chunks(codec::block_elements(stride) * stride) {
        let elements = block.len() / stride;
        for (k, previous) in last.iter_mut().enumerate() {
            let mut deltas = vec![0u8; elements.div_ceil(16) * 16];
            for (element, delta) in deltas[..elements].iter_mut().enumerate() {
                let value = block[element * stride + k];
                *delta = zigzag(value.wrapping_sub(*previous));
                *previous = value;
            }
            let mut header = vec![0u8; deltas.len().div_ceil(64)];
            let mut body = Vec::new();
            for (group, values) in deltas.as_chunks::<16>().0.iter().enumerate() {
                let mode = if values.iter().all(|&d| d == 0) {
                    choose(4)
                } else {
                    1 + choose(3)
                };
                header[group / 4] |= (mode as u8) << ((group % 4) * 2);
                encode_group(values, [0, 2, 4, 8][mode], &mut body);
            }
            out.extend(header);
            out.extend(body);
        }
    }
    out.extend(std::iter::repeat_n(0, stride.max(32) - stride));
    out.extend(baseline);
    out
}

fn varint(out: &mut Vec<u8>, mut value: u32) {
    while value > 127 {
        out.push((value & 127) as u8 | 128);
        value >>= 7;
    }
    out.push(value as u8);
}

/// meshoptimizer's index sequence encoder: a baseline switch whenever the delta from the current one reaches 30.
fn encode_index_sequence(indices: &[u32]) -> Vec<u8> {
    let mut out = vec![0xd1];
    let mut last = [0u32; 2];
    let mut current = 0;
    for &index in indices {
        current ^= usize::from((index.wrapping_sub(last[current]) as i32).unsigned_abs() >= 30);
        let delta = index.wrapping_sub(last[current]);
        let zigzag = (delta << 1) ^ ((delta as i32 >> 31) as u32);
        varint(&mut out, (zigzag << 1) | current as u32);
        last[current] = index;
    }
    out.extend([0; 4]);
    out
}

/// A column-wise walk of `deltas`, so neighbouring elements share most of their bytes as real attributes do.
fn walk(deltas: &[i8], stride: usize) -> Vec<u8> {
    let mut data = vec![0u8; deltas.len()];
    for (i, &delta) in deltas.iter().enumerate() {
        let previous = if i >= stride { data[i - stride] } else { 0x5a };
        data[i] = previous.wrapping_add(delta as u8);
    }
    data
}

fn delta() -> impl Strategy<Value = i8> {
    prop_oneof![3 => Just(0i8), 4 => -2i8..=2, 2 => -8i8..=8, 1 => any::<i8>()]
}

/// The spec's octahedral decode in f64, scaled to `max`.
fn octahedral_reference(x: f64, y: f64, one: f64, max: f64) -> [f64; 3] {
    let (mut x, mut y) = (x / one, y / one);
    let z = 1.0 - x.abs() - y.abs();
    let t = z.min(0.0);
    x -= t.copysign(x);
    y -= t.copysign(y);
    let length = (x * x + y * y + z * z).sqrt();
    [x / length * max, y / length * max, z / length * max]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn attribute_streams_round_trip_whatever_widths_the_groups_take(
        words in 1usize..=64,
        elements in 0usize..600,
        deltas in proptest::collection::vec(delta(), 1..2048),
        seed in any::<u64>(),
    ) {
        let stride = words * 4;
        let deltas: Vec<i8> = deltas.iter().copied().cycle().take(elements * stride).collect();
        let data = walk(&deltas, stride);
        let mut state = seed | 1;
        let mut choose = |n: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % n as u64) as usize
        };
        let stream = encode_vertex_buffer(&data, stride, &mut choose);
        prop_assert!(stream.len() >= codec::min_vertex_stream_bytes(elements, stride).unwrap());
        let mut decoded = vec![0u8; data.len()];
        codec::decode_vertex_buffer(&mut decoded, stride, &stream).unwrap();
        prop_assert_eq!(decoded, data);
    }

    #[test]
    fn index_sequences_round_trip_in_both_widths(
        indices in proptest::collection::vec(0u32..1 << 30, 0..400),
        wide in any::<bool>(),
    ) {
        let size = if wide { 4 } else { 2 };
        let indices: Vec<u32> = indices.iter().map(|&i| if wide { i } else { i & 0xffff }).collect();
        let stream = encode_index_sequence(&indices);
        let mut decoded = vec![0u8; indices.len() * size];
        codec::decode_index_sequence(&mut decoded, indices.len(), size, &stream).unwrap();
        let read: Vec<u32> = decoded
            .chunks_exact(size)
            .map(|b| if wide { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from(u16::from_le_bytes([b[0], b[1]])) })
            .collect();
        prop_assert_eq!(read, indices);
    }

    /// The fuzz target for the codecs alone: arbitrary bytes and layouts decode or fail, never panic.
    #[test]
    fn arbitrary_streams_decode_or_fail(
        stream in proptest::collection::vec(any::<u8>(), 0..1024),
        header in prop_oneof![Just(0xa0u8), Just(0xe0), Just(0xe1), Just(0xd1), any::<u8>()],
        elements in 0usize..300,
        words in 1usize..=64,
        wide in any::<bool>(),
    ) {
        let mut stream = stream;
        if let Some(first) = stream.first_mut() {
            *first = header;
        }
        let stride = words * 4;
        let mut out = vec![0u8; elements * stride];
        let _ = codec::decode_vertex_buffer(&mut out, stride, &stream);
        let size = if wide { 4 } else { 2 };
        let mut out = vec![0u8; elements * 3 * size];
        let _ = codec::decode_index_buffer(&mut out, elements * 3, size, &stream);
        let _ = codec::decode_index_sequence(&mut out, elements * 3, size, &stream);
        let mut filtered = stream.clone();
        filtered.truncate(filtered.len() / 8 * 8);
        let _ = codec::filter_octahedral(&mut filtered, if wide { 8 } else { 4 });
        codec::filter_quaternion(&mut filtered);
        codec::filter_exponential(&mut filtered);
    }

    #[test]
    fn octahedral_vectors_decode_within_a_unit_of_the_specification(
        bits in 2u32..=16,
        x in any::<i16>(),
        y in any::<i16>(),
        w in any::<i16>(),
    ) {
        let one = (1i32 << (bits - 1)) - 1;
        let clamp = |v: i16| i32::from(v).clamp(-one, one);
        let (x, y) = (clamp(x), clamp(y));
        let wide = bits > 8;
        let mut element: Vec<u8> = if wide {
            [x, y, one, i32::from(w)].iter().flat_map(|&c| (c as i16).to_le_bytes()).collect()
        } else {
            [x, y, one, i32::from(w)].iter().map(|&c| c as i8 as u8).collect()
        };
        let stride = element.len();
        codec::filter_octahedral(&mut element, stride).unwrap();
        let max = if wide { 32767.0 } else { 127.0 };
        let expected = octahedral_reference(f64::from(x), f64::from(y), f64::from(one), max);
        let got: Vec<i32> = if wide {
            element.as_chunks::<2>().0.iter().map(|&b| i32::from(i16::from_le_bytes(b))).collect()
        } else {
            element.iter().map(|&b| i32::from(b as i8)).collect()
        };
        for (component, expected) in got[..3].iter().zip(expected) {
            prop_assert!((f64::from(*component) - expected).abs() <= 1.0, "{got:?} against {expected}");
        }
        let passed = if wide { i32::from(w) } else { i32::from(w as i8) };
        prop_assert_eq!(got[3], passed, "the fourth component passes through");
    }

    #[test]
    fn quaternions_decode_within_a_unit_of_the_specification(
        bits in 4u32..=16,
        components in proptest::array::uniform3(-1.0f64..1.0),
        largest in 0usize..4,
    ) {
        // A unit quaternion's three smaller components, scaled by sqrt(2) into K-bit integers as gltfpack stores them.
        let length = components.iter().map(|c| c * c).sum::<f64>().sqrt().max(1.0);
        let small = components.map(|c| c / length / std::f64::consts::SQRT_2);
        let scale = f64::from((1i32 << (bits - 1)) - 1);
        let stored = small.map(|c| (c * std::f64::consts::SQRT_2 * scale).round() as i16);
        let one = (((1i32 << (bits - 1)) - 1) & !3) | largest as i32;
        let mut element: Vec<u8> = [stored[0], stored[1], stored[2], one as i16]
            .iter()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        codec::filter_quaternion(&mut element);
        let got: Vec<f64> = element.as_chunks::<2>().0.iter().map(|&b| f64::from(i16::from_le_bytes(b))).collect();
        let unit = f64::from(((1i32 << (bits - 1)) - 1) | 3);
        let range = 1.0 / std::f64::consts::SQRT_2;
        let xyz = stored.map(|s| f64::from(s) / unit * range);
        let w = (1.0 - xyz.iter().map(|c| c * c).sum::<f64>()).max(0.0).sqrt();
        for (k, value) in xyz.iter().enumerate() {
            prop_assert!((got[(largest + 1 + k) % 4] - value * 32767.0).abs() <= 1.0, "{got:?}");
        }
        prop_assert!((got[largest] - w * 32767.0).abs() <= 1.0, "{got:?}");
    }

    /// The loader's fuzz target aimed at the streams: a byte of the binary chunk flipped, the file refused or drawn.
    #[test]
    fn a_mutated_stream_is_refused_or_flattens(
        packed in 0usize..3,
        flips in proptest::collection::vec((any::<proptest::sample::Index>(), any::<u8>()), 1..6),
    ) {
        let name = TWINS[packed].packed;
        let bytes = edited_glb(name, |_, bin| {
            for (at, value) in &flips {
                let at = at.index(bin.len());
                bin[at] = *value;
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, &bytes).unwrap();
        if let Ok(model) = load_flat(&path, &LoadOptions::default()) {
            prop_assert!(model.indices.iter().all(|&i| (i as usize) < model.vertices.len()));
        }
        let _ = load_gltf_slice(&bytes);
    }
}
