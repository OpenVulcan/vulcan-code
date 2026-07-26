//! OpenAI Responses host-controlled provider transport adapter spec.
//! OpenAI Responses 宿主控制 Provider 传输适配器 spec。

use crate::agent_runtime::provider_transport_adapter_support::build_openai_responses_streaming_request;
use crate::agent_runtime::provider_transport_common_adapter::{
    CommonProviderStreamingAdapter, CommonProviderStreamingAdapterConfig,
    ProviderStreamingTransportSpec,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_openai_responses_http::execute_openai_responses_http_streaming_turn;
use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use async_trait::async_trait;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Shared OpenAI Responses streaming adapter type bound to the OpenAI Responses transport spec.
/// 绑定 OpenAI Responses 传输 spec 的共享 OpenAI Responses 流式适配器类型。
pub(crate) type OpenAiResponsesCoreStreamingAdapter =
    CommonProviderStreamingAdapter<OpenAiResponsesStreamingTransport>;

/// Zero-sized OpenAI Responses transport spec that delegates to the OpenAI Responses HTTP streaming executor.
/// 委托给 OpenAI Responses HTTP 流式执行器的零大小 OpenAI Responses 传输 spec。
#[derive(Clone)]
pub(crate) struct OpenAiResponsesStreamingTransport;

/// Creates an OpenAI Responses streaming adapter from prepared host inputs, API credentials, optional endpoint override, and the live delta sender; returns a shared adapter ready for the host-controlled stream runtime.
/// 根据已准备的宿主输入、API 凭据、可选端点覆盖值和实时增量发送器创建 OpenAI Responses 流式适配器；返回可交给宿主控制流运行时的共享适配器。
pub(crate) fn new_openai_responses_core_streaming_adapter(
    adapter_inputs: PreparedProviderTransportAdapterInputs,
    api_key: String,
    base_url: Option<String>,
    live_sender: ProviderLiveDeltaSender,
) -> OpenAiResponsesCoreStreamingAdapter {
    OpenAiResponsesCoreStreamingAdapter::new(
        adapter_inputs,
        CommonProviderStreamingAdapterConfig {
            request_label: "OpenAI Responses".to_string(),
            transport_label: "OpenAI Responses".to_string(),
            api_key,
            base_url,
            interleaved_reasoning: None,
            live_sender,
        },
    )
}

#[async_trait]
impl ProviderStreamingTransportSpec for OpenAiResponsesStreamingTransport {
    /// Builds an OpenAI Responses request while preserving provider-native continuation semantics.
    /// 构建 OpenAI Responses 请求，同时保留 Provider 原生 continuation 语义。
    fn build_streaming_request(
        adapter: &OpenAiResponsesCoreStreamingAdapter,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
        build_openai_responses_streaming_request(
            adapter.provider_id.as_str(),
            adapter.model_id.as_str(),
            snapshot,
            config,
            &adapter.provider_tool_definitions,
            &adapter.request_headers,
            &adapter.live_sender,
        )
    }

    /// Executes one OpenAI Responses HTTP streaming turn using the shared adapter state, built request, and normalized delta sink; returns the final normalized provider response.
    /// 使用共享适配器状态、已构建请求和规范化增量 sink 执行一次 OpenAI Responses HTTP 流式回合；返回最终规范化 Provider 响应。
    async fn execute_streaming_turn<S>(
        adapter: &OpenAiResponsesCoreStreamingAdapter,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<CoreProviderTurnResponse, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        execute_openai_responses_http_streaming_turn(
            adapter.provider_id.as_str(),
            adapter.transport_label.as_str(),
            adapter.api_key.as_str(),
            adapter.base_url.as_deref(),
            adapter.model_id.as_str(),
            adapter.request_headers.clone(),
            request,
            &adapter.provider_tool_definitions,
            &adapter.live_sender,
            sink,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    //! OpenAI Responses adapter request-construction tests.
    //! OpenAI Responses 适配器请求构建测试。

    use super::*;
    use crate::agent_runtime::provider_stream::ProviderStreamDelta;
    use crate::agent_runtime::provider_transport_openai_responses_http::FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM;
    use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
    use reqwest::header::HeaderMap;
    use serde_json::json;
    use tokio::sync::mpsc::unbounded_channel;
    use vulcan_core::provider::VulcanProviderAdapter;
    use vulcan_core::types::{
        ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
        ProviderContextAnchor as CoreProviderContextAnchor,
        ProviderContextInjection as CoreProviderContextInjection,
        ProviderContextMutationKind as CoreProviderContextMutationKind,
        ProviderContinuation as CoreProviderContinuation,
        ProviderPromptLayers as CoreProviderPromptLayers,
    };

    /// Builds one OpenAI Responses adapter with inert transport credentials for request-only tests.
    /// 为仅构建请求的测试创建一个携带惰性传输凭据的 OpenAI Responses 适配器。
    fn build_test_openai_responses_adapter() -> OpenAiResponsesCoreStreamingAdapter {
        // live_sender receives request snapshots emitted during build_request.
        // live_sender 接收 build_request 期间发出的请求快照。
        let (live_sender, _live_receiver) =
            unbounded_channel::<Result<ProviderStreamDelta, String>>();
        new_openai_responses_core_streaming_adapter(
            PreparedProviderTransportAdapterInputs {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-5"),
                request_headers: HeaderMap::new(),
                provider_tool_definitions: Vec::new(),
            },
            String::from("test-key"),
            None,
            live_sender,
        )
    }

    /// Builds one minimal controller config for OpenAI Responses request-construction tests.
    /// 为 OpenAI Responses 请求构建测试创建一份最小控制器配置。
    fn build_test_controller_config(
        additional_params: Option<serde_json::Value>,
        context_injection: Option<CoreProviderContextInjection>,
    ) -> CoreControllerConfig {
        CoreControllerConfig {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-5"),
            system_prompt: String::from("system"),
            prompt_layers: CoreProviderPromptLayers::from_system_prompt(String::from("system")),
            temperature: None,
            max_turns: 8,
            additional_params,
            context_injection,
        }
    }

    /// Builds one conversation snapshot with an OpenAI Responses continuation cursor and one outgoing user prompt.
    /// 创建一份携带 OpenAI Responses continuation 游标与单条出站用户提示词的会话快照。
    fn build_test_snapshot() -> CoreConversationSnapshot {
        CoreConversationSnapshot {
            messages: vec![
                CoreConversationMessage {
                    id: String::from("user-covered"),
                    payload: CoreConversationPayload::UserText {
                        text: String::from("earlier"),
                        attachments: Vec::new(),
                    },
                },
                CoreConversationMessage {
                    id: String::from("assistant-boundary"),
                    payload: CoreConversationPayload::Assistant {
                        text: String::from("earlier response"),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("resp_prev")),
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
                CoreConversationMessage {
                    id: String::from("user-1"),
                    payload: CoreConversationPayload::UserText {
                        text: String::from("hello"),
                        attachments: Vec::new(),
                    },
                },
            ],
            continuation: CoreProviderContinuation {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-5")),
                last_provider_message_id: Some(String::from("resp_prev")),
                last_local_message_id: Some(String::from("assistant-boundary")),
                context_cache_epoch_id: None,
                context_anchor_revision: None,
                context_rewrite_fingerprint: Some(
                    build_test_controller_config(None, None).provider_rewrite_fingerprint(),
                ),
                updated_at: Some(String::from("123")),
            },
            ..CoreConversationSnapshot::default()
        }
    }

    /// Verifies the active OpenAI Responses adapter build path injects the previous response id into additional params.
    /// 验证当前 OpenAI Responses 适配器构建路径会把上一响应标识注入附加参数。
    #[tokio::test]
    async fn openai_responses_adapter_build_request_injects_previous_response_id() {
        // adapter is the production shared adapter bound to the OpenAI Responses transport spec.
        // adapter 是绑定到 OpenAI Responses 传输 spec 的生产共享适配器。
        let adapter = build_test_openai_responses_adapter();
        // snapshot provides the provider-scoped continuation cursor to be injected.
        // snapshot 提供待注入的 Provider 作用域 continuation 游标。
        let snapshot = build_test_snapshot();
        // config preserves user-supplied OpenAI Responses params while injecting continuation.
        // config 在注入 continuation 的同时保留用户提供的 OpenAI Responses 参数。
        let config = build_test_controller_config(
            Some(json!({
                "reasoning": { "effort": "high" }
            })),
            None,
        );

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("OpenAI Responses request should build");
        let params = request
            .snapshot
            .additional_params
            .expect("additional params should exist");

        assert_eq!(params["previous_response_id"], "resp_prev");
        assert_eq!(params["reasoning"]["effort"], "high");
    }

    /// Verifies append-only exact-message VMM anchors preserve OpenAI Responses continuation reuse.
    /// 验证追加式精确消息 VMM 锚点会保留 OpenAI Responses continuation 复用。
    #[tokio::test]
    async fn openai_responses_adapter_preserves_continuation_for_append_only_anchor() {
        let adapter = build_test_openai_responses_adapter();
        let mut snapshot = build_test_snapshot();
        // The current cursor explicitly proves zero-anchor coverage in the initial VMM epoch.
        // 当前游标显式证明初始 VMM 纪元中的零锚点覆盖状态。
        snapshot.continuation.context_cache_epoch_id = Some(String::from("initial"));
        snapshot.continuation.context_anchor_revision = Some(0);
        let config = build_test_controller_config(
            Some(json!({"reasoning": {"effort": "high"}})),
            Some(CoreProviderContextInjection {
                anchored_prefixes: vec![CoreProviderContextAnchor {
                    anchor_id: String::from("vmm-memory-1"),
                    target_message_id: String::from("user-1"),
                    prefix: String::from("<VMM_MEMORY>memory</VMM_MEMORY>"),
                }],
                mutation_kind: CoreProviderContextMutationKind::AppendOnly,
                cache_epoch_id: Some(String::from("initial")),
                anchor_revision: Some(1),
                ..CoreProviderContextInjection::default()
            }),
        );

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("append-only anchored request should build");
        let params = request
            .snapshot
            .additional_params
            .expect("continuation params should exist");

        assert_eq!(params["previous_response_id"], "resp_prev");
        assert!(
            params
                .get(FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM)
                .is_none()
        );
    }

    /// Verifies a cursor without explicit VMM epoch coverage cannot use the current append-only path.
    /// 验证缺少显式 VMM 纪元覆盖信息的游标不能使用当前追加式路径。
    #[tokio::test]
    async fn openai_responses_adapter_rejects_cursor_without_vmm_epoch_coverage() {
        // adapter exercises the production continuation proof for an unsupported historical cursor.
        // adapter 使用不受支持的历史游标覆盖生产 continuation 证明。
        let adapter = build_test_openai_responses_adapter();
        // snapshot deliberately omits the mandatory current-format VMM coverage fields.
        // snapshot 刻意省略当前格式强制要求的 VMM 覆盖字段。
        let mut snapshot = build_test_snapshot();
        snapshot.continuation.context_cache_epoch_id = None;
        snapshot.continuation.context_anchor_revision = None;
        // config introduces one otherwise-valid tail-only anchor in the initial epoch.
        // config 在初始纪元引入一条其他条件均有效且仅位于尾部的锚点。
        let config = build_test_controller_config(
            None,
            Some(CoreProviderContextInjection {
                anchored_prefixes: vec![CoreProviderContextAnchor {
                    anchor_id: String::from("vmm-memory-1"),
                    target_message_id: String::from("user-1"),
                    prefix: String::from("<VMM_MEMORY>memory</VMM_MEMORY>"),
                }],
                mutation_kind: CoreProviderContextMutationKind::AppendOnly,
                cache_epoch_id: Some(String::from("initial")),
                anchor_revision: Some(1),
                ..CoreProviderContextInjection::default()
            }),
        );

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("unsupported VMM cursor should force full replay");
        let params = request
            .snapshot
            .additional_params
            .expect("full replay marker should exist");

        assert_eq!(
            params[FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM],
            serde_json::Value::Bool(true)
        );
        assert!(params.get("previous_response_id").is_none());
    }

    /// Verifies an uncovered initial anchor cannot reuse a cursor when it targets already-covered history instead of the unsent tail.
    /// 验证未覆盖的初始锚点若指向已覆盖历史而不是未发送尾部，就不能复用游标。
    #[tokio::test]
    async fn openai_responses_adapter_rejects_uncovered_anchor_on_historical_message() {
        // adapter exercises the production continuation proof and exact-message request builder.
        // adapter 覆盖生产 continuation 证明与精确消息请求构建器。
        let adapter = build_test_openai_responses_adapter();
        // snapshot contains one covered user message, the exact cursor boundary, and the unsent user tail.
        // snapshot 包含一条已覆盖用户消息、精确游标边界和未发送用户尾部。
        let mut snapshot = build_test_snapshot();
        snapshot.continuation.context_cache_epoch_id = Some(String::from("initial"));
        snapshot.continuation.context_anchor_revision = Some(0);
        // config attempts to add revision one to user-covered, which the existing cursor already covers.
        // config 尝试把第一版锚点加入已被现有游标覆盖的 user-covered。
        let config = build_test_controller_config(
            None,
            Some(CoreProviderContextInjection {
                anchored_prefixes: vec![CoreProviderContextAnchor {
                    anchor_id: String::from("vmm-memory-1"),
                    target_message_id: String::from("user-covered"),
                    prefix: String::from("<VMM_MEMORY>memory</VMM_MEMORY>"),
                }],
                mutation_kind: CoreProviderContextMutationKind::AppendOnly,
                cache_epoch_id: Some(String::from("initial")),
                anchor_revision: Some(1),
                ..CoreProviderContextInjection::default()
            }),
        );

        let request = adapter.build_request(&snapshot, &config).await.expect(
            "unsafe historical anchor should force full replay instead of failing assembly",
        );
        let params = request
            .snapshot
            .additional_params
            .expect("suppression params should exist");

        assert_eq!(
            params[FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM],
            serde_json::Value::Bool(true)
        );
        assert!(params.get("previous_response_id").is_none());
    }

    /// Verifies caller-provided cursors cannot bypass provider/model scoping when no local cursor is reusable.
    /// 验证在没有可复用本地游标时，调用方提供的游标不能绕过 Provider/模型作用域限制。
    #[tokio::test]
    async fn openai_responses_adapter_strips_unproven_caller_cursor() {
        let adapter = build_test_openai_responses_adapter();
        // snapshot belongs to another provider, so its cursor is discarded by exact scoping.
        // snapshot 属于另一个 Provider，因此其游标会被精确作用域校验丢弃。
        let mut snapshot = build_test_snapshot();
        snapshot.continuation.provider_id = Some(String::from("other-provider"));
        // config includes an untrusted cursor that must not replace host continuation state.
        // config 包含不得替代宿主 continuation 状态的不可信游标。
        let config = build_test_controller_config(
            Some(json!({
                "previous_response_id": "resp_unproven",
                "reasoning": { "effort": "low" }
            })),
            None,
        );

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("request should build without an unproven cursor");
        let params = request
            .snapshot
            .additional_params
            .expect("unrelated params should remain");

        assert!(params.get("previous_response_id").is_none());
        assert_eq!(params["reasoning"]["effort"], "low");
    }

    /// Verifies a legacy provider cursor without an exact local canonical boundary forces one full replay.
    /// 验证缺少精确本地规范边界的旧版 Provider 游标会强制执行一次完整回放。
    #[tokio::test]
    async fn openai_responses_adapter_replays_when_local_boundary_is_missing() {
        let adapter = build_test_openai_responses_adapter();
        // snapshot retains an upstream id but removes the local boundary introduced by the new protocol.
        // snapshot 保留上游标识，但移除新协议引入的本地边界。
        let mut snapshot = build_test_snapshot();
        snapshot.continuation.last_local_message_id = None;
        let config = build_test_controller_config(None, None);

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("legacy continuation should fall back to a full replay");
        let params = request
            .snapshot
            .additional_params
            .expect("full-replay marker should exist");

        assert_eq!(
            params[FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM],
            serde_json::Value::Bool(true)
        );
        assert!(params.get("previous_response_id").is_none());
    }

    /// Verifies a compaction epoch mismatch suppresses OpenAI Responses continuation reuse.
    /// 验证压缩纪元不匹配会抑制 OpenAI Responses continuation 复用。
    #[tokio::test]
    async fn openai_responses_adapter_suppresses_continuation_for_epoch_reset() {
        let adapter = build_test_openai_responses_adapter();
        // snapshot records an earlier VMM epoch already covered by the upstream cursor.
        // snapshot 记录上游游标已经覆盖的早期 VMM 纪元。
        let mut snapshot = build_test_snapshot();
        snapshot.continuation.context_cache_epoch_id = Some(String::from("initial"));
        snapshot.continuation.context_anchor_revision = Some(1);
        let config = build_test_controller_config(
            None,
            Some(CoreProviderContextInjection {
                anchored_prefixes: vec![CoreProviderContextAnchor {
                    anchor_id: String::from("vmm-baseline-1"),
                    target_message_id: String::from("user-1"),
                    prefix: String::from("<VMM_MEMORY_BASELINE>memory</VMM_MEMORY_BASELINE>"),
                }],
                mutation_kind: CoreProviderContextMutationKind::AppendOnly,
                cache_epoch_id: Some(String::from("compaction-1")),
                anchor_revision: Some(1),
                ..CoreProviderContextInjection::default()
            }),
        );

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("epoch-reset request should build without continuation reuse");
        let params = request
            .snapshot
            .additional_params
            .expect("suppression params should exist");

        assert_eq!(
            params[FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM],
            serde_json::Value::Bool(true)
        );
        assert!(params.get("previous_response_id").is_none());
    }

    /// Verifies transient provider-facing injection suppresses OpenAI Responses continuation reuse.
    /// 验证临时 Provider 可见注入会抑制 OpenAI Responses continuation 复用。
    #[tokio::test]
    async fn openai_responses_adapter_build_request_suppresses_continuation_for_transient_injection()
     {
        // adapter is the production shared adapter bound to the OpenAI Responses transport spec.
        // adapter 是绑定到 OpenAI Responses 传输 spec 的生产共享适配器。
        let adapter = build_test_openai_responses_adapter();
        // snapshot provides a continuation cursor that must not be reused with transient context.
        // snapshot 提供在存在临时上下文时不得复用的 continuation 游标。
        let snapshot = build_test_snapshot();
        // config carries transient context injection, which marks the request as replay-unsafe.
        // config 携带临时上下文注入，用于标记该请求不适合回放复用。
        let config = build_test_controller_config(
            Some(json!({
                "reasoning": { "effort": "high" }
            })),
            Some(CoreProviderContextInjection {
                transient_prefix: String::from("<temporary>hidden</temporary>"),
                ..CoreProviderContextInjection::default()
            }),
        );

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("OpenAI Responses request should build");
        let params = request
            .snapshot
            .additional_params
            .expect("additional params should exist");

        assert_eq!(
            params[FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM],
            serde_json::Value::Bool(true)
        );
        assert!(params.get("previous_response_id").is_none());
        assert_eq!(params["reasoning"]["effort"], "high");
    }

    /// Verifies removing a previously covered persistent context layer suppresses OpenAI Responses continuation reuse.
    /// 验证移除先前已覆盖的持久上下文层会抑制 OpenAI Responses continuation 复用。
    #[tokio::test]
    async fn openai_responses_adapter_suppresses_continuation_after_persistent_layer_removal() {
        // adapter is the production request builder under test.
        // adapter 是本测试覆盖的生产请求构建器。
        let adapter = build_test_openai_responses_adapter();
        // old_config models the persistent layer covered by the previous provider response.
        // old_config 模拟上一 Provider 响应已经覆盖的持久上下文层。
        let old_config = build_test_controller_config(
            None,
            Some(CoreProviderContextInjection {
                persistent_blocks: vec![String::from("<persistent>old</persistent>")],
                ..CoreProviderContextInjection::default()
            }),
        );
        // snapshot proves the previous response covered that exact rewrite-sensitive input.
        // snapshot 证明上一响应覆盖了该精确的改写敏感输入。
        let mut snapshot = build_test_snapshot();
        snapshot.continuation.context_rewrite_fingerprint =
            Some(old_config.provider_rewrite_fingerprint());
        // config removes that layer entirely, so replaying the old cursor would reconstruct different input.
        // config 完全移除该层，因此复用旧游标会重建出不同的输入。
        let config = build_test_controller_config(None, None);

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("persistent-layer removal request should build");
        // params expose the host-only suppression marker consumed by the HTTP adapter.
        // params 暴露由 HTTP 适配器消费的宿主内部抑制标记。
        let params = request
            .snapshot
            .additional_params
            .expect("suppression params should exist");

        assert_eq!(
            params[FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM],
            serde_json::Value::Bool(true)
        );
        assert!(params.get("previous_response_id").is_none());
    }

    /// Verifies an unchanged persistent context fingerprint preserves OpenAI Responses continuation reuse.
    /// 验证未变化的持久上下文指纹会保留 OpenAI Responses continuation 复用。
    #[tokio::test]
    async fn openai_responses_adapter_preserves_continuation_for_stable_persistent_layer() {
        // adapter is the production request builder under test.
        // adapter 是本测试覆盖的生产请求构建器。
        let adapter = build_test_openai_responses_adapter();
        // config carries the identical fingerprint and therefore preserves replay equivalence.
        // config 携带相同指纹，因此保持回放输入等价。
        let config = build_test_controller_config(
            None,
            Some(CoreProviderContextInjection {
                persistent_blocks: vec![String::from("<persistent>stable</persistent>")],
                rewrite_fingerprint: Some(String::from("persistent-stable")),
                ..CoreProviderContextInjection::default()
            }),
        );
        // snapshot records the exact persistent system input already covered by the previous response.
        // snapshot 记录上一响应已经覆盖的精确持久系统输入。
        let mut snapshot = build_test_snapshot();
        snapshot.continuation.context_rewrite_fingerprint =
            Some(config.provider_rewrite_fingerprint());

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("stable persistent-layer request should build");
        // params must retain the provider cursor and omit the host-only suppression marker.
        // params 必须保留 Provider 游标且不包含宿主内部抑制标记。
        let params = request
            .snapshot
            .additional_params
            .expect("continuation params should exist");

        assert_eq!(params["previous_response_id"], "resp_prev");
        assert!(
            params
                .get(FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM)
                .is_none()
        );
    }

    /// Verifies changing a system prompt covered by the previous response suppresses OpenAI Responses continuation reuse.
    /// 验证修改上一响应已覆盖的系统提示词会抑制 OpenAI Responses continuation 复用。
    #[tokio::test]
    async fn openai_responses_adapter_suppresses_continuation_after_system_prompt_change() {
        // adapter is the production request builder under test.
        // adapter 是本测试覆盖的生产请求构建器。
        let adapter = build_test_openai_responses_adapter();
        // snapshot covers the baseline `system` prompt produced by the shared test helper.
        // snapshot 覆盖共享测试辅助函数产出的基线 `system` 提示词。
        let snapshot = build_test_snapshot();
        // config replaces both canonical prompt representations with the new system instruction.
        // config 用新的系统指令同步替换两种规范提示词表示。
        let mut config = build_test_controller_config(None, None);
        config.system_prompt = String::from("changed-system");
        config.prompt_layers =
            CoreProviderPromptLayers::from_system_prompt(String::from("changed-system"));

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("system-prompt change request should build");
        // params expose the host-only suppression marker and must not carry the stale cursor.
        // params 暴露宿主内部抑制标记，且不得携带陈旧游标。
        let params = request
            .snapshot
            .additional_params
            .expect("suppression params should exist");

        assert_eq!(
            params[FORCE_FULL_OPENAI_RESPONSES_REPLAY_PARAM],
            serde_json::Value::Bool(true)
        );
        assert!(params.get("previous_response_id").is_none());
    }
}
