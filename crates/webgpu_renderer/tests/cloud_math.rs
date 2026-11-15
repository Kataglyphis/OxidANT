//! The shared cloud math (`shaders/common/volumetric_clouds.slang`) as `cloud_march.wgsl` emits it,
//! run on the adapter against CPU references: the box, the phase function and the uniform unpack.

use glam::Vec3;
use kataglyphis_webgpu_renderer::render::clouds::CloudSettings;
use kataglyphis_webgpu_renderer::GpuContext;
use wgpu::util::DeviceExt as _;

const CLOUD_MARCH_WGSL: &str = include_str!("../src/shaders/cloud_march.wgsl");

/// The brace-balanced text of the item `head` opens, such as `fn phase_HG_0(` or `struct Clouds_0`.
fn extract(head: &str) -> &'static str {
    let start = CLOUD_MARCH_WGSL
        .find(head)
        .unwrap_or_else(|| panic!("cloud_march.wgsl no longer has `{head}`"));
    let open = start
        + CLOUD_MARCH_WGSL[start..]
            .find('{')
            .expect("the item has a body");
    let mut depth = 0;
    for (i, c) in CLOUD_MARCH_WGSL[open..].char_indices() {
        depth += match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        };
        if depth == 0 {
            return &CLOUD_MARCH_WGSL[start..=open + i];
        }
    }
    panic!("`{head}` is never closed");
}

/// Runs `body` once per case over `inputs`, `per_case.0` vec4s in and `per_case.1` out, and returns the outputs.
fn probe(
    gpu: &GpuContext,
    items: &[&str],
    body: &str,
    inputs: &[[f32; 4]],
    per_case: (usize, usize),
) -> Vec<[f32; 4]> {
    let device = &gpu.device;
    let cases = inputs.len() / per_case.0;
    let source = format!(
        "{}\n@group(0) @binding(0) var<storage, read> inputs: array<vec4<f32>>;\n\
         @group(0) @binding(1) var<storage, read_write> outputs: array<vec4<f32>>;\n\
         @compute @workgroup_size(1)\n\
         fn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
         let i = id.x * {}u;\nlet o = id.x * {}u;\n{body}\n}}\n",
        items.join("\n"),
        per_case.0,
        per_case.1,
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cloud_math_probe"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("cloud_math_probe"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cloud_math_inputs"),
        contents: bytemuck::cast_slice(inputs),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let bytes = (cases * per_case.1 * 16) as u64;
    let output = buffer(
        device,
        bytes,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let readback = buffer(
        device,
        bytes,
        wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
    );
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cloud_math_probe"),
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
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(cases as u32, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
    gpu.queue.submit(Some(encoder.finish()));
    read(gpu, &readback)
}

fn buffer(device: &wgpu::Device, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cloud_math_probe"),
        size,
        usage,
        mapped_at_creation: false,
    })
}

fn read(gpu: &GpuContext, readback: &wgpu::Buffer) -> Vec<[f32; 4]> {
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.expect("map"));
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
    let data = slice.get_mapped_range().expect("mapped");
    bytemuck::pod_collect_to_vec(&data)
}

/// The reference slab test: entry and exit distances of a ray through an axis-aligned box, if it hits.
fn slab(ro: Vec3, rd: Vec3, centre: Vec3, half: Vec3) -> Option<(f32, f32)> {
    let inverse = rd.recip();
    let near = (centre - half - ro) * inverse;
    let far = (centre + half - ro) * inverse;
    let (entry, exit) = (near.min(far).max_element(), near.max(far).min_element());
    (entry <= exit && exit >= 0.0).then_some((entry, exit))
}

/// A tiny deterministic generator, so a failing ray reproduces.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, lo: f32, hi: f32) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        lo + (hi - lo) * ((self.0 >> 40) as f32 / (1u64 << 24) as f32)
    }

    fn vec3(&mut self, lo: f32, hi: f32) -> Vec3 {
        Vec3::new(self.next(lo, hi), self.next(lo, hi), self.next(lo, hi))
    }
}

/// One ray against one box: [ro, rd, centre, half].
fn box_cases(count: usize) -> Vec<[Vec3; 4]> {
    let mut random = Lcg(7);
    let mut cases = vec![
        // From (10, 20, 0) down -Z through a box at z = -30 with half depth 3: in at 27, out at 33.
        [
            Vec3::new(10.0, 20.0, 0.0),
            Vec3::new(0.0, 0.01, -1.0).normalize(),
            Vec3::new(10.0, 20.0, -30.0),
            Vec3::new(1.0, 2.0, 3.0),
        ],
        // The same ray 1.5 to the side of a box 1 wide misses it, which a sheared slab would not.
        [
            Vec3::new(11.5, 20.0, 0.0),
            Vec3::new(0.0, 0.01, -1.0).normalize(),
            Vec3::new(10.0, 20.0, -30.0),
            Vec3::new(1.0, 2.0, 3.0),
        ],
        // A sun straight overhead: zero x and z components must neither miss nor turn NaN.
        [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::Y,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 2.0, 4.0),
        ],
    ];
    while cases.len() < count {
        let rd = random.vec3(-1.0, 1.0);
        if rd.abs().min_element() < 0.05 {
            continue;
        }
        cases.push([
            random.vec3(-50.0, 50.0),
            rd.normalize(),
            random.vec3(-30.0, 30.0),
            random.vec3(0.5, 20.0),
        ]);
    }
    cases
}

#[test]
fn the_cloud_box_is_axis_aligned_around_its_offset() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let cases = box_cases(256);
    let inputs: Vec<[f32; 4]> = cases
        .iter()
        .flat_map(|case| case.map(|v| v.extend(0.0).to_array()))
        .collect();
    let body = "var cloud: Clouds_0;\ncloud.radius_0 = inputs[i + 3u].xyz;\ncloud.offset_0 = inputs[i + 2u].xyz;\n\
                var t = vec2<f32>(0.0);\n\
                let hit = box_intersect_0(inputs[i].xyz, inputs[i + 1u].xyz, cloud, &t);\n\
                outputs[o] = vec4<f32>(select(0.0, 1.0, hit), t, 0.0);";
    let items = [extract("struct Clouds_0"), extract("fn box_intersect_0(")];
    let results = probe(&gpu, &items, body, &inputs, (4, 1));
    let expected =
        slab(cases[0][0], cases[0][1], cases[0][2], cases[0][3]).expect("the first ray hits");
    assert!(
        (expected.0 - 27.0).abs() < 0.01 && (expected.1 - 33.0).abs() < 0.01,
        "{expected:?}"
    );
    let mut mismatches = Vec::new();
    for (case, [hit, near, far, _]) in cases.iter().zip(results) {
        let [ro, rd, centre, half] = *case;
        let gpu_hit = (hit > 0.5).then_some((near, far));
        let agree = match (slab(ro, rd, centre, half), gpu_hit) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                (a.0 - b.0).abs() <= 1e-3 * a.0.abs().max(1.0)
                    && (a.1 - b.1).abs() <= 1e-3 * a.1.abs().max(1.0)
            }
            (Some((a, b)), None) | (None, Some((a, b))) => (b - a).abs() < 1e-3 || b.abs() < 1e-3,
        };
        if !agree {
            mismatches.push(format!(
                "ro {ro} rd {rd} box {centre} ± {half}: gpu {gpu_hit:?}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "the shader's box is not the axis-aligned box around Offset:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn the_phase_function_peaks_toward_the_sun_and_integrates_to_one() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let steps = 2048;
    let cosines: Vec<[f32; 4]> = (0..=steps)
        .map(|i| [-1.0 + 2.0 * i as f32 / steps as f32, 0.5, 0.0, 0.0])
        .collect();
    let body = "outputs[o] = vec4<f32>(phase_HG_0(inputs[i].x, inputs[i].y), 0.0, 0.0, 0.0);";
    let phase: Vec<f32> = probe(&gpu, &[extract("fn phase_HG_0(")], body, &cosines, (1, 1))
        .into_iter()
        .map(|p| p[0])
        .collect();
    // Over the sphere: 2 pi times the integral over cos theta, by the trapezoid rule.
    let width = 2.0 / steps as f32;
    let inner: f32 = phase[1..steps].iter().sum();
    let integral = std::f32::consts::TAU * width * (inner + 0.5 * (phase[0] + phase[steps]));
    assert!(
        (integral - 1.0).abs() < 0.01,
        "the phase function integrates to {integral}"
    );
    assert!(
        phase[steps] > phase[0] * 10.0,
        "g = 0.5 must scatter forward, toward the sun"
    );
    assert!(
        phase.windows(2).all(|w| w[1] >= w[0]),
        "it must rise monotonically toward the sun"
    );
}

/// What the host packs is what the shader unpacks: the C++ SceneUBO layout, its clamps and the box.
#[test]
fn the_shader_unpacks_the_box_and_steps_the_host_packs() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let mut wild = CloudSettings {
        march_steps: 1000,
        light_steps: 0,
        powder: false,
        ..CloudSettings::default()
    };
    wild.place_box(Vec3::new(-3.0, 9.0, 40.0), Vec3::new(5.0, 0.5, 12.0), 0.25);
    let settings = [CloudSettings::default(), wild];
    let inputs: Vec<[f32; 4]> = settings
        .iter()
        .flat_map(|s| {
            let p = s.params();
            [p.mesh_scale, p.mesh_offset, p.parameters, p.light_march]
        })
        .collect();
    let body = "var params = CloudParams_std140_0(inputs[i], inputs[i + 1u], inputs[i + 2u], inputs[i + 3u]);\n\
                let cloud = cloud_unpack_0(&params);\n\
                outputs[o] = vec4<f32>(cloud.radius_0, f32(cloud.num_march_steps_0));\n\
                outputs[o + 1u] = vec4<f32>(cloud.offset_0, f32(cloud.num_march_steps_to_light_0));\n\
                outputs[o + 2u] = vec4<f32>(cloud.threshold_0, cloud.scale_0,\n\
                                            cloud.pillowness_0, cloud.cirrus_effect_0);\n\
                outputs[o + 3u] = vec4<f32>(select(0.0, 1.0, cloud.powder_effect_0), 0.0, 0.0, 0.0);";
    let items = [
        extract("struct CloudParams_std140_0"),
        extract("struct Clouds_0"),
        extract("fn cloud_unpack_0("),
    ];
    let results = probe(&gpu, &items, body, &inputs, (4, 4));
    for (clouds, unpacked) in settings.iter().zip(results.chunks(4)) {
        let radius = Vec3::from_slice(&unpacked[0][..3]);
        assert!(
            radius.abs_diff_eq(
                clouds.half_extents(),
                1e-3 * clouds.half_extents().max_element()
            ),
            "{radius} vs {:?}",
            clouds.half_extents()
        );
        assert_eq!(Vec3::from_slice(&unpacked[1][..3]), clouds.offset);
        let steps = [unpacked[0][3], unpacked[1][3]];
        let expected = [
            clouds.march_steps.clamp(4, 128) as f32,
            clouds.light_steps.clamp(1, 128) as f32,
        ];
        assert_eq!(steps, expected);
        assert_eq!(
            unpacked[2],
            [
                clouds.coverage,
                clouds.density,
                clouds.pillowness,
                clouds.cirrus
            ]
        );
        assert_eq!(unpacked[3][0], if clouds.powder { 1.0 } else { 0.0 });
    }
}
