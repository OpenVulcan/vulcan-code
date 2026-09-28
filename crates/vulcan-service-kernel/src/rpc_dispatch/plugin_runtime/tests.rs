//! Domain tests execute real native control and durable settings through the service boundary.
//! 领域测试通过服务边界执行真实原生控制及持久设置。

use super::*;
use crate::tests::test_support::{
    test_agent_runtime_bridge, test_service_state_with_parent_session,
};
use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
use std::sync::{Arc, Mutex};
use vulcan_plugin_runtime::{VulcanPluginCapacityPurpose, VulcanPluginRuntimeSettings};

/// Keep isolated disk roots, service state and the sole native owner alive until explicit shutdown.
/// 保持隔离磁盘根、服务状态及唯一原生所有者存活，直到明确关闭。
struct Fixture {
    /// Isolated files retain plugin and policy state without user-profile writes.
    /// 隔离文件保留插件及策略状态，不写用户配置。
    root: tempfile::TempDir,
    /// The real service state exposes the same manager used by direct assertions.
    /// 真实服务状态暴露直接断言使用的同一管理器。
    state: SharedServiceBridgeState,
    /// Exact manager owns one real LuaSkills parent.
    /// 精确管理器拥有单个真实 LuaSkills 父级。
    runtime: VulcanPluginRuntimeManager,
}

impl Fixture {
    /// Build a valid installed Lua package and attach native metadata; return the isolated service fixture.
    /// 构建有效已安装 Lua 包并附加原生元数据；返回隔离服务夹具。
    fn new() -> Self {
        // Every native engine path belongs to this retained temporary directory.
        // 每个原生引擎路径均属于此保留临时目录。
        let root = tempfile::tempdir().expect("control service fixture");
        // Canonical native paths avoid Windows extended-path syntax reaching Lua.
        // 规范原生路径避免 Windows 扩展路径语法进入 Lua。
        let native_root = vulcan_runtime_assets::native_host_path(
            &root.path().canonicalize().expect("canonical fixture root"),
        );
        // This engine is real, but metadata registration never executes the plugin module.
        // 此引擎为真实引擎，但元数据登记绝不执行插件模块。
        let engine = LuaEngine::new(LuaEngineOptions::new(
            LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            LuaRuntimeHostOptions::with_runtime_root(native_root),
        ))
        .expect("actual LuaSkills engine");
        // A valid package suffices for registration even before its first business call.
        // 即使首次业务调用尚未发生，有效包也足以用于登记。
        let package = root.path().join("system/control-plugin");
        std::fs::create_dir_all(package.join("runtime")).expect("package directory");
        std::fs::write(package.join("dependencies.yaml"), "{}\n").expect("dependencies");
        std::fs::write(
            package.join("runtime/plugin.lua"),
            "error('metadata must not run Lua')\n",
        )
        .expect("unexecuted module");
        std::fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: control-plugin
version: 1.0.0
default_enabled: false
display:
  name: Control fixture
  capability: Metadata-only management
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 1
"#,
        )
        .expect("manifest");
        // All writable roots remain inside this fixture.
        // 全部可写根保持在此夹具内。
        let runtime = VulcanPluginRuntimeManager::new(
            Arc::new(engine),
            root.path().join("system"),
            root.path().join("user"),
            root.path().join("bridge"),
            root.path().join("distributions"),
            root.path().join("state"),
            root.path().join("runtime"),
        );
        runtime
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("catalog");
        runtime
            .initialize_formal_runtime(VulcanPluginRuntimeSettings::default())
            .expect("parent");
        // Reuse established service fixtures without process-global configuration mutation.
        // 复用既有服务夹具，不修改进程全局配置。
        let mut bridge = test_agent_runtime_bridge();
        bridge.vulcan_plugin_runtime = runtime.clone();
        // This control path only reads the manager; it does not publish an extension generation.
        // 此控制路径仅读取管理器；不发布扩展代次。
        let mut state = test_service_state_with_parent_session(uuid::Uuid::new_v4());
        state.agent_runtime = Some(bridge);
        Self {
            root,
            state: Arc::new(Mutex::new(state)),
            runtime,
        }
    }

    /// Dispatch exact method and params through the top-level router; return successful JSON or a canonical error.
    /// 通过顶层路由分发精确 method 及 params；返回成功 JSON 或标准错误。
    async fn rpc(&self, method: &str, params: Value) -> Result<Value, WebuiServiceRpcError> {
        // Match the production HTTP dispatcher, which heap-owns its large cross-domain future.
        // 遵循生产 HTTP 分发器，在堆上持有大型跨领域 Future。
        Box::pin(crate::rpc_dispatch::dispatch_service_rpc(
            ServiceEventEmitter::new(Vec::new()),
            self.state.clone(),
            method.to_owned(),
            Some(params),
        ))
        .await
    }

    /// Close the real parent and await physical termination; return only when native ownership is released.
    /// 关闭真实父级并等待物理终止；仅在原生归属释放后返回。
    async fn close(&self) {
        self.runtime
            .request_formal_runtime_close()
            .expect("close parent");
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !self
                .runtime
                .poll_formal_runtime_closed()
                .expect("physical shutdown")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("shutdown within test deadline");
    }
}

/// Strict routing rejects malformed mutations before service access, including omitted reset fields.
/// 严格路由在访问服务前拒绝畸形变更，包括遗漏重置字段。
#[tokio::test]
async fn plugin_control_rpc_rejects_ambiguous_parameters() {
    // An uninitialized state proves parameter errors are returned before runtime lookup.
    // 未初始化状态证明参数错误在运行时查找前返回。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        uuid::Uuid::new_v4(),
    )));
    for (method, params) in [
        ("service_plugin_runtime_status", json!({"request": {}})),
        ("service_plugin_capacity_catalog", json!(null)),
        ("service_plugin_capacity_catalog", json!([])),
        (
            "service_plugin_capacity_policy_update_status",
            json!(["transaction"]),
        ),
        ("service_register_plugin_capacity", json!({"request": {}})),
        (
            "service_register_plugin_capacity",
            json!({"request": [
                uuid::Uuid::new_v4().to_string(), "runtime", "revision", "control-plugin", "tool_call"
            ]}),
        ),
        (
            "service_control_plugin_capacity",
            json!({"request": {"pluginId": "control-plugin", "action": "close"}}),
        ),
        (
            "service_plugin_capacity_registration_status",
            json!({"registrationId": "unknown"}),
        ),
        (
            "service_resume_plugin_capacity_policy_update",
            json!({"updateId": "id", "force": true}),
        ),
        ("service_cancel_plugin_capacity_policy_update", json!({})),
        (
            "service_forget_plugin_capacity_policy_update",
            json!({"updateId": null}),
        ),
        (
            "service_plugin_capacity_policy_update_status",
            json!({"updateId": 1}),
        ),
        (
            "service_begin_plugin_capacity_policy_update",
            json!({"request": {
                "updateId": uuid::Uuid::new_v4().to_string(), "runtimeId": "runtime",
                "capacityId": "capacity", "expectedRevision": "revision",
                "pluginId": "control-plugin", "purpose": "tool_call"
            }}),
        ),
    ] {
        assert!(
            matches!(
                dispatch(state.clone(), method, &params).await,
                Err(WebuiServiceRpcError::InvalidParams(_))
            ),
            "{method} must reject its malformed parameters"
        );
    }
    assert!(matches!(
        dispatch(state.clone(), "service_unknown_plugin_control", &json!({})).await,
        Ok(ServiceRpcDispatch::Unknown)
    ));
    assert!(matches!(
        dispatch(state, "service_plugin_runtime_status", &json!({})).await,
        Ok(ServiceRpcDispatch::Handled(Err(
            WebuiServiceRpcError::Service(_)
        )))
    ));
    // All declared management methods must be present in the top-level ownership contract.
    // 全部声明管理方法必须存在于顶层归属契约。
    let methods = crate::rpc_dispatch::service_rpc_method_names();
    assert!(RPC_METHODS.iter().all(|method| methods.contains(method)));
}

/// Dropping an admitted observer cannot discard its native mutation or redirect its retained registration receipt.
/// 丢弃已接纳观测者不能丢弃其原生变更或重定向保留登记回执。
#[tokio::test]
async fn plugin_control_rpc_observer_cancellation_preserves_admitted_work() {
    // The runtime remains shared with the service even after the observer task is aborted.
    // 即使观测任务被中断，运行时仍与服务共享。
    let fixture = Fixture::new();
    // Prepare one exact original request before transferring it to the blocking operation.
    // 将请求交给阻塞操作前准备单个精确原请求。
    let catalog = fixture
        .runtime
        .formal_capacities()
        .expect("catalog")
        .expect("attached");
    // The caller keeps its receipt identity independently of its awaited HTTP result.
    // 调用方独立于等待的 HTTP 结果保留回执身份。
    let request = VulcanPluginCapacityRegistrationRequest {
        registration_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: catalog.runtime_id,
        expected_registry_revision: catalog.registry_revision,
        plugin_id: "control-plugin".into(),
        purpose: VulcanPluginCapacityPurpose::ToolCall,
    };
    // Channels prove worker entry and actual completion, without arbitrary timing sleeps.
    // 通道证明工作者进入及实际完成，不使用任意定时休眠。
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    // Releasing this test-owned gate happens only after the observing task has been aborted.
    // 仅在观测任务被中断后释放此测试所有门控。
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    // Completion remains independently observed even though the original join handle is cancelled.
    // 即使原等待句柄被取消，仍独立观测完成。
    let (completed_tx, completed_rx) = tokio::sync::oneshot::channel();
    // Move the exact original request into the already admitted control operation.
    // 将精确原请求移入已接纳控制操作。
    let submission = request.clone();
    // A cloned service handle points at the same original manager.
    // 克隆服务句柄指向同一原管理器。
    let state = fixture.state.clone();
    // The task represents the HTTP observer; the inner blocking task owns accepted work.
    // 此任务表示 HTTP 观测者；内部阻塞任务拥有已接纳工作。
    let observer = tokio::spawn(async move {
        run_plugin_runtime_control(state, move |runtime| {
            entered_tx.send(()).expect("notify worker entry");
            release_rx.recv().expect("release admitted operation");
            // Publication follows cancellation of the observer, proving ownership is independent.
            // 发布发生在观测者取消之后，证明归属独立。
            let receipt = runtime.register_formal_capacity(submission);
            completed_tx.send(()).expect("notify actual completion");
            receipt
        })
        .await
    });
    entered_rx.await.expect("worker entered");
    observer.abort();
    assert!(
        observer
            .await
            .expect_err("observer cancelled")
            .is_cancelled()
    );
    release_tx.send(()).expect("release worker");
    completed_rx.await.expect("worker completed");
    assert!(
        !fixture
            .rpc(
                "service_plugin_capacity_registration_status",
                json!({
                    "runtimeId": request.runtime_id, "registrationId": request.registration_id
                })
            )
            .await
            .expect("independent receipt query")
            .is_null()
    );
    fixture.close().await;
}

/// A lost-response retry, durable policy update and stale close all traverse real RPC dispatch.
/// 丢失应答重试、持久策略更新及陈旧关闭均经过真实 RPC 分发。
#[tokio::test]
async fn plugin_control_rpc_preserves_registration_policy_and_exact_closure() {
    // This fixture has no pre-existing state file or executed plugin business.
    // 此夹具没有预先存在的状态文件或已执行插件业务。
    let fixture = Fixture::new();
    // Capture original wire identities without converting opaque tokens into numeric counters.
    // 捕获原线协议身份，不把不透明令牌转换为数值计数器。
    let catalog = fixture
        .rpc("service_plugin_capacity_catalog", json!({}))
        .await
        .expect("catalog");
    assert_eq!(catalog["capacities"], json!([]));
    // The same caller request is retained for explicit lost-response observation.
    // 保留同一调用方请求，用于明确丢失应答观测。
    let registration = json!({
        "registrationId": uuid::Uuid::new_v4().to_string(), "runtimeId": catalog["runtimeId"],
        "expectedRegistryRevision": catalog["registryRevision"], "pluginId": "control-plugin", "purpose": "tool_call"
    });
    // Native metadata registration must succeed despite a module that would throw if executed.
    // 即使模块一旦执行即抛错，原生元数据登记也必须成功。
    let original = fixture
        .rpc(
            "service_register_plugin_capacity",
            json!({"request": registration}),
        )
        .await
        .expect("register without Lua");
    assert_eq!(
        fixture
            .rpc(
                "service_register_plugin_capacity",
                json!({"request": registration})
            )
            .await
            .expect("same receipt"),
        original
    );
    assert_eq!(fixture.rpc("service_plugin_capacity_registration_status", json!({
        "runtimeId": registration["runtimeId"], "registrationId": registration["registrationId"]
    })).await.expect("lookup"), original);
    assert!(
        !fixture
            .root
            .path()
            .join("state/plugin-runtime-settings.json")
            .exists()
    );
    // Derive one valid policy change from the actual typed native status.
    // 从实际强类型原生状态派生单个有效策略变更。
    let status = fixture
        .runtime
        .formal_capacity_status("control-plugin", VulcanPluginCapacityPurpose::ToolCall)
        .expect("native status")
        .expect("registered");
    assert_eq!(status.snapshot.resources.resident, 0);
    assert_eq!(status.snapshot.retained_pools, 0);
    // Reduce one queue limit while preserving every other authoritative setting.
    // 减少单个队列上限，同时保留全部其他权威设置。
    let mut config = status.snapshot.config;
    config.max_queued_calls -= 1;
    // A durable caller identity survives HTTP result loss and later status reads.
    // 持久调用方身份在 HTTP 结果丢失及后续状态读取后保持存活。
    let update = json!({
        "updateId": uuid::Uuid::new_v4().to_string(), "runtimeId": original["runtimeId"],
        "capacityId": original["snapshot"]["capacity_id"], "expectedRevision": original["policyRevision"],
        "pluginId": "control-plugin", "purpose": "tool_call", "config": config
    });
    // Preserve the old close predecessor to verify a concurrent policy publication cannot be ignored.
    // 保留旧关闭前驱，验证并发策略发布不能被忽略。
    let mut control = json!({
        "runtimeId": original["runtimeId"], "capacityId": original["snapshot"]["capacity_id"],
        "expectedRevision": original["policyRevision"], "pluginId": "control-plugin",
        "purpose": "tool_call", "action": "close"
    });
    // Commit executes actual atomic settings persistence inside the blocking service task.
    // 提交在阻塞服务任务中执行实际原子设置持久化。
    let committed = fixture
        .rpc(
            "service_begin_plugin_capacity_policy_update",
            json!({"request": update}),
        )
        .await
        .expect("commit");
    assert_eq!(committed["phase"], "committed");
    assert!(
        fixture
            .root
            .path()
            .join("state/plugin-runtime-settings.json")
            .is_file()
    );
    assert_eq!(
        fixture
            .rpc(
                "service_plugin_capacity_policy_update_status",
                json!({"updateId": update["updateId"]})
            )
            .await
            .expect("receipt"),
        committed
    );
    assert_eq!(
        fixture
            .rpc(
                "service_resume_plugin_capacity_policy_update",
                json!({"updateId": update["updateId"]})
            )
            .await
            .expect("terminal resume"),
        committed
    );
    assert!(
        fixture
            .rpc(
                "service_cancel_plugin_capacity_policy_update",
                json!({"updateId": update["updateId"]})
            )
            .await
            .is_err()
    );
    assert!(
        fixture
            .rpc(
                "service_control_plugin_capacity",
                json!({"request": control})
            )
            .await
            .is_err()
    );
    // Runtime status exposes the same retained record for page-reload discovery.
    // 运行时状态暴露同一保留记录，供页面重新加载发现。
    let runtime_status = fixture
        .rpc("service_plugin_runtime_status", json!({}))
        .await
        .expect("status");
    assert_eq!(runtime_status["policyUpdate"], committed);
    assert_browser_wire_fixture(
        &original,
        &runtime_status,
        &fixture
            .rpc("service_plugin_capacity_catalog", json!({}))
            .await
            .expect("wire catalog"),
    );
    control["expectedRevision"] = committed["appliedRevision"].clone();
    assert_eq!(
        fixture
            .rpc(
                "service_control_plugin_capacity",
                json!({"request": control})
            )
            .await
            .expect("close"),
        true
    );
    control["action"] = json!("forget");
    assert_eq!(
        fixture
            .rpc(
                "service_control_plugin_capacity",
                json!({"request": control})
            )
            .await
            .expect("forget"),
        true
    );
    assert_eq!(
        fixture
            .rpc(
                "service_control_plugin_capacity",
                json!({"request": control})
            )
            .await
            .expect("absent"),
        false
    );
    fixture
        .rpc(
            "service_forget_plugin_capacity_policy_update",
            json!({"updateId": update["updateId"]}),
        )
        .await
        .expect("acknowledge transaction");
    assert_eq!(
        fixture
            .rpc(
                "service_plugin_capacity_policy_update_status",
                json!({"updateId": update["updateId"]})
            )
            .await
            .expect("forgotten"),
        Value::Null
    );
    fixture.close().await;
}

/// Compare actual RPC values with the browser fixture; an explicit test flag regenerates the portable file.
/// 将实际 RPC 值与浏览器夹具比较；明确测试标志重新生成可移植文件。
/// initial, runtime and catalog contain real serialized results; only opaque per-run identities are normalized.
/// initial、runtime 及 catalog 包含真实序列化结果；仅规范化每轮不透明身份。
fn assert_browser_wire_fixture(initial: &Value, runtime: &Value, catalog: &Value) {
    // Exact observed tokens map to portable examples without changing any field name, number or optionality.
    // 精确观测令牌映射为可移植示例，不改变任何字段名、数值或可选性。
    let tokens = [
        (
            initial["runtimeId"].as_str().expect("runtime"),
            "runtime-fixture",
        ),
        (
            initial["snapshot"]["capacity_id"]
                .as_str()
                .expect("capacity"),
            "capacity-fixture",
        ),
        (
            initial["policyRevision"].as_str().expect("before"),
            "policy-before",
        ),
        (
            runtime["policyUpdate"]["appliedRevision"]
                .as_str()
                .expect("after"),
            "policy-after",
        ),
        (
            initial["managementRequest"]["registrationId"]
                .as_str()
                .expect("registration"),
            "00000000-0000-4000-8000-000000000001",
        ),
        (
            runtime["policyUpdate"]["request"]["updateId"]
                .as_str()
                .expect("update"),
            "00000000-0000-4000-8000-000000000002",
        ),
        (
            initial["managementRequest"]["expectedRegistryRevision"]
                .as_str()
                .expect("mapping before"),
            "registry-before",
        ),
        (
            catalog["registryRevision"].as_str().expect("mapping after"),
            "registry-current",
        ),
    ];
    // Compare full JSON, including fields not currently rendered by the browser projection.
    // 比较完整 JSON，包含浏览器投影当前未渲染的字段。
    let mut actual = json!({"initialCapacity": initial, "runtime": runtime, "catalog": catalog});
    normalize_wire_tokens(&mut actual, &tokens);
    // The path is repository-relative and never embeds a developer machine root.
    // 此路径相对仓库，绝不嵌入开发者机器根。
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/vulcan-code-web/src/service/__fixtures__/plugin-runtime-wire.json");
    if std::env::var("VULCAN_UPDATE_PLUGIN_WIRE_FIXTURE").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
        std::fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&actual).expect("wire JSON")
            ),
        )
        .expect("explicit fixture regeneration");
    }
    // A stale or missing artifact fails with the exact explicit regeneration entrypoint.
    // 陈旧或缺失产物失败时给出精确明确重新生成入口。
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect(
        "browser wire fixture; regenerate this test with VULCAN_UPDATE_PLUGIN_WIRE_FIXTURE=1",
    ))
    .expect("browser wire fixture JSON");
    assert_eq!(
        actual, expected,
        "RPC contract changed; inspect the browser decoder before explicitly regenerating its fixture"
    );
}

/// Replace only exact string tokens in value using observed replacements; leave all structural and numeric evidence intact.
/// 使用已观测 replacements 仅替换 value 内精确字符串令牌；保持全部结构及数值证据完整。
fn normalize_wire_tokens(value: &mut Value, replacements: &[(&str, &str)]) {
    match value {
        Value::String(text) => {
            if let Some((_, replacement)) =
                replacements.iter().find(|(original, _)| *original == text)
            {
                *text = (*replacement).to_owned();
            }
        }
        Value::Object(fields) => {
            for field in fields.values_mut() {
                normalize_wire_tokens(field, replacements);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_wire_tokens(item, replacements);
            }
        }
        _ => {}
    }
}
