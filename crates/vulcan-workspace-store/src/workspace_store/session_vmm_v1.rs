//! Strict V1-locator persistence for Core-owned per-session VMM state.
//! Core 所有的每会话 VMM 状态严格 V1 定位持久化。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use serde::{Deserialize, Serialize};
use vulcan_vmm::{VMM_SESSION_MEMORY_MAX_BYTES, VmmSessionMemoryState};

use super::session_v1::atomic_io::write_file_atomic;
use super::session_v1_store::SessionV1StorageLocator;

/// Fixed schema version for the V1 session VMM document.
/// V1 会话 VMM 文档的固定结构版本。
const SESSION_VMM_DOCUMENT_SCHEMA_VERSION: u32 = 1;

/// Stable owner kind for the independent V1 session VMM document.
/// 独立 V1 会话 VMM 文档的稳定所有者类型。
const SESSION_VMM_DOCUMENT_KIND: &str = "session_vmm";

/// Process-local locks keyed by the exact V1 VMM document path.
/// 按精确 V1 VMM 文档路径索引的进程内锁。
static SESSION_VMM_FILE_LOCKS: OnceLock<Mutex<BTreeMap<PathBuf, Weak<Mutex<()>>>>> =
    OnceLock::new();

/// Strict durable envelope owned by one V1 session VMM controller.
/// 由单个 V1 会话 VMM 控制器持有的严格持久化信封。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionVmmDocument {
    /// Fixed V1 document schema version.
    /// 固定的 V1 文档结构版本。
    schema_version: u32,
    /// Stable VMM document owner kind.
    /// 稳定的 VMM 文档所有者类型。
    document_kind: String,
    /// Exact V1 session identity owning this document.
    /// 持有该文档的精确 V1 会话身份。
    session_id: String,
    /// Complete controller-owned VMM domain state.
    /// 控制器持有的完整 VMM 领域状态。
    state: VmmSessionMemoryState,
}

/// Strictly loads one V1 session's independent VMM document.
/// 严格加载单个 V1 会话的独立 VMM 文档。
///
/// # Parameters
/// 参数
/// `locator` is the exact SQLite-derived V1 storage owner.
/// `locator` 是由 SQLite 精确派生的 V1 存储所有者。
///
/// # Returns
/// 返回值
/// The validated state, clean absence, or one explicit storage/format error.
/// 返回已校验状态、干净缺失或一条明确的存储/格式错误。
pub fn load_session_vmm_state(
    locator: &SessionV1StorageLocator,
) -> Result<Option<VmmSessionMemoryState>, String> {
    let path = locator
        .validated_vmm_document_path()
        .map_err(|error| error.to_string())?;
    let file_lock = session_vmm_file_lock(&path)?;
    let _guard = file_lock
        .lock()
        .map_err(|error| format!("V1 session VMM file lock failed: {error}"))?;
    load_document_locked(&path, locator.session_id())
}

/// Atomically persists one complete V1 session VMM candidate.
/// 原子持久化一份完整的 V1 会话 VMM 候选状态。
///
/// # Parameters
/// 参数
/// `locator` is the exact SQLite-derived V1 storage owner.
/// `locator` 是由 SQLite 精确派生的 V1 存储所有者。
/// `state` is the complete candidate serialized by the session controller.
/// `state` 是由会话控制器串行化的完整候选状态。
///
/// # Returns
/// 返回值
/// Success only after exact idempotence or atomic publication.
/// 仅在精确幂等或原子发布完成后返回成功。
pub fn persist_session_vmm_state(
    locator: &SessionV1StorageLocator,
    state: &VmmSessionMemoryState,
) -> Result<(), String> {
    let expected_session_id = locator.session_id();
    state.validate(expected_session_id)?;
    let document = SessionVmmDocument {
        schema_version: SESSION_VMM_DOCUMENT_SCHEMA_VERSION,
        document_kind: SESSION_VMM_DOCUMENT_KIND.to_string(),
        session_id: expected_session_id.to_string(),
        state: state.clone(),
    };
    let mut encoded = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("Failed to encode V1 session VMM document: {error}"))?;
    encoded.push(b'\n');
    if encoded.len() > VMM_SESSION_MEMORY_MAX_BYTES {
        return Err(format!(
            "V1 session VMM document for session `{expected_session_id}` is {} bytes and exceeds the {} byte limit.",
            encoded.len(),
            VMM_SESSION_MEMORY_MAX_BYTES
        ));
    }
    let path = locator
        .validated_vmm_document_path()
        .map_err(|error| error.to_string())?;
    let file_lock = session_vmm_file_lock(&path)?;
    let _guard = file_lock
        .lock()
        .map_err(|error| format!("V1 session VMM file lock failed: {error}"))?;
    if let Some(existing) = load_document_locked(&path, expected_session_id)? {
        if existing.revision > state.revision {
            return Err(format!(
                "Rejected stale V1 session VMM revision {} because durable revision {} is newer for session `{expected_session_id}`.",
                state.revision, existing.revision
            ));
        }
        if existing.revision == state.revision {
            if existing == *state {
                return Ok(());
            }
            return Err(format!(
                "Rejected divergent V1 session VMM state at revision {} for session `{expected_session_id}`.",
                state.revision
            ));
        }
    }
    write_file_atomic(&path, &encoded, "V1 session VMM document").map_err(|error| error.to_string())
}

/// Returns one shared process-local lock for an exact VMM document path.
/// 返回一个精确 VMM 文档路径对应的共享进程内锁。
///
/// # Parameters
/// 参数
/// `path` is the exact locator-derived VMM document path.
/// `path` 是由定位器派生的精确 VMM 文档路径。
///
/// # Returns
/// 返回值
/// A shared mutex serializing strict reads and atomic publications for that path.
/// 返回一个共享互斥锁，用于串行化该路径的严格读取与原子发布。
fn session_vmm_file_lock(path: &Path) -> Result<Arc<Mutex<()>>, String> {
    let registry = SESSION_VMM_FILE_LOCKS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut guard = registry
        .lock()
        .map_err(|error| format!("V1 session VMM lock registry failed: {error}"))?;
    guard.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = guard.get(path).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    let lock = Arc::new(Mutex::new(()));
    guard.insert(path.to_path_buf(), Arc::downgrade(&lock));
    Ok(lock)
}

/// Loads and validates one exact VMM document while its file lock is held.
/// 在持有文件锁时加载并校验一个精确 VMM 文档。
///
/// # Parameters
/// 参数
/// `path` is the exact V1-owned `vmm.json` path.
/// `path` 是由 V1 持有的精确 `vmm.json` 路径。
/// `expected_session_id` is the locator owner that must match the document.
/// `expected_session_id` 是必须与文档一致的定位器所有者。
///
/// # Returns
/// 返回值
/// The validated state or clean absence.
/// 返回已校验状态或干净缺失。
fn load_document_locked(
    path: &Path,
    expected_session_id: &str,
) -> Result<Option<VmmSessionMemoryState>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Failed to inspect V1 session VMM document `{}`: {error}",
                path.display()
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "V1 session VMM document `{}` must not be a symbolic link.",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "V1 session VMM document `{}` is not a regular file.",
            path.display()
        ));
    }
    if metadata.len() > VMM_SESSION_MEMORY_MAX_BYTES as u64 {
        return Err(format!(
            "V1 session VMM document `{}` is {} bytes and exceeds the {} byte limit.",
            path.display(),
            metadata.len(),
            VMM_SESSION_MEMORY_MAX_BYTES
        ));
    }
    let encoded = fs::read(path).map_err(|error| {
        format!(
            "Failed to read V1 session VMM document `{}`: {error}",
            path.display()
        )
    })?;
    let document = serde_json::from_slice::<SessionVmmDocument>(&encoded).map_err(|error| {
        format!(
            "Failed to decode strict V1 session VMM document `{}`: {error}",
            path.display()
        )
    })?;
    validate_document(&document, expected_session_id)?;
    Ok(Some(document.state))
}

/// Validates one strict V1 VMM document before it becomes authoritative.
/// 在严格 V1 VMM 文档成为权威状态前校验它。
///
/// # Parameters
/// 参数
/// `document` is the decoded durable envelope.
/// `document` 是已解码的持久化信封。
/// `expected_session_id` is the exact locator owner.
/// `expected_session_id` 是精确的定位器所有者。
///
/// # Returns
/// 返回值
/// Success only when schema, kind, owner, and domain state agree.
/// 仅当结构、类型、所有者与领域状态全部一致时返回成功。
fn validate_document(
    document: &SessionVmmDocument,
    expected_session_id: &str,
) -> Result<(), String> {
    if document.schema_version != SESSION_VMM_DOCUMENT_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported V1 session VMM document schema version {}.",
            document.schema_version
        ));
    }
    if document.document_kind != SESSION_VMM_DOCUMENT_KIND {
        return Err(format!(
            "Unsupported V1 session VMM document kind `{}`.",
            document.document_kind
        ));
    }
    if document.session_id != expected_session_id {
        return Err(format!(
            "V1 session VMM owner mismatch: expected `{expected_session_id}`, found `{}`.",
            document.session_id
        ));
    }
    document.state.validate(expected_session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use uuid::Uuid;

    /// Monotonic suffix used to isolate VMM persistence tests inside the process temp root.
    /// 用于在进程临时根目录中隔离 VMM 持久化测试的单调后缀。
    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    /// Owns one exact V1 session fixture and removes its isolated data root on drop.
    /// 持有一个精确 V1 会话夹具，并在析构时移除其隔离数据根目录。
    struct TestVmmFixture {
        /// Isolated application data root.
        /// 隔离的应用数据根目录。
        root: PathBuf,
        /// Exact V1 storage owner.
        /// 精确的 V1 存储所有者。
        locator: SessionV1StorageLocator,
        /// Canonical session identity shared by the locator and VMM document.
        /// 由定位器与 VMM 文档共享的规范会话身份。
        session_id: String,
    }

    impl TestVmmFixture {
        /// Creates one fully published V1 session without using an external SQLite controller.
        /// 在不使用外部 SQLite Controller 的情况下创建一个完整发布的 V1 会话。
        ///
        /// # Returns
        /// 返回值
        /// A test-owned locator and canonical session identity.
        /// 返回测试独占的定位器与规范会话身份。
        fn create() -> Self {
            let suffix = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "vulcan-session-vmm-v1-{}-{suffix}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            let session_id = Uuid::new_v4().to_string();
            let locator = super::super::session_v1_store::create_session_v1_storage_fixture(
                &root,
                &session_id,
                1,
            )
            .expect("create V1 VMM fixture");
            Self {
                root,
                locator,
                session_id,
            }
        }

        /// Returns the exact V1-owned `vmm.json` path after metadata validation.
        /// 在元数据校验后返回由 V1 持有的精确 `vmm.json` 路径。
        ///
        /// # Returns
        /// 返回值
        /// The locator-derived VMM document path.
        /// 返回由定位器派生的 VMM 文档路径。
        fn document_path(&self) -> PathBuf {
            self.locator
                .validated_vmm_document_path()
                .expect("resolve V1 VMM document path")
        }

        /// Builds one valid initial VMM state owned by this fixture.
        /// 构建一份由该夹具持有的有效初始 VMM 状态。
        ///
        /// # Returns
        /// 返回值
        /// A revision-one strict VMM state.
        /// 返回 revision 为一的严格 VMM 状态。
        fn initial_state(&self) -> VmmSessionMemoryState {
            VmmSessionMemoryState::new(self.session_id.clone(), "1")
                .expect("create initial VMM state")
        }
    }

    impl Drop for TestVmmFixture {
        /// Removes only the fixture-owned isolated data root.
        /// 仅移除夹具独占的隔离数据根目录。
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// Verifies clean absence, atomic round-trip, and exact idempotent replay.
    /// 验证干净缺失、原子往返与精确幂等重放。
    #[test]
    fn round_trip_and_exact_replay_are_stable() {
        let fixture = TestVmmFixture::create();
        let state = fixture.initial_state();

        assert_eq!(
            load_session_vmm_state(&fixture.locator).expect("load absent VMM state"),
            None
        );
        persist_session_vmm_state(&fixture.locator, &state).expect("persist initial VMM state");
        let first_bytes = fs::read(fixture.document_path()).expect("read first VMM bytes");
        persist_session_vmm_state(&fixture.locator, &state).expect("replay identical VMM state");
        let replayed_bytes = fs::read(fixture.document_path()).expect("read replayed VMM bytes");

        assert_eq!(first_bytes, replayed_bytes);
        assert_eq!(
            load_session_vmm_state(&fixture.locator)
                .expect("reload VMM state")
                .expect("VMM state should exist"),
            state
        );
    }

    /// Verifies stale and divergent same-revision candidates cannot replace durable state.
    /// 验证过期候选与同 revision 分叉候选无法替换持久状态。
    #[test]
    fn stale_and_divergent_candidates_are_rejected_without_mutation() {
        let fixture = TestVmmFixture::create();
        let stale = fixture.initial_state();
        let mut durable = stale.clone();
        durable
            .record_committed_controller_command("command-2", stale.revision, "2")
            .expect("advance VMM state");
        persist_session_vmm_state(&fixture.locator, &durable).expect("persist newer VMM state");
        let durable_bytes = fs::read(fixture.document_path()).expect("read durable VMM bytes");

        let stale_error = persist_session_vmm_state(&fixture.locator, &stale)
            .expect_err("stale VMM state must fail");
        let mut divergent = durable.clone();
        divergent.updated_at = "same-revision-divergence".to_string();
        let divergent_error = persist_session_vmm_state(&fixture.locator, &divergent)
            .expect_err("divergent VMM state must fail");

        assert!(stale_error.contains("Rejected stale V1 session VMM revision"));
        assert!(divergent_error.contains("Rejected divergent V1 session VMM state"));
        assert_eq!(
            fs::read(fixture.document_path()).expect("read unchanged durable VMM bytes"),
            durable_bytes
        );
    }

    /// Verifies schema, kind, owner, and unknown envelope fields are rejected strictly.
    /// 验证结构版本、文档类型、所有者与未知信封字段都会被严格拒绝。
    #[test]
    fn strict_envelope_rejects_every_identity_drift() {
        let fixture = TestVmmFixture::create();
        persist_session_vmm_state(&fixture.locator, &fixture.initial_state())
            .expect("persist canonical VMM state");
        let path = fixture.document_path();
        let canonical = fs::read(&path).expect("read canonical VMM document");
        let canonical_value: serde_json::Value =
            serde_json::from_slice(&canonical).expect("decode canonical VMM JSON");
        let cases = [
            (
                "schema_version",
                serde_json::json!(2),
                "Unsupported V1 session VMM document schema",
            ),
            (
                "document_kind",
                serde_json::json!("other"),
                "Unsupported V1 session VMM document kind",
            ),
            (
                "session_id",
                serde_json::json!(Uuid::new_v4().to_string()),
                "V1 session VMM owner mismatch",
            ),
        ];
        for (field, value, expected_error) in cases {
            let mut candidate = canonical_value.clone();
            candidate[field] = value;
            fs::write(
                &path,
                serde_json::to_vec_pretty(&candidate).expect("encode invalid VMM document"),
            )
            .expect("write invalid VMM document");
            let error = load_session_vmm_state(&fixture.locator)
                .expect_err("identity drift must fail strict load");
            assert!(error.contains(expected_error), "unexpected error: {error}");
        }

        let mut unknown_field = canonical_value;
        unknown_field["unexpected"] = serde_json::json!(true);
        fs::write(
            &path,
            serde_json::to_vec_pretty(&unknown_field).expect("encode unknown-field document"),
        )
        .expect("write unknown-field VMM document");
        let unknown_error = load_session_vmm_state(&fixture.locator)
            .expect_err("unknown envelope field must fail strict load");
        assert!(unknown_error.contains("unknown field"));
    }

    /// Verifies malformed durable bytes are reported and never replaced by a later candidate.
    /// 验证损坏的持久字节会被报告，且绝不会被后续候选替换。
    #[test]
    fn malformed_document_is_reported_without_repair() {
        let fixture = TestVmmFixture::create();
        let path = fixture.document_path();
        let malformed = b"{\"schema_version\":1,\"document_kind\":\"session_vmm\"";
        fs::write(&path, malformed).expect("write malformed VMM document");

        let load_error = load_session_vmm_state(&fixture.locator)
            .expect_err("malformed VMM document must fail load");
        let persist_error = persist_session_vmm_state(&fixture.locator, &fixture.initial_state())
            .expect_err("malformed VMM document must block replacement");

        assert!(load_error.contains("Failed to decode strict V1 session VMM document"));
        assert!(persist_error.contains("Failed to decode strict V1 session VMM document"));
        assert_eq!(
            fs::read(&path).expect("read malformed VMM bytes"),
            malformed
        );
    }

    /// Verifies the VMM document target must be a regular file.
    /// 验证 VMM 文档目标必须是普通文件。
    #[test]
    fn non_file_target_is_rejected() {
        let fixture = TestVmmFixture::create();
        let path = fixture.document_path();
        fs::create_dir(&path).expect("create conflicting VMM directory");

        let error = load_session_vmm_state(&fixture.locator)
            .expect_err("non-file VMM target must fail load");

        assert!(error.contains("is not a regular file"));
        assert!(path.is_dir());
    }
}
