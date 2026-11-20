//! The hard cases replayed through the visit rules: each scenario under `tests/scenarios` must give
//! the visits written here, so a retuned constant or a new filter that changes one fails.

use std::path::{Path, PathBuf};

use kataglyphis_observant::capture::{replay, Visit};
use kataglyphis_observant::scenario::Scenario;

/// A recording's arrival and stay may drift by a run or two; the synthesised ones hit them exactly.
const SLACK_S: f64 = 1.0;

/// Visits as (arrival, stay) in seconds since the scenario started; `None` for one still open.
type Expected = &'static [(f64, Option<f64>)];

/// Each scenario's visits.
const EXPECTED: &[(&str, Expected)] = &[
    // The motion gate: a still false cat never arrives.
    ("drying-rack-still", &[]),
    // The ignored spot: the rack neither arrives nor keeps the cat's visit open.
    ("drying-rack-cat-in-front", &[(30.5, Some(44.0))]),
    // The minute's return: 50 s behind the sofa is one visit.
    ("cat-behind-the-sofa", &[(20.5, Some(112.0))]),
    // Breaks of 40 and 30 s stay one visit, one of 98 s starts a second.
    ("feeding-hour", &[(20.5, Some(281.0)), (400.5, Some(51.0))]),
    // Two hits in a row, the 0.25 score and the box limit: only the dim cat arrives.
    ("dark-room", &[(240.5, Some(51.0))]),
];

/// Built in, since the arm64 lane runs this binary on a runner that holds no checkout (`C:\ws` is the container's).
const SCENARIOS: &[(&str, &str)] = &[
    (
        "drying-rack-still",
        include_str!("scenarios/drying-rack-still.jsonl"),
    ),
    (
        "drying-rack-cat-in-front",
        include_str!("scenarios/drying-rack-cat-in-front.jsonl"),
    ),
    (
        "cat-behind-the-sofa",
        include_str!("scenarios/cat-behind-the-sofa.jsonl"),
    ),
    ("feeding-hour", include_str!("scenarios/feeding-hour.jsonl")),
    ("dark-room", include_str!("scenarios/dark-room.jsonl")),
];

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios")
}

fn text(name: &str) -> &'static str {
    SCENARIOS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, text)| *text)
        .unwrap_or_else(|| panic!("{name}: not built into the test"))
}

fn replayed(name: &str) -> Vec<Visit> {
    let scenario = Scenario::parse(text(name)).unwrap_or_else(|err| panic!("{name}: {err:#}"));
    replay(&scenario).unwrap_or_else(|err| panic!("{name}: {err:#}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= SLACK_S
}

#[test]
fn every_scenario_gives_its_visits_and_stays() {
    let mut wrong = Vec::new();
    for (name, expected) in EXPECTED {
        let got = replayed(name);
        let matches = got.len() == expected.len()
            && got
                .iter()
                .zip(expected.iter())
                .all(|(g, (arrived, stayed))| {
                    close(g.arrived, *arrived)
                        && match (g.stayed, stayed) {
                            (Some(a), Some(b)) => close(a, *b),
                            (None, None) => true,
                            _ => false,
                        }
                });
        if !matches {
            wrong.push(format!("{name}: expected {expected:?}, replayed {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_scenario_file_has_its_expected_visits() {
    let mut built: Vec<&str> = SCENARIOS.iter().map(|(n, _)| *n).collect();
    let mut expected: Vec<&str> = EXPECTED.iter().map(|(n, _)| *n).collect();
    built.sort_unstable();
    expected.sort_unstable();
    assert_eq!(
        built, expected,
        "a built-in scenario without expected visits, or the reverse"
    );
    // Where the checkout is present, a new file on disk must be built in and expected too.
    if !dir().is_dir() {
        eprintln!(
            "SKIP: no {} here (a staged test binary); the native lanes list it",
            dir().display()
        );
        return;
    }
    let mut files: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .filter_map(|e| {
            let path = e.unwrap().path();
            (path.extension()? == "jsonl").then(|| path.file_stem()?.to_str().map(str::to_owned))?
        })
        .collect();
    files.sort();
    let mut named: Vec<String> = EXPECTED.iter().map(|(n, _)| (*n).to_owned()).collect();
    named.sort();
    assert_eq!(
        files, named,
        "a scenario without expected visits, or the reverse"
    );
}

#[test]
fn each_scenario_says_where_its_frames_come_from() {
    for (name, _) in EXPECTED {
        let provenance = Scenario::parse(text(name)).unwrap().header.provenance;
        assert!(
            provenance.starts_with("recorded by ObservANT")
                || provenance.starts_with("synthesised, not recorded"),
            "{name}: {provenance}"
        );
    }
}
