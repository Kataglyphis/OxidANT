//! The shared GGX NDF (`shaders/common/brdf.slang`), evaluated on the GPU as forward.wgsl emits it.

use kataglyphis_webgpu_renderer::GpuContext;
use wgpu::util::DeviceExt as _;

const FORWARD_WGSL: &str = include_str!("../src/shaders/forward.wgsl");
const TRACED_WGSL: &str = include_str!("../src/shaders/traced.wgsl");

/// The brace-balanced text of `fn <name>(...) { ... }` in `wgsl`.
fn extract_fn(wgsl: &'static str, name: &str) -> &'static str {
    let start = wgsl
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("the module no longer declares `fn {name}`"));
    let body = start + wgsl[start..].find('{').expect("the function has a body");
    let mut depth = 0;
    for (i, c) in wgsl[body..].char_indices() {
        depth += match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        };
        if depth == 0 {
            return &wgsl[start..=body + i];
        }
    }
    panic!("`fn {name}` is never closed");
}

/// Each function of `wgsl` whose body calls `callee`, with its number of call sites.
fn callers(wgsl: &'static str, callee: &str) -> Vec<(&'static str, usize)> {
    wgsl.lines()
        .filter_map(|line| line.strip_prefix("fn ")?.split_once('('))
        .filter_map(|(name, _)| {
            let (_, body) = extract_fn(wgsl, name).split_once('{')?;
            let calls = body.matches(&format!("{callee}(")).count();
            (calls > 0).then_some((name, calls))
        })
        .collect()
}

/// `distribution_ggx(n_dot_h, roughness)` for each input pair, computed by the adapter.
fn ndf_on_gpu(gpu: &GpuContext, inputs: &[[f32; 2]]) -> Vec<f32> {
    let device = &gpu.device;
    let source = format!(
        "{}\n@group(0) @binding(0) var<storage, read> inputs: array<vec2<f32>>;\n\
         @group(0) @binding(1) var<storage, read_write> outputs: array<f32>;\n\
         @compute @workgroup_size(64)\n\
         fn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
         if (id.x < arrayLength(&inputs)) {{\n\
         outputs[id.x] = distribution_ggx_0(inputs[id.x].x, inputs[id.x].y);\n}}\n}}\n",
        extract_fn(FORWARD_WGSL, "distribution_ggx_0")
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("ndf_probe"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("ndf_probe"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ndf_inputs"),
        contents: bytemuck::cast_slice(inputs),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let bytes = (inputs.len() * 4) as u64;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ndf_outputs"),
        size: bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ndf_readback"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ndf_probe"),
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
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups((inputs.len() as u32).div_ceil(64), 1, 1);
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

#[test]
fn the_ggx_peak_is_one_over_pi_r4_down_to_the_roughness_floor() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    let roughness = [0.045f32, 0.06, 0.1, 0.15, 0.3, 0.6, 1.0];
    let inputs: Vec<[f32; 2]> = roughness.iter().map(|&r| [1.0, r]).collect();
    for (&r, &peak) in roughness.iter().zip(&ndf_on_gpu(&gpu, &inputs)) {
        let expected = 1.0 / (std::f64::consts::PI * f64::from(r).powi(4));
        assert!(
            (f64::from(peak) / expected - 1.0).abs() < 0.01,
            "D(n.h = 1, roughness {r}) is {peak}, not 1 / (pi r^4) = {expected:.1}"
        );
    }
    // Below the floor the lobe stays the floor's, finite in any HDR target, not a vanishing one.
    let floor_peak = ndf_on_gpu(&gpu, &[[1.0, 0.045], [1.0, 0.0]]);
    assert_eq!(
        floor_peak[0], floor_peak[1],
        "roughness 0 must evaluate as the 0.045 floor"
    );
}

/// The projected NDF integrates to one over the hemisphere: 2 pi * integral of D(mu) mu dmu.
#[test]
fn the_ggx_lobe_integrates_to_one_at_every_roughness() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    // n.h nodes that f32 holds exactly, dense where the lobe is: geometric in 1 - n.h.
    let mut nodes: Vec<f32> = std::iter::successors(Some(2f64.powi(-25)), |t| Some(t * 1.005))
        .take_while(|t| *t < 1.0)
        .map(|t| (1.0 - t) as f32)
        .chain([0.0, 1.0])
        .collect();
    nodes.sort_by(f32::total_cmp);
    nodes.dedup();

    for r in [0.045f32, 0.1, 0.15, 0.5, 1.0] {
        let inputs: Vec<[f32; 2]> = nodes.iter().map(|&mu| [mu, r]).collect();
        let d = ndf_on_gpu(&gpu, &inputs);
        let integrand =
            |i: usize| 2.0 * std::f64::consts::PI * f64::from(d[i]) * f64::from(nodes[i]);
        let integral: f64 = (0..nodes.len() - 1)
            .map(|i| f64::from(nodes[i + 1] - nodes[i]) * (integrand(i) + integrand(i + 1)) * 0.5)
            .sum();
        assert!(
            (integral - 1.0).abs() < 0.02,
            "the GGX lobe at roughness {r} integrates to {integral:.4}, not 1: a guard clips its peak"
        );
    }
}

/// brdf_smith_k takes the GGX energy from its caller, so a shading point fits it once however many lights it takes.
#[test]
fn the_energy_fit_runs_once_per_shading_point() {
    assert_eq!(
        callers(FORWARD_WGSL, "ggx_energy_0"),
        [("shade_surface_0", 1)],
        "forward.wgsl: once per fragment"
    );
    assert_eq!(
        callers(TRACED_WGSL, "ggx_energy_0"),
        [("ray_trace_pixel_0", 1), ("trace_path_0", 1)],
        "traced.wgsl: once per ray-traced hit, which its sun, lights and ambient share, and once per path vertex"
    );
}
