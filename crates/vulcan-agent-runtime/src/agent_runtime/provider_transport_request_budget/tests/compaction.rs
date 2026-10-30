//! Regression coverage for normalized usage and the final V1 compaction admission boundary.
//! 归一化用量及最终 V1 压缩准入边界的回归覆盖。

use super::*;
use serde_json::json;
use vulcan_llm_core::UpstreamProtocol;
use vulcan_llm_core::{UsageAccumulator, UsageRecordKind, standard_usage_profiles};

/// Builds a frozen guard for `profile`, `line` and host eligibility `armed`; returns no live transport.
/// 根据 `profile`、`line` 及宿主资格 `armed` 构建冻结守卫，不返回实时传输。
fn compaction_guard(profile: &str, line: u64, armed: bool) -> ProviderRequestBudgetGuard {
    ProviderRequestBudgetGuard::new_v1(
        "test-provider",
        "test-model",
        profile,
        "sha256:test-route",
        ModelCapabilities {
            context_limit: Some(1_000_000),
            ..Default::default()
        },
        ProviderRuntimeReasoningResolution::default(),
        Some(ContextCompactionPolicy {
            window: vulcan_context::ContextWindowPolicy {
                auto_compact_token_limit: Some(line),
                ..Default::default()
            },
            ..Default::default()
        }),
        None,
        None,
    )
    .with_turn_compaction_line(armed)
}

/// Extracts `records` using production profiles for `protocol`; returns the production controller usage.
/// 通过 `protocol` 的生产 Profile 提取 `records`，返回生产控制器用量。
fn normalized_usage(
    protocol: UpstreamProtocol,
    records: &[(UsageRecordKind, serde_json::Value)],
) -> ProviderUsageSnapshot {
    // The same accumulator combines Anthropic start/delta records and freezes complete responses.
    // 同一累计器合并 Anthropic 起始、增量记录，并冻结完整响应。
    let mut accumulator = UsageAccumulator::default();
    for (kind, record) in records {
        // Exactly one declared profile must own each wire record.
        // 每条线路记录必须恰好由一个声明的 Profile 拥有。
        let mut matches = 0;
        for profile in standard_usage_profiles(protocol) {
            if let Some(observation) = profile.extract(*kind, record).unwrap() {
                accumulator.apply(None, observation).unwrap();
                matches += 1;
            }
        }
        assert_eq!(
            matches, 1,
            "wire fixture must match exactly one current profile: {protocol:?}"
        );
    }
    crate::agent_runtime::provider_transport_vulcan::public_usage_to_core(accumulator.usage())
}

/// Verifies real wire accounting reaches the same compaction line on every protocol, without double counting.
/// 验证每种协议的真实线路记账到达相同压缩线，且不重复计数。
/// Takes no arguments; returns no value and never contacts a provider.
/// 无参数、无返回值，绝不联系供应商。
#[test]
fn normalized_protocol_usage_compacts_cache_writes_and_counts_cache_reads_once() {
    // All fixtures describe 449277 input and 18628 output tokens, including the reported cache-write case.
    // 所有夹具描述 449277 输入与 18628 输出 Token，包含报告中的缓存写入场景。
    let cases = [
        (
            UpstreamProtocol::AnthropicMessages,
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
            vec![
                (
                    UsageRecordKind::SseEvent,
                    json!({"type":"message_start","message":{"usage":{"input_tokens":4,"cache_read_input_tokens":0,"cache_creation_input_tokens":449273}}}),
                ),
                (
                    UsageRecordKind::SseEvent,
                    json!({"type":"message_delta","usage":{"output_tokens":18628}}),
                ),
            ],
        ),
        (
            UpstreamProtocol::AnthropicMessages,
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
            vec![(
                UsageRecordKind::JsonBody,
                json!({"usage":{"input_tokens":4,"cache_read_input_tokens":273,"cache_creation":{"ephemeral_5m_input_tokens":400000,"ephemeral_1h_input_tokens":49000},"output_tokens":18628}}),
            )],
        ),
        (
            UpstreamProtocol::OpenAiResponses,
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            vec![(
                UsageRecordKind::JsonBody,
                json!({"usage":{"input_tokens":449277,"input_tokens_details":{"cached_tokens":449273},"output_tokens":18628,"total_tokens":467905}}),
            )],
        ),
        (
            UpstreamProtocol::OpenAiChatCompletions,
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            vec![(
                UsageRecordKind::SseEvent,
                json!({"usage":{"prompt_tokens":449277,"prompt_tokens_details":{"cached_tokens":449273},"completion_tokens":18628,"total_tokens":467905}}),
            )],
        ),
        (
            UpstreamProtocol::GeminiGenerateContent,
            vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
            vec![(
                UsageRecordKind::JsonBody,
                json!({"usageMetadata":{"promptTokenCount":449277,"cachedContentTokenCount":449273,"candidatesTokenCount":18628,"totalTokenCount":467905}}),
            )],
        ),
        (
            UpstreamProtocol::GoogleCodeAssist,
            vulcan_llm_core::GOOGLE_CODE_ASSIST_PROFILE_ID,
            vec![(
                UsageRecordKind::JsonBody,
                json!({"response":{"usageMetadata":{"promptTokenCount":449277,"cachedContentTokenCount":449273,"candidatesTokenCount":18628,"totalTokenCount":467905}}}),
            )],
        ),
        (
            UpstreamProtocol::GeminiInteractions,
            vulcan_llm_core::GEMINI_INTERACTIONS_PROFILE_ID,
            vec![(
                UsageRecordKind::SseEvent,
                json!({"event_type":"interaction.completed","interaction":{"usage":{"total_input_tokens":449277,"total_cached_tokens":449273,"total_output_tokens":18628,"total_tokens":467905}}}),
            )],
        ),
    ];
    for (protocol, profile, records) in cases {
        // Production conversion, rather than handcrafted normalized counters, owns this regression.
        // 此回归由生产转换负责，而非手工拼装的归一化计数。
        let usage = normalized_usage(protocol, &records);
        assert_eq!(usage.input_tokens, Some(449277), "{protocol:?}");
        // The next input includes the previous output exactly once.
        // 下一次输入恰好包含一次上一响应输出。
        let response = CoreProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            usage: Some(usage),
            tool_calls: Vec::new(),
            continuation: None,
        };
        for (line, expected) in [(335700, true), (467905, true), (467906, false)] {
            // Every guard starts with no stale usage inherited from another route.
            // 每个守卫启动时均不继承其他路由的陈旧用量。
            let guard = compaction_guard(profile, line, true);
            guard.observe_turn_response(&response);
            assert_eq!(
                matches!(
                    guard.validate_turn_compaction_line(&request_snapshot(Vec::new())),
                    Err(VulcanCoreError::ContextCompactionRequired { .. })
                ),
                expected,
                "{protocol:?} line={line}"
            );
        }
    }
}

/// Verifies calibrated final accounting catches a first-request miss while disabled/attempted runs remain admissible.
/// 验证校准后的最终记账捕获首次请求漏检，而关闭或已尝试压缩的执行仍可准入。
/// Takes no arguments; returns no value and exercises both exact and prefix anchors without I/O.
/// 无参数、无返回值，无输入输出地覆盖精确锚点与前缀锚点。
#[test]
fn final_v1_budget_compacts_after_low_precheck_and_respects_host_eligibility() {
    // The tiny character estimate intentionally disagrees with real provider usage.
    // 微小字符估算故意与真实供应商用量不符。
    let request = request_snapshot(Vec::new());
    // The exact generation and request evidence qualify this provider anchor.
    // 精确代际及请求证据使此供应商锚点符合使用条件。
    let projection = v1_projection(4);
    // A first physical request after reopening has a durable anchor but no latest-turn usage.
    // 重新打开后的首次物理请求持有可靠锚点，但没有本回合最新用量。
    let anchor = v1_budget_anchor(&request, projection.clone(), 449277);
    for armed in [true, false] {
        // Only eligible host runs may request automatic compaction at the final gate.
        // 只有具备资格的宿主执行能在最终闸门请求自动压缩。
        let guard = compaction_guard(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID, 335700, armed);
        assert!(
            !guard
                .build_budget_snapshot(&request)
                .unwrap()
                .should_compact()
        );
        guard.validate_turn_compaction_line(&request).unwrap();
        // Final evidence must outrank the early estimate without altering the hard limit.
        // 最终证据必须优先于早期估算，且不改变硬上限。
        let decision = guard
            .build_v1_budget_decision(&request, &projection, Some(&anchor))
            .unwrap();
        assert!(decision.context_budget.should_compact());
        assert_eq!(decision.host_compaction_required, armed);
        assert_eq!(decision.context_budget.active_context_tokens, 449277);
        assert_eq!(decision.evidence, anchor.evidence);
    }
    // A new prompt after stopping extends the canonical prefix instead of matching the old request exactly.
    // 停止后的新提示词扩展规范前缀，而非与旧请求精确匹配。
    let mut previous = request_snapshot(vec![ConversationMessage {
        id: "old-user".into(),
        payload: ConversationPayload::UserText {
            text: "previous prompt".into(),
            attachments: Vec::new(),
        },
    }]);
    previous.prompt_in_history = true;
    previous.prompt = "previous prompt".into();
    // This anchor belongs to the last completed request before the new user input.
    // 此锚点属于新用户输入前最后一次完成的请求。
    let anchor = v1_budget_anchor(&previous, projection.clone(), 449277);
    previous.history.push(ConversationMessage {
        id: "new-user".into(),
        payload: ConversationPayload::UserText {
            text: "new prompt".into(),
            attachments: Vec::new(),
        },
    });
    previous.prompt = "new prompt".into();
    // Final accounting must retain the valid prefix anchor and add the new prompt estimate.
    // 最终记账必须保留有效前缀锚点，并加上新提示词估算。
    let decision = compaction_guard(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID, 335700, true)
        .build_v1_budget_decision(&previous, &projection, Some(&anchor))
        .unwrap();
    assert!(decision.host_compaction_required);
    assert_eq!(
        decision.context_budget.accounting_source,
        ContextBudgetAccountingSource::ProviderAnchorPlusCharacterDelta
    );
    assert!(decision.context_budget.active_context_tokens > 449277);
}

/// Verifies switching protocol, route, generation or request shape cannot reuse a foreign usage anchor.
/// 验证切换协议、路由、代际或请求形状不能复用其他身份的用量锚点。
/// Takes no arguments; returns no value and checks both directions for every protocol pair.
/// 无参数、无返回值，检查每对协议的两个方向。
#[test]
fn final_compaction_keeps_protocol_route_and_generation_anchor_isolation() {
    // Protocol fingerprints are the canonical request authority, independent of provider display names.
    // 协议指纹是规范请求权威，与供应商显示名称无关。
    let profiles = [
        vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
        vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
        vulcan_llm_core::GOOGLE_CODE_ASSIST_PROFILE_ID,
        vulcan_llm_core::GEMINI_INTERACTIONS_PROFILE_ID,
        vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID,
        vulcan_llm_core::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        vulcan_llm_core::COHERE_CHAT_PROFILE_ID,
        vulcan_llm_core::TYPESAFE_SYSTEMONE_PROFILE_ID,
        vulcan_llm_core::OPENVULCAN_NATIVE_PROFILE_ID,
    ];
    for source in profiles {
        for destination in profiles {
            // The final request stays tiny; only a matching anchor could put it above the line.
            // 最终请求保持微小；只有匹配锚点才可能使其超线。
            let request = request_snapshot(Vec::new());
            // Proof remains within the same source generation for the protocol comparison.
            // 协议比较时证明保持在同一来源代际内。
            let projection = v1_projection(4);
            // Build source evidence through the real serializer to detect protocol fingerprint regressions.
            // 经真实序列化器构建来源证据，检测协议指纹回归。
            let mut anchor = v1_budget_anchor(&request, projection.clone(), 449277);
            anchor.evidence =
                build_provider_request_budget_evidence(&request, source, "sha256:test-route")
                    .unwrap();
            // Destination owns a fresh guard, so no previous route's turn usage can leak in.
            // 目标拥有全新守卫，因此前一路由的回合用量无法泄漏。
            let mut guard = compaction_guard(destination, 335700, true);
            // Matching protocols reuse exact evidence; different protocols must fall back explicitly.
            // 相同协议复用精确证据；不同协议必须显式退回估算。
            let decision = guard
                .build_v1_budget_decision(&request, &projection, Some(&anchor))
                .unwrap();
            assert_eq!(
                decision.host_compaction_required,
                source == destination,
                "{source} -> {destination}"
            );
            assert_eq!(
                decision.context_budget.provider_anchor_call_id.is_some(),
                source == destination
            );
            guard.route_revision = String::from("sha256:other-route");
            assert!(
                !guard
                    .build_v1_budget_decision(&request, &projection, Some(&anchor))
                    .unwrap()
                    .host_compaction_required
            );
            guard.route_revision = String::from("sha256:test-route");
            assert!(
                !guard
                    .build_v1_budget_decision(&request, &v1_projection(5), Some(&anchor))
                    .unwrap()
                    .host_compaction_required
            );
            // A changed system prefix is a different request shape even when the route remains identical.
            // 即使路由相同，系统前缀改变也属于不同请求形状。
            let mut changed = request.clone();
            changed.system_prompt = "different system".into();
            assert!(
                !guard
                    .build_v1_budget_decision(&changed, &projection, Some(&anchor))
                    .unwrap()
                    .host_compaction_required
            );
        }
    }
}

/// Verifies final character growth is checked without an anchor while disabled compaction stays disabled.
/// 验证无锚点时仍检查最终字符增长，同时保持压缩关闭状态。
/// Takes no arguments; returns no value and uses a bounded synthetic request.
/// 无参数、无返回值，使用有界合成请求。
#[test]
fn final_character_budget_checks_growth_without_enabling_disabled_compaction() {
    // A small initial request grows when the unique owner adds authoritative history.
    // 唯一所有者添加权威历史时，初始小请求会增长。
    let mut request = request_snapshot(Vec::new());
    // This guard models a host turn whose early estimate was below the line.
    // 此守卫模拟早期估算低于压缩线的宿主回合。
    let mut guard = compaction_guard(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID, 1000, true);
    assert!(
        !guard
            .build_budget_snapshot(&request)
            .unwrap()
            .should_compact()
    );
    request.history.push(ConversationMessage {
        id: "restored-history".into(),
        payload: ConversationPayload::UserText {
            text: "restored text ".repeat(1000),
            attachments: Vec::new(),
        },
    });
    assert!(
        guard
            .build_v1_budget_decision(&request, &v1_projection(4), None)
            .unwrap()
            .host_compaction_required
    );
    guard.context_compaction_policy.as_mut().unwrap().auto = false;
    assert!(
        !guard
            .build_v1_budget_decision(&request, &v1_projection(4), None)
            .unwrap()
            .host_compaction_required
    );
}

/// Partial output evidence bounds the next input without adding a reasoning subset twice.
/// 局部输出证据为下一次输入提供下界，且不会重复添加思考子集。
#[test]
fn thinking_floor_and_missing_input_do_not_guess_cache_or_totals() {
    for (input, output, reasoning, floor) in [
        (Some(10000), Some(7500), Some(2500), Some(17500)),
        (Some(10000), Some(5000), Some(2500), Some(15000)),
        (Some(10000), None, Some(2500), Some(12500)),
        (None, Some(5000), Some(2500), None),
    ] {
        // Deliberately matching cache and thinking counts exercise the removed arithmetic heuristic.
        // 故意相同的缓存与思考计数覆盖已经移除的算术猜测。
        let response = CoreProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            usage: Some(ProviderUsageSnapshot {
                input_tokens: input,
                output_tokens: output,
                reasoning_tokens: reasoning,
                cached_input_tokens: Some(2500),
                total_tokens: Some(17500),
                ..Default::default()
            }),
            tool_calls: Vec::new(),
            continuation: None,
        };
        for line in [10001, 12500, 12501, 15000, 15001, 17500, 17501] {
            let guard = compaction_guard(
                vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
                line,
                true,
            );
            guard.observe_turn_response(&response);
            assert_eq!(
                matches!(
                    guard.validate_turn_compaction_line(&request_snapshot(Vec::new())),
                    Err(VulcanCoreError::ContextCompactionRequired { .. })
                ),
                floor.is_some_and(|floor| floor >= line as usize)
            );
        }
    }
}
