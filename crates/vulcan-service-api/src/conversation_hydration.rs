use super::*;

/// Describes one retained lazy conversation body window.
/// 描述一个已保留的懒加载会话正文窗口。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HydratedConversationWindowSummary {
    /// Conversation identifier owning this retained body window.
    /// 拥有该保留正文窗口的会话标识。
    pub session_id: String,
    /// Whether session-scoped subagent metadata has been loaded for this retained body window.
    /// 该保留正文窗口对应会话的子智能体元数据是否已经完成加载。
    pub subagent_metadata_loaded: bool,
    /// Earliest loaded transcript ordinal used to request older history.
    /// 用于继续请求更早历史的已加载最早转录序号。
    pub earliest_ordinal: Option<i64>,
    /// Whether older transcript turns still exist before this window.
    /// 当前窗口之前是否仍存在更早的转录轮次。
    pub has_more_history: bool,
}

/// Describes lazy loading progress for one conversation transcript body.
/// 描述单个会话正文懒加载进度。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHydrationProgress {
    /// Conversation identifier being loaded.
    /// 正在加载的会话标识。
    pub session_id: String,
    /// Current loading status key.
    /// 当前加载状态键。
    pub status: ConversationHydrationStatus,
    /// Current phase key used by the WebUI i18n layer.
    /// WebUI 多语言层使用的当前阶段键。
    pub phase: String,
    /// Human-readable fallback message.
    /// 面向用户的回退消息。
    pub message: String,
    /// Completed progress percentage from 0 to 100.
    /// 0 到 100 的已完成百分比。
    pub percent: u8,
}

/// Stable lazy conversation hydration status values.
/// 稳定的懒加载会话正文水合状态值。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ConversationHydrationStatus {
    /// The conversation body is being loaded.
    /// 会话正文正在加载。
    Loading,
    /// The conversation body has been loaded.
    /// 会话正文已经载入。
    Ready,
    /// The conversation body failed to load.
    /// 会话正文载入失败。
    Error,
}

/// Response returned by a lazy conversation body load request.
/// 懒加载会话正文请求返回的响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessagesLoadResponse {
    /// Complete workbench snapshot rebuilt with the hydrated selected conversation.
    /// 使用已水合选中会话重建出的完整工作台快照。
    pub workbench: Box<WorkbenchSnapshot>,
    /// Loaded conversation messages.
    /// 已加载的会话消息。
    pub messages: Vec<ConversationMessage>,
    /// Earliest loaded transcript ordinal used to request older history.
    /// 用于继续请求更早历史的已加载最早转录序号。
    pub earliest_ordinal: Option<i64>,
    /// Whether older transcript turns still exist before this window.
    /// 当前窗口之前是否仍存在更早的转录轮次。
    pub has_more_history: bool,
    /// Final loading progress.
    /// 最终加载进度。
    pub progress: ConversationHydrationProgress,
}

/// Response returned by one older conversation history page request.
/// 单次更早会话历史页请求返回的响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHistoryPageResponse {
    /// Conversation identifier owning this page.
    /// 拥有该页历史的会话标识。
    pub session_id: String,
    /// Complete workbench snapshot after merging this older page.
    /// 合并该页更早历史后的完整工作台快照。
    pub workbench: Box<WorkbenchSnapshot>,
    /// Older conversation messages in ascending display order.
    /// 按展示正序排列的更早会话消息。
    pub messages: Vec<ConversationMessage>,
    /// Earliest loaded transcript ordinal after this page.
    /// 加载该页后已加载的最早转录序号。
    pub earliest_ordinal: Option<i64>,
    /// Whether more older transcript turns still exist.
    /// 是否仍存在更多更早的转录轮次。
    pub has_more_history: bool,
}
