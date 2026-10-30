//! Real loopback HTTP coverage for usage extraction before semantic decoder projection.
//! 在语义解码投影之前提取用量的真实环回 HTTP 覆盖。

use super::*;
use crate::execution::test_support::bundled_manifest_route;
use serde_json::json;
use vulcan_ai_contract::{RequestUsage, UsageValue};
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID,
    GEMINI_INTERACTIONS_PROFILE_ID, GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
};

/// Build valid SSE records for `profile`; return a stream with optional partial usage records.
/// 为 `profile` 构建有效 SSE 记录，返回包含可选局部用量记录的流。
fn records(profile: &str) -> Vec<Value> {
    match profile {
        OPENAI_RESPONSES_PROFILE_ID => vec![
            json!({"type":"response.created","response":{"id":"test-response"}}),
            json!({"type":"response.output_text.delta","output_index":0,"item_id":"message","delta":"OK"}),
            json!({"type":"response.completed","response":{"id":"test-response","usage":{"input_tokens":10000,"input_tokens_details":{"cached_tokens":2500,"cache_write_tokens":3000},"output_tokens":5000,"output_tokens_details":{"reasoning_tokens":2500},"total_tokens":15000}}}),
        ],
        OPENAI_CHAT_PROFILE_ID => vec![
            json!({"id":"test-chat","choices":[{"index":0,"delta":{"role":"assistant","content":"OK"},"finish_reason":null}]}),
            json!({"id":"test-chat","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10000,"prompt_tokens_details":{"cached_tokens":2500,"cache_write_tokens":3000},"completion_tokens":5000,"completion_tokens_details":{"reasoning_tokens":2500},"total_tokens":15000}}),
        ],
        ANTHROPIC_MESSAGES_PROFILE_ID => vec![
            json!({"type":"message_start","message":{"id":"test-message","type":"message","role":"assistant","model":"test-model","content":[],"usage":{"input_tokens":4,"cache_read_input_tokens":2500,"cache_creation_input_tokens":7496,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"OK"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5000}}),
            json!({"type":"message_stop"}),
        ],
        GEMINI_GENERATE_CONTENT_PROFILE_ID | GOOGLE_CODE_ASSIST_PROFILE_ID => {
            // Thinking arrives after output, with no repeated input or candidate count.
            // 思考在输出之后到达，且不重复输入或候选计数。
            let records = vec![
                json!({"candidates":[{"content":{"parts":[{"text":"OK"}]}}],"usageMetadata":{"promptTokenCount":10000,"cachedContentTokenCount":2500}}),
                json!({"usageMetadata":{"candidatesTokenCount":5000}}),
                json!({"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"thoughtsTokenCount":2500}}),
            ];
            records
                .into_iter()
                .map(|record| {
                    if profile == GOOGLE_CODE_ASSIST_PROFILE_ID {
                        json!({"response":record})
                    } else {
                        record
                    }
                })
                .collect()
        }
        GEMINI_INTERACTIONS_PROFILE_ID => vec![
            json!({"event_type":"interaction.created","interaction":{"id":"test-interaction"}}),
            json!({"event_type":"interaction.completed","interaction":{"status":"completed","usage":{"total_input_tokens":10000,"total_cached_tokens":2500,"total_output_tokens":5000,"total_thought_tokens":2500,"total_tokens":17500}}}),
        ],
        _ => unreachable!("fixture requires an audited protocol"),
    }
}

/// Execute one isolated loopback response on `client` for `profile`; return merged public usage.
/// 在 `client` 上执行 `profile` 的独立环回响应，返回合并后的公共用量。
async fn execute_usage(client: &ProviderExecutionClient, profile: &str) -> RequestUsage {
    // Only this local listener receives the synthetic request and credential.
    // 只有此本地监听器接收合成请求与测试凭据。
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    // Bind the route to the operating system's allocated loopback port.
    // 将路由绑定到操作系统分配的环回端口。
    let address = listener.local_addr().unwrap();
    // Preserve independent SSE frames so production framing and partial merging are exercised.
    // 保留独立 SSE 帧，从而覆盖生产分帧与局部合并逻辑。
    let body = records(profile)
        .into_iter()
        .map(|record| format!("data: {record}\n\n"))
        .collect::<String>();
    // Serve one bounded response and join the task before this fixture returns.
    // 提供一条有界响应，并在样例返回前等待服务任务结束。
    let server = tokio::spawn(async move {
        // Read the complete request before returning the synthetic provider stream.
        // 先读取完整请求，再返回合成的供应商事件流。
        let (mut stream, _) = listener.accept().await.unwrap();
        let _request = read_test_http_request(&mut stream).await;
        // The real HTTP parser must honor the declared SSE content type and body length.
        // 真实 HTTP 解析器必须遵循声明的 SSE 内容类型和正文长度。
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    // Resolve real bundled manifests so fallback extraction and provider projection both run.
    // 解析真实内置清单，使默认用量提取和供应商投影同时执行。
    let (provider, entry) = match profile {
        GEMINI_GENERATE_CONTENT_PROFILE_ID | GEMINI_INTERACTIONS_PROFILE_ID => {
            ("google", "gemini-api")
        }
        GOOGLE_CODE_ASSIST_PROFILE_ID => ("google", "code-assist"),
        ANTHROPIC_MESSAGES_PROFILE_ID => ("anthropic", "public-api"),
        _ => ("openai", "public-api"),
    };
    // Keep manifest protocol defaults while overriding only the destination with the local fixture.
    // 保留清单协议默认值，仅将目的地址改为本地样例服务。
    let mut route = bundled_manifest_route(provider, entry, profile);
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    // Code Assist requires its typed OAuth and project envelope even against a mock endpoint.
    // 即使使用模拟端点，Code Assist 也需要类型化 OAuth 与项目封装。
    let secret = if profile == GOOGLE_CODE_ASSIST_PROFILE_ID {
        crate::CredentialSecretEnvelope::new(crate::CredentialSecretMaterial::GoogleOAuth {
            tokens: crate::OAuthTokenSet {
                access_token: "loopback-access".into(),
                refresh_token: "loopback-refresh".into(),
                issued_at_unix_ms: 1,
                expires_at_unix_ms: u64::MAX,
                scopes: Default::default(),
                account_subject: None,
                email: None,
            },
            client_id: "loopback-client".into(),
            client_secret: None,
            project_id: Some("loopback-project".into()),
            quota_project_id: None,
        })
        .unwrap()
    } else {
        crate::CredentialSecretEnvelope::new_api_key("loopback-key".into()).unwrap()
    }
    .execution_secret()
    .unwrap();
    // Use the shared valid request fixture and a model identity that cannot name a live target.
    // 使用共享的合法请求样例，以及不会指向真实目标的模型身份。
    let mut request = minimal_request();
    request.model_id = "test-model".into();
    // Consume the production transport, extraction and semantic decoder together.
    // 同时消费生产传输、用量提取与语义解码链路。
    let mut stream = client.execute(route, request, secret, "usage-accounting-test".into());
    // Track canonical usage and lifecycle counts as ordinary event consumers do.
    // 按普通事件消费者的方式跟踪统一用量与生命周期计数。
    let mut usage = RequestUsage::default();
    let mut starts = 0;
    let mut completes = 0;
    while let Some(event) = stream.next().await {
        match event.unwrap_or_else(|error| panic!("{profile}: {error:?}")) {
            AiStreamEvent::ResponseStarted { .. } => starts += 1,
            AiStreamEvent::ResponseCompleted => completes += 1,
            AiStreamEvent::UsageUpdated { usage: update } => {
                assert_eq!(starts, 1, "usage must follow the response start");
                usage.merge_update(*update);
            }
            _ => {}
        }
    }
    server.await.unwrap();
    assert_eq!((starts, completes), (1, 1), "{profile}");
    usage
}

/// Switching protocols on one client cannot retain another request's cache or thinking semantics.
/// 同一客户端切换协议时不得保留其他请求的缓存或思考语义。
#[tokio::test]
async fn http_protocol_switches_preserve_canonical_usage() {
    // Reuse one client to expose any accidental request-to-request state retention.
    // 复用同一客户端，以暴露请求之间意外保留的状态。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().unwrap()),
    );
    // Traverse every affected protocol in both directions on the same transport owner.
    // 在同一传输所有者上按正反两个方向遍历所有受影响协议。
    let profiles = [
        OPENAI_RESPONSES_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
        OPENAI_CHAT_PROFILE_ID,
    ];
    for profile in profiles.iter().chain(profiles.iter().rev()) {
        // Bound a broken local fixture without introducing retries or external network calls.
        // 为故障本地样例设置等待上限，不引入重试或外部网络调用。
        let usage = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            execute_usage(&client, profile),
        )
        .await
        .unwrap();
        // Google reports candidate output and thoughts separately; OpenAI and Anthropic output is complete.
        // Google 分别报告候选输出与思考；OpenAI 和 Anthropic 的输出已经完整。
        let output = if matches!(
            *profile,
            GEMINI_GENERATE_CONTENT_PROFILE_ID
                | GOOGLE_CODE_ASSIST_PROFILE_ID
                | GEMINI_INTERACTIONS_PROFILE_ID
        ) {
            7500
        } else {
            5000
        };
        assert!(
            matches!(usage.input_tokens, UsageValue::Known { value: 10000, .. }),
            "{profile}: {usage:?}"
        );
        assert!(
            matches!(
                usage.uncached_input_tokens,
                UsageValue::Known { value: 7500, .. }
            ),
            "{profile}"
        );
        assert!(
            matches!(usage.output_tokens,UsageValue::Known {value,..} if value==output),
            "{profile}: {usage:?}"
        );
        assert!(
            matches!(
                usage.cache_read_tokens,
                UsageValue::Known { value: 2500, .. }
            ),
            "{profile}"
        );
    }
}
