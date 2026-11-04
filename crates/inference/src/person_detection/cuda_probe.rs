//! One kernel first: third_party/ANTfrastructure/docs/linux-accelerator-images.md#a-card-without-a-cuda-kernel

use std::ffi::{c_char, c_int, c_uint, CStr};

use anyhow::Result;
use ort::execution_providers::{ExecutionProvider, CUDA};
use ort::logging::LogLevel;
use ort::session::Session;
use ort::value::Tensor;

use crate::provider::ProviderUnavailable;

/// A single float `Relu`: ONNX Runtime's own CUDA code, not a vendor library with its own kernels.
const ONE_KERNEL: &[u8] = &[
    0x08, 0x08, 0x3a, 0x33, 0x0a, 0x0c, 0x0a, 0x01, 0x78, 0x12, 0x01, 0x79, 0x22, 0x04, 0x52, 0x65,
    0x6c, 0x75, 0x12, 0x01, 0x70, 0x5a, 0x0f, 0x0a, 0x01, 0x78, 0x12, 0x0a, 0x0a, 0x08, 0x08, 0x01,
    0x12, 0x04, 0x0a, 0x02, 0x08, 0x01, 0x62, 0x0f, 0x0a, 0x01, 0x79, 0x12, 0x0a, 0x0a, 0x08, 0x08,
    0x01, 0x12, 0x04, 0x0a, 0x02, 0x08, 0x01, 0x42, 0x02, 0x10, 0x0d,
];
const NO_KERNEL: &str = "cudaErrorNoKernelImageForDevice";
#[cfg(windows)]
const CUDA_DRIVER: &str = "nvcuda.dll";
#[cfg(not(windows))]
const CUDA_DRIVER: &str = "libcuda.so.1";

/// Fails only when the card has no kernel. CUDA reports every other failure when the real session starts.
pub(super) fn require_kernel(provider: &'static str) -> Result<()> {
    verdict(provider, run_one_kernel(Some(&CUDA::default())), || {
        card_on(CUDA_DRIVER)
    })
}

fn verdict(
    provider: &'static str,
    run: Result<f32, String>,
    card: impl FnOnce() -> Option<String>,
) -> Result<()> {
    match run {
        Err(error) if error.contains(NO_KERNEL) => Err(no_kernel(provider, card()).into()),
        _ => Ok(()),
    }
}

fn no_kernel(provider: &'static str, card: Option<String>) -> ProviderUnavailable {
    let card = card.unwrap_or_else(|| "this NVIDIA GPU".into());
    ProviderUnavailable {
        provider,
        reason: format!(
            "cannot run on {card}: this ONNX Runtime has no CUDA kernel for it. KATAGLYPHIS_ORT_DEVICE=cpu runs on the CPU"
        ),
    }
}

fn run_one_kernel(provider: Option<&dyn ExecutionProvider>) -> Result<f32, String> {
    let text = |error: ort::Error| error.to_string();
    // The verdict below says it better than ONNX Runtime's red line about a failed node.
    let mut builder = Session::builder()
        .map_err(text)?
        .with_log_level(LogLevel::Fatal)
        .map_err(|error| error.to_string())?;
    if let Some(provider) = provider {
        provider.register(&mut builder).map_err(text)?;
    }
    let mut session = builder.commit_from_memory(ONE_KERNEL).map_err(text)?;
    let input = Tensor::from_array(([1_usize], vec![-1.0_f32])).map_err(text)?;
    let outputs = session.run(ort::inputs![input]).map_err(text)?;
    let (_, values) = outputs[0].try_extract_tensor::<f32>().map_err(text)?;
    Ok(values[0])
}

/// The card's name and compute capability, as the driver reports them.
fn card_on(driver: &str) -> Option<String> {
    // SAFETY: the signatures are those of NVIDIA's cuda.h, and the library outlives the calls.
    unsafe {
        let cuda = libloading::Library::new(driver).ok()?;
        let init = cuda.get::<Init>(b"cuInit\0").ok()?;
        let device_get = cuda.get::<DeviceGet>(b"cuDeviceGet\0").ok()?;
        let device_name = cuda.get::<DeviceName>(b"cuDeviceGetName\0").ok()?;
        let capability = cuda
            .get::<Capability>(b"cuDeviceComputeCapability\0")
            .ok()?;
        card_from(*init, *device_get, *device_name, *capability)
    }
}

type Init = unsafe extern "C" fn(c_uint) -> c_int;
type DeviceGet = unsafe extern "C" fn(*mut c_int, c_int) -> c_int;
type DeviceName = unsafe extern "C" fn(*mut c_char, c_int, c_int) -> c_int;
type Capability = unsafe extern "C" fn(*mut c_int, *mut c_int, c_int) -> c_int;

/// # Safety
/// The functions must follow cuda.h: `device_name` writes a NUL-terminated name of at most the length it is given.
unsafe fn card_from(
    init: Init,
    device_get: DeviceGet,
    device_name: DeviceName,
    capability: Capability,
) -> Option<String> {
    let mut device = 0;
    let mut name: [c_char; 256] = [0; 256];
    let (mut major, mut minor) = (0, 0);
    if init(0) != 0
        || device_get(&raw mut device, 0) != 0
        || device_name(name.as_mut_ptr(), 256, device) != 0
        || capability(&raw mut major, &raw mut minor, device) != 0
    {
        return None;
    }
    let name = CStr::from_ptr(name.as_ptr()).to_string_lossy();
    Some(format!("{name} (compute capability {major}.{minor})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON_THE_CARD: &str = "Non-zero status code returned while running Relu node. Status Message: CUDA error cudaErrorNoKernelImageForDevice:no kernel image is available";

    #[test]
    fn no_kernel_names_the_card_and_the_way_out() {
        let error = verdict("CUDAExecutionProvider", Err(ON_THE_CARD.into()), || {
            Some("NVIDIA GeForce RTX 2080 (compute capability 7.5)".into())
        })
        .unwrap_err();
        let unavailable = error.downcast_ref::<ProviderUnavailable>().unwrap();
        assert_eq!(unavailable.provider, "CUDAExecutionProvider");
        let message = error.to_string();
        assert!(message.contains("NVIDIA GeForce RTX 2080"), "{message}");
        assert!(message.contains("compute capability 7.5"), "{message}");
        assert!(message.contains("KATAGLYPHIS_ORT_DEVICE=cpu"), "{message}");
    }

    #[test]
    fn a_card_the_driver_does_not_name_still_fails() {
        let error = verdict("CUDAExecutionProvider", Err(ON_THE_CARD.into()), || None).unwrap_err();
        assert!(error.to_string().contains("this NVIDIA GPU"), "{error}");
    }

    #[test]
    fn every_other_outcome_is_left_to_the_real_session() {
        let never = || -> Option<String> { panic!("only a missing kernel asks for the card") };
        assert!(verdict("CUDAExecutionProvider", Ok(0.0), never).is_ok());
        assert!(verdict(
            "CUDAExecutionProvider",
            Err("CUDA failure 999: Unknown error".into()),
            never
        )
        .is_ok());
    }

    #[test]
    fn no_driver_or_one_without_the_symbols_names_nothing() {
        assert_eq!(card_on("libnot-a-cuda-driver.so"), None);
        #[cfg(target_os = "linux")]
        assert_eq!(card_on("libc.so.6"), None);
    }

    unsafe extern "C" fn answers(_: c_uint) -> c_int {
        0
    }
    unsafe extern "C" fn refuses(_: c_uint) -> c_int {
        1
    }
    unsafe extern "C" fn first_device(device: *mut c_int, _: c_int) -> c_int {
        *device = 3;
        0
    }
    unsafe extern "C" fn fake_name(name: *mut c_char, _: c_int, device: c_int) -> c_int {
        assert_eq!(device, 3);
        std::ptr::copy_nonoverlapping(c"Fake GPU".as_ptr(), name, 9);
        0
    }
    unsafe extern "C" fn fake_capability(major: *mut c_int, minor: *mut c_int, _: c_int) -> c_int {
        *major = 12;
        *minor = 0;
        0
    }

    #[test]
    fn a_driver_that_answers_names_the_card() {
        // SAFETY: the fakes above follow cuda.h.
        let card = unsafe { card_from(answers, first_device, fake_name, fake_capability) };
        assert_eq!(card.as_deref(), Some("Fake GPU (compute capability 12.0)"));
    }

    #[test]
    fn a_driver_that_refuses_names_nothing() {
        // SAFETY: the fakes above follow cuda.h.
        let card = unsafe { card_from(refuses, first_device, fake_name, fake_capability) };
        assert_eq!(card, None);
    }

    #[test]
    fn the_check_fails_only_with_the_way_out() {
        if crate::ort_runtime::ensure_ort_loaded().is_err() {
            return;
        }
        if let Err(error) = require_kernel("CUDAExecutionProvider") {
            let message = error.to_string();
            assert!(message.contains("KATAGLYPHIS_ORT_DEVICE=cpu"), "{message}");
        }
    }

    #[test]
    fn the_model_runs_its_one_kernel_where_onnx_runtime_loads() {
        if crate::ort_runtime::ensure_ort_loaded().is_err() {
            return;
        }
        assert_eq!(run_one_kernel(None), Ok(0.0));
    }
}
