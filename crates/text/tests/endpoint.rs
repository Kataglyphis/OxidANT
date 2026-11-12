//! The endpoint client against a local server that plays back recorded answers.

mod common;

use common::{chat_stream, FakeServer};
use kataglyphis_text::endpoint::{Endpoint, STREAM_LINE_LIMIT};
use kataglyphis_text::{
    CancelToken, ChatMessage, FinishReason, GenerationOptions, TextBackend, TextError,
};

fn endpoint(server: &FakeServer) -> Endpoint {
    let mut endpoint = Endpoint::new(&server.base_url);
    endpoint.model = Some("tiny".into());
    endpoint
}

#[test]
fn a_streamed_reply_arrives_piece_by_piece_with_the_servers_counts() {
    let server = FakeServer::answer(
        200,
        "text/event-stream",
        chat_stream(&["The capital", " is", " Paris."], "stop", 9),
    );
    let mut pieces = Vec::new();
    let result = endpoint(&server)
        .generate(
            &[ChatMessage::user("What is the capital of France?")],
            &GenerationOptions::default(),
            &CancelToken::default(),
            &mut |piece| pieces.push(piece.to_string()),
        )
        .unwrap();
    assert_eq!(pieces, ["The capital", " is", " Paris."]);
    assert_eq!(result.text, "The capital is Paris.");
    assert_eq!(result.finish_reason, FinishReason::Stop);
    assert_eq!((result.prompt_tokens, result.completion_tokens), (9, 3));
    assert!(result.tokens.is_empty(), "an endpoint reports no token ids");
    let request = server.request();
    assert!(
        request.starts_with("POST /v1/chat/completions "),
        "{request}"
    );
    assert!(
        request.contains(r#""stream":true"#) && request.contains(r#""model":"tiny""#),
        "{request}"
    );
}

#[test]
fn a_cancel_from_the_callback_ends_the_stream_after_its_piece() {
    let server = FakeServer::answer(
        200,
        "text/event-stream",
        chat_stream(&["a", "b", "c"], "stop", 1),
    );
    let cancel = CancelToken::default();
    let handle = cancel.clone();
    let result = endpoint(&server)
        .generate(
            &[ChatMessage::user("x")],
            &GenerationOptions::default(),
            &cancel,
            &mut |_| handle.cancel(),
        )
        .unwrap();
    assert_eq!(result.text, "a");
    assert_eq!(result.finish_reason, FinishReason::Cancelled);
}

/// One reply in each dialect a thinking model's server speaks, with the thinking characters it holds.
fn thinking_dialects() -> [(&'static str, Vec<serde_json::Value>, usize); 3] {
    use serde_json::json;
    let apart = "Two visits today.".len();
    [
        (
            "reasoning_content (llama-server, vLLM)",
            vec![
                json!({"reasoning_content": "Two visits"}),
                json!({"reasoning_content": " today.", "content": null}),
                json!({"content": "Twice"}),
                json!({"content": "."}),
            ],
            apart,
        ),
        (
            "reasoning (Ollama)",
            vec![
                json!({"role": "assistant", "content": "", "reasoning": "Two visits"}),
                json!({"role": "assistant", "content": "", "reasoning": " today."}),
                json!({"role": "assistant", "content": "Twice"}),
                json!({"role": "assistant", "content": "."}),
            ],
            apart,
        ),
        (
            "<think> in content, its closing tag split over two pieces",
            vec![
                json!({"content": "<think>"}),
                json!({"content": "Two visits"}),
                json!({"content": " today.</thi"}),
                json!({"content": "nk>Twice"}),
                json!({"content": "."}),
            ],
            apart + "<think></think>".len(),
        ),
    ]
}

#[test]
fn the_three_thinking_dialects_give_the_same_answer_and_count_their_thinking() {
    for (dialect, deltas, thinking_chars) in thinking_dialects() {
        let server = FakeServer::answer(
            200,
            "text/event-stream",
            common::delta_stream(&deltas, "stop", 9),
        );
        let mut shown = String::new();
        let result = endpoint(&server)
            .generate(
                &[ChatMessage::user("How often did the cat come?")],
                &GenerationOptions::default(),
                &CancelToken::default(),
                &mut |piece| shown.push_str(piece),
            )
            .unwrap();
        assert_eq!(result.text, "Twice.", "{dialect}");
        assert_eq!(shown, "Twice.", "{dialect}");
        assert_eq!(result.thinking_chars, thinking_chars, "{dialect}");
        assert_eq!(result.finish_reason, FinishReason::Stop, "{dialect}");
    }
}

#[test]
fn a_reply_cut_inside_its_thinking_has_no_answer_and_a_thinking_count() {
    use serde_json::json;
    for (dialect, deltas) in [
        (
            "reasoning_content",
            vec![json!({"reasoning_content": "Let me count"})],
        ),
        ("reasoning", vec![json!({"reasoning": "Let me count"})]),
        (
            "an unclosed <think>",
            vec![
                json!({"content": "<think>Let me"}),
                json!({"content": " count"}),
            ],
        ),
    ] {
        let server = FakeServer::answer(
            200,
            "text/event-stream",
            common::delta_stream(&deltas, "length", 9),
        );
        let mut shown = Vec::new();
        let result = endpoint(&server)
            .generate(
                &[ChatMessage::user("x")],
                &GenerationOptions::default(),
                &CancelToken::default(),
                &mut |piece| shown.push(piece.to_string()),
            )
            .unwrap();
        assert_eq!(result.text, "", "{dialect}");
        assert!(shown.is_empty(), "{dialect}: {shown:?}");
        assert!(result.thinking_chars >= "Let me count".len(), "{dialect}");
        assert_eq!(result.finish_reason, FinishReason::Length, "{dialect}");
    }
}

#[test]
fn thinking_pieces_count_as_tokens_when_the_server_reports_no_usage() {
    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"a\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"b\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"c\"},\"finish_reason\":\"stop\"}]}\n\n",
    );
    let server = FakeServer::answer(200, "text/event-stream", stream);
    let result = endpoint(&server)
        .generate(
            &[ChatMessage::user("x")],
            &GenerationOptions::default(),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!((result.text.as_str(), result.thinking_chars), ("c", 2));
    assert_eq!(result.completion_tokens, 3);
}

/// Takes `good` writes, then fails as a closed pipe does.
struct ClosingPipe {
    good: usize,
    written: Vec<u8>,
}

impl std::io::Write for ClosingPipe {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.good == 0 {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        self.good -= 1;
        self.written.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_reader_that_left_ends_the_generation_instead_of_decoding_for_nobody() {
    let server = FakeServer::answer(
        200,
        "text/event-stream",
        chat_stream(&["a", "b", "c", "d"], "stop", 1),
    );
    // The shape of the CLI's `chat`: a boxed backend, a writer, one token.
    let mut backend: Box<dyn TextBackend> = Box::new(endpoint(&server));
    let mut out = ClosingPipe {
        good: 1,
        written: Vec::new(),
    };
    let mut failed_write = None;
    let cancel = CancelToken::default();
    let result = {
        let mut on_text = kataglyphis_text::write_pieces(&mut out, &cancel, &mut failed_write);
        backend
            .generate(
                &[ChatMessage::user("x")],
                &GenerationOptions::default(),
                &cancel,
                &mut on_text,
            )
            .unwrap()
    };
    assert_eq!(result.finish_reason, FinishReason::Cancelled);
    assert_eq!(result.text, "ab", "decoded past the failed write");
    assert_eq!(out.written, b"a");
    assert_eq!(
        failed_write.map(|e| e.kind()),
        Some(std::io::ErrorKind::BrokenPipe)
    );
}

/// A server that answers `reply` and then says nothing more until the test ends.
fn silent_after(reply: &'static str) -> (Endpoint, std::sync::mpsc::Sender<()>) {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut endpoint = Endpoint::new(format!("http://{}", listener.local_addr().unwrap()));
    endpoint.timeout = std::time::Duration::from_secs(5);
    endpoint.model = Some("tiny".into());
    let (release, held) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request);
        let _ = stream.write_all(reply.as_bytes());
        let _ = stream.flush();
        let _ = held.recv();
    });
    (endpoint, release)
}

#[test]
fn a_cancel_during_the_servers_silence_ends_the_run_without_waiting_for_its_next_line() {
    const HEAD: &str =
        "HTTP/1.1 200 X\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
    const FIRST: &str = "HTTP/1.1 200 X\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n";
    // Silent before the status line, after the head, and after the first piece.
    for (reply, text) in [("", ""), (HEAD, ""), (FIRST, "a")] {
        let (mut endpoint, _release) = silent_after(reply);
        let cancel = CancelToken::default();
        let handle = cancel.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            handle.cancel();
        });
        let started = std::time::Instant::now();
        let result = endpoint
            .generate(
                &[ChatMessage::user("x")],
                &GenerationOptions::default(),
                &cancel,
                &mut |_| {},
            )
            .unwrap();
        canceller.join().unwrap();
        assert_eq!(result.finish_reason, FinishReason::Cancelled, "{reply:?}");
        assert_eq!(result.text, text, "{reply:?}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "{reply:?}: {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn a_length_finish_is_reported_as_such() {
    let server = FakeServer::answer(200, "text/event-stream", chat_stream(&["x"], "length", 1));
    let result = endpoint(&server)
        .generate(
            &[ChatMessage::user("x")],
            &GenerationOptions::default(),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(result.finish_reason, FinishReason::Length);
}

#[test]
fn an_http_error_keeps_its_status_and_body() {
    let server = FakeServer::answer(503, "application/json", r#"{"error":"loading"}"#);
    let refused = endpoint(&server)
        .generate(
            &[ChatMessage::user("x")],
            &GenerationOptions::default(),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap_err();
    assert_eq!(
        refused,
        TextError::EndpointStatus(503, r#"{"error":"loading"}"#.into())
    );
}

#[test]
fn an_error_body_cut_inside_a_character_keeps_what_came_before() {
    // The 2000-byte limit falls between the two bytes of the "é"; read as a string, nothing of the body was kept.
    let body = format!("{}é and the rest", "x".repeat(1999));
    let server = FakeServer::answer(502, "text/plain", &body);
    let refused = endpoint(&server)
        .generate(
            &[ChatMessage::user("x")],
            &GenerationOptions::default(),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap_err();
    let TextError::EndpointStatus(502, text) = refused else {
        panic!("{refused:?}");
    };
    assert_eq!(text.chars().count(), 2000, "{text}");
    assert!(text.starts_with(&"x".repeat(1999)), "{text}");
}

#[test]
fn a_stream_that_stops_without_a_finish_reason_is_an_error_not_an_answer() {
    let server = FakeServer::answer(
        200,
        "text/event-stream",
        "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n",
    );
    let refused = endpoint(&server)
        .generate(
            &[ChatMessage::user("x")],
            &GenerationOptions::default(),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap_err();
    assert!(
        matches!(refused, TextError::GenerationFailed(_)),
        "{refused}"
    );
}

#[test]
fn a_streamed_line_over_the_limit_ends_the_run_with_a_typed_error() {
    let flood = format!("data: {}\n\n", "x".repeat(2 * STREAM_LINE_LIMIT));
    let server = FakeServer::answer(200, "text/event-stream", flood);
    let refused = endpoint(&server)
        .generate(
            &[ChatMessage::user("x")],
            &GenerationOptions::default(),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap_err();
    assert_eq!(refused, TextError::StreamLineTooLong(STREAM_LINE_LIMIT));
}

#[test]
fn a_closed_port_is_unreachable() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut endpoint = Endpoint::new(format!("http://127.0.0.1:{port}"));
    endpoint.model = Some("m".into());
    let refused = endpoint
        .generate(
            &[ChatMessage::user("x")],
            &GenerationOptions::default(),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap_err();
    assert!(
        matches!(refused, TextError::EndpointUnreachable(_)),
        "{refused}"
    );
}

#[test]
fn embeddings_come_back_normalised_in_the_order_asked() {
    let answer = r#"{"data":[{"index":1,"embedding":[0,5]},{"index":0,"embedding":[3,4]}]}"#;
    let server = FakeServer::answer(200, "application/json", answer);
    let vectors = endpoint(&server)
        .embed(&["first".into(), "second".into()])
        .unwrap();
    assert_eq!(vectors, vec![vec![0.6, 0.8], vec![0.0, 1.0]]);
    let request = server.request();
    assert!(
        request.starts_with("POST /v1/embeddings ")
            && request.contains(r#""input":["first","second"]"#),
        "{request}"
    );
}

#[test]
fn the_seed_the_schema_and_the_stop_strings_reach_the_request_body() {
    let server = FakeServer::answer(
        200,
        "text/event-stream",
        chat_stream(&[r#"{"a":"x"}"#], "stop", 4),
    );
    let schema = r#"{"type":"object","properties":{"a":{"type":"string"}},"required":["a"]}"#;
    let options = GenerationOptions {
        temperature: 1.0,
        seed: Some(42),
        json_schema: Some(schema.into()),
        stop: vec!["}".into(), "END".into()],
        ..GenerationOptions::default()
    };
    let result = endpoint(&server)
        .generate(
            &[ChatMessage::user("x")],
            &options,
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(result.finish_reason, FinishReason::Stop);
    let request = server.request();
    let body: serde_json::Value =
        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["seed"], 42, "{request}");
    assert_eq!(body["stop"], serde_json::json!(["}", "END"]), "{request}");
    assert_eq!(body["response_format"]["type"], "json_schema", "{request}");
    assert_eq!(
        body["response_format"]["json_schema"]["schema"],
        serde_json::from_str::<serde_json::Value>(schema).unwrap(),
        "{request}"
    );
}

#[test]
fn without_a_seed_schema_or_stop_the_body_names_none_of_them() {
    let server = FakeServer::answer(200, "text/event-stream", chat_stream(&["x"], "stop", 1));
    endpoint(&server)
        .generate(
            &[ChatMessage::user("x")],
            &GenerationOptions::default(),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
    let request = server.request();
    for key in [r#""seed""#, r#""stop""#, r#""response_format""#] {
        assert!(!request.contains(key), "{key} in {request}");
    }
}
