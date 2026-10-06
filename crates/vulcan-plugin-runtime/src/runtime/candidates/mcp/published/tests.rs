//! Real managed selections and filesystem owners exercise published MCP source preparation without native launch.
//! 真实受管选择及文件系统所有者验证已发布 MCP 来源准备，不启动原生传输。

use super::*;
use crate::{
    PinnedVulcanPluginGeneration, VulcanPluginConfigurationSelection,
    VulcanPluginCredentialBackend, VulcanPluginCredentialKey, VulcanPluginCredentialRevision,
    VulcanPluginCredentialValues,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Empty explicit credential revisions must never access the secure backend.
/// 显式空凭据修订绝不能访问安全后端。
struct EmptyBackend;

impl VulcanPluginCredentialBackend for EmptyBackend {
    /// Reject plugin/revision/key reads and return an error; no secret is declared by this fixture.
    /// 拒绝插件／修订／键读取并返回错误；此夹具未声明秘密。
    fn read(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Err("Unexpected empty-fixture read.".into())
    }
    /// Reject plugin/revision/key/value writes and return an error without retaining any value.
    /// 拒绝插件／修订／键／值写入并返回错误，不保留任何值。
    fn write(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
        _: &str,
    ) -> Result<(), String> {
        Err("Unexpected empty-fixture write.".into())
    }
    /// Reject plugin/revision/key deletion and return an error because this fixture has no secure entries.
    /// 拒绝插件／修订／键删除并返回错误，因为此夹具没有安全条目。
    fn delete(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        Err("Unexpected empty-fixture delete.".into())
    }
}

/// An actual version-four selected package, credentials and catalog belong to one production manager.
/// 实际第四版已选插件包、凭据及目录属于单个生产管理器。
struct Fixture {
    /// Real manager supplies the exact epoch and immutable snapshot Arc.
    /// 真实管理器提供精确实例及不可变快照 Arc。
    manager: VulcanPluginRuntimeManager,
    /// Independent store writers mutate this actual canonical namespace.
    /// 独立存储写入者变更此实际规范命名空间。
    store: VulcanPluginGenerationStore,
    /// Real package pin is required by every production writer.
    /// 每个生产写入者要求真实插件包固定。
    package: PinnedVulcanPluginGeneration,
    /// Explicit completed credential revision remains fixed through source preparation.
    /// 来源准备期间显式完成的凭据修订保持固定。
    credentials: VulcanPluginCredentialRevision,
    /// Actual writer produced the complete current activation.
    /// 实际写入者产生完整当前激活。
    selected: VulcanPluginActivation,
    /// Snapshot retains the same selected package and server contributions.
    /// 快照保留同一已选插件包及服务器贡献。
    snapshot: Arc<VulcanPluginCatalogSnapshot>,
    /// Remove fixture files after the real owners above have retired.
    /// 上述真实所有者退役后移除夹具文件。
    temp: tempfile::TempDir,
}

/// Build an engine-free real manager over root and return the production runtime owner.
/// 在 root 上构建无引擎真实管理器，并返回生产运行时所有者。
fn manager(root: &Path) -> VulcanPluginRuntimeManager {
    fs::create_dir_all(root.join("user")).expect("user");
    fs::create_dir_all(root.join("bundled")).expect("bundled");
    VulcanPluginRuntimeManager::without_runtime(
        root.join("bundled"),
        root.join("user"),
        root.join("bridge"),
        root.join("distribution"),
        root.join("state"),
        root.join("environments"),
    )
}

/// Build real remote and optional current-platform native declarations with a complete selected configuration.
/// 构建真实远端及可选当前平台原生声明，并提供完整已选配置。
/// local selects an inert binary; return actual production ownership without launching the binary.
/// local 选择惰性二进制；返回实际生产所有权，不启动二进制。
fn fixture(local: bool) -> Fixture {
    // Every test owns independent actual files instead of fabricated snapshots or journals.
    // 每项测试持有独立实际文件，不伪造快照或日志。
    let temp = tempfile::tempdir().expect("published MCP fixture");
    // The manager is the sole issuer of the actual immutable catalog used below.
    // 管理器是下述实际不可变目录的唯一签发者。
    let manager = manager(temp.path());
    // This source is copied into a real immutable generation before publication.
    // 此来源在发布前复制至真实不可变代次。
    let source = temp.path().join("source/published-mcp");
    fs::create_dir_all(&source).expect("source");
    fs::write(source.join("schema.json"), r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#).expect("schema");
    fs::write(source.join("defaults.json"), r#"{"label":"default"}"#).expect("defaults");
    // Optional native bytes are deliberately inert; preparation must only resolve and retain them.
    // 可选原生字节有意为惰性；准备只能解析及保留它们。
    let (permissions, execution, stdio) = if local {
        fs::write(source.join("dependencies.yaml"), "{}\n").expect("dependencies");
        fs::write(source.join("fixture.bin"), "inert-published-fixture").expect("inert binary");
        // The exact current host target is obtained from the existing production resolver.
        // 精确当前宿主目标取自既有生产解析器。
        let target = crate::runtime::process::current_plugin_target_triple().expect("platform");
        (
            "network_https, config, process_binary, process_session",
            format!(
                "execution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\n  native_binaries:\n    - id: native\n      targets:\n        {target}: fixture.bin\n"
            ),
            "    - id: local\n      name: Local\n      transport:\n        type: stdio\n        launcher: {type: native_binary, binary_id: native}\n",
        )
    } else {
        ("network_https, config", String::new(), "")
    };
    fs::write(source.join("plugin.yaml"), format!("schema_version: 1\nid: published-mcp\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Published MCP\n  capability: Published source test\npermissions: [{permissions}]\nconfig:\n  schema_file: schema.json\n  defaults_file: defaults.json\n{execution}mcp:\n  servers:\n{stdio}    - id: remote\n      name: Remote\n      transport:\n        type: http\n        url: {{source: literal, value: 'https://example.test/mcp'}}\n")).expect("manifest");
    // Actual inspection validates the package, digest and permission contract.
    // 实际检查验证插件包、摘要及权限契约。
    let inspected = crate::inspect_user_plugin_package(&source).expect("inspect");
    // Production permission derivation avoids copying or guessing the required capability set.
    // 生产权限派生避免复制或猜测所需能力集合。
    let approval = crate::runtime::plugin_required_permissions(&inspected).expect("approval");
    // The canonical store belongs to this actual manager's configured user root.
    // 规范存储属于此实际管理器配置的用户根目录。
    let store = VulcanPluginGenerationStore::open(&manager.state.user_plugin_root).expect("store");
    // Keep a real pinned package for later competing writers.
    // 为后续竞争写入者保留真实固定插件包。
    let package = store
        .prepare(&source, &inspected.content_fingerprint)
        .expect("package");
    // A genuine completed empty revision is distinct from an unbound selection.
    // 真实完成的空修订独立于未绑定选择。
    let credentials = VulcanPluginCredentialRevision::allocate();
    store
        .begin_credential_staging(
            &package,
            credentials.clone(),
            &VulcanPluginCredentialValues::new(),
        )
        .expect("stage")
        .write_and_finish(&EmptyBackend, &VulcanPluginCredentialValues::new())
        .expect("complete");
    // Only a real version-four writer supplies the selected configuration source.
    // 仅真实第四版写入者提供已选配置来源。
    let selected = store
        .activate_configured(
            &package,
            0,
            approval,
            credentials.clone(),
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"label":"original"}),
            },
        )
        .expect("activate");
    // Refresh assigns real routing generations and retains actual selected pins.
    // 刷新分配真实路由代次，并保留实际已选固定。
    let snapshot = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("catalog");
    Fixture {
        manager,
        store,
        package,
        credentials,
        selected,
        snapshot,
        temp,
    }
}

/// Preserve source identity and fixed configuration while independent writers are excluded through dispatch.
/// 派发期间排除独立写入者，同时保留来源身份及固定配置。
/// No inputs or return value; source identity is explicitly not an actual-launch assertion.
/// 无输入或返回值；来源身份明确不是实际启动断言。
#[test]
fn published_mcp_same_profile_and_store_fence_survive_synchronous_dispatch() {
    // The fixture creates actual package, credential and configuration ownership.
    // 夹具创建实际插件包、凭据及配置所有权。
    let fixture = fixture(false);
    // The same registration is shared throughout preparation and final dispatch.
    // 准备及最终派发全过程共享同一注册。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // This owner retains the selected real resources without any activation request File.
    // 此所有者保留选定真实资源，不含任何激活请求 File。
    let prepared = fixture
        .manager
        .prepare_published_mcp_resources(&fixture.snapshot, "published-mcp", &authority)
        .expect("prepare");
    // The profile is pure source values, never a substitute for actual native launch inputs.
    // 画像是纯来源值，绝不能替代实际原生启动输入。
    let profile = prepared.host_profile(&authority).expect("profile");
    assert_eq!(
        prepared
            .servers(&authority)
            .expect("servers")
            .iter()
            .find(|server| server.id == "remote")
            .expect("remote declaration")
            .configuration()
            .and_then(|selection| selection.value()),
        Some(&serde_json::json!({"label":"original"}))
    );
    // This separate backend shares only the actual filesystem locks.
    // 此独立后端仅共享实际文件系统锁。
    let independent =
        VulcanPluginGenerationStore::open_existing(&fixture.manager.state.user_plugin_root)
            .expect("open")
            .expect("existing");
    // Final dispatch retains both the manager gate and the independent store writer fence.
    // 最终派发同时保留管理器门及独立存储写入屏障。
    let guard = prepared
        .begin_dispatch(&fixture.manager, &authority)
        .expect("dispatch");
    assert!(profile.same_source(&guard.host_profile()));
    assert!(matches!(
        fixture.manager.state.session_registration_gate.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    // A real competing activation write fails with the typed Busy class.
    // 真实竞争激活写入以类型化 Busy 分类失败。
    let blocked = independent
        .activate_configured(
            &fixture.package,
            fixture.selected.revision,
            fixture.selected.approved_permissions().to_vec(),
            fixture.credentials.clone(),
            fixture.selected.configuration().clone(),
        )
        .expect_err("shared fence excludes writer");
    assert_eq!(blocked.code, crate::VulcanPluginStoreErrorCode::Busy);
    drop(guard);
    independent
        .activate_configured(
            &fixture.package,
            fixture.selected.revision,
            fixture.selected.approved_permissions().to_vec(),
            fixture.credentials.clone(),
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"label":"changed"}),
            },
        )
        .expect("writer after guard release");
    assert!(
        prepared
            .begin_dispatch(&fixture.manager, &authority)
            .is_err()
    );
    assert_eq!(
        prepared
            .servers(&authority)
            .expect("frozen servers")
            .iter()
            .find(|server| server.id == "remote")
            .expect("remote")
            .configuration()
            .and_then(|selection| selection.value()),
        Some(&serde_json::json!({"label":"original"}))
    );
}

/// Reject a copied snapshot, another manager and a distinct issuer registered on the same manager.
/// 拒绝复制快照、另一管理器及同一管理器注册的不同签发者。
/// No inputs or return value; all source failures occur before native execution or local preparation.
/// 无输入或返回值；全部来源失败发生在原生执行或本地准备前。
#[test]
fn published_mcp_rejects_foreign_snapshot_manager_and_distinct_issuer() {
    // Retain the original actual immutable catalog from its creating manager.
    // 保留创建管理器产生的原实际不可变目录。
    let fixture = fixture(false);
    // Registration identity remains distinct even when the same manager registers another host.
    // 即使同一管理器注册另一宿主，注册身份仍保持不同。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // Copying snapshot contents does not copy the actual publication identity.
    // 复制快照内容不能复制实际发布身份。
    let copied = Arc::new((*fixture.snapshot).clone());
    assert!(
        fixture
            .manager
            .prepare_published_mcp_resources(&copied, "published-mcp", &authority)
            .is_err()
    );
    // A distinct manager over the same paths still has a different real issuer incarnation.
    // 相同路径上的不同管理器仍具有不同真实签发者实例。
    let foreign = manager(fixture.temp.path());
    // The original manager cannot adopt another manager's registered host.
    // 原管理器不能采用另一管理器的注册宿主。
    let foreign_authority = foreign.register_native_mcp_host_adapter();
    assert!(
        fixture
            .manager
            .prepare_published_mcp_resources(&fixture.snapshot, "published-mcp", &foreign_authority)
            .is_err()
    );
    // This resource owner belongs to the sole original registration.
    // 此资源所有者属于唯一原注册。
    let prepared = fixture
        .manager
        .prepare_published_mcp_resources(&fixture.snapshot, "published-mcp", &authority)
        .expect("prepare");
    // Fresh registration is never equivalent to cloning the original issuer.
    // 新注册绝不等同于克隆原签发者。
    let other = fixture.manager.register_native_mcp_host_adapter();
    assert!(prepared.identity(&other).is_err());
    assert!(prepared.servers(&other).is_err());
    assert!(prepared.host_profile(&other).is_err());
    assert!(prepared.begin_dispatch(&fixture.manager, &other).is_err());
    assert!(prepared.begin_dispatch(&foreign, &authority).is_err());
    assert!(
        prepared
            .begin_dispatch(&fixture.manager, &authority.clone())
            .is_ok()
    );
    assert_eq!(
        prepared.identity(&authority).expect("original route"),
        (
            "published-mcp",
            fixture.snapshot.plugins["published-mcp"].generation
        )
    );
}

/// Preserve old Enabled admission during a real Prepared update but reject the same selected unfinished target.
/// 真实 Prepared 更新期间保留旧 Enabled 入场，但拒绝相同已选未完成目标。
/// No inputs or return value; both outcomes use original production transaction records and the same predicate.
/// 无输入或返回值；两个结果使用原生产事务记录及同一谓词。
#[test]
fn published_mcp_real_prepared_old_admission_and_selected_target_rejection() {
    // Actual old business selection is already published and owns real pins.
    // 实际旧业务选择已发布，并持有真实固定。
    let fixture = fixture(false);
    // This long-lived registration remains the same across the original update.
    // 此长期注册在原更新期间保持相同。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // A real Prepared request names a successor configuration while leaving the old selection current.
    // 真实 Prepared 请求指定后继配置，同时保持旧选择为当前。
    let mut transaction = fixture
        .store
        .begin_activation_transaction(
            uuid::Uuid::new_v4(),
            &fixture.package,
            fixture.selected.revision,
            fixture.selected.approved_permissions().to_vec(),
            fixture.credentials.clone(),
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"label":"successor"}),
            },
        )
        .expect("Prepared original");
    // The same ordinary rule must continue to admit the actual selected predecessor.
    // 同一普通规则必须继续接纳实际已选前驱。
    let prepared = fixture
        .manager
        .prepare_published_mcp_resources(&fixture.snapshot, "published-mcp", &authority)
        .expect("old Prepared business allowed");
    assert!(
        prepared
            .begin_dispatch(&fixture.manager, &authority)
            .is_ok()
    );
    fixture
        .store
        .commit_activation_transaction(&mut transaction)
        .expect("publish actual target");
    // The original actual Arc remains current locally, while the real Store now selects its unfinished target.
    // 原实际 Arc 在本地仍为当前，而真实存储现已选中其未完成目标。
    // Rejection must follow the original pending-target rule before any descriptor reselection or native dispatch.
    // 拒绝必须遵循原待处理目标规则，先于任何描述符重选或原生派发。
    let rejected = fixture
        .manager
        .prepare_published_mcp_resources(&fixture.snapshot, "published-mcp", &authority)
        .err()
        .expect("selected unfinished target rejected");
    assert_eq!(
        rejected.message,
        "Selected activation target is awaiting its original route handoff."
    );
}

/// Retain the actual environment File across owner clones, while a source profile survives without blocking cleanup.
/// 在所有者克隆间保留实际环境 File，而来源画像存续时不阻挡清理。
/// No inputs or return value; the native fixture remains inert and no transport success is claimed.
/// 无输入或返回值；原生夹具保持惰性，不声称传输成功。
#[test]
fn published_mcp_profile_does_not_retain_real_environment_owner() {
    // A real current-platform native declaration requires actual environment retention before launcher preparation.
    // 真实当前平台原生声明要求在启动器准备前实际保留环境。
    let fixture = fixture(true);
    // The original authority is carried into the prepared resource bundle.
    // 原权威传入已准备资源包。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // Resolve inert bytes under real File protection without executing them.
    // 在真实 File 保护下解析惰性字节，不执行它们。
    let prepared = fixture
        .manager
        .prepare_published_mcp_resources(&fixture.snapshot, "published-mcp", &authority)
        .expect("prepare local");
    // Pure values must remain independently usable after all execution owners retire.
    // 全部执行所有者退役后，纯值必须仍可独立使用。
    let profile = prepared.host_profile(&authority).expect("profile");
    // Same original resource clone retains the actual shared environment File.
    // 同一原资源克隆保留实际共享环境 File。
    let retained = prepared.clone();
    // The frozen launcher points at the actual completed execution copy.
    // 固定启动器指向实际完成的执行副本。
    let program = prepared
        .stdio_launcher(&authority, "local")
        .expect("launcher")
        .program
        .clone();
    assert!(program.is_file());
    // Independent cleanup has no common Rust bookkeeping with this owner.
    // 独立清理与此所有者没有公共 Rust 记账。
    let cleanup = manager(fixture.temp.path());
    assert!(
        crate::runtime::environment::lock_plugin_execution_environment_cleanup(
            &cleanup.state,
            "published-mcp"
        )
        .is_err()
    );
    drop(prepared);
    assert!(
        crate::runtime::environment::lock_plugin_execution_environment_cleanup(
            &cleanup.state,
            "published-mcp"
        )
        .is_err()
    );
    drop(retained);
    // Actual File release permits cleanup despite the original profile still existing.
    // 即使原画像仍存在，实际 File 释放也允许清理。
    let exclusive = crate::runtime::environment::lock_plugin_execution_environment_cleanup(
        &cleanup.state,
        "published-mcp",
    )
    .expect("profile does not keep environment alive");
    assert!(profile.same_source(&profile.clone()));
    drop(exclusive);
}

/// Original pure publication source survives physical owner loss while fencing real writers without an environment File.
/// 原纯发布来源跨物理所有者丢失存续，在不持有环境 File 时对真实写入者建立屏障。
/// No inputs or return value; actual cleanup and independent writer results prove the source/resource separation.
/// 无输入及返回值；实际清理及独立写入者结果证明来源与资源分离。
#[test]
fn published_mcp_profile_publication_fences_original_without_pinning_environment() {
    // This genuine native declaration obtains the production shared environment before source extraction.
    // 此真实原生声明在提取来源之前取得生产共享环境。
    let fixture = fixture(true);
    // One registered host fixes the original private source issuer.
    // 单个已注册宿主固定原私有来源签发者。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // The physical owner precedes the pure profile and is explicitly retired before publication validation.
    // 物理所有者先于纯画像，并在发布校验之前明确退役。
    let prepared = fixture
        .manager
        .prepare_published_mcp_resources(&fixture.snapshot, "published-mcp", &authority)
        .expect("actual prepared owner");
    // This clone retains only the original source identity and validated path values.
    // 此克隆仅保留原来源身份及已校验路径值。
    let profile = prepared
        .host_profile(&authority)
        .expect("original pure source");
    drop(prepared);
    // A real independent environment cleanup File remains held across the source-only publication fence.
    // 真实独立环境清理 File 在仅来源发布屏障期间持续持有。
    let exclusive = crate::runtime::environment::lock_plugin_execution_environment_cleanup(
        &fixture.manager.state,
        "published-mcp",
    )
    .expect("pure source does not own execution environment");
    // A copied catalog and a repeat registration cannot substitute for the actual first publication.
    // 复制目录及重复注册不能替代实际首次发布。
    let copied = Arc::new((*fixture.snapshot).clone());
    assert!(
        profile
            .begin_publication(&fixture.manager, &authority, &copied)
            .is_err()
    );
    assert!(
        profile
            .begin_publication(
                &fixture.manager,
                &fixture.manager.register_native_mcp_host_adapter(),
                &fixture.snapshot,
            )
            .is_err()
    );
    // This second Store has no common Rust ownership accounting with the original source.
    // 此第二存储与原来源没有公共 Rust 所有权记账。
    let independent =
        VulcanPluginGenerationStore::open_existing(&fixture.manager.state.user_plugin_root)
            .expect("independent canonical store")
            .expect("actual existing store");
    // Only the original current source can hold the real cross-store shared namespace File.
    // 仅原当前来源可持有真实跨存储共享命名空间 File。
    let guard = profile
        .begin_publication(&fixture.manager, &authority, &fixture.snapshot)
        .expect("original source-only publication fence");
    assert!(profile.same_source(&guard.host_profile()));
    assert!(matches!(
        fixture.manager.state.session_registration_gate.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    assert_eq!(
        independent
            .activate_configured(
                &fixture.package,
                fixture.selected.revision,
                fixture.selected.approved_permissions().to_vec(),
                fixture.credentials.clone(),
                fixture.selected.configuration().clone(),
            )
            .expect_err("actual source fence excludes independent writer")
            .code,
        crate::VulcanPluginStoreErrorCode::Busy
    );
    drop(guard);
    independent
        .activate_configured(
            &fixture.package,
            fixture.selected.revision,
            fixture.selected.approved_permissions().to_vec(),
            fixture.credentials.clone(),
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"label":"replacement"}),
            },
        )
        .expect("independent writer resumes after original fence");
    assert!(
        profile
            .begin_publication(&fixture.manager, &authority, &fixture.snapshot)
            .is_err()
    );
    drop(exclusive);
}

/// A pure old source never certifies a refreshed actual catalog, even when the complete selected values are equal.
/// 即使完整选定值相等，纯旧来源也绝不认证刷新后的实际目录。
/// No inputs or return value; genuine production refresh and both original/replacement Arc checks are evidence.
/// 无输入及返回值；真实生产刷新及原／替代 Arc 的双向检查构成证据。
#[test]
fn published_mcp_profile_publication_rejects_actual_replacement_catalog() {
    // The original selection, catalog and private issuer are genuinely produced by the current manager.
    // 原选择、目录及私有签发者均由当前管理器真实产生。
    let fixture = fixture(false);
    // This first registration remains the only host accepted by the original source.
    // 此首次注册保持为原来源接受的唯一宿主。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // Do not retain the physical prepared bundle in the historical source.
    // 不在历史来源内保留物理已准备资源包。
    let profile = fixture
        .manager
        .prepare_published_mcp_resources(&fixture.snapshot, "published-mcp", &authority)
        .expect("original full source")
        .host_profile(&authority)
        .expect("pure original profile");
    // The production refresh replaces its actual immutable Arc while preserving this fixture's selected values.
    // 生产刷新替换实际不可变 Arc，同时保留此夹具选定值。
    let replacement = fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("actual catalog refresh");
    assert!(!Arc::ptr_eq(&replacement, &fixture.snapshot));
    assert_eq!(
        fixture
            .store
            .activation("published-mcp")
            .expect("actual selected values"),
        Some(fixture.selected.clone())
    );
    assert!(
        profile
            .begin_publication(&fixture.manager, &authority, &replacement)
            .err()
            .expect("replacement cannot acquire original source authority")
            .to_string()
            .contains("original publication catalog")
    );
    assert!(
        profile
            .begin_publication(&fixture.manager, &authority, &fixture.snapshot)
            .err()
            .expect("retired original cannot acquire current source authority")
            .to_string()
            .contains("actual current catalog")
    );
}

/// Reject a real replacement catalog after preparation.
/// 拒绝准备后的真实替代目录。
/// No inputs or return value; no existing runtime receives retrospective provenance.
/// 无输入或返回值；不向任何既有运行时追授来源证明。
#[test]
fn published_mcp_rejects_replaced_catalog() {
    // Begin with complete actual managed ownership.
    // 从完整实际受管所有权开始。
    let original = fixture(false);
    // One actual host registration fixes the original authority.
    // 单次实际宿主注册固定原权威。
    let authority = original.manager.register_native_mcp_host_adapter();
    // Retain the exact initial source before a genuine catalog replacement.
    // 在真实目录替代前保留精确初始来源。
    let prepared = original
        .manager
        .prepare_published_mcp_resources(&original.snapshot, "published-mcp", &authority)
        .expect("prepare");
    original
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("actual refresh");
    assert!(
        prepared
            .begin_dispatch(&original.manager, &authority)
            .is_err()
    );
}
