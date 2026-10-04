//! Real completed old native instances retire before one distinct private restoration instance starts.
//! 真实已完成旧原生实例退役后，才启动一个独立私有恢复实例。

use super::*;
use crate::mcp::{PredecessorPluginMcpRuntime, predecessor_global_mcp_server_configs};
use vulcan_plugin_runtime::{
    VulcanPluginPredecessorRestoration, VulcanPluginPredecessorRestorationSource,
};

mod completion;

/// Keep only production original owners while reusing the established real Node package fixture.
/// 复用既有真实 Node 插件包夹具，同时仅保留生产原所有者。
struct OldFixture {
    /// Completed target becomes the actual managed old generation before the new update starts.
    /// 新更新开始前，已完成目标成为实际托管旧代次。
    base: Fixture,
    /// Original independent Pending ownership is established before durable abort.
    /// 原独立 Pending 所有权在持久中止前建立。
    restoration: VulcanPluginPredecessorRestoration,
    /// Actual source retains the old immutable selection under the closed successor Arc.
    /// 实际来源在关闭后继 Arc 下保留旧不可变选择。
    source: Arc<VulcanPluginPredecessorRestorationSource>,
    /// Original native admission lineage retains the local plugin drain fence.
    /// 原原生入场谱系保留本地插件排空屏障。
    access: McpRuntimeAccess,
    /// Distinct target configuration would write here if accidentally used for old restoration.
    /// 若旧恢复意外使用目标，独立目标配置会写入此处。
    target_marker: PathBuf,
}

/// Cancelling the first observer preserves its registered original, while retirement permanently prevents replay.
/// 取消首次观察者保留其已登记原实例，退役则永久阻止重放。
/// No inputs or return value; real child markers and physical release establish the actual boundaries.
/// 无输入或返回值；真实子进程标记及物理释放确立实际边界。
#[tokio::test]
async fn predecessor_native_cancelled_observer_retired_original_never_replays() {
    // A current-thread executor keeps newly dispatched tasks pending until the first actual observer is polled.
    // 单线程执行器使新派发任务在首次实际观察者轮询前保持待处理。
    let fixture = old_fixture().await;
    // Retain the actual same physical preparation while proving duplicate startup rejection.
    // 证明重复启动拒绝期间保留实际相同物理准备。
    let prepared = fixture
        .base
        .manager
        .prepare_predecessor_mcp_resources(&fixture.source, &fixture.base.native_host)
        .expect("actual old preparation");
    // The same logical control retains no actual environment lease.
    // 相同逻辑控制不保留实际环境租约。
    let control = prepared.control();
    // Materialized definitions remain bound to the exact same physical bundle.
    // 物化定义保持绑定至精确相同物理资源包。
    let configs = predecessor_global_mcp_server_configs(prepared.clone(), &NoSecrets)
        .expect("actual old configurations");
    // Register and dispatch once before the cancellable observation begins.
    // 可取消观察开始前，登记及派发一次。
    let runtime = PredecessorPluginMcpRuntime::start(
        &fixture.access,
        &fixture.base.manager,
        &fixture.base.transaction,
        configs,
        &fixture.base.native_host,
    )
    .expect("actual old startup");
    {
        // Poll the real readiness future once, then drop this observer before any scheduled child work runs.
        // 轮询真实就绪 future 一次，然后在任何已调度子进程工作运行前释放此观察者。
        let ready = runtime.wait_until_ready_default();
        tokio::pin!(ready);
        tokio::select! {
            biased;
            outcome = &mut ready => panic!("newly dispatched original unexpectedly completed before executor scheduling: {outcome:?}"),
            _ = std::future::ready(()) => {}
        }
    }
    // Observing the same registered work again resumes no startup writer and starts no replacement process.
    // 再次观察相同已登记工作不会恢复启动写入者，也不会启动替代进程。
    runtime
        .wait_until_ready_default()
        .await
        .expect("same original after observer cancellation");
    assert_eq!(
        fs::read_to_string(&fixture.base.marker)
            .expect("one restored native child")
            .lines()
            .count(),
        3
    );
    // A second sealed definition pair still cannot replay the consumed source.
    // 第二个密封定义组合仍不能重放已消费来源。
    let repeated = predecessor_global_mcp_server_configs(prepared.clone(), &NoSecrets)
        .expect("same definitions again");
    assert!(
        PredecessorPluginMcpRuntime::start(
            &fixture.access,
            &fixture.base.manager,
            &fixture.base.transaction,
            repeated,
            &fixture.base.native_host
        )
        .err()
        .expect("duplicate startup rejection")
        .contains("already attempted")
    );
    // Independent release evidence remains useful after the last physical preparation is dropped.
    // 最后物理准备释放后，独立释放证据仍可使用。
    let observation = runtime.observer();
    drop(prepared);
    runtime.shutdown();
    drop(runtime);
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("actual cancelled observer original retirement");
    assert!(
        fixture
            .base
            .manager
            .prepare_predecessor_mcp_resources(&fixture.source, &fixture.base.native_host)
            .expect_err("retired physical bundle cannot rebuild")
            .message
            .contains("cannot be rebuilt")
    );
    assert!(
        control
            .begin_transport_attempt(
                &fixture.base.manager,
                &fixture.base.transaction,
                &fixture.base.native_host
            )
            .is_err()
    );
    assert!(
        fixture
            .base
            .store
            .predecessor_handoff_blocks_admission("target-mcp")
            .expect("Pending after retirement")
    );
    assert_eq!(
        fs::read_to_string(&fixture.base.marker)
            .expect("no native replay")
            .lines()
            .count(),
        3
    );
    assert!(!fixture.target_marker.exists());
}

/// Global shutdown rejects actual dispatch even when a private old source is valid and local drain is expected.
/// 即使私有旧来源有效且预期本地排空，全局关闭仍拒绝实际派发。
/// No inputs or return value; failure consumes the same attempt and never writes another child marker.
/// 无输入或返回值；失败消费相同尝试，绝不再写子进程标记。
#[tokio::test]
async fn predecessor_native_global_shutdown_rejects_dispatch_and_replay() {
    // All old instances physically retire before the restoration source is prepared.
    // 恢复来源准备前，全部旧实例物理退役。
    let fixture = old_fixture().await;
    // Retain the exact physical bundle for both rejected attempts without creating a second source.
    // 为两次被拒绝尝试保留精确物理资源包，不创建第二来源。
    let prepared = fixture
        .base
        .manager
        .prepare_predecessor_mcp_resources(&fixture.source, &fixture.base.native_host)
        .expect("private old resources");
    // Original control records irreversible admission consumption before backend shutdown rejection.
    // 原控制在后端关闭拒绝前记录不可逆入场消费。
    let control = prepared.control();
    fixture.access.shutdown();
    // This explicit global closure remains distinct from the permitted local plugin drain.
    // 此显式全局关闭保持独立于允许的本地插件排空。
    let configs = predecessor_global_mcp_server_configs(prepared.clone(), &NoSecrets)
        .expect("closed input materialization");
    assert!(
        PredecessorPluginMcpRuntime::start(
            &fixture.access,
            &fixture.base.manager,
            &fixture.base.transaction,
            configs,
            &fixture.base.native_host
        )
        .err()
        .expect("global shutdown prevents dispatch")
        .contains("admission is closed")
    );
    assert!(
        control
            .begin_transport_attempt(
                &fixture.base.manager,
                &fixture.base.transaction,
                &fixture.base.native_host
            )
            .err()
            .expect("failed original never replays")
            .message
            .contains("already attempted")
    );
    assert_eq!(
        fs::read_to_string(&fixture.base.marker)
            .expect("no shutdown dispatch child")
            .lines()
            .count(),
        2
    );
    assert!(!fixture.target_marker.exists());
    assert!(
        fixture
            .base
            .store
            .predecessor_handoff_blocks_admission("target-mcp")
            .expect("failed start remains Pending")
    );
}

/// Build actual Completed old publication, real Lua/native drain, Pending, Aborted and signed old Source.
/// 构建实际 Completed 旧发布、真实 Lua／原生排空、Pending、Aborted 及已签发旧来源。
/// No inputs; return actual production owners without cloned or fabricated readiness receipts.
/// 无输入；返回实际生产所有者，不克隆或伪造就绪回执。
async fn old_fixture() -> OldFixture {
    // Existing fixture runs one candidate validation process and one actual selected process.
    // 既有夹具运行一个候选校验进程及一个实际已选进程。
    let mut base = fixture().await;
    // Retain the same genuine lease through production completion so it becomes actual managed old work.
    // 跨生产完成保留同一真正租约，使其成为实际托管旧工作。
    let access = base
        .target
        .publication_access(&base.access)
        .expect("actual old lease view");
    // Exact closed target Arc publishes old discovery before native confirmation opens its same instance.
    // 精确关闭目标 Arc 在原生确认开放其相同实例前发布旧发现。
    let final_catalog = base
        .manager
        .publish_activation_target_handoff(&base.transaction, &base.staged, |_| Ok(()))
        .expect("real old catalog publication");
    base.manager
        .complete_activation_target_handoff_with(
            &mut base.transaction,
            &final_catalog,
            &base.target,
        )
        .expect("real old Completed");
    assert_eq!(
        base.transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Completed
    );
    assert_eq!(
        fs::read_to_string(&base.marker)
            .expect("initial old starts")
            .lines()
            .count(),
        2
    );
    // Completed is a durable result; the foreground original File must actually retire before a new update.
    // Completed 为持久结果；新更新前，前台原文件必须实际退役。
    // The genuine published transport remains retained independently through access and its same lease.
    // 真正已发布传输通过 access 及其相同租约独立保持保留。
    drop(base.transaction);
    // This reused target-only fixture lazily inspected recovery while its initial original was Published.
    // 此复用仅目标夹具在初始原请求 Published 时才延迟检查恢复。
    // Reconcile that real Completed receipt through normal catalog refresh before a different update exists.
    // 在不同更新存在前，通过普通目录刷新核对该真实 Completed 回执。
    base.manager
        .refresh_catalog(
            &BTreeSet::new(),
            &BTreeSet::from([String::from("target-mcp")]),
        )
        .expect("reconcile completed old original before new preparation");
    assert_eq!(
        base.manager.snapshot().plugins["target-mcp"].runtime_state,
        vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabled
    );
    // Original source bytes remain outside immutable generations and may become a distinct next candidate.
    // 原来源字节保持在不可变代次外，可以成为独立下一候选。
    let package = base._temp.path().join("source/target-mcp");
    // Deliberately change source version before inspection, leaving immutable old bytes untouched.
    // 检查前刻意改变来源版本，保持不可变旧字节不动。
    let manifest = fs::read_to_string(package.join("plugin.yaml"))
        .expect("source manifest")
        .replace("version: 1.0.0", "version: 2.0.0");
    fs::write(package.join("plugin.yaml"), manifest).expect("distinct next source");
    // The new selected value must never replace the old marker used by restoration.
    // 新所选值绝不能替换恢复使用的旧标记。
    let target_marker = base._temp.path().join("wrong-target-starts.txt");
    // Inspection supplies current exact permissions and fingerprint, not assumptions from the old source.
    // 检查提供当前精确权限及指纹，不使用旧来源假设。
    let preview = base
        .manager
        .inspect_local_user_plugin(&package)
        .expect("new source inspection");
    // Read actual old storage selection after its confirmed completion.
    // 在确认完成之后读取实际旧存储选择。
    let old = base
        .store
        .activation("target-mcp")
        .expect("old disk selection")
        .expect("managed old");
    // The full original update retains exact old values and stable policy before any drain.
    // 完整原更新在任何排空前保留精确旧值及稳定策略。
    let preparation = base
        .store
        .begin_activation_update_preparation(
            uuid::Uuid::new_v4(),
            &package,
            &preview.content_fingerprint,
            Some(&old),
            preview.permissions,
            VulcanPluginUpdatePolicy {
                explicitly_enabled: true,
                explicitly_disabled: false,
                predecessor_default_enabled: true,
            },
        )
        .expect("new original preparation");
    // Actual package preparation precedes credential staging under the same request.
    // 相同请求下，实际插件包准备先于凭据暂存。
    let generation = base
        .store
        .prepare_activation_package(&preparation, &package)
        .expect("new immutable bytes");
    base.store
        .begin_activation_credential_staging(
            &preparation,
            &generation,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("new credential preparation")
        .write_and_finish(&NoSecrets, &VulcanPluginCredentialValues::new())
        .expect("new credential writer retirement");
    // Exact new configuration remains isolated and deliberately differs from old selected values.
    // 精确新配置保持隔离，刻意区别于旧所选值。
    let mut transaction = base
        .store
        .finish_activation_preparation(
            preparation,
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"marker": target_marker}),
            },
        )
        .expect("exact configured new request");
    // Only a temporary actual candidate borrow authorizes the completed old drain and typed restoration.
    // 仅临时实际候选借用授权已完成旧排空及类型化恢复。
    let candidate = base
        .store
        .inspect_activation_candidate(&transaction)
        .expect("actual new candidate");
    // Freeze enabled old catalog before the production closure issues its draining generation.
    // 生产关闭签发排空代次前固定启用旧目录。
    let original = base.manager.snapshot();
    // Actual Lua drain authority remains distinct from external native retirement.
    // 实际 Lua 排空权威保持独立于外部原生退役。
    let drain = base
        .manager
        .begin_activation_predecessor_drain(&candidate, &original)
        .expect("old Lua closure");
    access
        .begin_plugin_mcp_drain("target-mcp")
        .expect("actual old native closure");
    access
        .wait_plugin_mcp_drain("target-mcp", OBSERVE)
        .await
        .expect("actual old native retirement");
    base.target
        .observer()
        .wait_until_released(OBSERVE)
        .await
        .expect("actual old release evidence");
    assert!(
        base.manager
            .drain_activation_predecessor(&drain)
            .await
            .expect("actual old Lua retirement")
            .failures
            .is_empty()
    );
    // No old lifecycle callback is declared; production validates its explicit absent restoration evidence.
    // 未声明旧生命周期回调；生产校验其明确缺失恢复证据。
    let mut restoration = base
        .manager
        .prepare_activation_predecessor_restoration(&candidate, drain)
        .expect("actual typed old restoration");
    drop(candidate);
    // The original immutable update policy supplies this plugin's actual explicit memberships.
    // 原不可变更新策略提供此插件实际明确成员关系。
    let policy = transaction
        .record()
        .update_policy()
        .expect("actual original plugin policy");
    // Preserve the declared memberships through reservation and abort instead of guessing default overrides.
    // 跨预留及中止保留已声明成员关系，而非猜测默认覆盖。
    let (explicitly_enabled, explicitly_disabled) =
        (policy.explicitly_enabled, policy.explicitly_disabled);
    base.manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            explicitly_enabled,
            explicitly_disabled,
        )
        .expect("independent Pending before abort");
    base.manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            explicitly_enabled,
            explicitly_disabled,
        )
        .expect("actual original Aborted closed successor");
    // Source factory signs the same physical owner without rewriting Pending or reopening ordinary business.
    // 来源工厂签发同一物理所有者，不重写 Pending 或重开普通业务。
    let source = base
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("actual old Source");
    assert_eq!(
        transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Aborted
    );
    // Reuse only the fixture's foreground slot for this exact new aborted original, never the completed owner.
    // 仅复用夹具前台槽保存此精确新中止原请求，绝不保留已完成所有者。
    base.transaction = transaction;
    OldFixture {
        base,
        restoration,
        source,
        access,
        target_marker,
    }
}

/// Real old restoration consumes once, observes its new PID and same lease, and keeps actual business closed.
/// 真实旧恢复消费一次，观察其新 PID 及相同租约，并保持实际业务关闭。
/// No inputs or return value; physical probes prove pure control/profile do not retain old recovery files.
/// 无输入或返回值；物理探测证明纯控制及画像不保留旧恢复文件。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn predecessor_native_real_old_once_ready_closed_and_physical_release() {
    // Reuse production old completion and retirement before private restored startup.
    // 私有恢复启动前，复用生产旧完成及退役。
    let fixture = old_fixture().await;
    // Preparation retains the same old source, environment and immutable configuration.
    // 准备保留同一旧来源、环境及不可变配置。
    let prepared = fixture
        .base
        .manager
        .prepare_predecessor_mcp_resources(&fixture.source, &fixture.base.native_host)
        .expect("old resources");
    // Historical control never retains the physical preparation itself.
    // 历史控制绝不保留物理准备本身。
    let control = prepared.control();
    // Original pure profile identifies actual consumed input allocation without owning its source File.
    // 原纯画像标识实际消费输入分配，不拥有其来源文件。
    let profile = control
        .host_profile(&fixture.base.native_host)
        .expect("same first issuer");
    // Foreign issuer must fail before consuming startup or spawning any child.
    // 外来签发者必须在消费启动或创建任何子进程前失败。
    let foreign = fixture.base.manager.register_native_mcp_host_adapter();
    assert!(control.host_profile(&foreign).is_err());
    assert!(
        fixture
            .base
            .manager
            .prepare_predecessor_mcp_resources(&fixture.source, &foreign)
            .is_err()
    );
    // Sealed materialization may be cloned only by retaining the same physical preparation.
    // 密封物化仅可通过保留同一物理准备来克隆。
    let rejected = predecessor_global_mcp_server_configs(prepared.clone(), &NoSecrets)
        .expect("foreign attempt exact old configs");
    assert!(
        PredecessorPluginMcpRuntime::start(
            &fixture.access,
            &fixture.base.manager,
            &fixture.base.transaction,
            rejected,
            &foreign
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(&fixture.base.marker)
            .expect("before restoration starts")
            .lines()
            .count(),
        2
    );
    // Actual original start uses the still-live same preparation and first registered host.
    // 实际原启动使用仍存活相同准备及首次已注册宿主。
    let configs = predecessor_global_mcp_server_configs(prepared.clone(), &NoSecrets)
        .expect("exact old inputs");
    // This controller owns the sole actual native restoration lease.
    // 此控制器拥有唯一实际原生恢复租约。
    let runtime = PredecessorPluginMcpRuntime::start(
        &fixture.access,
        &fixture.base.manager,
        &fixture.base.transaction,
        configs,
        &fixture.base.native_host,
    )
    .expect("private old native startup");
    runtime
        .wait_until_ready_default()
        .await
        .expect("actual old native Ready");
    runtime
        .wait_until_ready_default()
        .await
        .expect("same terminal result confirmation");
    assert!(runtime.profile().same_source(&profile));
    runtime
        .verify_source("target-mcp:actual", &profile)
        .expect("actual same source inputs");
    assert_ne!(
        runtime.actual_instance_ids(),
        fixture.base.target.actual_instance_ids()
    );
    assert!(!runtime.actual_instance_ids().0.is_nil());
    assert!(!runtime.actual_instance_ids().1.is_nil());
    // Every startup PID was committed by the real child fixture; restoration is a new process, never resurrection.
    // 每个启动 PID 均由真实子进程夹具提交；恢复为新进程，绝非复活。
    let pids = fs::read_to_string(&fixture.base.marker).expect("real child startup PIDs");
    assert_eq!(pids.lines().count(), 3);
    assert_eq!(pids.lines().collect::<BTreeSet<_>>().len(), 3);
    assert!(
        !fixture.target_marker.exists(),
        "old restore must not consume aborted target config"
    );
    // Actual discovery contains the remote tool while native business admission remains Preparing.
    // 实际发现包含远端工具，同时原生业务入场保持 Preparing。
    let native = runtime.observer().runtime;
    assert!(matches!(
        native.with_connected_target_launch_inputs(
            "target-mcp:actual",
            &fixture.base.target.profile(),
            |_, _| ()
        ),
        Err(crate::mcp::registry::McpLaunchInputCaptureError::DifferentPluginSource)
    ));
    // Diagnostic views come from the actual Ready snapshot without granting access.
    // 诊断视图来自实际 Ready 快照，不授予访问。
    let views = native
        .snapshot()
        .filtered_tool_views(&McpRuntimeToolAllowlist::default());
    // Feature lookup remains stable when other fixture tools are added.
    // 增加其他夹具工具时，特征查找保持稳定。
    let view = views
        .iter()
        .find(|view| view.origin_tool_id().ends_with("read_page"))
        .expect("read_page declaration may have changed");
    assert!(native.call_tool_by_id(view.origin_tool_id(), serde_json::json!({"url":"https://private-old.test","owner_session_id":"fixture-owner"})).await.is_err());
    assert!(
        native
            .available_tool_views_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    assert!(
        fixture
            .base
            .manager
            .snapshot()
            .enabled_mcp_servers
            .is_empty()
    );
    assert!(
        fixture
            .base
            .store
            .predecessor_handoff_blocks_admission("target-mcp")
            .expect("Pending remains")
    );
    assert!(
        control
            .begin_transport_attempt(
                &fixture.base.manager,
                &fixture.base.transaction,
                &fixture.base.native_host
            )
            .is_err()
    );
    // Probe the independently reserved production File, whose path is defined by predecessor_handoff.rs.
    // 探测独立预留生产文件，其路径由 predecessor_handoff.rs 定义。
    let probe = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                .base
                ._temp
                .path()
                .join("user/.vulcan-generations/locks")
                .join(format!(
                    "target-mcp.predecessor-handoff.{}.lock",
                    fixture.base.transaction.record().request_id()
                )),
        )
        .expect("physical recovery probe");
    assert!(matches!(
        probe.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    // Release non-native old owners first; actual transports alone must still retain the independent File.
    // 先释放非原生旧所有者；实际传输自身必须仍保留独立文件。
    drop(prepared);
    drop(fixture.restoration);
    drop(fixture.source);
    drop(fixture.base.transaction);
    assert!(matches!(
        probe.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    // Observer carries actual independent aggregate release evidence without holding physical resources.
    // 观察者携带实际独立聚合释放证据，不持有物理资源。
    let observation = runtime.observer();
    runtime.shutdown();
    drop(runtime);
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("real physical resource release");
    probe
        .try_lock()
        .expect("pure control/profile cannot retain recovery File");
    probe.unlock().expect("unlock probe only");
    assert!(
        control
            .host_profile(&fixture.base.native_host)
            .expect("historical pure profile")
            .same_source(&profile)
    );
    assert!(
        fixture
            .base
            .store
            .predecessor_handoff_blocks_admission("target-mcp")
            .expect("ownerless Pending remains closed")
    );
    assert_eq!(
        fs::read_to_string(&fixture.base.marker)
            .expect("no replay after release")
            .lines()
            .count(),
        3
    );
}
