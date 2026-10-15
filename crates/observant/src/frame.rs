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

/// A privacy block's edge at 640 pixels wide; other widths scale it, so the picture stays as coarse.
const PRIVACY_BLOCK_AT_640: u32 = 32;

/// How far a kept box grows on each side, as a share of its size: the model's boxes are up to
/// half a second old, and an animal walking out of its box should not show the room behind.
const KEEP_MARGIN: f32 = 0.15;

/// Pixelates the I420 `frame` everywhere but inside the `keep` boxes (pixels: x1, y1, x2, y2),
/// each grown by [`KEEP_MARGIN`]; with no box the whole frame. A block takes its own mean.
pub fn pixelate_outside(frame: &mut [u8], width: u32, height: u32, keep: &[[f32; 4]]) {
    let (w, h) = (width as usize, height as usize);
    // Even, so a luma block is whole chroma blocks.
    let block = ((PRIVACY_BLOCK_AT_640 * width / 640).max(8) & !1) as usize;
    let kept: Vec<[usize; 4]> = keep.iter().map(|r| grown(*r, w, h)).collect();
    let (luma, chroma) = frame.split_at_mut(w * h);
    let (u_plane, v_plane) = chroma.split_at_mut(w * h / 4);
    pixelate_plane(luma, w, h, block, &kept, 1);
    pixelate_plane(u_plane, w / 2, h / 2, block / 2, &kept, 2);
    pixelate_plane(v_plane, w / 2, h / 2, block / 2, &kept, 2);
}

/// `rect` grown by [`KEEP_MARGIN`] and clamped to the frame, as whole pixels x1, y1, x2, y2 (exclusive).
fn grown(rect: [f32; 4], w: usize, h: usize) -> [usize; 4] {
    let [x1, y1, x2, y2] = rect;
    let (mx, my) = ((x2 - x1).abs() * KEEP_MARGIN, (y2 - y1).abs() * KEEP_MARGIN);
    let clamp = |v: f32, len: usize| (v.max(0.0) as usize).min(len);
    [
        clamp((x1 - mx).floor(), w),
        clamp((y1 - my).floor(), h),
        clamp((x2 + mx).ceil(), w),
        clamp((y2 + my).ceil(), h),
    ]
}

/// One plane of [`pixelate_outside`]; `scale` maps its pixels back to luma pixels for `kept`.
fn pixelate_plane(
    plane: &mut [u8],
    w: usize,
    h: usize,
    block: usize,
    kept: &[[usize; 4]],
    scale: usize,
) {
    let inside = |x: usize, y: usize| {
        let (x, y) = (x * scale, y * scale);
        kept.iter()
            .any(|k| x >= k[0] && x < k[2] && y >= k[1] && y < k[3])
    };
    for by in (0..h).step_by(block) {
        let y2 = (by + block).min(h);
        for bx in (0..w).step_by(block) {
            let x2 = (bx + block).min(w);
            let sum: usize = (by..y2)
                .flat_map(|row| &plane[row * w + bx..row * w + x2])
                .map(|&p| usize::from(p))
                .sum();
            let mean = (sum / ((x2 - bx) * (y2 - by))) as u8;
            let touches = kept.iter().any(|k| {
                bx * scale < k[2] && x2 * scale > k[0] && by * scale < k[3] && y2 * scale > k[1]
            });
            for row in by..y2 {
                if !touches {
                    plane[row * w + bx..row * w + x2].fill(mean);
                    continue;
                }
                for col in bx..x2 {
                    if !inside(col, row) {
                        plane[row * w + col] = mean;
                    }
                }
            }
        }
    }
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
    fn each_pixel_takes_its_own_luma_and_its_own_2x2_blocks_chroma() {
        // 6x4: three by two chroma blocks, each its own colour, and a luma ramp across the frame.
        let (w, h) = (6usize, 4usize);
        let luma: Vec<u8> = (0..w * h).map(|i| 20 + 8 * i as u8).collect();
        let u: Vec<u8> = (0..6).map(|i| 60 + 25 * i).collect();
        let v: Vec<u8> = (0..6).map(|i| 200 - 20 * i).collect();
        let frame = [luma.clone(), u.clone(), v.clone()].concat();
        let rgba = to_rgba(&frame, w as u32, h as u32);
        for (row, col) in (0..h).flat_map(|r| (0..w).map(move |c| (r, c))) {
            let block = (row / 2) * (w / 2) + col / 2;
            let alone = [vec![luma[row * w + col]; 4], vec![u[block]], vec![v[block]]].concat();
            let at = (row * w + col) * 4;
            assert_eq!(
                rgba[at..at + 4],
                to_rgba(&alone, 2, 2)[..4],
                "pixel ({col}, {row})"
            );
        }
    }

    /// Which pixels of a `w`-wide plane an outline of `rect` (inclusive) and `t` pixels marks.
    fn outline_mask(w: usize, h: usize, [x1, y1, x2, y2]: [usize; 4], t: usize) -> Vec<bool> {
        (0..w * h)
            .map(|i| (i % w, i / w))
            .map(|(x, y)| {
                let on_box = (x1..=x2).contains(&x) && (y1..=y2).contains(&y);
                on_box && (y < y1 + t || y + t > y2 || x < x1 + t || x + t > x2)
            })
            .collect()
    }

    #[test]
    fn an_outline_is_its_thickness_in_luma_half_that_in_chroma_and_nothing_else() {
        let (w, h) = (32usize, 24usize);
        let mut f = gray(w as u32, h as u32, 100);
        draw_rect(&mut f, w as u32, h as u32, [4.0, 4.0, 28.0, 20.0], GREEN, 4);
        let planes = [
            (0, w, h, [4, 4, 28, 20], 4, GREEN[0], 100),
            (w * h, w / 2, h / 2, [2, 2, 14, 10], 2, GREEN[1], 128),
            (
                w * h * 5 / 4,
                w / 2,
                h / 2,
                [2, 2, 14, 10],
                2,
                GREEN[2],
                128,
            ),
        ];
        for (start, pw, ph, rect, t, value, background) in planes {
            let mask = outline_mask(pw, ph, rect, t);
            for (i, &marked) in mask.iter().enumerate() {
                let want = if marked { value } else { background };
                assert_eq!(
                    f[start + i],
                    want,
                    "plane at {start}, ({}, {})",
                    i % pw,
                    i / pw
                );
            }
        }
    }

    #[test]
    fn an_outline_on_the_last_row_or_column_stays_in_its_plane() {
        let (w, h) = (16usize, 12usize);
        let marked = |rect: [f32; 4]| {
            let mut f = gray(w as u32, h as u32, 100);
            draw_rect(&mut f, w as u32, h as u32, rect, GREEN, 2);
            assert!(
                f[w * h..].iter().all(|&p| p != GREEN[0]),
                "{rect:?} reached chroma"
            );
            (0..w * h)
                .filter(|&i| f[i] == GREEN[0])
                .map(|i| (i % w, i / w))
                .collect::<Vec<_>>()
        };
        // A line thicker than its box grows inward, so a one-pixel-wide box marks the column before it.
        let column: Vec<_> = (8..=11).flat_map(|y| [(14, y), (15, y)]).collect();
        assert_eq!(marked([15.0, 8.0, 15.0, 11.0]), column);
        let row: Vec<_> = (10..=11)
            .flat_map(|y| (10..=13).map(move |x| (x, y)))
            .collect();
        assert_eq!(marked([10.0, 11.0, 13.0, 11.0]), row);
    }

    #[test]
    fn a_box_with_its_corners_swapped_draws_nothing_and_a_one_pixel_wide_one_a_line() {
        let (w, h) = (16usize, 12usize);
        let mut f = gray(w as u32, h as u32, 100);
        draw_rect(&mut f, w as u32, h as u32, [10.0, 2.0, 5.0, 8.0], GREEN, 2);
        assert_eq!(f, gray(w as u32, h as u32, 100));
        draw_rect(&mut f, w as u32, h as u32, [6.0, 2.0, 6.0, 8.0], GREEN, 1);
        let column: Vec<usize> = (0..w * h).filter(|&i| f[i] == GREEN[0]).collect();
        assert_eq!(column, (2..=8).map(|y| y * w + 6).collect::<Vec<_>>());
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

    /// A frame whose every luma pixel differs from its neighbours: x + 3y, wrapped.
    fn ramp(w: u32, h: u32) -> Vec<u8> {
        let mut f = vec![128u8; size(w, h)];
        for y in 0..h as usize {
            for x in 0..w as usize {
                f[y * w as usize + x] = ((x + 3 * y) % 256) as u8;
            }
        }
        f
    }

    #[test]
    fn with_no_box_every_block_is_one_value() {
        let (w, h) = (640u32, 480u32);
        let mut f = ramp(w, h);
        pixelate_outside(&mut f, w, h, &[]);
        // 32-pixel blocks at 640 wide: each holds one value, its mean.
        for (bx, by) in [(0usize, 0usize), (320, 224), (608, 448)] {
            let first = f[by * 640 + bx];
            for y in by..by + 32 {
                assert!(f[y * 640 + bx..y * 640 + bx + 32]
                    .iter()
                    .all(|&p| p == first));
            }
        }
        assert_ne!(f[0], f[32]);
    }

    #[test]
    fn a_kept_box_and_its_margin_stay_sharp_and_the_rest_is_blocks() {
        let (w, h) = (640u32, 480u32);
        let original = ramp(w, h);
        let mut f = original.clone();
        // 100 x 100 at (200, 200): the margin grows it by 15 each side, to 185..315.
        pixelate_outside(&mut f, w, h, &[[200.0, 200.0, 300.0, 300.0]]);
        let at = |f: &[u8], x: usize, y: usize| f[y * 640 + x];
        for (x, y) in [(200, 200), (299, 299), (186, 186), (314, 314), (250, 186)] {
            assert_eq!(at(&f, x, y), at(&original, x, y), "({x}, {y}) kept");
        }
        // Outside the grown box, the block that holds (0, 0) and one beside the box are flat.
        assert_ne!(at(&f, 1, 0), at(&original, 1, 0));
        assert_eq!(at(&f, 1, 0), at(&f, 31, 31));
        assert_eq!(at(&f, 160, 250), at(&f, 170, 250));
        // Chroma follows: the planes keep their size, and a block outside is flat there too.
        assert_eq!(f.len(), size(w, h));
        let u = &f[(w * h) as usize..];
        assert!(u[..16].iter().all(|&p| p == u[0]));
    }

    /// [`ramp`] with chroma ramps too: U is x + y and V is 2x + y, per chroma pixel.
    fn ramp_all(w: u32, h: u32) -> Vec<u8> {
        let mut f = ramp(w, h);
        let (cw, ch) = (w as usize / 2, h as usize / 2);
        let luma = (w * h) as usize;
        for y in 0..ch {
            for x in 0..cw {
                f[luma + y * cw + x] = ((x + y) % 256) as u8;
                f[luma + cw * ch + y * cw + x] = ((2 * x + y) % 256) as u8;
            }
        }
        f
    }

    #[test]
    fn a_block_takes_its_own_mean() {
        let mut f = ramp(640, 480);
        pixelate_outside(&mut f, 640, 480, &[]);
        // x + 3y over a 32-pixel block: 15.5 + 46.5 at (0, 0), and 32 more each way at (32, 32).
        assert_eq!(f[0], 62);
        assert_eq!(f[32 * 640 + 32], 190);
    }

    #[test]
    fn the_margin_is_15_percent_of_the_box_each_way_and_no_more() {
        let original = ramp(640, 480);
        let mut f = original.clone();
        // 100 x 40 at (200, 100): kept from 185 to 315 across and from 94 to 146 down, ends exclusive.
        pixelate_outside(&mut f, 640, 480, &[[200.0, 100.0, 300.0, 140.0]]);
        let same = |x: usize, y: usize| f[y * 640 + x] == original[y * 640 + x];
        for (x, y) in [(185, 120), (314, 120), (250, 94), (250, 145)] {
            assert!(same(x, y), "({x}, {y}) is inside the margin");
        }
        for (x, y) in [(184, 120), (315, 120), (250, 93), (250, 146)] {
            assert!(!same(x, y), "({x}, {y}) is outside the margin");
        }
    }

    #[test]
    fn chroma_is_kept_inside_the_box_and_blocks_outside() {
        let original = ramp_all(640, 480);
        let mut f = original.clone();
        pixelate_outside(&mut f, 640, 480, &[[200.0, 100.0, 300.0, 140.0]]);
        let luma = 640 * 480;
        for plane in [luma, luma + 320 * 240] {
            let at = |f: &[u8], x: usize, y: usize| f[plane + y * 320 + x];
            // Chroma (93, 60) is luma (186, 120), inside; (92, 60) is luma 184, outside.
            assert_eq!(at(&f, 93, 60), at(&original, 93, 60), "plane at {plane}");
            assert_eq!(at(&f, 157, 72), at(&original, 157, 72), "plane at {plane}");
            assert_ne!(at(&f, 92, 60), at(&original, 92, 60), "plane at {plane}");
            // A 16-pixel chroma block far from the box is one value, not its ramp.
            let block: Vec<u8> = (0..16).map(|x| at(&f, 16 + x, 200)).collect();
            assert!(
                block.iter().all(|&p| p == block[0]),
                "plane at {plane}: {block:?}"
            );
            assert_ne!(at(&f, 16, 200), at(&original, 16, 200), "plane at {plane}");
            // Chroma blocks are 16 wide, the same area as luma's 32: the one before is its own.
            assert_ne!(at(&f, 0, 200), at(&f, 16, 200), "plane at {plane}");
        }
    }

    #[test]
    fn a_box_over_the_edge_and_an_odd_width_do_not_panic() {
        let (w, h) = (100u32, 50u32);
        let mut f = ramp(w, h);
        pixelate_outside(&mut f, w, h, &[[-20.0, -20.0, 500.0, 500.0]]);
        assert_eq!(f, ramp(w, h));
        let mut g = ramp(w, h);
        pixelate_outside(&mut g, w, h, &[[90.0, 40.0, 99.0, 49.0]]);
        assert_eq!(g.len(), size(w, h));
    }
}
