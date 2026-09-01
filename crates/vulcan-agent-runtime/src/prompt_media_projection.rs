//! Read-only prompt-owned media projection shared by historical consumers.
//! 历史消费者共用的只读提示词所有媒体投影。

use crate::agent_runtime::events::ProviderRequestMediaContext;
use std::collections::BTreeMap;
use vulcan_ai_contract::AiMessage;

/// Frozen host source authority for one chronological user-prompt projection, never a latest-file cache.
/// 一次时序用户提示词投影的冻结宿主来源权威，绝不是最近文件缓存。
#[async_trait::async_trait]
pub trait PromptMediaProjectionRuntime: Send + Sync + std::fmt::Debug {
    /// Projects exact ordered `source_message_ids` against frozen target `context` without uploading or mutating original prompts.
    /// 对照冻结目标`context`投影精确有序`source_message_ids`，不上传，也不修改原提示词。
    /// Returns replacements only for resource-bearing user messages; unknown or reordered source identities fail explicitly.
    /// 只返回含资源用户消息的替换项；未知或重排来源身份显式失败。
    async fn project_readonly(
        &self,
        source_message_ids: &[String],
        context: ProviderRequestMediaContext,
    ) -> Result<BTreeMap<String, AiMessage>, String>;

    /// Captures exact media for ordered `source_message_ids` under `context`, without sending uploads or freezing prompts.
    /// 在`context`下捕获有序`source_message_ids`的精确媒体，不发起上传，也不冻结提示词。
    /// New inline content may be inspected; a required new upload returns an explicit not-yet-capturable error.
    /// 可以检查新内联内容；必需的新上传返回明确的尚不可捕获错误。
    async fn project_capture(
        &self,
        source_message_ids: &[String],
        context: ProviderRequestMediaContext,
    ) -> Result<BTreeMap<String, AiMessage>, String>;
}
