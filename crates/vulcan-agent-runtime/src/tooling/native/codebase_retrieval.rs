//! Native projection of the single VCE code-semantic retrieval tool.
//! 唯一 VCE 代码语义检索工具的原生投影。

use super::*;
use vulcan_context_engine::{VceToolCall, VceToolCatalog};

/// Builds the native definition directly from the VCE catalog contract.
/// 直接从 VCE 目录契约构建原生定义。
///
/// # Returns
/// 返回值
/// The single AI-visible VCE native tool definition.
/// 返回唯一对 AI 可见的 VCE 原生工具定义。
pub(crate) fn build_codebase_retrieval_tool() -> Arc<NativeToolDefinition> {
    let catalog = VceToolCatalog::new();
    let definition = catalog
        .definition(CODEBASE_RETRIEVAL_TOOL_ID)
        .expect("VCE catalog must expose the canonical retrieval definition");
    build_native_tool_definition(
        NativeToolDescriptor {
            id: definition.descriptor.id.clone(),
            name: definition.descriptor.name.clone(),
            description: definition.descriptor.description.clone(),
            help_text: definition.descriptor.help_text.clone(),
            ai_visible: definition.descriptor.ai_visible,
            available_in_plan: definition.descriptor.available_in_plan,
        },
        definition.parameters_schema.clone(),
        |request| Box::pin(execute_codebase_retrieval(request)),
    )
}

/// Executes one VCE retrieval against the immutable host project identity.
/// 针对不可变宿主项目身份执行一次 VCE 检索。
///
/// # Parameters
/// 参数
/// `request` contains model arguments plus host-owned project and callback context.
/// `request` 包含模型参数以及宿主持有的项目与回调上下文。
///
/// # Returns
/// 返回值
/// The bounded model-facing VCE payload or an explicit authorization/lifecycle error.
/// 返回有界模型侧 VCE 载荷或显式授权/生命周期错误。
async fn execute_codebase_retrieval(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let call = VceToolCatalog::new()
        .parse_ai_call(CODEBASE_RETRIEVAL_TOOL_ID, request.arguments)
        .map_err(|error| NativeToolError::new(error.to_string()))?;
    let VceToolCall::CodebaseRetrieval(arguments) = call else {
        return Err(NativeToolError::new(
            "The VCE catalog returned a host-only operation for codebase_retrieval.",
        ));
    };
    let project_id = request.host_project_id.ok_or_else(|| {
        NativeToolError::new(
            "codebase_retrieval requires an authoritative project identity from the current execution.",
        )
    })?;
    let handler = request.vce_retrieval_handler.ok_or_else(|| {
        NativeToolError::new("The native VCE retrieval service is unavailable for this execution.")
    })?;
    let payload = handler
        .retrieve(project_id, arguments.information_request)
        .await
        .map_err(NativeToolError::new)?;
    Ok(VulcanToolInvocationResult::from_model_output(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::future::BoxFuture;
    use serde_json::json;

    /// Builds a minimal execution request for VCE native-tool tests.
    /// 为 VCE 原生工具测试构建最小执行请求。
    ///
    /// # Parameters
    /// 参数
    /// `arguments` contains the exact model-provided JSON object.
    /// `arguments` 包含模型提供的精确 JSON 对象。
    ///
    /// # Returns
    /// 返回值
    /// A request with no ambient project or callback authority.
    /// 返回不含隐式项目或回调权限的请求。
    fn request(arguments: Value) -> NativeToolExecutionRequest {
        NativeToolExecutionRequest {
            arguments,
            session_id: None,
            native_browser_runtime: None,
            host_project_id: None,
            workspace_root: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            vce_retrieval_handler: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: Default::default(),
            session_vmm: Default::default(),
        }
    }

    /// Verifies the native definition remains an exact projection of the VCE catalog.
    /// 验证原生定义始终是 VCE 目录的精确投影。
    #[test]
    fn native_definition_matches_vce_catalog() {
        let native = build_codebase_retrieval_tool();
        let catalog = VceToolCatalog::new();
        let vce = catalog
            .definition(CODEBASE_RETRIEVAL_TOOL_ID)
            .expect("canonical VCE tool definition");

        assert_eq!(native.descriptor.id, vce.descriptor.id);
        assert_eq!(native.descriptor.name, vce.descriptor.name);
        assert_eq!(native.descriptor.description, vce.descriptor.description);
        assert_eq!(native.args_schema, vce.parameters_schema);
        assert!(native.descriptor.ai_visible);
    }

    /// Verifies model arguments cannot inject a project identity.
    /// 验证模型参数不能注入项目身份。
    #[tokio::test]
    async fn rejects_model_project_override() {
        let tool = build_codebase_retrieval_tool();
        let error = tool
            .invoke(request(json!({
                "information_request": "find authentication flow",
                "project_id": "other-project"
            })))
            .await
            .expect_err("unexpected project argument must fail");

        assert!(error.message.contains("does not accept argument"));
    }

    /// Verifies the callback receives only the host project and validated information request.
    /// 验证回调只接收宿主项目与经过校验的信息请求。
    #[tokio::test]
    async fn invokes_host_callback_with_authoritative_project() {
        let handler = crate::agent_runtime::VceRetrievalHandler::new(
            |project_id, information_request| -> BoxFuture<'static, Result<Value, String>> {
                Box::pin(async move {
                    Ok(json!({
                        "projectId": project_id,
                        "informationRequest": information_request
                    }))
                })
            },
        );
        let mut request = request(json!({"information_request": "find settings persistence"}));
        request.host_project_id = Some(String::from("project-a"));
        request.vce_retrieval_handler = Some(handler);

        let result = build_codebase_retrieval_tool()
            .invoke(request)
            .await
            .expect("VCE callback should succeed");
        let output = result.model_output;

        assert_eq!(output["projectId"], "project-a");
        assert_eq!(output["informationRequest"], "find settings persistence");
    }

    /// Verifies an execution without a host project fails before any handler can be called.
    /// 验证缺少宿主项目的执行会在调用任何处理器前失败。
    #[tokio::test]
    async fn rejects_execution_without_authoritative_project() {
        let mut request = request(json!({"information_request": "find settings persistence"}));
        request.vce_retrieval_handler = Some(crate::agent_runtime::VceRetrievalHandler::new(
            |_, _| -> BoxFuture<'static, Result<Value, String>> {
                Box::pin(async { panic!("handler must not run without project authority") })
            },
        ));

        let error = build_codebase_retrieval_tool()
            .invoke(request)
            .await
            .expect_err("missing project authority must fail");

        assert!(error.message.contains("authoritative project identity"));
    }

    /// Verifies an unavailable host handler is reported explicitly instead of fabricating results.
    /// 验证宿主处理器不可用时会显式报告，而不是伪造结果。
    #[tokio::test]
    async fn reports_unavailable_retrieval_handler() {
        let mut request = request(json!({"information_request": "find settings persistence"}));
        request.host_project_id = Some(String::from("project-a"));

        let error = build_codebase_retrieval_tool()
            .invoke(request)
            .await
            .expect_err("missing handler must fail");

        assert!(error.message.contains("retrieval service is unavailable"));
    }
}
