@{
  Build = @{
    WorkspaceRootEnv = 'WORKSPACE_PATH'
    LogDir = 'logs/windows'

    CargoTargetDir = 'target'
    # The features both Windows arches ship; CARGO_FEATURES overrides them.
    CargoFeatures = @('gui_windows', 'onnxruntime_directml')

    # The GUI pipeline's by-name elements plus autovideosrc's fallbacks; GSTREAMER_PLUGIN_DIR overrides the dir.
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