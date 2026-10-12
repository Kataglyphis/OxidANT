//! Tells a webhook (a Home Assistant webhook trigger, ntfy, Node-RED, ...) when a cat arrives or
//! leaves, from a thread of its own so a slow receiver never holds up detection.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::clip::Arrival;
use crate::presence::Change;
use crate::web::{ARRIVAL_PATH, CLIP_PATH};

/// How long one delivery may take before it is given up.
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Notifier {
    name: String,
    tx: mpsc::Sender<serde_json::Value>,
}

impl Notifier {
    /// Starts the delivery thread; events queue while one is in flight.
    pub fn spawn(url: &str, name: &str) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel::<serde_json::Value>();
        let url = url.to_owned();
        thread::Builder::new()
            .name("cat-notify".into())
            .spawn(move || {
                let agent: ureq::Agent = ureq::Agent::config_builder()
                    .timeout_global(Some(TIMEOUT))
                    .build()
                    .into();
                for event in rx {
                    let sent = agent
                        .post(&url)
                        .header("content-type", "application/json")
                        .send(event.to_string());
                    if let Err(err) = sent {
                        log::warn!("notify {}: {err}", origin(&url));
                    }
                }
            })?;
        Ok(Self {
            name: name.to_owned(),
            tx,
        })
    }

    /// `time` is the event's Unix time; for an arrival, the visit log's, which the clip is kept under.
    pub fn send(&self, change: &Change, time: u64) {
        let _ = self.tx.send(payload(&self.name, change, time));
    }

    /// The clip of `arrival` is at [`CLIP_PATH`]: a second event, so a notification can add it.
    pub fn send_clip(&self, arrival: Arrival) {
        let _ = self.tx.send(clip_payload(&self.name, arrival));
    }
}

/// The event for a finished clip; `best_score` lets a receiver apply the same threshold as to arrivals.
fn clip_payload(name: &str, arrival: Arrival) -> serde_json::Value {
    serde_json::json!({
        "event": "clip",
        "name": name,
        "time": arrival.time,
        "best_score": arrival.best_score,
        "clip": CLIP_PATH,
        "snapshot": ARRIVAL_PATH,
    })
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `scheme://host` of a URL: a webhook's path is often its secret, so logs never carry it.
pub fn origin(url: &str) -> &str {
    let host_start = url.find("://").map_or(0, |i| i + 3);
    let end = url[host_start..]
        .find('/')
        .map_or(url.len(), |i| host_start + i);
    &url[..end]
}

/// The JSON a receiver gets: `event` is `arrived` or `left`, `time` Unix seconds.
pub fn payload(name: &str, change: &Change, time: u64) -> serde_json::Value {
    match change {
        Change::Arrived { count, best } => serde_json::json!({
            "event": "arrived",
            "name": name,
            "time": time,
            "detections": count,
            "best_score": best,
            "snapshot": ARRIVAL_PATH,
        }),
        Change::Left { stayed } => serde_json::json!({
            "event": "left",
            "name": name,
            "time": time,
            "stayed_s": stayed.as_secs(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::TcpListener;

    #[test]
    fn the_payload_names_the_event_and_its_numbers() {
        let arrived = payload(
            "Cam",
            &Change::Arrived {
                count: 2,
                best: 0.5,
            },
            7,
        );
        assert_eq!(arrived["event"], "arrived");
        assert_eq!(arrived["detections"], 2);
        assert_eq!(arrived["best_score"], 0.5);
        assert_eq!(arrived["snapshot"], ARRIVAL_PATH);
        let left = payload(
            "Cam",
            &Change::Left {
                stayed: Duration::from_millis(42_900),
            },
            8,
        );
        assert_eq!(left["event"], "left");
        assert_eq!(left["stayed_s"], 42);
        assert_eq!(left["time"], 8);
    }

    #[test]
    fn a_finished_clip_is_an_event_of_its_own_with_the_arrival_it_shows() {
        let clip = clip_payload(
            "Cam",
            Arrival {
                time: 1_791_763_273,
                best_score: 0.75,
            },
        );
        assert_eq!(clip["event"], "clip");
        assert_eq!(clip["name"], "Cam");
        assert_eq!(clip["time"], 1_791_763_273u64);
        assert_eq!(clip["best_score"], 0.75);
        assert_eq!(clip["clip"], CLIP_PATH);
        assert_eq!(clip["snapshot"], ARRIVAL_PATH);
    }

    #[test]
    fn logs_keep_a_webhook_s_secret_path_out() {
        assert_eq!(
            origin("http://localhost:8123/api/webhook/secret-id"),
            "http://localhost:8123"
        );
        assert_eq!(origin("https://ntfy.sh/my-cats"), "https://ntfy.sh");
        assert_eq!(origin("http://cats.local"), "http://cats.local");
    }

    #[test]
    fn an_event_is_posted_as_json_to_the_url() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/api/webhook/cats", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut head = Vec::new();
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 2 {
                head.push(line.trim_end().to_ascii_lowercase());
                line.clear();
            }
            let length: usize = head
                .iter()
                .find_map(|h| h.strip_prefix("content-length: "))
                .unwrap()
                .parse()
                .unwrap();
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            reader
                .get_mut()
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                .unwrap();
            (head, body)
        });
        let notifier = Notifier::spawn(&url, "Test Cam").unwrap();
        notifier.send(
            &Change::Arrived {
                count: 1,
                best: 0.8,
            },
            1_791_763_273,
        );
        let (head, body) = server.join().unwrap();
        assert_eq!(head[0], "post /api/webhook/cats http/1.1");
        assert!(
            head.contains(&"content-type: application/json".into()),
            "{head:?}"
        );
        let event: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(event["event"], "arrived");
        assert_eq!(event["time"], 1_791_763_273u64);
        assert_eq!(event["name"], "Test Cam");
    }
}
