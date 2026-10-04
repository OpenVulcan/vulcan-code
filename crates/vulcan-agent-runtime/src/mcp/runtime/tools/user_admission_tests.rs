//! Real user replacements stay in the original shutdown lineage while later execution owners remain alive.
//! 真实用户替换在后续执行所有者继续存活时仍属于原关闭谱系。

use super::*;
use crate::mcp::runtime::handle::{
    MCP_PARTITION_READINESS_TIMEOUT, bootstrap_mcp_runtime_access,
    replace_user_mcp_runtime_partition,
};
use std::path::Path;
use std::time::Duration;

/// Native readiness and physical release share the existing runtime observation budget.
/// 原生就绪及物理释放共享既有运行时观察预算。
const OBSERVE: Duration = MCP_PARTITION_READINESS_TIMEOUT;

/// Only actual fixture startup markers are polled at this interval.
/// 仅按此间隔轮询实际夹具启动标记。
const MARKER_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Resolve the real Node executable and prepare one explicit user server without spawning it.
/// 解析真实 Node 可执行文件并准备单个明确用户服务器，不启动它。
/// root fixes cwd, marker records actual child PIDs and stalled delays genuine initialization.
/// root 固定 cwd，marker 记录实际子进程 PID，stalled 延迟真正初始化。
/// Return a complete user declaration with explicit environment and no plugin source certification.
/// 返回具有明确环境且不带插件来源认证的完整用户声明。
fn user_server(root: &Path, marker: &Path, stalled: bool) -> McpServerConfig {
    // Production dependency discovery supplies one executable path, not a candidate command fallback.
    // 生产依赖发现提供单个可执行路径，不使用候选命令回退。
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .expect("actual Node dependency");
    assert!(node.status.success(), "actual Node discovery failed");
    // UTF-8 stdout contains the exact discovered executable path.
    // UTF-8 标准输出包含精确已发现可执行路径。
    let command = String::from_utf8(node.stdout).expect("actual Node path encoding");
    // The declared fixture is the same actual protocol producer used by native owner tests.
    // 已声明夹具为原生所有者测试使用的同一实际协议生产者。
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/owner-mcp.mjs");
    // Explicit original values reach the real child without ambient marker inheritance.
    // 明确原值进入真实子进程，不继承环境标记。
    let mut env = BTreeMap::from([(
        String::from("VULCAN_MCP_PROCESS_MARKER"),
        marker.to_string_lossy().into_owned(),
    )]);
    // Windows Node needs this exact system directory for crypto initialization under an explicit environment.
    // Windows Node 在明确环境下需要此精确系统目录完成加密初始化。
    #[cfg(windows)]
    env.insert(
        String::from("SystemRoot"),
        std::env::var("SystemRoot").expect("actual Windows system directory"),
    );
    if stalled {
        env.insert(
            String::from("VULCAN_MCP_STALL_INITIALIZE"),
            String::from("1"),
        );
    }
    McpServerConfig {
        source: McpServerSource::UserConfigured,
        id: String::from("actual-user"),
        name: String::from("Actual user original"),
        enabled: true,
        transport: McpServerTransportConfig::Stdio {
            command: command.trim().to_owned(),
            args: vec![script.to_string_lossy().into_owned()],
            cwd: Some(root.to_string_lossy().into_owned()),
            env,
            env_passthrough: Vec::new(),
            inherit_environment: false,
        },
    }
}

/// Await marker's genuine native creation; return no synthetic startup evidence.
/// 等待 marker 的真正原生创建；不返回合成启动证据。
/// The original producer alone writes this file, and the existing observation budget bounds absence.
/// 仅原生产者写入此文件，既有观察预算限制缺失等待。
async fn wait_for_start(marker: &Path) {
    tokio::time::timeout(OBSERVE, async {
        while !marker.is_file() {
            tokio::time::sleep(MARKER_POLL_INTERVAL).await;
        }
    })
    .await
    .expect("actual user child startup marker");
}

/// Original access shutdown retires both actual generations despite retained latest access and real bindings.
/// 即使保留最新访问及真实绑定，原访问关闭仍退役两个实际代次。
/// No inputs or result; native release and rejected business prove shared cleanup without dropped execution owners.
/// 无输入或结果；原生释放及业务拒绝证明共享清理，不依靠执行所有者销毁。
#[tokio::test]
async fn user_replacement_original_shutdown_closes_retained_latest_actual_process() {
    // Independent literal paths bound every actual native child and startup record.
    // 独立字面路径限定每个实际原生子进程及启动记录。
    let temp = tempfile::tempdir().expect("actual user replacement fixture");
    // Same declared file records both genuine starts without labels synthesized by the host.
    // 相同已声明文件记录两次真正启动，宿主不合成标签。
    let marker = temp.path().join("starts.txt");
    // Initial bootstrap owns the first actual user generation.
    // 首次引导拥有第一个实际用户代次。
    let original = bootstrap_mcp_runtime_access(vec![user_server(temp.path(), &marker, false)])
        .await
        .expect("initial actual user access");
    // A non-owning original control supplies real readiness and physical release observations.
    // 非拥有原控制提供真实就绪及物理释放观察。
    let initial = original
        .live_partitions
        .get("user")
        .expect("original user partition")
        .runtime
        .clone();
    // Diagnostics belong to this explicitly secret-free fixture and failures still clean its original native work.
    // 诊断属于此明确无秘密夹具，失败仍清理其原原生工作。
    let ready = initial.wait_until_ready(OBSERVE).await;
    if let Err(error) = ready {
        // Capture actual lifecycle errors before shutdown changes the final observable status.
        // 在关闭改变最终可观察状态之前捕获实际生命周期错误。
        let states = initial.server_states();
        original.shutdown();
        // Cleanup result is distinct from the failed readiness result and never certifies initial success.
        // 清理结果独立于失败就绪结果，绝不证明首次成功。
        let cleanup = initial
            .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
            .await;
        panic!(
            "first actual user readiness failed: {error}; actual states={states:?}; cleanup={cleanup:?}"
        );
    }
    // Retain a genuine captured binding through shutdown, excluding last-lease destruction as the cause.
    // 跨关闭保留真正捕获绑定，排除最后租约析构作为关闭原因。
    let initial_binding = original
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|binding| binding.origin_tool_id() == "actual-user/read_page")
        .expect("initial actual user tool declaration may have changed");
    // The production replacement shares the original access lineage while creating exactly one new process.
    // 生产替换共享原访问谱系，同时恰好创建一个新进程。
    let latest = replace_user_mcp_runtime_partition(
        &original,
        vec![user_server(temp.path(), &marker, false)],
    )
    .await
    .expect("actual user replacement");
    // Observe only the actual new runtime already selected by its published access.
    // 仅观察已发布访问已选定的实际新运行时。
    let replacement = latest
        .live_partitions
        .get("user")
        .expect("new user partition")
        .runtime
        .clone();
    replacement
        .wait_until_ready(OBSERVE)
        .await
        .expect("new actual user ready");
    // Latest binding deliberately remains a strong execution owner until after release evidence.
    // 最新绑定有意保持强执行所有者，直至释放证据之后。
    let latest_binding = latest
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|binding| binding.origin_tool_id() == "actual-user/read_page")
        .expect("latest actual user tool declaration may have changed");
    // Genuine records establish both child starts before original cleanup is requested.
    // 真正记录在请求原清理之前确立两次子进程启动。
    let starts = std::fs::read_to_string(&marker).expect("actual user starts");
    assert_eq!(
        starts.lines().count(),
        2,
        "expected original and replacement start counts may have changed"
    );
    assert!(original.wait_user_mcp_drain(OBSERVE).await.is_err());
    original.shutdown();
    initial
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("original actual user resources retired");
    // Preserve a failing release result while explicitly cleaning the real child for a safe reproduction failure.
    // 保留失败释放结果，同时明确清理真实子进程，使复现失败仍安全收尾。
    let retired = replacement
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await;
    if retired.is_err() {
        latest.shutdown_user_partition();
        replacement
            .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
            .await
            .expect("explicit cleanup after missing original retirement");
    }
    assert!(
        retired.is_ok(),
        "original access missed its actual replacement: {retired:?}"
    );
    original
        .wait_user_mcp_drain(OBSERVE)
        .await
        .expect("every retained actual user original physically retired");
    assert!(
        initial_binding
            .invoke(
                serde_json::json!({"url": "https://initial.invalid", "owner_session_id": "initial"})
            )
            .await
            .is_err()
    );
    assert!(
        latest_binding
            .invoke(
                serde_json::json!({"url": "https://latest.invalid", "owner_session_id": "latest"})
            )
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(&marker).expect("no user replay"),
        starts
    );
}

/// Global closure rejects a subsequent real replacement before native dispatch.
/// 整体关闭在原生派发之前拒绝后续真实替换。
/// No inputs or result; an absent actual child marker proves this declared replacement never starts.
/// 无输入或结果；实际子进程标记缺失证明此已声明替换从未启动。
#[tokio::test]
async fn user_replacement_rejects_original_closed_admission_before_dispatch() {
    // The isolated original has no native work before its lifecycle is closed.
    // 隔离原实例在其生命周期关闭之前没有原生工作。
    let temp = tempfile::tempdir().expect("closed user admission fixture");
    // Only the genuine child can create this marker.
    // 仅真正子进程可以创建此标记。
    let marker = temp.path().join("starts.txt");
    // Actual empty bootstrap provides the original shared admission, not a fabricated token.
    // 实际空引导提供原共享入场，不使用伪造令牌。
    let original = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("actual empty access");
    original.shutdown();
    original
        .wait_user_mcp_drain(OBSERVE)
        .await
        .expect("empty closed user lineage");
    // Keep the real result while cleaning any erroneous returned producer explicitly.
    // 保留真实结果，同时明确清理任何错误返回的生产者。
    let replacement = replace_user_mcp_runtime_partition(
        &original,
        vec![user_server(temp.path(), &marker, false)],
    )
    .await;
    if let Ok(access) = &replacement {
        access.shutdown_user_partition();
        access
            .live_partitions
            .get("user")
            .expect("erroneously admitted original")
            .runtime
            .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
            .await
            .expect("explicit cleanup of incorrectly admitted child");
    }
    assert!(
        replacement.is_err(),
        "closed original must reject user replacement"
    );
    assert!(
        !marker.exists(),
        "closed original must reject before an actual child starts"
    );
}

/// A genuinely started but still connecting user original remains drainable through the first access.
/// 真正已启动但仍连接中的用户原实例保持可经首次访问排空。
/// No inputs or result; actual startup and physical cleanup prove the observation precedes readiness and publication.
/// 无输入或结果；实际启动及物理清理证明观察先于就绪及发布。
#[tokio::test]
async fn user_replacement_connecting_original_survives_until_original_cleanup() {
    // Native initialization parks only this declared isolated child.
    // 原生初始化仅停驻此已声明隔离子进程。
    let temp = tempfile::tempdir().expect("connecting user replacement fixture");
    // The original child alone records its actual PID here.
    // 仅原子进程在此记录其实际 PID。
    let marker = temp.path().join("starts.txt");
    // Original observation lineage exists before replacement effects.
    // 原观察谱系在替换副作用之前存在。
    let original = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("actual empty lineage");
    // Returned access remains alive, so explicit original shutdown must provide retirement.
    // 返回访问保持存活，因此明确原关闭必须提供退役。
    let latest = replace_user_mcp_runtime_partition(
        &original,
        vec![user_server(temp.path(), &marker, true)],
    )
    .await
    .expect("actual parked user replacement");
    // This control is the actual registered runtime, not readiness-derived synthetic evidence.
    // 此控制为实际已登记运行时，不是从就绪派生的合成证据。
    let pending = latest
        .live_partitions
        .get("user")
        .expect("actual pending user")
        .runtime
        .clone();
    wait_for_start(&marker).await;
    assert!(
        pending
            .server_states()
            .iter()
            .any(|server| server.status == McpRuntimeServerStatus::Connecting)
    );
    original.shutdown();
    // Preserve failure evidence while explicitly cleaning this isolated child before reporting a regression.
    // 保留失败证据，同时在报告回归前明确清理此隔离子进程。
    let retired = pending
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await;
    if retired.is_err() {
        latest.shutdown_user_partition();
        pending
            .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
            .await
            .expect("explicit cleanup after a missed connecting original");
    }
    assert!(
        retired.is_ok(),
        "original access missed connecting user: {retired:?}"
    );
    original
        .wait_user_mcp_drain(OBSERVE)
        .await
        .expect("actual connecting user original physically retired");
    assert_eq!(
        std::fs::read_to_string(&marker)
            .expect("actual parked starts")
            .lines()
            .count(),
        1,
        "expected single actual parked startup may have changed"
    );
}
