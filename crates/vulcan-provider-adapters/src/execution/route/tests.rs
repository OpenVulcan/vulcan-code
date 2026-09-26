//! Regression coverage for registered protocol families and frozen manifest usage profiles.
//! 已注册协议家族与冻结清单用量 Profile 的回归覆盖。

use super::super::test_support::bundled_manifest_route;
use super::*;
use crate::ProviderRegistry;

/// Verifies every executable built-in provider protocol is owned by the Responses conversion core.
/// 验证每个可执行的内置供应商协议都由 Responses 转换核心负责。
#[test]
fn every_builtin_provider_protocol_routes_through_responses_core() {
    let registry = ProviderRegistry::first_batch().expect("provider registry should build");
    let unmapped = registry
        .providers()
        .into_iter()
        .flat_map(|provider| {
            provider.service_entries.iter().flat_map(move |entry| {
                entry
                    .protocol_routes
                    .iter()
                    .filter(|route| {
                        upstream_protocol_for_profile_id(route.protocol_profile_id.as_str())
                            .is_none()
                    })
                    .map(move |route| {
                        format!("{}/{}/{}", provider.id, entry.id, route.protocol_profile_id)
                    })
            })
        })
        .collect::<Vec<_>>();

    assert!(
        unmapped.is_empty(),
        "built-in provider protocols bypass Responses core: {unmapped:?}"
    );
}

/// Verifies provider-specialized dynamic usage paths are frozen by manifest declarations.
/// 验证供应商专属动态用量路径由清单声明冻结（方案 §15.4）。
#[test]
fn exact_provider_routes_install_specialized_usage_profiles() {
    // The bundled manifest is the single fact source for the Alibaba Responses billing details.
    // 内置清单是 Alibaba Responses 计费明细的唯一事实来源。
    let alibaba_route =
        bundled_manifest_route("alibaba", "model-studio-cn", OPENAI_RESPONSES_PROFILE_ID);
    // alibaba_route is the immutable execution snapshot inspected below.
    // alibaba_route 是下方检查的不可变执行快照。
    assert!(
        alibaba_route
            .protocol_route
            .usage_extraction_profiles
            .iter()
            .all(|profile| profile.profile_id.starts_with("alibaba.openai.responses."))
    );
    assert!(
        alibaba_route
            .protocol_route
            .usage_extraction_profiles
            .iter()
            .all(|profile| profile.bindings.iter().any(|binding| {
                binding.field == vulcan_llm_core::UsageField::BillingDetails
                    && binding.encoding
                        == vulcan_llm_core::UsageValueEncoding::AlibabaResponsesBillingDetails
            }))
    );

    // deepseek_route is resolved from the bundled manifest Chat route.
    // deepseek_route 由内置清单 Chat 路由解析。
    let deepseek_route = bundled_manifest_route("deepseek", "official-api", OPENAI_CHAT_PROFILE_ID);
    assert!(
        deepseek_route
            .protocol_route
            .usage_extraction_profiles
            .iter()
            .all(|profile| profile.profile_id.starts_with("deepseek.chat."))
    );
    assert!(
        deepseek_route
            .protocol_route
            .usage_extraction_profiles
            .iter()
            .all(|profile| profile.bindings.iter().any(|binding| {
                binding.field == vulcan_llm_core::UsageField::UncachedInputTokens
                    && binding.pointers == [String::from("/usage/prompt_cache_miss_tokens")]
            }))
    );

    // minimaxRoute proves additive totals and thinking tokens are enabled only by the exact
    // MiniMax Anthropic route declaration.
    // minimaxRoute 证明总额加法与思考 Token 只由精确的 MiniMax Anthropic 路由声明启用。
    let minimax_route = bundled_manifest_route(
        "minimax",
        "global",
        vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
    );
    assert_eq!(
        minimax_route.protocol_route.usage_extraction_profiles.len(),
        3
    );
    // minimaxFinalProfile is the authoritative final streaming usage profile.
    // minimaxFinalProfile 是权威的最终流式用量 Profile。
    let minimax_final_profile = minimax_route
        .protocol_route
        .usage_extraction_profiles
        .iter()
        .find(|profile| profile.profile_id == "minimax.anthropic.message-delta.usage.v1")
        .expect("MiniMax final Anthropic usage profile must be installed");
    assert!(
        minimax_final_profile
            .derivations
            .contains(&vulcan_llm_core::UsageDerivation::TotalFromInputComponentsAndOutput)
    );
    assert!(minimax_final_profile.bindings.iter().any(|binding| {
        binding.field == vulcan_llm_core::UsageField::ReasoningTokens
            && binding.pointers == [String::from("/usage/output_tokens_details/thinking_tokens")]
    }));
    // minimaxObservation executes the manifest-installed profile against the captured response
    // shape that previously persisted a missing total.
    // minimaxObservation 使用曾经把总额持久化为缺失值的已捕获响应形状执行 Manifest Profile。
    let minimax_observation = minimax_final_profile
        .extract(
            vulcan_llm_core::UsageRecordKind::SseEvent,
            &serde_json::json!({
                "type": "message_delta",
                "usage": {
                    "input_tokens": 29_401,
                    "output_tokens": 645,
                    "output_tokens_details": {
                        "thinking_tokens": 19
                    }
                }
            }),
        )
        .expect("MiniMax captured usage should parse")
        .expect("MiniMax final usage event should match");
    assert_eq!(
        minimax_observation.usage.total_tokens,
        vulcan_ai_contract::UsageValue::Known {
            value: 30_046,
            source: vulcan_ai_contract::UsageValueSource::Derived,
        }
    );
    assert_eq!(
        minimax_observation.usage.reasoning_tokens,
        vulcan_ai_contract::UsageValue::Known {
            value: 19,
            source: vulcan_ai_contract::UsageValueSource::Reported,
        }
    );

    // openrouter_route proves the manifest binds the usage profile to the exact Chat route.
    // openrouter_route 证明清单把用量 Profile 绑定到精确 Chat 路由。
    let openrouter_route =
        bundled_manifest_route("openrouter", "public-api", OPENAI_CHAT_PROFILE_ID);
    assert!(
        openrouter_route
            .protocol_route
            .usage_extraction_profiles
            .iter()
            .all(|profile| profile.profile_id.starts_with("openrouter.chat."))
    );
    assert!(
        openrouter_route
            .protocol_route
            .usage_extraction_profiles
            .iter()
            .all(|profile| profile.bindings.iter().all(|binding| {
                binding
                    .pointers
                    .iter()
                    .all(|pointer| pointer != "/usage/cost")
            }))
    );
}
