//! ModelANT's text-generation-v1 contract, replayed on the released model llm.lock fetches: exact greedy token ids.
#![cfg(any(target_os = "linux", target_os = "windows"))]

mod common;

use std::path::PathBuf;

use common::{genai_required, model_required};
use kataglyphis_text::genai::{GenaiGenerator, Provider};
use kataglyphis_text::{CancelToken, ChatMessage, FinishReason, GenerationOptions, TextBackend};

/// ModelANT's contracts/text-generation-v1.json at the recorded gitlink; its ids are those of the release llm.lock pins.
const CONTRACT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../third_party/ModelANT/contracts/text-generation-v1.json"
));

/// Where `fetch-models.sh --llm` unpacks, unless `KATAGLYPHIS_LLM_MODELS` names another directory.
fn models_dir() -> PathBuf {
    std::env::var_os("KATAGLYPHIS_LLM_MODELS")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/models"))
}

#[test]
fn every_contract_case_decodes_to_its_expected_ids() {
    if !GenaiGenerator::library_available() && !genai_required() {
        eprintln!("SKIP: no chain GenAI loads here; CI sets KATAGLYPHIS_REQUIRE_GENAI=1");
        return;
    }
    let contract: serde_json::Value = serde_json::from_str(CONTRACT).unwrap();
    let cases = contract["cases"].as_array().unwrap();
    assert!(!cases.is_empty(), "the contract has no case");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let dir = models_dir().join(case["model"].as_str().unwrap());
        if !dir.join("genai_config.json").is_file() {
            assert!(
                !model_required(),
                "{} is missing: scripts/linux/fetch-models.sh --llm fetches it",
                dir.display()
            );
            eprintln!("SKIP: {name}: no {}", dir.display());
            continue;
        }
        let messages: Vec<ChatMessage> = case["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| ChatMessage::new(m["role"].as_str().unwrap(), m["content"].as_str().unwrap()))
            .collect();
        let expected: Vec<i32> = case["expected_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i32)
            .collect();
        let options = GenerationOptions {
            max_new_tokens: case["max_new_tokens"].as_u64().unwrap() as u32,
            ..GenerationOptions::default()
        };
        let mut generator =
            GenaiGenerator::load(&dir, Provider::Cpu).unwrap_or_else(|e| panic!("{name}: {e}"));
        let result = generator
            .generate(&messages, &options, &CancelToken::default(), &mut |_| {})
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(result.tokens, expected, "{name}: {:?}", result.text);
        let must_contain = case["must_contain"].as_str().unwrap();
        assert!(
            result.text.contains(must_contain),
            "{name}: {:?} lacks {must_contain:?}",
            result.text
        );
        eprintln!("{name}: {:?} {:?}", result.tokens, result.text);
        assert_ne!(result.finish_reason, FinishReason::Cancelled);
    }
}
