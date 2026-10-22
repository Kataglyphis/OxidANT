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

#[inline]
fn write_pixel_chw(chw: &mut [f32], plane: usize, dst_idx: usize, r: f32, g: f32, b: f32) {
    chw[dst_idx] = r;
    chw[plane + dst_idx] = g;
    chw[2 * plane + dst_idx] = b;
}

#[inline]
fn sample_rgba_normalised(src: &[u8], sx: u32, sy: u32, src_w: u32) -> (f32, f32, f32) {
    let idx = (sy as usize * src_w as usize + sx as usize) * 4;
    let r = src[idx] as f32 / 255.0;
    let g = src[idx + 1] as f32 / 255.0;
    let b = src[idx + 2] as f32 / 255.0;
    (r, g, b)
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

    let content_x_end = pad_x_i + new_w;
    let content_y_end = pad_y_i + new_h;

    for dy in pad_y_i..content_y_end {
        let sy = ((dy - pad_y_i) as u64 * src_h as u64 / new_h as u64) as u32;
        for dx in pad_x_i..content_x_end {
            let sx = ((dx - pad_x_i) as u64 * src_w as u64 / new_w as u64) as u32;
            let dst_idx = dy as usize * dst_w as usize + dx as usize;
            let (r, g, b) = sample_rgba_normalised(rgba, sx, sy, src_w);
            write_pixel_chw(buf, plane, dst_idx, r, g, b);
        }
    }

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

    buf.clear();
    buf.resize(total, 0.0);

    for dy in 0..dst_h {
        let sy = (dy as u64 * src_h as u64 / dst_h as u64) as u32;
        for dx in 0..dst_w {
            let sx = (dx as u64 * src_w as u64 / dst_w as u64) as u32;
            let dst_idx = dy as usize * dst_w as usize + dx as usize;
            let (r, g, b) = sample_rgba_normalised(rgba, sx, sy, src_w);
            write_pixel_chw(buf, plane, dst_idx, r, g, b);
        }
    }

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

    #[test]
    fn matches_the_shared_modelant_contract() {
        let contract: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../third_party/ModelANT/contracts/inference-v1.json"
        )))
        .unwrap();
        assert_eq!(contract["schema_version"], 1);
        for item in contract["cases"].as_array().unwrap() {
            let u = |key: &str| item[key].as_u64().unwrap() as u32;
            let width = u("width");
            let height = u("height");
            let channels = if item["pixel_format"] == "RGB" { 3 } else { 4 };
            let offset = u("plane_offset_bytes") as usize;
            let stride = u("row_stride_bytes") as usize;
            let data = item["data"].as_array().unwrap();
            let mut rgba = Vec::new();
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let index = offset + y * stride + x * channels;
                    for c in 0..3 {
                        rgba.push(data[index + c].as_u64().unwrap() as u8);
                    }
                    rgba.push(255);
                }
            }
            let mut output = Vec::new();
            let transform = if item["mode"] == "letterbox" {
                rgba_to_nchw_f32_letterboxed
            } else {
                rgba_to_nchw_f32_stretched
            };
            let mapping = transform(
                &rgba,
                width,
                height,
                u("input_width"),
                u("input_height"),
                &mut output,
            )
            .unwrap();
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

    #[test]
    fn zero_dimensions_are_errors() {
        let mut output = Vec::new();
        assert!(rgba_to_nchw_f32_letterboxed(&[], 0, 1, 2, 2, &mut output).is_err());
        assert!(rgba_to_nchw_f32_stretched(&[0; 4], 1, 1, 0, 2, &mut output).is_err());
    }
}
