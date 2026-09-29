//! Selected managed approvals replace legacy sidecar authority without rewriting user state.
//! 选定受管批准替代旧附属文件权威，不重写用户状态。

use super::*;

/// The actual manager consumes one selected package/approval pair and never falls back after corruption.
/// 实际管理器消费单个选定包与批准组合，损坏后绝不回退。
#[test]
fn managed_approval_runtime_uses_selected_grants_without_sidecar_fallback() {
    // The real manager, immutable store and authorization files all belong to this isolated fixture.
    // 实际管理器、不可变存储及授权文件均属于此隔离夹具。
    let temp = tempfile::tempdir().expect("create runtime approval fixture");
    // Empty bundled contributions leave the user selection authoritative.
    // 空内置贡献使用户选择保持权威。
    let runtime_root = temp.path().join("runtime");
    // The manager fixture owns both configured user and authorization roots.
    // 管理器夹具持有配置中的用户根及授权根。
    let bundled = runtime_root.join("plugins");
    fs::create_dir_all(&bundled).expect("create approval bundled root");
    // This manager performs the same authorization projection as production refreshes.
    // 此管理器执行与生产刷新相同的授权投影。
    let manager = test_manager(&runtime_root, bundled.clone());
    // Inspect and copy a valid real package rather than assembling catalog state manually.
    // 检查并复制真实有效插件包，不手工组装目录状态。
    let source = temp.path().join("source/local-plugin");
    super::super::super::tests::write_user_plugin_package(&source, "local-plugin");
    // The shared source fixture has zero permissions; add an explicit capability for rejection coverage.
    // 共用来源夹具具有零权限；追加明确能力以覆盖拒绝路径。
    let manifest_path = source.join("plugin.yaml");
    // Keep the real Lua declaration intact while adding one supported manifest permission.
    // 添加一项支持的清单权限时保留真实 Lua 声明。
    let manifest = fs::read_to_string(&manifest_path).expect("read approval runtime manifest");
    fs::write(&manifest_path, format!("{manifest}permissions: [config]\n"))
        .expect("declare runtime approval capability");
    // Freeze exact permissions and content for both legacy and new authority checks.
    // 为旧权威及新权威检查固定精确权限与内容。
    let preview = manager
        .inspect_local_user_plugin(&source)
        .expect("inspect approval package");
    assert!(
        !preview.permissions.is_empty(),
        "fixture must declare real capabilities"
    );
    // The actual catalog root is shared with the immutable generation store.
    // 实际目录根与不可变代次存储共用。
    let store = crate::VulcanPluginGenerationStore::open(&runtime_root.join("user_plugins"))
        .expect("open runtime approval store");
    // Retain physical ownership while exercising both authorization formats.
    // 验证两种授权格式期间保留物理所有权。
    let generation = store
        .prepare(&source, &preview.content_fingerprint)
        .expect("prepare approval package");
    store
        .activate(&generation, 0)
        .expect("select legacy format");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load legacy package");
    manager
        .authorize_user_plugin("local-plugin", preview.permissions.clone())
        .expect("write legacy approval");
    // Corrupt legacy state becomes a tripwire: v2 must neither read it nor repair it.
    // 损坏旧状态成为检测点：第二版不得读取或修补它。
    let sidecar = runtime_root.join("plugin_state/local-plugin/authorization.json");
    fs::write(&sidecar, b"legacy approval deliberately unavailable")
        .expect("write sidecar tripwire");
    store
        .activate_authorized(&generation, 1, preview.permissions.clone())
        .expect("publish package and approval");
    // Runtime projection must accept v2 despite an unreadable legacy authorization document.
    // 即使旧授权文档不可解码，运行时投影也必须接受第二版。
    let selected = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("project selected approval");
    assert_eq!(
        selected.plugins["local-plugin"].runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    assert_eq!(
        selected.plugins["local-plugin"]
            .managed_package()
            .expect("managed descriptor")
            .approved_permissions(),
        Some(preview.permissions.as_slice())
    );
    assert!(
        manager
            .authorize_user_plugin("local-plugin", preview.permissions.clone())
            .expect_err("reject competing sidecar writer")
            .message
            .contains("activation transaction")
    );
    assert_eq!(
        fs::read(&sidecar).expect("unchanged sidecar"),
        b"legacy approval deliberately unavailable"
    );
    // Restarting the actual manager recovers approval from the same on-disk selection.
    // 重启实际管理器从相同磁盘选择恢复批准权限。
    let restarted = test_manager(&runtime_root, bundled);
    assert_eq!(
        restarted
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("reload selected approval")
            .plugins["local-plugin"]
            .runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    // Corrupt only the current approval while preserving a previously cloned immutable descriptor.
    // 保留先前克隆不可变描述符时，仅损坏当前批准权限。
    let activation_path =
        runtime_root.join("user_plugins/.vulcan-generations/active/local-plugin.json");
    // The malformed approval must not be silently filled from the old authorization path.
    // 不得从旧授权路径静默补齐损坏批准。
    let mut record: serde_json::Value =
        serde_json::from_slice(&fs::read(&activation_path).expect("read selected file"))
            .expect("decode selected file");
    record["approved_permissions"] = serde_json::json!([]);
    fs::write(
        &activation_path,
        serde_json::to_vec(&record).expect("encode corruption"),
    )
    .expect("write corrupt approval");
    // Invalid selection has no executable contribution surface.
    // 无效选择没有可执行贡献表面。
    let blocked = restarted
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("project blocked selection");
    assert_eq!(
        blocked.plugins["local-plugin"].runtime_state,
        VulcanPluginRuntimeState::Unavailable
    );
    assert!(blocked.enabled_tools.is_empty());
    assert!(blocked.enabled_hooks.is_empty());
    assert_eq!(
        selected.plugins["local-plugin"]
            .managed_package()
            .expect("old snapshot lease")
            .approved_permissions(),
        Some(preview.permissions.as_slice())
    );
    assert_eq!(
        fs::read(&sidecar).expect("retained legacy bytes"),
        b"legacy approval deliberately unavailable"
    );
}
