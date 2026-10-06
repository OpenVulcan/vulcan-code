//! Real producer histories are retained before removal effects without extra immutable package reader ownership.
//! 真实生产者历史在移除效果之前保留，不增加不可变包读者所有权。

use super::*;
use crate::VulcanPluginStoreErrorCode as Code;

/// Reject every secure backend call when the actual fixture's credential journals declare no slot.
/// 实际夹具凭据日志未声明任何槽位时，拒绝每个安全后端调用。
struct NoCredentialHistory;

impl crate::VulcanPluginCredentialBackend for NoCredentialHistory {
    /// An empty actual journal inventory must never request a read for plugin_id/revision/key; this fixture panics on a call.
    /// 空实际日志清单绝不能为 plugin_id／revision／key 请求读取；此夹具在调用时 panic。
    fn read(
        &self,
        _: &str,
        _: &crate::VulcanPluginCredentialRevision,
        _: &crate::VulcanPluginCredentialKey,
    ) -> std::result::Result<Option<String>, String> {
        panic!("empty original history cannot access secure backend")
    }

    /// Terminal storage confirmation never writes secure values; all supplied tuple/value arguments are rejected.
    /// 终态存储确认绝不写入安全值；全部所给元组／值参数均被拒绝。
    fn write(
        &self,
        _: &str,
        _: &crate::VulcanPluginCredentialRevision,
        _: &crate::VulcanPluginCredentialKey,
        _: &str,
    ) -> std::result::Result<(), String> {
        panic!("terminal storage confirmation cannot write secure backend")
    }

    /// Terminal storage confirmation never deletes secure values; all supplied tuple arguments are rejected.
    /// 终态存储确认绝不删除安全值；全部所给元组参数均被拒绝。
    fn delete(
        &self,
        _: &str,
        _: &crate::VulcanPluginCredentialRevision,
        _: &crate::VulcanPluginCredentialKey,
    ) -> std::result::Result<(), String> {
        panic!("terminal storage confirmation cannot delete secure backend")
    }
}

/// All actual prepared and collected markers are included deterministically, while the inventory cannot block current package GC.
/// 全部实际已准备及已回收标记确定性包含于清单，同时清单不能阻挡当前包回收。
#[test]
fn retained_deactivation_inventory_freezes_all_actual_package_histories_without_extra_pins() {
    // The original selected identity comes from the actual approved writer.
    // 原已选身份来自实际已批准写入者。
    let fixture = Fixture::new();
    // Independent source bytes produce a second real immutable identity in the same namespace.
    // 独立来源字节在相同命名空间内产生第二真实不可变身份。
    let old_source = write_package(
        &fixture._temp.path().join("old-source"),
        "sample-plugin",
        "older package",
    );
    // This real reader remains outside removal and must not be mistaken for captured metadata.
    // 此真实读者保持位于移除之外，不得被误认为捕获元数据。
    let old = prepare(&fixture.store, &old_source);
    // A third actual producer has already reclaimed its bytes while preserving its final identity marker.
    // 第三实际生产者已回收字节，同时保留最终身份标记。
    let collected_source = write_package(
        &fixture._temp.path().join("collected-source"),
        "sample-plugin",
        "collected package",
    );
    // The actual collector requires the physical producer pin to be explicitly released first.
    // 实际回收器要求先明确释放物理生产者固定。
    let collected = prepare(&fixture.store, &collected_source);
    // Pure identity remains usable as test evidence after actual reader release.
    // 实际读者释放之后，纯身份仍可作为测试证据使用。
    let collected_id = collected.identity().clone();
    drop(collected);
    fixture
        .store
        .collect(&collected_id)
        .expect("real historical package collection");
    // The same strong removal File freezes every history before publication.
    // 相同强移除文件在发布之前固定每个历史。
    let mut original = fixture.reserve();
    // Exact identities locate records without binding assertions to evolving directory order.
    // 精确身份定位记录，不把断言绑定到会演进的目录顺序。
    let histories = original
        .package_inventory()
        .map(|(identity, phase)| (identity.generation_id.clone(), phase))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(histories.len(), 3);
    assert_eq!(
        histories.get(&fixture.package.identity().generation_id),
        Some(&GenerationState::Prepared)
    );
    assert_eq!(
        histories.get(&old.identity().generation_id),
        Some(&GenerationState::Prepared)
    );
    assert_eq!(
        histories.get(&collected_id.generation_id),
        Some(&GenerationState::Collected)
    );
    fixture.assert_busy();
    fixture
        .store
        .publish_deactivation_retained(&mut original)
        .expect("actual same-file removal publication");
    drop(fixture.package);
    assert!(
        fixture
            .store
            .collect_deactivation_package_retained(&original)
            .expect("metadata snapshot does not pin original package")
    );
    assert!(old.package_root().is_dir());
    assert_eq!(
        original
            .package_inventory()
            .find(|(identity, _)| identity == &&collected_id)
            .expect("actual collected history stays captured")
            .1,
        GenerationState::Collected
    );
    assert_eq!(
        fixture
            .store
            .confirm_deactivation_inventory_collected_retained(&original, &NoCredentialHistory)
            .expect_err("actual historical reader must prevent terminal confirmation")
            .code,
        Code::Busy
    );
    assert_eq!(
        fixture
            .store
            .collect_deactivation_inventory_package_retained(&original, old.identity())
            .expect_err("actual historical reader must prevent collection")
            .code,
        Code::Busy
    );
    // A complete copied identity outside the frozen inventory cannot admit even an attempted filesystem collection.
    // 固定清单之外的完整复制身份不能准入任何尝试文件系统回收。
    let mut foreign = collected_id.clone();
    foreign.generation_id = Uuid::new_v4().to_string();
    assert_eq!(
        fixture
            .store
            .collect_deactivation_inventory_package_retained(&original, &foreign)
            .expect_err("foreign history cannot be adopted")
            .code,
        Code::Conflict
    );
    // Releasing the actual old reader admits only the exact retained historical producer.
    // 释放实际旧读者仅准入精确保留历史生产者。
    let old_id = old.identity().clone();
    // Retain the original actual producer record to exercise a later noncooperating phase rollback.
    // 保留原实际生产者记录，以覆盖随后非协作阶段回退。
    let old_record = fixture
        .store
        .read_generation(&old_id)
        .expect("actual prepared history marker")
        .1;
    drop(old);
    assert!(
        fixture
            .store
            .collect_deactivation_inventory_package_retained(&original, &old_id)
            .expect("collect real historical package after actual reader release")
    );
    assert!(
        !fixture
            .store
            .collect_deactivation_inventory_package_retained(&original, &collected_id)
            .expect("already collected history only confirms absence")
    );
    // The selected slot-free revision reaches its terminal phase without any secure backend call.
    // 已选无槽位修订在不调用任何安全后端的情况下到达终态。
    fixture
        .store
        .collect_deactivation_credentials_retained(&original, &NoCredentialHistory)
        .expect("collect selected slot-free revision");
    fixture
        .store
        .confirm_deactivation_inventory_collected_retained(&original, &NoCredentialHistory)
        .expect("complete actual historical storage census is terminal");
    // A foreign physical resurrection after terminal collection must remain intact and fail read-only confirmation.
    // 终态回收之后外来物理复活必须保持完整，并使只读确认失败。
    let resurrected = fixture
        .store
        .root
        .join("packages")
        .join(&old_id.plugin_id)
        .join(&old_id.generation_id)
        .join(&old_id.plugin_id);
    fs::create_dir(&resurrected).expect("actual foreign package resurrection");
    fs::write(resurrected.join("foreign.txt"), b"foreign bytes").expect("actual resurrected bytes");
    assert_eq!(
        fixture
            .store
            .confirm_deactivation_inventory_collected_retained(&original, &NoCredentialHistory)
            .expect_err("terminal confirmation refuses resurrected bytes")
            .code,
        Code::Conflict
    );
    assert!(
        fixture
            .store
            .collect_deactivation_inventory_package_retained(&original, &old_id)
            .is_err()
    );
    assert_eq!(
        fs::read(resurrected.join("foreign.txt")).expect("foreign resurrection remains intact"),
        b"foreign bytes"
    );
    // Restoring initial actual metadata must not reset the same original's observed terminal watermark.
    // 恢复初始实际元数据绝不能重置同原实例已观察终态水位。
    files::write_record(
        &resurrected
            .parent()
            .expect("actual generation root")
            .join("generation.json"),
        &old_record,
    )
    .expect("deliberate rollback to initial actual producer phase");
    assert_eq!(
        fixture
            .store
            .collect_deactivation_inventory_package_retained(&original, &old_id)
            .expect_err("same original must reject terminal package phase regression")
            .code,
        Code::Conflict
    );
    assert_eq!(
        fs::read(resurrected.join("foreign.txt"))
            .expect("phase regression never admits byte deletion"),
        b"foreign bytes"
    );
}

/// A foreign real producer marker in a historical directory is rejected before callback or selection admission.
/// 历史目录内外来真实生产者标记在回调或选择准入之前被拒绝。
#[test]
fn retained_deactivation_inventory_rejects_foreign_actual_historical_package_marker_without_effects()
 {
    // Selected original bytes remain valid so the error must come from the distinct historical marker.
    // 已选原字节保持有效，使错误必须来自独立历史标记。
    let fixture = Fixture::new();
    // This actual source creates the historical namespace entry to be replaced.
    // 此实际来源创建待替换历史命名空间条目。
    let old_source = write_package(
        &fixture._temp.path().join("old-source"),
        "sample-plugin",
        "older package",
    );
    // The old physical reader survives the metadata conflict independently.
    // 旧物理读者独立于元数据冲突存续。
    let old = prepare(&fixture.store, &old_source);
    // Another actual producer writes complete valid metadata for its own distinct identity.
    // 另一实际生产者为自身不同身份写入完整有效元数据。
    let peer_source = write_package(
        &fixture._temp.path().join("peer-source"),
        "peer-plugin",
        "unrelated peer",
    );
    // Copy actual producer bytes rather than inventing a malformed record shape.
    // 复制实际生产者字节，而非虚构损坏记录形状。
    let peer = prepare(&fixture.store, &peer_source);
    // Production readers expose metadata beside the exact physical package child.
    // 生产读取者在精确物理包子级旁暴露元数据。
    let old_marker = old
        .package_root()
        .parent()
        .expect("actual old generation root")
        .join("generation.json");
    fs::copy(
        peer.package_root()
            .parent()
            .expect("actual peer generation root")
            .join("generation.json"),
        &old_marker,
    )
    .expect("actual foreign historical record replacement");
    // Selection bytes are checked independently of the mutated unrelated historical marker.
    // 选择字节独立于改变的无关历史标记被检查。
    let before = fs::read(fixture.store.root.join("active/sample-plugin.json"))
        .expect("actual selection before failure");
    assert!(
        fixture
            .store
            .reserve_deactivation(Uuid::new_v4(), &fixture.selected)
            .is_err()
    );
    assert_eq!(
        fs::read(fixture.store.root.join("active/sample-plugin.json"))
            .expect("unchanged actual selection"),
        before
    );
    assert_eq!(
        fixture
            .contender
            .activation("sample-plugin")
            .expect("inert failure releases actual namespace"),
        Some(fixture.selected)
    );
    assert_eq!(
        fs::read(&old_marker).expect("foreign history remains preserved"),
        fs::read(
            peer.package_root()
                .parent()
                .expect("actual peer root")
                .join("generation.json")
        )
        .expect("actual foreign producer bytes")
    );
}
