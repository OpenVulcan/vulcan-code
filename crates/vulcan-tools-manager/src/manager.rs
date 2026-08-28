use super::*;

/// Host-owned tool manager that normalizes candidates, resolves mode-specific surfaces, and dispatches invocations.
/// 宿主自有工具管理器，负责归一化候选工具、解析模式化工具面并调度调用。
#[derive(Clone, Default)]
pub struct VulcanToolsManager {
    /// Candidate tools aggregated from host-owned native, LuaSkills, MCP, or future expert sources.
    /// 从宿主自有原生、LuaSkills、MCP 或未来专家来源聚合得到的候选工具集合。
    candidates: Vec<VulcanToolCandidate>,
}

impl VulcanToolsManager {
    /// Builds one unified tool manager from the current host-owned candidate surfaces.
    /// 根据当前宿主自有候选工具面构建统一工具管理器。
    pub fn new(candidates: Vec<VulcanToolCandidate>) -> Self {
        Self {
            candidates: normalize_candidate_public_tool_names(candidates),
        }
    }

    /// Resolves the final AI-visible tool set for one execution-mode label.
    /// 为单条执行模式标签解析最终 AI 可见工具集合。
    pub fn resolve_for_execution_mode(&self, _execution_mode_label: &str) -> ResolvedToolSet {
        let mut descriptors = Vec::new();
        let mut definitions = Vec::new();

        for candidate in self
            .candidates
            .iter()
            .filter(|candidate| candidate.ai_visible)
        {
            descriptors.push(candidate.descriptor.clone());
            definitions.push(ResolvedToolDefinition {
                descriptor: candidate.descriptor.clone(),
                parameters_schema: candidate.parameters_schema.clone(),
                plan_policy: candidate.plan_policy.clone(),
                request_streaming: candidate.request_streaming.clone(),
                invoke: candidate.invoke.clone(),
            });
        }

        ResolvedToolSet {
            descriptors,
            definitions,
        }
    }

    /// Dispatches one resolved tool call through its host-owned invocation closure.
    /// 通过宿主自有调用闭包调度单个已解析工具调用。
    pub async fn invoke_tool(
        &self,
        tool: &ResolvedToolDefinition,
        args: Value,
        context: VulcanToolInvocationContext,
        execution_mode_label: &str,
    ) -> Result<VulcanToolInvocationResult, String> {
        let mode = VulcanToolMode::from_execution_mode_label(execution_mode_label);
        if matches!(mode, VulcanToolMode::Plan) && !tool_is_allowed_in_plan_with_args(tool, &args) {
            return Ok(VulcanToolInvocationResult::from_model_output(
                Value::String(String::from(
                    "Plan Mode is active. This tool call is not allowed because it may mutate workspace, repository, Git, or external state. Continue by refining the plan and wait for the user to leave Plan Mode before implementing changes.",
                )),
            ));
        }
        (tool.invoke)(args, context).await
    }
}
/// Returns whether one resolved tool may execute while Plan mode is active.
/// 返回单个已解析工具在计划模式激活时是否允许执行。
#[cfg(test)]
pub(super) fn tool_is_allowed_in_plan(tool: &ResolvedToolDefinition) -> bool {
    tool_is_allowed_in_plan_with_args(tool, &Value::Null)
}

/// Returns whether one resolved tool and its exact arguments may execute in Plan mode.
/// 返回单个已解析工具及其精确参数是否允许在计划模式中执行。
pub(super) fn tool_is_allowed_in_plan_with_args(
    tool: &ResolvedToolDefinition,
    args: &Value,
) -> bool {
    match &tool.plan_policy {
        VulcanPlanToolPolicy::Hidden => false,
        VulcanPlanToolPolicy::Visible => true,
        VulcanPlanToolPolicy::HeuristicText(text) => {
            let text = text.to_ascii_lowercase();
            DEFAULT_PLAN_READ_ONLY_KEYWORDS
                .iter()
                .any(|keyword| text.contains(keyword))
        }
        VulcanPlanToolPolicy::StringArgumentAllowlist {
            field,
            allowed_values,
        } => args
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| allowed_values.iter().any(|allowed| allowed == value)),
    }
}

/// Shared read-only heuristic keywords used by the Plan-mode execution guard.
/// Plan 模式执行守卫使用的共享只读启发式关键字集合。
const DEFAULT_PLAN_READ_ONLY_KEYWORDS: &[&str] = &[
    "read",
    "list",
    "rg",
    "grep",
    "search",
    "find",
    "tree",
    "ast",
    "detail",
    "menu",
    "symbol",
    "hover",
    "reference",
    "fetch",
    "get",
    "query",
    "check",
    "lint",
    "typecheck",
    "test",
    "describe",
];
