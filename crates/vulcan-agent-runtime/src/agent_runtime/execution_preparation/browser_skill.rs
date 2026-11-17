use crate::tooling::native::{BROWSER_MCP_ORIGIN, BROWSER_PAGE_TARGET, NativeBrowserRuntime};
use std::sync::Arc;
use uuid::Uuid;
use vulcan_plugin_runtime::VulcanPluginMcpServerDescriptor;
use vulcan_tools_manager::public_name::build_prefixed_public_tool_name;
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolInvocationResult,
    VulcanToolOrigin, VulcanToolSourceKind,
};

use super::tool_exposure::{ToolExposureSelectorState, mcp_surface_selected};

/// Produces the original MCP candidates from the packaged listing without starting a server.
/// 从随包工具列表构建原始 MCP 候选，全程无需启动服务。
pub(super) fn candidates(
    server: &VulcanPluginMcpServerDescriptor,
    runtime: &Arc<dyn NativeBrowserRuntime>,
    selectors: &ToolExposureSelectorState,
) -> Vec<VulcanToolCandidate> {
    // Catalog validation bound all definitions to one explicit, pinned server.
    // 目录校验已将所有定义绑定到一个明确且版本锁定的服务。
    let Some(snapshot) = &server.prompt_tool_snapshot else {
        return Vec::new();
    };
    snapshot.tools.iter().filter_map(|(tool_name, tool)| {
        // Keep the original direct name for selectors and Plan policy.
        // 保留原直接名称，用于选择器和计划模式策略。
        let public_id = build_prefixed_public_tool_name("mcp", &[BROWSER_MCP_ORIGIN, tool_name]);
        // The qualified name matches the old live MCP binding's surface.
        // 限定名称与原实时 MCP 绑定入口一致。
        let qualified = format!("{BROWSER_MCP_ORIGIN}/{tool_name}");
        if !mcp_surface_selected(
            &[&public_id, &public_id, &tool.description, &qualified, BROWSER_MCP_ORIGIN, tool_name],
            Some(BROWSER_MCP_ORIGIN),
            selectors,
        ) {
            return None;
        }
        // Preserve the exact origin for both registration and execution hooks.
        // 保留精确来源，使注册与执行 Hook 均能命中。
        let descriptor = VulcanToolDescriptor::new(
            public_id.clone(), public_id.clone(), tool.description.clone(), None,
            VulcanToolOrigin::new(VulcanToolSourceKind::Mcp, BROWSER_MCP_ORIGIN, tool_name.clone()),
        );
        // The raw schema is checked against the live MCP listing at the dispatch boundary.
        // 在分派边界将原始 Schema 与实时 MCP 列表核对。
        let schema = Arc::new(tool.input_schema.clone());
        // The invocation holds the exact plugin generation and upstream name.
        // 调用持有精确插件代次与上游名称。
        let generation = server.plugin_generation;
        let runtime = Arc::clone(runtime);
        let tool_name = tool_name.clone();
        Some(VulcanToolCandidate {
            descriptor,
            parameters_schema: (*schema).clone(),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::HeuristicText(public_id),
            request_streaming: None,
            invoke: Arc::new(move |args, context| {
                // Capture the scoped selection before entering the concrete runtime.
                // 进入具体运行时前取出当前调用的限定选择。
                let target = BROWSER_PAGE_TARGET.try_with(Clone::clone).ok().flatten();
                let runtime = Arc::clone(&runtime);
                let name = tool_name.clone();
                let schema = Arc::clone(&schema);
                Box::pin(async move {
                    // Only the trusted invocation context supplies the conversation owner.
                    // 会话所有者只取自可信调用上下文。
                    let owner = Uuid::parse_str(&context.session_id).map_err(|error| error.to_string())?;
                    // A call received before selection cannot inherit a later browser.
                    // 未选浏览器时收到的调用不能继承随后选中的浏览器。
                    let target = target.ok_or_else(|| String::from("No browser was selected when this action was received. Open or select a browser and start a new action."))?;
                    runtime.invoke_page_tool(owner, &target, generation, &name, &schema, args)
                        .await
                        .map(VulcanToolInvocationResult::from_model_output)
                })
            }),
        })
    }).collect()
}
