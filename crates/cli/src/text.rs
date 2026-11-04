// src/text.rs — `chat` and `embed`: the text crate's backends on the command line.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use oxidant::text::endpoint::EndpointChoice;
use oxidant::text::{CancelToken, ChatMessage, GenerationOptions, GenerationResult, TextBackend};

use crate::cli::{ChatArgs, EndpointArgs, TextProvider};

impl EndpointArgs {
    fn choice(&self) -> EndpointChoice {
        EndpointChoice {
            base_url: self.base_url.clone(),
            backend: self.backend.clone(),
            registry: self.backends.clone(),
            model: self.model.clone(),
        }
    }
}

/// Streams the reply to stdout as it is generated and the figures to stderr.
pub fn chat(args: ChatArgs) -> Result<()> {
    let mut messages: Vec<ChatMessage> = args
        .system
        .iter()
        .map(|s| ChatMessage::new("system", s))
        .collect();
    messages.push(ChatMessage::user(&args.prompt));
    let options = GenerationOptions {
        max_new_tokens: args.max_new_tokens,
        temperature: args.temperature,
        seed: args.seed,
        json_schema: args.json_schema.clone(),
        stop: args.stop.clone(),
        ..GenerationOptions::default()
    };
    let mut backend = backend(&args)?;
    let mut stdout = std::io::stdout().lock();
    let mut failed_write = None;
    let result = backend.generate(&messages, &options, &CancelToken::default(), &mut |piece| {
        if let Err(e) = stdout
            .write_all(piece.as_bytes())
            .and_then(|()| stdout.flush())
        {
            failed_write.get_or_insert(e);
        }
    })?;
    if let Some(e) = failed_write {
        bail!("stdout: {e}");
    }
    writeln!(stdout)?;
    eprintln!("{}", figures(&result));
    Ok(())
}

fn figures(result: &GenerationResult) -> String {
    format!(
        "{} tokens, {:.1} ms to the first, {:.1} tokens/s, finish {:?}",
        result.completion_tokens,
        result.time_to_first_token_ms,
        result.decode_tokens_per_second,
        result.finish_reason
    )
}

#[cfg(any(target_os = "linux", windows))]
fn local(dir: &Path, provider: TextProvider) -> Result<oxidant::text::genai::GenaiGenerator> {
    use oxidant::text::genai::{GenaiGenerator, Provider};
    let provider = match provider {
        TextProvider::Cpu => Provider::Cpu,
        TextProvider::Cuda => Provider::Cuda,
        TextProvider::Dml => Provider::DirectMl,
    };
    Ok(GenaiGenerator::load(dir, provider)?)
}

#[cfg(any(target_os = "linux", windows))]
fn backend(args: &ChatArgs) -> Result<Box<dyn TextBackend>> {
    let Some(dir) = &args.model_dir else {
        return Ok(Box::new(args.endpoint.choice().resolve()?));
    };
    Ok(Box::new(local(dir, args.provider)?))
}

#[cfg(any(target_os = "linux", windows))]
fn local_vectors(dir: &Path, provider: TextProvider, texts: &[String]) -> Result<Vec<Vec<f32>>> {
    Ok(local(dir, provider)?.embed(texts)?)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn local_vectors(_: &Path, _: TextProvider, _: &[String]) -> Result<Vec<Vec<f32>>> {
    bail!(
        "--model-dir needs the chain's ONNX Runtime GenAI, which exists for Linux and Windows only"
    );
}

#[cfg(not(any(target_os = "linux", windows)))]
fn backend(args: &ChatArgs) -> Result<Box<dyn TextBackend>> {
    if args.model_dir.is_some() {
        bail!("--model-dir needs the chain's ONNX Runtime GenAI, which exists for Linux and Windows only");
    }
    Ok(Box::new(args.endpoint.choice().resolve()?))
}

/// One JSON array per text, in the order given.
pub fn embed(
    texts: Vec<String>,
    model_dir: Option<PathBuf>,
    provider: TextProvider,
    endpoint: EndpointArgs,
) -> Result<()> {
    let vectors = match model_dir {
        Some(dir) => local_vectors(&dir, provider, &texts)?,
        None => endpoint.choice().resolve()?.embed(&texts)?,
    };
    let mut stdout = std::io::stdout().lock();
    for vector in vectors {
        writeln!(stdout, "{}", serde_json::to_string(&vector)?)?;
    }
    Ok(())
}
