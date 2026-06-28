use super::*;

/// One persisted VMM PostAction writeback retry entry.
/// 一条持久化的 VMM PostAction 写回重试项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmmWritebackOutboxEntry {
    /// Stable local outbox entry identifier.
    /// 稳定的本地 outbox 条目标识。
    pub id: String,
    /// Original VMM PostAction request to replay.
    /// 需要重放的原始 VMM PostAction 请求。
    pub request: PostActionRequest,
    /// Number of failed delivery attempts recorded so far.
    /// 当前累计记录的失败投递次数。
    #[serde(default)]
    pub attempt_count: u32,
    /// Last failure message observed during delivery.
    /// 最近一次投递时观察到的失败消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Unix-millis text captured when the entry was created.
    /// 捕获该条目创建时间的 Unix 毫秒文本。
    pub created_at: String,
    /// Unix-millis text captured when the entry was last attempted or updated.
    /// 捕获该条目最近尝试或更新时刻的 Unix 毫秒文本。
    pub updated_at: String,
}

/// Session-scoped VMM profile synchronization state.
/// 会话级 VMM 画像同步状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmProfileSyncState {
    /// Latest compaction id already covered by a full profile hook injection.
    /// 已被全量画像 hook 注入覆盖的最近压缩标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covered_compaction_id: Option<String>,
    /// Number of completed user turns since the latest profile sync check.
    /// 自最近一次画像同步检查以来已经完成的用户回合数。
    #[serde(default)]
    pub completed_turns_since_sync: u32,
    /// Latest active profile node snapshot keyed by VMM profile node id.
    /// 按 VMM 画像节点标识记录的最近 active 画像节点快照。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nodes_by_id: BTreeMap<u64, ProfileNodeEntry>,
    /// Baseline full profile content reused on every provider request.
    /// 每次 Provider 请求都会复用的基础全量画像内容。
    #[serde(
        default,
        alias = "system_prompt_content",
        skip_serializing_if = "Option::is_none"
    )]
    pub base_profile_content: Option<String>,
    /// Latest full profile content used as the incremental refresh comparison baseline.
    /// 用作增量刷新对比基准的最近全量画像内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_profile_content: Option<String>,
    /// Unix-millis text captured when the latest profile sync completed.
    /// 最近一次画像同步完成时捕获的 Unix 毫秒文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl VmmProfileSyncState {
    /// Returns whether no profile sync state has been initialized yet.
    /// 返回当前是否尚未初始化任何画像同步状态。
    pub fn is_empty(&self) -> bool {
        self.covered_compaction_id.is_none()
            && self.completed_turns_since_sync == 0
            && self.nodes_by_id.is_empty()
            && self.base_profile_content.is_none()
            && self.delta_profile_content.is_none()
            && self.updated_at.is_none()
    }

    /// Records one completed logical user turn after provider success.
    /// 在 Provider 成功后记录一个已完成逻辑用户回合。
    pub fn record_completed_turn(&mut self) {
        self.completed_turns_since_sync = self.completed_turns_since_sync.saturating_add(1);
    }
}
