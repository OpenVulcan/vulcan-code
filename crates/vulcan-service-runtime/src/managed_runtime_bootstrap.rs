//! Managed Python and Node runtime bootstrap for the active LuaSkills root.
//! 活动 LuaSkills 根目录的受管 Python 与 Node 运行时初始化器。

use crate::managed_runtime_installer::{
    NativeManagedRuntimeInstallRequest, install_managed_runtimes,
};
use fs2::FileExt;
use serde::Deserialize;
use std::fs::{self, File, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

/// Maximum time spent waiting for another process to release the bootstrap lock.
/// 等待其他进程释放初始化锁的最长时间。
const LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Interval between non-blocking lock attempts.
/// 非阻塞锁尝试之间的轮询间隔。
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// LuaSkills release whose managed-runtime layout is implemented by this host.
/// 当前宿主实现其受管运行时目录契约的 LuaSkills 版本。
const EXPECTED_LUASKILLS_VERSION: &str = env!("VULCAN_LUASKILLS_DEPENDENCY_VERSION");

/// Exact CPython release packaged by this host integration.
/// 当前宿主集成打包的精确 CPython 版本。
const EXPECTED_PYTHON_VERSION: &str = env!("VULCAN_MANAGED_PYTHON_VERSION");

/// Exact uv release packaged by this host integration.
/// 当前宿主集成打包的精确 uv 版本。
const EXPECTED_UV_VERSION: &str = env!("VULCAN_MANAGED_UV_VERSION");

/// Exact Node.js release packaged by this host integration.
/// 当前宿主集成打包的精确 Node.js 版本。
const EXPECTED_NODE_VERSION: &str = env!("VULCAN_MANAGED_NODE_VERSION");

/// Exact pnpm release packaged by this host integration.
/// 当前宿主集成打包的精确 pnpm 版本。
const EXPECTED_PNPM_VERSION: &str = env!("VULCAN_MANAGED_PNPM_VERSION");

/// Comma-separated native targets published by the crate-owned runtime manifest.
/// crate 自有运行时清单发布的原生目标逗号分隔列表。
const SUPPORTED_PLATFORMS: &str = env!("VULCAN_MANAGED_SUPPORTED_PLATFORMS");

/// Final state of the managed runtime bootstrap.
/// 受管运行时初始化的最终状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedRuntimeBootstrapStatus {
    /// All declared runtimes were already valid.
    /// 所有声明的运行时原本已经有效。
    Ready,
    /// Missing runtimes were installed and validated.
    /// 缺失的运行时已安装并通过验证。
    Installed,
    /// The current native target is explicitly unsupported.
    /// 当前原生目标被明确标记为不支持。
    Unsupported,
    /// Bootstrap could not produce a complete valid layout.
    /// 初始化未能生成完整有效的目录布局。
    Unavailable,
}

/// Selects the managed runtime group installed by an explicit retry.
/// 选择显式重试要安装的受管运行时组。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedRuntimeInstallTarget {
    /// Installs only components that fail validation.
    /// 仅安装未通过校验的组件。
    Missing,
    /// Reinstalls Python and its uv companion.
    /// 重新安装 Python 及其配套 uv。
    Python,
    /// Reinstalls Node.js and its pnpm companion.
    /// 重新安装 Node.js 及其配套 pnpm。
    Node,
}

impl ManagedRuntimeInstallTarget {
    /// Parses the exact service wire key into an install target.
    /// 将精确的服务传输键解析为安装目标。
    pub fn from_key(value: &str) -> Result<Self, String> {
        match value {
            "python" => Ok(Self::Python),
            "node" => Ok(Self::Node),
            other => Err(format!(
                "unsupported managed runtime install target: {other}"
            )),
        }
    }
}

/// Describes one independently repairable managed runtime group.
/// 描述一个可独立修复的受管运行时组。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagedRuntimeComponentReport {
    /// Exact runtime version shown to the user.
    /// 向用户显示的精确运行时版本。
    pub version: String,
    /// Validation state for this runtime group.
    /// 当前运行时组的校验状态。
    pub status: ManagedRuntimeBootstrapStatus,
    /// Exact validation failure retained for diagnostics.
    /// 为诊断保留的精确校验失败信息。
    pub diagnostic: String,
}

impl Default for ManagedRuntimeBootstrapStatus {
    /// Uses unavailable as the safe empty-state value before the first bootstrap completes.
    /// 在首次初始化完成前使用不可用作为安全的空状态值。
    fn default() -> Self {
        Self::Unavailable
    }
}

impl ManagedRuntimeBootstrapStatus {
    /// Returns the stable wire key used by service and WebUI projections.
    /// 返回服务与 WebUI 投影使用的稳定传输键。
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Installed => "installed",
            Self::Unsupported => "unsupported",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Readable result returned after one bootstrap attempt.
/// 单次初始化尝试结束后返回的可读结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagedRuntimeBootstrapReport {
    /// Final bootstrap status.
    /// 最终初始化状态。
    pub status: ManagedRuntimeBootstrapStatus,
    /// Active LuaSkills runtime root.
    /// 活动 LuaSkills 运行时根目录。
    pub runtime_root: PathBuf,
    /// Stable platform key when the target is supported.
    /// 目标受支持时使用的稳定平台键。
    pub platform: Option<String>,
    /// Whether the native installer attempted to change the runtime layout.
    /// 原生安装器是否尝试修改运行时目录布局。
    pub install_attempted: bool,
    /// Concise human-readable diagnostic text.
    /// 简洁的可读诊断文本。
    pub diagnostic: String,
    /// Independently validated Python runtime state.
    /// 独立校验的 Python 运行时状态。
    pub python: ManagedRuntimeComponentReport,
    /// Independently validated Node.js runtime state.
    /// 独立校验的 Node.js 运行时状态。
    pub node: ManagedRuntimeComponentReport,
}

/// Structured progress emitted by the native managed-runtime installer.
/// 原生受管运行时安装器发出的结构化进度。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagedRuntimeBootstrapProgress {
    /// Stable installation phase key.
    /// 稳定的安装阶段键。
    pub phase: String,
    /// Runtime component currently being processed.
    /// 当前处理的运行时组件。
    pub component: Option<String>,
    /// Human-readable progress message.
    /// 可读的进度消息。
    pub message: String,
    /// Bytes transferred for the active download.
    /// 当前下载已传输的字节数。
    pub completed_bytes: Option<u64>,
    /// Total bytes advertised for the active download.
    /// 当前下载声明的总字节数。
    pub total_bytes: Option<u64>,
    /// Overall bootstrap completion percentage.
    /// 整体初始化完成百分比。
    pub overall_percent: Option<u8>,
}

/// Exact managed runtime versions packaged with the application.
/// 随应用打包的受管运行时精确版本配置。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct ManagedRuntimeBootstrapConfig {
    /// Configuration schema version.
    /// 配置 schema 版本。
    schema_version: u32,
    /// LuaSkills release matched by the copied scripts.
    /// 与复制脚本匹配的 LuaSkills 发布版本。
    luaskills_version: String,
    /// Exact CPython version.
    /// 精确 CPython 版本。
    python_version: String,
    /// Exact uv version.
    /// 精确 uv 版本。
    uv_version: String,
    /// Exact Node.js version.
    /// 精确 Node.js 版本。
    node_version: String,
    /// Exact pnpm version.
    /// 精确 pnpm 版本。
    pnpm_version: String,
    /// Native platform keys supported by the packaged scripts.
    /// 打包脚本支持的原生平台键。
    supported_platforms: Vec<String>,
}

/// Runtime manifest emitted by the native installer.
/// 原生安装器生成的运行时清单。
#[derive(Debug, Deserialize)]
struct RuntimeInstallManifest {
    /// Manifest schema version.
    /// 清单 schema 版本。
    schema_version: u32,
    /// Runtime or package-manager identifier.
    /// 运行时或包管理器标识。
    runtime: String,
    /// Exact installed version.
    /// 已安装的精确版本。
    version: String,
    /// Installed platform key or `any`.
    /// 安装平台键或 `any`。
    platform: String,
    /// Executable path relative to the installation directory.
    /// 相对于安装目录的可执行文件路径。
    executable: String,
}

/// Expected identity and location of one managed runtime installation.
/// 单个受管运行时安装的预期身份与位置。
struct RuntimeExpectation {
    /// Expected runtime identifier.
    /// 预期运行时标识。
    runtime: &'static str,
    /// Expected exact version.
    /// 预期精确版本。
    version: String,
    /// Expected platform field.
    /// 预期平台字段。
    platform: String,
    /// Expected versioned installation directory.
    /// 预期的版本化安装目录。
    install_dir: PathBuf,
}

/// Bootstrap managed Python and Node runtimes for one active LuaSkills root.
/// 为一个活动 LuaSkills 根目录初始化受管 Python 与 Node 运行时。
///
/// `runtime_root` is the exact root passed to `LuaRuntimeHostOptions`.
/// `runtime_root` 是传入 `LuaRuntimeHostOptions` 的同一个精确根目录。
/// `distribution_root` is the host-owned shared directory containing Python and Node distributions.
/// `distribution_root` 是包含 Python 与 Node 发行包的宿主共享目录。
/// `progress` receives concise startup messages and must not block.
/// `progress` 接收简洁启动消息，且不得阻塞。
/// The function always returns a report so ordinary Lua skills can remain available when downloads fail.
/// 此函数始终返回报告，使下载失败时普通 Lua 技能仍可继续使用。
pub fn bootstrap_managed_runtimes<F>(
    runtime_root: &Path,
    distribution_root: &Path,
    progress: F,
) -> ManagedRuntimeBootstrapReport
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    bootstrap_managed_runtimes_for_target(
        runtime_root,
        distribution_root,
        ManagedRuntimeInstallTarget::Missing,
        progress,
    )
}

/// Bootstraps or explicitly reinstalls one managed runtime group.
/// 初始化或显式重新安装一个受管运行时组。
pub fn bootstrap_managed_runtimes_for_target<F>(
    runtime_root: &Path,
    distribution_root: &Path,
    target: ManagedRuntimeInstallTarget,
    mut progress: F,
) -> ManagedRuntimeBootstrapReport
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    let config = compiled_bootstrap_config();
    let platform = match managed_runtime_platform_key() {
        Ok(platform) => platform,
        Err(reason) => {
            return report(
                runtime_root,
                distribution_root,
                &config,
                ManagedRuntimeBootstrapStatus::Unsupported,
                None,
                false,
                reason,
            );
        }
    };

    if !config
        .supported_platforms
        .iter()
        .any(|item| item == &platform)
    {
        return report(
            runtime_root,
            distribution_root,
            &config,
            ManagedRuntimeBootstrapStatus::Unsupported,
            Some(platform.clone()),
            false,
            format!("managed runtimes are not published for {platform}"),
        );
    }

    if let Err(error) = validate_config(&config) {
        return report(
            runtime_root,
            distribution_root,
            &config,
            ManagedRuntimeBootstrapStatus::Unavailable,
            Some(platform),
            false,
            error,
        );
    }
    if target == ManagedRuntimeInstallTarget::Missing
        && validate_runtime_layout(distribution_root, &platform, &config).is_ok()
    {
        return report(
            runtime_root,
            distribution_root,
            &config,
            ManagedRuntimeBootstrapStatus::Ready,
            Some(platform),
            false,
            "managed Python and Node runtimes are ready".to_string(),
        );
    }

    progress(&ManagedRuntimeBootstrapProgress {
        phase: "locking".to_string(),
        message: "正在等待受管 Python/Node 运行时初始化锁".to_string(),
        overall_percent: Some(2),
        ..ManagedRuntimeBootstrapProgress::default()
    });
    let lock_file = match acquire_bootstrap_lock(runtime_root) {
        Ok(file) => file,
        Err(error) => {
            return report(
                runtime_root,
                distribution_root,
                &config,
                ManagedRuntimeBootstrapStatus::Unavailable,
                Some(platform),
                false,
                error,
            );
        }
    };

    if target == ManagedRuntimeInstallTarget::Missing
        && validate_runtime_layout(distribution_root, &platform, &config).is_ok()
    {
        let _ = FileExt::unlock(&lock_file);
        return report(
            runtime_root,
            distribution_root,
            &config,
            ManagedRuntimeBootstrapStatus::Ready,
            Some(platform),
            false,
            "managed runtimes became ready while waiting for the bootstrap lock".to_string(),
        );
    }

    let invalid = runtime_expectations(distribution_root, &platform, &config)
        .into_iter()
        .map(|expectation| {
            let runtime = expectation.runtime;
            (
                runtime,
                validate_runtime_installation(&expectation).is_err(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let (install_uv, install_python, install_node, install_pnpm) = install_flags(target, &invalid);
    let request = NativeManagedRuntimeInstallRequest {
        runtime_root,
        distribution_root,
        platform: &platform,
        python_version: &config.python_version,
        uv_version: &config.uv_version,
        node_version: &config.node_version,
        pnpm_version: &config.pnpm_version,
        install_uv,
        install_python,
        install_node,
        install_pnpm,
    };
    let install_result = install_managed_runtimes(&request, &mut progress);
    let _ = FileExt::unlock(&lock_file);
    match install_result {
        Ok(()) => match validate_runtime_layout(distribution_root, &platform, &config) {
            Ok(()) => report(
                runtime_root,
                distribution_root,
                &config,
                ManagedRuntimeBootstrapStatus::Installed,
                Some(platform),
                true,
                "managed Python and Node runtimes were installed successfully".to_string(),
            ),
            Err(error) => report(
                runtime_root,
                distribution_root,
                &config,
                ManagedRuntimeBootstrapStatus::Unavailable,
                Some(platform),
                true,
                format!("native installer completed but validation failed: {error}"),
            ),
        },
        Err(error) => report(
            runtime_root,
            distribution_root,
            &config,
            ManagedRuntimeBootstrapStatus::Unavailable,
            Some(platform),
            true,
            error,
        ),
    }
}

/// Build one bootstrap report without duplicating field initialization.
/// 构造单个初始化报告，避免重复字段初始化。
fn report(
    runtime_root: &Path,
    distribution_root: &Path,
    config: &ManagedRuntimeBootstrapConfig,
    status: ManagedRuntimeBootstrapStatus,
    platform: Option<String>,
    install_attempted: bool,
    diagnostic: String,
) -> ManagedRuntimeBootstrapReport {
    let (python, node) = component_reports(distribution_root, platform.as_deref(), config);
    ManagedRuntimeBootstrapReport {
        status,
        runtime_root: runtime_root.to_path_buf(),
        platform,
        install_attempted,
        diagnostic,
        python,
        node,
    }
}

/// Resolves exact installer flags for the requested repair scope.
/// 为请求的修复范围解析精确的安装器开关。
fn install_flags(
    target: ManagedRuntimeInstallTarget,
    invalid: &std::collections::HashMap<&'static str, bool>,
) -> (bool, bool, bool, bool) {
    match target {
        ManagedRuntimeInstallTarget::Missing => (
            invalid.get("uv").copied().unwrap_or(true),
            invalid.get("python").copied().unwrap_or(true),
            invalid.get("node").copied().unwrap_or(true),
            invalid.get("pnpm").copied().unwrap_or(true),
        ),
        ManagedRuntimeInstallTarget::Python => (true, true, false, false),
        ManagedRuntimeInstallTarget::Node => (false, false, true, true),
    }
}

/// Validates Python and Node.js groups independently for WebUI status rows.
/// 为 WebUI 状态行独立校验 Python 与 Node.js 运行时组。
fn component_reports(
    distribution_root: &Path,
    platform: Option<&str>,
    config: &ManagedRuntimeBootstrapConfig,
) -> (ManagedRuntimeComponentReport, ManagedRuntimeComponentReport) {
    let Some(platform) = platform else {
        return (
            unsupported_component(&config.python_version),
            unsupported_component(&config.node_version),
        );
    };
    let expectations = runtime_expectations(distribution_root, platform, config);
    let python = component_report(&config.python_version, &expectations[0..2]);
    let node = component_report(&config.node_version, &expectations[2..4]);
    (python, node)
}

/// Builds one unavailable component report for an unsupported platform.
/// 为不受支持的平台构建一个不可用组件报告。
fn unsupported_component(version: &str) -> ManagedRuntimeComponentReport {
    ManagedRuntimeComponentReport {
        version: version.to_string(),
        status: ManagedRuntimeBootstrapStatus::Unsupported,
        diagnostic: String::from("managed runtime is not supported on this platform"),
    }
}

/// Validates every exact installation belonging to one runtime group.
/// 校验属于一个运行时组的每个精确安装项。
fn component_report(
    version: &str,
    expectations: &[RuntimeExpectation],
) -> ManagedRuntimeComponentReport {
    let errors = expectations
        .iter()
        .filter_map(|expectation| validate_runtime_installation(expectation).err())
        .collect::<Vec<_>>();
    if errors.is_empty() {
        ManagedRuntimeComponentReport {
            version: version.to_string(),
            status: ManagedRuntimeBootstrapStatus::Ready,
            diagnostic: String::new(),
        }
    } else {
        ManagedRuntimeComponentReport {
            version: version.to_string(),
            status: ManagedRuntimeBootstrapStatus::Unavailable,
            diagnostic: errors.join("; "),
        }
    }
}

/// Build the immutable runtime configuration exported from the crate-owned manifest.
/// 构造从 crate 自有清单导出的不可变运行时配置。
fn compiled_bootstrap_config() -> ManagedRuntimeBootstrapConfig {
    ManagedRuntimeBootstrapConfig {
        schema_version: 1,
        luaskills_version: EXPECTED_LUASKILLS_VERSION.to_string(),
        python_version: EXPECTED_PYTHON_VERSION.to_string(),
        uv_version: EXPECTED_UV_VERSION.to_string(),
        node_version: EXPECTED_NODE_VERSION.to_string(),
        pnpm_version: EXPECTED_PNPM_VERSION.to_string(),
        supported_platforms: SUPPORTED_PLATFORMS.split(',').map(str::to_string).collect(),
    }
}

/// Validate immutable configuration fields before any process is launched.
/// 在启动任何进程前校验不可变配置字段。
fn validate_config(config: &ManagedRuntimeBootstrapConfig) -> Result<(), String> {
    if config.schema_version != 1 {
        return Err(format!(
            "unsupported managed runtime config schema: {}",
            config.schema_version
        ));
    }
    for (field, actual, expected) in [
        (
            "luaskills_version",
            config.luaskills_version.as_str(),
            EXPECTED_LUASKILLS_VERSION,
        ),
        (
            "python_version",
            config.python_version.as_str(),
            EXPECTED_PYTHON_VERSION,
        ),
        (
            "uv_version",
            config.uv_version.as_str(),
            EXPECTED_UV_VERSION,
        ),
        (
            "node_version",
            config.node_version.as_str(),
            EXPECTED_NODE_VERSION,
        ),
        (
            "pnpm_version",
            config.pnpm_version.as_str(),
            EXPECTED_PNPM_VERSION,
        ),
    ] {
        if actual != expected {
            return Err(format!(
                "managed runtime {field} is {actual}, expected {expected}"
            ));
        }
    }
    Ok(())
}

/// Return the stable platform key accepted by the upstream scripts.
/// 返回上游脚本接受的稳定平台键。
fn managed_runtime_platform_key() -> Result<String, String> {
    managed_runtime_platform_key_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// Resolve one explicit OS and architecture pair to the packaged platform key.
/// 把一个显式操作系统与架构组合解析为打包平台键。
fn managed_runtime_platform_key_for(os: &str, arch: &str) -> Result<String, String> {
    match (os, arch) {
        ("windows", "x86_64") => Ok("windows-x64".to_string()),
        ("windows", "aarch64") => Err("windows_arm_is_not_supported".to_string()),
        ("linux", "x86_64") => Ok("linux-x64".to_string()),
        ("linux", "aarch64") => Ok("linux-arm64".to_string()),
        ("macos", "x86_64") => Ok("macos-x64".to_string()),
        ("macos", "aarch64") => Ok("macos-arm64".to_string()),
        (os, arch) => Err(format!("unsupported managed runtime target: {os}-{arch}")),
    }
}

/// Validate all exact runtime installations required by one configuration.
/// 校验一份配置所需的全部精确运行时安装。
fn validate_runtime_layout(
    distribution_root: &Path,
    platform: &str,
    config: &ManagedRuntimeBootstrapConfig,
) -> Result<(), String> {
    for expectation in runtime_expectations(distribution_root, platform, config) {
        validate_runtime_installation(&expectation)?;
    }
    Ok(())
}

/// Build the four exact runtime installation expectations.
/// 构造四个精确运行时安装预期。
fn runtime_expectations(
    distribution_root: &Path,
    platform: &str,
    config: &ManagedRuntimeBootstrapConfig,
) -> Vec<RuntimeExpectation> {
    let python_root = distribution_root.join("python");
    let node_root = distribution_root.join("node");
    vec![
        RuntimeExpectation {
            runtime: "python",
            version: config.python_version.clone(),
            platform: platform.to_string(),
            install_dir: python_root.join(format!("cpython-{}-{platform}", config.python_version)),
        },
        RuntimeExpectation {
            runtime: "uv",
            version: config.uv_version.clone(),
            platform: platform.to_string(),
            install_dir: python_root.join(format!("uv-{}-{platform}", config.uv_version)),
        },
        RuntimeExpectation {
            runtime: "node",
            version: config.node_version.clone(),
            platform: platform.to_string(),
            install_dir: node_root.join(format!("node-{}-{platform}", config.node_version)),
        },
        RuntimeExpectation {
            runtime: "pnpm",
            version: config.pnpm_version.clone(),
            platform: "any".to_string(),
            install_dir: node_root.join(format!("pnpm-{}", config.pnpm_version)),
        },
    ]
}

/// Validate one manifest and its contained executable path.
/// 校验单个清单及其受目录约束的可执行文件路径。
fn validate_runtime_installation(expectation: &RuntimeExpectation) -> Result<(), String> {
    let manifest_path = expectation.install_dir.join("runtime-manifest.json");
    let text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
    let manifest: RuntimeInstallManifest = serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse {}: {error}", manifest_path.display()))?;
    if manifest.schema_version != 1
        || manifest.runtime != expectation.runtime
        || manifest.version != expectation.version
        || manifest.platform != expectation.platform
    {
        return Err(format!(
            "managed runtime manifest identity mismatch: {}",
            manifest_path.display()
        ));
    }

    let executable_relative = Path::new(&manifest.executable);
    if manifest.executable.trim().is_empty()
        || executable_relative.is_absolute()
        || !executable_relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "managed runtime executable is not a safe relative path: {}",
            manifest_path.display()
        ));
    }
    let canonical_install_dir = fs::canonicalize(&expectation.install_dir).map_err(|error| {
        format!(
            "failed to canonicalize {}: {error}",
            expectation.install_dir.display()
        )
    })?;
    let executable_path = expectation.install_dir.join(executable_relative);
    let canonical_executable = fs::canonicalize(&executable_path)
        .map_err(|error| format!("failed to resolve {}: {error}", executable_path.display()))?;
    if !canonical_executable.starts_with(&canonical_install_dir)
        || !runtime_executable_is_usable(&canonical_executable)
    {
        return Err(format!(
            "managed runtime executable escapes its installation directory or is not executable: {}",
            executable_path.display()
        ));
    }
    Ok(())
}

/// Verify one manifest target is a file executable by the current native platform.
/// 验证一个清单目标是当前原生平台可执行的文件。
fn runtime_executable_is_usable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Acquire the cross-process bootstrap lock with a bounded wait.
/// 通过有界等待获取跨进程初始化锁。
fn acquire_bootstrap_lock(runtime_root: &Path) -> Result<File, String> {
    let state_dir = runtime_root.join("state");
    fs::create_dir_all(&state_dir)
        .map_err(|error| format!("failed to create {}: {error}", state_dir.display()))?;
    let lock_path = state_dir.join("managed-runtime-bootstrap.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| format!("failed to open {}: {error}", lock_path.display()))?;
    let deadline = Instant::now() + LOCK_WAIT_TIMEOUT;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(LOCK_POLL_INTERVAL);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Err(format!(
                    "timed out waiting for managed runtime bootstrap lock {}: {error}",
                    lock_path.display()
                ));
            }
            Err(error) => {
                return Err(format!(
                    "failed to acquire managed runtime bootstrap lock {}: {error}",
                    lock_path.display()
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Build one unique temporary directory without an external test dependency.
    /// 在不引入外部测试依赖的情况下构造唯一临时目录。
    fn test_root(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("vulcan-managed-runtime-{label}-{suffix}"))
    }

    /// Write one minimal valid runtime installation used by layout tests.
    /// 写入一个供目录测试使用的最小有效运行时安装。
    fn write_install(expectation: &RuntimeExpectation) {
        fs::create_dir_all(&expectation.install_dir).expect("install directory should be created");
        let executable = expectation.install_dir.join("runtime-bin");
        fs::write(&executable, b"test").expect("test executable should be written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&executable)
                .expect("test executable metadata should be readable")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&executable, permissions)
                .expect("test executable should be marked executable");
        }
        let manifest = serde_json::json!({
            "schema_version": 1,
            "runtime": expectation.runtime,
            "version": expectation.version,
            "platform": expectation.platform,
            "executable": "runtime-bin"
        });
        fs::write(
            expectation.install_dir.join("runtime-manifest.json"),
            serde_json::to_vec(&manifest).expect("manifest should serialize"),
        )
        .expect("manifest should be written");
    }

    /// Verify a complete exact layout passes validation.
    /// 验证完整精确目录可以通过校验。
    #[test]
    fn exact_runtime_layout_is_accepted() {
        let root = test_root("valid");
        let config = ManagedRuntimeBootstrapConfig {
            schema_version: 1,
            luaskills_version: EXPECTED_LUASKILLS_VERSION.to_string(),
            python_version: EXPECTED_PYTHON_VERSION.to_string(),
            uv_version: "0.11.28".to_string(),
            node_version: "24.18.0".to_string(),
            pnpm_version: "11.11.0".to_string(),
            supported_platforms: vec!["windows-x64".to_string()],
        };
        for expectation in runtime_expectations(&root, "windows-x64", &config) {
            write_install(&expectation);
        }
        validate_runtime_layout(&root, "windows-x64", &config)
            .expect("complete runtime layout should validate");
        fs::remove_dir_all(root).expect("test root should be removed");
    }

    /// Verify Python and Node.js validation states remain independent.
    /// 验证 Python 与 Node.js 的校验状态保持相互独立。
    #[test]
    fn runtime_component_reports_are_independent() {
        let root = test_root("component-report");
        let config = compiled_bootstrap_config();
        let expectations = runtime_expectations(&root, "windows-x64", &config);
        for expectation in &expectations[0..2] {
            write_install(expectation);
        }

        let (python, node) = component_reports(&root, Some("windows-x64"), &config);

        assert_eq!(python.status, ManagedRuntimeBootstrapStatus::Ready);
        assert_eq!(python.version, EXPECTED_PYTHON_VERSION);
        assert_eq!(node.status, ManagedRuntimeBootstrapStatus::Unavailable);
        assert_eq!(node.version, EXPECTED_NODE_VERSION);
        fs::remove_dir_all(root).expect("test root should be removed");
    }

    /// Verify explicit repairs reinstall only the selected runtime group.
    /// 验证显式修复仅重新安装选中的运行时组。
    #[test]
    fn explicit_runtime_reinstall_flags_are_isolated() {
        let invalid = std::collections::HashMap::from([
            ("uv", false),
            ("python", false),
            ("node", false),
            ("pnpm", false),
        ]);

        assert_eq!(
            install_flags(ManagedRuntimeInstallTarget::Python, &invalid),
            (true, true, false, false)
        );
        assert_eq!(
            install_flags(ManagedRuntimeInstallTarget::Node, &invalid),
            (false, false, true, true)
        );
    }

    /// Verify service wire targets reject every unrecognized repair scope.
    /// 验证服务传输目标会拒绝所有无法识别的修复范围。
    #[test]
    fn managed_runtime_install_target_is_strict() {
        assert_eq!(
            ManagedRuntimeInstallTarget::from_key("python")
                .expect("python target should be accepted"),
            ManagedRuntimeInstallTarget::Python
        );
        assert_eq!(
            ManagedRuntimeInstallTarget::from_key("node").expect("node target should be accepted"),
            ManagedRuntimeInstallTarget::Node
        );
        assert!(ManagedRuntimeInstallTarget::from_key("all").is_err());
    }

    /// Verify the packaged download manifest matches the exact compiled LuaSkills dependency.
    /// 验证打包下载清单与编译使用的精确 LuaSkills 依赖版本一致。
    #[test]
    fn repository_download_manifest_matches_compiled_luaskills_version() {
        let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("managed-runtimes.json");
        let text = fs::read_to_string(&config_path).expect("read managed runtime config");
        let config = serde_json::from_str::<ManagedRuntimeBootstrapConfig>(&text)
            .expect("parse managed runtime config");
        assert_eq!(config.luaskills_version, EXPECTED_LUASKILLS_VERSION);
        validate_config(&config).expect("validate exact packaged runtime versions");
    }

    /// Verify packaged version configuration cannot redirect startup downloads.
    /// 验证打包版本配置不能把启动下载重定向到其他版本。
    #[test]
    fn unexpected_managed_runtime_version_is_rejected() {
        let config = ManagedRuntimeBootstrapConfig {
            schema_version: 1,
            luaskills_version: EXPECTED_LUASKILLS_VERSION.to_string(),
            python_version: "3.14.5".to_string(),
            uv_version: EXPECTED_UV_VERSION.to_string(),
            node_version: EXPECTED_NODE_VERSION.to_string(),
            pnpm_version: EXPECTED_PNPM_VERSION.to_string(),
            supported_platforms: Vec::new(),
        };

        let error = validate_config(&config).expect_err("unexpected version should be rejected");

        assert!(error.contains("python_version"));
        assert!(error.contains(EXPECTED_PYTHON_VERSION));
    }

    /// Verify every published native target resolves to its stable directory key.
    /// 验证每个已发布原生目标都会解析到稳定目录键。
    #[test]
    fn published_platform_matrix_is_stable() {
        for (os, arch, expected) in [
            ("windows", "x86_64", "windows-x64"),
            ("linux", "x86_64", "linux-x64"),
            ("linux", "aarch64", "linux-arm64"),
            ("macos", "x86_64", "macos-x64"),
            ("macos", "aarch64", "macos-arm64"),
        ] {
            assert_eq!(
                managed_runtime_platform_key_for(os, arch).expect("platform should be supported"),
                expected
            );
        }
    }

    /// Verify Windows ARM remains an explicit stable unsupported target.
    /// 验证 Windows ARM 始终返回显式且稳定的不支持状态。
    #[test]
    fn windows_arm_is_explicitly_unsupported() {
        let error = managed_runtime_platform_key_for("windows", "aarch64")
            .expect_err("Windows ARM should remain unsupported");

        assert_eq!(error, "windows_arm_is_not_supported");
    }

    /// Verify a manifest version mismatch is rejected before script reuse.
    /// 验证清单版本不匹配会在复用脚本前被拒绝。
    #[test]
    fn mismatched_runtime_manifest_is_rejected() {
        let root = test_root("mismatch");
        let expectation = RuntimeExpectation {
            runtime: "node",
            version: "24.18.0".to_string(),
            platform: "windows-x64".to_string(),
            install_dir: root.join("node"),
        };
        write_install(&expectation);
        let manifest_path = expectation.install_dir.join("runtime-manifest.json");
        let text = fs::read_to_string(&manifest_path).expect("manifest should be readable");
        fs::write(&manifest_path, text.replace("24.18.0", "24.17.0"))
            .expect("manifest should be replaced");
        let error = validate_runtime_installation(&expectation)
            .expect_err("mismatched manifest should fail");
        assert!(error.contains("identity mismatch"));
        fs::remove_dir_all(root).expect("test root should be removed");
    }

    /// Verify unsafe executable paths are rejected.
    /// 验证不安全的可执行文件路径会被拒绝。
    #[test]
    fn executable_path_traversal_is_rejected() {
        let root = test_root("traversal");
        let install_dir = root.join("node");
        fs::create_dir_all(&install_dir).expect("install directory should be created");
        fs::write(root.join("outside"), b"test").expect("outside file should be written");
        fs::write(
            install_dir.join("runtime-manifest.json"),
            br#"{"schema_version":1,"runtime":"node","version":"24.18.0","platform":"windows-x64","executable":"../outside"}"#,
        )
        .expect("manifest should be written");
        let expectation = RuntimeExpectation {
            runtime: "node",
            version: "24.18.0".to_string(),
            platform: "windows-x64".to_string(),
            install_dir,
        };
        let error =
            validate_runtime_installation(&expectation).expect_err("path traversal should fail");
        assert!(error.contains("safe relative path"));
        fs::remove_dir_all(root).expect("test root should be removed");
    }

    /// Downloads and validates all runtimes through the native Rust installer.
    /// 通过 Rust 原生安装器下载并校验全部运行时。
    #[test]
    #[ignore = "requires internet access and downloads complete native runtimes"]
    fn native_installer_downloads_complete_runtime_layout() {
        let runtime_root = std::env::var_os("VULCAN_MANAGED_RUNTIME_SMOKE_ROOT")
            .map(PathBuf::from)
            .expect("VULCAN_MANAGED_RUNTIME_SMOKE_ROOT must be set");
        let distribution_root = std::env::var_os("VULCAN_MANAGED_RUNTIME_DISTRIBUTION_ROOT")
            .map(PathBuf::from)
            .expect("VULCAN_MANAGED_RUNTIME_DISTRIBUTION_ROOT must be set");
        let mut progress_events = Vec::new();
        let report = bootstrap_managed_runtimes(&runtime_root, &distribution_root, |progress| {
            progress_events.push(progress.clone());
        });
        let platform = managed_runtime_platform_key().expect("supported smoke-test platform");
        assert_eq!(report.status, ManagedRuntimeBootstrapStatus::Installed);
        assert!(report.install_attempted);
        assert!(!progress_events.is_empty());
        validate_runtime_layout(&distribution_root, &platform, &compiled_bootstrap_config())
            .expect("native installed runtime layout must satisfy the host contract");
    }

    /// Confirms a complete native runtime root is reused without reinstalling.
    /// 确认完整原生运行时根会在不重新安装的情况下复用。
    #[test]
    #[ignore = "requires the native smoke test runtime layout"]
    fn native_bootstrap_is_idempotent() {
        let runtime_root = std::env::var_os("VULCAN_MANAGED_RUNTIME_SMOKE_ROOT")
            .map(PathBuf::from)
            .expect("VULCAN_MANAGED_RUNTIME_SMOKE_ROOT must be set");
        let distribution_root = std::env::var_os("VULCAN_MANAGED_RUNTIME_DISTRIBUTION_ROOT")
            .map(PathBuf::from)
            .expect("VULCAN_MANAGED_RUNTIME_DISTRIBUTION_ROOT must be set");
        let report = bootstrap_managed_runtimes(&runtime_root, &distribution_root, |_| {});
        assert_eq!(report.status, ManagedRuntimeBootstrapStatus::Ready);
        assert!(!report.install_attempted);
    }
}
