//! `--precompress DIR`: zopfli `.gz` copies of a web build, made once at package time, so the
//! server sends smaller files than its own gzip and neither compresses nor caches them at run time.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context as _;

use crate::static_files::{compressible, content_type, MIN_GZIP_BYTES};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Done {
    pub files: usize,
    pub raw_bytes: u64,
    pub gz_bytes: u64,
}

/// Writes `<file>.gz` beside every file under `dir` that the server would send gzipped.
pub fn precompress(dir: &Path) -> anyhow::Result<Done> {
    let mut work = Vec::new();
    collect(dir, &mut work).with_context(|| format!("reading {}", dir.display()))?;
    // Ascending, so popping hands out the biggest first and no long run starts last.
    work.sort_unstable();
    let queue = Mutex::new(work);
    let done = Mutex::new(Done::default());
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| scope.spawn(|| drain(&queue, &done)))
            .collect();
        workers.into_iter().try_for_each(|w| {
            w.join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("a worker panicked")))
        })
    })?;
    Ok(done.into_inner().unwrap_or_default())
}

fn drain(queue: &Mutex<Vec<(u64, PathBuf)>>, done: &Mutex<Done>) -> anyhow::Result<()> {
    loop {
        let Some((len, file)) = queue.lock().ok().and_then(|mut q| q.pop()) else {
            return Ok(());
        };
        let raw = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
        let mut gz = Vec::with_capacity(raw.len() / 3);
        zopfli::compress(
            zopfli::Options::default(),
            zopfli::Format::Gzip,
            raw.as_slice(),
            &mut gz,
        )?;
        let mut target = file.into_os_string();
        target.push(".gz");
        std::fs::write(&target, &gz)
            .with_context(|| format!("writing {}", Path::new(&target).display()))?;
        if let Ok(mut done) = done.lock() {
            done.files += 1;
            done.raw_bytes += len;
            done.gz_bytes += gz.len() as u64;
        }
    }
}

fn collect(dir: &Path, work: &mut Vec<(u64, PathBuf)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let path = entry.path();
        if meta.is_dir() {
            collect(&path, work)?;
        } else if meta.len() >= MIN_GZIP_BYTES && compressible(content_type(&path)) {
            work.push((meta.len(), path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    fn gunzip(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(data)
            .read_to_end(&mut out)
            .unwrap();
        out
    }

    #[test]
    fn every_file_the_server_gzips_gets_a_gz_that_unzips_to_it() {
        let web = tempfile::tempdir().unwrap();
        let script = "let cat = 'Thundy';\n".repeat(400);
        std::fs::write(web.path().join("main.dart.js"), &script).unwrap();
        std::fs::create_dir_all(web.path().join("canvaskit")).unwrap();
        let wasm = [0u8, 0x61, 0x73, 0x6d].repeat(600);
        std::fs::write(web.path().join("canvaskit/skwasm.wasm"), &wasm).unwrap();
        std::fs::write(web.path().join("index.html"), "<p>tiny</p>").unwrap();
        std::fs::write(web.path().join("cat.png"), vec![7u8; 4096]).unwrap();

        let done = precompress(web.path()).unwrap();

        assert_eq!(done.files, 2);
        assert_eq!(done.raw_bytes, (script.len() + wasm.len()) as u64);
        let js = std::fs::read(web.path().join("main.dart.js.gz")).unwrap();
        assert_eq!(gunzip(&js), script.as_bytes());
        let nested = std::fs::read(web.path().join("canvaskit/skwasm.wasm.gz")).unwrap();
        assert_eq!(gunzip(&nested), wasm);
        // Under 1 KiB, and an image: the server sends both as they are.
        assert!(!web.path().join("index.html.gz").exists());
        assert!(!web.path().join("cat.png.gz").exists());
    }

    #[test]
    fn a_second_run_rewrites_rather_than_compressing_its_own_output() {
        let web = tempfile::tempdir().unwrap();
        std::fs::write(web.path().join("app.js"), "1;".repeat(2000)).unwrap();
        precompress(web.path()).unwrap();
        let again = precompress(web.path()).unwrap();
        assert_eq!(again.files, 1);
        assert!(!web.path().join("app.js.gz.gz").exists());
    }
}
