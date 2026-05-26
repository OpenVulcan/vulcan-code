/// Session prompt baseline state.
/// 会话提示词基线状态。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptSessionBaseline {
    /// Latest stable assembly fingerprint.
    /// 最新稳定装配指纹。
    pub assembly_fingerprint: u64,
    /// Whether the next turn requires a full rebuild.
    /// 下一轮是否要求全量重建。
    pub requires_full_rebuild: bool,
}

impl PromptSessionBaseline {
    /// Build one baseline from the provided fingerprint.
    /// 使用给定指纹构建一个基线。
    pub fn new(assembly_fingerprint: u64) -> Self {
        Self {
            assembly_fingerprint,
            requires_full_rebuild: false,
        }
    }

    /// Mark the baseline as requiring a full rebuild.
    /// 将基线标记为需要全量重建。
    pub fn mark_full_rebuild(&mut self) {
        self.requires_full_rebuild = true;
    }
}
