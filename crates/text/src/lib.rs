//! Text generation for the Kataglyphis family: the chain's ONNX Runtime GenAI in process, or an OpenAI-compatible
//! endpoint. Both follow ANTfrastructure's text contract (`docs/language-ai.md`); see `docs/text.md` here.

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
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            max_new_tokens: 128,
            temperature: 0.0,
            top_p: 1.0,
            top_k: 50,
        }
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
    Ok(())
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
        ];
        let texts: std::collections::HashSet<String> =
            errors.iter().map(ToString::to_string).collect();
        assert_eq!(texts.len(), errors.len());
        assert!(TextError::EndpointStatus(503, "busy".into())
            .to_string()
            .contains("HTTP 503"));
    }
}
