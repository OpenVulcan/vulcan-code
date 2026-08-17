//! Build-time LuaSkills dependency version discovery.
//! 构建期 LuaSkills 依赖版本发现。

use semver::Version;
use serde_json::Value as JsonValue;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value;

/// Environment key exported to the service runtime crate.
/// 导出给服务运行时 crate 的环境变量键。
const LUASKILLS_VERSION_ENV_KEY: &str = "VULCAN_LUASKILLS_DEPENDENCY_VERSION";

/// Build-time managed runtime manifest bundled into the service runtime binary.
/// 编译进服务运行时二进制的构建期受管运行时清单。
const MANAGED_RUNTIME_MANIFEST_FILE: &str = "managed-runtimes.json";

/// Discover the exact workspace LuaSkills dependency version and export it to rustc.
/// 发现工作区 LuaSkills 依赖的精确版本并导出给 rustc。
fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is required"),
    );
    let workspace_manifest = manifest_dir.join("..").join("..").join("Cargo.toml");
    // managed_runtime_manifest is the single source for every exact downloaded runtime version.
    // managed_runtime_manifest 是所有精确下载运行时版本的唯一来源。
    let managed_runtime_manifest = manifest_dir.join(MANAGED_RUNTIME_MANIFEST_FILE);
    println!("cargo:rerun-if-changed={}", workspace_manifest.display());
    println!(
        "cargo:rerun-if-changed={}",
        managed_runtime_manifest.display()
    );
    let version = read_exact_luaskills_version(&workspace_manifest).unwrap_or_else(|error| {
        panic!("failed to resolve exact LuaSkills dependency version: {error}")
    });
    // managed_runtime_versions contains the validated immutable download matrix.
    // managed_runtime_versions 包含经过校验的不可变下载版本矩阵。
    let managed_runtime_versions = read_managed_runtime_versions(&managed_runtime_manifest)
        .unwrap_or_else(|error| panic!("failed to load managed runtime manifest: {error}"));
    if managed_runtime_versions.luaskills_version != version.to_string() {
        panic!(
            "managed runtime LuaSkills version {} does not match dependency {}",
            managed_runtime_versions.luaskills_version, version
        );
    }
    println!("cargo:rustc-env={LUASKILLS_VERSION_ENV_KEY}={version}");
    for (key, value) in [
        (
            "VULCAN_MANAGED_PYTHON_VERSION",
            managed_runtime_versions.python_version.as_str(),
        ),
        (
            "VULCAN_MANAGED_UV_VERSION",
            managed_runtime_versions.uv_version.as_str(),
        ),
        (
            "VULCAN_MANAGED_NODE_VERSION",
            managed_runtime_versions.node_version.as_str(),
        ),
        (
            "VULCAN_MANAGED_PNPM_VERSION",
            managed_runtime_versions.pnpm_version.as_str(),
        ),
        (
            "VULCAN_MANAGED_SUPPORTED_PLATFORMS",
            managed_runtime_versions.supported_platforms.as_str(),
        ),
    ] {
        println!("cargo:rustc-env={key}={value}");
    }
}

/// Exact immutable runtime versions parsed from the repository manifest.
/// 从仓库清单解析出的精确不可变运行时版本。
struct ManagedRuntimeVersions {
    /// LuaSkills version that owns the runtime layout contract.
    /// 持有运行时目录契约的 LuaSkills 版本。
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
    /// Comma-separated stable platform keys compiled into the host.
    /// 编译进宿主的逗号分隔稳定平台键。
    supported_platforms: String,
}

/// Read and validate every immutable managed runtime version.
/// 读取并校验每个不可变受管运行时版本。
///
/// `manifest_path` points at the crate-owned JSON source of truth.
/// `manifest_path` 指向 crate 持有的 JSON 唯一事实源。
///
/// Returns exact versions ready to export through rustc environment variables.
/// 返回可通过 rustc 环境变量导出的精确版本。
fn read_managed_runtime_versions(manifest_path: &Path) -> Result<ManagedRuntimeVersions, String> {
    // text is the exact UTF-8 JSON document stored by the runtime crate.
    // text 是运行时 crate 保存的精确 UTF-8 JSON 文档。
    let text = fs::read_to_string(manifest_path)
        .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
    // document provides structural validation without duplicating a build-only serde model.
    // document 提供结构校验，避免重复构建期专用 serde 模型。
    let document = serde_json::from_str::<JsonValue>(&text)
        .map_err(|error| format!("failed to parse {}: {error}", manifest_path.display()))?;
    if document.get("schema_version").and_then(JsonValue::as_u64) != Some(1) {
        return Err(String::from("managed runtime schema_version must be 1"));
    }
    // required_string reads one exact non-empty scalar from the manifest.
    // required_string 从清单读取一个精确非空标量。
    let required_string = |field: &str| -> Result<String, String> {
        document
            .get(field)
            .and_then(JsonValue::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("managed runtime field `{field}` must be a non-empty string"))
    };
    // supported_platforms preserves the explicit published platform matrix.
    // supported_platforms 保留显式发布平台矩阵。
    let supported_platforms = document
        .get("supported_platforms")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| String::from("managed runtime supported_platforms must be an array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|item| !item.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| String::from("managed runtime platform must be a non-empty string"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if supported_platforms.is_empty() {
        return Err(String::from(
            "managed runtime supported_platforms must not be empty",
        ));
    }
    Ok(ManagedRuntimeVersions {
        luaskills_version: required_string("luaskills_version")?,
        python_version: required_string("python_version")?,
        uv_version: required_string("uv_version")?,
        node_version: required_string("node_version")?,
        pnpm_version: required_string("pnpm_version")?,
        supported_platforms: supported_platforms.join(","),
    })
}

/// Read and validate the exact `workspace.dependencies.luaskills` version.
/// 读取并校验精确的 `workspace.dependencies.luaskills` 版本。
///
/// `manifest_path` points at the workspace Cargo manifest.
/// `manifest_path` 指向工作区 Cargo 清单。
///
/// Returns a normalized full semantic version including the patch component.
/// 返回包含补丁号的规范化完整语义版本。
fn read_exact_luaskills_version(manifest_path: &Path) -> Result<Version, String> {
    let text = fs::read_to_string(manifest_path)
        .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
    let document = toml::from_str::<Value>(&text)
        .map_err(|error| format!("failed to parse {}: {error}", manifest_path.display()))?;
    let dependency = document
        .get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.get("luaskills"))
        .ok_or_else(|| "workspace.dependencies.luaskills is missing".to_string())?;
    let requirement = match dependency {
        Value::String(value) => value.as_str(),
        Value::Table(table) => table
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| "workspace LuaSkills dependency has no version".to_string())?,
        _ => {
            return Err("workspace LuaSkills dependency uses an unsupported shape".to_string());
        }
    };
    let exact = requirement.strip_prefix('=').ok_or_else(|| {
        format!(
            "workspace LuaSkills version `{requirement}` must use an exact `=x.y.z` requirement"
        )
    })?;
    Version::parse(exact)
        .map_err(|error| format!("workspace LuaSkills version `{requirement}` is invalid: {error}"))
}
