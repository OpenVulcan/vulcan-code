//! Actual dormant package capture and independent collection retain the same selection/state/environment originals.
//! 实际休眠包捕获及独立回收保留相同选择／状态／环境原实例。

use super::*;

mod faults;

/// Publish the exact removal and collect only local state/environment with real core producers; returns no Full or native receipt.
/// 使用真实核心生产者发布精确移除并仅回收本地状态／环境；不返回完整或原生回执。
/// manager/store/state/original are retained actual instances; dormant bytes and secrets are untouched by this fixture helper.
/// manager／store／state／original 为保留实际实例；此夹具辅助函数不触碰休眠字节及秘密。
fn collect_local(
    manager: &VulcanPluginRuntimeManager,
    store: &VulcanPluginGenerationStore,
    state: &mut VulcanPluginDeactivationStateReservation,
    original: &mut VulcanPluginDeactivationReservation,
) {
    manager
        .freeze_deactivation_state(state, original)
        .expect("actual local cleanup freeze");
    store
        .publish_deactivation_retained(original)
        .expect("actual exact tombstone publication");
    manager
        .refresh_catalog_with_deactivation(&BTreeSet::new(), &BTreeSet::new(), original)
        .expect("actual same-file catalog removal");
    manager
        .collect_deactivation_state(state, original)
        .expect("actual original local/environment collection");
}

/// Current versioned routing does not replace the real dormant legacy descriptor, and dormant cleanup shares rather than reacquires its environment lease.
/// 当前版本化路由不能替代真实休眠旧描述符，休眠清理共享而非重新取得其环境租约。
#[test]
fn deactivation_flat_predecessor_captures_actual_legacy_slots_and_collects_independently() {
    // Real managed and flat packages are independent actual paths within one original user namespace.
    // 真实受管及平铺包为同原用户命名空间内独立实际路径。
    let temp = tempfile::tempdir().expect("actual dormant flat source fixture");
    // Versioned current selection has its own empty actual credential journal.
    // 版本化当前选择具有自身空实际凭据日志。
    let (manager, store, selected) = fixture(temp.path(), true);
    // The current actual route is frozen only as evidence that dormant bytes are not its immutable package.
    // 当前实际路由仅作为证据固定，证明休眠字节并非其不可变包。
    let managed_root = manager
        .snapshot()
        .plugins
        .get("local-plugin")
        .expect("actual current managed source")
        .package_root
        .clone();
    // The sole dormant path remains physically present despite managed routing.
    // 尽管采用受管路由，唯一休眠路径仍物理存在。
    let flat_root = manager.user_plugin_root().join("local-plugin");
    fs::write(flat_root.join("plugin.yaml"), "schema_version: 1\nid: local-plugin\nversion: 0.9.0\ndefault_enabled: false\ndisplay:\n  name: Dormant fixture\n  capability: Legacy secret declaration\npermissions: [network_https, secrets_read]\nmcp:\n  servers:\n    - id: remote\n      name: Remote\n      transport:\n        type: http\n        url: {source: literal, value: 'https://example.test/mcp'}\n        headers:\n          X-Old: {source: secret, key: old_optional}\n").expect("actual independent dormant legacy declaration");
    // The inspector supplies complete exact dormant identity, fingerprint and slot declarations before original capture.
    // 检查器在原捕获之前提供完整精确休眠身份、指纹及槽位声明。
    let actual_flat = crate::inspect_user_plugin_package(&flat_root)
        .expect("actual independent dormant package inspection");
    // One non-cloneable state pair retains both local preimages and actual dormant metadata, without another namespace borrower.
    // 单个不可克隆状态配对保留本地前像及实际休眠元数据，不增加命名空间借用者。
    let (mut original, mut state) = reserve(&manager, &store, &selected);
    // The captured source is identified by actual contents, independent of current managed selection.
    // 捕获来源由实际内容标识，独立于当前受管选择。
    let captured = state
        .original_flat_predecessor()
        .expect("actual dormant legacy descriptor");
    assert_eq!(
        captured.content_fingerprint,
        actual_flat.content_fingerprint
    );
    assert_eq!(captured.version.as_deref(), Some("0.9.0"));
    assert!(captured.managed_package().is_none());
    assert!(captured.mcp_servers.iter().any(|server| {
        server.id == "remote"
            && server.credential_binding == crate::VulcanPluginCredentialBinding::Legacy
            && server
                .manifest
                .declared_secret_keys()
                .contains("old_optional")
    }));
    assert!(
        manager
            .collect_deactivation_flat_predecessor(&mut state, &original)
            .is_err()
    );
    assert!(flat_root.exists() && managed_root.exists());
    collect_local(&manager, &store, &mut state, &mut original);
    assert!(flat_root.exists() && state.environment_cleanup.is_some());
    assert!(
        manager
            .confirm_deactivation_flat_predecessor_collected(&state, &original)
            .is_err()
    );
    manager
        .collect_deactivation_flat_predecessor(&mut state, &original)
        .expect("collect using already retained actual environment lease");
    assert!(!flat_root.exists() && managed_root.exists());
    manager
        .confirm_deactivation_flat_predecessor_collected(&state, &original)
        .expect("actual dormant terminal confirmation");
    manager
        .collect_deactivation_flat_predecessor(&mut state, &original)
        .expect("terminal recovery does not delete again");
    assert_busy(&manager);
    // A terminal resurrection cannot be adopted or removed by a repeated original call.
    // 终态复活不能被重复原调用接纳或移除。
    fs::create_dir(&flat_root).expect("actual dormant foreign resurrection");
    fs::write(flat_root.join("foreign.txt"), b"foreign flat bytes")
        .expect("actual foreign dormant bytes");
    assert!(
        manager
            .collect_deactivation_flat_predecessor(&mut state, &original)
            .is_err()
    );
    assert!(
        manager
            .confirm_deactivation_flat_predecessor_collected(&state, &original)
            .is_err()
    );
    assert_eq!(
        fs::read(flat_root.join("foreign.txt")).expect("foreign dormant bytes preserved"),
        b"foreign flat bytes"
    );
}

/// Explicit original absence and changed actual dormant bytes cannot be replaced by later discovery or snapshot recapture.
/// 明确原缺失及改变实际休眠字节不能被随后发现或重新捕获快照替代。
#[test]
fn deactivation_flat_predecessor_refuses_absence_replacement_and_changed_original_bytes() {
    for absent in [false, true] {
        // Each physical state has independent original files and actual manager identity.
        // 每个物理状态具有独立原文件及实际管理器身份。
        let temp = tempfile::tempdir().expect("actual dormant source replacement fixture");
        // Actual current legacy managed selection is distinct from its independently inspected flat history.
        // 实际当前旧式受管选择独立于其单独检查平铺历史。
        let (manager, store, selected) = fixture(temp.path(), false);
        // Only the declared actual dormant child is changed by this test.
        // 此测试仅改变声明实际休眠子级。
        let flat_root = manager.user_plugin_root().join("local-plugin");
        if absent {
            fs::remove_dir_all(&flat_root).expect("explicit original dormant absence fixture");
        }
        // The same original pair permanently captures the actual source presence state before effects.
        // 相同原配对在效果之前永久捕获实际来源存在状态。
        let (mut original, mut state) = reserve(&manager, &store, &selected);
        assert_eq!(state.original_flat_predecessor().is_none(), absent);
        if absent {
            fs::create_dir(&flat_root).expect("foreign source introduced after absent capture");
        }
        fs::write(flat_root.join("foreign.txt"), b"changed source")
            .expect("actual source change after capture");
        assert!(
            manager
                .freeze_deactivation_state(&mut state, &mut original)
                .is_err()
        );
        assert!(store.publish_deactivation_retained(&mut original).is_err());
        assert_eq!(
            fs::read(flat_root.join("foreign.txt")).expect("changed source remains intact"),
            b"changed source"
        );
        assert!(manager.reserve_deactivation_state(&mut original).is_err());
        assert_busy(&manager);
    }
}

/// Original absent dormant storage can reach terminal confirmation without constructing a package or another environment owner.
/// 原缺失休眠存储可以达到终态确认，不构造包或另一环境所有者。
#[test]
fn deactivation_flat_predecessor_confirms_original_absence_with_same_actual_pair() {
    // Actual absence is created before reservation, leaving current managed immutable bytes intact.
    // 实际缺失在预留之前创建，保持当前受管不可变字节完整。
    let temp = tempfile::tempdir().expect("actual absent dormant collection fixture");
    // Production selection, store and manager retain their normal physical fences.
    // 生产选择、存储及管理器保留正常物理屏障。
    let (manager, store, selected) = fixture(temp.path(), false);
    fs::remove_dir_all(manager.user_plugin_root().join("local-plugin"))
        .expect("actual original flat absence");
    // One attached state original captures absence, and dropping it cannot permit replacement preimages.
    // 单个关联状态原实例捕获缺失，释放它不能许可替代前像。
    let (mut original, mut state) = reserve(&manager, &store, &selected);
    assert!(state.original_flat_predecessor().is_none());
    collect_local(&manager, &store, &mut state, &mut original);
    manager
        .collect_deactivation_flat_predecessor(&mut state, &original)
        .expect("confirm captured dormant absence");
    manager
        .confirm_deactivation_flat_predecessor_collected(&state, &original)
        .expect("actual absent terminal evidence");
    // Another actual manager with different roots cannot consume this same pair's retained authority.
    // 具有不同根的另一实际管理器不能消费此相同配对保留权威。
    let peer_temp = tempfile::tempdir().expect("independent actual dormant manager");
    // This actual peer is constructed through the normal fixture producer, not an invented root alias.
    // 此实际对等实例通过正常夹具生产者构造，而非虚构根别名。
    let (peer, _, _) = fixture(peer_temp.path(), false);
    assert!(
        peer.collect_deactivation_flat_predecessor(&mut state, &original)
            .is_err()
    );
    assert!(
        peer.confirm_deactivation_flat_predecessor_collected(&state, &original)
            .is_err()
    );
}
