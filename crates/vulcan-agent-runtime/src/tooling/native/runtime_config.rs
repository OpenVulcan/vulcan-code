//! Safe model-facing LuaSkills package configuration discovery tool.
//! 面向模型的安全 LuaSkills 技能包配置发现工具。

use super::*;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;

/// Parsed read-only action accepted by the `runtime-config` tool.
/// `runtime-config` 工具接受的已解析只读动作。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RuntimeConfigAction {
    /// Describe effective package configuration declarations without values.
    /// 描述不包含值的生效技能包配置声明。
    Describe,
    /// Validate one effective package configuration.
    /// 校验单个生效技能包配置。
    Validate,
}

/// Parsed argument payload accepted by the `runtime-config` tool.
/// `runtime-config` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeConfigArgs {
    /// Read-only configuration action selected by the model.
    /// 模型选择的只读配置动作。
    action: RuntimeConfigAction,
    /// Optional skill identifier for describe and required identifier for validate.
    /// describe 可选且 validate 必填的技能标识。
    skill_id: Option<String>,
}

/// Builds the AI-visible read-only LuaSkills package configuration tool.
/// 构建 AI 可见的只读 LuaSkills 技能包配置工具。
///
/// `runtime` is the live shared LuaSkills engine used by the current foundation.
/// `runtime` 是当前运行时基础使用的活动共享 LuaSkills 引擎。
///
/// Returns one native tool that exposes only effective describe and validate actions.
/// 返回一个仅公开生效 describe 与 validate 动作的原生工具。
pub(crate) fn build_runtime_config_tool(
    runtime: LuaSkillsRuntimeHandle,
) -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(RUNTIME_CONFIG_TOOL_ID),
        name: String::from(RUNTIME_CONFIG_TOOL_ID),
        description: String::from(
            "Inspect LuaSkills package configuration declarations and validation status. This read-only tool never returns configured values and cannot modify configuration.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };
    build_native_tool_definition(
        descriptor,
        runtime_config_args_schema(),
        move |request: NativeToolExecutionRequest| {
            let runtime = runtime.clone();
            Box::pin(async move { execute_runtime_config_tool(runtime, request).await })
        },
    )
}

/// Builds the strict model-facing JSON Schema for `runtime-config`.
/// 构建 `runtime-config` 的严格模型可见 JSON Schema。
///
/// Returns a schema that admits only describe and validate actions.
/// 返回一个仅允许 describe 与 validate 动作的 schema。
fn runtime_config_args_schema() -> Value {
    json!({
        "type": "object",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "const": "describe",
                        "description": "Inspect effective package configuration declarations without configured values."
                    },
                    "skill_id": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Optional effective skill package id. Omit it to describe every effective package declaration."
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "const": "validate",
                        "description": "Validate one effective package configuration without changing it."
                    },
                    "skill_id": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Required effective skill package id to validate."
                    }
                },
                "required": ["action", "skill_id"],
                "additionalProperties": false
            }
        ]
    })
}

/// Converts parsed model arguments into one host-constrained upstream dispatcher request.
/// 把已解析模型参数转换为受宿主约束的上游 dispatcher 请求。
///
/// `args` contains only fields admitted by the model-facing schema and strict decoder.
/// `args` 仅包含模型可见 schema 与严格解码器允许的字段。
///
/// Returns a read-only request with effective mode and value disclosure fixed by the host.
/// 返回由宿主固定为生效模式且禁止值披露的只读请求。
fn upstream_runtime_config_request(args: RuntimeConfigArgs) -> Result<Value, NativeToolError> {
    match args.action {
        RuntimeConfigAction::Describe => Ok(json!({
            "action": "describe",
            "skill_id": args.skill_id,
            "mode": "effective",
            "include_values": false
        })),
        RuntimeConfigAction::Validate => {
            let skill_id = args.skill_id.ok_or_else(|| {
                NativeToolError::new(
                    "Invalid runtime-config arguments: skill_id is required for validate",
                )
            })?;
            Ok(json!({
                "action": "validate",
                "skill_id": skill_id
            }))
        }
    }
}

/// Executes one read-only LuaSkills configuration request through the upstream dispatcher.
/// 通过上游 dispatcher 执行单次只读 LuaSkills 配置请求。
///
/// `runtime` owns the engine and `request` carries model-provided arguments.
/// `runtime` 持有引擎，`request` 携带模型提供的参数。
///
/// Returns the official structured LuaSkills response with values forcibly excluded.
/// 返回强制排除配置值的正式 LuaSkills 结构化响应。
async fn execute_runtime_config_tool(
    runtime: LuaSkillsRuntimeHandle,
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args = serde_json::from_value::<RuntimeConfigArgs>(request.arguments).map_err(|error| {
        NativeToolError::new(format!("Invalid runtime-config arguments: {error}"))
    })?;
    let upstream_request = upstream_runtime_config_request(args)?;
    let request_text = serde_json::to_string(&upstream_request).map_err(|error| {
        NativeToolError::new(format!("Failed to encode runtime-config request: {error}"))
    })?;
    let response_text = runtime
        .engine
        .lock()
        .map_err(|_| NativeToolError::new("LuaSkills runtime lock is poisoned"))?
        .dispatch_runtime_config_tool_json(&request_text);
    let response = serde_json::from_str::<Value>(&response_text).map_err(|error| {
        NativeToolError::new(format!("Failed to decode runtime-config response: {error}"))
    })?;
    Ok(VulcanToolInvocationResult::from_model_output(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the model-facing schema exposes only read-only configuration actions.
    /// 验证模型可见 schema 仅公开只读配置动作。
    #[test]
    fn runtime_config_schema_exposes_only_read_actions() {
        let schema = runtime_config_args_schema();
        assert_eq!(
            schema["oneOf"][0]["properties"]["action"]["const"],
            json!("describe")
        );
        assert_eq!(
            schema["oneOf"][1]["properties"]["action"]["const"],
            json!("validate")
        );
        assert_eq!(
            schema["oneOf"][1]["required"],
            json!(["action", "skill_id"])
        );
        assert_eq!(schema["oneOf"][0]["additionalProperties"], json!(false));
        assert_eq!(schema["oneOf"][1]["additionalProperties"], json!(false));
    }

    /// Verifies strict decoding rejects mutation actions and host-private disclosure fields.
    /// 验证严格解码会拒绝变更动作与宿主私有披露字段。
    #[test]
    fn runtime_config_arguments_reject_mutation_and_disclosure_fields() {
        assert!(
            serde_json::from_value::<RuntimeConfigArgs>(json!({
                "action": "set",
                "skill_id": "fixture"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<RuntimeConfigArgs>(json!({
                "action": "describe",
                "include_values": true
            }))
            .is_err()
        );
    }

    /// Verifies host request construction forces effective mode and excludes configured values.
    /// 验证宿主请求构造会强制生效模式并排除已配置值。
    #[test]
    fn runtime_config_request_forces_safe_describe_and_validate_shapes() {
        let describe = upstream_runtime_config_request(RuntimeConfigArgs {
            action: RuntimeConfigAction::Describe,
            skill_id: Some(String::from("fixture")),
        })
        .expect("build describe request");
        assert_eq!(describe["mode"], json!("effective"));
        assert_eq!(describe["include_values"], json!(false));
        assert!(describe.get("root_name").is_none());

        let validate = upstream_runtime_config_request(RuntimeConfigArgs {
            action: RuntimeConfigAction::Validate,
            skill_id: None,
        });
        assert!(validate.is_err());
    }

    /// Verifies the Gemini-specific projection preserves both strict runtime-config schema branches.
    /// 验证 Gemini 专用投影会保留两个严格 runtime-config Schema 分支。
    #[test]
    fn runtime_config_schema_survives_gemini_projection() {
        let schema = vulcan_tools_manager::project_tool_parameters_schema(
            &runtime_config_args_schema(),
            vulcan_tools_manager::VulcanToolSchemaProjectionProfile::GeminiSlim,
        );

        assert_eq!(schema["type"], json!("object"));
        assert_eq!(
            schema["oneOf"][0]["type"],
            json!("object"),
            "projected schema: {schema}"
        );
        assert_eq!(schema["oneOf"][1]["type"], json!("object"));
        assert_eq!(
            schema["oneOf"][1]["required"],
            json!(["action", "skill_id"])
        );
        assert!(schema["oneOf"][1]["properties"]["skill_id"].is_object());
    }
}
