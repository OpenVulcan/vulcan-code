use super::{
    ResolvedToolSet, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
    VulcanToolInvocationResult, VulcanToolMode, VulcanToolSchemaProjectionProfile,
    VulcanToolSourceKind, VulcanToolsManager, project_tool_parameters_schema,
    public_tool_name_is_valid,
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
        request_streaming: None,
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

/// Verifies Vulcan-managed tools preserve their direct public identity without an MCP prefix.
/// 验证 Vulcan 托管工具保留直接公开身份且不会增加 MCP 前缀。
#[test]
fn manager_preserves_vulcan_managed_public_name() {
    let manager = VulcanToolsManager::new(vec![test_candidate(
        "vulcan_codebase_memory_search_graph",
        VulcanToolSourceKind::VulcanManaged,
        VulcanPlanToolPolicy::Visible,
    )]);

    assert_eq!(
        resolved_names(manager.resolve_for_execution_mode("Build")),
        vec!["vulcan_codebase_memory_search_graph"]
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

/// Verifies Gemini projection preserves a meaningful root object defined only by combinator branches.
/// 验证 Gemini 投影会保留仅由组合分支定义的有效根对象。
#[test]
fn gemini_slim_projection_preserves_combinator_only_root_objects() {
    let schema = json!({
        "type": "object",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "const": "describe" }
                },
                "required": ["action"]
            },
            {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "const": "validate" },
                    "skill_id": { "type": "string" }
                },
                "required": ["action", "skill_id"]
            }
        ]
    });

    let projected =
        project_tool_parameters_schema(&schema, VulcanToolSchemaProjectionProfile::GeminiSlim);

    assert_eq!(projected["type"], json!("object"));
    assert_eq!(projected["oneOf"][0]["type"], json!("object"));
    assert_eq!(
        projected["oneOf"][1]["required"],
        json!(["action", "skill_id"])
    );
}

/// Verifies explicit pass-through projection preserves every canonical schema field exactly.
/// 验证显式原样投影会精确保留规范 Schema 的每个字段。
#[test]
fn pass_through_projection_preserves_canonical_schema_exactly() {
    let schema = json!({
        "type": "object",
        "additionalProperties": true,
        "properties": {
            "location": {
                "type": "object",
                "description": "Optional location.",
                "properties": {
                    "city": { "type": "string" }
                },
                "x-vulcan-extension": { "preserve": true }
            }
        },
        "$defs": {
            "unused": { "type": ["string", "null"] }
        }
    });

    let projected =
        project_tool_parameters_schema(&schema, VulcanToolSchemaProjectionProfile::PassThrough);

    assert_eq!(projected, schema);
}

/// Verifies lightweight projection recursively removes composition keywords from CodeKit-style schemas.
/// 验证轻量投影会递归移除 CodeKit 风格 Schema 中的组合关键字。
#[test]
fn standard_object_root_projection_lightly_normalizes_codekit_style_schema() {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "file": { "type": "string" },
            "patches": {
                "type": "array",
                "items": { "type": "object" }
            },
            "precondition": {
                "type": "object",
                "properties": {
                    "range": {
                        "anyOf": [
                            { "type": "string" },
                            {
                                "type": "object",
                                "properties": {
                                    "start_line": { "type": "integer" },
                                    "end_line": { "type": "integer" }
                                },
                                "required": ["start_line", "end_line"]
                            }
                        ]
                    }
                }
            }
        },
        "oneOf": [
            { "required": ["file"] },
            { "required": ["patches"] }
        ]
    });

    let projected = project_tool_parameters_schema(
        &schema,
        VulcanToolSchemaProjectionProfile::StandardObjectRoot,
    );

    assert_eq!(projected["type"], json!("object"));
    assert!(projected.get("oneOf").is_none());
    assert_eq!(projected["properties"]["file"]["type"], json!("string"));
    assert_eq!(
        projected["properties"]["patches"]["items"]["type"],
        json!("object")
    );
    assert_eq!(
        projected["properties"]["precondition"]["properties"]["range"]["type"],
        json!("string")
    );
    assert!(
        projected["properties"]["precondition"]["properties"]["range"]
            .get("anyOf")
            .is_none()
    );
    assert_eq!(projected["additionalProperties"], json!(false));
}

/// Verifies explicit closed empty objects use the portable no-argument schema shape.
/// 验证显式封闭空对象使用可移植的无参数 Schema 结构。
#[test]
fn standard_object_root_projection_normalizes_explicit_empty_closed_object() {
    let schema = json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    });

    let projected = project_tool_parameters_schema(
        &schema,
        VulcanToolSchemaProjectionProfile::StandardObjectRoot,
    );

    assert_eq!(
        projected,
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    );
}

/// Verifies open empty objects retain their dynamic-property semantics during projection.
/// 验证开放空对象在投影期间保留动态属性语义。
#[test]
fn standard_object_root_projection_preserves_open_empty_object() {
    let schema = json!({
        "type": "object",
        "properties": {},
        "additionalProperties": true
    });

    let projected = project_tool_parameters_schema(
        &schema,
        VulcanToolSchemaProjectionProfile::StandardObjectRoot,
    );

    assert_eq!(projected["type"], json!("object"));
    assert_eq!(projected["properties"], json!({}));
    assert_eq!(projected["additionalProperties"], json!(true));
    assert!(projected.get("required").is_none());
}

/// Verifies Moonshot-compatible lightweight projection removes nested auth alternatives while retaining their basic fields.
/// 验证 Moonshot 兼容的轻量投影会移除嵌套认证备选约束，同时保留基础字段。
#[test]
fn standard_object_root_projection_removes_nested_moonshot_anyof() {
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

    let projected = project_tool_parameters_schema(
        &schema,
        VulcanToolSchemaProjectionProfile::StandardObjectRoot,
    );
    let basic = &projected["properties"]["basic"];

    assert_eq!(basic["type"], json!("object"));
    assert_eq!(
        basic["description"],
        json!("Optional basic auth shortcut object.")
    );
    assert_eq!(basic["properties"]["username"]["type"], json!("string"));
    assert_eq!(basic["properties"]["password"]["type"], json!("string"));
    assert_eq!(basic["properties"]["user"]["type"], json!("string"));
    assert_eq!(basic["properties"]["pass"]["type"], json!("string"));
    assert!(basic.get("anyOf").is_none());
    assert!(basic.get("required").is_none());
}

/// Verifies lightweight projection merges object alternatives into one provider-portable action schema.
/// 验证轻量投影会把对象备选分支合并为单个 Provider 可移植操作 Schema。
#[test]
fn standard_object_root_projection_merges_object_alternatives() {
    let schema = json!({
        "type": "object",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "const": "describe" }
                },
                "required": ["action"]
            },
            {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "const": "validate" },
                    "skill_id": { "type": "string" }
                },
                "required": ["action", "skill_id"]
            }
        ]
    });

    let projected = project_tool_parameters_schema(
        &schema,
        VulcanToolSchemaProjectionProfile::StandardObjectRoot,
    );

    assert_eq!(projected["type"], json!("object"));
    assert!(projected.get("oneOf").is_none());
    assert_eq!(
        projected["properties"]["action"]["enum"],
        json!(["describe", "validate"])
    );
    assert_eq!(projected["properties"]["skill_id"]["type"], json!("string"));
    assert_eq!(projected["required"], json!(["action"]));
}

/// Verifies lightweight projection strips reference, conditional, and validation-only syntax at every depth.
/// 验证轻量投影会在每一层移除引用、条件与仅校验语法。
#[test]
fn standard_object_root_projection_strips_advanced_keywords_recursively() {
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "$defs": {
            "named": { "type": "string" }
        },
        "properties": {
            "value": {
                "type": ["string", "null"],
                "description": "Portable value.",
                "pattern": "^[a-z]+$",
                "not": { "enum": ["blocked"] },
                "if": { "const": "special" },
                "then": { "minLength": 3 },
                "else": { "maxLength": 12 }
            },
            "reference_only": {
                "$ref": "#/$defs/named"
            },
            "const_only": {
                "const": "fixed"
            },
            "description_only": {
                "description": "No portable structural declaration."
            }
        },
        "dependentSchemas": {
            "value": { "required": ["reference_only"] }
        }
    });

    let projected = project_tool_parameters_schema(
        &schema,
        VulcanToolSchemaProjectionProfile::StandardObjectRoot,
    );
    let value = &projected["properties"]["value"];

    assert_eq!(value["type"], json!("string"));
    assert_eq!(value["description"], json!("Portable value."));
    for keyword in [
        "$schema",
        "$defs",
        "$ref",
        "pattern",
        "not",
        "if",
        "then",
        "else",
        "dependentSchemas",
    ] {
        assert!(!projected.to_string().contains(&format!("\"{keyword}\"")));
    }
    assert!(projected["properties"].get("reference_only").is_none());
    assert_eq!(
        projected["properties"]["const_only"],
        json!({ "type": "string", "enum": ["fixed"] })
    );
    assert!(projected["properties"].get("description_only").is_none());
}
