//! The shared Slang modules' vector kernels on this device against `shaders/tests/vectors/`, the files
//! BeschleunigerBallett's C++ runner reads for the SPIR-V (docs/shaders.md#vector-kernels).

use std::collections::BTreeSet;

use kataglyphis_webgpu_renderer::GpuContext;
use serde_json::Value;
use wgpu::util::DeviceExt as _;

/// One shared module, embedded whole: the Windows arm64 lane runs this binary without the source tree.
struct Kernel {
    module: &'static str,
    vectors: &'static str,
    wgsl: &'static str,
    slang: &'static str,
}

const KERNELS: [Kernel; 7] = [
    Kernel {
        module: "brdf",
        vectors: include_str!("../../../shaders/tests/vectors/brdf.json"),
        wgsl: include_str!("shaders/brdf_test.wgsl"),
        slang: include_str!("../../../shaders/common/brdf.slang"),
    },
    Kernel {
        module: "noise",
        vectors: include_str!("../../../shaders/tests/vectors/noise.json"),
        wgsl: include_str!("shaders/noise_test.wgsl"),
        slang: include_str!("../../../shaders/common/noise.slang"),
    },
    Kernel {
        module: "path_estimator",
        vectors: include_str!("../../../shaders/tests/vectors/path_estimator.json"),
        wgsl: include_str!("shaders/path_estimator_test.wgsl"),
        slang: include_str!("../../../shaders/common/path_estimator.slang"),
    },
    Kernel {
        module: "hdr_safe",
        vectors: include_str!("../../../shaders/tests/vectors/hdr_safe.json"),
        wgsl: include_str!("shaders/hdr_safe_test.wgsl"),
        slang: include_str!("../../../shaders/common/hdr_safe.slang"),
    },
    Kernel {
        module: "aces",
        vectors: include_str!("../../../shaders/tests/vectors/aces.json"),
        wgsl: include_str!("shaders/aces_test.wgsl"),
        slang: include_str!("../../../shaders/common/aces.slang"),
    },
    Kernel {
        module: "fullscreen",
        vectors: include_str!("../../../shaders/tests/vectors/fullscreen.json"),
        wgsl: include_str!("shaders/fullscreen_test.wgsl"),
        slang: include_str!("../../../shaders/common/fullscreen.slang"),
    },
    Kernel {
        module: "camera_ray",
        vectors: include_str!("../../../shaders/tests/vectors/camera_ray.json"),
        wgsl: include_str!("shaders/camera_ray_test.wgsl"),
        slang: include_str!("../../../shaders/common/camera_ray.slang"),
    },
];

/// `width` words of `kind` (f32, u32 or i32); an f32 output passes within `abs + rel * |ref|`.
struct Port {
    name: String,
    kind: String,
    width: usize,
    abs: f64,
    rel: f64,
}

fn document(kernel: &Kernel) -> Value {
    serde_json::from_str(kernel.vectors)
        .unwrap_or_else(|e| panic!("{}.json must parse: {e}", kernel.module))
}

fn ports(doc: &Value, key: &str) -> Vec<Port> {
    let rows = doc[key]
        .as_array()
        .unwrap_or_else(|| panic!("no `{key}` list"));
    rows.iter()
        .map(|row| Port {
            name: row["name"].as_str().expect("a port name").to_owned(),
            kind: row["type"].as_str().expect("a port type").to_owned(),
            width: usize::try_from(row["width"].as_u64().expect("a port width")).unwrap(),
            abs: row["abs"].as_f64().unwrap_or(0.0),
            rel: row["rel"].as_f64().unwrap_or(0.0),
        })
        .collect()
}

fn words(ports: &[Port]) -> usize {
    ports.iter().map(|port| port.width).sum()
}

fn cases(doc: &Value) -> &[Value] {
    doc["cases"].as_array().expect("a `cases` list")
}

/// One value as the kernel's word: an f32's bits, or the integer.
fn encode(kind: &str, value: &Value) -> u32 {
    match kind {
        "f32" => (value.as_f64().expect("an f32") as f32).to_bits(),
        "u32" => u32::try_from(value.as_u64().expect("a u32")).expect("a u32 in range"),
        "i32" => i32::try_from(value.as_i64().expect("an i32")).expect("an i32 in range") as u32,
        other => panic!("unknown word type {other}"),
    }
}

/// The two header words, input and output words per case, then every case's inputs in port order.
fn pack(doc: &Value, inputs: &[Port], outputs: &[Port]) -> Vec<u32> {
    let mut packed = vec![words(inputs) as u32, words(outputs) as u32];
    for case in cases(doc) {
        for port in inputs {
            let values = case["inputs"][&port.name]
                .as_array()
                .unwrap_or_else(|| panic!("{}: no input {}", case["name"], port.name));
            assert_eq!(values.len(), port.width, "{}: {}", case["name"], port.name);
            packed.extend(values.iter().map(|value| encode(&port.kind, value)));
        }
    }
    packed
}

/// Runs `entry` once per case; the output buffer holds one more case of NaN words, which no thread may touch.
fn dispatch(
    gpu: &GpuContext,
    kernel: &Kernel,
    entry: &str,
    packed: &[u32],
    output_words: usize,
) -> Vec<u32> {
    let device = &gpu.device;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(kernel.module),
        source: wgpu::ShaderSource::Wgsl(kernel.wgsl.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(kernel.module),
        layout: None,
        module: &module,
        entry_point: Some(entry),
        compilation_options: Default::default(),
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vector_inputs"),
        contents: bytemuck::cast_slice(packed),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vector_outputs"),
        contents: bytemuck::cast_slice(&vec![u32::MAX; output_words]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let bytes = (output_words * 4) as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vector_readback"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(kernel.module),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let case_count = (packed.len() - 2) / packed[0] as usize;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(case_count as u32, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
    gpu.queue.submit(Some(encoder.finish()));
    readback.slice(..).map_async(wgpu::MapMode::Read, |result| {
        result.expect("readback must map")
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll must succeed");
    let data = readback.slice(..).get_mapped_range().expect("mapped");
    bytemuck::pod_collect_to_vec(&data)
}

/// `word`'s distance from `reference` in tolerances, above 1 a miss; NaN, an unwritten word, is infinitely far.
fn distance(port: &Port, reference: &Value, word: u32) -> f64 {
    if port.kind != "f32" {
        return if word == encode(&port.kind, reference) {
            0.0
        } else {
            f64::INFINITY
        };
    }
    let gpu = f64::from(f32::from_bits(word));
    let expected = reference.as_f64().expect("an f32 reference");
    if gpu == expected {
        return 0.0;
    }
    let distance = (gpu - expected).abs() / (port.abs + port.rel * expected.abs());
    if distance.is_nan() {
        f64::INFINITY
    } else {
        distance
    }
}

fn describe(port: &Port, reference: &Value, word: u32) -> String {
    if port.kind != "f32" {
        return format!("gpu {word} against {}", encode(&port.kind, reference));
    }
    let expected = reference.as_f64().expect("an f32 reference");
    let tolerance = port.abs + port.rel * expected.abs();
    format!(
        "gpu {:e} against {expected:e}, tolerance {tolerance:e}",
        f32::from_bits(word)
    )
}

/// Every component that misses its reference, named by case, output and component, and the closest call of the rest.
fn misses(doc: &Value, outputs: &[Port], got: &[u32]) -> (Vec<String>, (f64, String)) {
    let (mut found, mut closest) = (Vec::new(), (0.0, String::from("none")));
    let mut at = 0;
    for case in cases(doc) {
        let name = case["name"].as_str().expect("a case name");
        for port in outputs {
            let references = case["outputs"][&port.name]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: no output {}", port.name));
            assert_eq!(references.len(), port.width, "{name}: {}", port.name);
            for (k, reference) in references.iter().enumerate() {
                let distance = distance(port, reference, got[at]);
                let label = format!("{name}: {}[{k}]", port.name);
                if distance > 1.0 {
                    found.push(format!("{label} {}", describe(port, reference, got[at])));
                } else if distance > closest.0 {
                    closest = (distance, label);
                }
                at += 1;
            }
        }
    }
    if got[at..].iter().any(|&word| word != u32::MAX) {
        found.push("the kernel wrote past its last case".to_owned());
    }
    (found, closest)
}

fn check(kernel: &Kernel) {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let doc = document(kernel);
    let (inputs, outputs) = (ports(&doc, "inputs"), ports(&doc, "outputs"));
    let packed = pack(&doc, &inputs, &outputs);
    let entry = doc["entry"].as_str().expect("an entry point");
    let output_words = (cases(&doc).len() + 1) * words(&outputs);
    let got = dispatch(&gpu, kernel, entry, &packed, output_words);
    let (found, (distance, label)) = misses(&doc, &outputs, &got);
    eprintln!(
        "{}: {} cases, closest passing component {label} at {distance:.3} of its tolerance",
        kernel.module,
        cases(&doc).len()
    );
    assert!(
        found.is_empty(),
        "{}: {} component(s) miss their reference:\n{}",
        kernel.module,
        found.len(),
        found.join("\n")
    );
}

#[test]
fn brdf_kernel_meets_its_vectors() {
    check(&KERNELS[0]);
}

#[test]
fn noise_kernel_meets_its_vectors() {
    check(&KERNELS[1]);
}

#[test]
fn path_estimator_kernel_meets_its_vectors() {
    check(&KERNELS[2]);
}

#[test]
fn hdr_safe_kernel_meets_its_vectors() {
    check(&KERNELS[3]);
}

#[test]
fn aces_kernel_meets_its_vectors() {
    check(&KERNELS[4]);
}

#[test]
fn fullscreen_kernel_meets_its_vectors() {
    check(&KERNELS[5]);
}

#[test]
fn camera_ray_kernel_meets_its_vectors() {
    check(&KERNELS[6]);
}

/// The names of the functions `slang` defines at file scope, overloads once.
fn defined_functions(slang: &str) -> BTreeSet<String> {
    slang
        .lines()
        .filter(|line| line.starts_with(|c: char| c.is_ascii_alphabetic()))
        .filter(|line| !line.starts_with("static ") && !line.starts_with("import "))
        .filter_map(|line| line.split_once('('))
        .filter_map(|(head, _)| head.split_whitespace().last().map(str::to_owned))
        .collect()
}

/// Every function a shared module defines has an output and every output names one, so a new function needs vectors.
#[test]
fn every_public_function_has_vectors() {
    for kernel in &KERNELS {
        let doc = document(kernel);
        let named: BTreeSet<String> = ports(&doc, "outputs")
            .iter()
            .map(|port| port.name.split('.').next().unwrap().to_owned())
            .collect();
        assert_eq!(
            named,
            defined_functions(kernel.slang),
            "{}.json's outputs against the functions common/{}.slang defines",
            kernel.module,
            kernel.module
        );
        assert_eq!(doc["module"], format!("common/{}.slang", kernel.module));
        assert!(
            cases(&doc).len() >= 12,
            "{}.json lost its cases",
            kernel.module
        );
    }
}
