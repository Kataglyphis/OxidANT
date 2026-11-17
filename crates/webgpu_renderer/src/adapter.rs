//! Which adapter the renderer runs on: how it is named and classified, how a native viewer asks for one,
//! and the order the automatic choice prefers. Pure, so it is tested without a GPU.

/// The CLI flag and variable that name an adapter by a case-insensitive part of its label.
pub const ADAPTER_FLAG: &str = "--adapter";
pub const ADAPTER_ENV: &str = "KATAGLYPHIS_ADAPTER";

/// What kind of GPU an adapter is, as far as its backend tells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AdapterClass {
    Discrete,
    Integrated,
    Virtual,
    Cpu,
    #[default]
    Unknown,
}

/// What a browser's `GPUAdapterInfo` says; wgpu keeps only its description, which Chrome leaves empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebAdapterInfo {
    pub vendor: String,
    pub architecture: String,
    pub device: String,
    pub description: String,
}

/// The adapter as the status bar names it, and its class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterIdentity {
    pub label: String,
    pub class: AdapterClass,
}

impl AdapterIdentity {
    pub fn of(info: &wgpu::AdapterInfo, web: Option<&WebAdapterInfo>) -> Self {
        Self {
            label: label(info, web),
            class: classify(info, web),
        }
    }
}

/// The adapter's name; without one, what the backend gives: `intel gen-12lp` in a browser, `Intel 0x9a49` natively.
pub fn label(info: &wgpu::AdapterInfo, web: Option<&WebAdapterInfo>) -> String {
    if !info.name.trim().is_empty() {
        return info.name.trim().to_owned();
    }
    let parts: Vec<String> = match web {
        Some(web) if !web.description.trim().is_empty() => vec![web.description.clone()],
        Some(web) => [&web.vendor, &web.architecture, &web.device]
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .take(2)
            .cloned()
            .collect(),
        None => native_parts(info),
    };
    let label = parts.join(" ").trim().to_owned();
    if label.is_empty() {
        "unknown adapter".to_owned()
    } else {
        label
    }
}

/// The PCI vendor's name and the device id, then the driver; empty when the backend reports none.
fn native_parts(info: &wgpu::AdapterInfo) -> Vec<String> {
    let mut parts = Vec::new();
    match vendor_name(info.vendor) {
        Some(vendor) => parts.push(vendor.to_owned()),
        None if info.vendor != 0 => parts.push(format!("vendor {:#06x}", info.vendor)),
        None => {}
    }
    if info.device != 0 {
        parts.push(format!("{:#06x}", info.device));
    }
    if !info.driver.trim().is_empty() {
        parts.push(format!("({})", info.driver.trim()));
    }
    parts
}

/// The PCI vendor ids a renderer meets.
pub fn vendor_name(id: u32) -> Option<&'static str> {
    Some(match id {
        0x8086 => "Intel",
        0x10de => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x13b5 => "Arm",
        0x5143 => "Qualcomm",
        0x106b => "Apple",
        0x1414 => "Microsoft",
        0x1010 => "Imagination",
        0x144d => "Samsung",
        _ => return None,
    })
}

/// The device type where the backend knows it; in a browser, which reports none, a guess from the vendor.
pub fn classify(info: &wgpu::AdapterInfo, web: Option<&WebAdapterInfo>) -> AdapterClass {
    match info.device_type {
        wgpu::DeviceType::DiscreteGpu => AdapterClass::Discrete,
        wgpu::DeviceType::IntegratedGpu => AdapterClass::Integrated,
        wgpu::DeviceType::VirtualGpu => AdapterClass::Virtual,
        wgpu::DeviceType::Cpu => AdapterClass::Cpu,
        wgpu::DeviceType::Other => web.map_or(AdapterClass::Unknown, web_class),
    }
}

/// Dawn's names: Intel's `gen-12hp`, `xe-hpg` and `xe-2hpg` are its discrete Arc GPUs, every other Intel is integrated.
fn web_class(web: &WebAdapterInfo) -> AdapterClass {
    let vendor = web.vendor.to_lowercase();
    let architecture = web.architecture.to_lowercase();
    match vendor.as_str() {
        "intel" if architecture.contains("hp") => AdapterClass::Discrete,
        "intel" | "apple" | "qualcomm" | "arm" | "img-tec" | "samsung" => AdapterClass::Integrated,
        "nvidia" => AdapterClass::Discrete,
        "google" if architecture.contains("swiftshader") => AdapterClass::Cpu,
        "mesa" if architecture.contains("llvmpipe") => AdapterClass::Cpu,
        _ => AdapterClass::Unknown,
    }
}

/// The status bar's hint beside the adapter: a short word and the tooltip that explains it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterHint {
    pub text: &'static str,
    pub tooltip: &'static str,
}

const BROWSER_INTEGRATED: AdapterHint = AdapterHint {
    text: "Integrated GPU",
    tooltip: "The browser renders on an integrated GPU, and Chrome ignores the page's power preference on Windows.\n\
              To use the discrete GPU: Windows Settings > System > Display > Graphics > Chrome > Options > \
              High performance, then restart Chrome. chrome://gpu lists the GPU it uses.",
};

const NATIVE_INTEGRATED: AdapterHint = AdapterHint {
    text: "Integrated GPU",
    tooltip: "A discrete GPU is available: Renderer > Adapter > Use restarts the viewer on it, \
              as does --adapter <name> or KATAGLYPHIS_ADAPTER=<name>.",
};

/// The hint for an integrated adapter: in a browser always, natively only while a discrete one is listed.
pub fn hint(class: AdapterClass, browser: bool, discrete_listed: bool) -> Option<AdapterHint> {
    match (class, browser) {
        (AdapterClass::Integrated, true) => Some(BROWSER_INTEGRATED),
        (AdapterClass::Integrated, false) if discrete_listed => Some(NATIVE_INTEGRATED),
        _ => None,
    }
}

/// The API's name as the C++ status bar writes it.
pub fn api_name(backend: wgpu::Backend) -> &'static str {
    match backend {
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Dx12 => "Direct3D 12",
        wgpu::Backend::Gl => "OpenGL",
        wgpu::Backend::BrowserWebGpu => "WebGPU",
        wgpu::Backend::Noop => "No-op",
    }
}

/// One row of the adapter list, and what `--adapter` matches: `NVIDIA RTX 2000 Ada (Vulkan)`.
pub fn entry_label(info: &wgpu::AdapterInfo) -> String {
    format!("{} ({})", label(info, None), api_name(info.backend))
}

/// What the automatic choice and an explicit request read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterRequest {
    /// A case-insensitive part of [`entry_label`].
    pub name: Option<String>,
    /// `LowPower` puts integrated before discrete.
    pub power: wgpu::PowerPreference,
    /// Within the renderer's backends; `WGPU_BACKEND` narrows them.
    pub backends: wgpu::Backends,
}

impl Default for AdapterRequest {
    fn default() -> Self {
        Self {
            name: None,
            power: wgpu::PowerPreference::HighPerformance,
            backends: wgpu::Backends::PRIMARY,
        }
    }
}

impl AdapterRequest {
    /// `KATAGLYPHIS_ADAPTER`, else wgpu's `WGPU_ADAPTER_NAME`, and wgpu's `WGPU_POWER_PREF` and `WGPU_BACKEND`.
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        Self::from_vars(
            var(ADAPTER_ENV).or_else(|| var("WGPU_ADAPTER_NAME")),
            wgpu::PowerPreference::from_env(),
            wgpu::Backends::from_env(),
        )
    }

    /// The request the variables make; a backend list without one this renderer runs keeps every one.
    pub fn from_vars(
        name: Option<String>,
        power: Option<wgpu::PowerPreference>,
        backends: Option<wgpu::Backends>,
    ) -> Self {
        let defaults = Self::default();
        let backends = backends
            .map(|asked| asked & defaults.backends)
            .filter(|usable| !usable.is_empty())
            .unwrap_or(defaults.backends);
        Self {
            name,
            power: power.unwrap_or(defaults.power),
            backends,
        }
    }

    /// The command line's `--adapter` over the variables.
    pub fn with_name(mut self, name: Option<String>) -> Self {
        if name.is_some() {
            self.name = name;
        }
        self
    }
}

/// Lower is preferred: discrete, integrated, virtual, unknown, then a CPU rasterizer.
pub fn rank(class: AdapterClass, power: wgpu::PowerPreference) -> u8 {
    let low_power = power == wgpu::PowerPreference::LowPower;
    match class {
        AdapterClass::Discrete if low_power => 1,
        AdapterClass::Integrated if low_power => 0,
        AdapterClass::Discrete => 0,
        AdapterClass::Integrated => 1,
        AdapterClass::Virtual => 2,
        AdapterClass::Unknown => 3,
        AdapterClass::Cpu => 4,
    }
}

/// The automatic choice among the usable adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    pub index: usize,
    /// A name was asked for and no usable adapter matched it.
    pub unmatched_request: bool,
}

/// The best usable adapter that `request` names, else the best usable one; the first listed wins a tie.
pub fn choose(
    adapters: &[wgpu::AdapterInfo],
    usable: &[bool],
    request: &AdapterRequest,
) -> Option<Choice> {
    let candidates = || {
        adapters
            .iter()
            .enumerate()
            .filter(|(index, _)| usable.get(*index).copied().unwrap_or(false))
    };
    let best = |matches: &dyn Fn(&wgpu::AdapterInfo) -> bool| {
        candidates()
            .filter(|(_, info)| matches(info))
            .min_by_key(|(_, info)| rank(classify(info, None), request.power))
            .map(|(index, _)| index)
    };
    if let Some(name) = request.name.as_deref() {
        let wanted = name.trim().to_lowercase();
        if let Some(index) = best(&|info| entry_label(info).to_lowercase().contains(&wanted)) {
            return Some(Choice {
                index,
                unmatched_request: false,
            });
        }
    }
    best(&|_| true).map(|index| Choice {
        index,
        unmatched_request: request.name.is_some(),
    })
}

/// The command line without its `--adapter` (spaced or with `=`), and that adapter.
pub fn split_adapter_arg(args: &[String]) -> (Option<String>, Vec<String>) {
    let mut adapter = None;
    let mut rest = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == ADAPTER_FLAG {
            adapter = args.next().cloned();
        } else if let Some(value) = arg.strip_prefix("--adapter=") {
            adapter = Some(value.to_owned());
        } else {
            rest.push(arg.clone());
        }
    }
    (adapter.filter(|name| !name.trim().is_empty()), rest)
}

/// The arguments a relaunch on `entry` passes: the original ones with its `--adapter` replaced.
pub fn relaunch_args(args: &[String], entry: &str) -> Vec<String> {
    let (_, mut rest) = split_adapter_arg(args);
    rest.extend([ADAPTER_FLAG.to_owned(), entry.to_owned()]);
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(
        name: &str,
        device_type: wgpu::DeviceType,
        backend: wgpu::Backend,
    ) -> wgpu::AdapterInfo {
        let mut info = wgpu::AdapterInfo::new(device_type, backend);
        info.name = name.to_owned();
        info
    }

    /// The user's laptop as wgpu lists it on Windows, with a CPU rasterizer first.
    fn laptop() -> Vec<wgpu::AdapterInfo> {
        use wgpu::{Backend as B, DeviceType as T};
        vec![
            adapter("Microsoft Basic Render Driver", T::Cpu, B::Dx12),
            adapter("Intel(R) UHD Graphics", T::IntegratedGpu, B::Vulkan),
            adapter(
                "NVIDIA RTX 2000 Ada Generation Laptop GPU",
                T::DiscreteGpu,
                B::Vulkan,
            ),
            adapter("Intel(R) UHD Graphics", T::IntegratedGpu, B::Dx12),
            adapter(
                "NVIDIA RTX 2000 Ada Generation Laptop GPU",
                T::DiscreteGpu,
                B::Dx12,
            ),
        ]
    }

    fn named(name: &str) -> AdapterRequest {
        AdapterRequest {
            name: Some(name.to_owned()),
            ..AdapterRequest::default()
        }
    }

    #[test]
    fn a_browser_adapter_without_a_name_is_labelled_from_its_vendor_and_architecture() {
        let info = wgpu::AdapterInfo::new(wgpu::DeviceType::Other, wgpu::Backend::BrowserWebGpu);
        let intel = WebAdapterInfo {
            vendor: "intel".into(),
            architecture: "gen-12lp".into(),
            ..WebAdapterInfo::default()
        };
        let identity = AdapterIdentity::of(&info, Some(&intel));
        assert_eq!(identity.label, "intel gen-12lp");
        assert_eq!(identity.class, AdapterClass::Integrated);
        let described = WebAdapterInfo {
            description: "ANGLE (Intel, Intel(R) UHD Graphics)".into(),
            ..intel.clone()
        };
        assert_eq!(
            label(&info, Some(&described)),
            "ANGLE (Intel, Intel(R) UHD Graphics)"
        );
        assert_eq!(
            label(&info, Some(&WebAdapterInfo::default())),
            "unknown adapter"
        );
        assert_eq!(label(&info, None), "unknown adapter");
    }

    #[test]
    fn a_native_adapter_without_a_name_is_labelled_from_its_pci_ids_and_driver() {
        let mut info =
            wgpu::AdapterInfo::new(wgpu::DeviceType::IntegratedGpu, wgpu::Backend::Vulkan);
        info.vendor = 0x8086;
        info.device = 0x9a49;
        info.driver = "Intel open-source Mesa driver".into();
        assert_eq!(
            label(&info, None),
            "Intel 0x9a49 (Intel open-source Mesa driver)"
        );
        info.vendor = 0xabcd;
        info.driver.clear();
        assert_eq!(label(&info, None), "vendor 0xabcd 0x9a49");
        info.name = "  Named GPU ".into();
        assert_eq!(label(&info, None), "Named GPU");
    }

    #[test]
    fn a_browser_adapter_is_classified_by_its_vendor() {
        let class = |vendor: &str, architecture: &str| {
            let web = WebAdapterInfo {
                vendor: vendor.into(),
                architecture: architecture.into(),
                ..WebAdapterInfo::default()
            };
            let info =
                wgpu::AdapterInfo::new(wgpu::DeviceType::Other, wgpu::Backend::BrowserWebGpu);
            classify(&info, Some(&web))
        };
        assert_eq!(class("intel", "gen-12lp"), AdapterClass::Integrated);
        assert_eq!(class("intel", "xe-hpg"), AdapterClass::Discrete);
        assert_eq!(class("nvidia", "lovelace"), AdapterClass::Discrete);
        assert_eq!(class("apple", "metal-3"), AdapterClass::Integrated);
        assert_eq!(
            class("amd", "rdna-3"),
            AdapterClass::Unknown,
            "an APU or a card"
        );
        assert_eq!(class("google", "swiftshader"), AdapterClass::Cpu);
        let fallback = wgpu::AdapterInfo::new(wgpu::DeviceType::Cpu, wgpu::Backend::BrowserWebGpu);
        assert_eq!(
            classify(&fallback, None),
            AdapterClass::Cpu,
            "wgpu's fallback flag"
        );
    }

    #[test]
    fn the_hint_names_the_windows_setting_in_a_browser_and_the_switch_natively() {
        let browser = hint(AdapterClass::Integrated, true, false).expect("a browser hint");
        assert!(
            browser.tooltip.contains("High performance"),
            "{}",
            browser.tooltip
        );
        let native = hint(AdapterClass::Integrated, false, true).expect("a native hint");
        assert!(native.tooltip.contains("--adapter"), "{}", native.tooltip);
        assert_eq!(hint(AdapterClass::Integrated, false, false), None);
        assert_eq!(hint(AdapterClass::Discrete, true, false), None);
        assert_eq!(hint(AdapterClass::Unknown, true, false), None);
    }

    #[test]
    fn the_automatic_choice_prefers_discrete_and_never_a_cpu_while_hardware_is_usable() {
        let adapters = laptop();
        let all = [true; 5];
        let choice = choose(&adapters, &all, &AdapterRequest::default()).expect("a choice");
        assert_eq!(
            choice,
            Choice {
                index: 2,
                unmatched_request: false
            }
        );
        let low = AdapterRequest {
            power: wgpu::PowerPreference::LowPower,
            ..AdapterRequest::default()
        };
        assert_eq!(choose(&adapters, &all, &low).map(|c| c.index), Some(1));
        let cpu_and_intel = [true, false, false, true, false];
        assert_eq!(
            choose(&adapters, &cpu_and_intel, &AdapterRequest::default()).map(|c| c.index),
            Some(3)
        );
        let cpu_only = [true, false, false, false, false];
        assert_eq!(
            choose(&adapters, &cpu_only, &AdapterRequest::default()).map(|c| c.index),
            Some(0)
        );
        assert_eq!(
            choose(&adapters, &[false; 5], &AdapterRequest::default()),
            None
        );
    }

    #[test]
    fn a_request_matches_part_of_the_label_and_its_backend_ignoring_case() {
        let adapters = laptop();
        let all = [true; 5];
        let pick = |name: &str| choose(&adapters, &all, &named(name)).expect("a choice");
        assert_eq!(pick("uhd").index, 1);
        assert_eq!(pick("Intel(R) UHD Graphics (Direct3D 12)").index, 3);
        assert_eq!(
            pick("nvidia rtx 2000 ada generation laptop gpu (direct3d 12)").index,
            4
        );
        assert_eq!(
            pick("basic render").index,
            0,
            "an explicit request may name a CPU adapter"
        );
        let missing = pick("radeon");
        assert_eq!(
            missing,
            Choice {
                index: 2,
                unmatched_request: true
            }
        );
        let unusable = [true, true, false, true, false];
        let nvidia = choose(&adapters, &unusable, &named("nvidia")).expect("a choice");
        assert_eq!(
            nvidia,
            Choice {
                index: 1,
                unmatched_request: true
            }
        );
    }

    #[test]
    fn the_variables_narrow_within_the_renderers_backends() {
        let request = AdapterRequest::from_vars(
            Some("uhd".into()),
            Some(wgpu::PowerPreference::LowPower),
            Some(wgpu::Backends::DX12 | wgpu::Backends::GL),
        );
        assert_eq!(
            request.backends,
            wgpu::Backends::DX12,
            "GL never runs this renderer"
        );
        assert_eq!(request.power, wgpu::PowerPreference::LowPower);
        let gl_only = AdapterRequest::from_vars(None, None, Some(wgpu::Backends::GL));
        assert_eq!(gl_only, AdapterRequest::default());
        let cli = AdapterRequest::default().with_name(Some("nvidia".into()));
        assert_eq!(cli.name.as_deref(), Some("nvidia"));
        assert_eq!(named("uhd").with_name(None).name.as_deref(), Some("uhd"));
    }

    #[test]
    fn a_relaunch_keeps_the_arguments_and_replaces_the_adapter() {
        let args: Vec<String> = ["scene.glb", "--adapter", "uhd", "--adapter=old"]
            .map(String::from)
            .to_vec();
        let (adapter, rest) = split_adapter_arg(&args);
        assert_eq!(adapter.as_deref(), Some("old"), "the last one wins");
        assert_eq!(rest, ["scene.glb"]);
        let relaunch = relaunch_args(&args, "NVIDIA RTX 2000 Ada Generation Laptop GPU (Vulkan)");
        assert_eq!(
            relaunch,
            [
                "scene.glb",
                "--adapter",
                "NVIDIA RTX 2000 Ada Generation Laptop GPU (Vulkan)"
            ]
        );
        assert_eq!(split_adapter_arg(&["--adapter".to_owned()]), (None, vec![]));
        assert_eq!(
            entry_label(&laptop()[3]),
            "Intel(R) UHD Graphics (Direct3D 12)"
        );
    }
}
