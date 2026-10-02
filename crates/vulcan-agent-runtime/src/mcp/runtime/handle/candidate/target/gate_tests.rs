//! Real stdio preparation verifies gate admission, original process identity and actual resource retirement.
//! 真实标准输入输出准备验证业务门入场、原进程身份及实际资源退役。

use super::*;
use crate::mcp::{McpServerSource, McpServerTransportConfig};
use std::fs::OpenOptions;

/// Native process and package-resource observations share the existing readiness deadline.
/// 原生进程及插件包资源观察共用既有就绪期限。
const OBSERVE: Duration = MCP_PARTITION_READINESS_TIMEOUT;

/// Build one real declared fixture server; marker records starts and return config grants no business admission.
/// 构建单个真实已声明夹具服务器；marker 记录启动，返回配置不授予业务入场。
fn fixture_server(marker: &std::path::Path) -> McpServerConfig {
    McpServerConfig {
        source: McpServerSource::Plugin {
            plugin_id: String::from("target-gate"),
            generation: 7,
        },
        id: String::from("target-fixture"),
        name: String::from("Actual target gate"),
        enabled: true,
        transport: McpServerTransportConfig::Stdio {
            command: String::from("node"),
            args: vec![
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/owner-mcp.mjs")
                    .to_string_lossy()
                    .into_owned(),
            ],
            cwd: None,
            env: BTreeMap::from([(
                String::from("VULCAN_MCP_PROCESS_MARKER"),
                marker.to_string_lossy().into_owned(),
            )]),
            env_passthrough: Vec::new(),
            inherit_environment: true,
        },
    }
}

/// Ready preparation rejects binding and direct calls, then the same gate opens one original PID and retires its OS lease.
/// 就绪准备拒绝绑定及直接调用，随后相同业务门开放单个原 PID，并退役其操作系统租约。
/// This primitive gate test verifies no restart; durable combined completion is validated by separate participant tests.
/// 此底层业务门测试验证无重启；持久组合完成由独立参与者测试校验。
#[tokio::test]
async fn preparing_actual_target_keeps_same_process_and_resource_lease() {
    // Keep fixture files separate from immutable runtime inputs.
    // 将夹具文件与不可变运行时输入分离。
    let temp = tempfile::tempdir().expect("target gate fixture");
    // Startup marker is written only by the actual child.
    // 启动标记仅由实际子进程写入。
    let marker = temp.path().join("starts.txt");
    // Native shared file ownership witnesses real resource retirement.
    // 原生共享文件所有权见证真实资源退役。
    let lock_path = temp.path().join("resource.lock");
    // Actual ownership is transferred into the common transport scope.
    // 实际所有权移交至公共传输范围。
    let resource = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .expect("resource owner");
    resource
        .try_lock_shared()
        .expect("actual shared resource lease");
    // Independent native opener tests the OS lock rather than a diagnostic snapshot.
    // 独立原生打开者测试操作系统锁，而非诊断快照。
    let contender = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .expect("resource contender");
    // Original resources have one release observation, independent of runtime history.
    // 原资源具有单个释放观察，独立于运行时历史。
    let (resources, release) = McpTransportResources::new(resource);
    // Actual target startup begins closed and single-shot.
    // 实际目标启动初始关闭且仅尝试一次。
    let runtime = prepare_unproven_target_mcp_runtime(vec![fixture_server(&marker)], resources)
        .expect("prepare actual unproven target")
        .dispatch();
    runtime
        .wait_until_ready(OBSERVE)
        .await
        .expect("actual readiness");
    assert!(
        contender.try_lock().is_err(),
        "actual transport must retain native resource ownership"
    );
    assert!(
        runtime
            .available_tool_views_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    assert!(runtime.transport_for_server("target-fixture").is_none());
    // Same actual lease holds a preparation binding built from its real discovered view.
    // 相同实际租约持有从真实发现视图构建的准备绑定。
    let lease = Arc::new(McpRuntimePartitionLease::new(runtime.clone()));
    // Find by stable origin identity so fixture changes do not turn positions into authority.
    // 按稳定原身份查找，避免夹具变化将位置变成权威。
    let view = runtime
        .snapshot()
        .filtered_tool_views(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|view| view.origin_tool_id() == "target-fixture/read_page")
        .expect("read_page declaration may have changed");
    // This captured binding must remain closed before confirmation.
    // 确认前，此捕获绑定必须保持关闭。
    let binding = McpRuntimeToolBinding::new(
        view,
        Arc::clone(&lease),
        String::from("plugin:target-gate:7"),
    );
    // Exact tool args include the trusted fixture owner argument.
    // 精确工具参数包含受信夹具所有者参数。
    let args = serde_json::json!({"url":"https://target.test", "owner_session_id":"fixture-owner"});
    assert!(binding.invoke(args.clone()).await.is_err());
    assert!(
        runtime
            .call_tool_by_id("target-fixture/read_page", args.clone())
            .await
            .is_err()
    );
    // Copy the actual child PID only as observed test evidence, never as routing authority.
    // 仅复制实际子进程 PID 作为观察测试证据，绝不作为路由权威。
    let starts = std::fs::read_to_string(&marker).expect("real startup marker");
    assert_eq!(starts.lines().count(), 1);
    // Primitive test exercises the same write gate that the typed participant later confirms.
    // 底层测试操作类型化参与者随后确认的相同写门。
    *runtime
        .inner
        .business_gate
        .write()
        .expect("target business gate") = McpBusinessGate::Open;
    // Published views appear from the same runtime snapshot without refreshing the route generation.
    // 已发布视图从相同运行时快照出现，无需刷新路由代次。
    assert!(
        !runtime
            .available_tool_views_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    // The original binding succeeds without replacement or another child launch.
    // 原绑定无需替换或启动另一子进程便成功。
    let result = binding
        .invoke(args)
        .await
        .expect("same prepared binding opens");
    assert!(
        result.to_string().contains(starts.trim()),
        "business result must come from the original PID"
    );
    assert_eq!(
        std::fs::read_to_string(&marker).expect("start count"),
        starts
    );
    runtime.shutdown();
    assert!(binding.invoke(serde_json::json!({})).await.is_err());
    runtime
        .wait_until_stopped(OBSERVE)
        .await
        .expect("original protocol stopped");
    tokio::time::timeout(OBSERVE, release.wait())
        .await
        .expect("original native resource release deadline")
        .expect("original native resource release");
    contender
        .try_lock()
        .expect("actual retirement releases shared OS lock despite retained snapshots/binding");
}
