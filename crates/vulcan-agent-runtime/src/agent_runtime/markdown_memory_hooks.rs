//! Markdown memory injection through the locked system-prompt baseline.
//! 通过锁定系统提示词基线注入 Markdown 记忆。
//!
//! The snapshot block is rebuilt for every prompt turn, but it carries `MemoryProfile` baseline
//! provenance, so the provider request reuses the persisted baseline byte for byte and only a new
//! conversation or a context compaction picks up a fresh snapshot. Writes made during a
//! conversation therefore never break the prompt cache.
//! 快照块在每个提示回合都会重建，但带有 `MemoryProfile` 基线来源，因此 Provider 请求逐字节复用
//! 已持久化的基线，只有新会话或上下文压缩才会采用新快照。会话中的写入因此不会破坏提示词缓存。

use std::path::Path;

use vulcan_hooks::{
    SystemPromptHookBaselineKind, SystemPromptHookBaselineProvenance, SystemPromptHookBlock,
    SystemPromptHookPhase,
};
use vulcan_markdown_memory::build_injection_snapshot;

use crate::tooling::native::{MarkdownMemoryAccess, resolve_markdown_memory_access};

/// Stable system-prompt tag of the Markdown memory snapshot.
/// Markdown 记忆快照的稳定系统提示词标签。
pub(crate) const MARKDOWN_MEMORY_SYSTEM_PROMPT_TAG: &str = "long_term_memory";

/// Locked-baseline node that owns the snapshot.
/// 持有快照的锁定基线节点。
const MARKDOWN_MEMORY_BASELINE_NODE_ID: &str = "hook.markdown-memory";

/// Producer recorded as the snapshot's baseline source.
/// 记录为快照基线来源的生产者。
const MARKDOWN_MEMORY_BASELINE_SOURCE_ID: &str = "builtin.markdown-memory.snapshot";

/// Appends the Markdown memory snapshot block when Markdown memory is active for the project.
/// 当项目的 Markdown 记忆生效时追加记忆快照块。
///
/// `compacted` selects the phase label: the baseline is established by the first request of a
/// conversation or the first request after a compaction. Returns an error only when the memory
/// files cannot be read; inactive memory appends nothing.
/// `compacted` 决定阶段标签：基线由会话首个请求或压缩后的首个请求建立。仅在记忆文件无法读取时
/// 返回错误；记忆未生效时不追加任何内容。
pub(crate) fn append_markdown_memory_block(
    blocks: &mut Vec<SystemPromptHookBlock>,
    host_project_id: Option<&str>,
    workspace_path: &str,
    compacted: bool,
) -> Result<(), String> {
    blocks.retain(|block| block.tag != MARKDOWN_MEMORY_SYSTEM_PROMPT_TAG);
    let workspace_root = (!workspace_path.trim().is_empty()).then(|| Path::new(workspace_path));
    let Some(access) = resolve_markdown_memory_access(host_project_id, workspace_root)? else {
        return Ok(());
    };
    let phase = if compacted {
        SystemPromptHookPhase::AfterContextCompaction
    } else {
        SystemPromptHookPhase::FirstSession
    };
    if let Some(block) = markdown_memory_block(&access, phase)? {
        blocks.push(block);
    }
    Ok(())
}

/// Removes the snapshot from requests that must not see long-term memory.
/// 从不得看到长期记忆的请求中移除快照。
pub(crate) fn remove_markdown_memory_block(blocks: &mut Vec<SystemPromptHookBlock>) {
    blocks.retain(|block| block.tag != MARKDOWN_MEMORY_SYSTEM_PROMPT_TAG);
}

/// Builds the snapshot block, or `None` when every scope is empty.
/// 构建快照块；全部层为空时返回 `None`。
pub(crate) fn markdown_memory_block(
    access: &MarkdownMemoryAccess,
    phase: SystemPromptHookPhase,
) -> Result<Option<SystemPromptHookBlock>, String> {
    let documents = access
        .store
        .load_scopes(&access.context)
        .map_err(|error| error.to_string())?;
    let snapshot = build_injection_snapshot(&documents, &access.settings.budget);
    if snapshot.content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(SystemPromptHookBlock {
        tag: MARKDOWN_MEMORY_SYSTEM_PROMPT_TAG.to_string(),
        description: String::from(
            "Long-term memory captured when this conversation started or was last compacted. Changes written since then are not shown here; use memory_search for the current text.",
        ),
        content: snapshot.content,
        source: Some(String::from("Markdown memory")),
        phase,
        baseline_provenance: Some(SystemPromptHookBaselineProvenance::new(
            SystemPromptHookBaselineKind::MemoryProfile,
            MARKDOWN_MEMORY_BASELINE_NODE_ID,
            MARKDOWN_MEMORY_BASELINE_SOURCE_ID,
        )),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tooling::native::resolve_markdown_memory_access_in;
    use vulcan_markdown_memory::{
        InjectionBudget, MarkdownMemorySettings, MarkdownMemoryStore, MemoryMode, NodeSource,
        ProjectContext, WriteOperation,
    };

    /// The snapshot block carries memory-profile provenance and appears only with content.
    /// 快照块带有记忆画像来源，且只在有内容时出现。
    #[test]
    fn snapshot_block_is_a_locked_memory_profile_segment() {
        let root = tempfile::tempdir().expect("temp root");
        let store = MarkdownMemoryStore::new(root.path());
        store
            .save_settings(&MarkdownMemorySettings {
                mode: MemoryMode::Markdown,
                ..MarkdownMemorySettings::default()
            })
            .expect("settings");
        let access = resolve_markdown_memory_access_in(root.path(), None, None)
            .expect("resolve")
            .expect("active");
        assert!(
            markdown_memory_block(&access, SystemPromptHookPhase::FirstSession)
                .expect("block")
                .is_none()
        );

        let operation: WriteOperation = serde_json::from_value(serde_json::json!({
            "action": "create", "scope": "user", "section": "identity",
            "title": "Role", "body": "Backend engineer.",
        }))
        .expect("operation");
        store
            .write(
                &ProjectContext::default(),
                None,
                NodeSource::User,
                &InjectionBudget::default(),
                vec![operation],
            )
            .expect("write");
        let block = markdown_memory_block(&access, SystemPromptHookPhase::FirstSession)
            .expect("block")
            .expect("content");
        assert_eq!(block.tag, MARKDOWN_MEMORY_SYSTEM_PROMPT_TAG);
        assert!(block.content.contains("Backend engineer."));
        let provenance = block.baseline_provenance.clone().expect("provenance");
        assert_eq!(provenance.kind, SystemPromptHookBaselineKind::MemoryProfile);

        let mut blocks = vec![block];
        remove_markdown_memory_block(&mut blocks);
        assert!(blocks.is_empty());
    }
}
