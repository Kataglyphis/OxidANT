//! Pins histogram.wgsl's `histogram_bin` to the unit-tested CPU copy in `render::auto_exposure`.
//! Uses known texture contents, not a rendered frame, so a mismatch points at the binning.

use kataglyphis_webgpu_renderer::context::GpuContext;
use kataglyphis_webgpu_renderer::render::auto_exposure::{histogram_bin, HISTOGRAM_BINS};
use kataglyphis_webgpu_renderer::render::gpu_timing::PassScope;
use kataglyphis_webgpu_renderer::render::histogram::HistogramPass;

/// An Rgba32Float texture of grey pixels, so luminance is the channel value under any weighting.
fn texture_with_luminances(gpu: &GpuContext, luminances: &[f32], width: u32) -> wgpu::TextureView {
    let height = luminances.len() as u32 / width;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("histogram_test_source"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let mut pixels: Vec<f32> = Vec::with_capacity(luminances.len() * 4);
    for &l in luminances {
        pixels.extend_from_slice(&[l, l, l, 1.0]);
    }

    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&pixels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn build_histogram(gpu: &GpuContext, luminances: &[f32], width: u32) -> Vec<u32> {
    let view = texture_with_luminances(gpu, luminances, width);
    let mut pass = HistogramPass::new(gpu);
    pass.set_input(gpu, &view);

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    pass.encode(
        &mut encoder,
        width,
        luminances.len() as u32 / width,
        PassScope::disabled(),
    );
    pass.encode_readback(&mut encoder);
    gpu.queue.submit(Some(encoder.finish()));

    pass.read_back(gpu)
}

#[test]
fn gpu_binning_matches_the_cpu_binning() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // Black, sub-range, many in-range decades and above-range: every branch of the binning.
    let width = 16u32;
    let luminances: Vec<f32> = (0..256)
        .map(|i| match i % 8 {
            0 => 0.0,
            1 => 1e-9,
            2 => 1e-5,
            3 => 0.01,
            4 => 0.18,
            5 => 1.0,
            6 => 50.0,
            _ => 1e9,
        })
        .collect();

    let gpu_histogram = build_histogram(&gpu, &luminances, width);
    assert_eq!(gpu_histogram.len(), HISTOGRAM_BINS);

    let mut expected = vec![0u32; HISTOGRAM_BINS];
    for &l in &luminances {
        expected[histogram_bin(l)] += 1;
    }

    assert_eq!(
        gpu_histogram, expected,
        "shader binning disagrees with render::auto_exposure::histogram_bin"
    );
}

#[test]
fn every_pixel_is_counted_exactly_once() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // Not a multiple of the 16x16 workgroup, so dropped or double-counted edges show.
    let width = 40u32;
    let height = 24u32;
    let luminances = vec![0.5f32; (width * height) as usize];

    let histogram = build_histogram(&gpu, &luminances, width);
    let total: u32 = histogram.iter().sum();

    assert_eq!(
        total,
        width * height,
        "histogram counted {total} samples for a {width}x{height} image"
    );
}

#[test]
fn the_histogram_is_cleared_between_builds() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // A reused pass must not accumulate; without the clear, exposure silently drifts.
    let width = 16u32;
    let luminances = vec![0.25f32; 256];
    let view = texture_with_luminances(&gpu, &luminances, width);
    let mut pass = HistogramPass::new(&gpu);
    pass.set_input(&gpu, &view);

    let mut totals = Vec::new();
    for _ in 0..3 {
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        pass.encode(&mut encoder, width, 256 / width, PassScope::disabled());
        pass.encode_readback(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        totals.push(pass.read_back(&gpu).iter().sum::<u32>());
    }

    assert_eq!(totals[0], 256);
    assert_eq!(
        totals,
        vec![256, 256, 256],
        "counts accumulated across builds: {totals:?}"
    );
}

/// Runs build + reduce over a known image and returns (adapted EV, target EV).
fn reduce_exposure(
    gpu: &GpuContext,
    luminances: &[f32],
    width: u32,
    settings: kataglyphis_webgpu_renderer::render::histogram::ExposureSettings,
    start_ev: f32,
) -> (f32, f32) {
    let view = texture_with_luminances(gpu, luminances, width);
    let mut pass = HistogramPass::new(gpu);
    pass.set_input(gpu, &view);
    pass.reset_exposure(&gpu.queue, start_ev);
    pass.set_exposure_settings(&gpu.queue, settings);

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    pass.encode(
        &mut encoder,
        width,
        luminances.len() as u32 / width,
        PassScope::disabled(),
    );
    pass.encode_reduce(&mut encoder, PassScope::disabled());
    pass.encode_exposure_readback(&mut encoder);
    gpu.queue.submit(Some(encoder.finish()));

    pass.read_back_exposure(gpu)
}

#[test]
fn gpu_reduction_matches_the_cpu_exposure_maths() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    use kataglyphis_webgpu_renderer::render::auto_exposure::{
        average_luminance, exposure_ev_for_luminance,
    };
    use kataglyphis_webgpu_renderer::render::histogram::ExposureSettings;

    let width = 16u32;
    for &scene_luminance in &[0.01f32, 0.18, 1.0, 25.0] {
        let luminances = vec![scene_luminance; 256];

        // speed 0 disables smoothing, so this compares the maths, not the adaptation curve.
        let settings = ExposureSettings {
            delta_time_seconds: 1.0 / 60.0,
            speed: 0.0,
            auto_enabled: true,
            manual_ev: 0.0,
        };
        let (_adapted, gpu_target) = reduce_exposure(&gpu, &luminances, width, settings, 0.0);

        let mut expected_histogram = vec![0u32; HISTOGRAM_BINS];
        for &l in &luminances {
            expected_histogram[histogram_bin(l)] += 1;
        }
        let cpu_target =
            exposure_ev_for_luminance(average_luminance(&expected_histogram).expect("populated"));

        assert!(
            (gpu_target - cpu_target).abs() < 0.05,
            "scene luminance {scene_luminance}: GPU target EV {gpu_target}, CPU {cpu_target}"
        );
    }
}

#[test]
fn a_dark_scene_exposes_up_and_a_bright_scene_exposes_down() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    use kataglyphis_webgpu_renderer::render::histogram::ExposureSettings;

    let settings = ExposureSettings {
        speed: 0.0,
        ..ExposureSettings::default()
    };

    let (_, dark_target) = reduce_exposure(&gpu, &vec![0.005f32; 256], 16, settings, 0.0);
    let (_, bright_target) = reduce_exposure(&gpu, &vec![20.0f32; 256], 16, settings, 0.0);

    assert!(
        dark_target > 0.0,
        "a dark scene must expose up, got {dark_target}"
    );
    assert!(
        bright_target < 0.0,
        "a bright scene must expose down, got {bright_target}"
    );
}

#[test]
fn adaptation_moves_toward_the_target_without_jumping_to_it() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    use kataglyphis_webgpu_renderer::render::histogram::ExposureSettings;

    // One 60 Hz frame moves exposure part of the way; snapping would look like flicker.
    let settings = ExposureSettings {
        delta_time_seconds: 1.0 / 60.0,
        speed: 3.0,
        auto_enabled: true,
        manual_ev: 0.0,
    };
    let (adapted, target) = reduce_exposure(&gpu, &vec![0.005f32; 256], 16, settings, 0.0);

    assert!(
        target > 1.0,
        "test needs a target well away from the start, got {target}"
    );
    assert!(adapted > 0.0, "exposure moved the wrong way: {adapted}");
    assert!(
        adapted < target * 0.5,
        "one 16ms frame should cover a fraction of the distance, got {adapted} of {target}"
    );
}

#[test]
fn an_all_black_frame_holds_the_previous_exposure() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    use kataglyphis_webgpu_renderer::render::histogram::ExposureSettings;

    // An empty histogram would divide by zero; holding is the only safe answer.
    let start_ev = 1.75f32;
    let (adapted, _target) = reduce_exposure(
        &gpu,
        &vec![0.0f32; 256],
        16,
        ExposureSettings::default(),
        start_ev,
    );

    assert!(
        (adapted - start_ev).abs() < 1e-4,
        "an all-black frame changed exposure from {start_ev} to {adapted}"
    );
}

#[test]
fn a_zero_length_frame_does_not_snap_the_exposure() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    use kataglyphis_webgpu_renderer::render::histogram::ExposureSettings;

    // A stalled frame reports dt 0.0 and must hold, unlike speed 0, which snaps.
    let luminances = vec![0.005f32; 256];
    let settings = ExposureSettings {
        delta_time_seconds: 1.0 / 60.0,
        speed: 3.0,
        auto_enabled: true,
        manual_ev: 0.0,
    };
    let (moved, target) = reduce_exposure(&gpu, &luminances, 16, settings, 0.0);
    assert!(
        moved > 0.0 && moved < target,
        "setup frame should move partway toward the target, got {moved} of {target}"
    );

    let stalled_settings = ExposureSettings {
        delta_time_seconds: 0.0,
        ..settings
    };
    let (held, _target) = reduce_exposure(&gpu, &luminances, 16, stalled_settings, moved);

    assert_eq!(
        held, moved,
        "a zero-length frame moved exposure from {moved} to {held}"
    );
}

#[test]
fn manual_mode_writes_the_slider_value_through() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };
    use kataglyphis_webgpu_renderer::render::histogram::ExposureSettings;

    // Manual mode writes the same buffer, so no stale auto value survives a switch.
    let settings = ExposureSettings {
        auto_enabled: false,
        manual_ev: -2.5,
        ..ExposureSettings::default()
    };
    let (adapted, target) = reduce_exposure(&gpu, &vec![0.005f32; 256], 16, settings, 4.0);

    assert!(
        (adapted + 2.5).abs() < 1e-4,
        "manual EV did not reach the buffer: {adapted}"
    );
    assert!(
        (target + 2.5).abs() < 1e-4,
        "manual EV must overwrite the target too: {target}"
    );
}
