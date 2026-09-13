//! Read-only access to generated Rust dependency license artifacts.
//! 只读访问生成的 Rust 依赖许可证制品。

use std::{collections::HashSet, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A dependency as emitted by the checked-in cargo-about template.
/// 签入的 cargo-about 模板输出的一项依赖。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LicenseDependency {
    /// Exact Cargo package name.
    /// 精确 Cargo 包名称。
    name: String,
    /// Exact resolved version.
    /// 精确解析版本。
    version: String,
    /// Repository URL or the template's explicit crates.io source URL.
    /// 仓库 URL 或模板显式提供的 crates.io 来源 URL。
    source: String,
}

/// One independently readable copyright/license body and its actual dependents.
/// 一份可独立读取的版权或许可证正文及其实际依赖项。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LicenseSection {
    /// Index-owned identifier; never a filesystem path.
    /// 由索引持有的标识，绝不是文件系统路径。
    id: String,
    /// Dependencies covered by this exact body variant.
    /// 此精确正文变体覆盖的依赖。
    dependencies: Vec<LicenseDependency>,
    /// Inclusive one-based first body line.
    /// 正文首行，从一开始且包含此行。
    start_line: usize,
    /// Inclusive one-based final body line.
    /// 正文末行，从一开始且包含此行。
    end_line: usize,
}

/// Equal declared license names grouped without discarding body variants.
/// 按相同声明名称分组且不丢弃正文变体的许可证。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LicenseGroup {
    /// Stable identifier within this catalog revision.
    /// 此目录版本内的稳定标识。
    id: String,
    /// Declared title without inferred SPDX conversion.
    /// 声明标题，不推测转换为 SPDX。
    title: String,
    /// Declared or explicitly unknown title state.
    /// 已声明或明确未知的标题状态。
    status: String,
    /// All original bodies belonging to this declared type.
    /// 此声明类型对应的全部原始正文。
    sections: Vec<LicenseSection>,
}

/// Versioned lightweight catalog returned to both browser and desktop clients.
/// 返回浏览器与桌面客户端的带版本轻量目录。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LicenseCatalog {
    /// Exact supported index schema version.
    /// 精确支持的索引结构版本。
    schema_version: u32,
    /// Build-time dependency graph scope.
    /// 构建期依赖图范围。
    scope: String,
    /// Fixed artifact filename, validated before reading.
    /// 在读取前验证的固定制品文件名。
    source_file: String,
    /// SHA256 of UTF-8 Markdown with normalized LF newlines.
    /// 使用 LF 规范换行的 UTF-8 Markdown 的 SHA256。
    source_sha256: String,
    /// Declared license groups.
    /// 声明许可证分组。
    licenses: Vec<LicenseGroup>,
}

/// Exact selected body response tied to the catalog revision.
/// 绑定目录版本的精确选中正文响应。
#[derive(Debug, Serialize)]
pub(crate) struct LicenseBody {
    /// Original selected section identifier.
    /// 原始选中段落标识。
    section_id: String,
    /// Complete original license body as plain text.
    /// 以纯文本表示的完整原始许可证正文。
    text: String,
}

/// Reads a fixed artifact pair from licenses_root and validates identity, ranges and content revision.
/// 从 licenses_root 读取固定制品对，并验证身份、范围与内容版本。
/// Returns the verified catalog and normalized Markdown, or a visible error without rebuilding resources.
/// 返回已验证目录和规范 Markdown，或明确错误；不会重新构建资源。
fn read_license_artifacts(licenses_root: &Path) -> Result<(LicenseCatalog, String), String> {
    // catalog_bytes always comes from the fixed index filename, never a client-supplied path.
    // catalog_bytes 始终来自固定索引文件名，绝不使用客户端提供的路径。
    let catalog_bytes = std::fs::read(licenses_root.join("THIRD_PARTY_NOTICES_INDEX.json"))
        .map_err(|error| {
            format!(
                "License index unavailable; generate and synchronize license resources: {error}"
            )
        })?;
    let catalog: LicenseCatalog = serde_json::from_slice(&catalog_bytes).map_err(|error| {
        format!("Invalid license index; regenerate and synchronize resources: {error}")
    })?;
    if catalog.schema_version != 2
        || catalog.scope != "rust-desktop-dependencies"
        || catalog.source_file != "THIRD_PARTY_NOTICES.md"
    {
        return Err(
            "Unsupported license index contract; regenerate and synchronize resources".into(),
        );
    }
    // markdown normalizes only newlines, matching the generator's documented hash contract.
    // markdown 仅规范换行，与生成器声明的哈希合同一致。
    let markdown = std::fs::read_to_string(licenses_root.join("THIRD_PARTY_NOTICES.md"))
        .map_err(|error| {
            format!("License text unavailable; synchronize license resources: {error}")
        })?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let digest = hex::encode(Sha256::digest(markdown.as_bytes()));
    if digest != catalog.source_sha256 {
        return Err(
            "License index and text revisions differ; regenerate and synchronize resources".into(),
        );
    }
    // identifiers and line bounds reject ambiguous IDs and stale/invalid selections before projection.
    // identifiers 与行边界在投影前拒绝身份歧义以及陈旧或无效选择。
    let mut identifiers = HashSet::new();
    let lines: Vec<&str> = markdown.lines().collect();
    if catalog.licenses.is_empty() {
        return Err("License index contains no groups".into());
    }
    for group in &catalog.licenses {
        if group.id.is_empty()
            || !identifiers.insert(group.id.as_str())
            || group.sections.is_empty()
            || !matches!(group.status.as_str(), "declared" | "unknown")
            || (group.title.trim().is_empty() != (group.status == "unknown"))
        {
            return Err("Invalid license group identity or declaration state".into());
        }
        for section in &group.sections {
            if section.id.is_empty()
                || !identifiers.insert(section.id.as_str())
                || section.start_line < 2
                || section.end_line < section.start_line
                || section.end_line >= lines.len()
                || lines[section.start_line - 2].trim() != "```text"
                || lines[section.end_line].trim() != "```"
            {
                return Err("Invalid license body identity or line range".into());
            }
            if section.dependencies.iter().any(|dependency| {
                dependency.name.is_empty()
                    || dependency.version.is_empty()
                    || dependency.source.is_empty()
            }) {
                return Err("Incomplete dependency identity in license index".into());
            }
        }
    }
    Ok((catalog, markdown))
}

/// Loads the lightweight catalog from the effective packaged resource root; takes no mutable state.
/// 从生效的打包资源目录加载轻量目录；不接收可变状态。
/// Returns verified metadata only; license bodies remain absent from the wire response.
/// 仅返回已验证元数据；线协议响应不携带许可证正文。
pub(crate) async fn service_license_catalog() -> Result<LicenseCatalog, String> {
    tokio::task::spawn_blocking(|| {
        read_license_artifacts(
            &vulcan_runtime_assets::resolve_app_resources_directory().join("licenses"),
        )
        .map(|(catalog, _)| catalog)
    })
    .await
    .map_err(|error| format!("License catalog reader failed: {error}"))?
}

/// Resolves section_id only inside the verified index and rejects a stale expected_sha256 revision.
/// 仅在已验证索引内解析 section_id，并拒绝陈旧的 expected_sha256 版本。
/// Returns one complete body, with no path traversal or runtime generation capability.
/// 返回一份完整正文，不提供路径遍历或运行时生成能力。
pub(crate) async fn service_license_body(
    section_id: String,
    expected_sha256: String,
) -> Result<LicenseBody, String> {
    tokio::task::spawn_blocking(move || {
        let (catalog, markdown) = read_license_artifacts(
            &vulcan_runtime_assets::resolve_app_resources_directory().join("licenses"),
        )?;
        select_license_body(&catalog, &markdown, &section_id, &expected_sha256)
    })
    .await
    .map_err(|error| format!("License body reader failed: {error}"))?
}

/// Selects an exact indexed section and expected revision from already validated artifacts.
/// 从已验证制品中选择精确索引段及预期版本。
/// Returns the entire selected text or an explicit unknown/stale selection error.
/// 返回完整选中文本，或明确的未知选择或陈旧版本错误。
fn select_license_body(
    catalog: &LicenseCatalog,
    markdown: &str,
    section_id: &str,
    expected_sha256: &str,
) -> Result<LicenseBody, String> {
    if catalog.source_sha256 != expected_sha256 {
        return Err("License catalog changed; reload the catalog before reading a body".into());
    }
    let section = catalog
        .licenses
        .iter()
        .flat_map(|group| &group.sections)
        .find(|section| section.id == section_id)
        .ok_or_else(|| "Unknown license section".to_string())?;
    Ok(LicenseBody {
        section_id: section.id.clone(),
        text: markdown
            .lines()
            .skip(section.start_line - 1)
            .take(section.end_line - section.start_line + 1)
            .collect::<Vec<_>>()
            .join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every checked-in variant is addressable and unknown IDs/stale revisions are rejected.
    /// 验证每份签入变体均可读取，并拒绝未知标识和陈旧版本。
    #[test]
    fn source_catalog_preserves_all_bodies_and_validates_selection() {
        let (catalog, markdown) = read_license_artifacts(
            &vulcan_runtime_assets::source_resource_directory().join("licenses"),
        )
        .expect("checked-in license pair");
        assert_eq!(
            catalog
                .licenses
                .iter()
                .map(|group| group.sections.len())
                .sum::<usize>(),
            markdown
                .lines()
                .filter(|line| line.starts_with("### "))
                .count()
        );
        for section in catalog.licenses.iter().flat_map(|group| &group.sections) {
            assert!(
                !select_license_body(&catalog, &markdown, &section.id, &catalog.source_sha256)
                    .expect("indexed body")
                    .text
                    .is_empty()
            );
        }
        assert!(
            select_license_body(&catalog, &markdown, "../Cargo.toml", &catalog.source_sha256)
                .is_err()
        );
        assert!(
            select_license_body(
                &catalog,
                &markdown,
                &catalog.licenses[0].sections[0].id,
                "stale"
            )
            .is_err()
        );
    }

    /// Rejects missing, mismatched and out-of-bounds artifacts using an isolated temporary directory.
    /// 使用隔离临时目录拒绝缺失、内容不匹配和范围越界的制品。
    #[test]
    fn corrupt_artifacts_are_explicit_errors() {
        let temporary = tempfile::tempdir().expect("temporary fixture");
        assert!(read_license_artifacts(temporary.path()).is_err());
        let (mut catalog, markdown) = read_license_artifacts(
            &vulcan_runtime_assets::source_resource_directory().join("licenses"),
        )
        .expect("source fixtures");
        std::fs::write(temporary.path().join("THIRD_PARTY_NOTICES.md"), &markdown)
            .expect("write body");
        catalog.licenses[0].sections[0].end_line = usize::MAX;
        std::fs::write(
            temporary.path().join("THIRD_PARTY_NOTICES_INDEX.json"),
            serde_json::to_vec(&catalog).expect("encode"),
        )
        .expect("write index");
        assert!(read_license_artifacts(temporary.path()).is_err());
        catalog.source_sha256 = "mismatch".into();
        std::fs::write(
            temporary.path().join("THIRD_PARTY_NOTICES_INDEX.json"),
            serde_json::to_vec(&catalog).expect("encode"),
        )
        .expect("write index");
        assert!(read_license_artifacts(temporary.path()).is_err());
    }
}
