//! Real SQLite ownership and native initialization cross the complete service router.
//! 真实 SQLite 归属及原生初始化穿过完整服务路由。

use super::*;
use crate::tests::test_support::TestHostRuntimeRoot;

impl Fixture {
    /// Publish an exact visible project for owner with a workspace distinct from session storage.
    /// 为 owner 发布精确可见项目，其工作区与会话存储分离。
    /// Returns the path ordinary plugin tools must receive without loading any session runtime.
    /// 返回普通插件工具必须收到的路径，不加载任何会话运行时。
    fn attach_workspace(&self, owner: uuid::Uuid) -> std::path::PathBuf {
        // Separate physical project files from both native runtime and session persistence.
        // 将物理项目文件与原生运行时及会话持久化分离。
        let path = self.root.path().join("execution-workspace");
        std::fs::create_dir_all(&path).expect("execution workspace");
        // Use the same canonical session fixture shape as the service, changing only project ownership.
        // 使用与服务相同的规范会话夹具形状，仅改变项目归属。
        let mut workspace = test_service_state_with_parent_session(owner)
            .workspace_snapshot
            .take()
            .expect("retained workspace fixture");
        workspace
            .sessions
            .iter_mut()
            .find(|session| session.id == owner)
            .expect("fixture owner identity changed")
            .project_id = Some("v1-test-project".into());
        workspace.task_projects.push(crate::TaskProjectSummary::new(
            "v1-test-project",
            "Prewarm project",
            path.to_string_lossy().as_ref(),
            false,
            0,
            None,
            None,
            Vec::new(),
        ));
        self.state.lock().expect("service state").workspace_snapshot = Some(workspace);
        path
    }

    /// Enable one actual reusable package; return its current lossless generation token.
    /// 启用单个真实复用包；返回其当前无损代次令牌。
    fn enable_prewarm(&self) -> String {
        // Real execution requires the repository's authoritative bridge, unlike metadata-only capacity tests.
        // 与仅元数据容量测试不同，真实执行需要仓库权威桥接。
        let bridge = self.root.path().join("bridge");
        std::fs::create_dir_all(bridge.join("runtime")).expect("bridge runtime");
        std::fs::write(bridge.join("dependencies.yaml"), "{}\n").expect("bridge dependencies");
        std::fs::write(
            bridge.join("runtime/bridge.lua"),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"
            )),
        )
        .expect("authoritative bridge source");
        // This installed fixture package remains the sole source of declarations and Lua.
        // 此已安装夹具包仍是声明及 Lua 的唯一来源。
        let package = self.root.path().join("system/control-plugin");
        std::fs::write(
            package.join("parameters.json"),
            r#"{"type":"object","properties":{}}"#,
        )
        .expect("tool schema");
        std::fs::write(
            package.join("runtime/plugin.lua"),
            r#"
-- Keep exports private to each real module instance.
-- 将导出保留在每个真实模块实例内。
local M, calls = {}, 0
-- Count business calls on this original VM and return the version-one result frame.
-- 在此原始 VM 上计数业务调用，并返回版本一结果帧。
function M.dispatch(_envelope)
    calls = calls + 1
    return vulcan.json.encode({ok=true,result={count=calls}})
end
-- Accept a later ordinary call envelope; return successful preparation.
-- 接受后续普通调用信封；返回准备成功。
function M.before_call(_envelope) return true end
-- Accept original envelope and result state; return successful hygiene.
-- 接受原信封及结果状态；返回清理成功。
function M.after_call(_envelope, _succeeded) return true end
-- Report that this private module has no retained request state.
-- 报告此私有模块没有保留请求状态。
function M.reuse_check() return true end
-- Close the original envelope without executing any business operation.
-- 关闭原信封，不执行任何业务操作。
function M.shutdown(_envelope) return '{"ok":true,"result":{}}' end
return M
"#,
        )
        .expect("real reusable module");
        std::fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 2
id: control-plugin
version: 1.0.0
default_enabled: true
display:
  name: Control fixture
  capability: Explicit native prewarm
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
runtime_requirements:
  protocol_version: 1
  backend: in_process
  core_capabilities: [capacity_groups_v1]
  capacities: []
lua_runtime:
  bundle: runtime/plugin.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 1
  tools:
    - id: control_ping
      group: control_tools
      description: Execute the explicit control fixture.
      parameters_file: parameters.json
      execution:
        kind: stateless
        operation: ping
        timeout_ms: 10000
        reusable:
          before_call: before_call
          after_call: after_call
          reuse_check: reuse_check
          ordering: key_serial
          max_resident_vms: 2
          max_running_calls: 1
          idle_ttl_ms: 60000
          max_uses: 100
"#,
        )
        .expect("reusable manifest");
        self.runtime
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("enable tool");
        // Use the exact enabled descriptor; changed fixture declarations must fail with catalog diagnostics.
        // 使用精确已启用描述符；夹具声明变化必须携带目录诊断失败。
        let snapshot = self.runtime.snapshot();
        snapshot
            .enabled_tools
            .get("control_ping")
            .unwrap_or_else(|| {
                panic!(
                    "control tool missing; fixture declaration may have changed: {:?}",
                    snapshot.plugins["control-plugin"].diagnostics
                )
            })
            .plugin_generation
            .to_string()
    }

    /// Poll only request's original receipt until actual retirement within one test deadline.
    /// 在单个测试截止时间内仅轮询 request 的原回执，直到真实退役。
    /// Returns retained successful or failed evidence without acknowledging it.
    /// 返回保留成功或失败证据，不确认回执。
    async fn wait_prewarm(&self, request: &Value) -> Value {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                // Never replace an unknown receipt with another preparation.
                // 绝不将未知回执替换为其他准备。
                let status = self
                    .rpc("service_plugin_prewarm_status", request.clone())
                    .await
                    .expect("original status");
                if status["retired"] == true {
                    return status;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("prewarm cleanup reached its test deadline")
    }
}

/// Indexed ownership, exact retries and original native evidence survive the full management chain.
/// 索引归属、精确重试及原生证据在完整管理链路中保持有效。
#[tokio::test]
async fn plugin_prewarm_rpc_uses_indexed_workspace_and_original_receipts() {
    // Retain both real runtime and indexed storage for the full request lifetime.
    // 在完整请求寿命内保留真实运行时及索引存储。
    let fixture = Fixture::new();
    // Observe the exact enabled declaration rather than inventing a generation.
    // 观测精确已启用声明，不构造代次。
    let generation = fixture.enable_prewarm();
    // The session UUID selects SQLite authority; no workspace path crosses this request.
    // 会话 UUID 选择 SQLite 权威；没有工作区路径跨越此请求。
    let owner = uuid::Uuid::new_v4();
    // Ordinary plugin execution must share this project workspace, not the storage root below.
    // 普通插件执行必须共享此项目工作区，而非下方存储根。
    let execution_workspace = fixture.attach_workspace(owner);
    // Physical storage is retained separately from navigation and plugin state.
    // 物理存储与导航及插件状态分开保留。
    let storage = TestHostRuntimeRoot::new(owner);
    // Attach the actual indexed controller without loading a conversation runtime.
    // 附加真实索引控制器，不加载对话运行时。
    let controller = storage.attached_host_runtime_handle().await;
    fixture.state.lock().expect("state").controller_runtime = Some(controller);
    // The one native parent is the mandatory request fence.
    // 唯一原生父级是必需请求屏障。
    let runtime = fixture
        .runtime
        .formal_runtime_status()
        .expect("prewarm fixture must provide the requested evidence")
        .expect("prewarm fixture must provide the requested evidence")
        .runtime_id;
    // Retain exact read identities for every subsequent catalog observation.
    // 为每次后续目录观测保留精确读取身份。
    let read = json!({"runtimeId": runtime, "ownerSessionId": owner});
    // The enabled manifest is projected without native allocation.
    // 投影已启用清单，不分配原生资源。
    let catalog = fixture
        .rpc("service_plugin_prewarm_targets", read.clone())
        .await
        .expect("prewarm fixture must provide the requested evidence");
    assert_eq!(catalog["targets"], json!([]));
    assert_eq!(
        catalog["tools"],
        json!([{
            "pluginId":"control-plugin", "toolId":"control_ping",
            "pluginGeneration":generation, "maxResidentVms":2
        }])
    );
    // Prepare the original request with no untrusted filesystem field.
    // 准备原始请求，不携带不可信文件系统字段。
    let preparation = json!({"request":{
        "runtimeId":runtime, "ownerSessionId":owner, "pluginId":"control-plugin",
        "toolId":"control_ping", "pluginGeneration":generation, "targetIdle":2
    }});
    // Inert registration reserves a receipt but no VM.
    // 未启动登记预留回执，但不预留 VM。
    let prepared = fixture
        .rpc("service_prepare_plugin_prewarm", preparation.clone())
        .await
        .expect("prewarm fixture must provide the requested evidence");
    assert_eq!(prepared["workspaceRoot"], json!(execution_workspace));
    assert_ne!(
        prepared["workspaceRoot"],
        json!(storage.v1_storage_locator().project_directory())
    );
    assert_eq!(prepared["pluginGeneration"], generation);
    assert_eq!(prepared["started"], false);
    assert_eq!(prepared["retired"], false);
    assert_eq!(prepared["operation"], Value::Null);
    assert_eq!(
        fixture
            .runtime
            .formal_runtime_status()
            .expect("prewarm fixture must provide the requested evidence")
            .expect("prewarm fixture must provide the requested evidence")
            .resources
            .resident,
        0
    );
    assert!(
        fixture
            .state
            .lock()
            .expect("prewarm fixture must provide the requested evidence")
            .session_runtime_registry
            .get(owner)
            .is_err()
    );
    // Every action uses the original invocation rather than tool-based lookup.
    // 每个动作使用原始调用，而非按工具查找。
    let original =
        json!({"runtimeId":runtime,"ownerSessionId":owner,"invocationId":prepared["invocationId"]});
    assert!(
        fixture
            .rpc("service_forget_plugin_prewarm", original.clone())
            .await
            .is_err()
    );
    {
        // Outer admission covers the deletion transaction before plugin drain or storage locking.
        // 外层准入在插件排空或存储加锁前覆盖删除事务。
        let _deletion = crate::project_conversation_services::begin_session_lifecycle_transition(
            &fixture.state,
            [owner],
            "prewarm lifecycle race",
        )
        .expect("reserve conflicting lifecycle admission");
        assert!(
            fixture
                .rpc("service_start_plugin_prewarm", original.clone())
                .await
                .is_err()
        );
        assert!(
            fixture
                .rpc("service_prepare_plugin_prewarm", preparation.clone())
                .await
                .is_err()
        );
        assert_eq!(
            fixture
                .rpc("service_plugin_prewarm_status", original.clone())
                .await
                .expect("prewarm fixture must provide the requested evidence"),
            prepared
        );
    }
    {
        // Change only the retained project's execution path to simulate a reviewed workspace switch.
        // 仅改变保留项目的执行路径，模拟经过审阅的工作区切换。
        let mut state = fixture.state.lock().expect("service state");
        state
            .workspace_snapshot
            .as_mut()
            .expect("workspace")
            .task_projects
            .iter_mut()
            .find(|project| project.id == "v1-test-project")
            .expect("project fixture changed")
            .path = fixture
            .root
            .path()
            .join("replacement-workspace")
            .to_string_lossy()
            .into_owned();
    }
    assert!(
        fixture
            .rpc("service_start_plugin_prewarm", original.clone())
            .await
            .is_err(),
        "old receipt cannot redirect initialization to a new workspace"
    );
    fixture.attach_workspace(owner);
    // Another controller cannot observe or cancel this target even with its invocation UUID.
    // 即使持有调用 UUID，其他控制器也不能观测或取消此目标。
    let mut foreign = original.clone();
    foreign["ownerSessionId"] = json!(uuid::Uuid::new_v4());
    for method in [
        "service_plugin_prewarm_status",
        "service_cancel_plugin_prewarm",
        "service_recover_plugin_prewarm_cleanup",
        "service_forget_plugin_prewarm",
    ] {
        assert!(
            fixture.rpc(method, foreign.clone()).await.is_err(),
            "{method}"
        );
    }
    foreign = original.clone();
    foreign["runtimeId"] = json!("another-runtime");
    assert!(
        fixture
            .rpc("service_cancel_plugin_prewarm", foreign)
            .await
            .is_err()
    );
    fixture
        .rpc("service_start_plugin_prewarm", original.clone())
        .await
        .expect("prewarm fixture must provide the requested evidence");
    fixture
        .rpc("service_start_plugin_prewarm", original.clone())
        .await
        .expect("prewarm fixture must provide the requested evidence");
    // Two actual initialized VMs establish readiness before the first business invocation.
    // 首次业务调用前，两个真实已初始化 VM 建立就绪证据。
    let completed = fixture.wait_prewarm(&original).await;
    assert_eq!(completed["error"], Value::Null, "{completed}");
    assert_eq!(completed["succeeded"], true);
    assert_eq!(completed["satisfied"]["ready"], 2);
    assert_eq!(completed["satisfied"]["physical"]["resident"], 2);
    assert_eq!(
        completed["completed"]
            .as_array()
            .expect("prewarm fixture must provide the requested evidence")
            .len(),
        1
    );
    assert_eq!(
        fixture
            .rpc("service_start_plugin_prewarm", original.clone())
            .await
            .expect("prewarm fixture must provide the requested evidence"),
        completed
    );
    assert_eq!(
        fixture
            .rpc("service_recover_plugin_prewarm_cleanup", original.clone())
            .await
            .expect("prewarm fixture must provide the requested evidence"),
        completed
    );
    assert_eq!(
        fixture
            .rpc("service_plugin_prewarm_targets", read.clone())
            .await
            .expect("prewarm fixture must provide the requested evidence")["targets"],
        json!([completed])
    );
    // Ordinary dispatch must borrow the prewarmed workspace domain, not allocate a storage-directory domain.
    // 普通派发必须借用已预热工作区域，不能分配存储目录域。
    let business = fixture
        .runtime
        .invoke(vulcan_plugin_runtime::VulcanPluginInvocationRequest {
            tool_id: "control_ping".into(),
            arguments: json!({}),
            owner_session_id: owner.to_string(),
            workspace_root: execution_workspace.clone(),
        })
        .await
        .expect("ordinary tool must reuse the prewarmed domain");
    assert_eq!(
        business["count"], 1,
        "prewarm must not execute the business export"
    );
    assert_eq!(
        fixture
            .runtime
            .reusable_tool_domains(&owner.to_string())
            .expect("original controller domains")
            .len(),
        1,
        "prewarm and business must share one workspace domain"
    );
    assert_eq!(
        fixture
            .runtime
            .formal_runtime_status()
            .expect("native usage")
            .expect("native owner")
            .resources
            .resident,
        2
    );
    // A stale or non-canonical generation cannot allocate another inert receipt.
    // 过期或非规范代次不能分配其他未启动回执。
    for invalid in ["0", "01", "18446744073709551616"] {
        let mut stale = preparation.clone();
        stale["request"]["pluginGeneration"] = json!(invalid);
        assert!(
            fixture
                .rpc("service_prepare_plugin_prewarm", stale)
                .await
                .is_err()
        );
    }
    assert_eq!(
        fixture
            .rpc("service_forget_plugin_prewarm", original.clone())
            .await
            .expect("prewarm fixture must provide the requested evidence"),
        Value::Null
    );
    assert!(
        fixture
            .rpc("service_start_plugin_prewarm", original.clone())
            .await
            .is_err()
    );
    assert_eq!(
        fixture
            .rpc("service_plugin_prewarm_targets", read)
            .await
            .expect("prewarm fixture must provide the requested evidence")["targets"],
        json!([])
    );
    fixture.close().await;
}

/// Missing current session authority blocks initialization but never blocks cancellation or acknowledgement.
/// 缺失当前会话权威阻止初始化，但绝不阻止取消或确认。
#[tokio::test]
async fn plugin_prewarm_rpc_preserves_cleanup_without_live_session_authority() {
    // Keep native and indexed fixtures independently removable from service navigation.
    // 保持原生及索引夹具可独立从服务导航中移除。
    let fixture = Fixture::new();
    // Use a real enabled generation for the original preparation.
    // 为原准备使用真实已启用代次。
    let generation = fixture.enable_prewarm();
    // Authoritative identity selects a dedicated real storage fixture.
    // 权威身份选择专用真实存储夹具。
    let owner = uuid::Uuid::new_v4();
    fixture.attach_workspace(owner);
    // Storage survives removal of the service's live controller reference.
    // 存储在移除服务实时控制器引用后仍存活。
    let storage = TestHostRuntimeRoot::new(owner);
    // Finish asynchronous attachment before acquiring the synchronous service lock.
    // 在取得同步服务锁前完成异步附加。
    let controller = storage.attached_host_runtime_handle().await;
    fixture
        .state
        .lock()
        .expect("prewarm fixture must provide the requested evidence")
        .controller_runtime = Some(controller);
    // Exact native parent must be retained on every action.
    // 每个动作必须保留精确原生父级。
    let runtime = fixture
        .runtime
        .formal_runtime_status()
        .expect("prewarm fixture must provide the requested evidence")
        .expect("prewarm fixture must provide the requested evidence")
        .runtime_id;
    // Prepare no new target for an unindexed UUID, even when navigation has a different valid session.
    // 即使导航包含其他有效会话，也不为未索引 UUID 准备新目标。
    let mut preparation = json!({"request":{
        "runtimeId":runtime, "ownerSessionId":uuid::Uuid::new_v4(), "pluginId":"control-plugin",
        "toolId":"control_ping", "pluginGeneration":generation, "targetIdle":1
    }});
    assert!(
        fixture
            .rpc("service_prepare_plugin_prewarm", preparation.clone())
            .await
            .is_err()
    );
    preparation["request"]["ownerSessionId"] = json!(owner);
    // Original inert receipt remains queryable after its session controller becomes unavailable.
    // 原未启动回执在会话控制器不可用后保持可查询。
    let prepared = fixture
        .rpc("service_prepare_plugin_prewarm", preparation)
        .await
        .expect("prewarm fixture must provide the requested evidence");
    // Exact original invocation is reused for failed startup and successful cancellation.
    // 失败启动及成功取消复用精确原始调用。
    let original =
        json!({"runtimeId":runtime,"ownerSessionId":owner,"invocationId":prepared["invocationId"]});
    fixture
        .state
        .lock()
        .expect("prewarm fixture must provide the requested evidence")
        .controller_runtime = None;
    assert!(
        fixture
            .rpc("service_start_plugin_prewarm", original.clone())
            .await
            .is_err()
    );
    assert_eq!(
        fixture
            .rpc("service_plugin_prewarm_status", original.clone())
            .await
            .expect("prewarm fixture must provide the requested evidence"),
        prepared
    );
    // Explicit cancellation retires only the inert input and retains an inspectable original error.
    // 明确取消仅退役未启动输入，并保留可检查原始错误。
    let cancelled = fixture
        .rpc("service_cancel_plugin_prewarm", original.clone())
        .await
        .expect("prewarm fixture must provide the requested evidence");
    assert_eq!(cancelled["retired"], true);
    assert_eq!(cancelled["started"], false);
    assert_eq!(cancelled["succeeded"], false);
    assert!(cancelled["error"].is_string());
    assert_eq!(
        fixture
            .runtime
            .formal_runtime_status()
            .expect("prewarm fixture must provide the requested evidence")
            .expect("prewarm fixture must provide the requested evidence")
            .resources
            .resident,
        0
    );
    fixture
        .rpc("service_forget_plugin_prewarm", original)
        .await
        .expect("prewarm fixture must provide the requested evidence");
    fixture.close().await;
}

/// Wire decoding rejects unknown paths, numeric generations and positional structs before state access.
/// 线协议解码在访问状态前拒绝未知路径、数字代次及位置结构体。
#[tokio::test]
async fn plugin_prewarm_rpc_rejects_ambiguous_wire_parameters() {
    // Missing service runtime proves rejection occurs at the decoding boundary.
    // 缺失服务运行时证明拒绝发生于解码边界。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        uuid::Uuid::new_v4(),
    )));
    // Valid carrier identities allow individual malformed fields to be tested independently.
    // 有效载体身份允许独立测试各畸形字段。
    let owner = uuid::Uuid::new_v4();
    // The sole valid request shape deliberately excludes workspace paths.
    // 唯一有效请求形状刻意排除工作区路径。
    let valid = json!({"runtimeId":"runtime","ownerSessionId":owner,"pluginId":"plugin","toolId":"tool","pluginGeneration":"1","targetIdle":1});
    // Exercise each independent malformed value without inventing a fallback spelling.
    // 验证每个独立畸形值，不构造备用拼写。
    let mut path = valid.clone();
    path["workspaceRoot"] = json!("D:/untrusted");
    // JSON numeric generations are rejected even when the numeric value looks small.
    // 即使数值看似很小，也拒绝 JSON 数字代次。
    let mut number = valid;
    number["pluginGeneration"] = json!(1);
    for (method, params) in [
        ("service_prepare_plugin_prewarm", json!({"request":path})),
        ("service_prepare_plugin_prewarm", json!({"request":number})),
        (
            "service_prepare_plugin_prewarm",
            json!({"request":["runtime",owner,"plugin","tool","1",1]}),
        ),
        ("service_plugin_prewarm_targets", json!(["runtime", owner])),
        (
            "service_plugin_prewarm_targets",
            json!({"runtimeId":"runtime","ownerSessionId":"unknown"}),
        ),
        (
            "service_start_plugin_prewarm",
            json!({"runtimeId":"runtime","ownerSessionId":owner,"invocationId":"original","retry":true}),
        ),
        (
            "service_cancel_plugin_prewarm",
            json!({"runtimeId":"runtime","ownerSessionId":owner}),
        ),
    ] {
        assert!(
            matches!(
                dispatch(state.clone(), method, &params).await,
                Err(WebuiServiceRpcError::InvalidParams(_))
            ),
            "{method}"
        );
    }
}
