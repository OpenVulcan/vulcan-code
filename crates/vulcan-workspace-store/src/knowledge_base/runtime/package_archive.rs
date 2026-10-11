//! Bounded portable knowledge archives and ordinary derived-artifact file operations.
//! 有界可移植知识库归档与普通派生产物文件操作。

use super::super::models::{
    KNOWLEDGE_PACKAGE_FORMAT, KNOWLEDGE_PACKAGE_VERSION, KnowledgeLibrarySummary,
    KnowledgePackageArtifact, KnowledgePackageExportResult, KnowledgePackageManifest,
    KnowledgeVectorIndexStatus, MAX_KNOWLEDGE_PACKAGE_BYTES,
};
use super::super::schema::KNOWLEDGE_SCHEMA_VERSION;
use super::{
    KNOWLEDGE_PACKAGE_INSPECTION_TEMP_PREFIX, KnowledgeManifest, MAX_KNOWLEDGE_PACKAGE_JSON_BYTES,
    metadata_is_link_or_reparse, persist_tempfile_atomically, unix_epoch_millis,
    validate_library_id, validate_library_name,
};
use crate::runtime_assets;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

/// Exact portable package metadata entry.
/// 可移植包元数据的精确条目名称。
const KNOWLEDGE_PACKAGE_METADATA_ENTRY: &str = "package.json";

/// Exact portable internal manifest entry.
/// 可移植包内部清单的精确条目名称。
const KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY: &str = "library/manifest.json";

/// Exact portable SQLite entry.
/// 可移植包 SQLite 的精确条目名称。
const KNOWLEDGE_PACKAGE_DATABASE_ENTRY: &str = "library/content.sqlite3";

/// Exact portable derived SQLite entry.
/// 可移植派生 SQLite 的精确条目名称。
const KNOWLEDGE_PACKAGE_DISPLAY_DATABASE_ENTRY: &str = "library/display.sqlite3";

/// Portable derived vector files share this safe archive prefix.
/// 可移植派生向量文件共享这个安全归档前缀。
const KNOWLEDGE_PACKAGE_VECTOR_PREFIX: &str = "library/lancedb/";

/// Validated package extracted into one host-controlled temporary directory.
/// 已解压到宿主受控临时目录中的已校验知识库包。
pub(super) struct PreparedKnowledgePackage {
    /// Temporary directory retaining extracted files for the operation lifetime.
    /// 在操作生命周期内保留解压文件的临时目录。
    temporary: tempfile::TempDir,
    /// Validated public package metadata.
    /// 已校验的公开包元数据。
    pub(super) package: KnowledgePackageManifest,
    /// Validated source internal manifest.
    /// 已校验的来源内部清单。
    pub(super) source_manifest: KnowledgeManifest,
    /// Optional extracted display SQLite database.
    /// 可选的已解压显示库 SQLite 数据库。
    display_database_path: Option<PathBuf>,
    /// Optional extracted LanceDB directory containing vector artifacts.
    /// 可选的已解压 LanceDB 向量派生目录。
    vector_root: Option<PathBuf>,
}

impl PreparedKnowledgePackage {
    /// Returns the exact extracted SQLite path.
    /// 返回精确的已解压 SQLite 路径。
    pub(super) fn database_path(&self) -> PathBuf {
        self.temporary.path().join("content.sqlite3")
    }

    /// Returns the optional extracted display SQLite path.
    /// 返回可选的已解压显示库 SQLite 路径。
    pub(super) fn display_database_path(&self) -> Option<PathBuf> {
        self.display_database_path.clone()
    }

    /// Returns the optional extracted LanceDB root.
    /// 返回可选的已解压 LanceDB 根目录。
    pub(super) fn vector_root(&self) -> Option<PathBuf> {
        self.vector_root.clone()
    }
}

/// Canonicalizes one existing ordinary destination directory.
/// 规范化一个现有普通目标目录。
///
/// # Parameters
/// 参数
/// `path` is the selected existing export destination.
/// `path` 是选中的现有导出目标目录。
///
/// # Returns
/// 返回值
/// The canonical ordinary directory, or an explicit metadata/path failure.
/// 返回规范化普通目录，或明确的元数据/路径错误。
pub(super) fn canonical_existing_directory(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Failed to read the target directory ({}): {error}",
            path.display()
        )
    })?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(format!(
            "The target path is not a trusted regular directory: {}",
            path.display()
        ));
    }
    fs::canonicalize(path).map_err(|error| {
        format!(
            "Failed to canonicalize the target directory ({}): {error}",
            path.display()
        )
    })
}

/// Canonicalizes and bounds one user-selected .vkb archive.
/// 规范化并限制一个用户选择的 .vkb 归档。
///
/// # Parameters
/// 参数
/// `path` is the user-selected archive whose file kind, extension and size must be checked.
/// `path` 是用户选择的归档，必须检查文件类型、扩展名及大小。
///
/// # Returns
/// 返回值
/// The canonical bounded archive path, or an explicit rejection.
/// 返回有界归档的规范路径，或明确拒绝原因。
pub(super) fn canonical_knowledge_package_path(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Failed to read the knowledge package ({}): {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(format!(
            "The knowledge package is not a trusted regular file: {}",
            path.display()
        ));
    }
    if metadata.len() > MAX_KNOWLEDGE_PACKAGE_BYTES {
        return Err(format!(
            "The knowledge package exceeds the size limit of {} bytes.",
            MAX_KNOWLEDGE_PACKAGE_BYTES
        ));
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("vkb"))
    {
        return Err(String::from(
            "The knowledge package extension must be .vkb.",
        ));
    }
    fs::canonicalize(path).map_err(|error| {
        format!(
            "Failed to canonicalize the knowledge package ({}): {error}",
            path.display()
        )
    })
}

/// Extracts and validates one package into a host-controlled temporary directory.
/// 将一个知识库包解压并校验到宿主受控临时目录。
///
/// # Parameters
/// 参数
/// `archive_path` identifies the selected archive; `temporary_parent` owns extraction directories.
/// `archive_path` 标识选中归档；`temporary_parent` 持有解压临时目录。
///
/// # Returns
/// 返回值
/// Validated archive metadata and retained temporary files; host-level SQLite checks remain with installation.
/// 返回已校验归档元数据及生命周期受控的临时文件；宿主级 SQLite 检查仍由安装负责。
pub(super) fn prepare_knowledge_package(
    archive_path: &Path,
    temporary_parent: &Path,
) -> Result<PreparedKnowledgePackage, String> {
    fs::create_dir_all(temporary_parent).map_err(|error| {
        format!(
            "Failed to create the parent of the knowledge temporary directory ({}): {error}",
            temporary_parent.display()
        )
    })?;
    let temporary = tempfile::Builder::new()
        .prefix(KNOWLEDGE_PACKAGE_INSPECTION_TEMP_PREFIX)
        .tempdir_in(temporary_parent)
        .map_err(|error| {
            format!(
                "Failed to create the knowledge package inspection temporary directory: {error}"
            )
        })?;
    let input = File::open(archive_path).map_err(|error| {
        format!(
            "Failed to open the knowledge package ({}): {error}",
            archive_path.display()
        )
    })?;
    let mut archive = ZipArchive::new(input)
        .map_err(|error| format!("Failed to parse the knowledge ZIP package: {error}"))?;
    if archive.len() < 3 {
        return Err(format!(
            "Invalid knowledge package entry count: {}; at least 3 entries are required.",
            archive.len()
        ));
    }
    if archive.len() > 10_000 {
        return Err(String::from(
            "The knowledge package entry count exceeds the safety limit.",
        ));
    }
    let mut package_bytes: Option<Vec<u8>> = None;
    let mut manifest_bytes: Option<Vec<u8>> = None;
    let database_path = temporary.path().join("content.sqlite3");
    let display_database_path = temporary.path().join("display.sqlite3");
    let vector_root = temporary.path().join("lancedb");
    let mut extracted_artifacts = BTreeMap::<String, PathBuf>::new();
    let mut extracted_bytes = 0_u64;
    let mut database_seen = false;
    let mut seen = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Failed to read knowledge package entry {index}: {error}"))?;
        let name = entry.name().to_string();
        if entry.is_dir() || entry.enclosed_name().is_none() {
            return Err(format!(
                "The knowledge package contains an invalid entry: {name}"
            ));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!(
                "The knowledge package does not allow symbolic link entries: {name}"
            ));
        }
        if !seen.insert(name.clone()) {
            return Err(format!(
                "The knowledge package contains a duplicate entry: {name}"
            ));
        }
        match name.as_str() {
            KNOWLEDGE_PACKAGE_METADATA_ENTRY => {
                package_bytes = Some(read_bounded_zip_entry(
                    &mut entry,
                    MAX_KNOWLEDGE_PACKAGE_JSON_BYTES,
                    KNOWLEDGE_PACKAGE_METADATA_ENTRY,
                )?);
            }
            KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY => {
                manifest_bytes = Some(read_bounded_zip_entry(
                    &mut entry,
                    MAX_KNOWLEDGE_PACKAGE_JSON_BYTES,
                    KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY,
                )?);
            }
            KNOWLEDGE_PACKAGE_DATABASE_ENTRY => {
                let expected_bytes = entry.size();
                extracted_bytes = extracted_bytes.saturating_add(extract_bounded_zip_file(
                    &mut entry,
                    &database_path,
                    MAX_KNOWLEDGE_PACKAGE_BYTES,
                    expected_bytes,
                    KNOWLEDGE_PACKAGE_DATABASE_ENTRY,
                )?);
                if extracted_bytes > MAX_KNOWLEDGE_PACKAGE_BYTES {
                    return Err(String::from(
                        "The knowledge package's total extracted size exceeds the safety limit.",
                    ));
                }
                database_seen = true;
            }
            KNOWLEDGE_PACKAGE_DISPLAY_DATABASE_ENTRY => {
                let expected_bytes = entry.size();
                extracted_bytes = extracted_bytes.saturating_add(extract_bounded_zip_file(
                    &mut entry,
                    &display_database_path,
                    MAX_KNOWLEDGE_PACKAGE_BYTES,
                    expected_bytes,
                    KNOWLEDGE_PACKAGE_DISPLAY_DATABASE_ENTRY,
                )?);
                if extracted_bytes > MAX_KNOWLEDGE_PACKAGE_BYTES {
                    return Err(String::from(
                        "The knowledge package's total extracted size exceeds the safety limit.",
                    ));
                }
                extracted_artifacts.insert(
                    String::from(KNOWLEDGE_PACKAGE_DISPLAY_DATABASE_ENTRY),
                    display_database_path.clone(),
                );
            }
            _ if name.starts_with(KNOWLEDGE_PACKAGE_VECTOR_PREFIX) => {
                let relative = safe_vector_archive_relative_path(&name)?;
                let target = vector_root.join(&relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!(
                            "Failed to create the knowledge vector temporary directory: {error}"
                        )
                    })?;
                }
                let expected_bytes = entry.size();
                extracted_bytes = extracted_bytes.saturating_add(extract_bounded_zip_file(
                    &mut entry,
                    &target,
                    MAX_KNOWLEDGE_PACKAGE_BYTES,
                    expected_bytes,
                    &name,
                )?);
                if extracted_bytes > MAX_KNOWLEDGE_PACKAGE_BYTES {
                    return Err(String::from(
                        "The knowledge package's total extracted size exceeds the safety limit.",
                    ));
                }
                extracted_artifacts.insert(name, target);
            }
            _ => {
                return Err(format!(
                    "The knowledge package contains an unknown entry: {name}"
                ));
            }
        }
    }
    let package: KnowledgePackageManifest = serde_json::from_slice(
        &package_bytes
            .ok_or_else(|| String::from("The knowledge package is missing package.json."))?,
    )
    .map_err(|error| format!("Failed to parse the knowledge package.json: {error}"))?;
    let source_manifest: KnowledgeManifest =
        serde_json::from_slice(&manifest_bytes.ok_or_else(|| {
            String::from("The knowledge package is missing the internal manifest.json.")
        })?)
        .map_err(|error| {
            format!("Failed to parse the knowledge internal manifest.json: {error}")
        })?;
    if !database_seen {
        return Err(String::from(
            "The knowledge package is missing content.sqlite3.",
        ));
    }
    validate_knowledge_package_metadata(
        &package,
        &source_manifest,
        &database_path,
        &extracted_artifacts,
    )?;
    Ok(PreparedKnowledgePackage {
        temporary,
        package,
        source_manifest,
        display_database_path: extracted_artifacts
            .get(KNOWLEDGE_PACKAGE_DISPLAY_DATABASE_ENTRY)
            .cloned(),
        vector_root: extracted_artifacts
            .values()
            .any(|path| path.starts_with(&vector_root))
            .then_some(vector_root),
    })
}

/// Extracts one ordinary ZIP file with both metadata and expansion bounds.
/// 在元数据与解压膨胀双重限制下解压一个普通 ZIP 文件。
///
/// # Parameters
/// 参数
/// `entry` is the input stream; `target` is the owned output; `limit` bounds expansion; `expected_bytes` is declared size; `name` identifies errors.
/// `entry` 是输入流；`target` 是受控输出；`limit` 限制膨胀；`expected_bytes` 是声明大小；`name` 标识错误。
///
/// # Returns
/// 返回值
/// The exact synchronized byte count, or a size, input or output failure.
/// 返回精确且已同步的字节数，或大小、输入、输出错误。
fn extract_bounded_zip_file<R: Read>(
    entry: &mut R,
    target: &Path,
    limit: u64,
    expected_bytes: u64,
    name: &str,
) -> Result<u64, String> {
    if expected_bytes > limit {
        return Err(format!(
            "A knowledge package entry exceeds the size limit: {name}"
        ));
    }
    let mut output = File::create(target).map_err(|error| {
        format!(
            "Failed to create a knowledge package temporary entry ({}): {error}",
            target.display()
        )
    })?;
    let copied =
        std::io::copy(&mut entry.by_ref().take(limit + 1), &mut output).map_err(|error| {
            format!("Failed to extract the knowledge package entry ({name}): {error}")
        })?;
    if copied > limit {
        return Err(format!(
            "The extracted content of a knowledge package entry exceeds the size limit: {name}"
        ));
    }
    output.sync_all().map_err(|error| {
        format!("Failed to sync the knowledge temporary entry ({name}): {error}")
    })?;
    if copied != expected_bytes {
        return Err(format!(
            "The extracted byte count of a knowledge package entry is inconsistent: {name}"
        ));
    }
    Ok(copied)
}

/// Validates one LanceDB ZIP path and returns its relative ordinary path.
/// 校验一个 LanceDB ZIP 路径并返回其相对普通路径。
///
/// # Parameters
/// 参数
/// `name` is an archive entry under the exact LanceDB prefix.
/// `name` 是位于精确 LanceDB 前缀下的归档条目。
///
/// # Returns
/// 返回值
/// A nonempty ordinary relative path, or a rejected prefix, character or component.
/// 返回非空普通相对路径，或前缀、字符、组件拒绝错误。
fn safe_vector_archive_relative_path(name: &str) -> Result<PathBuf, String> {
    let relative = name
        .strip_prefix(KNOWLEDGE_PACKAGE_VECTOR_PREFIX)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Invalid knowledge vector entry path: {name}"))?;
    if relative.contains('\\') || relative.contains(':') {
        return Err(format!(
            "A knowledge vector entry contains invalid path characters: {name}"
        ));
    }
    let path = PathBuf::from(relative);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "A knowledge vector entry contains an invalid path component: {name}"
        ));
    }
    Ok(path)
}

/// Reads one bounded JSON entry without accepting decompression expansion beyond its limit.
/// 读取一个有界 JSON 条目，不接受超过上限的解压膨胀。
///
/// # Parameters
/// 参数
/// `entry` supplies bytes; `limit` bounds decoded length; `name` identifies the entry in errors.
/// `entry` 提供字节；`limit` 限制解压后长度；`name` 在错误中标识条目。
///
/// # Returns
/// 返回值
/// Complete bounded bytes, or an I/O or expansion-limit failure.
/// 返回完整有界字节，或 I/O、解压上限错误。
fn read_bounded_zip_entry<R: Read>(
    entry: &mut R,
    limit: u64,
    name: &str,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read knowledge package entry {name}: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "A knowledge package entry exceeds the size limit: {name}"
        ));
    }
    Ok(bytes)
}

/// Validates portable metadata, source identity, size, and content digest.
/// 校验可移植元数据、来源身份、大小和内容摘要。
///
/// # Parameters
/// 参数
/// `package` and `source_manifest` declare provenance; `database_path` and `extracted_artifacts` identify extracted files.
/// `package` 与 `source_manifest` 声明来源；`database_path` 与 `extracted_artifacts` 标识已解压文件。
///
/// # Returns
/// 返回值
/// Success only for matching outer metadata, exact artifact membership, sizes and digests; database contents are checked by the caller's installation lifecycle.
/// 仅在外层元数据、精确派生成员、大小与摘要匹配时成功；数据库内容由调用方安装生命周期检查。
fn validate_knowledge_package_metadata(
    package: &KnowledgePackageManifest,
    source_manifest: &KnowledgeManifest,
    database_path: &Path,
    extracted_artifacts: &BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    if package.format != KNOWLEDGE_PACKAGE_FORMAT
        || !matches!(package.package_version, 1 | KNOWLEDGE_PACKAGE_VERSION)
    {
        return Err(String::from(
            "The knowledge package format or package version is not supported.",
        ));
    }
    validate_library_id(&package.source_library_id)?;
    if validate_library_name(&package.name)? != package.name {
        return Err(String::from(
            "The knowledge package name is not normalized.",
        ));
    }
    if package.schema_version != KNOWLEDGE_SCHEMA_VERSION
        || source_manifest.schema_version != KNOWLEDGE_SCHEMA_VERSION
    {
        return Err(format!(
            "Incompatible knowledge package schema version: package={}, manifest={}, current={}",
            package.schema_version, source_manifest.schema_version, KNOWLEDGE_SCHEMA_VERSION
        ));
    }
    if source_manifest.library_id != package.source_library_id
        || source_manifest.name != package.name
    {
        return Err(String::from(
            "The knowledge package source manifest disagrees with package.json.",
        ));
    }
    if package.index_status != "ready" {
        return Err(String::from(
            "The knowledge package has no directly usable index in the ready state.",
        ));
    }
    let mut settings = package.settings.clone();
    settings.normalize_derived_compatibility();
    settings.validate()?;
    if package.package_version == 1
        && !matches!(
            settings.retrieval_mode,
            super::models::KnowledgeRetrievalMode::RawBm25
        )
    {
        return Err(String::from(
            "The legacy knowledge package has no verifiable display-library entity and can only be imported in raw BM25 state.",
        ));
    }
    if package.content_sha256.len() != 64
        || !package
            .content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(String::from(
            "The knowledge package SQLite SHA-256 field has an invalid format.",
        ));
    }
    let content_bytes = fs::metadata(database_path)
        .map_err(|error| format!("Failed to read the knowledge package SQLite metadata: {error}"))?
        .len();
    if content_bytes != package.content_bytes {
        return Err(String::from(
            "The knowledge package SQLite size disagrees with package.json.",
        ));
    }
    let digest = sha256_file_hex(database_path)?;
    if digest != package.content_sha256.to_ascii_lowercase() {
        return Err(String::from(
            "The knowledge package SQLite SHA-256 check failed.",
        ));
    }
    let declared_artifacts = package
        .derived_artifacts
        .iter()
        .map(|artifact| (artifact.archive_path.as_str(), artifact))
        .collect::<BTreeMap<_, _>>();
    if declared_artifacts.len() != package.derived_artifacts.len() {
        return Err(String::from(
            "The knowledge package derived-file manifest contains duplicate paths.",
        ));
    }
    let extracted_names = extracted_artifacts
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let declared_names = declared_artifacts.keys().copied().collect::<BTreeSet<_>>();
    if extracted_names != declared_names {
        return Err(String::from(
            "The knowledge package derived-file manifest disagrees with the actual archive entries.",
        ));
    }
    for artifact in &package.derived_artifacts {
        let path = extracted_artifacts
            .get(&artifact.archive_path)
            .ok_or_else(|| {
                format!(
                    "The knowledge package is missing a derived file: {}",
                    artifact.archive_path
                )
            })?;
        if !matches!(
            artifact.archive_path.as_str(),
            KNOWLEDGE_PACKAGE_DISPLAY_DATABASE_ENTRY
        ) && !artifact
            .archive_path
            .starts_with(KNOWLEDGE_PACKAGE_VECTOR_PREFIX)
        {
            return Err(format!(
                "Unsupported knowledge package derived-file path: {}",
                artifact.archive_path
            ));
        }
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("Failed to read a knowledge derived file: {error}"))?;
        if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
            return Err(format!(
                "A knowledge derived file is not a trusted regular file: {}",
                path.display()
            ));
        }
        if metadata.len() != artifact.bytes {
            return Err(format!(
                "Knowledge derived file size check failed: {}",
                artifact.archive_path
            ));
        }
        if artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || sha256_file_hex(path)? != artifact.sha256.to_ascii_lowercase()
        {
            return Err(format!(
                "Knowledge derived file SHA-256 check failed: {}",
                artifact.archive_path
            ));
        }
    }
    if settings.display_index.status == super::models::KnowledgeDisplayIndexStatus::Ready
        && !extracted_artifacts.contains_key(KNOWLEDGE_PACKAGE_DISPLAY_DATABASE_ENTRY)
    {
        return Err(String::from(
            "The knowledge package declares the display library ready but is missing display.sqlite3.",
        ));
    }
    if settings.vector_index.status == KnowledgeVectorIndexStatus::Ready
        && !extracted_artifacts
            .keys()
            .any(|path| path.starts_with(KNOWLEDGE_PACKAGE_VECTOR_PREFIX))
    {
        return Err(String::from(
            "The knowledge package declares the vector index ready but is missing the LanceDB derived files.",
        ));
    }
    Ok(())
}

/// Writes one complete package after the owning SQLite binding has been stopped.
/// 在所属 SQLite 绑定停止后写出一个完整知识库包。
///
/// # Parameters
/// 参数
/// `library` is the verified registry view; `library_root` owns stopped databases; `destination_directory` is the canonical export directory.
/// `library` 是已验证注册视图；`library_root` 持有已停止数据库；`destination_directory` 是规范导出目录。
///
/// # Returns
/// 返回值
/// The atomically published archive and metadata, or a validation/write failure; binding restoration stays with the caller.
/// 返回原子发布的归档及元数据，或校验/写入错误；绑定恢复仍由调用方负责。
pub(super) fn export_knowledge_package_files(
    library: &KnowledgeLibrarySummary,
    library_root: &Path,
    destination_directory: &Path,
) -> Result<KnowledgePackageExportResult, String> {
    let database_path = library_root.join("content.sqlite3");
    let manifest_path = library_root.join("manifest.json");
    let database_metadata = fs::symlink_metadata(&database_path)
        .map_err(|error| format!("Failed to read the SQLite to export: {error}"))?;
    let manifest_metadata = fs::symlink_metadata(&manifest_path)
        .map_err(|error| format!("Failed to read the manifest to export: {error}"))?;
    if !database_metadata.is_file()
        || metadata_is_link_or_reparse(&database_metadata)
        || !manifest_metadata.is_file()
        || metadata_is_link_or_reparse(&manifest_metadata)
    {
        return Err(String::from(
            "The knowledge export source contains non-regular files.",
        ));
    }
    if database_metadata.len() > MAX_KNOWLEDGE_PACKAGE_BYTES {
        return Err(format!(
            "The knowledge SQLite exceeds the portable package size limit of {} bytes.",
            MAX_KNOWLEDGE_PACKAGE_BYTES
        ));
    }
    for transient_name in [
        "content.sqlite3-wal",
        "content.sqlite3-shm",
        "content.sqlite3-journal",
        "display.sqlite3-wal",
        "display.sqlite3-shm",
        "display.sqlite3-journal",
    ] {
        // transientPath must be absent after the owning SQLite binding is stopped.
        // transientPath 在所属 SQLite 绑定停止后必须不存在。
        let transient_path = library_root.join(transient_name);
        if transient_path.try_exists().map_err(|error| {
            format!("Failed to check the knowledge SQLite temporary journal: {error}")
        })? {
            return Err(format!(
                "The knowledge SQLite journal is not fully checkpointed; export refused: {}",
                transient_path.display()
            ));
        }
    }
    let source_manifest: KnowledgeManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| format!("Failed to read the manifest to export: {error}"))?,
    )
    .map_err(|error| format!("Failed to parse the manifest to export: {error}"))?;
    if source_manifest.library_id != library.id
        || source_manifest.name != library.name
        || source_manifest.schema_version != KNOWLEDGE_SCHEMA_VERSION
    {
        return Err(String::from(
            "The knowledge manifest to export disagrees with the registry record.",
        ));
    }
    let artifact_files = collect_export_artifact_files(library_root)?;
    let artifact_bytes = artifact_files.iter().try_fold(0_u64, |total, (_, path)| {
        let bytes = fs::metadata(path)
            .map_err(|error| {
                format!("Failed to read the knowledge derived-artifact size: {error}")
            })?
            .len();
        total.checked_add(bytes).ok_or_else(|| {
            String::from("The total knowledge derived-artifact size is out of the supported range.")
        })
    })?;
    if database_metadata
        .len()
        .checked_add(artifact_bytes)
        .is_none_or(|total| total > MAX_KNOWLEDGE_PACKAGE_BYTES)
    {
        return Err(String::from(
            "The uncompressed content of the portable knowledge package exceeds the size limit.",
        ));
    }
    let derived_artifacts = artifact_files
        .iter()
        .map(|(archive_path, path)| {
            let metadata = fs::metadata(path).map_err(|error| {
                format!("Failed to read the knowledge derived-artifact metadata: {error}")
            })?;
            Ok(KnowledgePackageArtifact {
                archive_path: archive_path.clone(),
                sha256: sha256_file_hex(path)?,
                bytes: metadata.len(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let exported_at_ms = unix_epoch_millis()?;
    let package = KnowledgePackageManifest {
        format: KNOWLEDGE_PACKAGE_FORMAT.to_string(),
        package_version: KNOWLEDGE_PACKAGE_VERSION,
        source_library_id: library.id.clone(),
        name: library.name.clone(),
        description: library.description.clone(),
        settings: library.settings.clone(),
        schema_version: KNOWLEDGE_SCHEMA_VERSION,
        document_count: library.document_count,
        chunk_count: library.chunk_count,
        index_status: library.index_status.clone(),
        exported_at_ms,
        content_sha256: sha256_file_hex(&database_path)?,
        content_bytes: database_metadata.len(),
        derived_artifacts,
    };
    // exportId prevents same-millisecond or sanitized-name collisions from overwriting another package.
    // exportId 防止同毫秒或名称清理冲突覆盖另一份知识库包。
    let export_id = Uuid::new_v4().to_string();
    let archive_name = portable_knowledge_archive_name(&library.name, exported_at_ms, &export_id);
    let archive_path = destination_directory.join(archive_name);
    let mut temporary = tempfile::Builder::new()
        .prefix(".knowledge-export-")
        .suffix(".vkb.tmp")
        .tempfile_in(destination_directory)
        .map_err(|error| {
            format!("Failed to create the knowledge export temporary file: {error}")
        })?;
    {
        let mut writer = ZipWriter::new(temporary.as_file_mut());
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        writer
            .start_file(KNOWLEDGE_PACKAGE_METADATA_ENTRY, options)
            .map_err(|error| {
                format!("Failed to create the knowledge package.json entry: {error}")
            })?;
        writer
            .write_all(&serde_json::to_vec_pretty(&package).map_err(|error| {
                format!("Failed to serialize the knowledge package metadata: {error}")
            })?)
            .map_err(|error| format!("Failed to write the knowledge package.json: {error}"))?;
        writer
            .start_file(KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY, options)
            .map_err(|error| {
                format!("Failed to create the knowledge internal manifest entry: {error}")
            })?;
        writer
            .write_all(
                &serde_json::to_vec_pretty(&source_manifest).map_err(|error| {
                    format!("Failed to serialize the knowledge internal manifest: {error}")
                })?,
            )
            .map_err(|error| format!("Failed to write the knowledge internal manifest: {error}"))?;
        writer
            .start_file(KNOWLEDGE_PACKAGE_DATABASE_ENTRY, options)
            .map_err(|error| format!("Failed to create the knowledge SQLite entry: {error}"))?;
        let mut database = File::open(&database_path)
            .map_err(|error| format!("Failed to open the SQLite to export: {error}"))?;
        std::io::copy(&mut database, &mut writer)
            .map_err(|error| format!("Failed to write the knowledge SQLite entry: {error}"))?;
        for (archive_path, source_path) in &artifact_files {
            writer.start_file(archive_path, options).map_err(|error| {
                format!("Failed to create the knowledge derived entry: {archive_path}: {error}")
            })?;
            let mut source = File::open(source_path)
                .map_err(|error| format!("Failed to open a knowledge derived artifact: {error}"))?;
            std::io::copy(&mut source, &mut writer).map_err(|error| {
                format!("Failed to write the knowledge derived entry: {archive_path}: {error}")
            })?;
        }
        writer
            .finish()
            .map_err(|error| format!("Failed to finish the knowledge ZIP package: {error}"))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Failed to sync the knowledge export temporary file: {error}"))?;
    // archiveBytes enforces that an exported package can pass the same size gate during import.
    // archiveBytes 确保导出包能够通过导入时相同的大小门禁。
    let archive_bytes = temporary
        .as_file()
        .metadata()
        .map_err(|error| format!("Failed to read the knowledge export package size: {error}"))?
        .len();
    if archive_bytes > MAX_KNOWLEDGE_PACKAGE_BYTES {
        return Err(format!(
            "The knowledge export package exceeds the size limit of {} bytes.",
            MAX_KNOWLEDGE_PACKAGE_BYTES
        ));
    }
    persist_tempfile_atomically(temporary, &archive_path)
        .map_err(|error| format!("Failed to commit the knowledge export file: {error}"))?;
    Ok(KnowledgePackageExportResult {
        archive_path: runtime_assets::native_host_path_text_from_path(&archive_path),
        package,
    })
}

/// Collects optional display and LanceDB files in deterministic archive order.
/// 按确定性归档顺序收集可选显示库与 LanceDB 文件。
///
/// # Parameters
/// 参数
/// `library_root` is the source library's host-owned directory.
/// `library_root` 是源知识库的宿主受控目录。
///
/// # Returns
/// 返回值
/// Deterministically ordered ordinary display/vector artifacts within the package bounds, or an explicit failure.
/// 返回满足包大小边界、顺序确定的普通显示/向量派生产物，或明确错误。
fn collect_export_artifact_files(library_root: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut artifacts = Vec::new();
    let display_path = library_root.join("display.sqlite3");
    match fs::symlink_metadata(&display_path) {
        Ok(metadata) if metadata.is_file() && !metadata_is_link_or_reparse(&metadata) => {
            if metadata.len() > MAX_KNOWLEDGE_PACKAGE_BYTES {
                return Err(String::from(
                    "The knowledge display-library SQLite exceeds the portable package size limit.",
                ));
            }
            artifacts.push((
                String::from(KNOWLEDGE_PACKAGE_DISPLAY_DATABASE_ENTRY),
                display_path,
            ));
        }
        Ok(_) => {
            return Err(String::from(
                "The knowledge display-library file is not a trusted regular file.",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Failed to read the knowledge display-library file: {error}"
            ));
        }
    }
    let vector_root = library_root.join("lancedb");
    match fs::symlink_metadata(&vector_root) {
        Ok(metadata) if metadata.is_dir() && !metadata_is_link_or_reparse(&metadata) => {
            let mut files = Vec::new();
            collect_ordinary_files_recursively(&vector_root, &mut files)?;
            for path in files {
                let relative = path.strip_prefix(&vector_root).map_err(|error| {
                    format!("Failed to compute the knowledge vector archive path: {error}")
                })?;
                let archive_path = format!(
                    "{}{}",
                    KNOWLEDGE_PACKAGE_VECTOR_PREFIX,
                    relative_to_zip_path(relative)?
                );
                let metadata = fs::metadata(&path).map_err(|error| {
                    format!("Failed to read the knowledge vector file metadata: {error}")
                })?;
                if metadata.len() > MAX_KNOWLEDGE_PACKAGE_BYTES {
                    return Err(format!(
                        "A knowledge vector file exceeds the size limit: {archive_path}"
                    ));
                }
                artifacts.push((archive_path, path));
            }
        }
        Ok(_) => {
            return Err(String::from(
                "The knowledge LanceDB root is not a trusted regular directory.",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Failed to read the knowledge LanceDB root: {error}"
            ));
        }
    }
    artifacts.sort_by(|left, right| left.0.cmp(&right.0));
    let total_bytes = artifacts.iter().try_fold(0_u64, |total, (_, path)| {
        let bytes = fs::metadata(path)
            .map_err(|error| {
                format!("Failed to read the knowledge derived-artifact size: {error}")
            })?
            .len();
        total.checked_add(bytes).ok_or_else(|| {
            String::from("The total knowledge derived-artifact size is out of the supported range.")
        })
    })?;
    if total_bytes > MAX_KNOWLEDGE_PACKAGE_BYTES {
        return Err(String::from(
            "The total knowledge derived-artifact size exceeds the portable package limit.",
        ));
    }
    Ok(artifacts)
}

/// Copies all extracted derived artifacts into one host-owned library directory.
/// 将所有已解压派生产物复制到一个宿主持有的资料库目录。
///
/// # Parameters
/// 参数
/// `prepared` owns validated extracted artifacts; `target_root` is the caller-created library directory.
/// `prepared` 持有已校验解压产物；`target_root` 是调用方创建的知识库目录。
///
/// # Returns
/// 返回值
/// Success after copying present derived artifacts without following links, or a filesystem failure.
/// 不跟随链接复制现有派生产物后成功，或返回文件系统错误。
pub(super) fn copy_prepared_derived_artifacts(
    prepared: &PreparedKnowledgePackage,
    target_root: &Path,
) -> Result<(), String> {
    if let Some(source) = prepared.display_database_path() {
        let metadata = fs::symlink_metadata(&source)
            .map_err(|error| format!("Failed to read the staged display library: {error}"))?;
        if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
            return Err(String::from(
                "The staged display library is not a trusted regular file.",
            ));
        }
        fs::copy(&source, target_root.join("display.sqlite3"))
            .map_err(|error| format!("Failed to copy the knowledge display library: {error}"))?;
    }
    if let Some(source_root) = prepared.vector_root() {
        let target_root = target_root.join("lancedb");
        copy_ordinary_directory_recursively(&source_root, &target_root)?;
    }
    Ok(())
}

/// Copies one ordinary directory tree without following links or reparse points.
/// 在不跟随链接或重解析点的前提下复制一个普通目录树。
///
/// # Parameters
/// 参数
/// `source` is the extracted ordinary directory; `target` receives its deterministic recursive copy.
/// `source` 是已解压普通目录；`target` 接收按确定顺序递归复制的内容。
///
/// # Returns
/// 返回值
/// Success for an ordinary tree; links, special entries and I/O failures are explicitly rejected.
/// 普通目录树复制成功；链接、特殊条目与 I/O 失败明确拒绝。
fn copy_ordinary_directory_recursively(source: &Path, target: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "Failed to read the staged derived directory ({}): {error}",
            source.display()
        )
    })?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(format!(
            "The staged derived directory is not a trusted regular directory: {}",
            source.display()
        ));
    }
    fs::create_dir_all(target).map_err(|error| {
        format!(
            "Failed to create the target derived directory ({}): {error}",
            target.display()
        )
    })?;
    let mut entries = fs::read_dir(source)
        .map_err(|error| format!("Failed to read the staged derived directory: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to read a staged derived entry: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)
            .map_err(|error| format!("Failed to read a staged derived entry: {error}"))?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(format!(
                "The staged derived directory contains a link entry: {}",
                source_path.display()
            ));
        }
        if metadata.is_dir() {
            copy_ordinary_directory_recursively(&source_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &target_path)
                .map_err(|error| format!("Failed to copy a knowledge derived file: {error}"))?;
        } else {
            return Err(format!(
                "The staged derived directory contains a non-regular entry: {}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

/// Walks one host-owned ordinary directory without following links or reparse points.
/// 在不跟随链接或重解析点的前提下遍历一个宿主持有的普通目录。
///
/// # Parameters
/// 参数
/// `root` is a caller-validated directory; `files` accumulates ordinary file paths.
/// `root` 是调用方已校验目录；`files` 累积普通文件路径。
///
/// # Returns
/// 返回值
/// Success after deterministic traversal, or a linked/special entry or I/O failure.
/// 确定性遍历后成功，或返回链接/特殊条目、I/O 错误。
fn collect_ordinary_files_recursively(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(root)
        .map_err(|error| {
            format!(
                "Failed to read the knowledge derived directory ({}): {error}",
                root.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to read a knowledge derived directory entry: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "Failed to read a knowledge derived entry ({}): {error}",
                path.display()
            )
        })?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(format!(
                "The knowledge derived directory contains a link entry: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_ordinary_files_recursively(&path, files)?;
        } else if metadata.is_file() {
            files.push(path);
        } else {
            return Err(format!(
                "The knowledge derived directory contains a non-regular entry: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Converts a relative host path into a safe slash-separated ZIP path.
/// 将相对宿主路径转换为安全的斜杠分隔 ZIP 路径。
///
/// # Parameters
/// 参数
/// `path` is relative to the validated artifact root.
/// `path` 是相对于已校验产物根目录的路径。
///
/// # Returns
/// 返回值
/// A nonempty UTF-8 slash path with ordinary safe components, or an explicit rejection.
/// 返回由安全普通组件组成的非空 UTF-8 斜杠路径，或明确拒绝。
fn relative_to_zip_path(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            return Err(format!(
                "A knowledge derived-artifact relative path contains an invalid component: {}",
                path.display()
            ));
        };
        let value = value.to_str().ok_or_else(|| {
            format!(
                "A knowledge derived-artifact path is not valid UTF-8: {}",
                path.display()
            )
        })?;
        if value.is_empty()
            || value
                .chars()
                .any(|character| matches!(character, '/' | '\\' | ':'))
        {
            return Err(format!(
                "A knowledge derived-artifact path contains invalid characters: {value}"
            ));
        }
        parts.push(value);
    }
    if parts.is_empty() {
        return Err(String::from(
            "A knowledge derived-artifact relative path must not be empty.",
        ));
    }
    Ok(parts.join("/"))
}

/// Computes a streaming lowercase SHA-256 digest for one ordinary file.
/// 为一个普通文件计算流式小写 SHA-256 摘要。
///
/// # Parameters
/// 参数
/// `path` identifies the file previously checked by the archive lifecycle.
/// `path` 标识归档生命周期已经检查的文件。
///
/// # Returns
/// 返回值
/// Its streaming lowercase SHA-256 digest, or an open/read failure.
/// 返回流式小写 SHA-256 摘要，或打开/读取错误。
fn sha256_file_hex(path: &Path) -> Result<String, String> {
    let mut input = File::open(path).map_err(|error| {
        format!(
            "Failed to open the SHA-256 input file ({}): {error}",
            path.display()
        )
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read the SHA-256 input file: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

/// Produces a filesystem-safe timestamped archive name without overwriting prior exports.
/// 生成文件系统安全且带时间戳的归档名称，避免覆盖之前的导出。
///
/// # Parameters
/// 参数
/// `name` is the user-visible library name whose safe prefix is retained.
/// `name` 是保留安全前缀的用户可见知识库名称。
/// `timestamp_ms` is the package export timestamp used for chronological recognition.
/// `timestamp_ms` 是用于按时间识别的知识库包导出时间戳。
/// `export_id` is the canonical host-generated UUID preventing destination collisions.
/// `export_id` 是防止目标冲突的宿主生成规范 UUID。
///
/// # Returns
/// 返回值
/// Returns one bounded `.vkb` filename containing no Windows-reserved path characters.
/// 返回一个不含 Windows 保留路径字符且长度有界的 `.vkb` 文件名。
fn portable_knowledge_archive_name(name: &str, timestamp_ms: i64, export_id: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            value if value.is_control() => '_',
            value => value,
        })
        .take(48)
        .collect();
    let sanitized = sanitized.trim().trim_end_matches('.');
    let stem = if sanitized.is_empty() {
        "knowledge-base"
    } else {
        sanitized
    };
    format!("{stem}-{timestamp_ms}-{export_id}.vkb")
}

#[cfg(test)]
pub(super) mod tests;
