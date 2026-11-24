//! The text area without a model: refusals before GenAI, the option mapping and the C ABI's numbers.

use super::*;

fn options<'a>(stop: &'a [ffi::Stop<'a>], json_schema: &'a [u8]) -> ffi::Options<'a> {
    ffi::Options {
        max_new_tokens: 8,
        temperature: 0.0,
        top_p: 1.0,
        top_k: 50,
        has_seed: false,
        seed: -1,
        json_schema,
        stop,
    }
}

#[test]
fn a_provider_without_a_genai_route_is_refused_before_the_library_loads() {
    let mut generator = new_text_generator();
    for provider in [
        ffi::Provider::MigraphX,
        ffi::Provider::Xnnpack,
        ffi::Provider::Dnnl,
        ffi::Provider { repr: 42 },
    ] {
        let failed = generator.initialize(b"/no/such/model", provider);
        assert_eq!(
            failed.status,
            ffi::Status::ProviderUnavailable,
            "{provider:?}"
        );
        assert!(failed.genai.is_empty(), "{failed:?}");
        assert!(
            failed
                .message
                .starts_with("the execution provider is not available"),
            "{failed:?}"
        );
    }
    assert!(!generator.is_initialized());
}

#[test]
fn the_cpu_cuda_and_directml_have_their_genai_route() {
    for (engine, route) in [
        (ffi::Provider::Cpu, genai::Provider::Cpu),
        (ffi::Provider::Cuda, genai::Provider::Cuda),
        (ffi::Provider::DirectMl, genai::Provider::DirectMl),
    ] {
        assert_eq!(genai_provider(engine).unwrap(), route, "{engine:?}");
    }
}

#[test]
fn a_model_path_that_is_no_utf8_fails_to_load_before_the_library() {
    let mut generator = new_text_generator();
    let failed = generator.initialize(b"/models/\xff", ffi::Provider::Cpu);
    assert_eq!(failed.status, ffi::Status::ModelLoadFailed, "{failed:?}");
    assert!(failed.message.ends_with("path is not UTF-8"), "{failed:?}");
    assert!(failed.genai.is_empty());
}

#[test]
fn a_generator_without_a_model_is_not_initialized() {
    let mut generator = new_text_generator();
    assert!(!generator.is_initialized());
    let message = ffi::Message {
        role: b"user",
        content: b"abc",
    };
    let started = generator.start(&[message], &options(&[], b""));
    assert_eq!(started.status, ffi::Status::NotInitialized);
    let piece = generator.next_piece();
    assert!(piece.finished && piece.text.is_empty() && piece.tokens.is_empty());
    assert_eq!(piece.failure.status, ffi::Status::NotInitialized);
}

#[test]
fn the_engines_signed_options_map_onto_the_text_crates() {
    let stops = [ffi::Stop { text: b"END" }];
    let mut seeded = options(&stops, br#"{"type":"object"}"#);
    seeded.has_seed = true;
    seeded.seed = 7;
    let mapped = generation_options(&seeded).unwrap();
    assert_eq!(mapped.seed, Some(7));
    assert_eq!(mapped.stop, ["END"]);
    assert_eq!(mapped.json_schema.as_deref(), Some(r#"{"type":"object"}"#));
    assert_eq!((mapped.max_new_tokens, mapped.top_k), (8, 50));
    let plain = generation_options(&options(&[], b"")).unwrap();
    assert_eq!((plain.seed, plain.json_schema.as_deref()), (None, None));
    assert_eq!(
        plain,
        GenerationOptions {
            max_new_tokens: 8,
            ..GenerationOptions::default()
        }
    );
}

#[test]
fn a_negative_count_or_seed_and_text_that_is_no_utf8_are_invalid_options() {
    let bad_stop = [ffi::Stop { text: b"\xff" }];
    let mut cases = vec![
        options(&bad_stop, b""),
        options(&[], b"\xfe"),
        ffi::Options {
            max_new_tokens: -1,
            ..options(&[], b"")
        },
        ffi::Options {
            top_k: -1,
            ..options(&[], b"")
        },
    ];
    for seed in [-1, i64::from(u32::MAX) + 1] {
        cases.push(ffi::Options {
            has_seed: true,
            seed,
            ..options(&[], b"")
        });
    }
    for case in &cases {
        let refused = generation_options(case).unwrap_err();
        assert!(matches!(refused, TextError::InvalidOptions(_)), "{refused}");
    }
    let message = ffi::Message {
        role: b"user",
        content: b"a\xc3",
    };
    assert!(matches!(
        chat(&[message]),
        Err(TextError::InvalidOptions(_))
    ));
}

#[test]
fn each_status_is_the_c_abis_number_and_each_error_has_one() {
    let numbered = [
        (ffi::Status::Ok, 0),
        (ffi::Status::LibraryUnavailable, 1),
        (ffi::Status::ModelLoadFailed, 2),
        (ffi::Status::ProviderUnavailable, 3),
        (ffi::Status::TokenizerFailed, 4),
        (ffi::Status::ChatTemplateFailed, 5),
        (ffi::Status::EncodeFailed, 6),
        (ffi::Status::InvalidOptions, 7),
        (ffi::Status::GenerationFailed, 8),
        (ffi::Status::NotInitialized, 9),
        (ffi::Status::JsonSchemaUnsupported, 10),
        (ffi::Status::ContextLengthExceeded, 11),
    ];
    for (status, number) in numbered {
        assert_eq!(status.repr, number, "{status:?}");
    }
    let x = || "x".to_owned();
    for (error, expected) in [
        (
            TextError::LibraryUnavailable(x()),
            ffi::Status::LibraryUnavailable,
        ),
        (
            TextError::ModelLoadFailed(x()),
            ffi::Status::ModelLoadFailed,
        ),
        (
            TextError::ProviderUnavailable(x()),
            ffi::Status::ProviderUnavailable,
        ),
        (
            TextError::TokenizerFailed(x()),
            ffi::Status::TokenizerFailed,
        ),
        (
            TextError::ChatTemplateFailed(x()),
            ffi::Status::ChatTemplateFailed,
        ),
        (TextError::EncodeFailed(x()), ffi::Status::EncodeFailed),
        (TextError::InvalidOptions(x()), ffi::Status::InvalidOptions),
        (
            TextError::GenerationFailed(x()),
            ffi::Status::GenerationFailed,
        ),
        (
            TextError::JsonSchemaUnsupported(x()),
            ffi::Status::JsonSchemaUnsupported,
        ),
        (
            TextError::ContextLengthExceeded(x()),
            ffi::Status::ContextLengthExceeded,
        ),
        (
            TextError::EndpointStatus(503, x()),
            ffi::Status::GenerationFailed,
        ),
        (
            TextError::EmbeddingsUnsupported(x()),
            ffi::Status::GenerationFailed,
        ),
    ] {
        assert_eq!(status(&error), expected, "{error}");
        let failed = failure(&error);
        assert_eq!(failed.message, error.to_string());
    }
    assert_eq!(
        [
            ffi::Finish::Stop,
            ffi::Finish::Length,
            ffi::Finish::Cancelled
        ]
        .map(|f| f.repr),
        [0, 1, 2]
    );
}

#[test]
fn a_panic_is_a_failure_with_its_reason_and_never_unwinds() {
    let failed = guarded(
        || panic!("broken generator"),
        panicked(ffi::Status::GenerationFailed),
    );
    assert_eq!(failed.status, ffi::Status::GenerationFailed);
    assert_eq!(failed.message, "OxidANT panicked: broken generator");
    let piece = ended(failed);
    assert!(piece.finished);
}

#[test]
fn a_cancel_handle_shares_its_generators_token() {
    let generator = new_text_generator();
    let handle = generator.cancel_handle();
    assert!(!generator.cancel.is_cancelled());
    handle.cancel();
    assert!(generator.cancel.is_cancelled());
    CancelHandle::default().cancel();
}

#[test]
fn a_model_that_names_its_threads_keeps_them_and_one_without_takes_the_rule() {
    let own = tempfile::tempdir().unwrap();
    std::fs::write(
        own.path().join("genai_config.json"),
        r#"{"model":{"decoder":{"session_options":{"intra_op_num_threads":2}}}}"#,
    )
    .unwrap();
    assert_eq!(
        text_intra_op_threads(own.path().to_string_lossy().as_bytes()),
        0
    );
    let rule = text_intra_op_threads(b"/no/such/model");
    assert!((1..=4).contains(&rule), "{rule}");
}

#[test]
fn a_finished_pull_carries_the_figures_and_every_token() {
    let result = GenerationResult {
        text: "de".into(),
        tokens: vec![100, 101],
        finish_reason: FinishReason::Cancelled,
        prompt_tokens: 3,
        completion_tokens: 2,
        time_to_first_token_ms: 1.5,
        decode_tokens_per_second: 40.0,
        thinking_chars: 4,
    };
    let last = piece(Ok(Pulled {
        text: "e".into(),
        done: Some(result),
    }));
    assert!(last.finished && last.text == "e" && last.answer == "de");
    assert_eq!(last.tokens, [100, 101]);
    assert_eq!(last.stats.finish, ffi::Finish::Cancelled);
    assert_eq!(
        (
            last.stats.prompt_tokens,
            last.stats.completion_tokens,
            last.stats.thinking_chars
        ),
        (3, 2, 4)
    );
    assert_eq!(last.stats.time_to_first_token_ms, 1.5);
    let next = piece(Ok(Pulled {
        text: "d".into(),
        done: None,
    }));
    assert!(!next.finished && next.text == "d" && next.answer.is_empty());
    assert_eq!(next.failure.status, ffi::Status::Ok);
    let failed = piece(Err(TextError::InvalidOptions(
        "a stop string holds a NUL byte".into(),
    )));
    assert_eq!(failed.failure.status, ffi::Status::InvalidOptions);
    assert!(failed.failure.genai.is_empty());
}
