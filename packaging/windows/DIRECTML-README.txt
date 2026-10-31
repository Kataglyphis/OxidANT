OxidANT DirectML portable candidate

Run oxidant-directml.cmd gui to select the DirectML ONNX Runtime provider, or run
oxidant-directml.cmd onnx-check to test the model without a camera. A compatible
DirectX 12 GPU (including supported AMD and NVIDIA devices) and up-to-date
vendor drivers are required. DirectML availability
and GPU execution depend on the device and drivers; the candidate is not certified
on every GPU. ONNX Runtime may run unsupported model operations on the CPU.

DirectML.dll is a Microsoft redistributable. Microsoft publishes the package and its
license terms at https://www.nuget.org/packages/Microsoft.AI.DirectML. The exact
image DLL's origin and redistribution terms are tracked in ANTfrastructure CON180.

To use CPU inference instead, set KATAGLYPHIS_ORT_DEVICE=cpu before starting the
launcher. kataglyphis_cli.exe also retains its normal CPU default when started
without the launcher. This portable candidate does not include an installer.
