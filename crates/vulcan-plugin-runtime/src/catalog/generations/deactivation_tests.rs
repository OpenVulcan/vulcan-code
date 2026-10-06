//! Original removal catalogs use the same actual File and sole existing projection without ordinary target fallback.
//! 原移除目录使用同一实际文件及唯一既有投影，不使用普通目标备用读取。

use super::*;
use crate::{
    VulcanPluginActivation, VulcanPluginDeactivationReservation, VulcanPluginStoreErrorCode,
};
use uuid::Uuid;

/// Write one real stateless package whose explicit tool is validated by the production catalog fixture.
/// 写入一个真实无状态插件包，其显式工具由生产目录夹具校验。
/// Root and plugin/tool IDs identify physical bytes; return the package path consumed by real preparation.
/// 根及插件／工具标识定位物理字节；返回真实准备消费的插件包路径。
fn package(root: &Path, plugin_id: &str, tool_id: &str) -> PathBuf {
    crate::catalog::tests::write_named_stateless_package(root, plugin_id, &[tool_id]);
    root.join(plugin_id)
}

/// Prepare inspected source bytes through the actual store, returning its real immutable package pin.
/// 通过实际存储准备已检查来源字节，返回真实不可变插件包固定句柄。
fn prepare(store: &VulcanPluginGenerationStore, source: &Path) -> PinnedVulcanPluginGeneration {
    // The sole inspector provides the actual complete byte fingerprint.
    // 唯一检查器提供实际完整字节指纹。
    let descriptor = inspect_user_plugin_package(source).expect("actual source");
    store
        .prepare(source, &descriptor.content_fingerprint)
        .expect("actual prepare")
}

/// Compare all catalog projections and managed authority, excluding only native File debug handles after explicit authority checks.
/// 比较全部目录投影及受管权威，仅在显式权威检查之后排除原生 File 调试句柄。
fn assert_equivalent(actual: &VulcanPluginCatalogSnapshot, ordinary: &VulcanPluginCatalogSnapshot) {
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

/// Own the actual roots, prepared target and peer selections across namespace-preserving catalog reads.
/// 跨保留命名空间目录读取持有实际根、已准备目标及其他插件选择。
struct Fixture {
    /// Physical bytes outlive the real namespace and package owners.
    /// 物理字节比真实命名空间及包所有者存活更久。
    _root: tempfile::TempDir,
    /// Actual bundled discovery remains part of the complete merged projection.
    /// 实际内置发现仍为完整合并投影的一部分。
    bundled: PathBuf,
    /// The actual user root holds stray unmanaged directories beside the managed target and peer.
    /// 实际用户根在受管目标及其他插件旁包含零散非受管目录。
    user: PathBuf,
    /// The actual store supplies all record identities through production producers.
    /// 实际存储通过生产生产者提供全部记录身份。
    store: VulcanPluginGenerationStore,
    /// Exact full original is evidence for reservation, never an authority reconstructed from serialized bytes.
    /// 精确完整原记录为预留证据，绝不是从序列化字节重建的权威。
    selected: VulcanPluginActivation,
    /// The real pin proves catalog removal does not delete physical package bytes.
    /// 真实固定证明目录移除不删除物理包字节。
    target: PinnedVulcanPluginGeneration,
    /// The other managed namespace remains independently observable and owned.
    /// 其他受管命名空间保持独立可观测及持有。
    peer: VulcanPluginActivation,
}
impl Fixture {
    /// Publish complete approved target/peer records, a bundled peer and stray user directories; returns actual roots and owners.
    /// 发布完整已批准目标／其他记录、内置其他插件及零散用户目录；返回实际根及所有者。
    fn new() -> Self {
        // Isolate only this test's real package and namespace files.
        // 仅隔离此测试真实包及命名空间文件。
        let root = tempfile::tempdir().expect("original removal catalog fixture");
        // Keep each physical catalog root explicit instead of inferring it from package ancestors.
        // 保持每个物理目录根明确，而非从包祖先推断。
        let bundled = root.path().join("bundled");
        // The actual user root is passed directly to the sole store constructor.
        // 实际用户根直接传给唯一存储构造器。
        let user = root.path().join("user");
        package(&bundled, "bundled-peer", "bundled_peer_tool");
        package(&user, "flat-peer", "flat_peer_tool");
        package(&user, "target-plugin", "shadowed_target_tool");
        // Managed bytes are prepared from source outside public discovery.
        // 受管字节从公开发现之外的来源准备。
        let source_root = root.path().join("source");
        // Real target source has a distinct tool, proving tombstones cannot revive the stray directory.
        // 真实目标来源具有独立工具，证明停用记录不能复活零散目录。
        let target_source = package(&source_root, "target-plugin", "managed_target_tool");
        // A separate real managed peer remains subject to ordinary namespace pinning.
        // 独立真实受管其他插件仍受普通命名空间固定约束。
        let peer_source = package(&source_root, "managed-peer", "managed_peer_tool");
        // Only actual inspected sources determine authorization permissions.
        // 仅实际已检查来源确定授权权限。
        let target_descriptor =
            inspect_user_plugin_package(&target_source).expect("real target inspector");
        // Peer approval comes from the same sole inspector independently of target evidence.
        // 其他插件批准独立于目标证据，来自同一唯一检查器。
        let peer_descriptor =
            inspect_user_plugin_package(&peer_source).expect("real peer inspector");
        // The actual canonical store is the sole durable identity authority.
        // 实际规范存储为唯一持久身份权威。
        let store = VulcanPluginGenerationStore::open(&user).expect("actual canonical store");
        // Retain real prepared package bytes throughout every target removal read.
        // 在每次目标移除读取期间保留真实已准备包字节。
        let target = prepare(&store, &target_source);
        // A real peer selection uses its own immutable preparation and revision.
        // 真实其他插件选择使用其自身不可变准备及修订。
        let peer_package = prepare(&store, &peer_source);
        // Actual complete approval is produced from the target manifest, not hand-built record fields.
        // 实际完整批准从目标清单产生，不手工构造记录字段。
        let selected = store
            .activate_for_test(
                &target,
                0,
                crate::runtime::plugin_required_permissions(&target_descriptor)
                    .expect("target permissions"),
            )
            .expect("real target publication");
        // Preserve a separately produced actual peer record for strict equivalence assertions.
        // 保留单独产生实际其他插件记录，用于严格等价断言。
        let peer = store
            .activate_for_test(
                &peer_package,
                0,
                crate::runtime::plugin_required_permissions(&peer_descriptor)
                    .expect("peer permissions"),
            )
            .expect("real peer publication");
        Self {
            _root: root,
            bundled,
            user,
            store,
            selected,
            target,
            peer,
        }
    }
    /// Reserve one nonnil original using this fixture's actual complete selected record; returns its real File owner.
    /// 使用此夹具实际完整已选记录预留单个非空原请求；返回真实文件所有者。
    fn reserve(&self) -> VulcanPluginDeactivationReservation {
        self.store
            .reserve_deactivation(Uuid::new_v4(), &self.selected)
            .expect("original removal owner")
    }
    /// Read both actual roots with the same fixed comparison revision and original reservation; returns complete classified projection.
    /// 使用同一固定比较修订及原预留读取两个实际根；返回完整带类别投影。
    fn catalog(
        &self,
        original: &VulcanPluginDeactivationReservation,
    ) -> crate::VulcanPluginStoreResult<VulcanPluginCatalogSnapshot> {
        crate::catalog::load_vulcan_plugin_catalog_with_deactivation(
            &self.bundled,
            &self.user,
            &BTreeSet::new(),
            &BTreeSet::new(),
            19,
            original,
        )
    }
    /// Read ordinary comparison roots after explicit original release; returns the same policy/revision projection.
    /// 明确原释放之后读取普通比较根；返回相同策略／修订投影。
    fn ordinary(&self) -> VulcanPluginCatalogSnapshot {
        load_vulcan_plugin_catalog(
            &self.bundled,
            Some(&self.user),
            &BTreeSet::new(),
            &BTreeSet::new(),
            19,
        )
    }
}

/// Exact published removal reads the same File, hides every target contribution and preserves actual independent peer authority.
/// 精确已发布移除读取相同文件，隐藏全部目标贡献，并保留实际独立其他插件权威。
#[test]
fn deactivation_catalog_same_original_tombstone_matches_actual_ordinary_projection_after_release() {
    // Keep real physical source and selection owners throughout both guarded and ordinary comparisons.
    // 在受保护及普通比较期间保持真实物理来源及选择所有者。
    let fixture = Fixture::new();
    // The original reservation performs no record publication or deletion.
    // 原预留不执行记录发布或删除。
    let mut original = fixture.reserve();
    assert_eq!(
        fixture
            .catalog(&original)
            .expect_err("unpublished original cannot authorize removal projection")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    fixture
        .store
        .publish_deactivation_retained(&mut original)
        .expect("actual original tombstone write");
    // Actual projection must succeed while ordinary independent target pinning remains self-busy.
    // 实际投影必须在普通独立目标固定仍自身繁忙时成功。
    let actual = fixture
        .catalog(&original)
        .expect("same actual File projection");
    assert!(!actual.plugins.contains_key("target-plugin"));
    assert!(
        actual
            .enabled_tools
            .values()
            .all(|tool| tool.plugin_id != "target-plugin")
    );
    assert!(
        actual.plugins["managed-peer"]
            .managed_package()
            .expect("real peer pin")
            .matches_activation(&fixture.peer)
    );
    assert!(!actual.plugins.contains_key("flat-peer"));
    assert!(actual.plugins.contains_key("bundled-peer"));
    assert!(fixture.target.package_root().is_dir());
    assert_eq!(
        VulcanPluginGenerationStore::open(&fixture.user)
            .expect("independent actual store")
            .acquire_namespace_guard("target-plugin", Some(original.intended()))
            .expect_err("same original File remains held")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(original);
    assert_equivalent(&actual, &fixture.ordinary());
}

/// Explicit target ownership never borrows a peer's held namespace or expands one Busy error into a whole-catalog failure.
/// 明确目标所有权绝不借用其他插件持有命名空间，也不将一个繁忙错误扩大为整个目录失败。
#[test]
fn deactivation_catalog_peer_namespace_remains_ordinary_and_busy_without_target_fallback() {
    let fixture = Fixture::new();
    // This independent peer guard remains live through both same-root catalog reads.
    // 此独立其他插件守卫跨两个相同根目录读取保持存活。
    let peer = fixture
        .store
        .acquire_namespace_guard("managed-peer", Some(&fixture.peer))
        .expect("actual peer namespace owner");
    // Actual target removal retains a distinct exclusive File.
    // 实际目标移除保留独立排他文件。
    let mut original = fixture.reserve();
    fixture
        .store
        .publish_deactivation_retained(&mut original)
        .expect("actual target tombstone");
    // Only the original target receives borrowed authority; the actual peer becomes explicitly unavailable.
    // 仅原目标取得借用权威；实际其他插件变为明确不可用。
    let actual = fixture
        .catalog(&original)
        .expect("complete guarded removal catalog");
    assert!(!actual.plugins.contains_key("target-plugin"));
    assert_eq!(
        actual.plugins["managed-peer"].availability,
        VulcanPluginAvailability::Unavailable
    );
    assert!(!actual.plugins.contains_key("flat-peer"));
    drop(original);
    assert_equivalent(&actual, &fixture.ordinary());
    drop(peer);
}

/// A foreign valid producer record appearing between actual reads is rejected without consuming the owner or touching its bytes.
/// 实际读取之间出现外来有效生产者记录时拒绝，不消费所有者或触碰其字节。
#[test]
fn deactivation_catalog_refuses_foreign_root_and_actual_second_read_replacement() {
    let fixture = Fixture::new();
    // The other actual store produces a distinct valid tombstone with a strictly different revision.
    // 另一实际存储产生具有严格不同修订的独立有效停用记录。
    let other = Fixture::new();
    // Produce actual foreign bytes only with the existing sole selection writer.
    // 仅使用既有唯一选择写入器产生实际外来字节。
    let first = other
        .store
        .deactivate("target-plugin", other.selected.revision)
        .expect("actual foreign initial removal");
    // This second actual writer prevents coincidentally identical revision-two tombstones.
    // 此第二个实际写入器阻止偶然相同第二修订停用记录。
    let foreign = other
        .store
        .deactivate("target-plugin", first.revision)
        .expect("actual distinct foreign record");
    // Original authority remains retained after both wrong-root and actual readback conflicts.
    // 原权威在错误根及实际回读冲突之后均保持保留。
    let mut original = fixture.reserve();
    fixture
        .store
        .publish_deactivation_retained(&mut original)
        .expect("actual original publication");
    assert_eq!(
        other
            .catalog(&original)
            .expect_err("foreign root refused")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    // The scoped observer writes real producer bytes after the first validation, before the actual pin reader.
    // 作用域观察器在第一次验证之后及实际固定读取器之前写入真实生产者字节。
    let _observer = original.replace_catalog_selection_after_validation_for_test(
        serde_json::to_vec(&foreign).expect("actual foreign producer encoding"),
    );
    assert_eq!(
        fixture
            .catalog(&original)
            .expect_err("exact second read rejects foreign selection")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert!(fixture.target.package_root().is_dir());
    assert!(
        fixture
            .store
            .reconcile_deactivation_retained(&original)
            .is_err()
    );
    drop(_observer);
    drop(original);
    assert_eq!(
        fixture
            .store
            .activation("target-plugin")
            .expect("actual unchanged foreign record"),
        Some(foreign)
    );
}
