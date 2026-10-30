//! The chain GenAI on the hub's tiny model: each byte predicts its successor, so every answer is known.
#![cfg(any(target_os = "linux", target_os = "windows"))]

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use common::{genai_required, tiny_fixture, tiny_model_dir};
use kataglyphis_text::genai::{GenaiGenerator, Provider};
use kataglyphis_text::{
    CancelToken, ChatMessage, FinishReason, GenerationOptions, TextBackend, TextError,
};

/// From the byte 0xBF the model counts up to the end token, 256, in 65 tokens.
const NEAR_THE_END: &str = "\u{bf}";
const TOKENS_TO_THE_END: usize = 65;

/// The library first, so a host without GenAI skips before it needs Python for the model.
fn generator() -> Option<GenaiGenerator> {
    if !GenaiGenerator::library_available() && !genai_required() {
        eprintln!("SKIP: no chain GenAI loads here; CI sets KATAGLYPHIS_REQUIRE_GENAI=1");
        return None;
    }
    match GenaiGenerator::load(tiny_model_dir(), Provider::Cpu) {
        Ok(generator) => Some(generator),
        Err(e) => panic!("the tiny model did not load: {e}"),
    }
}

fn budget(tokens: usize) -> GenerationOptions {
    GenerationOptions {
        max_new_tokens: tokens as u32,
        ..GenerationOptions::default()
    }
}

#[test]
fn greedy_decoding_yields_the_oracles_continuation() {
    let Some(mut generator) = generator() else {
        return;
    };
    let (prompt, continuation, ids) = tiny_fixture();
    let mut pieces = Vec::new();
    let result = generator
        .generate(
            &[ChatMessage::user(&prompt)],
            &budget(ids.len()),
            &CancelToken::default(),
            &mut |p| pieces.push(p.to_string()),
        )
        .unwrap();
    assert_eq!(result.tokens, ids);
    assert_eq!(result.text, continuation);
    assert_eq!(pieces.concat(), continuation);
    assert_eq!(result.prompt_tokens, prompt.len());
    assert_eq!(result.completion_tokens, ids.len());
    assert_eq!(result.finish_reason, FinishReason::Length);
    assert!(result.time_to_first_token_ms >= 0.0 && result.decode_tokens_per_second > 0.0);
}

#[test]
fn the_end_token_stops_before_the_budget() {
    let Some(mut generator) = generator() else {
        return;
    };
    let result = generator
        .generate(
            &[ChatMessage::user(NEAR_THE_END)],
            &budget(TOKENS_TO_THE_END + 10),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(result.tokens.len(), TOKENS_TO_THE_END);
    assert_eq!(
        (result.tokens[0], result.tokens[TOKENS_TO_THE_END - 1]),
        (0xC0, 0x100)
    );
    assert_eq!(result.finish_reason, FinishReason::Stop);
}

#[test]
fn a_cancel_from_another_thread_ends_the_run_after_its_token() {
    let Some(mut generator) = generator() else {
        return;
    };
    let (prompt, _, ids) = tiny_fixture();
    let (cancel, first_piece, released) = (
        CancelToken::default(),
        AtomicBool::new(false),
        AtomicBool::new(false),
    );
    let result = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            generator.generate(
                &[ChatMessage::user(&prompt)],
                &budget(ids.len()),
                &cancel,
                &mut |_| {
                    first_piece.store(true, Ordering::SeqCst);
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while !released.load(Ordering::SeqCst) && Instant::now() < deadline {
                        std::thread::yield_now();
                    }
                },
            )
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !first_piece.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::yield_now();
        }
        cancel.cancel();
        released.store(true, Ordering::SeqCst);
        worker.join().unwrap()
    })
    .unwrap();
    assert_eq!(result.finish_reason, FinishReason::Cancelled);
    assert_eq!(result.tokens.len(), 1);
}

#[test]
fn a_cancel_left_from_an_earlier_run_does_not_stop_the_next() {
    let Some(mut generator) = generator() else {
        return;
    };
    let (prompt, continuation, ids) = tiny_fixture();
    let cancel = CancelToken::default();
    cancel.cancel();
    let result = generator
        .generate(
            &[ChatMessage::user(&prompt)],
            &budget(ids.len()),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(result.text, continuation);
}

#[test]
fn invalid_requests_are_refused_before_any_decoding() {
    let Some(mut generator) = generator() else {
        return;
    };
    let none = CancelToken::default();
    let empty = generator
        .generate(&[], &budget(1), &none, &mut |_| {})
        .unwrap_err();
    assert!(matches!(empty, TextError::InvalidOptions(_)), "{empty}");
    let zero = generator
        .generate(&[ChatMessage::user("a")], &budget(0), &none, &mut |_| {})
        .unwrap_err();
    assert!(matches!(zero, TextError::InvalidOptions(_)), "{zero}");
    let nul = generator
        .generate(&[ChatMessage::user("a\0b")], &budget(1), &none, &mut |_| {})
        .unwrap_err();
    assert!(matches!(nul, TextError::InvalidOptions(_)), "{nul}");
}

#[test]
fn a_missing_model_directory_fails_to_load() {
    if generator().is_none() {
        return;
    }
    let missing = tiny_model_dir().join("absent");
    let refused = GenaiGenerator::load(&missing, Provider::Cpu)
        .err()
        .expect("an absent directory loaded");
    assert!(
        matches!(refused, TextError::ModelLoadFailed(_)),
        "{refused}"
    );
}
