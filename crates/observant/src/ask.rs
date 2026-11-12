//! `/ask` and `/summary`: a language model answers from the visit log, never from the picture; see `docs/observant.md`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, TryLockError};
use std::time::Duration;

use anyhow::Context as _;
use bytes::Bytes;
use chrono::{DateTime, Days, FixedOffset, NaiveDate, Utc};
use http_body_util::Full;
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
use crate::web::{plain, Body};

/// `POST {"question": "..."}`: the model's answer and the facts it was given.
pub const ASK_PATH: &str = "/ask";
/// `GET` one day's figures; `POST` adds the model's words about them.
pub const SUMMARY_PATH: &str = "/summary";

const BODY_LIMIT: usize = 4096;
const QUESTION_LIMIT: usize = 1000;
/// Days of figures `/ask` hands the model, today included.
const ASK_DAYS: u64 = 7;
const LONGEST_OFFSET_MIN: i32 = 14 * 60;
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
) -> Result<Box<dyn TextBackend + Send>, TextError> {
    use kataglyphis_text::genai::{GenaiGenerator, Provider};
    let provider = match provider {
        "cuda" => Provider::Cuda,
        "dml" => Provider::DirectMl,
        _ => Provider::Cpu,
    };
    Ok(Box::new(GenaiGenerator::load(dir, provider)?))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn local_backend(_: &std::path::Path, _: &str) -> Result<Box<dyn TextBackend + Send>, TextError> {
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
        let provider = config.llm_provider.clone();
        let open: Opener = Box::new(move || match &model_dir {
            Some(dir) => local_backend(dir, &provider),
            None => Ok(Box::new(choice.resolve()?)),
        });
        Some(Self::with_opener(open, config.llm_max_new_tokens))
    }

    fn with_opener(open: Opener, max_new_tokens: u32) -> Self {
        Self {
            open,
            backend: Mutex::new(None),
            max_new_tokens,
            timeout: ANSWER_TIMEOUT,
        }
    }

    /// Runs on a blocking thread: a second question while one is running is refused, not queued.
    fn run(
        &self,
        messages: &[ChatMessage],
        cancel: &CancelToken,
    ) -> Result<GenerationResult, Refusal> {
        let mut backend = match self.backend.try_lock() {
            Ok(backend) => backend,
            // A panic in an earlier answer must not read as busy for the rest of the service's life.
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return Err(Refusal::Busy),
        };
        if backend.is_none() {
            *backend = Some((self.open)().map_err(Refusal::Text)?);
        }
        // A backend clears the token as it starts, so a client that left while the model loaded would get a whole answer decoded for nobody.
        if cancel.is_cancelled() {
            return Err(Refusal::Cancelled);
        }
        let options = GenerationOptions {
            max_new_tokens: self.max_new_tokens,
            ..GenerationOptions::default()
        };
        backend
            .as_mut()
            .expect("opened above")
            .generate(messages, &options, cancel, &mut |_| {})
            .map_err(Refusal::Text)
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
            Ok(Ok(result)) => result,
            Ok(Err(joined)) => Err(Refusal::Text(TextError::GenerationFailed(
                joined.to_string(),
            ))),
            Err(_) => Err(Refusal::TimedOut),
        }
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
    let mut response = Response::new(Full::new(Bytes::from(value.to_string())));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

fn text_reply(status: StatusCode, text: String) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from(text)));
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
    match refusal {
        Refusal::Busy => plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "still answering another question",
        ),
        Refusal::TimedOut => plain(StatusCode::GATEWAY_TIMEOUT, "the model took too long"),
        Refusal::Cancelled => plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "the question was cancelled",
        ),
        Refusal::Text(err) => {
            log::warn!("language model: {err}");
            text_reply(StatusCode::BAD_GATEWAY, err.to_string())
        }
    }
}

/// The refusal for a reply without words: a model that spent its budget thinking answers nothing, and 200 would show it as an answer.
fn no_words(result: &GenerationResult) -> Option<Response<Body>> {
    result.text.trim().is_empty().then(|| {
        let reason = finish_name(result.finish_reason);
        let mut text = format!("the model gave no answer (finish_reason {reason})");
        if result.thinking_chars > 0 {
            text.push_str(&thinking_hint(result));
        }
        text_reply(StatusCode::BAD_GATEWAY, text)
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

fn finish_name(reason: FinishReason) -> &'static str {
    match reason {
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::Cancelled => "cancelled",
    }
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
    match assistant.answer(messages).await {
        Ok(result) => no_words(&result).unwrap_or_else(|| {
            json_reply(
                StatusCode::OK,
                &serde_json::json!({
                    "answer": result.text,
                    "facts": facts,
                    "finish_reason": finish_name(result.finish_reason),
                    "prompt_tokens": result.prompt_tokens,
                    "completion_tokens": result.completion_tokens,
                    "thinking_chars": result.thinking_chars,
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
            reply["finish_reason"] = finish_name(result.finish_reason).into();
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
    }

    impl TextBackend for Stub {
        fn generate(
            &mut self,
            messages: &[ChatMessage],
            _: &GenerationOptions,
            cancel: &CancelToken,
            _: &mut dyn FnMut(&str),
        ) -> Result<GenerationResult, TextError> {
            *self.seen.lock().unwrap() = messages.to_vec();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while self.hold && !cancel.is_cancelled() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            let finish_reason = if cancel.is_cancelled() {
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
            }))
        });
        (Arc::new(Assistant::with_opener(open, 16)), seen)
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
