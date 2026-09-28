//! Real production routes share one immutable aggregate policy per plugin.
//! 真实生产路由按插件共享单个不可变聚合策略。

use super::*;
use crate::runtime::tests::{test_manager, write_hook_package, write_idle_session_package};
use crate::{VulcanPluginActionCaller, VulcanPluginActionRequest, VulcanPluginInvocationRequest};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::Duration;

mod capacities;

/// Bound evidence polling without substituting an arbitrary sleep for physical completion.
/// 限制证据轮询时长，不以任意休眠替代物理完成。
const WAIT: Duration = Duration::from_secs(5);
/// Poll actual cleanup state at this sole test observation interval.
/// 按此唯一测试观测间隔轮询实际清理状态。
const OBSERVATION_INTERVAL: Duration = Duration::from_millis(10);

/// Own two independently discovered packages and their actual production manager.
/// 拥有两个独立发现的包及其实际生产管理器。
struct Fixture {
    /// Keep package files alive until all physical runtime resources retire.
    /// 保持包文件存活，直至全部物理运行时资源退役。
    root: tempfile::TempDir,
    /// Manager under test; no synthetic pool is used for route coverage.
    /// 待测管理器；路由覆盖不使用合成池。
    manager: VulcanPluginRuntimeManager,
}

impl Fixture {
    /// Build both real packages using validated settings; return their retained owner.
    /// 使用已校验设置构建两个真实包；返回保留所有者。
    fn new(settings: VulcanPluginRuntimeSettings) -> Self {
        // Fixture files remain owned across asynchronous invocation and cleanup.
        // 夹具文件跨异步调用及清理保持所有权。
        let root = tempfile::tempdir().expect("aggregate fixture");
        // Discovery consumes the same bundled layout as production.
        // 发现过程消费与生产相同的内建布局。
        let bundled = root.path().join("bundled");
        write_hook_package(&bundled);
        write_idle_session_package(&bundled);
        // Attach one parent only after both package descriptors have been validated.
        // 两个包描述符校验后，才附加单个父级。
        let manager = test_manager(root.path(), bundled);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("discover packages");
        manager
            .initialize_formal_runtime(settings)
            .expect("attach configured parent");
        Self { root, manager }
    }

    /// Execute the given tool and arguments through the real public API; return its business result.
    /// 通过真实公开接口执行给定工具及参数；返回业务结果。
    async fn tool(&self, tool_id: &str, arguments: Value) -> Value {
        self.manager
            .invoke(VulcanPluginInvocationRequest {
                tool_id: tool_id.into(),
                arguments,
                owner_session_id: "aggregate-owner".into(),
                workspace_root: self.root.path().to_owned(),
            })
            .await
            .expect("real tool execution")
    }

    /// Construct a real Action request; reuse selects the declared persistent or transient route.
    /// 构造真实 Action 请求；reuse 选择已声明持久或临时路由。
    fn action(&self, reuse: bool) -> VulcanPluginActionRequest {
        VulcanPluginActionRequest {
            plugin_id: "hook-plugin".into(),
            action_id: "ui-action".into(),
            arguments: json!({}),
            caller: VulcanPluginActionCaller::PluginUi,
            workspace_root: self.root.path().to_owned(),
            reuse_enabled_runtime: reuse,
        }
    }

    /// Observe exact registered plugin ownership; return actual counters and policy provenance.
    /// 观测精确已注册插件所有权；返回实际计数及策略来源。
    fn status(&self, plugin_id: &str) -> VulcanPluginFormalPluginStatus {
        self.manager
            .formal_plugin_status(plugin_id)
            .expect("actual plugin snapshot")
            .expect("registered plugin")
    }

    /// Close the original parent and wait for its actual physical retirement; return only on completion.
    /// 关闭原父级并等待实际物理退役；仅在完成后返回。
    async fn close(&self) {
        self.manager
            .request_formal_runtime_close()
            .expect("request parent close");
        tokio::time::timeout(WAIT, async {
            while !self
                .manager
                .poll_formal_runtime_closed()
                .expect("physical closure")
            {
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
            }
        })
        .await
        .expect("original parent physically retired");
    }
}

/// Return a complete override limiting one plugin to one VM, one running call and one queued call.
/// 返回完整覆盖，将单个插件限制为一个 VM、一个运行调用及一个排队调用。
/// Other fields inherit once from the supplied settings before becoming an explicit whole override.
/// 其他字段从给定设置继承一次，然后成为显式整体覆盖。
fn one_vm_budget(settings: &VulcanPluginRuntimeSettings) -> EmbeddedPluginConfig {
    // Derive every unspecified fixture limit from the same authoritative parent.
    // 从同一权威父级派生全部未指定夹具上限。
    let (mut budget, _) = settings.plugin_budget("fixture-plugin");
    budget.max_resident_vms = 1;
    budget.max_running_calls = 1;
    budget.max_queued_calls = 1;
    budget
}

/// Hook and UI domains cannot each claim a separate copy of their common plugin budget.
/// Hook 与 UI 域不能各自占用一份共同插件预算。
/// Another plugin still runs against spare parent capacity after the limited plugin is saturated.
/// 受限插件饱和后，另一个插件仍可使用父级剩余容量执行。
#[tokio::test(flavor = "current_thread")]
async fn formal_plugin_budget_is_shared_across_domains_and_isolates_other_plugins() {
    // Leave parent headroom while restricting only the Hook/UI plugin.
    // 保留父级余量，同时仅限制 Hook/UI 插件。
    let mut settings = VulcanPluginRuntimeSettings::default();
    // Retain the exact expected plugin policy for subsequent provenance checks.
    // 保留精确预期插件策略，供后续来源检查。
    let budget = one_vm_budget(&settings);
    settings
        .plugin_budgets
        .insert("hook-plugin".into(), budget.clone());
    // Exercise real discovery, pool creation and module initialization.
    // 执行真实发现、池创建及模块初始化。
    let fixture = Fixture::new(settings.clone());
    assert!(
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("real Hook prewarm")
    );
    assert_eq!(fixture.status("hook-plugin").snapshot.resources.resident, 1);
    // A distinct UI domain must receive the core aggregate error instead of another full allowance.
    // 独立 UI 域必须收到核心聚合错误，不能获得另一份完整额度。
    let error = fixture
        .manager
        .invoke_action(fixture.action(true))
        .await
        .expect_err("aggregate VM limit");
    assert!(
        error.to_string().contains("plugin resident capacity"),
        "{error}"
    );
    assert_eq!(fixture.status("hook-plugin").snapshot.resources.resident, 1);
    // A different plugin retains admission; it does not borrow the saturated plugin's configuration.
    // 不同插件保留入场能力；它不会借用已饱和插件的配置。
    let result = fixture.tool("idle_ping", json!({})).await;
    assert_eq!(result["accepted"], true);
    // The snapshot identifies the exact parent and whole override that enforced this limit.
    // 快照标识执行此限制的精确父级及整体覆盖。
    let limited = fixture.status("hook-plugin");
    // Observe the independently admitted plugin's inherited policy.
    // 观测独立入场插件的继承策略。
    let inherited = fixture.status("idle-plugin");
    assert_eq!(limited.snapshot.config, budget);
    assert_eq!(
        limited.budget_source,
        VulcanPluginBudgetSource::HostOverride
    );
    assert_eq!(
        inherited.snapshot.config,
        settings.plugin_budget("idle-plugin").0
    );
    assert_eq!(inherited.budget_source, VulcanPluginBudgetSource::Parent);
    assert_eq!(limited.runtime_id, inherited.runtime_id);
    fixture.close().await;
}

/// Stateless, pinned-session, transient, persistent, lifecycle and probe routes resolve the same policy.
/// 无状态、固定会话、临时、持久、生命周期及探测路由解析同一策略。
#[tokio::test(flavor = "current_thread")]
async fn formal_plugin_budget_covers_all_admission_paths_without_policy_replacement() {
    // A queue smaller than the parent also verifies each domain derives its legal bound from this budget.
    // 小于父级的队列同时验证每个域从此预算派生合法边界。
    let mut settings = VulcanPluginRuntimeSettings::default();
    // Both explicit overrides use the same expected aggregate limit.
    // 两个显式覆盖使用同一预期聚合上限。
    let budget = one_vm_budget(&settings);
    settings
        .plugin_budgets
        .insert("hook-plugin".into(), budget.clone());
    settings
        .plugin_budgets
        .insert("idle-plugin".into(), budget.clone());
    // Both plugins now require the explicit override on every route.
    // 两个插件现在均要求每条路由采用显式覆盖。
    let fixture = Fixture::new(settings.clone());
    fixture.tool("idle_ping", json!({})).await;
    // Open and close one actual pinned session without waiting for its optional idle reaper.
    // 打开并关闭一个实际固定会话，不等待可选空闲回收器。
    let opened = fixture.tool("idle_open", json!({})).await;
    // The public response is the sole authority for this session identity.
    // 公开响应是此会话身份的唯一权威。
    let session = opened["session_id"]
        .as_str()
        .expect("opened session identity");
    assert_eq!(fixture.status("idle-plugin").snapshot.resources.resident, 1);
    fixture
        .tool("idle_close", json!({"session_id":session}))
        .await;
    for reuse in [false, true] {
        // State should start independently in each actual execution category.
        // 每种实际执行类别中的状态应独立开始。
        let result = fixture
            .manager
            .invoke_action(fixture.action(reuse))
            .await
            .expect("real Action");
        assert_eq!(result["count"], 100);
        assert_eq!(fixture.status("hook-plugin").snapshot.config, budget);
    }
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("close ordinary admission");
    // Drain the real UI instance before a one-VM lifecycle operation can be admitted.
    // 接纳单 VM 生命周期操作前，排空真实 UI 实例。
    let drained = fixture
        .manager
        .drain_plugin_sessions("hook-plugin")
        .await
        .expect("physical UI drain");
    assert!(drained.failures.is_empty(), "{:?}", drained.failures);
    // The declared lifecycle deliberately returns this business marker, proving its body ran.
    // 已声明生命周期刻意返回此业务标记，以证明其主体确实执行。
    let error = fixture
        .manager
        .invoke_lifecycle("hook-plugin", VulcanPluginLifecycleEvent::Disable)
        .await
        .expect_err("fixture lifecycle marker");
    assert!(
        error.to_string().contains("enable-rollback-marker"),
        "{error}"
    );
    fixture
        .manager
        .rollback_draining_to_enabling("hook-plugin")
        .expect("probe scope");
    fixture
        .manager
        .probe_plugin("hook-plugin")
        .await
        .expect("real module probe");
    for plugin_id in ["hook-plugin", "idle-plugin"] {
        // Every route retains the original immutable policy rather than restoring parent-sized limits.
        // 每条路由均保留原不可变策略，不恢复为父级大小的上限。
        let observed = fixture.status(plugin_id);
        assert_eq!(observed.snapshot.config, budget);
        assert_eq!(
            observed.budget_source,
            VulcanPluginBudgetSource::HostOverride
        );
    }
    // Changing a persisted file does not silently replace the settings frozen on the existing parent.
    // 修改持久文件不会静默替换既有父级冻结的设置。
    let path = fixture
        .root
        .path()
        .join("plugin_state/plugin-runtime-settings.json");
    std::fs::create_dir_all(path.parent().expect("state directory")).expect("state");
    std::fs::write(
        &path,
        serde_json::to_vec(&VulcanPluginRuntimeSettings::default()).expect("new file"),
    )
    .expect("external settings edit");
    assert_eq!(
        fixture
            .manager
            .formal_runtime_status()
            .expect("parent snapshot")
            .expect("attached")
            .settings,
        settings
    );
    fixture.close().await;
}

/// Observations cannot create owners, and inconsistent or permanently closed core owners cannot be replaced.
/// 观测不能创建所有者，不一致或永久关闭的核心所有者不能被替换。
#[tokio::test(flavor = "current_thread")]
async fn formal_plugin_budget_rejects_mismatch_and_closed_ownership_without_side_effects() {
    for closed in [false, true] {
        // Each rejection is isolated from earlier failed call ownership.
        // 每种拒绝均与先前失败调用所有权隔离。
        let settings = VulcanPluginRuntimeSettings::default();
        // Retain the isolated production owner until physical closure.
        // 保留隔离生产所有者直至物理关闭。
        let fixture = Fixture::new(settings.clone());
        assert!(
            fixture
                .manager
                .formal_plugin_status("idle-plugin")
                .expect("unregistered query")
                .is_none()
        );
        assert!(fixture.manager.formal_plugin_status("Invalid/Id").is_err());
        // Deliberately inject conflicting core ownership to exercise the host integrity guard.
        // 刻意注入冲突核心所有权，以验证宿主完整性防护。
        let owner = fixture
            .manager
            .formal_runtime_owner()
            .expect("owner")
            .expect("attached");
        assert_eq!(
            owner
                .host
                .runtime()
                .plugin("idle-plugin")
                .expect_err("query did not register")
                .code,
            EmbeddedErrorCode::NotFound
        );
        // A closed owner has matching policy; the other case has a deliberate immutable mismatch.
        // 已关闭所有者具有匹配策略；另一用例有意使用不可变不匹配策略。
        let budget = if closed {
            settings.plugin_budget("idle-plugin").0
        } else {
            one_vm_budget(&settings)
        };
        owner
            .host
            .runtime()
            .register_plugin("idle-plugin".into(), budget.clone())
            .expect("controlled core owner");
        if closed {
            owner
                .host
                .runtime()
                .close_plugin("idle-plugin")
                .expect("permanently closed");
            assert!(fixture.status("idle-plugin").snapshot.closing);
        } else {
            assert!(fixture.manager.formal_plugin_status("idle-plugin").is_err());
        }
        // Use a real production invocation to prove admission cannot silently replace the injected identity.
        // 使用真实生产调用证明入场不能静默替换注入身份。
        let error = fixture
            .manager
            .invoke(VulcanPluginInvocationRequest {
                tool_id: "idle_ping".into(),
                arguments: json!({}),
                owner_session_id: "aggregate-owner".into(),
                workspace_root: fixture.root.path().to_owned(),
            })
            .await
            .expect_err("invalid original ownership");
        assert!(
            error.to_string().contains(if closed {
                "permanently closed"
            } else {
                "differs from its frozen host configuration"
            }),
            "{error}"
        );
        assert_eq!(
            owner
                .host
                .runtime()
                .plugin("idle-plugin")
                .expect("original owner remains")
                .config,
            budget
        );
        fixture.close().await;
    }
}

/// Validation rejects an invalid configured plugin before attaching any formal runtime worker.
/// 校验在附加任何正式运行时工作线程前拒绝非法已配置插件。
#[test]
fn formal_plugin_budget_invalid_configuration_never_attaches_parent() {
    // No worker exists until explicit formal initialization succeeds.
    // 显式正式初始化成功前，不存在工作线程。
    let root = tempfile::tempdir().expect("invalid config fixture");
    // Construct the manager without attaching a formal parent.
    // 构造管理器，但不附加正式父级。
    let manager = test_manager(root.path(), root.path().join("bundled"));
    // A whole override outside the parent budget must fail before runtime construction.
    // 超出父级预算的整体覆盖必须在构造运行时前失败。
    let mut settings = VulcanPluginRuntimeSettings::default();
    // Mutate the exact override rather than the independently valid parent.
    // 修改精确覆盖，不修改独立有效的父级。
    let mut budget = one_vm_budget(&settings);
    budget.max_sessions = settings.parent.max_sessions + 1;
    settings.plugin_budgets.insert("idle-plugin".into(), budget);
    assert!(manager.initialize_formal_runtime(settings).is_err());
    assert!(
        manager
            .formal_runtime_status()
            .expect("unattached parent")
            .is_none()
    );
    assert!(
        manager
            .formal_plugin_status("idle-plugin")
            .expect("read only")
            .is_none()
    );
}
