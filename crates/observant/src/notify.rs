//! Tells a webhook (a Home Assistant webhook trigger, ntfy, Node-RED, ...) when a cat arrives or
//! leaves, from a thread of its own so a slow receiver never holds up detection.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use std::sync::Arc;

use crate::clip::Arrival;
use crate::presence::Change;
use crate::status::Status;
use crate::web::{ARRIVAL_PATH, CLIP_PATH};

/// How long one delivery may take before it is given up.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The waits before each new try of an event the receiver refused or did not answer: a Home
/// Assistant restart takes about a minute, an update a few.
const RETRY_DELAYS: [Duration; 4] = [
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(45),
    Duration::from_secs(120),
];

/// An event older than this is dropped untried: a notification that late helps nobody.
const MAX_AGE: Duration = Duration::from_secs(10 * 60);

/// When to try an event again, and when to give it up.
#[derive(Clone, Copy)]
struct Retry {
    delays: &'static [Duration],
    max_age: Duration,
}

#[derive(Clone)]
pub struct Notifier {
    /// The camera's name comes from here at each event, so a rename on the settings page reaches it.
    status: Arc<Status>,
    tx: mpsc::Sender<(Instant, serde_json::Value)>,
}

impl Notifier {
    /// Starts the delivery thread; events queue in order while one is in flight or waits to be retried.
    pub fn spawn(url: &str, status: Arc<Status>) -> std::io::Result<Self> {
        Self::spawn_with(
            url,
            status,
            Retry {
                delays: &RETRY_DELAYS,
                max_age: MAX_AGE,
            },
        )
    }

    fn spawn_with(url: &str, status: Arc<Status>, retry: Retry) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel::<(Instant, serde_json::Value)>();
        let url = url.to_owned();
        thread::Builder::new()
            .name("cat-notify".into())
            .spawn(move || {
                let agent: ureq::Agent = ureq::Agent::config_builder()
                    .timeout_global(Some(TIMEOUT))
                    .build()
                    .into();
                for (queued, event) in rx {
                    deliver(&agent, &url, queued, &event, retry);
                }
            })?;
        Ok(Self { status, tx })
    }

    /// `time` is the event's Unix time; for an arrival, the visit log's, which the clip is kept under.
    /// A departure names its `visit`, so a receiver can update what it sent for that arrival.
    pub fn send(&self, change: &Change, time: u64, visit: Option<Arrival>) {
        let _ = self.tx.send((
            Instant::now(),
            payload(&self.status.name(), change, time, visit),
        ));
    }

    /// The clip of `arrival` is at [`CLIP_PATH`]: a second event, so a notification can add it;
    /// `stayed_s` once the visit is over, as a short one ends before its clip is encoded.
    pub fn send_clip(&self, arrival: Arrival, stayed_s: Option<u64>) {
        let _ = self.tx.send((
            Instant::now(),
            clip_payload(&self.status.name(), arrival, stayed_s),
        ));
    }
}

/// The event for a finished clip; `best_score` lets a receiver apply the same threshold as to arrivals.
/// Posts `event` to `url`, tried again after each of the retry delays until the receiver takes it.
fn deliver(
    agent: &ureq::Agent,
    url: &str,
    queued: Instant,
    event: &serde_json::Value,
    retry: Retry,
) {
    let waits = std::iter::once(Duration::ZERO).chain(retry.delays.iter().copied());
    for (tried, wait) in waits.enumerate() {
        thread::sleep(wait);
        if queued.elapsed() > retry.max_age {
            log::warn!(
                "notify {}: dropped an event {} s old",
                origin(url),
                queued.elapsed().as_secs()
            );
            return;
        }
        let sent = agent
            .post(url)
            .header("content-type", "application/json")
            .send(event.to_string());
        match sent {
            Ok(_) if tried > 0 => {
                log::info!("notify {}: delivered on try {}", origin(url), tried + 1);
                return;
            }
            Ok(_) => return,
            Err(err) => log::warn!("notify {} (try {}): {err}", origin(url), tried + 1),
        }
    }
    log::warn!(
        "notify {}: gave up on an event after {} tries",
        origin(url),
        retry.delays.len() + 1
    );
}

fn clip_payload(name: &str, arrival: Arrival, stayed_s: Option<u64>) -> serde_json::Value {
    let mut event = serde_json::json!({
        "event": "clip",
        "name": name,
        "time": arrival.time,
        "best_score": arrival.best_score,
        "clip": CLIP_PATH,
        "snapshot": ARRIVAL_PATH,
    });
    if let Some(stayed) = stayed_s {
        event["stayed_s"] = stayed.into();
    }
    event
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
pub fn payload(
    name: &str,
    change: &Change,
    time: u64,
    visit: Option<Arrival>,
) -> serde_json::Value {
    let mut event = match change {
        Change::Arrived { count, best, class } => serde_json::json!({
            "event": "arrived",
            "name": name,
            "time": time,
            "class": crate::coco::name(*class),
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
    };
    // The arrival this departure ends, with the score that arrival was confirmed at.
    if let (Change::Left { .. }, Some(visit)) = (change, visit) {
        event["arrived"] = visit.time.into();
        event["best_score"] = visit.best_score.into();
    }
    event
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
                class: 15,
            },
            7,
            None,
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
            None,
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
            None,
        );
        assert_eq!(clip["event"], "clip");
        assert_eq!(clip["name"], "Cam");
        assert_eq!(clip["time"], 1_791_763_273u64);
        assert_eq!(clip["best_score"], 0.75);
        assert_eq!(clip["clip"], CLIP_PATH);
        assert_eq!(clip["snapshot"], ARRIVAL_PATH);
        assert!(clip.get("stayed_s").is_none());
    }

    #[test]
    fn a_clip_of_a_visit_already_over_says_how_long_it_lasted() {
        let arrival = Arrival {
            time: 100,
            best_score: 0.9,
        };
        assert_eq!(clip_payload("Cam", arrival, Some(5))["stayed_s"], 5);
    }

    #[test]
    fn a_departure_names_the_arrival_it_ends_and_its_score() {
        let visit = Arrival {
            time: 1_791_763_200,
            best_score: 0.8,
        };
        let left = payload(
            "Cam",
            &Change::Left {
                stayed: Duration::from_secs(240),
            },
            1_791_763_440,
            Some(visit),
        );
        assert_eq!(left["event"], "left");
        assert_eq!(left["arrived"], 1_791_763_200u64);
        assert!((left["best_score"].as_f64().unwrap() - 0.8).abs() < 1e-6);
        assert_eq!(left["stayed_s"], 240);
        // An arrival names itself; only a departure carries `arrived`.
        let arrived = payload(
            "Cam",
            &Change::Arrived {
                count: 1,
                best: 0.8,
                class: 15,
            },
            1_791_763_200,
            Some(visit),
        );
        assert!(arrived.get("arrived").is_none());
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

    /// The lowercased request head lines and the body of the one POST a [`webhook`] took.
    type Received = thread::JoinHandle<(Vec<String>, Vec<u8>)>;

    /// A webhook stand-in on loopback that takes one POST and hands back its head and body.
    fn webhook() -> (String, Received) {
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
        (url, server)
    }

    #[test]
    fn an_event_is_posted_as_json_to_the_url() {
        let (url, server) = webhook();
        let status = Arc::new(Status::new("Test Cam", true, 4, 4));
        let notifier = Notifier::spawn(&url, status).unwrap();
        notifier.send(
            &Change::Arrived {
                count: 1,
                best: 0.8,
                class: 15,
            },
            1_791_763_273,
            None,
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

    #[test]
    fn a_clip_event_goes_out_under_the_name_the_page_set_last() {
        let (url, server) = webhook();
        let status = Arc::new(Status::new("Old Name", true, 4, 4));
        let notifier = Notifier::spawn(&url, status.clone()).unwrap();
        // Renamed on the Camera page after the notifier started: the event must say so.
        status
            .apply(crate::status::Settings {
                name: Some("Futtertrog".into()),
                ..Default::default()
            })
            .unwrap();
        let arrival = Arrival {
            time: 1_791_763_273,
            best_score: 0.8,
        };
        notifier.send_clip(arrival, Some(12));
        let (_, body) = server.join().unwrap();
        let event: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(event["event"], "clip");
        assert_eq!(event["name"], "Futtertrog");
        assert_eq!(event["stayed_s"], 12);
    }

    #[test]
    fn a_refused_event_is_tried_again_and_the_next_one_waits_its_turn() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/api/webhook/cats", listener.local_addr().unwrap());
        // Home Assistant restarting: the first try gets a 503, the second and third a 200.
        let server = thread::spawn(move || {
            let mut events = Vec::new();
            for reply in [
                &b"HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\n\r\n"[..],
                b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n",
                b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n",
            ] {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream);
                let mut length = 0;
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 2 {
                    if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length: ") {
                        length = n.trim().parse().unwrap();
                    }
                    line.clear();
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                reader.get_mut().write_all(reply).unwrap();
                let event: serde_json::Value = serde_json::from_slice(&body).unwrap();
                events.push(event["event"].as_str().unwrap().to_owned());
            }
            events
        });
        static FAST: [Duration; 2] = [Duration::from_millis(50), Duration::from_millis(50)];
        let status = Arc::new(Status::new("Test Cam", true, 4, 4));
        let retry = Retry {
            delays: &FAST,
            max_age: Duration::from_secs(60),
        };
        let notifier = Notifier::spawn_with(&url, status, retry).unwrap();
        notifier.send(
            &Change::Arrived {
                count: 1,
                best: 0.8,
                class: 15,
            },
            1_791_763_273,
            None,
        );
        let arrival = Arrival {
            time: 1_791_763_273,
            best_score: 0.8,
        };
        notifier.send_clip(arrival, None);
        // The arrival twice (refused, then taken), and only then its clip: order holds.
        assert_eq!(server.join().unwrap(), ["arrived", "arrived", "clip"]);
    }

    #[test]
    fn an_event_older_than_its_limit_is_dropped_untried() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/hook", listener.local_addr().unwrap());
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(500)))
            .build()
            .into();
        let retry = Retry {
            delays: &[],
            max_age: Duration::from_secs(1),
        };
        let queued = Instant::now() - Duration::from_secs(5);
        deliver(
            &agent,
            &url,
            queued,
            &serde_json::json!({"event": "arrived"}),
            retry,
        );
        // Nobody knocked: a stale event never reaches the receiver.
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err());
    }

    #[test]
    fn a_receiver_that_keeps_refusing_gets_each_try_and_then_none() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/hook", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(2)))
            .build()
            .into();
        static TWO: [Duration; 2] = [Duration::from_millis(20), Duration::from_millis(20)];
        let retry = Retry {
            delays: &TWO,
            max_age: Duration::from_secs(60),
        };
        // Answers every try with a 500 on a thread of its own, and counts them.
        let tries = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = tries.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let server = thread::spawn(move || {
            while !stopped.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();
                        while reader.read_line(&mut line).unwrap_or(0) > 2 {
                            line.clear();
                        }
                        let _ = reader.get_mut().write_all(
                            b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\n\r\n",
                        );
                        counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        deliver(
            &agent,
            &url,
            Instant::now(),
            &serde_json::json!({"event": "arrived"}),
            retry,
        );
        thread::sleep(Duration::from_millis(100));
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        server.join().unwrap();
        // The first try and one per delay: three, and no fourth once it gave up.
        assert_eq!(tries.load(std::sync::atomic::Ordering::Relaxed), 3);
    }

    #[test]
    fn an_arrival_names_its_class() {
        let dog = payload(
            "Cam",
            &Change::Arrived {
                count: 1,
                best: 0.7,
                class: 16,
            },
            100,
            None,
        );
        assert_eq!(dog["class"], "dog");
    }
}
