//! Regression coverage for canonical usage accounting from actual protocol envelopes.
//! 从实际协议封装验证统一用量口径的回归覆盖。

use super::*;
use crate::UpstreamProtocol;
use serde_json::json;

/// Extract records with the production profiles and return their merged canonical usage.
/// 使用生产 Profile 提取记录，并返回合并后的统一用量。
/// `protocol` selects the wire contract; `kind` and `records` describe transport records.
/// `protocol` 选择线协议契约；`kind` 与 `records` 描述传输记录。
fn extract_usage(
    protocol: UpstreamProtocol,
    kind: UsageRecordKind,
    records: &[Value],
) -> RequestUsage {
    // Each request owns an isolated accumulator and frozen protocol profiles.
    // 每次请求拥有独立累加器和冻结的协议 Profile。
    let profiles = standard_usage_profiles(protocol);
    let mut accumulator = UsageAccumulator::default();
    for record in records {
        // Exactly one selected profile may claim each complete fixture record.
        // 每条完整样例记录只能由一个已选 Profile 处理。
        let observations = profiles
            .iter()
            .filter_map(|profile| profile.extract(kind, record).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            observations.len(),
            1,
            "one profile must own {protocol:?}: {record}"
        );
        for observation in observations {
            accumulator.apply(None, observation).unwrap();
        }
    }
    accumulator.usage().clone()
}

/// Return a known counter or preserve its absence; `value` is one normalized dimension.
/// 返回已知计数或保留缺失；`value` 是一个归一化维度。
fn count(value: UsageValue) -> Option<u64> {
    match value {
        UsageValue::Known { value, .. } => Some(value),
        UsageValue::Unknown => None,
    }
}

/// Remove fixture placeholders recursively; `record` is mutated to contain only present fields.
/// 递归移除样例占位值；`record` 被修改为仅包含实际存在的字段。
fn omit_absent_fields(record: &mut Value) {
    if let Value::Object(fields) = record {
        fields.retain(|_, value| !value.is_null());
        for value in fields.values_mut() {
            omit_absent_fields(value);
        }
    }
}

/// Build an inclusive-input fixture; optional counters remain absent on the wire.
/// 构造输入包含缓存的样例；可选计数在线协议中保持缺失。
/// `protocol` chooses field names; numeric parameters carry input, cache, thinking and total.
/// `protocol` 选择字段名；数字参数提供输入、缓存、思考和总量。
fn inclusive_record(
    protocol: UpstreamProtocol,
    input: Option<u64>,
    cache: Option<u64>,
    reasoning: Option<u64>,
    total: Option<u64>,
) -> Value {
    // These are declared protocol paths, not guessed fallback aliases.
    // 这些是已声明的协议路径，不是猜测的兜底别名。
    let mut record = match protocol {
        UpstreamProtocol::OpenAiResponses => {
            json!({"usage":{"input_tokens":input,"input_tokens_details":{"cached_tokens":cache},"output_tokens":5000,"output_tokens_details":{"reasoning_tokens":reasoning},"total_tokens":total}})
        }
        UpstreamProtocol::OpenAiChatCompletions => {
            json!({"usage":{"prompt_tokens":input,"prompt_tokens_details":{"cached_tokens":cache},"completion_tokens":5000,"completion_tokens_details":{"reasoning_tokens":reasoning},"total_tokens":total}})
        }
        UpstreamProtocol::GeminiGenerateContent | UpstreamProtocol::GoogleCodeAssist => {
            json!({"usageMetadata":{"promptTokenCount":input,"cachedContentTokenCount":cache,"candidatesTokenCount":5000,"thoughtsTokenCount":reasoning,"totalTokenCount":total}})
        }
        UpstreamProtocol::GeminiInteractions => {
            json!({"event_type":"interaction.completed","interaction":{"usage":{"total_input_tokens":input,"total_cached_tokens":cache,"total_output_tokens":5000,"total_thought_tokens":reasoning,"total_tokens":total}}})
        }
        _ => unreachable!("fixture requires an inclusive-input protocol"),
    };
    // Omit absent fields; a literal null is malformed under the unsigned-counter contract.
    // 省略缺失字段；无符号计数契约会将空值字面量视为格式错误。
    omit_absent_fields(&mut record);
    if protocol == UpstreamProtocol::GoogleCodeAssist {
        record = json!({"response":record});
    }
    record
}

/// Verify cache subsets and independent thinking across five protocols and missing totals.
/// 验证五种协议中缓存子集、独立思考与缺失总量的组合。
#[test]
fn inclusive_input_matrix_counts_cache_and_thinking_once() {
    for protocol in [
        UpstreamProtocol::OpenAiResponses,
        UpstreamProtocol::OpenAiChatCompletions,
        UpstreamProtocol::GeminiGenerateContent,
        UpstreamProtocol::GoogleCodeAssist,
        UpstreamProtocol::GeminiInteractions,
    ] {
        for cache in [None, Some(0), Some(1), Some(2500), Some(10000)] {
            for reasoning in [None, Some(0), Some(1), Some(2500)] {
                for reported_total in [false, true] {
                    // Only Google's declared independent thinking increases complete output.
                    // 只有 Google 声明为独立维度的思考会增加完整输出。
                    let independent = matches!(
                        protocol,
                        UpstreamProtocol::GeminiGenerateContent
                            | UpstreamProtocol::GoogleCodeAssist
                            | UpstreamProtocol::GeminiInteractions
                    );
                    // Expected complete output includes only the declared independent contribution.
                    // 预期完整输出仅包含契约声明为独立维度的贡献。
                    let output = 5000
                        + if independent {
                            reasoning.unwrap_or(0)
                        } else {
                            0
                        };
                    // Keep total presence independent of all component fields.
                    // 总额是否存在与所有分量字段保持独立。
                    let total = reported_total.then_some(10000 + output);
                    // Use actual protocol field names and their required transport envelope.
                    // 使用实际协议字段名及其必需的传输封装。
                    let record = inclusive_record(protocol, Some(10000), cache, reasoning, total);
                    let kind = if protocol == UpstreamProtocol::GeminiInteractions {
                        UsageRecordKind::SseEvent
                    } else {
                        UsageRecordKind::JsonBody
                    };
                    // Run the production extraction and accumulation path for each combination.
                    // 为每种组合执行生产提取与累加链路。
                    let usage = extract_usage(protocol, kind, &[record]);
                    assert_eq!(count(usage.input_tokens), Some(10000), "{protocol:?}");
                    assert_eq!(
                        count(usage.uncached_input_tokens),
                        cache.map(|read| 10000 - read)
                    );
                    assert_eq!(count(usage.cache_read_tokens), cache);
                    assert_eq!(count(usage.output_tokens), Some(output), "{protocol:?}");
                    assert_eq!(count(usage.reasoning_tokens), reasoning);
                    assert_eq!(count(usage.total_tokens), total);
                }
            }
        }
    }
}

/// Missing input must not suppress other fields or turn an explicit zero into a missing total.
/// 缺失输入不得阻止提取其他字段，也不得把明确的零总量当作缺失。
#[test]
fn missing_input_keeps_output_and_zero_total_without_inventing_input() {
    for protocol in [
        UpstreamProtocol::OpenAiResponses,
        UpstreamProtocol::OpenAiChatCompletions,
        UpstreamProtocol::GeminiGenerateContent,
        UpstreamProtocol::GoogleCodeAssist,
        UpstreamProtocol::GeminiInteractions,
    ] {
        // Interactions exposes usage on a terminal SSE event; other fixtures use complete bodies.
        // Interactions 在终态 SSE 事件中暴露用量，其他样例使用完整正文。
        let kind = if protocol == UpstreamProtocol::GeminiInteractions {
            UsageRecordKind::SseEvent
        } else {
            UsageRecordKind::JsonBody
        };
        // Missing input cannot prevent a present output or an explicit zero from being extracted.
        // 缺失输入不能阻止提取已报告输出或明确零值。
        let usage = extract_usage(
            protocol,
            kind,
            &[inclusive_record(protocol, None, Some(0), Some(0), Some(0))],
        );
        assert_eq!(count(usage.input_tokens), None);
        assert_eq!(count(usage.uncached_input_tokens), None);
        assert_eq!(count(usage.output_tokens), Some(5000));
        assert_eq!(count(usage.total_tokens), Some(0));
    }
}

/// Anthropic writes remain processed input across partial final records and duplicate delivery.
/// Anthropic 写入在局部终态记录及重复投递后仍属于已处理输入。
#[test]
fn anthropic_cache_write_matrix_survives_output_only_terminal() {
    for (ordinary, read, write) in [
        (0, 0, 0),
        (4, 0, 449273),
        (4, 449273, 0),
        (4, 100, 200),
        (0, 0, 100),
        (100, 100, 100),
    ] {
        for representation in 0..3 {
            for output in [0, 5000] {
                // Aggregate, duration-only and both forms must yield the same write count.
                // 聚合、仅时长分项以及两者并存必须产生相同的写入计数。
                let mut raw = json!({"input_tokens":ordinary,"cache_read_input_tokens":read,"output_tokens":0});
                if representation != 1 {
                    raw["cache_creation_input_tokens"] = json!(write);
                }
                if representation != 0 {
                    raw["cache_creation"] = json!({"ephemeral_5m_input_tokens":write});
                }
                // Repeat the output-only cumulative record to detect accidental additive merging.
                // 重复仅含输出的累计记录，用于发现意外执行的加法合并。
                let start = json!({"type":"message_start","message":{"usage":raw}});
                let end = json!({"type":"message_delta","usage":{"output_tokens":output}});
                // The normalized result must include each disjoint input component exactly once.
                // 归一化结果必须且只能包含一次每个互斥输入分量。
                let usage = extract_usage(
                    UpstreamProtocol::AnthropicMessages,
                    UsageRecordKind::SseEvent,
                    &[start, end.clone(), end],
                );
                assert_eq!(count(usage.input_tokens), Some(ordinary + read + write));
                assert_eq!(count(usage.uncached_input_tokens), Some(ordinary + write));
                assert_eq!(count(usage.cache_write_tokens), Some(write));
                assert_eq!(count(usage.output_tokens), Some(output));
            }
        }
    }
    for input in [None, Some(0), Some(1000)] {
        // Omitted cache counters are optional, while missing ordinary input remains unknown.
        // 省略的缓存计数是可选项，缺失的普通输入仍然保持未知。
        let mut record = json!({"usage":{"input_tokens":input,"output_tokens":0}});
        omit_absent_fields(&mut record);
        // Exercise the complete-body path separately from the streaming matrix.
        // 在流式矩阵之外单独覆盖完整正文路径。
        let usage = extract_usage(
            UpstreamProtocol::AnthropicMessages,
            UsageRecordKind::JsonBody,
            &[record],
        );
        assert_eq!(count(usage.input_tokens), input);
        assert_eq!(count(usage.uncached_input_tokens), input);
        assert_eq!(count(usage.cache_read_tokens), None);
        assert_eq!(count(usage.cache_write_tokens), None);
    }
}

/// Overflowing cache details remain unknown through dependent input and total derivations.
/// 缓存明细溢出后，依赖它的输入与总额派生必须保持未知。
#[test]
fn overflowing_cache_write_details_do_not_become_an_absent_contribution() {
    // Partial duration reports exercise both direct extraction and normalization after raw merging.
    // 分段时长报告同时覆盖直接提取与原始分量合并后的归一化。
    let records = [
        json!({"type":"message_start","message":{"usage":{"input_tokens":4,"output_tokens":0,"cache_creation":{"ephemeral_5m_input_tokens":u64::MAX}}}}),
        json!({"type":"message_delta","usage":{"output_tokens":5,"cache_creation":{"ephemeral_1h_input_tokens":1}}}),
        json!({"type":"message_stop"}),
    ];
    // The accumulator must preserve invalid sums as unknown rather than returning ordinary input alone.
    // 累加器必须让无效求和保持未知，不能只返回普通输入量。
    let usage = extract_usage(
        UpstreamProtocol::AnthropicMessages,
        UsageRecordKind::SseEvent,
        &records,
    );
    assert_eq!(count(usage.input_tokens), None);
    assert_eq!(count(usage.uncached_input_tokens), None);
    assert_eq!(count(usage.cache_write_tokens), None);
    assert_eq!(count(usage.output_tokens), Some(5));
    // A supplier may explicitly request total derivation; the same invalid dependency must propagate.
    // 供应商可以显式请求总额派生，同一个无效依赖也必须传播至总额。
    let mut profile = anthropic_usage_profiles()
        .into_iter()
        .find(|profile| profile.record_kind == UsageRecordKind::JsonBody)
        .expect("Anthropic JSON profile fixture may be outdated");
    profile
        .derivations
        .push(UsageDerivation::TotalFromInputComponentsAndOutput);
    // Both duration fields are present in one complete response in this branch.
    // 此分支在一条完整响应中同时报告两种时长字段。
    let record = json!({"usage":{"input_tokens":4,"output_tokens":5,"cache_creation":{"ephemeral_5m_input_tokens":u64::MAX,"ephemeral_1h_input_tokens":1}}});
    // Direct extraction must reach the same unknown result as the cumulative streaming path.
    // 直接提取必须与累计流式路径得到相同的未知结果。
    let observation = profile
        .extract(UsageRecordKind::JsonBody, &record)
        .unwrap()
        .unwrap();
    assert_eq!(count(observation.usage.input_tokens), None);
    assert_eq!(count(observation.usage.uncached_input_tokens), None);
    assert_eq!(count(observation.usage.total_tokens), None);
}

/// Partial Google snapshots must recompute complete output without losing or repeating thoughts.
/// Google 局部快照必须重新计算完整输出，不能丢失或重复思考量。
#[test]
fn partial_gemini_snapshots_recompute_complete_output() {
    for protocol in [
        UpstreamProtocol::GeminiGenerateContent,
        UpstreamProtocol::GoogleCodeAssist,
    ] {
        // Delayed thoughts must update prior output without adding a repeated candidate count.
        // 延迟到达的思考必须更新先前输出，不能累加重复的候选计数。
        let records = [
            json!({"promptTokenCount":10000,"cachedContentTokenCount":2500}),
            json!({"candidatesTokenCount":5000}),
            json!({"thoughtsTokenCount":2500}),
            json!({"candidatesTokenCount":5000}),
        ]
        .map(|usage| {
            // Only Code Assist adds the outer response envelope.
            // 只有 Code Assist 增加外层 response 封装。
            let record = json!({"usageMetadata":usage});
            if protocol == UpstreamProtocol::GoogleCodeAssist {
                json!({"response":record})
            } else {
                record
            }
        });
        // A cumulative snapshot must reflect all previously observed raw components.
        // 累计快照必须反映此前观察到的全部原始分量。
        let usage = extract_usage(protocol, UsageRecordKind::SseEvent, &records);
        assert_eq!(count(usage.input_tokens), Some(10000));
        assert_eq!(count(usage.output_tokens), Some(7500));
        assert_eq!(count(usage.total_tokens), None);
        assert_eq!(usage.update_kind, UsageUpdateKind::Snapshot);
    }
}

/// A provider override declaring complete output must retain its authoritative subset semantics.
/// 声明完整输出的供应商覆盖配置必须保留其权威子集语义。
#[test]
fn complete_output_profile_does_not_add_reasoning_subset() {
    // Explicit supplier overrides choose their own reasoning relationship.
    // 显式供应商覆盖配置自行选择思考与输出的关系。
    let mut profile = standard_usage_profiles(UpstreamProtocol::GeminiGenerateContent)
        .into_iter()
        .find(|profile| profile.record_kind == UsageRecordKind::JsonBody)
        .expect("Gemini body usage contract must exist; fixture may need updating");
    profile
        .derivations
        .retain(|derivation| *derivation != UsageDerivation::OutputFromDisjointReasoning);
    // The same numeric fixture must retain complete output when no additive derivation is declared.
    // 未声明加法派生时，相同数字样例必须保留其完整输出。
    let observation = profile
        .extract(
            UsageRecordKind::JsonBody,
            &inclusive_record(
                UpstreamProtocol::GeminiGenerateContent,
                Some(10000),
                Some(2500),
                Some(2500),
                None,
            ),
        )
        .unwrap()
        .unwrap();
    // The accumulator must preserve the selected override after extraction.
    // 累加器必须在提取后保留已选覆盖配置。
    let mut accumulator = UsageAccumulator::default();
    accumulator.apply(None, observation).unwrap();
    assert_eq!(count(accumulator.usage().output_tokens), Some(5000));
}

/// OpenAI cache writes and reasoning remain details of already complete input and output.
/// OpenAI 缓存写入和思考仍是已经完整的输入与输出的明细。
#[test]
fn openai_cache_writes_do_not_expand_input() {
    for protocol in [
        UpstreamProtocol::OpenAiResponses,
        UpstreamProtocol::OpenAiChatCompletions,
    ] {
        // Cache creation is a detail of the already complete reported input.
        // 缓存创建是已报告完整输入的一个明细。
        let mut record =
            inclusive_record(protocol, Some(10000), Some(3000), Some(2500), Some(15000));
        // Each protocol owns one explicit input-details field.
        // 每个协议拥有一个明确的输入明细字段。
        let details = if protocol == UpstreamProtocol::OpenAiResponses {
            "input_tokens_details"
        } else {
            "prompt_tokens_details"
        };
        record["usage"][details]["cache_write_tokens"] = json!(6000);
        // Extraction must retain the write metric without expanding complete input.
        // 提取必须保留写入指标，同时不扩增完整输入。
        let usage = extract_usage(protocol, UsageRecordKind::JsonBody, &[record]);
        assert_eq!(count(usage.input_tokens), Some(10000));
        assert_eq!(count(usage.uncached_input_tokens), Some(7000));
        assert_eq!(count(usage.cache_write_tokens), Some(6000));
        assert_eq!(count(usage.output_tokens), Some(5000));
    }
}

/// Declared dependency order and delta records must not duplicate earlier derived dimensions.
/// 声明依赖的顺序与增量记录不得重复此前的派生维度。
#[test]
fn disjoint_derivations_follow_dependencies_after_raw_delta_merge() {
    // An explicit test supplier combines additive dimensions in reverse dependency order.
    // 显式测试供应商按依赖的逆序组合加法维度。
    let mut profile = standard_usage_profiles(UpstreamProtocol::AnthropicMessages)
        .into_iter()
        .find(|profile| profile.record_kind == UsageRecordKind::JsonBody)
        .expect("Anthropic body usage contract must exist; fixture may need updating");
    profile.update_kind = UsageUpdateKind::Delta;
    profile.bindings.push(token_binding(
        UsageField::ReasoningTokens,
        "/usage/thinking",
    ));
    profile.derivations = vec![
        UsageDerivation::TotalFromInputComponentsAndOutput,
        UsageDerivation::OutputFromDisjointReasoning,
        UsageDerivation::InputFromDisjointDimensions,
        UsageDerivation::CacheWriteFromDurations,
    ];
    // Raw delta accumulation must precede all derived totals.
    // 原始增量累加必须先于所有总额派生。
    let mut accumulator = UsageAccumulator::default();
    for record in [
        json!({"usage":{"input_tokens":10,"cache_creation":{"ephemeral_1h_input_tokens":20},"output_tokens":30}}),
        json!({"usage":{"thinking":5}}),
    ] {
        accumulator
            .apply(
                None,
                profile
                    .extract(UsageRecordKind::JsonBody, &record)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
    }
    assert_eq!(count(accumulator.usage().input_tokens), Some(30));
    assert_eq!(count(accumulator.usage().uncached_input_tokens), Some(30));
    assert_eq!(count(accumulator.usage().output_tokens), Some(35));
    assert_eq!(count(accumulator.usage().total_tokens), Some(65));
    assert_eq!(accumulator.usage().update_kind, UsageUpdateKind::Snapshot);
}

/// Later Anthropic cumulative records replace raw input and cache fields before terminal freezing.
/// 后续 Anthropic 累计记录在终态冻结前替换原始输入与缓存字段。
#[test]
fn anthropic_cumulative_input_updates_freeze_only_at_message_stop() {
    // Server-tool usage can increase input after the initial message-start record.
    // 服务端工具用量可以在初始消息记录之后增加输入。
    let records = [
        json!({"type":"message_start","message":{"usage":{"input_tokens":4,"output_tokens":0}}}),
        json!({"type":"message_delta","usage":{"input_tokens":10,"cache_read_input_tokens":100,"cache_creation_input_tokens":200,"output_tokens":20}}),
        json!({"type":"message_delta","usage":{"input_tokens":15,"cache_creation_input_tokens":250,"output_tokens":30}}),
        json!({"type":"message_stop"}),
        json!({"type":"message_stop"}),
    ];
    // Finalization preserves the latest cumulative input and output even after a repeated stop.
    // 即使停止事件重复出现，终态也保留最新累计输入与输出。
    let usage = extract_usage(
        UpstreamProtocol::AnthropicMessages,
        UsageRecordKind::SseEvent,
        &records,
    );
    assert_eq!(count(usage.input_tokens), Some(365));
    assert_eq!(count(usage.uncached_input_tokens), Some(265));
    assert_eq!(count(usage.output_tokens), Some(30));
    assert_eq!(usage.update_kind, UsageUpdateKind::Final);
}

/// Responses body, SSE and WebSocket extraction share inclusive input and output semantics.
/// Responses 正文、SSE 和 WebSocket 提取共享包含式输入与输出语义。
#[test]
fn responses_transport_variants_keep_cache_and_reasoning_as_subsets() {
    for kind in [
        UsageRecordKind::JsonBody,
        UsageRecordKind::SseEvent,
        UsageRecordKind::WebSocketFrame,
    ] {
        // All three transports carry the same official response usage object.
        // 三种传输承载相同的官方响应用量对象。
        let body = json!({"usage":{"input_tokens":10000,"input_tokens_details":{"cached_tokens":2500,"cache_write_tokens":5000},"output_tokens":5000,"output_tokens_details":{"reasoning_tokens":2500},"total_tokens":15000}});
        // Event transports wrap the same response object once.
        // 事件传输将同一响应对象包装一次。
        let record = if kind == UsageRecordKind::JsonBody {
            body
        } else {
            json!({"type":"response.completed","response":body})
        };
        // Transport differences must not change the normalized accounting relationship.
        // 传输差异不能改变归一化计数关系。
        let usage = extract_usage(UpstreamProtocol::OpenAiResponses, kind, &[record]);
        assert_eq!(count(usage.input_tokens), Some(10000));
        assert_eq!(count(usage.output_tokens), Some(5000));
        assert_eq!(count(usage.cache_write_tokens), Some(5000));
        assert_eq!(count(usage.total_tokens), Some(15000));
    }
}

/// Similar Google payloads cannot become cross-protocol candidate paths.
/// 相似的 Google 载荷不能成为跨协议的候选路径。
#[test]
fn google_usage_envelopes_do_not_fall_back_to_the_other_protocol() {
    for (protocol, record) in [
        (
            UpstreamProtocol::GoogleCodeAssist,
            json!({"usageMetadata":{"promptTokenCount":10000}}),
        ),
        (
            UpstreamProtocol::GeminiGenerateContent,
            json!({"response":{"usageMetadata":{"promptTokenCount":10000}}}),
        ),
    ] {
        for profile in standard_usage_profiles(protocol) {
            assert!(
                profile
                    .extract(profile.record_kind, &record)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
