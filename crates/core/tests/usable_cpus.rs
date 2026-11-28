//! The hub's usable-CPU cases (ANTfrastructure CON304), replayed through this crate's quota reader.

use kataglyphis_core::config::{quota_along, usable_of};
use serde_json::Value;

const CASES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../third_party/ANTfrastructure/shared/contracts/usable-cpus.json"
));

fn count(value: &Value) -> usize {
    usize::try_from(value.as_u64().unwrap()).unwrap()
}

/// The quota of a case's `cpu_max`, own cgroup first: a path one segment shorter than the list, read up to the root.
fn quota_of(levels: &[Option<String>]) -> Option<usize> {
    if levels.is_empty() {
        return None;
    }
    let path = (1..levels.len())
        .map(|depth| format!("level{depth}"))
        .collect::<Vec<_>>()
        .join("/");
    quota_along(&path, |dir| {
        let depth = if dir.is_empty() {
            0
        } else {
            dir.split('/').count()
        };
        levels[levels.len() - 1 - depth].clone()
    })
}

#[test]
fn every_hub_case_gives_its_usable_cpu_count() {
    let file: Value = serde_json::from_str(CASES).unwrap();
    assert_eq!(file["schema_version"], 1, "a new schema needs a new reader");
    let cases = file["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let levels: Vec<Option<String>> = case["cpu_max"]
            .as_array()
            .unwrap()
            .iter()
            .map(|level| level.as_str().map(str::to_owned))
            .collect();
        assert_eq!(
            usable_of(count(&case["affinity"]), quota_of(&levels)),
            count(&case["expect"]["usable"]),
            "{}",
            case["name"]
        );
    }
}
