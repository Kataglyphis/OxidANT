//! The text area on the hub's tiny GenAI model, as the engine drives it: pulls, a cancel between them, the fixture's
//! cases with the C ABI's statuses, and GenAI's own words kept apart from OxidANT's refusals.
#![cfg(any(target_os = "linux", target_os = "windows"))]

#[path = "../../text/tests/common/mod.rs"]
mod common;

use common::{genai_required, tiny_fixture, tiny_model_dir};
use kataglyphis_accel_bridge::diagnostics::{ffi::LogLevel, take_log_records};
use kataglyphis_accel_bridge::text::{ffi, genai_available, new_text_generator, TextGenerator};

/// The library first, so a host without GenAI skips before it needs Python for the model.
fn generator() -> Option<Box<TextGenerator>> {
    if !genai_available() && !genai_required() {
        eprintln!("SKIP: no chain GenAI loads here; CI sets KATAGLYPHIS_REQUIRE_GENAI=1");
        return None;
    }
    let mut generator = new_text_generator();
    let dir = tiny_model_dir().to_string_lossy().into_owned();
    let loaded = generator.initialize(dir.as_bytes(), ffi::Provider::Cpu);
    assert_eq!(loaded.status, ffi::Status::Ok, "{loaded:?}");
    Some(generator)
}

fn budget(tokens: usize) -> ffi::Options<'static> {
    ffi::Options {
        max_new_tokens: i32::try_from(tokens).unwrap(),
        temperature: 0.0,
        top_p: 1.0,
        top_k: 50,
        has_seed: false,
        seed: -1,
        json_schema: b"",
        stop: &[],
    }
}

/// Pulls until the generation ends: every piece's text and the last piece.
fn drain(generator: &mut TextGenerator) -> (Vec<String>, ffi::Piece) {
    let mut texts = Vec::new();
    loop {
        let piece = generator.next_piece();
        if piece.finished {
            return (texts, piece);
        }
        assert!(!piece.text.is_empty(), "a piece before the end holds text");
        texts.push(piece.text);
    }
}

#[test]
fn pulls_give_the_oracles_continuation() {
    let Some(mut generator) = generator() else {
        return;
    };
    let (prompt, continuation, ids) = tiny_fixture();
    let user = [ffi::Message {
        role: b"user",
        content: prompt.as_bytes(),
    }];
    let started = generator.start(&user, &budget(ids.len()));
    assert_eq!(started.status, ffi::Status::Ok, "{started:?}");
    let (texts, last) = drain(&mut generator);
    assert_eq!(texts.concat() + &last.text, continuation);
    assert_eq!(last.answer, continuation);
    assert_eq!(last.failure.status, ffi::Status::Ok);
    assert_eq!(last.tokens, ids);
    assert_eq!(last.stats.finish, ffi::Finish::Length);
    assert_eq!(last.stats.prompt_tokens, prompt.len());
    assert_eq!(last.stats.completion_tokens, ids.len());
    let after = generator.next_piece();
    assert_eq!(
        after.failure.status,
        ffi::Status::GenerationFailed,
        "a pull past the end"
    );
}

/// The engine's lock step: a cancel the caller sends after a piece ends the generation before its next token.
#[test]
fn a_cancel_between_pulls_ends_after_the_current_token() {
    let Some(mut generator) = generator() else {
        return;
    };
    let (prompt, _, ids) = tiny_fixture();
    let user = [ffi::Message {
        role: b"user",
        content: prompt.as_bytes(),
    }];
    assert_eq!(
        generator.start(&user, &budget(ids.len())).status,
        ffi::Status::Ok
    );
    let first = generator.next_piece();
    assert!(!first.finished && !first.text.is_empty(), "{first:?}");
    generator.cancel_handle().cancel();
    let last = generator.next_piece();
    assert!(last.finished);
    assert_eq!(last.stats.finish, ffi::Finish::Cancelled);
    assert_eq!(last.tokens.len(), 1);
    assert_eq!(generator.start(&user, &budget(2)).status, ffi::Status::Ok);
    let (_, again) = drain(&mut generator);
    assert_eq!(
        again.stats.finish,
        ffi::Finish::Length,
        "a new start clears the cancel"
    );
}

/// The fixture's `stop` as bridge stops, which borrow the strings.
fn stops(texts: &[String]) -> Vec<ffi::Stop<'_>> {
    texts
        .iter()
        .map(|text| ffi::Stop {
            text: text.as_bytes(),
        })
        .collect()
}

fn expected_status(kind: Option<&str>) -> ffi::Status {
    match kind {
        None => ffi::Status::Ok,
        Some("invalid_options") => ffi::Status::InvalidOptions,
        Some("generation_failed") => ffi::Status::GenerationFailed,
        Some("context_length_exceeded") => ffi::Status::ContextLengthExceeded,
        Some(other) => panic!("an error kind this binding does not map: {other}"),
    }
}

/// One fixture case through the bridge: the refused start, or the pieces' text and the last piece.
fn outcome(
    generator: &mut TextGenerator,
    case: &serde_json::Value,
) -> Result<(String, ffi::Piece), ffi::Failure> {
    let pairs: Vec<(String, String)> = case["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["role"].as_str().unwrap().into(),
                m["content"].as_str().unwrap().into(),
            )
        })
        .collect();
    let messages: Vec<ffi::Message<'_>> = pairs
        .iter()
        .map(|(role, content)| ffi::Message {
            role: role.as_bytes(),
            content: content.as_bytes(),
        })
        .collect();
    let options = &case["options"];
    let stop_texts: Vec<String> = options["stop"].as_array().map_or_else(Vec::new, |s| {
        s.iter().map(|v| v.as_str().unwrap().to_owned()).collect()
    });
    let stop = stops(&stop_texts);
    let request = ffi::Options {
        max_new_tokens: i32::try_from(options["max_new_tokens"].as_u64().unwrap()).unwrap(),
        temperature: match &options["temperature"] {
            serde_json::Value::String(word) if word == "NaN" => f64::NAN,
            value => value.as_f64().unwrap_or(0.0),
        },
        json_schema: options["json_schema"]
            .as_str()
            .unwrap_or_default()
            .as_bytes(),
        stop: &stop,
        ..budget(1)
    };
    let started = generator.start(&messages, &request);
    if started.status != ffi::Status::Ok {
        return Err(started);
    }
    let (texts, last) = drain(generator);
    Ok((texts.concat() + &last.text, last))
}

/// The hub's `fixture.json` cases (hub CON237, CON288 (c)) through the bridge, with the C ABI's statuses.
#[test]
fn every_fixture_case_answers_through_the_bridge_as_the_hub_says() {
    let Some(mut generator) = generator() else {
        return;
    };
    let written = std::fs::read_to_string(tiny_model_dir().join("fixture.json")).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(&written).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let expect = &case["expect"];
        let wanted = expected_status(expect["error"].as_str());
        let outcome = outcome(&mut generator, case);
        let failure = match &outcome {
            Ok((_, last)) => &last.failure,
            Err(started) => started,
        };
        assert_eq!(failure.status, wanted, "{name}: {failure:?}");
        if matches!(
            wanted,
            ffi::Status::InvalidOptions | ffi::Status::ContextLengthExceeded
        ) {
            assert!(
                failure.genai.is_empty(),
                "{name}: OxidANT's own refusal: {failure:?}"
            );
        }
        let Some((text, last)) = outcome.ok().filter(|_| wanted == ffi::Status::Ok) else {
            continue;
        };
        assert_eq!(text, expect["text"].as_str().unwrap(), "{name}");
        assert_eq!(
            last.answer, text,
            "{name}: the pieces are the answer without thinking"
        );
        let finish = match last.stats.finish {
            ffi::Finish::Stop => "stop",
            ffi::Finish::Length => "length",
            other => panic!("{name}: {other:?}"),
        };
        assert_eq!(finish, expect["finish"].as_str().unwrap(), "{name}");
        if let Some(ids) = expect["ids"].as_array() {
            let ids: Vec<i32> = ids
                .iter()
                .map(|v| i32::try_from(v.as_i64().unwrap()).unwrap())
                .collect();
            assert_eq!(last.tokens, ids, "{name}");
        }
    }
}

#[test]
fn a_nul_byte_in_a_stop_string_is_an_invalid_option_of_oxidants_own() {
    let Some(mut generator) = generator() else {
        return;
    };
    let user = [ffi::Message {
        role: b"user",
        content: b"abc",
    }];
    let stop = [ffi::Stop { text: b"d\0e" }];
    let refused = generator.start(
        &user,
        &ffi::Options {
            stop: &stop,
            ..budget(4)
        },
    );
    assert_eq!(refused.status, ffi::Status::InvalidOptions, "{refused:?}");
    assert!(refused.message.contains("NUL"), "{refused:?}");
    assert!(refused.genai.is_empty(), "{refused:?}");
}

/// GenAI's words reach the engine's `last_error` apart from OxidANT's sentence, and its log; a failed load keeps the model.
#[test]
fn a_missing_model_directory_names_genais_own_words_and_logs_them() {
    let Some(mut generator) = generator() else {
        return;
    };
    take_log_records();
    let missing = tiny_model_dir()
        .join("absent")
        .to_string_lossy()
        .into_owned();
    let failed = generator.initialize(missing.as_bytes(), ffi::Provider::Cpu);
    assert_eq!(failed.status, ffi::Status::ModelLoadFailed, "{failed:?}");
    assert!(failed.genai.starts_with("config: "), "{failed:?}");
    assert!(failed.message.ends_with(&failed.genai), "{failed:?}");
    let logged = take_log_records();
    assert!(
        logged.iter().any(|r| r.level == LogLevel::Error
            && r.message == format!("ONNX Runtime GenAI: {}", failed.genai)),
        "{logged:?}"
    );
    assert!(generator.is_initialized(), "the earlier model stays loaded");
    let user = [ffi::Message {
        role: b"user",
        content: b"abc",
    }];
    assert_eq!(generator.start(&user, &budget(2)).status, ffi::Status::Ok);
    let (texts, last) = drain(&mut generator);
    assert_eq!(texts.concat() + &last.text, "de");
}
