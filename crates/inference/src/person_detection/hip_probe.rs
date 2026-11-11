//! One copy to the AMD GPU before MIGraphX starts. Under WSL on an RX 9070 XT the first copy never
//! returns, and MIGraphX would wait for it without end: hub BACKLOG CON202. The copy runs in a child
//! process: a hung copy would also stop this process from exiting, as the runtime's teardown waits for it.

use std::ffi::{c_int, c_uint, c_void};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::provider::ProviderUnavailable;

const HIP_RUNTIME: &str = "libamdhip64.so.7";
const ANSWER_LIMIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(50);
const HIP_SUCCESS: c_int = 0;
const HIP_MEMCPY_HOST_TO_DEVICE: c_int = 1;

/// Fails when the GPU does not answer. No HIP runtime or no device passes: MIGraphX reports those.
pub(super) fn require_answer(provider: &'static str) -> Result<()> {
    if answers_within(ANSWER_LIMIT, || copy_to_device(HIP_RUNTIME)) {
        return Ok(());
    }
    Err(no_answer(provider).into())
}

fn no_answer(provider: &'static str) -> ProviderUnavailable {
    ProviderUnavailable {
        provider,
        reason: format!(
            "cannot run: a copy to the AMD GPU took over {} seconds. KATAGLYPHIS_ORT_DEVICE=cpu runs on the CPU",
            ANSWER_LIMIT.as_secs()
        ),
    }
}

/// False only when `work` outlives `limit`: a child that crashes or cannot start has answered nothing
/// about a hang, and MIGraphX reports the rest.
fn answers_within(limit: Duration, work: impl FnOnce()) -> bool {
    // SAFETY: the child only runs `work` and leaves with `_exit`, so it runs none of this process's
    // exit handlers, and the parent reaps the pid fork returned.
    unsafe {
        let child = libc::fork();
        if child == 0 {
            work();
            libc::_exit(0);
        }
        if child < 0 {
            return true;
        }
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            if libc::waitpid(child, std::ptr::null_mut(), libc::WNOHANG) != 0 {
                return true;
            }
            std::thread::sleep(POLL);
        }
        libc::kill(child, libc::SIGKILL);
        // A child stuck in the driver may take its time to die; nothing here waits for that.
        std::thread::spawn(move || libc::waitpid(child, std::ptr::null_mut(), 0));
    }
    false
}

fn copy_to_device(runtime: &str) {
    type Init = unsafe extern "C" fn(c_uint) -> c_int;
    type Malloc = unsafe extern "C" fn(*mut *mut c_void, usize) -> c_int;
    type Memcpy = unsafe extern "C" fn(*mut c_void, *const c_void, usize, c_int) -> c_int;

    // SAFETY: the signatures are those of ROCm's hip_runtime_api.h, and the allocation holds the
    // four bytes copied to it.
    unsafe {
        let Ok(hip) = libloading::Library::new(runtime) else {
            return;
        };
        let (Ok(init), Ok(malloc), Ok(memcpy)) = (
            hip.get::<Init>(b"hipInit\0"),
            hip.get::<Malloc>(b"hipMalloc\0"),
            hip.get::<Memcpy>(b"hipMemcpy\0"),
        ) else {
            return;
        };
        let mut device = std::ptr::null_mut();
        if init(0) == HIP_SUCCESS && malloc(&mut device, 4) == HIP_SUCCESS {
            let value = 0u32;
            let source = (&raw const value).cast::<c_void>();
            let _ = memcpy(device, source, 4, HIP_MEMCPY_HOST_TO_DEVICE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_failure_names_the_provider_and_the_way_out() {
        let message = no_answer("MIGraphXExecutionProvider").to_string();
        assert!(
            message.contains("ProviderUnavailable: MIGraphXExecutionProvider"),
            "{message}"
        );
        assert!(message.contains("30 seconds"), "{message}");
        assert!(message.contains("KATAGLYPHIS_ORT_DEVICE=cpu"), "{message}");
    }

    #[test]
    fn a_missing_runtime_copies_nothing() {
        copy_to_device("libnot-a-hip-runtime.so");
    }

    #[test]
    fn a_library_without_the_hip_symbols_copies_nothing() {
        copy_to_device("libc.so.6");
    }

    #[test]
    fn work_that_finishes_answers() {
        assert!(answers_within(Duration::from_secs(5), || {}));
    }

    #[test]
    fn a_child_that_crashes_counts_as_an_answer() {
        assert!(answers_within(Duration::from_secs(5), || {
            // No core file: where dumps are on, the abort left 19 MB in the crate's directory.
            let none = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            // SAFETY: the limit is a plain struct that lives through the call, in a child about to end.
            unsafe { libc::setrlimit(libc::RLIMIT_CORE, &none) };
            std::process::abort()
        }));
    }

    #[test]
    fn work_that_never_returns_does_not_answer() {
        assert!(!answers_within(Duration::from_millis(100), || loop {
            std::thread::park();
        }));
    }
}
