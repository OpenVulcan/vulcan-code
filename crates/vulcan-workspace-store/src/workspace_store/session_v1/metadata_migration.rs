//! Versioned metadata upgrades with verified provenance and atomic publication.
//! 具有来源校验和原子发布的版本化元数据升级。

use std::fs::{self, File, OpenOptions};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;
use vulcan_session_core::v1::collaboration::{
    CollaborationAfterState, CollaborationTaskMessageEffectKind,
};
use vulcan_session_core::v1::config::{
    METADATA_CONTRACT, METADATA_SCHEMA_VERSION, MetadataDocument,
};
use vulcan_session_core::v1::event::{EventBody, SystemContextOperation, UserInputSource};

use super::atomic_io::write_file_atomic;
use super::errors::{ErrorCode, SessionV1Error};
use super::layout::SessionV1Layout;
use super::segment::parse_segment;

/// Minimal version discriminator, decoded before the current required fields.
/// 在当前必填字段之前解码的最小版本判别结构。
#[derive(Deserialize)]
struct MetadataHeader {
    /// Stable metadata family, independent from its evolvable schema.
    /// 稳定元数据家族标识，独立于可演进模式。
    contract: String,
    /// Exact persisted schema revision; never inferred from missing fields.
    /// 精确持久化模式修订；绝不根据缺失字段猜测版本。
    schema_version: u32,
}

/// Acquires the permanent metadata publication lock for an existing session directory.
/// 为已有会话目录取得永久元数据发布锁。
/// `path` is the exact metadata path; dropping the returned handle releases the OS lock.
/// `path` 是精确元数据路径；释放返回的句柄即释放操作系统锁。
pub(super) fn publication_lock(path: &Path) -> Result<File, SessionV1Error> {
    // A separate stable inode survives atomic replacement of metadata and process crashes.
    // 独立稳定文件节点不受元数据原子替换和进程崩溃影响。
    let lock_path = path.with_extension("write.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0x0000_0003).custom_flags(0x0020_0000);
    }
    let file = options.open(&lock_path).map_err(|error| {
        SessionV1Error::io(
            &lock_path,
            format!("open metadata publication lock: {error}"),
        )
    })?;
    let info = fs::symlink_metadata(&lock_path).map_err(|error| {
        SessionV1Error::io(
            &lock_path,
            format!("inspect metadata publication lock: {error}"),
        )
    })?;
    if !info.is_file() || info.file_type().is_symlink() {
        return Err(SessionV1Error::io(
            &lock_path,
            "metadata lock must be a regular file",
        ));
    }
    file.lock().map_err(|error| {
        SessionV1Error::io(
            &lock_path,
            format!("acquire metadata publication lock: {error}"),
        )
    })?;
    Ok(file)
}

/// Reads current metadata, or upgrades an explicitly supported old version under the writer lock.
/// 读取当前元数据，或在写入锁内升级明确支持的旧版本。
/// `path` identifies one session; returns typed metadata after backup and atomic publication succeed.
/// `path` 标识一个会话；备份和原子发布成功后返回强类型元数据。
pub(super) fn read_metadata(path: &Path) -> Result<MetadataDocument, SessionV1Error> {
    let bytes = read_bytes(path)?;
    let version = read_schema_version(path, &bytes)?;
    if version == METADATA_SCHEMA_VERSION {
        return decode(path, &bytes);
    }
    let _guard = publication_lock(path)?;
    // A writer may have advanced the document since the unlocked fast-path read.
    // 无锁快速读取之后，写入方可能已推进文档。
    let original = read_bytes(path)?;
    let version = read_schema_version(path, &original)?;
    if version == METADATA_SCHEMA_VERSION {
        return decode(path, &original);
    }
    let raw: Value = decode(path, &original)?;
    let document = upgrade(path, raw)?;
    let encoded = serde_json::to_vec(&document)
        .map_err(|error| corrupt(path, format!("encode upgraded metadata: {error}")))?;
    preserve_original(path, &original, version)?;
    write_file_atomic(path, &encoded, "metadata migration")?;
    Ok(document)
}

/// Preserves validated legacy bytes before either automatic migration or an explicit rebuild publishes.
/// 在自动迁移或显式重建发布前保留经过校验的旧字节。
/// Caller holds the publication lock; existing identical backups are reused, conflicting ones rejected.
/// 调用方持有发布锁；复用已有相同备份，拒绝冲突备份。
pub(super) fn preserve_original(
    path: &Path,
    original: &[u8],
    version: u32,
) -> Result<(), SessionV1Error> {
    if version == METADATA_SCHEMA_VERSION {
        return Ok(());
    }
    let backup = path.with_file_name(format!("metadata.schema-{version}.backup.json"));
    match fs::read(&backup) {
        Ok(existing) if existing == original => {}
        Ok(_) => {
            return Err(corrupt(
                &backup,
                "migration backup differs from the source; refusing to overwrite",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_file_atomic(&backup, original, "metadata migration backup")?;
        }
        Err(error) => {
            return Err(SessionV1Error::io(
                &backup,
                format!("read metadata backup: {error}"),
            ));
        }
    }
    Ok(())
}

/// Upgrades a detached JSON document without publishing it, including explicit capability rebuilds.
/// 升级独立 JSON 文档而不发布，也适用于显式能力重建。
/// `path` resolves provenance when required; returns only a fully typed supported version.
/// `path` 在必要时解析来源；仅返回完整强类型且受支持的版本。
pub(super) fn upgrade(path: &Path, mut raw: Value) -> Result<MetadataDocument, SessionV1Error> {
    if !raw.is_object() {
        return Err(corrupt(path, "metadata must be a JSON object"));
    }
    let encoded = serde_json::to_vec(&raw)
        .map_err(|error| corrupt(path, format!("encode migration input: {error}")))?;
    let mut version = read_schema_version(path, &encoded)?;
    while version < METADATA_SCHEMA_VERSION {
        match version {
            1 => migrate_v1_to_v2(path, &mut raw)?,
            _ => {
                return Err(SessionV1Error::unsupported(
                    path,
                    format!("no metadata migration from schema {version}"),
                ));
            }
        }
        version += 1;
        raw["schema_version"] = Value::from(version);
    }
    let document: MetadataDocument = serde_json::from_value(raw)
        .map_err(|error| corrupt(path, format!("validate upgraded metadata: {error}")))?;
    validate_hook_identity(path, &document)?;
    Ok(document)
}

/// Validates Hook schema and immutable owner coordinates without requiring history for explicit policies.
/// 校验 Hook 模式和不可变所属坐标；明确策略不要求读取历史。
/// `metadata` is detached; invalid identity aborts publication rather than clearing the binding.
/// `metadata` 为独立候选；身份无效会中止发布，而非清空绑定。
fn validate_hook_identity(path: &Path, metadata: &MetadataDocument) -> Result<(), SessionV1Error> {
    metadata
        .business_hook
        .validate(metadata.segment_generation)
        .map_err(|error| corrupt(path, format!("invalid legacy Hook: {error}")))?;
    if metadata
        .business_hook
        .binding
        .as_ref()
        .is_some_and(|binding| {
            binding.session_id != metadata.session_id
                || binding.turn_id.value() >= metadata.id_frontier.next_local_id
                || binding.start_seq.value() >= metadata.id_frontier.next_seq
        })
    {
        return Err(corrupt(path, "legacy Hook identity or frontier mismatch"));
    }
    Ok(())
}

/// Adds only the missing observer policy to schema 1, preserving already explicit values.
/// 仅为模式 1 补充缺失的观察策略，保留已有明确值。
/// The original event proves human or agent ownership; an unprovable binding aborts migration.
/// 原始事件证明用户或代理归属；无法证明的绑定会中止迁移。
fn migrate_v1_to_v2(path: &Path, raw: &mut Value) -> Result<(), SessionV1Error> {
    let missing = raw
        .pointer("/business_hook/binding")
        .and_then(Value::as_object)
        .is_some_and(|binding| !binding.contains_key("observes_user"));
    if !missing {
        return Ok(());
    }
    // This provisional value exists only in a detached candidate and is never published or executed.
    // 临时值仅存在于独立候选中，绝不发布或执行。
    raw["business_hook"]["binding"]["observes_user"] = Value::Bool(false);
    let candidate: MetadataDocument = serde_json::from_value(raw.clone()).map_err(|error| {
        corrupt(
            path,
            format!("decode schema 1 migration candidate: {error}"),
        )
    })?;
    let observes_user = infer_observer_from_start(path, &candidate)?;
    raw["business_hook"]["binding"]["observes_user"] = Value::Bool(observes_user);
    Ok(())
}

/// Resolves the exact Hook start event in the declared segment tree and checks its owner.
/// 在声明分片树中解析精确 Hook 起始事件并校验所属者。
/// Inputs are a metadata path and typed candidate; returns the policy without guessing missing history.
/// 输入为元数据路径和强类型候选；返回策略，绝不猜测缺失历史。
fn infer_observer_from_start(
    path: &Path,
    metadata: &MetadataDocument,
) -> Result<bool, SessionV1Error> {
    validate_hook_identity(path, metadata)?;
    let binding = metadata
        .business_hook
        .binding
        .as_ref()
        .ok_or_else(|| corrupt(path, "migration requires the legacy Hook binding"))?;
    let mut matches = metadata.split_list.iter().filter(|entry| {
        entry.start_seq <= binding.start_seq
            && entry.end_seq.is_none_or(|end| binding.start_seq <= end)
    });
    let entry = matches
        .next()
        .ok_or_else(|| corrupt(path, "legacy Hook start segment is absent"))?;
    if matches.next().is_some()
        || !SessionV1Layout::is_segment_file_name(entry.segment_number, &entry.file_name)
    {
        return Err(corrupt(
            path,
            "legacy Hook segment map is ambiguous or invalid",
        ));
    }
    let directory = path
        .parent()
        .ok_or_else(|| corrupt(path, "metadata has no session directory"))?;
    let segment = SessionV1Layout::segment_entry_in_directory(
        directory,
        metadata.segment_generation,
        &entry.file_name,
    );
    let events = parse_segment(&segment, entry.segment_number, Some(entry.start_seq))?;
    let event = events
        .iter()
        .find(|event| event.seq == binding.start_seq)
        .ok_or_else(|| corrupt(path, "legacy Hook start event is absent"))?;
    if event.turn_id != Some(binding.turn_id) {
        return Err(corrupt(path, "legacy Hook start belongs to another turn"));
    }
    // These branches mirror canonical Turn origins in SessionDocument::rebuild_derived_state.
    // 以下分支对应 SessionDocument::rebuild_derived_state 的规范回合来源。
    match &event.event {
        EventBody::UserInput(input)
            if matches!(
                input.input_source,
                UserInputSource::Root | UserInputSource::QueuedScheduled
            ) =>
        {
            Ok(true)
        }
        EventBody::SubagentReturn(_) => Ok(true),
        EventBody::SystemContext(context)
            if context.context_key == "goal_continuation"
                || (context.context_key == "goal_state"
                    && context.operation == SystemContextOperation::Upsert) =>
        {
            Ok(true)
        }
        EventBody::Collaboration(record)
            if let CollaborationAfterState::TaskMessageEffect { effect } = &record.after_state
                && let CollaborationTaskMessageEffectKind::Claimed { turn_id, .. } =
                    &effect.outcome
                && *turn_id == binding.turn_id =>
        {
            Ok(false)
        }
        _ => Err(corrupt(
            path,
            "legacy Hook start does not prove a supported business origin",
        )),
    }
}

/// Validates the metadata family and supported version before interpreting its body.
/// 在解释正文之前校验元数据家族及受支持版本。
/// Returns the exact version, rejecting future versions without creating backups or rewriting files.
/// 返回精确版本；拒绝未来版本时不创建备份或改写文件。
pub(super) fn read_schema_version(path: &Path, bytes: &[u8]) -> Result<u32, SessionV1Error> {
    let header: MetadataHeader = decode(path, bytes)?;
    if header.contract != METADATA_CONTRACT
        || !(1..=METADATA_SCHEMA_VERSION).contains(&header.schema_version)
    {
        return Err(SessionV1Error::unsupported(
            path,
            format!(
                "unsupported metadata contract {:?} schema {}; supported schemas are 1 through {}",
                header.contract, header.schema_version, METADATA_SCHEMA_VERSION,
            ),
        ));
    }
    Ok(header.schema_version)
}

/// Reads exact metadata bytes, preserving absence and I/O diagnostics.
/// 读取精确元数据字节，保留缺失和输入输出诊断。
/// Returns original bytes for validation and backup; never creates a missing session.
/// 返回用于校验和备份的原始字节；绝不创建缺失会话。
fn read_bytes(path: &Path) -> Result<Vec<u8>, SessionV1Error> {
    fs::read(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => corrupt(path, "metadata file does not exist"),
        _ => SessionV1Error::io(path, format!("read metadata: {error}")),
    })
}

/// Deserializes a metadata shape and preserves an explicit corruption error.
/// 反序列化元数据结构并保留明确的损坏错误。
/// `bytes` are untouched input; the caller chooses the required typed shape.
/// `bytes` 为未改写输入；调用方选择所需强类型结构。
fn decode<T: serde::de::DeserializeOwned>(path: &Path, bytes: &[u8]) -> Result<T, SessionV1Error> {
    serde_json::from_slice(bytes).map_err(|error| corrupt(path, format!("parse metadata: {error}")))
}

/// Builds one metadata-specific error without hiding migration or provenance failures.
/// 构造元数据专用错误，不掩盖迁移或来源失败。
/// Associates the diagnostic with the exact offending path.
/// 将诊断关联到精确出错路径。
fn corrupt(path: &Path, detail: impl Into<String>) -> SessionV1Error {
    SessionV1Error::corrupt(ErrorCode::MetadataCorrupt, path, detail)
}
