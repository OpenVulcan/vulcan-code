//! Real raw startup verifies deferred dispatch and unpublished ownership retirement.
//! 真实原始启动验证延迟派发及未发布所有权退役。

use super::*;
use crate::mcp::{McpServerSource, McpServerTransportConfig};

/// Build the exact real Node fixture definition; marker records only actual native process starts.
/// 构建精确真实 Node 夹具定义；marker 仅记录实际原生进程启动。
/// Returns one declared raw plugin server without source certification.
/// 返回单个已声明原始插件服务器，不进行来源认证。
fn server(marker: &std::path::Path) -> McpServerConfig {
    McpServerConfig {
        source: McpServerSource::Plugin {
            plugin_id: "raw-retained".into(),
            generation: 5,
        },
        id: "raw-retained:actual".into(),
        name: "Actual raw retained fixture".into(),
        enabled: true,
        transport: McpServerTransportConfig::Stdio {
            command: "node".into(),
            args: vec![
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/owner-mcp.mjs")
                    .to_string_lossy()
                    .into_owned(),
            ],
            cwd: None,
            env: BTreeMap::from([(
                "VULCAN_MCP_PROCESS_MARKER".into(),
                marker.to_string_lossy().into_owned(),
            )]),
            env_passthrough: Vec::new(),
            inherit_environment: true,
        },
    }
}

/// Cancelling actual undispatched work releases its original resources without ever creating a native process.
/// 取消实际未派发工作释放其原资源，且绝不创建原生进程。
#[tokio::test]
async fn legacy_replacement_abort_before_dispatch_never_starts_native_work() {
    // All native observations belong to one isolated temporary root.
    // 全部原生观测属于单个隔离临时根。
    let root = tempfile::tempdir().expect("raw deferred fixture");
    // Only the actual process can create this startup marker.
    // 仅实际进程可创建此启动标记。
    let marker = root.path().join("starts.txt");
    // The actual empty access supplies the original independent admission lineage.
    // 实际空访问提供原独立准入谱系。
    let current = McpRuntimeAccess::new(
        configured_mcp_registry_overview(Vec::new()),
        BTreeMap::new(),
    );
    // Pure preparation constructs the retained owner without spawning a lifecycle task.
    // 纯准备构造保留所有者，不启动生命周期任务。
    let native =
        LegacyPluginMcpReplacement::prepare(&current, "raw-retained", 5, vec![server(&marker)])
            .expect("actual undispatched work");
    assert!(!marker.exists());
    native
        .abort_and_drain()
        .await
        .expect("positive undispatched original release");
    assert!(!marker.exists());
    assert!(native.dispatch().is_err());
    assert!(
        current
            .available_tool_bindings_filtered(&Default::default())
            .is_empty()
    );
}

/// Actual Ready remains business-closed and a second dispatch cannot replay the same original initialization.
/// 实际就绪保持业务关闭，第二次派发不能重放相同原初始化。
#[tokio::test]
async fn legacy_replacement_ready_is_closed_until_actual_full_publication() {
    // Real startup marker exposes process identity without retaining native resources.
    // 真实启动标记暴露进程身份，不保留原生资源。
    let root = tempfile::tempdir().expect("raw Ready fixture");
    // This file records every real native process start.
    // 此文件记录每次真实原生进程启动。
    let marker = root.path().join("starts.txt");
    // Original empty access has no live execution partition to preserve or replace.
    // 原空访问没有需要保留或替换的活动执行分区。
    let current = McpRuntimeAccess::new(
        configured_mcp_registry_overview(Vec::new()),
        BTreeMap::new(),
    );
    // The same actual controller owns preparation, dispatch and its original execution lease.
    // 相同实际控制器拥有准备、派发及其原执行租约。
    let native =
        LegacyPluginMcpReplacement::prepare(&current, "raw-retained", 5, vec![server(&marker)])
            .expect("exact raw preparation");
    native.dispatch().expect("register before actual dispatch");
    native
        .wait_until_ready()
        .await
        .expect("actual raw complete discovery");
    assert!(native.dispatch().is_err());
    assert_eq!(
        std::fs::read_to_string(&marker)
            .expect("actual startup evidence")
            .lines()
            .count(),
        1
    );
    // Prepared access owns the same actual lease but grants no business before full-pointer confirmation.
    // 已准备访问拥有相同实际租约，但在完整指针确认前不授予业务。
    let next = native
        .publication_access(&current)
        .expect("same original publication access");
    assert!(
        next.available_tool_bindings_filtered(&Default::default())
            .is_empty()
    );
    native
        .abort_and_drain()
        .await
        .expect("actual original cleanup and physical release");
    assert!(
        next.available_tool_bindings_filtered(&Default::default())
            .is_empty()
    );
}
