//! The chain GenAI on the hub's tiny model: each byte predicts its successor, so every answer is known.
#![cfg(any(target_os = "linux", target_os = "windows"))]

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use common::{
    genai_required, tiny_embedding_fixture, tiny_embedding_model_dir, tiny_fixture, tiny_model_dir,
};
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

/// Full-vocabulary sampling at temperature 1: the tiny model's successor keeps about 99 % of the mass.
fn sampled(seed: u32) -> GenerationOptions {
    GenerationOptions {
        max_new_tokens: 24,
        temperature: 1.0,
        top_p: 1.0,
        top_k: 0,
        seed: Some(seed),
        ..GenerationOptions::default()
    }
}

fn tokens(generator: &mut GenaiGenerator, options: &GenerationOptions) -> Vec<i32> {
    let (prompt, _, _) = tiny_fixture();
    generator
        .generate(
            &[ChatMessage::user(&prompt)],
            options,
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap()
        .tokens
}

#[test]
fn a_repeated_seed_yields_the_same_tokens() {
    let Some(mut generator) = generator() else {
        return;
    };
    let first = tokens(&mut generator, &sampled(7));
    assert_eq!(first.len(), 24);
    assert_eq!(tokens(&mut generator, &sampled(7)), first);
}

#[test]
fn four_seeds_sample_at_least_two_different_answers() {
    let Some(mut generator) = generator() else {
        return;
    };
    let answers: std::collections::HashSet<Vec<i32>> = (1..=4)
        .map(|seed| tokens(&mut generator, &sampled(seed)))
        .inspect(|answer| assert!(answer.len() >= 16, "{answer:?}"))
        .collect();
    assert!(answers.len() >= 2, "{answers:?}");
}

/// Tight enough that the tiny model's successor rule ends in one valid answer.
const SCHEMA: &str = r#"{"type":"object","properties":{"a":{"type":"string","maxLength":3}},"required":["a"],"additionalProperties":false,"x-guidance":{"whitespace_flexible":false}}"#;

fn schema_run(
    generator: &mut GenaiGenerator,
) -> Result<kataglyphis_text::GenerationResult, TextError> {
    let (prompt, _, _) = tiny_fixture();
    generator.generate(
        &[ChatMessage::user(&prompt)],
        &GenerationOptions {
            max_new_tokens: 40,
            json_schema: Some(SCHEMA.into()),
            ..GenerationOptions::default()
        },
        &CancelToken::default(),
        &mut |_| {},
    )
}

#[test]
fn a_json_schema_holds_the_answer_to_a_valid_object() {
    let Some(mut generator) = generator() else {
        return;
    };
    let result = schema_run(&mut generator).unwrap();
    let answer: serde_json::Value =
        serde_json::from_str(&result.text).unwrap_or_else(|e| panic!("{e}: {:?}", result.text));
    let object = answer.as_object().expect("an object");
    assert_eq!(object.len(), 1, "{answer}");
    let a = object["a"].as_str().expect("a string");
    assert!(a.chars().count() <= 3, "{answer}");
    assert_eq!(result.finish_reason, FinishReason::Stop);
}

#[test]
fn a_stop_string_ends_the_answer_before_it() {
    let Some(mut generator) = generator() else {
        return;
    };
    let (prompt, _, _) = tiny_fixture();
    let mut pieces = Vec::new();
    let result = generator
        .generate(
            &[ChatMessage::user(&prompt)],
            &GenerationOptions {
                max_new_tokens: 10,
                stop: vec!["fg".into()],
                ..GenerationOptions::default()
            },
            &CancelToken::default(),
            &mut |p| pieces.push(p.to_string()),
        )
        .unwrap();
    assert_eq!(result.text, "de");
    assert_eq!(pieces.concat(), "de");
    assert_eq!(result.finish_reason, FinishReason::Stop);
    assert_eq!(result.tokens, [100, 101, 102, 103]);
}

/// The library first, like [`generator`], for the model that has a `hidden_states` output.
fn embedder() -> Option<GenaiGenerator> {
    if !GenaiGenerator::library_available() && !genai_required() {
        eprintln!("SKIP: no chain GenAI loads here; CI sets KATAGLYPHIS_REQUIRE_GENAI=1");
        return None;
    }
    Some(GenaiGenerator::load(tiny_embedding_model_dir(), Provider::Cpu).unwrap())
}

fn assert_close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-6, "{actual:?} is not {expected:?}");
    }
}

#[test]
fn an_embedding_is_the_oracles_unit_length_mean_of_the_hidden_states() {
    let Some(mut embedder) = embedder() else {
        return;
    };
    let (text, expected) = tiny_embedding_fixture();
    let vectors = embedder.embed(std::slice::from_ref(&text)).unwrap();
    assert_eq!(vectors.len(), 1);
    assert_close(&vectors[0], &expected);
    let norm = vectors[0].iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-6, "{norm}");
}

#[test]
fn several_texts_keep_their_order_and_the_same_text_repeats_its_vector() {
    let Some(mut embedder) = embedder() else {
        return;
    };
    let (text, expected) = tiny_embedding_fixture();
    let vectors = embedder
        .embed(&["\u{ff}\u{ff}".to_string(), text.clone(), text])
        .unwrap();
    assert_eq!(vectors.len(), 3);
    assert_close(&vectors[1], &expected);
    assert_eq!(vectors[1], vectors[2]);
    let cosine: f32 = vectors[0].iter().zip(&vectors[1]).map(|(a, b)| a * b).sum();
    assert!(
        cosine < 0.999,
        "two different texts embedded alike: {cosine}"
    );
}

#[test]
fn embedding_leaves_generation_working_on_the_same_model() {
    let Some(mut embedder) = embedder() else {
        return;
    };
    let (prompt, continuation, ids) = tiny_fixture();
    embedder.embed(std::slice::from_ref(&prompt)).unwrap();
    let result = embedder
        .generate(
            &[ChatMessage::user(&prompt)],
            &budget(ids.len()),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(result.text, continuation);
}

#[test]
fn a_model_without_hidden_states_refuses_to_embed() {
    let Some(mut generator) = generator() else {
        return;
    };
    let refused = generator.embed(&["abc".to_string()]).unwrap_err();
    assert!(
        matches!(&refused, TextError::EmbeddingsUnsupported(d) if d.contains("include_hidden_states=true")),
        "{refused}"
    );
}

#[test]
fn texts_that_cannot_be_embedded_are_errors_not_vectors() {
    let Some(mut embedder) = embedder() else {
        return;
    };
    for texts in [vec![], vec![String::new()], vec!["a\0b".to_string()]] {
        let refused = embedder.embed(&texts).unwrap_err();
        assert!(
            matches!(refused, TextError::InvalidOptions(_)),
            "{texts:?}: {refused}"
        );
    }
    let refused = embedder.embed(&["a".repeat(600)]).unwrap_err();
    assert!(refused.to_string().contains("context_length"), "{refused}");
}
