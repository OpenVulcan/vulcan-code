use super::*;

/// One persisted VMM PostAction writeback retry entry.
/// 一条持久化的 VMM PostAction 写回重试项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmmWritebackOutboxEntry {
    /// Exact process-local delivery generation; old generations are never adopted after a source switch.
    /// 准确进程内投递代次；来源切换后绝不接管旧代次。
    pub permit_id: u64,
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
#[serde(deny_unknown_fields)]
pub struct VmmProfileSyncState {
    /// Exact source and native visibility that produced this baseline and its deltas.
    /// 产生当前基线及增量的准确来源和原生可见范围。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_source: Option<vulcan_vmm::VmmMemoryReadSource>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
        self.read_source.is_none()
            && self.covered_compaction_id.is_none()
            && self.completed_turns_since_sync == 0
            && self.nodes_by_id.is_empty()
            && self.base_profile_content.is_none()
            && self.delta_profile_content.is_none()
            && self.updated_at.is_none()
    }

    /// Starts a fresh baseline whenever the owner, transport or native visibility changes.
    /// 归属、传输或原生可见范围变化时开始新基线。
    /// Takes the current validated read source; returns whether all prior baseline/delta metadata was reset.
    /// 接收当前已校验读取来源；返回是否重置了全部此前基线及增量元数据。
    pub fn select_read_source(
        &mut self,
        source: &vulcan_vmm::VmmMemoryReadSource,
    ) -> Result<bool, vulcan_vmm::VmmClientError> {
        source.validate()?;
        if self.read_source.as_ref() == Some(source) {
            return Ok(false);
        }
        // Numeric node IDs and refresh watermarks belong to one source, not to the host session alone.
        // 数字节点 ID 和刷新水位属于一个来源，而不是仅属于宿主会话。
        *self = Self {
            read_source: Some(source.clone()),
            ..Default::default()
        };
        Ok(true)
    }

    /// Records one completed logical user turn after provider success.
    /// 在 Provider 成功后记录一个已完成逻辑用户回合。
    pub fn record_completed_turn(&mut self) {
        self.completed_turns_since_sync = self.completed_turns_since_sync.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source switch resets profile content and refresh metadata even before the threshold is reached.
    /// 即使尚未达到阈值，来源切换也重置画像内容和刷新元数据。
    #[test]
    fn profile_baseline_is_bound_to_complete_read_source() {
        // Preserve same-source cache stability and reset only when its exact read identity changes.
        // 保持同来源缓存稳定，仅在准确读取身份变化时重置。
        let owner = vulcan_vmm::VmmMemorySource::native(7).expect("owner");
        let first = vulcan_vmm::VmmMemoryReadSource::new(owner.clone(), vec![7, 8])
            .expect("initial visibility");
        let mut state = VmmProfileSyncState {
            read_source: Some(first.clone()),
            base_profile_content: Some("old baseline".to_owned()),
            delta_profile_content: Some("old delta".to_owned()),
            covered_compaction_id: Some("compaction".to_owned()),
            completed_turns_since_sync: 2,
            updated_at: Some("42".to_owned()),
            ..Default::default()
        };
        let before = state.clone();
        assert!(!state.select_read_source(&first).expect("same source"));
        assert_eq!(state, before);
        let changed =
            vulcan_vmm::VmmMemoryReadSource::new(owner, vec![7, 9]).expect("changed visibility");
        assert!(state.select_read_source(&changed).expect("new source"));
        assert_eq!(
            state,
            VmmProfileSyncState {
                read_source: Some(changed),
                ..Default::default()
            }
        );
        // A baseline without provenance is never silently assigned to a current project.
        // 没有来源的基线绝不被静默归入当前项目。
        state.read_source = None;
        state.base_profile_content = Some("unowned".to_owned());
        assert!(state.select_read_source(&first).expect("unowned reset"));
        assert!(state.base_profile_content.is_none());
        assert!(!state.is_empty());
        let json = serde_json::to_value(&state).expect("encode");
        assert_eq!(
            serde_json::from_value::<VmmProfileSyncState>(json).expect("decode"),
            state
        );
    }

    /// Verifies the removed profile field alias is rejected instead of migrated or ignored.
    /// 验证已移除的画像字段别名会被拒绝，而不是迁移或忽略。
    #[test]
    fn vmm_profile_sync_rejects_historical_field_alias() {
        // historical_json uses the removed persisted field spelling.
        // historical_json 使用已经移除的持久化字段拼写。
        let historical_json = serde_json::json!({
            "system_prompt_content": "historical profile"
        });

        // error proves strict current-format decoding rejects the historical alias.
        // error 证明严格的当前格式解码会拒绝历史别名。
        let error = serde_json::from_value::<VmmProfileSyncState>(historical_json)
            .expect_err("historical VMM profile alias must be rejected");

        assert!(
            error
                .to_string()
                .contains("unknown field `system_prompt_content`")
        );
    }
}
