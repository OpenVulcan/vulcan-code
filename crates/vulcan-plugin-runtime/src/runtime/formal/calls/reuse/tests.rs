//! Production invocation tests for hot reuse, private partitions and actual retirement.
//! 热复用、私有分区及实际退役的生产调用测试。

use super::*;
use crate::VulcanPluginInvocationRequest;
use crate::{VulcanPluginHostServices, VulcanPluginUiEventPublisher, VulcanPluginUiInvalidation};
use std::collections::BTreeSet;
use std::sync::Condvar;

mod budget;
mod prewarm;
mod recovery;

/// Two real native callbacks must overlap before either may return.
/// 两个真实原生回调必须重叠，任一回调才能返回。
#[derive(Default)]
struct ParallelEvents {
    /// Actual callback entries are recorded under one synchronization lock.
    /// 真实回调入场记录在同一个同步锁下。
    entered: Mutex<usize>,
    /// Callback workers wait for the second original VM, never a synthetic result.
    /// 回调工作者等待第二个原始 VM，绝不等待伪造结果。
    changed: Condvar,
}
impl VulcanPluginUiEventPublisher for ParallelEvents {
    /// Wait for both real callbacks from event; return an error if parallel execution never occurs.
    /// 等待来自 event 的两个真实回调；若始终没有并行执行则返回错误。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        if event.topic != "parallel" {
            return Err("Unexpected parallel test event.".to_owned());
        }
        // A bounded native wait fails deterministic serial execution without hanging the test.
        // 有界原生等待使串行执行确定失败，不挂起测试。
        let mut entered = self.entered.lock().map_err(|error| error.to_string())?;
        *entered += 1;
        self.changed.notify_all();
        let (entered, timeout) = self
            .changed
            .wait_timeout_while(entered, WAIT, |entered| *entered < 2)
            .map_err(|error| error.to_string())?;
        if timeout.timed_out() && *entered < 2 {
            return Err("Second reusable VM did not enter the native callback.".to_owned());
        }
        Ok(())
    }
}

/// Explicit parallel domains admit two distinct VMs while both original native callbacks are active.
/// 显式并行域在两个原始原生回调均活跃时接纳两个不同 VM。
#[tokio::test]
async fn hot_reuse_parallel_policy_runs_distinct_real_instances() {
    // Replace only this fixture's exact declaration before any Lua is admitted.
    // 在任何 Lua 入场前，仅替换此夹具精确声明。
    let fixture = Fixture::new(10_000, 100);
    let path = fixture.root.path().join("bundled/idle-plugin/plugin.yaml");
    let mut manifest: Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("manifest"))
            .expect("manifest mapping");
    manifest["permissions"] = json!(["config", "ui_host_context"]);
    let tool = manifest["lua_runtime"]["tools"]
        .as_array_mut()
        .expect("tools")
        .iter_mut()
        .find(|tool| tool["id"] == "idle_ping")
        .expect("ping");
    tool["execution"]["reusable"]["ordering"] = json!("parallel");
    tool["execution"]["reusable"]["max_running_calls"] = json!(2);
    tool["execution"]["reusable"]["max_resident_vms"] = json!(2);
    std::fs::write(&path, serde_yaml::to_string(&manifest).expect("encode")).expect("write");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("parallel catalog");
    // Native event delivery supplies the real overlap barrier.
    // 原生事件投递提供真实重叠屏障。
    let events = Arc::new(ParallelEvents::default());
    fixture
        .manager
        .install_host_services(VulcanPluginHostServices {
            storage: None,
            vldb: None,
            ui_events: Some(events.clone()),
        })
        .expect("native event service");
    let (first, second) = tokio::join!(
        fixture.invoke("owner", fixture.root.path().to_path_buf(), "parallel"),
        fixture.invoke("owner", fixture.root.path().to_path_buf(), "parallel")
    );
    assert_eq!(first.expect("first original VM")["count"], json!(1));
    assert_eq!(second.expect("second original VM")["count"], json!(1));
    assert_eq!(*events.entered.lock().expect("actual callback entries"), 2);
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("actual residency")
            .resident,
        2
    );
    fixture.close().await;
}

/// Concurrent first borrows publish one domain and the core serializes its actual execution.
/// 并发首次借用仅发布一个域，核心串行化其实际执行。
#[tokio::test]
async fn hot_reuse_concurrent_creation_has_one_domain_and_one_serial_vm() {
    // Eight requests fit the fixture's explicit active-call limit.
    // 八个请求符合夹具显式活动调用上限。
    let fixture = Fixture::new(10_000, 100);
    let mut tasks = Vec::new();
    for _ in 0..8 {
        // Each spawned caller shares the original manager and exact partition.
        // 每个派生调用方共享原管理器及精确分区。
        let manager = fixture.manager.clone();
        let workspace = fixture.root.path().to_path_buf();
        tasks.push(tokio::spawn(async move {
            manager
                .invoke(VulcanPluginInvocationRequest {
                    tool_id: "idle_ping".to_owned(),
                    arguments: json!({"mode":"ok"}),
                    owner_session_id: "owner".to_owned(),
                    workspace_root: workspace,
                })
                .await
                .expect("concurrent reusable call")
        }));
    }
    // Counter uniqueness proves that request state did not overlap inside the one serial VM.
    // 计数唯一性证明请求状态没有在单个串行 VM 内重叠。
    let mut counts = Vec::new();
    for task in tasks {
        counts.push(
            task.await.expect("caller task")["count"]
                .as_u64()
                .expect("module counter"),
        );
    }
    counts.sort_unstable();
    assert_eq!(counts, (1..=8).collect::<Vec<_>>());
    assert_eq!(
        fixture
            .manager
            .reusable_tool_domains("owner")
            .expect("domain query")
            .len(),
        1
    );
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("actual resources")
            .resident,
        1
    );
    fixture.close().await;
}

/// Reject ambiguous callback aliases, incomplete declarations and reuse on unsupported legacy/session entries.
/// 拒绝含混回调别名、不完整声明，以及旧版／会话入口上的复用。
#[test]
fn hot_reuse_manifest_is_strict_and_versioned() {
    // Use the real package loader so semantic failures cannot hide behind successful deserialization.
    // 使用真实包加载器，使语义失败不能被成功反序列化隐藏。
    let fixture = Fixture::new(10_000, 100);
    let root = fixture.root.path().join("bundled");
    let path = root.join("idle-plugin/plugin.yaml");
    let original: Value = serde_yaml::from_str(&std::fs::read_to_string(&path).expect("manifest"))
        .expect("manifest mapping");
    // The offline authoring fragment must accept the real declaration and reject malformed structure.
    // 离线编写片段必须接受真实声明，并拒绝畸形结构。
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../../../../docs/contracts/plugin-tool-reuse.schema.json"
    ))
    .expect("reuse authoring schema");
    let validator = jsonschema::validator_for(&schema).expect("valid offline reuse schema");
    let declared = &original["lua_runtime"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["id"] == "idle_ping")
        .expect("ping")["execution"]["reusable"];
    assert!(validator.is_valid(declared));
    for (name, replacement) in [
        ("null", Value::Null),
        ("partial", json!({"before_call":"before_call"})),
        ("unknown", {
            let mut reuse = original["lua_runtime"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .find(|tool| tool["id"] == "idle_ping")
                .expect("ping")["execution"]["reusable"]
                .clone();
            reuse["extra"] = json!(true);
            reuse
        }),
        ("alias", {
            let mut reuse = original["lua_runtime"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .find(|tool| tool["id"] == "idle_ping")
                .expect("ping")["execution"]["reusable"]
                .clone();
            reuse["after_call"] = json!("dispatch");
            reuse
        }),
        ("zero", {
            let mut reuse = original["lua_runtime"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .find(|tool| tool["id"] == "idle_ping")
                .expect("ping")["execution"]["reusable"]
                .clone();
            reuse["idle_ttl_ms"] = json!(0);
            reuse
        }),
    ] {
        if name != "alias" {
            assert!(!validator.is_valid(&replacement), "{name}");
        }
        let mut manifest = original.clone();
        let tool = manifest["lua_runtime"]["tools"]
            .as_array_mut()
            .expect("tools")
            .iter_mut()
            .find(|tool| tool["id"] == "idle_ping")
            .expect("ping");
        tool["execution"]["reusable"] = replacement;
        std::fs::write(&path, serde_yaml::to_string(&manifest).expect("encode")).expect("write");
        let snapshot =
            crate::load_vulcan_plugin_catalog(&root, None, &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(!snapshot.enabled_tools.contains_key("idle_ping"), "{name}");
        assert!(
            !snapshot.plugins["idle-plugin"].diagnostics.is_empty(),
            "{name}"
        );
    }
    for kind in ["legacy", "session"] {
        let mut manifest = original.clone();
        if kind == "legacy" {
            manifest["schema_version"] = json!(crate::VULCAN_PLUGIN_LEGACY_MANIFEST_SCHEMA_VERSION);
            manifest
                .as_object_mut()
                .expect("mapping")
                .remove("runtime_requirements");
        } else {
            let tool = manifest["lua_runtime"]["tools"]
                .as_array_mut()
                .expect("tools")
                .iter_mut()
                .find(|tool| tool["id"] == "idle_ping")
                .expect("ping");
            tool["execution"]["kind"] = json!("session_open");
        }
        std::fs::write(&path, serde_yaml::to_string(&manifest).expect("encode")).expect("write");
        let snapshot =
            crate::load_vulcan_plugin_catalog(&root, None, &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(
            snapshot.plugins["idle-plugin"]
                .diagnostics
                .iter()
                .any(|message| message.contains("version-two stateless")),
            "{kind}: {:?}",
            snapshot.plugins["idle-plugin"].diagnostics
        );
    }
}
use crate::runtime::tests::{test_manager, write_idle_session_package};

/// One bounded observation deadline for real native cleanup.
/// 真实原生清理的唯一有界观测截止时间。
const WAIT: Duration = Duration::from_secs(10);

/// Activation initialization must validate every declared private hygiene callback before exposing business.
/// 激活初始化必须在暴露业务前校验全部声明私有清理回调。
#[tokio::test]
async fn hot_reuse_probe_rejects_missing_private_hygiene_export() {
    // Remove the declared function from the real bundle while leaving its manifest declaration intact.
    // 从真实 bundle 移除声明函数，同时保持其清单声明不变。
    let fixture = Fixture::new(10_000, 100);
    let path = fixture
        .root
        .path()
        .join("bundled/idle-plugin/runtime/plugin.bundle.lua");
    let source = std::fs::read_to_string(&path).expect("real source");
    assert!(
        source.contains("function M.reuse_check()"),
        "fixture hygiene function changed"
    );
    std::fs::write(
        &path,
        source.replace("function M.reuse_check()", "function M.unlisted_check()"),
    )
    .expect("remove declared hygiene");
    fixture
        .manager
        .refresh_catalog(
            &BTreeSet::from(["idle-plugin".to_owned()]),
            &BTreeSet::new(),
        )
        .expect("disabled catalog");
    fixture
        .manager
        .begin_enabling("idle-plugin")
        .expect("activation preparation");
    let error = fixture
        .manager
        .probe_plugin("idle-plugin")
        .await
        .expect_err("missing hygiene must fail activation");
    assert!(error.to_string().contains("reuse_check"), "{error}");
    assert!(
        !fixture
            .manager
            .snapshot()
            .enabled_tools
            .contains_key("idle_ping")
    );
    fixture.close().await;
}

/// Own a real package and manager for all tests; no mock VM or scheduler is used.
/// 为全部测试持有真实包及管理器；不使用模拟 VM 或调度器。
struct Fixture {
    /// Temporary package and workspace files outlive the runtime.
    /// 临时包及工作区文件比运行时存活更久。
    root: tempfile::TempDir,
    /// Public production entrypoint under test.
    /// 被测公开生产入口。
    manager: VulcanPluginRuntimeManager,
}
impl Fixture {
    /// Build the real fixture under exact settings; ttl and uses remain explicit manifest limits.
    /// 在精确 settings 下构建真实夹具；ttl 和 uses 保持为显式清单上限。
    /// Returns one enabled manager without changing or clipping supplied host limits.
    /// 返回单个已启用管理器，不更改或裁剪提供的宿主上限。
    fn with_settings(ttl: u64, uses: u64, settings: VulcanPluginRuntimeSettings) -> Self {
        let root = tempfile::tempdir().expect("reuse fixture root");
        let bundled = root.path().join("bundled");
        write_idle_session_package(&bundled);
        let package = bundled.join("idle-plugin");
        let path = package.join("plugin.yaml");
        let mut manifest: Value =
            serde_yaml::from_str(&std::fs::read_to_string(&path).expect("read manifest"))
                .expect("manifest mapping");
        manifest["schema_version"] = json!(crate::VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION);
        manifest["runtime_requirements"] = json!({
            "protocol_version": luaskills::ffi_embedded::embedded_core_description().protocol_version,
            "backend":"in_process","core_capabilities":["capacity_groups_v1"],"capacities":[]
        });
        manifest["config"] =
            json!({"schema_file":"config.schema.json","defaults_file":"config.defaults.json"});
        manifest["permissions"] = json!(["config"]);
        manifest["lua_runtime"]["max_active_sessions"] = json!(8);
        // Select by stable identity; fixture entry ordering is not part of the contract.
        // 按稳定身份选择；夹具入口顺序不属于契约。
        let tool = manifest["lua_runtime"]["tools"]
            .as_array_mut()
            .expect("tools")
            .iter_mut()
            .find(|tool| tool["id"] == "idle_ping")
            .expect("ping declaration");
        tool["execution"]["timeout_ms"] = json!(5000);
        tool["execution"]["reusable"] = json!({
            "before_call":"before_call","after_call":"after_call","reuse_check":"reuse_check",
            "ordering":"key_serial","max_resident_vms":1,"max_running_calls":1,
            "idle_ttl_ms":ttl,"max_uses":uses
        });
        std::fs::write(
            &path,
            serde_yaml::to_string(&manifest).expect("encode manifest"),
        )
        .expect("write manifest");
        std::fs::write(package.join("config.schema.json"), r#"{"type":"object","required":["value"],"properties":{"value":{"type":"integer"}},"additionalProperties":false}"#).expect("configuration schema");
        std::fs::write(package.join("config.defaults.json"), r#"{"value":1}"#)
            .expect("configuration defaults");
        std::fs::write(
            package.join("runtime/plugin.bundle.lua"),
            r#"
-- All state is private to one original Lua module.
-- 全部状态属于单个原始 Lua 模块。
local M, count, active, shutdown_error, checked = {}, 0, nil, false, true
-- Prepare this exact envelope, rejecting leaked state from a previous invocation; return true.
-- 准备此精确信封，拒绝上次调用泄漏状态；返回 true。
function M.before_call(envelope)
    assert(active == nil)
    active = vulcan.json.decode(envelope.payload_json)
    return active.mode ~= 'before_error'
end
-- Dispatch the original envelope, returning a frame or a deliberate business/Lua failure.
-- 分发原信封，返回帧或刻意的业务／Lua 失败。
function M.dispatch(envelope)
    assert(active.request_id == vulcan.json.decode(envelope.payload_json).request_id)
    count = count + 1
    shutdown_error = active.mode == 'shutdown_error'
    if active.mode == 'parallel' then vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='parallel',revision=count}) end
    if active.mode == 'lua_error' then error('lua-marker') end
    if active.mode == 'business_error' then return '{"ok":false,"error":"business-marker"}' end
    return vulcan.json.encode({ok=true,result={count=count,request_id=active.request_id}})
end
-- Clear request state even for an exception; report whether cleanup succeeded.
-- 即使异常也清除请求状态；报告清理是否成功。
function M.after_call(envelope, succeeded)
    assert(active.request_id == vulcan.json.decode(envelope.payload_json).request_id)
    assert(succeeded == (active.mode ~= 'lua_error'))
    local ok = active.mode ~= 'after_error'
    checked = active.mode ~= 'check_error'
    active = nil
    return ok
end
-- Return the exact post-cleanup safety decision with no caller-controlled arguments.
-- 无调用方控制参数地返回精确清理后安全决定。
function M.reuse_check() return checked and active == nil end
-- Shutdown observes original module state and may deliberately fail to test retained evidence.
-- 关闭观测原模块状态，可刻意失败以测试保留证据。
function M.shutdown(envelope)
    assert(vulcan.json.decode(envelope.payload_json).op == 'shutdown')
    if shutdown_error then return '{"ok":false,"error":"shutdown-marker"}' end
    return '{"ok":true,"result":{}}'
end
return M
"#,
        )
        .expect("real reusable bundle");
        let manager = test_manager(root.path(), bundled);
        manager
            .initialize_formal_runtime(settings)
            .expect("formal owner");
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("enabled reusable package");
        assert!(
            manager.snapshot().enabled_tools.contains_key("idle_ping"),
            "{:?}",
            manager.snapshot().plugins["idle-plugin"].diagnostics
        );
        Self { root, manager }
    }

    /// Build a validated version-two package using ttl and uses as exact declared retirement limits.
    /// 使用 ttl 及 uses 作为精确声明退役上限，构建已校验第二版包。
    /// Returns an enabled manager with real core execution and configuration state.
    /// 返回具有真实核心执行及配置状态的已启用管理器。
    fn new(ttl: u64, uses: u64) -> Self {
        Self::with_settings(ttl, uses, VulcanPluginRuntimeSettings::default())
    }

    /// Execute the public stateless route for controller and mode using workspace; return its exact result.
    /// 使用 workspace 为 controller 及 mode 执行公开无状态路由；返回精确结果。
    async fn invoke(
        &self,
        controller: &str,
        workspace: PathBuf,
        mode: &str,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        self.manager
            .invoke(VulcanPluginInvocationRequest {
                tool_id: "idle_ping".to_owned(),
                arguments: json!({"mode":mode}),
                owner_session_id: controller.to_owned(),
                workspace_root: workspace,
            })
            .await
    }

    /// Return this fixture's exact native owner without creating any replacement.
    /// 返回此夹具精确原生所有者，不创建替代。
    fn owner(&self) -> Arc<ManagedFormalRuntime> {
        self.manager
            .formal_runtime_owner()
            .expect("query parent")
            .expect("attached parent")
    }

    /// Request parent shutdown and await actual physical closure, failing on retained ownership.
    /// 请求父级关闭并等待实际物理关闭，保留归属时失败。
    async fn close(&self) {
        self.manager
            .request_formal_runtime_close()
            .expect("request actual close");
        let deadline = Instant::now() + WAIT;
        while !self
            .manager
            .poll_formal_runtime_closed()
            .expect("poll actual close")
        {
            assert!(Instant::now() < deadline, "physical reuse closure expired");
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }
}

/// Calls reuse one original VM only inside their controller/workspace partition and always clear request state.
/// 调用仅在控制器／工作区分区内复用同一原 VM，并始终清理请求状态。
#[tokio::test]
async fn hot_reuse_preserves_exact_partition_and_request_hygiene() {
    let fixture = Fixture::new(10_000, 100);
    let workspace = fixture.root.path().to_path_buf();
    for count in 1..=3 {
        assert_eq!(
            fixture
                .invoke("owner", workspace.clone(), "ok")
                .await
                .expect("reused call")["count"],
            json!(count)
        );
    }
    assert!(
        fixture
            .manager
            .formal_calls("owner")
            .expect("finished calls")
            .is_empty()
    );
    let domains = fixture
        .manager
        .reusable_tool_domains("owner")
        .expect("domain query");
    assert_eq!(domains.len(), 1);
    assert_eq!(
        domains
            .iter()
            .find(|domain| domain.tool_id == "idle_ping")
            .expect("retained ping domain")
            .borrowers,
        0
    );
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("native usage")
            .resident,
        1
    );
    assert_eq!(
        fixture
            .invoke("other", workspace.clone(), "ok")
            .await
            .expect("other controller")["count"],
        json!(1)
    );
    let other_workspace = workspace.join("other");
    std::fs::create_dir(&other_workspace).expect("other workspace");
    assert_eq!(
        fixture
            .invoke("owner", other_workspace, "ok")
            .await
            .expect("other workspace")["count"],
        json!(1)
    );
    let report = fixture
        .manager
        .drain_owner_sessions("owner")
        .await
        .expect("owner drain");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(
        fixture
            .manager
            .reusable_tool_domains("owner")
            .expect("drained owner")
            .is_empty()
    );
    assert_eq!(
        fixture
            .invoke("other", workspace, "ok")
            .await
            .expect("other retained")["count"],
        json!(2)
    );
    fixture.close().await;
}

/// Application failures may reuse a cleaned VM; Lua and hygiene failures force a fresh instance.
/// 应用失败可复用已清理 VM；Lua 及清理失败强制使用新实例。
#[tokio::test]
async fn hot_reuse_retires_poisoned_instances_and_preserves_business_failure() {
    let fixture = Fixture::new(10_000, 100);
    let workspace = fixture.root.path().to_path_buf();
    assert!(
        fixture
            .invoke("owner", workspace.clone(), "business_error")
            .await
            .expect_err("business failure")
            .to_string()
            .contains("business-marker")
    );
    assert_eq!(
        fixture
            .invoke("owner", workspace.clone(), "ok")
            .await
            .expect("cleaned failed business")["count"],
        json!(2)
    );
    for mode in ["lua_error", "before_error", "after_error", "check_error"] {
        fixture
            .invoke("owner", workspace.clone(), mode)
            .await
            .expect_err("poisoned invocation");
        assert_eq!(
            fixture
                .invoke("owner", workspace.clone(), "ok")
                .await
                .expect("new VM after poison")["count"],
            json!(1),
            "{mode}"
        );
    }
    for call in fixture
        .manager
        .formal_calls("owner")
        .expect("retained failures")
    {
        assert!(call.retired);
        fixture
            .manager
            .forget_formal_call(&call.invocation_id, "owner")
            .expect("acknowledge business failure");
    }
    fixture.close().await;
}

/// Managed configuration updates fence cached state and max-use shutdown records do not exhaust history.
/// 受管配置更新隔离缓存状态，最大使用次数关闭记录不耗尽历史。
#[tokio::test]
async fn hot_reuse_configuration_and_retirement_keep_actual_ownership() {
    let fixture = Fixture::new(10_000, 2);
    let workspace = fixture.root.path().to_path_buf();
    for _ in 0..4 {
        assert_eq!(
            fixture
                .invoke("owner", workspace.clone(), "ok")
                .await
                .expect("fresh max-use VM")["count"],
            json!(1)
        );
        assert_eq!(
            fixture
                .invoke("owner", workspace.clone(), "ok")
                .await
                .expect("last max-use call")["count"],
            json!(2)
        );
    }
    fixture
        .manager
        .update_plugin_config("idle-plugin", json!({"value":2}))
        .expect("configuration commit");
    assert_eq!(
        fixture
            .invoke("owner", workspace, "ok")
            .await
            .expect("new configuration VM")["count"],
        json!(1)
    );
    let deadline = Instant::now() + WAIT;
    loop {
        fixture
            .manager
            .maintain_reusable_domains(&fixture.owner())
            .expect("observe independent finalization");
        if fixture
            .owner()
            .host
            .runtime()
            .plugin("idle-plugin")
            .expect("retained history")
            .retained_operations
            == 0
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "independent shutdown history did not drain"
        );
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    }
    fixture.close().await;
}

/// Idle shutdown failures keep their real identity until the original controller explicitly acknowledges them.
/// 空闲关闭失败保留真实身份，直到原控制器明确确认。
#[tokio::test]
async fn hot_reuse_idle_shutdown_failure_requires_exact_acknowledgement() {
    let fixture = Fixture::new(20, 100);
    fixture
        .invoke("owner", fixture.root.path().to_path_buf(), "shutdown_error")
        .await
        .expect("business succeeded");
    let deadline = Instant::now() + WAIT;
    let domain = loop {
        let domains = fixture
            .manager
            .reusable_tool_domains("owner")
            .expect("retained idle evidence");
        if let Some(domain) = domains
            .into_iter()
            .find(|domain| domain.pool_retired && !domain.finalization_failures.is_empty())
        {
            break domain;
        }
        assert!(Instant::now() < deadline, "idle finalization not retained");
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    };
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("native resources")
            .resident,
        0
    );
    assert!(domain.retained_input_bytes > 0);
    assert_eq!(
        fixture
            .owner()
            .reusable
            .lock()
            .expect("domain registry")
            .retained_input_bytes()
            .expect("retained failed routing"),
        domain.retained_input_bytes,
        "physical pool retirement cannot free unacknowledged routing evidence"
    );
    // A normal retry must not silently replace the failed original partition.
    // 普通重试不能静默替换失败的原分区。
    fixture
        .invoke("owner", fixture.root.path().to_path_buf(), "ok")
        .await
        .expect_err("unacknowledged shutdown fences replacement");
    for call in fixture
        .manager
        .formal_calls("owner")
        .expect("rejected retry evidence")
    {
        fixture
            .manager
            .forget_formal_call(&call.invocation_id, "owner")
            .expect("acknowledge rejected retry");
    }
    fixture
        .manager
        .forget_reusable_tool_domain(&domain.domain_id, "other")
        .expect_err("cross-controller acknowledgement denied");
    assert!(
        domain
            .finalization_failures
            .iter()
            .any(|failure| format!("{failure:?}").contains("shutdown-marker"))
    );
    fixture
        .manager
        .forget_reusable_tool_domain(&domain.domain_id, "owner")
        .expect("explicit acknowledgement");
    assert_eq!(
        fixture
            .owner()
            .reusable
            .lock()
            .expect("domain registry")
            .retained_input_bytes()
            .expect("acknowledged routing"),
        0
    );
    assert!(
        fixture
            .manager
            .reusable_tool_domains("owner")
            .expect("acknowledged")
            .is_empty()
    );
    fixture.close().await;
}
