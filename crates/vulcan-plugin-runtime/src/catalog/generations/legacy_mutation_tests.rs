//! Actual legacy namespace owners provide strict catalog selection without replacing ordinary peer pinning.
//! 实际旧格式命名空间所有者提供严格目录选择，不替换普通其他插件固定操作。

use super::*;
use crate::{VulcanPluginLegacyMutationGuard, VulcanPluginStoreErrorCode};

/// Write one real stateless package whose explicit tool is validated by the production catalog fixture.
/// 写入一个真实无状态插件包，其显式工具由生产目录夹具校验。
/// Root and plugin/tool IDs identify physical bytes; return the package path consumed by real preparation.
/// 根及插件／工具标识定位物理字节；返回真实准备消费的插件包路径。
pub(super) fn package(root: &Path, plugin_id: &str, tool_id: &str) -> PathBuf {
    crate::catalog::tests::write_named_stateless_package(root, plugin_id, &[tool_id]);
    root.join(plugin_id)
}

/// Prepare inspected source bytes through the actual store, returning its real immutable package pin.
/// 通过实际存储准备已检查来源字节，返回真实不可变插件包固定句柄。
pub(super) fn prepare(
    store: &VulcanPluginGenerationStore,
    source: &Path,
) -> PinnedVulcanPluginGeneration {
    // The sole inspector provides the actual complete byte fingerprint.
    // 唯一检查器提供实际完整字节指纹。
    let descriptor = inspect_user_plugin_package(source).expect("actual source");
    store
        .prepare(source, &descriptor.content_fingerprint)
        .expect("actual prepare")
}

/// Load both roots at one fixed revision with empty policies and the explicit retained target owner.
/// 使用空策略及显式保留目标所有者，以一个固定修订加载两个根。
fn guarded_catalog(
    bundled: &Path,
    user: &Path,
    plugin_id: &str,
    guard: &VulcanPluginLegacyMutationGuard,
) -> crate::VulcanPluginStoreResult<VulcanPluginCatalogSnapshot> {
    crate::catalog::load_vulcan_plugin_catalog_with_legacy_mutation(
        bundled,
        user,
        &BTreeSet::new(),
        &BTreeSet::new(),
        19,
        plugin_id,
        guard,
    )
}

/// Load the ordinary catalog after the original guard drops, using precisely the same roots, policies and revision.
/// 原守卫释放后，使用精确相同根、策略及修订加载普通目录。
fn ordinary_catalog(bundled: &Path, user: &Path) -> VulcanPluginCatalogSnapshot {
    load_vulcan_plugin_catalog(bundled, Some(user), &BTreeSet::new(), &BTreeSet::new(), 19)
}

/// Compare all catalog projections and managed authority, excluding only native File debug handles after explicit authority checks.
/// 比较全部目录投影及受管权威，仅在显式权威检查之后排除原生 File 调试句柄。
pub(super) fn assert_equivalent(
    actual: &VulcanPluginCatalogSnapshot,
    ordinary: &VulcanPluginCatalogSnapshot,
) {
    assert_eq!(
        actual.plugins.keys().collect::<Vec<_>>(),
        ordinary.plugins.keys().collect::<Vec<_>>()
    );
    for (plugin_id, package) in &actual.plugins {
        match (
            &package.managed_package,
            &ordinary.plugins[plugin_id].managed_package,
        ) {
            (Some(left), Some(right)) => {
                assert_eq!(left.generation.identity(), right.generation.identity());
                assert_eq!(
                    left.generation.package_root(),
                    right.generation.package_root()
                );
                assert_eq!(left.activation_revision, right.activation_revision);
                assert_eq!(left.approved_permissions, right.approved_permissions);
                assert_eq!(
                    format!("{:?}", left.credential_binding),
                    format!("{:?}", right.credential_binding)
                );
                assert_eq!(left.configuration, right.configuration);
                assert_eq!(
                    left._credential_lease.record(),
                    right._credential_lease.record(),
                );
            }
            (None, None) => {}
            _ => panic!("same real selection must retain the same managed authority"),
        }
    }
    // These clones are comparison-only projections; production catalogs continue retaining their original real pins.
    // 此些克隆仅用于比较投影；生产目录继续保留原真实固定句柄。
    let mut actual_projection = actual.clone();
    // The independently loaded ordinary snapshot has the same semantic authority but different native file handles.
    // 独立加载普通快照具有相同语义权威，但原生文件句柄不同。
    let mut ordinary_projection = ordinary.clone();
    for snapshot in [&mut actual_projection, &mut ordinary_projection] {
        for package in snapshot.plugins.values_mut() {
            package.managed_package = None;
            for server in &mut package.mcp_servers {
                server.credential_lease = None;
            }
        }
        for server in snapshot.enabled_mcp_servers.values_mut() {
            server.credential_lease = None;
        }
    }
    assert_eq!(
        format!("{actual_projection:?}"),
        format!("{ordinary_projection:?}")
    );
}

/// Empty manifest slots perform zero secure operations; every unexpected backend call is a test failure.
/// 空清单槽位执行零安全操作；每次意外后端调用均为测试失败。
struct EmptyBackend;

impl crate::VulcanPluginCredentialBackend for EmptyBackend {
    /// Reject unexpected reads for the explicit plugin/revision/key; empty real inventory requires none.
    /// 拒绝显式插件／修订／键的意外读取；真实空清单不需要读取。
    fn read(
        &self,
        _: &str,
        _: &crate::VulcanPluginCredentialRevision,
        _: &crate::VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        panic!("empty fixture must never read secure storage")
    }
    /// Reject unexpected value writes; empty real inventory requires none.
    /// 拒绝意外值写入；真实空清单不需要写入。
    fn write(
        &self,
        _: &str,
        _: &crate::VulcanPluginCredentialRevision,
        _: &crate::VulcanPluginCredentialKey,
        _: &str,
    ) -> Result<(), String> {
        panic!("empty fixture must never write secure storage")
    }
    /// Reject unexpected deletion; read-only catalog selection cannot invoke cleanup.
    /// 拒绝意外删除；只读目录选择不能调用清理。
    fn delete(
        &self,
        _: &str,
        _: &crate::VulcanPluginCredentialRevision,
        _: &crate::VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        panic!("catalog must never delete secure storage")
    }
}

/// Select a real version-four document with an actual empty staged credential inventory, returning its full selection.
/// 使用实际空暂存凭据清单选择真实第四版文档，返回完整选择。
fn select_configured(
    store: &VulcanPluginGenerationStore,
    generation: &PinnedVulcanPluginGeneration,
) -> crate::VulcanPluginActivation {
    // This actual revision is staged by the original production writer without backend calls.
    // 此实际修订由原生产写入器暂存，不调用后端。
    let revision = crate::VulcanPluginCredentialRevision::allocate();
    store
        .begin_credential_staging(
            generation,
            revision.clone(),
            &crate::VulcanPluginCredentialValues::new(),
        )
        .expect("actual empty inventory")
        .write_and_finish(&EmptyBackend, &crate::VulcanPluginCredentialValues::new())
        .expect("actual Staged inventory");
    // Exact capabilities are derived from the same prepared actual manifest.
    // 精确能力派生自相同已准备实际清单。
    let descriptor =
        inspect_user_plugin_package(generation.package_root()).expect("actual selected manifest");
    store
        .activate_configured(
            generation,
            0,
            crate::runtime::plugin_required_permissions(&descriptor).expect("full permissions"),
            revision,
            crate::VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("actual configured selection");
    store
        .activation("target-plugin")
        .expect("actual selected read")
        .expect("full selected record")
}

/// A true first flat installation and later physical uninstall both validate the same actual absent selection.
/// 真实首次平铺安装及后续物理卸载均校验相同实际缺失选择。
#[test]
fn legacy_guard_catalog_validates_flat_install_and_uninstalled_absence() {
    // Roots are independently allocated, with no initial target in either enumeration.
    // 根被独立分配，两种枚举中起初均没有目标。
    let temp = tempfile::tempdir().expect("flat owner fixture");
    // The system root contributes a real independent surface throughout both target states.
    // 系统根在两种目标状态期间持续贡献真实独立表面。
    let bundled = temp.path().join("bundled");
    package(&bundled, "system-plugin", "system-tool");
    // Opening storage is explicit before acquiring the real original None owner.
    // 取得真实原缺失所有者之前，显式打开存储。
    let user = temp.path().join("user");
    fs::create_dir(&user).expect("existing user root before explicit store initialization");
    let store = VulcanPluginGenerationStore::open(&user).expect("actual store");
    // Guard authority predates physical first publication, rather than being guessed from a catalog row.
    // 守卫权威先于物理首次发布，而非从目录条目猜测。
    let guard = store
        .acquire_legacy_mutation_guard("target-plugin", None)
        .expect("actual None guard");
    // Real first publication appears after the same namespace owner exists.
    // 相同命名空间所有者存在之后，真实首次发布出现。
    let target = package(&user, "target-plugin", "target-tool");
    let installed =
        guarded_catalog(&bundled, &user, "target-plugin", &guard).expect("flat guarded catalog");
    assert!(
        installed.plugins["target-plugin"]
            .managed_package()
            .is_none()
    );
    assert!(installed.enabled_tools.contains_key("target-tool"));
    assert_eq!(
        store
            .acquire_legacy_mutation_guard("target-plugin", None)
            .expect_err("same owner remains")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(guard);
    assert_equivalent(&installed, &ordinary_catalog(&bundled, &user));
    // A fresh real None owner spans uninstall and the subsequent absent target scan.
    // 新真实缺失所有者跨越卸载及随后目标缺失扫描。
    let guard = store
        .acquire_legacy_mutation_guard("target-plugin", None)
        .expect("uninstall original guard");
    fs::remove_dir_all(&target).expect("actual fixture uninstall");
    let absent = guarded_catalog(&bundled, &user, "target-plugin", &guard)
        .expect("guard validates unenumerated target");
    assert!(!absent.plugins.contains_key("target-plugin"));
    assert!(absent.enabled_tools.contains_key("system-tool"));
    assert_eq!(
        guarded_catalog(&bundled, &user, "different-plugin", &guard)
            .expect_err("absence cannot hide foreign target")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    drop(guard);
    assert_equivalent(&absent, &ordinary_catalog(&bundled, &user));
}

/// Full configured selection and exact target pins survive held namespace scans without changing durable bytes.
/// 完整已配置选择及精确目标固定句柄在持有命名空间扫描中存续，不改变持久字节。
#[test]
fn legacy_guard_catalog_retains_full_configured_selection() {
    // Independent sources distinguish selected immutable bytes from hidden flat bytes.
    // 独立来源区分选定不可变字节与隐藏平铺字节。
    let temp = tempfile::tempdir().expect("full selected fixture");
    // An empty existing system root isolates actual managed user authority.
    // 空既有系统根隔离实际受管用户权威。
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("system root");
    // The actual flat package must never override the selected managed package.
    // 实际平铺插件包绝不能覆盖选定受管插件包。
    let user = temp.path().join("user");
    package(&user, "target-plugin", "hidden-tool");
    // Production preparation and configured publication create all durable evidence.
    // 生产准备及已配置发布创建全部持久证据。
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    let generation = prepare(
        &store,
        &package(
            &temp.path().join("source"),
            "target-plugin",
            "selected-tool",
        ),
    );
    let original = select_configured(&store, &generation);
    // Capture both actual selection and package contents before this read-only path.
    // 此只读路径之前捕获实际选择及插件包内容。
    let path = user.join(".vulcan-generations/active/target-plugin.json");
    let bytes = fs::read(&path).expect("actual full original bytes");
    let modified = fs::metadata(&path)
        .expect("original metadata")
        .modified()
        .expect("original mtime");
    let guard = store
        .acquire_legacy_mutation_guard("target-plugin", Some(&original))
        .expect("full original owner");
    let selected = guarded_catalog(&bundled, &user, "target-plugin", &guard)
        .expect("same owner configured scan");
    assert!(
        selected.plugins["target-plugin"]
            .managed_package()
            .expect("actual pinned package")
            .matches_activation(&original)
    );
    assert!(selected.enabled_tools.contains_key("selected-tool"));
    assert!(!selected.enabled_tools.contains_key("hidden-tool"));
    assert_eq!(fs::read(&path).expect("readback"), bytes);
    assert_eq!(
        fs::metadata(&path)
            .expect("readback metadata")
            .modified()
            .expect("mtime"),
        modified
    );
    drop(guard);
    assert_equivalent(&selected, &ordinary_catalog(&bundled, &user));
}

/// A validated actual tombstone remains authoritative while a same-id flat directory is still present.
/// 相同标识平铺目录仍存在期间，已校验实际停用记录保持权威。
#[test]
fn legacy_guard_catalog_preserves_actual_tombstone() {
    // The hidden flat package supplies a visible regression if tombstone handling incorrectly returns unmanaged.
    // 隐藏平铺插件包在停用处理错误返回非受管时提供可见回归。
    let temp = tempfile::tempdir().expect("tombstone fixture");
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("system root");
    let user = temp.path().join("user");
    package(&user, "target-plugin", "hidden-tool");
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    let generation = prepare(
        &store,
        &package(
            &temp.path().join("source"),
            "target-plugin",
            "selected-tool",
        ),
    );
    store
        .activate_for_test(&generation, 0, generation.required_permissions_for_test())
        .expect("actual selection");
    store
        .deactivate("target-plugin", 1)
        .expect("actual tombstone");
    let original = store
        .activation("target-plugin")
        .expect("actual tombstone read")
        .expect("tombstone record");
    let guard = store
        .acquire_legacy_mutation_guard("target-plugin", Some(&original))
        .expect("tombstone owner");
    let removed =
        guarded_catalog(&bundled, &user, "target-plugin", &guard).expect("tombstone scan");
    assert!(!removed.plugins.contains_key("target-plugin"));
    assert!(user.join("target-plugin/plugin.yaml").is_file());
    drop(guard);
    assert_equivalent(&removed, &ordinary_catalog(&bundled, &user));
}

/// Only the literal guarded target avoids reacquisition; an independently locked peer retains ordinary Busy health.
/// 仅字面守卫目标避免重新取得锁；独立加锁其他插件保留普通 Busy 健康状态。
#[test]
fn legacy_guard_catalog_keeps_peer_pinning_ordinary() {
    // Two real namespace Files let the test detect an accidental global lock bypass.
    // 两个真实命名空间 File 使测试检测意外全局跳锁。
    let temp = tempfile::tempdir().expect("peer pin fixture");
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("system root");
    let user = temp.path().join("user");
    package(&user, "target-plugin", "target-tool");
    package(&user, "peer-plugin", "peer-tool");
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    let guard = store
        .acquire_legacy_mutation_guard("target-plugin", None)
        .expect("target owner");
    let peer = store
        .acquire_legacy_mutation_guard("peer-plugin", None)
        .expect("peer owner");
    let busy = guarded_catalog(&bundled, &user, "target-plugin", &guard)
        .expect("target succeeds independently");
    assert_eq!(
        busy.plugins["peer-plugin"].availability,
        VulcanPluginAvailability::Unavailable
    );
    assert!(busy.enabled_tools.contains_key("target-tool"));
    assert!(!busy.enabled_tools.contains_key("peer-tool"));
    drop(peer);
    let ready = guarded_catalog(&bundled, &user, "target-plugin", &guard)
        .expect("ordinary peer lock available");
    assert!(ready.enabled_tools.contains_key("peer-tool"));
    drop(guard);
    assert_equivalent(&ready, &ordinary_catalog(&bundled, &user));
}

/// Foreign roots and absent stores cannot borrow or synthesize the retained original owner, even with no target rows.
/// 即使没有目标条目，外部根及缺失存储也不能借用或合成保留原所有者。
#[test]
fn legacy_guard_catalog_rejects_foreign_and_absent_roots() {
    // The original target has no flat or active row, so validation cannot depend on enumeration.
    // 原目标没有平铺或活动条目，因此校验不能依赖枚举。
    let temp = tempfile::tempdir().expect("foreign root fixture");
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("system root");
    let user = temp.path().join("user");
    fs::create_dir(&user).expect("existing user root for absent target");
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    let guard = store
        .acquire_legacy_mutation_guard("target-plugin", None)
        .expect("original owner");
    let foreign = temp.path().join("foreign");
    fs::create_dir(&foreign).expect("existing foreign root before explicit store initialization");
    let foreign_store = VulcanPluginGenerationStore::open(&foreign).expect("foreign actual store");
    let foreign_guard = foreign_store
        .acquire_legacy_mutation_guard("target-plugin", None)
        .expect("foreign owner");
    assert_eq!(
        guarded_catalog(&bundled, &user, "target-plugin", &foreign_guard)
            .expect_err("foreign guard")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(
        guarded_catalog(&bundled, &foreign, "target-plugin", &guard)
            .expect_err("foreign root")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    let missing_store = temp.path().join("missing-store");
    fs::create_dir(&missing_store).expect("ordinary root without managed storage");
    assert_eq!(
        guarded_catalog(&bundled, &missing_store, "target-plugin", &guard)
            .expect_err("no existing store")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert!(!missing_store.join(".vulcan-generations").exists());
    let absent = temp.path().join("absent");
    assert!(guarded_catalog(&bundled, &absent, "target-plugin", &guard).is_err());
    assert!(!absent.exists());
}

/// Same-revision complete selection changes invalidate the actual guard and never become health-only successful snapshots.
/// 相同修订完整选择变动使实际守卫失效，绝不变为仅健康诊断的成功快照。
#[test]
fn legacy_guard_catalog_rejects_changed_complete_selection() {
    // Production configured selection makes approval, credential and configuration authorities explicit.
    // 生产已配置选择使批准、凭据及配置权威显式化。
    let temp = tempfile::tempdir().expect("changed full selection fixture");
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("system root");
    let user = temp.path().join("user");
    fs::create_dir(&user).expect("existing user root for selected original");
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    let generation = prepare(
        &store,
        &package(
            &temp.path().join("source"),
            "target-plugin",
            "selected-tool",
        ),
    );
    let original = select_configured(&store, &generation);
    let guard = store
        .acquire_legacy_mutation_guard("target-plugin", Some(&original))
        .expect("exact selected owner");
    let path = user.join(".vulcan-generations/active/target-plugin.json");
    let bytes = fs::read(&path).expect("actual original body");
    // External test-only mutation is explicit; the real original namespace excludes cooperating publishers.
    // 外部仅测试变更保持显式；真实原命名空间排除协作发布者。
    let mut changed = serde_json::to_value(&original).expect("full serialized original");
    changed["approved_permissions"] = serde_json::json!(["config"]);
    fs::write(
        &path,
        serde_json::to_vec(&changed).expect("changed full body"),
    )
    .expect("explicit external mutation");
    assert_eq!(
        guarded_catalog(&bundled, &user, "target-plugin", &guard)
            .expect_err("same revision changed approval")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert_eq!(
        store
            .acquire_legacy_mutation_guard("target-plugin", Some(&original))
            .expect_err("same File still owns namespace")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    fs::write(&path, bytes).expect("restore original exact body");
    let restored = guarded_catalog(&bundled, &user, "target-plugin", &guard)
        .expect("same actual owner revalidates");
    drop(guard);
    assert_equivalent(&restored, &ordinary_catalog(&bundled, &user));
}

/// Invalid actual flat target bytes are explicit errors, while bundled-first collision semantics remain unchanged.
/// 无效实际平铺目标字节是显式错误，而内置优先碰撞语义保持未变。
#[test]
fn legacy_guard_catalog_rejects_invalid_flat_target_and_preserves_bundled_precedence() {
    // Actual filesystem corruption exercises the sole production package validator.
    // 实际文件系统损坏验证唯一生产插件包校验器。
    let temp = tempfile::tempdir().expect("target validation fixture");
    let bundled = temp.path().join("bundled");
    package(&bundled, "target-plugin", "system-tool");
    let user = temp.path().join("user");
    let target = package(&user, "target-plugin", "user-tool");
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    let guard = store
        .acquire_legacy_mutation_guard("target-plugin", None)
        .expect("original None");
    let path = target.join("plugin.yaml");
    let bytes = fs::read(&path).expect("actual manifest");
    fs::write(&path, b"schema_version: invalid\n").expect("actual invalid manifest");
    assert_eq!(
        guarded_catalog(&bundled, &user, "target-plugin", &guard)
            .expect_err("target invalid even if bundled shadows")
            .code,
        VulcanPluginStoreErrorCode::Invalid
    );
    fs::write(&path, bytes).expect("restore actual manifest");
    let shadowed = guarded_catalog(&bundled, &user, "target-plugin", &guard)
        .expect("valid target keeps ordinary precedence");
    assert_eq!(
        shadowed.plugins["target-plugin"].origin,
        VulcanPluginPackageOrigin::Bundled
    );
    assert!(shadowed.enabled_tools.contains_key("system-tool"));
    assert!(!shadowed.enabled_tools.contains_key("user-tool"));
    assert!(
        shadowed
            .diagnostics
            .iter()
            .any(|message| message.contains("shadowed"))
    );
    drop(guard);
    assert_equivalent(&shadowed, &ordinary_catalog(&bundled, &user));
}

/// A selected target's actual byte mismatch propagates its strict pin failure instead of reviving a healthy flat package.
/// 选定目标实际字节不匹配传播严格固定失败，而非重新启用健康平铺插件包。
#[test]
fn legacy_guard_catalog_rejects_changed_actual_managed_package() {
    // Flat and selected tool sets differ, so any hidden recovery would expose the wrong execution surface.
    // 平铺及选定工具集合不同，因此任何隐藏恢复均会暴露错误执行表面。
    let temp = tempfile::tempdir().expect("changed actual target package fixture");
    // The empty bundled root leaves target health entirely controlled by its actual managed selection.
    // 空内置根使目标健康完全由其实际受管选择控制。
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("system root");
    // A valid hidden package is available physically but has no managed recovery authority.
    // 有效隐藏插件包在物理上可用，但没有受管恢复权威。
    let user = temp.path().join("user");
    package(&user, "target-plugin", "hidden-tool");
    // Actual preparation fixes and verifies the immutable selected byte identity.
    // 实际准备固定并验证不可变选定字节身份。
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    // The production package fixture declares this exact runtime path.
    // 生产插件包夹具声明此精确运行时路径。
    let generation = prepare(
        &store,
        &package(
            &temp.path().join("source"),
            "target-plugin",
            "selected-tool",
        ),
    );
    store
        .activate_for_test(&generation, 0, generation.required_permissions_for_test())
        .expect("actual selected package");
    // The actual complete published original is the only guard authority.
    // 实际完整已发布原选择是唯一守卫权威。
    let original = store
        .activation("target-plugin")
        .expect("selection")
        .expect("selected record");
    // Original selected namespace ownership remains held throughout corruption detection and restoration.
    // 原选定命名空间所有权在损坏检测及恢复期间持续被持有。
    let guard = store
        .acquire_legacy_mutation_guard("target-plugin", Some(&original))
        .expect("same selected owner");
    // Non-cooperating test mutation changes only actual package bytes, without substituting its prepared marker or identity.
    // 非协作测试变更仅改变实际插件包字节，不替换已准备标记或身份。
    let path = generation.package_root().join("runtime/plugin.bundle.lua");
    // Original true byte evidence is retained solely for explicit fixture restoration.
    // 原真实字节证据仅为显式夹具恢复保留。
    let bytes = fs::read(&path).expect("actual immutable runtime bytes");
    fs::write(
        &path,
        b"return { dispatch=function() return 'changed' end, shutdown=function() end }\n",
    )
    .expect("explicit external package mutation");
    // The corrupted target cannot become a successful health-only catalog result.
    // 损坏目标不能变为仅健康诊断的成功目录结果。
    let error = guarded_catalog(&bundled, &user, "target-plugin", &guard)
        .expect_err("strict actual target byte mismatch");
    assert_eq!(error.code, VulcanPluginStoreErrorCode::Invalid);
    assert_eq!(
        store
            .acquire_legacy_mutation_guard("target-plugin", Some(&original))
            .expect_err("same owner still holds namespace")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    fs::write(&path, bytes).expect("restore exact selected bytes");
    // The same original owner validates the restored real package without acquiring a replacement guard.
    // 相同原所有者校验恢复真实插件包，不取得替换守卫。
    let restored = guarded_catalog(&bundled, &user, "target-plugin", &guard)
        .expect("same guard recovers only restored exact bytes");
    drop(guard);
    assert_equivalent(&restored, &ordinary_catalog(&bundled, &user));
}
