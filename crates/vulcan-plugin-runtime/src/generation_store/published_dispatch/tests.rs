//! Independent real stores and original reservations exercise the ordinary dispatch namespace fence.
//! 独立真实存储及原保留权验证普通派发命名空间屏障。

use super::*;
use std::fs;
use uuid::Uuid;

/// Reject unexpected secret operations for this genuinely empty credential revision fixture.
/// 为此真实空凭据修订夹具拒绝非预期秘密操作。
struct EmptyBackend;

impl VulcanPluginCredentialBackend for EmptyBackend {
    /// Reject plugin_id/revision/key reads; return an error because the fixture declares no secret keys.
    /// 拒绝 plugin_id／revision／key 读取；夹具未声明秘密键，返回错误。
    fn read(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> std::result::Result<Option<String>, String> {
        Err("Unexpected empty-fixture secret read.".into())
    }
    /// Reject plugin_id/revision/key/value writes; return an error without retaining values.
    /// 拒绝 plugin_id／revision／key／value 写入；返回错误，不保留值。
    fn write(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
        _: &str,
    ) -> std::result::Result<(), String> {
        Err("Unexpected empty-fixture secret write.".into())
    }
    /// Reject plugin_id/revision/key deletion; return an error because no secure entry belongs to this fixture.
    /// 拒绝 plugin_id／revision／key 删除；没有安全条目属于此夹具，返回错误。
    fn delete(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> std::result::Result<(), String> {
        Err("Unexpected empty-fixture secret delete.".into())
    }
}

/// Actual production selections and pins share one isolated filesystem namespace.
/// 实际生产选择及固定句柄共享单个隔离文件系统命名空间。
struct Fixture {
    /// Canonical store owns every selection operation.
    /// 规范存储拥有每个选择操作。
    store: VulcanPluginGenerationStore,
    /// Real package pin is passed to production activation writers.
    /// 真实插件包固定传入生产激活写入者。
    package: PinnedVulcanPluginGeneration,
    /// Actual completed empty credential revision is bound to this package.
    /// 实际完成的空凭据修订绑定至此插件包。
    credentials: VulcanPluginCredentialRevision,
    /// Complete original selection comes from the actual writer.
    /// 完整原选择来自实际写入者。
    selected: VulcanPluginActivation,
    /// Delete isolated files after all owning handles have retired.
    /// 全部拥有句柄退役后删除隔离文件。
    _temp: tempfile::TempDir,
}

/// Build a real version-four selection with an empty revision and no declared configuration.
/// 构建带空修订且未声明配置的真实第四版选择。
/// No inputs; returns production pins, selection and canonical store without fabricated journal bytes.
/// 无输入；返回生产固定、选择及规范存储，不伪造日志字节。
fn fixture() -> Fixture {
    // Independent actual files isolate all namespace and credential leases.
    // 独立实际文件隔离全部命名空间及凭据租约。
    let temp = tempfile::tempdir().expect("published dispatch fixture");
    // The package declares only an inert remote endpoint; no process is started.
    // 插件包仅声明惰性远端端点；不启动进程。
    let source = temp.path().join("source/fenced-mcp");
    fs::create_dir_all(&source).expect("source");
    fs::write(source.join("plugin.yaml"), "schema_version: 1\nid: fenced-mcp\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Fence\n  capability: Fence test\npermissions: [network_https]\nmcp:\n  servers:\n    - id: remote\n      name: Remote\n      transport:\n        type: http\n        url: {source: literal, value: 'https://example.test/mcp'}\n").expect("manifest");
    // Production package inspection supplies the sole exact digest and permission authority.
    // 生产插件包检查提供唯一精确摘要及权限权威。
    let inspected = crate::inspect_user_plugin_package(&source).expect("inspect");
    // This store is separately reopened by competing readers and writers in the tests.
    // 此存储由测试中的竞争读取者及写入者独立重开。
    fs::create_dir_all(temp.path().join("user")).expect("existing user root");
    let store = VulcanPluginGenerationStore::open(&temp.path().join("user")).expect("store");
    // Retain the actual generation File rather than copying a generation identifier.
    // 保留实际代次 File，而非复制代次标识。
    let package = store
        .prepare(&source, &inspected.content_fingerprint)
        .expect("package");
    // Empty is an explicit fully completed revision, never an unconfigured fallback.
    // 空值是显式完整完成的修订，绝不是未配置回退。
    let credentials = VulcanPluginCredentialRevision::allocate();
    store
        .begin_credential_staging(
            &package,
            credentials.clone(),
            &VulcanPluginCredentialValues::new(),
        )
        .expect("stage")
        .write_and_finish(&EmptyBackend, &VulcanPluginCredentialValues::new())
        .expect("complete revision");
    // Complete configuration absence is published atomically with the actual package and revision.
    // 完整配置缺失与实际插件包及修订原子发布。
    let selected = store
        .activate_configured(
            &package,
            0,
            vec!["network_https".into()],
            credentials.clone(),
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("selected");
    Fixture {
        store,
        package,
        credentials,
        selected,
        _temp: temp,
    }
}

/// Hold a real shared fence against an independent store writer, then release it and permit the same writer.
/// 对独立存储写入者持有真实共享屏障，随后释放并允许同一写入者。
/// No inputs or return value; also proves pinning does not reopen a self-busy namespace.
/// 无输入或返回值；同时证明固定不会重开自身繁忙命名空间。
#[test]
fn published_dispatch_shared_fence_excludes_independent_writer_until_drop() {
    // Use two production store objects over the same canonical files.
    // 在同一规范文件上使用两个生产存储对象。
    let fixture = fixture();
    // This independent store has no shared Rust lock with the original fence.
    // 此独立存储与原屏障没有共享 Rust 锁。
    let independent =
        VulcanPluginGenerationStore::open_existing(&fixture._temp.path().join("user"))
            .expect("open")
            .expect("existing");
    // Keep the exact namespace File live while acquiring package and credential pins.
    // 获取插件包及凭据固定期间，使精确命名空间 File 保持存活。
    let fence = fixture
        .store
        .acquire_published_dispatch_fence(&fixture.selected)
        .expect("fence");
    assert!(
        fence
            .pin_selected()
            .expect("same fence pins")
            .credentials
            .is_some()
    );
    // The competing writer must report its actual OS lock failure.
    // 竞争写入者必须报告实际操作系统锁失败。
    let blocked = independent
        .activate_configured(
            &fixture.package,
            fixture.selected.revision,
            vec!["network_https".into()],
            fixture.credentials.clone(),
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect_err("held shared File must exclude writer");
    assert_eq!(blocked.code, Code::Busy);
    drop(fence);
    independent
        .activate_configured(
            &fixture.package,
            fixture.selected.revision,
            vec!["network_https".into()],
            fixture.credentials.clone(),
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("same writer after actual fence release");
}

/// Reject a frozen full selection after an actual independent writer publishes its successor.
/// 实际独立写入者发布后继选择后，拒绝固定完整选择。
/// No inputs or return value; changes are generated by the real activation writer, not hand-edited JSON.
/// 无输入或返回值；变更由真实激活写入者生成，不手工修改 JSON。
#[test]
fn published_dispatch_rejects_real_changed_full_selection() {
    // Preserve the original exact record before a real successor publication.
    // 在真实后继发布前保留原精确记录。
    let fixture = fixture();
    fixture
        .store
        .activate_configured(
            &fixture.package,
            fixture.selected.revision,
            vec!["network_https".into()],
            fixture.credentials.clone(),
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("actual successor");
    // A changed full record cannot be accepted merely because package and credentials still match.
    // 即使插件包及凭据仍匹配，也不能接受已改变的完整记录。
    let rejected = fixture
        .store
        .acquire_published_dispatch_fence(&fixture.selected)
        .err()
        .expect("changed selection rejected");
    assert_eq!(rejected.code, Code::Conflict);
}

/// Keep Pending and Transferred admission closed until the same real independent predecessor lease retires.
/// 使 Pending 及 Transferred 入场保持关闭，直至同一真实独立前驱租约退役。
/// No inputs or return value; test-only transfer authority borrows the real original reservation.
/// 无输入或返回值；仅测试移交权威借用真实原保留权。
#[test]
fn published_dispatch_observes_pending_and_live_terminal_handoff_lease() {
    // The old version-four selection owns the actual managed predecessor pins.
    // 旧第四版选择持有实际受管前驱固定。
    let fixture = fixture();
    // Create a genuine prepared successor; old Enabled business would otherwise remain admissible.
    // 创建真实已准备后继；否则旧 Enabled 业务仍可入场。
    let mut transaction = fixture
        .store
        .begin_activation_transaction(
            Uuid::new_v4(),
            &fixture.package,
            fixture.selected.revision,
            vec!["network_https".into()],
            fixture.credentials.clone(),
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("prepared original");
    // Freeze the actual original record before allocating an independent native reservation File.
    // 分配独立原生保留权 File 前固定实际原记录。
    let original = transaction.record().clone();
    // The production reservation retains real old package and credential ownership.
    // 生产保留权保留真实旧插件包及凭据所有权。
    let mut reservation = fixture
        .store
        .prepare_predecessor_handoff_reservation(&mut transaction, &original)
        .expect("reservation");
    fixture
        .store
        .persist_predecessor_handoff_reservation(&mut reservation)
        .expect("Pending");
    {
        // Observe persisted Pending without owning or reconstructing the original reservation.
        // 观测持久 Pending，不拥有或重建原保留权。
        let fence = fixture
            .store
            .acquire_published_dispatch_fence(&fixture.selected)
            .expect("Pending observation");
        assert!(fence.handoff_blocked());
    }
    fixture
        .store
        .abort_activation_predecessor_restoration_owned(&mut transaction, &original)
        .expect("original owned restoration abort");
    {
        // This fixture capability models completed composite guards while retaining the actual same File.
        // 此夹具能力在保留实际同一 File 时模拟已完成组合守卫。
        let transfer = fixture
            .store
            .predecessor_handoff_transfer_test_authority(&reservation);
        fixture
            .store
            .complete_predecessor_handoff_owned(&transfer)
            .expect("Transferred");
    }
    {
        // Terminal metadata alone does not retire the original native lease.
        // 仅终态元数据不能退役原原生租约。
        let fence = fixture
            .store
            .acquire_published_dispatch_fence(&fixture.selected)
            .expect("terminal observation");
        assert!(fence.handoff_blocked());
    }
    drop(reservation);
    // Only actual physical File release opens ordinary admission.
    // 仅实际物理 File 释放打开普通入场。
    let released = fixture
        .store
        .acquire_published_dispatch_fence(&fixture.selected)
        .expect("released observation");
    assert!(!released.handoff_blocked());
}
