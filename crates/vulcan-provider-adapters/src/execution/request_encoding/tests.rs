//! Regression coverage for final structured-output and disabled-reasoning contracts.
//! 最终结构化输出与关闭思考契约的回归覆盖。

use super::super::test_support::{deepseek_route, minimal_request, pinned_openrouter_route};
use super::*;
use std::sync::Arc;
use vulcan_ai_protocols::ProtocolRegistry;

/// Verifies every schema-capable protocol retains one exact structured-output schema.
/// 验证每个支持 Schema 的协议都会保留同一份精确结构化输出 Schema。
#[test]
fn encode_request_preserves_structured_output_schema_for_supported_protocols() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let schema = json!({
        "type": "object",
        "properties": { "accepted": { "type": "boolean" } },
        "required": ["accepted"],
        "additionalProperties": false,
    });

    for protocol_id in [
        OPENAI_CHAT_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        ANTHROPIC_MESSAGES_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
    ] {
        let mut request = minimal_request();
        request.structured_output = StructuredOutputPolicy::JsonSchema {
            name: String::from("result"),
            description: Some(String::from("Structured result")),
            schema: schema.clone(),
            strict: true,
        };
        // route reuses one valid provider shell while selecting the exact protocol under test.
        // route 复用一个有效供应商外壳，同时选择当前测试的准确协议。
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.protocol_route.protocol_profile_id = protocol_id.to_owned();
        let encoded = client
            .encode_request(&route, &request)
            .expect("supported protocol should preserve structured output schema");
        let projected_schema = match protocol_id {
            OPENAI_CHAT_PROFILE_ID => encoded.body.pointer("/response_format/json_schema/schema"),
            OPENAI_RESPONSES_PROFILE_ID => encoded.body.pointer("/text/format/schema"),
            ANTHROPIC_MESSAGES_PROFILE_ID => encoded.body.pointer("/output_config/format/schema"),
            GEMINI_GENERATE_CONTENT_PROFILE_ID => {
                encoded.body.pointer("/generationConfig/responseJsonSchema")
            }
            GOOGLE_CODE_ASSIST_PROFILE_ID => encoded
                .body
                .pointer("/request/generationConfig/responseJsonSchema"),
            GEMINI_INTERACTIONS_PROFILE_ID => encoded.body.pointer("/response_format/schema"),
            _ => unreachable!("the loop only contains supported protocol profiles"),
        };
        assert_eq!(projected_schema, Some(&schema));
    }
}

/// Verifies every JSON-object protocol retains its native wire control after all projections.
/// 验证每个 JSON Object 协议在全部投影后仍保留其原生线路控制项。
#[test]
fn encode_request_preserves_json_object_mode_for_supported_protocols() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );

    for protocol_id in [
        OPENAI_CHAT_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
    ] {
        let mut request = minimal_request();
        request.structured_output = StructuredOutputPolicy::JsonObject;
        // route reuses one valid provider shell while selecting the exact protocol under test.
        // route 复用一个有效供应商外壳，同时选择当前测试的准确协议。
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.protocol_route.protocol_profile_id = protocol_id.to_owned();
        let encoded = client
            .encode_request(&route, &request)
            .expect("supported protocol should preserve JSON Object mode");
        let projected_type = match protocol_id {
            OPENAI_CHAT_PROFILE_ID => encoded
                .body
                .pointer("/response_format/type")
                .and_then(Value::as_str),
            OPENAI_RESPONSES_PROFILE_ID => encoded
                .body
                .pointer("/text/format/type")
                .and_then(Value::as_str),
            GEMINI_GENERATE_CONTENT_PROFILE_ID => encoded
                .body
                .pointer("/generationConfig/responseMimeType")
                .and_then(Value::as_str),
            GOOGLE_CODE_ASSIST_PROFILE_ID => encoded
                .body
                .pointer("/request/generationConfig/responseMimeType")
                .and_then(Value::as_str),
            GEMINI_INTERACTIONS_PROFILE_ID => encoded
                .body
                .pointer("/response_format/mime_type")
                .and_then(Value::as_str),
            _ => unreachable!("the loop only contains supported protocol profiles"),
        };
        let expected_type = if matches!(
            protocol_id,
            GEMINI_GENERATE_CONTENT_PROFILE_ID
                | GOOGLE_CODE_ASSIST_PROFILE_ID
                | GEMINI_INTERACTIONS_PROFILE_ID
        ) {
            "application/json"
        } else {
            "json_object"
        };
        assert_eq!(projected_type, Some(expected_type));
    }
}

/// Verifies a late provider projection cannot silently remove JSON Object mode.
/// 验证后置供应商投影不能静默移除 JSON Object 模式。
#[test]
fn encode_request_rejects_json_object_mode_loss_after_custom_projection() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.custom_model_projection = Some(CustomModelRequestProjection {
        delete: vec![String::from("text")],
        ..Default::default()
    });
    let mut request = minimal_request();
    request.structured_output = StructuredOutputPolicy::JsonObject;

    let error = client
        .encode_request(&route, &request)
        .expect_err("JSON Object mode loss must fail before dispatch");
    assert!(error.error.message.contains("JSON Object mode"));
}

/// Verifies the exact DeepSeek Responses route used by native VMM sends JSON mode with reasoning off.
/// 验证原生 VMM 使用的准确 DeepSeek Responses 路由会发送 JSON 模式并关闭思考。
#[test]
fn deepseek_responses_preserves_vmm_json_and_disabled_reasoning_controls() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut request = minimal_request();
    request.structured_output = StructuredOutputPolicy::JsonObject;
    request.reasoning_effort = Some(ReasoningEffort::None);

    let encoded = client
        .encode_required_disabled_reasoning_request(
            &deepseek_route(OPENAI_RESPONSES_PROFILE_ID),
            &request,
            false,
        )
        .expect("DeepSeek VMM request must preserve both mandatory controls");

    assert_eq!(
        encoded.body.pointer("/text/format/type"),
        Some(&json!("json_object"))
    );
    assert_eq!(
        encoded.body.pointer("/reasoning/effort"),
        Some(&json!("none"))
    );
}

/// Verifies a custom projector cannot silently remove the required Anthropic schema.
/// 验证自定义投影器不能静默删除 Anthropic 所需的 Schema。
#[test]
fn encode_request_rejects_structured_schema_loss_after_custom_projection() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(ANTHROPIC_MESSAGES_PROFILE_ID);
    route.custom_model_projection = Some(CustomModelRequestProjection {
        delete: vec![String::from("output_config")],
        ..Default::default()
    });
    let mut request = minimal_request();
    request.structured_output = StructuredOutputPolicy::JsonSchema {
        name: String::from("result"),
        description: None,
        schema: json!({
            "type": "object",
            "properties": { "accepted": { "type": "boolean" } },
            "required": ["accepted"],
            "additionalProperties": false,
        }),
        strict: true,
    };

    let error = client
        .encode_request(&route, &request)
        .expect_err("schema loss must fail before dispatch");
    assert!(error.error.message.contains("dropped or changed"));
}

/// Verifies custom model projections apply typed parameters, reasoning, deletion, and overrides deterministically.
/// 验证自定义模型投影会确定性地应用强类型参数、推理、删除与强制覆盖。
#[test]
fn custom_model_projection_applies_registered_request_mutations() {
    let mut request = minimal_request();
    request.reasoning_effort = Some(ReasoningEffort::High);
    request.model_parameters.insert(
        String::from("vendor.temperature"),
        ModelParameterValue::Decimal(String::from("0.3")),
    );
    let projection = CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::High,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.mode"),
                    Value::String(String::from("enabled")),
                )]),
                delete: Vec::new(),
            },
        )]),
        model_parameter_paths: BTreeMap::from([(
            String::from("vendor.temperature"),
            String::from("sampling.temperature"),
        )]),
        defaults: BTreeMap::from([(String::from("vendor.keep"), Value::Bool(true))]),
        overrides: BTreeMap::from([(
            String::from("vendor.forced"),
            Value::String(String::from("yes")),
        )]),
        delete: vec![String::from("remove_me")],
    };
    let mut encoded = EncodedWireRequest {
        path: String::from("/v1/chat/completions"),
        headers: BTreeMap::new(),
        body: json!({"model": "custom-model", "messages": [], "remove_me": true}),
    };

    project_custom_model_request(&projection, &request, &mut encoded)
        .expect("custom projection should succeed");

    assert_eq!(encoded.body["thinking"]["mode"], "enabled");
    assert_eq!(encoded.body["sampling"]["temperature"], json!(0.3));
    assert_eq!(encoded.body["vendor"]["keep"], true);
    assert_eq!(encoded.body["vendor"]["forced"], "yes");
    assert!(encoded.body.get("remove_me").is_none());
}

/// Verifies a configured reasoning projection reaches the final encoded request body.
/// 验证已配置的思考投影会抵达最终编码请求体，而不是只停留在编辑器状态。
#[test]
fn encode_request_applies_custom_reasoning_projection_to_final_wire_body() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    route.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::High,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.type"),
                    Value::String(String::from("enabled")),
                )]),
                delete: vec![String::from("reasoning_effort")],
            },
        )]),
        ..Default::default()
    });
    let mut request = minimal_request();
    request.reasoning_effort = Some(ReasoningEffort::High);

    let encoded = client
        .encode_request(&route, &request)
        .expect("custom reasoning projection should encode");

    assert_eq!(encoded.body["thinking"]["type"], "enabled");
    assert!(encoded.body.get("reasoning_effort").is_none());
}

/// Verifies required no-reasoning validation inspects the final body and fails closed.
/// 验证必需无思考校验检查最终请求体并采用失败关闭策略。
#[test]
fn required_disabled_reasoning_validation_rejects_dropped_or_contradictory_projection() {
    // client owns the same encoder and projection chain used by production dispatch.
    // client 持有与生产分派相同的编码器与投影链。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // request is the exact VMM semantic: reasoning effort must be none.
    // request 是精确 VMM 语义：思考档位必须为 none。
    let mut request = minimal_request();
    request.reasoning_effort = Some(ReasoningEffort::None);

    // explicitRoute proves the OpenRouter profile retains reasoning.effort=none.
    // explicitRoute 证明 OpenRouter Profile 保留 reasoning.effort=none。
    let explicit_route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    let explicit = client
        .encode_required_disabled_reasoning_request(&explicit_route, &request, false)
        .expect("explicit OpenRouter disabled projection should pass");
    assert_eq!(explicit.body["reasoning"]["effort"], "none");

    // omissionRoute proves a provider may define none as deleting its exact Chat field.
    // omissionRoute 证明供应商可以把删除其精确 Chat 字段定义为 none 的线路语义。
    let mut omission_route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    omission_route.request_projection_profiles =
        vec![crate::provider_manifest::RequestProjectionProfile {
            profile_id: String::from("vendor.chat.omit-disabled.v1"),
            revision: 1,
            steps: vec![crate::provider_manifest::MappingStep::DeleteIfEquals {
                path: String::from("/reasoning_effort"),
                source: crate::provider_manifest::MappingStepSource::RequestField,
                source_path: String::from("@reasoning_effort"),
                equals: String::from("none"),
            }],
        }];
    let omission = client
        .encode_required_disabled_reasoning_request(&omission_route, &request, false)
        .expect("a profile-declared disabled omission should pass");
    assert!(omission.body.get("reasoning_effort").is_none());

    // droppedRoute removes the only Chat provider profile; canonical Chat conversion omits
    // effort none, so a switchable model must be rejected rather than using provider default.
    // droppedRoute 移除唯一的 Chat 供应商 Profile；规范 Chat 转换会省略 none 档位，
    // 因此可切换模型必须被拒绝，而不能使用供应商默认值。
    let mut dropped_route = explicit_route.clone();
    dropped_route.request_projection_profiles.clear();
    let dropped = client
        .encode_required_disabled_reasoning_request(&dropped_route, &request, false)
        .expect_err("a dropped disabled projection must fail closed");
    assert!(dropped.error.message.contains("does not preserve"));

    // fixedOff accepts an absent toggle only because the frozen model capability says the
    // model can never reason; no name-based inference is involved.
    // fixedOff 仅因冻结模型能力声明模型永远不会思考，才接受缺失开关；不涉及模型名推断。
    client
        .encode_required_disabled_reasoning_request(&dropped_route, &request, true)
        .expect("fixed-off models do not require a redundant wire toggle");

    // contradictoryRoute simulates a late custom projection that re-enables thinking after
    // the provider profile emitted none; this must fail even for a fixed-off declaration.
    // contradictoryRoute 模拟供应商 Profile 输出 none 后，后置自定义投影重新开启思考；
    // 即使声明为固定关闭也必须失败。
    let mut contradictory_route = explicit_route;
    contradictory_route.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.type"),
                    Value::String(String::from("enabled")),
                )]),
                delete: Vec::new(),
            },
        )]),
        ..Default::default()
    });
    let contradictory = client
        .encode_required_disabled_reasoning_request(&contradictory_route, &request, true)
        .expect_err("a contradictory enabled control must fail closed");
    assert!(contradictory.error.message.contains("enables reasoning"));

    // budgetRoute proves a disabled type cannot hide a positive reasoning budget left by a
    // later custom projection.
    // budgetRoute 证明关闭类型不能掩盖后置自定义投影残留的正数思考预算。
    let mut budget_route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    budget_route.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([
                    (
                        String::from("thinking.type"),
                        Value::String(String::from("disabled")),
                    ),
                    (
                        String::from("thinking.budget_tokens"),
                        Value::Number(4096.into()),
                    ),
                ]),
                delete: Vec::new(),
            },
        )]),
        ..Default::default()
    });
    let budget_conflict = client
        .encode_required_disabled_reasoning_request(&budget_route, &request, true)
        .expect_err("a non-zero reasoning budget must fail closed");
    assert!(budget_conflict.error.message.contains("enables reasoning"));

    // unknownTypeRoute rejects every non-disabled thinking type instead of recognizing only
    // the historical enabled/adaptive labels.
    // unknownTypeRoute 拒绝所有非关闭 thinking 类型，而不是只识别历史 enabled/adaptive 标签。
    let mut unknown_type_route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
    unknown_type_route.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.type"),
                    Value::String(String::from("high")),
                )]),
                delete: Vec::new(),
            },
        )]),
        ..Default::default()
    });
    let unknown_type = client
        .encode_required_disabled_reasoning_request(&unknown_type_route, &request, true)
        .expect_err("an unknown non-disabled thinking type must fail closed");
    assert!(unknown_type.error.message.contains("enables reasoning"));

    // crossDialectResponses removes the authoritative Responses effort and leaves an
    // Anthropic-shaped disabled marker; protocol-specific validation must reject the decoy.
    // crossDialectResponses 移除权威 Responses effort 并留下 Anthropic 形态的关闭标记；
    // 协议专属校验必须拒绝该诱饵字段。
    let mut cross_dialect_responses = deepseek_route(OPENAI_RESPONSES_PROFILE_ID);
    cross_dialect_responses.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("thinking.type"),
                    Value::String(String::from("disabled")),
                )]),
                delete: vec![String::from("reasoning.effort")],
            },
        )]),
        ..Default::default()
    });
    let cross_dialect_responses_failure = client
        .encode_required_disabled_reasoning_request(&cross_dialect_responses, &request, false)
        .expect_err("Responses must not accept an Anthropic-shaped disabled marker");
    assert!(
        cross_dialect_responses_failure
            .error
            .message
            .contains("does not preserve")
    );

    // crossDialectAnthropic removes the authoritative Anthropic thinking type and leaves a
    // Responses-shaped effort; the wrong protocol vocabulary must not satisfy VMM admission.
    // crossDialectAnthropic 移除权威 Anthropic thinking type 并留下 Responses 形态 effort；
    // 错误协议词汇不得满足 VMM 准入条件。
    let mut cross_dialect_anthropic = deepseek_route(ANTHROPIC_MESSAGES_PROFILE_ID);
    cross_dialect_anthropic.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("reasoning.effort"),
                    Value::String(String::from("none")),
                )]),
                delete: vec![String::from("thinking.type")],
            },
        )]),
        ..Default::default()
    });
    let cross_dialect_anthropic_failure = client
        .encode_required_disabled_reasoning_request(&cross_dialect_anthropic, &request, false)
        .expect_err("Anthropic must not accept a Responses-shaped disabled marker");
    assert!(
        cross_dialect_anthropic_failure
            .error
            .message
            .contains("does not preserve")
    );

    // crossDialectChat removes the DeepSeek Chat profile's authoritative thinking object
    // and leaves a Responses-shaped effort; only the profile-declared path may prove none.
    // crossDialectChat 移除 DeepSeek Chat Profile 的权威 thinking 对象并留下
    // Responses 形态的 effort；只有 Profile 声明的路径才能证明 none。
    let mut cross_dialect_chat = deepseek_route(OPENAI_CHAT_PROFILE_ID);
    cross_dialect_chat.custom_model_projection = Some(CustomModelRequestProjection {
        reasoning: BTreeMap::from([(
            ReasoningEffort::None,
            crate::CustomRequestMutation {
                set: BTreeMap::from([(
                    String::from("reasoning.effort"),
                    Value::String(String::from("none")),
                )]),
                delete: vec![String::from("thinking.type")],
            },
        )]),
        ..Default::default()
    });
    let cross_dialect_chat_failure = client
        .encode_required_disabled_reasoning_request(&cross_dialect_chat, &request, false)
        .expect_err("DeepSeek Chat must not accept a Responses-shaped disabled marker");
    assert!(
        cross_dialect_chat_failure
            .error
            .message
            .contains("does not preserve")
    );
}
