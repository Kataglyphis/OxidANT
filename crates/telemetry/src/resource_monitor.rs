// src/resource_monitor.rs — Lightweight background resource monitor.

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use log::info;
use sysinfo::{Pid, ProcessesToUpdate, System};

const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;

#[cfg(target_os = "windows")]
use crate::gpu_wmi::{gpu_connect, gpu_sample_wmi};

#[derive(Clone, Debug, Default)]
pub(crate) struct GpuSample {
    pub utilization_pct: Option<f64>,
    pub dedicated_used_bytes: Option<u64>,
    pub shared_used_bytes: Option<u64>,
    pub total_committed_bytes: Option<u64>,
}

/// Global counters singleton for tracking inference and camera metrics.
static GLOBAL_COUNTERS: GlobalCounters = GlobalCounters {
    inference_completions: AtomicU64::new(0),
    camera_frames: AtomicU64::new(0),
    inference_time_ns_total: AtomicU64::new(0),
    inference_time_samples: AtomicU64::new(0),
};

struct GlobalCounters {
    inference_completions: AtomicU64,
    camera_frames: AtomicU64,
    inference_time_ns_total: AtomicU64,
    inference_time_samples: AtomicU64,
}

impl GlobalCounters {
    #[inline]
    fn record_inference_completion(&self) {
        self.inference_completions.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_camera_frame(&self) {
        self.camera_frames.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_inference_duration(&self, duration: Duration) {
        let ns = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.inference_time_ns_total
            .fetch_add(ns, Ordering::Relaxed);
        self.inference_time_samples.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> CounterValues {
        CounterValues {
            camera_frames: self.camera_frames.load(Ordering::Relaxed),
            inference_completions: self.inference_completions.load(Ordering::Relaxed),
            inference_time_ns_total: self.inference_time_ns_total.load(Ordering::Relaxed),
            inference_time_samples: self.inference_time_samples.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CounterValues {
    camera_frames: u64,
    inference_completions: u64,
    inference_time_ns_total: u64,
    inference_time_samples: u64,
}

#[derive(Clone, Debug)]
pub struct ResourceMonitorConfig {
    pub interval: Duration,
    pub log_file: Option<PathBuf>,
    pub include_gpu: bool,
}

#[inline]
#[cfg_attr(not(gui_wgpu_backend), allow(dead_code))]
pub fn record_inference_completion() {
    GLOBAL_COUNTERS.record_inference_completion();
}

#[inline]
#[cfg_attr(not(gui_wgpu_backend), allow(dead_code))]
pub fn record_camera_frame() {
    GLOBAL_COUNTERS.record_camera_frame();
}

#[inline]
#[cfg_attr(not(gui_wgpu_backend), allow(dead_code))]
pub fn record_inference_duration(duration: Duration) {
    GLOBAL_COUNTERS.record_inference_duration(duration);
}

#[inline]
pub fn bytes_to_mib(bytes: u64) -> f64 {
    (bytes as f64) / BYTES_PER_MIB
}

pub struct ResourceMonitor {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ResourceMonitor {
    pub fn start(config: ResourceMonitorConfig) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let handle = match thread::Builder::new()
            .name("resource-monitor".to_string())
            .spawn(move || run_monitor_loop(config, stop_thread))
        {
            Ok(h) => Some(h),
            Err(e) => {
                log::warn!("Failed to spawn resource-monitor thread: {e}");
                None
            }
        };

        Self { stop, handle }
    }
}

impl Drop for ResourceMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            if let Err(e) = handle.join() {
                log::warn!("Resource monitor thread panicked: {e:?}");
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub cam_fps: f64,
    pub infer_fps: f64,
    pub infer_latency_ms: f64,
    pub infer_capacity_fps: f64,
}

pub struct CounterSnapshot {
    at: Instant,
    values: CounterValues,
}

impl Default for CounterSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

impl CounterSnapshot {
    pub fn new() -> Self {
        Self {
            at: Instant::now(),
            values: GLOBAL_COUNTERS.snapshot(),
        }
    }

    pub fn tick(&mut self, now: Instant) -> Rates {
        let dt_s = now.duration_since(self.at).as_secs_f64().max(0.001);
        self.at = now;

        let new_values = GLOBAL_COUNTERS.snapshot();

        let cam_fps = new_values
            .camera_frames
            .wrapping_sub(self.values.camera_frames) as f64
            / dt_s;
        let infer_fps = new_values
            .inference_completions
            .wrapping_sub(self.values.inference_completions) as f64
            / dt_s;

        let ns_delta = new_values
            .inference_time_ns_total
            .wrapping_sub(self.values.inference_time_ns_total);
        let samples_delta = new_values
            .inference_time_samples
            .wrapping_sub(self.values.inference_time_samples);

        self.values = new_values;

        let infer_latency_ms = if samples_delta > 0 {
            (ns_delta as f64 / samples_delta as f64) / 1_000_000.0
        } else {
            0.0
        };
        let infer_capacity_fps = if infer_latency_ms > 0.0 {
            1000.0 / infer_latency_ms
        } else {
            0.0
        };

        Rates {
            cam_fps,
            infer_fps,
            infer_latency_ms,
            infer_capacity_fps,
        }
    }
}

fn run_monitor_loop(config: ResourceMonitorConfig, stop: Arc<AtomicBool>) {
    let pid = Pid::from_u32(std::process::id());
    let mut sys = System::new();

    let mut file_writer = config
        .log_file
        .as_ref()
        .and_then(|path| OpenOptions::new().create(true).append(true).open(path).ok())
        .map(BufWriter::new);

    #[cfg(target_os = "windows")]
    let wmi_conn = if config.include_gpu {
        gpu_connect()
    } else {
        None
    };

    let mut next_tick = Instant::now();
    let mut counters = CounterSnapshot::new();

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let sample_instant = Instant::now();
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        sys.refresh_memory();

        let (proc_cpu_pct, proc_rss_bytes) = sys
            .process(pid)
            .map(|p| (p.cpu_usage() as f64, p.memory()))
            .unwrap_or((0.0, 0));

        let sys_total_bytes = sys.total_memory();
        let sys_used_bytes = sys.used_memory();

        let gpu = if config.include_gpu {
            #[cfg(target_os = "windows")]
            {
                wmi_conn.as_ref().and_then(gpu_sample_wmi)
            }
            #[cfg(not(target_os = "windows"))]
            {
                None
            }
        } else {
            None
        };

        let rates = counters.tick(sample_instant);

        let mut line = format!(
            "resource cpu={cpu:.1}% rss={rss:.1}MiB sys_used={sys_used:.1}MiB \
             sys_total={sys_total:.1}MiB cam_fps={cam_fps:.2} infer_fps={infer_fps:.2} \
             infer_ms={infer_ms:.2} infer_capacity_fps={infer_cap:.2}",
            cpu = proc_cpu_pct,
            rss = bytes_to_mib(proc_rss_bytes),
            sys_used = bytes_to_mib(sys_used_bytes),
            sys_total = bytes_to_mib(sys_total_bytes),
            cam_fps = rates.cam_fps,
            infer_fps = rates.infer_fps,
            infer_ms = rates.infer_latency_ms,
            infer_cap = rates.infer_capacity_fps,
        );

        if let Some(sample) = gpu {
            append_gpu_metrics(&mut line, &sample);
        }

        info!("{line}");
        write_line(&mut file_writer, &line);

        next_tick += config.interval;
        let sleep_for = next_tick.saturating_duration_since(Instant::now());
        thread::sleep(sleep_for);
    }

    if let Some(mut w) = file_writer {
        if let Err(e) = w.flush() {
            log::warn!("Failed to flush resource log file: {e}");
        }
    }
}

fn write_line(writer: &mut Option<BufWriter<std::fs::File>>, line: &str) {
    let Some(w) = writer.as_mut() else {
        return;
    };
    if let Err(e) = writeln!(w, "{line}") {
        log::warn!("Failed to write resource log line: {e}");
    }
}

fn append_gpu_metrics(line: &mut String, sample: &GpuSample) {
    use std::fmt::Write;
    if let Some(util) = sample.utilization_pct {
        let _ = write!(line, " gpu_util={util:.1}%");
    }
    if let Some(dedicated) = sample.dedicated_used_bytes {
        let _ = write!(line, " gpu_dedicated={:.1}MiB", bytes_to_mib(dedicated));
    }
    if let Some(shared) = sample.shared_used_bytes {
        let _ = write!(line, " gpu_shared={:.1}MiB", bytes_to_mib(shared));
    }
    if let Some(total) = sample.total_committed_bytes {
        let _ = write!(line, " gpu_total={:.1}MiB", bytes_to_mib(total));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_to_mib() {
        assert!((bytes_to_mib(1024 * 1024) - 1.0).abs() < 0.001);
        assert!((bytes_to_mib(512 * 1024 * 1024) - 512.0).abs() < 0.001);
        assert!((bytes_to_mib(0) - 0.0).abs() < 0.001);
    }

    // The only test that records: the counters are process-wide, so a second one would race it.
    #[test]
    fn tick_turns_the_counters_into_rates_over_the_elapsed_time() {
        let mut snapshot = CounterSnapshot::new();
        let start = snapshot.at;
        for _ in 0..30 {
            record_camera_frame();
        }
        for _ in 0..10 {
            record_inference_completion();
            record_inference_duration(Duration::from_millis(25));
        }
        let rates = snapshot.tick(start + Duration::from_secs(2));
        assert_eq!(rates.cam_fps, 15.0);
        assert_eq!(rates.infer_fps, 5.0);
        assert!((rates.infer_latency_ms - 25.0).abs() < 1e-9, "{rates:?}");
        assert!((rates.infer_capacity_fps - 40.0).abs() < 1e-9, "{rates:?}");

        // Nothing since: every rate falls to zero rather than repeating the last one.
        let idle = snapshot.tick(start + Duration::from_secs(3));
        assert_eq!(
            idle,
            Rates {
                cam_fps: 0.0,
                infer_fps: 0.0,
                infer_latency_ms: 0.0,
                infer_capacity_fps: 0.0
            }
        );
    }

    #[test]
    fn gpu_metrics_name_only_what_the_sample_has() {
        let mut line = String::from("resource");
        append_gpu_metrics(&mut line, &GpuSample::default());
        assert_eq!(line, "resource");

        append_gpu_metrics(
            &mut line,
            &GpuSample {
                utilization_pct: Some(42.25),
                dedicated_used_bytes: Some(512 * 1024 * 1024),
                shared_used_bytes: None,
                total_committed_bytes: Some(1024 * 1024 * 1024),
            },
        );
        assert_eq!(
            line,
            "resource gpu_util=42.2% gpu_dedicated=512.0MiB gpu_total=1024.0MiB"
        );
    }

    #[test]
    fn write_line_appends_a_line_and_without_a_file_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("resources.log");
        let mut writer = Some(BufWriter::new(std::fs::File::create(&path).unwrap()));
        write_line(&mut writer, "first");
        write_line(&mut writer, "second");
        drop(writer);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\nsecond\n");

        write_line(&mut None, "nowhere");
    }

    #[test]
    fn the_monitor_logs_every_field_and_stops_when_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("resources.log");
        let monitor = ResourceMonitor::start(ResourceMonitorConfig {
            interval: Duration::from_millis(20),
            log_file: Some(path.clone()),
            include_gpu: false,
        });
        std::thread::sleep(Duration::from_millis(150));
        let started = Instant::now();
        drop(monitor);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "Drop did not stop the thread"
        );

        let log = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert!(lines.len() >= 2, "{log:?}");
        for field in [
            "cpu=",
            "rss=",
            "sys_used=",
            "sys_total=",
            "cam_fps=",
            "infer_fps=",
            "infer_ms=",
            "infer_capacity_fps=",
        ] {
            assert!(lines[0].contains(field), "no {field} in {:?}", lines[0]);
        }
        assert!(lines[0].starts_with("resource "), "{:?}", lines[0]);
        assert!(
            !lines[0].contains("gpu_"),
            "GPU fields without include_gpu: {:?}",
            lines[0]
        );
    }
}
