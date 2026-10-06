//! Real atomic removal outcomes retain the original namespace and its exact cleanup identities.
//! 真实原子移除结果保留原命名空间及其精确清理身份。

use super::*;
use crate::generation_store::deactivation::observation;
use crate::{
    VulcanPluginDeactivationObservation as Observation, VulcanPluginDeactivationReservation,
};
use uuid::Uuid;

mod collection;
mod inventory;

/// An actual source, immutable package, approved selection and independent store exercise physical namespace exclusion.
/// 实际来源、不可变插件包、已批准选择及独立存储验证物理命名空间互斥。
struct Fixture {
    /// Keep all actual files alive until the original ownership has been released.
    /// 保持全部实际文件存活，直至原所有权已释放。
    _temp: tempfile::TempDir,
    /// The actual producer store retains the canonical original root.
    /// 实际生产者存储保留规范原根。
    store: VulcanPluginGenerationStore,
    /// A separately opened store proves exclusion without sharing in-memory registry state.
    /// 单独打开的存储证明互斥，不共享内存注册表状态。
    contender: VulcanPluginGenerationStore,
    /// Exact complete selection comes from the actual approved writer.
    /// 精确完整选择来自实际已批准写入者。
    selected: VulcanPluginActivation,
    /// The immutable package's real lease permits verifying that removal never deletes its bytes.
    /// 不可变插件包实际租约允许验证移除绝不删除其字节。
    package: PinnedVulcanPluginGeneration,
}

impl Fixture {
    /// Create an actual approved immutable selection; returns only real producer values, not hand-built activation records.
    /// 创建实际已批准不可变选择；仅返回真实生产者值，而非手写激活记录。
    fn new() -> Self {
        // Own both the selected store and its independent source tree.
        // 拥有所选存储及其独立来源树。
        let temp = tempfile::tempdir().expect("original removal fixture");
        // Both store instances open the same actual canonical namespace.
        // 两个存储实例打开相同实际规范命名空间。
        let store = VulcanPluginGenerationStore::open(temp.path()).expect("actual original store");
        // The valid inspected source carries actual permission requirements.
        // 有效已检查来源携带实际权限要求。
        let source = write_package(
            &temp.path().join("source"),
            "sample-plugin",
            "original removal source",
        );
        // The real generation owns its immutable bytes independently from selection.
        // 真实代次独立于选择拥有其不可变字节。
        let package = prepare(&store, &source);
        // The sole approval producer freezes complete authorization before removal.
        // 唯一批准生产者在移除之前固定完整授权。
        let selected = store
            .activate_for_test(&package, 0, package.required_permissions.clone())
            .expect("actual approved selection");
        // An independent actual instance has no access to the original owner's File.
        // 独立实际实例不能访问原所有者的文件。
        let contender =
            VulcanPluginGenerationStore::open(temp.path()).expect("actual independent store");
        Self {
            _temp: temp,
            store,
            contender,
            selected,
            package,
        }
    }

    /// Reserve one actual original UUID against the current complete selection; returns its exclusive File owner.
    /// 对照当前完整选择预留单个实际原 UUID；返回其排他文件所有者。
    fn reserve(&self) -> VulcanPluginDeactivationReservation {
        self.store
            .reserve_deactivation(Uuid::new_v4(), &self.selected)
            .expect("actual original removal reservation")
    }

    /// Require the actual independent reader to fail Busy while this original owns the namespace.
    /// 此原实例拥有命名空间期间，要求实际独立读取者以忙碌失败。
    fn assert_busy(&self) {
        assert_eq!(
            self.contender
                .activation("sample-plugin")
                .expect_err("same actual file excludes independent readers")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
    }
}

/// Reservation changes no selection, package or callback state, while exclusion lasts until actual original drop.
/// 预留不改变选择、插件包或回调状态，而互斥持续至实际原实例释放。
#[test]
fn retained_deactivation_reserves_before_effects_and_keeps_the_original_file() {
    // Every observation uses the same real selected package fixture.
    // 每个观测使用相同真实已选插件包夹具。
    let fixture = Fixture::new();
    // The raw bytes are evidence, not a reconstructed transaction or alternate selection authority.
    // 原始字节为证据，不是重建事务或替代选择权威。
    let before = fs::read(fixture.store.root.join("active/sample-plugin.json"))
        .expect("original selected bytes");
    // Original ownership is registered conceptually before any effect, without cloning or consuming it.
    // 原所有权在任何效果之前取得，不克隆或消费它。
    let original = fixture.reserve();
    assert_eq!(original.original(), &fixture.selected);
    assert_eq!(original.intended().revision, fixture.selected.revision + 1);
    assert!(original.intended().generation.is_none());
    assert!(original.intended().approved_permissions().is_empty());
    assert_eq!(
        fs::read(fixture.store.root.join("active/sample-plugin.json"))
            .expect("unchanged selected bytes"),
        before
    );
    assert_eq!(
        fixture
            .store
            .reconcile_deactivation_retained(&original)
            .expect("read original through same file"),
        Observation::Original
    );
    fixture.assert_busy();
    assert!(fixture.package.package_root().join("plugin.yaml").is_file());
    drop(original);
    assert_eq!(
        fixture
            .contender
            .activation("sample-plugin")
            .expect("released original file"),
        Some(fixture.selected)
    );
}

/// Actual completed-write return loss and panic preserve the same original and recover without another write or revision.
/// 实际写完返回丢失及 panic 保留相同原实例，恢复不产生另一写入或修订。
#[test]
fn retained_deactivation_completed_write_recovers_without_replaying_the_producer() {
    for panic_after_write in [false, true] {
        // Independent actual files and original authority isolate both failure modes.
        // 独立实际文件及原权威隔离两种失败模式。
        let fixture = Fixture::new();
        // This concrete owner survives actual failure and the following same-owner reconciliation.
        // 此具体所有者跨实际失败及随后同所有者核对存续。
        let mut original = fixture.reserve();
        // Count only actual successful writes for this exact root and original UUID.
        // 仅计算此精确根及原 UUID 的实际成功写入。
        let scope = observation::arm(
            &fixture.store.root,
            original.request_id(),
            panic_after_write,
        );
        if panic_after_write {
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fixture
                    .store
                    .publish_deactivation_retained(&mut original)))
                .is_err()
            );
        } else {
            assert_eq!(
                fixture
                    .store
                    .publish_deactivation_retained(&mut original)
                    .expect_err("actual completed write loses foreground result")
                    .code,
                VulcanPluginStoreErrorCode::Storage
            );
        }
        assert_eq!(scope.writes(), 1);
        fixture.assert_busy();
        assert_eq!(
            fixture
                .store
                .reconcile_deactivation_retained(&original)
                .expect("actual original target readback"),
            Observation::Published
        );
        assert_eq!(
            fixture
                .store
                .continue_deactivation_retained(&mut original)
                .expect("same actual admitted intent continues read-only completion"),
            Observation::Published
        );
        assert_eq!(scope.writes(), 1);
        assert!(
            fixture
                .store
                .publish_deactivation_retained(&mut original)
                .is_err()
        );
        assert_eq!(scope.writes(), 1);
        // Preserve the exact intended value as evidence only after all original recovery operations.
        // 仅在全部原恢复操作之后保存精确期望值作为证据。
        let intended = original.intended().clone();
        drop(original);
        assert_eq!(
            fixture
                .contender
                .activation("sample-plugin")
                .expect("actual selected tombstone"),
            Some(intended)
        );
        assert!(fixture.package.package_root().join("plugin.yaml").is_file());
    }
}

/// A real prewrite path obstruction keeps publication unadmitted and allows only the original first start after resolution.
/// 真实写前路径阻碍保持发布未准入，仅在解决之后允许原首次启动。
#[test]
fn retained_deactivation_prewrite_conflict_preserves_unstarted_original() {
    // The plain fixture remains wholly inside the isolated user root.
    // 普通夹具完整位于隔离用户根内。
    let fixture = Fixture::new();
    // Original owner retains its fixed successor through actual path obstruction.
    // 原所有者跨实际路径阻碍保留其固定后继。
    let mut original = fixture.reserve();
    // Obstruct only the exact actual activation file while keeping its original bytes available for restoration.
    // 仅阻碍精确实际激活文件，同时保留其原字节供恢复。
    let path = fixture.store.root.join("active/sample-plugin.json");
    // This independent evidence snapshot is never passed as recovery authority.
    // 此独立证据快照绝不作为恢复权威传入。
    let bytes = fs::read(&path).expect("original physical record");
    fs::remove_file(&path).expect("remove isolated record to obstruct writer");
    fs::create_dir(&path).expect("actual obstruction");
    assert!(
        fixture
            .store
            .publish_deactivation_retained(&mut original)
            .is_err()
    );
    fixture.assert_busy();
    // Before-write validation refused the original mismatch, so publication was never admitted.
    // 写前校验拒绝原记录不匹配，因此发布从未准入。
    assert!(
        fixture
            .store
            .resume_deactivation_retained(&mut original)
            .is_err()
    );
    fs::remove_dir(&path).expect("remove exact empty obstruction");
    fs::write(&path, bytes).expect("restore actual original");
    assert_eq!(
        fixture
            .store
            .reconcile_deactivation_retained(&original)
            .expect("same restored original observation"),
        Observation::Original
    );
    assert_eq!(
        fixture
            .store
            .publish_deactivation_retained(&mut original)
            .expect("original first admitted publication"),
        Observation::Published
    );
}

/// Full original equality, not merely revision, is required and foreign records are never replaced during recovery.
/// 要求完整原记录相等，而非仅修订一致，恢复绝不替换外部记录。
#[test]
fn retained_deactivation_refuses_foreign_complete_selection_and_wrong_store() {
    // Two real stores have independent complete selections and must never share removal authority.
    // 两个真实存储具有独立完整选择，绝不能共享移除权威。
    let fixture = Fixture::new();
    // The unrelated store uses the same plugin name to expose root-identity errors independently from string identity.
    // 无关存储使用相同插件名，使根身份错误独立于字符串身份暴露。
    let other = Fixture::new();
    // A real different selection is produced by the actual writer, never assembled by a recovery helper.
    // 真实不同选择由实际写入者生产，绝不由恢复辅助方法拼装。
    let first_foreign = other
        .store
        .deactivate("sample-plugin", other.selected.revision)
        .expect("actual unrelated tombstone");
    // A second real producer revision differs from the original owner's one frozen successor.
    // 第二个真实生产者修订不同于原所有者单个固定后继。
    let foreign = other
        .store
        .deactivate("sample-plugin", first_foreign.revision)
        .expect("actual distinct foreign tombstone");
    assert_eq!(fixture.selected.revision, other.selected.revision);
    assert_eq!(
        fixture
            .store
            .reserve_deactivation(Uuid::new_v4(), &other.selected)
            // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
            // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
            .map(|_| ())
            .expect_err("same revision does not authorize different original generation")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    // The original owns the actual namespace until all conflict evidence has been asserted.
    // 原实例拥有实际命名空间，直至全部冲突证据已断言。
    let mut original = fixture.reserve();
    assert_eq!(
        other
            .store
            .publish_deactivation_retained(&mut original)
            .expect_err("another root has no original authority")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    // External replacement is deliberately noncooperating; actual record readback must reject its different bindings.
    // 外部替换刻意不协作；实际记录回读必须拒绝其不同绑定。
    let path = fixture.store.root.join("active/sample-plugin.json");
    fs::write(
        &path,
        serde_json::to_vec(&foreign).expect("actual producer record evidence"),
    )
    .expect("actual external replacement");
    assert_eq!(
        fixture
            .store
            .reconcile_deactivation_retained(&original)
            .expect_err("unadmitted foreign tombstone")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert!(
        fixture
            .store
            .publish_deactivation_retained(&mut original)
            .is_err()
    );
    assert_eq!(
        fs::read(&path).expect("preserved foreign record"),
        serde_json::to_vec(&foreign).expect("foreign bytes")
    );
    fixture.assert_busy();
}

/// Reservation rejects absent intent identities, stale complete originals and already deactivated selections.
/// 预留拒绝缺失意图身份、陈旧完整原记录及已经停用选择。
#[test]
fn retained_deactivation_rejects_invalid_request_and_inactive_selection_before_effects() {
    // Actual setup publishes its complete approved active record first.
    // 实际设置先发布完整已批准有效记录。
    let fixture = Fixture::new();
    assert!(
        fixture
            .store
            .reserve_deactivation(Uuid::nil(), &fixture.selected)
            .is_err()
    );
    // The real deactivation writer creates a valid inactive record, preserving history.
    // 真实停用写入者创建有效非活动记录，保留历史。
    let removed = fixture
        .store
        .deactivate("sample-plugin", fixture.selected.revision)
        .expect("actual prior removal");
    assert!(
        fixture
            .store
            .reserve_deactivation(Uuid::new_v4(), &removed)
            .is_err()
    );
    assert_eq!(
        fixture
            .store
            .reserve_deactivation(Uuid::new_v4(), &fixture.selected)
            // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
            // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
            .map(|_| ())
            .expect_err("stale complete active record")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(
        fixture
            .store
            .activation("sample-plugin")
            .expect("original tombstone unchanged"),
        Some(removed)
    );
}

/// Actual configured removal preserves the original credential and configuration cleanup authorities without exposing values in Debug.
/// 实际已配置移除保留原凭据及配置清理权威，不在调试输出中暴露值。
#[test]
fn retained_deactivation_keeps_complete_configured_cleanup_bindings() {
    // Share the existing real schema and empty-credential producer, avoiding a second fixture authority.
    // 共享既有真实模式及空凭据生产者，避免第二夹具权威。
    let temp = tempfile::tempdir().expect("configured original removal");
    // All selection values come from the actual existing configured package pipeline.
    // 全部选择值来自实际既有已配置插件包流水线。
    let (store, package, credentials) = super::configuration::fixture(temp.path(), true);
    // The complete user document is deliberately recognizable in any accidental debug exposure.
    // 完整用户文档刻意可识别，以发现意外调试暴露。
    let configuration = VulcanPluginConfigurationSelection::Document {
        value: serde_json::json!({"label":"retained-removal-private"}),
    };
    // The actual sole configured selection writer establishes approval and bindings atomically.
    // 实际唯一已配置选择写入者原子建立批准及绑定。
    let selected = store
        .activate_configured(
            &package,
            0,
            package.required_permissions.clone(),
            credentials.clone(),
            configuration.clone(),
        )
        .expect("actual complete configured selection");
    // The original owns exact complete cleanup references before its only removal publication.
    // 原实例在唯一移除发布之前拥有精确完整清理引用。
    let mut original = store
        .reserve_deactivation(Uuid::new_v4(), &selected)
        .expect("configured original reservation");
    assert_eq!(
        original.intended().schema_version,
        ACTIVATION_RECORD_VERSION
    );
    assert_eq!(original.intended().credential_revision(), &credentials);
    assert_eq!(original.intended().configuration(), &configuration);
    assert!(original.intended().approved_permissions().is_empty());
    assert!(!format!("{original:?}").contains("retained-removal-private"));
    assert_eq!(
        store
            .publish_deactivation_retained(&mut original)
            .expect("same configured original publication"),
        Observation::Published
    );
    // The independently read actual record must preserve both identities after the original releases its File.
    // 原实例释放其文件之后，独立读取实际记录必须保留两个身份。
    let intended = original.intended().clone();
    drop(original);
    assert_eq!(
        store
            .activation("configured-plugin")
            .expect("actual configured tombstone"),
        Some(intended)
    );
    assert!(package.package_root().join("plugin.yaml").is_file());
}

/// A genuine Windows replacement failure retains admitted intent and the real original bytes for one explicit retry.
/// 真实窗口平台替换失败保留已准入意图及真实原字节，供单个明确重试。
#[cfg(windows)]
#[test]
fn retained_deactivation_actual_replacement_failure_resumes_the_same_intent() {
    use std::os::windows::fs::OpenOptionsExt;
    // The actual existing record remains readable while Windows denies replacement of its real file.
    // 窗口平台拒绝替换真实文件期间，实际既有记录保持可读。
    let fixture = Fixture::new();
    // Keep this same concrete removal owner across the actual operating-system failure.
    // 跨实际操作系统失败保持此相同具体移除所有者。
    let mut original = fixture.reserve();
    // Freeze actual original bytes as verification evidence without fabricating recovery authority.
    // 固定实际原字节作为验证证据，不虚构恢复权威。
    let path = fixture.store.root.join("active/sample-plugin.json");
    // Allow actual reading and writing but deny deletion/rename, matching the existing atomic-writer regression contract.
    // 允许实际读写但拒绝删除／重命名，与既有原子写入器回归契约一致。
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(&path)
        .expect("actual original replacement blocker");
    // The byte comparison proves actual failure never replaced the original selection.
    // 字节比较证明实际失败从未替换原选择。
    let before = fs::read(&path).expect("actual original preimage");
    assert_eq!(
        fixture
            .store
            .publish_deactivation_retained(&mut original)
            .expect_err("actual atomic replacement denied")
            .code,
        VulcanPluginStoreErrorCode::Storage
    );
    assert_eq!(
        fs::read(&path).expect("original survives failed replacement"),
        before
    );
    assert_eq!(
        fixture
            .store
            .reconcile_deactivation_retained(&original)
            .expect("actual original remains selected"),
        Observation::Original
    );
    assert!(
        fixture
            .store
            .publish_deactivation_retained(&mut original)
            .is_err()
    );
    fixture.assert_busy();
    drop(held);
    assert_eq!(
        fixture
            .store
            .resume_deactivation_retained(&mut original)
            .expect("explicit same original intent retry"),
        Observation::Published
    );
    assert_eq!(original.intended().revision, fixture.selected.revision + 1);
    assert!(fixture.package.package_root().join("plugin.yaml").is_file());
}
