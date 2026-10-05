//! Split-sum IBL checked against properties the maths guarantees, not driver-dependent golden images.
//! A dropped sin(theta) solid-angle weight doubles irradiance yet looks plausible on a rendered frame.

use kataglyphis_webgpu_renderer::render::ibl::{
    BrdfLut, IblEnvironment, BRDF_LUT_SIZE, IRRADIANCE_SIZE, PREFILTER_MIPS, PREFILTER_SIZE,
};
use kataglyphis_webgpu_renderer::{
    decode_hdr, load_gltf, EquirectImage, ForwardRenderer, GpuContext, OrbitCamera,
};

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/cube.gltf")
}

/// Bright above the equator, dark below: its variance is resolution-independent across mips.
fn split_environment(width: u32, height: u32, upper: f32, lower: f32) -> EquirectImage {
    let mut rgba32f = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let value = if y < height / 2 { upper } else { lower };
        for _ in 0..width {
            rgba32f.extend_from_slice(&[value, value, value, 1.0]);
        }
    }
    EquirectImage::new(width, height, rgba32f).expect("split environment is well formed")
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len() as f32
}

fn variance(values: &[f32]) -> f32 {
    let m = mean(values);
    values.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / values.len() as f32
}

#[test]
fn a_constant_environment_convolves_to_its_own_radiance() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // Constant L gives E = PI * L and the map stores E / PI, so every texel reads back L.
    let radiance = [0.25f32, 0.5, 0.7];
    let environment = IblEnvironment::bake(&gpu, &EquirectImage::constant(64, 32, radiance));

    let mut worst = 0.0f32;
    for face in 0..6u32 {
        let texels = environment.read_irradiance_face(&gpu, face);
        assert_eq!(texels.len(), (IRRADIANCE_SIZE * IRRADIANCE_SIZE) as usize);
        for texel in &texels {
            for channel in 0..3 {
                let relative = (texel[channel] - radiance[channel]).abs() / radiance[channel];
                worst = worst.max(relative);
            }
        }
    }

    // 0.001 fails a regression to left-endpoint quadrature yet allows half-float storage.
    assert!(
        worst < 0.001,
        "constant environment did not convolve to itself: worst relative error {worst}"
    );
    eprintln!("uniform-environment irradiance: worst relative error {worst:.6}");
}

#[test]
fn a_constant_environment_prefilters_to_its_own_radiance_at_every_roughness() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // A normalised average leaves a constant untouched; a bad `total_weight` darkens with roughness.
    let radiance = [0.4f32, 0.4, 0.4];
    let environment = IblEnvironment::bake(&gpu, &EquirectImage::constant(64, 32, radiance));

    for mip in 0..PREFILTER_MIPS {
        let mut worst = 0.0f32;
        for face in 0..6u32 {
            for texel in environment.read_prefiltered_face(&gpu, face, mip) {
                worst = worst.max((texel[0] - radiance[0]).abs() / radiance[0]);
            }
        }
        assert!(
            worst < 0.01,
            "prefilter mip {mip} (roughness {}) drifted from a constant environment: {worst}",
            mip as f32 / (PREFILTER_MIPS - 1) as f32
        );
        eprintln!("prefilter mip {mip}: worst relative error {worst:.5}");
    }
}

#[test]
fn irradiance_never_exceeds_the_brightest_radiance_in_the_environment() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // (1/PI) * integral of L cos(theta) dw <= L_max; over-counted solid angle breaks it.
    let bright = 4.0f32;
    let image = split_environment(64, 32, bright, 0.0);
    assert_eq!(image.max_radiance(), bright);
    let environment = IblEnvironment::bake(&gpu, &image);

    let mut highest = 0.0f32;
    let mut lowest = f32::INFINITY;
    for face in 0..6u32 {
        for texel in environment.read_irradiance_face(&gpu, face) {
            highest = highest.max(texel[0]);
            lowest = lowest.min(texel[0]);
        }
    }

    assert!(
        highest <= bright * 1.01,
        "irradiance {highest} exceeds the environment's maximum radiance {bright}"
    );
    // Not trivially: up- and down-facing normals must span most of the range.
    assert!(
        highest > bright * 0.8,
        "the up-facing normal should see nearly the full bright hemisphere, got {highest}"
    );
    assert!(
        lowest < bright * 0.2,
        "the down-facing normal should see nearly none of it, got {lowest}"
    );
    eprintln!("bounded-environment irradiance: max {highest:.4}, min {lowest:.4}, bound {bright}");
}

#[test]
fn higher_roughness_prefilter_mips_are_strictly_blurrier() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let environment = IblEnvironment::bake(&gpu, &split_environment(128, 64, 1.0, 0.0));

    // Face 4 (+Z) straddles the equator; blur moves texels to the mean, so variance falls.
    let variances: Vec<f32> = (0..PREFILTER_MIPS)
        .map(|mip| {
            let face: Vec<f32> = environment
                .read_prefiltered_face(&gpu, 4, mip)
                .into_iter()
                .map(|texel| texel[0])
                .collect();
            assert_eq!(
                face.len(),
                ((PREFILTER_SIZE >> mip) * (PREFILTER_SIZE >> mip)) as usize
            );
            variance(&face)
        })
        .collect();

    eprintln!("prefilter variance by mip (roughness 0 -> 1): {variances:?}");
    for mip in 1..PREFILTER_MIPS as usize {
        assert!(
            variances[mip] < variances[mip - 1],
            "mip {mip} (variance {}) is not blurrier than mip {} (variance {})",
            variances[mip],
            mip - 1,
            variances[mip - 1]
        );
    }
    // Substantially: roughness 1.0 should smear the edge nearly flat.
    assert!(
        variances[PREFILTER_MIPS as usize - 1] < variances[0] * 0.25,
        "roughness 1.0 barely blurred anything: {variances:?}"
    );
}

#[test]
fn the_brdf_lut_stays_in_range_and_conserves_energy() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let lut = BrdfLut::new(&gpu);
    let table = lut.read_back(&gpu);
    assert_eq!(table.len(), (BRDF_LUT_SIZE * BRDF_LUT_SIZE) as usize);

    let mut worst_sum = 0.0f32;
    for entry in &table {
        assert!(
            (0.0..=1.0).contains(&entry[0]) && (0.0..=1.0).contains(&entry[1]),
            "BRDF LUT entry out of [0,1]: {entry:?}"
        );
        // scale + bias is the reflectance at F0 = 1, which cannot exceed 1.
        worst_sum = worst_sum.max(entry[0] + entry[1]);
    }
    assert!(
        worst_sum <= 1.0 + 1e-2,
        "BRDF LUT creates energy: max scale + bias = {worst_sum}"
    );
    eprintln!("BRDF LUT: max scale + bias = {worst_sum:.4}");
}

#[test]
fn the_brdf_lut_reproduces_the_known_mirror_and_grazing_behaviour() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let table = BrdfLut::new(&gpu).read_back(&gpu);
    let at = |n_dot_v_index: u32, roughness_index: u32| {
        table[(roughness_index * BRDF_LUT_SIZE + n_dot_v_index) as usize]
    };

    let smooth = 0u32;
    let rough = BRDF_LUT_SIZE - 1;
    let grazing = 0u32;
    let normal_incidence = BRDF_LUT_SIZE - 1;
    // A perfect mirror is lossless: scale + bias is 1 at every angle, Fresnel only splits it.

    for n_dot_v in [grazing, BRDF_LUT_SIZE / 2, normal_incidence] {
        let [scale, bias] = at(n_dot_v, smooth);
        assert!(
            (scale + bias - 1.0).abs() < 0.02,
            "a mirror must be lossless at N.V index {n_dot_v}: {scale} + {bias}"
        );
    }

    // At normal incidence F = F0, so the whole answer is the scale on F0.
    let [scale, bias] = at(normal_incidence, smooth);
    assert!(
        scale > 0.98 && bias < 0.02,
        "mirror at normal incidence should be (1, 0), got ({scale}, {bias})"
    );

    // At grazing incidence Fresnel goes to 1 whatever F0 is, so the bias dominates.
    let [grazing_scale, grazing_bias] = at(grazing, smooth);
    assert!(
        grazing_bias > grazing_scale,
        "grazing Fresnel should be F0-independent, got scale {grazing_scale} bias {grazing_bias}"
    );

    // Roughness costs energy: the Smith term loses light to shadowing and masking.
    let smooth_total = {
        let [s, b] = at(BRDF_LUT_SIZE / 2, smooth);
        s + b
    };
    let rough_total = {
        let [s, b] = at(BRDF_LUT_SIZE / 2, rough);
        s + b
    };
    assert!(
        rough_total < smooth_total,
        "roughness 1 ({rough_total}) must lose more energy than roughness 0 ({smooth_total})"
    );
    eprintln!(
        "BRDF LUT: mirror total {smooth_total:.4}, roughest total {rough_total:.4}, \
         grazing (scale {grazing_scale:.4}, bias {grazing_bias:.4})"
    );
}

#[test]
fn the_equirect_projection_puts_the_sky_on_the_right_faces() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // A flipped latitude or mirrored basis swaps +Y and -Y, which a smooth panorama hides.
    let environment = IblEnvironment::bake(&gpu, &split_environment(128, 64, 1.0, 0.0));
    let face_mean = |face: u32| {
        let values: Vec<f32> = environment
            .read_environment_face(&gpu, face, 0)
            .into_iter()
            .map(|texel| texel[0])
            .collect();
        mean(&values)
    };

    let up = face_mean(2);
    let down = face_mean(3);
    assert!(
        up > 0.95,
        "+Y face must be entirely the bright half, got {up}"
    );
    assert!(
        down < 0.05,
        "-Y face must be entirely the dark half, got {down}"
    );

    // Side faces straddle the equator; one taking latitude from the wrong axis would not.
    for face in [0u32, 1, 4, 5] {
        let side = face_mean(face);
        assert!(
            (side - 0.5).abs() < 0.1,
            "side face {face} should straddle the equator, got mean {side}"
        );
    }
}

/// Renders the bundled cube and returns the frame bytes.
fn render(renderer: &mut ForwardRenderer, gpu: &GpuContext) -> Vec<u8> {
    renderer
        .render_to_pixels(gpu, 128, 128, &OrbitCamera::default())
        .expect("headless render must succeed")
}

#[test]
fn with_no_environment_the_analytic_path_renders_exactly_what_it_always_did() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    assert!(
        !renderer.environment_enabled(),
        "IBL must be off until an environment is set"
    );
    let baseline = render(&mut renderer, &gpu);

    // Setting and clearing an environment must restore the analytic frame byte for byte.
    let mut round_tripped = ForwardRenderer::new(&gpu, 128, 128);
    round_tripped.upload_scene(&gpu, &scene);
    round_tripped.set_environment(&gpu, &EquirectImage::sky(64, 32));
    assert!(round_tripped.environment_enabled());
    round_tripped.clear_environment(&gpu);
    assert!(!round_tripped.environment_enabled());

    assert_eq!(
        baseline,
        render(&mut round_tripped, &gpu),
        "clearing the environment did not restore the analytic path exactly"
    );
}

#[test]
fn setting_an_environment_actually_changes_the_rendered_frame() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // The only test that fails if the baked maps never reach the shader.
    let scene = load_gltf(cube_path()).expect("cube.gltf must load");
    let mut renderer = ForwardRenderer::new(&gpu, 128, 128);
    renderer.upload_scene(&gpu, &scene);
    let analytic = render(&mut renderer, &gpu);

    // Far brighter than the analytic sky, so the cube's shaded side must lift.
    renderer.set_environment(&gpu, &EquirectImage::constant(64, 32, [3.0, 3.0, 3.0]));
    let lit = render(&mut renderer, &gpu);
    assert_ne!(
        analytic, lit,
        "the baked environment never reached the frame"
    );

    // Cube pixels only: the sky pass ignores IBL and would dilute the signal.
    let cube_luma = |pixels: &[u8]| {
        let mut total = 0u64;
        let mut count = 0u64;
        for pixel in pixels.as_chunks::<4>().0 {
            // The cube is red-dominant; the sky is blue-dominant.
            if pixel[0] > pixel[2] {
                total += pixel[0] as u64 + pixel[1] as u64 + pixel[2] as u64;
                count += 1;
            }
        }
        assert!(count > 100, "found only {count} cube pixels to compare");
        total as f64 / count as f64
    };

    let analytic_luma = cube_luma(&analytic);
    let lit_luma = cube_luma(&lit);
    eprintln!("cube mean luma: analytic {analytic_luma:.2}, environment-lit {lit_luma:.2}");
    assert!(
        lit_luma > analytic_luma + 5.0,
        "a 3.0-radiance environment should brighten the cube: {analytic_luma} -> {lit_luma}"
    );

    // A dark environment must push the other way, so adding a constant cannot pass.
    renderer.set_environment(&gpu, &EquirectImage::constant(64, 32, [0.01, 0.01, 0.01]));
    let dim_luma = cube_luma(&render(&mut renderer, &gpu));
    eprintln!("cube mean luma: dark environment {dim_luma:.2}");
    assert!(
        dim_luma < analytic_luma,
        "a near-black environment should darken the cube: {analytic_luma} -> {dim_luma}"
    );
}

/// Encodes a flat (unRLE'd) `.hdr`; test-local, since the renderer only reads `.hdr`.
fn encode_hdr_flat(image: &EquirectImage) -> Vec<u8> {
    let mut out = format!(
        "#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y {} +X {}\n",
        image.height, image.width
    )
    .into_bytes();
    for texel in image.rgba32f.as_chunks::<4>().0 {
        let max = texel[0].max(texel[1]).max(texel[2]);
        if max < 1e-32 {
            out.extend_from_slice(&[0; 4]);
            continue;
        }
        // frexp by hand, matching Radiance's setcolr: max = v * 2^e, v in [0.5, 1).
        let mut e = 0i32;
        let mut v = max;
        while v >= 1.0 {
            v *= 0.5;
            e += 1;
        }
        while v < 0.5 {
            v *= 2.0;
            e -= 1;
        }
        let scale = f64::from(v) * 256.0 / f64::from(max);
        out.extend_from_slice(&[
            (f64::from(texel[0]) * scale) as u8,
            (f64::from(texel[1]) * scale) as u8,
            (f64::from(texel[2]) * scale) as u8,
            (e + 128) as u8,
        ]);
    }
    out
}

#[test]
fn hdr_bytes_decode_and_bake_into_the_same_environment_as_the_source_pixels() {
    // sky -> .hdr -> decode -> bake against baking the sky directly; only the bake needs a GPU.
    let sky = EquirectImage::sky(64, 32);
    let bytes = encode_hdr_flat(&sky);
    let decoded = decode_hdr(&bytes).expect("the encoded sky must decode");
    assert_eq!((decoded.width, decoded.height), (sky.width, sky.height));

    let mut worst = 0.0f32;
    for (got, want) in decoded
        .rgba32f
        .as_chunks::<4>()
        .0
        .iter()
        .zip(sky.rgba32f.as_chunks::<4>().0)
    {
        let max = want[0].max(want[1]).max(want[2]);
        for channel in 0..3 {
            worst = worst.max((got[channel] - want[channel]).abs() / max);
        }
    }
    eprintln!("hdr round-trip of the sky: worst relative error {worst:.6}");
    assert!(
        worst < 1.0 / 128.0,
        "RGBE round-trip error {worst} exceeds its quantum"
    );

    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    let direct = IblEnvironment::bake(&gpu, &sky);
    let via_hdr = IblEnvironment::bake_hdr(&gpu, &bytes).expect("bake_hdr composes decode + bake");

    // Averaging cannot grow the RGBE quantum; 2% also covers half-float storage.
    let mut worst = 0.0f32;
    for face in 0..6u32 {
        let a = direct.read_irradiance_face(&gpu, face);
        let b = via_hdr.read_irradiance_face(&gpu, face);
        for (x, y) in a.iter().zip(&b) {
            for channel in 0..3 {
                worst = worst.max((x[channel] - y[channel]).abs() / x[channel].max(1e-3));
            }
        }
    }
    eprintln!("irradiance from .hdr vs from source pixels: worst relative diff {worst:.6}");
    assert!(
        worst < 0.02,
        "baking the decoded .hdr diverged from the source: {worst}"
    );
}

#[test]
fn the_brdf_table_is_baked_once_and_shared_across_environments() {
    let Some(gpu) = GpuContext::headless_or_skip() else {
        return;
    };

    // The LUT has no environment term, so rebaking it per environment would be waste.
    let mut renderer = ForwardRenderer::new(&gpu, 64, 64);
    assert!(
        renderer.brdf_lut().is_none(),
        "nothing should bake before use"
    );

    renderer.set_environment(&gpu, &EquirectImage::constant(32, 16, [1.0, 1.0, 1.0]));
    let first = renderer
        .brdf_lut()
        .expect("set_environment bakes the LUT")
        .read_back(&gpu);

    renderer.set_environment(&gpu, &EquirectImage::sky(32, 16));
    let second = renderer
        .brdf_lut()
        .expect("the LUT survives")
        .read_back(&gpu);

    assert_eq!(
        first, second,
        "the BRDF LUT must not depend on the environment"
    );
}
