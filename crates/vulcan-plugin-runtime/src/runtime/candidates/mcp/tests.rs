//! Real transaction locks and immutable packages exercise unpublished MCP preparation.
//! 通过真实事务锁与不可变插件包验证未发布 MCP 准备。

/// Keeps actual prepared launch files alive against the current targeted production cleanup.
/// 对现行定向生产清理保留实际准备启动文件。
/// No inputs; asserts shared ownership across clones and independent managers until final release.
/// 无输入；断言克隆与独立管理器之间的共享所有权直至最后释放。
#[test]
fn prepared_mcp_candidate_blocks_environment_reclamation_until_last_owner() {
    // Use the current host declaration without executing its inert fixture binary.
    // 使用当前宿主声明，不执行其惰性夹具二进制。
    let fixture = fixture(
        Some(crate::runtime::process::current_plugin_target_triple().expect("host platform")),
        false,
    );
    // The inspected view supplies the unchanged original request authority.
    // 已检查视图提供未改变的原请求权威。
    let candidate = fixture
        .store
        .inspect_activation_candidate(&fixture.transaction)
        .expect("inspect candidate authority");
    // Preparation owns the actual environment lease before resolving its launch files.
    // 准备在解析启动文件前拥有实际环境租约。
    let prepared = fixture
        .manager
        .prepare_mcp_activation_candidate(&candidate)
        .expect("prepare candidate environment");
    // Keep the clone after releasing the foreground prepared object.
    // 释放前台准备对象后保留克隆。
    let retained = prepared.clone();
    // This path must remain usable throughout the shared owner's lifetime.
    // 此路径必须在共享所有者存续期间始终可用。
    let program = prepared
        .stdio_launcher("local")
        .expect("local launcher")
        .program
        .clone();
    // The cleanup identity comes from the exact inspected package.
    // 清理身份来自精确已检查插件包。
    let plugin = candidate.package();
    // A separate manager proves filesystem exclusion rather than local manager bookkeeping.
    // 独立管理器证明文件系统互斥，而非本地管理器记账。
    let cleanup_manager = manager(fixture._temp.path());
    drop(prepared);
    assert!(
        crate::runtime::environment::remove_plugin_execution_environment_generation(
            &cleanup_manager.state,
            &plugin.id,
            plugin.version.as_deref().expect("validated version"),
            &plugin.content_fingerprint,
        )
        .is_err(),
        "live candidate environment must reject targeted deletion"
    );
    assert!(
        program.is_file(),
        "failed cleanup must preserve exact launch bytes"
    );
    drop(retained);
    crate::runtime::environment::remove_plugin_execution_environment_generation(
        &cleanup_manager.state,
        &plugin.id,
        plugin.version.as_deref().expect("validated version"),
        &plugin.content_fingerprint,
    )
    .expect("final owner release permits reclamation");
    assert!(!program.exists());
    // A later preparation must use the permanent same lock identity after environment recreation.
    // 环境重新创建后的准备必须使用永久的同一锁身份。
    let reopened = fixture
        .manager
        .prepare_mcp_activation_candidate(&candidate)
        .expect("recreate released environment");
    assert!(
        reopened
            .stdio_launcher("local")
            .expect("recreated launcher")
            .program
            .is_file()
    );
}

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::{
    VulcanPluginActivationTransaction, VulcanPluginConfigurationSelection,
    VulcanPluginCredentialBackend, VulcanPluginCredentialBinding, VulcanPluginCredentialKey,
    VulcanPluginCredentialRevision, VulcanPluginCredentialValues, VulcanPluginGenerationStore,
    VulcanPluginRuntimeManager, VulcanPluginStoreErrorCode,
};

/// Rejects secret storage calls because this fixture has a genuine empty credential revision.
/// 拒绝秘密存储调用，因为此夹具具有真实的空凭据修订。
struct NoSecrets;

impl VulcanPluginCredentialBackend for NoSecrets {
    /// Rejects a read for the supplied identity; returns no fabricated secret.
    /// 拒绝给定身份的读取；不返回伪造秘密。
    fn read(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Err(String::from("Unexpected empty-fixture secret read."))
    }
    /// Rejects a write for the supplied identity and value; no external state changes.
    /// 拒绝给定身份和值的写入；不改变外部状态。
    fn write(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
        _: &str,
    ) -> Result<(), String> {
        Err(String::from("Unexpected empty-fixture secret write."))
    }
    /// Rejects deletion for the supplied identity; the fixture owns no external entry.
    /// 拒绝给定身份的删除；夹具不拥有外部条目。
    fn delete(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        Err(String::from("Unexpected empty-fixture secret delete."))
    }
}

/// Holds production store authorities and an engine-free manager in one isolated directory.
/// 在单个隔离目录中持有生产存储权威及无引擎管理器。
struct Fixture {
    /// Exact manager whose configured user root owns the transaction.
    /// 配置用户根拥有事务的精确管理器。
    manager: VulcanPluginRuntimeManager,
    /// Production immutable store used for every ownership assertion.
    /// 每项所有权断言使用的生产不可变存储。
    store: VulcanPluginGenerationStore,
    /// Original exclusive request owner, explicitly dropped in retention tests.
    /// 原排他请求所有者，在保留测试中显式释放。
    transaction: VulcanPluginActivationTransaction,
    /// Target credential revision differs from any published predecessor.
    /// 目标凭据修订与任何已发布前驱不同。
    credentials: VulcanPluginCredentialRevision,
    /// Source remains available only for constructing a genuine bundled precedence conflict.
    /// 来源仅为构造真实内置优先级冲突而保留。
    source: PathBuf,
    /// Removed last, after the owning handles above have been dropped.
    /// 在上述所有者句柄释放后最后移除。
    _temp: tempfile::TempDir,
}

/// Creates an engine-free manager with an existing isolated user root.
/// 创建具有现存隔离用户根的无引擎管理器。
/// The root selects every runtime directory; returns an ordinary production manager.
/// 根路径选择全部运行目录；返回普通生产管理器。
fn manager(root: &Path) -> VulcanPluginRuntimeManager {
    fs::create_dir_all(root.join("user")).expect("create user root");
    fs::create_dir_all(root.join("bundled")).expect("create bundled root");
    VulcanPluginRuntimeManager::without_runtime(
        root.join("bundled"),
        root.join("user"),
        root.join("bridge"),
        root.join("distribution"),
        root.join("state"),
        root.join("environments"),
    )
}

/// Builds remote MCP plus an optional native launcher, optionally publishing a different predecessor selection.
/// 构建远端 MCP 及可选原生启动器，并可发布不同的前驱选择。
/// The target is an exact declared platform; returns real validated transaction and resource ownership.
/// target 为精确声明平台；返回真实已校验事务及资源所有权。
fn fixture(target: Option<&str>, publish_predecessor: bool) -> Fixture {
    // All executable bytes are inert fixture data; preparation must never execute them.
    // 全部可执行字节均为惰性夹具数据；准备绝不能执行它们。
    let temp = tempfile::tempdir().expect("MCP candidate fixture");
    let manager = manager(temp.path());
    let source = temp.path().join("source/candidate-mcp");
    fs::create_dir_all(&source).expect("create source");
    fs::write(source.join("dependencies.yaml"), "{}\n").expect("dependencies");
    fs::write(source.join("fixture.bin"), "candidate-only-binary-bytes").expect("inert binary");
    fs::write(source.join("schema.json"),
        r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#,
    ).expect("configuration schema");
    fs::write(
        source.join("defaults.json"),
        r#"{"label":"package-default"}"#,
    )
    .expect("defaults");
    // Native declarations are selected by the test; every other manifest authority is identical.
    // 原生声明由测试选择；其余全部清单权威一致。
    let (permissions, execution, stdio) = match target {
        Some(target) => (
            "network_https, config, process_binary, process_session",
            format!(
                "execution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\n  native_binaries:\n    - id: native\n      targets:\n        {target}: fixture.bin\n"
            ),
            "    - id: local\n      name: Local candidate\n      transport:\n        type: stdio\n        launcher: {type: native_binary, binary_id: native}\n",
        ),
        None => ("network_https, config", String::new(), ""),
    };
    fs::write(source.join("plugin.yaml"), format!(
        "schema_version: 1\nid: candidate-mcp\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Candidate MCP\n  capability: Private MCP fixture\npermissions: [{permissions}]\nconfig:\n  schema_file: schema.json\n  defaults_file: defaults.json\n{execution}mcp:\n  servers:\n{stdio}    - id: remote\n      name: Remote candidate\n      transport:\n        type: http\n        url: {{source: literal, value: 'https://example.test/mcp'}}\n"
    )).expect("candidate manifest");
    // Production inspection determines the approved permissions and content fingerprint.
    // 生产检查决定已批准权限及内容指纹。
    let inspected = crate::inspect_user_plugin_package(&source).expect("inspect source");
    let permissions = crate::runtime::plugin_required_permissions(&inspected).expect("permissions");
    let store = VulcanPluginGenerationStore::open(&manager.state.user_plugin_root).expect("store");
    let package = store
        .prepare(&source, &inspected.content_fingerprint)
        .expect("immutable package");
    let credentials = VulcanPluginCredentialRevision::allocate();
    // The predecessor and candidate receive separate actual credential staging checkpoints.
    // 前驱与候选分别具有真实凭据暂存检查点。
    let predecessor_credentials = VulcanPluginCredentialRevision::allocate();
    for revision in [&credentials, &predecessor_credentials] {
        store
            .begin_credential_staging(
                &package,
                revision.clone(),
                &VulcanPluginCredentialValues::new(),
            )
            .expect("credential staging")
            .write_and_finish(&NoSecrets, &VulcanPluginCredentialValues::new())
            .expect("empty credential completion");
    }
    let expected_revision = if publish_predecessor {
        let active = store
            .activate_configured(
                &package,
                0,
                permissions.clone(),
                predecessor_credentials,
                VulcanPluginConfigurationSelection::Document {
                    value: serde_json::json!({"label":"published-predecessor"}),
                },
            )
            .expect("publish predecessor");
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("public predecessor");
        active.revision
    } else {
        0
    };
    let transaction = store
        .begin_activation_transaction(
            Uuid::new_v4(),
            &package,
            expected_revision,
            permissions,
            credentials.clone(),
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"label":"private-candidate-value"}),
            },
        )
        .expect("prepare original request");
    Fixture {
        manager,
        store,
        transaction,
        credentials,
        source,
        _temp: temp,
    }
}

/// Prepares exact candidate bytes while ordinary resolution still rejects its unassigned generation.
/// 准备精确候选字节，同时普通解析仍拒绝其未分配代次。
/// No inputs or return value; checks configuration, credentials, launch paths and unchanged public selection.
/// 无输入与返回值；检查配置、凭据、启动路径及未改变的公开选择。
#[test]
fn prepared_mcp_candidate_preserves_exact_authority_without_publication() {
    // A published predecessor makes accidental public lookups observable.
    // 已发布前驱使意外公开查找可被观察。
    let target = crate::runtime::process::current_plugin_target_triple().expect("platform");
    let mut fixture = fixture(Some(target), true);
    let original = fixture
        .store
        .activation("candidate-mcp")
        .expect("original selection");
    let prepared = {
        let candidate = fixture
            .store
            .inspect_activation_candidate(&fixture.transaction)
            .expect("inspect");
        fixture
            .manager
            .prepare_mcp_activation_candidate(&candidate)
            .expect("prepare MCP")
    };
    assert_eq!(prepared.plugin_id(), "candidate-mcp");
    assert_eq!(
        prepared.request_id(),
        fixture.transaction.record().request_id()
    );
    assert!(!format!("{prepared:?}").contains("private-candidate-value"));
    for server in prepared.servers() {
        assert_eq!(server.plugin_generation, 0);
        assert_eq!(
            server.configuration().and_then(|selected| selected.value()),
            Some(&serde_json::json!({"label":"private-candidate-value"}))
        );
        assert_eq!(
            server.credential_binding,
            VulcanPluginCredentialBinding::Revision(fixture.credentials.clone())
        );
    }
    // The candidate is never smuggled through the ordinary published generation gate.
    // 绝不让候选绕过普通已发布代次关卡。
    let server = prepared
        .servers()
        .iter()
        .find(|server| server.id == "local")
        .expect("expected local server may have changed");
    assert!(
        fixture
            .manager
            .resolve_mcp_stdio_launcher_descriptor(server)
            .expect_err("ordinary routing must reject zero generation")
            .message
            .contains("stale plugin generation")
    );
    let launcher = prepared
        .stdio_launcher("local")
        .expect("exact private launcher");
    assert_eq!(
        fs::read_to_string(&launcher.program).expect("actual candidate bytes"),
        "candidate-only-binary-bytes"
    );
    assert!(launcher.program.starts_with(&launcher.cwd));
    assert!(
        launcher.cwd.starts_with(
            fixture
                .manager
                .state
                .plugin_runtime_root
                .canonicalize()
                .expect("runtime root")
        )
    );
    assert!(prepared.stdio_launcher("remote").is_err());
    assert!(prepared.stdio_launcher("missing").is_err());
    assert_eq!(
        fixture
            .store
            .activation("candidate-mcp")
            .expect("selection after preparation"),
        original
    );
    assert!(
        fixture
            .manager
            .formal_runtime_status()
            .expect("runtime status")
            .is_none()
    );
    assert_eq!(
        fixture
            .store
            .commit_activation_transaction(&mut fixture.transaction)
            .expect_err("prepared resource owner fences commit")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        fixture
            .store
            .abort_activation_transaction(&mut fixture.transaction)
            .expect_err("prepared resource owner fences abort")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(prepared);
    fixture
        .store
        .abort_activation_transaction(&mut fixture.transaction)
        .expect("released resources permit abort");
}

/// The real request lock survives loss of its original transaction and every foreground prepared view.
/// 原事务及全部前台准备视图消失后，真实请求锁仍存续。
/// No inputs or return value; the final resource owner alone controls recovery eligibility.
/// 无输入与返回值；仅最终资源所有者决定是否可恢复。
#[test]
fn prepared_mcp_candidate_retains_original_request_after_foreground_drop() {
    // A remote-only candidate proves retention does not depend on a process or Lua engine already existing.
    // 仅远端候选证明保留不依赖进程或 Lua 引擎已存在。
    let fixture = fixture(None, false);
    let request_id = fixture.transaction.record().request_id();
    let candidate = fixture
        .store
        .inspect_activation_candidate(&fixture.transaction)
        .expect("inspect");
    let prepared = fixture
        .manager
        .prepare_mcp_activation_candidate(&candidate)
        .expect("prepare remote");
    let retained = prepared.clone();
    drop(candidate);
    drop(prepared);
    drop(fixture.transaction);
    let reopened =
        VulcanPluginGenerationStore::open_existing(&fixture.manager.state.user_plugin_root)
            .expect("independent store")
            .expect("existing store");
    assert_eq!(
        reopened
            .resume_activation_transaction("candidate-mcp", request_id)
            .expect_err("last prepared clone owns actual OS lock")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(fixture.manager);
    assert_eq!(
        reopened
            .resume_activation_transaction("candidate-mcp", request_id)
            .expect_err("manager destruction must not release another owner's lock")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(retained);
    let mut resumed = reopened
        .resume_activation_transaction("candidate-mcp", request_id)
        .expect("exact request recovery");
    reopened
        .abort_activation_transaction(&mut resumed)
        .expect("clean recovered request");
}

/// Preparation rejects foreign catalogs and bundled namespace shadowing before materializing environments.
/// 准备在物化环境前拒绝外部目录及内置命名空间覆盖。
/// No inputs or return value; both rejections leave the inspected transaction recoverable.
/// 无输入与返回值；两种拒绝均保持已检查事务可恢复。
#[test]
fn prepared_mcp_candidate_rejects_foreign_catalog_and_bundled_shadow() {
    let mut fixture = fixture(None, false);
    let foreign_root = tempfile::tempdir().expect("foreign root");
    let foreign = manager(foreign_root.path());
    {
        let candidate = fixture
            .store
            .inspect_activation_candidate(&fixture.transaction)
            .expect("inspect");
        assert!(
            foreign
                .prepare_mcp_activation_candidate(&candidate)
                .expect_err("foreign catalog rejected")
                .message
                .contains("another user catalog")
        );
        // Use a genuine bundled source with the same namespace, not a forged snapshot field.
        // 使用相同命名空间的真实内置来源，而非伪造快照字段。
        crate::runtime::local_files::copy_plugin_directory_tree(
            &fixture.source,
            &fixture
                .manager
                .state
                .vulcan_plugin_root
                .join("candidate-mcp"),
        )
        .expect("bundled copy");
        fixture
            .manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("bundled precedence");
        assert!(
            fixture
                .manager
                .prepare_mcp_activation_candidate(&candidate)
                .expect_err("bundled owner rejected")
                .message
                .contains("shadow a bundled")
        );
    }
    assert!(!fixture.manager.state.plugin_runtime_root.exists());
    fixture
        .store
        .abort_activation_transaction(&mut fixture.transaction)
        .expect("failed preparation released authority");
}

/// A failed execution-cache validation cannot leak readiness ownership after preparation returns.
/// 执行缓存校验失败后，准备返回时不能泄漏就绪所有权。
/// No inputs or return value; recovery uses a separate store and the original durable identity.
/// 无输入与返回值；恢复使用独立存储及原持久身份。
#[test]
fn failed_mcp_launcher_preparation_releases_original_request() {
    // Start with a package accepted for this actual host, then corrupt only its separate execution cache.
    // 从当前实际宿主接受的插件包开始，随后仅损坏其独立执行缓存。
    let target = crate::runtime::process::current_plugin_target_triple().expect("platform");
    let fixture = fixture(Some(target), false);
    let request_id = fixture.transaction.record().request_id();
    {
        // The store remains valid; the failure must occur after readiness ownership was retained.
        // 存储保持有效；失败必须发生在就绪所有权保留之后。
        let candidate = fixture
            .store
            .inspect_activation_candidate(&fixture.transaction)
            .expect("inspect");
        let plugin = candidate.package();
        let environment = fixture
            .manager
            .state
            .plugin_runtime_root
            .join(&plugin.id)
            .join(plugin.version.as_deref().expect("validated version"))
            .join(&plugin.content_fingerprint);
        fs::create_dir_all(environment).expect("incomplete environment cache");
        fixture
            .manager
            .prepare_mcp_activation_candidate(&candidate)
            .expect_err("existing environment has no valid completion marker");
        // Failure must also release the new environment owner while the original transaction remains held.
        // 原事务仍被持有时，失败也必须释放新增环境所有者。
        crate::runtime::environment::remove_plugin_execution_environment_generation(
            &fixture.manager.state,
            &plugin.id,
            plugin.version.as_deref().expect("version"),
            &plugin.content_fingerprint,
        )
        .expect("failed preparation released execution environment lease");
    }
    drop(fixture.transaction);
    // A distinct reacquisition proves the failed materializer retained no hidden exclusive owner.
    // 独立重新获取证明失败物化器未保留隐藏排他所有者。
    let mut resumed = fixture
        .store
        .resume_activation_transaction("candidate-mcp", request_id)
        .expect("failed preparation retained no hidden request owner");
    fixture
        .store
        .abort_activation_transaction(&mut resumed)
        .expect("abort failed preparation");
}
