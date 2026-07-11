//! Portable subagent catalog path and identifier rules.
//! 可移植的子代理目录路径与标识规则。

use super::SubagentCatalogLoadError;
use std::path::{Component, Path, PathBuf};

/// File extension used by user-facing subagent definition files.
/// 面向用户的子代理定义文件使用的扩展名。
pub(super) const SUBAGENT_DEFINITION_EXTENSION: &str = "md";

/// YAML sidecar extension.
/// YAML sidecar 扩展名。
pub(super) const YAML_EXTENSION: &str = "yaml";

/// Short YAML sidecar extension.
/// YAML sidecar 短扩展名。
pub(super) const YML_EXTENSION: &str = "yml";

/// JSON sidecar extension.
/// JSON sidecar 扩展名。
pub(super) const JSON_EXTENSION: &str = "json";

/// Returns the top-level display category id for one canonical agent id.
/// 返回规范智能体标识对应的顶层展示分类标识。
pub fn display_subagent_category_id(id: &str, package_id: Option<&str>) -> Option<String> {
    // Holds the non-empty package prefix when the definition belongs to a package.
    // 当定义属于包时持有非空包前缀。
    let package_id = package_id.map(str::trim).filter(|value| !value.is_empty());
    // Removes the package prefix before resolving the first category segment.
    // 在解析首个分类片段前移除包前缀。
    let relative_id = package_id
        .and_then(|package_id| id.strip_prefix(&format!("{package_id}/")))
        .unwrap_or(id);
    // Splits the canonical id into its top-level category and remaining path.
    // 将规范标识拆分为顶层分类和剩余路径。
    let (top_level_category, _) = relative_id.split_once('/')?;
    // Trims the category for defensive handling of external public callers.
    // 裁剪分类文本以防御外部公开调用方。
    let top_level_category = top_level_category.trim();
    if top_level_category.is_empty() {
        return None;
    }
    package_id
        .map(|package_id| format!("{package_id}/{top_level_category}"))
        .or_else(|| Some(top_level_category.to_string()))
}

/// Returns the canonical category id for a directory below one catalog root.
/// 返回目录根下某个目录对应的规范分类标识。
pub(super) fn category_id_for_directory(base_root: &Path, directory: &Path) -> Option<String> {
    relative_path_id(base_root, directory)
}

/// Returns one portable package id from a package directory name.
/// 从包目录名称返回可移植包标识。
pub(super) fn package_id_for_directory(directory: &Path) -> Option<String> {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| path_segment_is_portable(name))
        .map(str::to_string)
}

/// Returns whether a directory is exactly one portable segment below the catalog root.
/// 返回目录是否恰好位于目录根下一层且片段可移植。
pub(super) fn directory_is_top_level_category(base_root: &Path, directory: &Path) -> bool {
    // Resolves the directory relative to the declared catalog root.
    // 将目录解析为相对于声明目录根的路径。
    let Ok(relative) = directory.strip_prefix(base_root) else {
        return false;
    };
    // Counts validated normal components and rejects nested categories immediately.
    // 统计已验证的普通组件，并立即拒绝嵌套分类。
    let mut normal_count = 0usize;
    // Validates every path component without normalizing unsafe input.
    // 在不规范化不安全输入的前提下验证每个路径组件。
    for component in relative.components() {
        // Accepts only literal path segments below the root.
        // 仅接受目录根下的字面路径片段。
        let Component::Normal(value) = component else {
            return false;
        };
        // Converts the platform path segment into the canonical UTF-8 identifier form.
        // 将平台路径片段转换为规范 UTF-8 标识形式。
        let Some(segment) = value.to_str() else {
            return false;
        };
        if !path_segment_is_portable(segment) {
            return false;
        }
        normal_count += 1;
        if normal_count > 1 {
            return false;
        }
    }
    normal_count == 1
}

/// Returns the canonical extensionless id for a portable catalog file.
/// 返回可移植目录文件对应的无扩展名规范标识。
pub(super) fn canonical_id_for_file(base_root: &Path, file: &Path) -> Option<String> {
    // Resolves the file relative to the declared catalog root.
    // 将文件解析为相对于声明目录根的路径。
    let relative = file.strip_prefix(base_root).ok()?;
    // Removes exactly the catalog file extension before canonicalization.
    // 在规范化前仅移除目录文件扩展名。
    let without_extension = relative.with_extension("");
    // Builds the slash-separated canonical identifier.
    // 构建使用斜杠分隔的规范标识。
    let id = path_to_canonical_id(&without_extension)?;
    // Final agent ids are extensionless; dots remain valid only in parent categories.
    // 最终智能体标识不带扩展名；点号仅允许出现在父分类中。
    (!id.rsplit('/').next()?.contains('.')).then_some(id)
}

/// Returns the canonical id for a portable path below one catalog root.
/// 返回目录根下可移植路径对应的规范标识。
fn relative_path_id(base_root: &Path, path: &Path) -> Option<String> {
    // Resolves the path relative to the declared catalog root.
    // 将路径解析为相对于声明目录根的路径。
    let relative = path.strip_prefix(base_root).ok()?;
    path_to_canonical_id(relative)
}

/// Converts a relative portable path into one slash-separated canonical id.
/// 将相对可移植路径转换为使用斜杠分隔的规范标识。
fn path_to_canonical_id(path: &Path) -> Option<String> {
    // Collects validated path segments in source order.
    // 按源路径顺序收集已验证片段。
    let mut parts = Vec::new();
    // Rejects root, prefix, parent and current-directory components.
    // 拒绝根、前缀、父目录和当前目录组件。
    for component in path.components() {
        // Accepts only literal path segments.
        // 仅接受字面路径片段。
        let Component::Normal(value) = component else {
            return None;
        };
        // Converts the platform component into a UTF-8 identifier segment.
        // 将平台组件转换为 UTF-8 标识片段。
        let part = value.to_str()?;
        if !path_segment_is_portable(part) {
            return None;
        }
        parts.push(part.to_string());
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Builds a safe relative category directory path from one canonical id.
/// 根据规范标识构建安全的相对分类目录路径。
pub(super) fn safe_relative_directory_path(id: &str) -> Result<PathBuf, SubagentCatalogLoadError> {
    safe_relative_id_path(id, "category")
}

/// Builds a safe relative definition file path with the requested extension.
/// 使用指定扩展名构建安全的相对定义文件路径。
pub(super) fn safe_relative_agent_path(
    id: &str,
    extension: &str,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    // Builds the validated extensionless path shared with category handling.
    // 构建与分类处理共享的已验证无扩展名路径。
    let mut path = safe_relative_id_path(id, "definition")?;
    // Reads the final segment to enforce the extensionless agent-id contract.
    // 读取最终片段以执行智能体标识不带扩展名的契约。
    let final_segment = path
        .file_name()
        .and_then(|segment| segment.to_str())
        .ok_or_else(|| invalid_subagent_id(id, "definition"))?;
    if final_segment.contains('.') {
        return Err(invalid_subagent_id(id, "definition"));
    }
    path.set_extension(extension);
    Ok(path)
}

/// Returns one portable extensionless definition file stem.
/// 返回可移植且不带扩展名的定义文件 stem。
pub(super) fn definition_file_stem(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| path_segment_is_portable(stem) && !stem.contains('.'))
        .map(str::to_string)
}

/// Returns whether one path uses the Markdown definition extension.
/// 返回路径是否使用 Markdown 定义扩展名。
pub(super) fn path_has_markdown_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(SUBAGENT_DEFINITION_EXTENSION))
}

/// Returns whether one path uses a supported sidecar extension.
/// 返回路径是否使用受支持的 sidecar 扩展名。
pub(super) fn path_has_sidecar_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case(YAML_EXTENSION)
                || extension.eq_ignore_ascii_case(YML_EXTENSION)
                || extension.eq_ignore_ascii_case(JSON_EXTENSION)
        })
}

/// Returns whether one path uses the JSON sidecar extension.
/// 返回路径是否使用 JSON sidecar 扩展名。
pub(super) fn path_has_json_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(JSON_EXTENSION))
}

/// Builds a safe relative path from one canonical subagent identifier.
/// 根据规范子代理标识构建安全相对路径。
fn safe_relative_id_path(
    id: &str,
    identity_kind: &str,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    if id.is_empty() || id != id.trim() {
        return Err(invalid_subagent_id(id, identity_kind));
    }
    // Collects validated portable segments without trimming or rewriting identifiers.
    // 在不裁剪或改写标识的前提下收集已验证的可移植片段。
    let mut path = PathBuf::new();
    // Validates every slash-delimited identifier segment.
    // 验证每个使用斜杠分隔的标识片段。
    for part in id.split('/') {
        if !path_segment_is_portable(part) {
            return Err(invalid_subagent_id(id, identity_kind));
        }
        path.push(part);
    }
    Ok(path)
}

/// Returns whether one identifier segment is portable across supported desktop platforms.
/// 返回标识片段是否可在受支持桌面平台间移植。
fn path_segment_is_portable(segment: &str) -> bool {
    if segment.is_empty()
        || segment == "."
        || segment == ".."
        || segment.ends_with('.')
        || !segment.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return false;
    }
    !path_segment_is_windows_reserved(segment)
}

/// Returns whether one otherwise portable segment is a reserved Windows device name.
/// 返回原本可移植的片段是否为 Windows 保留设备名。
fn path_segment_is_windows_reserved(segment: &str) -> bool {
    // Uses the portion before the first dot because Windows reserves names with any extension.
    // 使用首个点号前的部分，因为 Windows 会保留带任意扩展名的设备名。
    let stem = segment.split('.').next().unwrap_or(segment);
    // Normalizes ASCII case for the case-insensitive Windows comparison.
    // 规范化 ASCII 大小写以执行 Windows 不区分大小写的比较。
    let uppercase = stem.to_ascii_uppercase();
    if matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    uppercase.len() == 4
        && (uppercase.starts_with("COM") || uppercase.starts_with("LPT"))
        && uppercase
            .as_bytes()
            .last()
            .is_some_and(|digit| matches!(digit, b'1'..=b'9'))
}

/// Builds one catalog load error for an invalid category or definition identifier.
/// 为无效分类或定义标识构建目录加载错误。
fn invalid_subagent_id(id: &str, identity_kind: &str) -> SubagentCatalogLoadError {
    SubagentCatalogLoadError {
        path: PathBuf::from(id),
        message: format!("Invalid subagent {identity_kind} id `{id}`."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies one portable nested definition id maps to the expected Markdown path.
    /// 验证可移植的嵌套定义标识会映射到预期 Markdown 路径。
    #[test]
    fn builds_portable_nested_definition_path() {
        // Captures the path produced from the canonical nested id.
        // 捕获根据规范嵌套标识生成的路径。
        let path = safe_relative_agent_path("engineering/code-reviewer", "md")
            .expect("portable id should produce a path");

        assert_eq!(path, PathBuf::from("engineering").join("code-reviewer.md"));
    }

    /// Verifies unsafe, non-portable and extension-bearing definition ids are rejected.
    /// 验证不安全、不可移植或携带扩展名的定义标识会被拒绝。
    #[test]
    fn rejects_non_portable_definition_ids() {
        // Exercises every invalid identifier class enforced by the catalog boundary.
        // 覆盖目录边界执行的每类无效标识。
        for invalid_id in [
            "",
            " reviewer",
            "reviewer ",
            "engineering//reviewer",
            "../reviewer",
            "engineering\\reviewer",
            "reviewer:name",
            "reviewer?",
            "reviewer.v2",
            "CON",
            "category/NUL",
        ] {
            assert!(
                safe_relative_agent_path(invalid_id, "md").is_err(),
                "expected `{invalid_id}` to be rejected"
            );
        }
    }

    /// Verifies category and file ids use the same portable canonicalization rules.
    /// 验证分类与文件标识使用相同的可移植规范化规则。
    #[test]
    fn canonicalizes_portable_category_and_file_paths() {
        // Represents the catalog root for relative path resolution.
        // 表示相对路径解析使用的目录根。
        let root = PathBuf::from("catalog-root");
        // Represents one portable definition file below a category.
        // 表示分类下的一个可移植定义文件。
        let definition_file = root.join("engineering").join("code-reviewer.md");

        assert_eq!(
            canonical_id_for_file(&root, &definition_file).as_deref(),
            Some("engineering/code-reviewer")
        );
        assert_eq!(
            safe_relative_directory_path("vendor.v1/engineering")
                .expect("portable category should produce a path"),
            PathBuf::from("vendor.v1").join("engineering")
        );
        assert_eq!(
            package_id_for_directory(Path::new("packages/vendor.v1")).as_deref(),
            Some("vendor.v1")
        );
        assert!(package_id_for_directory(Path::new("packages/CON")).is_none());
        assert!(directory_is_top_level_category(
            &root,
            &root.join("engineering")
        ));
        assert!(!directory_is_top_level_category(
            &root,
            &root.join("engineering").join("nested")
        ));
    }

    /// Verifies catalog extension matching remains case-insensitive.
    /// 验证目录扩展名匹配仍保持不区分大小写。
    #[test]
    fn recognizes_catalog_extensions_case_insensitively() {
        assert!(path_has_markdown_extension(Path::new("reviewer.MD")));
        assert!(path_has_sidecar_extension(Path::new("reviewer.YML")));
        assert!(path_has_sidecar_extension(Path::new("reviewer.JSON")));
        assert!(path_has_json_extension(Path::new("reviewer.JsOn")));
        assert!(!path_has_sidecar_extension(Path::new("reviewer.toml")));
    }
}
