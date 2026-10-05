//! Which camera to capture: a Raspberry Pi CSI camera when one is attached, else a USB webcam.
//! The probes are impure; [`choose`] is a pure function over their result, so the order is testable.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::bail;
use gstreamer as gst;
use gstreamer::prelude::*;

/// What the config's `camera` key asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Auto,
    Rpicam,
    Libcamera,
    V4l2(Option<String>),
    MediaFoundation,
    KernelStreaming,
    Test,
    Image(String),
}

impl Choice {
    pub fn parse(spec: &str) -> anyhow::Result<Self> {
        let spec = spec.trim();
        // The first colon only: `image:C:\cats\tabby.jpg` keeps its drive letter.
        let (kind, arg) = match spec.split_once(':') {
            Some((kind, arg)) => (kind, Some(arg.trim())),
            None => (spec, None),
        };
        let arg = arg.filter(|a| !a.is_empty());
        Ok(match (kind.to_ascii_lowercase().as_str(), arg) {
            ("auto", None) => Self::Auto,
            ("rpicam", None) => Self::Rpicam,
            ("libcamera", None) => Self::Libcamera,
            ("v4l2", device) => Self::V4l2(device.map(str::to_owned)),
            ("mf", None) => Self::MediaFoundation,
            ("ks", None) => Self::KernelStreaming,
            ("test", None) => Self::Test,
            ("image", Some(path)) => Self::Image(path.to_owned()),
            _ => bail!(
                "unknown camera {spec:?}: use auto, rpicam, libcamera, v4l2[:DEVICE], mf, ks, test or image:FILE"
            ),
        })
    }
}

/// One device the GStreamer device monitor reported, as plain data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceInfo {
    /// The element the device creates: `v4l2src`, `libcamerasrc`, `mfvideosrc`, `ksvideosrc`, ...
    pub factory: String,
    pub name: String,
    pub path: Option<String>,
    pub driver: Option<String>,
    pub bus: Option<String>,
    /// libcamera's camera id, which names the sensor's device-tree node for a CSI camera.
    pub camera_id: Option<String>,
    /// Caps structure names, e.g. `video/x-raw`, `image/jpeg`, `video/x-bayer`.
    pub media_types: Vec<String>,
}

impl DeviceInfo {
    fn has(&self, media_type: &str) -> bool {
        self.media_types.iter().any(|t| t == media_type)
    }

    /// Unknown caps count as usable; raw Bayer only (a Pi's `rp1-cfe`/`unicam` nodes) does not.
    fn decodable(&self) -> bool {
        self.media_types.is_empty() || self.has("video/x-raw") || self.has("image/jpeg")
    }

    /// A UVC webcam; the Pi's ISP, codec and CSI receiver nodes have other drivers.
    pub fn is_usb_webcam(&self) -> bool {
        let uvc = self.driver.as_deref() == Some("uvcvideo")
            || self
                .bus
                .as_deref()
                .is_some_and(|bus| bus.starts_with("usb-"));
        self.factory == "v4l2src" && uvc && self.decodable()
    }

    /// A camera libcamera drives that is not a USB one (libcamera's UVC handler lists those too).
    pub fn is_csi_camera(&self) -> bool {
        let id = self.camera_id.as_deref().unwrap_or(&self.name);
        self.factory == "libcamerasrc" && !id.to_ascii_lowercase().contains("usb")
    }

    /// MJPEG when the device offers JPEG but no raw format at all.
    fn wants_mjpeg(&self) -> bool {
        !self.has("video/x-raw") && self.has("image/jpeg")
    }
}

/// Everything the probes found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe {
    /// Sensors `rpicam-hello --list-cameras` printed; it never lists USB cameras.
    pub rpicam_cameras: Vec<String>,
    pub rpicam_vid: bool,
    pub devices: Vec<DeviceInfo>,
}

/// How to capture, chosen from a [`Probe`].
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// `rpicam-vid` piping raw I420: the host's own camera stack, whatever its libcamera version.
    Rpicam,
    /// A device-monitor device, by index into [`Probe::devices`].
    Device {
        index: usize,
        mjpeg: bool,
    },
    /// Plain `libcamerasrc`, for an explicit `libcamera` the monitor did not list.
    Libcamera,
    V4l2Path(String),
    /// A test pattern; `Some(reason)` when it stands in for a missing camera.
    Test(Option<String>),
    Image(String),
}

impl Plan {
    /// Stand-ins are re-probed periodically, so a camera plugged in later is picked up.
    pub fn is_stand_in(&self) -> bool {
        matches!(self, Self::Test(Some(_)))
    }

    pub fn describe(&self, probe: &Probe) -> String {
        match self {
            Self::Rpicam => format!(
                "Raspberry Pi camera via rpicam-vid ({})",
                probe
                    .rpicam_cameras
                    .first()
                    .map(String::as_str)
                    .unwrap_or("?")
            ),
            Self::Device { index, mjpeg } => {
                let device = &probe.devices[*index];
                let format = if *mjpeg { ", MJPEG" } else { "" };
                match &device.path {
                    Some(path) => format!("{} {path} ({}{format})", device.factory, device.name),
                    None => format!("{} ({}{format})", device.factory, device.name),
                }
            }
            Self::Libcamera => "libcamerasrc".into(),
            Self::V4l2Path(path) => format!("v4l2src {path}"),
            Self::Test(None) => "test pattern".into(),
            Self::Test(Some(reason)) => format!("test pattern ({reason})"),
            Self::Image(path) => format!("still image {path}"),
        }
    }
}

/// The capture plan for `choice`; a missing camera becomes a test pattern rather than an exit.
pub fn choose(choice: &Choice, probe: &Probe) -> Plan {
    let devices = &probe.devices;
    let device = |i: usize| Plan::Device {
        index: i,
        mjpeg: devices[i].wants_mjpeg(),
    };
    let rpicam = || (probe.rpicam_vid && !probe.rpicam_cameras.is_empty()).then_some(Plan::Rpicam);
    let csi = || {
        devices
            .iter()
            .position(DeviceInfo::is_csi_camera)
            .map(device)
    };
    let usb = || {
        devices
            .iter()
            .position(DeviceInfo::is_usb_webcam)
            .map(device)
    };
    let by_factory = |factory: &str| {
        devices
            .iter()
            .position(|d| d.factory == factory)
            .map(device)
    };
    let stand_in = |reason: &str| Plan::Test(Some(reason.to_owned()));

    match choice {
        Choice::Auto => rpicam()
            .or_else(csi)
            .or_else(usb)
            .or_else(|| by_factory("mfvideosrc"))
            .or_else(|| by_factory("ksvideosrc"))
            .unwrap_or_else(|| stand_in("no camera found")),
        Choice::Rpicam => rpicam().unwrap_or_else(|| stand_in("rpicam-hello lists no camera")),
        Choice::Libcamera => csi().unwrap_or(Plan::Libcamera),
        Choice::V4l2(Some(path)) => devices
            .iter()
            .position(|d| d.factory == "v4l2src" && d.path.as_deref() == Some(path.as_str()))
            .map(device)
            .unwrap_or_else(|| Plan::V4l2Path(path.clone())),
        Choice::V4l2(None) => usb().unwrap_or_else(|| stand_in("no USB webcam found")),
        Choice::MediaFoundation => {
            by_factory("mfvideosrc").unwrap_or_else(|| stand_in("no Media Foundation camera"))
        }
        Choice::KernelStreaming => {
            by_factory("ksvideosrc").unwrap_or_else(|| stand_in("no kernel-streaming camera"))
        }
        Choice::Test => Plan::Test(None),
        Choice::Image(path) => Plan::Image(path.clone()),
    }
}

/// Sensor names from `rpicam-hello --list-cameras`, e.g. `imx219 [3280x2464 10-bit RGGB] (...)`.
pub fn parse_rpicam_list(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let (index, rest) = line.trim().split_once(':')?;
            index.trim().parse::<u32>().ok()?;
            Some(rest.trim().to_owned())
        })
        .filter(|camera| !camera.is_empty())
        .collect()
}

/// A probe plus the monitor's devices, which [`Plan::Device`] indexes.
pub struct Found {
    pub probe: Probe,
    pub devices: Vec<gst::Device>,
}

/// Runs every probe; each one that cannot run (no rpicam-apps, no device provider) finds nothing.
pub fn probe() -> Found {
    let rpicam_cameras = if find_on_path("rpicam-hello").is_some() {
        run_with_timeout("rpicam-hello", &["--list-cameras"], Duration::from_secs(15))
            .map(|out| parse_rpicam_list(&out))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let rpicam_vid = find_on_path("rpicam-vid").is_some();
    let (infos, devices) = monitor_devices();
    Found {
        probe: Probe {
            rpicam_cameras,
            rpicam_vid,
            devices: infos,
        },
        devices,
    }
}

/// The device providers that can see a camera this service captures. A device monitor would start
/// every provider instead, NDI and PipeWire included, each probe.
const CAMERA_PROVIDERS: &[&str] = &[
    "libcameraprovider",
    "v4l2deviceprovider",
    "mfdeviceprovider",
    "ksdeviceprovider",
];

fn monitor_devices() -> (Vec<DeviceInfo>, Vec<gst::Device>) {
    let mut devices = Vec::new();
    for name in CAMERA_PROVIDERS {
        let Some(provider) = gst::DeviceProviderFactory::find(name).and_then(|f| f.get()) else {
            continue;
        };
        devices.extend(
            provider
                .devices()
                .into_iter()
                .filter(|device| device.has_classes("Video/Source")),
        );
    }
    let infos = devices.iter().map(describe_device).collect();
    (infos, devices)
}

fn describe_device(device: &gst::Device) -> DeviceInfo {
    let element = device.create_element(None).ok();
    let factory = element
        .as_ref()
        .and_then(|e| e.factory())
        .map(|f| f.name().to_string())
        .unwrap_or_default();
    let props = device.properties();
    // The provider's keys moved from `v4l2.device.*` to `api.v4l2.*` in newer GStreamer.
    let get = |keys: &[&str]| {
        keys.iter().find_map(|key| {
            props
                .as_ref()
                .and_then(|s| s.get::<String>(*key).ok())
                .filter(|v| !v.is_empty())
        })
    };
    let camera_id = element
        .as_ref()
        .filter(|_| factory == "libcamerasrc")
        .and_then(|e| e.find_property("camera-name").map(|_| e))
        .and_then(|e| e.property::<Option<String>>("camera-name"));
    let mut media_types: Vec<String> = device
        .caps()
        .map(|caps| caps.iter().map(|s| s.name().to_string()).collect())
        .unwrap_or_default();
    media_types.sort();
    media_types.dedup();
    DeviceInfo {
        factory,
        name: device.display_name().to_string(),
        path: get(&["device.path", "api.v4l2.path"]),
        driver: get(&["v4l2.device.driver", "api.v4l2.cap.driver"]),
        bus: get(&["v4l2.device.bus_info", "api.v4l2.cap.bus_info"]),
        camera_id,
        media_types,
    }
}

/// `name` on `PATH`, the way a shell would find it.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let candidates: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.exe"), name.to_owned()]
    } else {
        vec![name.to_owned()]
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|dir| candidates.iter().map(move |c| dir.join(c)))
        .find(|path| path.is_file())
}

/// stdout of `program args`, or `None` when it fails or outlives `timeout`.
fn run_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => Some(String::from_utf8_lossy(&output.stdout).into_owned()),
        Ok(Err(err)) => {
            log::warn!("{program} failed: {err}");
            None
        }
        Err(_) => {
            log::warn!("{program} did not answer within {timeout:?}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uvc(path: &str) -> DeviceInfo {
        DeviceInfo {
            factory: "v4l2src".into(),
            name: "HD Pro Webcam C920".into(),
            path: Some(path.into()),
            driver: Some("uvcvideo".into()),
            bus: Some("usb-xhci-hcd.1-1".into()),
            media_types: vec!["video/x-raw".into(), "image/jpeg".into()],
            ..DeviceInfo::default()
        }
    }

    fn pi_node(driver: &str) -> DeviceInfo {
        DeviceInfo {
            factory: "v4l2src".into(),
            name: driver.into(),
            path: Some("/dev/video0".into()),
            driver: Some(driver.into()),
            bus: Some("platform:1f00110000.csi".into()),
            media_types: vec!["video/x-bayer".into()],
            ..DeviceInfo::default()
        }
    }

    fn csi() -> DeviceInfo {
        DeviceInfo {
            factory: "libcamerasrc".into(),
            name: "imx219".into(),
            camera_id: Some("/base/axi/pcie@120000/rp1/i2c@88000/imx219@10".into()),
            ..DeviceInfo::default()
        }
    }

    const RPICAM_LIST: &str = "Available cameras\n-----------------\n\
        0 : imx219 [3280x2464 10-bit RGGB] (/base/axi/pcie@120000/rp1/i2c@88000/imx219@10)\n    \
        Modes: 'SRGGB10_CSI2P' : 640x480 [206.65 fps - (1000, 752)/1280x960 crop]\n                             \
        1640x1232 [41.85 fps - (0, 0)/3280x2464 crop]\n";

    #[test]
    fn parses_the_choices_the_config_documents() {
        assert_eq!(Choice::parse("auto").unwrap(), Choice::Auto);
        assert_eq!(Choice::parse(" RPICAM ").unwrap(), Choice::Rpicam);
        assert_eq!(Choice::parse("v4l2").unwrap(), Choice::V4l2(None));
        assert_eq!(
            Choice::parse("v4l2:/dev/video2").unwrap(),
            Choice::V4l2(Some("/dev/video2".into()))
        );
        assert_eq!(
            Choice::parse(r"image:C:\cats\tabby.jpg").unwrap(),
            Choice::Image(r"C:\cats\tabby.jpg".into())
        );
        for bad in ["", "webcam", "image:", "auto:1", "test:x"] {
            assert!(Choice::parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn rpicam_list_yields_sensors_and_ignores_mode_lines() {
        assert_eq!(
            parse_rpicam_list(RPICAM_LIST),
            ["imx219 [3280x2464 10-bit RGGB] (/base/axi/pcie@120000/rp1/i2c@88000/imx219@10)"]
        );
        assert!(parse_rpicam_list("No cameras available!\n").is_empty());
    }

    #[test]
    fn auto_prefers_the_pi_camera_over_a_usb_webcam() {
        let probe = Probe {
            rpicam_cameras: parse_rpicam_list(RPICAM_LIST),
            rpicam_vid: true,
            devices: vec![uvc("/dev/video8"), csi()],
        };
        assert_eq!(choose(&Choice::Auto, &probe), Plan::Rpicam);
    }

    #[test]
    fn auto_uses_libcamera_where_rpicam_apps_are_missing() {
        let probe = Probe {
            devices: vec![uvc("/dev/video8"), csi()],
            ..Probe::default()
        };
        assert_eq!(
            choose(&Choice::Auto, &probe),
            Plan::Device {
                index: 1,
                mjpeg: false
            }
        );
    }

    #[test]
    fn auto_falls_back_to_the_usb_webcam_and_skips_the_pi_isp_nodes() {
        let probe = Probe {
            devices: vec![pi_node("rp1-cfe"), pi_node("pispbe"), uvc("/dev/video8")],
            ..Probe::default()
        };
        assert_eq!(
            choose(&Choice::Auto, &probe),
            Plan::Device {
                index: 2,
                mjpeg: false
            }
        );
    }

    #[test]
    fn a_libcamera_usb_camera_is_not_mistaken_for_a_csi_one() {
        let usb_via_libcamera = DeviceInfo {
            camera_id: Some("/base/scb/pcie@7d500000/pci@0,0/usb@0,0-1:1.0-046d:0825".into()),
            ..csi()
        };
        let probe = Probe {
            devices: vec![usb_via_libcamera, uvc("/dev/video8")],
            ..Probe::default()
        };
        assert_eq!(
            choose(&Choice::Auto, &probe),
            Plan::Device {
                index: 1,
                mjpeg: false
            }
        );
    }

    #[test]
    fn a_jpeg_only_webcam_is_decoded_as_mjpeg() {
        let jpeg_only = DeviceInfo {
            media_types: vec!["image/jpeg".into()],
            ..uvc("/dev/video0")
        };
        let probe = Probe {
            devices: vec![jpeg_only],
            ..Probe::default()
        };
        assert_eq!(
            choose(&Choice::Auto, &probe),
            Plan::Device {
                index: 0,
                mjpeg: true
            }
        );
    }

    #[test]
    fn nothing_attached_streams_a_stand_in_that_is_re_probed() {
        let plan = choose(
            &Choice::Auto,
            &Probe {
                devices: vec![pi_node("rp1-cfe")],
                ..Probe::default()
            },
        );
        assert!(plan.is_stand_in(), "{plan:?}");
        assert!(!choose(&Choice::Test, &Probe::default()).is_stand_in());
    }

    #[test]
    fn explicit_choices_win_over_the_auto_order() {
        let probe = Probe {
            rpicam_cameras: parse_rpicam_list(RPICAM_LIST),
            rpicam_vid: true,
            devices: vec![csi(), uvc("/dev/video8")],
        };
        assert_eq!(
            choose(&Choice::V4l2(None), &probe),
            Plan::Device {
                index: 1,
                mjpeg: false
            }
        );
        assert_eq!(
            choose(&Choice::V4l2(Some("/dev/video5".into())), &probe),
            Plan::V4l2Path("/dev/video5".into())
        );
        assert_eq!(
            choose(&Choice::Libcamera, &Probe::default()),
            Plan::Libcamera
        );
        assert!(choose(&Choice::Rpicam, &Probe::default()).is_stand_in());
    }
}
