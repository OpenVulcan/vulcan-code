//! Real transient Actions and enabling probes share tracked temporary core ownership.
//! 真实瞬时 Action 与启用探测共享受跟踪的临时核心所有权。

/// Replacing a generation during native initialization cannot admit business against either generation.
/// 原生初始化期间替换代次，不能向任一代次接纳后续业务。
#[test]
fn formal_transient_initialization_cannot_cross_generation_publication() {
    // The old package starts initialization before the host publishes a replacement generation.
    // 宿主发布替代代次之前，旧包开始初始化。
    let fixture = Fixture::transient(true, false, 1000);
    fixture.transient_identity();
    // Freeze the old catalog identity for the actual public request.
    // 为真实公开请求冻结旧目录身份。
    let generation = fixture.manager.snapshot().plugins["hook-plugin"].generation;
    // One executor observes initialization and the later rejection.
    // 单个执行器观测初始化及后续拒绝。
    let runtime = executor();
    runtime.block_on(async {
        // Capture only the original manager and request authority.
        // 仅捕获原管理器及请求权威。
        let manager = fixture.manager.clone();
        // The caller explicitly selects a transient Action.
        // 调用方明确选择瞬时 Action。
        let request = fixture.transient_request(json!({}));
        // This waiter observes one original invocation without retrying it.
        // 此等待者观测单个原调用，不重试它。
        let pending = tokio::spawn(async move { manager.invoke_action(request).await });
        fixture.entered().await;
        // Preserve the original host ownership record throughout the publication.
        // 在整个发布期间保留原宿主所有权记录。
        let original = fixture.admitted().await;
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("stop old admission");
        fixture
            .manager
            .finish_lifecycle_transition("hook-plugin", &BTreeSet::new(), &BTreeSet::new(), vec![])
            .expect("publish new enabled generation");
        assert_ne!(
            fixture.manager.snapshot().plugins["hook-plugin"].generation,
            generation
        );
        fixture.events.release();
        assert!(pending.await.expect("original observer").is_err());
        fixture.empty().await;
        assert!(
            original.lock().expect("old record").operation.is_none(),
            "no Action business operation may cross publication"
        );
        assert!(original.lock().expect("old physical owner").retired);
        // The sole committed effect is the original initialization already entered before publication.
        // 唯一已提交副作用是发布之前已进入的原初始化。
        let effects = fixture.events.values.lock().expect("real effects");
        assert_eq!(effects.len(), 1);
        assert!(effects.iter().all(|event| event.topic == "enable"));
    });
}

use super::*;
use crate::{VulcanPluginActionCaller, VulcanPluginActionIdentity, VulcanPluginActionRequest};

impl Fixture {
    /// Adapt the known lifecycle fixture to one transient Action and initialization observer.
    /// 将已知生命周期夹具适配为单个瞬时 Action 及初始化观测者。
    /// The flags select real blocking initialization and mutation semantics; timeout is the manifest budget.
    /// 标志选择真实阻塞初始化及变更语义；timeout 为清单预算。
    /// Returns the same production manager with the exact revised catalog published.
    /// 返回已发布精确修订目录的同一生产管理器。
    fn transient(block_initialization: bool, mutation: bool, timeout: u64) -> Self {
        // Retain the existing real services, declared native binary and bounded parent history.
        // 保留既有真实服务、声明原生二进制及有界父级历史。
        let fixture = Self::new();
        // Only this isolated fixture package is modified before its first execution.
        // 仅在首次执行之前修改此隔离夹具包。
        let package = fixture._root.path().join("bundled/hook-plugin");
        // Freeze a manifest deadline and effect before catalog publication.
        // 在目录发布之前冻结清单截止预算及副作用类别。
        let manifest = std::fs::read_to_string(package.join("plugin.yaml"))
            .expect("fixture manifest")
            .replace("timeout_ms: 1000", &format!("timeout_ms: {timeout}"));
        // Mutation is a declared effect, never inferred from the call payload.
        // 变更是声明副作用，绝不从调用载荷推断。
        let manifest = if mutation {
            manifest.replace("effect: read", "effect: mutation")
        } else {
            manifest
        };
        std::fs::write(package.join("plugin.yaml"), manifest).expect("transient manifest");
        // Every initialization emits one real effect; only the controlled variant blocks natively.
        // 每次初始化发出一个真实副作用；只有受控变体在原生代码内阻塞。
        let topic = if block_initialization {
            "enable"
        } else {
            "initialized"
        };
        // Preserve all declared exports while making accidental business or finalizer calls observable.
        // 保留全部声明导出，同时使意外业务或关闭器调用可观测。
        let source = format!(
            "vulcan.host.call('vulcan.plugin.ui.invalidate', {{topic='{topic}',revision=0}})\n{}",
            r#"
-- Each temporary instance owns a new business counter.
-- 每个临时实例持有全新业务计数器。
local M, count = {}, 0
-- No arguments are needed; ordinary exports only satisfy the validated declaration.
-- 不需要参数；普通导出仅满足已校验声明。
function M.dispatch() return '{"ok":true,"result":{}}' end
-- No arguments are needed; any implicit finalizer produces an observable failure.
-- 不需要参数；任何隐式关闭器都产生可观测失败。
function M.shutdown()
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='wrong-finalizer',revision=0})
    error('transient call must not invoke tool or Hook shutdown')
end
M.shutdown_hooks, M.on_precheck, M.on_lifecycle = M.shutdown, M.dispatch, M.dispatch
-- args contains the original Action envelope; return fresh state and workspace authorization.
-- args 包含原 Action 信封；返回全新状态及工作区授权。
function M.ui_action(args)
    assert(type(args.arguments) == 'table' and args.payload_json == nil)
    -- Business parameters select only the test behavior, never the trusted host workspace.
    -- 业务参数仅选择测试行为，绝不选择可信宿主工作区。
    local value = args.arguments
    count = count + 1
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='business',revision=count})
    if value.block then
        vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='enable',revision=count})
    end
    -- Resolution verifies authorization without launching the fixture binary.
    -- 解析验证授权，不启动夹具二进制。
    local allowed = nil
    if value.cwd then
        allowed = pcall(vulcan.host.call, 'vulcan.plugin.process.resolve',
            {entry_id='fixture',mode='exec',options={cwd=value.cwd}})
    end
    if value.fail then return '{"ok":false,"error":"transient-business-marker"}' end
    return vulcan.json.encode({ok=true,result={count=count,allowed=allowed}})
end
return M
"#
        );
        std::fs::write(package.join("runtime/plugin.bundle.lua"), source)
            .expect("real transient bundle");
        fixture
            .manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("enabled transient catalog");
        fixture
    }

    /// Build a transient request using the original host workspace and supplied business arguments.
    /// 使用原宿主工作区及提供的业务参数构建瞬时请求。
    /// Returns a public Action request with persistent reuse explicitly disabled.
    /// 返回明确禁用持久复用的公开 Action 请求。
    fn transient_request(&self, arguments: Value) -> VulcanPluginActionRequest {
        VulcanPluginActionRequest {
            plugin_id: "hook-plugin".into(),
            action_id: "ui-action".into(),
            arguments,
            caller: VulcanPluginActionCaller::PluginUi,
            workspace_root: self._root.path().join("request"),
            reuse_enabled_runtime: false,
        }
    }

    /// Create a real request directory and freeze its current catalog incarnation.
    /// 创建真实请求目录并冻结其当前目录代次身份。
    /// Returns the unique caller identity and actual service-generation identity.
    /// 返回唯一调用方身份及真实服务代次身份。
    fn transient_identity(&self) -> (VulcanPluginActionIdentity, String) {
        std::fs::create_dir_all(self._root.path().join("request")).expect("host request workspace");
        // Identity derives only from the current immutable catalog.
        // 身份仅派生自当前不可变目录。
        let snapshot = self.manager.snapshot();
        (
            VulcanPluginActionIdentity {
                client_id: Uuid::new_v4().to_string(),
                invocation_id: Uuid::new_v4().to_string(),
            },
            format!(
                "{}:{}",
                snapshot.service_epoch, snapshot.plugins["hook-plugin"].generation
            ),
        )
    }

    /// Poll the exact public Action identity until real physical ownership has been released.
    /// 轮询精确公开 Action 身份，直到真实物理所有权释放。
    /// The identity and incarnation select one record; returns its final diagnostic phase.
    /// 身份及代次身份选择单个记录；返回最终诊断阶段。
    async fn terminal_action(
        &self,
        identity: &VulcanPluginActionIdentity,
        incarnation: &str,
    ) -> String {
        tokio::time::timeout(WAIT, async {
            loop {
                // Read the retained invocation instead of replaying it.
                // 读取保留调用，不重放它。
                let status = self
                    .manager
                    .action_status("hook-plugin", incarnation, identity, false)
                    .expect("original Action status");
                if status.terminal {
                    return status.phase;
                }
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
            }
        })
        .await
        .expect("Action actually retired")
    }
}

/// Verify fresh state, request workspace, repeated errors and absence of implicit finalizers through production.
/// 通过生产入口验证全新状态、请求工作区、重复错误及无隐式关闭器。
#[test]
fn formal_transient_state_workspace_history_and_no_implicit_finalizer() {
    // Use more sequential calls than the actual core history capacity.
    // 使用超过实际核心历史容量的连续调用数。
    let fixture = Fixture::transient(false, false, 1000);
    fixture.transient_identity();
    // This sibling is real but is outside the caller-authorized workspace.
    // 此同级目录真实存在，但位于调用方授权工作区之外。
    let sibling = fixture._root.path().join("sibling");
    std::fs::create_dir_all(&sibling).expect("sibling workspace");
    // Own only the observer executor; the formal parent owns all VMs.
    // 仅持有观测执行器；正式父级持有全部 VM。
    let runtime = executor();
    runtime.block_on(async {
        for index in 0..20 {
            // Alternate explicit business failures with successful, fresh-state calls.
            // 交替执行明确业务失败与成功的全新状态调用。
            let failing = index % 2 == 1;
            // The host request retains the same workspace regardless of the business cwd parameter.
            // 无论业务 cwd 参数如何，宿主请求都保留同一工作区。
            let result = fixture
                .manager
                .invoke_action(fixture.transient_request(json!({
                    "fail": failing,
                    "cwd": fixture._root.path().join("request")
                })))
                .await;
            if failing {
                assert!(
                    result
                        .expect_err("business failure retained")
                        .to_string()
                        .contains("transient-business-marker")
                );
            } else {
                // Decode the actual version-one business value.
                // 解码真实版本一业务值。
                let value = result.expect("successful transient Action");
                assert_eq!(value["count"], 1);
                assert_eq!(value["allowed"], true);
            }
            fixture.empty().await;
        }
        assert_eq!(
            fixture
                .manager
                .invoke_action(fixture.transient_request(json!({"cwd": sibling})))
                .await
                .expect("sibling rejection reported")["allowed"],
            false
        );
        fixture.empty().await;
        assert!(
            fixture
                .owner()
                .ui_actions
                .lock()
                .expect("persistent UI registry")
                .is_empty()
        );
        assert!(
            fixture
                .manager
                .formal_calls("")
                .expect("controller isolation")
                .is_empty()
        );
        assert!(
            !fixture
                .events
                .values
                .lock()
                .expect("actual effects")
                .iter()
                .any(|event| event.topic == "wrong-finalizer")
        );
    });
}

/// A dropped read observer withdraws before business; a declared mutation survives the same disconnect.
/// 读取观测者丢弃会在业务前撤回；声明变更在同样断开后继续。
#[test]
fn formal_transient_disconnect_distinguishes_read_and_mutation() {
    // Explicit read cancellation and observer loss share the same pre-execution boundary.
    // 明确读取取消与观测者丢失共享同一执行前边界。
    for (mutation, explicit) in [(false, false), (false, true), (true, false)] {
        // Native initialization remains blocked while the external observer is discarded.
        // 丢弃外部观测者期间，原生初始化保持阻塞。
        let fixture = Fixture::transient(true, mutation, 1000);
        // Freeze exact public diagnostic authority before dispatch.
        // 分发前冻结精确公开诊断权威。
        let (identity, incarnation) = fixture.transient_identity();
        // Keep observation on a deterministic executor.
        // 将观测保留在确定性执行器。
        let runtime = executor();
        runtime.block_on(async {
            // Move a manager clone into the public API waiter.
            // 将管理器克隆移入公开接口等待者。
            let manager = fixture.manager.clone();
            // Preserve the actual request before transferring it.
            // 转移前保留真实请求。
            let request = fixture.transient_request(json!({}));
            // Clone the exact authenticated identities for the observer.
            // 为观测者克隆精确已认证身份。
            let (caller, generation) = (identity.clone(), incarnation.clone());
            // Dropping this task emulates an HTTP waiter disconnect.
            // 丢弃此任务模拟 HTTP 等待者断开。
            let pending = tokio::spawn(async move {
                manager.invoke_ui_action(request, &generation, caller).await
            });
            fixture.entered().await;
            if explicit {
                fixture
                    .manager
                    .action_status("hook-plugin", &incarnation, &identity, true)
                    .expect("explicit read withdrawal");
            }
            pending.abort();
            assert!(
                pending
                    .await
                    .expect_err("observer cancelled")
                    .is_cancelled()
            );
            assert!(
                !fixture
                    .manager
                    .action_status("hook-plugin", &incarnation, &identity, false)
                    .expect("actual status")
                    .terminal
            );
            assert!(fixture.manager.admit_action("hook-plugin", 1).is_err());
            assert_eq!(
                fixture.manager.snapshot().plugins["hook-plugin"].active_session_count,
                1
            );
            fixture.events.release();
            assert_eq!(
                fixture.terminal_action(&identity, &incarnation).await,
                if mutation { "succeeded" } else { "cancelled" }
            );
            fixture.empty().await;
            // Actual business effects are counted independently from initialization and caller completion.
            // 独立于初始化及调用方完成统计实际业务副作用。
            let effects = fixture.events.values.lock().expect("actual effects");
            assert_eq!(
                effects
                    .iter()
                    .filter(|event| event.topic == "business")
                    .count(),
                usize::from(mutation)
            );
            assert_eq!(
                effects
                    .iter()
                    .filter(|event| event.topic == "enable")
                    .count(),
                1
            );
            assert!(!effects.iter().any(|event| event.topic == "wrong-finalizer"));
        });
    }
}

/// Executor loss preserves both initialization and running Action owners until the original native work ends.
/// 执行器丢失后保留初始化及运行中 Action 所有者，直到原原生工作结束。
#[test]
fn formal_transient_executor_loss_retains_original_owner() {
    for initialization in [true, false] {
        // Block either real initialization or the sole admitted business operation.
        // 阻塞真实初始化或唯一已接纳业务操作。
        let fixture = Fixture::transient(initialization, false, 1000);
        // Freeze exact public diagnostic authority before dispatch.
        // 分发前冻结精确公开诊断权威。
        let (identity, incarnation) = fixture.transient_identity();
        // This executor will be destroyed while native work stays alive.
        // 此执行器将在原生工作仍存活时销毁。
        let runtime = executor();
        // Preserve the same call object and pool identity across recovery.
        // 跨恢复保留同一调用对象及池身份。
        let original = runtime.block_on(async {
            // The public request owns the original envelope and authority.
            // 公开请求持有原信封及权威。
            let request = fixture.transient_request(json!({"block": !initialization}));
            // Move the manager and caller identities into the observer.
            // 将管理器及调用方身份移入观测者。
            let (manager, caller, generation) = (
                fixture.manager.clone(),
                identity.clone(),
                incarnation.clone(),
            );
            tokio::spawn(
                async move { manager.invoke_ui_action(request, &generation, caller).await },
            );
            fixture.entered().await;
            fixture.admitted().await
        });
        // Retain the exact physical pool receipt before destroying observation.
        // 销毁观测之前保留精确物理池回执。
        let pool = original
            .lock()
            .expect("original state")
            .pool
            .clone()
            .expect("original pool");
        drop(runtime);
        assert!(
            !fixture
                .manager
                .action_status("hook-plugin", &incarnation, &identity, false)
                .expect("interrupted status")
                .terminal
        );
        assert!(fixture.manager.admit_action("hook-plugin", 1).is_err());
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("reject new business");
        // A fresh observer can retire original work but cannot reconstruct or replay it.
        // 全新观测者可以退役原工作，但不能重建或重放它。
        let recovery = executor();
        recovery.block_on(async {
            // Finite drain must retain the blocked physical owner.
            // 有限排空必须保留阻塞中的物理所有者。
            let report = fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("bounded drain");
            assert_eq!(report.closed_formal_calls, 0);
            assert!(!report.failures.is_empty());
            assert_eq!(
                original.lock().expect("retained state").pool.as_ref(),
                Some(&pool)
            );
            assert!(fixture.manager.admit_action("hook-plugin", 1).is_err());
            fixture.events.release();
            assert_eq!(
                fixture.terminal_action(&identity, &incarnation).await,
                "supervisor_failed"
            );
            fixture.empty().await;
            assert!(original.lock().expect("original retirement").retired);
            assert!(fixture.manager.admit_action("hook-plugin", 1).is_ok());
            // Committed effects prove there was no replacement initialization or finalizer.
            // 已提交副作用证明没有替代初始化或关闭器。
            let effects = fixture.events.values.lock().expect("actual effects");
            assert_eq!(
                effects
                    .iter()
                    .filter(|event| event.topic == "business")
                    .count(),
                usize::from(!initialization)
            );
            assert_eq!(
                effects
                    .iter()
                    .filter(|event| event.topic == "enable")
                    .count(),
                1
            );
            assert!(!effects.iter().any(|event| event.topic == "wrong-finalizer"));
        });
    }
}

/// The original manifest deadline bounds initialization and business without releasing native ownership early.
/// 原清单截止时间约束初始化及业务，不提前释放原生所有权。
#[test]
fn formal_transient_deadline_retains_actual_native_work() {
    for initialization in [true, false] {
        // The short original deadline expires while a native callback deliberately remains blocked.
        // 刻意阻塞原生回调期间，短原始截止时间到期。
        let fixture = Fixture::transient(initialization, false, DRAIN_MS);
        // Freeze caller identity and request workspace before invocation.
        // 调用前冻结调用方身份及请求工作区。
        let (identity, incarnation) = fixture.transient_identity();
        // Only the observer uses Tokio; native workers belong to the core.
        // 仅观测者使用 Tokio；原生工作者属于核心。
        let runtime = executor();
        runtime.block_on(async {
            // Move the exact request into the external observer.
            // 将精确请求移入外部观测者。
            let request = fixture.transient_request(json!({"block": !initialization}));
            // Keep the exact original identities for later diagnostic queries.
            // 保留精确原身份供后续诊断查询。
            let (manager, caller, generation) = (
                fixture.manager.clone(),
                identity.clone(),
                incarnation.clone(),
            );
            // The public caller may time out while actual native work remains retained.
            // 实际原生工作仍被保留时，公开调用方可能超时。
            let pending = tokio::spawn(async move {
                manager.invoke_ui_action(request, &generation, caller).await
            });
            fixture.entered().await;
            assert!(
                pending
                    .await
                    .expect("deadline observer")
                    .expect_err("original deadline")
                    .to_string()
                    .contains("timed out")
            );
            assert!(
                !fixture
                    .manager
                    .action_status("hook-plugin", &incarnation, &identity, true)
                    .expect("late cancellation preserves deadline status")
                    .terminal
            );
            assert!(fixture.manager.admit_action("hook-plugin", 1).is_err());
            fixture.events.release();
            assert_eq!(
                fixture.terminal_action(&identity, &incarnation).await,
                "timed_out"
            );
            fixture.empty().await;
            assert_eq!(
                fixture
                    .events
                    .values
                    .lock()
                    .expect("effects")
                    .iter()
                    .filter(|event| event.topic == "business")
                    .count(),
                usize::from(!initialization)
            );
        });
    }
}

/// Probe initialization validates declared exports, reclaims bounded history and never executes business or shutdown.
/// 探测初始化校验声明导出、回收有界历史，且绝不执行业务或关闭函数。
#[test]
fn formal_probe_validates_initialization_without_business() {
    // The production probe uses the same real module loader as Actions.
    // 生产探测使用与 Action 相同的真实模块加载器。
    let fixture = Fixture::transient(false, false, 1000);
    // Observe public probe behavior through the actual executor.
    // 通过真实执行器观测公开探测行为。
    let runtime = executor();
    runtime.block_on(async {
        assert!(
            fixture.manager.probe_plugin("hook-plugin").await.is_err(),
            "enabled is not enabling"
        );
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("begin state transition");
        fixture
            .manager
            .rollback_draining_to_enabling("hook-plugin")
            .expect("probe scope");
        for _ in 0..20 {
            fixture
                .manager
                .probe_plugin("hook-plugin")
                .await
                .expect("real initialization");
            fixture.empty().await;
        }
        // No tool capacity or persistent UI instance is created by these probes.
        // 这些探测不创建工具容量或持久 UI 实例。
        let effects = fixture.events.values.lock().expect("probe effects");
        assert_eq!(effects.len(), 20);
        assert!(effects.iter().all(|event| event.topic == "initialized"));
    });
    // An invalid export must fail initialization and still retire its actual owner.
    // 无效导出必须使初始化失败，同时仍退役其实际所有者。
    let invalid = Fixture::transient(false, false, 1000);
    // Modify this isolated package before its first execution and republish its generation.
    // 首次执行前修改此隔离包并重新发布其代次。
    let bundle = invalid
        ._root
        .path()
        .join("bundled/hook-plugin/runtime/plugin.bundle.lua");
    // The exact required Action export is deliberately removed, with no substitute lookup.
    // 刻意移除精确必需 Action 导出，不进行替代查找。
    let source = std::fs::read_to_string(&bundle)
        .expect("original exports")
        .replace("return M\n", "M.ui_action = nil\nreturn M\n");
    std::fs::write(bundle, source).expect("invalid declared export");
    invalid
        .manager
        .refresh_catalog(&BTreeSet::from(["hook-plugin".into()]), &BTreeSet::new())
        .expect("disabled invalid catalog");
    invalid
        .manager
        .begin_enabling("hook-plugin")
        .expect("invalid probe scope");
    runtime.block_on(async {
        // Inspect the original loader error rather than a synthetic probe export failure.
        // 检查原加载器错误，不使用合成探测导出失败。
        let error = invalid
            .manager
            .probe_plugin("hook-plugin")
            .await
            .expect_err("missing Action export");
        assert!(error.to_string().contains("ui_action"), "{error}");
        invalid.empty().await;
    });
}

/// Probe interruption retains zero-quota work and cleanup diagnostics until actual native retirement.
/// 探测中断后保留不占配额的工作及清理诊断，直到原生工作实际退役。
#[test]
fn formal_probe_interruption_preserves_ownership_and_warning() {
    // Initialization is the only permitted real work in this scenario.
    // 此场景中初始化是唯一允许的真实工作。
    let fixture = Fixture::transient(true, false, 1000);
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("begin transition");
    fixture
        .manager
        .rollback_draining_to_enabling("hook-plugin")
        .expect("probe scope");
    // Destroy the observer independently of the real callback worker.
    // 独立于真实回调工作者销毁观测者。
    let runtime = executor();
    // Retain the original registered call across executor loss.
    // 跨执行器丢失保留原已登记调用。
    let original = runtime.block_on(async {
        // The public probe does not own its actual initialization worker.
        // 公开探测不拥有其真实初始化工作者。
        let manager = fixture.manager.clone();
        tokio::spawn(async move { manager.probe_plugin("hook-plugin").await });
        fixture.entered().await;
        fixture.admitted().await
    });
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].active_session_count,
        0
    );
    drop(runtime);
    // Recovery operates exclusively on the original owner.
    // 恢复仅操作原所有者。
    let recovery = executor();
    recovery.block_on(async {
        // The finite drain reports unfinished initialization while retaining its identity.
        // 有限排空报告未完成初始化，同时保留其身份。
        let report = fixture
            .manager
            .drain_plugin_sessions("hook-plugin")
            .await
            .expect("bounded probe drain");
        assert!(!report.failures.is_empty());
        assert_eq!(report.closed_formal_calls, 0);
        fixture
            .manager
            .clear_cleanup_diagnostics_if_fully_drained("hook-plugin")
            .expect("inspect real ownership");
        assert!(
            fixture.manager.snapshot().plugins["hook-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(&original.id))
        );
        fixture.events.release();
        fixture.empty().await;
        assert!(original.lock().expect("original checkpoint").retired);
        fixture
            .manager
            .clear_cleanup_diagnostics_if_fully_drained("hook-plugin")
            .expect("clear completed warning");
        assert!(
            !fixture.manager.snapshot().plugins["hook-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(&original.id))
        );
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("one initialization")
                .len(),
            1
        );
    });
}
