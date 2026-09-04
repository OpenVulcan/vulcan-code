//! Conditional VCE and CodeKit tool-routing prompt contribution.
//! VCE 与 CodeKit 工具路由提示词的条件化贡献逻辑。

use vulcan_tools_manager::{VulcanToolDescriptor, VulcanToolSourceKind};

use crate::tooling::native::CODEBASE_RETRIEVAL_TOOL_ID;

/// Host-visible CodeKit discovery tools referenced by the cross-family guidance.
/// 跨工具家族说明中引用的宿主可见 CodeKit 发现工具。
const CODEKIT_DISCOVERY_TOOL_IDS: [&str; 3] = [
    "vulcan_codekit_rg",
    "vulcan_codekit_ast_tree",
    "vulcan_codekit_ast_detail",
];

/// VCE-only routing guidance used when CodeKit is not completely visible.
/// CodeKit 未完整可见时使用的纯 VCE 路由说明。
const VCE_ROUTING_PROMPT: &str = r#"## Codebase Semantic Retrieval

Use `codebase_retrieval` when the request is expressed by intent, behavior, architecture, responsibility, or an unfamiliar concept rather than an exact text token. Describe the information needed in one focused natural-language request and use the returned evidence to choose the next source inspection.

- Prefer it for conceptual discovery, likely owners, related symbols, implementation context, and cross-file semantic evidence.
- Treat returned paths, spans, and source excerpts as evidence, not as a substitute for inspecting exact source before editing.
- Refine the information request when the first result reveals the project's vocabulary.
- Do not submit project identity, storage parameters, embedding settings, or retrieval tuning; those remain host-owned."#;

/// Cross-family routing guidance used when VCE and the complete CodeKit discovery set are visible.
/// VCE 与完整 CodeKit 发现工具集同时可见时使用的跨工具家族路由说明。
const VCE_CODEKIT_ROUTING_PROMPT: &str = r#"## Codebase Tool Selection

Choose the tool by the kind of evidence required:

- Intent, behavior, architecture, responsibility, unfamiliar concepts, or semantically related code -> `codebase_retrieval`.
- Exact text, regex, filename, log text, symbol spelling, or a clue that must be mapped to its owning function or class -> `vulcan_codekit_rg`.
- Directory-level symbol inventory and structural overview -> `vulcan_codekit_ast_tree`.
- Detailed structure of already selected exact files -> `vulcan_codekit_ast_detail`.

Start with one focused discovery call. Use VCE to learn the project's vocabulary, then use CodeKit or direct source inspection for exact structure and edit locations. Treat returned paths, spans, and excerpts as evidence. Do not send project identity, storage parameters, embedding settings, or retrieval tuning to `codebase_retrieval`; those remain host-owned."#;

/// Builds request-scoped codebase tool-selection guidance from the final visible tool set.
/// 根据最终可见工具集合构建请求级代码库工具选择说明。
///
/// # Parameters
/// 参数
/// - `visible_tools`: Tool descriptors remaining after all host and model filters.
/// - `visible_tools`：经过全部宿主与模型过滤后保留的工具描述。
///
/// # Returns
/// 返回值
/// VCE-aware routing guidance, or `None` when the canonical native retrieval tool is unavailable.
/// 包含 VCE 的路由说明；规范原生检索工具不可用时返回 `None`。
pub(super) fn codebase_tool_routing_prompt(
    visible_tools: &[VulcanToolDescriptor],
) -> Option<String> {
    let vce_visible = visible_tools.iter().any(|descriptor| {
        descriptor.id == CODEBASE_RETRIEVAL_TOOL_ID
            && descriptor.source == VulcanToolSourceKind::Native
    });
    if !vce_visible {
        return None;
    }

    let complete_codekit_visible = CODEKIT_DISCOVERY_TOOL_IDS.iter().all(|tool_id| {
        visible_tools.iter().any(|descriptor| {
            descriptor.id == *tool_id && descriptor.source == VulcanToolSourceKind::LuaSkills
        })
    });
    let routing_prompt = if complete_codekit_visible {
        VCE_CODEKIT_ROUTING_PROMPT
    } else {
        VCE_ROUTING_PROMPT
    };
    Some(routing_prompt.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one minimal visible tool descriptor for routing-prompt tests.
    /// 为路由提示词测试构建一条最小可见工具描述。
    ///
    /// # Parameters
    /// 参数
    /// - `id`: Exact host-visible tool identifier.
    /// - `id`：宿主侧精确可见工具标识。
    /// - `source`: Authoritative tool source family.
    /// - `source`：权威工具来源家族。
    ///
    /// # Returns
    /// 返回值
    /// A descriptor carrying the requested identifier and source.
    /// 一条携带指定标识与来源的工具描述。
    fn test_tool_descriptor(id: &str, source: VulcanToolSourceKind) -> VulcanToolDescriptor {
        VulcanToolDescriptor {
            id: id.to_string(),
            name: id.to_string(),
            description: String::from("test tool"),
            help_text: None,
            source,
        }
    }

    /// Builds the canonical native VCE descriptor.
    /// 构建规范原生 VCE 工具描述。
    ///
    /// # Returns
    /// 返回值
    /// The authoritative descriptor used by routing tests.
    /// 路由测试使用的权威工具描述。
    fn vce_tool_descriptor() -> VulcanToolDescriptor {
        test_tool_descriptor(CODEBASE_RETRIEVAL_TOOL_ID, VulcanToolSourceKind::Native)
    }

    /// Verifies cross-family guidance appears when VCE and CodeKit are completely visible.
    /// 验证 VCE 与 CodeKit 完整可见时生成跨工具家族说明。
    #[test]
    fn appends_cross_family_prompt_for_complete_vce_and_codekit_tools() {
        let mut visible_tools = vec![vce_tool_descriptor()];
        visible_tools.extend(
            CODEKIT_DISCOVERY_TOOL_IDS
                .iter()
                .map(|tool_id| test_tool_descriptor(tool_id, VulcanToolSourceKind::LuaSkills)),
        );

        let prompt = codebase_tool_routing_prompt(&visible_tools)
            .expect("routing prompt should be produced");
        assert!(prompt.contains("## Codebase Tool Selection"));
        assert!(prompt.contains("`codebase_retrieval`"));
        assert!(prompt.contains("`vulcan_codekit_rg`"));
    }

    /// Verifies VCE-only guidance never references unavailable CodeKit tools.
    /// 验证纯 VCE 说明不会引用不可用的 CodeKit 工具。
    #[test]
    fn appends_vce_only_prompt_when_codekit_tools_are_absent() {
        let prompt = codebase_tool_routing_prompt(&[vce_tool_descriptor()])
            .expect("VCE-only routing prompt should be produced");
        assert!(prompt.contains("## Codebase Semantic Retrieval"));
        assert!(!prompt.contains("vulcan_codekit"));
    }

    /// Verifies a partial CodeKit contract falls back to VCE-only guidance.
    /// 验证不完整的 CodeKit 契约回退到纯 VCE 说明。
    #[test]
    fn uses_vce_only_prompt_when_codekit_contract_is_incomplete() {
        let visible_tools = vec![
            vce_tool_descriptor(),
            test_tool_descriptor("vulcan_codekit_rg", VulcanToolSourceKind::LuaSkills),
        ];
        let prompt = codebase_tool_routing_prompt(&visible_tools)
            .expect("VCE-only routing prompt should be produced");
        assert!(prompt.contains("## Codebase Semantic Retrieval"));
        assert!(!prompt.contains("vulcan_codekit"));
    }

    /// Verifies another source family cannot impersonate the native VCE tool.
    /// 验证其他来源家族不能冒充原生 VCE 工具。
    #[test]
    fn rejects_non_native_vce_identifier_collision() {
        let visible_tools = vec![test_tool_descriptor(
            CODEBASE_RETRIEVAL_TOOL_ID,
            VulcanToolSourceKind::Mcp,
        )];
        assert!(codebase_tool_routing_prompt(&visible_tools).is_none());
    }

    /// Verifies an empty final tool set never produces routing guidance.
    /// 验证最终工具集合为空时不会生成路由说明。
    #[test]
    fn skips_prompt_for_empty_final_tool_set() {
        assert!(codebase_tool_routing_prompt(&[]).is_none());
    }
}
