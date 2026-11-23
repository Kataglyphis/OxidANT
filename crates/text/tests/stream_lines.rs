//! The hub's shared OpenAI stream lines, replayed through this crate's reader (ANTfrastructure CON237, CON288).

use kataglyphis_text::endpoint::{parse_stream_line, StreamEvent};
use kataglyphis_text::TextError;
use serde_json::Value;

const CASES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../third_party/ANTfrastructure/shared/contracts/openai-stream-lines.json"
));

/// What a stream of lines comes to, in the file's terms: text, thinking, finish, usage and done.
fn replay(lines: &[Value]) -> Result<Value, TextError> {
    let (mut text, mut thinking, mut finish, mut usage, mut done) = (
        String::new(),
        String::new(),
        Value::Null,
        Value::Null,
        false,
    );
    'lines: for line in lines {
        for event in parse_stream_line(line.as_str().unwrap())? {
            match event {
                StreamEvent::Text(piece) => text.push_str(&piece),
                StreamEvent::Thinking(piece) => thinking.push_str(&piece),
                StreamEvent::Finish { raw, .. } => finish = raw.into(),
                StreamEvent::Usage { prompt, completion } => {
                    usage = serde_json::json!([prompt, completion])
                }
                // The endpoint backend stops reading here too (owner D8).
                StreamEvent::Done => {
                    done = true;
                    break 'lines;
                }
            }
        }
    }
    Ok(
        serde_json::json!({"text": text, "thinking": thinking, "finish": finish, "usage": usage, "done": done}),
    )
}

#[test]
fn every_shared_stream_case_reads_as_the_hub_file_says() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let cases = doc["cases"].as_array().unwrap();
    assert!(cases.len() >= 22, "the file shrank: {}", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let got = replay(case["lines"].as_array().unwrap());
        match case["expect"]["error"].as_str() {
            Some(message) => match got {
                Err(TextError::GenerationFailed(why)) => {
                    assert!(
                        why.contains(message),
                        "{name}: {why:?} names no {message:?}"
                    )
                }
                other => panic!("{name}: expected the error {message:?}, got {other:?}"),
            },
            None => assert_eq!(got.unwrap(), case["expect"], "{name}"),
        }
    }
}
