//! Text generation for the engine, in the C++ namespace `kataglyphis::oxidant::accel::text`: `kataglyphis_text`'s
//! GenAI generator, pulled piece by piece on the caller's thread, since cxx cannot take a C++ callback. The engine's
//! `TextGenerator` and its C ABI stay as adapters: a status is the C ABI's number, and GenAI's own words come apart
//! from OxidANT's sentence, empty for a refusal of OxidANT's own.

use std::path::Path;

use kataglyphis_text::genai::{self, GenaiGenerator, Pulled};
use kataglyphis_text::{
    CancelToken, ChatMessage, FinishReason, GenerationOptions, GenerationResult, TextError,
};

use crate::{guarded, nothing};

#[allow(unsafe_code)]
#[cxx::bridge(namespace = "kataglyphis::oxidant::accel::text")]
pub mod ffi {
    /// The engine's `ExecutionProvider`, value for value; GenAI has a route to the CPU, CUDA and DirectML only.
    #[derive(Debug)]
    enum Provider {
        Cpu = 0,
        Cuda = 1,
        MigraphX = 2,
        DirectMl = 3,
        Xnnpack = 4,
        Dnnl = 5,
    }

    /// The C ABI's status: the engine's `TextGenError` plus one, and `Ok` for none.
    #[derive(Debug)]
    enum Status {
        Ok = 0,
        LibraryUnavailable = 1,
        ModelLoadFailed = 2,
        ProviderUnavailable = 3,
        TokenizerFailed = 4,
        ChatTemplateFailed = 5,
        EncodeFailed = 6,
        InvalidOptions = 7,
        GenerationFailed = 8,
        NotInitialized = 9,
        JsonSchemaUnsupported = 10,
        ContextLengthExceeded = 11,
    }

    /// The C ABI's finish reasons.
    #[derive(Debug)]
    enum Finish {
        Stop = 0,
        Length = 1,
        Cancelled = 2,
    }

    /// One turn of a chat: `system`, `user`, `assistant` or `tool`, and its text, both UTF-8.
    struct Message<'a> {
        role: &'a [u8],
        content: &'a [u8],
    }

    /// One stop string, UTF-8 without a NUL byte.
    struct Stop<'a> {
        text: &'a [u8],
    }

    /// The engine's `GenerationOptions`: signed as there, a seed only with `has_seed`, an empty schema for none.
    struct Options<'a> {
        max_new_tokens: i32,
        temperature: f64,
        top_p: f64,
        top_k: i32,
        has_seed: bool,
        seed: i64,
        json_schema: &'a [u8],
        stop: &'a [Stop<'a>],
    }

    /// Why a call failed: `Ok` and two empty texts after a success.
    #[derive(Debug)]
    struct Failure {
        status: Status,
        /// OxidANT's sentence, the kind's words and the detail.
        message: String,
        /// GenAI's own words, `<step>: <message>`, which the engine's `last_error` returns; empty for OxidANT's own.
        genai: String,
    }

    /// How a generation ended, on its last piece.
    #[derive(Debug)]
    struct Stats {
        finish: Finish,
        prompt_tokens: usize,
        completion_tokens: usize,
        /// Characters of `<think>` blocks with their tags, which the text leaves out.
        thinking_chars: usize,
        time_to_first_token_ms: f64,
        decode_tokens_per_second: f64,
    }

    /// One pull: the answer's next text and, once `finished`, the failure or the whole answer, the stats and every
    /// token id.
    #[derive(Debug)]
    struct Piece {
        text: String,
        finished: bool,
        /// The answer without the model's thinking, on the last piece; less than the pieces after a late `</think>`.
        answer: String,
        failure: Failure,
        stats: Stats,
        tokens: Vec<i32>,
    }

    extern "Rust" {
        type TextGenerator;
        type CancelHandle;

        /// Whether the chain GenAI is found, verified and bound in this process; the first call loads it.
        fn genai_available() -> bool;
        /// The decoder threads a load sets for this model directory: the text rule, or 0 when the model names its own.
        fn text_intra_op_threads(model_dir: &[u8]) -> i32;
        fn new_text_generator() -> Box<TextGenerator>;
        /// Loads a model directory; a failed load keeps the model loaded before.
        fn initialize(self: &mut TextGenerator, model_dir: &[u8], provider: Provider) -> Failure;
        fn is_initialized(self: &TextGenerator) -> bool;
        /// Starts a generation, dropping one left unfinished; the pulls decode it.
        fn start(self: &mut TextGenerator, messages: &[Message], options: &Options) -> Failure;
        /// Decodes up to the answer's next text, or to the end, which a piece with `finished` reports.
        fn next_piece(self: &mut TextGenerator) -> Piece;
        /// A handle that cancels this generator's generations from any thread.
        fn cancel_handle(self: &TextGenerator) -> Box<CancelHandle>;
        /// Ends the running generation at its next pull, before that pull's first token.
        fn cancel(self: &CancelHandle);
    }
}

/// Never unwinds into C++, like every bridged call.
pub fn genai_available() -> bool {
    guarded(GenaiGenerator::library_available, nothing)
}

/// Reads only the model's `genai_config.json`; a path that is not UTF-8 reads none and takes the rule.
pub fn text_intra_op_threads(model_dir: &[u8]) -> i32 {
    guarded(
        || {
            let dir = String::from_utf8_lossy(model_dir);
            genai::default_text_threads(Path::new(dir.as_ref()))
                .and_then(|threads| i32::try_from(threads).ok())
                .unwrap_or(0)
        },
        nothing,
    )
}

/// A generator without a model, as the engine's constructor leaves one.
pub fn new_text_generator() -> Box<TextGenerator> {
    guarded(Box::default, nothing)
}

/// No model until `initialize`, then one generation at a time, which the engine's lock keeps so.
#[derive(Default)]
pub struct TextGenerator {
    engine: Option<GenaiGenerator>,
    cancel: CancelToken,
}

/// Cancels from any thread, as the engine's `request_cancel` does.
#[derive(Default)]
pub struct CancelHandle(CancelToken);

impl CancelHandle {
    pub fn cancel(&self) {
        guarded(|| self.0.cancel(), nothing);
    }
}

impl TextGenerator {
    pub fn initialize(&mut self, model_dir: &[u8], provider: ffi::Provider) -> ffi::Failure {
        guarded(
            || self.load(model_dir, provider),
            panicked(ffi::Status::ModelLoadFailed),
        )
    }

    pub fn is_initialized(&self) -> bool {
        guarded(|| self.engine.is_some(), nothing)
    }

    pub fn start(&mut self, messages: &[ffi::Message], options: &ffi::Options) -> ffi::Failure {
        guarded(
            || self.begin(messages, options),
            panicked(ffi::Status::GenerationFailed),
        )
    }

    pub fn next_piece(&mut self) -> ffi::Piece {
        guarded(
            || self.pull(),
            |reason| ended(panicked(ffi::Status::GenerationFailed)(reason)),
        )
    }

    pub fn cancel_handle(&self) -> Box<CancelHandle> {
        guarded(
            || Box::new(CancelHandle(self.cancel.clone())),
            |_| Box::new(CancelHandle(self.cancel.clone())),
        )
    }

    /// The provider is refused before the library loads, as the engine refuses it.
    fn load(&mut self, model_dir: &[u8], provider: ffi::Provider) -> ffi::Failure {
        genai::take_genai_failure();
        let loaded = genai_provider(provider).and_then(|provider| {
            let dir = std::str::from_utf8(model_dir).map_err(|_| {
                TextError::ModelLoadFailed("the model directory's path is not UTF-8".into())
            })?;
            GenaiGenerator::load(Path::new(dir), provider)
        });
        match loaded {
            Ok(engine) => {
                self.engine = Some(engine);
                succeeded()
            }
            Err(error) => failure(&error),
        }
    }

    // A panic drops the engine taken out here, so a later call finds no half-changed model but NotInitialized.
    fn begin(&mut self, messages: &[ffi::Message], options: &ffi::Options) -> ffi::Failure {
        genai::take_genai_failure();
        let Some(mut engine) = self.engine.take() else {
            return not_initialized();
        };
        let started = chat(messages).and_then(|messages| {
            engine.start(&messages, &generation_options(options)?, &self.cancel)
        });
        self.engine = Some(engine);
        started.map_or_else(|error| failure(&error), |()| succeeded())
    }

    fn pull(&mut self) -> ffi::Piece {
        genai::take_genai_failure();
        let Some(mut engine) = self.engine.take() else {
            return ended(not_initialized());
        };
        let pulled = engine.pull();
        self.engine = Some(engine);
        piece(pulled)
    }
}

/// GenAI's route for an engine provider; MIGraphX, XNNPACK and DNNL have none.
fn genai_provider(provider: ffi::Provider) -> Result<genai::Provider, TextError> {
    match provider {
        ffi::Provider::Cpu => Ok(genai::Provider::Cpu),
        ffi::Provider::Cuda => Ok(genai::Provider::Cuda),
        ffi::Provider::DirectMl => Ok(genai::Provider::DirectMl),
        other => Err(TextError::ProviderUnavailable(format!(
            "GenAI has no route to the engine's provider {}",
            other.repr
        ))),
    }
}

fn utf8(bytes: &[u8], what: &str) -> Result<String, TextError> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| TextError::InvalidOptions(format!("{what} is not UTF-8")))
}

fn chat(messages: &[ffi::Message]) -> Result<Vec<ChatMessage>, TextError> {
    messages
        .iter()
        .map(|m| {
            Ok(ChatMessage::new(
                utf8(m.role, "a role")?,
                utf8(m.content, "a message")?,
            ))
        })
        .collect()
}

/// The engine's signed options as `kataglyphis_text`'s; a negative count or seed is invalid there as here.
fn generation_options(options: &ffi::Options) -> Result<GenerationOptions, TextError> {
    let unsigned = |value: i64, name: &str| {
        u32::try_from(value).map_err(|_| {
            TextError::InvalidOptions(format!("{name} is {value}, outside 0 to {}", u32::MAX))
        })
    };
    let seed = if options.has_seed {
        Some(unsigned(options.seed, "seed")?)
    } else {
        None
    };
    let json_schema = if options.json_schema.is_empty() {
        None
    } else {
        Some(utf8(options.json_schema, "json_schema")?)
    };
    let stop = options
        .stop
        .iter()
        .map(|stop| utf8(stop.text, "a stop string"))
        .collect::<Result<_, _>>()?;
    Ok(GenerationOptions {
        max_new_tokens: unsigned(options.max_new_tokens.into(), "max_new_tokens")?,
        temperature: options.temperature,
        top_p: options.top_p,
        top_k: unsigned(options.top_k.into(), "top_k")?,
        seed,
        json_schema,
        stop,
    })
}

/// The C ABI's status of each error; a new kind fails to compile here until it has one.
fn status(error: &TextError) -> ffi::Status {
    match error {
        TextError::LibraryUnavailable(_) => ffi::Status::LibraryUnavailable,
        TextError::ModelLoadFailed(_) => ffi::Status::ModelLoadFailed,
        TextError::ProviderUnavailable(_) => ffi::Status::ProviderUnavailable,
        TextError::TokenizerFailed(_) => ffi::Status::TokenizerFailed,
        TextError::ChatTemplateFailed(_) => ffi::Status::ChatTemplateFailed,
        TextError::EncodeFailed(_) => ffi::Status::EncodeFailed,
        TextError::InvalidOptions(_) => ffi::Status::InvalidOptions,
        TextError::GenerationFailed(_) => ffi::Status::GenerationFailed,
        TextError::JsonSchemaUnsupported(_) => ffi::Status::JsonSchemaUnsupported,
        TextError::ContextLengthExceeded(_) => ffi::Status::ContextLengthExceeded,
        // An endpoint's and an embedding's kinds, which a GenAI generation never gives.
        TextError::EndpointUnreachable(_)
        | TextError::EndpointStatus(..)
        | TextError::EndpointTimedOut(_)
        | TextError::StreamLineTooLong(_)
        | TextError::EmbeddingsUnsupported(_) => ffi::Status::GenerationFailed,
    }
}

fn failure(error: &TextError) -> ffi::Failure {
    ffi::Failure {
        status: status(error),
        message: error.to_string(),
        genai: genai::take_genai_failure().unwrap_or_default(),
    }
}

fn succeeded() -> ffi::Failure {
    ffi::Failure {
        status: ffi::Status::Ok,
        message: String::new(),
        genai: String::new(),
    }
}

fn not_initialized() -> ffi::Failure {
    ffi::Failure {
        status: ffi::Status::NotInitialized,
        message: "the generator is not initialized".into(),
        genai: String::new(),
    }
}

/// The failure a caught panic leaves, with the status the call would have failed with.
fn panicked(status: ffi::Status) -> impl FnOnce(String) -> ffi::Failure {
    move |reason| ffi::Failure {
        status,
        message: format!("OxidANT panicked: {reason}"),
        genai: String::new(),
    }
}

fn no_stats() -> ffi::Stats {
    ffi::Stats {
        finish: ffi::Finish::Stop,
        prompt_tokens: 0,
        completion_tokens: 0,
        thinking_chars: 0,
        time_to_first_token_ms: 0.0,
        decode_tokens_per_second: 0.0,
    }
}

fn stats(result: &GenerationResult) -> ffi::Stats {
    ffi::Stats {
        finish: match result.finish_reason {
            FinishReason::Stop => ffi::Finish::Stop,
            FinishReason::Length => ffi::Finish::Length,
            FinishReason::Cancelled => ffi::Finish::Cancelled,
        },
        prompt_tokens: result.prompt_tokens,
        completion_tokens: result.completion_tokens,
        thinking_chars: result.thinking_chars,
        time_to_first_token_ms: result.time_to_first_token_ms,
        decode_tokens_per_second: result.decode_tokens_per_second,
    }
}

/// The last piece of a generation that failed.
fn ended(failure: ffi::Failure) -> ffi::Piece {
    ffi::Piece {
        text: String::new(),
        finished: true,
        answer: String::new(),
        failure,
        stats: no_stats(),
        tokens: Vec::new(),
    }
}

fn piece(pulled: Result<Pulled, TextError>) -> ffi::Piece {
    match pulled {
        Ok(Pulled { text, done: None }) => ffi::Piece {
            text,
            finished: false,
            answer: String::new(),
            failure: succeeded(),
            stats: no_stats(),
            tokens: Vec::new(),
        },
        Ok(Pulled {
            text,
            done: Some(result),
        }) => ffi::Piece {
            text,
            finished: true,
            failure: succeeded(),
            stats: stats(&result),
            answer: result.text,
            tokens: result.tokens,
        },
        Err(error) => ended(failure(&error)),
    }
}

#[cfg(test)]
mod tests;
