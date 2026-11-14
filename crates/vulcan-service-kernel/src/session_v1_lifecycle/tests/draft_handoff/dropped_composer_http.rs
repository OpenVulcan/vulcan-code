//! Exercises drag delivery through two independent WebUI HTTP clients.
//! 通过两个独立 WebUI HTTP 客户端验证拖入交付。

use super::*;
use serde_json::{Value, json};
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, CredentialSecretEnvelope, CredentialSecretResolver,
    CredentialSecretStore, PersistedAiConfiguration, PublicAiRuntime,
};
use vulcan_service_api::{
    ServiceEventEmitter, WebuiServiceRpcHttpResponse, webui_service_ready_discovery_snapshot,
};
use vulcan_service_host::{WebuiServiceHttpConfig, serve_webui_service_http_listener};

/// Resolves only synthetic credentials for the isolated service.
/// 只为隔离服务解析合成凭据。
struct DropTestSecrets;

#[async_trait::async_trait]
impl CredentialSecretResolver for DropTestSecrets {
    /// Returns a fixture credential without consulting a user store.
    /// 返回夹具凭据，无需查询用户存储。
    async fn read_optional_secret(
        &self,
        reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        if reference != "drop-test-secret" {
            return Err("Unexpected credential reference.".into());
        }
        Ok(Some(CredentialSecretEnvelope::new_api_key(
            "offline-drop-fixture".into(),
        )?))
    }
}

#[async_trait::async_trait]
impl CredentialSecretStore for DropTestSecrets {
    /// Rejects creation of any secret in this immutable fixture.
    /// 拒绝在此不可变夹具中创建任何凭据。
    async fn create_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Read-only drop test credentials.".into())
    }

    /// Rejects changing a fixture reference at any revision.
    /// 拒绝在任何修订版本更改夹具引用。
    async fn replace_secret(
        &self,
        _reference: &str,
        _revision: u64,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Read-only drop test credentials.".into())
    }

    /// Rejects deleting a fixture reference.
    /// 拒绝删除夹具引用。
    async fn delete_secret(&self, _reference: &str) -> Result<(), String> {
        Err("Read-only drop test credentials.".into())
    }

    /// Rejects restoring credential material into the fixture.
    /// 拒绝向夹具恢复凭据材料。
    async fn restore_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Read-only drop test credentials.".into())
    }
}

/// Posts a production RPC and returns the decoded response from this exact client.
/// 发送生产 RPC，并返回此精确客户端收到的解码响应。
async fn rpc(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    method: &str,
    params: Value,
) -> WebuiServiceRpcHttpResponse {
    client
        .post(url)
        .header("x-vulcan-service-token", token)
        .json(&json!({ "method": method, "params": params }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// Creates a catalog with one video-enabled model and one text-only model, with no network inference.
/// 创建含视频模型与纯文本模型的目录，无网络推理。
fn drop_test_runtime() -> Arc<PublicAiRuntime> {
    let runtime = PublicAiRuntime::first_batch(Arc::new(DropTestSecrets)).unwrap();
    let mut config = PersistedAiConfiguration::default();
    for (provider, model) in [
        ("stepfun", "step-3.7-flash"),
        ("deepseek", "deepseek-v4-pro"),
    ] {
        config
            .connection_graph
            .add_connection(
                &runtime.providers().unwrap(),
                CredentialConnection {
                    id: format!("drop-{provider}"),
                    provider_id: provider.into(),
                    service_entry_id: "official-api".into(),
                    credential_profile_id: "api-key".into(),
                    credential_mode: CredentialMode::Token,
                    credential_id: format!("drop-credential-{provider}"),
                    display_name: format!("Drop fixture {provider}"),
                    secret_reference: "drop-test-secret".into(),
                    management_secret_reference: None,
                    service_origin_override: None,
                },
            )
            .unwrap();
        config.set_model_enabled(provider, model, true);
    }
    runtime.reapply_model_catalog_overlays(&config).unwrap();
    Arc::new(runtime)
}

/// Drives the real HTTP dispatcher on the production-shaped Core runtime.
/// 在生产形态 Core 运行时上驱动真实 HTTP 分派器。
#[test]
fn dropped_composer_keeps_one_route_and_budget_across_two_http_clients() {
    crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap()
        .block_on(verify_dropped_composer_http());
}

/// Verifies read-only drop admission, shared count, body limit, and a concurrent model switch.
/// 验证只读拖入准入、共享数量限制、请求体上限和并发模型切换。
async fn verify_dropped_composer_http() {
    let fixture = LifecycleStorageFixture::start().await;
    prepare_recovery_project(&fixture).await;
    let project = fixture._temporary.path().to_str().unwrap();
    let config_root = fixture._temporary.path().join("private-provider-config");
    std::fs::create_dir(&config_root).unwrap();
    let state = create_service_bridge_state(test_service_shell_host());
    let session_id = SessionId::new_v4();
    let registry = {
        let mut bridge = state.lock().unwrap();
        bridge.controller_runtime = Some(fixture.controller.clone());
        let mut agent = crate::tests::test_support::test_agent_runtime_bridge();
        agent.public_ai_runtime = drop_test_runtime();
        bridge.agent_runtime = Some(agent);
        bridge.session_runtime_registry.clone()
    };
    let handle = create_fixture_session(&fixture, &registry, session_id).await;
    crate::workbench_refresh::refresh_workbench_snapshot(state.clone())
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let token = Uuid::new_v4().to_string();
    let discovery = webui_service_ready_discovery_snapshot(
        address.port(),
        std::process::id(),
        crate::current_unix_millis_text(),
        token.clone(),
    );
    let (events, _) = tokio::sync::broadcast::channel(16);
    let host = WebuiServiceHttpConfig {
        discovery: Arc::new(std::sync::Mutex::new(discovery)),
        service_token: token.clone(),
        event_bus: events,
        static_root: None,
        rpc_dispatcher: Arc::new({
            let state = state.clone();
            move |method, params| {
                Box::pin(
                    crate::tests::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT.scope(
                        config_root.clone(),
                        crate::rpc_dispatch::dispatch_service_rpc(
                            ServiceEventEmitter::new(Vec::new()),
                            state.clone(),
                            method,
                            params,
                        ),
                    ),
                )
            }
        }),
        session_resource_resolver: Arc::new(|_| Box::pin(async { unreachable!() })),
        client_disconnected: Arc::new(|_| {}),
        status_provider: None,
        browser_stream: None,
        browser_download_resolver: None,
        mcp_endpoint: None,
    };
    let verify = async {
        // One client composes while the other changes the same session's pending model.
        // 一个客户端撰写，另一个客户端修改同一会话的待生效模型。
        let composer = reqwest::Client::builder().no_proxy().build().unwrap();
        let selector = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = format!("http://{address}/rpc");
        let slot = format!("conversation:{session_id}");
        let call = |method, params| rpc(&composer, &url, &token, method, params);
        let selected = rpc(
            &selector,
            &url,
            &token,
            "service_select_public_ai_model",
            json!({ "request": { "providerId": "stepfun", "modelId": "step-3.7-flash",
                "connectionId": "drop-stepfun", "conversationId": session_id } }),
        )
        .await;
        assert!(selected.ok, "{:?}", selected.error);
        let request = |attachments: Value, files: Value, for_submission| {
            json!({
                "request": { "projectId": RECOVERY_PROJECT_ID, "conversationId": session_id, "draftSlotKey": slot,
                "modelSelection": null, "attachments": attachments, "files": files, "maxNewAttachments": 10,
                "forSubmission": for_submission }
            })
        };
        let video = json!({"fileName":"fixture.mp4","mimeType":"video/mp4","byteLen":3});
        let restricted = call(
            "service_classify_dropped_composer_files",
            request(
                json!([]),
                json!([
                    {"fileName":"help.chm","mimeType":"application/vnd.ms-htmlhelp","byteLen":3},
                    {"fileName":"picture.tiff","mimeType":"image/tiff","byteLen":3},
                    {"fileName":"unsupported.webm","mimeType":"video/webm","byteLen":3},
                    {"fileName":"boundary.mp4","mimeType":"video/mp4","byteLen":50331645},
                    video,
                ]),
                false,
            ),
        )
        .await;
        assert!(restricted.ok, "{:?}", restricted.error);
        assert_eq!(
            restricted.result.unwrap()["eligible"],
            json!([false, false, false, false, true])
        );
        let prefix = crate::attachments::stage_composer_attachment_base64(
            project,
            "picked.mp4",
            "video/mp4",
            "YWJj",
        )
        .unwrap();
        let scope = crate::attachments::composer_attachment_draft_scope(&slot).unwrap();
        vulcan_agent_runtime::attachment_asset_index::acquire_staged_attachment_ref(
            project,
            &scope,
            &[crate::composer_attachment_summary_to_core(prefix.clone()).unwrap()],
        )
        .unwrap();
        let mixed = call(
            "service_classify_dropped_composer_files",
            request(json!([prefix]), json!([video]), false),
        )
        .await;
        assert!(mixed.ok, "{:?}", mixed.error);
        assert_eq!(mixed.result.unwrap()["eligible"], json!([false]));
        let planned = call(
            "service_classify_dropped_composer_files",
            request(json!([]), json!([video]), true),
        )
        .await;
        assert!(planned.ok, "{:?}", planned.error);
        let result = planned.result.unwrap();
        assert_eq!(result["eligible"], json!([true]));
        let plan_id = &result["planId"];
        let changed = rpc(
            &selector,
            &url,
            &token,
            "service_select_public_ai_model",
            json!({ "request": { "providerId": "deepseek", "modelId": "deepseek-v4-pro",
                "connectionId": "drop-deepseek", "conversationId": session_id } }),
        )
        .await;
        assert!(changed.ok, "{:?}", changed.error);
        let staged = call(
            "service_stage_dropped_composer_file",
            json!({"planId":plan_id,"index":0,"dataBase64":"eHl6"}),
        )
        .await;
        assert!(staged.ok, "{:?}", staged.error);
        let attachment = staged.result.unwrap();
        assert!(attachment["assetId"].is_string());

        // Keep this test entirely offline by leaving a synthetic stream owner in place through queue admission.
        // 通过保留合成流所有者直至排队准入，确保此测试始终离线。
        let blocker = tokio::spawn(std::future::pending::<()>());
        let abort = blocker.abort_handle();
        state.lock().unwrap().active_stream_tasks.insert(
            session_id,
            crate::state::ActiveStreamTask::new(Uuid::new_v4(), blocker, None),
        );
        let sent = call(
            "service_submit_conversation_prompt",
            json!({
                "clientSubmissionId":"drop-test-submission", "projectId":RECOVERY_PROJECT_ID,
                "conversationId":session_id,"draftSlotKey":slot,"prompt":"Fixture send",
                "executionMode":"standard", "attachments":[attachment],"droppedPlanId":plan_id
            }),
        )
        .await;
        abort.abort();
        assert!(sent.ok, "{:?}", sent.error);
        assert_eq!(sent.result.unwrap()["queued"], true);
        let (queued_model, pending_model) = handle
            .read(|document| {
                (
                    document
                        .metadata
                        .input_state
                        .queued_inputs
                        .iter()
                        .find(|input| input.client_action_id.as_str() == "drop-test-submission")
                        .and_then(|input| input.bound_request_config.as_ref())
                        .and_then(|config| config.model.clone()),
                    document
                        .metadata
                        .config
                        .pending_request
                        .as_ref()
                        .and_then(|config| config.model.clone()),
                )
            })
            .unwrap();
        assert_eq!(queued_model.as_deref(), Some("step-3.7-flash"));
        assert_eq!(pending_model.as_deref(), Some("deepseek-v4-pro"));
        let after = call(
            "service_classify_dropped_composer_files",
            request(json!([]), json!([video]), true),
        )
        .await;
        assert!(after.ok, "{:?}", after.error);
        assert_eq!(after.result.unwrap()["eligible"], json!([false]));
    };
    tokio::select! {
        result = tokio::time::timeout(std::time::Duration::from_secs(30), verify) => result.unwrap(),
        result = serve_webui_service_http_listener(listener, host) => panic!("Drop fixture listener failed: {result:?}"),
    }
    fixture.shutdown().await;
}
