//! Real predecessor MCP preparation separates physical resources from historical control.
//! 真实前驱 MCP 准备区分物理资源与历史控制。

use super::*;
use crate::VulcanPluginPredecessorRestorationSource;

/// Prepare old local/remote MCP declarations and a distinct target through actual storage and drain.
/// 通过实际存储及排空准备旧本地／远端 MCP 声明和不同目标。
/// No inputs; return the original production fixture, store, aborted transaction, restoration and source.
/// 无输入；返回原生产夹具、存储、中止事务、恢复对象及来源。
fn mcp_source_fixture() -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
    VulcanPluginPredecessorRestoration,
    Arc<VulcanPluginPredecessorRestorationSource>,
) {
    // Reuse the real formal parent and native declaration before immutable inspection.
    // 不可变检查前复用真实正式父级及原生声明。
    let fixture = Fixture::new();
    // This exact temporary package is the only pre-publication source being edited.
    // 此精确临时插件包是唯一被修改的发布前来源。
    let package = fixture._root.path().join("bundled/hook-plugin");
    // Parse the actual manifest before updating its authoritative declarations.
    // 更新权威声明前解析实际清单。
    let mut manifest: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(package.join("plugin.yaml")).expect("actual old manifest"),
    )
    .expect("parse old manifest");
    manifest["permissions"]
        .as_sequence_mut()
        .expect("declared permissions")
        .extend(
            ["config", "process_session", "network_https"].map(|permission| {
                // Each additional capability is declared before production approval derivation.
                // 每个附加能力均在生产批准派生前声明。
                serde_yaml::Value::String(permission.into())
            }),
        );
    manifest["config"] = serde_yaml::from_str(
        "schema_file: config.schema.json\ndefaults_file: config.defaults.json\n",
    )
    .expect("declared old configuration");
    manifest["lua_runtime"]["lifecycle"]
        .as_mapping_mut()
        .expect("actual lifecycle")
        .remove(serde_yaml::Value::String("enable".into()));
    manifest["lua_runtime"]["lifecycle"]
        .as_mapping_mut()
        .expect("actual lifecycle")
        .remove(serde_yaml::Value::String("disable".into()));
    manifest["mcp"] = serde_yaml::from_str(
        "servers:\n  - id: old-local\n    name: Old local\n    scope: global\n    transport:\n      type: stdio\n      launcher: {type: native_binary, binary_id: fixture}\n  - id: old-remote\n    name: Old remote\n    scope: global\n    transport:\n      type: http\n      url: {source: literal, value: 'https://example.test/old-mcp'}\n",
    ).expect("actual old MCP declarations");
    std::fs::write(
        package.join("plugin.yaml"),
        serde_yaml::to_string(&manifest).expect("serialize old manifest"),
    )
    .expect("write old manifest");
    std::fs::write(package.join("config.schema.json"),
        r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#)
        .expect("actual schema");
    std::fs::write(
        package.join("config.defaults.json"),
        r#"{"label":"package-default"}"#,
    )
    .expect("actual defaults");
    // Move only these verified temporary paths before production managed publication.
    // 生产托管发布前仅移动这些已验证临时路径。
    let source_path = fixture._root.path().join("source/hook-plugin");
    assert!(
        package.starts_with(fixture._root.path()) && source_path.starts_with(fixture._root.path())
    );
    std::fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("source directory");
    std::fs::rename(&package, &source_path).expect("move exact fixture package");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove bundled source");
    // Production publication freezes the full old document, credentials, manifest and binary bytes.
    // 生产发布固定完整旧文档、凭据、清单及二进制字节。
    let (store, initial) = transaction_for(&fixture, &source_path, true);
    // Close only the unused fixture request before the tested original update.
    // 受测原更新前仅关闭未使用夹具请求。
    let initial_id = initial.record().request_id();
    drop(initial);
    store
        .abort_activation_request("hook-plugin", initial_id)
        .expect("close unused original");
    // This captured original catalog predates all target source changes.
    // 此捕获原目录早于全部目标来源修改。
    let original_catalog = fixture.manager.snapshot();
    manifest["mcp"] = serde_yaml::from_str(
        "servers:\n  - id: target-remote\n    name: Target remote\n    scope: global\n    transport:\n      type: http\n      url: {source: literal, value: 'https://example.test/target-mcp'}\n",
    ).expect("distinct target MCP declaration");
    std::fs::write(
        source_path.join("plugin.yaml"),
        serde_yaml::to_string(&manifest).expect("serialize target"),
    )
    .expect("write distinct target manifest");
    std::fs::write(
        source_path.join("fixture.bin"),
        b"target must not be predecessor",
    )
    .expect("distinct target bytes");
    // The original update uses a different selected document and actual independently pinned target.
    // 原更新使用不同已选文档及实际独立固定目标。
    let mut transaction = prepare_configured(
        &fixture,
        &store,
        VulcanPluginConfigurationSelection::Document {
            value: json!({"label":"new-selected"}),
        },
    );
    // Borrow actual candidate readiness only while executing the real resource drain.
    // 仅执行真实资源排空期间借用实际候选就绪状态。
    let candidate = store
        .inspect_activation_candidate(&transaction)
        .expect("actual target");
    // The production drain closes ordinary old admission before restoration preparation.
    // 生产排空在恢复准备之前关闭普通旧入场。
    let drain = fixture
        .manager
        .begin_activation_predecessor_drain(&candidate, &original_catalog)
        .expect("actual predecessor drain");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .drain_activation_predecessor(&drain)
                .await
                .expect("actual old resources retired")
                .failures
                .is_empty()
        );
    });
    // Missing old callback declaration is validated by the existing typed production restoration.
    // 旧回调声明缺失由既有类型化生产恢复校验。
    let mut restoration = fixture
        .manager
        .prepare_activation_predecessor_restoration(&candidate, drain)
        .expect("actual restoration");
    drop(candidate);
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual independent Pending owner");
    fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual aborted old successor");
    // Sign only the source retaining the original independent File and closed catalog allocation.
    // 仅签发保留原独立文件及关闭目录分配的来源。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("actual original source");
    (fixture, store, transaction, restoration, source)
}

/// Same-source retries preserve the actual physical launcher allocation and complete old selection.
/// 相同来源重试保留实际物理启动器分配及完整旧选择。
/// No inputs or return value; Pending bytes and ordinary exposure remain closed throughout preparation.
/// 无输入或返回值；准备全过程中 Pending 字节及普通暴露保持关闭。
#[test]
fn predecessor_mcp_source_retry_preserves_actual_old_inputs_and_pending() {
    // Every owner originates in production storage, drain and abort rather than fabricated values.
    // 每个所有者均来自生产存储、排空及中止，而非伪造值。
    let (fixture, store, transaction, restoration, source) = mcp_source_fixture();
    // The first actual registered issuer remains fixed for every source retry.
    // 首个实际注册签发者在每次来源重试中保持固定。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // Preserve physical sidecar bytes and obstruct writes during both preparation calls.
    // 在两次准备调用期间保留物理附属记录字节并阻挡写入。
    let record_path = restoration
        .private_owner_test_path(true)
        .expect("actual Pending path");
    // Reading bytes detects rewrites even if a parsed record would compare equal.
    // 读取字节可检测重写，即使解析记录比较相等。
    let before = std::fs::read(&record_path).expect("actual Pending bytes");
    // A real write obstruction must remain unused because preparation validates only existing ownership.
    // 实际写入阻挡必须保持未使用，因为准备仅校验既有所有权。
    let obstruction = store.obstruct_predecessor_handoff_write_once(source.request_id());
    // Resolve old native bytes without launching this inert binary.
    // 解析旧原生字节，不启动此惰性二进制。
    let prepared = fixture
        .manager
        .prepare_predecessor_mcp_resources(&source, &authority)
        .expect("actual preparation");
    // A retry must return the same physical allocation, not merely equivalent fields.
    // 重试必须返回相同物理分配，而非仅相同字段。
    let retry = fixture
        .manager
        .prepare_predecessor_mcp_resources(&source, &authority)
        .expect("same actual preparation");
    drop(obstruction);
    assert!(std::ptr::eq(
        prepared.servers().as_ptr(),
        retry.servers().as_ptr()
    ));
    assert_eq!(prepared.servers().len(), 2);
    assert!(
        prepared
            .servers()
            .iter()
            .any(|server| server.id == "old-local")
    );
    assert!(
        prepared
            .servers()
            .iter()
            .any(|server| server.id == "old-remote")
    );
    assert!(
        !prepared
            .servers()
            .iter()
            .any(|server| server.id == "target-remote")
    );
    // Exact launcher reference identity independently proves reuse of the physical prepared map.
    // 精确启动器引用身份独立证明复用物理准备映射。
    let launcher = prepared
        .stdio_launcher("old-local")
        .expect("actual old launcher");
    assert!(std::ptr::eq(
        launcher,
        retry.stdio_launcher("old-local").expect("same launcher")
    ));
    assert_eq!(
        std::fs::read(&launcher.program).expect("retained old binary"),
        b"never executed"
    );
    assert!(prepared.stdio_launcher("old-remote").is_err());
    assert!(prepared.stdio_launcher("missing").is_err());
    assert_eq!(prepared.generation(), source.plugin().generation);
    assert!(prepared.generation() > restoration.predecessor().generation);
    assert_eq!(prepared.plugin_id(), "hook-plugin");
    assert_eq!(prepared.request_id(), transaction.record().request_id());
    assert_eq!(
        source
            .plugin()
            .managed_package()
            .expect("actual old package")
            .configuration()
            .value(),
        Some(&json!({"label":"old-published"}))
    );
    assert!(
        source
            .plugin()
            .managed_package()
            .expect("actual old pins")
            .matches_activation(
                transaction
                    .record()
                    .predecessor()
                    .expect("complete old selection")
            )
    );
    // Pure profiles compare the opaque original allocation rather than equal identity fields.
    // 纯画像比较不透明原分配，而非相同身份字段。
    let profile = prepared
        .host_profile(&authority)
        .expect("original pure profile");
    assert!(profile.same_source(&retry.host_profile(&authority).expect("same profile")));
    assert_eq!(
        std::fs::read(record_path).expect("unchanged Pending bytes"),
        before
    );
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("actual fence")
    );
    assert!(fixture.manager.snapshot().enabled_mcp_servers.is_empty());
}

/// Foreign managers, issuers and full originals reject startup before consuming the real dispatch.
/// 外来管理器、签发者及完整原请求在消费真实派发前拒绝启动。
/// No inputs or return value; one valid start consumes authority permanently without fabricating native Ready.
/// 无输入或返回值；一次合法启动永久消费权威，不伪造原生就绪。
#[test]
fn predecessor_mcp_source_rejects_foreign_authority_before_consuming_dispatch() {
    // The original source and distinct foreign original both come from actual production preparation.
    // 原来源及不同外来原请求均来自实际生产准备。
    let (fixture, store, transaction, restoration, source) = mcp_source_fixture();
    // A second real manager supplies an actual foreign issuer and transaction.
    // 第二个真实管理器提供实际外来签发者及事务。
    let (foreign, _foreign_store, foreign_transaction, _foreign_restoration, _foreign_source) =
        mcp_source_fixture();
    // One original issuer is fixed before any rejected observation.
    // 任何被拒绝观测前固定单个原签发者。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // Register another actual host in the same manager to test allocation-based first-issuer authority.
    // 在相同管理器注册另一实际宿主，以测试基于分配的首次签发者权威。
    let other_issuer = fixture.manager.register_native_mcp_host_adapter();
    // A foreign manager issuer must not sign or consume this source.
    // 外来管理器签发者不得签发或消费此来源。
    let foreign_issuer = foreign.manager.register_native_mcp_host_adapter();
    assert!(
        foreign
            .manager
            .prepare_predecessor_mcp_resources(&source, &foreign_issuer)
            .is_err()
    );
    // The true first issuer still prepares normally after the earlier foreign rejection.
    // 先前外来拒绝后，真实首次签发者仍正常准备。
    let prepared = fixture
        .manager
        .prepare_predecessor_mcp_resources(&source, &authority)
        .expect("actual preparation");
    // Logical observation must not grant resource or startup authority to another registered host.
    // 逻辑观测不得向另一已注册宿主授予资源或启动权威。
    let control = prepared.control();
    assert!(control.host_profile(&other_issuer).is_err());
    assert!(
        fixture
            .manager
            .prepare_predecessor_mcp_resources(&source, &other_issuer)
            .is_err()
    );
    assert!(
        control
            .begin_transport_attempt(&foreign.manager, &transaction, &authority)
            .is_err()
    );
    assert!(
        control
            .begin_transport_attempt(&fixture.manager, &transaction, &other_issuer)
            .is_err()
    );
    assert!(
        control
            .begin_transport_attempt(&fixture.manager, &foreign_transaction, &authority)
            .is_err()
    );
    assert!(
        control
            .finish_transport_observation(&authority, true)
            .is_err()
    );
    // Snapshot allocation authority remains strict even for a field-identical replacement.
    // 即使字段相同替代，快照分配权威仍保持严格。
    let staged = Arc::clone(source.staged_catalog());
    *fixture
        .manager
        .state
        .snapshot
        .write()
        .expect("test replacement catalog") = Arc::new(staged.as_ref().clone());
    assert!(
        control
            .begin_transport_attempt(&fixture.manager, &transaction, &authority)
            .is_err()
    );
    *fixture
        .manager
        .state
        .snapshot
        .write()
        .expect("restore actual original Arc") = staged;
    // Capture the original bytes before dispatch to prove there is no new attempt ledger.
    // 派发前捕获原字节，以证明没有新尝试账本。
    let record_path = restoration
        .private_owner_test_path(true)
        .expect("actual Pending path");
    // The actual disk observation remains independent of logical control state.
    // 实际磁盘观测独立于逻辑控制状态。
    let before = std::fs::read(&record_path).expect("Pending before dispatch");
    // Enter the one real source gate, without claiming that a native runtime or lease was constructed.
    // 进入唯一真实来源门，不宣称已构造原生运行时或租约。
    let mut start = control
        .begin_transport_attempt(&fixture.manager, &transaction, &authority)
        .expect("sole dispatch");
    assert!(
        start
            .bind_actual_instances(Uuid::nil(), Uuid::nil(), &authority)
            .is_err()
    );
    assert!(
        control
            .finish_transport_observation(&authority, true)
            .is_err()
    );
    drop(start);
    assert!(
        control
            .begin_transport_attempt(&fixture.manager, &transaction, &authority)
            .err()
            .expect("cancelled unbound attempt is consumed")
            .message
            .contains("already attempted")
    );
    assert_eq!(
        std::fs::read(record_path).expect("same Pending after attempt"),
        before
    );
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("actual Pending remains")
    );
}

/// Historical controls and profiles do not pin the original source or actual environment File.
/// 历史控制及画像不固定原来源或实际环境文件。
/// No inputs or return value; independent physical probes succeed after the last preparation owner drops.
/// 无输入或返回值；最后准备所有者释放后，独立物理探测成功。
#[test]
fn predecessor_mcp_source_control_and_profile_release_actual_physical_owners() {
    // Actual managed restoration retains independent recovery and execution environment owners.
    // 实际托管恢复保留独立恢复及执行环境所有者。
    let (fixture, store, transaction, restoration, source) = mcp_source_fixture();
    // The original registration carries only a weak manager identity.
    // 原注册仅携带弱管理器身份。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // The physical bundle must acquire the actual local environment File.
    // 物理资源包必须获取实际本地环境文件。
    let prepared = fixture
        .manager
        .prepare_predecessor_mcp_resources(&source, &authority)
        .expect("local preparation");
    // A clone is another owner of this same actual bundle, not a reconstructed preparation.
    // 克隆是此相同实际资源包的另一所有者，而非重建准备。
    let retained = prepared.clone();
    // Keep logical and pure historical observations after physical ownership ends.
    // 物理所有权结束后保留逻辑及纯历史观测。
    let control = prepared.control();
    // Pure source identity deliberately survives the physical owners.
    // 纯来源身份有意在物理所有者释放后存续。
    let profile = control
        .host_profile(&authority)
        .expect("pure historical profile");
    // Weak ownership detects any forbidden strong source retention by logical values.
    // 弱所有权检测逻辑值任何禁止的强来源保留。
    let weak_source = Arc::downgrade(&source);
    // Open the actual independent lease rather than guessing the writer namespace.
    // 打开实际独立租约，而非猜测写入器命名空间。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual lease path"),
        )
        .expect("independent probe");
    drop(restoration);
    drop(transaction);
    drop(source);
    drop(prepared);
    assert!(weak_source.upgrade().is_some());
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(
        crate::runtime::environment::lock_plugin_execution_environment_cleanup(
            &fixture.manager.state,
            "hook-plugin"
        )
        .is_err()
    );
    drop(retained);
    assert!(
        weak_source.upgrade().is_none(),
        "historical values must not own the actual source"
    );
    competing
        .try_lock()
        .expect("last preparation releases independent File");
    competing.unlock().expect("release probe");
    // The independent exclusive environment lock succeeds while control and profile remain alive.
    // 控制及画像仍存活时，独立排他环境锁成功。
    let cleanup = crate::runtime::environment::lock_plugin_execution_environment_cleanup(
        &fixture.manager.state,
        "hook-plugin",
    )
    .expect("historical values do not retain environment File");
    assert!(
        profile.same_source(
            &control
                .host_profile(&authority)
                .expect("same historical profile")
        )
    );
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("ownerless Pending remains closed")
    );
    drop(cleanup);
}

/// A released physical preparation cannot be rebuilt even while its exact original source survives.
/// 即使精确原来源仍存活，也不能重建已释放物理准备。
/// No inputs or return value; the source retains Pending but neither retry nor first dispatch creates resources.
/// 无输入或返回值；来源保留 Pending，但重试和首次派发均不创建资源。
#[test]
fn predecessor_mcp_source_released_preparation_cannot_be_rebuilt() {
    // Keep the actual source and original typed transaction after preparation retirement.
    // 准备退役后保留实际来源及原类型化事务。
    let (fixture, store, transaction, _restoration, source) = mcp_source_fixture();
    // The first issuer remains the same after all physical preparation handles retire.
    // 全部物理准备句柄退役后，首次签发者保持相同。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // Initial local preparation creates the sole source slot.
    // 首次本地准备创建唯一来源槽。
    let prepared = fixture
        .manager
        .prepare_predecessor_mcp_resources(&source, &authority)
        .expect("sole preparation");
    // Historical control observes retirement without extending actual ownership.
    // 历史控制观测退役，不延长实际所有权。
    let control = prepared.control();
    drop(prepared);
    assert!(
        fixture
            .manager
            .prepare_predecessor_mcp_resources(&source, &authority)
            // Discard unexpected successful ownership before asserting the error, without requiring the guard to implement Debug.
            // 断言错误前释放意外成功的所有权，不要求守卫实现 Debug。
            .map(|_| ())
            .expect_err("cannot rebuild same source")
            .message
            .contains("cannot be rebuilt")
    );
    assert!(
        control
            .begin_transport_attempt(&fixture.manager, &transaction, &authority)
            .err()
            .expect("retired physical inputs cannot start")
            .message
            .contains("preparation was released")
    );
    // Release of actual preparation permits cleanup despite the original source remaining alive.
    // 实际准备释放后，即使原来源仍存活，也允许清理。
    let cleanup = crate::runtime::environment::lock_plugin_execution_environment_cleanup(
        &fixture.manager.state,
        "hook-plugin",
    )
    .expect("source alone does not pin MCP environment");
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("original durable fence")
    );
    drop(cleanup);
}

/// The short actual start guard retains physical resources across all external preparation drops.
/// 短时实际启动守卫在全部外部准备释放期间保留物理资源。
/// No inputs or return value; guard exit releases Files while historical control and profile remain alive.
/// 无输入或返回值；历史控制及画像仍存活时，守卫退出释放文件。
#[test]
fn predecessor_mcp_source_short_start_guard_retains_original_files_until_exit() {
    // Production preparation supplies the original physical owner for the guard to retain.
    // 生产准备提供由守卫保留的原物理所有者。
    let (fixture, store, transaction, restoration, source) = mcp_source_fixture();
    // Only this original registration may begin the short critical section.
    // 仅此原注册可以开始短临界区。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // Resolve inert local inputs under the actual environment File.
    // 在实际环境文件下解析惰性本地输入。
    let prepared = fixture
        .manager
        .prepare_predecessor_mcp_resources(&source, &authority)
        .expect("actual local preparation");
    // Logical control outlives the physical preparation handle.
    // 逻辑控制比物理准备句柄存活更久。
    let control = prepared.control();
    // Historical profile must neither extend nor substitute for the guard's physical authority.
    // 历史画像既不得延长也不得替代守卫物理权威。
    let profile = control
        .host_profile(&authority)
        .expect("pure original profile");
    // Actual weak identity verifies the guard's retained source allocation.
    // 实际弱身份验证守卫保留的来源分配。
    let weak_source = Arc::downgrade(&source);
    // The independent physical probe sees no Rust reference-count bookkeeping.
    // 独立物理探测不依赖 Rust 引用计数记账。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual independent lease"),
        )
        .expect("probe");
    // Begin consumes dispatch once but does not construct or attest any native runtime in this core test.
    // 开始仅消费一次派发，本核心测试不构造或证明任何原生运行时。
    let start = control
        .begin_transport_attempt(&fixture.manager, &transaction, &authority)
        .expect("actual short guard");
    drop(prepared);
    drop(source);
    drop(restoration);
    drop(transaction);
    assert!(weak_source.upgrade().is_some());
    assert!(matches!(
        competing.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(
        crate::runtime::environment::lock_plugin_execution_environment_cleanup(
            &fixture.manager.state,
            "hook-plugin"
        )
        .is_err()
    );
    drop(start);
    assert!(weak_source.upgrade().is_none());
    competing
        .try_lock()
        .expect("guard exit releases recovery File");
    competing.unlock().expect("release probe");
    // Registration has ended before physical destruction allows the independent exclusive cleanup lock.
    // 物理销毁允许独立排他清理锁之前，注册已结束。
    let cleanup = crate::runtime::environment::lock_plugin_execution_environment_cleanup(
        &fixture.manager.state,
        "hook-plugin",
    )
    .expect("guard exit releases execution File");
    assert!(profile.same_source(&control.host_profile(&authority).expect("same pure profile")));
    assert!(
        control
            .finish_transport_observation(&authority, false)
            .is_err()
    );
    assert!(
        store
            .predecessor_handoff_blocks_admission("hook-plugin")
            .expect("Pending survives guard")
    );
    drop(cleanup);
}
