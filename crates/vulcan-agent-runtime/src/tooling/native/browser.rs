use super::{
    NativeToolDefinition, NativeToolDescriptor, NativeToolError, NativeToolExecutionRequest,
    build_native_tool_definition,
};
use crate::model::SessionId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;
use std::sync::Arc;
use vulcan_tools_manager::VulcanToolInvocationResult;

/// Stable identifier for the AI-visible browser list tool.
/// AI 可见浏览器列表工具的稳定标识。
pub const BROWSER_LIST_TOOL_ID: &str = "browser_list";

/// Stable identifier for the AI-visible browser open tool.
/// AI 可见浏览器打开工具的稳定标识。
pub const BROWSER_OPEN_TOOL_ID: &str = "browser_open";

/// Stable identifier for the AI-visible browser selection tool.
/// AI 可见浏览器选择工具的稳定标识。
pub const BROWSER_SELECT_TOOL_ID: &str = "browser_select";

/// Stable identifier for the AI-visible browser close tool.
/// AI 可见浏览器关闭工具的稳定标识。
pub const BROWSER_CLOSE_TOOL_ID: &str = "browser_close";

/// One Core browser projection returned to the AI without exposing a control credential.
/// 返回给 AI 的单个 Core 浏览器投影，不暴露控制凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeBrowserSnapshot {
    /// Opaque Core browser session identifier accepted by select and close.
    /// 由选择和关闭操作接受的不透明 Core 浏览器会话标识。
    pub browser_id: String,
    /// Current Core browser lifecycle label.
    /// 当前 Core 浏览器生命周期标签。
    pub status: String,
    /// Whether this browser is selected for the owning AI conversation.
    /// 此浏览器是否已被所属 AI 会话选中。
    pub selected: bool,
    /// Current page URL, if Core has observed one.
    /// Core 已观察到的当前页面地址（如有）。
    pub current_url: Option<String>,
    /// Current page title, if Core has observed one.
    /// Core 已观察到的当前页面标题（如有）。
    pub current_title: Option<String>,
}

/// Host-owned browser scheduler used by direct AI tools in one trusted conversation.
/// 单个可信会话中供 AI 直接工具使用的宿主浏览器调度接口。
#[async_trait::async_trait]
pub trait NativeBrowserRuntime: fmt::Debug + Send + Sync {
    /// Lists only browsers owned by `owner_session_id` and returns their selected state.
    /// 仅列出 `owner_session_id` 所拥有的浏览器并返回其选中状态。
    async fn list(&self, owner_session_id: SessionId)
    -> Result<Vec<NativeBrowserSnapshot>, String>;

    /// Opens one Core browser for `owner_session_id`, optionally retaining its profile.
    /// 为 `owner_session_id` 打开一个 Core 浏览器，并可选择保留其用户配置。
    async fn open(
        &self,
        owner_session_id: SessionId,
        persistent_profile: bool,
    ) -> Result<NativeBrowserSnapshot, String>;

    /// Selects one browser already owned by `owner_session_id` for subsequent page tools.
    /// 为后续页面工具选择一个已归属 `owner_session_id` 的浏览器。
    async fn select(
        &self,
        owner_session_id: SessionId,
        browser_id: &str,
    ) -> Result<NativeBrowserSnapshot, String>;

    /// Closes one browser owned by `owner_session_id` and revokes its automation access.
    /// 关闭 `owner_session_id` 所拥有的浏览器并撤销其自动化访问权限。
    async fn close(
        &self,
        owner_session_id: SessionId,
        browser_id: &str,
    ) -> Result<NativeBrowserSnapshot, String>;
}

/// Parsed arguments accepted by the browser-open tool.
/// 浏览器打开工具接受的已解析参数。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserOpenArgs {
    /// Whether the browser should retain a persistent profile.
    /// 浏览器是否保留持久化用户配置。
    #[serde(default)]
    #[serde(rename = "persistentProfile")]
    persistent_profile: bool,
}

/// Parsed arguments accepted by browser selection and close tools.
/// 浏览器选择和关闭工具接受的已解析参数。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserIdArgs {
    /// Opaque browser identifier returned by browser-list or browser-open.
    /// 由 browser-list 或 browser-open 返回的不透明浏览器标识。
    #[serde(rename = "browserId")]
    browser_id: String,
}

/// Builds the AI-visible browser list tool.
/// 构建 AI 可见的浏览器列表工具。
pub(crate) fn build_browser_list_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(BROWSER_LIST_TOOL_ID),
        name: String::from(BROWSER_LIST_TOOL_ID),
        description: String::from(
            "List the Core browsers owned by this conversation and identify the selected browser.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };
    build_native_tool_definition(
        descriptor,
        empty_object_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move {
                ensure_browser_list_arguments(&request.arguments)?;
                let owner = browser_owner_from_request(&request)?;
                let runtime = browser_runtime_from_request(&request)?;
                let browsers = runtime.list(owner).await.map_err(|error| {
                    NativeToolError::new(format!("failed to list browsers: {error}"))
                })?;
                Ok(VulcanToolInvocationResult::from_model_output(
                    Value::String(browser_result_markdown(&browsers)),
                ))
            })
        },
    )
}

/// Builds the AI-visible browser open tool.
/// 构建 AI 可见的浏览器打开工具。
pub(crate) fn build_browser_open_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(BROWSER_OPEN_TOOL_ID),
        name: String::from(BROWSER_OPEN_TOOL_ID),
        description: String::from(
            "Open a Core browser owned by this conversation. The browser becomes the selected browser for subsequent page tools.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };
    build_native_tool_definition(
        descriptor,
        json!({
            "type": "object",
            "properties": {
                "persistentProfile": {
                    "type": "boolean",
                    "description": "Whether to retain the browser profile across sessions. Defaults to false."
                }
            },
            "additionalProperties": false
        }),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move {
                let owner = browser_owner_from_request(&request)?;
                let runtime = browser_runtime_from_request(&request)?;
                let args: BrowserOpenArgs = parse_browser_arguments(request.arguments)?;
                let browser =
                    runtime
                        .open(owner, args.persistent_profile)
                        .await
                        .map_err(|error| {
                            NativeToolError::new(format!("failed to open browser: {error}"))
                        })?;
                Ok(VulcanToolInvocationResult::from_model_output(
                    Value::String(browser_result_markdown(&[browser])),
                ))
            })
        },
    )
}

/// Builds the AI-visible browser selection tool.
/// 构建 AI 可见的浏览器选择工具。
pub(crate) fn build_browser_select_tool() -> Arc<NativeToolDefinition> {
    build_browser_id_tool(
        BROWSER_SELECT_TOOL_ID,
        "Select a Core browser owned by this conversation for subsequent page tools.",
        |runtime, owner, browser_id| async move { runtime.select(owner, &browser_id).await },
    )
}

/// Builds the AI-visible browser close tool.
/// 构建 AI 可见的浏览器关闭工具。
pub(crate) fn build_browser_close_tool() -> Arc<NativeToolDefinition> {
    build_browser_id_tool(
        BROWSER_CLOSE_TOOL_ID,
        "Close a Core browser owned by this conversation and revoke its automation access.",
        |runtime, owner, browser_id| async move { runtime.close(owner, &browser_id).await },
    )
}

/// Builds one browser tool whose only model argument is an opaque browser identifier.
/// 构建一个仅接受不透明浏览器标识作为模型参数的浏览器工具。
fn build_browser_id_tool<F, Fut>(
    tool_id: &'static str,
    description: &'static str,
    operation: F,
) -> Arc<NativeToolDefinition>
where
    F: Fn(Arc<dyn NativeBrowserRuntime>, SessionId, String) -> Fut + Send + Sync + Copy + 'static,
    Fut: std::future::Future<Output = Result<NativeBrowserSnapshot, String>> + Send + 'static,
{
    let descriptor = NativeToolDescriptor {
        id: String::from(tool_id),
        name: String::from(tool_id),
        description: String::from(description),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };
    build_native_tool_definition(
        descriptor,
        json!({
            "type": "object",
            "properties": {
                "browserId": {
                    "type": "string",
                    "description": "Opaque browser identifier returned by browser-list or browser-open.",
                    "minLength": 1
                }
            },
            "required": ["browserId"],
            "additionalProperties": false
        }),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move {
                let owner = browser_owner_from_request(&request)?;
                let runtime = browser_runtime_from_request(&request)?;
                let args: BrowserIdArgs = parse_browser_arguments(request.arguments)?;
                let browser_id = non_empty_browser_id(args.browser_id)?;
                let browser = operation(runtime, owner, browser_id)
                    .await
                    .map_err(|error| {
                        NativeToolError::new(format!("browser operation failed: {error}"))
                    })?;
                Ok(VulcanToolInvocationResult::from_model_output(
                    Value::String(browser_result_markdown(&[browser])),
                ))
            })
        },
    )
}

/// Renders observed browser state with stable IDs usable by subsequent tools.
/// 渲染已观察浏览器状态，保留后续工具可用的稳定标识。
/// Takes authorized snapshots; returns one Markdown body without a duplicate host payload.
/// 接收已授权快照，返回一份 Markdown 正文，不复制宿主载荷。
fn browser_result_markdown(browsers: &[NativeBrowserSnapshot]) -> String {
    use vulcan_tools_manager::result_markdown::inline_code;
    // Optional URL/title values reflect observation, not inferred browser readiness.
    // 可选地址和标题反映观察结果，不据此推断浏览器是否就绪。
    let mut output = String::from("# Browsers\n");
    if browsers.is_empty() {
        output.push_str("\nNo browsers owned by this conversation.\n");
    }
    for browser in browsers {
        output.push_str(&format!(
            "\n## {}\n\nStatus: {}; selected: {}\n",
            inline_code(&browser.browser_id),
            inline_code(&browser.status),
            browser.selected
        ));
        if let Some(title) = &browser.current_title {
            output.push_str(&format!("\nTitle: {}\n", inline_code(title)));
        }
        if let Some(url) = &browser.current_url {
            output.push_str(&format!("\nURL: {}\n", inline_code(url)));
        }
    }
    output
}

/// Parses one browser argument object and turns schema violations into bounded tool errors.
/// 解析一个浏览器参数对象，并将结构错误转换为有界工具错误。
fn parse_browser_arguments<T: for<'de> Deserialize<'de>>(
    arguments: Value,
) -> Result<T, NativeToolError> {
    serde_json::from_value(arguments).map_err(|error| {
        NativeToolError::new(format!("failed to parse browser arguments: {error}"))
    })
}

/// Rejects non-empty arguments for the browser-list tool.
/// 拒绝 browser-list 工具的非空参数。
fn ensure_browser_list_arguments(arguments: &Value) -> Result<(), NativeToolError> {
    match arguments {
        Value::Object(map) if map.is_empty() => Ok(()),
        Value::Null => Ok(()),
        _ => Err(NativeToolError::new(
            "browser_list does not accept any arguments",
        )),
    }
}

/// Returns the authenticated owner session or rejects an invocation without trusted context.
/// 返回经过认证的所属会话；缺少可信上下文时拒绝调用。
fn browser_owner_from_request(
    request: &NativeToolExecutionRequest,
) -> Result<SessionId, NativeToolError> {
    let owner = request.session_id.ok_or_else(|| {
        NativeToolError::new("browser tools require a trusted conversation session id")
    })?;
    if owner.is_nil() {
        return Err(NativeToolError::new(
            "browser tools require a non-nil trusted conversation session id",
        ));
    }
    Ok(owner)
}

/// Returns the request-scoped browser runtime or rejects an unavailable browser capability.
/// 返回请求范围内的浏览器运行时；浏览器能力不可用时拒绝调用。
fn browser_runtime_from_request(
    request: &NativeToolExecutionRequest,
) -> Result<Arc<dyn NativeBrowserRuntime>, NativeToolError> {
    request
        .native_browser_runtime
        .clone()
        .ok_or_else(|| NativeToolError::new("the Core browser runtime is currently unavailable"))
}

/// Rejects blank browser identifiers before they reach the host scheduler.
/// 在标识进入宿主调度器前拒绝空白浏览器标识。
fn non_empty_browser_id(browser_id: String) -> Result<String, NativeToolError> {
    if browser_id.trim().is_empty() {
        return Err(NativeToolError::new("browserId must not be empty"));
    }
    Ok(browser_id)
}

/// Returns the strict empty-object schema shared by browser-list.
/// 返回 browser-list 使用的严格空对象 Schema。
fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct FakeBrowserRuntime {
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl NativeBrowserRuntime for FakeBrowserRuntime {
        async fn list(
            &self,
            owner_session_id: SessionId,
        ) -> Result<Vec<NativeBrowserSnapshot>, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("list:{owner_session_id}"));
            Ok(vec![snapshot("browser-a", true)])
        }

        async fn open(
            &self,
            owner_session_id: SessionId,
            persistent_profile: bool,
        ) -> Result<NativeBrowserSnapshot, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("open:{owner_session_id}:{persistent_profile}"));
            Ok(snapshot("browser-opened", true))
        }

        async fn select(
            &self,
            owner_session_id: SessionId,
            browser_id: &str,
        ) -> Result<NativeBrowserSnapshot, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("select:{owner_session_id}:{browser_id}"));
            Ok(snapshot(browser_id, true))
        }

        async fn close(
            &self,
            owner_session_id: SessionId,
            browser_id: &str,
        ) -> Result<NativeBrowserSnapshot, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("close:{owner_session_id}:{browser_id}"));
            Ok(snapshot(browser_id, false))
        }
    }

    fn snapshot(browser_id: &str, selected: bool) -> NativeBrowserSnapshot {
        NativeBrowserSnapshot {
            browser_id: browser_id.to_string(),
            status: String::from("ready"),
            selected,
            current_url: None,
            current_title: None,
        }
    }

    fn request(
        session_id: Option<SessionId>,
        arguments: Value,
        native_browser_runtime: Option<Arc<dyn NativeBrowserRuntime>>,
    ) -> NativeToolExecutionRequest {
        NativeToolExecutionRequest {
            invocation_context: None,
            arguments,
            session_id,
            native_browser_runtime,
            host_project_id: None,
            workspace_root: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: Default::default(),
            session_vmm: Default::default(),
        }
    }

    #[tokio::test]
    async fn browser_tools_use_trusted_session_and_model_arguments_only() {
        let runtime = Arc::new(FakeBrowserRuntime::default());
        let owner = uuid::Uuid::from_u128(7);
        let list = build_browser_list_tool();
        let open = build_browser_open_tool();
        let select = build_browser_select_tool();
        let close = build_browser_close_tool();

        let result = list
            .invoke(request(Some(owner), json!({}), Some(runtime.clone())))
            .await
            .unwrap();
        assert!(
            result
                .model_output
                .as_str()
                .expect("Markdown")
                .contains("browser-a")
        );
        assert!(result.host_result.is_none());
        open.invoke(request(
            Some(owner),
            json!({"persistentProfile": true}),
            Some(runtime.clone()),
        ))
        .await
        .unwrap();
        select
            .invoke(request(
                Some(owner),
                json!({"browserId": "browser-a"}),
                Some(runtime.clone()),
            ))
            .await
            .unwrap();
        close
            .invoke(request(
                Some(owner),
                json!({"browserId": "browser-a"}),
                Some(runtime.clone()),
            ))
            .await
            .unwrap();
        assert_eq!(runtime.calls.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn browser_tools_reject_missing_owner_and_unknown_arguments() {
        let runtime = Arc::new(FakeBrowserRuntime::default());
        let list = build_browser_list_tool();
        let error = list
            .invoke(request(None, json!({}), Some(runtime.clone())))
            .await
            .unwrap_err();
        assert!(error.message.contains("trusted conversation session id"));
        let select = build_browser_select_tool();
        let error = select
            .invoke(request(
                Some(uuid::Uuid::from_u128(7)),
                json!({"browserId": "x", "extra": true}),
                Some(runtime),
            ))
            .await
            .unwrap_err();
        assert!(error.message.contains("failed to parse browser arguments"));
    }
}
