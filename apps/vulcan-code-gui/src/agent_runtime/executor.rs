//! Execution-preparation models and real provider execution helpers for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备模型与真实 Provider 执行辅助逻辑。

#[cfg(test)]
mod tests {
    use crate::agent_runtime::config::{RuntimeAgentDefinition, RuntimeAgentKind};
    use crate::agent_runtime::execution_models::ExecutionPreparation;
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::agent_runtime::provider_protocol_bridge::{
        attach_tool_name_to_runtime_metadata, build_host_tool_result_id,
        inject_previous_response_id_into_additional_params,
    };
    use crate::agent_runtime::provider_request_bridge_conversation::{
        build_core_conversation_snapshot, core_snapshot_to_session_conversation_state,
    };
    use crate::agent_runtime::provider_request_bridge_messages::core_messages_to_provider_visible_history;
    use crate::agent_runtime::provider_request_preview::build_provider_request_preview_markdown;
    use crate::agent_runtime::provider_request_snapshot_builder::build_core_provider_request_snapshot;
    use crate::agent_runtime::provider_request_turn_snapshot::build_openai_compatible_turn_request_snapshot;
    use crate::agent_runtime::provider_runtime_context::{
        build_runtime_system_preamble, resolve_runtime_max_turns,
    };
    use crate::agent_runtime::provider_runtime_dispatch::{
        DEFAULT_RUNTIME_MAX_TURNS_LIMIT, resolve_runtime_default_max_turns_limit,
    };
    use crate::agent_runtime::provider_stream_projection::{
        SyntheticThinkingPreambleState, classify_synthetic_thinking_preamble,
        normalize_streamed_thinking_output,
    };
    use crate::conversation_protocol::{
        SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
        SessionConversationState, SessionConversationToolCall, SessionProviderContinuationState,
    };
    use crate::model::{SessionMessageKind, SessionMessageRecordKind};
    use crate::provider_config::ProviderRuntimeKind;
    use serde_json::json;
    use std::collections::BTreeMap;
    use vulcan_core::types::{
        ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
        ConversationToolCall as CoreConversationToolCall,
        ProviderContinuation as CoreProviderContinuation,
        ProviderRequestSnapshot as CoreProviderRequestSnapshot,
        ProviderRequestToolSchema as CoreProviderRequestToolSchema,
    };
    use vulcan_tools_manager::{
        ResolvedToolDefinition, VulcanToolDescriptor, VulcanToolSourceKind,
    };

    /// Resolves the host-visible tool-call identifier used by transcript cards and tool-result correlation.
    /// 解析供转录卡片与工具结果关联使用的宿主可见工具调用标识。
    fn resolve_host_visible_tool_call_id(tool_call: &CoreConversationToolCall) -> String {
        tool_call
            .tool_call_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| tool_call.id.clone())
    }

    /// Verifies synthetic reasoning preamble text is removed before streamed thinking content enters transcript storage.
    /// 验证合成推理前导语会在流式思考内容进入转录存储之前被移除。
    #[test]
    fn streamed_thinking_preamble_is_removed_before_storage() {
        let chunk_one = "Here's a think";
        let chunk_two = "ing process:\n\n1. 分析问题";

        assert_eq!(normalize_streamed_thinking_output(chunk_one), "");
        assert_eq!(
            normalize_streamed_thinking_output(&format!("{chunk_one}{chunk_two}")),
            "1. 分析问题"
        );
    }

    /// Verifies normal reasoning text is preserved verbatim instead of being mistaken for the synthetic preamble.
    /// 验证正常推理文本会被原样保留，而不会被误判成合成前导语。
    #[test]
    fn normal_reasoning_text_is_not_mistaken_for_synthetic_preamble() {
        let raw_output = "Hello reasoning\n- 实际思考内容";

        assert!(matches!(
            classify_synthetic_thinking_preamble(raw_output),
            SyntheticThinkingPreambleState::Rejected
        ));
        assert_eq!(normalize_streamed_thinking_output(raw_output), raw_output);
    }

    /// Verifies the last-request preview contains the full system prompt, user prompt, and tool schema sections.
    /// 验证最后一次请求预览会包含完整系统提示词、用户提示词与工具 schema 分节。
    #[test]
    fn provider_request_preview_contains_core_sections() {
        let preparation = ExecutionPreparation {
            session_id: uuid::Uuid::nil(),
            session_title: String::from("测试会话"),
            agent_id: String::from("default-agent"),
            agent_name: String::from("默认代理"),
            agent_kind: String::from("内置"),
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("openai"),
                provider_name: String::from("OpenAI"),
                runtime_kind: ProviderRuntimeKind::OpenAi,
                runtime_label: String::from("OpenAI"),
                model_id: String::from("gpt-test"),
                model_name: String::from("GPT Test"),
                openrouter_endpoint_route: None,
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: json!({ "reasoning": { "effort": "high" } }),
                reasoning: Default::default(),
                capabilities: Default::default(),
                interleaved_reasoning: None,
            },
            prompt_preview: String::from("请分析这段代码"),
            workspace_path: String::from(r"D:\workspace"),
            conversation_attachments: Vec::new(),
            context_history_count: 2,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            skill_names: vec![String::from("codekit")],
            tool_descriptors: vec![VulcanToolDescriptor {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source: VulcanToolSourceKind::Native,
            }],
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            context_budget: None,
            context_assembly: None,
            context_baseline: None,
        };
        let request_snapshot = CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::from("system prompt body"),
            prompt: String::from("effective prompt body"),
            history: vec![
                CoreConversationMessage {
                    id: String::from("history-user-1"),
                    payload: CoreConversationPayload::UserText {
                        text: String::from("之前的用户问题"),
                        attachments: Vec::new(),
                    },
                },
                CoreConversationMessage {
                    id: String::from("history-assistant-1"),
                    payload: CoreConversationPayload::Assistant {
                        text: String::from("之前的助手回答"),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("resp_1")),
                        provider_message_state: None,
                        tool_calls: Vec::new(),
                        usage: None,
                    },
                },
            ],
            temperature: Some(0.2),
            max_turns: DEFAULT_RUNTIME_MAX_TURNS_LIMIT,
            tool_schemas: vec![CoreProviderRequestToolSchema {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source_label: String::from("原生工具"),
                parameters_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
            additional_params: Some(json!({ "reasoning": { "effort": "high" } })),
            request_headers: Some(json!({
                "user-agent": "vulcancode/test",
                "authorization": "Bearer masked"
            })),
            continuation: CoreProviderContinuation {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp_prev")),
                updated_at: Some(String::from("123")),
            },
        };

        let preview = build_provider_request_preview_markdown(&preparation, &request_snapshot);

        assert!(preview.contains("# 最后一次真实 API 请求快照"));
        assert!(preview.contains("## System Preamble"));
        assert!(preview.contains("system prompt body"));
        assert!(preview.contains("## Continuation"));
        assert!(preview.contains("resp_prev"));
        assert!(preview.contains("## Outgoing History"));
        assert!(preview.contains("之前的用户问题"));
        assert!(preview.contains("## Effective Outgoing Prompt"));
        assert!(preview.contains("effective prompt body"));
        assert!(preview.contains("### `read_file`"));
        assert!(preview.contains("\"path\""));
        assert!(preview.contains("## Request Headers"));
        assert!(preview.contains("vulcancode/test"));
        assert!(preview.contains("\"effort\": \"high\""));
        assert!(preview.contains("- 最大轮次：不限制"));
        assert!(!preview.contains("之前的推理摘要"));
    }

    /// Verifies `vulcan-core` request snapshots preserve continuation, prompt, and tool-schema details.
    /// 验证 `vulcan-core` 请求快照会保留 continuation、提示词与工具 schema 细节。
    #[test]
    fn core_request_snapshot_preserves_continuation_and_tool_details() {
        let preparation = ExecutionPreparation {
            session_id: uuid::Uuid::nil(),
            session_title: String::from("测试会话"),
            agent_id: String::from("default-agent"),
            agent_name: String::from("默认代理"),
            agent_kind: String::from("内置"),
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("openai"),
                provider_name: String::from("OpenAI"),
                runtime_kind: ProviderRuntimeKind::OpenAi,
                runtime_label: String::from("OpenAI"),
                model_id: String::from("gpt-test"),
                model_name: String::from("GPT Test"),
                openrouter_endpoint_route: None,
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: Some(String::from("vulcancode/test")),
                extra_headers: BTreeMap::from([(
                    String::from("x-trace-id"),
                    String::from("trace-1"),
                )]),
                extra_body: json!({ "reasoning": { "effort": "medium" } }),
                reasoning: Default::default(),
                capabilities: Default::default(),
                interleaved_reasoning: None,
            },
            prompt_preview: String::from("请分析这段代码"),
            workspace_path: String::from(r"D:\workspace"),
            conversation_attachments: Vec::new(),
            context_history_count: 1,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            skill_names: vec![String::from("codekit")],
            tool_descriptors: vec![VulcanToolDescriptor {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source: VulcanToolSourceKind::Native,
            }],
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            context_budget: None,
            context_assembly: None,
            context_baseline: None,
        };
        let conversation_state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("history-user-1"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("之前的用户问题"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("history-assistant-1"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("之前的助手回答"),
                        reasoning: String::from("之前的推理摘要"),
                        provider_message_id: Some(String::from("resp_1")),
                        provider_message_state: None,
                        tool_calls: Vec::new(),
                        usage: None,
                    },
                },
            ],
            records: Vec::new(),
            continuation: crate::conversation_protocol::SessionProviderContinuationState {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp_prev")),
                updated_at: Some(String::from("123")),
            },
        };
        let resolved_tools = vec![ResolvedToolDefinition {
            descriptor: VulcanToolDescriptor {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            invoke: std::sync::Arc::new(|_, _| unreachable!("test preview does not invoke tools")),
        }];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        let snapshot = runtime
            .block_on(build_core_provider_request_snapshot(
                &preparation,
                &conversation_state,
                "system prompt body",
                "effective prompt body",
                &resolved_tools,
                Some(5),
            ))
            .expect("request snapshot should build");

        assert_eq!(snapshot.prompt, "effective prompt body");
        assert_eq!(snapshot.max_turns, 5);
        assert_eq!(snapshot.tool_schemas.len(), 1);
        assert_eq!(
            snapshot.continuation.last_provider_message_id.as_deref(),
            Some("resp_prev")
        );
        let Some(CoreConversationMessage {
            payload: CoreConversationPayload::Assistant { reasoning, .. },
            ..
        }) = snapshot.history.get(1)
        else {
            panic!("assistant history item should exist");
        };
        assert!(reasoning.is_empty());
        assert_eq!(
            snapshot.request_headers,
            Some(json!({
                "user-agent": "vulcancode/test",
                "x-trace-id": "trace-1"
            }))
        );
    }

    /// Verifies `vulcan-core` snapshot round-trips preserve pre-existing durable records, metadata, and continuation state.
    /// 验证 `vulcan-core` 快照往返过程会保留既有持久化记录、元数据与 continuation 状态。
    #[test]
    fn core_snapshot_round_trip_preserves_records_and_continuation() {
        let conversation_state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("user-1"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("请分析这个项目"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("assistant-1"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("我先读取配置文件。"),
                        reasoning: String::from("先定位入口再读取配置。"),
                        provider_message_id: Some(String::from("resp-1")),
                        provider_message_state: None,
                        tool_calls: vec![SessionConversationToolCall {
                            id: String::from("tool-1"),
                            tool_call_id: Some(String::from("call-1")),
                            tool_name: String::from("read_file"),
                            arguments: json!({ "path": "Cargo.toml" }),
                        }],
                        usage: None,
                    },
                },
            ],
            records: vec![
                SessionConversationRecord {
                    id: String::from("record-user"),
                    kind: SessionMessageKind::User,
                    record_kind: SessionMessageRecordKind::Transcript,
                    visible: true,
                    title: String::from("用户输入"),
                    body: String::from("请分析这个项目"),
                    metadata: None,
                    created_at: String::from("100"),
                },
                SessionConversationRecord {
                    id: String::from("record-tool-call"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::ToolCallOutput,
                    visible: true,
                    title: String::from("调用 read_file 工具"),
                    body: String::from("{\"path\":\"Cargo.toml\"}"),
                    metadata: Some(json!({
                        "tool_call_id": "call-1",
                        "tool_name": "read_file",
                        "arguments": { "path": "Cargo.toml" }
                    })),
                    created_at: String::from("101"),
                },
                SessionConversationRecord {
                    id: String::from("record-tool-result"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::ToolResultOutput,
                    visible: true,
                    title: String::from("调用 read_file 工具"),
                    body: String::from("name = \"vulcan-code\""),
                    metadata: Some(json!({
                        "tool_call_id": "call-1",
                        "tool_name": "read_file",
                        "kind": "file_read"
                    })),
                    created_at: String::from("102"),
                },
            ],
            continuation: SessionProviderContinuationState {
                provider_id: Some(String::from("openai-compatible")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp-1")),
                updated_at: Some(String::from("123")),
            },
        };

        let snapshot = build_core_conversation_snapshot(&conversation_state);
        let rebuilt_state = core_snapshot_to_session_conversation_state(&snapshot);

        assert_eq!(rebuilt_state.records.len(), 3);
        assert_eq!(
            rebuilt_state.records[1].record_kind,
            SessionMessageRecordKind::ToolCallOutput
        );
        assert_eq!(
            rebuilt_state.records[2].record_kind,
            SessionMessageRecordKind::ToolResultOutput
        );
        assert_eq!(
            rebuilt_state.records[2].metadata,
            conversation_state.records[2].metadata
        );
        assert_eq!(
            rebuilt_state
                .continuation
                .last_provider_message_id
                .as_deref(),
            Some("resp-1")
        );
    }

    /// Verifies provider-visible history preserves tool calls while still excluding reasoning replay from assistant turns.
    /// 验证 Provider 可见历史会保留工具调用，同时仍然排除助手回合中的思考回放。
    #[test]
    fn provider_visible_history_preserves_tool_calls_without_reasoning_replay() {
        let messages = vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("我先读取文件。"),
                reasoning: String::from("这段思考不应回放。"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: vec![vulcan_core::types::ConversationToolCall {
                    id: String::from("tool-1"),
                    tool_call_id: Some(String::from("call-1")),
                    tool_name: String::from("read_file"),
                    arguments: json!({ "path": "src/main.rs" }),
                }],
                usage: None,
            },
        }];

        let history = core_messages_to_provider_visible_history(&messages);
        let [message] = history.as_slice() else {
            panic!("history should contain one assistant message");
        };
        let CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            provider_message_state: _,
            tool_calls,
            usage: _,
        } = &message.payload
        else {
            panic!("history should contain one assistant payload");
        };

        assert_eq!(provider_message_id.as_deref(), Some("resp-1"));
        assert_eq!(text, "我先读取文件。");
        assert!(reasoning.is_empty());
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "tool-1");
        assert_eq!(tool_calls[0].tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(tool_calls[0].tool_name, "read_file");
        assert_eq!(tool_calls[0].arguments, json!({ "path": "src/main.rs" }));
    }

    /// Verifies the runtime leaves multi-turn providers unlimited when no explicit max-turn override exists.
    /// 验证当未显式提供最大轮次覆盖值时，运行时会让多轮 Provider 保持不限制状态。
    #[test]
    fn runtime_max_turns_default_to_unlimited_without_override() {
        assert_eq!(resolve_runtime_max_turns(None), None);
        assert_eq!(
            resolve_runtime_default_max_turns_limit(None),
            DEFAULT_RUNTIME_MAX_TURNS_LIMIT
        );

        let worker_agent = RuntimeAgentDefinition {
            id: String::from("worker"),
            name: String::from("worker"),
            kind: RuntimeAgentKind::Worker,
            description: String::new(),
            system_prompt: String::new(),
            skill_ids: Vec::new(),
            native_tools: Vec::new(),
            mcp_tools: Vec::new(),
            delegates: Vec::new(),
            tool_policy: None,
            routes: Vec::new(),
            fallback_delegate: None,
            model: None,
            temperature: None,
            max_turns: None,
        };
        assert_eq!(resolve_runtime_max_turns(Some(&worker_agent)), None);
        assert_eq!(
            resolve_runtime_default_max_turns_limit(Some(&worker_agent)),
            DEFAULT_RUNTIME_MAX_TURNS_LIMIT
        );
    }

    /// Verifies runtime system preamble no longer embeds agent-authored prompt text or duplicated tool catalogs.
    /// 验证运行时系统提示词不再嵌入代理自定义提示词文本与重复工具目录。
    #[test]
    fn runtime_system_preamble_uses_base_prompt_without_tool_catalog_text() {
        let preparation = ExecutionPreparation {
            session_id: uuid::Uuid::nil(),
            session_title: String::from("测试会话"),
            agent_id: String::from("default-agent"),
            agent_name: String::from("默认代理"),
            agent_kind: String::from("内置"),
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("openai"),
                provider_name: String::from("OpenAI"),
                runtime_kind: ProviderRuntimeKind::OpenAi,
                runtime_label: String::from("OpenAI"),
                model_id: String::from("gpt-test"),
                model_name: String::from("GPT Test"),
                openrouter_endpoint_route: None,
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: json!({}),
                reasoning: Default::default(),
                capabilities: Default::default(),
                interleaved_reasoning: None,
            },
            prompt_preview: String::from("请分析这段代码"),
            workspace_path: String::from(r"D:\workspace"),
            conversation_attachments: Vec::new(),
            context_history_count: 2,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            skill_names: vec![String::from("codekit")],
            tool_descriptors: vec![VulcanToolDescriptor {
                id: String::from("read_file"),
                name: String::from("read_file"),
                description: String::from("Read one file"),
                source: VulcanToolSourceKind::Native,
            }],
            delegate_agents: vec![String::from("delegate-a")],
            enabled_mcp_servers: vec![String::from("demo-server")],
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            context_budget: None,
            context_assembly: None,
            context_baseline: None,
        };
        let preamble = build_runtime_system_preamble(&preparation);

        assert!(!preamble.contains("AGENT OVERRIDE PROMPT"));
        assert!(!preamble.contains("原生工具已真实注册"));
        assert!(!preamble.contains("LuaSkills 工具目录"));
        assert!(!preamble.contains("MCP 工具已真实注册"));
        assert!(!preamble.contains("可委派代理"));
        assert!(preamble.contains(r"Current workspace: D:\workspace"));
        assert!(preamble.contains("Current execution mode: Build"));
    }

    /// Verifies host-side tool-result correlation prefers provider call ids while still generating stable synthetic tool-result ids.
    /// 验证宿主侧工具结果关联会优先使用 Provider call id，同时仍生成稳定的合成工具结果标识。
    #[test]
    fn host_tool_result_correlation_prefers_provider_call_id() {
        let tool_call = CoreConversationToolCall {
            id: String::from("provider-tool-id"),
            tool_call_id: Some(String::from("provider-call-id")),
            tool_name: String::from("read_file"),
            arguments: json!({ "path": "src/main.rs" }),
        };

        let resolved_tool_call_id = resolve_host_visible_tool_call_id(&tool_call);

        assert_eq!(resolved_tool_call_id, "provider-call-id");
        assert_eq!(
            build_host_tool_result_id(&resolved_tool_call_id),
            "tool-result-provider-call-id"
        );
    }

    /// Verifies tool-finished metadata always keeps the tool name even when host metadata already carries other structured payloads.
    /// 验证工具完成元数据即使已经携带其他结构化载荷，也会始终保留工具名称字段。
    #[test]
    fn tool_finished_metadata_keeps_tool_name_for_display_fallback() {
        let metadata = attach_tool_name_to_runtime_metadata(
            Some(json!({
                "tool_call_id": "call-1",
                "tool_result_id": "result-1",
                "kind": "change_set"
            })),
            "write_file",
        )
        .expect("metadata should exist");

        assert_eq!(metadata["tool_call_id"], "call-1");
        assert_eq!(metadata["tool_result_id"], "result-1");
        assert_eq!(metadata["kind"], "change_set");
        assert_eq!(metadata["tool_name"], "write_file");
    }

    /// Verifies OpenAI Responses continuation injection preserves existing additional params while adding one missing `previous_response_id`.
    /// 验证 OpenAI Responses 的 continuation 注入会在补入缺失 `previous_response_id` 的同时保留既有附加参数。
    #[test]
    fn openai_responses_continuation_injection_preserves_existing_additional_params() {
        let continuation = CoreProviderContinuation {
            provider_id: Some(String::from("openai")),
            model_id: Some(String::from("gpt-5")),
            last_provider_message_id: Some(String::from("resp_prev")),
            updated_at: Some(String::from("123")),
        };

        let merged = inject_previous_response_id_into_additional_params(
            Some(json!({
                "reasoning": { "effort": "high" }
            })),
            &continuation,
        )
        .expect("merged params should exist");
        assert_eq!(merged["reasoning"]["effort"], "high");
        assert_eq!(merged["previous_response_id"], "resp_prev");

        let preserved = inject_previous_response_id_into_additional_params(
            Some(json!({
                "previous_response_id": "resp_existing",
                "reasoning": { "effort": "medium" }
            })),
            &continuation,
        )
        .expect("preserved params should exist");
        assert_eq!(preserved["previous_response_id"], "resp_existing");
        assert_eq!(preserved["reasoning"]["effort"], "medium");
    }

    /// Verifies host-controlled OpenAI-compatible snapshots treat tool-result messages as the real outgoing prompt of the next turn.
    /// 验证宿主控制的 OpenAI 兼容请求快照会把工具结果消息视作下一轮真实出站提示词。
    #[test]
    fn openai_compatible_turn_snapshot_uses_tool_result_as_outgoing_prompt() {
        let headers = reqwest::header::HeaderMap::new();
        let history = vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("请调用工具处理文件。"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("assistant-1")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                usage: None,
            },
        }];
        let prompt = CoreConversationMessage {
            id: String::from("result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-1"),
                tool_call_id: Some(String::from("call-1")),
                text: String::from("文件修改完成"),
            },
        };
        let continuation = CoreProviderContinuation {
            provider_id: Some(String::from("groq")),
            model_id: Some(String::from("llama-4")),
            last_provider_message_id: Some(String::from("assistant-1")),
            updated_at: Some(String::from("123")),
        };

        let snapshot = build_openai_compatible_turn_request_snapshot(
            "groq",
            "llama-4",
            "system prompt body",
            &continuation,
            &history,
            &prompt,
            Some(0.3),
            6,
            &[],
            &headers,
            None,
        );

        assert_eq!(snapshot.provider_id, "groq");
        assert_eq!(snapshot.model_id, "llama-4");
        assert_eq!(snapshot.prompt, "文件修改完成");
        assert_eq!(snapshot.history.len(), 1);
        assert_eq!(
            snapshot.continuation.last_provider_message_id.as_deref(),
            Some("assistant-1")
        );
    }

    /// Verifies host-controlled OpenAI-compatible snapshots drop a foreign continuation cursor when the session switches to another provider family mid-history.
    /// 验证当会话在中途切换到另一家 Provider 时，宿主控制的 OpenAI 兼容请求快照会丢弃外部 continuation 游标。
    #[test]
    fn openai_compatible_turn_snapshot_drops_foreign_provider_continuation() {
        let headers = reqwest::header::HeaderMap::new();
        let history = vec![CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("之前由其他协议完成了一轮调用。"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("foreign-response-1")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                usage: None,
            },
        }];
        let prompt = CoreConversationMessage {
            id: String::from("user-2"),
            payload: CoreConversationPayload::UserText {
                text: String::from("现在切到 Anthropic 继续。"),
                attachments: Vec::new(),
            },
        };
        let foreign_continuation = CoreProviderContinuation {
            provider_id: Some(String::from("openai-responses")),
            model_id: Some(String::from("gpt-4.1")),
            last_provider_message_id: Some(String::from("resp_prev")),
            updated_at: Some(String::from("123")),
        };

        let snapshot = build_openai_compatible_turn_request_snapshot(
            "anthropic",
            "claude-sonnet-4-6",
            "system prompt body",
            &foreign_continuation,
            &history,
            &prompt,
            Some(0.3),
            6,
            &[],
            &headers,
            None,
        );

        assert!(snapshot.continuation.provider_id.is_none());
        assert!(snapshot.continuation.model_id.is_none());
        assert!(snapshot.continuation.last_provider_message_id.is_none());
    }
}
