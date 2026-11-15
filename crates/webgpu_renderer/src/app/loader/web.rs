//! The browser loader: without threads a fetch streams in a task, and a frame shows `parsing` before the blocking parse.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::{parse_bytes, LoadProgress, LoadStage, LoadedScene, SceneLoader, SceneRequest};

/// What the fetch task has delivered so far.
#[derive(Default)]
pub(super) struct Fetch {
    received: u64,
    total: Option<u64>,
    done: Option<Result<Vec<u8>, String>>,
}

pub(super) enum State {
    Fetching(Rc<RefCell<Fetch>>),
    /// Bytes in hand, parsed on the next poll so `parsing` shows for a frame first.
    Parse(Option<Arc<[u8]>>),
    /// Parsed, handed out on the next poll so `uploading` shows for a frame first.
    Ready(Option<Result<LoadedScene, String>>),
}

impl SceneLoader {
    pub(super) fn begin(request: SceneRequest) -> (State, LoadStage) {
        match request {
            SceneRequest::Bytes(bytes) => (State::Parse(Some(bytes)), LoadStage::Parsing),
            SceneRequest::Url(url) => {
                let fetch = Rc::new(RefCell::new(Fetch::default()));
                spawn_fetch(url, Rc::clone(&fetch));
                (State::Fetching(fetch), LoadStage::Fetching)
            }
            SceneRequest::File(path) => {
                let error = format!("{}: a browser tab reads no files", path.display());
                (State::Ready(Some(Err(error))), LoadStage::Parsing)
            }
        }
    }

    pub(super) fn advance(
        state: &mut State,
        progress: &mut LoadProgress,
    ) -> Option<Result<LoadedScene, String>> {
        match state {
            State::Fetching(fetch) => {
                let done = {
                    let mut fetch = fetch.borrow_mut();
                    let received = fetch.received as f32;
                    progress.fraction = fetch
                        .total
                        .filter(|&total| total > 0)
                        .map(|total| received / total as f32);
                    fetch.done.take()
                }?;
                let bytes = match done {
                    Ok(bytes) => bytes,
                    Err(error) => return Some(Err(error)),
                };
                *state = State::Parse(Some(bytes.into()));
                progress.stage = LoadStage::Parsing;
                progress.fraction = None;
                None
            }
            State::Parse(bytes) => {
                let result = parse_bytes(&bytes.take()?);
                if result.is_err() {
                    return Some(result);
                }
                *state = State::Ready(Some(result));
                progress.stage = LoadStage::Uploading;
                None
            }
            State::Ready(result) => result.take(),
        }
    }
}

fn spawn_fetch(url: String, fetch: Rc<RefCell<Fetch>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let result = stream(&url, &fetch)
            .await
            .map_err(|error| format!("{url}: {error}"));
        fetch.borrow_mut().done = Some(result);
    });
}

/// The response body chunk by chunk, counting what arrived against its length.
async fn stream(url: &str, fetch: &RefCell<Fetch>) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or("no window")?;
    let response: web_sys::Response = JsFuture::from(window.fetch_with_str(url))
        .await
        .and_then(JsCast::dyn_into)
        .map_err(js_error)?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let length = response.headers().get("content-length").ok().flatten();
    let total = length.and_then(|length| length.parse::<u64>().ok());
    fetch.borrow_mut().total = total;
    let body = response.body().ok_or("an empty response")?;
    let reader: web_sys::ReadableStreamDefaultReader = body
        .get_reader()
        .dyn_into()
        .map_err(|reader: js_sys::Object| js_error(reader.into()))?;
    let mut bytes = Vec::with_capacity(total.unwrap_or(0) as usize);
    loop {
        let chunk = JsFuture::from(reader.read()).await.map_err(js_error)?;
        let field =
            |name: &str| js_sys::Reflect::get(&chunk, &JsValue::from_str(name)).map_err(js_error);
        if field("done")?.as_bool().unwrap_or(true) {
            break;
        }
        let array = js_sys::Uint8Array::new(&field("value")?);
        let start = bytes.len();
        bytes.resize(start + array.length() as usize, 0);
        array.copy_to(&mut bytes[start..]);
        fetch.borrow_mut().received = bytes.len() as u64;
    }
    Ok(bytes)
}

fn js_error(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}
