//! Frozen read-only projection of exact user-prompt media for historical consumers.
//! 为历史消费者提供精确用户提示词媒体的冻结只读投影。

use super::*;
use vulcan_agent_runtime::prompt_media_projection::PromptMediaProjectionRuntime;

/// Keeps the planner's exact snapshot and source-message mapping without consulting newer session state.
/// 保存规划器精确快照及来源消息映射，不查询更新的会话状态。
struct FrozenPromptMediaProjection {
    /// Original project and prompt parameters from the durable planning frontier.
    /// 来自可靠规划前沿的原项目及提示词参数。
    snapshot: SessionProviderProjectionSnapshot,
    /// Exact host session owning the source snapshot.
    /// 持有来源快照的精确宿主会话。
    session_id: String,
    /// Stable planner message identity mapped to its original projection position.
    /// 映射至原投影位置的稳定规划器消息身份。
    message_indexes: BTreeMap<String, usize>,
    /// The existing project catalog owner, never a new per-request registry.
    /// 既有项目目录所有者，绝不是每次请求新建的注册表。
    registry: Arc<ProjectResourceCatalogRegistry>,
}

impl std::fmt::Debug for FrozenPromptMediaProjection {
    /// Formats only source count into `formatter`, never private paths, prompt text or upload references.
    /// 只向`formatter`格式化来源数量，绝不输出私有路径、提示词文本或上传引用。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FrozenPromptMediaProjection")
            .field("message_count", &self.message_indexes.len())
            .finish_non_exhaustive()
    }
}

/// Freezes exact `snapshot`, ordered `(message_id, index)` sources and `session_id` with the existing `registry`.
/// 将精确`snapshot`、有序`(message_id, index)`来源及`session_id`与既有`registry`一同冻结。
/// Returns a read-only media authority or rejects duplicate, unordered or out-of-range source mappings.
/// 返回只读媒体权威，或拒绝重复、乱序、越界来源映射。
pub(crate) fn frozen_prompt_media_projection(
    snapshot: SessionProviderProjectionSnapshot,
    session_id: String,
    sources: Vec<(String, usize)>,
    registry: Arc<ProjectResourceCatalogRegistry>,
) -> Result<Arc<dyn PromptMediaProjectionRuntime>, String> {
    // Freezing cannot silently repair a contradictory source snapshot before window selection.
    // 冻结不能在窗口选择之前静默修复矛盾来源快照。
    if snapshot.items.len() != snapshot.sourced_items.len()
        || snapshot
            .items
            .iter()
            .zip(&snapshot.sourced_items)
            .any(|(item, source)| item != &source.item)
    {
        return Err(String::from(
            "Frozen prompt media semantics differ from their source projection.",
        ));
    }
    validate_prompt_media_preparation_frontier(&snapshot).map_err(|error| error.to_string())?;
    if session_id.trim().is_empty() || sources.windows(2).any(|pair| pair[0].1 >= pair[1].1) {
        return Err(String::from(
            "Frozen prompt media source owner or chronology is invalid.",
        ));
    }
    // Build only a one-to-one index from the planner's actual source identities.
    // 只从规划器真实来源身份建立一一对应索引。
    let mut message_indexes = BTreeMap::new();
    for (id, index) in sources {
        if id.trim().is_empty()
            || index >= snapshot.sourced_items.len()
            || message_indexes.insert(id, index).is_some()
        {
            return Err(String::from(
                "Frozen prompt media source mapping is ambiguous or invalid.",
            ));
        }
    }
    Ok(Arc::new(FrozenPromptMediaProjection {
        snapshot,
        session_id,
        message_indexes,
        registry,
    }))
}

#[async_trait::async_trait]
impl PromptMediaProjectionRuntime for FrozenPromptMediaProjection {
    /// Replays requested `source_message_ids` through the shared planner under exact `context`, without upload authority.
    /// 在精确`context`下通过共享规划器重放请求的`source_message_ids`，不具有上传权限。
    async fn project_readonly(
        &self,
        source_message_ids: &[String],
        context: ProviderRequestMediaContext,
    ) -> Result<BTreeMap<String, AiMessage>, String> {
        self.project_with_mode(
            source_message_ids,
            context,
            PromptMediaPreparationMode::ReadOnly,
        )
        .await
    }

    /// Captures the same original sources for `context`, refusing any newly required upload.
    /// 为`context`捕获相同原来源，拒绝任何新必需上传。
    async fn project_capture(
        &self,
        source_message_ids: &[String],
        context: ProviderRequestMediaContext,
    ) -> Result<BTreeMap<String, AiMessage>, String> {
        self.project_with_mode(
            source_message_ids,
            context,
            PromptMediaPreparationMode::Capture,
        )
        .await
    }
}

impl FrozenPromptMediaProjection {
    /// Selects exact `source_message_ids` for frozen `context` under the declared read-only `mode`.
    /// 在声明的只读`mode`下为冻结`context`选择精确`source_message_ids`。
    /// Returns keyed user replacements or an explicit source/side-effect boundary error.
    /// 返回按键索引的用户替换项，或明确来源/副作用边界错误。
    async fn project_with_mode(
        &self,
        source_message_ids: &[String],
        context: ProviderRequestMediaContext,
        mode: PromptMediaPreparationMode,
    ) -> Result<BTreeMap<String, AiMessage>, String> {
        if context.session_scope_id != self.session_id {
            return Err(String::from(
                "Read-only prompt media projection belongs to another session.",
            ));
        }
        // Validate the entire selected window before consulting the resource catalog.
        // 查询资源目录之前验证整个所选窗口。
        let indexes = source_message_ids.iter().map(|id| self.message_indexes.get(id).copied().ok_or_else(|| String::from("Read-only prompt media source is absent from its frozen planning snapshot."))).collect::<Result<Vec<_>, _>>()?;
        if indexes.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(String::from(
                "Read-only prompt media source order differs from its frozen chronology.",
            ));
        }
        // Select exact source items; original parameters remain byte-for-byte untouched in the captured snapshot.
        // 选择精确来源项；捕获快照中的原参数保持逐字节不变。
        let mut selected = self.snapshot.clone();
        selected.sourced_items = indexes
            .iter()
            .map(|index| self.snapshot.sourced_items[*index].clone())
            .collect();
        selected.items = selected
            .sourced_items
            .iter()
            .map(|source| source.item.clone())
            .collect();
        selected.preparing_user_prompt_media_seqs.retain(|seq| {
            selected
                .sourced_items
                .iter()
                .any(|source| source.source_from_seq == *seq)
        });
        // Read-only mode independently forbids all fresh upload decisions and prompt-origin bindings.
        // 只读模式独立禁止全部新上传决定及提示词原身份绑定。
        let prepared = prepare_user_media_with_mode(
            &selected,
            Some(&context),
            Arc::clone(&self.registry),
            mode,
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok(prepared
            .prompts
            .into_iter()
            .map(|(index, prompt)| {
                (
                    source_message_ids[index].clone(),
                    AiMessage {
                        role: MessageRole::User,
                        content: prompt.content,
                        provider_context_items: Vec::new(),
                    },
                )
            })
            .collect())
    }
}
