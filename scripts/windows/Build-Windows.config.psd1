@{
  Build = @{
    WorkspaceRootEnv = 'WORKSPACE_PATH'
    LogDir = 'logs/windows'

    CargoTargetDir = 'target'
    # The features the Windows packages ship, on x64 and arm64 alike; CARGO_FEATURES
    # overrides them for a one-off build.
    CargoFeatures = @('gui_windows', 'onnxruntime_directml')

    # The GStreamer plugins an exe that links GStreamer ships in lib\gstreamer-1.0: the
    # elements the GUI's camera pipeline creates by name (crates/gui/src/gui_wgpu/pipeline.rs),
    # and what autovideosrc falls back to without Media Foundation (ksvideosrc) or without a
    # camera (videotestsrc). `kataglyphis_cli media-check` proves the pipeline builds from
    # them, in both lanes. The directory is the image's, for the target arch;
    # GSTREAMER_PLUGIN_DIR overrides it.
    GStreamerPluginDir = 'C:\runtime\lib\gstreamer-1.0'
    GStreamerPlugins = @('gstcoreelements', 'gstapp', 'gstvideoconvertscale', 'gstmediafoundation', 'gstautodetect', 'gstwinks', 'gstvideotestsrc')
  }

  Msix = @{
    PackageName = 'Kataglyphis.OxidANT'
    Publisher = 'CN=Kataglyphis'
    PublisherDisplayName = 'Kataglyphis'
    DisplayName = 'OxidANT'
    Description = 'Rust project template with optional GUI, ONNX backends, profiling, packaging, and CI workflows.'
    Version = '0.1.0.0'
    MinVersion = '10.0.19041.0'
    ManifestTemplate = 'packaging/msix/AppxManifest.template.xml'
    Binary = 'kataglyphis_cli'
  }

  Msi = @{
    # Enable/disable MSI packaging
    Enabled = $true
    # Product name shown in installer
    ProductName = 'OxidANT'
    # Manufacturer name
    Manufacturer = 'Kataglyphis'
    # Path to WiX source file (relative to workspace root)
    WxsFile = 'wix/main.wxs'
    # Path to license file (relative to workspace root)  
    LicenseFile = 'wix/License.rtf'
    # Output filename pattern (version will be appended)
    OutputName = 'kataglyphis_cli'
  }
}