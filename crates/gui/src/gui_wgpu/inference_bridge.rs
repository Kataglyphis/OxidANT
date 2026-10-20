use ::std::sync::Arc;
use ::std::time::Instant;

use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

use kataglyphis_core::detection::Detection;

pub(crate) struct InferRequest {
    pub frame_id: u64,
    pub rgba: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    pub score_threshold: f32,
}

pub(crate) struct InferResult {
    pub frame_id: u64,
    pub detections: Vec<Detection>,
    pub error: Option<String>,
}

pub(crate) type InferChannels = (
    Option<SyncSender<InferRequest>>,
    Option<Receiver<InferResult>>,
    Option<String>,
    Option<String>,
);

pub(crate) fn spawn_inference_thread() -> InferChannels {
    use kataglyphis_inference::person_detection::{resolve_model_path, PersonDetector};
    use kataglyphis_telemetry::resource_monitor;

    let path = resolve_model_path(None);

    let (req_tx, req_rx) = sync_channel::<InferRequest>(1);
    let (res_tx, res_rx) = sync_channel::<InferResult>(1);

    let detector = match PersonDetector::new(&path) {
        Ok(detector) => Some(detector),
        Err(e) => {
            let err_msg = format!("Failed to load model '{path}': {e:#}");
            log::error!("{err_msg}");
            let _ = res_tx.try_send(InferResult {
                frame_id: 0,
                detections: Vec::new(),
                error: Some(err_msg),
            });
            None
        }
    };

    std::thread::spawn(move || {
        let Some(mut detector) = detector else {
            return;
        };

        while let Ok(req) = req_rx.recv() {
            let infer_start = Instant::now();
            let result = match detector.infer_persons_rgba(
                &req.rgba,
                req.width,
                req.height,
                req.score_threshold,
            ) {
                Ok(dets) => InferResult {
                    frame_id: req.frame_id,
                    detections: dets,
                    error: None,
                },
                Err(e) => {
                    let err_msg = format!("Inference failed: {e:#}");
                    log::warn!("{err_msg}");
                    InferResult {
                        frame_id: req.frame_id,
                        detections: Vec::new(),
                        error: Some(err_msg),
                    }
                }
            };

            resource_monitor::record_inference_duration(infer_start.elapsed());
            resource_monitor::record_inference_completion();

            if let Err(e) = res_tx.try_send(result) {
                log::warn!("Inference result channel full, dropping result: {e}");
            }
        }
    });

    (Some(req_tx), Some(res_rx), None, Some(path))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_thread_answers_a_frame_or_says_why_it_cannot() {
        let (tx, rx, error, path) = spawn_inference_thread();
        let (tx, rx) = (tx.unwrap(), rx.unwrap());
        assert_eq!(error, None);
        let path = path.unwrap();
        if !std::path::Path::new(&path).is_file() {
            assert!(
                std::env::var("KATAGLYPHIS_REQUIRE_MODEL").as_deref() != Ok("1"),
                "no model at {path}"
            );
            let result = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(
                result
                    .error
                    .as_deref()
                    .is_some_and(|e| e.starts_with("Failed to load model")),
                "{:?}",
                result.error
            );
            return;
        }
        tx.send(InferRequest {
            frame_id: 9,
            rgba: Arc::from(vec![0u8; 64 * 64 * 4]),
            width: 64,
            height: 64,
            score_threshold: 0.5,
        })
        .unwrap();
        let result = rx.recv_timeout(Duration::from_secs(120)).unwrap();
        assert_eq!((result.frame_id, result.error), (9, None));
        assert!(result.detections.is_empty(), "{:?}", result.detections);
    }
}
