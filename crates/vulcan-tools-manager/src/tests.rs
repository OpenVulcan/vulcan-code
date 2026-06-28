use super::{
    ResolvedToolSet, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
    VulcanToolInvocationResult, VulcanToolMode, VulcanToolSchemaProjectionContext,
    VulcanToolSchemaProjectionProfile, VulcanToolSourceKind, VulcanToolsManager,
    project_tool_parameters_schema, public_tool_name_is_valid,
    resolve_tool_schema_projection_profile,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// Builds one inert candidate descriptor for public-name allocation tests.
/// 为公开名称分配测试构建单个惰性候选工具描述。
fn test_candidate(
    id: &str,
    source: VulcanToolSourceKind,
    plan_policy: VulcanPlanToolPolicy,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: id.to_string(),
            name: id.to_string(),
            description: format!("测试工具 {id}"),
            help_text: None,
            source,
        },
        parameters_schema: json!({"type": "object"}),
        ai_visible: true,
        plan_policy,
        invoke: Arc::new(|_args: Value, _session_id| {
            Box::pin(async { Ok(VulcanToolInvocationResult::from_model_output(json!("ok"))) })
        }),
    }
}

/// Extracts resolved tool names from one resolved tool set.
/// 从一份已解析工具集合中提取公开工具名。
fn resolved_names(tool_set: ResolvedToolSet) -> Vec<String> {
    tool_set
        .definitions
        .into_iter()
        .map(|definition| definition.descriptor.name)
        .collect()
}

/// Verifies strict provider-safe public tool-name validation.
/// 验证严格 Provider 安全公开工具名校验。
#[test]
fn public_tool_name_validation_rejects_common_provider_breakers() {
    assert!(public_tool_name_is_valid("mcp_server_read_file"));
    assert!(!public_tool_name_is_valid("workspace.describe"));
    assert!(!public_tool_name_is_valid("registry/list-skills"));
    assert!(!public_tool_name_is_valid("1tool"));
}

/// Verifies native names keep highest priority while LuaSkills collisions gain the readable `ls_` prefix.
/// 验证 native 名称保持最高优先级，而 LuaSkills 冲突会获得可读的 `ls_` 前缀。
#[test]
fn manager_allocates_luaskills_collision_with_ls_prefix() {
    let manager = VulcanToolsManager::new(vec![
        test_candidate(
            "abc_abc",
            VulcanToolSourceKind::Native,
            VulcanPlanToolPolicy::Visible,
        ),
        test_candidate(
            "abc-abc",
            VulcanToolSourceKind::LuaSkills,
            VulcanPlanToolPolicy::Visible,
        ),
    ]);

    assert_eq!(
        resolved_names(manager.resolve_for_execution_mode("Build")),
        vec!["abc_abc", "ls_abc_abc"]
    );
}

/// Verifies MCP tools always retain an `mcp_` public prefix and collide through numeric suffixes.
/// 验证 MCP 工具始终保留 `mcp_` 公开前缀，并通过数字后缀处理冲突。
#[test]
fn manager_allocates_mcp_names_with_prefix_and_suffixes() {
    let manager = VulcanToolsManager::new(vec![
        test_candidate(
            "server/read-file",
            VulcanToolSourceKind::Mcp,
            VulcanPlanToolPolicy::Visible,
        ),
        test_candidate(
            "server.read_file",
            VulcanToolSourceKind::Mcp,
            VulcanPlanToolPolicy::Visible,
        ),
    ]);

    assert_eq!(
        resolved_names(manager.resolve_for_execution_mode("Build")),
        vec!["mcp_server_read_file", "mcp_server_read_file_1"]
    );
}

/// Verifies Plan mode keeps the provider-visible tool surface stable after public-name normalization.
/// 验证 Plan 模式会在公开名称归一化后保持 Provider 可见工具面稳定。
#[test]
fn manager_keeps_plan_mode_tool_surface_stable_after_public_name_normalization() {
    let manager = VulcanToolsManager::new(vec![
        test_candidate(
            "read-file",
            VulcanToolSourceKind::Native,
            VulcanPlanToolPolicy::HeuristicText(String::from("read file")),
        ),
        test_candidate(
            "write-file",
            VulcanToolSourceKind::Native,
            VulcanPlanToolPolicy::Hidden,
        ),
    ]);

    assert_eq!(
        VulcanToolMode::from_execution_mode_label("Plan"),
        VulcanToolMode::Plan
    );
    assert_eq!(
        resolved_names(manager.resolve_for_execution_mode("Plan")),
        vec!["read_file", "write_file"]
    );
}

/// Verifies ripgrep-style rg tools are treated as Plan-mode read-only exploration.
/// 验证 rg 风格工具会在计划模式中被视为只读探索工具。
#[test]
fn manager_allows_rg_heuristic_tool_in_plan_mode() {
    let manager = VulcanToolsManager::new(vec![test_candidate(
        "codekit-rg",
        VulcanToolSourceKind::LuaSkills,
        VulcanPlanToolPolicy::HeuristicText(String::from("rg")),
    )]);
    let resolved = manager.resolve_for_execution_mode("Plan");
    let tool = resolved
        .definitions
        .first()
        .expect("resolved rg tool should exist");

    assert!(super::tool_is_allowed_in_plan(tool));
}

/// Verifies native command-line tools remain executable in Plan mode for exploration and validation.
/// 验证原生命令行工具在计划模式下仍可用于探索与验证。
#[test]
fn manager_allows_command_line_tools_in_plan_mode() {
    let manager = VulcanToolsManager::new(vec![
        test_candidate(
            "shell_command",
            VulcanToolSourceKind::Native,
            VulcanPlanToolPolicy::Visible,
        ),
        test_candidate(
            "terminal_session",
            VulcanToolSourceKind::Native,
            VulcanPlanToolPolicy::Visible,
        ),
    ]);
    let resolved = manager.resolve_for_execution_mode("Plan");

    assert_eq!(
        resolved_names(resolved.clone()),
        vec!["shell_command", "terminal_session"]
    );
    assert!(
        resolved
            .definitions
            .iter()
            .all(super::tool_is_allowed_in_plan)
    );
}

/// Verifies OpenRouter Gemini-family models select the compact Gemini projection even on an OpenAI-compatible wire API.
/// 验证 OpenRouter 的 Gemini 模型即使走 OpenAI 兼容线协议也会选择紧凑 Gemini 投影。
#[test]
fn projection_profile_detects_openrouter_gemini_models() {
    let context = VulcanToolSchemaProjectionContext {
        provider_id: "openrouter",
        api_family_key: "openai-chat-compatible",
        model_id: "google/gemini-2.5-pro",
        additional_params: None,
    };

    assert_eq!(
        resolve_tool_schema_projection_profile(&context),
        VulcanToolSchemaProjectionProfile::GeminiSlim
    );
}

/// Verifies OpenRouter pinned endpoint hints can select the Gemini projection for aliased models.
/// 验证 OpenRouter 固定端点提示可为别名模型选择 Gemini 投影。
#[test]
fn projection_profile_detects_openrouter_gemini_endpoint_hints() {
    let params = json!({
        "provider": {
            "order": ["google-ai-studio"],
            "allow_fallbacks": false
        }
    });
    let context = VulcanToolSchemaProjectionContext {
        provider_id: "openrouter",
        api_family_key: "openai-chat-compatible",
        model_id: "alias/model",
        additional_params: Some(&params),
    };

    assert_eq!(
        resolve_tool_schema_projection_profile(&context),
        VulcanToolSchemaProjectionProfile::GeminiSlim
    );
}

/// Verifies the Gemini projection keeps mode selectors compact while hydrating only required branch properties.
/// 验证 Gemini 投影保持模式选择器紧凑，同时仅补充分支必填字段对应的属性 Schema。
#[test]
fn gemini_slim_projection_keeps_oneof_branches_required_only() {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "file": { "type": "string", "description": "Target file." },
            "replacement": { "type": "string" },
            "patches": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "file": { "type": "string" },
                        "replacement": { "type": "string" }
                    },
                    "required": ["file", "replacement"]
                }
            }
        },
        "oneOf": [
            { "required": ["file", "replacement"] },
            { "required": ["patches"] }
        ]
    });

    let projected =
        project_tool_parameters_schema(&schema, VulcanToolSchemaProjectionProfile::GeminiSlim);

    assert_eq!(projected["type"], json!("object"));
    assert!(projected["additionalProperties"].is_null());
    assert_eq!(
        projected["oneOf"],
        json!([
            {
                "type": "object",
                "required": ["file", "replacement"],
                "properties": {
                    "file": { "type": "string", "description": "Target file." },
                    "replacement": { "type": "string" }
                }
            },
            {
                "type": "object",
                "required": ["patches"],
                "properties": {
                    "patches": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "file": { "type": "string" },
                                "replacement": { "type": "string" }
                            },
                            "required": ["file", "replacement"]
                        }
                    }
                }
            }
        ])
    );
    assert!(projected["oneOf"][0]["properties"]["patches"].is_null());
}

/// Verifies nested Gemini `anyOf` required-only aliases hydrate minimal required properties.
/// 验证嵌套 Gemini `anyOf` 必填别名分支会补充最小必填属性 Schema。
#[test]
fn gemini_slim_projection_marks_nested_required_only_anyof_branches_as_objects() {
    let schema = json!({
        "type": "object",
        "properties": {
            "basic": {
                "type": "object",
                "description": "Optional basic auth shortcut object.",
                "properties": {
                    "username": { "type": "string" },
                    "password": { "type": "string" },
                    "user": { "type": "string" },
                    "pass": { "type": "string" }
                },
                "anyOf": [
                    { "required": ["username", "password"] },
                    { "required": ["username", "pass"] },
                    { "required": ["user", "password"] },
                    { "required": ["user", "pass"] }
                ]
            }
        }
    });

    let projected =
        project_tool_parameters_schema(&schema, VulcanToolSchemaProjectionProfile::GeminiSlim);

    assert_eq!(
        projected["properties"]["basic"]["anyOf"],
        json!([
            {
                "type": "object",
                "required": ["username", "password"],
                "properties": {
                    "username": { "type": "string" },
                    "password": { "type": "string" }
                }
            },
            {
                "type": "object",
                "required": ["username", "pass"],
                "properties": {
                    "username": { "type": "string" },
                    "pass": { "type": "string" }
                }
            },
            {
                "type": "object",
                "required": ["user", "password"],
                "properties": {
                    "user": { "type": "string" },
                    "password": { "type": "string" }
                }
            },
            {
                "type": "object",
                "required": ["user", "pass"],
                "properties": {
                    "user": { "type": "string" },
                    "pass": { "type": "string" }
                }
            }
        ])
    );
    assert!(projected["properties"]["basic"]["anyOf"][0]["properties"]["user"].is_null());
}

/// Verifies the Gemini projection collapses nullable multi-type arrays without expanding them into token-heavy `anyOf` branches.
/// 验证 Gemini 投影会压缩可空多类型数组，而不会膨胀成高 Token 成本的 `anyOf` 分支。
#[test]
fn gemini_slim_projection_collapses_nullable_type_arrays() {
    let schema = json!({
        "type": "object",
        "properties": {
            "value": {
                "type": ["string", "null"],
                "description": "Optional value."
            }
        }
    });

    let projected =
        project_tool_parameters_schema(&schema, VulcanToolSchemaProjectionProfile::GeminiSlim);

    assert_eq!(projected["properties"]["value"]["type"], json!("string"));
    assert_eq!(projected["properties"]["value"]["nullable"], json!(true));
    assert!(projected["properties"]["value"]["anyOf"].is_null());
}

/// Verifies OpenAI Responses strict projection still adds strict object constraints recursively.
/// 验证 OpenAI Responses 严格投影仍会递归补齐严格对象约束。
#[test]
fn openai_responses_strict_projection_adds_required_object_constraints() {
    let schema = json!({
        "type": "object",
        "properties": {
            "location": {
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                }
            }
        }
    });

    let projected = project_tool_parameters_schema(
        &schema,
        VulcanToolSchemaProjectionProfile::OpenAiResponsesStrict,
    );

    assert_eq!(projected["additionalProperties"], json!(false));
    assert_eq!(projected["required"], json!(["location"]));
    assert_eq!(
        projected["properties"]["location"]["additionalProperties"],
        json!(false)
    );
    assert_eq!(
        projected["properties"]["location"]["required"],
        json!(["city"])
    );
}
