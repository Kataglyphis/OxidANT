//! The one running session of an API that Dart stops from its UI thread.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::thread::JoinHandle;

use anyhow::Result;

/// A session whose drop waits for its threads and devices.
pub(crate) trait Session: Send + 'static {
    /// Whether it ended by itself.
    fn is_finished(&self) -> bool;
    /// Asks it to end without waiting for that.
    fn request_stop(&self);
}

pub(crate) struct SessionSlot<S> {
    current: Mutex<Option<S>>,
    /// The thread that drops the session before; a start waits for it, since the camera is free only then.
    retiring: Mutex<Option<JoinHandle<()>>>,
    stops: AtomicU64,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl<S: Session> SessionSlot<S> {
    pub(crate) const fn new() -> Self {
        Self {
            current: Mutex::new(None),
            retiring: Mutex::new(None),
            stops: AtomicU64::new(0),
        }
    }

    /// Opens a session unless one runs; a stop that arrives while `open` works ends the new session at once.
    pub(crate) fn start(&self, open: impl FnOnce() -> Result<S>) -> Result<()> {
        let stops = self.stops.load(Ordering::Acquire);
        let mut current = locked(&self.current);
        // A session whose camera or model gave up has ended; only a live one is in the way.
        if current
            .as_ref()
            .is_some_and(|session| !session.is_finished())
        {
            anyhow::bail!("webcam inference is already running; stop it first");
        }
        *current = None;
        let retired = locked(&self.retiring).take();
        if let Some(retired) = retired {
            let _ = retired.join();
        }
        let session = open()?;
        // A stop cannot wait for this lock, so it counted itself instead.
        if self.stops.load(Ordering::Acquire) == stops {
            *current = Some(session);
        } else {
            self.retire(session);
        }
        Ok(())
    }

    /// Never waits: for a model that loads, an inference that runs or a camera driver that closes.
    pub(crate) fn stop(&self) {
        self.stops.fetch_add(1, Ordering::AcqRel);
        let session = match self.current.try_lock() {
            Ok(mut current) => current.take(),
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().take(),
            Err(TryLockError::WouldBlock) => None,
        };
        if let Some(session) = session {
            self.retire(session);
        }
    }

    fn retire(&self, session: S) {
        session.request_stop();
        let mut retiring = locked(&self.retiring);
        let before = retiring.take();
        // Without a thread the closure, and the session in it, is dropped here: the wait this avoids, but no leak.
        *retiring = std::thread::Builder::new()
            .name("session-retire".into())
            .spawn(move || {
                if let Some(before) = before {
                    let _ = before.join();
                }
                drop(session);
            })
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::*;

    /// A session whose drop waits until the test lets it go, as a camera driver that closes slowly.
    struct Slow {
        asked: Arc<AtomicBool>,
        finished: bool,
        release: mpsc::Receiver<()>,
        dropped: mpsc::Sender<&'static str>,
    }

    impl Session for Slow {
        fn is_finished(&self) -> bool {
            self.finished
        }

        fn request_stop(&self) {
            self.asked.store(true, Ordering::Release);
        }
    }

    impl Drop for Slow {
        fn drop(&mut self) {
            let _ = self.release.recv_timeout(Duration::from_secs(10));
            let _ = self.dropped.send("dropped");
        }
    }

    struct Handles {
        asked: Arc<AtomicBool>,
        release: mpsc::Sender<()>,
        dropped: mpsc::Receiver<&'static str>,
    }

    fn slow(finished: bool) -> (Slow, Handles) {
        let asked = Arc::new(AtomicBool::new(false));
        let (release, held) = mpsc::channel();
        let (dropped, seen) = mpsc::channel();
        let session = Slow {
            asked: asked.clone(),
            finished,
            release: held,
            dropped,
        };
        let handles = Handles {
            asked,
            release,
            dropped: seen,
        };
        (session, handles)
    }

    #[test]
    fn stop_asks_the_session_to_end_and_does_not_wait_for_it() {
        let slot = SessionSlot::new();
        let (session, handles) = slow(false);
        slot.start(|| Ok(session)).unwrap();
        let started = Instant::now();
        slot.stop();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(handles.asked.load(Ordering::Acquire));
        assert!(
            handles.dropped.try_recv().is_err(),
            "the drop was waited for"
        );
        handles.release.send(()).unwrap();
        assert_eq!(
            handles.dropped.recv_timeout(Duration::from_secs(10)),
            Ok("dropped")
        );
        // An idle stop is a no-op.
        slot.stop();
    }

    #[test]
    fn a_start_waits_until_the_session_before_let_go_of_the_camera() {
        let slot = Arc::new(SessionSlot::new());
        let (first, first_handles) = slow(false);
        slot.start(|| Ok(first)).unwrap();
        slot.stop();
        let (second, second_handles) = slow(false);
        let (opened, seen) = mpsc::channel();
        let starter = {
            let slot = slot.clone();
            std::thread::spawn(move || {
                slot.start(|| {
                    opened.send(()).unwrap();
                    Ok(second)
                })
            })
        };
        assert!(
            seen.recv_timeout(Duration::from_millis(200)).is_err(),
            "the camera was opened while the session before still held it"
        );
        first_handles.release.send(()).unwrap();
        seen.recv_timeout(Duration::from_secs(10)).unwrap();
        starter.join().unwrap().unwrap();
        slot.stop();
        second_handles.release.send(()).unwrap();
    }

    #[test]
    fn a_second_start_is_refused_while_a_session_runs_and_taken_once_it_ended() {
        let slot = SessionSlot::new();
        let (running, running_handles) = slow(false);
        slot.start(|| Ok(running)).unwrap();
        let refused = slot.start(|| -> Result<Slow> { panic!("opened beside a running session") });
        assert!(refused.unwrap_err().to_string().contains("already running"));
        slot.stop();
        running_handles.release.send(()).unwrap();

        let (ended, ended_handles) = slow(true);
        ended_handles.release.send(()).unwrap();
        slot.start(|| Ok(ended)).unwrap();
        let (next, next_handles) = slow(false);
        next_handles.release.send(()).unwrap();
        slot.start(|| Ok(next)).unwrap();
        assert_eq!(
            ended_handles.dropped.recv_timeout(Duration::from_secs(10)),
            Ok("dropped")
        );
        slot.stop();
    }

    #[test]
    fn a_stop_while_the_model_loads_returns_at_once_and_ends_the_new_session() {
        let slot = Arc::new(SessionSlot::new());
        let (session, handles) = slow(false);
        let (loading, seen) = mpsc::channel();
        let (go, wait) = mpsc::channel::<()>();
        let starter = {
            let slot = slot.clone();
            std::thread::spawn(move || {
                slot.start(|| {
                    loading.send(()).unwrap();
                    wait.recv_timeout(Duration::from_secs(10)).unwrap();
                    Ok(session)
                })
            })
        };
        seen.recv_timeout(Duration::from_secs(10)).unwrap();
        let started = Instant::now();
        slot.stop();
        assert!(started.elapsed() < Duration::from_secs(1));
        go.send(()).unwrap();
        starter.join().unwrap().unwrap();
        assert!(handles.asked.load(Ordering::Acquire));
        handles.release.send(()).unwrap();
        assert_eq!(
            handles.dropped.recv_timeout(Duration::from_secs(10)),
            Ok("dropped")
        );
    }
}
