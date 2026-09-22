//! Source-based tool admission for restored child requests and their internal continuations.
//! 恢复后的子请求及其内部续接使用的基于来源的工具准入。

use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionPromptPurpose};
use std::collections::BTreeMap;
use vulcan_session_core::ExecutionScope;
use vulcan_session_core::v1::child_execution::{ChildToolOrigin, ChildToolSourceKind};
use vulcan_tools_manager::{ResolvedToolSet, VulcanToolDescriptor, VulcanToolSourceKind};

/// Rejects a child Q&A request that lacks its original host-owned tool admission.
/// 拒绝缺少原宿主工具准入记录的子答疑请求。
/// `request` supplies the trusted execution scope; an empty admitted map is valid and denies all tools.
/// `request` 提供可信执行范围；空准入映射有效且禁止所有工具。
pub(super) fn validate_request(request: &ExecutionPreparationRequest) -> Result<(), String> {
    if request.prompt_purpose == ExecutionPromptPurpose::QuestionExecution
        && (!request.prompt.is_empty()
            || request.continuation_action_id.is_some()
            || request.turn_start_mode
                == Some(crate::agent_runtime::execution_models::ExecutionTurnStartMode::UserPrompt))
    {
        return Err(String::from(
            "Question execution must restore canonical history without business user input.",
        ));
    }
    if request.prompt_purpose == ExecutionPromptPurpose::QuestionExecution
        && matches!(request.execution_scope, ExecutionScope::TaskSession { .. })
        && request.captured_child_tool_admission.is_none()
    {
        return Err(String::from(
            "Child question execution requires its original tool admission.",
        ));
    }
    Ok(())
}

/// Intersects current registrations with immutable admitted source identities on both tool surfaces.
/// 在两个工具入口上将当前注册与不可变准入来源身份取交集。
/// `admitted` is absent for ordinary work; present-empty removes every callable definition and descriptor.
/// 普通工作不提供 `admitted`；存在但为空时移除全部可调用定义与描述。
pub(super) fn filter_tools(
    resolved: &mut ResolvedToolSet,
    admitted: Option<&BTreeMap<String, ChildToolOrigin>>,
) {
    let Some(admitted) = admitted else {
        return;
    };
    // Match origin instead of public ID so renamed originals remain usable and same-name replacements do not.
    // 匹配来源而非公开 ID，使改名的原工具仍可使用，同名替换工具无法取得权限。
    let allowed = |descriptor: &VulcanToolDescriptor| {
        admitted
            .values()
            .any(|origin| origin_matches(descriptor, origin))
    };
    resolved.descriptors.retain(&allowed);
    resolved
        .definitions
        .retain(|definition| allowed(&definition.descriptor));
}

/// Compares one current descriptor with its durable source identity before public-name normalization.
/// 将当前描述与公开名称规范化前的持久来源身份比较。
/// Returns true only for the same source family, namespace and original source tool name.
/// 仅在来源类别、命名空间和来源原始工具名均相同时返回 true。
fn origin_matches(descriptor: &VulcanToolDescriptor, admitted: &ChildToolOrigin) -> bool {
    // This exhaustive conversion makes a new host source family require an explicit durable mapping.
    // 此穷尽转换要求新增宿主来源类别时明确添加持久映射。
    let origin = descriptor.origin();
    let kind = match origin.source_kind() {
        VulcanToolSourceKind::Native => ChildToolSourceKind::Native,
        VulcanToolSourceKind::VulcanManaged => ChildToolSourceKind::VulcanManaged,
        VulcanToolSourceKind::VulcanPlugin => ChildToolSourceKind::VulcanPlugin,
        VulcanToolSourceKind::LuaSkills => ChildToolSourceKind::LuaSkills,
        VulcanToolSourceKind::Mcp => ChildToolSourceKind::Mcp,
        VulcanToolSourceKind::AiHostBinding => ChildToolSourceKind::AiHostBinding,
    };
    admitted.source_kind == kind
        && admitted.namespace == origin.namespace()
        && admitted.original_name == origin.original_name()
}
