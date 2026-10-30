//! What the text suites share: a one-answer HTTP server and the hub's tiny GenAI model.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};

/// Every wait here ends on its own, so an interrupted test leaves no thread behind.
const BOUND: Duration = Duration::from_secs(30);

/// Answers the first request with `status` and `body`, then hands the request it read to [`Self::request`].
pub struct FakeServer {
    pub base_url: String,
    request: mpsc::Receiver<String>,
}

impl FakeServer {
    pub fn answer(status: u16, content_type: &str, body: impl Into<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let (body, content_type) = (body.into(), content_type.to_string());
        let (sender, request) = mpsc::channel();
        std::thread::spawn(move || {
            let deadline = Instant::now() + BOUND;
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => return,
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(BOUND)).unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let (mut head, mut length) = (String::new(), 0usize);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                head.push_str(&line);
            }
            let mut sent = vec![0; length];
            reader.read_exact(&mut sent).unwrap();
            let _ = sender.send(format!("{head}\r\n{}", String::from_utf8_lossy(&sent)));
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        });
        Self { base_url, request }
    }

    /// The request line, headers and body the client sent.
    pub fn request(&self) -> String {
        self.request
            .recv_timeout(BOUND)
            .expect("the client sent no request")
    }
}

/// A server-sent stream of `pieces`, then a finish reason and the usage report.
pub fn chat_stream(pieces: &[&str], finish: &str, prompt: usize) -> String {
    let mut body = String::new();
    for piece in pieces {
        let chunk =
            serde_json::json!({"choices": [{"delta": {"content": piece}, "finish_reason": null}]});
        body.push_str(&format!("data: {chunk}\n\n"));
    }
    let end = serde_json::json!({"choices": [{"delta": {}, "finish_reason": finish}],
        "usage": {"prompt_tokens": prompt, "completion_tokens": pieces.len()}});
    body.push_str(&format!("data: {end}\n\ndata: [DONE]\n\n"));
    body
}

/// The hub's generator, which writes the model and its `fixture.json` oracle.
pub fn tiny_model_generator() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/ANTfrastructure/linux/scripts/lib/tiny-genai-model.py")
}

/// The tiny model, written once per test process; see `third_party/ANTfrastructure/docs/language-ai.md` § The test model.
pub fn tiny_model_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("oxidant-tiny-genai-{}", std::process::id()));
        let generator = tiny_model_generator();
        let ran = ["python3", "python"].iter().find_map(|python| {
            std::process::Command::new(python)
                .arg(&generator)
                .arg("--out")
                .arg(&dir)
                .status()
                .ok()
        });
        assert!(
            ran.is_some_and(|s| s.success()),
            "{} did not write {}",
            generator.display(),
            dir.display()
        );
        dir
    })
}

/// The oracle: the prompt, its greedy continuation and that continuation's ids.
pub fn tiny_fixture() -> (String, String, Vec<i32>) {
    let text = std::fs::read_to_string(tiny_model_dir().join("fixture.json")).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
    let ids = fixture["continuation_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as i32)
        .collect();
    (
        fixture["prompt"].as_str().unwrap().to_string(),
        fixture["continuation"].as_str().unwrap().to_string(),
        ids,
    )
}

/// Whether a missing GenAI fails the suite: CI sets `KATAGLYPHIS_REQUIRE_GENAI=1` where the image carries one.
pub fn genai_required() -> bool {
    std::env::var("KATAGLYPHIS_REQUIRE_GENAI").as_deref() == Ok("1")
}
