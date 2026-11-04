//! `/ask` against the backends for real: the chain GenAI on the hub's tiny model, and an endpoint that plays back one answer.

#[path = "../../text/tests/common/mod.rs"]
mod common;

use std::sync::Arc;

use kataglyphis_observant::ask::Assistant;
use kataglyphis_observant::config::Config;
use kataglyphis_observant::static_files::GzipCache;
use kataglyphis_observant::status::Status;
use kataglyphis_observant::web::{serve, Site};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

async fn start(config: &Config) -> std::net::SocketAddr {
    let site = Site {
        web_root: None,
        signalling: ("127.0.0.1".into(), 9),
        status: Arc::new(Status::new("Cam", false, 4, 4)),
        gzip: GzipCache::default(),
        allowed_hosts: Vec::new(),
        max_viewers: 1,
        peers: Vec::new(),
        assistant: Assistant::from_config(config).map(Arc::new),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, Arc::new(site)));
    addr
}

async fn ask(addr: std::net::SocketAddr, question: &str) -> (String, serde_json::Value) {
    let body = serde_json::json!({ "question": question }).to_string();
    let request = format!(
        "POST /ask HTTP/1.1\r\nhost: cat.local\r\nconnection: close\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let reply = String::from_utf8_lossy(&response).into_owned();
    let text = &reply[reply.find("\r\n\r\n").unwrap() + 4..];
    let json = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
    (reply, json)
}

#[cfg(any(target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread")]
async fn the_chain_genai_answers_with_the_oracles_tokens_after_the_facts() {
    use kataglyphis_text::genai::GenaiGenerator;
    if !GenaiGenerator::library_available() && !common::genai_required() {
        eprintln!("SKIP: no chain GenAI loads here; CI sets KATAGLYPHIS_REQUIRE_GENAI=1");
        return;
    }
    let (prompt, continuation, ids) = common::tiny_fixture();
    let addr = start(&Config {
        llm_model_dir: Some(common::tiny_model_dir().to_path_buf()),
        llm_max_new_tokens: ids.len() as u32,
        ..Config::default()
    })
    .await;
    let (reply, json) = ask(addr, &prompt).await;
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(json["answer"], continuation.as_str());
    assert_eq!(json["finish_reason"], "length");
    // The template joins the contents, so each byte of the facts must have reached the model as a token.
    let facts = json["facts"].as_str().unwrap();
    let prompt_tokens = json["prompt_tokens"].as_u64().unwrap() as usize;
    assert!(prompt_tokens > facts.len() + prompt.len(), "{json}");
    let (_, again) = ask(addr, &prompt).await;
    assert_eq!(again["answer"], continuation.as_str());
}

#[cfg(any(target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_model_directory_is_the_replys_error_and_the_service_lives() {
    use kataglyphis_text::genai::GenaiGenerator;
    if !GenaiGenerator::library_available() && !common::genai_required() {
        eprintln!("SKIP: no chain GenAI loads here; CI sets KATAGLYPHIS_REQUIRE_GENAI=1");
        return;
    }
    let addr = start(&Config {
        llm_model_dir: Some("/absent/model".into()),
        ..Config::default()
    })
    .await;
    let (reply, _) = ask(addr, "abc").await;
    assert!(reply.starts_with("HTTP/1.1 502"), "{reply}");
    assert!(reply.contains("could not be loaded"), "{reply}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_endpoint_receives_the_facts_and_the_question() {
    let server = common::FakeServer::answer(
        200,
        "text/event-stream",
        common::chat_stream(&["Once", "."], "stop", 30),
    );
    let addr = start(&Config {
        llm_base_url: Some(server.base_url.clone()),
        llm_model: Some("m".into()),
        ..Config::default()
    })
    .await;
    let (reply, json) = ask(addr, "How often?").await;
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(json["answer"], "Once.");
    assert_eq!(json["finish_reason"], "stop");
    assert_eq!(json["prompt_tokens"], 30);
    let request = server.request();
    assert!(
        request.contains("No visits in the last 7 days."),
        "{request}"
    );
    assert!(request.contains("How often?"), "{request}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_endpoint_is_the_replys_error() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr = start(&Config {
        llm_base_url: Some(format!("http://127.0.0.1:{port}")),
        llm_model: Some("m".into()),
        ..Config::default()
    })
    .await;
    let (reply, _) = ask(addr, "How often?").await;
    assert!(reply.starts_with("HTTP/1.1 502"), "{reply}");
    assert!(reply.contains("unreachable"), "{reply}");
}
