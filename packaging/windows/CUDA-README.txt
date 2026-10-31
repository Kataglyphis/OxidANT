OxidANT CUDA portable candidate

Run oxidant-cuda.cmd gui to select CUDA inference, or oxidant-cuda.cmd onnx-check
to test the model without a camera. A compatible NVIDIA GPU and driver and the
matching CUDA and cuDNN runtime DLLs must be installed on the host. This package
contains the ONNX Runtime CUDA provider, not the NVIDIA SDK. GPU execution is
not certified by candidate packaging. ONNX Runtime may place unsupported model
operations on the CPU.

Set KATAGLYPHIS_ORT_DEVICE=cpu before running the launcher to select CPU only.
