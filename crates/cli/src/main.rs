// src/main.rs — Thin CLI driver.  All logic lives in the library crate.

/// Floor for the resource logging interval in milliseconds, against excessive logging overhead.
const MIN_RESOURCE_LOG_INTERVAL_MS: u64 = 100;
use anyhow::Result;
use clap::Parser;
use log::info;

mod cli;
mod text;

use cli::{Cli, Commands};
use oxidant::{logging, resource_monitor, utils};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    logging::init_logger();

    let _resource_monitor = if cli.resource_log {
        Some(resource_monitor::ResourceMonitor::start(
            resource_monitor::ResourceMonitorConfig {
                interval: std::time::Duration::from_millis(
                    cli.resource_log_interval_ms
                        .max(MIN_RESOURCE_LOG_INTERVAL_MS),
                ),
                log_file: cli.resource_log_file.clone(),
                include_gpu: cli.resource_log_gpu,
            },
        ))
    } else {
        None
    };

    match cli.command {
        Commands::Read { path } => {
            info!("Reading file: {}", path.display());
            let content = utils::read_file(&path).await?;
            println!("{}", content);
        }
        Commands::Stats { path } => {
            let stats = utils::file_stats(&path).await?;
            println!(
                "Lines: {}, Words: {}, Bytes: {}",
                stats.lines, stats.words, stats.bytes
            );
        }
        Commands::Gui { backend } => {
            kataglyphis_gui::gui_wgpu::run_with_backend(&backend)?;
        }
        #[cfg(target_os = "linux")]
        Commands::GuiGtk => {
            let code = kataglyphis_gui::gui::run()?;
            anyhow::ensure!(code == 0.into(), "GTK application exited with {code:?}");
        }
        #[cfg(any(target_os = "linux", windows))]
        Commands::OnnxRuntime => {
            let path = oxidant::ort_runtime::ensure_ort_loaded()?;
            println!("ONNX Runtime: {}", path.display());
        }
        #[cfg(any(target_os = "linux", windows))]
        Commands::OnnxCheck { profile } => {
            let model = oxidant::person_detection::resolve_model_path(None);
            let mut detector = match profile {
                Some(ref prefix) => {
                    oxidant::person_detection::PersonDetector::with_ort_profiling(&model, prefix)?
                }
                None => {
                    oxidant::person_detection::PersonDetector::with_backend(&model, Some("ort"))?
                }
            };
            let detections = detector.infer_rgba(&[0; 64 * 64 * 4], 64, 64, 0.5, None)?;
            println!("ONNX inference completed ({} detections)", detections.len());
            if profile.is_some() {
                println!("ONNX profile: {}", detector.end_ort_profiling()?);
            }
        }
        Commands::MediaCheck => {
            print!("{}", kataglyphis_gui::gui_wgpu::media_check()?);
        }
        Commands::Chat(args) => text::chat(args)?,
        Commands::Embed { texts, endpoint } => text::embed(texts, endpoint)?,
    }
    Ok(())
}
