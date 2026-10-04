//! Catalog selection regressions using real packages, records, and filesystem leases.
//! 使用真实插件包、记录及文件系统租约的目录选择回归。

use super::*;
use crate::VulcanPluginStoreErrorCode;

/// Only the effective selection may reserve global tool names; hidden legacy bytes have no surface.
/// 只有生效选择可以保留全局工具名；隐藏的旧包字节没有暴露表面。
#[test]
fn managed_catalog_resolves_selection_before_tool_conflicts() {
    // Own legacy collisions and a managed replacement with a different tool namespace.
    // 持有旧包冲突及具有不同工具命名空间的受管替换包。
    let temp = tempfile::tempdir().expect("create selection conflict fixture");
    // The empty system root lets both user identities appear in the final public catalog.
    // 空系统根使两个用户身份都出现在最终公开目录。
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("create conflict bundled root");
    // Deterministic ordering makes the stale legacy owner reserve the conflicting name first.
    // 确定性排序使陈旧的旧包所有者先保留冲突名称。
    let user = temp.path().join("user");
    crate::catalog::tests::write_named_stateless_package(&user, "alpha-plugin", &["shared-tool"]);
    crate::catalog::tests::write_named_stateless_package(&user, "beta-plugin", &["shared-tool"]);
    // Selected replacement no longer declares the conflicting name.
    // 选定替换包不再声明冲突名称。
    let source = temp.path().join("source");
    crate::catalog::tests::write_named_stateless_package(
        &source,
        "alpha-plugin",
        &["replacement-tool"],
    );
    // Prepare and select through the same immutable storage API as the host transaction.
    // 通过与宿主事务相同的不可变存储接口准备并选择。
    let store = VulcanPluginGenerationStore::open(&user).expect("open conflict store");
    // Retain the selected source while loading the public catalog.
    // 加载公开目录期间保留选定来源。
    let generation = prepare(&store, &source.join("alpha-plugin"));
    store
        .activate(&generation, 0)
        .expect("select non-conflicting replacement");
    // Conflict validation must run after managed precedence has removed hidden legacy declarations.
    // 冲突校验必须在受管优先级移除隐藏旧声明之后运行。
    let selected = catalog(&bundled, &user);
    assert_eq!(
        selected.plugins["beta-plugin"].availability,
        VulcanPluginAvailability::Enabled
    );
    assert!(selected.enabled_tools.contains_key("shared-tool"));
    assert!(selected.enabled_tools.contains_key("replacement-tool"));
    store
        .deactivate("alpha-plugin", 1)
        .expect("hide legacy collision with tombstone");
    // Tombstones likewise remove the old package before any global reservation takes place.
    // 停用记录同样在任何全局保留发生之前移除旧包。
    let removed = catalog(&bundled, &user);
    assert!(!removed.plugins.contains_key("alpha-plugin"));
    assert_eq!(
        removed.plugins["beta-plugin"].availability,
        VulcanPluginAvailability::Enabled
    );
    assert!(removed.enabled_tools.contains_key("shared-tool"));
}

/// Writes a content package named `id` below `root`; `body` distinguishes its distributable bytes.
/// 在 `root` 下写入名为 `id` 的内容包；`body` 区分其可分发字节。
/// Returns the actual package directory consumed by production inspection and catalog loading.
/// 返回生产检查及目录加载消费的实际插件包目录。
fn package(root: &Path, id: &str, body: &str) -> PathBuf {
    // Content-only fixtures exercise discovery without a Lua engine or synthetic catalog records.
    // 纯内容夹具不需要 Lua 引擎或合成目录记录即可验证发现流程。
    let path = root.join(id);
    fs::create_dir_all(path.join("skills/review")).expect("create managed catalog skill");
    fs::write(
        path.join("skills/review/SKILL.md"),
        format!("---\nname: review\ndescription: Review code.\n---\n\n{body}\n"),
    )
    .expect("write managed catalog skill");
    fs::write(path.join("plugin.yaml"), format!("schema_version: 1\nid: {id}\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Managed Catalog\n  capability: Read-only review\n"))
        .expect("write managed catalog manifest");
    path
}

/// Prepares `source` using its production fingerprint and returns the actual generation lease.
/// 使用生产指纹准备 `source`，返回实际代次租约。
fn prepare(store: &VulcanPluginGenerationStore, source: &Path) -> PinnedVulcanPluginGeneration {
    // Inspect the same bytes that authorization and immutable preparation consume.
    // 检查授权及不可变准备所消费的同一份字节。
    let inspected = inspect_user_plugin_package(source).expect("inspect catalog source");
    store
        .prepare(source, &inspected.content_fingerprint)
        .expect("prepare catalog generation")
}

/// Calls the actual public catalog loader with empty user enablement overrides.
/// 使用空用户启停覆盖调用实际公开目录加载器。
/// `bundled` and `user` are independent roots; the result retains every selected package lease.
/// `bundled` 与 `user` 是独立根；返回值保留每个选中插件包的租约。
fn catalog(bundled: &Path, user: &Path) -> VulcanPluginCatalogSnapshot {
    load_vulcan_plugin_catalog(bundled, Some(user), &BTreeSet::new(), &BTreeSet::new(), 1)
}

/// Refreshing legacy roots neither creates managed storage nor discovers unselected preparations.
/// 刷新旧根目录既不创建受管存储，也不发现未选择的准备包。
#[test]
fn managed_catalog_preserves_explicit_legacy_and_prepared_boundaries() {
    // Own independent roots for legacy, bundled, and managed preparation.
    // 持有旧包、内置包及受管准备的独立根。
    let temp = tempfile::tempdir().expect("create discovery fixture");
    // An empty bundled root keeps user provenance observable.
    // 空内置根使用户来源可观察。
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("create bundled root");
    // The user root contains an ordinary legacy package before managed storage exists.
    // 受管存储出现之前，用户根包含普通旧包。
    let user = temp.path().join("user");
    package(&user, "sample-plugin", "legacy contents");
    // Discovery alone must not create the new private layout.
    // 单纯发现绝不能创建新私有布局。
    let legacy = catalog(&bundled, &user);
    assert_eq!(
        legacy.plugins["sample-plugin"].origin,
        VulcanPluginPackageOrigin::UserInstalled
    );
    assert!(legacy.plugins["sample-plugin"].managed_package().is_none());
    assert!(!user.join(".vulcan-generations").exists());
    assert!(
        VulcanPluginGenerationStore::open_existing(&user)
            .expect("inspect absent store")
            .is_none()
    );
    // Preparation is explicit and does not publish the candidate into the live catalog.
    // 准备是显式操作，不会把候选发布到活动目录。
    let store = VulcanPluginGenerationStore::open(&user).expect("initialize store");
    // Same-id candidates remain hidden until activation.
    // 相同标识的候选在激活之前保持隐藏。
    let source = package(
        &temp.path().join("source"),
        "sample-plugin",
        "managed contents",
    );
    // Retain the prepared package while checking live selection.
    // 检查活动选择时保留已准备插件包。
    let generation = prepare(&store, &source);
    assert_eq!(
        catalog(&bundled, &user).plugins["sample-plugin"].content_fingerprint,
        legacy.plugins["sample-plugin"].content_fingerprint
    );
    store
        .activate(&generation, 0)
        .expect("publish managed package");
    // User disablement continues to apply to the selected immutable package.
    // 用户停用继续应用于选中的不可变插件包。
    let disabled = load_vulcan_plugin_catalog(
        &bundled,
        Some(&user),
        &BTreeSet::from(["sample-plugin".into()]),
        &BTreeSet::new(),
        2,
    );
    assert_eq!(
        disabled.plugins["sample-plugin"].availability,
        VulcanPluginAvailability::Disabled
    );
    assert!(disabled.enabled_skills.is_empty());
    assert_eq!(
        disabled.plugins["sample-plugin"]
            .managed_package()
            .expect("managed selection")
            .activation_revision(),
        1
    );
    store
        .deactivate("sample-plugin", 1)
        .expect("publish tombstone");
    assert!(
        !catalog(&bundled, &user)
            .plugins
            .contains_key("sample-plugin")
    );
    assert!(user.join("sample-plugin/plugin.yaml").is_file());
}

/// Old snapshots and cloned descriptors retain real leases after a new generation is selected.
/// 选择新代次后，旧快照及克隆描述符继续保留实际租约。
#[test]
fn managed_catalog_snapshots_protect_original_bytes_until_last_owner_drops() {
    // Keep source and installed storage roots independent.
    // 保持来源及已安装存储根独立。
    let temp = tempfile::tempdir().expect("create catalog ownership fixture");
    // Bundled contributions must continue to override a same-id managed selection.
    // 内置贡献必须继续覆盖相同标识的受管选择。
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("create bundled root");
    // User root is initialized separately from source preparation.
    // 用户根独立于来源准备初始化。
    let user = temp.path().join("user");
    fs::create_dir(&user).expect("create user root");
    // Production store owns the immutable bytes.
    // 生产存储持有不可变字节。
    let store = VulcanPluginGenerationStore::open(&user).expect("open ownership store");
    // Original and successor differ only in real skill content.
    // 原包与后继仅真实技能内容不同。
    let first = prepare(
        &store,
        &package(&temp.path().join("first"), "sample-plugin", "first"),
    );
    // Identity copies themselves must not prevent collection.
    // 身份副本本身不得阻止回收。
    let first_identity = first.identity().clone();
    store.activate(&first, 0).expect("activate original");
    // The live catalog obtains its own generation lease from the actual selection record.
    // 活动目录从实际选择记录取得自己的代次租约。
    let old_catalog = catalog(&bundled, &user);
    // A detached descriptor remains a real owner after the full snapshot drops.
    // 完整快照释放后，分离描述符仍是真实所有者。
    let descriptor = old_catalog.plugins["sample-plugin"].clone();
    assert_eq!(descriptor.package_root, first.package_root());
    drop(first);
    // Select different code while the original snapshot remains alive.
    // 原快照仍存活时选择不同代码。
    let next = prepare(
        &store,
        &package(&temp.path().join("next"), "sample-plugin", "next"),
    );
    store.activate(&next, 1).expect("activate successor");
    assert_eq!(
        catalog(&bundled, &user).plugins["sample-plugin"].content_fingerprint,
        next.identity().content_fingerprint
    );
    assert_eq!(
        store
            .collect(&first_identity)
            .expect_err("snapshot owns old generation")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(old_catalog);
    assert_eq!(
        store
            .collect(&first_identity)
            .expect_err("descriptor owns old generation")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert!(
        fs::read_to_string(descriptor.package_root.join("skills/review/SKILL.md"))
            .expect("read retained original")
            .contains("first")
    );
    drop(descriptor);
    assert!(
        store
            .collect(&first_identity)
            .expect("collect after actual owners exit")
    );
    package(&bundled, "sample-plugin", "bundled content");
    // A same-id bundled package does not inherit managed ownership or authorization semantics.
    // 相同标识的内置包不继承受管所有权或授权语义。
    let shadowed = catalog(&bundled, &user);
    assert_eq!(
        shadowed.plugins["sample-plugin"].origin,
        VulcanPluginPackageOrigin::Bundled
    );
    assert!(
        shadowed.plugins["sample-plugin"]
            .managed_package()
            .is_none()
    );
}

/// Corrupt or busy managed records never revive same-id legacy bytes and do not block unrelated plugins.
/// 损坏或忙碌的受管记录绝不重新启用同名旧包，也不阻止无关插件。
#[test]
fn managed_catalog_record_failures_are_explicit_and_namespace_local() {
    // Every failure is produced by actual filesystem state rather than a mocked store response.
    // 每个失败均来自实际文件系统状态，而非模拟存储响应。
    let temp = tempfile::tempdir().expect("create catalog failure fixture");
    // Independent bundled content remains available under any user-store failure.
    // 独立内置内容在任何用户存储失败时保持可用。
    let bundled = temp.path().join("bundled");
    package(&bundled, "system-plugin", "system");
    // A legacy same-id package must never become the recovery candidate implicitly.
    // 同名旧包绝不能隐式成为恢复候选。
    let user = temp.path().join("user");
    package(&user, "sample-plugin", "legacy");
    package(&user, "other-plugin", "unrelated");
    // Real activation initializes the namespace and package records.
    // 实际激活初始化命名空间及插件包记录。
    let store = VulcanPluginGenerationStore::open(&user).expect("open failure store");
    // Pin exact managed bytes before injecting a metadata failure.
    // 注入元数据失败前固定精确受管字节。
    let generation = prepare(
        &store,
        &package(&temp.path().join("source"), "sample-plugin", "managed"),
    );
    store.activate(&generation, 0).expect("activate fixture");
    // The committed filename, not a diagnostic path, identifies the record under test.
    // 被测记录由已提交文件名标识，而非诊断路径。
    let record = user.join(".vulcan-generations/active/sample-plugin.json");
    // Preserve original bytes to exercise a later independent lock failure.
    // 保存原始字节，以便随后验证独立锁失败。
    let original = fs::read(&record).expect("read activation bytes");
    fs::write(&record, b"{broken activation").expect("corrupt committed record");
    // The affected row is visible but contributes no stale skill.
    // 受影响条目可见，但不贡献陈旧技能。
    let broken = catalog(&bundled, &user);
    assert_eq!(
        broken.plugins["sample-plugin"].availability,
        VulcanPluginAvailability::Unavailable
    );
    assert!(broken.plugins["sample-plugin"].skills.is_empty());
    assert_eq!(
        broken.plugins["other-plugin"].availability,
        VulcanPluginAvailability::Enabled
    );
    assert_eq!(
        broken.plugins["system-plugin"].availability,
        VulcanPluginAvailability::Enabled
    );
    assert_eq!(
        fs::read(&record).expect("read unchanged corrupt record"),
        b"{broken activation"
    );
    fs::write(&record, &original).expect("restore exact fixture record");
    // A separate file handle owns the real namespace lock during refresh.
    // 刷新期间，独立文件句柄拥有真实命名空间锁。
    let namespace = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(user.join(".vulcan-generations/locks/sample-plugin.lock"))
        .expect("open namespace lock");
    namespace.try_lock().expect("hold namespace lock");
    // Only the locked identity becomes unavailable.
    // 只有已锁定身份变为不可用。
    let busy = catalog(&bundled, &user);
    assert_eq!(
        busy.plugins["sample-plugin"].availability,
        VulcanPluginAvailability::Unavailable
    );
    assert_eq!(
        busy.plugins["other-plugin"].availability,
        VulcanPluginAvailability::Enabled
    );
    namespace
        .unlock()
        .expect("release exclusive namespace owner");
    namespace
        .try_lock_shared()
        .expect("hold another catalog reader's namespace lock");
    assert_eq!(
        catalog(&bundled, &user).plugins["sample-plugin"].availability,
        VulcanPluginAvailability::Enabled
    );
    assert_eq!(
        store
            .deactivate("sample-plugin", 1)
            .expect_err("shared selection readers still block mutation")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(namespace);
    assert_eq!(
        catalog(&bundled, &user).plugins["sample-plugin"].availability,
        VulcanPluginAvailability::Enabled
    );
    // A missing structural directory makes the affected namespace set unknowable; block all user packages.
    // 结构目录缺失使受影响命名空间集合无法确定；阻止全部用户包。
    fs::rename(
        user.join(".vulcan-generations/active"),
        user.join("saved-active"),
    )
    .expect("remove required layout component");
    // Refresh must neither reconstruct metadata nor fall back to legacy packages.
    // 刷新既不得重建元数据，也不得回退到旧包。
    let incomplete = catalog(&bundled, &user);
    assert_eq!(
        incomplete.plugins["sample-plugin"].availability,
        VulcanPluginAvailability::Unavailable
    );
    assert_eq!(
        incomplete.plugins["other-plugin"].availability,
        VulcanPluginAvailability::Unavailable
    );
    assert_eq!(
        incomplete.plugins["system-plugin"].availability,
        VulcanPluginAvailability::Enabled
    );
    assert!(!user.join(".vulcan-generations/active").exists());
}
