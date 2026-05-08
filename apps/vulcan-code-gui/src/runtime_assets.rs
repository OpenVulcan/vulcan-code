//! Runtime asset discovery and output-bundle hydration helpers.
//! 运行时资产发现与输出包同步辅助模块。

use std::fs;
use std::path::{Path, PathBuf};

/// Environment variable that explicitly points the application at one prepared output root.
/// 显式将应用指向某个已准备好 output 根目录的环境变量。
const OUTPUT_ROOT_ENV_KEY: &str = "VULCAN_CODE_OUTPUT_ROOT";

/// Relative resource directory name used by packaged output builds.
/// 打包 output 构建使用的相对资源目录名称。
const RESOURCES_DIR_NAME: &str = "resources";

/// Relative configuration directory name used by packaged output builds.
/// 打包 output 构建使用的相对配置目录名称。
const CONFIG_DIR_NAME: &str = "config";

/// Relative provider catalog directory name nested below the packaged resources root.
/// 位于打包资源根目录下的相对供应商目录名称。
const PROVIDERS_DIR_NAME: &str = "providers";

/// Relative Lua runtime packages directory name bundled into the output root.
/// 打包到 output 根目录中的 Lua 运行时包目录名称。
const LUA_PACKAGES_DIR_NAME: &str = "lua_packages";

/// Relative native library directory name bundled into the output root.
/// 打包到 output 根目录中的原生库目录名称。
const LIBS_DIR_NAME: &str = "libs";

/// Relative license directory name bundled into the output root.
/// 打包到 output 根目录中的许可证目录名称。
const LICENSES_DIR_NAME: &str = "licenses";

/// Relative skills directory name bundled into the output root.
/// 打包到 output 根目录中的技能目录名称。
const SKILLS_DIR_NAME: &str = "skills";

/// Relative tool directory name bundled into the output root.
/// 打包到 output 根目录中的工具目录名称。
const TOOLS_DIR_NAME: &str = "bin/tools";

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

/// Returns the effective provider catalog directory for the current process.
/// 返回当前进程使用的生效供应商目录路径。
pub fn resolve_provider_catalog_directory() -> PathBuf {
    resolve_app_resources_directory().join(PROVIDERS_DIR_NAME)
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

/// Synchronizes bundled LuaSkills runtime payloads from one packaged output root into the user runtime root.
/// 将某个打包 output 根目录中的 LuaSkills 运行时载荷同步到用户运行时根目录。
pub fn sync_bundled_luaskills_runtime(runtime_root: &Path) -> Result<(), String> {
    let bundle_root = if let Some(output_root) = resolve_output_root() {
        output_root
    } else if let Some(source_runtime_root) = resolve_source_luaskills_runtime_root() {
        source_runtime_root
    } else {
        return Ok(());
    };

    sync_directory_if_exists(
        &bundle_root.join(LUA_PACKAGES_DIR_NAME),
        &runtime_root.join(LUA_PACKAGES_DIR_NAME),
    )?;
    sync_directory_if_exists(
        &bundle_root.join(LIBS_DIR_NAME),
        &runtime_root.join(LIBS_DIR_NAME),
    )?;
    sync_directory_if_exists(
        &bundle_root.join(RESOURCES_DIR_NAME),
        &runtime_root.join(RESOURCES_DIR_NAME),
    )?;
    sync_directory_if_exists(
        &bundle_root.join(LICENSES_DIR_NAME),
        &runtime_root.join(LICENSES_DIR_NAME),
    )?;
    sync_directory_if_exists(
        &bundle_root.join(TOOLS_DIR_NAME),
        &runtime_root.join(TOOLS_DIR_NAME),
    )?;

    let packaged_skills_root = bundle_root.join(SKILLS_DIR_NAME);
    if packaged_skills_root.is_dir() {
        let packaged_root_skills = packaged_skills_root.join("root");
        let packaged_user_skills = packaged_skills_root.join("user");

        if packaged_root_skills.is_dir() || packaged_user_skills.is_dir() {
            sync_directory_if_exists(
                &packaged_root_skills,
                &runtime_root.join(SKILLS_DIR_NAME).join("root"),
            )?;
            sync_directory_if_exists(
                &packaged_user_skills,
                &runtime_root.join(SKILLS_DIR_NAME).join("user"),
            )?;
        } else {
            sync_directory_if_exists(
                &packaged_skills_root,
                &runtime_root.join(SKILLS_DIR_NAME).join("root"),
            )?;
        }
    }

    Ok(())
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

/// Returns the source-managed LuaSkills runtime package root when local development dependencies were bootstrapped.
/// 当本地开发依赖已经初始化时，返回源码托管的 LuaSkills 运行时包根目录。
fn resolve_source_luaskills_runtime_root() -> Option<PathBuf> {
    let runtime_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("third_party")
        .join("luaskills_runtime");

    runtime_root.is_dir().then_some(runtime_root)
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

/// Synchronizes one source directory into one destination directory when the source exists.
/// 当来源目录存在时，将其同步到目标目录。
fn sync_directory_if_exists(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }

    if !source.is_dir() {
        return Err(format!("运行时资产目录不是有效目录：{}", source.display()));
    }

    reset_directory_contents(destination).map_err(|error| {
        format!(
            "清理运行时资产目录失败（{}）：{error}",
            destination.display()
        )
    })?;

    for entry in fs::read_dir(source)
        .map_err(|error| format!("读取运行时资产目录失败（{}）：{error}", source.display()))?
    {
        let entry = entry
            .map_err(|error| format!("遍历运行时资产目录失败（{}）：{error}", source.display()))?;
        copy_path_recursive(&entry.path(), &destination.join(entry.file_name()))?;
    }

    Ok(())
}

/// Resets one directory so the next synchronization starts from a clean state.
/// 重置某个目录，使下一次同步从干净状态开始。
fn reset_directory_contents(path: &Path) -> Result<(), std::io::Error> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }

    fs::create_dir_all(path)
}

/// Copies one path recursively while preserving the original nested structure.
/// 递归复制单个路径，并保持原始嵌套结构。
fn copy_path_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    if source.is_dir() {
        fs::create_dir_all(destination)
            .map_err(|error| format!("创建运行时目录失败（{}）：{error}", destination.display()))?;

        for entry in fs::read_dir(source)
            .map_err(|error| format!("读取运行时目录失败（{}）：{error}", source.display()))?
        {
            let entry = entry
                .map_err(|error| format!("遍历运行时目录失败（{}）：{error}", source.display()))?;
            copy_path_recursive(&entry.path(), &destination.join(entry.file_name()))?;
        }

        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建运行时目录失败（{}）：{error}", parent.display()))?;
    }

    fs::copy(source, destination).map_err(|error| {
        format!(
            "复制运行时资产失败（{} -> {}）：{error}",
            source.display(),
            destination.display()
        )
    })?;

    Ok(())
}
