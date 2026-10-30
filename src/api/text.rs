//! flutter_rust_bridge surface for language models: `kataglyphis_text` mapped to flat types for Dart, see docs/text.md.

use crate::frb_generated::StreamSink;

pub struct TextMessage {
    /// `system`, `user`, `assistant` or `tool`.
    pub role: String,
    pub content: String,
}

pub struct TextOptions {
    pub max_new_tokens: u32,
    /// 0 decodes greedily and ignores `top_p` and `top_k`.
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: u32,
}

/// Where the answer comes from: `model_dir` runs the chain's GenAI in process (Linux and Windows); without it the
/// endpoint fields resolve as the CLI's do.
pub struct TextSource {
    pub model_dir: Option<String>,
    /// `cpu`, `cuda` or `dml`, for `model_dir`.
    pub provider: String,
    pub base_url: Option<String>,
    /// A backend named in `backends_path`, or `LLM_BACKENDS`.
    pub backend: Option<String>,
    pub backends_path: Option<String>,
    pub model: Option<String>,
    /// Overrides the key a backend names, for platforms without its environment variable.
    pub api_key: Option<String>,
}

/// A flattened stream event: `piece` sets `text`; `done`, the last one, sets the finish reason and the figures.
pub struct TextEvent {
    pub kind: String,
    pub text: Option<String>,
    /// `stop`, `length` or `cancelled`.
    pub finish_reason: Option<String>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub time_to_first_token_ms: Option<f64>,
    pub decode_tokens_per_second: Option<f64>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn text_default_options() -> TextOptions {
    TextOptions {
        max_new_tokens: 128,
        temperature: 0.0,
        top_p: 1.0,
        top_k: 50,
    }
}

/// Whether this build can run a model directory in process: the chain GenAI loads here.
#[flutter_rust_bridge::frb(sync)]
pub fn text_local_available() -> bool {
    enabled::local_available()
}

/// Streams the answer into `sink` and returns after its `done` event; a second call waits for the first.
pub fn text_generate(
    source: TextSource,
    messages: Vec<TextMessage>,
    options: TextOptions,
    sink: StreamSink<TextEvent>,
) -> Result<(), String> {
    enabled::generate(source, messages, options, sink).map_err(|e| e.to_string())
}

/// Ends the running generation after its current token; its `done` event says `cancelled`.
#[flutter_rust_bridge::frb(sync)]
pub fn text_cancel() {
    enabled::cancel();
}

/// One L2-normalised vector per text, from the endpoint the source names.
pub fn text_embed(source: TextSource, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
    enabled::embed(source, texts).map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
mod enabled {
    use std::sync::{Mutex, OnceLock};

    use kataglyphis_text::endpoint::{Endpoint, EndpointChoice};
    use kataglyphis_text::{
        CancelToken, ChatMessage, FinishReason, GenerationOptions, GenerationResult, TextBackend,
        TextError,
    };

    use super::{TextEvent, TextMessage, TextOptions, TextSource};
    use crate::frb_generated::StreamSink;

    fn cancel_token() -> &'static CancelToken {
        static TOKEN: OnceLock<CancelToken> = OnceLock::new();
        TOKEN.get_or_init(CancelToken::default)
    }

    pub(super) fn cancel() {
        cancel_token().cancel();
    }

    fn endpoint(source: &TextSource) -> Result<Endpoint, TextError> {
        let mut endpoint = EndpointChoice {
            base_url: source.base_url.clone(),
            backend: source.backend.clone(),
            registry: source.backends_path.as_ref().map(Into::into),
            model: source.model.clone(),
        }
        .resolve()?;
        if source.api_key.is_some() {
            endpoint.api_key.clone_from(&source.api_key);
        }
        Ok(endpoint)
    }

    pub(super) fn embed(
        source: TextSource,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, TextError> {
        endpoint(&source)?.embed(&texts)
    }

    fn piece(text: &str) -> TextEvent {
        TextEvent {
            kind: "piece".into(),
            text: Some(text.into()),
            finish_reason: None,
            prompt_tokens: None,
            completion_tokens: None,
            time_to_first_token_ms: None,
            decode_tokens_per_second: None,
        }
    }

    pub(super) fn done(result: &GenerationResult) -> TextEvent {
        let reason = match result.finish_reason {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
            FinishReason::Cancelled => "cancelled",
        };
        TextEvent {
            kind: "done".into(),
            text: None,
            finish_reason: Some(reason.into()),
            prompt_tokens: Some(result.prompt_tokens as u32),
            completion_tokens: Some(result.completion_tokens as u32),
            time_to_first_token_ms: Some(result.time_to_first_token_ms),
            decode_tokens_per_second: Some(result.decode_tokens_per_second),
        }
    }

    pub(super) fn request(
        messages: Vec<TextMessage>,
        options: &TextOptions,
    ) -> (Vec<ChatMessage>, GenerationOptions) {
        let messages = messages
            .into_iter()
            .map(|m| ChatMessage::new(m.role, m.content))
            .collect();
        let options = GenerationOptions {
            max_new_tokens: options.max_new_tokens,
            temperature: options.temperature,
            top_p: options.top_p,
            top_k: options.top_k,
        };
        (messages, options)
    }

    /// One generation at a time: the app has one conversation running, and a local model is too large to hold twice.
    fn turn() -> std::sync::MutexGuard<'static, Option<local::Cached>> {
        static TURN: Mutex<Option<local::Cached>> = Mutex::new(None);
        TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn generate(
        source: TextSource,
        messages: Vec<TextMessage>,
        options: TextOptions,
        sink: StreamSink<TextEvent>,
    ) -> Result<(), TextError> {
        let (messages, options) = request(messages, &options);
        let mut cached = turn();
        let mut on_text = |text: &str| {
            let _ = sink.add(piece(text));
        };
        let result = match &source.model_dir {
            Some(dir) => local::backend(&mut cached, dir, &source.provider)?.generate(
                &messages,
                &options,
                cancel_token(),
                &mut on_text,
            ),
            None => endpoint(&source)?.generate(&messages, &options, cancel_token(), &mut on_text),
        }?;
        let _ = sink.add(done(&result));
        Ok(())
    }

    #[cfg(any(target_os = "linux", windows))]
    mod local {
        use kataglyphis_text::genai::{GenaiGenerator, Provider};
        use kataglyphis_text::TextError;

        /// The last model loaded, kept for the next turn of the conversation.
        pub(super) struct Cached {
            key: (String, Provider),
            generator: GenaiGenerator,
        }

        pub(super) fn provider(name: &str) -> Result<Provider, TextError> {
            match name {
                "" | "cpu" => Ok(Provider::Cpu),
                "cuda" => Ok(Provider::Cuda),
                "dml" => Ok(Provider::DirectMl),
                other => Err(TextError::ProviderUnavailable(format!(
                    "'{other}' is not cpu, cuda or dml"
                ))),
            }
        }

        pub(super) fn backend<'a>(
            cached: &'a mut Option<Cached>,
            dir: &str,
            provider_name: &str,
        ) -> Result<&'a mut GenaiGenerator, TextError> {
            let key = (dir.to_string(), provider(provider_name)?);
            if cached.as_ref().is_none_or(|c| c.key != key) {
                *cached = None;
                let generator = GenaiGenerator::load(std::path::Path::new(dir), key.1)?;
                *cached = Some(Cached { key, generator });
            }
            Ok(&mut cached.as_mut().expect("loaded above").generator)
        }

        pub(super) fn available() -> bool {
            GenaiGenerator::library_available()
        }
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    mod local {
        use kataglyphis_text::{TextBackend, TextError};

        pub(super) enum Cached {}

        pub(super) fn backend<'a>(
            _: &'a mut Option<Cached>,
            _: &str,
            _: &str,
        ) -> Result<&'a mut dyn TextBackend, TextError> {
            Err(TextError::LibraryUnavailable(
                "the chain's ONNX Runtime GenAI exists for Linux and Windows only; use an endpoint"
                    .into(),
            ))
        }

        pub(super) fn available() -> bool {
            false
        }
    }

    pub(super) fn local_available() -> bool {
        local::available()
    }
}

/// The web build has no file system for a model and no blocking socket for an endpoint; Dart talks to it there.
#[cfg(target_arch = "wasm32")]
mod enabled {
    use super::{TextEvent, TextMessage, TextOptions, TextSource};
    use crate::frb_generated::StreamSink;

    const WEB: &str = "the web build reaches language models from Dart, not through this bridge";

    pub(super) fn cancel() {}

    pub(super) fn local_available() -> bool {
        false
    }

    pub(super) fn generate(
        _: TextSource,
        _: Vec<TextMessage>,
        _: TextOptions,
        _: StreamSink<TextEvent>,
    ) -> Result<(), String> {
        Err(WEB.into())
    }

    pub(super) fn embed(_: TextSource, _: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        Err(WEB.into())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use kataglyphis_text::{FinishReason, GenerationResult};

    #[test]
    fn the_default_options_are_the_contracts() {
        let options = text_default_options();
        assert_eq!(
            (options.max_new_tokens, options.temperature, options.top_k),
            (128, 0.0, 50)
        );
    }

    #[test]
    fn the_done_event_carries_the_finish_reason_and_the_figures() {
        let result = GenerationResult {
            text: "x".into(),
            tokens: vec![1],
            finish_reason: FinishReason::Cancelled,
            prompt_tokens: 3,
            completion_tokens: 1,
            time_to_first_token_ms: 2.5,
            decode_tokens_per_second: 0.0,
        };
        let event = enabled::done(&result);
        assert_eq!(event.kind, "done");
        assert_eq!(event.finish_reason.as_deref(), Some("cancelled"));
        assert_eq!(
            (event.prompt_tokens, event.completion_tokens),
            (Some(3), Some(1))
        );
    }

    #[test]
    fn messages_and_options_map_onto_the_text_crate() {
        let (messages, options) = enabled::request(
            vec![TextMessage {
                role: "system".into(),
                content: "be brief".into(),
            }],
            &TextOptions {
                max_new_tokens: 7,
                temperature: 0.5,
                top_p: 0.9,
                top_k: 3,
            },
        );
        assert_eq!(
            (messages[0].role.as_str(), messages[0].content.as_str()),
            ("system", "be brief")
        );
        assert_eq!((options.max_new_tokens, options.top_k), (7, 3));
    }
}
