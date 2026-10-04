//! Capture authority follows the exact original Arc and native resource release.
//! 捕获权威跟随精确原 Arc 及原生资源释放。

use super::*;
use async_trait::async_trait;
use std::collections::BTreeMap;
use tokio::sync::broadcast;
use vulcan_mcp_runtime::{McpRuntimeError, McpToolSchema, McpTransportEvent};

/// Minimal protocol fixture gives two distinct actual Arc allocations with identical labels.
/// 最小协议夹具提供标签相同的两个不同实际 Arc 分配。
struct FixtureTransport {
    /// The real event channel belongs to this exact fixture allocation.
    /// 真实事件通道属于此精确夹具分配。
    events: broadcast::Sender<McpTransportEvent>,
}

#[async_trait]
impl McpTransport for FixtureTransport {
    /// Returns the shared label; labels deliberately cannot establish identity.
    /// 返回相同标签；标签刻意不能建立身份。
    fn endpoint(&self) -> &str {
        "identical-label"
    }
    /// Returns a valid empty discovery result without native startup.
    /// 返回有效空发现结果，不执行原生启动。
    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
        Ok(Vec::new())
    }
    /// Subscribes to this original allocation's lifecycle events.
    /// 订阅此原分配的生命周期事件。
    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
        self.events.subscribe()
    }
    /// Returns a successful fixed result for supplied tool and arguments.
    /// 对给定工具及参数返回固定成功结果。
    async fn call_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, McpRuntimeError> {
        Ok(serde_json::json!({}))
    }
}

/// Creates a distinct transport allocation without inferring identity from its endpoint.
/// 创建不同传输分配，不从其端点推断身份。
fn transport() -> Arc<dyn McpTransport> {
    let (events, _) = broadcast::channel(1);
    Arc::new(FixtureTransport { events })
}

/// Creates exact explicit stdio inputs; env may intentionally be empty.
/// 创建精确显式标准输入输出输入；env 可以刻意为空。
fn stdio_spec(inherit_environment: bool) -> McpTransportLaunchSpec {
    McpTransportLaunchSpec::Stdio {
        endpoint: "fixture".into(),
        program: "fixture".into(),
        args: Vec::new(),
        cwd: Some(std::env::current_dir().expect("fixture directory")),
        env: BTreeMap::new(),
        inherit_environment,
    }
}

/// Binds synthetic protocol ownership to a genuine resource-release receipt for precise rejection tests.
/// 将合成协议所有权绑定真实资源释放回执，用于精确拒绝测试。
fn input_lease(
    transport: &Arc<dyn McpTransport>,
    specification: McpTransportLaunchSpec,
) -> (McpActualLaunchInputLease, McpTransportResources) {
    let inputs = Arc::new(McpActualLaunchInputs {
        specification,
        auth_session: None,
        // This synthetic lease has no actual prepared published producer.
        // 此合成租约没有实际已准备已发布生产者。
        source: McpPluginLaunchSource::Unproven,
    });
    let (resources, release) = McpTransportResources::new(inputs.clone());
    (
        McpActualLaunchInputLease {
            transport: Arc::downgrade(transport),
            inputs,
            release,
        },
        resources,
    )
}

/// Identical labels cannot substitute another attempt; the original input lease does not retain the transport.
/// 相同标签不能替代另一次尝试；原输入租约不会保留传输。
#[test]
fn launch_capture_rejects_other_actual_transport_and_released_resources() {
    let original = transport();
    let substitute = transport();
    let (lease, resources) = input_lease(&original, stdio_spec(false));
    assert_eq!(
        lease.with_current_transport(&substitute, |_, _| ()),
        Err(McpLaunchInputCaptureError::DifferentTransport)
    );
    assert!(lease.with_current_transport(&original, |_, _| ()).is_ok());
    drop(resources);
    assert_eq!(
        lease.with_current_transport(&original, |_, _| ()),
        Err(McpLaunchInputCaptureError::Retired)
    );
    let weak = Arc::downgrade(&original);
    drop(original);
    assert!(
        weak.upgrade().is_none(),
        "input evidence must not own the protocol transport"
    );
}

/// Empty explicit env is complete, while ambient inheritance or directory dependence is unavailable.
/// 空显式环境是完整输入，环境继承或目录依赖则不可用。
#[test]
fn launch_capture_preserves_empty_env_and_rejects_implicit_native_inputs() {
    let original = transport();
    let (lease, _resources) = input_lease(&original, stdio_spec(false));
    assert!(lease.with_current_transport(&original, |spec, _| matches!(spec, McpTransportLaunchSpec::Stdio { env, .. } if env.is_empty())).expect("explicit empty env"));
    let (inherited, _inherited_resources) = input_lease(&original, stdio_spec(true));
    assert_eq!(
        inherited.with_current_transport(&original, |_, _| ()),
        Err(McpLaunchInputCaptureError::InheritedEnvironment)
    );
    let mut implicit_spec = stdio_spec(false);
    if let McpTransportLaunchSpec::Stdio { cwd, .. } = &mut implicit_spec {
        *cwd = None;
    }
    let (implicit, _implicit_resources) = input_lease(&original, implicit_spec);
    assert_eq!(
        implicit.with_current_transport(&original, |_, _| ()),
        Err(McpLaunchInputCaptureError::ImplicitWorkingDirectory)
    );
}

/// Invalid secret-bearing headers fail before connection and use the exact frozen redaction values.
/// 含密钥的无效请求头在连接前失败，并使用精确冻结值脱敏。
#[tokio::test]
async fn launch_capture_invalid_http_header_has_no_successful_owner_or_secret_diagnostic() {
    let secret = "captured-old-secret\r\ninjected";
    let specification = McpTransportLaunchSpec::Http {
        endpoint: "safe".into(),
        uri: "https://127.0.0.1:1/mcp?secret=old".into(),
        bearer_token: None,
        headers: BTreeMap::from([("X-Secret".into(), secret.into())]),
        oauth: None,
    };
    let (resources, release) = McpTransportResources::new(());
    let error = match McpLaunchedTransport::launch(
        specification,
        None,
        resources,
        McpPluginLaunchSource::Unproven,
    )
    .await
    {
        Ok(_) => panic!("invalid header must not create a successful capture owner"),
        Err(error) => error,
    };
    assert!(!error.contains("captured-old-secret"));
    assert!(!error.contains("secret=old"));
    assert!(
        release.is_released(),
        "invalid headers cannot retain native ownership"
    );
}

/// A future ambient lookup cannot reconstruct or replace the already consumed projection.
/// 未来环境查找不能重建或替换已经消费的投影。
#[test]
fn launch_capture_redacts_original_values_after_later_configuration_changes() {
    let inputs = McpActualLaunchInputs {
        specification: McpTransportLaunchSpec::Http {
            endpoint: "safe".into(),
            uri: "https://example.test/private-old".into(),
            bearer_token: Some("old-bearer".into()),
            headers: BTreeMap::from([("X-Secret".into(), "old-header-secret".into())]),
            oauth: None,
        },
        auth_session: None,
        // Redaction alone cannot establish a full published source.
        // 仅脱敏不能建立完整已发布来源。
        source: McpPluginLaunchSource::Unproven,
    };
    let error = inputs
        .redact_diagnostic("old-bearer old-header-secret https://example.test/private-old".into());
    assert_eq!(error, "[REDACTED] [REDACTED] [REDACTED]");
}
