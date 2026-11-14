use super::preprocess;
use super::preprocess::ImageMapping;
use anyhow::{bail, Result};
use kataglyphis_core::coco;
use kataglyphis_core::detection::Detection;

/// Rows and values per row of a `[n, s]` or `[1, n, s]` output, checked against how many values arrived.
fn rows_and_stride(output_shape: &[usize], values: usize) -> Result<(usize, usize)> {
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

    // A shorter buffer used to end the rows early without a word; the shape and the data must agree.
    anyhow::ensure!(
        values >= n * stride,
        "the output holds {values} values where its shape {output_shape:?} needs {}",
        n * stride
    );
    Ok((n, stride))
}

pub(crate) fn parse_yolo_like_detections(
    output_shape: &[usize],
    data: &[f32],
    score_threshold: f32,
    mapping: ImageMapping,
    swap_xy: bool,
) -> Result<Vec<Detection>> {
    let (n, stride) = rows_and_stride(output_shape, data.len())?;
    let mut detections = Vec::with_capacity(n.min(64));

    for i in 0..n {
        let row = i * stride;

        let mut x1 = data[row];
        let mut y1 = data[row + 1];
        let mut x2 = data[row + 2];
        let mut y2 = data[row + 3];
        let score = data[row + 4];
        let class = data[row + 5];

        if !score.is_finite() || score < score_threshold {
            continue;
        }
        // A class without a name is no detection in any detector of the family; the bare cast would make NaN a person.
        if class.is_nan() || class < 0.0 || class >= coco::NAMES.len() as f32 {
            continue;
        }
        // A NaN corner survives every clamp and reaches the overlay as a box that cannot be drawn.
        if ![x1, y1, x2, y2].iter().all(|corner| corner.is_finite()) {
            continue;
        }
        // Corners in the wrong order are no box: the family's three detectors drop such a row.
        if x2 < x1 || y2 < y1 {
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
            class_id: class as i64,
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
    fn an_output_shorter_than_its_shape_is_an_error_not_fewer_rows() {
        let err =
            parse_yolo_like_detections(&[2, 6], &[0.0; 6], 0.5, make_mapping(640, 640), false)
                .unwrap_err()
                .to_string();
        assert!(err.contains("holds 6 values"), "{err}");
        assert!(err.contains("needs 12"), "{err}");
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
    fn a_row_with_no_such_class_or_with_its_corners_swapped_is_no_detection() {
        let mut data = Vec::new();
        data.extend([10.0, 10.0, 50.0, 50.0, 0.9, 15.0]);
        for class in [80.0, -1.0, -0.5, f32::NAN, f32::INFINITY] {
            data.extend([10.0, 10.0, 50.0, 50.0, 0.8, class]);
        }
        data.extend([50.0, 10.0, 10.0, 50.0, 0.8, 15.0]);
        data.extend([10.0, 50.0, 50.0, 10.0, 0.8, 15.0]);
        // A point has its corners in order, so it stays, as it does in the other two detectors.
        data.extend([20.0, 20.0, 20.0, 20.0, 0.7, 79.0]);
        let found = parse_yolo_like_detections(&[9, 6], &data, 0.5, make_mapping(100, 100), false);
        let found: Vec<(i64, [f32; 4])> = found
            .unwrap()
            .iter()
            .map(|d| (d.class_id, [d.x1, d.y1, d.x2, d.y2]))
            .collect();
        assert_eq!(
            found,
            [
                (15, [10.0, 10.0, 50.0, 50.0]),
                (79, [20.0, 20.0, 20.0, 20.0])
            ]
        );
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

    /// A row's number; JSON has no NaN or infinity, so the contract spells them as strings that `f32` parses.
    fn number(value: &serde_json::Value) -> f32 {
        match value.as_str() {
            Some(text) => text.parse().unwrap(),
            None => value.as_f64().unwrap() as f32,
        }
    }

    #[test]
    fn replays_the_decode_cases_of_the_shared_modelant_contract() {
        let contract: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../third_party/ModelANT/contracts/inference-v1.json"
        )))
        .unwrap();
        let decode = &contract["decode"];
        assert_eq!(decode["layout"], "xyxy_score_class");
        assert_eq!(decode["order"], "row");
        // The cases are written for the class count this decoder checks against.
        let last_class = contract["output"]["class_range"][1].as_u64().unwrap();
        assert_eq!(last_class as usize + 1, coco::NAMES.len());
        let score_tolerance = decode["score_tolerance"].as_f64().unwrap() as f32;
        let coordinate_tolerance = decode["coordinate_tolerance"].as_f64().unwrap() as f32;
        let cases = decode["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = &case["name"];
            let size = |key: &str| case[key].as_u64().unwrap() as u32;
            let map = |key: &str| case["mapping"][key].as_f64().unwrap() as f32;
            let mapping = ImageMapping {
                src_w: size("width"),
                src_h: size("height"),
                scale_x: map("scale_x"),
                scale_y: map("scale_y"),
                pad_x: map("pad_x"),
                pad_y: map("pad_y"),
            };
            let rows = case["rows"].as_array().unwrap();
            let data: Vec<f32> = rows
                .iter()
                .flat_map(|row| row.as_array().unwrap().iter().map(number))
                .collect();
            let threshold = case["score_threshold"].as_f64().unwrap() as f32;
            let found =
                parse_yolo_like_detections(&[1, rows.len(), 6], &data, threshold, mapping, false)
                    .unwrap();
            let expected = case["expected"].as_array().unwrap();
            assert_eq!(found.len(), expected.len(), "{name}");
            // In row order: the contract compares the list entry by entry.
            for (found, expected) in found.iter().zip(expected) {
                assert_eq!(
                    found.class_id,
                    expected["class"].as_i64().unwrap(),
                    "{name}"
                );
                let score = expected["score"].as_f64().unwrap() as f32;
                assert!((found.score - score).abs() <= score_tolerance, "{name}");
                let corners = [found.x1, found.y1, found.x2, found.y2];
                for (corner, expected) in corners.iter().zip(expected["box"].as_array().unwrap()) {
                    let expected = expected.as_f64().unwrap() as f32;
                    assert!(
                        (corner - expected).abs() <= coordinate_tolerance,
                        "{name}: {corners:?}"
                    );
                }
            }
        }
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
