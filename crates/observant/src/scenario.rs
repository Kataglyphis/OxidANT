//! A scenario: what the model returned run by run, as JSON lines, so a replay can hold the visit rules
//! against the hard cases. The first line is a [`Header`], every further one a [`Frame`].
//! `--record-scenario FILE` writes one from a live run; `tests/scenarios` replays them.

use std::fs::File;
use std::io::{BufWriter, Write as _};
use std::path::Path;
use std::time::Instant;

use anyhow::Context as _;
use kataglyphis_core::Detection;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::motion::Moving;
use crate::presence::Zone;

/// The format's version, the header's `observant_scenario`.
pub const VERSION: u32 = 1;

/// A recording asks the model down to this score, so a replay can try a lower threshold than the live one.
pub const RECORD_FLOOR: f32 = 0.1;

/// The camera's facts a replay needs; the rules themselves come from the build that replays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub observant_scenario: u32,
    /// Where the frames come from: a recording names its camera and date, a synthesised one its sources.
    pub provenance: String,
    pub width: u32,
    pub height: u32,
    pub inference_fps: f32,
    /// The lowest score the model was asked for; a replay cannot judge a threshold below it.
    pub floor: f32,
    /// The spots marked "not a cat" when the recording started.
    pub ignore: Vec<Zone>,
}

/// One model run: when, what it returned, and where something moved lately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    /// Seconds since the recording started.
    pub t: f64,
    pub boxes: Vec<Sighting>,
    /// `None` with the motion gate off.
    pub moving: Option<MovingRuns>,
}

/// One detection in pixels of the `width` x `height` frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sighting {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub score: f32,
    /// The COCO class.
    pub class: i64,
}

impl From<&Detection> for Sighting {
    fn from(d: &Detection) -> Self {
        Self {
            x1: d.x1,
            y1: d.y1,
            x2: d.x2,
            y2: d.y2,
            score: d.score,
            class: d.class_id,
        }
    }
}

impl From<&Sighting> for Detection {
    fn from(s: &Sighting) -> Self {
        Self {
            x1: s.x1,
            y1: s.y1,
            x2: s.x2,
            y2: s.y2,
            score: s.score,
            class_id: s.class,
        }
    }
}

/// [`Moving`] as runs of row-major block indices, `[first, count]`: a few runs where a bitmap takes thousands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovingRuns {
    pub cols: usize,
    pub rows: usize,
    pub runs: Vec<[usize; 2]>,
}

impl From<&Moving> for MovingRuns {
    fn from(moving: &Moving) -> Self {
        let (cols, blocks) = moving.blocks();
        let mut runs: Vec<[usize; 2]> = Vec::new();
        for (i, _) in blocks.iter().enumerate().filter(|(_, &m)| m) {
            match runs.last_mut() {
                Some(run) if run[0] + run[1] == i => run[1] += 1,
                _ => runs.push([i, 1]),
            }
        }
        Self {
            cols,
            rows: blocks.len().checked_div(cols).unwrap_or(0),
            runs,
        }
    }
}

impl From<&MovingRuns> for Moving {
    fn from(r: &MovingRuns) -> Self {
        let len = r.cols * r.rows;
        let mut blocks = vec![false; len];
        for &[first, count] in &r.runs {
            for block in blocks.iter_mut().skip(first).take(count) {
                *block = true;
            }
        }
        Moving::from_blocks(r.cols, blocks)
    }
}

/// A whole scenario file.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub header: Header,
    pub frames: Vec<Frame>,
}

impl Scenario {
    /// Reads the JSON lines of a scenario; a blank line is skipped, anything else malformed is an error.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let mut lines = text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty());
        let (_, first) = lines.next().context("an empty scenario has no header")?;
        let header: Header = serde_json::from_str(first).context("line 1: the header")?;
        anyhow::ensure!(
            header.observant_scenario == VERSION,
            "scenario format {} where this build reads {VERSION}",
            header.observant_scenario
        );
        let frames = lines
            .map(|(i, l)| serde_json::from_str(l).with_context(|| format!("line {}", i + 1)))
            .collect::<anyhow::Result<Vec<Frame>>>()?;
        anyhow::ensure!(
            frames.windows(2).all(|w| w[0].t <= w[1].t),
            "the frames' times run backwards"
        );
        Ok(Self { header, frames })
    }
}

/// Writes a live run's scenario, one line per model run, flushed so a stop keeps every line.
pub struct Recorder {
    out: BufWriter<File>,
    started: Instant,
}

impl Recorder {
    /// Creates `path` and writes the header from `config` and the spots marked "not a cat".
    pub fn create(path: &Path, config: &Config, ignore: Vec<Zone>) -> anyhow::Result<Self> {
        let file =
            File::create(path).with_context(|| format!("scenario file {}", path.display()))?;
        let header = Header {
            observant_scenario: VERSION,
            provenance: format!(
                "recorded by ObservANT {} on {}, unix time {}",
                crate::status::VERSION,
                config.name,
                crate::notify::unix_now()
            ),
            width: config.width,
            height: config.height,
            inference_fps: config.inference_fps,
            floor: RECORD_FLOOR.min(config.score),
            ignore,
        };
        let mut recorder = Self {
            out: BufWriter::new(file),
            started: Instant::now(),
        };
        recorder.line(&header)?;
        Ok(recorder)
    }

    /// One run's `detections`, seen at `now`.
    pub fn frame(
        &mut self,
        now: Instant,
        detections: &[Detection],
        moving: Option<&Moving>,
    ) -> anyhow::Result<()> {
        let frame = Frame {
            t: now.saturating_duration_since(self.started).as_secs_f64(),
            boxes: detections.iter().map(Sighting::from).collect(),
            moving: moving.map(MovingRuns::from),
        };
        self.line(&frame)
    }

    fn line(&mut self, value: &impl Serialize) -> anyhow::Result<()> {
        serde_json::to_writer(&mut self.out, value)?;
        self.out.write_all(b"\n")?;
        self.out.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_blocks_survive_the_runs_both_ways() {
        let blocks = vec![
            true, true, false, false, //
            false, true, true, true, //
            false, false, false, true,
        ];
        let moving = Moving::from_blocks(4, blocks.clone());
        let runs = MovingRuns::from(&moving);
        assert_eq!(runs.runs, vec![[0, 2], [5, 3], [11, 1]]);
        assert_eq!((runs.cols, runs.rows), (4, 3));
        assert_eq!(Moving::from(&runs).blocks(), (4, blocks.as_slice()));
        let still = MovingRuns::from(&Moving::from_blocks(0, Vec::new()));
        assert_eq!((still.rows, still.runs.len()), (0, 0));
    }

    #[test]
    fn a_recording_reads_back_as_it_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.jsonl");
        let config = Config {
            name: "himbeere2".into(),
            score: 0.3,
            ..Config::default()
        };
        let zone = [0.25, 0.75, 0.75, 1.0];
        let mut recorder = Recorder::create(&path, &config, vec![zone]).unwrap();
        let cat = Detection {
            x1: 1.0,
            y1: 2.0,
            x2: 30.0,
            y2: 40.0,
            score: 0.5,
            class_id: 15,
        };
        let moving = Moving::from_blocks(2, vec![false, true, true, false]);
        let t = Instant::now();
        recorder.frame(t, &[cat], Some(&moving)).unwrap();
        recorder.frame(t, &[], None).unwrap();
        drop(recorder);
        let read = Scenario::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(read.header.provenance.contains("recorded by ObservANT"));
        assert!(read.header.provenance.contains("himbeere2"));
        assert_eq!((read.header.width, read.header.height), (640, 480));
        assert_eq!(read.header.floor, RECORD_FLOOR);
        assert_eq!(read.header.ignore, vec![zone]);
        assert_eq!(read.frames.len(), 2);
        assert_eq!(Detection::from(&read.frames[0].boxes[0]), cat);
        assert_eq!(
            Moving::from(read.frames[0].moving.as_ref().unwrap()),
            moving
        );
        assert!(read.frames[1].moving.is_none());
        assert!(read.frames[1].t >= read.frames[0].t);
    }

    #[test]
    fn a_malformed_scenario_names_its_line() {
        assert!(Scenario::parse("").is_err());
        let header = r#"{"observant_scenario":1,"provenance":"x","width":64,"height":48,"inference_fps":2.0,"floor":0.1,"ignore":[]}"#;
        assert_eq!(Scenario::parse(header).unwrap().frames.len(), 0);
        let wrong = header.replace("\"observant_scenario\":1", "\"observant_scenario\":2");
        assert!(Scenario::parse(&wrong)
            .unwrap_err()
            .to_string()
            .contains("format 2"));
        let bad = format!("{header}\n\n{{\"t\":1.0,\"boxes\":[],\"moving\":null}}\n{{\"t\":0.5}}");
        assert!(format!("{:#}", Scenario::parse(&bad).unwrap_err()).contains("line 4"));
        let backwards = format!(
            "{header}\n{{\"t\":1.0,\"boxes\":[],\"moving\":null}}\n{{\"t\":0.5,\"boxes\":[],\"moving\":null}}"
        );
        assert!(Scenario::parse(&backwards)
            .unwrap_err()
            .to_string()
            .contains("backwards"));
    }
}
