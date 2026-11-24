//! A model's `[n, 6]` or `[1, n, 6]` rows of x1, y1, x2, y2, score and class to detections in frame pixels.

use crate::detection::Detection;
use crate::preprocess::{unmap_point, ImageMapping};
use std::fmt;

/// Why a model's output could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// A row has fewer than six values.
    TooFewColumns { shape: Vec<usize> },
    /// A rank-3 output whose batch is not 1.
    BatchNotOne { shape: Vec<usize> },
    /// Neither `[n, s]` nor `[1, n, s]`.
    UnsupportedRank { shape: Vec<usize> },
    /// The shape needs more values than arrived.
    ShorterThanShape {
        values: usize,
        needed: usize,
        shape: Vec<usize>,
    },
    /// The shape's value count is beyond what this machine can address.
    SizeOverflow { shape: Vec<usize> },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewColumns { shape } => {
                write!(
                    f,
                    "Unexpected output shape {shape:?}; need at least 6 columns"
                )
            }
            Self::BatchNotOne { shape } => {
                write!(f, "Unexpected output shape {shape:?}; expected batch=1")
            }
            Self::UnsupportedRank { shape } => write!(f, "Unsupported output rank: {shape:?}"),
            Self::ShorterThanShape {
                values,
                needed,
                shape,
            } => write!(
                f,
                "the output holds {values} values where its shape {shape:?} needs {needed}"
            ),
            Self::SizeOverflow { shape } => write!(f, "the output shape {shape:?} overflows"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Rows and values per row of a `[n, s]` or `[1, n, s]` output, checked against how many values arrived.
fn rows_and_stride(output_shape: &[usize], values: usize) -> Result<(usize, usize), DecodeError> {
    let shape = || output_shape.to_vec();
    let (n, stride) = match *output_shape {
        [n, s] => (n, s),
        [1, n, s] => (n, s),
        [_, _, _] => return Err(DecodeError::BatchNotOne { shape: shape() }),
        _ => return Err(DecodeError::UnsupportedRank { shape: shape() }),
    };
    if stride < 6 {
        return Err(DecodeError::TooFewColumns { shape: shape() });
    }
    let needed = n
        .checked_mul(stride)
        .ok_or_else(|| DecodeError::SizeOverflow { shape: shape() })?;
    // A shorter buffer used to end the rows early without a word; the shape and the data must agree.
    if values < needed {
        return Err(DecodeError::ShorterThanShape {
            values,
            needed,
            shape: shape(),
        });
    }
    Ok((n, stride))
}

/// The detections of every row ModelANT's decode rules keep, in row order; a class at or past `class_count` is none.
pub fn parse_yolo_like_detections(
    output_shape: &[usize],
    data: &[f32],
    score_threshold: f32,
    mapping: ImageMapping,
    swap_xy: bool,
    class_count: usize,
) -> Result<Vec<Detection>, DecodeError> {
    let (n, stride) = rows_and_stride(output_shape, data.len())?;
    let mut detections = Vec::with_capacity(n.min(64));

    for row in data.chunks_exact(stride).take(n) {
        let [mut x1, mut y1, mut x2, mut y2, score, class, ..] = *row else {
            continue;
        };

        if !score.is_finite() || score < score_threshold {
            continue;
        }
        // A class without a name is no detection in any detector of the family; the bare cast would make NaN a person.
        if class.is_nan() || class < 0.0 || class >= class_count as f32 {
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

        let (x1, y1) = unmap_point(x1, y1, mapping);
        let (x2, y2) = unmap_point(x2, y2, mapping);

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

    const COCO: usize = 80;

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

    fn parse(
        shape: &[usize],
        data: &[f32],
        mapping: ImageMapping,
    ) -> Result<Vec<Detection>, DecodeError> {
        parse_yolo_like_detections(shape, data, 0.5, mapping, false, COCO)
    }

    #[test]
    fn an_output_shorter_than_its_shape_is_an_error_not_fewer_rows() {
        let err = parse(&[2, 6], &[0.0; 6], make_mapping(640, 640)).unwrap_err();
        assert_eq!(
            err,
            DecodeError::ShorterThanShape {
                values: 6,
                needed: 12,
                shape: vec![2, 6]
            }
        );
        let err = err.to_string();
        assert!(err.contains("holds 6 values"), "{err}");
        assert!(err.contains("needs 12"), "{err}");
    }

    #[test]
    fn a_shape_whose_value_count_overflows_is_an_error() {
        let err = parse(&[usize::MAX, 6], &[], make_mapping(640, 640)).unwrap_err();
        assert_eq!(
            err,
            DecodeError::SizeOverflow {
                shape: vec![usize::MAX, 6]
            }
        );
    }

    #[test]
    fn test_parse_yolo_like_detections_empty() {
        assert_eq!(parse(&[0, 6], &[], make_mapping(100, 100)), Ok(vec![]));
    }

    #[test]
    fn test_parse_yolo_like_detections_below_threshold() {
        let data = [10.0, 20.0, 30.0, 40.0, 0.3, 0.0];
        assert_eq!(parse(&[1, 6], &data, make_mapping(100, 100)), Ok(vec![]));
    }

    #[test]
    fn a_row_at_the_threshold_is_kept() {
        let data = [10.0, 20.0, 30.0, 40.0, 0.5, 0.0];
        assert_eq!(
            parse(&[1, 6], &data, make_mapping(100, 100)).unwrap().len(),
            1
        );
    }

    #[test]
    fn test_parse_yolo_like_detections_single() {
        let data = [10.0, 20.0, 30.0, 40.0, 0.8, 0.0];
        let detections = parse(&[1, 6], &data, make_mapping(100, 100)).unwrap();
        let expected = Detection {
            x1: 10.0,
            y1: 20.0,
            x2: 30.0,
            y2: 40.0,
            score: 0.8,
            class_id: 0,
        };
        assert_eq!(detections, [expected]);
    }

    #[test]
    fn test_parse_yolo_like_detections_3d_shape() {
        let data = [5.0, 10.0, 15.0, 20.0, 0.9, 1.0];
        assert_eq!(
            parse(&[1, 1, 6], &data, make_mapping(50, 50))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn rows_wider_than_six_values_are_read_at_their_stride() {
        let data = [
            10.0, 20.0, 30.0, 40.0, 0.9, 2.0, -1.0, 11.0, 21.0, 31.0, 41.0, 0.8, 3.0, -1.0,
        ];
        let found = parse(&[2, 7], &data, make_mapping(100, 100)).unwrap();
        let classes: Vec<i64> = found.iter().map(|d| d.class_id).collect();
        assert_eq!(classes, [2, 3]);
    }

    #[test]
    fn test_parse_yolo_like_detections_swap_xy() {
        let data = [10.0, 20.0, 30.0, 40.0, 0.7, 0.0];
        let found =
            parse_yolo_like_detections(&[1, 6], &data, 0.5, make_mapping(100, 100), true, COCO);
        let d = found.unwrap()[0];
        assert_eq!((d.x1, d.y1, d.x2, d.y2), (20.0, 10.0, 40.0, 30.0));
    }

    #[test]
    fn a_row_with_a_non_finite_corner_is_no_detection() {
        let mut data = Vec::new();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            data.extend([10.0, bad, 30.0, 40.0, 0.9, 0.0]);
        }
        data.extend([10.0, 20.0, 30.0, 40.0, 0.9, 0.0]);
        let found = parse(&[4, 6], &data, make_mapping(100, 100)).unwrap();
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
        let found = parse(&[9, 6], &data, make_mapping(100, 100)).unwrap();
        let found: Vec<(i64, [f32; 4])> = found
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
    fn the_class_count_comes_per_call() {
        let data = [
            10.0, 10.0, 50.0, 50.0, 0.9, 0.0, 10.0, 10.0, 50.0, 50.0, 0.9, 1.9,
        ];
        let classes = |count| {
            let found = parse_yolo_like_detections(
                &[2, 6],
                &data,
                0.5,
                make_mapping(100, 100),
                false,
                count,
            );
            found
                .unwrap()
                .iter()
                .map(|d| d.class_id)
                .collect::<Vec<_>>()
        };
        assert_eq!(classes(2), [0, 1]);
        assert_eq!(classes(1), [0]);
        assert_eq!(classes(0), Vec::<i64>::new());
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
        let data = [
            10.0, 35.0, 60.0, 60.0, 0.9, 15.0, -5.0, 0.0, 120.0, 99.0, 0.8, 0.0,
        ];
        let found = parse(&[2, 6], &data, mapping).unwrap();
        let corners = |d: &Detection| [d.x1, d.y1, d.x2, d.y2];
        assert_eq!(corners(&found[0]), [20.0, 20.0, 120.0, 70.0]);
        // Padding and overshoot land on the frame's edge, never outside it.
        assert_eq!(corners(&found[1]), [0.0, 0.0, 200.0, 100.0]);
    }

    #[test]
    fn x_and_y_unmap_through_their_own_scale() {
        let mapping = ImageMapping {
            src_w: 400,
            src_h: 400,
            scale_x: 0.5,
            scale_y: 0.25,
            pad_x: 0.0,
            pad_y: 0.0,
        };
        let found = parse(&[1, 6], &[10.0, 10.0, 20.0, 20.0, 0.9, 0.0], mapping).unwrap();
        assert_eq!(
            [found[0].x1, found[0].y1, found[0].x2, found[0].y2],
            [20.0, 40.0, 40.0, 80.0]
        );
    }

    #[test]
    fn shapes_that_are_no_rows_of_six_are_typed_errors() {
        let mapping = make_mapping(100, 100);
        assert_eq!(
            parse(&[1, 4], &[0.0; 4], mapping),
            Err(DecodeError::TooFewColumns { shape: vec![1, 4] })
        );
        assert_eq!(
            parse(&[1, 1, 5], &[0.0; 5], mapping),
            Err(DecodeError::TooFewColumns {
                shape: vec![1, 1, 5]
            })
        );
        assert_eq!(
            parse(&[2, 1, 6], &[0.0; 12], mapping),
            Err(DecodeError::BatchNotOne {
                shape: vec![2, 1, 6]
            })
        );
        assert_eq!(
            parse(&[6], &[0.0; 6], mapping),
            Err(DecodeError::UnsupportedRank { shape: vec![6] })
        );
        assert!(parse(&[2, 1, 6], &[0.0; 12], mapping)
            .unwrap_err()
            .to_string()
            .contains("batch=1"));
    }
}
