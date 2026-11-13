//! What a model says about itself in its ONNX metadata, held against what the detector is about to do.

use anyhow::{bail, Result};
use kataglyphis_core::config::PreprocessMode;

/// The one layout this detector decodes; its rows get no overlap suppression.
const DECODED_LAYOUT: &str = "xyxy_score_class";

/// The two keys ModelANT's contract lists, as the file carries them; `None` for a key it does not carry.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ModelSays {
    preprocess: Option<String>,
    output: Option<String>,
}

impl ModelSays {
    /// Read from the file itself, so ONNX Runtime and tract get the same answer; a file that cannot be read says nothing.
    pub(crate) fn read(model_path: &str) -> Self {
        use tract_onnx::prelude::Framework;
        let proto = match tract_onnx::onnx().proto_model_for_path(model_path) {
            Ok(proto) => proto,
            Err(err) => {
                // The backend names the fault of such a file better than a metadata reader could.
                log::debug!("no metadata read from {model_path}: {err:#}");
                return Self::default();
            }
        };
        let value = |key: &str| {
            proto
                .metadata_props
                .iter()
                .find(|entry| entry.key == key && !entry.value.is_empty())
                .map(|entry| entry.value.clone())
        };
        Self {
            preprocess: value("kataglyphis.preprocess"),
            output: value("kataglyphis.output"),
        }
    }

    /// Fails when the model names another preprocessing or another layout: either gives wrong boxes on every frame.
    pub(crate) fn check(&self, preprocess: PreprocessMode) -> Result<()> {
        let ours = match preprocess {
            PreprocessMode::Stretch => "stretch",
            PreprocessMode::Letterbox => "letterbox",
        };
        if let Some(named) = self.preprocess.as_deref() {
            if named != ours {
                bail!(
                    "the model names its preprocessing {named} (kataglyphis.preprocess), the detector is set to {ours}: set KATAGLYPHIS_PREPROCESS={named}"
                );
            }
        }
        if let Some(named) = self.output.as_deref() {
            if named != DECODED_LAYOUT {
                bail!(
                    "the model names its output layout {named} (kataglyphis.output), this detector decodes {DECODED_LAYOUT} only"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn says(preprocess: Option<&str>, output: Option<&str>) -> ModelSays {
        ModelSays {
            preprocess: preprocess.map(str::to_owned),
            output: output.map(str::to_owned),
        }
    }

    #[test]
    fn a_model_that_says_nothing_or_what_the_detector_does_passes() {
        for mode in [PreprocessMode::Stretch, PreprocessMode::Letterbox] {
            says(None, None).check(mode).unwrap();
            says(None, Some("xyxy_score_class")).check(mode).unwrap();
        }
        says(Some("stretch"), Some("xyxy_score_class"))
            .check(PreprocessMode::Stretch)
            .unwrap();
        says(Some("letterbox"), None)
            .check(PreprocessMode::Letterbox)
            .unwrap();
    }

    #[test]
    fn another_preprocessing_or_another_layout_is_an_error_that_names_both() {
        let err = says(Some("letterbox"), None)
            .check(PreprocessMode::Stretch)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("names its preprocessing letterbox")
                && err.contains("set to stretch")
                && err.contains("KATAGLYPHIS_PREPROCESS=letterbox"),
            "{err}"
        );
        let err = says(Some("stretch"), None)
            .check(PreprocessMode::Letterbox)
            .unwrap_err()
            .to_string();
        assert!(err.contains("preprocessing stretch") && err.contains("set to letterbox"));
        let err = says(Some("stretch"), Some("cxcywh_scores_by_anchor"))
            .check(PreprocessMode::Stretch)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("output layout cxcywh_scores_by_anchor")
                && err.contains("decodes xyxy_score_class only"),
            "{err}"
        );
    }

    /// An ONNX model file that holds nothing but metadata: field 14, each entry a key (1) and a value (2).
    fn metadata_only(entries: &[(&str, &str)]) -> Vec<u8> {
        let text = |tag: u8, text: &str| [&[tag, text.len() as u8], text.as_bytes()].concat();
        entries
            .iter()
            .flat_map(|(key, value)| {
                let entry = [text(0x0A, key), text(0x12, value)].concat();
                [&[0x72, entry.len() as u8], entry.as_slice()].concat()
            })
            .collect()
    }

    #[test]
    fn the_keys_are_read_from_the_file_and_a_file_that_is_no_model_says_nothing() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("model-says-{}.onnx", std::process::id()));
        std::fs::write(
            &path,
            metadata_only(&[
                ("producer", "someone"),
                ("kataglyphis.preprocess", "letterbox"),
                ("kataglyphis.output", "xyxy_score_class"),
            ]),
        )
        .unwrap();
        let read = ModelSays::read(path.to_str().unwrap());
        assert_eq!(read, says(Some("letterbox"), Some("xyxy_score_class")));
        // An empty value names nothing.
        std::fs::write(&path, metadata_only(&[("kataglyphis.preprocess", "")])).unwrap();
        assert_eq!(
            ModelSays::read(path.to_str().unwrap()),
            ModelSays::default()
        );
        std::fs::write(&path, b"\xff\xff this is not an ONNX protobuf").unwrap();
        assert_eq!(
            ModelSays::read(path.to_str().unwrap()),
            ModelSays::default()
        );
        std::fs::remove_file(&path).ok();
        assert_eq!(
            ModelSays::read(path.to_str().unwrap()),
            ModelSays::default()
        );
    }
}
