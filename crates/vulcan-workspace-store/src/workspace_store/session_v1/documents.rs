//! Typed read/write of the V1 metadata, runtime, and compactions documents.
//! V1 metadata、runtime、compactions 文档的类型化读写。
//!
//! Metadata uses explicit version migrations; runtime and compaction contracts stay independent.
//! 元数据使用明确的版本迁移；运行时和压缩契约保持独立。

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use vulcan_session_core::v1::config::MetadataDocument;
use vulcan_session_core::v1::context::{
    COMPACTIONS_CONTRACT, CompactionsDocument, RUNTIME_CONTRACT, RuntimeDocument,
};
use vulcan_session_core::v1::errors::DomainError;

use super::atomic_io::write_file_atomic;
use super::errors::{ErrorCode, SessionV1Error, SessionV1ErrorDetails};

/// Writes one typed document as compact JSON through an atomic publish.
/// 通过原子发布把一个类型化文档写成紧凑 JSON。
pub fn write_json_document<T: Serialize>(
    path: &Path,
    document: &T,
    artifact_kind: &str,
) -> Result<(), SessionV1Error> {
    let bytes = serde_json::to_vec(document).map_err(|error| {
        SessionV1Error::atomic_write(
            path,
            "serialize",
            format!("serialize {artifact_kind}: {error}"),
        )
    })?;
    // Metadata migration and all normal publishers share the same stable file lock.
    // 元数据迁移和所有正常发布者共用同一个稳定文件锁。
    let _metadata_guard = if path
        .file_name()
        .is_some_and(|name| name == super::layout::METADATA_FILE)
    {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                SessionV1Error::io(parent, format!("create metadata parent: {error}"))
            })?;
        }
        Some(super::metadata_migration::publication_lock(path)?)
    } else {
        None
    };
    write_file_atomic(path, &bytes, artifact_kind)
}

/// Reads and parses one typed document as compact JSON.
/// 读取并解析一个类型化文档为紧凑 JSON。
fn read_json_document<T: DeserializeOwned + std::fmt::Debug>(
    path: &Path,
    artifact_kind: &str,
    error_code: ErrorCode,
) -> Result<T, SessionV1Error> {
    let bytes = std::fs::read(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => SessionV1Error::corrupt(
            error_code,
            path,
            format!("{artifact_kind} file does not exist"),
        ),
        _ => SessionV1Error::io(path.to_path_buf(), format!("read {artifact_kind}: {error}")),
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        SessionV1Error::corrupt(error_code, path, format!("parse {artifact_kind}: {error}"))
    })
}

/// Reads metadata through explicit, backed-up schema migrations before returning the current type.
/// 通过明确且有备份的模式迁移读取元数据，再返回当前类型。
pub fn read_metadata(path: &Path) -> Result<MetadataDocument, SessionV1Error> {
    super::metadata_migration::read_metadata(path)
}

/// Reads `path` for an explicit capability rebuild, replacing only the known capability subtree.
/// 为显式能力重建读取 `path`，仅替换确定的能力子树。
/// Returns typed metadata with `selection`; invalid JSON, other fields and contracts remain errors.
/// 返回包含 `selection` 的类型化元数据；无效 JSON、其他字段及契约仍然报错。
pub(crate) fn read_metadata_for_provider_tool_rebuild(
    path: &Path,
    selection: &vulcan_session_core::v1::config::ProviderToolSelectionConfig,
) -> Result<MetadataDocument, SessionV1Error> {
    // Decode just enough structure to replace a malformed optional addon, never the conversation.
    // 仅解码替换损坏可选附加项所需的结构，绝不替换会话本身。
    let _guard = super::metadata_migration::publication_lock(path)?;
    let original = std::fs::read(path)
        .map_err(|error| SessionV1Error::io(path, format!("read rebuild metadata: {error}")))?;
    let mut raw: serde_json::Value = serde_json::from_slice(&original).map_err(|error| {
        SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            path,
            format!("parse rebuild metadata: {error}"),
        )
    })?;
    let original_version = super::metadata_migration::read_schema_version(path, &original)?;
    let selections = raw
        .pointer_mut("/config/selection")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| {
            SessionV1Error::corrupt(
                ErrorCode::MetadataCorrupt,
                path,
                "metadata config.selection must be an object; capability-only rebuild refused",
            )
        })?;
    selections.insert(
        "provider_tools".to_owned(),
        serde_json::to_value(selection).map_err(|error| {
            SessionV1Error::corrupt(
                ErrorCode::MetadataCorrupt,
                path,
                format!("serialize capability defaults: {error}"),
            )
        })?,
    );
    // Reuse the same detached upgrade engine; the explicit rebuild owns publication.
    // 复用同一独立升级引擎；显式重建负责发布。
    let document = super::metadata_migration::upgrade(path, raw)?;
    // Upgrade has validated the exact u32 header; a rebuild must preserve the same original bytes.
    // 升级已校验精确 u32 头部；重建必须保留同样的原始字节。
    super::metadata_migration::preserve_original(path, &original, original_version)?;
    Ok(document)
}

/// Reads the runtime document and validates its frozen contract.
/// 读取 runtime 文档并校验其冻结契约。
pub fn read_runtime(path: &Path) -> Result<RuntimeDocument, SessionV1Error> {
    let document: RuntimeDocument = read_json_document(path, "runtime", ErrorCode::RuntimeCorrupt)?;
    validate_contract(
        &document.contract,
        RUNTIME_CONTRACT,
        path,
        document.schema_version,
    )?;
    Ok(document)
}

/// Reads the compactions document and validates its frozen contract.
/// 读取 compactions 文档并校验其冻结契约。
pub fn read_compactions(path: &Path) -> Result<CompactionsDocument, SessionV1Error> {
    let document: CompactionsDocument =
        read_json_document(path, "compactions", ErrorCode::CompactionCorrupt)?;
    validate_contract(
        &document.contract,
        COMPACTIONS_CONTRACT,
        path,
        document.schema_version,
    )?;
    Ok(document)
}

/// Validates one contract string and schema version against the frozen V1
/// contract.
/// 校验一个契约字符串与 schema 版本是否符合冻结的 V1 契约。
///
/// Old development formats must be reported as unsupported, never migrated,
/// repaired, or silently accepted.
/// 旧开发格式必须被报告为不受支持，绝不迁移、修复或静默接受。
fn validate_contract(
    actual: &str,
    expected: &str,
    path: &Path,
    schema_version: u32,
) -> Result<(), SessionV1Error> {
    if actual == expected && schema_version == 1 {
        return Ok(());
    }
    Err(SessionV1Error::from_details(
        ErrorCode::UnsupportedDevelopmentFormat,
        SessionV1ErrorDetails {
            session_id: None,
            path: Some(path.to_path_buf()),
            context: None,
            line: None,
            byte_offset: None,
            expected: Some(format!("{expected} schema_version 1")),
            actual: Some(format!("{actual} schema_version {schema_version}")),
            detail: format!(
                "contract {actual:?} or schema_version {schema_version} is not the frozen V1 contract; old development sessions are unsupported"
            ),
        },
    ))
}

/// Maps one domain error (from derived-state rebuild) into a storage error.
/// 把一个领域错误(来自派生状态重建)映射为存储错误。
pub fn map_domain_error(
    error: DomainError,
    session_id: &str,
    path: impl Into<std::path::PathBuf>,
) -> SessionV1Error {
    match error {
        DomainError::ProviderSourceFingerprintMismatch {
            seq,
            stored,
            expected,
        } => SessionV1Error::from_details(
            ErrorCode::ProviderSourceFingerprintMismatch,
            SessionV1ErrorDetails {
                session_id: Some(session_id.to_string()),
                path: Some(path.into()),
                context: Some(format!("provider_source checkpoint seq {seq}")),
                line: None,
                byte_offset: None,
                expected: Some(expected),
                actual: Some(stored),
                detail: String::from(
                    "No supported fingerprint format matches the persisted checkpoint; history was not rewritten. Inspect the session data before recovery.",
                ),
            },
        ),
        DomainError::SequenceViolation { expected, actual } => {
            SessionV1Error::discontinuity(path, "main event stream", expected, actual)
                .into_session(session_id)
        }
        DomainError::ReferenceViolation { kind, value } => SessionV1Error::from_details(
            ErrorCode::ReferenceViolation,
            SessionV1ErrorDetails {
                session_id: Some(session_id.to_string()),
                path: None,
                context: Some(format!("{kind} {value}")),
                line: None,
                byte_offset: None,
                expected: None,
                actual: None,
                detail: format!("reference to missing {kind} id {value}"),
            },
        ),
        other => SessionV1Error::from_details(
            ErrorCode::SegmentCorrupt,
            SessionV1ErrorDetails {
                session_id: Some(session_id.to_string()),
                path: None,
                context: Some("derived-state rebuild".to_string()),
                line: None,
                byte_offset: None,
                expected: None,
                actual: None,
                detail: other.to_string(),
            },
        ),
    }
}

/// Local extension attaching a session id to one storage error.
/// 为一条存储错误附加会话 id 的局部扩展。
trait IntoSession: Sized {
    /// Returns the error with the session id attached.
    /// 返回携带会话 id 的错误。
    fn into_session(self, session_id: &str) -> Self;
}

impl IntoSession for SessionV1Error {
    fn into_session(mut self, session_id: &str) -> Self {
        if self.session_id.is_none() {
            self.session_id = Some(session_id.to_string());
        }
        self
    }
}
