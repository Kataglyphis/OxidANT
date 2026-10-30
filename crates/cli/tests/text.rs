//! `chat` and `embed` against a local server that plays back one answer, and `chat` on the hub's tiny GenAI model.

#[path = "../../text/tests/common/mod.rs"]
mod common;

use assert_cmd::Command;
use common::{chat_stream, FakeServer};
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

/// The CLI without the endpoint variables a developer's shell may carry.
fn cli() -> Command {
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("kataglyphis_cli");
    for var in ["LLM_BASE_URL", "OLLAMA_BASE_URL", "LLM_BACKENDS"] {
        command.env_remove(var);
    }
    command
}

#[test]
fn chat_streams_an_endpoint_reply_to_stdout_and_its_figures_to_stderr() {
    let server = FakeServer::answer(
        200,
        "text/event-stream",
        chat_stream(&["The capital", " is", " Paris."], "stop", 9),
    );
    cli()
        .args([
            "chat",
            "--base-url",
            &server.base_url,
            "--model",
            "m",
            "--prompt",
            "Capital of France?",
        ])
        .args(["--system", "Be brief."])
        .assert()
        .success()
        .stdout("The capital is Paris.\n")
        .stderr(contains("3 tokens,").and(contains("finish Stop")));
    let request = server.request();
    assert!(
        request.contains(r#"{"content":"Be brief.","role":"system"}"#),
        "{request}"
    );
}

#[test]
fn chat_reaches_a_named_backend_through_its_registry() {
    let server = FakeServer::answer(200, "text/event-stream", chat_stream(&["ok"], "stop", 1));
    let dir = tempfile::tempdir().unwrap();
    let registry = dir.path().join("backends.json");
    let entry = serde_json::json!({"default": "x", "backends": {"lab": {"base_url": server.base_url, "model": "lab-model"}}});
    std::fs::write(&registry, entry.to_string()).unwrap();
    cli()
        .args(["chat", "--prompt", "hi", "--backend", "lab"])
        .env("LLM_BACKENDS", &registry)
        .assert()
        .success()
        .stdout("ok\n");
    assert!(server.request().contains(r#""model":"lab-model""#));
}

#[test]
fn chat_without_any_endpoint_fails_and_says_where_one_comes_from() {
    cli()
        .args(["chat", "--prompt", "hi"])
        .assert()
        .failure()
        .stderr(contains("no endpoint"));
}

#[test]
fn an_endpoint_error_fails_with_its_http_status() {
    let server = FakeServer::answer(404, "application/json", r#"{"error":"no such model"}"#);
    cli()
        .args([
            "chat",
            "--base-url",
            &server.base_url,
            "--model",
            "m",
            "--prompt",
            "hi",
        ])
        .assert()
        .failure()
        .stderr(contains("HTTP 404").and(contains("no such model")));
}

#[test]
fn embed_prints_one_normalised_vector_per_text() {
    let server = FakeServer::answer(
        200,
        "application/json",
        r#"{"data":[{"index":0,"embedding":[3,4]},{"index":1,"embedding":[2,0]}]}"#,
    );
    cli()
        .args([
            "embed",
            "--base-url",
            &server.base_url,
            "--model",
            "e",
            "first",
            "second",
        ])
        .assert()
        .success()
        .stdout("[0.6,0.8]\n[1.0,0.0]\n");
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn chat_with_a_model_directory_answers_from_the_chain_genai() {
    use oxidant::text::genai::GenaiGenerator;
    if !GenaiGenerator::library_available() && !common::genai_required() {
        eprintln!("SKIP: no chain GenAI loads here; CI sets KATAGLYPHIS_REQUIRE_GENAI=1");
        return;
    }
    let dir = common::tiny_model_dir();
    let (prompt, continuation, ids) = common::tiny_fixture();
    cli()
        .arg("chat")
        .arg("--model-dir")
        .arg(dir)
        .args([
            "--prompt",
            &prompt,
            "--max-new-tokens",
            &ids.len().to_string(),
        ])
        .assert()
        .success()
        .stdout(format!("{continuation}\n"))
        .stderr(contains(format!("{} tokens,", ids.len())).and(contains("finish Length")));
}

#[test]
fn a_model_directory_and_an_endpoint_exclude_each_other() {
    cli()
        .args([
            "chat",
            "--prompt",
            "hi",
            "--model-dir",
            "x",
            "--base-url",
            "http://y",
        ])
        .assert()
        .failure()
        .stderr(contains("cannot be used with"));
}
