//! Real journal files and fault-injected backend effects verify original preparation recovery.
//! 实际日志文件及故障注入后端副作用验证原准备恢复。

use super::*;
use crate::generation_store::{PinnedVulcanPluginGeneration, VulcanPluginStoreErrorCode};
use std::cell::{Cell, RefCell};
use std::fs;

mod preparation;
mod removal_inventory;
mod reservation;
mod retention;

/// Deterministic secure-backend fixture records actual writes and supports one exact failing key.
/// 确定性安全后端夹具记录实际写入，并支持单个精确失败键。
#[derive(Default)]
struct Backend {
    /// Values are retained only in memory, separated by the original revision and declared key.
    /// 值仅保留在内存，按原修订及声明键隔离。
    values: RefCell<BTreeMap<(VulcanPluginCredentialRevision, VulcanPluginCredentialKey), String>>,
    /// Total actual writes detects unwanted overwrite or duplicate resume effects.
    /// 实际写入总数检测非预期覆盖或重复恢复副作用。
    writes: Cell<usize>,
    /// Optional fixture key where writes fail after any preceding key has already succeeded.
    /// 可选夹具键，写入在此前键已经成功后于此失败。
    fail_write: RefCell<Option<String>>,
    /// Optional fixture key where cleanup stops and retains its durable Discarding phase.
    /// 可选夹具键，清理在此停止并保留持久 Discarding 阶段。
    fail_delete: RefCell<Option<String>>,
}

impl VulcanPluginCredentialBackend for Backend {
    /// Read the exact fixture tuple; returns actual absence without probing another revision.
    /// 读取精确夹具元组；返回实际缺失，不探测其他修订。
    fn read(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        assert_eq!(plugin_id, "staging-plugin");
        Ok(self
            .values
            .borrow()
            .get(&(revision.clone(), key.clone()))
            .cloned())
    }
    /// Store the supplied tuple or inject a secret-bearing diagnostic to verify error redaction.
    /// 存储所给元组，或注入含秘密诊断以验证错误脱敏。
    fn write(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
        value: &str,
    ) -> Result<(), String> {
        assert_eq!(plugin_id, "staging-plugin");
        if self.fail_write.borrow().as_deref() == Some(key.secret_key.as_str()) {
            return Err(format!("backend rejected secret {value}"));
        }
        self.values
            .borrow_mut()
            .insert((revision.clone(), key.clone()), value.into());
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }
    /// Remove one exact entry or fail at the configured key; actual absence remains success.
    /// 移除单个精确条目，或在配置键处失败；实际缺失保持成功。
    fn delete(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        assert_eq!(plugin_id, "staging-plugin");
        if self.fail_delete.borrow().as_deref() == Some(key.secret_key.as_str()) {
            return Err("fixture deletion failure".into());
        }
        self.values
            .borrow_mut()
            .remove(&(revision.clone(), key.clone()));
        Ok(())
    }
}

/// Build a valid exact-manifest package and its immutable generation beneath `root`; return the store and real pin.
/// 在 `root` 下构建有效精确清单插件包及其不可变代次；返回存储和实际固定句柄。
fn fixture(root: &std::path::Path) -> (VulcanPluginGenerationStore, PinnedVulcanPluginGeneration) {
    // All source and store bytes are isolated within the test's temporary directory.
    // 全部来源及存储字节隔离在测试临时目录内。
    let source = root.join("source/staging-plugin");
    fs::create_dir_all(&source).expect("create isolated staging source");
    fs::write(
        source.join("plugin.yaml"),
        r#"schema_version: 1
id: staging-plugin
version: 1.0.0
default_enabled: false
display:
  name: Credential staging fixture
  capability: Revision-bound secure values
permissions: [network_https, secrets_read]
mcp:
  servers:
    - id: remote
      name: Remote
      transport:
        type: http
        url: {source: literal, value: 'https://example.test/mcp'}
        headers:
          X-First: {source: secret, key: first}
          X-Second: {source: secret, key: second}
          X-Optional: {source: secret, key: optional}
"#,
    )
    .expect("write staging fixture manifest");
    // The actual production inspector supplies the package fingerprint.
    // 实际生产检查器提供插件包指纹。
    let descriptor =
        crate::inspect_user_plugin_package(&source).expect("inspect actual staging package");
    // Package preparation uses original file publication and actual generation leases.
    // 插件包准备使用原文件发布及实际代次租约。
    let store = VulcanPluginGenerationStore::open(root).expect("open staging fixture store");
    // The returned package stays pinned while each test exercises its credentials independently.
    // 返回插件包在每个测试独立操作凭据期间保持固定。
    let generation = store
        .prepare(&source, &descriptor.content_fingerprint)
        .expect("prepare immutable staging package");
    (store, generation)
}

/// Return the exact remote fixture key named by `secret_key` without normalization.
/// 返回 `secret_key` 指定的精确远端夹具键，不规范化。
fn key(secret_key: &str) -> VulcanPluginCredentialKey {
    VulcanPluginCredentialKey {
        server_id: "remote".into(),
        secret_key: secret_key.into(),
    }
}

/// Return the complete candidate values, including the deliberately absent optional slot.
/// 返回完整候选值，包含刻意缺失的可选槽位。
fn values() -> VulcanPluginCredentialValues {
    BTreeMap::from([
        (key("first"), Some("fixture-sensitive-first".into())),
        (key("second"), Some("fixture-sensitive-second".into())),
        (key("optional"), None),
    ])
}

/// Verify durable intent, exclusive ownership, interrupted writes and exact recovery without overwriting prior success.
/// 验证持久意图、排他所有权、中断写入及精确恢复，不覆盖此前成功值。
#[test]
fn credential_staging_recovers_partial_writes_without_values_in_journal() {
    // A real file-backed store survives dropping every original preparation owner.
    // 实际文件存储在丢弃所有原准备所有者后仍存在。
    let temp = tempfile::tempdir().expect("create partial-write fixture");
    // The package handle remains distinct from credential mutation ownership.
    // 插件包句柄独立于凭据变更所有权。
    let (store, generation) = fixture(temp.path());
    // One caller-created revision remains unchanged through failure and recovery.
    // 单个调用方创建修订在失败及恢复期间保持不变。
    let revision = VulcanPluginCredentialRevision::allocate();
    // This complete request is reused explicitly after interruption.
    // 此完整请求在中断后明确复用。
    let values = values();
    // Mutation cannot start until the inventory has been durably acknowledged.
    // 清单得到持久确认前，变更不能开始。
    let mut owner = store
        .begin_credential_staging(&generation, revision.clone(), &values)
        .expect("persist original credential inventory");
    assert_eq!(
        store
            .resume_credential_staging("staging-plugin", &revision)
            .expect_err("live staging owner must exclude recovery")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    // The backend fails on the second configured key after persisting the first.
    // 后端在持久保存第一个已配置键后，于第二个键失败。
    let backend = Backend::default();
    *backend.fail_write.borrow_mut() = Some("second".into());
    // Public errors must not retain the deliberately secret-bearing backend diagnostic.
    // 公开错误不得保留刻意包含秘密的后端诊断。
    let error = owner
        .write_and_finish(&backend, &values)
        .expect_err("second credential write must fail");
    assert!(!error.to_string().contains("fixture-sensitive"));
    assert_eq!(backend.writes.get(), 1);
    assert_eq!(
        store
            .credential_record("staging-plugin", &revision)
            .expect("read retained partial preparation")
            .expect("partial preparation record must exist")
            .phase(),
        VulcanPluginCredentialPhase::Preparing
    );
    drop(owner);
    drop(store);
    // Reopening reads the original journal without replaying any backend operation.
    // 重开读取原日志，不重放任何后端操作。
    let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen interrupted credential store")
        .expect("interrupted credential store must exist");
    assert_eq!(
        reopened
            .credential_plugin_ids()
            .expect("discover original credential plugin"),
        BTreeSet::from([String::from("staging-plugin")])
    );
    assert_eq!(
        reopened
            .credential_revisions("staging-plugin")
            .expect("discover original credential revision"),
        BTreeSet::from([revision.clone()])
    );
    // No secret bytes or secret digests are written to the durable inventory.
    // 持久清单中不写入秘密字节或秘密摘要。
    let journal = fs::read_to_string(
        reopened
            .root
            .join("credentials/staging-plugin")
            .join(format!("{}.json", revision.as_str())),
    )
    .expect("read original non-secret journal");
    assert!(!journal.contains("fixture-sensitive"));
    assert!(!journal.contains("sha256"));
    // A differing already-written value fails before any additional write occurs.
    // 已写入值不同时，在发生任何额外写入前失败。
    let mut resumed = reopened
        .resume_credential_staging("staging-plugin", &revision)
        .expect("resume original preparation");
    // Only the supplied retry value changes; the durable intent and backend remain original.
    // 仅改变所给重试值；持久意图及后端保持原样。
    let mut changed = values.clone();
    changed.insert(key("first"), Some("different-sensitive-value".into()));
    assert_eq!(
        resumed
            .write_and_finish(&backend, &changed)
            .expect_err("changed existing candidate value must be rejected")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(backend.writes.get(), 1);
    *backend.fail_write.borrow_mut() = None;
    assert_eq!(
        resumed
            .write_and_finish(&backend, &values)
            .expect("finish original preparation after recovery")
            .phase(),
        VulcanPluginCredentialPhase::Staged
    );
    assert_eq!(backend.writes.get(), 2);
    drop(resumed);
    assert!(
        reopened
            .resume_credential_staging("staging-plugin", &revision)
            .is_err()
    );
    assert!(
        reopened
            .discard_credential_staging("staging-plugin", &revision, &backend)
            .is_err()
    );
    assert!(
        reopened
            .begin_credential_staging(&generation, revision, &values)
            .is_err()
    );
}

/// Verify failed cleanup remains Discarding, resumes the original inventory and never recreates a discarded identity.
/// 验证失败清理保持 Discarding、恢复原清单，并且绝不重建已丢弃身份。
#[test]
fn credential_staging_cleanup_survives_partial_failure_and_owner_release() {
    // The candidate and all backend values belong to this test alone.
    // 候选及全部后端值仅属于此测试。
    let temp = tempfile::tempdir().expect("create cleanup fixture");
    // Source bytes need not remain present once the non-secret cleanup inventory is durable.
    // 非秘密清理清单持久化后，来源字节无需继续存在。
    let (store, generation) = fixture(temp.path());
    // One exact revision is kept through both cleanup attempts.
    // 两次清理尝试均保留单个精确修订。
    let revision = VulcanPluginCredentialRevision::allocate();
    // The backend exposes the actual partial side effect left by failed preparation.
    // 后端暴露失败准备留下的实际部分副作用。
    let backend = Backend::default();
    // Preparation writes the first key and fails at the second.
    // 准备写入第一个键并在第二个键失败。
    let mut owner = store
        .begin_credential_staging(&generation, revision.clone(), &values())
        .expect("persist cleanup candidate inventory");
    *backend.fail_write.borrow_mut() = Some("second".into());
    assert!(owner.write_and_finish(&backend, &values()).is_err());
    assert_eq!(
        store
            .discard_credential_staging("staging-plugin", &revision, &backend)
            .expect_err("live preparation must exclude cleanup")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(owner);
    *backend.fail_delete.borrow_mut() = Some("second".into());
    assert!(
        store
            .discard_credential_staging("staging-plugin", &revision, &backend)
            .is_err()
    );
    assert_eq!(
        store
            .credential_record("staging-plugin", &revision)
            .expect("read failed cleanup checkpoint")
            .expect("failed cleanup record must exist")
            .phase(),
        VulcanPluginCredentialPhase::Discarding
    );
    assert!(
        store
            .resume_credential_staging("staging-plugin", &revision)
            .is_err()
    );
    drop(store);
    // The next store consumes the original cleanup phase without reconstructing candidate values.
    // 后续存储消费原清理阶段，不重建候选值。
    let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen retained cleanup store")
        .expect("retained cleanup store must exist");
    *backend.fail_delete.borrow_mut() = None;
    assert!(
        reopened
            .discard_credential_staging("staging-plugin", &revision, &backend)
            .expect("continue original cleanup")
    );
    assert!(backend.values.borrow().is_empty());
    assert!(
        !reopened
            .discard_credential_staging("staging-plugin", &revision, &backend)
            .expect("confirm completed cleanup idempotently")
    );
    assert_eq!(
        reopened
            .credential_record("staging-plugin", &revision)
            .expect("read completed cleanup record")
            .expect("completed cleanup record must exist")
            .phase(),
        VulcanPluginCredentialPhase::Discarded
    );
    assert!(
        reopened
            .begin_credential_staging(&generation, revision, &values())
            .is_err()
    );
}

/// Verify exact inventory admission and strict persisted-field validation without backend activity.
/// 验证精确清单准入及严格持久字段校验，不产生后端活动。
#[test]
fn credential_staging_rejects_incomplete_values_and_corrupt_inventory() {
    // Journal corruption stays inside a disposable real filesystem fixture.
    // 日志损坏限定在一次性实际文件系统夹具内。
    let temp = tempfile::tempdir().expect("create corruption fixture");
    // The manifest declares all three keys regardless of their configured presence.
    // 清单声明全部三个键，与其配置存在性无关。
    let (store, generation) = fixture(temp.path());
    // The same identity must remain available after admission rejects a malformed request.
    // 准入拒绝损坏请求后，同一身份必须保持可用。
    let revision = VulcanPluginCredentialRevision::allocate();
    // Omitting an explicit absent slot is different from declaring that slot unconfigured.
    // 省略显式缺失槽位不同于声明该槽位未配置。
    let mut incomplete = values();
    incomplete.remove(&key("optional"));
    assert!(
        store
            .begin_credential_staging(&generation, revision.clone(), &incomplete)
            .is_err()
    );
    assert!(
        store
            .credential_record("staging-plugin", &revision)
            .expect("read absence after rejected admission")
            .is_none()
    );
    drop(
        store
            .begin_credential_staging(&generation, revision.clone(), &values())
            .expect("prepare complete corruption fixture"),
    );
    // Each malformed record is derived from the original valid bytes.
    // 每份损坏记录均从原有效字节派生。
    let path = store
        .root
        .join("credentials/staging-plugin")
        .join(format!("{}.json", revision.as_str()));
    // Keep exact original bytes to test duplicate JSON fields separately from parsed maps.
    // 保留精确原字节，以独立于解析映射测试重复 JSON 字段。
    let original = fs::read_to_string(&path).expect("read valid inventory bytes");
    // Object copies isolate every invalid case from the preceding corruption.
    // 对象副本将每个无效场景与此前损坏隔离。
    let document: serde_json::Value =
        serde_json::from_str(&original).expect("decode original inventory object");
    for field in [
        "schema_version",
        "package",
        "revision",
        "keys",
        "configured_keys",
        "phase",
    ] {
        // Every journal authority field is mandatory, including empty inventories.
        // 每个日志权威字段均必需，包括空清单。
        let mut changed = document.clone();
        changed
            .as_object_mut()
            .expect("inventory must remain an object")
            .remove(field);
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("encode missing-field corruption"),
        )
        .expect("write missing-field corruption");
        assert!(
            store
                .credential_record("staging-plugin", &revision)
                .is_err()
        );
    }
    for (field, invalid) in [
        ("schema_version", serde_json::json!(99)),
        ("phase", serde_json::json!("unknown")),
        ("configured_keys", serde_json::json!([key("undeclared")])),
        ("keys", serde_json::json!([key("first"), key("first")])),
    ] {
        // Unsupported states, extra keys and duplicates must not be normalized into a valid inventory.
        // 不支持状态、额外键及重复项不得规范化为有效清单。
        let mut changed = document.clone();
        changed[field] = invalid;
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("encode invalid inventory field"),
        )
        .expect("write invalid inventory field");
        assert!(
            store
                .credential_record("staging-plugin", &revision)
                .is_err()
        );
    }
    fs::write(
        &path,
        original.replacen('{', "{\"phase\":\"preparing\",", 1),
    )
    .expect("inject duplicate journal field");
    assert!(
        store
            .credential_record("staging-plugin", &revision)
            .is_err()
    );
    fs::write(&path, original).expect("restore original journal bytes");
    assert!(
        store
            .resume_credential_staging("staging-plugin", &revision)
            .is_ok()
    );
}

/// Verify actual selection requires the exact staged package and rejects missing or damaged staging evidence.
/// 验证实际选择要求精确已暂存插件包，并拒绝缺失或损坏的暂存证据。
#[test]
fn credential_staging_is_required_before_selection_and_catalog_pinning() {
    // The real selection record and inventory share one disposable store.
    // 实际选择记录及清单共用单个一次性存储。
    let temp = tempfile::tempdir().expect("create credential selection fixture");
    // Permissions come from the same verified package used by production selection.
    // 权限来自生产选择使用的同一已验证插件包。
    let (store, generation) = fixture(temp.path());
    // This revision starts without any journal or configured backend entries.
    // 此修订开始时没有日志或已配置后端条目。
    let revision = VulcanPluginCredentialRevision::allocate();
    assert!(
        store
            .activate_with_credentials_for_test(
                &generation,
                0,
                generation.required_permissions.clone(),
                revision.clone()
            )
            .is_err()
    );
    assert!(
        store
            .activation("staging-plugin")
            .expect("read unchanged selection after rejection")
            .is_none()
    );
    // Even durable intent alone must not expose an incompletely written candidate.
    // 即使意图已经持久化，也不能暴露尚未完整写入的候选。
    let mut owner = store
        .begin_credential_staging(&generation, revision.clone(), &values())
        .expect("persist candidate selection inventory");
    assert!(
        store
            .activate_with_credentials_for_test(
                &generation,
                0,
                generation.required_permissions.clone(),
                revision.clone()
            )
            .is_err()
    );
    owner
        .write_and_finish(&Backend::default(), &values())
        .expect("complete candidate backend verification");
    drop(owner);
    store
        .activate_with_credentials_for_test(
            &generation,
            0,
            generation.required_permissions.clone(),
            revision.clone(),
        )
        .expect("publish staged credential selection");
    assert!(
        store
            .pin_active("staging-plugin")
            .expect("pin fully staged selected package")
            .generation
            .is_some()
    );
    // Another physical package cannot borrow this exact package's staging approval even when its bytes match.
    // 即使字节匹配，另一物理插件包也不能借用此精确插件包的暂存批准。
    let other = store
        .prepare(
            generation.package_root(),
            &generation.identity.content_fingerprint,
        )
        .expect("prepare distinct physical package");
    assert!(
        store
            .activate_with_credentials_for_test(
                &other,
                1,
                other.required_permissions.clone(),
                revision.clone()
            )
            .is_err()
    );
    // A selected inventory that later becomes corrupt is unavailable, never interpreted as absent.
    // 选定清单随后损坏时不可用，绝不解释为缺失。
    let path = store
        .root
        .join("credentials/staging-plugin")
        .join(format!("{}.json", revision.as_str()));
    // Preserve the actual committed bytes so the fixture can demonstrate explicit restoration.
    // 保留实际已提交字节，使夹具能够证明明确恢复。
    let original = fs::read_to_string(&path).expect("read selected inventory bytes");
    // A syntactically canonical inventory still fails when it silently omits a declared package key.
    // 语法规范的清单若静默遗漏声明插件包键，仍会失败。
    let mut incomplete: serde_json::Value =
        serde_json::from_str(&original).expect("decode selected inventory");
    incomplete["keys"] = serde_json::json!([key("first"), key("second")]);
    fs::write(
        &path,
        serde_json::to_vec(&incomplete).expect("encode incomplete key inventory"),
    )
    .expect("write incomplete key inventory");
    assert!(store.pin_active("staging-plugin").is_err());
    fs::write(&path, original.replace("\"staged\"", "\"preparing\""))
        .expect("inject preparing state into selected journal");
    assert!(store.pin_active("staging-plugin").is_err());
    fs::remove_file(&path).expect("remove selected journal for missing-record validation");
    assert!(store.pin_active("staging-plugin").is_err());
    assert!(
        store
            .begin_credential_staging(&generation, revision, &values())
            .is_err()
    );
    fs::write(&path, original).expect("restore exact selected inventory");
    assert!(
        store
            .pin_active("staging-plugin")
            .expect("pin explicitly restored credential selection")
            .generation
            .is_some()
    );
}
