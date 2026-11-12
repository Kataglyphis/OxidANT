//! Text generation for the Kataglyphis family: the chain's ONNX Runtime GenAI in process, or an OpenAI-compatible
//! endpoint. Both follow the text contract in `third_party/ANTfrastructure/docs/language-ai.md`; see `docs/text.md` here.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
pub mod endpoint;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod genai;

/// One turn of a chat; `role` is `system`, `user`, `assistant` or `tool`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new("user", content)
    }
}

/// Temperature 0, the default, decodes greedily and ignores `top_p` and `top_k`.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationOptions {
    pub max_new_tokens: u32,
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: u32,
    /// Makes sampling repeatable: the same seed, model and options yield the same tokens.
    pub seed: Option<u32>,
    /// A JSON schema the answer must match, as JSON text.
    pub json_schema: Option<String>,
    /// Generation ends before the first of these strings, which the answer leaves out.
    pub stop: Vec<String>,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            max_new_tokens: 128,
            temperature: 0.0,
            top_p: 1.0,
            top_k: 50,
            seed: None,
            json_schema: None,
            stop: Vec::new(),
        }
    }
}

impl GenerationOptions {
    /// The schema as JSON; [`check_request`] has refused one that is not an object.
    pub(crate) fn schema_value(&self) -> Option<serde_json::Value> {
        self.json_schema
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    /// The model ended its reply.
    Stop,
    /// `max_new_tokens` ran out.
    Length,
    /// A [`CancelToken`] ended the run after its current token.
    Cancelled,
}

/// An endpoint reports no token ids, so `tokens` is empty there and `completion_tokens` comes from its usage.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationResult {
    pub text: String,
    pub tokens: Vec<i32>,
    pub finish_reason: FinishReason,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub time_to_first_token_ms: f64,
    pub decode_tokens_per_second: f64,
}

/// Each failure of the contract, never an empty answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    LibraryUnavailable(String),
    ModelLoadFailed(String),
    ProviderUnavailable(String),
    TokenizerFailed(String),
    ChatTemplateFailed(String),
    EncodeFailed(String),
    InvalidOptions(String),
    GenerationFailed(String),
    EndpointUnreachable(String),
    EndpointStatus(u16, String),
    /// The byte limit one streamed line exceeded.
    StreamLineTooLong(usize),
    /// This GenAI build has no llguidance, so it cannot hold an answer to a schema.
    JsonSchemaUnsupported(String),
    /// This model directory was exported without the `hidden_states` output embeddings read.
    EmbeddingsUnsupported(String),
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LibraryUnavailable(d) => {
                write!(f, "the ONNX Runtime GenAI library could not be loaded: {d}")
            }
            Self::ModelLoadFailed(d) => write!(f, "the model directory could not be loaded: {d}"),
            Self::ProviderUnavailable(d) => {
                write!(f, "the execution provider is not available: {d}")
            }
            Self::TokenizerFailed(d) => write!(f, "the tokenizer failed: {d}"),
            Self::ChatTemplateFailed(d) => write!(f, "the chat template could not be applied: {d}"),
            Self::EncodeFailed(d) => write!(f, "the prompt could not be encoded: {d}"),
            Self::InvalidOptions(d) => write!(f, "the messages or options are invalid: {d}"),
            Self::GenerationFailed(d) => write!(f, "generation failed: {d}"),
            Self::EndpointUnreachable(d) => write!(f, "the endpoint is unreachable: {d}"),
            Self::EndpointStatus(code, d) => write!(f, "the endpoint answered HTTP {code}: {d}"),
            Self::StreamLineTooLong(limit) => {
                write!(f, "the endpoint streamed a line longer than {limit} bytes")
            }
            Self::JsonSchemaUnsupported(d) => {
                write!(f, "json_schema is unsupported on this lane: {d}")
            }
            Self::EmbeddingsUnsupported(d) => {
                write!(f, "this model cannot embed: {d}")
            }
        }
    }
}

impl std::error::Error for TextError {}

/// Shared between the generating thread and any other; cloning keeps one flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// A backend clears it as a run starts, so a cancel applies to the running generation only.
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// An `on_text` that writes each piece to `out` and cancels the generation at the first write that fails: once the
/// reader has left (`chat | head`), the rest of the answer would be decoded for nobody.
pub fn write_pieces<'a, W: std::io::Write>(
    out: &'a mut W,
    cancel: &'a CancelToken,
    failed: &'a mut Option<std::io::Error>,
) -> impl FnMut(&str) + 'a {
    move |piece| {
        if failed.is_some() {
            return;
        }
        if let Err(e) = out.write_all(piece.as_bytes()).and_then(|()| out.flush()) {
            *failed = Some(e);
            cancel.cancel();
        }
    }
}

/// What every backend offers: messages in, text through `on_text` as it is generated.
pub trait TextBackend {
    fn generate(
        &mut self,
        messages: &[ChatMessage],
        options: &GenerationOptions,
        cancel: &CancelToken,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<GenerationResult, TextError>;
}

/// Refuses what no backend can answer, so both report it the same way.
pub(crate) fn check_request(
    messages: &[ChatMessage],
    options: &GenerationOptions,
) -> Result<(), TextError> {
    if messages.is_empty() {
        return Err(TextError::InvalidOptions("no messages".into()));
    }
    if options.max_new_tokens == 0 {
        return Err(TextError::InvalidOptions("max_new_tokens is 0".into()));
    }
    // GenAI reads the seed as a 32-bit signed integer and takes -1 for a random one.
    if options.seed.is_some_and(|s| s > i32::MAX as u32) {
        return Err(TextError::InvalidOptions(format!(
            "seed is above {}",
            i32::MAX
        )));
    }
    if options.stop.iter().any(String::is_empty) {
        return Err(TextError::InvalidOptions("a stop string is empty".into()));
    }
    if options.json_schema.is_some() && !options.schema_value().is_some_and(|v| v.is_object()) {
        return Err(TextError::InvalidOptions(
            "json_schema is not a JSON object".into(),
        ));
    }
    Ok(())
}

/// Where a stop string first starts in `text`, and how much of `text` is safe to show: the rest may begin one.
pub(crate) fn stop_scan(text: &str, stops: &[String]) -> (Option<usize>, usize) {
    if let Some(at) = stops.iter().filter_map(|s| text.find(s.as_str())).min() {
        return (Some(at), at);
    }
    let held = stops
        .iter()
        .flat_map(|stop| {
            stop.char_indices()
                .skip(1)
                .map(|(i, _)| &stop[..i])
                .filter(|prefix| text.ends_with(prefix))
                .map(str::len)
        })
        .max()
        .unwrap_or(0);
    (None, text.len() - held)
}

/// Messages as the OpenAI chat format, which GenAI's chat template reads too.
pub(crate) fn messages_json(messages: &[ChatMessage]) -> serde_json::Value {
    serde_json::Value::Array(
        messages
            .iter()
            .map(|m| serde_json::json!({ "role": m.role, "content": m.content }))
            .collect(),
    )
}

/// `vector` scaled to length 1, or `None` when it has no direction: empty, all zero or not finite.
pub(crate) fn unit_length(vector: Vec<f32>) -> Option<Vec<f32>> {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    (norm.is_finite() && norm > 0.0).then(|| vector.into_iter().map(|v| v / norm).collect())
}

/// The decode rate after the first token, 0 when fewer than two arrived.
pub(crate) fn decode_rate(tokens: usize, first_to_last_seconds: f64) -> f64 {
    if tokens < 2 || first_to_last_seconds <= 0.0 {
        return 0.0;
    }
    (tokens - 1) as f64 / first_to_last_seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_messages_and_a_zero_budget_are_refused() {
        let options = GenerationOptions::default();
        assert!(matches!(
            check_request(&[], &options),
            Err(TextError::InvalidOptions(_))
        ));
        let zero = GenerationOptions {
            max_new_tokens: 0,
            ..options.clone()
        };
        assert!(matches!(
            check_request(&[ChatMessage::user("a")], &zero),
            Err(TextError::InvalidOptions(_))
        ));
        assert_eq!(check_request(&[ChatMessage::user("a")], &options), Ok(()));
    }

    #[test]
    fn the_defaults_are_the_contracts() {
        let options = GenerationOptions::default();
        assert_eq!(options.max_new_tokens, 128);
        assert_eq!(options.temperature, 0.0);
        assert_eq!((options.seed, options.json_schema), (None, None));
        assert!(options.stop.is_empty());
    }

    #[test]
    fn a_large_seed_an_empty_stop_and_a_schema_that_is_no_object_are_refused() {
        let one = [ChatMessage::user("a")];
        for options in [
            GenerationOptions {
                seed: Some(i32::MAX as u32 + 1),
                ..GenerationOptions::default()
            },
            GenerationOptions {
                stop: vec![String::new()],
                ..GenerationOptions::default()
            },
            GenerationOptions {
                json_schema: Some("[1]".into()),
                ..GenerationOptions::default()
            },
            GenerationOptions {
                json_schema: Some("{".into()),
                ..GenerationOptions::default()
            },
        ] {
            assert!(
                matches!(
                    check_request(&one, &options),
                    Err(TextError::InvalidOptions(_))
                ),
                "{options:?}"
            );
        }
        let fine = GenerationOptions {
            seed: Some(i32::MAX as u32),
            json_schema: Some(r#"{"type":"object"}"#.into()),
            stop: vec!["x".into()],
            ..GenerationOptions::default()
        };
        assert_eq!(check_request(&one, &fine), Ok(()));
    }

    #[test]
    fn a_stop_scan_cuts_at_the_earliest_stop_and_holds_back_a_partial_one() {
        let stops = vec!["fg".to_string(), "e".to_string()];
        assert_eq!(stop_scan("abcdefg", &stops), (Some(4), 4));
        assert_eq!(stop_scan("abcf", &stops), (None, 3));
        assert_eq!(stop_scan("abc", &stops), (None, 3));
        assert_eq!(
            stop_scan("x\u{e9}", &["\u{e9}t\u{e9}".to_string()]),
            (None, 1)
        );
        assert_eq!(stop_scan("anything", &[]), (None, 8));
    }

    #[test]
    fn messages_become_the_openai_chat_format() {
        let json = messages_json(&[
            ChatMessage::new("system", "be brief"),
            ChatMessage::user("hi \"there\""),
        ]);
        assert_eq!(
            json.to_string(),
            r#"[{"content":"be brief","role":"system"},{"content":"hi \"there\"","role":"user"}]"#
        );
    }

    #[test]
    fn a_cancel_is_shared_by_clones_and_cleared_by_reset() {
        let token = CancelToken::default();
        let other = token.clone();
        other.cancel();
        assert!(token.is_cancelled());
        token.reset();
        assert!(!other.is_cancelled());
    }

    #[test]
    fn the_decode_rate_counts_tokens_after_the_first() {
        assert_eq!(decode_rate(0, 1.0), 0.0);
        assert_eq!(decode_rate(1, 1.0), 0.0);
        assert_eq!(decode_rate(5, 2.0), 2.0);
        assert_eq!(decode_rate(5, 0.0), 0.0);
    }

    #[test]
    fn a_vector_scales_to_unit_length_and_one_without_a_direction_is_refused() {
        assert_eq!(unit_length(vec![3.0, 4.0]), Some(vec![0.6, 0.8]));
        for none in [
            vec![],
            vec![0.0, 0.0],
            vec![f32::NAN, 1.0],
            vec![f32::INFINITY],
        ] {
            assert_eq!(unit_length(none.clone()), None, "{none:?}");
        }
    }

    #[test]
    fn every_error_names_its_kind() {
        let errors = [
            TextError::LibraryUnavailable("x".into()),
            TextError::ModelLoadFailed("x".into()),
            TextError::ProviderUnavailable("x".into()),
            TextError::TokenizerFailed("x".into()),
            TextError::ChatTemplateFailed("x".into()),
            TextError::EncodeFailed("x".into()),
            TextError::InvalidOptions("x".into()),
            TextError::GenerationFailed("x".into()),
            TextError::EndpointUnreachable("x".into()),
            TextError::EndpointStatus(503, "x".into()),
            TextError::StreamLineTooLong(1),
            TextError::JsonSchemaUnsupported("x".into()),
            TextError::EmbeddingsUnsupported("x".into()),
        ];
        let texts: std::collections::HashSet<String> =
            errors.iter().map(ToString::to_string).collect();
        assert_eq!(texts.len(), errors.len());
        assert!(TextError::EndpointStatus(503, "busy".into())
            .to_string()
            .contains("HTTP 503"));
    }
}
