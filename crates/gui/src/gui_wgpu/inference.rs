use ::std::sync::Arc;
use ::std::time::{Duration, Instant};

use std::sync::mpsc::{Receiver, SyncSender};

use kataglyphis_core::detection::Detection;

use super::inference_bridge::{InferRequest, InferResult};
use super::pipeline::Frame;

pub(crate) struct InferenceState {
    pub detector_error: Option<String>,
    pub model_path: Option<String>,
    pub score_threshold: f32,
    pub infer_every: Duration,
    pub last_infer: Instant,
    pub last_detections: Vec<Detection>,
    pub last_detections_frame_id: Option<u64>,
    pub infer_tx: Option<SyncSender<InferRequest>>,
    pub infer_rx: Option<Receiver<InferResult>>,
    pub infer_in_flight: bool,
    pub infer_enabled: bool,
}

impl InferenceState {
    pub(crate) fn poll(&mut self) {
        if let Some(rx) = self.infer_rx.as_ref() {
            while let Ok(res) = rx.try_recv() {
                self.infer_in_flight = false;
                self.last_detections = res.detections;
                self.last_detections_frame_id = Some(res.frame_id);
                self.detector_error = res.error;
            }
        }
    }

    pub(crate) fn maybe_infer(&mut self, frame: &Frame) {
        let Some(tx) = self.infer_tx.as_ref() else {
            return;
        };

        if !self.infer_enabled {
            return;
        }

        if self.last_infer.elapsed() < self.infer_every {
            return;
        }

        if self.infer_in_flight {
            return;
        }

        if tx
            .try_send(InferRequest {
                frame_id: frame.id,
                rgba: Arc::clone(&frame.data),
                width: frame.width,
                height: frame.height,
                score_threshold: self.score_threshold,
            })
            .is_ok()
        {
            self.last_infer = Instant::now();
            self.infer_in_flight = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::sync_channel;

    use super::*;

    fn state(
        every: Duration,
    ) -> (
        InferenceState,
        Receiver<InferRequest>,
        SyncSender<InferResult>,
    ) {
        let (req_tx, req_rx) = sync_channel(1);
        let (res_tx, res_rx) = sync_channel(1);
        let state = InferenceState {
            detector_error: None,
            model_path: None,
            score_threshold: 0.5,
            infer_every: every,
            last_infer: Instant::now() - Duration::from_secs(1),
            last_detections: Vec::new(),
            last_detections_frame_id: None,
            infer_tx: Some(req_tx),
            infer_rx: Some(res_rx),
            infer_in_flight: false,
            infer_enabled: true,
        };
        (state, req_rx, res_tx)
    }

    fn frame(id: u64) -> Frame {
        Frame {
            id,
            data: Arc::from(vec![0u8; 4]),
            width: 1,
            height: 1,
        }
    }

    #[test]
    fn one_request_is_in_flight_at_a_time() {
        let (mut state, req_rx, res_tx) = state(Duration::ZERO);
        state.maybe_infer(&frame(1));
        state.maybe_infer(&frame(2));
        assert_eq!(req_rx.try_recv().unwrap().frame_id, 1);
        assert!(
            req_rx.try_recv().is_err(),
            "a second request while one is in flight"
        );

        res_tx
            .send(InferResult {
                frame_id: 1,
                detections: vec![Detection::default()],
                error: None,
            })
            .unwrap();
        state.poll();
        assert!(!state.infer_in_flight);
        assert_eq!(state.last_detections_frame_id, Some(1));
        assert_eq!(state.last_detections.len(), 1);

        state.maybe_infer(&frame(3));
        assert_eq!(req_rx.try_recv().unwrap().frame_id, 3);
    }

    #[test]
    fn nothing_is_sent_while_disabled_or_too_soon() {
        let (mut state, req_rx, _res_tx) = state(Duration::from_secs(3600));
        state.last_infer = Instant::now();
        state.maybe_infer(&frame(1));
        assert!(req_rx.try_recv().is_err(), "sent before infer_every passed");

        state.infer_every = Duration::ZERO;
        state.infer_enabled = false;
        state.maybe_infer(&frame(2));
        assert!(req_rx.try_recv().is_err(), "sent while disabled");
    }

    #[test]
    fn an_error_result_is_kept_for_the_overlay() {
        let (mut state, _req_rx, res_tx) = state(Duration::ZERO);
        res_tx
            .send(InferResult {
                frame_id: 7,
                detections: Vec::new(),
                error: Some("Inference failed: boom".into()),
            })
            .unwrap();
        state.poll();
        assert_eq!(
            state.detector_error.as_deref(),
            Some("Inference failed: boom")
        );
    }
}
