//! Candidate inspection exercises the real journal, catalog projection and filesystem leases.
//! 候选检查验证真实日志、目录投影及文件系统租约。

use super::*;
use crate::{VulcanPluginAvailability, VulcanPluginPackageOrigin, VulcanPluginRuntimeState};

/// Read the public catalog at root without overriding package enablement; returns the observed published surface.
/// 在 root 读取公开目录，不覆盖插件包启停选择；返回观察到的已发布表面。
fn public_catalog(root: &Path) -> crate::VulcanPluginCatalogSnapshot {
    crate::load_vulcan_plugin_catalog(
        &root.join("bundled"),
        Some(root),
        &std::collections::BTreeSet::new(),
        &std::collections::BTreeSet::new(),
        1,
    )
}

/// Inspect a pending update while the old package remains the only public selection.
/// 检查等待更新，同时旧插件包仍是唯一公开选择。
#[test]
fn candidate_inspection_preserves_published_predecessor_and_exact_target() {
    // All package and selection bytes belong to this isolated directory.
    // 全部插件包及选择字节属于此隔离目录。
    let temp = tempfile::tempdir().expect("create candidate fixture");
    // Actual predecessor and candidate identities differ despite sharing one version number.
    // 实际前驱及候选身份不同，尽管共用同一版本号。
    let (store, first, second, _, credentials) = fixture(temp.path());
    // Keep the original transaction owner alive throughout inspection.
    // 在检查全程保持原事务所有者存活。
    let transaction = begin(&store, &second, &credentials, Uuid::new_v4());
    // Preserve every durable authority to prove the read path does not publish or repair anything.
    // 保留每个持久权威，证明读取路径不发布或修复任何内容。
    let paths = [
        store.root.join("active/sample-plugin.json"),
        store.root.join("transactions/sample-plugin/current"),
        record_path(temp.path(), transaction.record().request_id()),
    ];
    // Byte comparisons cover both the selection and the complete request checkpoint.
    // 字节比较覆盖选择及完整请求检查点。
    let originals = paths
        .iter()
        .map(|path| fs::read(path).expect("retain original authority bytes"))
        .collect::<Vec<_>>();
    // Candidate access borrows the exclusive transaction, without changing public discovery.
    // 候选访问借用排他事务，不改变公开发现。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("inspect prepared candidate");
    assert_eq!(candidate.transaction_record(), transaction.record());
    assert_eq!(candidate.package().package_root, second.package_root());
    assert_eq!(candidate.package().generation, 0);
    assert_eq!(
        candidate.package().availability,
        VulcanPluginAvailability::Disabled
    );
    assert_eq!(
        candidate.package().runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    assert_eq!(
        candidate.package().origin,
        VulcanPluginPackageOrigin::UserInstalled
    );
    assert!(!candidate.package().desired_enabled);
    assert!(!candidate.package().runtime_execution_available);
    // The exact target selection owns all managed metadata, independently from the predecessor.
    // 精确目标选择独立于前驱持有全部受管元数据。
    let managed = candidate
        .package()
        .managed_package()
        .expect("managed candidate");
    assert_eq!(managed.activation_revision(), 2);
    assert_eq!(managed.generation().identity(), second.identity());
    assert!(managed.approved_permissions().is_empty());
    assert_eq!(
        managed.credential_binding(),
        &VulcanPluginCredentialBinding::Revision(credentials)
    );
    assert_eq!(
        managed.configuration(),
        &VulcanPluginConfigurationSelection::NotDeclared {}
    );
    // A fresh production scan must still resolve the predecessor's exact physical package.
    // 新生产扫描仍须解析前驱的精确物理插件包。
    let published = public_catalog(temp.path());
    assert_eq!(
        published
            .plugins
            .get("sample-plugin")
            .expect("published predecessor")
            .package_root,
        first.package_root()
    );
    for (path, original) in paths.iter().zip(originals) {
        assert_eq!(fs::read(path).expect("read unchanged authority"), original);
    }
}

/// Build a real configuration-bearing remote package; returns its store, generation and staged credential revision.
/// 构建具有真实配置的远端插件包；返回存储、代次及已暂存凭据修订。
fn configured_candidate_fixture(
    root: &Path,
) -> (
    VulcanPluginGenerationStore,
    PinnedVulcanPluginGeneration,
    VulcanPluginCredentialRevision,
) {
    // The source path is separate from immutable generations and public flat package discovery.
    // 来源路径独立于不可变代次及公开平铺插件包发现。
    let source = root.join("source/configured-plugin");
    fs::create_dir_all(&source).expect("create configured candidate source");
    fs::write(
        source.join("plugin.yaml"),
        "schema_version: 1\nid: configured-plugin\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Candidate fixture\n  capability: Candidate configuration\npermissions: [network_https, config]\nconfig:\n  schema_file: schema.json\n  defaults_file: defaults.json\nmcp:\n  servers:\n    - id: remote\n      name: Remote\n      transport:\n        type: http\n        url: {source: literal, value: 'https://example.test/mcp'}\n",
    ).expect("write configured candidate manifest");
    fs::write(
        source.join("schema.json"),
        r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#,
    ).expect("write candidate configuration schema");
    fs::write(source.join("defaults.json"), r#"{"label":"default"}"#)
        .expect("write candidate defaults");
    // Preparation uses the same production validation as ordinary immutable installation.
    // 准备使用与普通不可变安装相同的生产校验。
    let store = VulcanPluginGenerationStore::open(root).expect("open candidate store");
    // This pin is real filesystem ownership, not a reconstructed path.
    // 此固定句柄为真实文件系统所有权，并非重建路径。
    let generation = prepare(&store, &source);
    // An empty secret set still has an actual staged revision and a reader lease.
    // 即使秘密集合为空，仍具有实际已暂存修订及读租约。
    let credentials = VulcanPluginCredentialRevision::allocate();
    stage_empty_credentials(&store, &generation, &credentials);
    (store, generation, credentials)
}

/// First installation exposes no public package until commit, while candidate and future descriptors share exact authorities.
/// 首次安装在提交前不暴露公开插件包，而候选与未来描述符共享精确权威。
#[test]
fn candidate_inspection_freezes_configuration_and_credentials_before_first_publication() {
    // This fixture has no predecessor selection or mutable legacy configuration.
    // 此夹具没有前驱选择或可变旧配置。
    let temp = tempfile::tempdir().expect("create configured candidate fixture");
    // The manifest contributes an actual remote MCP descriptor without contacting a network.
    // 清单提供实际远端 MCP 描述符，不访问网络。
    let (store, generation, credentials) = configured_candidate_fixture(temp.path());
    // The target differs from the package default to detect an incorrect configuration source.
    // 目标不同于插件包默认值，以检测错误配置来源。
    let configuration = VulcanPluginConfigurationSelection::Document {
        value: serde_json::json!({"label":"private-candidate-selection-value"}),
    };
    // The exact full permission set is derived by production package inspection.
    // 精确完整权限集合由生产插件包检查派生。
    let mut transaction = store
        .begin_activation_transaction(
            Uuid::new_v4(),
            &generation,
            0,
            generation.required_permissions.clone(),
            credentials.clone(),
            configuration.clone(),
        )
        .expect("prepare first configured activation");
    // Cloned contributions preserve real ownership after the borrowed view is released.
    // 释放借用视图后，克隆贡献仍保留实际所有权。
    let server = {
        // Borrow the complete prepared target before publication is possible.
        // 在能够发布前借用完整准备目标。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("inspect initial configured candidate");
        assert!(!format!("{candidate:?}").contains("private-candidate-selection-value"));
        assert!(candidate.transaction_record().predecessor().is_none());
        assert_eq!(
            candidate
                .package()
                .managed_package()
                .expect("managed configuration")
                .configuration(),
            &configuration
        );
        // Resolve the server by its stable identity rather than a manifest array position.
        // 按稳定身份解析服务器，而非清单数组位置。
        let server = candidate
            .package()
            .mcp_servers
            .iter()
            .find(|server| server.id == "remote")
            .expect("expected remote fixture contribution");
        assert_eq!(server.configuration(), Some(&configuration));
        assert_eq!(
            server.credential_binding,
            VulcanPluginCredentialBinding::Revision(credentials.clone())
        );
        assert!(server.credential_lease.is_some());
        assert!(
            !public_catalog(temp.path())
                .plugins
                .contains_key("configured-plugin")
        );
        server.clone()
    };
    store
        .commit_activation_transaction(&mut transaction)
        .expect("publish prepared selection");
    store
        .complete_activation_transaction(&mut transaction)
        .expect("acknowledge test route handoff");
    // Public discovery now consumes the same shared descriptor projector.
    // 公开发现现在消费同一共享描述符投影器。
    let published = public_catalog(temp.path());
    // Namespace lookup cannot accidentally accept the source directory's unrelated diagnostic row.
    // 命名空间查询不会意外接受来源目录的无关诊断条目。
    let plugin = published
        .plugins
        .get("configured-plugin")
        .expect("published configured package");
    assert_eq!(plugin.availability, VulcanPluginAvailability::Enabled);
    assert_eq!(
        Some(
            plugin
                .managed_package()
                .expect("published managed metadata")
                .configuration()
        ),
        server.configuration()
    );
    assert_eq!(
        plugin
            .mcp_servers
            .iter()
            .find(|entry| entry.id == "remote")
            .expect("published remote contribution")
            .credential_binding,
        server.credential_binding
    );
    assert_eq!(
        plugin.content_fingerprint,
        generation.identity().content_fingerprint
    );
}

/// A descriptor escaping the borrowed view retains the actual candidate package and credential readers after abort.
/// 从借用视图派生的描述符在中止后保留实际候选插件包及凭据读者。
#[test]
fn candidate_inspection_retains_real_resources_until_the_last_descriptor_drops() {
    // Actual filesystem lease contention is exercised without involving user-installed packages.
    // 验证实际文件系统租约争用，不涉及用户已安装插件。
    let temp = tempfile::tempdir().expect("create candidate retention fixture");
    // Keep the predecessor selected while this candidate is explicitly abandoned.
    // 明确放弃此候选时，保持前驱被选中。
    let (store, _, second, _, credentials) = fixture(temp.path());
    // The transaction's original request owns the preparation.
    // 事务原请求拥有准备过程。
    let mut transaction = begin(&store, &second, &credentials, Uuid::new_v4());
    // Descriptor clones are historical observations, not mutation or execution grants.
    // 描述符克隆为历史观测，并非变更或执行授权。
    let descriptor = store
        .inspect_activation_candidate(&transaction)
        .expect("inspect retained candidate")
        .package()
        .clone();
    // Preserve exact identity before releasing the original package handle.
    // 在释放原插件包句柄前保留精确身份。
    let identity = second.identity().clone();
    store
        .abort_activation_transaction(&mut transaction)
        .expect("abort untouched selection");
    drop(transaction);
    drop(second);
    assert_eq!(
        store
            .collect(&identity)
            .expect_err("descriptor retains actual package")
            .code,
        Code::Busy
    );
    assert_eq!(
        store
            .collect_credentials("sample-plugin", &credentials, &EmptyCredentialBackend)
            .expect_err("descriptor retains actual credential reader")
            .code,
        Code::Busy
    );
    drop(descriptor);
    assert!(
        store
            .collect_credentials("sample-plugin", &credentials, &EmptyCredentialBackend)
            .expect("collect after last candidate reader")
    );
    assert!(
        store
            .collect(&identity)
            .expect("collect after last candidate package owner")
    );
}

/// Every checkpoint after preparation rejects candidate reinspection, including an uncertain commit before selection.
/// 准备之后的每个检查点都拒绝再次检查候选，包含选择写入前的不确定提交。
#[test]
fn candidate_inspection_rejects_committing_published_completed_and_aborted_requests() {
    // Isolated actual files permit an exact crash-checkpoint simulation.
    // 隔离实际文件允许模拟精确崩溃检查点。
    let temp = tempfile::tempdir().expect("create candidate phase fixture");
    // A valid predecessor allows resuming the original committing request.
    // 有效前驱允许恢复原提交中请求。
    let (store, _, second, _, credentials) = fixture(temp.path());
    // The journal identity remains unchanged throughout recovery.
    // 日志身份在整个恢复期间保持不变。
    let request = Uuid::new_v4();
    // Begin through the actual public preparation API.
    // 通过实际公开准备接口开始。
    let transaction = begin(&store, &second, &credentials, request);
    // Simulate process death after the durable committing checkpoint, before the activation write.
    // 模拟持久提交中检查点之后、激活写入之前进程退出。
    let mut journal =
        serde_json::to_value(transaction.record()).expect("encode prepared checkpoint");
    drop(transaction);
    journal["phase"] = serde_json::json!("committing");
    fs::write(
        record_path(temp.path(), request),
        serde_json::to_vec(&journal).expect("encode committing checkpoint"),
    )
    .expect("persist committing checkpoint");
    // Recovery owns the same original request and target; it cannot reclassify it as a fresh candidate.
    // 恢复拥有相同原请求及目标；不能将其重新归类为新候选。
    let mut resumed = store
        .resume_activation_transaction("sample-plugin", request)
        .expect("resume exact committing request");
    assert_eq!(resumed.record().phase(), Phase::Committing);
    assert_eq!(
        store
            .inspect_activation_candidate(&resumed)
            .expect_err("committing is not prepared")
            .code,
        Code::Conflict
    );
    store
        .commit_activation_transaction(&mut resumed)
        .expect("finish original commit");
    assert_eq!(
        store
            .inspect_activation_candidate(&resumed)
            .expect_err("published is not prepared")
            .code,
        Code::Conflict
    );
    store
        .complete_activation_transaction(&mut resumed)
        .expect("finish original handoff");
    assert_eq!(
        store
            .inspect_activation_candidate(&resumed)
            .expect_err("completed is not prepared")
            .code,
        Code::Conflict
    );
    // Release the actual confirmed original request before a different UUID may replace its current index.
    // 另一 UUID 替换当前索引之前，释放实际已确认原请求。
    drop(resumed);
    // A successor with the same package tests phase authority independently from content identity.
    // 使用同包后继，独立于内容身份检查阶段权威。
    let mut successor = store
        .begin_activation_transaction(
            Uuid::new_v4(),
            &second,
            2,
            vec![],
            credentials,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("prepare same-package successor");
    store
        .abort_activation_transaction(&mut successor)
        .expect("abort successor");
    assert_eq!(
        store
            .inspect_activation_candidate(&successor)
            .expect_err("aborted is not prepared")
            .code,
        Code::Conflict
    );
}

/// Foreign roots, a missing current index and same-revision predecessor changes cannot authorize candidate inspection.
/// 外部根目录、缺失当前索引及相同修订前驱变化均不能授权候选检查。
#[test]
fn candidate_inspection_rejects_changed_authorities_without_repair() {
    // Every deliberate corruption is confined to an isolated fixture.
    // 每项主动损坏都限制在隔离夹具内。
    let temp = tempfile::tempdir().expect("create candidate authority fixture");
    // Retain real ownership while changing one disk authority at a time.
    // 每次改变一个磁盘权威期间保留实际所有权。
    let (store, _, second, _, credentials) = fixture(temp.path());
    // This live owner must continue matching the indexed checkpoint exactly.
    // 此存活所有者必须持续精确匹配已索引检查点。
    let transaction = begin(&store, &second, &credentials, Uuid::new_v4());
    // Another legitimate store cannot interpret this owner's paths or records.
    // 另一个合法存储不能解释此所有者的路径或记录。
    fs::create_dir_all(temp.path().join("foreign")).expect("create distinct user catalog");
    let foreign = VulcanPluginGenerationStore::open(&temp.path().join("foreign"))
        .expect("open distinct store");
    assert_eq!(
        foreign
            .inspect_activation_candidate(&transaction)
            .expect_err("reject foreign owner")
            .code,
        Code::Conflict
    );
    assert!(
        foreign
            .activation_transaction_plugin_ids()
            .expect("foreign namespace remains empty")
            .is_empty()
    );
    // Remove only this disposable fixture's request index; reads must not reconstruct it.
    // 仅移除此可丢弃夹具的请求索引；读取不得重建它。
    let index = store.root.join("transactions/sample-plugin/current");
    // Keep the exact original bytes for explicit test restoration.
    // 保留精确原字节，供测试明确恢复。
    let original_index = fs::read(&index).expect("retain original index");
    fs::remove_file(&index).expect("remove fixture index");
    assert!(store.inspect_activation_candidate(&transaction).is_err());
    assert!(!index.exists());
    fs::write(&index, original_index).expect("restore original index");
    // Equal revision with a different selected physical package is still a conflicting predecessor.
    // 相同修订选择不同物理插件包仍是冲突前驱。
    let active = store.root.join("active/sample-plugin.json");
    // Decode existing bytes rather than manufacturing another selection format.
    // 解码既有字节，不制造另一种选择格式。
    let mut changed: serde_json::Value =
        serde_json::from_slice(&fs::read(&active).expect("read predecessor"))
            .expect("decode predecessor");
    changed["generation"] =
        serde_json::to_value(second.identity()).expect("encode changed package identity");
    // The rejected read must preserve this exact conflicting evidence.
    // 被拒绝读取必须保留此精确冲突证据。
    let changed_bytes = serde_json::to_vec(&changed).expect("encode conflicting predecessor");
    fs::write(&active, &changed_bytes).expect("write conflicting predecessor");
    assert_eq!(
        store
            .inspect_activation_candidate(&transaction)
            .expect_err("reject changed complete predecessor")
            .code,
        Code::Conflict
    );
    assert_eq!(
        fs::read(&active).expect("read unrepaired conflict"),
        changed_bytes
    );
}

/// Physical package reinspection catches modifications after the transaction was prepared.
/// 物理插件包复核捕获事务准备之后的修改。
#[test]
fn candidate_inspection_rejects_modified_package_without_publishing_it() {
    // The fixture intentionally mutates its own immutable generation to reproduce corruption.
    // 夹具有意修改自身不可变代次以复现损坏。
    let temp = tempfile::tempdir().expect("create candidate corruption fixture");
    // The original predecessor remains selected while the candidate becomes invalid.
    // 候选变为无效时，原前驱仍保持选中。
    let (store, first, second, _, credentials) = fixture(temp.path());
    // Prepare before corruption so the test exercises reinspection instead of initial validation.
    // 在损坏前准备，使测试验证复核而非初始校验。
    let transaction = begin(&store, &second, &credentials, Uuid::new_v4());
    fs::write(
        second.package_root().join("skills/review-helper/SKILL.md"),
        "changed",
    )
    .expect("corrupt only fixture candidate bytes");
    assert_eq!(
        store
            .inspect_activation_candidate(&transaction)
            .expect_err("reject modified physical target")
            .code,
        Code::Invalid
    );
    assert_eq!(
        public_catalog(temp.path())
            .plugins
            .get("sample-plugin")
            .expect("old published package remains intact")
            .package_root,
        first.package_root()
    );
    assert_eq!(
        store
            .activation_transaction("sample-plugin", transaction.record().request_id())
            .expect("read unchanged transaction")
            .expect("prepared journal exists")
            .phase(),
        Phase::Prepared
    );
}
