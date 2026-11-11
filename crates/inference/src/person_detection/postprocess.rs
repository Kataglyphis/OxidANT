use super::preprocess;
use super::preprocess::ImageMapping;
use anyhow::{bail, Result};
use kataglyphis_core::detection::Detection;

pub(crate) fn parse_yolo_like_detections(
    output_shape: &[usize],
    data: &[f32],
    score_threshold: f32,
    mapping: ImageMapping,
    swap_xy: bool,
) -> Result<Vec<Detection>> {
    let (n, stride) = match output_shape.len() {
        2 => {
            let (n, s) = (output_shape[0], output_shape[1]);
            if s < 6 {
                bail!(
                    "Unexpected output shape {:?}; need at least 6 columns",
                    output_shape
                );
            }
            (n, s)
        }
        3 => {
            if output_shape[0] != 1 {
                bail!(
                    "Unexpected output shape {:?}; expected batch=1",
                    output_shape
                );
            }
            let (n, s) = (output_shape[1], output_shape[2]);
            if s < 6 {
                bail!(
                    "Unexpected output shape {:?}; need at least 6 columns",
                    output_shape
                );
            }
            (n, s)
        }
        _ => bail!("Unsupported output rank: {:?}", output_shape),
    };

    let mut detections = Vec::with_capacity(n.min(64));

    for i in 0..n {
        let row = i * stride;
        if row + 6 > data.len() {
            break;
        }

        let mut x1 = data[row];
        let mut y1 = data[row + 1];
        let mut x2 = data[row + 2];
        let mut y2 = data[row + 3];
        let score = data[row + 4];
        let class_id = data[row + 5] as i64;

        if !score.is_finite() || score < score_threshold {
            continue;
        }
        // A NaN corner survives every clamp and reaches the overlay as a box that cannot be drawn.
        if ![x1, y1, x2, y2].iter().all(|corner| corner.is_finite()) {
            continue;
        }

        if swap_xy {
            std::mem::swap(&mut x1, &mut y1);
            std::mem::swap(&mut x2, &mut y2);
        }

        let (x1, y1) = preprocess::unmap_point(x1, y1, mapping);
        let (x2, y2) = preprocess::unmap_point(x2, y2, mapping);

        detections.push(Detection {
            x1: x1.clamp(0.0, mapping.src_w as f32),
            y1: y1.clamp(0.0, mapping.src_h as f32),
            x2: x2.clamp(0.0, mapping.src_w as f32),
            y2: y2.clamp(0.0, mapping.src_h as f32),
            score,
            class_id,
        });
    }

    Ok(detections)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapping(src_w: u32, src_h: u32) -> ImageMapping {
        ImageMapping {
            src_w,
            src_h,
            scale_x: 1.0,
            scale_y: 1.0,
            pad_x: 0.0,
            pad_y: 0.0,
        }
    }

    #[test]
    fn test_parse_yolo_like_detections_empty() {
        let shape = vec![0usize, 6];
        let data: Vec<f32> = vec![];
        let mapping = make_mapping(100, 100);
        let result = parse_yolo_like_detections(&shape, &data, 0.5, mapping, false);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_yolo_like_detections_below_threshold() {
        let shape = vec![1usize, 6];
        let data: Vec<f32> = vec![10.0, 20.0, 30.0, 40.0, 0.3, 0.0];
        let mapping = make_mapping(100, 100);
        let result = parse_yolo_like_detections(&shape, &data, 0.5, mapping, false);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_yolo_like_detections_single() {
        let shape = vec![1usize, 6];
        let data: Vec<f32> = vec![10.0, 20.0, 30.0, 40.0, 0.8, 0.0];
        let mapping = make_mapping(100, 100);
        let result = parse_yolo_like_detections(&shape, &data, 0.5, mapping, false);
        assert!(result.is_ok());
        let detections = result.unwrap();
        assert_eq!(detections.len(), 1);
        let d = &detections[0];
        assert!((d.x1 - 10.0).abs() < 0.001);
        assert!((d.y1 - 20.0).abs() < 0.001);
        assert!((d.x2 - 30.0).abs() < 0.001);
        assert!((d.y2 - 40.0).abs() < 0.001);
        assert!((d.score - 0.8).abs() < 0.001);
        assert_eq!(d.class_id, 0);
    }

    #[test]
    fn test_parse_yolo_like_detections_3d_shape() {
        let shape = vec![1usize, 1, 6];
        let data: Vec<f32> = vec![5.0, 10.0, 15.0, 20.0, 0.9, 1.0];
        let mapping = make_mapping(50, 50);
        let result = parse_yolo_like_detections(&shape, &data, 0.5, mapping, false);
        assert!(result.is_ok());
        let detections = result.unwrap();
        assert_eq!(detections.len(), 1);
    }

    #[test]
    fn test_parse_yolo_like_detections_swap_xy() {
        let shape = vec![1usize, 6];
        let data: Vec<f32> = vec![10.0, 20.0, 30.0, 40.0, 0.7, 0.0];
        let mapping = make_mapping(100, 100);
        let result = parse_yolo_like_detections(&shape, &data, 0.5, mapping, true);
        assert!(result.is_ok());
        let d = &result.unwrap()[0];
        assert!((d.x1 - 20.0).abs() < 0.001); // swapped
        assert!((d.y1 - 10.0).abs() < 0.001); // swapped
    }

    #[test]
    fn a_row_with_a_non_finite_corner_is_no_detection() {
        let shape = vec![4usize, 6];
        let mut data = Vec::new();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            data.extend([10.0, bad, 30.0, 40.0, 0.9, 0.0]);
        }
        data.extend([10.0, 20.0, 30.0, 40.0, 0.9, 0.0]);
        let found = parse_yolo_like_detections(&shape, &data, 0.5, make_mapping(100, 100), false);
        let found = found.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].x1, found[0].y1), (10.0, 20.0));
    }

    #[test]
    fn boxes_are_unmapped_through_the_letterbox_and_clamped_to_the_frame() {
        // A 200x100 frame letterboxed into 100x100: scale 0.5, 25 rows of padding above.
        let mapping = ImageMapping {
            src_w: 200,
            src_h: 100,
            scale_x: 0.5,
            scale_y: 0.5,
            pad_x: 0.0,
            pad_y: 25.0,
        };
        let data = vec![
            10.0, 35.0, 60.0, 60.0, 0.9, 15.0, -5.0, 0.0, 120.0, 99.0, 0.8, 0.0,
        ];
        let found = parse_yolo_like_detections(&[2, 6], &data, 0.5, mapping, false).unwrap();
        let corners = |d: &Detection| [d.x1, d.y1, d.x2, d.y2];
        assert_eq!(corners(&found[0]), [20.0, 20.0, 120.0, 70.0]);
        // Padding and overshoot land on the frame's edge, never outside it.
        assert_eq!(corners(&found[1]), [0.0, 0.0, 200.0, 100.0]);
    }

    #[test]
    fn test_parse_yolo_like_detections_invalid_shape() {
        let shape = vec![1usize, 4usize];
        let data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        let mapping = make_mapping(100, 100);
        let result = parse_yolo_like_detections(&shape, &data, 0.5, mapping, false);
        assert!(result.is_err());
    }
}
