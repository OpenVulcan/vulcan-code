//! Managed Python and Node runtime bootstrap for the active LuaSkills root.
//! 活动 LuaSkills 根目录的受管 Python 与 Node 运行时初始化器。

use fs2::FileExt;
use serde::Deserialize;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Maximum time spent waiting for another process to release the bootstrap lock.
/// 等待其他进程释放初始化锁的最长时间。
const LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Interval between non-blocking lock attempts.
/// 非阻塞锁尝试之间的轮询间隔。
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Maximum stdout or stderr bytes retained in one diagnostic report.
/// 单个诊断报告保留的 stdout 或 stderr 最大字节数。
const MAX_DIAGNOSTIC_BYTES: usize = 8 * 1024;

/// Maximum wall-clock time allowed for one upstream fetch script execution.
/// 单次上游拉取脚本执行允许的最长墙钟时间。
const FETCH_SCRIPT_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Poll interval used while supervising the upstream fetch process.
/// 监督上游拉取进程时使用的轮询间隔。
const FETCH_SCRIPT_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// LuaSkills release whose managed-runtime layout is implemented by this host.
/// 当前宿主实现其受管运行时目录契约的 LuaSkills 版本。
const EXPECTED_LUASKILLS_VERSION: &str = "0.5.0";

/// Exact CPython release packaged by this host integration.
/// 当前宿主集成打包的精确 CPython 版本。
const EXPECTED_PYTHON_VERSION: &str = "3.14.4";

/// Exact uv release packaged by this host integration.
/// 当前宿主集成打包的精确 uv 版本。
const EXPECTED_UV_VERSION: &str = "0.11.28";

/// Exact Node.js release packaged by this host integration.
/// 当前宿主集成打包的精确 Node.js 版本。
const EXPECTED_NODE_VERSION: &str = "24.18.0";

/// Exact pnpm release packaged by this host integration.
/// 当前宿主集成打包的精确 pnpm 版本。
const EXPECTED_PNPM_VERSION: &str = "11.11.0";

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

/// Readable result returned after one bootstrap attempt.
/// 单次初始化尝试结束后返回的可读结果。
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Whether the upstream fetch script was executed.
    /// 是否执行过上游拉取脚本。
    pub script_executed: bool,
    /// Concise human-readable diagnostic text.
    /// 简洁的可读诊断文本。
    pub diagnostic: String,
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

/// Runtime manifest emitted by the upstream fetch scripts.
/// 上游拉取脚本生成的运行时清单。
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

/// One fetch-script failure with an exact process-start state.
/// 一次带有精确进程启动状态的拉取脚本失败。
struct FetchScriptError {
    /// Whether the script process was successfully started.
    /// 脚本进程是否已成功启动。
    started: bool,
    /// Readable bounded failure diagnostic.
    /// 可读的有界失败诊断。
    message: String,
}

/// Bootstrap managed Python and Node runtimes for one active LuaSkills root.
/// 为一个活动 LuaSkills 根目录初始化受管 Python 与 Node 运行时。
///
/// `runtime_root` is the exact root passed to `LuaRuntimeHostOptions`.
/// `runtime_root` 是传入 `LuaRuntimeHostOptions` 的同一个精确根目录。
/// `progress` receives concise startup messages and must not block.
/// `progress` 接收简洁启动消息，且不得阻塞。
/// The function always returns a report so ordinary Lua skills can remain available when downloads fail.
/// 此函数始终返回报告，使下载失败时普通 Lua 技能仍可继续使用。
pub fn bootstrap_managed_runtimes<F>(
    runtime_root: &Path,
    mut progress: F,
) -> ManagedRuntimeBootstrapReport
where
    F: FnMut(&str),
{
    let platform = match managed_runtime_platform_key() {
        Ok(platform) => platform,
        Err(reason) => {
            return report(
                runtime_root,
                ManagedRuntimeBootstrapStatus::Unsupported,
                None,
                false,
                reason,
            );
        }
    };

    let config = match load_bootstrap_config(runtime_root) {
        Ok(config) => config,
        Err(error) => {
            return report(
                runtime_root,
                ManagedRuntimeBootstrapStatus::Unavailable,
                Some(platform),
                false,
                error,
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
            ManagedRuntimeBootstrapStatus::Unsupported,
            Some(platform.clone()),
            false,
            format!("managed runtimes are not published for {platform}"),
        );
    }

    if let Err(error) = validate_config(&config) {
        return report(
            runtime_root,
            ManagedRuntimeBootstrapStatus::Unavailable,
            Some(platform),
            false,
            error,
        );
    }
    if validate_runtime_layout(runtime_root, &platform, &config).is_ok() {
        return report(
            runtime_root,
            ManagedRuntimeBootstrapStatus::Ready,
            Some(platform),
            false,
            "managed Python and Node runtimes are ready".to_string(),
        );
    }

    progress("正在等待受管 Python/Node 运行时初始化锁");
    let lock_file = match acquire_bootstrap_lock(runtime_root) {
        Ok(file) => file,
        Err(error) => {
            return report(
                runtime_root,
                ManagedRuntimeBootstrapStatus::Unavailable,
                Some(platform),
                false,
                error,
            );
        }
    };

    if validate_runtime_layout(runtime_root, &platform, &config).is_ok() {
        let _ = FileExt::unlock(&lock_file);
        return report(
            runtime_root,
            ManagedRuntimeBootstrapStatus::Ready,
            Some(platform),
            false,
            "managed runtimes became ready while waiting for the bootstrap lock".to_string(),
        );
    }

    progress("正在按需拉取受管 Python/Node 运行时");
    let script_result = run_fetch_script(runtime_root, &config);
    let _ = FileExt::unlock(&lock_file);
    match script_result {
        Ok(output) if output.status.success() => {
            match validate_runtime_layout(runtime_root, &platform, &config) {
                Ok(()) => report(
                    runtime_root,
                    ManagedRuntimeBootstrapStatus::Installed,
                    Some(platform),
                    true,
                    "managed Python and Node runtimes were installed successfully".to_string(),
                ),
                Err(error) => report(
                    runtime_root,
                    ManagedRuntimeBootstrapStatus::Unavailable,
                    Some(platform),
                    true,
                    format!("fetch script completed but validation failed: {error}"),
                ),
            }
        }
        Ok(output) => report(
            runtime_root,
            ManagedRuntimeBootstrapStatus::Unavailable,
            Some(platform),
            true,
            format_process_failure(&output),
        ),
        Err(error) => report(
            runtime_root,
            ManagedRuntimeBootstrapStatus::Unavailable,
            Some(platform),
            error.started,
            error.message,
        ),
    }
}

/// Build one bootstrap report without duplicating field initialization.
/// 构造单个初始化报告，避免重复字段初始化。
fn report(
    runtime_root: &Path,
    status: ManagedRuntimeBootstrapStatus,
    platform: Option<String>,
    script_executed: bool,
    diagnostic: String,
) -> ManagedRuntimeBootstrapReport {
    ManagedRuntimeBootstrapReport {
        status,
        runtime_root: runtime_root.to_path_buf(),
        platform,
        script_executed,
        diagnostic,
    }
}

/// Load the exact runtime version configuration packaged under the active root.
/// 加载活动根目录下打包的精确运行时版本配置。
fn load_bootstrap_config(runtime_root: &Path) -> Result<ManagedRuntimeBootstrapConfig, String> {
    let config_path = runtime_root.join("scripts/deps/managed-runtimes.json");
    let text = fs::read_to_string(&config_path)
        .map_err(|error| format!("failed to read {}: {error}", config_path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse {}: {error}", config_path.display()))
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
    runtime_root: &Path,
    platform: &str,
    config: &ManagedRuntimeBootstrapConfig,
) -> Result<(), String> {
    for expectation in runtime_expectations(runtime_root, platform, config) {
        validate_runtime_installation(&expectation)?;
    }
    Ok(())
}

/// Build the four exact runtime installation expectations.
/// 构造四个精确运行时安装预期。
fn runtime_expectations(
    runtime_root: &Path,
    platform: &str,
    config: &ManagedRuntimeBootstrapConfig,
) -> Vec<RuntimeExpectation> {
    let python_root = runtime_root.join("dependencies/runtimes/python");
    let node_root = runtime_root.join("dependencies/runtimes/node");
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

/// Execute the platform-specific upstream fetch script with exact versions.
/// 使用精确版本执行平台对应的上游拉取脚本。
fn run_fetch_script(
    runtime_root: &Path,
    config: &ManagedRuntimeBootstrapConfig,
) -> Result<Output, FetchScriptError> {
    let mut command = build_fetch_script_command(runtime_root, config)?;
    configure_fetch_process_group(&mut command);
    let state_dir = runtime_root.join("state");
    fs::create_dir_all(&state_dir).map_err(|error| FetchScriptError {
        started: false,
        message: format!("failed to create {}: {error}", state_dir.display()),
    })?;
    let stdout_path = state_dir.join("managed-runtime-bootstrap.stdout.log");
    let stderr_path = state_dir.join("managed-runtime-bootstrap.stderr.log");
    let stdout_file = File::create(&stdout_path).map_err(|error| FetchScriptError {
        started: false,
        message: format!("failed to create {}: {error}", stdout_path.display()),
    })?;
    let stderr_file = match File::create(&stderr_path) {
        Ok(file) => file,
        Err(error) => {
            drop(stdout_file);
            remove_fetch_logs(&stdout_path, &stderr_path);
            return Err(FetchScriptError {
                started: false,
                message: format!("failed to create {}: {error}", stderr_path.display()),
            });
        }
    };
    command
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            drop(command);
            remove_fetch_logs(&stdout_path, &stderr_path);
            return Err(FetchScriptError {
                started: false,
                message: format!("failed to start managed runtime fetch script: {error}"),
            });
        }
    };
    drop(command);
    let deadline = Instant::now() + FETCH_SCRIPT_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(FETCH_SCRIPT_POLL_INTERVAL),
            Ok(None) => {
                terminate_fetch_process_tree(&mut child);
                let stdout = read_bounded_log(&stdout_path);
                let stderr = read_bounded_log(&stderr_path);
                remove_fetch_logs(&stdout_path, &stderr_path);
                return Err(FetchScriptError {
                    started: true,
                    message: format!(
                        "managed runtime fetch script timed out after {} seconds; stdout={stdout}; stderr={stderr}",
                        FETCH_SCRIPT_TIMEOUT.as_secs()
                    ),
                });
            }
            Err(error) => {
                terminate_fetch_process_tree(&mut child);
                let stdout = read_bounded_log(&stdout_path);
                let stderr = read_bounded_log(&stderr_path);
                remove_fetch_logs(&stdout_path, &stderr_path);
                return Err(FetchScriptError {
                    started: true,
                    message: format!(
                        "failed while waiting for managed runtime fetch script: {error}; stdout={stdout}; stderr={stderr}"
                    ),
                });
            }
        }
    };
    let stdout = read_bounded_log_bytes(&stdout_path);
    let stderr = read_bounded_log_bytes(&stderr_path);
    remove_fetch_logs(&stdout_path, &stderr_path);
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Builds the platform-specific fetch command with exact version arguments.
/// 使用精确版本参数构建平台专用拉取命令。
fn build_fetch_script_command(
    runtime_root: &Path,
    config: &ManagedRuntimeBootstrapConfig,
) -> Result<Command, FetchScriptError> {
    #[cfg(target_os = "windows")]
    {
        let script = runtime_root.join("scripts/deps/fetch_managed_runtimes.ps1");
        if !script.is_file() {
            return Err(FetchScriptError {
                started: false,
                message: format!(
                    "managed runtime fetch script is missing: {}",
                    script.display()
                ),
            });
        }
        let shell = if Command::new("pwsh").arg("--version").output().is_ok() {
            "pwsh"
        } else {
            "powershell.exe"
        };
        let mut command = Command::new(shell);
        command
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(script)
            .arg("-Target")
            .arg("all")
            .arg("-RuntimeRoot")
            .arg(runtime_root)
            .arg("-PythonVersion")
            .arg(&config.python_version)
            .arg("-UvVersion")
            .arg(&config.uv_version)
            .arg("-NodeVersion")
            .arg(&config.node_version)
            .arg("-PnpmVersion")
            .arg(&config.pnpm_version)
            .current_dir(runtime_root);
        Ok(command)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let script = runtime_root.join("scripts/deps/fetch_managed_runtimes.sh");
        if !script.is_file() {
            return Err(FetchScriptError {
                started: false,
                message: format!(
                    "managed runtime fetch script is missing: {}",
                    script.display()
                ),
            });
        }
        let mut command = Command::new("bash");
        command
            .arg(script)
            .arg("all")
            .env("RUNTIME_ROOT", runtime_root)
            .env("PYTHON_VERSION", &config.python_version)
            .env("UV_VERSION", &config.uv_version)
            .env("NODE_VERSION", &config.node_version)
            .env("PNPM_VERSION", &config.pnpm_version)
            .current_dir(runtime_root);
        Ok(command)
    }
}

/// Places the fetch shell in an isolated process group on Unix.
/// 在 Unix 上把拉取 shell 放入独立进程组。
#[cfg(unix)]
fn configure_fetch_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

/// Leaves Windows process creation unchanged; taskkill performs tree termination when needed.
/// Windows 进程创建保持不变；需要时由 taskkill 执行进程树终止。
#[cfg(windows)]
fn configure_fetch_process_group(_command: &mut Command) {}

/// Terminates the complete fetch process tree after timeout or supervision failure.
/// 在超时或监督失败后终止完整拉取进程树。
fn terminate_fetch_process_tree(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        // The child is created as its own process-group leader, so a negative PID targets only that fetch tree.
        // 子进程会作为独立进程组组长创建，因此负 PID 只会终止该拉取进程树。
        let _ = libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let process_id = child.id().to_string();
        let mut taskkill = Command::new("taskkill");
        taskkill
            .args(["/PID", process_id.as_str(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW);
        let _ = taskkill.status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads the bounded tail of one process log as bytes.
/// 以字节形式读取单个进程日志的有界尾部。
fn read_bounded_log_bytes(path: &Path) -> Vec<u8> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    if length > MAX_DIAGNOSTIC_BYTES as u64 {
        let _ = file.seek(SeekFrom::End(-(MAX_DIAGNOSTIC_BYTES as i64)));
    }
    let mut bytes = Vec::with_capacity(length.min(MAX_DIAGNOSTIC_BYTES as u64) as usize);
    let _ = file.read_to_end(&mut bytes);
    bytes
}

/// Reads the bounded tail of one process log as diagnostic text.
/// 以诊断文本形式读取单个进程日志的有界尾部。
fn read_bounded_log(path: &Path) -> String {
    truncate_output(&read_bounded_log_bytes(path))
}

/// Removes transient process logs after their bounded tails have been captured.
/// 在捕获有界日志尾部后删除临时进程日志。
fn remove_fetch_logs(stdout_path: &Path, stderr_path: &Path) {
    let _ = fs::remove_file(stdout_path);
    let _ = fs::remove_file(stderr_path);
}

/// Format one failed process result without retaining unbounded output.
/// 格式化一次失败进程结果，同时不保留无限输出。
fn format_process_failure(output: &Output) -> String {
    format!(
        "managed runtime fetch script failed with status {}; stdout={}; stderr={}",
        output.status,
        truncate_output(&output.stdout),
        truncate_output(&output.stderr)
    )
}

/// Decode and truncate diagnostic bytes on a UTF-8 boundary.
/// 在 UTF-8 边界上解码并截断诊断字节。
fn truncate_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= MAX_DIAGNOSTIC_BYTES {
        return text.trim().to_string();
    }
    let mut boundary = MAX_DIAGNOSTIC_BYTES;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}…", text[..boundary].trim())
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
            luaskills_version: "0.5.0".to_string(),
            python_version: "3.14.4".to_string(),
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

    /// Verify process diagnostics retain only the configured log tail.
    /// 验证进程诊断只保留配置上限内的日志尾部。
    #[test]
    fn process_diagnostic_log_is_bounded_to_tail() {
        let root = test_root("bounded-log");
        fs::create_dir_all(&root).expect("test root should be created");
        let log_path = root.join("process.log");
        let prefix = vec![b'a'; MAX_DIAGNOSTIC_BYTES];
        let suffix = vec![b'b'; MAX_DIAGNOSTIC_BYTES];
        let mut content = prefix;
        content.extend_from_slice(&suffix);
        fs::write(&log_path, content).expect("process log should be written");

        let bytes = read_bounded_log_bytes(&log_path);

        assert_eq!(bytes.len(), MAX_DIAGNOSTIC_BYTES);
        assert!(bytes.iter().all(|byte| *byte == b'b'));
        fs::remove_dir_all(root).expect("test root should be removed");
    }

    /// Verify a missing packaged script is reported before process startup.
    /// 验证缺少打包脚本时会在启动进程前返回错误。
    #[test]
    fn missing_fetch_script_reports_not_started() {
        let root = test_root("missing-script");
        fs::create_dir_all(&root).expect("test root should be created");
        let config = ManagedRuntimeBootstrapConfig {
            schema_version: 1,
            luaskills_version: "0.5.0".to_string(),
            python_version: "3.14.4".to_string(),
            uv_version: "0.11.28".to_string(),
            node_version: "24.18.0".to_string(),
            pnpm_version: "11.11.0".to_string(),
            supported_platforms: Vec::new(),
        };

        let error = build_fetch_script_command(&root, &config)
            .expect_err("missing fetch script should be rejected");

        assert!(!error.started);
        assert!(error.message.contains("fetch script is missing"));
        fs::remove_dir_all(root).expect("test root should be removed");
    }

    /// Verify a pre-spawn stderr failure releases and removes the already-created stdout log.
    /// 验证启动前 stderr 创建失败时会释放并删除已创建的 stdout 日志。
    #[test]
    fn pre_spawn_log_failure_removes_partial_stdout_log() {
        let root = test_root("partial-process-log");
        let scripts_dir = root.join("scripts/deps");
        let state_dir = root.join("state");
        fs::create_dir_all(&scripts_dir).expect("scripts directory should be created");
        fs::create_dir_all(state_dir.join("managed-runtime-bootstrap.stderr.log"))
            .expect("stderr collision directory should be created");
        #[cfg(target_os = "windows")]
        fs::write(
            scripts_dir.join("fetch_managed_runtimes.ps1"),
            b"Write-Output 'unused'",
        )
        .expect("dummy PowerShell script should be written");
        #[cfg(not(target_os = "windows"))]
        fs::write(
            scripts_dir.join("fetch_managed_runtimes.sh"),
            b"#!/usr/bin/env bash\nexit 0\n",
        )
        .expect("dummy shell script should be written");
        let config = ManagedRuntimeBootstrapConfig {
            schema_version: 1,
            luaskills_version: EXPECTED_LUASKILLS_VERSION.to_string(),
            python_version: EXPECTED_PYTHON_VERSION.to_string(),
            uv_version: EXPECTED_UV_VERSION.to_string(),
            node_version: EXPECTED_NODE_VERSION.to_string(),
            pnpm_version: EXPECTED_PNPM_VERSION.to_string(),
            supported_platforms: Vec::new(),
        };

        let error = run_fetch_script(&root, &config)
            .expect_err("stderr path collision should fail before process startup");

        assert!(!error.started);
        assert!(
            !state_dir
                .join("managed-runtime-bootstrap.stdout.log")
                .exists()
        );
        fs::remove_dir_all(root).expect("test root should be removed");
    }

    /// Validates a real externally fetched runtime tree when the explicit smoke-test root is provided.
    /// 在显式提供烟测根目录时校验真实的外部拉取运行时树。
    #[test]
    #[ignore = "requires runtimes fetched by scripts/deps/fetch_managed_runtimes"]
    fn externally_fetched_runtime_layout_matches_host_contract() {
        let runtime_root = std::env::var_os("VULCAN_MANAGED_RUNTIME_SMOKE_ROOT")
            .map(PathBuf::from)
            .expect("VULCAN_MANAGED_RUNTIME_SMOKE_ROOT must be set");
        let platform = managed_runtime_platform_key().expect("supported smoke-test platform");
        let config = ManagedRuntimeBootstrapConfig {
            schema_version: 1,
            luaskills_version: "0.5.0".to_string(),
            python_version: "3.14.4".to_string(),
            uv_version: "0.11.28".to_string(),
            node_version: "24.18.0".to_string(),
            pnpm_version: "11.11.0".to_string(),
            supported_platforms: vec![platform.clone()],
        };
        validate_runtime_layout(&runtime_root, &platform, &config)
            .expect("externally fetched runtime layout must satisfy the host contract");
    }

    /// Confirms a complete packaged root returns Ready without executing the fetch script again.
    /// 确认完整打包根目录会返回 Ready，且不会再次执行拉取脚本。
    #[test]
    #[ignore = "requires a packaged runtime root with fetched managed runtimes"]
    fn externally_fetched_bootstrap_is_idempotent() {
        let runtime_root = std::env::var_os("VULCAN_MANAGED_RUNTIME_SMOKE_ROOT")
            .map(PathBuf::from)
            .expect("VULCAN_MANAGED_RUNTIME_SMOKE_ROOT must be set");
        let report = bootstrap_managed_runtimes(&runtime_root, |_| {});
        assert_eq!(report.status, ManagedRuntimeBootstrapStatus::Ready);
        assert!(!report.script_executed);
    }
}
