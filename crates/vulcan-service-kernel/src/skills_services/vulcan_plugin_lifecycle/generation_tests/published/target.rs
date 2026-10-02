//! Actual selected-target publication uses the service Full slot and original native ownership.
//! 实际已选目标发布使用服务完整槽及原原生所有权。

use super::*;
use vulcan_agent_runtime::mcp::{
    CandidatePluginMcpRuntime, McpRuntimeToolAllowlist, TargetPluginMcpRuntime,
    candidate_global_mcp_server_configs, target_global_mcp_server_configs,
};
use vulcan_plugin_runtime::{
    VulcanPluginActivationTransaction, VulcanPluginActivationTransactionPhase,
    VulcanPluginCatalogSnapshot, VulcanPluginUpdatePolicy,
};

/// Keep the real selected request, its closed catalog and actual native controller together.
/// 将真实已选请求、其关闭目录及实际原生控制器共同保留。
struct TargetFixture {
    /// The reused production service owns the sole Full pointer.
    /// 复用的生产服务拥有唯一完整指针。
    state: SharedServiceBridgeState,
    /// The original manager is the only authority permitted to complete this request.
    /// 原管理器是唯一允许完成此请求的权威。
    manager: VulcanPluginRuntimeManager,
    /// Original transaction ownership survives every failed publication attempt.
    /// 原事务所有权跨每个失败发布尝试存续。
    transaction: VulcanPluginActivationTransaction,
    /// Exact closed catalog fixes the target routing generation before startup.
    /// 精确关闭目录在启动前固定目标路由代次。
    staged: Arc<VulcanPluginCatalogSnapshot>,
    /// The actual controller owns its Preparing lease until original completion.
    /// 实际控制器拥有其准备中租约，直至原完成。
    target: TargetPluginMcpRuntime,
    /// Only actual children append their startup identity to this isolated file.
    /// 仅实际子进程向此隔离文件追加其启动身份。
    marker: std::path::PathBuf,
    /// Last ownership field keeps native source files alive through physical retirement.
    /// 最后所有权字段在物理退役期间保持原生来源文件存活。
    _temp: tempfile::TempDir,
}

/// Prepare an enabled update using the existing real fixture and physically retire candidate validation.
/// 使用既有真实夹具准备启用更新，并物理退役候选校验。
/// No inputs; returns one selected ready native target without a substitute publisher or extra native issuer.
/// 无输入；返回单个已选就绪原生目标，不使用替代发布器或额外原生签发者。
async fn target_fixture() -> TargetFixture {
    // Reuse the exact managed predecessor, immutable bytes, credential backend and actual service dependencies.
    // 复用精确受管前驱、不可变字节、凭据后端及实际服务依赖。
    let base = fixture();
    // The original first issuer belongs to this same service bridge and manager.
    // 原首次签发者属于此相同服务桥及管理器。
    let (manager, authority) =
        crate::service_state_access::service_vulcan_plugin_native_mcp_host(&base.state)
            .expect("actual original service authority");
    assert_eq!(
        manager.snapshot().plugins["published-service"].runtime_state,
        vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabled
    );
    // Release the unused ordinary producer's pins before selecting the new target; no child was dispatched.
    // 在选择新目标前释放未使用普通生产者的固定句柄；没有派发子进程。
    let Fixture {
        state,
        configs,
        marker,
        environment: _,
        _temp,
    } = base;
    drop(configs);
    // This mutable preparation source remains distinct from the already activated immutable package.
    // 此可变准备来源保持区别于已激活不可变插件包。
    let source = _temp.path().join("source/published-service");
    // Change only known source version bytes before inspection, preserving selected predecessor bytes.
    // 仅在检查前改变已知来源版本字节，保留已选前驱字节。
    let manifest =
        std::fs::read_to_string(source.join("plugin.yaml")).expect("known preparation manifest");
    assert!(manifest.contains("version: 1.0.0"));
    std::fs::write(
        source.join("plugin.yaml"),
        manifest.replacen("version: 1.0.0", "version: 1.0.1", 1),
    )
    .expect("new mutable source version");
    // Current selected configuration is the exact document later consumed by the native child.
    // 当前已选配置为随后被原生子进程消费的精确文档。
    let configuration: serde_json::Value = serde_json::from_slice(
        &std::fs::read(source.join("defaults.json")).expect("known selected configuration"),
    )
    .expect("exact selected configuration document");
    // Inspection supplies the new complete fingerprint and approved manifest permissions.
    // 检查提供新完整指纹及已批准清单权限。
    let preview = manager
        .inspect_local_user_plugin(&source)
        .expect("inspect new preparation source");
    // The actual store supplies a complete selected predecessor rather than a generation counter.
    // 实际存储提供完整已选前驱，不使用代次计数器。
    let store = VulcanPluginGenerationStore::open(manager.user_plugin_root())
        .expect("actual original generation store");
    // Freeze original enabled policy before any update package copying or credential writes.
    // 在任何更新插件包复制或凭据写入前固定原启用策略。
    let predecessor = store
        .activation("published-service")
        .expect("actual predecessor selection")
        .expect("managed predecessor must exist");
    // This original request retains its actual full predecessor and explicit enabled state.
    // 此原请求保留其实际完整前驱及明确启用状态。
    let preparation = store
        .begin_activation_update_preparation(
            uuid::Uuid::new_v4(),
            &source,
            &preview.content_fingerprint,
            Some(&predecessor),
            preview.permissions,
            VulcanPluginUpdatePolicy {
                explicitly_enabled: true,
                explicitly_disabled: false,
                predecessor_default_enabled: true,
            },
        )
        .expect("freeze actual enabled update");
    // Copy new immutable bytes under this same request owner.
    // 在此相同请求所有者下复制新不可变字节。
    let package = store
        .prepare_activation_package(&preparation, &source)
        .expect("actual update package");
    store
        .begin_activation_credential_staging(
            &preparation,
            &package,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("actual empty update credentials")
        .write_and_finish(&EmptyCredentials, &VulcanPluginCredentialValues::new())
        .expect("actual credential staging retirement");
    // The configured transaction is the original owner consumed by every subsequent stage.
    // 已配置事务为每个后续阶段消费的原所有者。
    let mut transaction = store
        .finish_activation_preparation(
            preparation,
            VulcanPluginConfigurationSelection::Document {
                value: configuration,
            },
        )
        .expect("actual configured original update");
    {
        // Candidate validation uses real native work and must physically retire before selection.
        // 候选校验使用真实原生工作，必须在选择前物理退役。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("actual candidate source");
        // Exact candidate preparation cannot substitute mutable source bytes or unbound configuration.
        // 精确候选准备不能替代可变来源字节或未绑定配置。
        let prepared = manager
            .prepare_mcp_activation_candidate(&candidate)
            .expect("actual candidate resources");
        // The sealed original configurations are materialized through the existing empty secret backend.
        // 密封原配置通过既有空秘密后端物化。
        let configs = candidate_global_mcp_server_configs(prepared, &EmptyCredentials)
            .expect("actual candidate native inputs");
        CandidatePluginMcpRuntime::start(configs)
            .expect("actual validation child")
            .validate_and_release()
            .await
            .expect("actual candidate ready and physically released");
    }
    store
        .commit_activation_transaction(&mut transaction)
        .expect("publish actual selected package");
    assert_eq!(
        transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Published
    );
    assert_eq!(transaction.record().predecessor(), Some(&predecessor));
    // The sole actual stage allocates the selected route and keeps its business surfaces closed.
    // 唯一实际暂存分配已选路由，并保持其业务表面关闭。
    let staged = manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from([String::from("published-service")]),
        )
        .expect("actual selected closed catalog");
    // The same first service issuer supplies exact target preparation, without registering another host.
    // 相同首次服务签发者提供精确目标准备，不注册另一宿主。
    let prepared = manager
        .prepare_mcp_activation_target(&transaction, &staged, &authority)
        .expect("same issuer actual target preparation");
    // Materialization preserves actual selected source and request resource ownership.
    // 物化保留实际已选来源及请求资源所有权。
    let configs = target_global_mcp_server_configs(prepared, &EmptyCredentials)
        .expect("sealed actual target native inputs");
    // This current access comes from the real sole service Full pointer.
    // 此当前访问来自真实唯一服务完整指针。
    let access = current_full(&state).mcp_runtime();
    // Start the original target once, before its real asynchronous readiness observation.
    // 在真实异步就绪观察前仅启动一次原目标。
    let target =
        TargetPluginMcpRuntime::start(&access, &manager, &transaction, configs, &authority)
            .expect("actual original selected target startup");
    target
        .wait_until_ready_default()
        .await
        .expect("actual original target ready");
    // Durable Ready must identify this same actual runtime and lease before any Full publication.
    // 任何完整发布之前，持久就绪必须标识此相同实际运行时及租约。
    let receipt = store
        .target_mcp_initialization_receipt("published-service", transaction.record().request_id())
        .expect("actual original target startup readback")
        .expect("actual startup receipt exists");
    assert_eq!(
        receipt.phase(),
        vulcan_plugin_runtime::VulcanPluginTargetMcpInitializationPhase::Ready
    );
    assert_eq!(receipt.instance_ids(), Some(target.actual_instance_ids()));
    TargetFixture {
        state,
        manager,
        transaction,
        staged,
        target,
        marker,
        _temp,
    }
}

/// Read the sole actual Full pointer under its service mutex without creating a replacement.
/// 在服务互斥锁下读取唯一实际完整指针，不创建替代者。
/// state selects the actual service; returns an owning observation of its current immutable generation.
/// state 选择实际服务；返回其当前不可变代次的拥有型观察。
fn current_full(
    state: &SharedServiceBridgeState,
) -> Arc<vulcan_agent_runtime::agent_runtime::extension_generation::ExtensionGeneration> {
    Arc::clone(
        &state
            .lock()
            .expect("actual service slot observation")
            .agent_runtime
            .as_ref()
            .expect("actual original bridge")
            .extension_generation,
    )
}

/// Check the same original selected child through a real discovered business tool, then retire its physical resources.
/// 通过真实发现业务工具检查相同原已选子进程，随后退役其物理资源。
/// fixture supplies original owners and starts is the pre-publication marker; returns no value after PID and no-replay assertions.
/// fixture 提供原所有者，starts 为发布前标记；在 PID 及不重播断言之后无返回值。
async fn verify_original_and_retire(fixture: &TargetFixture, starts: &str) {
    // Semantic lookup identifies the declared original tool without catalog-position assumptions.
    // 语义查找标识已声明原工具，不假设目录位置。
    let full = current_full(&fixture.state);
    assert!(Arc::ptr_eq(
        &full.plugin_catalog().expect("actual Full catalog"),
        &fixture.manager.snapshot()
    ));
    // Ready does not imply publication; Completed and exact Full identity are independently checked.
    // 就绪不代表发布；独立检查完成及精确完整指针身份。
    let binding = full
        .mcp_runtime()
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|binding| binding.origin_tool_id() == "published-service:actual/original_process")
        .expect("actual original_process declaration may have changed");
    // One candidate child and one selected target are the only actual starts in this complete setup.
    // 单个候选子进程及单个已选目标为此完整准备中的仅有实际启动。
    assert_eq!(starts.lines().count(), 2);
    // The final completed producer record belongs to the original selected target, after candidate retirement.
    // 最后完成的生产者记录属于原已选目标，位于候选退役之后。
    let selected: serde_json::Value =
        serde_json::from_str(starts.lines().last().expect("original selected startup"))
            .expect("actual selected startup JSON");
    // Invocation crosses the actual service Full access and original MCP business gate.
    // 调用跨越实际服务完整访问及原 MCP 业务门。
    let result = binding
        .invoke(serde_json::json!({"message":"selected actual Full"}))
        .await
        .expect("same original selected native business");
    assert_eq!(result["pid"], selected["pid"]);
    assert!(selected["pid"].as_u64().expect("actual selected PID") > 0);
    assert_eq!(result["message"], "selected actual Full");
    assert_eq!(
        std::fs::read_to_string(&fixture.marker).expect("actual start records remain unchanged"),
        starts
    );
    // Independent release observation grants no authority to keep a drained process alive.
    // 独立释放观察不授予保持已排空进程存活的权威。
    let observer = fixture.target.observer();
    // The same actual Full lease owns retirement after transfer, rather than a replacement shutdown handle.
    // 转交后相同实际完整租约拥有退役，不使用替代关闭句柄。
    let access = full.mcp_runtime();
    access
        .begin_plugin_mcp_drain("published-service")
        .expect("same original native drain");
    access
        .wait_plugin_mcp_drain("published-service", OBSERVE)
        .await
        .expect("same original native physical retirement");
    observer
        .wait_until_released(OBSERVE)
        .await
        .expect("actual original release evidence");
}

/// The service publishes its real Full slot and completes the original ready target without changing its lease or PID.
/// 服务发布其真实完整槽，并完成原就绪目标，不改变其租约或 PID。
/// No inputs or return; actual request, service/manager Arc identities and real tool invocation prove this publication seam.
/// 无输入或返回；实际请求、服务／管理器 Arc 身份及真实工具调用证明此发布接点。
#[tokio::test]
async fn service_selected_target_full_completion_transfers_same_actual_native() {
    // All prerequisite work runs through actual configured update and native readiness APIs.
    // 全部前置工作经由实际已配置更新及原生就绪接口执行。
    let mut fixture = target_fixture().await;
    // Opaque identities originate from the actual registered runtime and its original lease.
    // 不透明身份来自实际已登记运行时及其原租约。
    let identities = fixture.target.actual_instance_ids();
    // Capture real effects before any publication operation.
    // 在任何发布操作前捕获真实副作用。
    let starts = std::fs::read_to_string(&fixture.marker).expect("actual pre-publication starts");
    // The original service request retains its successful Full witness before fallible completion.
    // 原服务请求在可失败完成之前保留其成功完整凭据。
    let mut retained = None;
    publish_activation_target_plugin_extension_generation(
        &fixture.state,
        &mut fixture.transaction,
        &fixture.staged,
        Some(&fixture.target),
        &mut retained,
    )
    .expect("actual Full and original combined completion");
    assert_eq!(
        fixture.transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Completed
    );
    assert_eq!(fixture.target.actual_instance_ids(), identities);
    retained
        .as_ref()
        .expect("actual original retained publication")
        .validate_current(&fixture.state, &fixture.manager)
        .expect("exact actual service and manager witnesses");
    verify_original_and_retire(&fixture, &starts).await;
}

/// A foreign issuer fails before Full replacement, and restoring only the first issuer completes the same prepared child.
/// 外来签发者在完整替换之前失败，仅恢复首次签发者即完成相同已准备子进程。
/// No inputs or return; unchanged actual pointers, closed handlers and unchanged startup identities prove non-replay recovery.
/// 无输入或返回；未改变实际指针、关闭处理器及未改变启动身份证明不重播恢复。
#[tokio::test]
async fn service_selected_target_wrong_issuer_preserves_closed_original_then_recovers() {
    // Prepare one actual target; this test never allocates a second native controller.
    // 准备单个实际目标；此测试绝不分配第二原生控制器。
    let mut fixture = target_fixture().await;
    // Exact service pointer and startup identities are frozen before the issuer fault.
    // 精确服务指针及启动身份在签发者故障前固定。
    let expected = current_full(&fixture.state);
    // Original registered pair is read from the actual service getter.
    // 原注册配对从实际服务读取入口读取。
    let (_, authority) =
        crate::service_state_access::service_vulcan_plugin_native_mcp_host(&fixture.state)
            .expect("original actual first issuer");
    // Registered identities originate solely from the original target's real runtime and lease.
    // 注册身份仅来自原目标真实运行时及租约。
    let identities = fixture.target.actual_instance_ids();
    // Prepared actual handlers exist but remain closed throughout this rejected publication.
    // 准备中实际处理器存在，但在此被拒绝发布全程保持关闭。
    let bindings = fixture
        .target
        .prepared_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .expect("original actual closed bindings");
    // Actual producer observations distinguish recovery from an unnoticed process restart.
    // 实际生产者观察区分恢复与未注意到的进程重启。
    let starts = std::fs::read_to_string(&fixture.marker).expect("original actual starts");
    fixture
        .state
        .lock()
        .expect("inject actual issuer fault")
        .agent_runtime
        .as_mut()
        .expect("actual bridge")
        .vulcan_plugin_native_mcp_host = fixture.manager.register_native_mcp_host_adapter();
    // No Full witness can be created before the actual issuer matches the original native source.
    // 实际签发者匹配原原生来源之前，不能创建完整凭据。
    let mut retained = None;
    assert!(
        publish_activation_target_plugin_extension_generation(
            &fixture.state,
            &mut fixture.transaction,
            &fixture.staged,
            Some(&fixture.target),
            &mut retained,
        )
        .is_err()
    );
    assert!(retained.is_none());
    assert!(Arc::ptr_eq(&expected, &current_full(&fixture.state)));
    assert!(Arc::ptr_eq(&fixture.staged, &fixture.manager.snapshot()));
    assert_eq!(fixture.target.actual_instance_ids(), identities);
    assert_eq!(
        fixture.transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Published
    );
    // Stable declared tool lookup avoids dependence on mutable discovery ordering.
    // 稳定已声明工具查找避免依赖可变发现顺序。
    let binding = bindings
        .iter()
        .find(|binding| binding.origin_tool_id() == "published-service:actual/original_process")
        .expect("actual original_process declaration may have changed");
    assert!(
        binding
            .invoke(serde_json::json!({"message":"closed"}))
            .await
            .is_err()
    );
    fixture
        .state
        .lock()
        .expect("restore only original first issuer")
        .agent_runtime
        .as_mut()
        .expect("actual bridge")
        .vulcan_plugin_native_mcp_host = authority;
    publish_activation_target_plugin_extension_generation(
        &fixture.state,
        &mut fixture.transaction,
        &fixture.staged,
        Some(&fixture.target),
        &mut retained,
    )
    .expect("same actual target recovery");
    assert_eq!(fixture.target.actual_instance_ids(), identities);
    verify_original_and_retire(&fixture, &starts).await;
}

/// Equal Full fields in a foreign Arc cannot replace the successful service witness or authorize completion replay.
/// 外来 Arc 中相等完整字段不能替代成功服务凭据，也不能授权完成重播。
/// No inputs or return; exact pointer rejection preserves original owners and startup identities.
/// 无输入或返回；精确指针拒绝保留原所有者及启动身份。
#[tokio::test]
async fn service_selected_target_foreign_full_rejects_original_witness_without_replay() {
    // Complete one actual original target through the production service seam.
    // 通过生产服务接点完成单个实际原目标。
    let mut fixture = target_fixture().await;
    // The successful retained witness remains the sole actual publication authority in this fixture.
    // 成功保留凭据保持为此夹具中的唯一实际发布权威。
    let mut retained = None;
    publish_activation_target_plugin_extension_generation(
        &fixture.state,
        &mut fixture.transaction,
        &fixture.staged,
        Some(&fixture.target),
        &mut retained,
    )
    .expect("actual original Full completion");
    // Keep original committed ownership independent from the deliberately foreign actual slot.
    // 将原已提交所有权独立于故意外来实际槽保留。
    let committed = current_full(&fixture.state);
    // Startup records identify real processes rather than assigning authority to a counter.
    // 启动记录标识真实进程，不向计数器赋予权威。
    let starts = std::fs::read_to_string(&fixture.marker).expect("actual original startups");
    // Exact original identities must survive rejection and restoring the actual Full slot.
    // 精确原身份必须跨拒绝及恢复实际完整槽存续。
    let identities = fixture.target.actual_instance_ids();
    // This equal-field Arc has genuinely different pointer identity, without starting resources.
    // 此等字段 Arc 具有真实不同指针身份，不启动资源。
    let foreign = Arc::new((*committed).clone());
    fixture
        .state
        .lock()
        .expect("inject foreign actual Full slot")
        .agent_runtime
        .as_mut()
        .expect("actual bridge")
        .extension_generation = Arc::clone(&foreign);
    assert!(
        retained
            .as_ref()
            .expect("original witness remains retained")
            .validate_current(&fixture.state, &fixture.manager)
            .is_err()
    );
    assert!(
        publish_activation_target_plugin_extension_generation(
            &fixture.state,
            &mut fixture.transaction,
            &fixture.staged,
            Some(&fixture.target),
            &mut retained,
        )
        .is_err()
    );
    assert!(Arc::ptr_eq(&foreign, &current_full(&fixture.state)));
    assert_eq!(fixture.target.actual_instance_ids(), identities);
    assert_eq!(
        std::fs::read_to_string(&fixture.marker)
            .expect("no actual replay during pointer rejection"),
        starts
    );
    fixture
        .state
        .lock()
        .expect("restore exact original actual Full slot")
        .agent_runtime
        .as_mut()
        .expect("actual bridge")
        .extension_generation = committed;
    retained
        .as_ref()
        .expect("same original retained witness")
        .validate_current(&fixture.state, &fixture.manager)
        .expect("same exact original Full witness restored");
    verify_original_and_retire(&fixture, &starts).await;
}
