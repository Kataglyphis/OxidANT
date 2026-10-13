//! Frames travel from capture to output as I420, which a Pi camera delivers and the encoder takes,
//! so neither end converts; RGB is made only for the frames the model takes and for snapshots.

use gstreamer as gst;

use crate::config::Config;

/// The boxes' green in BT.601 limited-range YUV.
pub const GREEN: [u8; 3] = [145, 54, 34];

/// Bytes in one I420 frame: a full luma plane and two quarter-size chroma planes.
pub fn size(width: u32, height: u32) -> usize {
    let pixels = width as usize * height as usize;
    pixels + pixels / 2
}

/// The caps from capture to output.
pub fn caps(config: &Config) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", "I420")
        .field("width", config.width as i32)
        .field("height", config.height as i32)
        .field("framerate", gst::Fraction::new(config.fps as i32, 1))
        // Square pixels: videoscale then borders a picture turned by 90 degrees instead of stretching it.
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build()
}

/// I420 to RGBA with BT.601 limited-range coefficients, the ones `videoconvert` takes for these caps.
pub fn to_rgba(frame: &[u8], width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let (luma, chroma) = frame.split_at(w * h);
    let (u_plane, v_plane) = chroma.split_at(w * h / 4);
    let mut rgba = vec![255u8; w * h * 4];
    for (row, out_row) in rgba.chunks_exact_mut(w * 4).enumerate() {
        let chroma_row = (row / 2) * (w / 2);
        let (pixels, _) = out_row.as_chunks_mut::<4>();
        for (col, px) in pixels.iter_mut().enumerate() {
            let y = (i32::from(luma[row * w + col]) - 16) * 298;
            let u = i32::from(u_plane[chroma_row + col / 2]) - 128;
            let v = i32::from(v_plane[chroma_row + col / 2]) - 128;
            px[0] = channel(y + 409 * v);
            px[1] = channel(y - 100 * u - 208 * v);
            px[2] = channel(y + 516 * u);
        }
    }
    rgba
}

fn channel(scaled: i32) -> u8 {
    ((scaled + 128) >> 8).clamp(0, 255) as u8
}

/// A box outline in an I420 frame: `thickness` pixels in luma, half that in chroma.
pub fn draw_rect(
    frame: &mut [u8],
    width: u32,
    height: u32,
    rect: [f32; 4],
    yuv: [u8; 3],
    thickness: usize,
) {
    let (w, h) = (width as usize, height as usize);
    let (luma, chroma) = frame.split_at_mut(w * h);
    let (u_plane, v_plane) = chroma.split_at_mut(w * h / 4);
    outline(luma, w, h, rect, yuv[0], thickness);
    let half = rect.map(|v| v / 2.0);
    let chroma_thickness = (thickness / 2).max(1);
    outline(u_plane, w / 2, h / 2, half, yuv[1], chroma_thickness);
    outline(v_plane, w / 2, h / 2, half, yuv[2], chroma_thickness);
}

fn outline(plane: &mut [u8], w: usize, h: usize, rect: [f32; 4], value: u8, thickness: usize) {
    let clamp = |v: f32, len: usize| (v.round().max(0.0) as usize).min(len - 1);
    let [x1, y1, x2, y2] = rect;
    let (x1, y1, x2, y2) = (clamp(x1, w), clamp(y1, h), clamp(x2, w), clamp(y2, h));
    if x1 > x2 || y1 > y2 {
        return;
    }
    for t in 0..thickness {
        for y in [(y1 + t).min(h - 1), y2.saturating_sub(t)] {
            plane[y * w + x1..=y * w + x2].fill(value);
        }
        for y in y1..=y2 {
            plane[y * w + (x1 + t).min(w - 1)] = value;
            plane[y * w + x2.saturating_sub(t)] = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(w: u32, h: u32, luma: u8) -> Vec<u8> {
        let mut f = vec![128u8; size(w, h)];
        f[..(w * h) as usize].fill(luma);
        f
    }

    #[test]
    fn a_frame_is_a_luma_plane_and_two_quarter_chroma_planes() {
        assert_eq!(size(640, 480), 640 * 480 * 3 / 2);
    }

    #[test]
    fn limited_range_black_white_and_green_convert_to_rgb() {
        let px = |f: &[u8]| to_rgba(f, 2, 2)[..4].to_vec();
        assert_eq!(px(&gray(2, 2, 16)), [0, 0, 0, 255]);
        assert_eq!(px(&gray(2, 2, 235)), [255, 255, 255, 255]);
        let green = [GREEN[0]; 4]
            .into_iter()
            .chain([GREEN[1], GREEN[2]])
            .collect::<Vec<_>>();
        let rgb = px(&green);
        assert!(rgb[0] < 8 && rgb[1] > 247 && rgb[2] < 8, "{rgb:?}");
    }

    #[test]
    fn a_box_marks_its_outline_in_every_plane_and_leaves_the_inside_alone() {
        let (w, h) = (16u32, 16u32);
        let mut f = gray(w, h, 100);
        draw_rect(&mut f, w, h, [2.0, 2.0, 13.0, 13.0], GREEN, 2);
        let luma = |x: usize, y: usize| f[y * 16 + x];
        assert_eq!(luma(2, 2), GREEN[0]);
        assert_eq!(luma(13, 8), GREEN[0]);
        assert_eq!(luma(8, 8), 100);
        let u = |x: usize, y: usize| f[256 + y * 8 + x];
        let v = |x: usize, y: usize| f[256 + 64 + y * 8 + x];
        assert_eq!((u(1, 1), v(1, 1)), (GREEN[1], GREEN[2]));
        assert_eq!((u(4, 4), v(4, 4)), (128, 128));
    }

    #[test]
    fn a_box_that_leaves_the_frame_is_clamped_and_a_backward_one_is_skipped() {
        let (w, h) = (8u32, 8u32);
        let mut f = gray(w, h, 100);
        draw_rect(&mut f, w, h, [-10.0, -10.0, 40.0, 40.0], GREEN, 4);
        assert_eq!(f[0], GREEN[0]);
        let mut g = gray(w, h, 100);
        draw_rect(&mut g, w, h, [6.0, 6.0, 2.0, 2.0], GREEN, 1);
        assert_eq!(g, gray(w, h, 100));
    }
}
