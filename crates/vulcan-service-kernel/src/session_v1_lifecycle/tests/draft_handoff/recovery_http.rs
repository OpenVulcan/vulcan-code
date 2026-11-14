//! Verifies recovery publication through a disconnected and reconnected HTTP client.
//! 验证 HTTP 客户端断开并重连时的恢复发布。

use super::*;
use std::{sync::Mutex, time::Duration};
use tokio::{io::AsyncWriteExt, net::TcpStream, sync::Notify, time::timeout};
use vulcan_provider_adapters::{CredentialConnection, CredentialMode, PersistedAiConfiguration};
use vulcan_service_api::{
    ServiceEvent, ServiceEventEmitter, WebuiServiceRpcHttpResponse, WorkbenchSnapshot,
    webui_service_ready_discovery_snapshot,
};
use vulcan_service_host::{WebuiServiceHttpConfig, serve_webui_service_http_listener};

/// Signals when the original HTTP observer stops waiting.
/// 在原 HTTP 观测者停止等待时发出信号。
struct RecoveryRpcObserver(Arc<Notify>);

impl Drop for RecoveryRpcObserver {
    /// Notifies the test that the HTTP handler has dropped its recovery future.
    /// 通知测试 HTTP 处理器已丢弃其恢复 Future。
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

/// Reads the retained workbench over `rpc_url` using the private listener's `token`.
/// 使用私有监听器的 `token` 通过 `rpc_url` 读取保留工作台。
/// Returns the decoded snapshot or fails the test on an HTTP or RPC error.
/// 返回解码后的快照，遇到 HTTP 或 RPC 错误时使测试失败。
async fn reconnect_workbench(
    client: &reqwest::Client,
    rpc_url: &str,
    token: &str,
) -> WorkbenchSnapshot {
    // A snapshot read must work without retrying the original recovery action.
    // 读取快照时无需重试原恢复操作。
    let response: WebuiServiceRpcHttpResponse = client
        .post(rpc_url)
        .header("x-vulcan-service-token", token)
        .json(&serde_json::json!({ "method": "workbench.snapshot" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    serde_json::from_value(response.result.expect("successful workbench RPC")).unwrap()
}

/// Disconnects the real recovery RPC before publication and reads the projected session after reconnecting.
/// 在发布前断开真实恢复 RPC，并在重连后读取已经投影的会话。
#[test]
fn reasoning_recovery_refreshes_workbench_after_http_disconnect() {
    // Run the full RPC dispatcher on the same worker stacks as the production bridge.
    // 使用与生产桥接相同的工作线程栈执行完整 RPC 分发。
    crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .expect("recovery HTTP runtime")
        .block_on(async {
            tokio::spawn(verify_recovery_after_http_disconnect())
                .await
                .expect("recovery HTTP regression");
        });
}

/// Drives the isolated service and its two HTTP connections until the workbench is updated.
/// 驱动隔离服务及其两条 HTTP 连接，直到工作台更新。
async fn verify_recovery_after_http_disconnect() {
    // The isolated database and test runtime serve one source conversation without Provider traffic.
    // 隔离数据库和测试运行时提供单个来源会话，无需访问供应商。
    let fixture = LifecycleStorageFixture::start().await;
    prepare_recovery_project(&fixture).await;
    // Use the bundled fixed catalog with an in-memory connection and no credential material.
    // 使用内置固定目录和内存连接，无需凭据材料。
    let agent = crate::tests::test_support::test_agent_runtime_bridge();
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &agent.public_ai_runtime.providers().unwrap(),
            CredentialConnection {
                id: "recovery-http-account".into(),
                provider_id: "deepseek".into(),
                service_entry_id: "official-api".into(),
                credential_profile_id: "api-key".into(),
                credential_mode: CredentialMode::Token,
                credential_id: "recovery-http-credential".into(),
                display_name: "Recovery HTTP fixture".into(),
                secret_reference: "recovery-http-fixture-reference".into(),
                management_secret_reference: None,
                service_origin_override: None,
            },
        )
        .unwrap();
    configuration.set_model_enabled("deepseek", "deepseek-flash", true);
    agent
        .public_ai_runtime
        .reapply_model_catalog_overlays(&configuration)
        .unwrap();
    // The dispatcher and the source both use this service's session registry.
    // 分发器和来源会话均使用此服务的会话注册表。
    let state = create_service_bridge_state(test_service_shell_host());
    let registry = {
        let mut bridge = state.lock().unwrap();
        bridge.controller_runtime = Some(fixture.controller.clone());
        bridge.agent_runtime = Some(agent);
        bridge.session_runtime_registry.clone()
    };
    // The original session stays present before and after recovery.
    // 原会话在恢复前后均保留。
    let source_id = SessionId::new_v4();
    create_fixture_session(&fixture, &registry, source_id).await;
    crate::workbench_refresh::refresh_workbench_snapshot(state.clone())
        .await
        .unwrap();

    // Bind a real WebUI host on an isolated ephemeral loopback port.
    // 在独立的临时回环端口启动真实 WebUI 宿主。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let token = SessionId::new_v4().to_string();
    let discovery = webui_service_ready_discovery_snapshot(
        address.port(),
        std::process::id(),
        crate::current_unix_millis_text(),
        token.clone(),
    );
    // Observe both the original RPC's cancellation and the service's eventual workbench broadcast.
    // 同时观测原 RPC 取消和服务最终发出的工作台广播。
    let (events, mut updates) = tokio::sync::broadcast::channel(16);
    let started = Arc::new(Notify::new());
    let disconnected = Arc::new(Notify::new());
    let emitter =
        ServiceEventEmitter::new(vec![vulcan_service_host::webui_service_event_bus_sink(
            events.clone(),
        )]);
    // Only the recovery call needs observation. Both methods still use the production dispatcher.
    // 仅恢复调用需要观测，两种方法都仍经生产分发器执行。
    let host = WebuiServiceHttpConfig {
        discovery: Arc::new(Mutex::new(discovery)),
        service_token: token.clone(),
        event_bus: events,
        static_root: None,
        rpc_dispatcher: Arc::new({
            let (started, disconnected) = (started.clone(), disconnected.clone());
            move |method, params| {
                let state = state.clone();
                let emitter = emitter.clone();
                let (started, disconnected) = (started.clone(), disconnected.clone());
                Box::pin(async move {
                    let recovery = method == "service_create_reasoning_recovery_session";
                    let mut rpc = Box::pin(crate::rpc_dispatch::dispatch_service_rpc(
                        emitter, state, method, params,
                    ));
                    if recovery {
                        let _observer = RecoveryRpcObserver(disconnected);
                        assert!(futures_util::poll!(&mut rpc).is_pending());
                        started.notify_one();
                        rpc.await
                    } else {
                        rpc.await
                    }
                })
            }
        }),
        session_resource_resolver: Arc::new(|_| Box::pin(async { unreachable!() })),
        client_disconnected: Arc::new(|_| {}),
        status_provider: None,
        browser_stream: None,
        browser_download_resolver: None,
        mcp_endpoint: None,
    };
    // Hold the same configuration guard used by the publisher until the original HTTP request has dropped.
    // 持有发布者使用的同一配置守卫，直到原 HTTP 请求被丢弃。
    let publication_gate = crate::begin_public_ai_configuration_transaction().await;
    // Use an actual TCP disconnect, then a separate HTTP client for workbench reads.
    // 实际断开 TCP 连接，再使用独立 HTTP 客户端读取工作台。
    let verify = async move {
        let mut original = TcpStream::connect(address).await.unwrap();
        let body = serde_json::json!({
            "method": "service_create_reasoning_recovery_session",
            "params": { "request": {
                "actionId": "http-recovery-disconnect",
                "conversationId": source_id,
                "providerId": "deepseek",
                "modelId": "deepseek-flash",
                "connectionId": "recovery-http-account"
            }}
        })
        .to_string();
        original
            .write_all(
                format!(
                    "POST /rpc HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nX-Vulcan-Service-Token: {token}\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len(),
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        started.notified().await;
        drop(original);
        disconnected.notified().await;

        // Reconnect while publication is still blocked, as when a browser reloads quickly.
        // 在发布仍被阻止时重连，模拟浏览器快速刷新。
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let rpc_url = format!("http://{address}/rpc");
        let before = reconnect_workbench(&client, &rpc_url, &token).await;
        assert_eq!(before.conversations.len(), 1);
        drop(publication_gate);

        // The completed publisher notifies an already reconnected client and retains the same new session.
        // 完成发布的任务通知已重连的客户端，并保留同一个新会话。
        let ServiceEvent::Workbench(updated) = updates.recv().await.unwrap() else {
            panic!("expected the recovery workbench broadcast");
        };
        let after = reconnect_workbench(&client, &rpc_url, &token).await;
        let source_id = source_id.to_string();
        let recovered = after
            .conversations
            .iter()
            .find(|session| session.id != source_id)
            .expect("reconnected workbench contains the recovered session");
        assert_eq!(after.conversations.len(), 2);
        assert!(after.revision > before.revision);
        assert!(
            updated
                .conversations
                .iter()
                .any(|session| session.id == recovered.id)
        );
        assert_eq!(
            after.selected_conversation_id.as_deref(),
            Some(recovered.id.as_str())
        );
    };
    tokio::select! {
        result = timeout(Duration::from_secs(10), verify) => {
            result.expect("recovery must update the workbench after the HTTP client disconnects");
        }
        result = serve_webui_service_http_listener(listener, host) => {
            panic!("private WebUI listener terminated early: {result:?}");
        }
    }
    fixture.shutdown().await;
}
