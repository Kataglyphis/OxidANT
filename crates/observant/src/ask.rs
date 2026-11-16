//! `/ask` and `/summary`: a language model answers from the visit log, never from the picture; see `docs/observant.md`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, TryLockError, Weak};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use bytes::Bytes;
use chrono::{DateTime, Days, FixedOffset, NaiveDate, Utc};
use http_body_util::BodyExt as _;
use hyper::body::Incoming;
use hyper::header::{self, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use kataglyphis_text::endpoint::EndpointChoice;
use kataglyphis_text::{
    CancelToken, ChatMessage, FinishReason, GenerationOptions, GenerationResult, TextBackend,
    TextError,
};
use serde::Deserialize;

use crate::config::Config;
use crate::status::{Status, Stay};
use crate::web::{full, plain, Body};

/// `POST {"question": "..."}`: the model's answer and the facts it was given.
pub const ASK_PATH: &str = "/ask";
/// `GET` one day's figures; `POST` adds the model's words about them.
pub const SUMMARY_PATH: &str = "/summary";

const BODY_LIMIT: usize = 4096;
/// What a client puts in `Accept` to get `/ask`'s answer as it is decoded, one JSON object per line.
const NDJSON: &str = "application/x-ndjson";
/// Lines waiting for a slow client before the model waits for it.
const STREAM_BUFFER: usize = 64;
const QUESTION_LIMIT: usize = 1000;
/// Days of figures `/ask` hands the model, today included.
const ASK_DAYS: u64 = 7;
const LONGEST_OFFSET_MIN: i32 = 14 * 60;
/// The default of `llm_timeout_s`.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(300);

const SYSTEM: &str = "You answer questions about a camera's visit log. Use only these facts, and say so when they do not answer the question.";
const SUMMARISE: &str = "Summarise this day's visits in at most two sentences.";

type Opener = Box<dyn Fn() -> Result<Box<dyn TextBackend + Send>, TextError> + Send + Sync>;

/// The configured language model: opened at the first question, then kept, one answer at a time.
pub struct Assistant {
    open: Opener,
    backend: Mutex<Option<Box<dyn TextBackend + Send>>>,
    max_new_tokens: u32,
    timeout: Duration,
    /// How long an in-process model may rest before it is unloaded; `None` keeps it (an endpoint, or 0).
    idle_unload: Option<Duration>,
    /// When the last answer ended.
    last_used: Mutex<Instant>,
}

/// Why no answer came; each one reaches the client with its own status.
#[derive(Debug)]
enum Refusal {
    Busy,
    TimedOut,
    /// The client left before the model was asked.
    Cancelled,
    Text(TextError),
}

/// Cancels a running answer when the client leaves or the time runs out, which drops the request's future.
struct CancelOnDrop(CancelToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[cfg(any(target_os = "linux", windows))]
fn local_backend(
    dir: &std::path::Path,
    provider: &str,
    threads: u32,
) -> Result<Box<dyn TextBackend + Send>, TextError> {
    use kataglyphis_text::genai::{GenaiGenerator, Provider};
    let provider: Provider = provider.parse()?;
    let threads = (threads > 0).then_some(threads);
    Ok(Box::new(GenaiGenerator::load_with(dir, provider, threads)?))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn local_backend(
    _: &std::path::Path,
    _: &str,
    _: u32,
) -> Result<Box<dyn TextBackend + Send>, TextError> {
    Err(TextError::LibraryUnavailable(
        "the chain's ONNX Runtime GenAI exists for Linux and Windows only; use an endpoint".into(),
    ))
}

impl Assistant {
    /// `None` when the config names no language model, which leaves `/ask` answering 503.
    pub fn from_config(config: &Config) -> Option<Self> {
        let model_dir: Option<PathBuf> = config.llm_model_dir.clone();
        let choice = EndpointChoice {
            base_url: config.llm_base_url.clone(),
            backend: config.llm_backend.clone(),
            registry: None,
            model: config.llm_model.clone(),
        };
        if !config.names_a_language_model() {
            return None;
        }
        let (provider, threads) = (config.llm_provider.clone(), config.llm_threads);
        let limit = Duration::from_secs(u64::from(config.llm_timeout_s));
        // An endpoint holds no memory here; only a model in this process is worth unloading (owner decision).
        let idle_unload = (model_dir.is_some() && config.llm_idle_unload_s > 0)
            .then(|| Duration::from_secs(u64::from(config.llm_idle_unload_s)));
        let open: Opener = Box::new(move || match &model_dir {
            Some(dir) => local_backend(dir, &provider, threads),
            None => {
                let mut endpoint = choice.resolve()?;
                endpoint.timeout = limit;
                Ok(Box::new(endpoint))
            }
        });
        let mut assistant = Self::with_opener(open, config.llm_max_new_tokens);
        assistant.idle_unload = idle_unload;
        assistant.timeout = limit;
        Some(assistant)
    }

    fn with_opener(open: Opener, max_new_tokens: u32) -> Self {
        Self {
            open,
            backend: Mutex::new(None),
            max_new_tokens,
            timeout: ANSWER_TIMEOUT,
            idle_unload: None,
            last_used: Mutex::new(Instant::now()),
        }
    }

    /// Drops an in-process model that rested longer than `idle_unload`; a running answer holds the lock and keeps it.
    pub(crate) fn unload_if_idle(&self, now: Instant) -> bool {
        let Some(idle) = self.idle_unload else {
            return false;
        };
        let last = *self
            .last_used
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let rested = now.saturating_duration_since(last);
        if rested < idle {
            return false;
        }
        let Ok(mut backend) = self.backend.try_lock() else {
            return false;
        };
        if backend.take().is_none() {
            return false;
        }
        log::info!(
            "unloaded the language model after {} s without a question",
            rested.as_secs()
        );
        true
    }

    /// Runs on a blocking thread: a second question while one is running is refused, not queued.
    fn run(
        &self,
        messages: &[ChatMessage],
        cancel: &CancelToken,
    ) -> Result<GenerationResult, Refusal> {
        self.run_with(messages, cancel, &mut || {}, &mut |_| {})
    }

    /// [`Self::run`] with `on_start` once the model is loaded and about to answer, and `on_text` for each piece.
    fn run_with(
        &self,
        messages: &[ChatMessage],
        cancel: &CancelToken,
        on_start: &mut dyn FnMut(),
        on_text: &mut dyn FnMut(&str),
    ) -> Result<GenerationResult, Refusal> {
        let mut backend = match self.backend.try_lock() {
            Ok(backend) => backend,
            // A panic in an earlier answer must not read as busy for the rest of the service's life.
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return Err(Refusal::Busy),
        };
        let options = GenerationOptions {
            max_new_tokens: self.max_new_tokens,
            ..GenerationOptions::default()
        };
        let slot = &mut *backend;
        let open = &self.open;
        let answered = kataglyphis_text::after_load(
            cancel,
            move || {
                let slot = slot;
                if slot.is_none() {
                    *slot = Some(open()?);
                }
                Ok(slot.as_mut().expect("opened above"))
            },
            |model| {
                on_start();
                model.generate(messages, &options, cancel, on_text)
            },
        );
        *self
            .last_used
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Instant::now();
        // A client that left while the model loaded gets no answer decoded for nobody.
        answered.map_err(Refusal::Text)?.ok_or(Refusal::Cancelled)
    }

    async fn answer(
        self: Arc<Self>,
        messages: Vec<ChatMessage>,
    ) -> Result<GenerationResult, Refusal> {
        let guard = CancelOnDrop(CancelToken::default());
        let cancel = guard.0.clone();
        let timeout = self.timeout;
        let work = tokio::task::spawn_blocking(move || self.run(&messages, &cancel));
        match tokio::time::timeout(timeout, work).await {
            Ok(joined) => joined_result(joined),
            Err(_) => Err(Refusal::TimedOut),
        }
    }

    /// The answer as NDJSON lines: facts, pieces, then `done` or `error`; a refusal before the start keeps its status.
    async fn stream(self: Arc<Self>, messages: Vec<ChatMessage>, facts: String) -> Response<Body> {
        let guard = CancelOnDrop(CancelToken::default());
        let cancel = guard.0.clone();
        let deadline = tokio::time::Instant::now() + self.timeout;
        let (lines, body) = tokio::sync::mpsc::channel::<Bytes>(STREAM_BUFFER);
        // The buffer is empty, so the facts always fit.
        let _ = lines.try_send(ndjson(&serde_json::json!({ "facts": facts })));
        let (started, has_started) = tokio::sync::oneshot::channel::<()>();
        let pieces = lines.clone();
        let mut work = tokio::task::spawn_blocking(move || {
            let mut started = Some(started);
            let piece_cancel = cancel.clone();
            self.run_with(
                &messages,
                &cancel,
                &mut || {
                    if let Some(started) = started.take() {
                        let _ = started.send(());
                    }
                },
                &mut |piece| {
                    // A client that left closed the channel: the rest would be decoded for nobody.
                    if pieces
                        .blocking_send(ndjson(&serde_json::json!({ "piece": piece })))
                        .is_err()
                    {
                        piece_cancel.cancel();
                    }
                },
            )
        });
        tokio::select! {
            joined = &mut work => return match joined_result(joined) {
                Err(refusal) => refused(refusal),
                Ok(_) => plain(StatusCode::BAD_GATEWAY, "the model ended before it started"),
            },
            Ok(()) = has_started => {}
            () = tokio::time::sleep_until(deadline) => return refused(Refusal::TimedOut),
        }
        tokio::spawn(async move {
            let _cancel_when_done = guard;
            let last = tokio::select! {
                joined = tokio::time::timeout_at(deadline, work) => match joined {
                    Ok(joined) => outcome_line(joined_result(joined)),
                    Err(_) => error_line(Refusal::TimedOut),
                },
                () = lines.closed() => return,
            };
            let _ = lines.send(last).await;
        });
        ndjson_reply(body)
    }
}

/// A finished blocking run as the answer or its refusal; a panic in it is a failed generation.
fn joined_result(
    joined: Result<Result<GenerationResult, Refusal>, tokio::task::JoinError>,
) -> Result<GenerationResult, Refusal> {
    let result = joined.unwrap_or_else(|panicked| {
        Err(Refusal::Text(TextError::GenerationFailed(
            panicked.to_string(),
        )))
    });
    if let Ok(done) = &result {
        log::info!(
            "answer: {} tokens, {:.0} ms to the first, {:.1} tokens/s",
            done.completion_tokens,
            done.time_to_first_token_ms,
            done.decode_tokens_per_second
        );
    }
    result
}

fn ndjson(value: &serde_json::Value) -> Bytes {
    Bytes::from(format!("{value}\n"))
}

/// The stream's last line: `done` with the figures, or `error` with the status a whole reply would have had.
fn outcome_line(result: Result<GenerationResult, Refusal>) -> Bytes {
    let result = match result {
        Ok(result) => result,
        Err(refusal) => return error_line(refusal),
    };
    if let Some(text) = no_words_text(&result) {
        return ndjson(&serde_json::json!({ "error": text, "status": 502 }));
    }
    ndjson(&serde_json::json!({ "done": {
        "finish_reason": result.finish_reason.as_str(),
        "prompt_tokens": result.prompt_tokens,
        "completion_tokens": result.completion_tokens,
        "thinking_chars": result.thinking_chars,
        "time_to_first_token_ms": result.time_to_first_token_ms,
        "decode_tokens_per_second": result.decode_tokens_per_second,
    }}))
}

fn error_line(refusal: Refusal) -> Bytes {
    let (status, text) = refusal_parts(refusal);
    ndjson(&serde_json::json!({ "error": text, "status": status.as_u16() }))
}

fn ndjson_reply(lines: tokio::sync::mpsc::Receiver<Bytes>) -> Response<Body> {
    let mut response = Response::new(crate::web::ChannelBody(lines).boxed_unsync());
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(NDJSON));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Asks now and then whether the in-process model rested long enough to unload; ends with the assistant.
pub async fn unload_when_idle(assistant: Weak<Assistant>) {
    let Some(idle) = assistant.upgrade().and_then(|a| a.idle_unload) else {
        return;
    };
    let mut tick =
        tokio::time::interval((idle / 4).clamp(Duration::from_secs(1), Duration::from_secs(60)));
    loop {
        tick.tick().await;
        let Some(assistant) = assistant.upgrade() else {
            return;
        };
        assistant.unload_if_idle(Instant::now());
    }
}

/// One local day of the visit log; a stay still running counts toward the visits only.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DayFacts {
    day: NaiveDate,
    visits: usize,
    first: Option<String>,
    last: Option<String>,
    longest_s: u64,
    total_s: u64,
}

fn local(arrived: u64, offset: FixedOffset) -> Option<DateTime<FixedOffset>> {
    DateTime::<Utc>::from_timestamp(i64::try_from(arrived).ok()?, 0)
        .map(|t| t.with_timezone(&offset))
}

/// `stays` is newest first, as the log keeps it.
pub(crate) fn day_facts(stays: &[Stay], day: NaiveDate, offset: FixedOffset) -> DayFacts {
    let of_day: Vec<(DateTime<FixedOffset>, Option<u64>)> = stays
        .iter()
        .filter_map(|(arrived, stayed)| Some((local(*arrived, offset)?, *stayed)))
        .filter(|(at, _)| at.date_naive() == day)
        .collect();
    let clock = |at: &DateTime<FixedOffset>| at.format("%H:%M").to_string();
    DayFacts {
        day,
        visits: of_day.len(),
        first: of_day.iter().map(|(at, _)| at).min().map(clock),
        last: of_day.iter().map(|(at, _)| at).max().map(clock),
        longest_s: of_day.iter().filter_map(|(_, s)| *s).max().unwrap_or(0),
        total_s: of_day.iter().filter_map(|(_, s)| *s).sum(),
    }
}

impl DayFacts {
    fn line(&self) -> String {
        format!(
            "{}: {} visits, first {}, last {}, longest stay {} s, total stay {} s.",
            self.day,
            self.visits,
            self.first.as_deref().unwrap_or("-"),
            self.last.as_deref().unwrap_or("-"),
            self.longest_s,
            self.total_s
        )
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "day": self.day.to_string(),
            "visits": self.visits,
            "first": self.first,
            "last": self.last,
            "longest_stay_s": self.longest_s,
            "total_stay_s": self.total_s,
        })
    }
}

/// What `/ask` tells the model: bounded by [`ASK_DAYS`] lines, whatever the history holds.
pub(crate) fn facts_text(
    camera: &str,
    watching: &str,
    in_view: bool,
    stays: &[Stay],
    now: DateTime<FixedOffset>,
) -> String {
    let today = now.date_naive();
    let days: Vec<String> = (0..ASK_DAYS)
        .filter_map(|back| today.checked_sub_days(Days::new(back)))
        .map(|day| day_facts(stays, day, *now.offset()))
        .filter(|facts| facts.visits > 0)
        .map(|facts| facts.line())
        .collect();
    let log = if days.is_empty() {
        format!("No visits in the last {ASK_DAYS} days.")
    } else {
        days.join("\n")
    };
    format!(
        "Camera {camera:?} watches for {watching}. Now {}. In view now: {}.\n{log}",
        now.format("%Y-%m-%d %H:%M %:z"),
        if in_view { "yes" } else { "no" },
    )
}

fn offset_of(minutes: i32) -> anyhow::Result<FixedOffset> {
    anyhow::ensure!(
        minutes.abs() <= LONGEST_OFFSET_MIN,
        "tz_offset_min must lie within {LONGEST_OFFSET_MIN} minutes of UTC"
    );
    FixedOffset::east_opt(minutes * 60).context("tz_offset_min is not an offset")
}

fn language_line(language: Option<&str>) -> anyhow::Result<&'static str> {
    match language {
        None => Ok(""),
        Some("en") => Ok("\nAnswer in English."),
        Some("de") => Ok("\nAnswer in German."),
        Some(other) => anyhow::bail!("language must be de or en (got {other:?})"),
    }
}

/// The local date now, or `day` when one is given.
fn chosen_day(day: Option<&str>, now: DateTime<FixedOffset>) -> anyhow::Result<NaiveDate> {
    day.map_or(Ok(now.date_naive()), |text| {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").context("day must be YYYY-MM-DD")
    })
}

fn now_at(offset: FixedOffset) -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&offset)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskBody {
    question: String,
    #[serde(default)]
    tz_offset_min: i32,
    language: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SummaryBody {
    day: Option<String>,
    #[serde(default)]
    tz_offset_min: i32,
    language: Option<String>,
}

fn json_reply(status: StatusCode, value: &serde_json::Value) -> Response<Body> {
    let mut response = Response::new(full(Bytes::from(value.to_string())));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

fn text_reply(status: StatusCode, text: String) -> Response<Body> {
    let mut response = Response::new(full(Bytes::from(text)));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn bad_request(err: &anyhow::Error) -> Response<Body> {
    text_reply(StatusCode::BAD_REQUEST, format!("{err:#}"))
}

fn refused(refusal: Refusal) -> Response<Body> {
    let (status, text) = refusal_parts(refusal);
    text_reply(status, text)
}

/// A refusal's status and words, the same in a whole reply and in a stream's `error` line.
fn refusal_parts(refusal: Refusal) -> (StatusCode, String) {
    match refusal {
        Refusal::Busy => (
            StatusCode::SERVICE_UNAVAILABLE,
            "still answering another question".into(),
        ),
        Refusal::TimedOut => (
            StatusCode::GATEWAY_TIMEOUT,
            "the model took too long".into(),
        ),
        Refusal::Cancelled => (
            StatusCode::SERVICE_UNAVAILABLE,
            "the question was cancelled".into(),
        ),
        Refusal::Text(err) => {
            log::warn!("language model ({}): {err}", err.kind());
            (status_of(&err), err.to_string())
        }
    }
}

/// The caller's mistake is 400, a model that cannot load 503, a server too slow 504, and everything else past it 502.
fn status_of(err: &TextError) -> StatusCode {
    match err {
        TextError::InvalidOptions(_) => StatusCode::BAD_REQUEST,
        TextError::EndpointTimedOut(_) => StatusCode::GATEWAY_TIMEOUT,
        TextError::LibraryUnavailable(_)
        | TextError::ModelLoadFailed(_)
        | TextError::ProviderUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_GATEWAY,
    }
}

/// The refusal for a reply without words: a model that spent its budget thinking answers nothing, and 200 would show it as an answer.
fn no_words(result: &GenerationResult) -> Option<Response<Body>> {
    no_words_text(result).map(|text| text_reply(StatusCode::BAD_GATEWAY, text))
}

fn no_words_text(result: &GenerationResult) -> Option<String> {
    result.text.trim().is_empty().then(|| {
        let reason = result.finish_reason.as_str();
        let mut text = format!("the model gave no answer (finish_reason {reason})");
        if result.thinking_chars > 0 {
            text.push_str(&thinking_hint(result));
        }
        text
    })
}

/// Why a reply that was only thinking has no words, and the ways out; a larger budget helps only a reply the budget cut.
fn thinking_hint(result: &GenerationResult) -> String {
    let (what, way_out) = match result.finish_reason {
        FinishReason::Length => (
            "it spent its budget thinking",
            "set a larger llm_max_new_tokens or switch thinking off at the endpoint",
        ),
        FinishReason::Stop | FinishReason::Cancelled => {
            ("it only thought", "switch thinking off at the endpoint")
        }
    };
    format!(
        ": {what} ({} characters of thinking, {} tokens); {way_out}",
        result.thinking_chars, result.completion_tokens
    )
}

/// A POST body as `T`: JSON only, for the reason `/settings` gives, and small.
async fn read_body<T: for<'a> Deserialize<'a>>(
    req: Request<Incoming>,
) -> Result<T, Box<Response<Body>>> {
    let json = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if !json {
        return Err(Box::new(plain(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "application/json only",
        )));
    }
    let body = crate::web::read_limited(req.into_body(), BODY_LIMIT).await?;
    serde_json::from_slice(&body).map_err(|err| Box::new(bad_request(&anyhow::Error::from(err))))
}

fn check_question(question: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!question.trim().is_empty(), "the question is empty");
    anyhow::ensure!(
        question.chars().count() <= QUESTION_LIMIT,
        "the question is longer than {QUESTION_LIMIT} characters"
    );
    Ok(())
}

fn no_model() -> Response<Body> {
    plain(
        StatusCode::SERVICE_UNAVAILABLE,
        "no language model is configured: set llm_model_dir, llm_base_url or llm_backend",
    )
}

/// `POST /ask`.
pub(crate) async fn ask(
    req: Request<Incoming>,
    assistant: Option<Arc<Assistant>>,
    status: Arc<Status>,
) -> Response<Body> {
    if req.method() != Method::POST {
        return plain(StatusCode::METHOD_NOT_ALLOWED, "POST only");
    }
    let streamed = req
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains(NDJSON));
    let body: AskBody = match read_body(req).await {
        Ok(body) => body,
        Err(response) => return *response,
    };
    let Some(assistant) = assistant else {
        return no_model();
    };
    let prompt = check_question(&body.question)
        .and_then(|()| offset_of(body.tz_offset_min))
        .and_then(|offset| {
            let language = language_line(body.language.as_deref())?;
            let facts = facts_text(
                &status.name(),
                &status.watching_text(),
                status.anything_seen(),
                &status.stays(),
                now_at(offset),
            );
            Ok((facts, language))
        });
    let (facts, language) = match prompt {
        Ok(prompt) => prompt,
        Err(err) => return bad_request(&err),
    };
    let messages = vec![
        ChatMessage::new("system", format!("{SYSTEM}\n{facts}{language}")),
        ChatMessage::user(body.question),
    ];
    if streamed {
        return assistant.stream(messages, facts).await;
    }
    match assistant.answer(messages).await {
        Ok(result) => no_words(&result).unwrap_or_else(|| {
            json_reply(
                StatusCode::OK,
                &serde_json::json!({
                    "answer": result.text,
                    "facts": facts,
                    "finish_reason": result.finish_reason.as_str(),
                    "prompt_tokens": result.prompt_tokens,
                    "completion_tokens": result.completion_tokens,
                    "thinking_chars": result.thinking_chars,
                    "time_to_first_token_ms": result.time_to_first_token_ms,
                    "decode_tokens_per_second": result.decode_tokens_per_second,
                }),
            )
        }),
        Err(refusal) => refused(refusal),
    }
}

fn query_value<'a>(query: Option<&'a str>, key: &str) -> Option<&'a str> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

/// `GET /summary?day=YYYY-MM-DD&tz=<minutes east of UTC>`: the figures alone; no model runs.
fn summary_figures(query: Option<&str>, status: &Status) -> anyhow::Result<serde_json::Value> {
    let minutes = query_value(query, "tz")
        .map(str::parse)
        .transpose()
        .context("tz must be whole minutes")?
        .unwrap_or(0);
    let offset = offset_of(minutes)?;
    let day = chosen_day(query_value(query, "day"), now_at(offset))?;
    Ok(day_facts(&status.stays(), day, offset).json())
}

/// `POST /summary`: the figures and the model's words; a day without visits asks no model and has none.
async fn summary_words(
    req: Request<Incoming>,
    assistant: Option<Arc<Assistant>>,
    status: Arc<Status>,
) -> Response<Body> {
    let body: SummaryBody = match read_body(req).await {
        Ok(body) => body,
        Err(response) => return *response,
    };
    let Some(assistant) = assistant else {
        return no_model();
    };
    let chosen = offset_of(body.tz_offset_min).and_then(|offset| {
        let language = language_line(body.language.as_deref())?;
        let day = chosen_day(body.day.as_deref(), now_at(offset))?;
        Ok((day_facts(&status.stays(), day, offset), language))
    });
    let (facts, language) = match chosen {
        Ok(chosen) => chosen,
        Err(err) => return bad_request(&err),
    };
    let mut reply = facts.json();
    if facts.visits == 0 {
        reply["summary"] = serde_json::Value::Null;
        return json_reply(StatusCode::OK, &reply);
    }
    let messages = vec![
        ChatMessage::new(
            "system",
            format!(
                "{SYSTEM}\nCamera {:?} watches for {}.\n{}{language}",
                status.name(),
                status.watching_text(),
                facts.line()
            ),
        ),
        ChatMessage::user(SUMMARISE),
    ];
    match assistant.answer(messages).await {
        Ok(result) => {
            if let Some(refusal) = no_words(&result) {
                return refusal;
            }
            reply["summary"] = result.text.into();
            reply["finish_reason"] = result.finish_reason.as_str().into();
            reply["time_to_first_token_ms"] = result.time_to_first_token_ms.into();
            reply["decode_tokens_per_second"] = result.decode_tokens_per_second.into();
            json_reply(StatusCode::OK, &reply)
        }
        Err(refusal) => refused(refusal),
    }
}

/// `/summary`, either way.
pub(crate) async fn summary(
    req: Request<Incoming>,
    assistant: Option<Arc<Assistant>>,
    status: Arc<Status>,
) -> Response<Body> {
    if req.method() == Method::POST {
        return summary_words(req, assistant, status).await;
    }
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain(StatusCode::METHOD_NOT_ALLOWED, "GET or POST only");
    }
    match summary_figures(req.uri().query(), &status) {
        Ok(figures) => json_reply(StatusCode::OK, &figures),
        Err(err) => bad_request(&err),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Answers without a model; `hold` keeps it busy until a cancel, as a slow model would.
    pub(crate) struct Stub {
        pub(crate) seen: Arc<Mutex<Vec<ChatMessage>>>,
        pub(crate) reply: Result<String, TextError>,
        pub(crate) hold: bool,
        /// What the reply reports as thinking, as a thinking model's backend would.
        pub(crate) thinking_chars: usize,
        /// Set once a held run ended because its cancel came.
        pub(crate) saw_cancel: Arc<std::sync::atomic::AtomicBool>,
    }

    impl TextBackend for Stub {
        fn generate(
            &mut self,
            messages: &[ChatMessage],
            _: &GenerationOptions,
            cancel: &CancelToken,
            on_text: &mut dyn FnMut(&str),
        ) -> Result<GenerationResult, TextError> {
            *self.seen.lock().unwrap() = messages.to_vec();
            if let Ok(reply) = &self.reply {
                reply.split_inclusive(' ').for_each(&mut *on_text);
            }
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while self.hold && !cancel.is_cancelled() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            let finish_reason = if cancel.is_cancelled() {
                self.saw_cancel.store(true, Ordering::SeqCst);
                FinishReason::Cancelled
            } else {
                FinishReason::Stop
            };
            Ok(GenerationResult {
                text: self.reply.clone()?,
                tokens: Vec::new(),
                finish_reason,
                prompt_tokens: 11,
                completion_tokens: 2,
                time_to_first_token_ms: 0.0,
                decode_tokens_per_second: 0.0,
                thinking_chars: self.thinking_chars,
            })
        }
    }

    pub(crate) fn stub_assistant(
        reply: Result<String, TextError>,
        hold: bool,
    ) -> (Arc<Assistant>, Arc<Mutex<Vec<ChatMessage>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (shared, reply) = (seen.clone(), Mutex::new(Some(reply)));
        let open: Opener = Box::new(move || {
            let reply = reply.lock().unwrap().take().expect("opened once");
            Ok(Box::new(Stub {
                seen: shared.clone(),
                reply,
                hold,
                thinking_chars: 0,
                saw_cancel: Arc::default(),
            }))
        });
        (Arc::new(Assistant::with_opener(open, 16)), seen)
    }

    /// An assistant whose every open is counted, so a test sees when the model loads again.
    fn counting_assistant(idle: Option<Duration>) -> (Assistant, Arc<AtomicUsize>) {
        let opens = Arc::new(AtomicUsize::new(0));
        let counted = opens.clone();
        let open: Opener = Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Stub {
                seen: Arc::default(),
                reply: Ok("Twice.".into()),
                hold: false,
                thinking_chars: 0,
                saw_cancel: Arc::default(),
            }))
        });
        let mut assistant = Assistant::with_opener(open, 16);
        assistant.idle_unload = idle;
        (assistant, opens)
    }

    #[test]
    fn only_an_in_process_model_unloads_and_zero_keeps_it() {
        let local = Config {
            llm_model_dir: Some("/m".into()),
            ..Config::default()
        };
        let idle = |config: &Config| Assistant::from_config(config).unwrap().idle_unload;
        assert_eq!(idle(&local), Some(Duration::from_secs(600)));
        let endpoint = Config {
            llm_base_url: Some("http://127.0.0.1:1".into()),
            ..Config::default()
        };
        assert_eq!(idle(&endpoint), None);
        let kept = Config {
            llm_idle_unload_s: 0,
            ..local
        };
        assert_eq!(idle(&kept), None);
    }

    #[test]
    fn an_idle_model_is_unloaded_and_the_next_question_loads_it_again() {
        let (assistant, opens) = counting_assistant(Some(Duration::from_secs(600)));
        let ask = || {
            assistant
                .run(&[ChatMessage::user("hi")], &CancelToken::default())
                .unwrap()
        };
        ask();
        let now = Instant::now();
        assert!(!assistant.unload_if_idle(now), "it just answered");
        assert!(assistant.unload_if_idle(now + Duration::from_secs(601)));
        assert!(
            !assistant.unload_if_idle(now + Duration::from_secs(602)),
            "nothing left to unload"
        );
        ask();
        assert_eq!(opens.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_running_answer_or_an_endpoint_keeps_the_model() {
        let (assistant, _) = counting_assistant(Some(Duration::from_secs(1)));
        assistant
            .run(&[ChatMessage::user("hi")], &CancelToken::default())
            .unwrap();
        let later = Instant::now() + Duration::from_secs(5);
        let running = assistant.backend.lock().unwrap();
        assert!(
            !assistant.unload_if_idle(later),
            "an answer holds the model"
        );
        drop(running);
        assert!(assistant.unload_if_idle(later));
        let (endpoint, _) = counting_assistant(None);
        endpoint
            .run(&[ChatMessage::user("hi")], &CancelToken::default())
            .unwrap();
        assert!(!endpoint.unload_if_idle(later + Duration::from_secs(3600)));
    }

    /// An assistant whose model holds its answer until a cancel comes, and the flag it sets then.
    pub(crate) fn held_assistant() -> (Arc<Assistant>, Arc<std::sync::atomic::AtomicBool>) {
        let saw_cancel: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let flag = saw_cancel.clone();
        let open: Opener = Box::new(move || {
            Ok(Box::new(Stub {
                seen: Arc::default(),
                reply: Ok("never".into()),
                hold: true,
                thinking_chars: 0,
                saw_cancel: flag.clone(),
            }))
        });
        (Arc::new(Assistant::with_opener(open, 16)), saw_cancel)
    }

    /// An assistant whose model thinks for `thinking_chars` characters and then answers `reply`.
    pub(crate) fn thinking_assistant(reply: &str, thinking_chars: usize) -> Arc<Assistant> {
        let reply = reply.to_string();
        let open: Opener = Box::new(move || {
            Ok(Box::new(Stub {
                seen: Arc::default(),
                reply: Ok(reply.clone()),
                hold: false,
                thinking_chars,
                saw_cancel: Arc::default(),
            }))
        });
        Arc::new(Assistant::with_opener(open, 16))
    }

    fn utc_plus_one() -> FixedOffset {
        FixedOffset::east_opt(3600).unwrap()
    }

    /// 2026-11-04 00:30 UTC, which is 01:30 in UTC+1 and still 2026-11-03 at UTC-1.
    const NIGHT: u64 = 1793752200;

    #[test]
    fn a_day_counts_the_visits_of_its_local_date_only() {
        let stays = [
            (NIGHT + 3600, None),
            (NIGHT, Some(60)),
            (NIGHT - 2 * 3600, Some(40)),
        ];
        let day = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let facts = day_facts(&stays, day, utc_plus_one());
        assert_eq!(
            facts,
            DayFacts {
                day,
                visits: 2,
                first: Some("01:30".into()),
                last: Some("02:30".into()),
                longest_s: 60,
                total_s: 60,
            }
        );
        let west = day_facts(&stays, day, FixedOffset::west_opt(3600).unwrap());
        assert_eq!(west.visits, 1, "{west:?}");
    }

    #[test]
    fn an_empty_day_has_no_clock_and_no_stay() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let facts = day_facts(&[(NIGHT, Some(5))], day, utc_plus_one());
        assert_eq!((facts.visits, facts.first, facts.total_s), (0, None, 0));
        assert_eq!(facts.day.to_string(), "2026-01-01",);
    }

    #[test]
    fn the_facts_name_the_camera_today_and_only_days_with_visits() {
        let now = local(NIGHT, utc_plus_one()).unwrap();
        let older = NIGHT - 2 * 86_400;
        let past_the_window = NIGHT - (ASK_DAYS + 1) * 86_400;
        let text = facts_text(
            "Garden",
            "cat",
            true,
            &[
                (NIGHT, Some(60)),
                (older, Some(5)),
                (past_the_window, Some(9)),
            ],
            now,
        );
        assert!(text.contains("\"Garden\" watches for cat"), "{text}");
        assert!(text.contains("Now 2026-11-04 01:30 +01:00"), "{text}");
        assert!(text.contains("In view now: yes"), "{text}");
        assert!(
            text.contains("2026-11-04: 1 visits, first 01:30, last 01:30, longest stay 60 s, total stay 60 s."),
            "{text}"
        );
        assert!(text.contains("2026-11-02: 1 visits"), "{text}");
        assert!(!text.contains("total stay 9 s"), "{text}");
        assert_eq!(text.lines().count(), 3, "{text}");
        let none = facts_text("Garden", "cat", false, &[], now);
        assert!(none.contains("No visits in the last 7 days."), "{none}");
        assert!(none.contains("In view now: no"), "{none}");
    }

    #[test]
    fn a_full_history_keeps_the_facts_bounded() {
        let now = local(NIGHT, utc_plus_one()).unwrap();
        let stays: Vec<Stay> = (0..10_000).map(|i| (NIGHT - i * 300, Some(5))).collect();
        let text = facts_text("Garden", "cat", false, &stays, now);
        assert_eq!(text.lines().count(), 1 + ASK_DAYS as usize, "{text}");
        assert!(text.len() < 1000, "{}", text.len());
    }

    #[test]
    fn an_offset_a_language_a_day_and_a_question_are_checked() {
        assert!(offset_of(840).is_ok());
        assert!(offset_of(-840).is_ok());
        assert!(offset_of(841).is_err());
        assert_eq!(language_line(None).unwrap(), "");
        assert!(language_line(Some("de")).unwrap().contains("German"));
        assert!(language_line(Some("en")).unwrap().contains("English"));
        assert!(language_line(Some("fr")).is_err());
        let now = local(NIGHT, utc_plus_one()).unwrap();
        assert_eq!(chosen_day(None, now).unwrap().to_string(), "2026-11-04");
        assert_eq!(
            chosen_day(Some("2026-01-02"), now).unwrap().to_string(),
            "2026-01-02"
        );
        assert!(chosen_day(Some("yesterday"), now).is_err());
        assert!(check_question("how many?").is_ok());
        assert!(check_question(" \n").is_err());
        assert!(check_question(&"x".repeat(QUESTION_LIMIT)).is_ok());
        assert!(check_question(&"x".repeat(QUESTION_LIMIT + 1)).is_err());
    }

    #[tokio::test]
    async fn a_failed_open_is_tried_again_at_the_next_question() {
        let tries = Arc::new(AtomicUsize::new(0));
        let counted = tries.clone();
        let open: Opener = Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Err(TextError::ModelLoadFailed("not yet".into()))
        });
        let assistant = Arc::new(Assistant::with_opener(open, 1));
        for _ in 0..2 {
            let refused = assistant.clone().answer(vec![ChatMessage::user("a")]).await;
            assert!(
                matches!(refused, Err(Refusal::Text(TextError::ModelLoadFailed(_)))),
                "{refused:?}"
            );
        }
        assert_eq!(tries.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_second_question_is_refused_while_the_first_runs_and_the_time_limit_cancels() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let shared = seen.clone();
        let open: Opener = Box::new(move || {
            Ok(Box::new(Stub {
                seen: shared.clone(),
                reply: Ok("late".into()),
                hold: true,
                thinking_chars: 0,
                saw_cancel: Arc::default(),
            }))
        });
        let mut slow = Assistant::with_opener(open, 1);
        slow.timeout = Duration::from_millis(300);
        let slow = Arc::new(slow);
        let first = tokio::spawn(slow.clone().answer(vec![ChatMessage::user("one")]));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while seen.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let second = slow.clone().answer(vec![ChatMessage::user("two")]).await;
        assert!(matches!(second, Err(Refusal::Busy)), "{second:?}");
        assert!(matches!(first.await.unwrap(), Err(Refusal::TimedOut)));
        // The cancel frees the model on its next look, so a later question runs.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while slow.backend.try_lock().is_err() && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert!(slow.backend.try_lock().is_ok());
    }

    #[test]
    fn a_client_that_left_while_the_model_opened_is_not_answered() {
        let (assistant, seen) = stub_assistant(Ok("an answer".into()), false);
        let cancel = CancelToken::default();
        cancel.cancel();
        let refused = assistant.run(&[ChatMessage::user("hi")], &cancel);
        assert!(matches!(refused, Err(Refusal::Cancelled)), "{refused:?}");
        // The backend was opened and kept, but never asked.
        assert!(seen.lock().unwrap().is_empty());
        assert!(assistant.backend.lock().unwrap().is_some());
        let answered = assistant.run(&[ChatMessage::user("hi")], &CancelToken::default());
        assert_eq!(answered.unwrap().text, "an answer");
    }

    #[test]
    fn a_reply_without_words_is_a_refusal_that_names_why_the_model_stopped() {
        let mut result = GenerationResult {
            text: " \n".into(),
            tokens: Vec::new(),
            finish_reason: FinishReason::Length,
            prompt_tokens: 3,
            completion_tokens: 128,
            time_to_first_token_ms: 0.0,
            decode_tokens_per_second: 0.0,
            thinking_chars: 0,
        };
        let refusal = no_words(&result).expect("a blank reply is no answer");
        assert_eq!(refusal.status(), StatusCode::BAD_GATEWAY);
        result.text = "Twice.".into();
        assert!(no_words(&result).is_none());
        // Thinking beside an answer is no refusal.
        result.thinking_chars = 412;
        assert!(no_words(&result).is_none());
    }

    #[test]
    fn the_thinking_hint_names_the_count_the_tokens_and_both_ways_out() {
        let mut result = GenerationResult {
            text: String::new(),
            tokens: Vec::new(),
            finish_reason: FinishReason::Length,
            prompt_tokens: 3,
            completion_tokens: 128,
            time_to_first_token_ms: 0.0,
            decode_tokens_per_second: 0.0,
            thinking_chars: 412,
        };
        let hint = thinking_hint(&result);
        for part in [
            "spent its budget thinking",
            "412 characters of thinking, 128 tokens",
            "llm_max_new_tokens",
            "thinking off at the endpoint",
        ] {
            assert!(hint.contains(part), "{part}: {hint}");
        }
        // A model that ended by itself had budget left: a larger one is no way out.
        result.finish_reason = FinishReason::Stop;
        let hint = thinking_hint(&result);
        assert!(
            hint.contains("it only thought (412 characters of thinking, 128 tokens)"),
            "{hint}"
        );
        assert!(hint.contains("thinking off at the endpoint"), "{hint}");
        assert!(!hint.contains("llm_max_new_tokens"), "{hint}");
    }

    #[test]
    fn a_config_without_a_language_model_has_no_assistant() {
        assert!(Assistant::from_config(&Config::default()).is_none());
        for config in [
            Config {
                llm_base_url: Some("http://127.0.0.1:1".into()),
                ..Config::default()
            },
            Config {
                llm_backend: Some("lab".into()),
                ..Config::default()
            },
            Config {
                llm_model_dir: Some("/m".into()),
                ..Config::default()
            },
        ] {
            assert!(Assistant::from_config(&config).is_some());
        }
    }
}
