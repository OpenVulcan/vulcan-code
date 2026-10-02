//! Real children witness exact launch inputs without unsafe mutation of the Rust host environment.
//! 真实子进程见证精确启动输入，不通过不安全操作修改 Rust 宿主环境。

use super::*;
use crate::mcp::config::{McpServerSource, McpServerTransportConfig};
use crate::mcp::registry::McpLaunchInputCaptureError;
use vulcan_mcp_runtime::McpTransportLaunchSpec;

/// The independent Node child records startup values, then changes only its own environment on a tool call.
/// 独立 Node 子进程记录启动值，之后仅在工具调用时修改自身环境。
const CHILD_SCRIPT: &str = r#"
// The marker witnesses real startup before initialize returns.
// 标记见证 initialize 返回之前的真实启动。
const fs = require('node:fs');
const startup = {pid: process.pid, value: process.env.VULCAN_CAPTURE_TOKEN ?? null};
fs.writeFileSync(process.argv[1], JSON.stringify(startup));
const reader = require('node:readline').createInterface({input: process.stdin, crlfDelay: Infinity});
// A single actual process serves discovery and the environment-change probe.
// 单个实际进程提供发现及环境变化探测。
reader.on('line', line => {
  const request = JSON.parse(line);
  if (request.id === undefined) return;
  let result = {};
  if (request.method === 'initialize') result = {protocolVersion: request.params.protocolVersion, capabilities: {tools: {}}, serverInfo: {name: 'actual-input-fixture', version: '1'}};
  if (request.method === 'tools/list') result = {tools: [{name: 'change_child_env', description: 'Change only this child environment.', inputSchema: {type: 'object', additionalProperties: false}}]};
  if (request.method === 'tools/call') {
    process.env.VULCAN_CAPTURE_TOKEN = 'child-new-value';
    result = {content: [{type: 'text', text: JSON.stringify({pid: process.pid, value: process.env.VULCAN_CAPTURE_TOKEN})}]};
  }
  process.stdout.write(JSON.stringify({jsonrpc: '2.0', id: request.id, result}) + '\n');
});
"#;

/// Resolves the real installed Node executable before building an environment-cleared actual child.
/// 构建清除环境的实际子进程前，解析真实已安装 Node 可执行文件。
/// No inputs; returns its absolute process executable path or fails explicitly when unavailable.
/// 无输入；返回其绝对进程可执行文件路径，不可用时显式失败。
fn node_executable() -> String {
    let output = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .expect("Node is required for actual MCP startup tests");
    assert!(
        output.status.success(),
        "actual Node executable lookup failed"
    );
    String::from_utf8(output.stdout)
        .expect("Node path UTF-8")
        .trim()
        .to_owned()
}

/// Creates an exact enabled declaration with explicit platform input and optional business token.
/// 创建具有显式平台输入及可选业务令牌的精确启用声明。
/// Marker is a private child witness; token selects the explicit value, inheritance is retained as declared.
/// marker 为私有子进程见证；token 选择显式值，inheritance 按声明保留。
fn actual_server(
    marker: &std::path::Path,
    token: Option<&str>,
    inheritance: bool,
) -> McpServerConfig {
    // The optional business value is independent from the platform inputs needed by this real executable.
    // 可选业务值独立于此真实可执行文件需要的平台输入。
    let mut env = token
        .map(|value| BTreeMap::from([("VULCAN_CAPTURE_TOKEN".into(), value.into())]))
        .unwrap_or_default();
    // Windows Node initializes its native CSPRNG before evaluating JavaScript and requires SystemRoot.
    // Windows Node 在求值 JavaScript 前初始化原生随机数生成器，需要 SystemRoot。
    // Freeze this explicit fixture input instead of changing production launch defaults or inheriting ambient secrets.
    // 冻结此显式夹具输入，不更改生产启动默认值，也不继承环境中的密钥。
    #[cfg(windows)]
    env.insert(
        String::from("SystemRoot"),
        std::env::var("SystemRoot").expect("Windows Node fixture requires explicit SystemRoot"),
    );
    McpServerConfig {
        source: McpServerSource::UserConfigured,
        id: "actual-launch-input".into(),
        name: "Actual launch input".into(),
        enabled: true,
        transport: McpServerTransportConfig::Stdio {
            command: node_executable(),
            args: vec![
                "-e".into(),
                CHILD_SCRIPT.into(),
                marker.to_string_lossy().into_owned(),
            ],
            cwd: Some(
                marker
                    .parent()
                    .expect("marker parent")
                    .to_string_lossy()
                    .into_owned(),
            ),
            env,
            env_passthrough: Vec::new(),
            inherit_environment: inheritance,
        },
    }
}

/// Returns only the protected test value during a trusted synchronous live-owner capture.
/// 在可信同步实时所有者捕获期间，仅返回受保护测试值。
fn captured_token(
    runtime: &McpRuntimeHandle,
) -> Result<Option<String>, McpLaunchInputCaptureError> {
    runtime.with_connected_launch_inputs("actual-launch-input", |spec, _| match spec {
        McpTransportLaunchSpec::Stdio { env, .. } => env.get("VULCAN_CAPTURE_TOKEN").cloned(),
        _ => panic!("exact declared fixture transport changed"),
    })
}

/// Startup and a later environment mutation belong to one actual child; capture retains its original passed inputs.
/// 启动及之后的环境修改属于同一实际子进程；捕获保留其原传入输入。
/// No host environment is mutated; a second configuration cannot replace the recorded original projection.
/// 不修改宿主环境；第二份配置不能替换有记录的原投影。
#[tokio::test]
async fn launch_capture_same_actual_child_keeps_original_inputs_after_child_env_changes() {
    for token in [Some("original-launch-value"), None] {
        let directory = tempfile::tempdir().expect("actual launch witness");
        let marker = directory.path().join("startup.json");
        let server = actual_server(&marker, token, false);
        let runtime = bootstrap_mcp_runtime(vec![server.clone()])
            .await
            .expect("actual runtime");
        let mut cleanup = McpRuntimeStartupGuard::new();
        cleanup.track(&runtime);
        runtime
            .wait_until_ready(Duration::from_secs(10))
            .await
            .expect("real child connected");
        let startup: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&marker).expect("actual child marker"))
                .expect("actual marker JSON");
        assert_eq!(
            startup["value"],
            token
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null)
        );
        assert_eq!(
            captured_token(&runtime).expect("same actual connected owner"),
            token.map(str::to_owned)
        );
        // This later declaration has changed values but cannot become evidence for the existing child.
        // 此后续声明值已变化，但不能成为现有子进程的证据。
        let mut later = server.clone();
        if let McpServerTransportConfig::Stdio { env, .. } = &mut later.transport {
            env.insert(
                "VULCAN_CAPTURE_TOKEN".into(),
                "later-configuration-value".into(),
            );
        }
        assert!(
            matches!(later.launch_spec().expect("later independent projection"), McpTransportLaunchSpec::Stdio { env, .. } if env["VULCAN_CAPTURE_TOKEN"] == "later-configuration-value")
        );
        let result = runtime
            .call_tool_by_id(
                "actual-launch-input/change_child_env",
                serde_json::json!({}),
            )
            .await
            .expect("same child tool call");
        // The authoritative machine projection decodes the single JSON text block to its JSON value.
        // 权威机器投影将单个 JSON 文本块解码为其 JSON 值。
        let current = result;
        assert_eq!(
            current["pid"], startup["pid"],
            "capture and mutation must identify one real process"
        );
        assert_eq!(current["value"], "child-new-value");
        assert_eq!(
            captured_token(&runtime).expect("frozen original input"),
            token.map(str::to_owned)
        );
        runtime.shutdown();
        assert_eq!(
            captured_token(&runtime),
            Err(McpLaunchInputCaptureError::Unavailable)
        );
        runtime
            .wait_until_drained(tokio::time::Instant::now() + Duration::from_secs(10))
            .await
            .expect("original physical retirement");
        assert!(
            runtime
                .inner
                .launch_inputs
                .read()
                .expect("input map")
                .is_empty()
        );
    }
}

/// Complete normal inherited launch still connects, while full input capture remains explicitly unavailable.
/// 正常继承启动仍完整连接，而完整输入捕获保持显式不可用。
#[tokio::test]
async fn launch_capture_unavailable_inherited_inputs_do_not_fail_server_startup() {
    let directory = tempfile::tempdir().expect("inherited launch witness");
    let server = actual_server(
        &directory.path().join("startup.json"),
        Some("explicit-original"),
        true,
    );
    let runtime = bootstrap_mcp_runtime(vec![server])
        .await
        .expect("normal inherited runtime");
    let mut cleanup = McpRuntimeStartupGuard::new();
    cleanup.track(&runtime);
    runtime
        .wait_until_ready(Duration::from_secs(10))
        .await
        .expect("inherited child connected normally");
    assert_eq!(
        captured_token(&runtime),
        Err(McpLaunchInputCaptureError::InheritedEnvironment)
    );
    runtime.shutdown();
    runtime
        .wait_until_drained(tokio::time::Instant::now() + Duration::from_secs(10))
        .await
        .expect("inherited child retirement");
}

/// A connected instance without launch evidence is unavailable rather than reconstructed from its current declaration.
/// 没有启动证据的已连接实例不可用，不从其当前声明重建。
#[tokio::test]
async fn launch_capture_missing_original_record_is_unavailable_without_current_configuration_fallback()
 {
    let directory = tempfile::tempdir().expect("missing evidence witness");
    let server = actual_server(
        &directory.path().join("startup.json"),
        Some("actual-old"),
        false,
    );
    let runtime = bootstrap_mcp_runtime(vec![server])
        .await
        .expect("actual runtime");
    let mut cleanup = McpRuntimeStartupGuard::new();
    cleanup.track(&runtime);
    runtime
        .wait_until_ready(Duration::from_secs(10))
        .await
        .expect("actual connected child");
    runtime
        .inner
        .launch_inputs
        .write()
        .expect("input map")
        .remove("actual-launch-input");
    assert_eq!(
        captured_token(&runtime),
        Err(McpLaunchInputCaptureError::Unavailable)
    );
    runtime.shutdown();
    runtime
        .wait_until_drained(tokio::time::Instant::now() + Duration::from_secs(10))
        .await
        .expect("actual child retirement");
}

/// Failed and inactive declarations cannot claim a connected sibling's actual inputs.
/// 失败及停用声明不能声称拥有已连接兄弟实例的实际输入。
/// One-shot readiness preserves per-server isolation; no retry counter or shared server label grants capture authority.
/// 单次就绪保留单服务器隔离；重试计数或共享服务器标签不授予捕获权威。
#[tokio::test]
async fn launch_capture_failed_and_inactive_servers_cannot_impersonate_connected_owner() {
    let directory = tempfile::tempdir().expect("isolated server witness");
    let healthy = actual_server(
        &directory.path().join("startup.json"),
        Some("healthy-original"),
        false,
    );
    let mut failed = healthy.clone();
    failed.id = "failed-input-source".into();
    if let McpServerTransportConfig::Stdio { command, .. } = &mut failed.transport {
        *command = directory
            .path()
            .join("missing-native-program.exe")
            .to_string_lossy()
            .into_owned();
    }
    let mut inactive = healthy.clone();
    inactive.id = "inactive-input-source".into();
    inactive.enabled = false;
    let (resources, _) = vulcan_mcp_runtime::McpTransportResources::new(());
    let runtime = create_mcp_runtime(
        vec![healthy, failed, inactive],
        resources,
        McpRuntimePurpose::CandidateReadiness,
    )
    .expect("per-server runtime creation");
    let mut cleanup = McpRuntimeStartupGuard::new();
    cleanup.track(&runtime);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.servers["actual-launch-input"].status == McpRuntimeServerStatus::Connected
                && snapshot.servers["failed-input-source"].status == McpRuntimeServerStatus::Failed
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("healthy and failed original native starts completed");
    assert_eq!(
        captured_token(&runtime).expect("healthy original owner"),
        Some("healthy-original".into())
    );
    for id in ["failed-input-source", "inactive-input-source"] {
        assert_eq!(
            runtime.with_connected_launch_inputs(id, |_, _| ()),
            Err(McpLaunchInputCaptureError::Unavailable)
        );
    }
    assert_eq!(
        runtime
            .inner
            .launch_inputs
            .read()
            .expect("input map")
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["actual-launch-input"]
    );
    runtime.shutdown();
    runtime
        .wait_until_drained(tokio::time::Instant::now() + Duration::from_secs(10))
        .await
        .expect("all original resources retired");
}
