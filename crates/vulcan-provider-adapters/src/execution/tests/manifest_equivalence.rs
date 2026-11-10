//! Complete wire-contract regressions driven by current bundled manifests.
//! 由当前内置清单驱动的完整线路契约回归。

use super::super::ProviderExecutionClient;
use super::super::test_support::{bundled_manifest_route, minimal_request};
use crate::{MINIMAX_REASONING_SPLIT_PARAMETER_ID, SAMPLING_TEMPERATURE_PARAMETER_ID};
use std::sync::Arc;
use vulcan_ai_contract::{ModelParameterValue, ReasoningEffort};
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_RESPONSES_PROFILE_ID, ProtocolRegistry,
};

/// Builds one execution client used by the projection equivalence tests.
/// 构建投影等价测试使用的执行客户端。
fn projection_test_client() -> ProviderExecutionClient {
    ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    )
}

/// Verifies every bundled DeepSeek protocol projects the current official reasoning contract.
/// 验证每个内置 DeepSeek 协议都投影当前官方思考控制契约。
#[test]
fn deepseek_manifest_profiles_produce_legacy_equivalent_wire() {
    // client executes the same protocol encoder and manifest projection chain as production.
    // client 执行与生产环境相同的协议编码器和 Manifest 投影链。
    let client = projection_test_client();
    // chatRoute owns the DeepSeek OpenAI Chat projection.
    // chatRoute 持有 DeepSeek OpenAI Chat 投影。
    let chat_route = bundled_manifest_route("deepseek", "official-api", OPENAI_CHAT_PROFILE_ID);
    // anthropicRoute owns the DeepSeek Anthropic projection.
    // anthropicRoute 持有 DeepSeek Anthropic 投影。
    let anthropic_route =
        bundled_manifest_route("deepseek", "official-api", ANTHROPIC_MESSAGES_PROFILE_ID);
    // responsesRoute owns the DeepSeek Responses projection.
    // responsesRoute 持有 DeepSeek Responses 投影。
    let responses_route =
        bundled_manifest_route("deepseek", "official-api", OPENAI_RESPONSES_PROFILE_ID);
    assert!(
        !chat_route.request_projection_profiles.is_empty()
            && !anthropic_route.request_projection_profiles.is_empty()
            && !responses_route.request_projection_profiles.is_empty(),
        "bundled DeepSeek routes must install manifest request profiles"
    );

    // enabledEfforts is the exact current DeepSeek effort set shared by all three protocols.
    // enabledEfforts 是三个协议当前共享的精确 DeepSeek 思考强度集合。
    let enabled_efforts = [
        (ReasoningEffort::Low, "low"),
        (ReasoningEffort::High, "high"),
        (ReasoningEffort::Max, "max"),
    ];
    for (effort, wire_label) in enabled_efforts {
        // request carries one enabled effort and a temperature that thinking mode must remove.
        // request 携带一个已启用强度，以及思考模式必须移除的 temperature。
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-v4-pro");
        request.reasoning_effort = Some(effort);
        request.model_parameters.insert(
            String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
            ModelParameterValue::Decimal(String::from("0.7")),
        );

        // chatEncoded must retain the OpenAI-format effort and enable thinking.
        // chatEncoded 必须保留 OpenAI 格式强度并开启思考。
        let chat_encoded = client
            .encode_request(&chat_route, &request)
            .expect("DeepSeek Chat enabled-effort request should encode");
        assert_eq!(chat_encoded.body["thinking"]["type"], "enabled");
        assert_eq!(chat_encoded.body["reasoning_effort"], wire_label);
        assert!(chat_encoded.body.get("temperature").is_none());

        // anthropicEncoded must move the effort to output_config and enable thinking.
        // anthropicEncoded 必须把强度放入 output_config 并开启思考。
        let anthropic_encoded = client
            .encode_request(&anthropic_route, &request)
            .expect("DeepSeek Anthropic enabled-effort request should encode");
        assert_eq!(anthropic_encoded.body["thinking"]["type"], "enabled");
        assert_eq!(
            anthropic_encoded.body["output_config"]["effort"],
            wire_label
        );
        assert!(anthropic_encoded.body.get("temperature").is_none());
        assert!(anthropic_encoded.body.get("reasoning_effort").is_none());

        // responsesEncoded must use only the Responses reasoning object.
        // responsesEncoded 必须只使用 Responses reasoning 对象。
        let responses_encoded = client
            .encode_request(&responses_route, &request)
            .expect("DeepSeek Responses enabled-effort request should encode");
        assert_eq!(responses_encoded.body["reasoning"]["effort"], wire_label);
        assert!(responses_encoded.body.get("thinking").is_none());
        assert!(responses_encoded.body.get("reasoning_effort").is_none());
        assert!(responses_encoded.body.get("output_config").is_none());
        assert!(responses_encoded.body.get("store").is_none());
        assert!(responses_encoded.body.get("include").is_none());
    }

    // disabledRequest verifies every protocol's explicit no-thinking representation.
    // disabledRequest 验证每个协议显式关闭思考时的表达。
    let mut disabled_request = minimal_request();
    disabled_request.model_id = String::from("deepseek-v4-pro");
    disabled_request.reasoning_effort = Some(ReasoningEffort::None);
    disabled_request.model_parameters.insert(
        String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
        ModelParameterValue::Decimal(String::from("0.7")),
    );

    // chatDisabled uses thinking.disabled and removes the incompatible effort field.
    // chatDisabled 使用 thinking.disabled，并移除不兼容的强度字段。
    let chat_disabled = client
        .encode_request(&chat_route, &disabled_request)
        .expect("DeepSeek Chat disabled request should encode");
    assert_eq!(chat_disabled.body["thinking"]["type"], "disabled");
    assert!(chat_disabled.body.get("reasoning_effort").is_none());
    assert_eq!(chat_disabled.body["temperature"], 0.7);

    // anthropicDisabled uses thinking.disabled without an empty output_config object.
    // anthropicDisabled 使用 thinking.disabled，且不保留空的 output_config 对象。
    let anthropic_disabled = client
        .encode_request(&anthropic_route, &disabled_request)
        .expect("DeepSeek Anthropic disabled request should encode");
    assert_eq!(anthropic_disabled.body["thinking"]["type"], "disabled");
    assert!(anthropic_disabled.body.get("output_config").is_none());
    assert_eq!(anthropic_disabled.body["temperature"], 0.7);

    // responsesDisabled expresses no-thinking directly as reasoning.effort=none.
    // responsesDisabled 通过 reasoning.effort=none 直接表达关闭思考。
    let responses_disabled = client
        .encode_request(&responses_route, &disabled_request)
        .expect("DeepSeek Responses disabled request should encode");
    assert_eq!(responses_disabled.body["reasoning"]["effort"], "none");
    assert!(responses_disabled.body.get("thinking").is_none());
    assert!(responses_disabled.body.get("reasoning_effort").is_none());
    assert!(responses_disabled.body.get("output_config").is_none());
    assert!(responses_disabled.body.get("store").is_none());
    assert!(responses_disabled.body.get("include").is_none());
}

/// Verifies the bundled Alibaba request profiles produce the legacy-equivalent wire.
/// 验证内置 Alibaba 请求 Profile 产生与旧版等价的 Wire 请求。
#[test]
fn alibaba_manifest_profiles_produce_legacy_equivalent_wire() {
    let client = projection_test_client();
    let chat_route = bundled_manifest_route("alibaba", "model-studio-cn", OPENAI_CHAT_PROFILE_ID);
    let anthropic_route =
        bundled_manifest_route("alibaba", "model-studio-cn", ANTHROPIC_MESSAGES_PROFILE_ID);
    let mut request = minimal_request();
    request.model_id = String::from("qwen3.7-max");

    request.reasoning_effort = Some(ReasoningEffort::Adaptive);
    let encoded = client
        .encode_request(&chat_route, &request)
        .expect("Alibaba Chat request should encode");
    assert_eq!(encoded.body["enable_thinking"], true);
    assert!(encoded.body.get("reasoning_effort").is_none());

    request.reasoning_effort = Some(ReasoningEffort::None);
    let encoded = client
        .encode_request(&chat_route, &request)
        .expect("Alibaba Chat none request should encode");
    assert_eq!(encoded.body["enable_thinking"], false);

    request.reasoning_effort = Some(ReasoningEffort::None);
    let encoded = client
        .encode_request(&anthropic_route, &request)
        .expect("Alibaba Anthropic request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "disabled");
    assert!(encoded.body.get("output_config").is_none());

    request.reasoning_effort = Some(ReasoningEffort::Adaptive);
    let encoded = client
        .encode_request(&anthropic_route, &request)
        .expect("Alibaba Anthropic adaptive request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "enabled");
}

/// Verify the bundled Token Plan route preserves an explicit thinking effort and can disable thinking.
/// 验证内置 Token Plan 路线保留明确的思考档位，并能够关闭思考。
#[test]
fn alibaba_token_plan_projects_thinking_levels_and_off() {
    // Use the same compiled request profile that the Beijing personal route uses in production.
    // 使用北京个人版生产路线所用的已编译请求配置。
    let route = bundled_manifest_route("alibaba", "token-plan-personal-cn", OPENAI_CHAT_PROFILE_ID);
    let client = projection_test_client();
    // The 1017 snapshot accepts Ultra on this gateway, while disabling thinking omits the effort.
    // 1017 快照在该网关支持 Ultra，关闭思考时则省略档位字段。
    let mut request = minimal_request();
    request.model_id = String::from("deepseek-v4.1-flash-1017");
    request.reasoning_effort = Some(ReasoningEffort::Ultra);
    let enabled = client
        .encode_request(&route, &request)
        .expect("Token Plan must encode the tested Ultra effort");
    assert_eq!(enabled.body["enable_thinking"], true);
    assert_eq!(enabled.body["reasoning_effort"], "ultra");

    // Send the verified boolean switch without an additional effort that can override it.
    // 发送已验证的布尔开关，避免额外的档位字段覆盖它。
    request.reasoning_effort = Some(ReasoningEffort::None);
    let disabled = client
        .encode_request(&route, &request)
        .expect("Token Plan must encode thinking disabled");
    assert_eq!(disabled.body["enable_thinking"], false);
    assert!(disabled.body.get("reasoning_effort").is_none());

    // The same user-facing level occupies a different field on the other protocols.
    // 同一用户可选档位在另外两种协议中使用不同字段。
    let messages_route = bundled_manifest_route(
        "alibaba",
        "token-plan-personal-cn",
        ANTHROPIC_MESSAGES_PROFILE_ID,
    );
    let responses_route = bundled_manifest_route(
        "alibaba",
        "token-plan-personal-cn",
        OPENAI_RESPONSES_PROFILE_ID,
    );
    request.model_id = String::from("qwen3.8-max");
    request.reasoning_effort = Some(ReasoningEffort::High);
    let messages = client
        .encode_request(&messages_route, &request)
        .expect("Token Plan Messages must encode the tested high effort");
    assert_eq!(messages.body["thinking"]["type"], "enabled");
    assert_eq!(messages.body["output_config"]["effort"], "high");
    let responses = client
        .encode_request(&responses_route, &request)
        .expect("Token Plan Responses must encode the tested high effort");
    assert_eq!(responses.body["enable_thinking"], true);
    assert_eq!(responses.body["reasoning"]["effort"], "high");
}

/// Verifies the bundled OpenRouter request profiles produce the legacy-equivalent wire.
/// 验证内置 OpenRouter 请求 Profile 产生与旧版等价的 Wire 请求。
#[test]
fn openrouter_manifest_profiles_produce_legacy_equivalent_wire() {
    let client = projection_test_client();
    let chat_route = bundled_manifest_route("openrouter", "public-api", OPENAI_CHAT_PROFILE_ID);
    let anthropic_route =
        bundled_manifest_route("openrouter", "public-api", ANTHROPIC_MESSAGES_PROFILE_ID);
    let responses_route =
        bundled_manifest_route("openrouter", "public-api", OPENAI_RESPONSES_PROFILE_ID);
    let mut request = minimal_request();
    request.model_id = String::from("openai/gpt-5.6-sol");

    // Adaptive Chat: reasoning.enabled, no effort, no reasoning_effort.
    // 自适应Chat使用reasoning.enabled，不保留effort或reasoning_effort。
    request.reasoning_effort = Some(ReasoningEffort::Adaptive);
    let encoded = client
        .encode_request(&chat_route, &request)
        .expect("OpenRouter Chat adaptive request should encode");
    assert_eq!(encoded.body["reasoning"]["enabled"], true);
    assert!(encoded.body["reasoning"].get("effort").is_none());
    assert!(encoded.body.get("reasoning_effort").is_none());

    // Non-adaptive Anthropic: output_config.effort + reasoning.effort.
    // 非自适应Anthropic同时保留output_config.effort与reasoning.effort。
    request.reasoning_effort = Some(ReasoningEffort::Low);
    let encoded = client
        .encode_request(&anthropic_route, &request)
        .expect("OpenRouter Anthropic request should encode");
    assert_eq!(encoded.body["output_config"]["effort"], "low");
    assert_eq!(encoded.body["reasoning"]["effort"], "low");
    assert!(encoded.body["reasoning"].get("enabled").is_none());

    // Responses max effort: reasoning.effort without the protocol effort field removal.
    // Responses最高强度使用reasoning.effort，不执行其他协议的强度字段删除。
    request.reasoning_effort = Some(ReasoningEffort::Max);
    let encoded = client
        .encode_request(&responses_route, &request)
        .expect("OpenRouter Responses request should encode");
    assert_eq!(encoded.body["reasoning"]["effort"], "max");
}

/// Verifies the OpenRouter reasoning token budget parameter projects via the manifest profile.
/// 验证 OpenRouter 推理 Token 预算参数经 Manifest Profile 投影。
#[test]
fn openrouter_reasoning_max_tokens_projects_via_profile() {
    let client = projection_test_client();
    let chat_route = bundled_manifest_route("openrouter", "public-api", OPENAI_CHAT_PROFILE_ID);
    let mut request = minimal_request();
    request.model_id = String::from("openai/gpt-5.6-sol");
    request.reasoning_effort = None;
    request.model_parameters.insert(
        String::from("reasoning.max_tokens"),
        ModelParameterValue::Integer(16384),
    );
    let encoded = client
        .encode_request(&chat_route, &request)
        .expect("OpenRouter max tokens request should encode");
    assert_eq!(encoded.body["reasoning"]["max_tokens"], 16384);
}

/// Verifies the bundled Kimi default profile matches the non-coding-model legacy behavior.
/// 验证内置 Kimi 默认 Profile 与非编码模型旧版行为一致。
#[test]
fn kimi_manifest_default_profile_matches_legacy_wire() {
    let client = projection_test_client();
    let chat_route = bundled_manifest_route("kimi", "platform-global", OPENAI_CHAT_PROFILE_ID);
    assert!(
        !chat_route.request_projection_profiles.is_empty(),
        "bundled Kimi chat route must install the default request profile"
    );
    let mut request = minimal_request();
    request.model_id = String::from("moonshot-v1-8k");

    request.reasoning_effort = Some(ReasoningEffort::High);
    let encoded = client
        .encode_request(&chat_route, &request)
        .expect("Kimi Chat reasoning request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "enabled");
    assert!(encoded.body.get("reasoning_effort").is_none());

    request.reasoning_effort = Some(ReasoningEffort::None);
    let encoded = client
        .encode_request(&chat_route, &request)
        .expect("Kimi Chat none request should encode");
    assert!(encoded.body.get("thinking").is_none());
}

/// Verifies the bundled MiniMax profiles produce the legacy-equivalent wire.
/// 验证内置 MiniMax Profile 产生与旧版等价的 Wire 请求。
#[test]
fn minimax_manifest_request_profiles_produce_legacy_equivalent_wire() {
    let client = projection_test_client();
    let chat_route = bundled_manifest_route("minimax", "global", OPENAI_CHAT_PROFILE_ID);
    let anthropic_route =
        bundled_manifest_route("minimax", "global", ANTHROPIC_MESSAGES_PROFILE_ID);
    assert!(
        !chat_route.request_projection_profiles.is_empty()
            && !anthropic_route.request_projection_profiles.is_empty(),
        "bundled MiniMax routes must install manifest request profiles"
    );
    let mut request = minimal_request();
    request.model_id = String::from("MiniMax-M3");

    // Chat adaptive + reasoning split.
    // Chat自适应思考与思考输出拆分同时生效。
    request.reasoning_effort = Some(ReasoningEffort::Adaptive);
    request.model_parameters.insert(
        MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
        ModelParameterValue::Boolean(true),
    );
    let encoded = client
        .encode_request(&chat_route, &request)
        .expect("MiniMax Chat request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "adaptive");
    assert_eq!(encoded.body["reasoning_split"], true);
    assert!(encoded.body.get("reasoning_effort").is_none());

    // Anthropic disabled thinking.
    // Anthropic显式关闭思考。
    request.reasoning_effort = Some(ReasoningEffort::None);
    request
        .model_parameters
        .remove(MINIMAX_REASONING_SPLIT_PARAMETER_ID);
    let encoded = client
        .encode_request(&anthropic_route, &request)
        .expect("MiniMax Anthropic request should encode");
    assert_eq!(encoded.body["thinking"]["type"], "disabled");
    assert!(encoded.body.get("output_config").is_none());

    // Anthropic rejects reasoning_split with the controlled legacy message.
    // Anthropic使用既有受控错误拒绝reasoning_split。
    request.model_parameters.insert(
        MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
        ModelParameterValue::Boolean(true),
    );
    let error = client
        .encode_request(&anthropic_route, &request)
        .expect_err("MiniMax Anthropic must reject reasoning_split");
    assert!(
        error
            .error
            .message
            .contains("registered only for OpenAI Chat"),
        "unexpected error: {}",
        error.error.message
    );
}

/// Verifies the formal M3.1 Flash rule and every explicit reasoning effort across credential scopes.
/// 验证正式版 M3.1 Flash 在各凭据作用域中的规则与全部显式推理档位。
#[test]
fn minimax_m3_1_flash_projects_reasoning_efforts() {
    // manifest is the compiled bundled authority used to resolve credential-scoped model rules.
    // manifest 是解析凭据作用域模型规则所使用的已编译内置权威清单。
    let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
    let manifest = manifests
        .get("minimax")
        .expect("MiniMax manifest must be bundled");
    // tokenPlanRule resolves the formal model independently of the older M3 reasoning toggle.
    // tokenPlanRule 独立于旧 M3 思考开关解析正式版模型。
    let token_plan_rule = manifest
        .rule_for_model("global", Some("token-plan-key"), "MiniMax-M3.1-Flash")
        .expect("Token Plan M3.1 Flash rule must resolve");
    assert_eq!(
        token_plan_rule.selected_protocol.as_deref(),
        Some(ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID)
    );
    // capabilities is the fully inherited exact-rule contract compiled for both Token Plan routes.
    // capabilities 是为两个 Token Plan 路由编译出的完整继承后精确规则契约。
    let capabilities = token_plan_rule
        .capabilities
        .as_ref()
        .expect("M3.1 Flash capabilities must compile");
    assert_eq!(capabilities.context_limit, Some(1_000_000));
    assert_eq!(capabilities.output_limit, Some(128_000));
    assert_eq!(
        capabilities.reasoning_efforts,
        vec![
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::XHigh,
            ReasoningEffort::Max,
        ]
    );
    // serviceEntry and credentialProfile exercise both regions and every supported credential kind.
    // serviceEntry 与 credentialProfile 覆盖两个区域及每种受支持的凭据类型。
    for service_entry in ["global", "cn"] {
        for credential_profile in ["payg-api-key", "token-plan-key", "token-plan-oauth"] {
            // formalRule must retain the exact effort contract instead of inheriting the M3 toggle.
            // formalRule 必须保留精确的档位契约，不能继承 M3 的思考开关。
            let formal_rule = manifest
                .rule_for_model(
                    service_entry,
                    Some(credential_profile),
                    "MiniMax-M3.1-Flash",
                )
                .expect("formal M3.1 Flash rule must resolve for this credential");
            assert_eq!(formal_rule.rule_id, token_plan_rule.rule_id);
        }
    }

    // effortProfile is the model-owned projection that replaces the older adaptive-thinking chain.
    // effortProfile 是替换旧自适应思考链的模型专属投影。
    let effort_profile = manifest
        .request_profiles
        .iter()
        .find(|profile| profile.profile_id == "minimax.anthropic.m3-1-flash-effort.v1")
        .and_then(|profile| profile.payload.clone())
        .expect("M3.1 Flash effort profile must compile");
    // route uses the exact multimodal Anthropic protocol selected by the model rule.
    // route 使用模型规则选定的精确多模态 Anthropic 协议。
    let mut route =
        bundled_manifest_route("minimax", "global", ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID);
    route.request_projection_profiles = vec![effort_profile];
    // client runs the production protocol encoder followed by the model request profile.
    // client 依次运行生产协议编码器与模型请求投影。
    let client = projection_test_client();
    // efforts is the closed effort set observed in the official managed MiniMax Code catalog.
    // efforts 是官方 MiniMax Code 管理目录中观测到的封闭档位集合。
    let efforts = [
        (ReasoningEffort::Low, "low"),
        (ReasoningEffort::Medium, "medium"),
        (ReasoningEffort::High, "high"),
        (ReasoningEffort::XHigh, "xhigh"),
        (ReasoningEffort::Max, "max"),
    ];
    for (effort, wire_label) in efforts {
        // request carries one exact user-selected effort without a synthetic thinking toggle.
        // request 携带一个精确的用户所选档位，且不引入虚构的思考开关。
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3.1-Flash");
        request.reasoning_effort = Some(effort);
        // encoded must match MiniMax Code's forced-on output_config effort contract.
        // encoded 必须符合 MiniMax Code 强制开启思考时的 output_config 档位契约。
        let encoded = client
            .encode_request(&route, &request)
            .expect("M3.1 Flash explicit effort should encode");
        assert_eq!(encoded.body["output_config"]["effort"], wire_label);
        assert!(encoded.body.get("thinking").is_none());
    }

    // defaultRequest represents the user-visible default option and uses the documented maximum effort.
    // defaultRequest 表示用户可见的默认选项，使用文档声明的最高思考档位。
    let mut default_request = minimal_request();
    default_request.model_id = String::from("MiniMax-M3.1-Flash");
    // defaultEncoded must never send the retired Preview-only default effort label.
    // defaultEncoded 不能继续发送已退役 Preview 专属的 default 档位标签。
    let default_encoded = client
        .encode_request(&route, &default_request)
        .expect("M3.1 Flash provider default should encode");
    assert!(default_encoded.body.get("thinking").is_none());
    assert_eq!(default_encoded.body["output_config"]["effort"], "max");
}

/// Verifies every TokenRhythm Messages rule on the reasoning profile sends each declared effort.
/// 验证使用推理 Profile 的每条 TokenRhythm Messages 规则发送每个声明档位。
///
/// The compatible projector writes no Messages effort, and TokenRhythm rejects
/// `output_config.effort: "none"`; so `none` must become `thinking: {"type": "disabled"}` without
/// `output_config`, and every other declared effort must reach `output_config.effort` unchanged.
/// 兼容投影器不写 Messages 档位，TokenRhythm 又拒绝 `output_config.effort: "none"`；因此 `none`
/// 必须变为不带 `output_config` 的 `thinking: {"type": "disabled"}`，其他声明档位原样写入
/// `output_config.effort`。
#[test]
fn tokenrhythm_messages_reasoning_profile_sends_declared_efforts() {
    const PROFILE_ID: &str = "tokenrhythm.messages.reasoning.toggle.v1";
    let client = projection_test_client();
    let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
    let manifest = manifests
        .get("tokenrhythm")
        .expect("TokenRhythm manifest must be bundled");
    // profileRules are the Messages rules that reference the TokenRhythm reasoning profile.
    // profileRules 是引用 TokenRhythm 推理 Profile 的 Messages 规则。
    let profile_rules = manifest
        .rule_instances
        .iter()
        .filter(|rule| rule.selected_protocol.as_deref() == Some(ANTHROPIC_MESSAGES_PROFILE_ID))
        .filter(|rule| rule.request_profile_refs.iter().any(|id| id == PROFILE_ID))
        .collect::<Vec<_>>();
    assert!(
        !profile_rules.is_empty(),
        "expected TokenRhythm Messages rules on `{PROFILE_ID}`; the manifest may have changed"
    );
    for rule in profile_rules {
        let mut route =
            bundled_manifest_route("tokenrhythm", "public-api", ANTHROPIC_MESSAGES_PROFILE_ID);
        route.request_projection_profiles = rule
            .request_profile_refs
            .iter()
            .map(|profile_id| {
                manifest
                    .request_profiles
                    .iter()
                    .find(|profile| &profile.profile_id == profile_id)
                    .and_then(|profile| profile.payload.clone())
                    .expect("referenced request profile must compile")
            })
            .collect();
        let mut request = minimal_request();
        request.model_id = rule.rule_id.clone();
        let efforts = rule
            .capabilities
            .as_ref()
            .map(|capabilities| capabilities.reasoning_efforts.clone())
            .unwrap_or_default();
        assert!(
            !efforts.is_empty(),
            "rule {} declares no efforts",
            rule.rule_id
        );
        for effort in efforts {
            request.reasoning_effort = Some(effort);
            let encoded = client
                .encode_request(&route, &request)
                .expect("TokenRhythm Messages reasoning request should encode");
            let context = format!("rule {} effort {}", rule.rule_id, effort.as_str());
            if effort == ReasoningEffort::None {
                assert_eq!(encoded.body["thinking"]["type"], "disabled", "{context}");
                assert!(encoded.body.get("output_config").is_none(), "{context}");
            } else {
                assert!(encoded.body.get("thinking").is_none(), "{context}");
                assert_eq!(
                    encoded.body["output_config"]["effort"],
                    effort.as_str(),
                    "{context}"
                );
            }
        }
    }
}

/// Verifies the projection executor rejects an unsupported MiniMax effort like the legacy fn.
/// 验证投影执行器像旧版函数一样拒绝不支持的 MiniMax 推理档位。
#[test]
fn minimax_profile_rejects_unsupported_effort() {
    let client = projection_test_client();
    let chat_route = bundled_manifest_route("minimax", "global", OPENAI_CHAT_PROFILE_ID);
    let mut request = minimal_request();
    request.model_id = String::from("MiniMax-M3");
    request.reasoning_effort = Some(ReasoningEffort::High);
    let error = client
        .encode_request(&chat_route, &request)
        .expect_err("MiniMax must reject unsupported effort");
    assert!(
        error
            .error
            .message
            .contains("not in the declared mapping table"),
        "unexpected error: {}",
        error.error.message
    );
}
