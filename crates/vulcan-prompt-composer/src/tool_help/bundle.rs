use crate::cache::keys::bundle_cache_key;
use crate::cache::memory::MemoryPromptCache;
use crate::composer::diagnostics::PromptComposerDiagnostics;
use crate::source::hashing::stable_text_hash;
use crate::source::snapshot::ToolCatalogSnapshot;
use crate::tool_help::registry::ToolHelpRegistry;
use crate::types::{PromptNodeKind, PromptSegment, PromptTarget};

/// Build one cached tool-help bundle segment.
/// 构建一个带缓存的工具帮助组合片段。
pub fn build_tool_help_bundle(
    tool_catalog: &ToolCatalogSnapshot,
    registry: &ToolHelpRegistry,
    cache: &MemoryPromptCache,
    diagnostics: &mut PromptComposerDiagnostics,
) -> Option<PromptSegment> {
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
    if let Some(segment) = cache.get_bundle(&cache_key) {
        diagnostics.record_bundle_cache(&cache_key, true);
        return Some(segment);
    }
    diagnostics.record_bundle_cache(&cache_key, false);

    let mut lines = Vec::new();
    for tool in &tool_catalog.tools {
        let help = tool
            .help_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| registry.get(&tool.id).map(|entry| entry.body.clone()));
        if let Some(help) = help {
            lines.push(format!("- `{}`: {}", tool.id, help.trim()));
        }
    }

    if lines.is_empty() {
        return None;
    }

    let body = format!(
        "## Tool Help\nUse the following tool-specific guidance when choosing or invoking tools.\n{}",
        lines.join("\n")
    );
    let segment = PromptSegment::new(
        "tool_help_bundle",
        cache_key,
        PromptNodeKind::ToolMetadata,
        PromptTarget::ToolMetadata,
        "Tool Help Bundle",
        body,
    );
    cache.put_bundle(segment.clone());
    Some(segment)
}
