//! Run the application's detector on a real image for package verification.

use std::path::Path;

use anyhow::{Context, Result};

pub fn run(
    image_path: &Path,
    output: &Path,
    profile: &Path,
    repeat_output: Option<&Path>,
) -> Result<()> {
    let image = image::ImageReader::open(image_path)
        .with_context(|| format!("Cannot open image {}", image_path.display()))?
        .with_guessed_format()?
        .decode()?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let model = crate::person_detection::resolve_model_path(None);
    let mut detector =
        crate::person_detection::PersonDetector::with_ort_profiling(&model, profile)?;
    let found = detector.infer_rgba(image.as_raw(), width, height, 0.25, None)?;
    let repeated = repeat_output
        .map(|_| detector.infer_rgba(image.as_raw(), width, height, 0.25, None))
        .transpose()?;
    detector.end_ort_profiling()?;
    write_outputs(
        output,
        repeat_output.zip(repeated.as_deref()),
        width,
        height,
        &found,
    )
}

fn write_outputs(
    output: &Path,
    repeated: Option<(&Path, &[crate::Detection])>,
    width: u32,
    height: u32,
    found: &[crate::Detection],
) -> Result<()> {
    write_results(output, width, height, found)?;
    if let Some((path, results)) = repeated {
        write_results(path, width, height, results)?;
    }
    Ok(())
}

fn write_results(output: &Path, width: u32, height: u32, found: &[crate::Detection]) -> Result<()> {
    let detections: Vec<_> = found
        .iter()
        .map(|item| {
            serde_json::json!({
                "class": item.class_id,
                "score": item.score,
                "box": [item.x1, item.y1, item.x2, item.y2],
            })
        })
        .collect();
    let document = serde_json::json!({
        "schema_version": 1,
        "width": width,
        "height": height,
        "detections": detections,
    });
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .with_context(|| format!("Cannot create results {}", output.display()))?;
    serde_json::to_writer(file, &document)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_outputs;

    fn detection(score: f32) -> crate::Detection {
        crate::Detection {
            x1: 1.0,
            y1: 2.0,
            x2: 3.0,
            y2: 4.0,
            score,
            class_id: 15,
        }
    }

    #[test]
    fn writes_both_results_in_the_detector_schema_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.json");
        let second = dir.path().join("second.json");
        let first_detections = [detection(0.5)];
        let second_detections = [detection(0.75)];
        write_outputs(
            &first,
            Some((&second, &second_detections)),
            200,
            100,
            &first_detections,
        )
        .unwrap();
        let expected = serde_json::json!({
            "schema_version": 1,
            "width": 200,
            "height": 100,
            "detections": [{"class": 15, "score": 0.5, "box": [1.0, 2.0, 3.0, 4.0]}],
        });
        let bytes = std::fs::read(&first).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            expected
        );
        let repeated: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&second).unwrap()).unwrap();
        assert_eq!(repeated["detections"][0]["score"], 0.75);
        assert!(write_outputs(&first, None, 200, 100, &first_detections).is_err());
        assert_eq!(std::fs::read(&first).unwrap(), bytes);
    }

    #[test]
    fn repeated_output_collisions_fail_without_replacing_the_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("same.json");
        let first_detections = [detection(0.5)];
        let second_detections = [detection(0.75)];
        assert!(write_outputs(
            &first,
            Some((&first, &second_detections)),
            200,
            100,
            &first_detections,
        )
        .is_err());
        let first_result: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&first).unwrap()).unwrap();
        assert_eq!(first_result["detections"][0]["score"], 0.5);

        let new_first = dir.path().join("new-first.json");
        let existing_repeat = dir.path().join("existing-repeat.json");
        std::fs::write(&existing_repeat, b"preserve").unwrap();
        assert!(write_outputs(
            &new_first,
            Some((&existing_repeat, &second_detections)),
            200,
            100,
            &first_detections,
        )
        .is_err());
        assert_eq!(std::fs::read(&existing_repeat).unwrap(), b"preserve");
    }
}
