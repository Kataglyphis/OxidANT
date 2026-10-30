//! An OpenAI-compatible server, by URL or by name through the hub's `linux/llm-stack/backends.json`.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::{
    check_request, decode_rate, messages_json, CancelToken, ChatMessage, FinishReason,
    GenerationOptions, GenerationResult, TextBackend, TextError,
};

/// Error bodies are cut here, so a server's HTML page cannot flood a message.
const ERROR_BODY_LIMIT: usize = 2000;

#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    /// Without `/v1`, as `backends.json` spells it.
    pub base_url: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub headers: Vec<(String, String)>,
    /// Body keys the caller's own keys override.
    pub request_extra: Map<String, Value>,
    pub timeout: Duration,
}

impl Endpoint {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: None,
            api_key: None,
            headers: Vec::new(),
            request_extra: Map::new(),
            timeout: Duration::from_secs(300),
        }
    }

    /// The entry `name` of a `backends.json` text, or its `default`; the key comes from the variable it names.
    pub fn from_registry(registry: &str, name: Option<&str>) -> Result<Self, TextError> {
        let invalid = |d: String| TextError::InvalidOptions(format!("backends.json: {d}"));
        let root: Value = serde_json::from_str(registry).map_err(|e| invalid(e.to_string()))?;
        let name = match name {
            Some(n) => n.to_string(),
            None => root["default"]
                .as_str()
                .ok_or_else(|| invalid("no default backend".into()))?
                .to_string(),
        };
        let entry = root["backends"]
            .get(&name)
            .ok_or_else(|| invalid(format!("no backend named '{name}'")))?;
        let base_url = entry["base_url"]
            .as_str()
            .ok_or_else(|| invalid(format!("'{name}' has no base_url")))?;
        let mut endpoint = Self::new(base_url);
        endpoint.model = entry["model"].as_str().map(str::to_string);
        if let Some(var) = entry["api_key_env"].as_str() {
            let key = std::env::var(var).ok().filter(|k| !k.is_empty());
            endpoint.api_key =
                Some(key.ok_or_else(|| invalid(format!("'{name}' needs its key in ${var}")))?);
        }
        if let Some(headers) = entry["headers"].as_object() {
            endpoint.headers = headers
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect();
        }
        if let Some(extra) = entry["request_extra"].as_object() {
            endpoint.request_extra = extra.clone();
        }
        Ok(endpoint)
    }

    fn post(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<ureq::http::Response<ureq::Body>, TextError> {
        let url = format!("{}{path}", self.base_url);
        let mut request = ureq::post(&url)
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(self.timeout))
            .build()
            .header("Content-Type", "application/json");
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        if let Some(key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
        let mut response = request
            .send(body.to_string())
            .map_err(|e| TextError::EndpointUnreachable(format!("{url}: {e}")))?;
        let status = response.status().as_u16();
        if status >= 400 {
            let mut text = String::new();
            let _ = response
                .body_mut()
                .as_reader()
                .take(ERROR_BODY_LIMIT as u64)
                .read_to_string(&mut text);
            return Err(TextError::EndpointStatus(status, text));
        }
        Ok(response)
    }

    fn model(&self) -> Result<&str, TextError> {
        self.model.as_deref().ok_or_else(|| {
            TextError::InvalidOptions("no model: name one, or give the backend a model".into())
        })
    }

    /// The chat request, the backend's `request_extra` first so the caller's own keys win.
    pub fn chat_body(
        &self,
        messages: &[ChatMessage],
        options: &GenerationOptions,
    ) -> Result<Value, TextError> {
        let mut body = self.request_extra.clone();
        body.insert("model".into(), json!(self.model()?));
        body.insert("messages".into(), messages_json(messages));
        body.insert("max_tokens".into(), json!(options.max_new_tokens));
        body.insert("temperature".into(), json!(options.temperature));
        body.insert("stream".into(), json!(true));
        body.insert("stream_options".into(), json!({ "include_usage": true }));
        if options.temperature > 0.0 {
            body.insert("top_p".into(), json!(options.top_p));
            body.insert("top_k".into(), json!(options.top_k));
        }
        Ok(Value::Object(body))
    }

    /// One L2-normalised vector per text, in the order given.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, TextError> {
        if texts.is_empty() {
            return Err(TextError::InvalidOptions("no texts".into()));
        }
        let mut response = self.post(
            "/v1/embeddings",
            &json!({ "model": self.model()?, "input": texts }),
        )?;
        let text = response.body_mut().read_to_string().map_err(|e| {
            TextError::EndpointUnreachable(format!("the embeddings answer broke: {e}"))
        })?;
        let answer: Value = serde_json::from_str(&text)
            .map_err(|e| TextError::GenerationFailed(format!("the embeddings answer: {e}")))?;
        parse_embeddings(&answer, texts.len())
    }
}

/// Where an endpoint comes from, in the hub tools' order: a URL, `LLM_BASE_URL` or `OLLAMA_BASE_URL`, a named
/// backend, the registry's default. The registry is `registry` or `LLM_BACKENDS`.
#[derive(Debug, Default, Clone)]
pub struct EndpointChoice {
    pub base_url: Option<String>,
    pub backend: Option<String>,
    pub registry: Option<PathBuf>,
    /// Overrides the backend's own model.
    pub model: Option<String>,
}

impl EndpointChoice {
    pub fn resolve(&self) -> Result<Endpoint, TextError> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        let env_url = var("LLM_BASE_URL").or_else(|| var("OLLAMA_BASE_URL"));
        self.resolve_with(env_url, var("LLM_BACKENDS").map(PathBuf::from))
    }

    /// [`Self::resolve`] with the environment passed in.
    pub fn resolve_with(
        &self,
        env_url: Option<String>,
        env_registry: Option<PathBuf>,
    ) -> Result<Endpoint, TextError> {
        let mut endpoint = match self.base_url.clone().or(env_url) {
            Some(url) => Endpoint::new(url),
            None => {
                let path = self.registry.clone().or(env_registry).ok_or_else(|| {
                    TextError::InvalidOptions(
                        "no endpoint: give a URL, set LLM_BASE_URL, or name a backends.json through LLM_BACKENDS".into(),
                    )
                })?;
                let registry = std::fs::read_to_string(&path)
                    .map_err(|e| TextError::InvalidOptions(format!("{}: {e}", path.display())))?;
                Endpoint::from_registry(&registry, self.backend.as_deref())?
            }
        };
        if self.model.is_some() {
            endpoint.model.clone_from(&self.model);
        }
        Ok(endpoint)
    }
}

/// What one server-sent line of a streamed chat carries.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Text(String),
    Finish(FinishReason),
    Usage { prompt: usize, completion: usize },
    Done,
}

/// The events of one `data:` line; other lines, keep-alives and empty deltas carry none.
pub fn parse_stream_line(line: &str) -> Result<Vec<StreamEvent>, TextError> {
    let Some(data) = line.strip_prefix("data:").map(str::trim) else {
        return Ok(Vec::new());
    };
    if data == "[DONE]" {
        return Ok(vec![StreamEvent::Done]);
    }
    let chunk: Value = serde_json::from_str(data).map_err(|e| {
        TextError::GenerationFailed(format!("an unreadable stream chunk ({e}): {data}"))
    })?;
    if let Some(message) = chunk["error"]["message"].as_str() {
        return Err(TextError::GenerationFailed(message.to_string()));
    }
    let mut events = Vec::new();
    if let Some(choice) = chunk["choices"].get(0) {
        if let Some(text) = choice["delta"]["content"]
            .as_str()
            .filter(|t| !t.is_empty())
        {
            events.push(StreamEvent::Text(text.to_string()));
        }
        match choice["finish_reason"].as_str() {
            Some("length") => events.push(StreamEvent::Finish(FinishReason::Length)),
            Some(_) => events.push(StreamEvent::Finish(FinishReason::Stop)),
            None => {}
        }
    }
    if let Some(usage) = chunk["usage"].as_object() {
        let count = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0) as usize;
        events.push(StreamEvent::Usage {
            prompt: count("prompt_tokens"),
            completion: count("completion_tokens"),
        });
    }
    Ok(events)
}

/// The vectors of an `/v1/embeddings` answer, by index, each scaled to unit length.
pub fn parse_embeddings(answer: &Value, expected: usize) -> Result<Vec<Vec<f32>>, TextError> {
    let bad = |d: &str| TextError::GenerationFailed(format!("the embeddings answer {d}"));
    let data = answer["data"]
        .as_array()
        .ok_or_else(|| bad("has no data"))?;
    if data.len() != expected {
        return Err(bad(&format!(
            "holds {} vectors for {expected} texts",
            data.len()
        )));
    }
    let mut vectors = vec![Vec::new(); expected];
    for (position, item) in data.iter().enumerate() {
        let index = item["index"].as_u64().map_or(position, |i| i as usize);
        let values = item["embedding"]
            .as_array()
            .ok_or_else(|| bad("has an item without an embedding"))?;
        let vector: Vec<f32> = values
            .iter()
            .filter_map(Value::as_f64)
            .map(|v| v as f32)
            .collect();
        let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if vector.len() != values.len() || norm == 0.0 {
            return Err(bad("has a vector that is not a nonzero list of numbers"));
        }
        let slot = vectors
            .get_mut(index)
            .ok_or_else(|| bad("has an index out of range"))?;
        *slot = vector.into_iter().map(|v| v / norm).collect();
    }
    if vectors.iter().any(Vec::is_empty) {
        return Err(bad("repeats an index"));
    }
    Ok(vectors)
}

impl TextBackend for Endpoint {
    fn generate(
        &mut self,
        messages: &[ChatMessage],
        options: &GenerationOptions,
        cancel: &CancelToken,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<GenerationResult, TextError> {
        check_request(messages, options)?;
        cancel.reset();
        let body = self.chat_body(messages, options)?;
        let start = Instant::now();
        let response = self.post("/v1/chat/completions", &body)?;
        let reader = BufReader::new(response.into_body().into_reader());
        let mut result = GenerationResult {
            text: String::new(),
            tokens: Vec::new(),
            finish_reason: FinishReason::Length,
            prompt_tokens: 0,
            completion_tokens: 0,
            time_to_first_token_ms: 0.0,
            decode_tokens_per_second: 0.0,
        };
        let (mut pieces, mut first, mut finished, mut usage) = (0usize, start, false, None);
        for line in reader.lines() {
            if cancel.is_cancelled() {
                result.finish_reason = FinishReason::Cancelled;
                break;
            }
            let line =
                line.map_err(|e| TextError::EndpointUnreachable(format!("the stream broke: {e}")))?;
            for event in parse_stream_line(&line)? {
                match event {
                    StreamEvent::Text(text) => {
                        pieces += 1;
                        if pieces == 1 {
                            first = Instant::now();
                            result.time_to_first_token_ms =
                                first.duration_since(start).as_secs_f64() * 1000.0;
                        }
                        result.text.push_str(&text);
                        on_text(&text);
                    }
                    StreamEvent::Finish(reason) => {
                        result.finish_reason = reason;
                        finished = true;
                    }
                    StreamEvent::Usage { prompt, completion } => usage = Some((prompt, completion)),
                    StreamEvent::Done => {}
                }
            }
        }
        if !finished && result.finish_reason != FinishReason::Cancelled {
            return Err(TextError::GenerationFailed(
                "the stream ended without a finish reason".into(),
            ));
        }
        // A server without usage reports leaves the piece count, which undercounts merged tokens.
        let (prompt, completion) = usage.unwrap_or((0, pieces));
        result.prompt_tokens = prompt;
        result.completion_tokens = completion;
        result.decode_tokens_per_second = decode_rate(completion, first.elapsed().as_secs_f64());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &str = r#"{"default": "local", "backends": {
        "local": {"base_url": "http://127.0.0.1:11434/", "note": "x"},
        "paid": {"base_url": "https://api.example", "model": "m", "api_key_env": "OXIDANT_TEXT_TEST_KEY_UNSET",
                 "headers": {"X-Lane": "a"}, "request_extra": {"num_ctx": 4096, "max_tokens": 1}}}}"#;

    #[test]
    fn the_default_backend_is_taken_without_a_name_and_its_slash_dropped() {
        let endpoint = Endpoint::from_registry(REGISTRY, None).unwrap();
        assert_eq!(endpoint.base_url, "http://127.0.0.1:11434");
        assert_eq!(endpoint.model, None);
    }

    #[test]
    fn a_backend_whose_key_variable_is_empty_is_refused_by_name() {
        let refused = Endpoint::from_registry(REGISTRY, Some("paid")).unwrap_err();
        assert!(
            refused.to_string().contains("$OXIDANT_TEXT_TEST_KEY_UNSET"),
            "{refused}"
        );
        assert!(Endpoint::from_registry(REGISTRY, Some("absent"))
            .unwrap_err()
            .to_string()
            .contains("'absent'"));
        assert!(Endpoint::from_registry("{", None).is_err());
    }

    #[test]
    fn the_callers_keys_win_over_request_extra() {
        let mut endpoint = Endpoint::new("http://x");
        endpoint.model = Some("m".into());
        endpoint.request_extra =
            serde_json::from_str(r#"{"num_ctx": 4096, "max_tokens": 1}"#).unwrap();
        let body = endpoint
            .chat_body(&[ChatMessage::user("hi")], &GenerationOptions::default())
            .unwrap();
        assert_eq!(body["max_tokens"], 128);
        assert_eq!(body["num_ctx"], 4096);
        assert_eq!(body["stream"], true);
        assert!(
            body.get("top_k").is_none(),
            "greedy sends no sampling knobs"
        );
        let sampled = GenerationOptions {
            temperature: 0.7,
            ..GenerationOptions::default()
        };
        assert_eq!(
            endpoint
                .chat_body(&[ChatMessage::user("hi")], &sampled)
                .unwrap()["top_k"],
            50
        );
        endpoint.model = None;
        assert!(matches!(
            endpoint.chat_body(&[ChatMessage::user("hi")], &sampled),
            Err(TextError::InvalidOptions(_))
        ));
    }

    #[test]
    fn a_url_beats_the_environment_which_beats_a_named_backend() {
        let dir =
            std::env::temp_dir().join(format!("oxidant-endpoint-choice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let registry = dir.join("backends.json");
        std::fs::write(&registry, REGISTRY).unwrap();
        let named = EndpointChoice {
            backend: Some("local".into()),
            model: Some("chosen".into()),
            ..EndpointChoice::default()
        };
        let from_registry = named.resolve_with(None, Some(registry.clone())).unwrap();
        assert_eq!(
            (
                from_registry.base_url.as_str(),
                from_registry.model.as_deref()
            ),
            ("http://127.0.0.1:11434", Some("chosen"))
        );
        let from_env = named
            .resolve_with(Some("http://env".into()), Some(registry.clone()))
            .unwrap();
        assert_eq!(from_env.base_url, "http://env");
        let explicit = EndpointChoice {
            base_url: Some("http://flag/".into()),
            ..named.clone()
        };
        assert_eq!(
            explicit
                .resolve_with(Some("http://env".into()), None)
                .unwrap()
                .base_url,
            "http://flag"
        );
        let nothing = EndpointChoice::default()
            .resolve_with(None, None)
            .unwrap_err();
        assert!(nothing.to_string().contains("no endpoint"), "{nothing}");
        let missing = EndpointChoice::default()
            .resolve_with(None, Some(dir.join("absent.json")))
            .unwrap_err();
        assert!(missing.to_string().contains("absent.json"), "{missing}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stream_lines_become_events() {
        let text = r#"data: {"choices":[{"delta":{"content":"Pa"},"finish_reason":null}]}"#;
        assert_eq!(
            parse_stream_line(text).unwrap(),
            vec![StreamEvent::Text("Pa".into())]
        );
        let end = r#"data: {"choices":[{"delta":{},"finish_reason":"length"}],"usage":{"prompt_tokens":7,"completion_tokens":3}}"#;
        assert_eq!(
            parse_stream_line(end).unwrap(),
            vec![
                StreamEvent::Finish(FinishReason::Length),
                StreamEvent::Usage {
                    prompt: 7,
                    completion: 3
                }
            ]
        );
        assert_eq!(
            parse_stream_line("data: [DONE]").unwrap(),
            vec![StreamEvent::Done]
        );
        assert_eq!(parse_stream_line(": keep-alive").unwrap(), vec![]);
        assert!(parse_stream_line("data: {broken").is_err());
        assert!(
            parse_stream_line(r#"data: {"error":{"message":"model gone"}}"#)
                .unwrap_err()
                .to_string()
                .contains("model gone")
        );
    }

    #[test]
    fn embeddings_are_ordered_by_index_and_normalised() {
        let answer = json!({"data": [{"index": 1, "embedding": [0.0, 2.0]}, {"index": 0, "embedding": [3.0, 4.0]}]});
        let vectors = parse_embeddings(&answer, 2).unwrap();
        assert_eq!(vectors, vec![vec![0.6, 0.8], vec![0.0, 1.0]]);
        assert!(parse_embeddings(&answer, 3).is_err());
        let zero = json!({"data": [{"embedding": [0.0, 0.0]}]});
        assert!(parse_embeddings(&zero, 1).is_err());
        let repeated =
            json!({"data": [{"index": 0, "embedding": [1.0]}, {"index": 0, "embedding": [1.0]}]});
        assert!(parse_embeddings(&repeated, 2).is_err());
    }
}
