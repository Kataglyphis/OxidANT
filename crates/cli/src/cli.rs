// src/cli.rs — Command-line interface definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use kataglyphis_gui::gui_wgpu::GpuBackend;

#[derive(Parser)]
#[command(name = "kataglyphis")]
#[command(
    about = "The OxidANT CLI: file tools, the wgpu camera GUI with ONNX inference, language models, and resource monitoring.",
    long_about = None
)]
pub struct Cli {
    /// Enable lightweight periodic resource logging (CPU/RAM and best-effort GPU on Windows).
    #[arg(long, global = true, default_value_t = false)]
    pub resource_log: bool,

    /// Resource log interval in milliseconds.
    #[arg(long, global = true, default_value_t = 1000)]
    pub resource_log_interval_ms: u64,

    /// Optional file path to append resource log lines to (in addition to standard logging).
    #[arg(long, global = true)]
    pub resource_log_file: Option<PathBuf>,

    /// Include GPU metrics in resource logging (best-effort; Windows only).
    #[arg(
        long,
        global = true,
        default_value_t = true,
        value_parser = clap::value_parser!(bool),
        action = clap::ArgAction::Set
    )]
    pub resource_log_gpu: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Read the contents of a file and print them to stdout.
    Read {
        #[arg(short, long)]
        path: PathBuf,
    },
    /// Print line / word / byte statistics for a file.
    Stats {
        #[arg(short, long)]
        path: PathBuf,
    },
    /// Launch the wgpu camera GUI with ONNX detection.
    Gui {
        /// WGPU backend to use (dx12 is Windows-only).
        #[arg(long, value_enum, default_value_t = GpuBackend::Auto)]
        backend: GpuBackend,
    },
    /// Launch the GTK4 camera GUI.
    #[cfg(target_os = "linux")]
    GuiGtk,
    /// Load the chain-built ONNX Runtime and print where it was found; fails when none loads.
    #[cfg(any(target_os = "linux", windows))]
    OnnxRuntime,
    /// Build the GUI's camera pipeline without starting it and print each element's plugin file.
    MediaCheck,
    /// Answer a prompt with a GenAI model directory or an OpenAI-compatible endpoint; the reply streams to stdout.
    Chat(ChatArgs),
    /// Print one L2-normalised vector per text from an OpenAI-compatible endpoint, one JSON array per line.
    Embed {
        #[arg(required = true)]
        texts: Vec<String>,
        #[command(flatten)]
        endpoint: EndpointArgs,
    },
}

#[derive(Args)]
pub struct ChatArgs {
    /// The user's message.
    #[arg(long)]
    pub prompt: String,
    /// A system message before it.
    #[arg(long)]
    pub system: Option<String>,
    /// A GenAI model directory, run in this process on the chain's ONNX Runtime GenAI (Linux and Windows).
    #[arg(long, conflicts_with_all = ["base_url", "backend"])]
    pub model_dir: Option<PathBuf>,
    /// The execution provider for --model-dir.
    #[arg(long, value_enum, default_value_t = TextProvider::Cpu)]
    pub provider: TextProvider,
    #[command(flatten)]
    pub endpoint: EndpointArgs,
    #[arg(long, default_value_t = 128)]
    pub max_new_tokens: u32,
    /// 0 decodes greedily.
    #[arg(long, default_value_t = 0.0)]
    pub temperature: f64,
}

/// The endpoint: --base-url, else LLM_BASE_URL or OLLAMA_BASE_URL, else --backend or the default in backends.json.
#[derive(Args, Clone)]
pub struct EndpointArgs {
    #[arg(long)]
    pub base_url: Option<String>,
    /// A backend named in backends.json.
    #[arg(long)]
    pub backend: Option<String>,
    /// The backends.json to read; LLM_BACKENDS when absent.
    #[arg(long)]
    pub backends: Option<PathBuf>,
    /// Overrides the backend's model.
    #[arg(long)]
    pub model: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum TextProvider {
    Cpu,
    Cuda,
    Dml,
}
