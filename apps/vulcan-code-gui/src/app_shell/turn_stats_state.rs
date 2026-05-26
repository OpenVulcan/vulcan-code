//! Turn statistics merge anchor extracted from the main workbench module.
//! 从主工作台模块中抽取的轮次统计合并锚点。

/// Stable merge anchor derived from the visible user row that belongs to one completed turn.
/// 从属于某个已完成轮次的可见用户行派生出来的稳定合并锚点。
#[derive(Debug, Clone)]
pub(super) struct TurnStatsMergeAnchor {
    /// Stable visible user record identifier for the turn.
    /// 该轮次的稳定可见用户记录标识。
    pub(super) user_record_id: Option<String>,
    /// Stable creation timestamp of the visible user row.
    /// 可见用户行的稳定创建时间戳。
    pub(super) user_created_at: Option<String>,
    /// Normalized visible user text for legacy rows that do not yet carry dedicated anchor metadata.
    /// 针对尚未携带专用锚点元数据的旧记录使用的归一化可见用户文本。
    pub(super) user_text: Option<String>,
}

impl PartialEq for TurnStatsMergeAnchor {
    /// Compares two turn anchors using the strongest available stable fields first.
    /// 按最强的稳定字段优先对比两个轮次锚点。
    fn eq(&self, other: &Self) -> bool {
        match (&self.user_record_id, &other.user_record_id) {
            (Some(left), Some(right)) => left == right,
            _ => self.user_created_at == other.user_created_at && self.user_text == other.user_text,
        }
    }
}

impl Eq for TurnStatsMergeAnchor {}
