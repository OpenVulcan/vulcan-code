//! Locates isolated native host executables shipped beside the service binary.
//! 定位与服务二进制一同发布的隔离原生宿主可执行文件。

use std::env;
use std::path::PathBuf;

/// Resolves one host executable whose Cargo package and binary share `stem`.
/// 解析一个 Cargo 包名与二进制名均为 `stem` 的宿主可执行文件。
///
/// Resolution is strict: an explicit environment path must exist; otherwise the binary
/// beside the current executable is preferred, followed by the matching Cargo target profile.
/// 解析过程是严格的：显式环境变量路径必须存在；否则优先使用当前可执行文件旁的二进制，
/// 最后检查匹配的 Cargo target 构建配置目录。
///
/// # Parameters / 参数
/// - `stem`: Binary name without the platform executable suffix.
/// - `override_variable`: Environment variable that names an explicit executable path.
/// - `stem`：不含平台可执行后缀的二进制名称。
/// - `override_variable`：指定显式可执行文件路径的环境变量。
///
/// # Returns / 返回值
/// Returns one existing executable path or a diagnostic listing every checked location.
/// 返回一个已存在的可执行文件路径，或列出所有检查位置的诊断信息。
pub fn resolve_native_host_binary(stem: &str, override_variable: &str) -> Result<PathBuf, String> {
    resolve_native_host_binary_in_cargo_target(stem, override_variable, "target")
}

/// Resolves one host executable built by a separate Cargo workspace with its own target directory.
/// 解析一个由独立 Cargo 工作区构建、拥有自有 target 目录的宿主可执行文件。
///
/// # Parameters / 参数
/// - `stem`, `override_variable`: Same as [`resolve_native_host_binary`].
/// - `cargo_target`: Target directory relative to the repository root, such as `target/code-mode-host`.
/// - `stem`、`override_variable`：与 [`resolve_native_host_binary`] 相同。
/// - `cargo_target`：相对仓库根目录的 target 目录，例如 `target/code-mode-host`。
///
/// # Returns / 返回值
/// Returns one existing executable path or a diagnostic listing every checked location.
/// 返回一个已存在的可执行文件路径，或列出所有检查位置的诊断信息。
pub fn resolve_native_host_binary_in_cargo_target(
    stem: &str,
    override_variable: &str,
    cargo_target: &str,
) -> Result<PathBuf, String> {
    if let Ok(explicit_path) = env::var(override_variable) {
        let explicit_path = PathBuf::from(explicit_path);
        if explicit_path.is_file() {
            return Ok(explicit_path);
        }
        return Err(format!(
            "{override_variable} points to `{}`, but that file does not exist.",
            explicit_path.display()
        ));
    }
    let current_exe = env::current_exe()
        .map_err(|error| format!("Failed to resolve the current executable path: {error}"))?;
    let host_name = format!("{stem}{}", env::consts::EXE_SUFFIX);
    let beside_current = current_exe.with_file_name(&host_name);
    if beside_current.is_file() {
        return Ok(beside_current);
    }
    // Cargo integration tests run from `<profile>/deps`; the host remains one directory above.
    // Cargo 集成测试从 `<profile>/deps` 运行，而宿主仍位于其上一级目录。
    let beside_cargo_test_profile = current_exe.parent().and_then(|current_parent| {
        (current_parent.file_name().and_then(|name| name.to_str()) == Some("deps"))
            .then(|| {
                current_parent
                    .parent()
                    .map(|profile| profile.join(&host_name))
            })
            .flatten()
    });
    if let Some(candidate) = beside_cargo_test_profile.as_ref()
        && candidate.is_file()
    {
        return Ok(candidate.clone());
    }
    let cargo_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(cargo_target)
        .join(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
        .join(&host_name);
    if cargo_target.is_file() {
        return Ok(cargo_target);
    }
    let cargo_test_profile_diagnostic = beside_cargo_test_profile
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| String::from("<not a Cargo deps executable>"));
    Err(format!(
        "Host `{host_name}` was not found beside `{}`, beside its Cargo test profile at `{cargo_test_profile_diagnostic}`, or at `{}`. Build it with `cargo build -p {stem}`.",
        current_exe.display(),
        cargo_target.display()
    ))
}
