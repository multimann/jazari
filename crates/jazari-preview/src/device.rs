//! wgpu device creation: eframe's shared device and the private software
//! device of the offscreen renderer.
//!
//! Both follow Ruffle's own `create_wgpu_instance` and `request_device`
//! (`render/wgpu/src/backend.rs`), so Ruffle gets the limits and features it
//! was tested with. In CPU mode only a `DeviceType::Cpu` adapter is accepted:
//! WARP (DX12) on Windows, lavapipe (Vulkan) or llvmpipe (GL) on Linux.

use std::future::Future;
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use eframe::egui_wgpu;
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::wgpu_profiler::GpuProfiler;

use crate::{FORCE_CPU_ENV, PreviewError, RenderMode};

/// What the CPU-mode adapter selector of [`wgpu_setup`] chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterChoice {
    /// A software adapter, as asked.
    Software {
        /// The adapter, as [`adapter_summary`] prints it.
        adapter: String,
    },
    /// No software adapter existed, so the default (hardware) selection was
    /// used. Never happens while `JAZARI_FORCE_CPU` is set.
    HardwareFallback {
        /// The adapter, as [`adapter_summary`] prints it.
        adapter: String,
        /// Why the software adapter was not used.
        reason: String,
    },
}

static LAST_CHOICE: Mutex<Option<AdapterChoice>> = Mutex::new(None);

/// The last choice made by the CPU-mode adapter selector of [`wgpu_setup`],
/// for the settings panel or a startup notice. `None` in GPU mode or before
/// eframe created its device.
pub fn last_adapter_choice() -> Option<AdapterChoice> {
    LAST_CHOICE.lock().ok().and_then(|c| c.clone())
}

fn record_choice(choice: AdapterChoice) {
    if let Ok(mut slot) = LAST_CHOICE.lock() {
        *slot = Some(choice);
    }
}

/// A one-line description of an adapter, for example
/// `Microsoft Basic Render Driver (Dx12, Cpu)`.
pub fn adapter_summary(info: &wgpu::AdapterInfo) -> String {
    format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type)
}

/// Whether the adapter rasterises on the CPU (WARP, lavapipe, llvmpipe).
pub fn is_software_adapter(info: &wgpu::AdapterInfo) -> bool {
    info.device_type == wgpu::DeviceType::Cpu
}

/// The eframe device setup for a render mode: Ruffle's instance flags
/// (without `VALIDATION_INDIRECT_CALL`), its device limits and features, and
/// in CPU mode an adapter selector that picks a `DeviceType::Cpu` adapter.
///
/// `JAZARI_FORCE_CPU` wins here too: the mode is passed through
/// [`RenderMode::resolve`] first. In CPU mode, when no software adapter
/// exists, the selector falls back to the best hardware adapter and records
/// an [`AdapterChoice::HardwareFallback`], unless `JAZARI_FORCE_CPU` is set:
/// then it fails, and eframe reports the error, because that machine must
/// never render on its GPU.
///
/// In CPU mode the instance runs without backend validation layers and debug
/// info (they make WARP much slower); `WGPU_VALIDATION=1` turns them back on.
pub fn wgpu_setup(mode: RenderMode) -> egui_wgpu::WgpuSetup {
    let mode = RenderMode::resolve(mode);
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = backends(mode);
    setup.instance_descriptor.flags = instance_flags(mode);
    setup.device_descriptor = Arc::new(device_descriptor);
    if mode == RenderMode::Cpu {
        let selector: egui_wgpu::NativeAdapterSelectorMethod = Arc::new(select_software_adapter);
        setup.native_adapter_selector = Some(selector);
    }
    setup.into()
}

/// Builds Ruffle's descriptors from eframe's render state (the shared
/// device). Build it once and share it between every [`PreviewPlayer`]:
/// it holds Ruffle's pipeline caches.
///
/// [`PreviewPlayer`]: crate::PreviewPlayer
pub fn descriptors_from(rs: &egui_wgpu::RenderState) -> Arc<Descriptors> {
    Arc::new(Descriptors::new(
        rs.instance.clone(),
        rs.adapter.clone(),
        rs.device.clone(),
        rs.queue.clone(),
    ))
}

/// The backends that can offer a software adapter on this platform. WARP is
/// only exposed through DX12.
fn software_backends() -> wgpu::Backends {
    if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::VULKAN | wgpu::Backends::GL
    }
}

fn backends(mode: RenderMode) -> wgpu::Backends {
    match mode {
        RenderMode::Cpu => software_backends(),
        RenderMode::Gpu => {
            wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY | wgpu::Backends::GL)
        }
    }
}

/// Instance flags. Like Ruffle, never `VALIDATION_INDIRECT_CALL` (wgpu#8799).
/// CPU mode also drops validation and debug info, which debug builds would
/// otherwise turn on.
pub(crate) fn instance_flags(mode: RenderMode) -> wgpu::InstanceFlags {
    let base = match mode {
        RenderMode::Cpu => wgpu::InstanceFlags::empty(),
        RenderMode::Gpu => wgpu::InstanceFlags::from_build_config()
            .difference(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL),
    };
    base.with_env()
}

/// A copy of Ruffle's private `request_device` limits and features: WebGL2
/// limits raised to what the adapter offers, four colour attachments, and
/// the optional features Ruffle uses where the adapter has them.
pub(crate) fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    let adapter_limits = adapter.limits();
    let mut limits = wgpu::Limits::downlevel_webgl2_defaults()
        .using_resolution(adapter_limits.clone())
        .using_alignment(adapter_limits.clone());
    limits.max_uniform_buffer_binding_size = adapter_limits.max_uniform_buffer_binding_size;
    limits.max_inter_stage_shader_variables = adapter_limits.max_inter_stage_shader_variables;
    limits.max_color_attachments = 4;

    let optional = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
        | wgpu::Features::TEXTURE_COMPRESSION_BC
        | wgpu::Features::FLOAT32_FILTERABLE
        | GpuProfiler::ALL_WGPU_TIMER_FEATURES;

    wgpu::DeviceDescriptor {
        label: Some("jazari"),
        required_features: optional & adapter.features(),
        required_limits: limits,
        memory_hints: Default::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }
}

/// Preference among software adapters: WARP on Windows; lavapipe (Vulkan)
/// before llvmpipe (GL) on Linux.
fn backend_rank(backend: wgpu::Backend) -> u8 {
    match backend {
        wgpu::Backend::Dx12 | wgpu::Backend::Vulkan => 0,
        wgpu::Backend::Gl => 1,
        _ => 2,
    }
}

/// Preference among hardware adapters for the fallback ("default selection").
fn device_type_rank(device_type: wgpu::DeviceType) -> u8 {
    match device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    }
}

fn describe_adapters(adapters: &[wgpu::Adapter]) -> String {
    if adapters.is_empty() {
        return "none".to_owned();
    }
    adapters
        .iter()
        .map(|a| adapter_summary(&a.get_info()))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The best software adapter in `adapters`.
fn best_software_adapter<'a>(
    adapters: &'a [wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> Option<&'a wgpu::Adapter> {
    adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .filter(|a| is_software_adapter(&a.get_info()))
        .min_by_key(|a| backend_rank(a.get_info().backend))
}

/// eframe's adapter selector for CPU mode.
fn select_software_adapter(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, String> {
    if let Some(adapter) = best_software_adapter(adapters, surface) {
        let summary = adapter_summary(&adapter.get_info());
        log::info!("Al-Jazari renders on the software adapter {summary}");
        record_choice(AdapterChoice::Software { adapter: summary });
        return Ok(adapter.clone());
    }

    let found = describe_adapters(adapters);
    let reason = format!("no software adapter is available (adapters found: {found})");
    if RenderMode::cpu_forced_by_env() {
        return Err(format!(
            "{reason}; {FORCE_CPU_ENV} is set, so no hardware adapter is used"
        ));
    }

    let fallback = adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .min_by_key(|a| device_type_rank(a.get_info().device_type));
    match fallback {
        Some(adapter) => {
            let summary = adapter_summary(&adapter.get_info());
            log::warn!("CPU mode: {reason}; falling back to {summary}");
            record_choice(AdapterChoice::HardwareFallback {
                adapter: summary,
                reason,
            });
            Ok(adapter.clone())
        }
        None => Err(reason),
    }
}

/// Creates the offscreen renderer's own device on a software adapter. It
/// never falls back to hardware.
///
/// Note: enumerating DX12 adapters makes wgpu create (and drop) a D3D12
/// device on every DXGI adapter, the real GPU included, to read its
/// capabilities. That is driver initialisation, not rendering work.
pub(crate) fn create_software_descriptors() -> Result<Descriptors, PreviewError> {
    let backends = software_backends();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: instance_flags(RenderMode::Cpu),
        backend_options: wgpu::BackendOptions::from_env_or_default(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapters = block_on(instance.enumerate_adapters(backends));
    let adapter = best_software_adapter(&adapters, None)
        .cloned()
        .ok_or_else(|| PreviewError::NoSoftwareAdapter {
            found: describe_adapters(&adapters),
        })?;
    let (device, queue) = block_on(adapter.request_device(&device_descriptor(&adapter)))?;
    Ok(Descriptors::new(instance, adapter, device, queue))
}

/// Drives a future to completion on this thread. wgpu's native futures are
/// ready immediately, so this never really waits.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_mode_drops_validation_and_indirect_validation() {
        // Only meaningful when the environment does not override the flags.
        if std::env::var_os("WGPU_VALIDATION").is_none()
            && std::env::var_os("WGPU_DEBUG").is_none()
            && std::env::var_os("WGPU_VALIDATION_INDIRECT_CALL").is_none()
        {
            let flags = instance_flags(RenderMode::Cpu);
            assert!(!flags.contains(wgpu::InstanceFlags::VALIDATION));
            assert!(!flags.contains(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL));
            let gpu = instance_flags(RenderMode::Gpu);
            assert!(!gpu.contains(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL));
        }
    }

    #[test]
    fn software_backends_hold_warp_or_lavapipe() {
        if cfg!(windows) {
            assert_eq!(software_backends(), wgpu::Backends::DX12);
        } else {
            assert!(software_backends().contains(wgpu::Backends::VULKAN));
        }
    }

    #[test]
    fn block_on_returns_ready_values() {
        assert_eq!(block_on(async { 41 + 1 }), 42);
    }
}
