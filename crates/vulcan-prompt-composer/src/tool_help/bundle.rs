use crate::cache::keys::bundle_cache_key;
use crate::cache::memory::MemoryPromptCache;
use crate::composer::diagnostics::PromptComposerDiagnostics;
use crate::source::hashing::stable_text_hash;
use crate::source::snapshot::ToolCatalogSnapshot;
use crate::tool_help::registry::ToolHelpRegistry;
use crate::types::{PromptNodeKind, PromptSegment, PromptTarget};

/// Build cached per-tool help segments for the current visible tool catalog.
/// 为当前可见工具目录构建带缓存的逐工具帮助片段。
pub fn build_tool_help_bundle(
    tool_catalog: &ToolCatalogSnapshot,
    registry: &ToolHelpRegistry,
    cache: &MemoryPromptCache,
    diagnostics: &mut PromptComposerDiagnostics,
) -> Vec<PromptSegment> {
    let bundle_fingerprints = tool_catalog
        .tools
        .iter()
        .map(|tool| {
            let inline_help = tool.help_text.as_deref().map(stable_text_hash).unwrap_or(0);
            let registry_help = registry
                .get(&tool.id)
                .map(|entry| entry.fingerprint)
                .unwrap_or(0);
            stable_text_hash(&format!(
                "{}|{}|{}|{}",
                tool.id,
                tool.schema_fingerprint.as_deref().unwrap_or_default(),
                inline_help,
                registry_help
            ))
        })
        .collect::<Vec<_>>();
    let cache_key = bundle_cache_key("tool_help", &bundle_fingerprints);
    if let Some(segments) = cache.get_bundle(&cache_key) {
        diagnostics.record_bundle_cache(&cache_key, true);
        return segments;
    }
    diagnostics.record_bundle_cache(&cache_key, false);

    let mut segments = Vec::new();
    for tool in &tool_catalog.tools {
        let inline_help = tool
            .help_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let help = if let Some(help) = inline_help {
            Some((help.to_string(), format!("Inline tool help `{}`", tool.id)))
        } else {
            registry.get(&tool.id).map(|entry| {
                (
                    entry.body.clone(),
                    format!("{} for `{}`", entry.source_label, tool.id),
                )
            })
        };
        if let Some((help, source_label)) = help {
            let trimmed = help.trim();
            if trimmed.is_empty() {
                continue;
            }
            let body = format!(
                "## Tool Help: `{}`\nUse this guidance when choosing or invoking `{}`.\n{}",
                tool.id, tool.id, trimmed
            );
            let node_id = format!("tool_help_{}", sanitize_tool_help_node_id(&tool.id));
            segments.push(
                PromptSegment::new(
                    node_id,
                    cache_key.clone(),
                    PromptNodeKind::ToolMetadata,
                    PromptTarget::ToolMetadata,
                    format!("Tool Help {}", tool.id),
                    body,
                )
                .with_source_label(source_label),
            );
        }
    }

    if !segments.is_empty() {
        cache.put_bundle(cache_key, segments.clone());
    }
    segments
}

/// Sanitize one tool id for use in a readable prompt node identifier.
/// 清理工具标识，使其可安全用于可读提示词节点标识。
fn sanitize_tool_help_node_id(tool_id: &str) -> String {
    tool_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}
