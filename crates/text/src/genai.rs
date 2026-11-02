//! The chain's ONNX Runtime GenAI through its C API, loaded at run time from the chain install and verified like ORT.

use std::ffi::{c_char, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use crate::{
    check_request, decode_rate, messages_json, stop_scan, CancelToken, ChatMessage, FinishReason,
    GenerationOptions, GenerationResult, TextBackend, TextError,
};

#[cfg(windows)]
const LIBRARY: &str = "onnxruntime-genai.dll";
#[cfg(not(windows))]
const LIBRARY: &str = "libonnxruntime-genai.so";

/// The chain install the family images carry when no variable names it.
#[cfg(not(windows))]
const IMAGE_CHAIN_DIR: &str = "/usr/local/lib/onnxruntime-genai/lib";

/// The chain's GenAI build tree, which its bundled dependencies embed through `__FILE__`; must match the hub's build.
#[cfg(windows)]
const CHAIN_BUILD_MARKER: &str = r"C:\temp\onnx-genai-src\build\";
#[cfg(not(windows))]
const CHAIN_BUILD_MARKER: &str = "/opt/onnxruntime-genai/build/";

/// A GenAI built without llguidance accepts the guidance and refuses the generator with this message.
const NO_GUIDANCE: &str = "Guidance is unavailable in this build";

#[repr(C)]
struct Opaque {
    _private: [u8; 0],
}

type Status = *mut Opaque;
type Handle = *mut Opaque;

struct Api {
    result_get_error: unsafe extern "C" fn(Status) -> *const c_char,
    destroy_result: unsafe extern "C" fn(Status),
    create_config: unsafe extern "C" fn(*const c_char, *mut Handle) -> Status,
    config_clear_providers: unsafe extern "C" fn(Handle) -> Status,
    config_append_provider: unsafe extern "C" fn(Handle, *const c_char) -> Status,
    destroy_config: unsafe extern "C" fn(Handle),
    create_model_from_config: unsafe extern "C" fn(Handle, *mut Handle) -> Status,
    destroy_model: unsafe extern "C" fn(Handle),
    create_tokenizer: unsafe extern "C" fn(Handle, *mut Handle) -> Status,
    destroy_tokenizer: unsafe extern "C" fn(Handle),
    tokenizer_get_eos_token_ids:
        unsafe extern "C" fn(Handle, *mut *const i32, *mut usize) -> Status,
    tokenizer_apply_chat_template: unsafe extern "C" fn(
        Handle,
        *const c_char,
        *const c_char,
        *const c_char,
        bool,
        *mut *const c_char,
    ) -> Status,
    destroy_string: unsafe extern "C" fn(*const c_char),
    create_sequences: unsafe extern "C" fn(*mut Handle) -> Status,
    destroy_sequences: unsafe extern "C" fn(Handle),
    tokenizer_encode: unsafe extern "C" fn(Handle, *const c_char, Handle) -> Status,
    sequences_get_sequence_count: unsafe extern "C" fn(Handle, usize) -> usize,
    create_tokenizer_stream: unsafe extern "C" fn(Handle, *mut Handle) -> Status,
    destroy_tokenizer_stream: unsafe extern "C" fn(Handle),
    tokenizer_stream_decode: unsafe extern "C" fn(Handle, i32, *mut *const c_char) -> Status,
    create_generator_params: unsafe extern "C" fn(Handle, *mut Handle) -> Status,
    destroy_generator_params: unsafe extern "C" fn(Handle),
    set_search_number: unsafe extern "C" fn(Handle, *const c_char, f64) -> Status,
    set_search_bool: unsafe extern "C" fn(Handle, *const c_char, bool) -> Status,
    set_guidance:
        Option<unsafe extern "C" fn(Handle, *const c_char, *const c_char, bool) -> Status>,
    create_generator: unsafe extern "C" fn(Handle, Handle, *mut Handle) -> Status,
    destroy_generator: unsafe extern "C" fn(Handle),
    append_token_sequences: unsafe extern "C" fn(Handle, Handle) -> Status,
    is_done: unsafe extern "C" fn(Handle) -> bool,
    generate_next_token: unsafe extern "C" fn(Handle) -> Status,
    get_next_tokens: unsafe extern "C" fn(Handle, *mut *const i32, *mut usize) -> Status,
    _library: libloading::Library,
}

macro_rules! bind {
    ($library:expr, $name:literal) => {{
        // SAFETY: each field's type is the C signature ort_genai_c.h declares for this symbol.
        let symbol = unsafe { $library.get(concat!($name, "\0").as_bytes()) }
            .map_err(|e| format!("{} is missing from the loaded library: {e}", $name))?;
        *symbol
    }};
}

impl Api {
    fn bind(library: libloading::Library) -> Result<Self, String> {
        Ok(Self {
            result_get_error: bind!(library, "OgaResultGetError"),
            destroy_result: bind!(library, "OgaDestroyResult"),
            create_config: bind!(library, "OgaCreateConfig"),
            config_clear_providers: bind!(library, "OgaConfigClearProviders"),
            config_append_provider: bind!(library, "OgaConfigAppendProvider"),
            destroy_config: bind!(library, "OgaDestroyConfig"),
            create_model_from_config: bind!(library, "OgaCreateModelFromConfig"),
            destroy_model: bind!(library, "OgaDestroyModel"),
            create_tokenizer: bind!(library, "OgaCreateTokenizer"),
            destroy_tokenizer: bind!(library, "OgaDestroyTokenizer"),
            tokenizer_get_eos_token_ids: bind!(library, "OgaTokenizerGetEosTokenIds"),
            tokenizer_apply_chat_template: bind!(library, "OgaTokenizerApplyChatTemplate"),
            destroy_string: bind!(library, "OgaDestroyString"),
            create_sequences: bind!(library, "OgaCreateSequences"),
            destroy_sequences: bind!(library, "OgaDestroySequences"),
            tokenizer_encode: bind!(library, "OgaTokenizerEncode"),
            sequences_get_sequence_count: bind!(library, "OgaSequencesGetSequenceCount"),
            create_tokenizer_stream: bind!(library, "OgaCreateTokenizerStream"),
            destroy_tokenizer_stream: bind!(library, "OgaDestroyTokenizerStream"),
            tokenizer_stream_decode: bind!(library, "OgaTokenizerStreamDecode"),
            create_generator_params: bind!(library, "OgaCreateGeneratorParams"),
            destroy_generator_params: bind!(library, "OgaDestroyGeneratorParams"),
            set_search_number: bind!(library, "OgaGeneratorParamsSetSearchNumber"),
            set_search_bool: bind!(library, "OgaGeneratorParamsSetSearchBool"),
            // SAFETY: the type is the C signature ort_genai_c.h declares; an older GenAI lacks the symbol.
            set_guidance: unsafe { library.get(b"OgaGeneratorParamsSetGuidance\0") }
                .ok()
                .map(|symbol| *symbol),
            create_generator: bind!(library, "OgaCreateGenerator"),
            destroy_generator: bind!(library, "OgaDestroyGenerator"),
            append_token_sequences: bind!(library, "OgaGenerator_AppendTokenSequences"),
            is_done: bind!(library, "OgaGenerator_IsDone"),
            generate_next_token: bind!(library, "OgaGenerator_GenerateNextToken"),
            get_next_tokens: bind!(library, "OgaGenerator_GetNextTokens"),
            _library: library,
        })
    }

    /// `Ok` for a null status, else the library's message, the status released.
    fn check(&self, status: Status, what: &str) -> Result<(), String> {
        if status.is_null() {
            return Ok(());
        }
        // SAFETY: a non-null status owns a NUL-terminated message until it is destroyed below.
        let message = unsafe { CStr::from_ptr((self.result_get_error)(status)) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: the status came from this library and is released exactly once.
        unsafe { (self.destroy_result)(status) };
        Err(format!("{what}: {message}"))
    }
}

/// Where the library is looked for: beside the executable, then the chain install; no bare name, which finds any copy.
pub fn library_candidates(exe_dir: Option<&Path>, genai_root: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = exe_dir.map(|d| d.join(LIBRARY)).into_iter().collect();
    if let Some(root) = genai_root {
        candidates.extend(
            ["bin", "lib"]
                .iter()
                .map(|sub| root.join(sub).join(LIBRARY)),
        );
    }
    #[cfg(not(windows))]
    candidates.push(Path::new(IMAGE_CHAIN_DIR).join(LIBRARY));
    candidates
}

pub use kataglyphis_core::provenance::embeds_marker;

/// Refuses a GenAI that was not built in the chain's tree: a pip, NuGet or downloaded copy.
pub fn verify_chain_build(path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if embeds_marker(&bytes, CHAIN_BUILD_MARKER) {
        return Ok(());
    }
    Err(format!(
        "{} is not the family's chain-built GenAI: it does not embed the chain build path '{CHAIN_BUILD_MARKER}'",
        path.display()
    ))
}

fn api() -> Result<&'static Api, TextError> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(|| {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let root = std::env::var_os("ONNX_GENAI_ROOT")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        let candidates = library_candidates(exe_dir.as_deref(), root.as_deref());
        let Some(path) = candidates.iter().find(|c| c.is_file()) else {
            let tried: Vec<String> = candidates.iter().map(|c| c.display().to_string()).collect();
            return Err(format!(
                "no {LIBRARY} in {}; ONNX_GENAI_ROOT names the chain install",
                tried.join(", ")
            ));
        };
        verify_chain_build(path)?;
        // SAFETY: GenAI's initialisers run here and make no assumption about the loading program.
        let library = unsafe { libloading::Library::new(path) }
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Api::bind(library)
    })
    .as_ref()
    .map_err(|e| TextError::LibraryUnavailable(e.clone()))
}

/// A GenAI handle released by its destroy function when it goes out of scope.
struct Owned {
    handle: Handle,
    destroy: unsafe extern "C" fn(Handle),
}

impl Owned {
    fn new(handle: Handle, destroy: unsafe extern "C" fn(Handle)) -> Self {
        Self { handle, destroy }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: the handle came from the matching create call and is released once.
            unsafe { (self.destroy)(self.handle) };
        }
    }
}

fn c_string(text: &str, what: &str) -> Result<CString, TextError> {
    CString::new(text).map_err(|_| TextError::InvalidOptions(format!("{what} contains a NUL byte")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Provider {
    #[default]
    Cpu,
    Cuda,
    DirectMl,
}

impl Provider {
    fn name(self) -> Option<&'static CStr> {
        match self {
            Self::Cpu => None,
            Self::Cuda => Some(c"cuda"),
            Self::DirectMl => Some(c"dml"),
        }
    }
}

/// What generation sets on GenAI's generator parameters; a test records the calls without the library.
pub(crate) trait SearchParams {
    fn set_bool(&mut self, name: &'static CStr, value: bool) -> Result<(), TextError>;
    fn set_number(&mut self, name: &'static CStr, value: f64) -> Result<(), TextError>;
    fn set_guidance(&mut self, kind: &'static CStr, data: &CStr) -> Result<(), TextError>;
}

/// The search options and guidance of one generation: greedy unless the temperature is above 0.
pub(crate) fn configure(
    params: &mut dyn SearchParams,
    prompt_tokens: usize,
    options: &GenerationOptions,
) -> Result<(), TextError> {
    let sample = options.temperature > 0.0;
    params.set_bool(c"do_sample", sample)?;
    params.set_number(
        c"max_length",
        prompt_tokens as f64 + f64::from(options.max_new_tokens),
    )?;
    if sample {
        params.set_number(c"temperature", options.temperature)?;
        params.set_number(c"top_p", options.top_p)?;
        params.set_number(c"top_k", f64::from(options.top_k))?;
    }
    if let Some(seed) = options.seed {
        params.set_number(c"random_seed", f64::from(seed))?;
    }
    if let Some(schema) = &options.json_schema {
        params.set_guidance(c"json_schema", &c_string(schema, "json_schema")?)?;
    }
    Ok(())
}

/// The error of a refused generator: [`TextError::JsonSchemaUnsupported`] when this build has no llguidance.
pub fn generator_error(message: String) -> TextError {
    if message.contains(NO_GUIDANCE) {
        TextError::JsonSchemaUnsupported(message)
    } else {
        TextError::GenerationFailed(message)
    }
}

struct GenaiParams {
    api: &'static Api,
    handle: Handle,
}

impl SearchParams for GenaiParams {
    fn set_bool(&mut self, name: &'static CStr, value: bool) -> Result<(), TextError> {
        let api = self.api;
        // SAFETY: the params handle is live and the name is a static C string.
        api.check(
            unsafe { (api.set_search_bool)(self.handle, name.as_ptr(), value) },
            "search option",
        )
        .map_err(TextError::InvalidOptions)
    }

    fn set_number(&mut self, name: &'static CStr, value: f64) -> Result<(), TextError> {
        let api = self.api;
        // SAFETY: the params handle is live and the name is a static C string.
        api.check(
            unsafe { (api.set_search_number)(self.handle, name.as_ptr(), value) },
            "search option",
        )
        .map_err(TextError::InvalidOptions)
    }

    fn set_guidance(&mut self, kind: &'static CStr, data: &CStr) -> Result<(), TextError> {
        let api = self.api;
        let Some(set_guidance) = api.set_guidance else {
            return Err(TextError::JsonSchemaUnsupported(
                "this GenAI has no OgaGeneratorParamsSetGuidance".into(),
            ));
        };
        // SAFETY: the params handle is live and both strings are NUL-terminated for the call.
        api.check(
            unsafe { set_guidance(self.handle, kind.as_ptr(), data.as_ptr(), false) },
            "guidance",
        )
        .map_err(generator_error)
    }
}

/// One GenAI model directory, loaded once; `generate` takes it exclusively.
pub struct GenaiGenerator {
    api: &'static Api,
    tokenizer: Owned,
    model: Owned,
    eos_tokens: Vec<i32>,
}

// SAFETY: the handles are only touched through &mut self, and GenAI's model and tokenizer are not tied to a thread.
unsafe impl Send for GenaiGenerator {}

impl GenaiGenerator {
    /// Whether the chain GenAI is found, verified and bound here; the first call loads it.
    pub fn library_available() -> bool {
        api().is_ok()
    }

    pub fn load(model_dir: &Path, provider: Provider) -> Result<Self, TextError> {
        let api = api()?;
        let dir = c_string(&model_dir.to_string_lossy(), "the model path")?;
        let mut raw = std::ptr::null_mut();
        // SAFETY: valid C string and out pointer; the result is checked before raw is used.
        api.check(
            unsafe { (api.create_config)(dir.as_ptr(), &mut raw) },
            "config",
        )
        .map_err(TextError::ModelLoadFailed)?;
        let config = Owned::new(raw, api.destroy_config);
        // SAFETY: config is a live handle from create_config.
        api.check(
            unsafe { (api.config_clear_providers)(config.handle) },
            "providers",
        )
        .map_err(TextError::ProviderUnavailable)?;
        if let Some(name) = provider.name() {
            // SAFETY: config is live and name is a static C string.
            api.check(
                unsafe { (api.config_append_provider)(config.handle, name.as_ptr()) },
                "provider",
            )
            .map_err(TextError::ProviderUnavailable)?;
        }
        let mut raw_model = std::ptr::null_mut();
        // SAFETY: config is live; the out pointer is checked through the status.
        api.check(
            unsafe { (api.create_model_from_config)(config.handle, &mut raw_model) },
            "model",
        )
        .map_err(TextError::ModelLoadFailed)?;
        let model = Owned::new(raw_model, api.destroy_model);
        let mut raw_tokenizer = std::ptr::null_mut();
        // SAFETY: model is live.
        api.check(
            unsafe { (api.create_tokenizer)(model.handle, &mut raw_tokenizer) },
            "tokenizer",
        )
        .map_err(TextError::TokenizerFailed)?;
        let tokenizer = Owned::new(raw_tokenizer, api.destroy_tokenizer);
        let (mut ids, mut count) = (std::ptr::null(), 0usize);
        // SAFETY: tokenizer is live; GenAI owns the returned array for the tokenizer's lifetime.
        api.check(
            unsafe { (api.tokenizer_get_eos_token_ids)(tokenizer.handle, &mut ids, &mut count) },
            "end tokens",
        )
        .map_err(TextError::TokenizerFailed)?;
        let eos_tokens = if count == 0 {
            Vec::new()
        } else {
            // SAFETY: ids points to count tokens while the tokenizer lives, which it does here.
            unsafe { std::slice::from_raw_parts(ids, count) }.to_vec()
        };
        Ok(Self {
            api,
            tokenizer,
            model,
            eos_tokens,
        })
    }

    fn prompt(&self, messages: &[ChatMessage]) -> Result<Owned, TextError> {
        let api = self.api;
        // The template's output is read up to its first NUL, so one in a message would cut the prompt short.
        if messages
            .iter()
            .any(|m| m.content.contains('\0') || m.role.contains('\0'))
        {
            return Err(TextError::InvalidOptions(
                "a message contains a NUL byte".into(),
            ));
        }
        let chat = c_string(&messages_json(messages).to_string(), "a message")?;
        let mut templated = std::ptr::null();
        // SAFETY: tokenizer and chat are live; null template and tools select the model's own and none.
        api.check(
            unsafe {
                (api.tokenizer_apply_chat_template)(
                    self.tokenizer.handle,
                    std::ptr::null(),
                    chat.as_ptr(),
                    std::ptr::null(),
                    true,
                    &mut templated,
                )
            },
            "chat template",
        )
        .map_err(TextError::ChatTemplateFailed)?;
        // SAFETY: on success GenAI returns a NUL-terminated string this code must destroy.
        let text = unsafe { CStr::from_ptr(templated) }.to_owned();
        // SAFETY: the string came from apply_chat_template and is destroyed once.
        unsafe { (api.destroy_string)(templated) };
        let mut raw = std::ptr::null_mut();
        // SAFETY: plain out pointer.
        api.check(unsafe { (api.create_sequences)(&mut raw) }, "sequences")
            .map_err(TextError::EncodeFailed)?;
        let sequences = Owned::new(raw, api.destroy_sequences);
        // SAFETY: tokenizer, text and sequences are live.
        api.check(
            unsafe {
                (api.tokenizer_encode)(self.tokenizer.handle, text.as_ptr(), sequences.handle)
            },
            "encode",
        )
        .map_err(TextError::EncodeFailed)?;
        Ok(sequences)
    }

    fn generator(
        &self,
        prompt: &Owned,
        prompt_tokens: usize,
        options: &GenerationOptions,
    ) -> Result<Owned, TextError> {
        let api = self.api;
        let mut raw = std::ptr::null_mut();
        // SAFETY: model is live.
        api.check(
            unsafe { (api.create_generator_params)(self.model.handle, &mut raw) },
            "generator params",
        )
        .map_err(TextError::GenerationFailed)?;
        let params = Owned::new(raw, api.destroy_generator_params);
        configure(
            &mut GenaiParams {
                api,
                handle: params.handle,
            },
            prompt_tokens,
            options,
        )?;
        let mut raw_generator = std::ptr::null_mut();
        // SAFETY: model and params are live.
        api.check(
            unsafe { (api.create_generator)(self.model.handle, params.handle, &mut raw_generator) },
            "generator",
        )
        .map_err(generator_error)?;
        let generator = Owned::new(raw_generator, api.destroy_generator);
        // SAFETY: generator and prompt are live.
        api.check(
            unsafe { (api.append_token_sequences)(generator.handle, prompt.handle) },
            "prompt",
        )
        .map_err(TextError::GenerationFailed)?;
        Ok(generator)
    }

    fn next_token(
        &self,
        generator: &Owned,
        stream: &Owned,
        result: &mut GenerationResult,
    ) -> Result<String, TextError> {
        let api = self.api;
        let (mut next, mut count) = (std::ptr::null(), 0usize);
        // SAFETY: generator is live.
        api.check(
            unsafe { (api.generate_next_token)(generator.handle) },
            "next token",
        )
        .and_then(|()| {
            api.check(
                unsafe { (api.get_next_tokens)(generator.handle, &mut next, &mut count) },
                "next tokens",
            )
        })
        .map_err(TextError::GenerationFailed)?;
        if count == 0 {
            return Err(TextError::GenerationFailed("no token was produced".into()));
        }
        // SAFETY: next points to count >= 1 tokens until the next generator call.
        let token = unsafe { *next };
        result.tokens.push(token);
        let mut piece = std::ptr::null();
        // SAFETY: stream is live; GenAI owns the returned piece until the next decode.
        api.check(
            unsafe { (api.tokenizer_stream_decode)(stream.handle, token, &mut piece) },
            "decode",
        )
        .map_err(TextError::TokenizerFailed)?;
        if piece.is_null() {
            return Ok(String::new());
        }
        // SAFETY: a non-null piece is NUL-terminated.
        Ok(unsafe { CStr::from_ptr(piece) }
            .to_string_lossy()
            .into_owned())
    }
}

impl TextBackend for GenaiGenerator {
    fn generate(
        &mut self,
        messages: &[ChatMessage],
        options: &GenerationOptions,
        cancel: &CancelToken,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<GenerationResult, TextError> {
        check_request(messages, options)?;
        cancel.reset();
        // The clock starts before the prompt goes in, so the first token's time includes the prefill.
        let start = Instant::now();
        let api = self.api;
        let prompt = self.prompt(messages)?;
        // SAFETY: prompt is a live sequences handle holding one sequence.
        let prompt_tokens = unsafe { (api.sequences_get_sequence_count)(prompt.handle, 0) };
        let generator = self.generator(&prompt, prompt_tokens, options)?;
        let mut raw = std::ptr::null_mut();
        // SAFETY: tokenizer is live.
        api.check(
            unsafe { (api.create_tokenizer_stream)(self.tokenizer.handle, &mut raw) },
            "tokenizer stream",
        )
        .map_err(TextError::TokenizerFailed)?;
        let stream = Owned::new(raw, api.destroy_tokenizer_stream);
        let mut result = GenerationResult {
            text: String::new(),
            tokens: Vec::new(),
            finish_reason: FinishReason::Length,
            prompt_tokens,
            completion_tokens: 0,
            time_to_first_token_ms: 0.0,
            decode_tokens_per_second: 0.0,
        };
        let (mut first, mut shown, mut stopped) = (start, 0usize, false);
        // SAFETY: generator is live for the whole loop.
        while !unsafe { (api.is_done)(generator.handle) } {
            if cancel.is_cancelled() {
                result.finish_reason = FinishReason::Cancelled;
                break;
            }
            // A piece that ends inside a UTF-8 character stays in the stream until it is complete.
            let piece = self.next_token(&generator, &stream, &mut result)?;
            if result.tokens.len() == 1 {
                first = Instant::now();
            }
            result.text.push_str(&piece);
            let (stop_at, safe) = stop_scan(&result.text, &options.stop);
            if safe > shown {
                on_text(&result.text[shown..safe]);
                shown = safe;
            }
            if let Some(at) = stop_at {
                result.text.truncate(at);
                stopped = true;
                break;
            }
        }
        if !stopped && shown < result.text.len() {
            on_text(&result.text[shown..]);
        }
        if stopped {
            result.finish_reason = FinishReason::Stop;
        } else if result.finish_reason != FinishReason::Cancelled {
            let stopped = result
                .tokens
                .last()
                .is_some_and(|t| self.eos_tokens.contains(t));
            result.finish_reason = if stopped {
                FinishReason::Stop
            } else {
                FinishReason::Length
            };
        }
        result.completion_tokens = result.tokens.len();
        if !result.tokens.is_empty() {
            result.time_to_first_token_ms = first.duration_since(start).as_secs_f64() * 1000.0;
        }
        result.decode_tokens_per_second =
            decode_rate(result.tokens.len(), first.elapsed().as_secs_f64());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_is_looked_for_beside_the_exe_then_in_the_chain_install() {
        let candidates = library_candidates(Some(Path::new("/app")), Some(Path::new("/genai")));
        assert_eq!(candidates.first(), Some(&Path::new("/app").join(LIBRARY)));
        assert_eq!(candidates[1], Path::new("/genai").join("bin").join(LIBRARY));
        assert_eq!(candidates[2], Path::new("/genai").join("lib").join(LIBRARY));
    }

    #[test]
    fn no_candidate_is_a_bare_name_the_loader_would_search() {
        for candidates in [
            library_candidates(None, None),
            library_candidates(Some(&std::env::temp_dir().join("app")), None),
        ] {
            assert!(
                candidates
                    .iter()
                    .all(|c| c.is_absolute() && c.ends_with(LIBRARY)),
                "{candidates:?}"
            );
        }
    }

    #[test]
    fn the_marker_matches_across_case_and_separators() {
        assert!(embeds_marker(
            b"x C:/TEMP/onnx-genai-src/build/y",
            r"C:\temp\onnx-genai-src\build\"
        ));
        assert!(embeds_marker(
            b"/opt/onnxruntime-genai/build/Linux",
            "/opt/onnxruntime-genai/build/"
        ));
        assert!(!embeds_marker(
            b"/opt/onnxruntime/build/",
            "/opt/onnxruntime-genai/build/"
        ));
    }

    #[test]
    fn a_copy_built_elsewhere_is_refused() {
        let dir = std::env::temp_dir().join(format!("oxidant-genai-verify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let foreign = dir.join(LIBRARY);
        std::fs::write(
            &foreign,
            b"\x7fELF built in /home/runner/work/onnxruntime-genai",
        )
        .unwrap();
        let refused = verify_chain_build(&foreign).unwrap_err();
        assert!(
            refused.contains("not the family's chain-built GenAI"),
            "{refused}"
        );
        std::fs::write(&foreign, format!("\x7fELF {CHAIN_BUILD_MARKER}_deps")).unwrap();
        assert_eq!(verify_chain_build(&foreign), Ok(()));
        assert!(verify_chain_build(&dir.join("absent"))
            .unwrap_err()
            .contains("cannot read"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Records each call as GenAI would receive it.
    #[derive(Default)]
    struct Recorder(Vec<String>);

    impl SearchParams for Recorder {
        fn set_bool(&mut self, name: &'static CStr, value: bool) -> Result<(), TextError> {
            self.0.push(format!("{}={value}", name.to_string_lossy()));
            Ok(())
        }

        fn set_number(&mut self, name: &'static CStr, value: f64) -> Result<(), TextError> {
            self.0.push(format!("{}={value}", name.to_string_lossy()));
            Ok(())
        }

        fn set_guidance(&mut self, kind: &'static CStr, data: &CStr) -> Result<(), TextError> {
            self.0.push(format!(
                "guidance {}={}",
                kind.to_string_lossy(),
                data.to_string_lossy()
            ));
            Ok(())
        }
    }

    fn recorded(options: &GenerationOptions) -> Vec<String> {
        let mut recorder = Recorder::default();
        configure(&mut recorder, 3, options).unwrap();
        recorder.0
    }

    #[test]
    fn temperature_zero_decodes_greedily_without_sampling_options() {
        assert_eq!(
            recorded(&GenerationOptions {
                max_new_tokens: 4,
                ..GenerationOptions::default()
            }),
            ["do_sample=false", "max_length=7"]
        );
    }

    #[test]
    fn a_temperature_samples_with_the_seed_genai_reads() {
        let options = GenerationOptions {
            max_new_tokens: 4,
            temperature: 1.0,
            top_p: 0.5,
            top_k: 0,
            seed: Some(7),
            ..GenerationOptions::default()
        };
        assert_eq!(
            recorded(&options),
            [
                "do_sample=true",
                "max_length=7",
                "temperature=1",
                "top_p=0.5",
                "top_k=0",
                "random_seed=7"
            ]
        );
    }

    #[test]
    fn a_schema_reaches_genai_as_json_schema_guidance() {
        let options = GenerationOptions {
            json_schema: Some(r#"{"type":"object"}"#.into()),
            ..GenerationOptions::default()
        };
        assert_eq!(
            recorded(&options).last().map(String::as_str),
            Some(r#"guidance json_schema={"type":"object"}"#)
        );
    }

    #[test]
    fn a_build_without_llguidance_reports_the_schema_as_unsupported() {
        let refused = generator_error(format!(
            "generator: {NO_GUIDANCE} (guidance_type=\"json_schema\"). Rebuild with use_guidance=true."
        ));
        assert!(
            matches!(refused, TextError::JsonSchemaUnsupported(_)),
            "{refused}"
        );
        assert!(matches!(
            generator_error("generator: out of memory".into()),
            TextError::GenerationFailed(_)
        ));
    }

    #[test]
    fn providers_name_genai_s_identifiers() {
        assert_eq!(Provider::Cpu.name(), None);
        assert_eq!(Provider::Cuda.name(), Some(c"cuda"));
        assert_eq!(Provider::DirectMl.name(), Some(c"dml"));
    }
}
