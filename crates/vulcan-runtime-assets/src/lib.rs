//! Runtime asset discovery and output-bundle hydration helpers.
//! 运行时资产发现与输出包同步辅助模块。

use std::path::{Path, PathBuf};

/// Shared external user-text decoding boundaries.
/// 共享的外部用户文本解码边界。
pub mod text_decode;

/// Convert one raw host path into native text without Windows verbatim prefixes.
/// 把一个原始宿主路径转换为不含 Windows verbatim 前缀的原生文本。
///
/// `path` is raw path text produced by filesystem canonicalization or user input.
/// `path` 是文件系统规范化或用户输入产生的原始路径文本。
///
/// Returns equivalent drive or UNC addressing with native Windows separators when applicable.
/// 适用时返回使用 Windows 原生分隔符的等价盘符或 UNC 寻址。
pub fn native_host_path_text(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    // path_without_verbatim_prefix preserves the original drive or UNC address.
    // path_without_verbatim_prefix 保留原始盘符或 UNC 地址。
    let path_without_verbatim_prefix = strip_windows_verbatim_prefix(path);
    if looks_like_windows_path(path, &path_without_verbatim_prefix) {
        path_without_verbatim_prefix.replace('/', "\\")
    } else {
        path_without_verbatim_prefix
    }
}

/// Convert one filesystem path into the shared native host path representation.
/// 把一个文件系统路径转换为共享的宿主原生路径表示。
///
/// `path` is an existing or prospective filesystem path.
/// `path` 是一个已存在或预期的文件系统路径。
///
/// Returns a path buffer safe to pass across ordinary host-process boundaries.
/// 返回可安全传入常规宿主进程边界的路径缓冲区。
pub fn native_host_path(path: &Path) -> PathBuf {
    PathBuf::from(native_host_path_text(&path.to_string_lossy()))
}

/// Render one filesystem path as host-visible native text without Windows verbatim prefixes.
/// 将一个文件系统路径渲染为不含 Windows verbatim 前缀的宿主可见原生文本。
///
/// # Parameters
/// 参数
/// `path` is the filesystem path crossing a JSON, process, persistence, UI, or model boundary.
/// `path` 是即将跨越 JSON、进程、持久化、界面或模型边界的文件系统路径。
///
/// # Returns
/// 返回值
/// A stable native path string suitable for host-facing protocols.
/// 返回适用于宿主侧协议的稳定原生路径字符串。
pub fn native_host_path_text_from_path(path: &Path) -> String {
    native_host_path_text(&path.to_string_lossy())
}

/// Return whether one normalized path retains Windows drive or UNC semantics.
/// 返回一条规范路径是否保留 Windows 盘符或 UNC 语义。
fn looks_like_windows_path(original_path: &str, normalized_path: &str) -> bool {
    is_windows_drive_absolute_path(normalized_path)
        || normalized_path.starts_with(r"\\")
        || original_path.starts_with(r"\\?\UNC\")
        || original_path.starts_with("//?/UNC/")
        || (cfg!(windows) && normalized_path.starts_with("//"))
}

/// Remove Windows verbatim prefixes while preserving ordinary addressing semantics.
/// 移除 Windows verbatim 前缀，同时保留常规寻址语义。
fn strip_windows_verbatim_prefix(path: &str) -> String {
    if path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\UNC\"))
    {
        let rest = &path[8..];
        return format!(r"\\{rest}");
    }
    if path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("//?/UNC/"))
    {
        let rest = &path[8..];
        return format!("//{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\")
        && is_windows_drive_absolute_path(rest)
    {
        return rest.to_string();
    }
    if let Some(rest) = path.strip_prefix("//?/")
        && is_windows_drive_absolute_path(rest)
    {
        return rest.to_string();
    }
    path.to_string()
}

/// Return whether one path starts with an absolute Windows drive address.
/// 返回一条路径是否以 Windows 绝对盘符地址开头。
fn is_windows_drive_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

/// Environment variable that explicitly points the application at one prepared output root.
/// 显式将应用指向某个已准备好 output 根目录的环境变量。
const OUTPUT_ROOT_ENV_KEY: &str = "VULCAN_CODE_OUTPUT_ROOT";

/// Relative resource directory name used by packaged output builds.
/// 打包 output 构建使用的相对资源目录名称。
const RESOURCES_DIR_NAME: &str = "resources";

/// Relative configuration directory name used by packaged output builds.
/// 打包 output 构建使用的相对配置目录名称。
const CONFIG_DIR_NAME: &str = "config";

/// Relative prompt skills resource directory name nested below the packaged resources root.
/// 位于打包资源根目录下的相对提示词技能资源目录名称。
const PROMPT_SKILLS_DIR_NAME: &str = "prompt-skills";

/// Relative system prompt skills resource directory name nested below prompt skills resources.
/// 位于提示词技能资源目录下的相对系统提示词技能目录名称。
const SYSTEM_PROMPT_SKILLS_DIR_NAME: &str = "system";

/// Relative subagent definition resource directory name nested below resources.
/// 位于资源目录下的相对子代理定义目录名称。
const SUBAGENTS_DIR_NAME: &str = "subagents";

/// Relative provider-manifest resource directory name nested below resources.
/// 位于资源目录下的相对供应商清单资源目录名称。
const PROVIDER_MANIFESTS_DIR_NAME: &str = "provider-manifests";

/// Relative bundled provider manifest directory name nested below provider-manifests.
/// 位于供应商清单资源目录下的相对内置清单目录名称。
const BUNDLED_MANIFESTS_DIR_NAME: &str = "manifests";

/// Relative bundled provider-manifest integrity index file name.
/// 相对内置供应商清单完整性索引文件名。
const BUNDLED_MANIFEST_INDEX_FILE_NAME: &str = "bundled-index.json";

/// Relative JSON Schema directory nested below provider-manifests.
/// 位于供应商清单资源目录下的相对 JSON Schema 目录名称。
const PROVIDER_MANIFEST_SCHEMA_DIR_NAME: &str = "schema";

/// Relative executable fixture directory nested below provider-manifests.
/// 位于供应商清单资源目录下的相对可执行 fixture 目录名称。
const PROVIDER_MANIFEST_FIXTURES_DIR_NAME: &str = "fixtures";

/// Relative Lua runtime packages directory name bundled into the output root.
/// 打包到 output 根目录中的 Lua 运行时包目录名称。
const LUA_PACKAGES_DIR_NAME: &str = "lua_packages";

/// Relative LuaSkills runtime root directory name bundled below the output root.
/// 打包在 output 根目录下的 LuaSkills 运行时根目录名称。
const LUASKILLS_RUNTIME_ROOT_DIR_NAME: &str = "lua_runtime";

/// Relative directory containing Vulcan-managed plugin packages.
/// 包含 Vulcan 托管插件包的相对目录。
const PLUGINS_DIR_NAME: &str = "plugins";

/// Relative dependency directory name shared by host subsystems below the output root.
/// output 根目录下由宿主各子系统共享的相对依赖目录名称。
const DEPENDENCIES_DIR_NAME: &str = "dependencies";

/// Relative managed-runtime distribution directory nested below the shared dependency root.
/// 位于共享依赖根目录下的相对受管运行时发行目录名称。
const MANAGED_RUNTIME_DISTRIBUTIONS_DIR_NAME: &str = "runtimes";

/// Relative controller binary directory name bundled into the output root.
/// 打包到 output 根目录中的控制器二进制目录名称。
const CONTROLLER_BIN_DIR_NAME: &str = "bin";

/// Returns the source-tree resource directory compiled into the application.
/// 返回编译进应用的源码树资源目录。
pub fn source_resource_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
}

/// Returns the source-tree configuration directory used by local development runs.
/// 返回本地开发运行使用的源码树配置目录。
pub fn source_config_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("config")
}

/// Returns the effective packaged system prompt skills directory for the current process.
/// 返回当前进程使用的生效打包系统提示词技能目录。
pub fn resolve_system_prompt_skills_resource_directory() -> PathBuf {
    resolve_app_resources_directory()
        .join(PROMPT_SKILLS_DIR_NAME)
        .join(SYSTEM_PROMPT_SKILLS_DIR_NAME)
}

/// Returns the effective packaged system subagent definition directory for the current process.
/// 返回当前进程使用的生效打包系统子代理定义目录。
pub fn resolve_system_subagents_resource_directory() -> PathBuf {
    resolve_app_resources_directory().join(SUBAGENTS_DIR_NAME)
}

/// Returns the effective provider-manifest resource directory for the current process.
/// 返回当前进程使用的生效供应商清单资源目录。
pub fn resolve_provider_manifests_directory() -> PathBuf {
    resolve_app_resources_directory().join(PROVIDER_MANIFESTS_DIR_NAME)
}

/// Returns the effective bundled provider manifest directory for the current process.
/// 返回当前进程使用的生效内置供应商清单目录。
pub fn resolve_bundled_provider_manifests_directory() -> PathBuf {
    resolve_provider_manifests_directory().join(BUNDLED_MANIFESTS_DIR_NAME)
}

/// Returns the effective bundled provider-manifest integrity index for the current process.
/// 返回当前进程使用的生效内置供应商清单完整性索引。
pub fn resolve_bundled_provider_manifest_index_path() -> PathBuf {
    resolve_provider_manifests_directory().join(BUNDLED_MANIFEST_INDEX_FILE_NAME)
}

/// Returns the source-tree bundled provider manifest directory for local development runs.
/// 返回本地开发运行使用的源码树内置供应商清单目录。
pub fn source_bundled_provider_manifests_directory() -> PathBuf {
    source_resource_directory()
        .join(PROVIDER_MANIFESTS_DIR_NAME)
        .join(BUNDLED_MANIFESTS_DIR_NAME)
}

/// Returns the source-tree bundled provider-manifest integrity index for local verification.
/// 返回本地校验使用的源码树内置供应商清单完整性索引。
pub fn source_bundled_provider_manifest_index_path() -> PathBuf {
    source_resource_directory()
        .join(PROVIDER_MANIFESTS_DIR_NAME)
        .join(BUNDLED_MANIFEST_INDEX_FILE_NAME)
}

/// Returns the source-tree provider-manifest JSON Schema directory.
/// 返回源码树 Provider Manifest JSON Schema 目录。
pub fn source_provider_manifest_schema_directory() -> PathBuf {
    source_resource_directory()
        .join(PROVIDER_MANIFESTS_DIR_NAME)
        .join(PROVIDER_MANIFEST_SCHEMA_DIR_NAME)
}

/// Returns the source-tree provider-manifest execution fixture directory.
/// 返回源码树 Provider Manifest 执行 fixture 目录。
pub fn source_provider_manifest_fixtures_directory() -> PathBuf {
    source_resource_directory()
        .join(PROVIDER_MANIFESTS_DIR_NAME)
        .join(PROVIDER_MANIFEST_FIXTURES_DIR_NAME)
}

/// Returns the effective application resource directory for the current process.
/// 返回当前进程使用的生效应用资源目录。
pub fn resolve_app_resources_directory() -> PathBuf {
    if let Some(output_root) = resolve_output_root() {
        let packaged_resources = output_root.join(RESOURCES_DIR_NAME);
        if packaged_resources.is_dir() {
            return packaged_resources;
        }
    }

    source_resource_directory()
}

/// Returns the effective application configuration directory for the current process.
/// 返回当前进程使用的生效应用配置目录。
pub fn resolve_app_config_directory() -> PathBuf {
    if let Some(output_root) = resolve_output_root() {
        let packaged_config = output_root.join(CONFIG_DIR_NAME);
        if packaged_config.is_dir() {
            return packaged_config;
        }
    }

    source_config_directory()
}

/// Returns the output-owned configuration directory without falling back to source files.
/// 返回 output 持有的配置目录，且不会回退到源码文件。
///
/// # Returns
/// 返回值
/// The packaged `output/config` directory candidate, or `None` when no output root exists.
/// 返回打包的 `output/config` 目录候选；不存在 output 根目录时返回 `None`。
pub fn resolve_output_config_directory() -> Option<PathBuf> {
    if let Some(output_root) = resolve_output_root() {
        return Some(output_root.join(CONFIG_DIR_NAME));
    }

    let repository_output_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("output");
    repository_output_root
        .is_dir()
        .then(|| repository_output_root.join(CONFIG_DIR_NAME))
}

/// Returns the effective controller configuration file path for the current process.
/// 返回当前进程使用的生效控制器配置文件路径。
pub fn resolve_controller_config_file_path() -> PathBuf {
    resolve_app_config_directory().join("controller.json")
}

/// Returns one bundled controller executable path when the current process runs from one packaged output root.
/// 当当前进程从某个打包 output 根目录运行时，返回其中的控制器可执行文件路径。
pub fn resolve_bundled_controller_executable_path() -> Option<PathBuf> {
    let executable_name = controller_executable_name();
    let mut candidates = Vec::new();

    if let Some(output_root) = resolve_output_root() {
        candidates.push(
            output_root
                .join(CONTROLLER_BIN_DIR_NAME)
                .join(executable_name),
        );
    }

    if let Some(executable_dir) = current_executable_directory() {
        candidates.push(executable_dir.join(executable_name));
    }

    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("third_party")
            .join("vldb_controller")
            .join("bin")
            .join(executable_name),
    );

    candidates.into_iter().find(|path| path.is_file())
}

/// Resolves the active LuaSkills runtime root used by the WebUI host.
/// 解析WebUI宿主实际使用的 LuaSkills 活动运行根目录。
pub fn resolve_luaskills_runtime_root() -> Option<PathBuf> {
    if let Some(output_root) = resolve_output_root() {
        let packaged_runtime_root = output_root.join(LUASKILLS_RUNTIME_ROOT_DIR_NAME);
        if packaged_runtime_root.is_dir() {
            return Some(packaged_runtime_root);
        }
        return Some(output_root);
    }

    let repository_output_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("output");
    if repository_output_root.is_dir() {
        let packaged_runtime_root = repository_output_root.join(LUASKILLS_RUNTIME_ROOT_DIR_NAME);
        if packaged_runtime_root.is_dir() {
            return Some(packaged_runtime_root);
        }
        return Some(repository_output_root);
    }

    None
}

/// Returns the bundled Vulcan plugin root below the active output directory.
/// 返回当前 output 目录下的内建 Vulcan 插件根目录。
///
/// # Returns
/// 返回值
/// The exact `<VULCAN_CODE_ROOT>/plugins` directory, or `None` when no output root exists.
/// 返回精确的 `<VULCAN_CODE_ROOT>/plugins` 目录；不存在 output 根时返回 `None`。
pub fn resolve_bundled_plugin_root() -> Option<PathBuf> {
    if let Some(output_root) = resolve_output_root() {
        return Some(output_root.join(PLUGINS_DIR_NAME));
    }
    let repository_output_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("output");
    repository_output_root
        .is_dir()
        .then(|| repository_output_root.join(PLUGINS_DIR_NAME))
}

/// Returns the current user's Vulcan plugin installation root.
/// 返回当前用户的 Vulcan 插件安装根目录。
///
/// # Returns
/// 返回值
/// The exact `~/vulcan/code/plugins` directory, or `None` when the home directory is unavailable.
/// 返回精确的 `~/vulcan/code/plugins` 目录；无法取得用户主目录时返回 `None`。
pub fn resolve_user_plugin_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join("vulcan").join("code").join("plugins"))
}

/// Returns the current user's Vulcan plugin state root.
/// 返回当前用户的 Vulcan 插件状态根目录。
///
/// # Returns
/// 返回值
/// The exact `~/vulcan/code/plugin-state` directory, or `None` when home is unavailable.
/// 返回精确的 `~/vulcan/code/plugin-state` 目录；无法取得用户主目录时返回 `None`。
pub fn resolve_user_plugin_state_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join("vulcan").join("code").join("plugin-state"))
}

/// Returns the current user's Vulcan plugin dependency runtime root.
/// 返回当前用户的 Vulcan 插件依赖运行根目录。
///
/// # Returns
/// 返回值
/// The exact `~/vulcan/code/plugin-runtime` directory, or `None` when home is unavailable.
/// 返回精确的 `~/vulcan/code/plugin-runtime` 目录；无法取得用户主目录时返回 `None`。
pub fn resolve_user_plugin_runtime_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join("vulcan").join("code").join("plugin-runtime"))
}

/// Resolves the host-owned distribution root shared by managed Python and Node consumers.
/// 解析由宿主持有、供受管 Python 与 Node 消费方共享的发行根目录。
///
/// # Returns
/// 返回值
/// The `output/dependencies/runtimes` path selected by the packaged or repository output root.
/// 由打包或仓库 output 根目录选出的 `output/dependencies/runtimes` 路径。
pub fn resolve_managed_runtime_distribution_root() -> Option<PathBuf> {
    if let Some(output_root) = resolve_output_root() {
        return Some(
            output_root
                .join(DEPENDENCIES_DIR_NAME)
                .join(MANAGED_RUNTIME_DISTRIBUTIONS_DIR_NAME),
        );
    }

    let repository_output_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("output");
    repository_output_root.is_dir().then(|| {
        repository_output_root
            .join(DEPENDENCIES_DIR_NAME)
            .join(MANAGED_RUNTIME_DISTRIBUTIONS_DIR_NAME)
    })
}

/// Resolves one packaged output root from the environment or the current executable location.
/// 根据环境变量或当前可执行文件位置解析一个打包 output 根目录。
fn resolve_output_root() -> Option<PathBuf> {
    if let Some(explicit_root) = std::env::var_os(OUTPUT_ROOT_ENV_KEY)
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    {
        return Some(explicit_root);
    }

    let executable_dir = current_executable_directory()?;
    let mut candidates = vec![executable_dir.clone()];

    if let Some(parent) = executable_dir.parent() {
        candidates.push(parent.to_path_buf());
    }

    candidates.into_iter().find(|candidate| {
        candidate.join(RESOURCES_DIR_NAME).is_dir()
            || candidate.join(CONFIG_DIR_NAME).is_dir()
            || candidate.join(LUASKILLS_RUNTIME_ROOT_DIR_NAME).is_dir()
            || candidate.join(LUA_PACKAGES_DIR_NAME).is_dir()
            || candidate.join(CONTROLLER_BIN_DIR_NAME).is_dir()
    })
}

/// Returns the current executable directory when the process was launched from a real binary path.
/// 当进程从真实二进制路径启动时，返回当前可执行文件所在目录。
fn current_executable_directory() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

/// Returns the platform-specific controller binary file name.
/// 返回平台相关的控制器二进制文件名。
fn controller_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "vldb-controller.exe"
    } else {
        "vldb-controller"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify drive and UNC verbatim paths share one stable native conversion.
    /// 验证盘符与 UNC verbatim 路径共享同一稳定原生转换。
    #[test]
    fn native_host_path_text_removes_windows_verbatim_prefixes() {
        assert_eq!(
            native_host_path_text(r"\\?\D:\openvulcan\vulcan-code"),
            r"D:\openvulcan\vulcan-code"
        );
        assert_eq!(
            native_host_path_text(r"\\?\UNC\server\share\runtime"),
            r"\\server\share\runtime"
        );
        assert_eq!(
            native_host_path_text("//?/D:/openvulcan/vulcan-code"),
            r"D:\openvulcan\vulcan-code"
        );
        assert_eq!(
            native_host_path_text("//?/UNC/server/share/runtime"),
            r"\\server\share\runtime"
        );
        assert_eq!(
            native_host_path_text_from_path(Path::new(r"\\?\D:\openvulcan\vulcan-code")),
            r"D:\openvulcan\vulcan-code"
        );
        assert_eq!(
            native_host_path_text(r"D:\directory with trailing space "),
            r"D:\directory with trailing space "
        );
        assert_eq!(
            native_host_path_text(r"\\?\Volume{01234567-89AB-CDEF-0123-456789ABCDEF}\runtime"),
            r"\\?\Volume{01234567-89AB-CDEF-0123-456789ABCDEF}\runtime"
        );
    }
}
