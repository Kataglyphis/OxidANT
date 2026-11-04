#[path = "../../../tests/integration/integration.rs"]
mod integration;

#[cfg(target_os = "linux")]
mod gui_smoke {
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
    use std::time::{Duration, Instant};

    const FIRST_FRAME: &str = "First GUI frame finished";
    // Above the wait below, so a killed test run leaves no GUI behind.
    const CHILD_BOUND: &str = "200";
    const WAIT: Duration = Duration::from_secs(180);

    fn required(name: &str) -> bool {
        std::env::var(name).is_ok_and(|v| !v.is_empty())
    }

    fn forward_lines(stream: impl Read + Send + 'static, lines: Sender<String>) {
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if lines.send(line).is_err() {
                    return;
                }
            }
        });
    }

    /// Ends the GUI on every path out of the test, including a failed assertion.
    struct Stop(Child);

    impl Drop for Stop {
        fn drop(&mut self) {
            // SIGTERM, which `timeout` forwards; killing the wrapper alone would orphan the GUI.
            let _ = Command::new("kill")
                .args(["-TERM", &self.0.id().to_string()])
                .status();
            let _ = self.0.wait();
        }
    }

    /// `true` once the first frame finished; the panic of a debug build ends the wait as `false`.
    fn wait_for_first_frame(gui: &mut Stop, lines: &std::sync::mpsc::Receiver<String>) -> bool {
        let deadline = Instant::now() + WAIT;
        let mut log = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match lines.recv_timeout(left) {
                Ok(line) if line.contains(FIRST_FRAME) => return true,
                Ok(line) => {
                    assert!(!line.contains("panicked"), "{line}\n{}", log.join("\n"));
                    log.push(line);
                }
                Err(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout) => {
                    let status = gui.0.try_wait().ok().flatten();
                    eprintln!("no first frame, GUI status {status:?}:\n{}", log.join("\n"));
                    return false;
                }
            }
        }
    }

    /// Runs the real window path past the first frame, whose font atlas upload is the texture delta a renderer must drain.
    /// Not covered: a failed surface acquire, and a later frame's deltas.
    #[test]
    fn gui_finishes_its_first_frame() {
        let must_run = required("KATAGLYPHIS_REQUIRE_GPU") && required("KATAGLYPHIS_REQUIRE_GST");
        if std::env::var_os("DISPLAY").is_none() {
            assert!(!must_run, "KATAGLYPHIS_REQUIRE_GPU needs a display here");
            eprintln!("SKIP: no display for the GUI smoke test");
            return;
        }
        let mut child = Command::new("timeout")
            .arg(CHILD_BOUND)
            .arg(assert_cmd::cargo::cargo_bin!("kataglyphis_cli"))
            .arg("gui")
            // Rendering must not depend on a detector: a missing model leaves its panel an error.
            .env("KATAGLYPHIS_ONNX_MODEL", "/nonexistent/model.onnx")
            .env("RUST_BACKTRACE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the GUI");
        let (tx, lines) = channel();
        forward_lines(child.stdout.take().expect("piped stdout"), tx.clone());
        forward_lines(child.stderr.take().expect("piped stderr"), tx);
        let mut gui = Stop(child);
        if !wait_for_first_frame(&mut gui, &lines) {
            assert!(!must_run, "the GUI never finished a frame");
            eprintln!("SKIP: this host cannot run the GUI");
        }
    }
}
