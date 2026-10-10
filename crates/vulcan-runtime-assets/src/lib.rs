//! Runtime asset discovery and output-bundle hydration helpers.
//! 运行时资产发现与输出包同步辅助模块。

use std::path::{Path, PathBuf};

/// Build-profile-specific user root directory name used by every user-scoped Vulcan resource.
/// 所有用户级 Vulcan 资源统一使用的按构建配置区分的用户根目录名。
pub const USER_VULCAN_ROOT_DIRECTORY_NAME: &str = if cfg!(debug_assertions) {
    ".vulcan-debug"
} else {
    ".vulcan"
};

/// User-level Vulcan Code namespace directory name.
/// 用户级 Vulcan Code 命名空间目录名。
pub const USER_VULCAN_CODE_DIRECTORY_NAME: &str = "code";

/// Release browser-facing service start port.
/// Release 浏览器侧服务起始端口。
pub const RELEASE_WEBUI_SERVICE_HTTP_PORT: u16 = 17_891;

/// Release browser-facing service end port.
/// Release 浏览器侧服务结束端口。
pub const RELEASE_WEBUI_SERVICE_HTTP_PORT_END: u16 = 17_920;

/// Debug browser-facing service start port.
/// Debug 浏览器侧服务起始端口。
pub const DEBUG_WEBUI_SERVICE_HTTP_PORT: u16 = 17_921;

/// Debug browser-facing service end port.
/// Debug 浏览器侧服务结束端口。
pub const DEBUG_WEBUI_SERVICE_HTTP_PORT_END: u16 = 17_950;

/// Returns the default browser-facing service start port for the current Cargo profile.
/// 返回当前 Cargo 构建配置对应的浏览器侧服务起始端口。
pub const fn default_webui_service_http_port() -> u16 {
    if cfg!(debug_assertions) {
        DEBUG_WEBUI_SERVICE_HTTP_PORT
    } else {
        RELEASE_WEBUI_SERVICE_HTTP_PORT
    }
}

/// Returns the default browser-facing service end port for the current Cargo profile.
/// 返回当前 Cargo 构建配置对应的浏览器侧服务结束端口。
pub const fn default_webui_service_http_port_end() -> u16 {
    if cfg!(debug_assertions) {
        DEBUG_WEBUI_SERVICE_HTTP_PORT_END
    } else {
        RELEASE_WEBUI_SERVICE_HTTP_PORT_END
    }
}

/// Builds the user Vulcan root below one explicit home directory.
/// 根据一个明确的用户主目录构建用户级 Vulcan 根目录。
pub fn user_vulcan_root_for_home(home: &Path) -> PathBuf {
    #[cfg(feature = "test-support")]
    if let Some(root) = test_support::isolated_user_root() {
        return root;
    }
    home.join(USER_VULCAN_ROOT_DIRECTORY_NAME)
}

/// Builds the user Vulcan Code root below one explicit home directory.
/// 根据一个明确的用户主目录构建用户级 Vulcan Code 根目录。
pub fn user_vulcan_code_root_for_home(home: &Path) -> PathBuf {
    user_vulcan_root_for_home(home).join(USER_VULCAN_CODE_DIRECTORY_NAME)
}

/// Resolves the current user's build-profile-specific Vulcan root.
/// 解析当前用户对应当前构建配置的 Vulcan 根目录。
pub fn resolve_user_vulcan_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| user_vulcan_root_for_home(&home))
}

/// Resolves the current user's build-profile-specific Vulcan Code root.
/// 解析当前用户对应当前构建配置的 Vulcan Code 根目录。
pub fn resolve_user_vulcan_code_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| user_vulcan_code_root_for_home(&home))
}

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

/// Relative directory containing host-owned bundled command-line tools.
/// 包含宿主自有内建命令行工具的相对目录。
const TOOLS_DIR_NAME: &str = "tools";

/// Relative directory containing the bundled ONNX Runtime of the local permission judge.
/// 包含本机权限判定器随包 ONNX Runtime 的相对目录。
const JUDGE_RUNTIME_DIR_NAME: &str = "judge-runtime";

/// Relative executable directory nested below the bundled-tools root.
/// 位于内建工具根目录下的相对可执行文件目录。
const TOOLS_BIN_DIR_NAME: &str = "bin";

/// Relative dependency directory name shared by host subsystems below the output root.
/// output 根目录下由宿主各子系统共享的相对依赖目录名称。
const DEPENDENCIES_DIR_NAME: &str = "dependencies";

/// Relative managed-runtime distribution directory nested below the shared dependency root.
/// 位于共享依赖根目录下的相对受管运行时发行目录名称。
const MANAGED_RUNTIME_DISTRIBUTIONS_DIR_NAME: &str = "runtimes";

/// Relative bundled binary directory name inside the output root.
/// 打包到 output 根目录中的二进制目录名称。
const BIN_DIR_NAME: &str = "bin";

/// Returns the source-tree resource directory compiled into the application.
/// 返回编译进应用的源码树资源目录。
pub fn source_resource_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
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

/// Explicit path isolation for standalone integration-test processes; excluded from normal builds.
/// 独立集成测试进程的显式路径隔离；普通构建不包含此入口。
#[cfg(feature = "test-support")]
pub mod test_support;

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

/// Returns the bundled command-line tool directory below the active output root.
/// 返回当前 output 根目录下的内建命令行工具目录。
///
/// # Returns
/// 返回值
/// The exact existing `<VULCAN_CODE_ROOT>/tools/bin` directory, or `None` when it is not deployed.
/// 返回精确且已存在的 `<VULCAN_CODE_ROOT>/tools/bin` 目录；尚未部署时返回 `None`。
pub fn resolve_bundled_tool_bin_directory() -> Option<PathBuf> {
    // candidates preserves packaged-output precedence before the repository development output.
    // candidates 保持打包 output 优先于仓库开发 output 的顺序。
    let mut candidates = Vec::new();
    // outputRoot is the packaged runtime root resolved from environment or executable location.
    // outputRoot 是从环境变量或可执行文件位置解析的打包运行时根目录。
    if let Some(output_root) = resolve_output_root() {
        candidates.push(output_root.join(TOOLS_DIR_NAME).join(TOOLS_BIN_DIR_NAME));
    }
    // repositoryOutputRoot supports binaries launched through Cargo while make sync owns deployment.
    // repositoryOutputRoot 支持通过 Cargo 启动的二进制，同时仍由 make sync 持有部署过程。
    let repository_output_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("output");
    candidates.push(
        repository_output_root
            .join(TOOLS_DIR_NAME)
            .join(TOOLS_BIN_DIR_NAME),
    );
    candidates.into_iter().find(|path| path.is_dir())
}

/// Returns the bundled judge runtime directory below the active output root.
/// 返回当前 output 根目录下的随包判定执行库目录。
///
/// # Returns
/// 返回值
/// The exact existing `<VULCAN_CODE_ROOT>/judge-runtime` directory, or `None` when this host ships none.
/// 返回精确且已存在的 `<VULCAN_CODE_ROOT>/judge-runtime` 目录；本宿主未随包提供时返回 `None`。
pub fn resolve_bundled_judge_runtime_root() -> Option<PathBuf> {
    // candidates preserves packaged-output precedence before the repository development output.
    // candidates 保持打包 output 优先于仓库开发 output 的顺序。
    let mut candidates = Vec::new();
    if let Some(output_root) = resolve_output_root() {
        candidates.push(output_root.join(JUDGE_RUNTIME_DIR_NAME));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("output")
            .join(JUDGE_RUNTIME_DIR_NAME),
    );
    candidates.into_iter().find(|path| path.is_dir())
}

/// Returns the user-owned root of the judge's managed CUDA runtime, which survives app upgrades.
/// 返回判定器受管 CUDA 执行库的用户级根目录，应用升级后仍保留。
///
/// # Returns
/// 返回值
/// The exact `<user-vulcan-root>/code/judge-cuda` directory, or `None` when home is unavailable.
/// 返回精确的 `<user-vulcan-root>/code/judge-cuda` 目录；无法取得用户主目录时返回 `None`。
pub fn resolve_judge_cuda_runtime_root() -> Option<PathBuf> {
    resolve_user_vulcan_code_root().map(|root| root.join("judge-cuda"))
}

/// Returns the user-owned directory of the permission judge's decision log.
/// 返回权限判定判定记录的用户级目录。
///
/// # Returns
/// 返回值
/// The exact `<user-vulcan-root>/code/judge-records` directory, or `None` when home is unavailable.
/// 返回精确的 `<user-vulcan-root>/code/judge-records` 目录；无法取得用户主目录时返回 `None`。
pub fn resolve_judge_records_root() -> Option<PathBuf> {
    resolve_user_vulcan_code_root().map(|root| root.join("judge-records"))
}

/// Returns the current user's Vulcan plugin installation root.
/// 返回当前用户的 Vulcan 插件安装根目录。
///
/// # Returns
/// 返回值
/// The exact `<user-vulcan-root>/code/plugins` directory, or `None` when the home directory is unavailable.
/// 返回精确的 `<user-vulcan-root>/code/plugins` 目录；无法取得用户主目录时返回 `None`。
pub fn resolve_user_plugin_root() -> Option<PathBuf> {
    resolve_user_vulcan_code_root().map(|root| root.join("plugins"))
}

/// Returns the current user's Vulcan plugin state root.
/// 返回当前用户的 Vulcan 插件状态根目录。
///
/// # Returns
/// 返回值
/// The exact `<user-vulcan-root>/code/plugin-state` directory, or `None` when home is unavailable.
/// 返回精确的 `<user-vulcan-root>/code/plugin-state` 目录；无法取得用户主目录时返回 `None`。
pub fn resolve_user_plugin_state_root() -> Option<PathBuf> {
    resolve_user_vulcan_code_root().map(|root| root.join("plugin-state"))
}

/// Returns the current user's Vulcan plugin dependency runtime root.
/// 返回当前用户的 Vulcan 插件依赖运行根目录。
///
/// # Returns
/// 返回值
/// The exact `<user-vulcan-root>/code/plugin-runtime` directory, or `None` when home is unavailable.
/// 返回精确的 `<user-vulcan-root>/code/plugin-runtime` 目录；无法取得用户主目录时返回 `None`。
pub fn resolve_user_plugin_runtime_root() -> Option<PathBuf> {
    resolve_user_vulcan_code_root().map(|root| root.join("plugin-runtime"))
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
            || candidate.join(TOOLS_DIR_NAME).is_dir()
            || candidate.join(BIN_DIR_NAME).is_dir()
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

/// Built-in base system prompt used when neither the user nor the project supplies one.
/// 用户与项目都未提供时使用的内置基础系统提示词。
pub const DEFAULT_SYSTEM_PROMPT_MD: &str = include_str!("../resources/system-prompts/default.md");

/// Product-owned conversation rendering guide attached after the editable base prompt.
/// 附加在可编辑基础提示词之后、由产品维护的会话渲染说明。
pub const CAPABILITY_GUIDE_MD: &str =
    include_str!("../resources/system-prompts/capability-guide.md");

/// Canonical compile-time usage guide for implicit LuaSkills dispatch.
/// 隐式 LuaSkills 调度使用的规范编译期指引。
pub const CALL_LUASKILL_USAGE_MD: &str = include_str!("../resources/tool-guides/call-luaskill.md");

/// Interactive answer mode guidance; capacity placeholders are filled from session Core.
/// 互动回答模式指引；容量占位符由会话 Core 填充。
pub const ANSWER_MODE_INTERACTIVE_MD: &str =
    include_str!("../resources/agent-prompts/answer-mode-interactive.md");

/// Standard answer mode notice, sent only to revoke earlier interactive guidance.
/// 标准回答模式通知，仅用于撤销先前的互动指引。
pub const ANSWER_MODE_STANDARD_MD: &str =
    include_str!("../resources/agent-prompts/answer-mode-standard.md");

/// System prompt of the reviewing LLM that decides escalated tool calls.
/// 决定升级工具调用的复核 LLM 系统提示词。
pub const PERMISSION_REVIEW_MD: &str =
    include_str!("../resources/agent-prompts/permission-review.md");

/// System prompt that drafts or revises the base prompt of a dedicated session.
/// 起草或修订专职会话基础提示词的系统提示词。
pub const DEDICATED_PROMPT_GENERATOR_MD: &str =
    include_str!("../resources/agent-prompts/dedicated-prompt-generator.md");

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the user root name is selected by the compiled Cargo profile.
    /// 验证用户根目录名由当前编译使用的 Cargo 配置选择。
    #[test]
    fn user_vulcan_root_name_matches_build_profile() {
        let expected = if cfg!(debug_assertions) {
            ".vulcan-debug"
        } else {
            ".vulcan"
        };
        assert_eq!(USER_VULCAN_ROOT_DIRECTORY_NAME, expected);
    }

    /// Verifies debug and release service defaults are disjoint.
    /// 验证 Debug 与 Release 的服务默认端口彼此不重叠。
    #[test]
    fn profile_port_defaults_are_disjoint() {
        assert_ne!(
            DEBUG_WEBUI_SERVICE_HTTP_PORT,
            RELEASE_WEBUI_SERVICE_HTTP_PORT
        );
    }

    /// Verifies plugin roots stay below the profile-specific user Code namespace.
    /// 验证插件根目录始终位于按构建配置区分的用户 Code 命名空间下。
    #[test]
    fn user_plugin_roots_use_profile_specific_code_root() {
        let home = PathBuf::from(r"C:\Users\example");
        let code_root = user_vulcan_code_root_for_home(&home);

        assert_eq!(
            code_root.join("plugins"),
            home.join(USER_VULCAN_ROOT_DIRECTORY_NAME)
                .join(USER_VULCAN_CODE_DIRECTORY_NAME)
                .join("plugins")
        );
        assert_eq!(
            code_root.join("plugin-state"),
            home.join(USER_VULCAN_ROOT_DIRECTORY_NAME)
                .join(USER_VULCAN_CODE_DIRECTORY_NAME)
                .join("plugin-state")
        );
        assert_eq!(
            code_root.join("plugin-runtime"),
            home.join(USER_VULCAN_ROOT_DIRECTORY_NAME)
                .join(USER_VULCAN_CODE_DIRECTORY_NAME)
                .join("plugin-runtime")
        );
    }

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
