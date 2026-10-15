//! What the ObservANT achieves and costs, sampled every [`PERIOD`] for `/healthz`: its frame and model
//! rates and the model's time per run, and where `/proc` and `/sys` exist (Linux) its CPU share,
//! memory, the system load and the board's temperature. A number that cannot be read is `None`.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::status::Status;

/// How often the sampler looks; the rates are over this window.
pub const PERIOD: Duration = Duration::from_secs(2);

/// Clock ticks per second in `/proc/<pid>/stat`: Linux's USER_HZ, 100 wherever the ObservANT runs.
const TICKS_PER_SECOND: f64 = 100.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Stats {
    pub capture_fps: Option<f64>,
    pub model_runs_per_s: Option<f64>,
    /// The model's time per run, smoothed over the last runs.
    pub model_ms: Option<f64>,
    /// Of one core: 100 is one core busy.
    pub cpu_percent: Option<f64>,
    pub cores: usize,
    pub rss_mb: Option<f64>,
    pub load_1m: Option<f64>,
    pub temperature_c: Option<f64>,
    pub uptime_s: u64,
}

/// The counters at one look.
#[derive(Debug, Clone, Copy)]
pub struct Counters {
    pub at: Instant,
    pub frames: u64,
    pub runs: u64,
    pub cpu_ticks: Option<u64>,
}

/// Turns two looks into rates.
#[derive(Debug, Default)]
pub struct Sampler {
    last: Option<Counters>,
}

impl Sampler {
    /// The rates since the last look; `None` on the first one.
    pub fn rates(&mut self, now: Counters) -> (Option<f64>, Option<f64>, Option<f64>) {
        let Some(last) = self.last.replace(now) else {
            return (None, None, None);
        };
        let seconds = now.at.duration_since(last.at).as_secs_f64();
        if seconds <= 0.0 {
            return (None, None, None);
        }
        let per_second = |now: u64, then: u64| now.checked_sub(then).map(|d| d as f64 / seconds);
        let cpu = match (now.cpu_ticks, last.cpu_ticks) {
            (Some(now), Some(then)) => per_second(now, then).map(|t| t / TICKS_PER_SECOND * 100.0),
            _ => None,
        };
        (
            per_second(now.frames, last.frames).map(|v| round(v, 1)),
            per_second(now.runs, last.runs).map(|v| round(v, 2)),
            cpu.map(|v| round(v, 1)),
        )
    }
}

/// Samples `status` every [`PERIOD`] on a thread of its own, for as long as the process runs.
pub fn spawn(status: Arc<Status>) {
    let started = Instant::now();
    let spawned = thread::Builder::new()
        .name("observant-stats".into())
        .spawn(move || {
            let mut sampler = Sampler::default();
            loop {
                let (frames, runs) = status.counters();
                let (capture_fps, model_runs_per_s, cpu_percent) = sampler.rates(Counters {
                    at: Instant::now(),
                    frames,
                    runs,
                    cpu_ticks: read("/proc/self/stat").and_then(|s| cpu_ticks(&s)),
                });
                status.set_stats(Stats {
                    capture_fps,
                    model_runs_per_s,
                    model_ms: status.model_ms().map(|v| round(v, 1)),
                    cpu_percent,
                    cores: thread::available_parallelism().map_or(1, |n| n.get()),
                    rss_mb: read("/proc/self/status")
                        .and_then(|s| rss_kb(&s))
                        .map(|kb| round(kb as f64 / 1024.0, 1)),
                    load_1m: read("/proc/loadavg").and_then(|s| load_1m(&s)),
                    temperature_c: read("/sys/class/thermal/thermal_zone0/temp")
                        .and_then(|s| millidegrees(&s))
                        .map(|c| round(c, 1)),
                    uptime_s: started.elapsed().as_secs(),
                });
                thread::sleep(PERIOD);
            }
        });
    if let Err(err) = spawned {
        log::warn!("no stats sampler: {err}");
    }
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn round(value: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    (value * scale).round() / scale
}

/// User plus system time from `/proc/<pid>/stat`; the name in parentheses may hold spaces.
fn cpu_ticks(stat: &str) -> Option<u64> {
    let fields: Vec<&str> = stat[stat.rfind(')')? + 1..].split_whitespace().collect();
    // Field 3, the state, is the first after the name: utime is field 14, stime 15.
    Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
}

fn rss_kb(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn load_1m(loadavg: &str) -> Option<f64> {
    loadavg.split_whitespace().next()?.parse().ok()
}

fn millidegrees(temp: &str) -> Option<f64> {
    temp.trim().parse::<f64>().ok().map(|m| m / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sampler_fills_the_stats_at_once() {
        let status = Arc::new(Status::new("Cam", true, 4, 4));
        spawn(status.clone());
        let stats = |s: &Status| {
            serde_json::from_str::<serde_json::Value>(&s.to_json()).unwrap()["stats"].clone()
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while stats(&status)["cores"] == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        let first = stats(&status);
        assert!(first["cores"].as_u64().is_some_and(|n| n >= 1), "{first}");
        // Where /proc exists, the process's own memory is read from it.
        #[cfg(target_os = "linux")]
        assert!(
            first["rss_mb"]
                .as_f64()
                .is_some_and(|mb| (1.0..100_000.0).contains(&mb)),
            "{first}"
        );
    }

    #[test]
    fn the_proc_files_read_as_the_kernel_writes_them() {
        // A name with a space and a parenthesis, as a thread may carry.
        let stat = "4242 (cat (webrtc) x) S 1 4242 4242 0 -1 4194560 3045 0 0 0 1234 567 0 0 20 0 9 0 99 1000 2000";
        assert_eq!(cpu_ticks(stat), Some(1234 + 567));
        assert_eq!(cpu_ticks("garbage"), None);
        let status =
            "Name:\tkataglyphis_cat\nVmPeak:\t  300000 kB\nVmRSS:\t  225432 kB\nThreads:\t9\n";
        assert_eq!(rss_kb(status), Some(225_432));
        assert_eq!(rss_kb("Name:\tx\n"), None);
        assert_eq!(load_1m("0.13 0.20 0.25 1/412 3001\n"), Some(0.13));
        assert_eq!(millidegrees("51250\n"), Some(51.25));
        assert_eq!(millidegrees(""), None);
    }

    #[test]
    fn two_looks_give_the_rates_between_them() {
        let t = Instant::now();
        let mut sampler = Sampler::default();
        let first = Counters {
            at: t,
            frames: 1000,
            runs: 40,
            cpu_ticks: Some(500),
        };
        assert_eq!(sampler.rates(first), (None, None, None));
        let second = Counters {
            at: t + Duration::from_secs(2),
            frames: 1060,
            runs: 41,
            cpu_ticks: Some(520),
        };
        // 60 frames and one run in 2 s; 20 ticks in 2 s are a tenth of a core.
        assert_eq!(sampler.rates(second), (Some(30.0), Some(0.5), Some(10.0)));
        let unreadable = Counters {
            at: t + Duration::from_secs(4),
            frames: 1120,
            runs: 41,
            cpu_ticks: None,
        };
        assert_eq!(sampler.rates(unreadable), (Some(30.0), Some(0.0), None));
    }
}
