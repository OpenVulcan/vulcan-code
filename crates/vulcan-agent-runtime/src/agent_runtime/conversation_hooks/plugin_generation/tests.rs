//! Regression tests for per-execution plugin hook selection over shared turn state.
//! 共享回合状态上按执行选择插件 Hook 的回归测试。

use super::*;
use crate::agent_runtime::ExtensionGeneration;
use crate::agent_runtime::conversation_hooks::HOOK_CONVERSATION_TURN_PRECHECK;
use crate::model::{PendingSessionPromptSource, SessionId};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use vulcan_hooks::{
    FnHookEventListener, FnHookHandler, HookContext, HookDispatchRequest, HookEvent, HookEventKind,
    HookInvocation, HookMode, HookPayload, HookPointDescriptor, HookRegistration, HookResult,
    SystemPromptHookPhase,
};

/// Creates a metadata-only manager under `root`; no engine, network, or credentials are accessed.
/// 在 `root` 下创建仅元数据管理器；不访问引擎、网络或凭据。
/// Returns the manager whose empty catalog supplies real snapshot ownership.
/// 返回管理器，其空目录提供真实快照所有权。
fn manager(root: &std::path::Path) -> VulcanPluginRuntimeManager {
    VulcanPluginRuntimeManager::without_runtime(
        root.join("system"),
        root.join("user"),
        root.join("bridge"),
        root.join("distributions"),
        root.join("state"),
        root.join("runtime"),
    )
}

/// Installs `id` with a concrete `label` callback into `dispatcher`, recording executions in `calls`.
/// 将具有具体 `label` 回调的 `id` 安装到 `dispatcher`，在 `calls` 记录执行。
/// Returns after fixture registration or fails the test.
/// 夹具注册后返回，否则使测试失败。
fn record(
    dispatcher: &mut HookDispatcher,
    id: &str,
    label: &'static str,
    calls: &Arc<Mutex<Vec<&'static str>>>,
) {
    // Closure ownership makes frozen and live callback identity independently observable.
    // 闭包所有权使冻结与实时回调身份可分别观察。
    let calls = Arc::clone(calls);
    dispatcher
        .register(
            HookRegistration::new(id, HOOK_CONVERSATION_TURN_PRECHECK, HookMode::Observe),
            Arc::new(FnHookHandler::new(move |_: HookInvocation| {
                // Each invocation records the callback captured at registration time.
                // 每次调用记录注册时捕获的回调。
                let calls = Arc::clone(&calls);
                async move {
                    calls
                        .lock()
                        .expect("fixture runtime or observation lock should remain healthy")
                        .push(label);
                    Ok(HookResult::default())
                }
            })),
        )
        .expect("hook generation fixture operation should succeed");
}

/// Executes `dispatcher` after its preparation lock has been released and asserts actual handlers.
/// 在准备锁释放后执行 `dispatcher`，断言实际处理器。
async fn dispatch(dispatcher: HookDispatcher) {
    dispatcher
        .dispatch(HookDispatchRequest::new(
            HookContext::new("generation-test", HOOK_CONVERSATION_TURN_PRECHECK),
            HookPayload::from_serializable(serde_json::json!({}))
                .expect("hook generation fixture operation should succeed"),
        ))
        .await
        .expect("hook generation fixture operation should succeed");
}

/// Proves old and new callbacks remain distinct while host handlers and logical turns remain shared.
/// 证明新旧回调保持独立，同时宿主处理器与逻辑回合继续共享。
#[tokio::test]
async fn scoped_generations_keep_exact_handlers_and_one_turn_state() {
    // Real catalog ownership is independent from the synthetic callback identity fixture.
    // 真实目录所有权独立于合成回调身份夹具。
    let temp = tempfile::tempdir().expect("create isolated generation fixture");
    // A metadata-only manager avoids executing Lua in a dispatcher ownership test.
    // 仅元数据管理器避免在调度器所有权测试中执行 Lua。
    let manager = manager(temp.path());
    // Observations distinguish two callbacks sharing one registration ID.
    // 观察记录区分共享一个注册标识的两个回调。
    let calls = Arc::new(Mutex::new(Vec::new()));
    // Shared state contains the latest base registration.
    // 共享状态包含最新基础注册项。
    let runtime = Mutex::new(ConversationHookRuntime::new());
    // Build the old partition before the host installs the newer handler.
    // 在宿主安装较新处理器前构建旧分区。
    let mut old_dispatcher = HookDispatcher::default();
    // This identity uses the sole production prefix constant.
    // 本身份使用唯一生产前缀常量。
    let id = format!("{VULCAN_PLUGIN_HOOK_REGISTRATION_PREFIX}same");
    record(&mut old_dispatcher, &id, "old", &calls);
    // Private construction isolates projection behavior; production preparation is tested separately.
    // 私有构造隔离投影行为；生产准备另行测试。
    let old = VulcanPluginConversationHooks {
        catalog: manager.snapshot(),
        registry: old_dispatcher.registry().clone(),
    };
    // Lifecycle event counts must not change merely from reading a captured partition.
    // 仅读取捕获分区不得改变生命周期事件计数。
    let lifecycle_events = Arc::new(AtomicUsize::new(0));
    // Actual dispatch notifications prove listeners were preserved rather than silently dropped.
    // 实际派发通知证明监听器被保留，而非被静默丢弃。
    let dispatch_events = Arc::new(AtomicUsize::new(0));
    {
        // Permanent host registration uses the base lock, outside any generation scope.
        // 持久宿主注册使用基础锁，位于所有代次作用域之外。
        let mut base = runtime
            .lock()
            .expect("fixture runtime or observation lock should remain healthy");
        record(base.dispatcher_mut(), &id, "new", &calls);
        record(base.dispatcher_mut(), "host/live", "host", &calls);
        // The base listener must survive projection and observe real later dispatches.
        // 基础监听器必须在投影后保留，并观察后续真实派发。
        let events = Arc::clone(&lifecycle_events);
        // Track dispatched requests separately from forbidden projection lifecycle notifications.
        // 分开统计派发请求与投影过程中禁止的生命周期通知。
        let dispatched = Arc::clone(&dispatch_events);
        base.dispatcher_mut()
            .add_listener(Arc::new(FnHookEventListener::new(
                move |event: HookEvent| {
                    if matches!(
                        event.kind,
                        HookEventKind::Registered | HookEventKind::Unregistered
                    ) {
                        events.fetch_add(1, Ordering::SeqCst);
                    }
                    if matches!(event.kind, HookEventKind::DispatchStarted) {
                        dispatched.fetch_add(1, Ordering::SeqCst);
                    }
                },
            )));
    }
    // The new frozen partition shares catalog ownership but contains the new callback identity.
    // 新冻结分区共享目录所有权，但包含新的回调身份。
    let new = VulcanPluginConversationHooks {
        catalog: manager.snapshot(),
        registry: runtime
            .lock()
            .expect("fixture runtime or observation lock should remain healthy")
            .dispatcher_mut()
            .registry()
            .clone(),
    };
    // An agent-originated turn avoids unrelated asynchronous user-observer activity.
    // 代理发起回合避免无关的异步用户观察活动。
    let session = SessionId::new_v4();
    // Prepared dispatches retain their selected callbacks after the scope restores the base.
    // 准备的派发在作用域恢复基础状态后仍保留选定回调。
    let prepared_old = {
        // The guard mutates the one real turn map, not a cloned runtime.
        // 守卫修改唯一真实回合映射，而非克隆运行时。
        let mut scoped = ConversationHookGenerationGuard::new(
            runtime
                .lock()
                .expect("fixture runtime or observation lock should remain healthy"),
            Some(&old),
        )
        .expect("hook generation fixture operation should succeed");
        scoped.prepare_user_prompt_hook_requests(
            session,
            "root",
            None,
            temp.path().display().to_string(),
            PendingSessionPromptSource::AgentTask,
            SystemPromptHookPhase::SubsequentSession,
            "task",
            &Default::default(),
            false,
        );
        scoped.dispatcher_mut().clone()
    };
    assert!(
        runtime
            .lock()
            .expect("fixture runtime or observation lock should remain healthy")
            .has_active_turn(session)
    );
    // A second projection sees the original turn and the latest host registration.
    // 第二次投影观察原回合及最新宿主注册项。
    let prepared_new = {
        // The two projections share exactly the same mutex-protected state.
        // 两次投影共享完全相同的互斥锁状态。
        let mut scoped = ConversationHookGenerationGuard::new(
            runtime
                .lock()
                .expect("fixture runtime or observation lock should remain healthy"),
            Some(&new),
        )
        .expect("hook generation fixture operation should succeed");
        assert!(scoped.has_active_turn(session));
        scoped.dispatcher_mut().clone()
    };
    assert_eq!(lifecycle_events.load(Ordering::SeqCst), 0);
    dispatch(prepared_old).await;
    assert_eq!(
        *calls
            .lock()
            .expect("fixture runtime or observation lock should remain healthy"),
        ["host", "old"]
    );
    calls
        .lock()
        .expect("fixture runtime or observation lock should remain healthy")
        .clear();
    dispatch(prepared_new).await;
    assert_eq!(
        *calls
            .lock()
            .expect("fixture runtime or observation lock should remain healthy"),
        ["host", "new"]
    );
    calls
        .lock()
        .expect("fixture runtime or observation lock should remain healthy")
        .clear();
    // Base order proves the temporary projection never replaced permanent registration state.
    // 基础顺序证明临时投影从未替换持久注册状态。
    let base = runtime
        .lock()
        .expect("fixture runtime or observation lock should remain healthy")
        .dispatcher_mut()
        .clone();
    dispatch(base).await;
    assert_eq!(
        *calls
            .lock()
            .expect("fixture runtime or observation lock should remain healthy"),
        ["new", "host"]
    );
    assert_eq!(lifecycle_events.load(Ordering::SeqCst), 0);
    assert_eq!(dispatch_events.load(Ordering::SeqCst), 3);
}

/// Verifies failed live-policy validation and unwind both preserve the original dispatcher.
/// 验证实时策略校验失败及栈展开均保留原调度器。
#[test]
fn scoped_generation_failure_and_unwind_restore_base() {
    // A real empty catalog provides the frozen partition's ownership identity.
    // 真实空目录提供冻结分区的所有权身份。
    let temp = tempfile::tempdir().expect("create isolated generation fixture");
    // Shared base state has a policy that rejects the candidate fixture's transform.
    // 共享基础状态具有拒绝候选夹具转换的策略。
    let runtime = Mutex::new(ConversationHookRuntime::new());
    runtime
        .lock()
        .expect("hook generation fixture operation should succeed")
        .dispatcher_mut()
        .register_point(HookPointDescriptor::new(
            "policy-point",
            [HookMode::Observe],
            "policy fixture",
        ))
        .expect("hook generation fixture operation should succeed");
    // The deliberately incompatible registry models a host policy change after capture.
    // 故意不兼容的注册表模拟捕获后的宿主策略变更。
    let mut registry = HookRegistry::new();
    registry
        .register(
            HookRegistration::new(
                format!("{VULCAN_PLUGIN_HOOK_REGISTRATION_PREFIX}invalid"),
                "policy-point",
                HookMode::Transform,
            ),
            Arc::new(FnHookHandler::new(|_: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("hook generation fixture operation should succeed");
    // No callback is executed during either failing or successful scope preparation.
    // 失败或成功的作用域准备均不执行回调。
    let invalid = VulcanPluginConversationHooks {
        catalog: manager(temp.path()).snapshot(),
        registry,
    };
    assert!(
        ConversationHookGenerationGuard::new(
            runtime
                .lock()
                .expect("fixture runtime or observation lock should remain healthy"),
            Some(&invalid)
        )
        .is_err()
    );
    assert_eq!(
        runtime
            .lock()
            .expect("hook generation fixture operation should succeed")
            .dispatcher_mut()
            .registry()
            .count_for("policy-point"),
        0
    );
    // A valid empty partition can be installed and then restored during unwind.
    // 有效空分区可被安装，并在栈展开期间恢复。
    let empty = VulcanPluginConversationHooks {
        catalog: invalid.catalog(),
        registry: HookRegistry::new(),
    };
    // Catching the fixture panic lets the test inspect the poisoned mutex's retained value.
    // 捕获夹具恐慌使测试能够检查中毒互斥锁保留的值。
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Temporary point replacement must disappear with the scoped dispatcher.
        // 临时点位替换必须随作用域调度器消失。
        let mut scoped = ConversationHookGenerationGuard::new(
            runtime
                .lock()
                .expect("fixture runtime or observation lock should remain healthy"),
            Some(&empty),
        )
        .expect("hook generation fixture operation should succeed");
        scoped
            .dispatcher_mut()
            .register_point(HookPointDescriptor::new(
                "policy-point",
                [HookMode::Transform],
                "temporary",
            ))
            .expect("hook generation fixture operation should succeed");
        panic!("intentional scope unwind");
    }));
    assert!(unwind.is_err());
    // Production still treats mutex poisoning as an error; only this test inspects the retained state.
    // 生产仍将互斥锁中毒视为错误；仅本测试检查保留状态。
    let mut retained = runtime
        .lock()
        .err()
        .expect("intentional panic should poison the runtime mutex")
        .into_inner();
    assert_eq!(
        retained
            .dispatcher_mut()
            .registry()
            .point_descriptor("policy-point")
            .expect("hook generation fixture operation should succeed")
            .modes,
        std::collections::BTreeSet::from([HookMode::Observe])
    );
}

/// Checks that unrelated extension republishing retains the exact plugin catalog and handler owner.
/// 检查无关扩展重新发布保留精确插件目录及处理器所有者。
#[test]
fn extension_republish_preserves_frozen_plugin_routes_and_checks_revision() {
    // Empty production preparation still binds catalog ownership through the real constructor.
    // 空生产准备仍通过真实构造器绑定目录所有权。
    let temp = tempfile::tempdir().expect("create isolated generation fixture");
    // Metadata-only ownership avoids network or Lua side effects.
    // 仅元数据所有权避免网络或 Lua 副作用。
    let manager = manager(temp.path());
    // The catalog is compared by Arc identity, not only equal metadata.
    // 按 Arc 身份而非仅相等元数据比较目录。
    let catalog = manager.snapshot();
    // Host policy is read without copying active turn state.
    // 读取宿主策略，不复制活动回合状态。
    let mut runtime = ConversationHookRuntime::new();
    // Real preparation creates no callbacks for an empty catalog.
    // 真实准备不会为空目录创建回调。
    let hooks = Arc::new(
        VulcanPluginConversationHooks::prepare(
            runtime.dispatcher_mut(),
            manager,
            Arc::clone(&catalog),
        )
        .expect("hook generation fixture operation should succeed"),
    );
    // Unconfigured MCP access creates no network activity.
    // 未配置 MCP 访问面不会产生网络活动。
    let mcp = crate::mcp::configured_mcp_runtime_access(&Default::default());
    // The constructor derives catalog ownership from the hook partition itself.
    // 构造器从 Hook 分区自身派生目录所有权。
    let initial = ExtensionGeneration::new_with_plugin_conversation_hooks(
        7,
        Arc::clone(&hooks),
        mcp.clone(),
        None,
    );
    // A settings successor changes only the requested non-plugin resources.
    // 设置后继仅改变请求的非插件资源。
    let next = initial
        .republish(mcp.clone(), None)
        .expect("hook generation fixture operation should succeed");
    assert_eq!(next.revision(), 8);
    assert!(Arc::ptr_eq(
        &catalog,
        &next
            .plugin_catalog()
            .expect("hook generation fixture operation should succeed")
    ));
    assert!(std::ptr::eq(
        hooks.as_ref(),
        next.plugin_conversation_hooks()
            .expect("hook generation fixture operation should succeed")
    ));
    // Exhausted generations reject publication rather than reusing the same identity.
    // 耗尽的代次拒绝发布，不复用相同身份。
    let exhausted =
        ExtensionGeneration::new_with_plugin_conversation_hooks(u64::MAX, hooks, mcp.clone(), None);
    assert!(exhausted.republish(mcp, None).is_err());
    assert_eq!(exhausted.revision(), u64::MAX);
}
