//! Durable preparation checkpoints wrap real Windows credentials, including partial writes and interrupted publication.
//! 持久准备检查点封装真实 Windows 凭据，包含部分写入及中断发布。

use super::*;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::os::windows::fs::OpenOptionsExt;
use vulcan_plugin_runtime::{
    VulcanPluginCredentialBackend, VulcanPluginCredentialKey, VulcanPluginCredentialPhase,
    VulcanPluginCredentialValues, VulcanPluginGenerationStore,
};

/// Delegate to the actual OS backend while optionally failing before the second credential write.
/// 委托实际 OS 后端，同时可在第二次凭据写入前失败。
struct InterruptedOs {
    /// A deterministic boundary leaves the first actual secure entry present after interruption.
    /// 确定边界使中断后第一个实际安全条目继续存在。
    fail_second: Cell<bool>,
    /// Count only acknowledged actual writes so recovery cannot silently rewrite them.
    /// 仅统计已确认实际写入，使恢复不能静默重写它们。
    writes: Cell<usize>,
}

impl VulcanPluginCredentialBackend for InterruptedOs {
    /// Read the exact tuple through the production OS implementation; return real absence or value.
    /// 经生产 OS 实现读取精确元组；返回实际缺失或值。
    fn read(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        VulcanPluginCredentialBackend::read(&OsPluginMcpSecretStore, plugin, revision, key)
    }
    /// Write through the production backend unless this is the explicit second-key interruption boundary.
    /// 除非此处为明确的第二键中断边界，否则经生产后端写入。
    fn write(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
        value: &str,
    ) -> Result<(), String> {
        if self.fail_second.get() && key.secret_key == "second" {
            return Err("Injected before the second actual OS write.".into());
        }
        VulcanPluginCredentialBackend::write(
            &OsPluginMcpSecretStore,
            plugin,
            revision,
            key,
            value,
        )?;
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }
    /// Delete the exact tuple using actual OS semantics, including idempotent absence.
    /// 使用实际 OS 语义删除精确元组，包含幂等缺失。
    fn delete(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        VulcanPluginCredentialBackend::delete(&OsPluginMcpSecretStore, plugin, revision, key)
    }
}

/// Verify real partial secure writes, failed file publication, same-identity recovery and candidate-only cleanup.
/// 验证真实部分安全写入、文件发布失败、同身份恢复及仅候选清理。
#[test]
fn credential_staging_windows_recovers_real_secure_entries_and_locked_checkpoint() {
    // The real journal and package stay inside a disposable filesystem root.
    // 实际日志及插件包保持在一次性文件系统根内。
    let temp = tempfile::tempdir().expect("create Windows staging fixture");
    // No installed plugin or prior credential can share this randomly generated namespace.
    // 已安装插件或既有凭据不能共享此随机生成命名空间。
    let plugin = format!("staging-{}", uuid::Uuid::new_v4());
    // The package declares two configured slots and one deliberately absent slot.
    // 插件包声明两个已配置槽位及一个刻意缺失槽位。
    let source = temp.path().join("source").join(&plugin);
    fs::create_dir_all(&source).expect("create isolated Windows package");
    fs::write(
        source.join("plugin.yaml"),
        format!(
            r#"schema_version: 1
id: {plugin}
version: 1.0.0
default_enabled: false
display:
  name: Windows staging fixture
  capability: Real secure entry recovery
permissions: [network_https, secrets_read]
mcp:
  servers:
    - id: remote
      name: Remote
      transport:
        type: http
        url: {{source: literal, value: 'https://example.test/mcp'}}
        headers:
          X-First: {{source: secret, key: first}}
          X-Second: {{source: secret, key: second}}
          X-Optional: {{source: secret, key: optional}}
"#
        ),
    )
    .expect("write Windows staging manifest");
    // The authoritative package inspector derives the actual content fingerprint.
    // 权威插件包检查器派生实际内容指纹。
    let inspected = vulcan_plugin_runtime::inspect_user_plugin_package(&source)
        .expect("inspect Windows staging package");
    // Filesystem preparation uses the same store later reopened for interrupted recovery.
    // 文件系统准备使用后续中断恢复重开的相同存储。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open Windows staging store");
    // A real package lease stays live throughout both credential preparations.
    // 实际插件包租约在两次凭据准备期间持续存活。
    let generation = store
        .prepare(&source, &inspected.content_fingerprint)
        .expect("prepare Windows immutable package");
    // Candidate A will reach durable Staged after a failed checkpoint replacement.
    // 候选 A 在检查点替换失败后最终达到持久 Staged。
    let first = VulcanPluginCredentialRevision::allocate();
    // Candidate B is explicitly abandoned after its first actual secure write.
    // 候选 B 在第一次实际安全写入后被明确放弃。
    let abandoned = VulcanPluginCredentialRevision::allocate();
    // Every value is a disposable fixture string and is never serialized into the journal.
    // 每个值均为一次性夹具字符串，绝不序列化进日志。
    let values: VulcanPluginCredentialValues = BTreeMap::from([
        (
            VulcanPluginCredentialKey {
                server_id: "remote".into(),
                secret_key: "first".into(),
            },
            Some("fixture-staging-first".into()),
        ),
        (
            VulcanPluginCredentialKey {
                server_id: "remote".into(),
                secret_key: "second".into(),
            },
            Some("fixture-staging-second".into()),
        ),
        (
            VulcanPluginCredentialKey {
                server_id: "remote".into(),
                secret_key: "optional".into(),
            },
            None,
        ),
    ]);
    // Acquire cleanup handles before the first write so a panic still attempts to remove every test entry.
    // 首次写入前取得清理句柄，使恐慌时仍尝试移除每个测试条目。
    let mut cleanup = SecureFixtureEntries {
        entries: Vec::new(),
    };
    for revision in [&first, &abandoned] {
        for key in values.keys() {
            cleanup.entries.push(
                versioned_plugin_secret_entry(&plugin, &key.server_id, &key.secret_key, revision)
                    .expect("open isolated versioned cleanup handle"),
            );
        }
    }
    // The legacy account remains populated to prove candidate cleanup cannot address it.
    // 旧账户保持有值，以证明候选清理无法寻址它。
    let legacy = plugin_secret_entry(&plugin, "remote", "first")
        .expect("open isolated legacy cleanup handle");
    cleanup.entries.push(legacy);
    for entry in &cleanup.entries {
        assert!(matches!(entry.get_password(), Err(KeyringError::NoEntry)));
    }
    OsPluginMcpSecretStore
        .write_secret(&plugin, "remote", "first", "fixture-legacy-retained")
        .expect("write retained legacy fixture");
    // Actual OS writes are counted separately from journal checkpoint attempts.
    // 实际 OS 写入与日志检查点尝试分别计数。
    let backend = InterruptedOs {
        fail_second: Cell::new(true),
        writes: Cell::new(0),
    };
    // The original owner must persist its full inventory before the backend's first side effect.
    // 原所有者必须在后端首次副作用前持久保存完整清单。
    let mut owner = store
        .begin_credential_staging(&generation, first.clone(), &values)
        .expect("persist original OS candidate inventory");
    assert!(owner.write_and_finish(&backend, &values).is_err());
    assert_eq!(backend.writes.get(), 1);
    drop(owner);
    drop(store);
    // Discover and resume the original durable record through a fresh store handle.
    // 通过新存储句柄发现并恢复原持久记录。
    let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen interrupted OS journal")
        .expect("OS journal store must still exist");
    assert!(
        reopened
            .credential_revisions(&plugin)
            .expect("discover interrupted OS revision")
            .contains(&first)
    );
    backend.fail_second.set(false);
    // Denying delete sharing causes a real Windows atomic checkpoint replacement to fail after OS readback.
    // 拒绝删除共享，使实际 Windows 原子检查点替换在 OS 回读后失败。
    let record_path = temp
        .path()
        .join(".vulcan-generations/credentials")
        .join(&plugin)
        .join(format!("{}.json", first.as_str()));
    // Reads and writes remain shared; replacement is the specific denied filesystem operation.
    // 读取及写入仍共享；替换是被明确拒绝的文件系统操作。
    let occupied = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&record_path)
        .expect("occupy checkpoint without delete sharing");
    // Resuming uses the existing first value and writes only the missing second value.
    // 恢复使用既有第一个值，仅写入缺失第二个值。
    let mut resumed = reopened
        .resume_credential_staging(&plugin, &first)
        .expect("resume original OS candidate");
    assert!(resumed.write_and_finish(&backend, &values).is_err());
    assert_eq!(backend.writes.get(), 2);
    assert_eq!(
        reopened
            .credential_record(&plugin, &first)
            .expect("read checkpoint after failed replacement")
            .expect("original preparing checkpoint must exist")
            .phase(),
        VulcanPluginCredentialPhase::Preparing
    );
    drop(occupied);
    assert_eq!(
        resumed
            .write_and_finish(&backend, &values)
            .expect("finish checkpoint after releasing occupied file")
            .phase(),
        VulcanPluginCredentialPhase::Staged
    );
    assert_eq!(backend.writes.get(), 2);
    drop(resumed);
    // Abandoned B must be reclaimed through its own persisted key inventory, leaving A and legacy untouched.
    // 放弃的 B 必须经自身持久键清单回收，保持 A 及旧账户不变。
    let mut abandoned_owner = reopened
        .begin_credential_staging(&generation, abandoned.clone(), &values)
        .expect("prepare independently abandoned revision");
    backend.fail_second.set(true);
    assert!(abandoned_owner.write_and_finish(&backend, &values).is_err());
    drop(abandoned_owner);
    assert!(
        reopened
            .discard_credential_staging(&plugin, &abandoned, &backend)
            .expect("discard only abandoned revision")
    );
    for (key, expected) in &values {
        assert_eq!(
            backend
                .read(&plugin, &first, key)
                .expect("read retained staged OS value")
                .as_ref(),
            expected.as_ref()
        );
        assert!(
            backend
                .read(&plugin, &abandoned, key)
                .expect("verify abandoned OS entry is absent")
                .is_none()
        );
    }
    // A real shared reader and its clone retain staged OS entries until their final release.
    // 实际共享读者及其克隆保留已暂存 OS 条目，直至最后释放。
    let reader = reopened
        .pin_credentials(&generation, &first)
        .expect("pin real staged OS credentials");
    // A separate wrapper must keep the exact same filesystem retention after the original is dropped.
    // 原对象释放后，独立包装对象必须保留完全相同的文件系统所有权。
    let retained = reader.clone();
    drop(reader);
    assert_eq!(
        reopened
            .collect_credentials(&plugin, &first, &backend)
            .expect_err("real reader must block OS credential deletion")
            .code,
        vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy
    );
    for (key, expected) in &values {
        assert_eq!(
            backend
                .read(&plugin, &first, key)
                .expect("verify blocked cleanup preserved OS entries")
                .as_ref(),
            expected.as_ref()
        );
    }
    drop(retained);
    assert!(
        reopened
            .collect_credentials(&plugin, &first, &backend)
            .expect("collect staged OS entries after final reader")
    );
    for key in values.keys() {
        assert!(
            backend
                .read(&plugin, &first, key)
                .expect("verify collected OS entry absence")
                .is_none()
        );
    }
    assert_eq!(
        OsPluginMcpSecretStore
            .read_optional_secret(&plugin, "remote", "first")
            .expect("read unchanged legacy fixture")
            .as_deref(),
        Some("fixture-legacy-retained")
    );
    assert!(
        !fs::read_to_string(record_path)
            .expect("read final non-secret OS journal")
            .contains("fixture-staging")
    );
    for entry in &cleanup.entries {
        assert!(matches!(
            entry.delete_credential(),
            Ok(()) | Err(KeyringError::NoEntry)
        ));
        assert!(matches!(entry.get_password(), Err(KeyringError::NoEntry)));
    }
}
