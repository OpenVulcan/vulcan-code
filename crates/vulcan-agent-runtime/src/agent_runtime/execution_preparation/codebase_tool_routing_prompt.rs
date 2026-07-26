//! Conditional Codebase Memory and CodeKit tool-routing prompt contribution.
//! Codebase Memory 与 CodeKit 工具路由提示词的条件化贡献逻辑。

use std::collections::BTreeSet;

use vulcan_tools_manager::{VulcanToolDescriptor, VulcanToolSourceKind};

/// Complete host-visible read-only Codebase Memory tool contract required by the routing prompt.
/// 路由提示词要求完整可见的宿主侧 Codebase Memory 只读工具契约。
const CODEBASE_MEMORY_TOOL_IDS: [&str; 10] = [
    "mcp_vulcan_codebase_memory_get_graph_schema",
    "mcp_vulcan_codebase_memory_search_graph",
    "mcp_vulcan_codebase_memory_query_graph",
    "mcp_vulcan_codebase_memory_index_status",
    "mcp_vulcan_codebase_memory_check_index_coverage",
    "mcp_vulcan_codebase_memory_trace_path",
    "mcp_vulcan_codebase_memory_get_architecture",
    "mcp_vulcan_codebase_memory_get_code_snippet",
    "mcp_vulcan_codebase_memory_search_code",
    "mcp_vulcan_codebase_memory_detect_changes",
];

/// Host-visible CodeKit discovery tools required by the cross-family routing prompt.
/// 跨工具家族路由提示词要求可见的宿主侧 CodeKit 发现工具。
const CODEKIT_DISCOVERY_TOOL_IDS: [&str; 3] = [
    "vulcan_codekit_rg",
    "vulcan_codekit_ast_tree",
    "vulcan_codekit_ast_detail",
];

/// Graph-only routing guidance used when Codebase Memory is visible without CodeKit.
/// Codebase Memory 可见而 CodeKit 不可见时使用的纯图谱路由说明。
const CODEBASE_MEMORY_ROUTING_PROMPT: &str = r#"## Codebase Memory Tool Selection

Use the registered Codebase Memory tools for graph-backed code intelligence:

### Discovery and Source
- Natural-language or conceptual symbol discovery → `mcp_vulcan_codebase_memory_search_graph`
- Literal text enriched into containing indexed symbols and structural ranking → `mcp_vulcan_codebase_memory_search_code`
- Exact indexed symbol source by qualified name → `mcp_vulcan_codebase_memory_get_code_snippet`
- Complex multi-hop, aggregation, or cross-service analysis → `mcp_vulcan_codebase_memory_query_graph`

### Relationships, Impact, and Architecture
- Callers, callees, data flow, or cross-service paths → `mcp_vulcan_codebase_memory_trace_path`
- Git diff blast radius → `mcp_vulcan_codebase_memory_detect_changes`
- Project architecture, dependencies, clusters, hotspots, layers, or broad file tree → `mcp_vulcan_codebase_memory_get_architecture`
- Graph labels and edge types needed for Cypher → `mcp_vulcan_codebase_memory_get_graph_schema`

### Index Ground Truth
- Index readiness and project-wide diagnostics → `mcp_vulcan_codebase_memory_index_status`
- Exact file or bounded-scope coverage verification → `mcp_vulcan_codebase_memory_check_index_coverage`
- Treat exact source as ground truth. Before exhaustive or negative claims, or when graph results are unexpectedly empty, check index coverage and fall back to direct source inspection for uncovered paths.
- Do not reconstruct callers or blast radius with repeated text searches when `mcp_vulcan_codebase_memory_trace_path` or `mcp_vulcan_codebase_memory_detect_changes` is available."#;

/// Cross-family routing guidance used when both Codebase Memory and CodeKit are visible.
/// Codebase Memory 与 CodeKit 同时可见时使用的跨工具家族路由说明。
const CODEBASE_MEMORY_CODEKIT_ROUTING_PROMPT: &str = r#"## Codebase Tool Selection

Select tools by the information required:

### Search and Discovery
- Literal text plus its owning function or class → `vulcan_codekit_rg`
- Natural-language or conceptual symbol discovery → `mcp_vulcan_codebase_memory_search_graph`
- Literal text enriched into containing indexed symbols and structural ranking → `mcp_vulcan_codebase_memory_search_code`
- Complex multi-hop, aggregation, or cross-service analysis → `mcp_vulcan_codebase_memory_query_graph`

### Trace and Impact
- Callers, callees, data flow, or cross-service paths → `mcp_vulcan_codebase_memory_trace_path`
- Git diff blast radius → `mcp_vulcan_codebase_memory_detect_changes`

### Architecture and Structure
- Project architecture, dependencies, clusters, hotspots, layers, or broad file tree → `mcp_vulcan_codebase_memory_get_architecture`
- Directory-level symbol inventory → `vulcan_codekit_ast_tree`
- Detailed structure of exact files → `vulcan_codekit_ast_detail`
- Graph labels and edge types needed for Cypher → `mcp_vulcan_codebase_memory_get_graph_schema`

### Source and Coverage
- Exact indexed symbol source by qualified name → `mcp_vulcan_codebase_memory_get_code_snippet`
- Index readiness and project-wide diagnostics → `mcp_vulcan_codebase_memory_index_status`
- Exact file or bounded-scope coverage verification → `mcp_vulcan_codebase_memory_check_index_coverage`
- Treat exact source as ground truth. Before exhaustive or negative claims, or when graph results are unexpectedly empty, check index coverage and use CodeKit or direct source inspection for uncovered paths.

### Anti-patterns
- Do not reconstruct callers or blast radius with repeated text searches when `mcp_vulcan_codebase_memory_trace_path` or `mcp_vulcan_codebase_memory_detect_changes` is available.
- Do not use sequential text searches to explore related concepts when `mcp_vulcan_codebase_memory_search_graph` is available.
- Do not use AST directory trees as a substitute for project architecture analysis."#;

/// Appends request-scoped codebase tool-selection guidance only for a complete visible CBM contract.
/// 仅在完整 CBM 契约真实可见时追加请求级 Codebase 工具选择说明。
///
/// # Parameters
/// 参数
/// - `developer_segment`: Mutable Developer Prompt segment assembled for the current request.
/// - `developer_segment`：为当前请求装配的可变 Developer Prompt 片段。
/// - `visible_tools`: Final provider-visible tool descriptors after every host and model filter.
/// - `visible_tools`：经过全部宿主与模型过滤后的最终 Provider 可见工具描述。
pub(super) fn append_codebase_tool_routing_prompt(
    developer_segment: &mut Option<String>,
    visible_tools: &[VulcanToolDescriptor],
) {
    // visibleMcpIds is the exact post-filter MCP public tool-name set sent to the provider.
    // visibleMcpIds 是发送给 Provider 的精确过滤后 MCP 公开工具名集合。
    let visible_mcp_ids = visible_tools
        .iter()
        .filter(|descriptor| descriptor.source == VulcanToolSourceKind::Mcp)
        .map(|descriptor| descriptor.id.as_str())
        .collect::<BTreeSet<_>>();
    if !CODEBASE_MEMORY_TOOL_IDS
        .iter()
        .all(|tool_id| visible_mcp_ids.contains(tool_id))
    {
        return;
    }

    // visibleLuaSkillsIds is the exact post-filter LuaSkills public tool-name set sent to the provider.
    // visibleLuaSkillsIds 是发送给 Provider 的精确过滤后 LuaSkills 公开工具名集合。
    let visible_luaskills_ids = visible_tools
        .iter()
        .filter(|descriptor| descriptor.source == VulcanToolSourceKind::LuaSkills)
        .map(|descriptor| descriptor.id.as_str())
        .collect::<BTreeSet<_>>();
    // routingPrompt includes CodeKit comparisons only when every referenced CodeKit tool is visible.
    // routingPrompt 仅在引用的 CodeKit 工具全部可见时包含 CodeKit 对比说明。
    let routing_prompt = if CODEKIT_DISCOVERY_TOOL_IDS
        .iter()
        .all(|tool_id| visible_luaskills_ids.contains(tool_id))
    {
        CODEBASE_MEMORY_CODEKIT_ROUTING_PROMPT
    } else {
        CODEBASE_MEMORY_ROUTING_PROMPT
    };

    match developer_segment {
        Some(segment) if !segment.trim().is_empty() => {
            segment.push_str("\n\n");
            segment.push_str(routing_prompt);
        }
        Some(segment) => *segment = routing_prompt.to_string(),
        None => *developer_segment = Some(routing_prompt.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one minimal visible tool descriptor for routing-prompt tests.
    /// 为路由提示词测试构建一条最小可见工具描述。
    ///
    /// # Parameters
    /// 参数
    /// - `id`: Exact host-visible public tool identifier.
    /// - `id`：宿主侧精确可见的公开工具标识。
    ///
    /// # Returns
    /// 返回值
    /// A descriptor carrying the requested public identifier.
    /// 一条携带指定公开标识的工具描述。
    fn test_tool_descriptor(id: &str) -> VulcanToolDescriptor {
        VulcanToolDescriptor {
            id: id.to_string(),
            name: id.to_string(),
            description: String::from("test tool"),
            help_text: None,
            source: if id.starts_with("mcp_") {
                VulcanToolSourceKind::Mcp
            } else {
                VulcanToolSourceKind::LuaSkills
            },
        }
    }

    /// Builds the complete visible CBM descriptor contract.
    /// 构建完整可见的 CBM 工具描述契约。
    ///
    /// # Returns
    /// 返回值
    /// All host-visible read-only CBM tool descriptors.
    /// 全部宿主侧可见的 CBM 只读工具描述。
    fn codebase_memory_tool_descriptors() -> Vec<VulcanToolDescriptor> {
        CODEBASE_MEMORY_TOOL_IDS
            .iter()
            .map(|tool_id| test_tool_descriptor(tool_id))
            .collect()
    }

    /// Verifies the cross-family prompt is selected when CBM and CodeKit are both complete.
    /// 验证 CBM 与 CodeKit 均完整可见时选择跨工具家族提示词。
    #[test]
    fn appends_cross_family_prompt_for_complete_cbm_and_codekit_tools() {
        // visibleTools models the final provider tool set for an enabled CBM project with CodeKit.
        // visibleTools 模拟启用 CBM 且具备 CodeKit 的项目最终 Provider 工具集合。
        let mut visible_tools = codebase_memory_tool_descriptors();
        visible_tools.extend(
            CODEKIT_DISCOVERY_TOOL_IDS
                .iter()
                .map(|tool_id| test_tool_descriptor(tool_id)),
        );
        // developerSegment captures the conditional runtime contribution.
        // developerSegment 捕获条件化运行时提示贡献。
        let mut developer_segment = None;

        append_codebase_tool_routing_prompt(&mut developer_segment, &visible_tools);

        // prompt is the final cross-family Developer Prompt contribution.
        // prompt 是最终的跨工具家族 Developer Prompt 贡献。
        let prompt = developer_segment.expect("routing prompt should be appended");
        assert!(prompt.contains("## Codebase Tool Selection"));
        assert!(prompt.contains("`vulcan_codekit_rg`"));
        assert!(prompt.contains("`mcp_vulcan_codebase_memory_search_graph`"));
    }

    /// Verifies the graph-only prompt never references absent CodeKit tools.
    /// 验证纯图谱提示词绝不引用缺失的 CodeKit 工具。
    #[test]
    fn appends_cbm_only_prompt_when_codekit_tools_are_absent() {
        // developerSegment captures the CBM-only fallback contribution.
        // developerSegment 捕获仅包含 CBM 的回退提示贡献。
        let mut developer_segment = None;

        append_codebase_tool_routing_prompt(
            &mut developer_segment,
            &codebase_memory_tool_descriptors(),
        );

        // prompt is the final CBM-only Developer Prompt contribution.
        // prompt 是最终仅包含 CBM 的 Developer Prompt 贡献。
        let prompt = developer_segment.expect("CBM-only routing prompt should be appended");
        assert!(prompt.contains("## Codebase Memory Tool Selection"));
        assert!(!prompt.contains("vulcan_codekit"));
    }

    /// Verifies one missing CBM tool suppresses the complete routing contribution.
    /// 验证缺少任意一个 CBM 工具时抑制完整路由提示贡献。
    #[test]
    fn skips_prompt_when_cbm_tool_contract_is_incomplete() {
        // visibleTools intentionally omits detect_changes from the required CBM contract.
        // visibleTools 故意从必需 CBM 契约中省略 detect_changes。
        let mut visible_tools = codebase_memory_tool_descriptors();
        visible_tools.retain(|tool| tool.id != "mcp_vulcan_codebase_memory_detect_changes");
        // developerSegment remains absent when the contract is incomplete.
        // developerSegment 在契约不完整时保持缺失。
        let mut developer_segment = None;

        append_codebase_tool_routing_prompt(&mut developer_segment, &visible_tools);

        assert!(developer_segment.is_none());
    }

    /// Verifies non-MCP tools cannot impersonate the CBM contract by occupying its public identifiers.
    /// 验证非 MCP 工具无法通过占用 CBM 公开标识冒充其工具契约。
    #[test]
    fn skips_prompt_when_cbm_public_ids_are_owned_by_another_source() {
        // visibleTools uses the complete CBM names but intentionally assigns the wrong source family.
        // visibleTools 使用完整 CBM 名称，但故意分配错误的来源家族。
        let visible_tools = CODEBASE_MEMORY_TOOL_IDS
            .iter()
            .map(|tool_id| {
                // descriptor intentionally carries a non-MCP source for collision-hardening coverage.
                // descriptor 故意携带非 MCP 来源，用于覆盖名称冲突加固逻辑。
                let mut descriptor = test_tool_descriptor(tool_id);
                descriptor.source = VulcanToolSourceKind::LuaSkills;
                descriptor
            })
            .collect::<Vec<_>>();
        // developerSegment must remain absent for an impersonated contract.
        // developerSegment 对冒充契约必须保持缺失。
        let mut developer_segment = None;

        append_codebase_tool_routing_prompt(&mut developer_segment, &visible_tools);

        assert!(developer_segment.is_none());
    }

    /// Verifies routing guidance is appended after an existing Developer Prompt segment.
    /// 验证路由说明追加在既有 Developer Prompt 片段之后。
    #[test]
    fn appends_prompt_after_existing_developer_guidance() {
        // developerSegment represents existing Prompt Skills and LuaSkills guidance.
        // developerSegment 表示既有 Prompt Skills 与 LuaSkills 指导。
        let mut developer_segment = Some(String::from("Existing developer guidance."));

        append_codebase_tool_routing_prompt(
            &mut developer_segment,
            &codebase_memory_tool_descriptors(),
        );

        // prompt is the merged existing and CBM Developer Prompt text.
        // prompt 是合并既有说明与 CBM 路由后的 Developer Prompt 文本。
        let prompt = developer_segment.expect("combined developer prompt should exist");
        assert!(prompt.starts_with("Existing developer guidance.\n\n"));
        assert!(prompt.contains("## Codebase Memory Tool Selection"));
    }

    /// Verifies an empty final tool set never produces routing guidance.
    /// 验证最终工具集合为空时绝不产生路由说明。
    #[test]
    fn skips_prompt_for_empty_final_tool_set() {
        // developerSegment starts absent and must remain absent.
        // developerSegment 初始缺失且必须保持缺失。
        let mut developer_segment = None;

        append_codebase_tool_routing_prompt(&mut developer_segment, &[]);

        assert!(developer_segment.is_none());
    }
}
