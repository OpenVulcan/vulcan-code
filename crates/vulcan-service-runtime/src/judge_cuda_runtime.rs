//! CUDA runtime of the local permission judge: official catalog, detection, validation and install.
//! 本机权限判定器的 CUDA 执行库：官方目录、检测、验证与安装。
//!
//! The CUDA device needs two parts the bundled runtime does not ship, because both depend on the
//! user's GPU and driver: the GPU build of ONNX Runtime, and the CUDA libraries its CUDA provider
//! links (cuBLAS on Windows; cuBLAS, cudart and cuRAND on Linux). `judge-cuda-runtime.json` pins the
//! official archives of both, keyed by the ONNX Runtime release the bundled runtime reports, so the
//! GPU build always matches the bundled one. The CUDA libraries may instead come from an installed
//! CUDA Toolkit or a directory the user picks; detection lists only directories that contain every
//! required library.
//! CUDA 设备需要两部分随包执行库未提供的内容，因为两者都取决于用户的显卡与驱动：ONNX Runtime 的
//! GPU 构建，以及其 CUDA 提供器链接的 CUDA 库（Windows 为 cuBLAS；Linux 为 cuBLAS、cudart 与 cuRAND）。
//! `judge-cuda-runtime.json` 以随包执行库报告的 ONNX Runtime 版本为键固定两者的官方归档，使 GPU 构建
//! 始终与随包构建一致。CUDA 库也可来自已安装的 CUDA Toolkit 或用户选择的目录；检测只列出包含全部
//! 必需库的目录。

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use flate2::read::GzDecoder;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use vulcan_host_tools::process_creation::apply_windows_release_no_window_to_std_command;
use xz2::read::XzDecoder;
use zip::ZipArchive;

use crate::managed_runtime_bootstrap::ManagedRuntimeBootstrapProgress;
use crate::managed_runtime_installer::{
    commit_staging_directory, download_file, emit_progress, official_download_client,
    prepare_clean_directory, remove_path_if_exists, sha256_file_hex, sibling_work_path,
    verify_sha256,
};

/// Official archives and links of the CUDA runtime, embedded at build time.
/// 构建时嵌入的 CUDA 执行库官方归档与链接。
const CATALOG_JSON: &str = include_str!("../judge-cuda-runtime.json");
/// The only catalog format this build reads.
/// 本构建唯一读取的目录格式。
const CATALOG_SCHEMA_VERSION: u32 = 1;
/// Manifest written into every installed component directory.
/// 写入每个已安装组件目录的清单。
pub const COMPONENT_MANIFEST_FILE: &str = "component.json";
/// The only component manifest format this build reads.
/// 本构建唯一读取的组件清单格式。
const COMPONENT_MANIFEST_SCHEMA_VERSION: u32 = 1;
/// Directory below the managed root that keeps verified archives until their component installs.
/// 受管根目录下保存已校验归档直至其组件安装完成的目录。
const DOWNLOADS_DIR_NAME: &str = ".downloads";
/// Lock file serializing installs below one managed root.
/// 串行化同一受管根目录下安装的锁文件。
const INSTALL_LOCK_FILE: &str = ".install.lock";
/// Waiting limit for `nvidia-smi`, which can stall while the driver initializes.
/// 等待 `nvidia-smi` 的上限；驱动初始化期间它可能停滞。
const NVIDIA_SMI_TIMEOUT: Duration = Duration::from_secs(10);
/// Polling interval while waiting for `nvidia-smi`.
/// 等待 `nvidia-smi` 时的轮询间隔。
const NVIDIA_SMI_POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Overall progress reserved for downloads; extraction takes the rest.
/// 下载所占的总体进度；其余属于解压。
const DOWNLOAD_PROGRESS_END: u8 = 90;

/// Catalog as stored in `judge-cuda-runtime.json`.
/// `judge-cuda-runtime.json` 中保存的目录。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    /// Catalog format.
    /// 目录格式。
    schema_version: u32,
    /// CUDA runtime per ONNX Runtime release.
    /// 每个 ONNX Runtime 发行版本对应的 CUDA 执行库。
    runtimes: BTreeMap<String, RuntimeEntry>,
    /// Official pages for manual installation.
    /// 手动安装使用的官方页面。
    links: Vec<OfficialLink>,
}

/// CUDA runtime of one ONNX Runtime release.
/// 一个 ONNX Runtime 发行版本的 CUDA 执行库。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeEntry {
    /// CUDA major version the GPU build links.
    /// GPU 构建链接的 CUDA 主版本。
    cuda_major: u32,
    /// Lowest driver branch that runs this CUDA major under minor version compatibility.
    /// 在次版本兼容下运行该 CUDA 主版本的最低驱动分支。
    minimum_driver_major: u32,
    /// Archives per `<os>-<arch>` platform key.
    /// 按 `<os>-<arch>` 平台键划分的归档。
    platforms: BTreeMap<String, PlatformEntry>,
}

/// Archives of one platform.
/// 一个平台的归档。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlatformEntry {
    /// GPU build of ONNX Runtime.
    /// ONNX Runtime 的 GPU 构建。
    onnxruntime: OnnxruntimeEntry,
    /// CUDA libraries the CUDA provider links.
    /// CUDA 提供器链接的 CUDA 库。
    cuda: CudaEntry,
}

/// GPU build of ONNX Runtime.
/// ONNX Runtime 的 GPU 构建。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct OnnxruntimeEntry {
    /// Core library passed to the worker, relative to the component directory.
    /// 传给 worker 的核心库，相对组件目录。
    core_library: String,
    /// Official archive.
    /// 官方归档。
    archive: ArchiveEntry,
}

/// CUDA libraries the CUDA provider links.
/// CUDA 提供器链接的 CUDA 库。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CudaEntry {
    /// File names that must all be present in one directory.
    /// 必须全部位于同一目录的文件名。
    libraries: Vec<String>,
    /// Official NVIDIA redistributable archives.
    /// NVIDIA 官方可再分发归档。
    archives: Vec<ArchiveEntry>,
}

/// One pinned official archive and the members installed from it.
/// 一个固定的官方归档及从中安装的成员。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveEntry {
    /// Upstream file name.
    /// 上游文件名。
    pub name: String,
    /// Official HTTPS download address, also shown for manual download.
    /// 官方 HTTPS 下载地址，也用于手动下载展示。
    pub url: String,
    /// Lowercase SHA-256 of the archive.
    /// 归档的小写 SHA-256。
    pub sha256: String,
    /// Archive size in bytes.
    /// 归档字节数。
    pub size: u64,
    /// Archive member to destination relative to the component directory.
    /// 归档成员到相对组件目录的目标路径。
    pub files: BTreeMap<String, String>,
}

/// Official page for manual installation; the interface labels it by `id`.
/// 手动安装使用的官方页面；界面按 `id` 显示标签。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfficialLink {
    /// Stable identifier the interface maps to a label.
    /// 界面映射为标签的稳定标识。
    pub id: String,
    /// Official HTTPS address.
    /// 官方 HTTPS 地址。
    pub url: String,
}

/// Returns the parsed embedded catalog.
/// 返回解析后的嵌入目录。
fn catalog() -> Result<&'static Catalog, String> {
    static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| parse_catalog(CATALOG_JSON))
        .as_ref()
        .map_err(Clone::clone)
}

/// Parses a catalog and checks every digest, address and destination.
/// 解析目录并检查每个摘要、地址与目标路径。
fn parse_catalog(text: &str) -> Result<Catalog, String> {
    let catalog: Catalog = serde_json::from_str(text)
        .map_err(|error| format!("The CUDA runtime catalog is malformed: {error}."))?;
    if catalog.schema_version != CATALOG_SCHEMA_VERSION {
        return Err(format!(
            "CUDA runtime catalog schema {} is not supported.",
            catalog.schema_version
        ));
    }
    for link in &catalog.links {
        if !link.url.starts_with("https://") {
            return Err(format!("CUDA runtime link `{}` is not HTTPS.", link.id));
        }
    }
    for (release, runtime) in &catalog.runtimes {
        for (platform, entry) in &runtime.platforms {
            let context = format!("{release} {platform}");
            check_component(
                &context,
                std::slice::from_ref(&entry.onnxruntime.archive),
                std::slice::from_ref(&entry.onnxruntime.core_library),
            )?;
            check_component(&context, &entry.cuda.archives, &entry.cuda.libraries)?;
        }
    }
    Ok(catalog)
}

/// Checks one component's archives and that each required file is installed exactly once.
/// 检查一个组件的归档，以及每个必需文件恰好安装一次。
fn check_component(
    context: &str,
    archives: &[ArchiveEntry],
    required: &[String],
) -> Result<(), String> {
    let mut destinations = BTreeSet::new();
    for archive in archives {
        let digest_valid = archive.sha256.len() == 64
            && archive
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if !digest_valid || !archive.url.starts_with("https://") || archive.files.is_empty() {
            return Err(format!(
                "CUDA runtime archive `{}` of {context} is invalid.",
                archive.name
            ));
        }
        for (member, destination) in &archive.files {
            if !is_contained(member) || !is_contained(destination) {
                return Err(format!(
                    "CUDA runtime archive `{}` of {context} has an unsafe path.",
                    archive.name
                ));
            }
            if !destinations.insert(destination.as_str()) {
                return Err(format!(
                    "CUDA runtime destination `{destination}` of {context} is installed twice."
                ));
            }
        }
    }
    for file in required {
        if !destinations.contains(file.as_str()) {
            return Err(format!(
                "CUDA runtime file `{file}` of {context} is not installed by any archive."
            ));
        }
    }
    Ok(())
}

/// Whether `path` is a relative path made of plain components only.
/// `path` 是否为仅由普通组件构成的相对路径。
fn is_contained(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

/// Platform key of this host in NVIDIA's `<os>-<arch>` spelling, such as `windows-x86_64`.
/// 本宿主采用 NVIDIA `<os>-<arch>` 拼写的平台键，例如 `windows-x86_64`。
pub fn host_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Official pages for manual installation.
/// 手动安装使用的官方页面。
pub fn official_links() -> Result<&'static [OfficialLink], String> {
    Ok(&catalog()?.links)
}

/// Installable part of the CUDA runtime.
/// CUDA 执行库的可安装部分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CudaComponent {
    /// GPU build of ONNX Runtime; only installable from the official archive.
    /// ONNX Runtime 的 GPU 构建；只能从官方归档安装。
    Onnxruntime,
    /// CUDA libraries; installable, or taken from a toolkit or a chosen directory.
    /// CUDA 库；可安装，也可取自 Toolkit 或所选目录。
    CudaLibraries,
}

/// CUDA runtime requirements and sources for one ONNX Runtime release on this host.
/// 本宿主上一个 ONNX Runtime 发行版本的 CUDA 执行库要求与来源。
#[derive(Debug, Clone)]
pub struct CudaRuntimeTarget {
    /// ONNX Runtime release shared with the bundled runtime.
    /// 与随包执行库相同的 ONNX Runtime 发行版本。
    pub onnxruntime_version: String,
    /// CUDA major version the GPU build links.
    /// GPU 构建链接的 CUDA 主版本。
    pub cuda_major: u32,
    /// Lowest NVIDIA driver branch that runs it.
    /// 可运行它的最低 NVIDIA 驱动分支。
    pub minimum_driver_major: u32,
    /// Platform key the archives were selected for.
    /// 选择归档所用的平台键。
    pub platform: String,
    /// Archives of this platform.
    /// 此平台的归档。
    entry: PlatformEntry,
}

impl CudaRuntimeTarget {
    /// Selects the CUDA runtime of `onnxruntime_version` for this host, or `None` when the catalog
    /// declares no CUDA build for the release or the host.
    /// 为本宿主选择 `onnxruntime_version` 的 CUDA 执行库；目录未为该版本或宿主声明 CUDA 构建时返回 `None`。
    pub fn for_host(onnxruntime_version: &str) -> Result<Option<Self>, String> {
        Self::select(catalog()?, onnxruntime_version, &host_platform())
    }

    /// Selects from an explicit catalog and platform.
    /// 从明确的目录与平台中选择。
    fn select(
        catalog: &Catalog,
        onnxruntime_version: &str,
        platform: &str,
    ) -> Result<Option<Self>, String> {
        let Some(runtime) = catalog.runtimes.get(onnxruntime_version) else {
            return Ok(None);
        };
        Ok(runtime.platforms.get(platform).map(|entry| Self {
            onnxruntime_version: onnxruntime_version.to_string(),
            cuda_major: runtime.cuda_major,
            minimum_driver_major: runtime.minimum_driver_major,
            platform: platform.to_string(),
            entry: entry.clone(),
        }))
    }

    /// File names that must all be present in the CUDA library directory.
    /// CUDA 库目录中必须全部存在的文件名。
    pub fn required_libraries(&self) -> &[String] {
        &self.entry.cuda.libraries
    }

    /// Core library inside the installed GPU build, relative to its component directory.
    /// 已安装 GPU 构建内的核心库，相对其组件目录。
    pub fn core_library(&self) -> &str {
        &self.entry.onnxruntime.core_library
    }

    /// Official archives of `component`, also shown for manual download.
    /// `component` 的官方归档，也用于手动下载展示。
    pub fn archives(&self, component: CudaComponent) -> &[ArchiveEntry] {
        match component {
            CudaComponent::Onnxruntime => std::slice::from_ref(&self.entry.onnxruntime.archive),
            CudaComponent::CudaLibraries => &self.entry.cuda.archives,
        }
    }

    /// Total download size of `component` in bytes.
    /// `component` 的下载总字节数。
    pub fn download_bytes(&self, component: CudaComponent) -> u64 {
        self.archives(component)
            .iter()
            .map(|archive| archive.size)
            .sum()
    }

    /// Directory `component` is installed into below `managed_root`.
    /// `component` 在 `managed_root` 下的安装目录。
    pub fn component_directory(&self, managed_root: &Path, component: CudaComponent) -> PathBuf {
        let name = match component {
            CudaComponent::Onnxruntime => String::from("onnxruntime-gpu"),
            CudaComponent::CudaLibraries => format!("cuda-{}", self.cuda_major),
        };
        managed_root
            .join(&self.onnxruntime_version)
            .join(&self.platform)
            .join(name)
    }

    /// Destinations the catalog installs for `component`.
    /// 目录为 `component` 安装的目标路径。
    fn destinations(&self, component: CudaComponent) -> BTreeSet<&str> {
        self.archives(component)
            .iter()
            .flat_map(|archive| archive.files.values().map(String::as_str))
            .collect()
    }
}

/// Manifest written into an installed component directory.
/// 写入已安装组件目录的清单。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentManifest {
    /// Manifest format.
    /// 清单格式。
    schema_version: u32,
    /// Installed component.
    /// 已安装的组件。
    component: CudaComponent,
    /// ONNX Runtime release the component belongs to.
    /// 组件所属的 ONNX Runtime 发行版本。
    onnxruntime: String,
    /// Platform key.
    /// 平台键。
    platform: String,
    /// Lowercase SHA-256 per installed file.
    /// 每个已安装文件的小写 SHA-256。
    files: BTreeMap<String, String>,
}

/// Returns the directory of `component` when it is installed for `target` and every file exists.
/// 当 `component` 已为 `target` 安装且每个文件都存在时，返回其目录。
pub fn installed_component(
    target: &CudaRuntimeTarget,
    managed_root: &Path,
    component: CudaComponent,
) -> Option<PathBuf> {
    let directory = target.component_directory(managed_root, component);
    let bytes = fs::read(directory.join(COMPONENT_MANIFEST_FILE)).ok()?;
    let manifest: ComponentManifest = serde_json::from_slice(&bytes).ok()?;
    let expected = target.destinations(component);
    let matches = manifest.schema_version == COMPONENT_MANIFEST_SCHEMA_VERSION
        && manifest.component == component
        && manifest.onnxruntime == target.onnxruntime_version
        && manifest.platform == target.platform
        && manifest.files.keys().map(String::as_str).eq(expected);
    (matches
        && manifest
            .files
            .keys()
            .all(|file| directory.join(file).is_file()))
    .then_some(directory)
}

/// Installed NVIDIA display driver.
/// 已安装的 NVIDIA 显示驱动。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NvidiaDriver {
    /// Driver version as reported, such as `591.86`.
    /// 报告的驱动版本，例如 `591.86`。
    pub version: String,
    /// Driver branch, such as `591`.
    /// 驱动分支，例如 `591`。
    pub major: u32,
    /// Names of the NVIDIA GPUs the driver sees.
    /// 驱动可见的 NVIDIA 显卡名称。
    pub gpus: Vec<String>,
    /// Lowest CUDA compute capability among those GPUs, such as `(8, 9)`; `None` when the driver
    /// does not report it.
    /// 这些显卡中最低的 CUDA 计算能力，例如 `(8, 9)`；驱动不报告时为 `None`。
    pub compute_capability: Option<(u32, u32)>,
}

/// Reads the NVIDIA driver through `nvidia-smi`, which every driver installs; `None` when it is
/// absent, which means no usable NVIDIA driver. Compute capability is queried separately because
/// older drivers reject the field; its failure only leaves the capability unknown.
/// 通过每个驱动都会安装的 `nvidia-smi` 读取 NVIDIA 驱动；不存在时返回 `None`，表示没有可用的 NVIDIA 驱动。
/// 计算能力单独查询，因为旧驱动不认识该字段；查询失败只会让计算能力保持未知。
pub fn detect_nvidia_driver() -> Result<Option<NvidiaDriver>, String> {
    let Some(output) = query_nvidia_smi("driver_version,name")? else {
        return Ok(None);
    };
    let mut driver = parse_nvidia_smi(&output)?;
    driver.compute_capability = query_nvidia_smi("compute_cap")
        .ok()
        .flatten()
        .and_then(|output| parse_compute_capabilities(&output));
    Ok(Some(driver))
}

/// Runs one `nvidia-smi --query-gpu` CSV query; `None` when the tool is absent or fails.
/// 运行一次 `nvidia-smi --query-gpu` CSV 查询；工具不存在或失败时返回 `None`。
fn query_nvidia_smi(fields: &str) -> Result<Option<String>, String> {
    let mut command = Command::new("nvidia-smi");
    command
        .args([
            format!("--query-gpu={fields}").as_str(),
            "--format=csv,noheader",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    apply_windows_release_no_window_to_std_command(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Failed to start nvidia-smi: {error}.")),
    };
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to wait for nvidia-smi: {error}."))?
        {
            break status;
        }
        if started.elapsed() >= NVIDIA_SMI_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(String::from("nvidia-smi did not answer in time."));
        }
        std::thread::sleep(NVIDIA_SMI_POLL_INTERVAL);
    };
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout
            .read_to_string(&mut output)
            .map_err(|error| format!("Failed to read nvidia-smi output: {error}."))?;
    }
    Ok(status.success().then_some(output))
}

/// Parses `driver_version,name` CSV rows; every GPU reports the same driver.
/// 解析 `driver_version,name` CSV 行；每块显卡报告相同的驱动。
fn parse_nvidia_smi(output: &str) -> Result<NvidiaDriver, String> {
    let mut version = None;
    let mut gpus = Vec::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let (driver, name) = line
            .split_once(',')
            .ok_or_else(|| format!("Unexpected nvidia-smi row `{line}`."))?;
        version.get_or_insert_with(|| driver.trim().to_string());
        gpus.push(name.trim().to_string());
    }
    let version = version.ok_or_else(|| String::from("nvidia-smi reported no GPU."))?;
    let major = version
        .split('.')
        .next()
        .and_then(|major| major.parse().ok())
        .ok_or_else(|| format!("Unexpected NVIDIA driver version `{version}`."))?;
    Ok(NvidiaDriver {
        version,
        major,
        gpus,
        compute_capability: None,
    })
}

/// Parses `compute_cap` rows such as `8.9` into the lowest capability; `None` when any row is not
/// a capability, so an unknown GPU never looks more capable than it is.
/// 将 `8.9` 这类 `compute_cap` 行解析为最低计算能力；任一行不是计算能力时返回 `None`，避免未知显卡被
/// 视为能力更强。
fn parse_compute_capabilities(output: &str) -> Option<(u32, u32)> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (major, minor) = line.trim().split_once('.')?;
            Some((major.parse().ok()?, minor.parse().ok()?))
        })
        .collect::<Option<Vec<(u32, u32)>>>()?
        .into_iter()
        .min()
}

/// Where a CUDA library directory was found.
/// CUDA 库目录的发现位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CudaLibrarySource {
    /// Installed by Vulcan from the official archives.
    /// 由 Vulcan 从官方归档安装。
    Managed,
    /// An installed CUDA Toolkit of the required major version.
    /// 已安装的所需主版本 CUDA Toolkit。
    Toolkit,
    /// A directory on the library search path.
    /// 库搜索路径上的目录。
    SearchPath,
}

/// One directory that contains every required CUDA library.
/// 包含全部必需 CUDA 库的一个目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CudaLibraryDirectory {
    /// Where it was found.
    /// 发现位置。
    pub source: CudaLibrarySource,
    /// Absolute directory.
    /// 绝对目录。
    pub directory: PathBuf,
}

/// Required libraries missing from `directory`; empty when the directory is complete.
/// `directory` 中缺失的必需库；目录完整时为空。
pub fn missing_cuda_libraries(target: &CudaRuntimeTarget, directory: &Path) -> Vec<String> {
    target
        .required_libraries()
        .iter()
        .filter(|library| !directory.join(library).is_file())
        .cloned()
        .collect()
}

/// Lists complete CUDA library directories in preference order: the managed install, installed
/// toolkits of the required major version (newest first), then the library search path.
/// 按优先顺序列出完整的 CUDA 库目录：受管安装、所需主版本的已安装 Toolkit（新版本在前），然后是库搜索路径。
pub fn find_cuda_library_directories(
    target: &CudaRuntimeTarget,
    managed_root: &Path,
) -> Vec<CudaLibraryDirectory> {
    let mut candidates = Vec::new();
    if let Some(directory) = installed_component(target, managed_root, CudaComponent::CudaLibraries)
    {
        candidates.push((CudaLibrarySource::Managed, directory));
    }
    candidates.extend(
        toolkit_directories(target.cuda_major)
            .into_iter()
            .map(|directory| (CudaLibrarySource::Toolkit, directory)),
    );
    if let Some(search_path) = std::env::var_os(library_search_variable()) {
        candidates.extend(
            std::env::split_paths(&search_path)
                .filter(|directory| directory.is_absolute())
                .map(|directory| (CudaLibrarySource::SearchPath, directory)),
        );
    }
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|(_, directory)| missing_cuda_libraries(target, directory).is_empty())
        .filter(|(_, directory)| {
            seen.insert(fs::canonicalize(directory).unwrap_or_else(|_| directory.clone()))
        })
        .map(|(source, directory)| CudaLibraryDirectory { source, directory })
        .collect()
}

/// Variable the platform loader searches for dependent libraries.
/// 平台加载器搜索依赖库所用的变量。
fn library_search_variable() -> &'static str {
    if cfg!(windows) {
        "PATH"
    } else {
        "LD_LIBRARY_PATH"
    }
}

/// Library directories of installed CUDA Toolkits of `major`, newest version first.
/// 主版本为 `major` 的已安装 CUDA Toolkit 库目录，新版本在前。
fn toolkit_directories(major: u32) -> Vec<PathBuf> {
    // roots pairs each toolkit root with its parsed version for ordering.
    // roots 将每个 Toolkit 根目录与其解析出的版本配对以便排序。
    let mut roots: Vec<((u32, u32), PathBuf)> = Vec::new();
    let mut add_versioned_children = |parent: PathBuf, prefix: &str| {
        let Ok(entries) = fs::read_dir(&parent) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(version) = name
                .strip_prefix(prefix)
                .and_then(|version| parse_toolkit_version(version, major))
            {
                roots.push((version, entry.path()));
            }
        }
    };
    if cfg!(windows) {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            add_versioned_children(
                PathBuf::from(program_files).join("NVIDIA GPU Computing Toolkit/CUDA"),
                "v",
            );
        }
    } else {
        add_versioned_children(PathBuf::from("/usr/local"), "cuda-");
    }
    roots.sort_by_key(|(version, _)| std::cmp::Reverse(*version));
    // explicit lists environment-declared roots first, then the versioned installs.
    // explicit 先列出环境变量声明的根目录，再列出带版本的安装。
    let mut explicit: Vec<PathBuf> = std::env::vars_os()
        .filter(|(name, _)| is_toolkit_variable(name, major))
        .map(|(_, value)| PathBuf::from(value))
        .collect();
    explicit.sort();
    if !cfg!(windows) {
        explicit.push(PathBuf::from("/usr/local/cuda"));
    }
    explicit
        .into_iter()
        .chain(roots.into_iter().map(|(_, root)| root))
        .flat_map(|root| {
            if cfg!(windows) {
                vec![root.join("bin").join("x64"), root.join("bin")]
            } else {
                vec![
                    root.join("lib64"),
                    root.join("targets")
                        .join(format!("{}-linux", std::env::consts::ARCH))
                        .join("lib"),
                ]
            }
        })
        .collect()
}

/// Parses a `13.2`-style toolkit version of the required major.
/// 解析所需主版本的 `13.2` 形式 Toolkit 版本。
fn parse_toolkit_version(text: &str, major: u32) -> Option<(u32, u32)> {
    let (found_major, minor) = text.split_once('.')?;
    let found_major: u32 = found_major.parse().ok()?;
    (found_major == major).then_some((found_major, minor.parse().ok()?))
}

/// Whether `name` is `CUDA_PATH`, `CUDA_HOME` or `CUDA_PATH_V<major>_<minor>`.
/// `name` 是否为 `CUDA_PATH`、`CUDA_HOME` 或 `CUDA_PATH_V<major>_<minor>`。
fn is_toolkit_variable(name: &OsString, major: u32) -> bool {
    let name = name.to_string_lossy();
    name == "CUDA_PATH"
        || name == "CUDA_HOME"
        || name
            .strip_prefix(&format!("CUDA_PATH_V{major}_"))
            .is_some_and(|minor| minor.parse::<u32>().is_ok())
}

/// Downloads, verifies and installs `component` below `managed_root`, returning its directory.
/// Archives are checked against the catalog digests before any member is read, and only the
/// declared members are written; the component replaces any previous install atomically.
/// 下载、校验并在 `managed_root` 下安装 `component`，返回其目录。读取任何成员前先按目录摘要校验归档，
/// 且只写入声明的成员；组件以原子方式替换此前的安装。
pub fn install_component<F>(
    target: &CudaRuntimeTarget,
    component: CudaComponent,
    managed_root: &Path,
    progress: &mut F,
) -> Result<PathBuf, String>
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    fs::create_dir_all(managed_root).map_err(|error| {
        format!(
            "Failed to create the CUDA runtime directory {}: {error}.",
            managed_root.display()
        )
    })?;
    // lock serializes installs; it is released when dropped.
    // lock 串行化安装；释放于 drop 时。
    let lock = File::create(managed_root.join(INSTALL_LOCK_FILE))
        .map_err(|error| format!("Failed to open the CUDA runtime install lock: {error}."))?;
    lock.try_lock_exclusive().map_err(|_| {
        String::from("Another CUDA runtime installation is running; wait for it to finish.")
    })?;
    let label = match component {
        CudaComponent::Onnxruntime => "onnxruntime-gpu",
        CudaComponent::CudaLibraries => "cuda",
    };
    let archives = target.archives(component);
    let client = official_download_client("judge-cuda-runtime")?;
    let downloads = managed_root.join(DOWNLOADS_DIR_NAME);
    let total = target.download_bytes(component).max(1);
    let mut downloaded = 0_u64;
    let mut local_archives = Vec::new();
    for archive in archives {
        let start = progress_share(downloaded, total);
        downloaded += archive.size;
        let end = progress_share(downloaded, total);
        let local = downloads.join(&archive.name);
        // A verified archive left by an interrupted install is reused instead of downloaded again.
        // 中断安装留下的已校验归档直接复用，不重新下载。
        if verify_sha256(&local, &archive.sha256).is_err() {
            download_file(
                &client,
                &archive.url,
                &local,
                label,
                &format!("Downloading {}", archive.name),
                start,
                end,
                progress,
            )?;
            verify_sha256(&local, &archive.sha256)?;
        }
        local_archives.push((archive, local));
    }
    emit_progress(
        progress,
        "extracting",
        Some(label),
        "Installing CUDA runtime files",
        DOWNLOAD_PROGRESS_END,
        None,
        None,
    );
    let directory = target.component_directory(managed_root, component);
    let staging = sibling_work_path(&directory, "staging")?;
    prepare_clean_directory(&staging)?;
    let mut files = BTreeMap::new();
    for (archive, local) in &local_archives {
        extract_members(local, archive, &staging)?;
        for destination in archive.files.values() {
            files.insert(
                destination.clone(),
                sha256_file_hex(&staging.join(destination))?,
            );
        }
    }
    let manifest = ComponentManifest {
        schema_version: COMPONENT_MANIFEST_SCHEMA_VERSION,
        component,
        onnxruntime: target.onnxruntime_version.clone(),
        platform: target.platform.clone(),
        files,
    };
    let manifest = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("Failed to encode the CUDA component manifest: {error}."))?;
    fs::write(staging.join(COMPONENT_MANIFEST_FILE), manifest)
        .map_err(|error| format!("Failed to write the CUDA component manifest: {error}."))?;
    commit_staging_directory(&staging, &directory)?;
    for (_, local) in &local_archives {
        remove_path_if_exists(local)?;
    }
    emit_progress(
        progress,
        "completed",
        Some(label),
        "CUDA runtime files installed",
        100,
        None,
        None,
    );
    Ok(directory)
}

/// Maps downloaded bytes onto the download share of the overall progress.
/// 将已下载字节映射到总体进度中的下载份额。
fn progress_share(downloaded: u64, total: u64) -> u8 {
    (downloaded.min(total) * u64::from(DOWNLOAD_PROGRESS_END) / total) as u8
}

/// Writes the declared members of one verified archive into `staging`.
/// 将一个已校验归档中声明的成员写入 `staging`。
fn extract_members(local: &Path, archive: &ArchiveEntry, staging: &Path) -> Result<(), String> {
    let mut remaining: BTreeMap<&str, &str> = archive
        .files
        .iter()
        .map(|(member, destination)| (member.as_str(), destination.as_str()))
        .collect();
    let open = || {
        File::open(local).map_err(|error| format!("Failed to open {}: {error}.", local.display()))
    };
    if archive.name.ends_with(".zip") {
        let mut zip = ZipArchive::new(open()?)
            .map_err(|error| format!("Failed to read {}: {error}.", archive.name))?;
        for (member, destination) in std::mem::take(&mut remaining) {
            let mut entry = zip
                .by_name(member)
                .map_err(|error| format!("{} lacks `{member}`: {error}.", archive.name))?;
            if !entry.is_file() {
                return Err(format!("`{member}` in {} is not a file.", archive.name));
            }
            write_member(&mut entry, &staging.join(destination))?;
        }
    } else {
        let reader: Box<dyn Read> = if archive.name.ends_with(".tar.xz") {
            Box::new(XzDecoder::new(open()?))
        } else if archive.name.ends_with(".tgz") || archive.name.ends_with(".tar.gz") {
            Box::new(GzDecoder::new(open()?))
        } else {
            return Err(format!(
                "Unsupported CUDA runtime archive {}.",
                archive.name
            ));
        };
        let mut tar = tar::Archive::new(reader);
        let entries = tar
            .entries()
            .map_err(|error| format!("Failed to read {}: {error}.", archive.name))?;
        for entry in entries {
            let mut entry =
                entry.map_err(|error| format!("Failed to read {}: {error}.", archive.name))?;
            let path = entry
                .path()
                .map_err(|error| format!("Failed to read {}: {error}.", archive.name))?
                .to_string_lossy()
                .trim_start_matches("./")
                .to_string();
            if let Some(destination) = remaining.remove(path.as_str()) {
                if !entry.header().entry_type().is_file() {
                    return Err(format!("`{path}` in {} is not a file.", archive.name));
                }
                write_member(&mut entry, &staging.join(destination))?;
            }
        }
    }
    match remaining.keys().next() {
        Some(member) => Err(format!("{} lacks `{member}`.", archive.name)),
        None => Ok(()),
    }
}

/// Copies one archive member to `path`, creating its parent directory.
/// 将一个归档成员复制到 `path`，并创建其父目录。
fn write_member(reader: &mut dyn Read, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create {}: {error}.", parent.display()))?;
    }
    let mut output = File::create(path)
        .map_err(|error| format!("Failed to create {}: {error}.", path.display()))?;
    io::copy(reader, &mut output)
        .map_err(|error| format!("Failed to write {}: {error}.", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small catalog whose archives are built by the tests.
    /// 归档由测试构造的小型目录。
    fn catalog_with(archive: ArchiveEntry) -> Catalog {
        parse_catalog(
            &serde_json::json!({
                "schema_version": 1,
                "runtimes": { "9.9.9": {
                    "cuda_major": 13,
                    "minimum_driver_major": 580,
                    "platforms": { "test-x86_64": {
                        "onnxruntime": { "core_library": "core.dll", "archive": archive },
                        "cuda": { "libraries": ["blas.dll"], "archives": [{
                            "name": "cuda.zip",
                            "url": "https://example.invalid/cuda.zip",
                            "sha256": "0".repeat(64),
                            "size": 1,
                            "files": { "cuda/blas.dll": "blas.dll" }
                        }] }
                    } }
                } },
                "links": []
            })
            .to_string(),
        )
        .unwrap()
    }

    /// Builds a zip archive from `(member, contents)` pairs.
    /// 由 `(成员, 内容)` 对构造 zip 归档。
    fn zip_bytes(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut cursor);
        for (name, contents) in members {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            io::Write::write_all(&mut writer, contents).unwrap();
        }
        writer.finish().unwrap();
        cursor.into_inner()
    }

    #[test]
    fn embedded_catalog_is_valid_and_pairs_with_the_bundled_runtime_release() {
        let catalog = catalog().unwrap();
        // The bundled runtime release comes from the build-input lock; it must have a CUDA entry.
        // 随包执行库版本来自构建输入锁文件；它必须有对应的 CUDA 条目。
        let lock: serde_json::Value =
            serde_json::from_str(include_str!("../../../scripts/build-inputs.lock.json")).unwrap();
        let release = lock["judge_runtime"]["onnxruntime"].as_str().unwrap();
        let runtime = catalog.runtimes.get(release).unwrap_or_else(|| {
            panic!("judge-cuda-runtime.json has no entry for the bundled ONNX Runtime {release}")
        });
        assert!(!runtime.platforms.is_empty());
    }

    #[test]
    fn catalog_refuses_unsafe_paths_and_uninstalled_libraries() {
        let base = |files: serde_json::Value, core: &str| {
            serde_json::json!({
                "schema_version": 1,
                "runtimes": { "1.0.0": { "cuda_major": 13, "minimum_driver_major": 580,
                    "platforms": { "p": {
                        "onnxruntime": { "core_library": core, "archive": {
                            "name": "a.zip", "url": "https://example.invalid/a.zip",
                            "sha256": "0".repeat(64), "size": 1, "files": files } },
                        "cuda": { "libraries": [], "archives": [] } } } } },
                "links": []
            })
            .to_string()
        };
        assert!(
            parse_catalog(&base(
                serde_json::json!({ "lib/core.dll": "core.dll" }),
                "core.dll"
            ))
            .is_ok()
        );
        assert!(
            parse_catalog(&base(
                serde_json::json!({ "../core.dll": "core.dll" }),
                "core.dll"
            ))
            .is_err()
        );
        assert!(
            parse_catalog(&base(
                serde_json::json!({ "lib/core.dll": "../core.dll" }),
                "core.dll"
            ))
            .is_err()
        );
        assert!(
            parse_catalog(&base(
                serde_json::json!({ "lib/other.dll": "other.dll" }),
                "core.dll"
            ))
            .is_err()
        );
    }

    #[test]
    fn installs_declared_members_and_reports_the_component_installed() {
        let root = tempfile::tempdir().unwrap();
        let bytes = zip_bytes(&[
            ("pkg/lib/core.dll", b"core"),
            ("pkg/lib/unused.pdb", b"symbols"),
        ]);
        let archive = ArchiveEntry {
            name: String::from("ort.zip"),
            url: String::from("https://example.invalid/ort.zip"),
            sha256: {
                let path = root.path().join("probe.zip");
                fs::write(&path, &bytes).unwrap();
                sha256_file_hex(&path).unwrap()
            },
            size: bytes.len() as u64,
            files: BTreeMap::from([(String::from("pkg/lib/core.dll"), String::from("core.dll"))]),
        };
        let catalog = catalog_with(archive.clone());
        let target = CudaRuntimeTarget::select(&catalog, "9.9.9", "test-x86_64")
            .unwrap()
            .unwrap();
        let managed = root.path().join("managed");
        // A verified archive already in the download directory installs without network access.
        // 下载目录中已校验的归档无需联网即可安装。
        fs::create_dir_all(managed.join(DOWNLOADS_DIR_NAME)).unwrap();
        fs::write(managed.join(DOWNLOADS_DIR_NAME).join("ort.zip"), &bytes).unwrap();
        assert!(installed_component(&target, &managed, CudaComponent::Onnxruntime).is_none());
        let directory =
            install_component(&target, CudaComponent::Onnxruntime, &managed, &mut |_| {}).unwrap();
        assert_eq!(fs::read(directory.join("core.dll")).unwrap(), b"core");
        assert!(!directory.join("unused.pdb").exists());
        assert!(!managed.join(DOWNLOADS_DIR_NAME).join("ort.zip").exists());
        assert_eq!(
            installed_component(&target, &managed, CudaComponent::Onnxruntime),
            Some(directory.clone())
        );
        fs::remove_file(directory.join("core.dll")).unwrap();
        assert!(installed_component(&target, &managed, CudaComponent::Onnxruntime).is_none());
    }

    /// Installs both components for this host from the real catalog into
    /// `VULCAN_JUDGE_CUDA_TEST_ROOT`; seed its `.downloads` with the official archives to skip the
    /// network. Run the worker self-check on the result separately.
    /// 按真实目录为本宿主安装两个组件到 `VULCAN_JUDGE_CUDA_TEST_ROOT`；预先放入官方归档到其 `.downloads`
    /// 可跳过联网。安装结果另行用 worker 自检验证。
    #[test]
    #[ignore = "installs the real CUDA runtime; needs VULCAN_JUDGE_CUDA_TEST_ROOT"]
    fn installs_the_real_cuda_runtime() {
        let root = PathBuf::from(std::env::var_os("VULCAN_JUDGE_CUDA_TEST_ROOT").unwrap());
        let lock: serde_json::Value =
            serde_json::from_str(include_str!("../../../scripts/build-inputs.lock.json")).unwrap();
        let release = lock["judge_runtime"]["onnxruntime"].as_str().unwrap();
        let target = CudaRuntimeTarget::for_host(release).unwrap().unwrap();
        for component in [CudaComponent::Onnxruntime, CudaComponent::CudaLibraries] {
            let directory = install_component(&target, component, &root, &mut |_| {}).unwrap();
            println!("{component:?}: {}", directory.display());
        }
        let found = find_cuda_library_directories(&target, &root);
        assert_eq!(found[0].source, CudaLibrarySource::Managed);
        println!("driver: {:?}", detect_nvidia_driver().unwrap());
    }

    #[test]
    fn complete_directories_only_and_driver_rows_parse() {
        let root = tempfile::tempdir().unwrap();
        let catalog = catalog_with(ArchiveEntry {
            name: String::from("ort.zip"),
            url: String::from("https://example.invalid/ort.zip"),
            sha256: "0".repeat(64),
            size: 1,
            files: BTreeMap::from([(String::from("core.dll"), String::from("core.dll"))]),
        });
        let target = CudaRuntimeTarget::select(&catalog, "9.9.9", "test-x86_64")
            .unwrap()
            .unwrap();
        assert_eq!(
            missing_cuda_libraries(&target, root.path()),
            vec!["blas.dll"]
        );
        fs::write(root.path().join("blas.dll"), b"blas").unwrap();
        assert!(missing_cuda_libraries(&target, root.path()).is_empty());
        assert!(
            CudaRuntimeTarget::select(&catalog, "1.0.0", "test-x86_64")
                .unwrap()
                .is_none()
        );

        let driver =
            parse_nvidia_smi("591.86, NVIDIA GeForce RTX 5080\n591.86, NVIDIA RTX A2000\n")
                .unwrap();
        assert_eq!(driver.major, 591);
        assert_eq!(driver.gpus.len(), 2);
        assert!(parse_nvidia_smi("").is_err());
        assert_eq!(parse_compute_capabilities("12.0\n8.6\n"), Some((8, 6)));
        assert_eq!(parse_compute_capabilities("8.9\n[N/A]\n"), None);
        assert_eq!(parse_compute_capabilities(""), None);
        assert_eq!(parse_toolkit_version("13.2", 13), Some((13, 2)));
        assert_eq!(parse_toolkit_version("12.8", 13), None);
    }
}
