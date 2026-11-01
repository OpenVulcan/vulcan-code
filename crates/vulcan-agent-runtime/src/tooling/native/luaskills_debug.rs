//! `luaskills_debug`: reports and installs luaskills-debug, the standalone LuaSkills developer
//! tool, through the same host installer the LuaSkills settings page uses. `status` reads local
//! state; `install` downloads a release and is classified as an extension effect.
//! `luaskills_debug`：经由 LuaSkills 设置页使用的同一宿主安装器，报告并安装独立 LuaSkills 开发工具
//! luaskills-debug。`status` 读取本地状态；`install` 下载 release，并被归类为扩展效果。

use super::*;

/// Stable identifier of the luaskills-debug tool.
/// luaskills-debug 工具的稳定标识。
pub const LUASKILLS_DEBUG_TOOL_ID: &str = "luaskills_debug";

/// The only `luaskills_debug` action that reads instead of installing.
/// `luaskills_debug` 中唯一读取而非安装的动作。
pub const LUASKILLS_DEBUG_STATUS_ACTION: &str = "status";

/// One parsed `luaskills_debug` call.
/// 一次已解析的 `luaskills_debug` 调用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LuaSkillsDebugRequest {
    /// Report the installed version, its paths and whether its debug runtime is ready.
    /// 报告已安装版本、其路径以及调试运行时是否就绪。
    Status,
    /// Install the newest release that matches the embedded LuaSkills series.
    /// 安装与内嵌 LuaSkills 系列匹配的最新 release。
    Install,
}

/// Wire shape of the arguments; a struct, so unknown fields are refused.
/// 参数的传输形状；使用结构体，以便拒绝未知字段。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LuaSkillsDebugArguments {
    /// Requested action.
    /// 请求的动作。
    action: LuaSkillsDebugRequest,
}

impl LuaSkillsDebugRequest {
    /// Parses the arguments of one `luaskills_debug` call.
    /// 解析一次 `luaskills_debug` 调用的参数。
    pub fn parse(args: &Value) -> Result<Self, String> {
        serde_json::from_value::<LuaSkillsDebugArguments>(args.clone())
            .map(|arguments| arguments.action)
            .map_err(|error| format!("Invalid `{LUASKILLS_DEBUG_TOOL_ID}` arguments: {error}"))
    }
}

/// Result of one `luaskills_debug` call.
/// 一次 `luaskills_debug` 调用的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct LuaSkillsDebugOutcome {
    /// Result the model reads.
    /// 模型读取的结果。
    pub text: String,
    /// Structured result for clients.
    /// 供客户端使用的结构化结果。
    pub payload: Value,
}

/// Host-injected luaskills-debug port. The host owns the installer, so the tool and the settings
/// page install through the same code and share one running install.
/// 宿主注入的 luaskills-debug 端口。宿主拥有安装器，因此工具与设置页走同一代码并共享同一个进行中的安装。
#[async_trait::async_trait]
pub trait NativeLuaSkillsDebugHost: fmt::Debug + Send + Sync {
    /// Reads or installs luaskills-debug for `session_id`. An install runs only after the
    /// permission gate admitted it; the host still refuses installs from spawned agents.
    /// 为 `session_id` 读取或安装 luaskills-debug。安装只在权限闸门准入后运行；宿主仍拒绝派生代理的安装。
    async fn run(
        &self,
        session_id: SessionId,
        request: LuaSkillsDebugRequest,
    ) -> Result<LuaSkillsDebugOutcome, String>;
}

/// Builds the model-visible `luaskills_debug` tool; only `status` runs in Plan mode.
/// 构建模型可见的 `luaskills_debug` 工具；计划模式中只有 `status` 可以运行。
pub(crate) fn build_luaskills_debug_tool() -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        NativeToolDescriptor {
            id: LUASKILLS_DEBUG_TOOL_ID.to_string(),
            name: LUASKILLS_DEBUG_TOOL_ID.to_string(),
            description: String::from(
                "Report or install luaskills-debug, the standalone developer tool that validates, runs, debugs and packages LuaSkills. Before using it, call skill(\"luaskills-dev\") and follow it.\n\
- `status`: the installed version, `compatible` (it matches the LuaSkills series this host embeds), the `executable`, `package_root` and `runtime_root` paths, and `runtime_ready` (its Lua runtime packages are set up).\n\
- `install`: download the newest matching release through the LuaSkills network settings, verify its checksum, and install it; returns the new status. The user approves the download unless the session allows it.\n\
Run the tool itself with `shell_command`, using the returned paths.",
            ),
            help_text: None,
            ai_visible: true,
            available_in_plan: true,
        },
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [LUASKILLS_DEBUG_STATUS_ACTION, "install"],
                    "description": "`status` reads the installed state; `install` downloads and installs the matching release."
                }
            },
            "required": ["action"],
            "additionalProperties": false
        }),
        |request| Box::pin(execute_luaskills_debug_tool(request)),
    )
}

/// Executes one `luaskills_debug` call through the host.
/// 经由宿主执行一次 `luaskills_debug` 调用。
async fn execute_luaskills_debug_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed = LuaSkillsDebugRequest::parse(&request.arguments).map_err(NativeToolError::new)?;
    let host = request
        .luaskills_debug
        .clone()
        .ok_or_else(|| NativeToolError::new("luaskills-debug is not available in this run."))?;
    let session_id = request.session_id.ok_or_else(|| {
        NativeToolError::new(format!("`{LUASKILLS_DEBUG_TOOL_ID}` needs a session."))
    })?;
    let outcome = host
        .run(session_id, parsed)
        .await
        .map_err(NativeToolError::new)?;
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(outcome.text))
            .with_host_result(VulcanToolHostResult {
                kind: LUASKILLS_DEBUG_TOOL_ID.to_string(),
                payload: outcome.payload,
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requests parse strictly: known actions only, no extra fields.
    /// 请求严格解析：只接受已知动作，不接受额外字段。
    #[test]
    fn requests_parse_strictly() {
        assert_eq!(
            LuaSkillsDebugRequest::parse(&json!({"action": "status"})),
            Ok(LuaSkillsDebugRequest::Status)
        );
        assert_eq!(
            LuaSkillsDebugRequest::parse(&json!({"action": "install"})),
            Ok(LuaSkillsDebugRequest::Install)
        );
        assert!(
            LuaSkillsDebugRequest::parse(&json!({"action": "install", "version": "0.8.0"}))
                .is_err()
        );
        assert!(LuaSkillsDebugRequest::parse(&json!({"action": "remove"})).is_err());
        assert!(LuaSkillsDebugRequest::parse(&json!({})).is_err());
    }
}
