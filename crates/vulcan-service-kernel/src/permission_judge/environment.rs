//! What this machine offers the local judge: the bundled runtime, the CUDA runtime and devices.
//! 本机为本机判定器提供的条件：随包执行库、CUDA 执行库与设备。
//!
//! Probing reads files and runs `nvidia-smi`, so callers run it on a blocking thread. A device is
//! recommended only when everything it needs is present; loading never replaces the chosen device.
//! 探测会读取文件并运行 `nvidia-smi`，因此调用方在阻塞线程上运行。只有设备所需条件全部具备时才推荐该
//! 设备；加载时绝不替换所选设备。

use std::path::{Path, PathBuf};

use vulcan_permission_judge::BundledRuntime;
use vulcan_permission_judge::protocol::JudgeDevice;
use vulcan_provider_config::provider_config::{
    PersistedPermissionJudgeConfiguration, RegisteredJudgeModel,
};
use vulcan_service_runtime::judge_cuda_runtime::{
    CudaComponent, CudaLibraryDirectory, CudaRuntimeTarget, NvidiaDriver, detect_nvidia_driver,
    find_cuda_library_directories, installed_component, missing_cuda_libraries,
};

/// Devices in preference order: CUDA is fastest, WebGPU runs on any modern GPU, CPU always works.
/// 按优先顺序排列的设备：CUDA 最快，WebGPU 可在任何现代 GPU 上运行，CPU 始终可用。
pub(crate) const DEVICE_PREFERENCE: [JudgeDevice; 3] =
    [JudgeDevice::Cuda, JudgeDevice::WebGpu, JudgeDevice::Cpu];

/// Everything one worker start needs, compared to decide whether a running worker still fits.
/// 一次 worker 启动所需的全部内容；通过比较判断运行中的 worker 是否仍然适用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadSpec {
    /// Registration identity.
    /// 登记身份。
    pub(crate) model_id: String,
    /// Absolute package directory.
    /// 绝对模型包目录。
    pub(crate) package_dir: PathBuf,
    /// Device to load on.
    /// 加载使用的设备。
    pub(crate) device: JudgeDevice,
    /// ONNX Runtime core library.
    /// ONNX Runtime 核心库。
    pub(crate) runtime_library: PathBuf,
    /// Plugin execution-provider library for plugin devices.
    /// 插件设备使用的插件执行提供程序库。
    pub(crate) execution_provider_library: Option<PathBuf>,
    /// Directories prepended to the worker's library search path.
    /// 前置到 worker 库搜索路径的目录。
    pub(crate) library_dirs: Vec<PathBuf>,
}

/// One probe of the machine.
/// 对本机的一次探测。
pub(crate) struct JudgeEnvironment {
    /// Bundled CPU and WebGPU runtime.
    /// 随包 CPU 与 WebGPU 执行库。
    pub(crate) bundled: Result<BundledRuntime, String>,
    /// Root of the managed CUDA runtime.
    /// 受管 CUDA 执行库根目录。
    pub(crate) cuda_root: Option<PathBuf>,
    /// CUDA runtime matching the bundled release on this platform.
    /// 本平台上与随包版本匹配的 CUDA 执行库。
    pub(crate) cuda_target: Option<CudaRuntimeTarget>,
    /// Installed NVIDIA driver.
    /// 已安装的 NVIDIA 驱动。
    pub(crate) driver: Result<Option<NvidiaDriver>, String>,
    /// Installed GPU build of ONNX Runtime.
    /// 已安装的 ONNX Runtime GPU 构建。
    pub(crate) onnxruntime_gpu: Option<PathBuf>,
    /// Complete CUDA library directories in preference order.
    /// 按优先顺序排列的完整 CUDA 库目录。
    pub(crate) detected: Vec<CudaLibraryDirectory>,
}

impl JudgeEnvironment {
    /// Probes the machine; blocking.
    /// 探测本机；阻塞调用。
    pub(crate) fn probe() -> Self {
        let bundled = vulcan_runtime_assets::resolve_bundled_judge_runtime_root()
            .ok_or_else(|| String::from("This build ships no judge runtime for this platform."))
            .and_then(|root| BundledRuntime::load(&root).map_err(|error| error.to_string()));
        let cuda_root = vulcan_runtime_assets::resolve_judge_cuda_runtime_root();
        let cuda_target = bundled.as_ref().ok().and_then(|runtime| {
            CudaRuntimeTarget::for_host(&runtime.onnxruntime_version)
                .ok()
                .flatten()
        });
        let (driver, onnxruntime_gpu, detected) = match (&cuda_target, &cuda_root) {
            (Some(target), Some(root)) => (
                detect_nvidia_driver(),
                installed_component(target, root, CudaComponent::Onnxruntime),
                find_cuda_library_directories(target, root),
            ),
            _ => (Ok(None), None, Vec::new()),
        };
        Self {
            bundled,
            cuda_root,
            cuda_target,
            driver,
            onnxruntime_gpu,
            detected,
        }
    }

    /// Stable reason `device` cannot be used, or `None` when it can be loaded.
    /// `device` 无法使用的稳定原因；可以加载时返回 `None`。
    pub(crate) fn unavailable_reason(
        &self,
        device: JudgeDevice,
        locked_cuda_directory: Option<&Path>,
    ) -> Option<&'static str> {
        if self.bundled.is_err() {
            return Some("runtime_missing");
        }
        if device != JudgeDevice::Cuda {
            return None;
        }
        let Some(target) = &self.cuda_target else {
            return Some("unsupported_platform");
        };
        match &self.driver {
            Err(_) | Ok(None) => return Some("driver_missing"),
            Ok(Some(driver)) if driver.major < target.minimum_driver_major => {
                return Some("driver_too_old");
            }
            Ok(Some(_)) => {}
        }
        if self.onnxruntime_gpu.is_none() {
            return Some("onnxruntime_missing");
        }
        if self.cuda_library_directory(locked_cuda_directory).is_none() {
            return Some("cuda_libraries_missing");
        }
        None
    }

    /// First usable device in preference order.
    /// 按优先顺序的第一个可用设备。
    pub(crate) fn recommended_device(
        &self,
        locked_cuda_directory: Option<&Path>,
    ) -> Option<JudgeDevice> {
        DEVICE_PREFERENCE.into_iter().find(|device| {
            self.unavailable_reason(*device, locked_cuda_directory)
                .is_none()
        })
    }

    /// CUDA library directory a CUDA load uses: the locked one when complete, otherwise the first
    /// detected one while nothing is locked. A locked directory is never replaced.
    /// CUDA 加载使用的库目录：锁定目录完整时使用它；未锁定时使用第一个检测到的目录。锁定目录绝不被替换。
    pub(crate) fn cuda_library_directory(&self, locked: Option<&Path>) -> Option<PathBuf> {
        let target = self.cuda_target.as_ref()?;
        match locked {
            Some(directory) => missing_cuda_libraries(target, directory)
                .is_empty()
                .then(|| directory.to_path_buf()),
            None => self.detected.first().map(|found| found.directory.clone()),
        }
    }

    /// Resolves everything needed to load `model` with the saved settings.
    /// 按已保存设置解析加载 `model` 所需的全部内容。
    pub(crate) fn load_spec(
        &self,
        settings: &PersistedPermissionJudgeConfiguration,
        model: &RegisteredJudgeModel,
    ) -> Result<LoadSpec, String> {
        let locked = settings.cuda_library_directory.as_deref().map(Path::new);
        let device = match settings.device {
            Some(device) => device,
            None => self.recommended_device(locked).ok_or_else(|| {
                String::from("No device can run the local judge on this machine.")
            })?,
        };
        if let Some(reason) = self.unavailable_reason(device, locked) {
            return Err(format!(
                "The {} device cannot run the local judge: {reason}.",
                device_name(device)
            ));
        }
        let bundled = self.bundled.as_ref().map_err(Clone::clone)?;
        let (runtime_library, execution_provider_library, library_dirs) = match device {
            JudgeDevice::Cpu => (
                bundled.core_library.clone(),
                None,
                bundled.library_directories(),
            ),
            JudgeDevice::WebGpu => (
                bundled.core_library.clone(),
                Some(bundled.webgpu_library.clone()),
                bundled.library_directories(),
            ),
            JudgeDevice::Cuda => {
                let target = self
                    .cuda_target
                    .as_ref()
                    .ok_or_else(|| String::from("CUDA is not supported on this platform."))?;
                let onnxruntime = self
                    .onnxruntime_gpu
                    .clone()
                    .ok_or_else(|| String::from("The ONNX Runtime GPU build is not installed."))?;
                let libraries = self
                    .cuda_library_directory(locked)
                    .ok_or_else(|| String::from("No complete CUDA library directory was found."))?;
                (
                    onnxruntime.join(target.core_library()),
                    None,
                    vec![onnxruntime, libraries],
                )
            }
        };
        Ok(LoadSpec {
            model_id: model.id.clone(),
            package_dir: PathBuf::from(&model.path),
            device,
            runtime_library,
            execution_provider_library,
            library_dirs,
        })
    }
}

/// Wire name of a device for messages.
/// 用于消息的设备线名称。
pub(crate) fn device_name(device: JudgeDevice) -> &'static str {
    match device {
        JudgeDevice::Cuda => "cuda",
        JudgeDevice::WebGpu => "webgpu",
        JudgeDevice::Cpu => "cpu",
    }
}
