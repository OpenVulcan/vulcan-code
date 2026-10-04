//! Actual package leases, original files and Windows deletion failures exercise retained removal collection.
//! 实际包租约、原文件及 Windows 删除失败验证保留移除回收。

use super::*;

/// Real package readers block collection, and the original namespace survives both deletion and terminal confirmation.
/// 真实包读者阻挡回收，原命名空间跨删除及终态确认继续存活。
#[test]
fn retained_deactivation_collection_excludes_actual_package_readers() {
    // The same actual approved source provides every identity used by removal.
    // 相同实际已批准来源提供移除使用的每个身份。
    let fixture = Fixture::new();
    // Preserve only the actual path as test evidence, without creating another package owner.
    // 仅保留实际路径作为测试证据，不创建另一包所有者。
    let package_root = fixture.package.package_root().to_path_buf();
    // One original File remains retained throughout the test.
    // 单个原文件跨整个测试保持保留。
    let mut original = fixture.reserve();
    assert_eq!(
        fixture
            .store
            .collect_deactivation_package_retained(&original)
            .expect_err("unpublished original cannot collect package")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    fixture
        .store
        .publish_deactivation_retained(&mut original)
        .expect("publish actual original tombstone");
    assert_eq!(
        fixture
            .store
            .collect_deactivation_package_retained(&original)
            .expect_err("actual package lease must prevent deletion")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert!(package_root.is_dir());
    fixture.assert_busy();
    drop(fixture.package);
    assert!(
        fixture
            .store
            .collect_deactivation_package_retained(&original)
            .expect("collect through the same retained original File")
    );
    assert!(!package_root.exists());
    assert!(
        !fixture
            .store
            .collect_deactivation_package_retained(&original)
            .expect("confirm actual prior collection without another deletion")
    );
    assert_eq!(
        fixture
            .contender
            .activation("sample-plugin")
            .expect_err("terminal collection retains original namespace")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        fixture
            .store
            .reconcile_deactivation_retained(&original)
            .expect("collection preserves exact tombstone"),
        Observation::Published
    );
    drop(original);
    assert!(
        fixture
            .contender
            .activation("sample-plugin")
            .expect("original drop releases actual namespace")
            .is_some()
    );
}

/// A completed marker cannot authorize deleting bytes reintroduced after the actual original collection.
/// 已完成标记不能授权删除实际原回收之后重新引入的字节。
#[test]
fn retained_deactivation_collection_rejects_resurrected_package_bytes() {
    // An actual original selection is removed once before foreign bytes appear.
    // 外来字节出现之前，实际原选择被移除一次。
    let fixture = Fixture::new();
    // Freeze the actual producer path before releasing its real reader.
    // 在释放真实读者之前，固定实际生产者路径。
    let package_root = fixture.package.package_root().to_path_buf();
    // Keep the one original owner across first completion and subsequent confirmation.
    // 跨首次完成及随后确认保持单个原所有者。
    let mut original = fixture.reserve();
    fixture
        .store
        .publish_deactivation_retained(&mut original)
        .expect("publish removal");
    drop(fixture.package);
    assert!(
        fixture
            .store
            .collect_deactivation_package_retained(&original)
            .expect("collect actual original bytes")
    );
    fs::create_dir(&package_root).expect("introduce noncooperating foreign package bytes");
    fs::write(
        package_root.join("foreign.txt"),
        b"foreign surviving content",
    )
    .expect("persist foreign bytes after terminal collection");
    assert!(
        fixture
            .store
            .collect_deactivation_package_retained(&original)
            .is_err()
    );
    assert_eq!(
        fs::read(package_root.join("foreign.txt")).expect("foreign bytes remain"),
        b"foreign surviving content"
    );
    assert_eq!(
        fixture
            .contender
            .activation("sample-plugin")
            .expect_err("conflict retains same original namespace")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
}

/// Reservation verifies current actual package bytes before retaining an inventory or permitting callback effects.
/// 预留在保留清单或允许回调效果之前验证当前实际包字节。
#[test]
fn retained_deactivation_reservation_rejects_changed_package_before_effects() {
    // The current activation comes from the real original approval producer.
    // 当前激活来自真实原批准生产者。
    let fixture = Fixture::new();
    // Capture original persisted selection bytes to prove rejection performs no publication.
    // 捕获原持久选择字节，以证明拒绝不执行发布。
    let before = fs::read(fixture.store.root.join("active/sample-plugin.json"))
        .expect("original selection bytes");
    fs::write(
        fixture
            .package
            .package_root()
            .join("skills/review-helper/SKILL.md"),
        b"noncooperating package mutation",
    )
    .expect("change actual selected package content");
    assert!(
        fixture
            .store
            .reserve_deactivation(Uuid::new_v4(), &fixture.selected)
            .is_err()
    );
    assert_eq!(
        fs::read(fixture.store.root.join("active/sample-plugin.json"))
            .expect("selection after refusal"),
        before
    );
    assert_eq!(
        fixture
            .contender
            .activation("sample-plugin")
            .expect("failed reservation releases namespace"),
        Some(fixture.selected.clone())
    );
}

/// Actual Windows delete-sharing refusal preserves the original owner and resumes the same Collecting identity.
/// 实际 Windows 删除共享拒绝保留原所有者，并恢复同一清理中身份。
#[cfg(windows)]
#[test]
fn retained_deactivation_collection_recovers_real_windows_delete_failure() {
    use std::os::windows::fs::OpenOptionsExt;
    // Real deletion is denied by an actual Windows file handle, not a mocked collector result.
    // 真实删除由实际 Windows 文件句柄拒绝，而非模拟回收器结果。
    let fixture = Fixture::new();
    // This exact inspected source file belongs to the original immutable package.
    // 此精确已检查来源文件属于原不可变包。
    let package_root = fixture.package.package_root().to_path_buf();
    // The handle permits reads and writes but excludes actual delete sharing.
    // 此句柄允许读写，但排除实际删除共享。
    let blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(package_root.join("skills/review-helper/SKILL.md"))
        .expect("hold actual Windows deletion blocker");
    // The same original remains available after the actual partial deletion error.
    // 相同原实例在实际部分删除错误之后仍可使用。
    let mut original = fixture.reserve();
    fixture
        .store
        .publish_deactivation_retained(&mut original)
        .expect("publish removal");
    drop(fixture.package);
    assert!(
        fixture
            .store
            .collect_deactivation_package_retained(&original)
            .is_err()
    );
    assert!(package_root.exists());
    assert_eq!(
        fixture
            .contender
            .activation("sample-plugin")
            .expect_err("actual failure retains namespace File")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(blocker);
    assert!(
        fixture
            .store
            .collect_deactivation_package_retained(&original)
            .expect("resume same original physical collection")
    );
    assert!(!package_root.exists());
    assert!(
        !fixture
            .store
            .collect_deactivation_package_retained(&original)
            .expect("verify exact completed collection")
    );
}
