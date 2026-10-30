//! The endpoint client against a local server that plays back recorded answers.

mod common;

use common::{chat_stream, FakeServer};
use kataglyphis_text::endpoint::Endpoint;
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
