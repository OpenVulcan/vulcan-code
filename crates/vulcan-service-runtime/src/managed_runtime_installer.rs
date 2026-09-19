//! Native managed Python and Node runtime downloader and installer.
//! 原生受管 Python 与 Node 运行时下载与安装器。

use crate::managed_runtime_bootstrap::ManagedRuntimeBootstrapProgress;
use base64::Engine as _;
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use tar::Archive;
use vulcan_runtime_assets::text_decode::decode_complete_user_text;
use xz2::read::XzDecoder;
use zip::ZipArchive;

/// HTTP connect timeout for one official runtime asset request.
/// 单个官方运行时资产请求的 HTTP 连接超时。
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// HTTP request timeout for one complete runtime asset transfer.
/// 单个完整运行时资产传输的 HTTP 请求超时。
const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Minimum interval between emitted byte-progress updates.
/// 字节进度更新之间的最短间隔。
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// Download buffer size used for bounded memory and regular progress updates.
/// 用于限制内存并定期更新进度的下载缓冲区大小。
const DOWNLOAD_BUFFER_BYTES: usize = 128 * 1024;

/// Maximum subprocess output included in one installation error.
/// 单次安装错误包含的子进程输出上限。
const MAX_PROCESS_DIAGNOSTIC_BYTES: usize = 8 * 1024;

/// Windows creation flag that prevents a console window for a child process.
/// 防止子进程创建控制台窗口的 Windows 创建标志。
#[cfg(all(windows, not(debug_assertions)))]
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply the Release-only no-console flag to a managed-runtime child process.
/// 为受管运行时子进程应用仅 Release 生效的无控制台标志。
fn apply_release_no_window_to_command(command: &mut Command) {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(WINDOWS_CREATE_NO_WINDOW);
    }
    #[cfg(any(not(windows), debug_assertions))]
    {
        let _ = command;
    }
}

/// Immutable native installation request for one managed runtime bootstrap.
/// 单次受管运行时初始化使用的不可变原生安装请求。
pub(crate) struct NativeManagedRuntimeInstallRequest<'a> {
    /// Writable LuaSkills runtime root used for caches and downloaded archives.
    /// 用于缓存与下载归档的可写 LuaSkills 运行时根。
    pub(crate) runtime_root: &'a Path,
    /// Host-owned root that directly contains Python and Node distributions.
    /// 直接包含 Python 与 Node 发行包的宿主自有根目录。
    pub(crate) distribution_root: &'a Path,
    /// Stable supported platform key.
    /// 稳定的受支持平台键。
    pub(crate) platform: &'a str,
    /// Exact CPython version.
    /// 精确 CPython 版本。
    pub(crate) python_version: &'a str,
    /// Exact uv version.
    /// 精确 uv 版本。
    pub(crate) uv_version: &'a str,
    /// Exact Node.js version.
    /// 精确 Node.js 版本。
    pub(crate) node_version: &'a str,
    /// Exact pnpm version.
    /// 精确 pnpm 版本。
    pub(crate) pnpm_version: &'a str,
    /// Whether uv must be rebuilt.
    /// 是否必须重建 uv。
    pub(crate) install_uv: bool,
    /// Whether CPython must be rebuilt.
    /// 是否必须重建 CPython。
    pub(crate) install_python: bool,
    /// Whether Node.js must be rebuilt.
    /// 是否必须重建 Node.js。
    pub(crate) install_node: bool,
    /// Whether pnpm must be rebuilt.
    /// 是否必须重建 pnpm。
    pub(crate) install_pnpm: bool,
}

/// Platform-specific official asset names and executable paths.
/// 平台专用的官方资产名称与可执行文件路径。
struct PlatformAssets {
    /// Exact uv release asset name.
    /// 精确 uv 发布资产名。
    uv_asset: String,
    /// Exact Node.js release asset name.
    /// 精确 Node.js 发布资产名。
    node_asset: String,
    /// Expected top-level directory inside the Node.js archive.
    /// Node.js 归档内预期的顶层目录。
    node_extract_name: String,
    /// uv executable path relative to its installation root.
    /// uv 可执行文件相对安装根的路径。
    uv_executable: &'static str,
    /// Node.js executable path relative to its installation root.
    /// Node.js 可执行文件相对安装根的路径。
    node_executable: &'static str,
}

/// Stable runtime manifest written after an installation validates successfully.
/// 安装通过校验后写入的稳定运行时清单。
#[derive(Serialize)]
struct RuntimeManifest<'a> {
    /// Manifest schema version.
    /// 清单 schema 版本。
    schema_version: u32,
    /// Runtime or package-manager identifier.
    /// 运行时或包管理器标识。
    runtime: &'a str,
    /// Exact installed version.
    /// 精确安装版本。
    version: &'a str,
    /// Stable platform key or `any`.
    /// 稳定平台键或 `any`。
    platform: &'a str,
    /// Executable path relative to the installation directory.
    /// 相对于安装目录的可执行文件路径。
    executable: String,
    /// Exact official source used for the installation.
    /// 安装使用的精确官方来源。
    source: &'a str,
}

/// npm registry metadata needed to verify and install one pnpm release.
/// 校验并安装一个 pnpm 版本所需的 npm registry 元数据。
#[derive(Deserialize)]
struct NpmPackageMetadata {
    /// Distribution archive metadata.
    /// 发行归档元数据。
    dist: NpmDistributionMetadata,
}

/// Exact npm tarball location and Subresource Integrity value.
/// 精确 npm tarball 地址与子资源完整性值。
#[derive(Deserialize)]
struct NpmDistributionMetadata {
    /// Official package tarball URL.
    /// 官方包 tarball 地址。
    tarball: String,
    /// Expected SHA-512 integrity value.
    /// 期望的 SHA-512 完整性值。
    integrity: String,
}

/// Install every missing managed runtime with native Rust I/O and archive handling.
/// 使用 Rust 原生 I/O 与归档处理安装每个缺失的受管运行时。
///
/// `request` contains exact immutable versions and missing-component decisions.
/// `request` 包含精确不可变版本与缺失组件判定。
/// `progress` receives throttled structured progress events and must not block.
/// `progress` 接收节流后的结构化进度事件，且不得阻塞。
///
/// Returns success only after every requested component is installed and executable.
/// 仅当每个请求组件均已安装且可执行时返回成功。
pub(crate) fn install_managed_runtimes<F>(
    request: &NativeManagedRuntimeInstallRequest<'_>,
    progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    // assets resolves the only official archive names accepted for the current platform.
    // assets 解析当前平台唯一接受的官方归档名称。
    let assets = PlatformAssets::for_platform(request.platform, request.node_version)?;
    // client owns TLS, redirects, and bounded request timeouts for all official downloads.
    // client 持有所有官方下载使用的 TLS、重定向与有界请求超时。
    let client = Client::builder()
        .connect_timeout(HTTP_CONNECT_TIMEOUT)
        .timeout(HTTP_REQUEST_TIMEOUT)
        .user_agent(format!(
            "Vulcan-Code/{} managed-runtime",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|error| format!("failed to create managed runtime HTTP client: {error}"))?;
    fs::create_dir_all(request.distribution_root).map_err(|error| {
        format!(
            "failed to create managed runtime distribution root {}: {error}",
            request.distribution_root.display()
        )
    })?;

    if request.install_uv {
        install_uv_runtime(&client, request, &assets, progress)?;
    }
    if request.install_python {
        install_python_runtime(request, &assets, progress)?;
    }
    if request.install_node {
        install_node_runtime(&client, request, &assets, progress)?;
    }
    if request.install_pnpm {
        install_pnpm_runtime(&client, request, &assets, progress)?;
    }
    Ok(())
}

impl PlatformAssets {
    /// Resolve official archive names for one supported stable platform key.
    /// 为一个受支持的稳定平台键解析官方归档名称。
    ///
    /// Returns an error for every unpublished platform, including Windows ARM.
    /// 对每个未发布平台返回错误，包括 Windows ARM。
    fn for_platform(platform: &str, node_version: &str) -> Result<Self, String> {
        match platform {
            "windows-x64" => Ok(Self {
                uv_asset: String::from("uv-x86_64-pc-windows-msvc.zip"),
                node_asset: format!("node-v{node_version}-win-x64.zip"),
                node_extract_name: format!("node-v{node_version}-win-x64"),
                uv_executable: "uv.exe",
                node_executable: "node.exe",
            }),
            "linux-x64" => Ok(Self {
                uv_asset: String::from("uv-x86_64-unknown-linux-gnu.tar.gz"),
                node_asset: format!("node-v{node_version}-linux-x64.tar.xz"),
                node_extract_name: format!("node-v{node_version}-linux-x64"),
                uv_executable: "uv",
                node_executable: "bin/node",
            }),
            "linux-arm64" => Ok(Self {
                uv_asset: String::from("uv-aarch64-unknown-linux-gnu.tar.gz"),
                node_asset: format!("node-v{node_version}-linux-arm64.tar.xz"),
                node_extract_name: format!("node-v{node_version}-linux-arm64"),
                uv_executable: "uv",
                node_executable: "bin/node",
            }),
            "macos-x64" => Ok(Self {
                uv_asset: String::from("uv-x86_64-apple-darwin.tar.gz"),
                node_asset: format!("node-v{node_version}-darwin-x64.tar.gz"),
                node_extract_name: format!("node-v{node_version}-darwin-x64"),
                uv_executable: "uv",
                node_executable: "bin/node",
            }),
            "macos-arm64" => Ok(Self {
                uv_asset: String::from("uv-aarch64-apple-darwin.tar.gz"),
                node_asset: format!("node-v{node_version}-darwin-arm64.tar.gz"),
                node_extract_name: format!("node-v{node_version}-darwin-arm64"),
                uv_executable: "uv",
                node_executable: "bin/node",
            }),
            _ => Err(format!(
                "managed runtime assets are not published for {platform}"
            )),
        }
    }
}

/// Returns the managed-runtime download staging root beneath one runtime root.
/// 返回某个运行时根之下的受管运行时下载暂存根。
///
/// Every managed component stages under a versioned child of this root and wipes only
/// that child before installing, so the LuaSkills skill download cache at
/// `temp/downloads` is never touched by a runtime repair.
/// 每个受管组件都在本根之下的版本化子目录中暂存，且安装前只清空该子目录，因此
/// `temp/downloads` 处的 LuaSkills 技能下载缓存绝不会被运行时修复触碰。
pub(crate) fn managed_runtime_download_root(runtime_root: &Path) -> PathBuf {
    runtime_root.join("temp").join("managed-runtimes")
}

/// Download, verify, extract, and atomically install the managed uv executable.
/// 下载、校验、解压并原子安装受管 uv 可执行文件。
fn install_uv_runtime<F>(
    client: &Client,
    request: &NativeManagedRuntimeInstallRequest<'_>,
    assets: &PlatformAssets,
    progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    // asset_url is the exact official uv release asset URL.
    // asset_url 是精确的官方 uv 发布资产地址。
    let asset_url = format!(
        "https://github.com/astral-sh/uv/releases/download/{}/{}",
        request.uv_version, assets.uv_asset
    );
    // download_root isolates reusable archives from final installations.
    // download_root 将可复用归档与最终安装隔离。
    let download_root = managed_runtime_download_root(request.runtime_root)
        .join(format!("uv-{}-{}", request.uv_version, request.platform));
    prepare_clean_directory(&download_root)?;
    // archive_path stores the complete verified uv archive.
    // archive_path 保存完整且经过校验的 uv 归档。
    let archive_path = download_root.join(&assets.uv_asset);
    download_file(
        client,
        &asset_url,
        &archive_path,
        "uv",
        &format!("正在下载 uv {}", request.uv_version),
        3,
        15,
        progress,
    )?;
    // checksum_path stores the official uv SHA-256 sidecar.
    // checksum_path 保存官方 uv SHA-256 旁路文件。
    let checksum_path = download_root.join(format!("{}.sha256", assets.uv_asset));
    download_file(
        client,
        &format!("{asset_url}.sha256"),
        &checksum_path,
        "uv",
        "正在下载 uv 校验文件",
        15,
        16,
        progress,
    )?;
    emit_progress(
        progress,
        "verifying",
        Some("uv"),
        "正在校验 uv 下载内容",
        17,
        None,
        None,
    );
    // expected_hash is the official lowercase SHA-256 digest.
    // expected_hash 是官方小写 SHA-256 摘要。
    let expected_hash =
        parse_first_sha256(&fs::read_to_string(&checksum_path).map_err(|error| {
            format!(
                "failed to read uv checksum {}: {error}",
                checksum_path.display()
            )
        })?)?;
    verify_sha256(&archive_path, &expected_hash)?;
    // target is the final versioned uv installation directory.
    // target 是最终带版本的 uv 安装目录。
    let target = request
        .distribution_root
        .join("python")
        .join(format!("uv-{}-{}", request.uv_version, request.platform));
    // staging is a same-volume directory committed atomically after validation.
    // staging 是校验后原子提交的同卷目录。
    let staging = sibling_work_path(&target, "installing")?;
    // extraction is a separate same-volume directory for safe archive expansion.
    // extraction 是用于安全展开归档的独立同卷目录。
    let extraction = sibling_work_path(&target, "extracting")?;
    prepare_clean_directory(&staging)?;
    prepare_clean_directory(&extraction)?;
    emit_progress(
        progress,
        "extracting",
        Some("uv"),
        "正在解压 uv",
        18,
        None,
        None,
    );
    extract_archive(&archive_path, &extraction)?;
    // extracted_uv is the only matching executable from the official archive.
    // extracted_uv 是官方归档内唯一匹配的可执行文件。
    let extracted_uv = find_file_named(&extraction, OsStr::new(assets.uv_executable))?
        .ok_or_else(|| format!("uv executable {} was not found", assets.uv_executable))?;
    // staged_uv is the exact executable path written into the final manifest.
    // staged_uv 是写入最终清单的精确可执行文件路径。
    let staged_uv = staging.join(assets.uv_executable);
    fs::copy(&extracted_uv, &staged_uv).map_err(|error| {
        format!(
            "failed to copy uv executable from {} to {}: {error}",
            extracted_uv.display(),
            staged_uv.display()
        )
    })?;
    mark_executable(&staged_uv)?;
    verify_binary_version(&staged_uv, &["--version"], request.uv_version)?;
    write_runtime_manifest(
        &staging,
        RuntimeManifest {
            schema_version: 1,
            runtime: "uv",
            version: request.uv_version,
            platform: request.platform,
            executable: assets.uv_executable.to_string(),
            source: &asset_url,
        },
    )?;
    commit_staging_directory(&staging, &target)?;
    remove_path_if_exists(&extraction)?;
    emit_progress(
        progress,
        "installed",
        Some("uv"),
        "uv 安装完成",
        20,
        None,
        None,
    );
    Ok(())
}

/// Install CPython into a same-volume staging directory through the managed uv executable.
/// 通过受管 uv 可执行文件把 CPython 安装到同卷暂存目录。
fn install_python_runtime<F>(
    request: &NativeManagedRuntimeInstallRequest<'_>,
    assets: &PlatformAssets,
    progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    // uv_executable is the already validated host-owned package manager.
    // uv_executable 是已经校验的宿主自有包管理器。
    let uv_executable = request
        .distribution_root
        .join("python")
        .join(format!("uv-{}-{}", request.uv_version, request.platform))
        .join(assets.uv_executable);
    // target is the final CPython version root used by LuaSkills.
    // target 是 LuaSkills 使用的最终 CPython 版本根。
    let target = request.distribution_root.join("python").join(format!(
        "cpython-{}-{}",
        request.python_version, request.platform
    ));
    // staging receives the complete uv-managed interpreter before atomic commit.
    // staging 在原子提交前接收完整的 uv 受管解释器。
    let staging = sibling_work_path(&target, "installing")?;
    prepare_clean_directory(&staging)?;
    // cache_root isolates uv downloads from user-level uv state.
    // cache_root 将 uv 下载与用户级 uv 状态隔离。
    let cache_root = request.runtime_root.join("temp/managed-runtimes/uv-cache");
    fs::create_dir_all(&cache_root).map_err(|error| {
        format!(
            "failed to create uv cache {}: {error}",
            cache_root.display()
        )
    })?;
    emit_progress(
        progress,
        "downloading",
        Some("python"),
        &format!("正在下载并安装 Python {}", request.python_version),
        25,
        None,
        None,
    );
    // install_output captures bounded diagnostics without mutating user registry or executable paths.
    // install_output 捕获有界诊断，且不会修改用户注册表或可执行路径。
    // process_uv_executable avoids exposing Win32 device prefixes to uv's argument parser.
    // process_uv_executable 避免把 Win32 设备前缀暴露给 uv 参数解析器。
    let process_uv_executable = vulcan_runtime_assets::native_host_path(&uv_executable);
    // process_staging and process_cache_root preserve filesystem identity in process-compatible form.
    // process_staging 与 process_cache_root 以进程兼容形式保留文件系统身份。
    let process_staging = vulcan_runtime_assets::native_host_path(&staging);
    let process_cache_root = vulcan_runtime_assets::native_host_path(&cache_root);
    // install_command owns the Release-aware uv invocation that installs Python.
    // install_command 持有带 Release 无控制台处理的 uv Python 安装调用。
    let mut install_command = Command::new(&process_uv_executable);
    install_command
        .args([
            "python",
            "install",
            request.python_version,
            "--no-bin",
            "--no-registry",
        ])
        .env("UV_PYTHON_INSTALL_DIR", &process_staging)
        .env("UV_CACHE_DIR", &process_cache_root)
        .env("UV_MANAGED_PYTHON", "1")
        .env("UV_NO_CONFIG", "1");
    apply_release_no_window_to_command(&mut install_command);
    let install_output = install_command.output().map_err(|error| {
        format!(
            "failed to start managed uv {}: {error}",
            uv_executable.display()
        )
    })?;
    ensure_process_success("uv python install", &install_output)?;
    // find_output resolves the installed interpreter inside the exact staging root.
    // find_output 解析精确暂存根内安装的解释器。
    // find_command owns the Release-aware uv invocation that locates Python.
    // find_command 持有带 Release 无控制台处理的 uv Python 定位调用。
    let mut find_command = Command::new(&process_uv_executable);
    find_command
        .args(["python", "find", request.python_version])
        .env("UV_PYTHON_INSTALL_DIR", &process_staging)
        .env("UV_CACHE_DIR", &process_cache_root)
        .env("UV_MANAGED_PYTHON", "1")
        .env("UV_NO_CONFIG", "1");
    apply_release_no_window_to_command(&mut find_command);
    let find_output = find_command
        .output()
        .map_err(|error| format!("failed to resolve managed Python interpreter: {error}"))?;
    ensure_process_success("uv python find", &find_output)?;
    // python_text is the UTF-8 path emitted by uv.
    // python_text 是 uv 输出的 UTF-8 路径。
    let python_text = String::from_utf8(find_output.stdout)
        .map_err(|error| format!("uv python find returned non-UTF-8 output: {error}"))?;
    // python_path is the first non-empty interpreter path returned by uv.
    // python_path 是 uv 返回的首个非空解释器路径。
    let python_path = python_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| String::from("uv python find returned no interpreter path"))?;
    // relative_python proves the manifest executable remains inside staging.
    // relative_python 证明清单可执行文件始终位于暂存目录内。
    let relative_python = relative_canonical_path(&staging, &python_path)?;
    verify_binary_version(&python_path, &["--version"], request.python_version)?;
    // uv also creates a short-version directory link with an absolute target inside staging.
    // uv 还会创建指向暂存目录内部的短版本目录链接；提交前必须移除以保证发行包可搬移。
    remove_uv_staging_aliases(&staging, &relative_python)?;
    write_runtime_manifest(
        &staging,
        RuntimeManifest {
            schema_version: 1,
            runtime: "python",
            version: request.python_version,
            platform: request.platform,
            executable: relative_path_text(&relative_python),
            source: "uv-managed-python",
        },
    )?;
    commit_staging_directory(&staging, &target)?;
    emit_progress(
        progress,
        "installed",
        Some("python"),
        "Python 安装完成",
        50,
        None,
        None,
    );
    Ok(())
}

/// Remove uv's direct short-version links before the staging directory is renamed.
/// 在暂存目录重命名前移除 uv 创建的直属短版本链接。
/// staging is the temporary Python root and relative_python is its verified executable path; returns an error for any unrelated link.
/// staging 是临时 Python 根，relative_python 是已验证的可执行文件相对路径；遇到无关链接时返回错误。
fn remove_uv_staging_aliases(staging: &Path, relative_python: &Path) -> Result<(), String> {
    // version_name is the first verified path component, independent of Windows or Unix bin layouts.
    // version_name 是已验证路径的首段，不依赖 Windows 或 Unix 的 bin 目录布局。
    let version_name = match relative_python.components().next() {
        Some(std::path::Component::Normal(name)) => name,
        _ => {
            return Err(format!(
                "managed Python executable has no version directory: {}",
                relative_python.display()
            ));
        }
    };
    // installed_root is the exact version directory that uv's short-version alias may reference.
    // installed_root 是 uv 短版本别名可能指向的精确版本目录。
    let installed_root = staging.join(version_name);
    // installed_root is canonicalized before comparing it with uv's direct alias target.
    // 比较 uv 直属别名目标之前，先将 installed_root 规范化。
    let installed_root = fs::canonicalize(&installed_root).map_err(|error| {
        format!(
            "failed to resolve managed Python directory {}: {error}",
            installed_root.display()
        )
    })?;
    // entry visits direct children because uv creates its version alias at this level.
    // entry 遍历直属子项，因为 uv 在这一层创建版本别名。
    for entry in fs::read_dir(staging).map_err(|error| {
        format!(
            "failed to read Python staging directory {}: {error}",
            staging.display()
        )
    })? {
        // entry is the resolved directory entry inspected without following links.
        // entry 是不跟随链接而检查的已解析目录子项。
        let entry = entry.map_err(|error| {
            format!("failed to read entry under {}: {error}", staging.display())
        })?;
        // metadata describes the entry itself, including its Windows reparse attribute.
        // metadata 描述子项本身，包括 Windows 重解析属性。
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            format!(
                "failed to inspect Python staging entry {}: {error}",
                entry.path().display()
            )
        })?;
        // Windows junctions are reparse points and are not reliably classified as symbolic links.
        // Windows 目录联接属于重解析点，不能只依赖符号链接分类来识别。
        #[cfg(windows)]
        let is_link = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let is_link = metadata.file_type().is_symlink();
        if !is_link {
            continue;
        }
        // linked_root must equal the verified interpreter directory; unrelated links remain explicit errors.
        // linked_root 必须等于已验证的解释器目录；无关链接会明确报错。
        let linked_root = fs::canonicalize(entry.path()).map_err(|error| {
            format!(
                "failed to resolve Python staging link {}: {error}",
                entry.path().display()
            )
        })?;
        if linked_root != installed_root {
            return Err(format!(
                "unexpected Python staging link {} -> {}",
                entry.path().display(),
                linked_root.display()
            ));
        }
        #[cfg(windows)]
        fs::remove_dir(entry.path()).map_err(|error| {
            format!(
                "failed to remove Python staging directory link {}: {error}",
                entry.path().display()
            )
        })?;
        #[cfg(not(windows))]
        fs::remove_file(entry.path()).map_err(|error| {
            format!(
                "failed to remove Python staging link {}: {error}",
                entry.path().display()
            )
        })?;
    }
    Ok(())
}

/// Download, verify, extract, and atomically install Node.js.
/// 下载、校验、解压并原子安装 Node.js。
fn install_node_runtime<F>(
    client: &Client,
    request: &NativeManagedRuntimeInstallRequest<'_>,
    assets: &PlatformAssets,
    progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    // base_url is the official directory for the exact Node.js release.
    // base_url 是精确 Node.js 版本的官方目录。
    let base_url = format!("https://nodejs.org/dist/v{}", request.node_version);
    // asset_url is the exact official Node.js archive URL.
    // asset_url 是精确的官方 Node.js 归档地址。
    let asset_url = format!("{base_url}/{}", assets.node_asset);
    // download_root isolates Node.js archives from final installations.
    // download_root 将 Node.js 归档与最终安装隔离。
    let download_root = managed_runtime_download_root(request.runtime_root).join(format!(
        "node-{}-{}",
        request.node_version, request.platform
    ));
    prepare_clean_directory(&download_root)?;
    // archive_path stores the complete Node.js archive.
    // archive_path 保存完整 Node.js 归档。
    let archive_path = download_root.join(&assets.node_asset);
    download_file(
        client,
        &asset_url,
        &archive_path,
        "node",
        &format!("正在下载 Node.js {}", request.node_version),
        52,
        72,
        progress,
    )?;
    // checksums_path stores the official Node.js checksum manifest.
    // checksums_path 保存官方 Node.js 校验清单。
    let checksums_path = download_root.join("SHASUMS256.txt");
    download_file(
        client,
        &format!("{base_url}/SHASUMS256.txt"),
        &checksums_path,
        "node",
        "正在下载 Node.js 校验清单",
        72,
        73,
        progress,
    )?;
    emit_progress(
        progress,
        "verifying",
        Some("node"),
        "正在校验 Node.js 下载内容",
        75,
        None,
        None,
    );
    // expected_hash is the unique digest paired with the exact asset name.
    // expected_hash 是与精确资产名配对的唯一摘要。
    let expected_hash = parse_named_sha256(
        &fs::read_to_string(&checksums_path).map_err(|error| {
            format!(
                "failed to read Node.js checksums {}: {error}",
                checksums_path.display()
            )
        })?,
        &assets.node_asset,
    )?;
    verify_sha256(&archive_path, &expected_hash)?;
    // target is the final versioned Node.js installation directory.
    // target 是最终带版本的 Node.js 安装目录。
    let target = request.distribution_root.join("node").join(format!(
        "node-{}-{}",
        request.node_version, request.platform
    ));
    // staging receives the exact top-level archive payload.
    // staging 接收精确的归档顶层载荷。
    let staging = sibling_work_path(&target, "installing")?;
    // extraction isolates archive entries until the expected root is confirmed.
    // extraction 隔离归档条目，直到确认预期根目录。
    let extraction = sibling_work_path(&target, "extracting")?;
    prepare_clean_directory(&extraction)?;
    emit_progress(
        progress,
        "extracting",
        Some("node"),
        "正在解压 Node.js",
        78,
        None,
        None,
    );
    extract_archive(&archive_path, &extraction)?;
    // extracted_root is the exact official top-level Node.js directory.
    // extracted_root 是精确的官方 Node.js 顶层目录。
    let extracted_root = extraction.join(&assets.node_extract_name);
    if !extracted_root.is_dir() {
        return Err(format!(
            "Node.js archive root {} was not found in {}",
            assets.node_extract_name,
            archive_path.display()
        ));
    }
    remove_path_if_exists(&staging)?;
    fs::rename(&extracted_root, &staging).map_err(|error| {
        format!(
            "failed to stage Node.js from {} to {}: {error}",
            extracted_root.display(),
            staging.display()
        )
    })?;
    // staged_node is the exact executable validated before atomic commit.
    // staged_node 是原子提交前校验的精确可执行文件。
    let staged_node = staging.join(assets.node_executable);
    mark_executable(&staged_node)?;
    verify_binary_version(&staged_node, &["--version"], request.node_version)?;
    write_runtime_manifest(
        &staging,
        RuntimeManifest {
            schema_version: 1,
            runtime: "node",
            version: request.node_version,
            platform: request.platform,
            executable: assets.node_executable.to_string(),
            source: &asset_url,
        },
    )?;
    commit_staging_directory(&staging, &target)?;
    remove_path_if_exists(&extraction)?;
    emit_progress(
        progress,
        "installed",
        Some("node"),
        "Node.js 安装完成",
        80,
        None,
        None,
    );
    Ok(())
}

/// Download, verify, extract, and atomically install pnpm without global npm state.
/// 在不触碰全局 npm 状态的情况下下载、校验、解压并原子安装 pnpm。
fn install_pnpm_runtime<F>(
    client: &Client,
    request: &NativeManagedRuntimeInstallRequest<'_>,
    assets: &PlatformAssets,
    progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    emit_progress(
        progress,
        "resolving",
        Some("pnpm"),
        "正在读取 pnpm 官方元数据",
        82,
        None,
        None,
    );
    // metadata_url selects one exact pnpm version from the official npm registry.
    // metadata_url 从官方 npm registry 选择一个精确 pnpm 版本。
    let metadata_url = format!("https://registry.npmjs.org/pnpm/{}", request.pnpm_version);
    // metadata supplies the authoritative tarball URL and SHA-512 integrity value.
    // metadata 提供权威 tarball 地址与 SHA-512 完整性值。
    let metadata = client
        .get(&metadata_url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("failed to fetch pnpm metadata {metadata_url}: {error}"))?
        .json::<NpmPackageMetadata>()
        .map_err(|error| format!("failed to decode pnpm metadata {metadata_url}: {error}"))?;
    // expected_digest removes the mandatory npm integrity algorithm prefix.
    // expected_digest 移除强制 npm 完整性算法前缀。
    let expected_digest = metadata
        .dist
        .integrity
        .strip_prefix("sha512-")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "pnpm metadata for {} has no sha512 integrity",
                request.pnpm_version
            )
        })?;
    // download_root isolates the package tarball from its final installation.
    // download_root 将包 tarball 与最终安装隔离。
    let download_root = managed_runtime_download_root(request.runtime_root)
        .join(format!("pnpm-{}", request.pnpm_version));
    prepare_clean_directory(&download_root)?;
    // tarball_path stores the exact verified pnpm package tarball.
    // tarball_path 保存精确且经过校验的 pnpm 包 tarball。
    let tarball_path = download_root.join(format!("pnpm-{}.tgz", request.pnpm_version));
    download_file(
        client,
        &metadata.dist.tarball,
        &tarball_path,
        "pnpm",
        &format!("正在下载 pnpm {}", request.pnpm_version),
        84,
        95,
        progress,
    )?;
    emit_progress(
        progress,
        "verifying",
        Some("pnpm"),
        "正在校验 pnpm 下载内容",
        97,
        None,
        None,
    );
    verify_sha512_base64(&tarball_path, expected_digest)?;
    // target is the final versioned pnpm package directory.
    // target 是最终带版本的 pnpm 包目录。
    let target = request
        .distribution_root
        .join("node")
        .join(format!("pnpm-{}", request.pnpm_version));
    // staging receives the standard npm `package` archive root.
    // staging 接收标准 npm `package` 归档根。
    let staging = sibling_work_path(&target, "installing")?;
    // extraction isolates every tar entry before selecting the package root.
    // extraction 在选择包根之前隔离每个 tar 条目。
    let extraction = sibling_work_path(&target, "extracting")?;
    prepare_clean_directory(&extraction)?;
    emit_progress(
        progress,
        "extracting",
        Some("pnpm"),
        "正在解压 pnpm",
        98,
        None,
        None,
    );
    extract_archive(&tarball_path, &extraction)?;
    // package_root is the standard root produced by npm tarballs.
    // package_root 是 npm tarball 生成的标准根目录。
    let package_root = extraction.join("package");
    if !package_root.is_dir() {
        return Err(format!(
            "pnpm package root was not found in {}",
            tarball_path.display()
        ));
    }
    remove_path_if_exists(&staging)?;
    fs::rename(&package_root, &staging).map_err(|error| {
        format!(
            "failed to stage pnpm from {} to {}: {error}",
            package_root.display(),
            staging.display()
        )
    })?;
    // node_executable is the exact managed Node.js binary used to verify pnpm.
    // node_executable 是用于校验 pnpm 的精确受管 Node.js 二进制。
    let node_executable = request
        .distribution_root
        .join("node")
        .join(format!(
            "node-{}-{}",
            request.node_version, request.platform
        ))
        .join(assets.node_executable);
    // pnpm_entry is the CommonJS executable recorded in the runtime manifest.
    // pnpm_entry 是运行时清单记录的 CommonJS 可执行入口。
    let pnpm_entry = staging.join("bin/pnpm.cjs");
    verify_binary_version(
        &node_executable,
        &[pnpm_entry.as_os_str(), OsStr::new("--version")],
        request.pnpm_version,
    )?;
    write_runtime_manifest(
        &staging,
        RuntimeManifest {
            schema_version: 1,
            runtime: "pnpm",
            version: request.pnpm_version,
            platform: "any",
            executable: String::from("bin/pnpm.cjs"),
            source: &metadata.dist.tarball,
        },
    )?;
    commit_staging_directory(&staging, &target)?;
    remove_path_if_exists(&extraction)?;
    emit_progress(
        progress,
        "installed",
        Some("pnpm"),
        "pnpm 安装完成",
        99,
        None,
        None,
    );
    Ok(())
}

/// Download one HTTP resource to a partial file and atomically publish it with throttled progress.
/// 把一个 HTTP 资源下载到临时文件，并在节流进度更新下原子发布。
#[allow(clippy::too_many_arguments)]
fn download_file<F>(
    client: &Client,
    url: &str,
    destination: &Path,
    component: &str,
    message: &str,
    overall_start: u8,
    overall_end: u8,
    progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create download directory {}: {error}",
                parent.display()
            )
        })?;
    }
    // partial_path prevents interrupted responses from masquerading as complete archives.
    // partial_path 防止中断响应伪装成完整归档。
    let partial_path = destination.with_extension(format!(
        "{}part",
        destination
            .extension()
            .and_then(OsStr::to_str)
            .map(|value| format!("{value}."))
            .unwrap_or_default()
    ));
    remove_path_if_exists(&partial_path)?;
    // response is the successful official HTTP body after redirect handling.
    // response 是处理重定向后的成功官方 HTTP 响应体。
    let mut response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("failed to download {url}: {error}"))?;
    // total_bytes is the server-declared body size when available.
    // total_bytes 是服务器声明的响应体大小（如可用）。
    let total_bytes = response.content_length();
    // output receives the response body before an atomic rename.
    // output 在原子重命名前接收响应体。
    let mut output = File::create(&partial_path).map_err(|error| {
        format!(
            "failed to create download file {}: {error}",
            partial_path.display()
        )
    })?;
    // completed_bytes tracks the exact bytes persisted to disk.
    // completed_bytes 跟踪精确写入磁盘的字节数。
    let mut completed_bytes = 0_u64;
    // buffer bounds memory while preserving useful progress cadence.
    // buffer 限制内存，同时保持有用的进度更新频率。
    let mut buffer = vec![0_u8; DOWNLOAD_BUFFER_BYTES];
    // last_emit throttles service and WebUI snapshot traffic.
    // last_emit 对服务与 WebUI 快照流量进行节流。
    let mut last_emit = Instant::now() - PROGRESS_EMIT_INTERVAL;
    emit_download_progress(
        progress,
        component,
        message,
        completed_bytes,
        total_bytes,
        overall_start,
        overall_end,
    );
    loop {
        // read_count is one bounded HTTP body chunk.
        // read_count 是一个有界 HTTP 响应体分块。
        let read_count = response
            .read(&mut buffer)
            .map_err(|error| format!("failed while downloading {url}: {error}"))?;
        if read_count == 0 {
            break;
        }
        output.write_all(&buffer[..read_count]).map_err(|error| {
            format!(
                "failed to write download {}: {error}",
                partial_path.display()
            )
        })?;
        completed_bytes = completed_bytes.saturating_add(read_count as u64);
        if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
            emit_download_progress(
                progress,
                component,
                message,
                completed_bytes,
                total_bytes,
                overall_start,
                overall_end,
            );
            last_emit = Instant::now();
        }
    }
    output.sync_all().map_err(|error| {
        format!(
            "failed to flush download {}: {error}",
            partial_path.display()
        )
    })?;
    drop(output);
    if let Some(expected) = total_bytes
        && completed_bytes != expected
    {
        return Err(format!(
            "download size mismatch for {url}: expected {expected} bytes, received {completed_bytes}"
        ));
    }
    emit_download_progress(
        progress,
        component,
        message,
        completed_bytes,
        total_bytes,
        overall_start,
        overall_end,
    );
    remove_path_if_exists(destination)?;
    fs::rename(&partial_path, destination).map_err(|error| {
        format!(
            "failed to publish download {} as {}: {error}",
            partial_path.display(),
            destination.display()
        )
    })?;
    Ok(())
}

/// Emit one structured byte-progress event with a monotonic overall percentage range.
/// 发出一个带单调总体百分比区间的结构化字节进度事件。
#[allow(clippy::too_many_arguments)]
fn emit_download_progress<F>(
    progress: &mut F,
    component: &str,
    message: &str,
    completed_bytes: u64,
    total_bytes: Option<u64>,
    overall_start: u8,
    overall_end: u8,
) where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    // overall_percent maps this asset's byte ratio into its reserved bootstrap range.
    // overall_percent 把当前资产字节比例映射到其预留初始化区间。
    let overall_percent = total_bytes
        .filter(|total| *total > 0)
        .map(|total| {
            let span = u64::from(overall_end.saturating_sub(overall_start));
            let mapped = completed_bytes.min(total).saturating_mul(span) / total;
            overall_start.saturating_add(mapped as u8)
        })
        .unwrap_or(overall_start);
    emit_progress(
        progress,
        "downloading",
        Some(component),
        message,
        overall_percent,
        Some(completed_bytes),
        total_bytes,
    );
}

/// Emit one immutable managed-runtime progress event.
/// 发出一个不可变受管运行时进度事件。
#[allow(clippy::too_many_arguments)]
fn emit_progress<F>(
    progress: &mut F,
    phase: &str,
    component: Option<&str>,
    message: &str,
    overall_percent: u8,
    completed_bytes: Option<u64>,
    total_bytes: Option<u64>,
) where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    progress(&ManagedRuntimeBootstrapProgress {
        phase: phase.to_string(),
        component: component.map(str::to_string),
        message: message.to_string(),
        completed_bytes,
        total_bytes,
        overall_percent: Some(overall_percent.min(100)),
    });
}

/// Parse the first lowercase SHA-256 token from one official sidecar document.
/// 从一个官方旁路文档解析首个小写 SHA-256 标记。
fn parse_first_sha256(text: &str) -> Result<String, String> {
    // digest is the first whitespace-delimited checksum token.
    // digest 是首个以空白分隔的校验和标记。
    let digest = text
        .split_whitespace()
        .next()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| String::from("checksum document is empty"))?;
    validate_sha256_text(&digest)?;
    Ok(digest)
}

/// Parse the unique SHA-256 entry paired with one exact archive name.
/// 解析与一个精确归档名称配对的唯一 SHA-256 条目。
fn parse_named_sha256(text: &str, asset_name: &str) -> Result<String, String> {
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(digest) = fields.next() else {
            continue;
        };
        let Some(name) = fields.next() else {
            continue;
        };
        if name.trim_start_matches('*') == asset_name {
            let normalized = digest.to_ascii_lowercase();
            validate_sha256_text(&normalized)?;
            return Ok(normalized);
        }
    }
    Err(format!("checksum entry for {asset_name} was not found"))
}

/// Validate one textual SHA-256 digest before comparing it to downloaded content.
/// 在与下载内容比较前校验一个文本 SHA-256 摘要。
fn validate_sha256_text(digest: &str) -> Result<(), String> {
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("invalid SHA-256 digest `{digest}`"));
    }
    Ok(())
}

/// Verify one file against an expected lowercase SHA-256 digest.
/// 使用期望的小写 SHA-256 摘要校验一个文件。
fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
    // actual is the digest calculated from the complete downloaded file.
    // actual 是从完整下载文件计算出的摘要。
    let actual = digest_file::<Sha256>(path)?;
    // actual_hex is the deterministic lowercase digest representation.
    // actual_hex 是确定性小写摘要表示。
    let actual_hex = hex_text(&actual);
    if actual_hex != expected {
        return Err(format!(
            "SHA-256 mismatch for {}: expected {expected}, got {actual_hex}",
            path.display()
        ));
    }
    Ok(())
}

/// Verify one file against an expected npm SHA-512 Base64 digest.
/// 使用期望的 npm SHA-512 Base64 摘要校验一个文件。
fn verify_sha512_base64(path: &Path, expected: &str) -> Result<(), String> {
    // actual is the complete SHA-512 digest calculated from disk.
    // actual 是从磁盘完整计算的 SHA-512 摘要。
    let actual = digest_file::<Sha512>(path)?;
    // actual_base64 matches npm Subresource Integrity encoding.
    // actual_base64 对齐 npm 子资源完整性编码。
    let actual_base64 = base64::engine::general_purpose::STANDARD.encode(actual);
    if actual_base64 != expected {
        return Err(format!("SHA-512 integrity mismatch for {}", path.display()));
    }
    Ok(())
}

/// Stream one file through a selected SHA-2 digest implementation.
/// 通过选定的 SHA-2 摘要实现流式读取一个文件。
fn digest_file<D>(path: &Path) -> Result<Vec<u8>, String>
where
    D: Digest + Default,
{
    // input is the downloaded asset opened without loading it all into memory.
    // input 是无需整体载入内存即可打开的下载资产。
    let mut input = File::open(path)
        .map_err(|error| format!("failed to open {} for hashing: {error}", path.display()))?;
    // digest accumulates every bounded file chunk.
    // digest 累积每个有界文件分块。
    let mut digest = D::default();
    // buffer bounds hashing memory independently from archive size.
    // buffer 使哈希内存独立于归档大小。
    let mut buffer = vec![0_u8; DOWNLOAD_BUFFER_BYTES];
    loop {
        let read_count = input
            .read(&mut buffer)
            .map_err(|error| format!("failed to hash {}: {error}", path.display()))?;
        if read_count == 0 {
            break;
        }
        digest.update(&buffer[..read_count]);
    }
    Ok(digest.finalize().to_vec())
}

/// Render bytes as deterministic lowercase hexadecimal text.
/// 把字节渲染为确定性小写十六进制文本。
fn hex_text(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// Extract one supported official archive while rejecting paths outside the destination.
/// 解压一个受支持的官方归档，同时拒绝目标目录外路径。
fn extract_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    // file_name selects the exact decoder from the official asset suffix.
    // file_name 根据官方资产后缀选择精确解码器。
    let file_name = archive_path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("archive has no UTF-8 file name: {}", archive_path.display()))?;
    if file_name.ends_with(".zip") {
        return extract_zip(archive_path, destination);
    }
    if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
        let file = File::open(archive_path)
            .map_err(|error| format!("failed to open {}: {error}", archive_path.display()))?;
        return extract_tar(GzDecoder::new(file), destination, archive_path);
    }
    if file_name.ends_with(".tar.xz") {
        let file = File::open(archive_path)
            .map_err(|error| format!("failed to open {}: {error}", archive_path.display()))?;
        return extract_tar(XzDecoder::new(file), destination, archive_path);
    }
    Err(format!(
        "unsupported managed runtime archive: {}",
        archive_path.display()
    ))
}

/// Extract one ZIP archive using enclosed entry paths only.
/// 仅使用受约束条目路径解压一个 ZIP 归档。
fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    // input is the complete verified ZIP archive.
    // input 是完整且经过校验的 ZIP 归档。
    let input = File::open(archive_path)
        .map_err(|error| format!("failed to open {}: {error}", archive_path.display()))?;
    // archive provides bounded random access to verified ZIP entries.
    // archive 提供对经过校验 ZIP 条目的有界随机访问。
    let mut archive = ZipArchive::new(input)
        .map_err(|error| format!("failed to read ZIP {}: {error}", archive_path.display()))?;
    for index in 0..archive.len() {
        // entry is one archive member whose path must remain enclosed.
        // entry 是一个路径必须保持受约束的归档成员。
        let mut entry = archive.by_index(index).map_err(|error| {
            format!(
                "failed to read ZIP entry {index} in {}: {error}",
                archive_path.display()
            )
        })?;
        // relative is the sanitized archive member path.
        // relative 是经过清理的归档成员路径。
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("ZIP entry {} escapes extraction root", entry.name()))?;
        // output_path is guaranteed to remain beneath destination.
        // output_path 保证始终位于目标目录下。
        let output_path = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|error| {
                format!(
                    "failed to create ZIP directory {}: {error}",
                    output_path.display()
                )
            })?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("failed to create ZIP parent {}: {error}", parent.display())
            })?;
        }
        let mut output = File::create(&output_path).map_err(|error| {
            format!(
                "failed to create ZIP output {}: {error}",
                output_path.display()
            )
        })?;
        std::io::copy(&mut entry, &mut output).map_err(|error| {
            format!(
                "failed to extract ZIP entry {}: {error}",
                output_path.display()
            )
        })?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output_path, fs::Permissions::from_mode(mode)).map_err(
                |error| {
                    format!(
                        "failed to apply ZIP mode to {}: {error}",
                        output_path.display()
                    )
                },
            )?;
        }
    }
    Ok(())
}

/// Extract one TAR stream while requiring every member to remain under the destination.
/// 解压一个 TAR 流，同时要求每个成员始终位于目标目录下。
fn extract_tar<R>(reader: R, destination: &Path, archive_path: &Path) -> Result<(), String>
where
    R: Read,
{
    // archive streams entries without buffering the complete decompressed payload.
    // archive 流式处理条目，无需缓存完整解压载荷。
    let mut archive = Archive::new(reader);
    // entries is the verified sequential TAR member iterator.
    // entries 是经过校验的顺序 TAR 成员迭代器。
    let entries = archive
        .entries()
        .map_err(|error| format!("failed to read TAR {}: {error}", archive_path.display()))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| {
            format!(
                "failed to read TAR entry in {}: {error}",
                archive_path.display()
            )
        })?;
        let unpacked = entry.unpack_in(destination).map_err(|error| {
            format!(
                "failed to extract TAR entry from {}: {error}",
                archive_path.display()
            )
        })?;
        if !unpacked {
            return Err(format!(
                "TAR entry escapes extraction root in {}",
                archive_path.display()
            ));
        }
    }
    Ok(())
}

/// Write one manifest atomically inside a fully staged installation.
/// 在完整暂存安装内原子写入一个清单。
fn write_runtime_manifest(directory: &Path, manifest: RuntimeManifest<'_>) -> Result<(), String> {
    // manifest_path is the stable file consumed by LuaSkills and host validation.
    // manifest_path 是 LuaSkills 与宿主校验消费的稳定文件。
    let manifest_path = directory.join("runtime-manifest.json");
    // temporary_path prevents partial JSON from being observed.
    // temporary_path 防止外部观察到部分 JSON。
    let temporary_path = directory.join("runtime-manifest.json.tmp");
    // payload is deterministic pretty UTF-8 JSON with a trailing newline.
    // payload 是带尾换行的确定性格式化 UTF-8 JSON。
    let mut payload = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("failed to encode runtime manifest: {error}"))?;
    payload.push(b'\n');
    fs::write(&temporary_path, payload)
        .map_err(|error| format!("failed to write {}: {error}", temporary_path.display()))?;
    fs::rename(&temporary_path, &manifest_path).map_err(|error| {
        format!(
            "failed to publish runtime manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    Ok(())
}

/// Commit one same-volume staging directory with rollback of an invalid previous target.
/// 提交一个同卷暂存目录，并在失败时回滚旧的无效目标。
fn commit_staging_directory(staging: &Path, target: &Path) -> Result<(), String> {
    // backup is a same-volume rollback location for the previous invalid target.
    // backup 是旧无效目标使用的同卷回滚位置。
    let backup = sibling_work_path(target, "previous")?;
    remove_path_if_exists(&backup)?;
    // target_existed records whether rollback is required after a failed commit.
    // target_existed 记录提交失败后是否需要回滚。
    let target_existed = target.exists();
    if target_existed {
        fs::rename(target, &backup).map_err(|error| {
            format!(
                "failed to stage previous runtime {} as {}: {error}",
                target.display(),
                backup.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(staging, target) {
        if target_existed {
            let _ = fs::rename(&backup, target);
        }
        return Err(format!(
            "failed to publish runtime {} as {}: {error}",
            staging.display(),
            target.display()
        ));
    }
    remove_path_if_exists(&backup)?;
    Ok(())
}

/// Build a deterministic same-volume work directory beside one final target.
/// 在一个最终目标旁构建确定性的同卷工作目录。
fn sibling_work_path(target: &Path, suffix: &str) -> Result<PathBuf, String> {
    // parent is the exact distribution directory that guarantees same-volume rename.
    // parent 是保证同卷重命名的精确发行目录。
    let parent = target
        .parent()
        .ok_or_else(|| format!("runtime target has no parent: {}", target.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create runtime parent {}: {error}",
            parent.display()
        )
    })?;
    // name is the final directory name reused in the hidden work directory.
    // name 是复用于隐藏工作目录的最终目录名。
    let name = target
        .file_name()
        .ok_or_else(|| format!("runtime target has no file name: {}", target.display()))?;
    Ok(parent.join(format!(".{}.{}", name.to_string_lossy(), suffix)))
}

/// Recreate one empty working directory.
/// 重新创建一个空工作目录。
fn prepare_clean_directory(path: &Path) -> Result<(), String> {
    remove_path_if_exists(path)?;
    fs::create_dir_all(path)
        .map_err(|error| format!("failed to create directory {}: {error}", path.display()))
}

/// Remove one file, symlink, or directory when it exists.
/// 在文件、符号链接或目录存在时将其删除。
fn remove_path_if_exists(path: &Path) -> Result<(), String> {
    // metadata inspects the link itself so cleanup never follows a stale symlink.
    // metadata 检查链接自身，使清理过程永不跟随陈旧符号链接。
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("failed to inspect {}: {error}", path.display())),
    };
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
            .map_err(|error| format!("failed to remove directory {}: {error}", path.display()))
    } else {
        fs::remove_file(path)
            .map_err(|error| format!("failed to remove file {}: {error}", path.display()))
    }
}

/// Find one exact file name recursively without following directory symlinks.
/// 在不跟随目录符号链接的情况下递归查找一个精确文件名。
fn find_file_named(root: &Path, file_name: &OsStr) -> Result<Option<PathBuf>, String> {
    for entry in fs::read_dir(root)
        .map_err(|error| format!("failed to read directory {}: {error}", root.display()))?
    {
        let entry = entry
            .map_err(|error| format!("failed to read entry under {}: {error}", root.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", entry.path().display()))?;
        if file_type.is_file() && entry.file_name() == file_name {
            return Ok(Some(entry.path()));
        }
        if file_type.is_dir()
            && let Some(found) = find_file_named(&entry.path(), file_name)?
        {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// Mark one runtime binary executable on Unix and leave Windows ACL behavior unchanged.
/// 在 Unix 上把一个运行时二进制标记为可执行，并保持 Windows ACL 行为不变。
fn mark_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("failed to inspect executable {}: {error}", path.display()))?
            .permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed to mark {} executable: {error}", path.display()))?;
    }
    #[cfg(windows)]
    {
        if !path.is_file() {
            return Err(format!("runtime executable is missing: {}", path.display()));
        }
    }
    Ok(())
}

/// Verify a managed executable starts successfully and reports the exact version.
/// 校验受管可执行文件可成功启动并报告精确版本。
fn verify_binary_version<A>(
    executable: &Path,
    arguments: &[A],
    expected: &str,
) -> Result<(), String>
where
    A: AsRef<OsStr>,
{
    // output captures version text and bounded error diagnostics.
    // output 捕获版本文本与有界错误诊断。
    // process_executable and process_arguments remove Windows device prefixes only at the subprocess boundary.
    // process_executable 与 process_arguments 仅在子进程边界移除 Windows 设备前缀。
    let process_executable = vulcan_runtime_assets::native_host_path(executable);
    let process_arguments = arguments
        .iter()
        .map(|argument| {
            vulcan_runtime_assets::native_host_path(Path::new(argument.as_ref())).into_os_string()
        })
        .collect::<Vec<_>>();
    // command owns the Release-aware executable version-check invocation.
    // command 持有带 Release 无控制台处理的可执行文件版本校验调用。
    let mut command = Command::new(&process_executable);
    command.args(&process_arguments);
    apply_release_no_window_to_command(&mut command);
    let output = command
        .output()
        .map_err(|error| format!("failed to start {}: {error}", executable.display()))?;
    ensure_process_success(
        &format!("version check for {}", executable.display()),
        &output,
    )?;
    // combined_text accepts tools that report versions on either standard stream.
    // combined_text 接受把版本报告到任一标准流的工具。
    let combined_text = format!(
        "{}\n{}",
        decode_process_output_bytes(&output.stdout),
        decode_process_output_bytes(&output.stderr)
    );
    if !combined_text.contains(expected) {
        return Err(format!(
            "{} did not report expected version {expected}: {}",
            executable.display(),
            truncate_diagnostic(combined_text.as_bytes())
        ));
    }
    Ok(())
}

/// Convert one failed subprocess output into a bounded actionable error.
/// 把一个失败的子进程输出转换为有界且可处理的错误。
fn ensure_process_success(label: &str, output: &Output) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{label} failed with {}; stdout={}; stderr={}",
        output.status,
        truncate_diagnostic(&output.stdout),
        truncate_diagnostic(&output.stderr)
    ))
}

/// Truncate one process diagnostic without splitting UTF-8 code points.
/// 在不拆分 UTF-8 码点的情况下截断一个进程诊断。
fn truncate_diagnostic(bytes: &[u8]) -> String {
    let text = decode_process_output_bytes(bytes);
    if text.len() <= MAX_PROCESS_DIAGNOSTIC_BYTES {
        return text.trim().to_string();
    }
    let mut boundary = MAX_PROCESS_DIAGNOSTIC_BYTES;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}…", text[..boundary].trim())
}

/// Decodes complete managed-runtime process output as user text while retaining a lossy last-resort diagnostic.
/// 将完整的受管运行时进程输出解码为用户文本，并保留有损的最终诊断回退。
///
/// Parameters: `bytes` contains one complete stdout or stderr payload.
/// Returns: UTF-8 or GB18030 text, falling back to replacement decoding only for malformed diagnostics.
/// 参数：`bytes` 包含一段完整的 stdout 或 stderr 载荷。
/// 返回值：UTF-8 或 GB18030 文本；仅在诊断字节损坏时回退到替代字符解码。
fn decode_process_output_bytes(bytes: &[u8]) -> String {
    decode_complete_user_text(bytes)
        .map(|decoded| decoded.content)
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned())
}

/// Resolve a canonical target path relative to one canonical containing directory.
/// 解析一个规范目标路径相对于规范包含目录的路径。
fn relative_canonical_path(base: &Path, target: &Path) -> Result<PathBuf, String> {
    // canonical_base normalizes device prefixes and path case behavior consistently.
    // canonical_base 一致规范化设备前缀与路径大小写行为。
    let canonical_base = fs::canonicalize(base)
        .map_err(|error| format!("failed to canonicalize {}: {error}", base.display()))?;
    // canonical_target normalizes the exact executable returned by uv.
    // canonical_target 规范化 uv 返回的精确可执行文件。
    let canonical_target = fs::canonicalize(target)
        .map_err(|error| format!("failed to canonicalize {}: {error}", target.display()))?;
    canonical_target
        .strip_prefix(&canonical_base)
        .map(Path::to_path_buf)
        .map_err(|_| {
            format!(
                "managed executable {} is outside staging root {}",
                canonical_target.display(),
                canonical_base.display()
            )
        })
}

/// Render one relative path with stable forward separators for JSON manifests.
/// 使用稳定正斜杠为 JSON 清单渲染一个相对路径。
fn relative_path_text(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify every published platform maps to exact official asset names.
    /// 验证每个已发布平台都会映射到精确官方资产名。
    #[test]
    fn published_platform_assets_are_exact() {
        let windows = PlatformAssets::for_platform("windows-x64", "24.18.0")
            .expect("Windows x64 should be supported");
        let linux_arm = PlatformAssets::for_platform("linux-arm64", "24.18.0")
            .expect("Linux ARM64 should be supported");

        assert_eq!(windows.uv_asset, "uv-x86_64-pc-windows-msvc.zip");
        assert_eq!(windows.node_asset, "node-v24.18.0-win-x64.zip");
        assert_eq!(linux_arm.uv_asset, "uv-aarch64-unknown-linux-gnu.tar.gz");
        assert_eq!(linux_arm.node_asset, "node-v24.18.0-linux-arm64.tar.xz");
        assert!(PlatformAssets::for_platform("windows-arm64", "24.18.0").is_err());
    }

    /// Verify checksum parsing selects only the exact requested Node.js asset.
    /// 验证校验和解析只选择精确请求的 Node.js 资产。
    #[test]
    fn named_checksum_parser_selects_exact_asset() {
        let first = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  node-a.zip";
        let second = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  node-b.zip";
        let document = format!("{first}\n{second}\n");

        assert_eq!(
            parse_named_sha256(&document, "node-b.zip").expect("checksum should exist"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert!(parse_named_sha256(&document, "node-c.zip").is_err());
    }

    /// Verify work paths remain siblings of their final target for atomic rename.
    /// 验证工作路径始终与最终目标同级，以支持原子重命名。
    #[test]
    fn staging_path_is_same_volume_sibling() {
        let target = std::env::temp_dir()
            .join("vulcan-runtime-installer-path-test")
            .join("node-24.18.0-windows-x64");
        let staging =
            sibling_work_path(&target, "installing").expect("staging path should resolve");

        assert_eq!(staging.parent(), target.parent());
        assert_eq!(
            staging.file_name().and_then(OsStr::to_str),
            Some(".node-24.18.0-windows-x64.installing")
        );
        if let Some(parent) = target.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    /// Verify uv's Windows version junction is removed without deleting its interpreter before the atomic rename.
    /// 验证原子重命名前移除 uv 的 Windows 版本目录联接，同时保留实际解释器。
    #[cfg(windows)]
    #[test]
    fn uv_staging_alias_does_not_escape_into_published_runtime() {
        use std::os::windows::process::CommandExt;
        use std::time::{SystemTime, UNIX_EPOCH};

        // nonce separates simultaneous test runs without touching existing temporary data.
        // nonce 隔离并行测试运行，且不触碰已有临时数据。
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        // root is a unique test-owned directory, and all cleanup remains beneath this exact root.
        // root 是测试独占的唯一目录，所有清理均限制在这个精确根目录之下。
        let root = std::env::temp_dir().join(format!(
            "vulcan-python-alias-test-{}-{nonce}",
            std::process::id()
        ));
        // published is the final Python runtime directory used after commit.
        // published 是提交后的最终 Python 运行时目录。
        let published = root.join("cpython-3.14.6-windows-x64");
        // staging is the same-volume sibling given to uv before commit.
        // staging 是提交前交给 uv 的同卷同级暂存目录。
        let staging = sibling_work_path(&published, "installing").expect("staging should resolve");
        // interpreter_root is the actual full-version installation behind uv's alias.
        // interpreter_root 是 uv 别名背后的实际完整版本安装目录。
        let interpreter_root = staging.join("cpython-3.14.6-windows-x86_64-none");
        // interpreter marks the exact executable location recorded in the runtime manifest.
        // interpreter 标记运行时清单记录的精确可执行文件位置。
        let interpreter = interpreter_root.join("python.exe");
        // unix_interpreter exercises the additional bin directory used on Unix platforms.
        // unix_interpreter 覆盖 Unix 平台在版本根之下额外使用 bin 目录的布局。
        let unix_interpreter = interpreter_root.join("bin/python3");
        // alias is the short-version directory junction created by uv.
        // alias 是 uv 创建的短版本目录联接。
        let alias = staging.join("cpython-3.14-windows-x86_64-none");
        fs::create_dir_all(
            unix_interpreter
                .parent()
                .expect("nested interpreter has a parent"),
        )
        .expect("interpreter root should be created");
        fs::write(&interpreter, b"test interpreter").expect("interpreter marker should be created");
        fs::write(&unix_interpreter, b"test nested interpreter")
            .expect("nested interpreter marker should be created");
        // A directory junction requires no Developer Mode and reproduces uv's actual Windows alias.
        // 目录联接不要求开发者模式，且复现 uv 在 Windows 创建的实际别名。
        // status confirms native junction creation before testing alias removal.
        // status 在测试别名清理前确认原生目录联接已创建。
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&alias)
            .arg(&interpreter_root)
            .creation_flags(0x0800_0000)
            .status()
            .expect("junction command should launch");
        assert!(status.success(), "junction creation should succeed");

        remove_uv_staging_aliases(
            &staging,
            Path::new("cpython-3.14.6-windows-x86_64-none/bin/python3"),
        )
        .expect("uv alias should be removed");
        assert!(
            fs::symlink_metadata(&alias).is_err(),
            "uv alias should be absent"
        );
        commit_staging_directory(&staging, &published).expect("runtime should publish");
        assert!(
            published
                .join("cpython-3.14.6-windows-x86_64-none/python.exe")
                .exists(),
            "actual interpreter should survive alias cleanup and publish"
        );
        fs::remove_dir_all(&root).expect("test-owned runtime should be removed");
    }

    /// Verify Node.js version checks accept a canonical Windows verbatim executable path.
    /// 验证 Node.js 版本校验可接受规范的 Windows verbatim 可执行路径。
    #[cfg(windows)]
    #[test]
    #[ignore = "requires VULCAN_MANAGED_NODE_EXECUTABLE to reference an installed Node.js"]
    fn node_version_check_accepts_verbatim_executable_path() {
        let executable = std::env::var_os("VULCAN_MANAGED_NODE_EXECUTABLE")
            .map(PathBuf::from)
            .expect("VULCAN_MANAGED_NODE_EXECUTABLE must be set");
        let canonical_executable =
            fs::canonicalize(&executable).expect("managed Node.js executable should canonicalize");

        verify_binary_version(
            &canonical_executable,
            &["--version"],
            env!("VULCAN_MANAGED_NODE_VERSION"),
        )
        .expect("Node.js should accept the normalized subprocess boundary path");
    }
}
