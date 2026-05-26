use crate::cache::keys::node_cache_key;
use crate::cache::memory::MemoryPromptCache;
use crate::composer::diagnostics::PromptComposerDiagnostics;
use crate::composer::request::PromptAssemblyInput;
use crate::personality::catalog::PersonalityDefinition;
use crate::source::hashing::stable_text_hash;
use crate::strategy::PlannedNode;
use crate::types::{PromptNodeKind, PromptSegment, PromptTarget};

/// Render one planned non-bundle node.
/// 渲染一个非组合型计划节点。
pub fn render_node(
    planned_node: PlannedNode,
    input: &PromptAssemblyInput,
    personality: Option<&PersonalityDefinition>,
    cache: &MemoryPromptCache,
    diagnostics: &mut PromptComposerDiagnostics,
) -> Option<PromptSegment> {
    let built = match planned_node {
        PlannedNode::ProviderBaseSystem => input.provider_prompt.as_ref().map(|snapshot| {
            build_cached_segment(
                cache,
                diagnostics,
                "provider_base_system",
                snapshot.fingerprint,
                PromptNodeKind::System,
                PromptTarget::System,
                "Provider Base System Prompt",
                &snapshot.text,
            )
        }),
        PlannedNode::AgentBaseSystem => input.agent_prompt.as_ref().map(|snapshot| {
            build_cached_segment(
                cache,
                diagnostics,
                "agent_base_system",
                snapshot.fingerprint,
                PromptNodeKind::System,
                PromptTarget::System,
                "Agent Base System Prompt",
                &snapshot.text,
            )
        }),
        PlannedNode::FunctionalAgentSystem => input
            .subagent
            .as_ref()
            .and_then(|snapshot| snapshot.functional_system_prompt.as_deref())
            .filter(|text| !text.trim().is_empty())
            .map(|text| {
                build_cached_segment(
                    cache,
                    diagnostics,
                    "functional_agent_system",
                    stable_text_hash(text),
                    PromptNodeKind::System,
                    PromptTarget::System,
                    "Functional Agent System Prompt",
                    text,
                )
            }),
        PlannedNode::Personality => personality.map(|definition| {
            let target = if input.model.supports_native_personality {
                PromptTarget::System
            } else {
                PromptTarget::Developer
            };
            let kind = if input.model.supports_native_personality {
                PromptNodeKind::System
            } else {
                PromptNodeKind::Developer
            };
            build_cached_segment(
                cache,
                diagnostics,
                "personality",
                stable_text_hash(&format!(
                    "{}|{}|{}",
                    definition.id, definition.fingerprint, input.model.supports_native_personality
                )),
                kind,
                target,
                format!("Personality {}", definition.label),
                &definition.text,
            )
        }),
        PlannedNode::ProjectRules => input.project_rules.as_ref().map(|snapshot| {
            build_cached_segment(
                cache,
                diagnostics,
                "project_rules",
                snapshot.content_hash,
                PromptNodeKind::User,
                PromptTarget::User,
                format!("Project Rules {}", snapshot.source),
                &snapshot.text,
            )
        }),
        PlannedNode::EnvironmentContext => input.environment_context.as_ref().map(|snapshot| {
            build_cached_segment(
                cache,
                diagnostics,
                "environment_context",
                snapshot.fingerprint,
                PromptNodeKind::User,
                PromptTarget::User,
                "Environment Context",
                &snapshot.text,
            )
        }),
        PlannedNode::TaskGoal => input.task_goal.as_ref().map(|snapshot| {
            build_cached_segment(
                cache,
                diagnostics,
                "task_goal",
                snapshot.fingerprint,
                PromptNodeKind::User,
                PromptTarget::User,
                "Task Goal",
                &snapshot.text,
            )
        }),
        PlannedNode::SubagentIdentity => input.subagent.as_ref().map(|snapshot| {
            let body = format!(
                "Subagent identity: {}\nDelegated task: {}\n{}",
                snapshot.identity,
                snapshot.task,
                snapshot
                    .delegation
                    .as_deref()
                    .map(|text| format!("Delegation guidance: {text}"))
                    .unwrap_or_default()
            )
            .trim()
            .to_string();
            build_cached_segment(
                cache,
                diagnostics,
                "subagent_identity",
                snapshot.fingerprint(),
                PromptNodeKind::Developer,
                PromptTarget::Developer,
                "Subagent Identity",
                body,
            )
        }),
        PlannedNode::ToolHelpBundle => None,
    };
    built.flatten()
}

/// Build one cached prompt segment.
/// 构建一个带缓存的提示词片段。
fn build_cached_segment(
    cache: &MemoryPromptCache,
    diagnostics: &mut PromptComposerDiagnostics,
    namespace: &str,
    fingerprint: u64,
    kind: PromptNodeKind,
    target: PromptTarget,
    title: impl Into<String>,
    body: impl Into<String>,
) -> Option<PromptSegment> {
    let cache_key = node_cache_key(namespace, fingerprint);
    if let Some(segment) = cache.get_node(&cache_key) {
        diagnostics.record_node_cache(&cache_key, true);
        return Some(segment);
    }
    diagnostics.record_node_cache(&cache_key, false);
    let body = body.into();
    if body.trim().is_empty() {
        return None;
    }
    let segment = PromptSegment::new(namespace, cache_key, kind, target, title, body);
    cache.put_node(segment.clone());
    Some(segment)
}
