//! Parent-bound secure writes retain the original request across partial failure and exact recovery.
//! 父绑定安全写入在部分失败及精确恢复期间保留原请求。

use super::*;
use crate::{VulcanPluginActivationTransactionPhase as Phase, VulcanPluginConfigurationSelection};

/// Partial backend effects retain their parent, reject promotion while owned, and resume without replaying successful keys.
/// 部分后端副作用保留父级，在仍有所有者时拒绝晋级，并在恢复时不重放成功键。
#[test]
fn activation_preparation_partial_credentials_recover_under_original_parent() {
    // The existing fixture owns genuine package bytes and the deterministic secure backend.
    // 既有夹具拥有真实插件包字节及确定性安全后端。
    let temp = tempfile::tempdir().expect("fixture");
    // This extra original pin is unrelated to the new transaction's reserved generation.
    // 此额外原固定句柄与新事务预留代次无关。
    let (store, initial) = fixture(temp.path());
    drop(initial);
    // The authoritative source path is created by fixture, not discovered through fallback paths.
    // 权威来源路径由 fixture 创建，不通过备用路径发现。
    let source = temp.path().join("source/staging-plugin");
    // Actual inspection derives the approved content and complete permission set.
    // 实际检查派生已批准内容及完整权限集合。
    let descriptor = crate::inspect_user_plugin_package(&source).expect("source");
    // This UUID remains the sole parent before and after interruption.
    // 此 UUID 在中断前后始终为唯一父级。
    let request_id = uuid::Uuid::new_v4();
    // No candidate allocation or secure write precedes this original durable intent.
    // 此原持久意图先于任何候选分配或安全写入。
    let preparation = store
        .begin_activation_preparation(
            request_id,
            &source,
            &descriptor.content_fingerprint,
            None,
            crate::runtime::plugin_required_permissions(&descriptor).expect("permissions"),
        )
        .expect("original intent");
    // Both identity fields remain fixed independently of actual ownership.
    // 两个身份字段独立于实际所有权保持固定。
    let revision = preparation.intent().credential_revision().clone();
    // The copied package is exactly the allocation frozen by original intent.
    // 已复制插件包精确对应原意图固定的分配。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("copy");
    // The writer persists its parent UUID and retains its actual request lock.
    // 写入者持久保存父 UUID，并保留实际请求锁。
    let mut writer = store
        .begin_activation_credential_staging(&preparation, &package, &values())
        .expect("writer");
    // Failure on the second configured value happens after a genuine first backend write.
    // 第二个已配置值的失败发生于真实首个后端写入之后。
    let backend = Backend::default();
    *backend.fail_write.borrow_mut() = Some("second".into());
    // Public diagnostics cannot retain deliberately secret-bearing backend failures.
    // 公开诊断不能保留后端故意含秘密的错误。
    let error = writer
        .write_and_finish(&backend, &values())
        .expect_err("partial failure");
    assert!(!error.to_string().contains("fixture-sensitive"));
    assert_eq!(backend.writes.get(), 1);
    assert_eq!(
        store
            .activation_request("staging-plugin", request_id)
            .expect("original")
            .expect("record")
            .phase(),
        Phase::Preparing
    );
    assert_eq!(
        store
            .finish_activation_preparation(
                preparation,
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .expect_err("live writer blocks promotion")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        store
            .abort_activation_preparation_request("staging-plugin", request_id)
            .expect_err("writer retains parent")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(writer);
    drop(package);
    drop(store);
    // A new store reads exact original files after every foreground owner has retired.
    // 全部前台所有者退役后，新存储读取精确原文件。
    let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen")
        .expect("store");
    assert_eq!(
        reopened
            .discard_credential_staging("staging-plugin", &revision, &backend)
            .expect_err("durable pending intent retains partial values")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    // Both journals disclose identities only and never serialize the values supplied to the backend.
    // 两份日志仅披露身份，绝不序列化传给后端的值。
    for path in [
        reopened
            .root
            .join("transactions/staging-plugin")
            .join(format!("{request_id}.json")),
        reopened
            .root
            .join("credentials/staging-plugin")
            .join(format!("{}.json", revision.as_str())),
    ] {
        assert!(
            !fs::read_to_string(path)
                .expect("journal")
                .contains("fixture-sensitive")
        );
    }
    // Revision recovery reacquires the declared original parent's lock before any further backend access.
    // 修订恢复在继续访问后端前重新取得声明原父级的锁。
    let mut resumed = reopened
        .resume_credential_staging("staging-plugin", &revision)
        .expect("resume writer");
    assert_eq!(resumed.record().activation_request(), Some(request_id));
    assert_eq!(
        reopened
            .abort_activation_preparation_request("staging-plugin", request_id)
            .expect_err("resumed writer excludes abandon")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    *backend.fail_write.borrow_mut() = None;
    resumed
        .write_and_finish(&backend, &values())
        .expect("finish exact original values");
    assert_eq!(backend.writes.get(), 2);
    drop(resumed);
    // Full readiness is possible only after actual secure work relinquishes the original ownership.
    // 仅实际安全工作放弃原所有权后，才可能达到完整就绪。
    let original = reopened
        .resume_activation_preparation("staging-plugin", request_id)
        .expect("resume preparation");
    // The configured transaction inherits the same UUID and physical locks without a release gap.
    // 已配置事务无释放间隙地继承相同 UUID 及物理锁。
    let transaction = reopened
        .finish_activation_preparation(original, VulcanPluginConfigurationSelection::NotDeclared {})
        .expect("promote");
    assert_eq!(transaction.record().request_id(), request_id);
    assert_eq!(transaction.record().phase(), Phase::Prepared);
    assert_eq!(backend.writes.get(), 2);
}
