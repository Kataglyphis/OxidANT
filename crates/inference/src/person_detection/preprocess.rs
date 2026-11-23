use anyhow::{bail, Context, Result};

/// Standard YOLO letterbox padding gray (114/255 ≈ 0.447).
const LETTERBOX_FILL_VALUE: f32 = 114.0 / 255.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ImageMapping {
    pub src_w: u32,
    pub src_h: u32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub pad_x: f32,
    pub pad_y: f32,
}

#[inline]
fn validate_rgba(rgba: &[u8], src_w: u32, src_h: u32) -> Result<()> {
    if src_w == 0 || src_h == 0 {
        bail!("Image dimensions must be positive");
    }
    let expected = (src_w as usize)
        .checked_mul(src_h as usize)
        .and_then(|p| p.checked_mul(4))
        .context("Image dimensions overflow")?;
    if rgba.len() != expected {
        bail!(
            "RGBA buffer length mismatch: got {}, expected {} ({}x{}x4)",
            rgba.len(),
            expected,
            src_w,
            src_h
        );
    }
    Ok(())
}

/// `value / 255` for every byte: a lookup gives the floats the division gave, without dividing per pixel.
static UNIT_FLOATS: [f32; 256] = {
    let mut table = [0.0; 256];
    let mut value = 0;
    while value < 256 {
        table[value] = value as f32 / 255.0;
        value += 1;
    }
    table
};

/// Nearest-neighbour resize of `rgba` into the `new_w` x `new_h` block of the three planes at (`pad_x`, `pad_y`).
fn resize_into_planes(
    rgba: &[u8],
    src_w: u32,
    src_h: u32,
    buf: &mut [f32],
    dst_w: u32,
    (pad_x, pad_y): (u32, u32),
    (new_w, new_h): (u32, u32),
) {
    // Every row samples the same columns, so their byte offsets are divided out once per frame.
    let columns: Vec<usize> = (0..new_w as u64)
        .map(|dx| (dx * src_w as u64 / new_w as u64) as usize * 4)
        .collect();
    let plane = buf.len() / 3;
    let (red, rest) = buf.split_at_mut(plane);
    let (green, blue) = rest.split_at_mut(plane);
    let row_bytes = src_w as usize * 4;
    for dy in 0..new_h as usize {
        let sy = (dy as u64 * src_h as u64 / new_h as u64) as usize;
        let row = &rgba[sy * row_bytes..(sy + 1) * row_bytes];
        let start = (dy + pad_y as usize) * dst_w as usize + pad_x as usize;
        let end = start + new_w as usize;
        let planes = red[start..end]
            .iter_mut()
            .zip(&mut green[start..end])
            .zip(&mut blue[start..end]);
        for (((r, g), b), &offset) in planes.zip(&columns) {
            *r = UNIT_FLOATS[row[offset] as usize];
            *g = UNIT_FLOATS[row[offset + 1] as usize];
            *b = UNIT_FLOATS[row[offset + 2] as usize];
        }
    }
}

pub(crate) fn rgba_to_nchw_f32_letterboxed(
    rgba: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    buf: &mut Vec<f32>,
) -> Result<ImageMapping> {
    validate_rgba(rgba, src_w, src_h)?;
    if dst_w == 0 || dst_h == 0 {
        bail!("Model input dimensions must be positive");
    }

    let scale = f64::min(dst_w as f64 / src_w as f64, dst_h as f64 / src_h as f64);
    let new_w = ((src_w as f64 * scale).round() as u32).clamp(1, dst_w);
    let new_h = ((src_h as f64 * scale).round() as u32).clamp(1, dst_h);
    let pad_x_i = (dst_w - new_w) / 2;
    let pad_y_i = (dst_h - new_h) / 2;

    let plane = output_plane(dst_w, dst_h)?;
    let total = 3 * plane;

    buf.clear();
    buf.resize(total, LETTERBOX_FILL_VALUE);

    resize_into_planes(
        rgba,
        src_w,
        src_h,
        buf,
        dst_w,
        (pad_x_i, pad_y_i),
        (new_w, new_h),
    );

    Ok(ImageMapping {
        src_w,
        src_h,
        scale_x: new_w as f32 / src_w as f32,
        scale_y: new_h as f32 / src_h as f32,
        pad_x: pad_x_i as f32,
        pad_y: pad_y_i as f32,
    })
}

pub(crate) fn rgba_to_nchw_f32_stretched(
    rgba: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    buf: &mut Vec<f32>,
) -> Result<ImageMapping> {
    validate_rgba(rgba, src_w, src_h)?;
    if dst_w == 0 || dst_h == 0 {
        bail!("Model input dimensions must be positive");
    }

    let plane = output_plane(dst_w, dst_h)?;
    let total = 3 * plane;

    // Every element is written below, so only a changed size touches the allocation.
    buf.resize(total, 0.0);

    resize_into_planes(rgba, src_w, src_h, buf, dst_w, (0, 0), (dst_w, dst_h));

    Ok(ImageMapping {
        src_w,
        src_h,
        scale_x: dst_w as f32 / src_w.max(1) as f32,
        scale_y: dst_h as f32 / src_h.max(1) as f32,
        pad_x: 0.0,
        pad_y: 0.0,
    })
}

fn output_plane(width: u32, height: u32) -> Result<usize> {
    let plane = (width as usize)
        .checked_mul(height as usize)
        .context("Model input dimensions overflow")?;
    plane
        .checked_mul(3)
        .context("Model input dimensions overflow")?;
    Ok(plane)
}

#[inline]
pub(crate) fn unmap_point(x: f32, y: f32, mapping: ImageMapping) -> (f32, f32) {
    let x = (x - mapping.pad_x) / mapping.scale_x.max(1e-6);
    let y = (y - mapping.pad_y) / mapping.scale_y.max(1e-6);
    (x, y)
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use serde_json::Value;

    fn contract() -> Value {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../third_party/ModelANT/contracts/inference-v1.json"
        )))
        .unwrap()
    }

    /// A case's strided RGB or RGBA bytes as the RGBA frame the transforms take, alpha opaque.
    fn rgba_from(item: &Value, data: &[u8]) -> Vec<u8> {
        let u = |key: &str| item[key].as_u64().unwrap() as usize;
        let channels = if item["pixel_format"] == "RGB" { 3 } else { 4 };
        let (offset, stride) = (u("plane_offset_bytes"), u("row_stride_bytes"));
        let mut rgba = Vec::with_capacity(u("width") * u("height") * 4);
        for y in 0..u("height") {
            for x in 0..u("width") {
                let index = offset + y * stride + x * channels;
                rgba.extend_from_slice(&data[index..index + 3]);
                rgba.push(255);
            }
        }
        rgba
    }

    /// The production transform the case's mode names, its NCHW output and mapping.
    fn transformed(item: &Value, rgba: &[u8]) -> (Vec<f32>, ImageMapping) {
        let u = |key: &str| item[key].as_u64().unwrap() as u32;
        let transform = if item["mode"] == "letterbox" {
            rgba_to_nchw_f32_letterboxed
        } else {
            rgba_to_nchw_f32_stretched
        };
        let mut output = Vec::new();
        let mapping = transform(
            rgba,
            u("width"),
            u("height"),
            u("input_width"),
            u("input_height"),
            &mut output,
        )
        .unwrap();
        (output, mapping)
    }

    #[test]
    fn matches_the_shared_modelant_contract() {
        let contract = contract();
        assert_eq!(contract["schema_version"], 1);
        for item in contract["cases"].as_array().unwrap() {
            let data: Vec<u8> = item["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|byte| byte.as_u64().unwrap() as u8)
                .collect();
            let (output, mapping) = transformed(item, &rgba_from(item, &data));
            let expected = item["expected_nchw"].as_array().unwrap();
            assert_eq!(output.len(), expected.len(), "{}", item["name"]);
            for (actual, expected) in output.iter().zip(expected) {
                assert!(
                    (*actual - expected.as_f64().unwrap() as f32).abs() < 1e-6,
                    "{}",
                    item["name"]
                );
            }
            for (actual, key) in [
                (mapping.scale_x, "scale_x"),
                (mapping.scale_y, "scale_y"),
                (mapping.pad_x, "pad_x"),
                (mapping.pad_y, "pad_y"),
            ] {
                assert!(
                    (actual - item["mapping"][key].as_f64().unwrap() as f32).abs() < 1e-6,
                    "{} {key}",
                    item["name"]
                );
            }
        }
    }

    /// ModelANT's full-size frames (hub CON288 (e)): the tensor bit for bit, by its samples and its SHA-256.
    #[test]
    fn full_size_frames_match_their_digest() {
        use sha2::{Digest, Sha256};
        let contract = contract();
        let cases = contract["digest_cases"]["cases"].as_array().unwrap();
        assert!(cases.len() >= 4, "the digest cases shrank: {}", cases.len());
        for item in cases {
            let u = |key: &str| item[key].as_u64().unwrap() as usize;
            let channels = item["pixel_format"].as_str().unwrap().len();
            let (offset, stride) = (u("plane_offset_bytes"), u("row_stride_bytes"));
            // digest_cases.frame: each byte from x, y and the channel, 171 in every padding and offset byte.
            let mut data = vec![171_u8; offset + stride * u("height")];
            for y in 0..u("height") {
                for x in 0..u("width") {
                    for c in 0..channels {
                        data[offset + y * stride + x * channels + c] =
                            ((7 * x + 13 * y + 61 * c) % 256) as u8;
                    }
                }
            }
            let (output, _) = transformed(item, &rgba_from(item, &data));
            let (width, height) = (u("input_width"), u("input_height"));
            for sample in item["samples"].as_array().unwrap() {
                let at = |i: usize| sample[i].as_u64().unwrap() as usize;
                let value = output[at(0) * width * height + at(1) * width + at(2)];
                assert_eq!(
                    value.to_bits(),
                    (sample[3].as_f64().unwrap() as f32).to_bits(),
                    "{} {sample}",
                    item["name"]
                );
            }
            let mut digest = Sha256::new();
            for value in &output {
                digest.update(value.to_le_bytes());
            }
            assert_eq!(
                format!("{:x}", digest.finalize()),
                item["expected_sha256"].as_str().unwrap(),
                "{}",
                item["name"]
            );
        }
    }

    /// The formula the shared contract fixes, one division per pixel and channel.
    fn reference(rgba: &[u8], src: (u32, u32), dst: (u32, u32), letterbox: bool) -> Vec<f32> {
        let ((src_w, src_h), (dst_w, dst_h)) = (src, dst);
        let (new_w, new_h, fill) = if letterbox {
            let scale = f64::min(dst_w as f64 / src_w as f64, dst_h as f64 / src_h as f64);
            let new_w = ((src_w as f64 * scale).round() as u32).clamp(1, dst_w);
            let new_h = ((src_h as f64 * scale).round() as u32).clamp(1, dst_h);
            (new_w, new_h, LETTERBOX_FILL_VALUE)
        } else {
            (dst_w, dst_h, 0.0)
        };
        let (pad_x, pad_y) = ((dst_w - new_w) / 2, (dst_h - new_h) / 2);
        let plane = (dst_w * dst_h) as usize;
        let mut out = vec![fill; 3 * plane];
        for dy in 0..new_h {
            let sy = (dy as u64 * src_h as u64 / new_h as u64) as usize;
            for dx in 0..new_w {
                let sx = (dx as u64 * src_w as u64 / new_w as u64) as usize;
                let at = ((dy + pad_y) * dst_w + dx + pad_x) as usize;
                for channel in 0..3 {
                    let byte = rgba[(sy * src_w as usize + sx) * 4 + channel];
                    out[channel * plane + at] = byte as f32 / 255.0;
                }
            }
        }
        out
    }

    #[test]
    fn the_tables_give_the_per_pixel_formulas_floats_bit_for_bit() {
        let sizes = [
            ((37, 23), (16, 12)),
            ((8, 40), (32, 32)),
            ((5, 3), (64, 48)),
        ];
        for (src, dst) in sizes {
            let rgba: Vec<u8> = (0..src.0 * src.1 * 4)
                .map(|i| (i * 37 % 256) as u8)
                .collect();
            // A buffer left over from a larger, other frame: nothing of it may survive.
            let mut output = vec![9.0; 7 * (dst.0 * dst.1) as usize];
            for letterbox in [false, true] {
                let transform = if letterbox {
                    rgba_to_nchw_f32_letterboxed
                } else {
                    rgba_to_nchw_f32_stretched
                };
                transform(&rgba, src.0, src.1, dst.0, dst.1, &mut output).unwrap();
                let expected = reference(&rgba, src, dst, letterbox);
                assert_eq!(output.len(), expected.len(), "{src:?} -> {dst:?}");
                let same = output
                    .iter()
                    .zip(&expected)
                    .all(|(a, b)| a.to_bits() == b.to_bits());
                assert!(same, "{src:?} -> {dst:?}, letterbox {letterbox}");
            }
        }
    }

    #[test]
    fn every_byte_normalises_to_its_quotient() {
        for value in 0..=255u8 {
            assert_eq!(
                UNIT_FLOATS[value as usize].to_bits(),
                (value as f32 / 255.0).to_bits()
            );
        }
    }

    #[test]
    fn zero_dimensions_are_errors() {
        let mut output = Vec::new();
        assert!(rgba_to_nchw_f32_letterboxed(&[], 0, 1, 2, 2, &mut output).is_err());
        assert!(rgba_to_nchw_f32_stretched(&[0; 4], 1, 1, 0, 2, &mut output).is_err());
    }
}
