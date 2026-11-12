//! V1 compaction frontier rules (build-time fixture).
//! V1 压缩前沿规则(构建期 fixture)。
//!
//! The runtime pointer in `runtime.json` is the published frontier: nodes the
//! pointer does not reference are unpublished candidates that must be trimmed
//! before the next append, while the loader only reports and ignores them.
//! `runtime.json` 中的指针是已发布前沿：指针未引用的节点是未发布候选，在
//! 下次追加前必须被裁剪，而加载器只报告并忽略它们。

use crate::v1::context::CompactionNode;
use crate::v1::ids::{CompactionId, Seq};

/// Trims unpublished candidate nodes located after the runtime pointer.
/// 裁剪位于 runtime 指针之后的未发布候选节点。
///
/// With no published pointer the nodes list must be empty; once a pointer is
/// published every node at or before it is retained, and anything after it is
/// an unpublished candidate that must be dropped before the next append.
/// 没有已发布指针时节点列表必须为空；指针发布后，位于其上的节点全部保留，
/// 其后的任何节点都是未发布候选，在下次追加前必须丢弃。
pub fn trim_unpublished_candidates(
    nodes: &[CompactionNode],
    current_id: Option<CompactionId>,
) -> Vec<CompactionNode> {
    match current_id {
        None => Vec::new(),
        Some(current) => nodes
            .iter()
            .take_while(|node| node.compaction_id <= current)
            .cloned()
            .collect(),
    }
}

/// Selects the last valid compaction node at or before one target seq.
/// 选择目标 seq 及之前最后一个有效压缩节点。
pub fn select_node_for_seq(nodes: &[CompactionNode], target_seq: Seq) -> Option<&CompactionNode> {
    nodes
        .iter()
        .rfind(|node| node.occurred_at_seq <= target_seq)
}

/// Returns whether `nodes` have increasing occurrences/ids and a nondecreasing retained frontier.
/// 返回`nodes`的发生点及 ID 是否递增、保留前沿是否不倒退。
pub fn assert_node_order(nodes: &[CompactionNode]) -> bool {
    let mut previous_occurred = 0u64;
    let mut previous_id = 0u64;
    // A published generation cannot restore already discarded history through a later compaction.
    // 已发布代次不能通过后续压缩重新带回已经丢弃的历史。
    let mut previous_retained = 0u64;
    for node in nodes {
        if node.occurred_at_seq.value() <= previous_occurred
            || node.compaction_id.value() <= previous_id
            || node.retained_from_seq.value() < previous_retained
        {
            return false;
        }
        previous_occurred = node.occurred_at_seq.value();
        previous_id = node.compaction_id.value();
        previous_retained = node.retained_from_seq.value();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::ids::ApiCallId;

    fn node(id: u64, occurred: u64, retained: u64) -> CompactionNode {
        CompactionNode {
            compaction_id: CompactionId::try_new(id).unwrap(),
            api_call_id: ApiCallId::try_new(1).unwrap(),
            api_stats_seq: Seq::try_new(occurred + 1).unwrap(),
            occurred_at_seq: Seq::try_new(occurred).unwrap(),
            retained_from_seq: Seq::try_new(retained).unwrap(),
            content: crate::v1::context::CompactionContent::Summary {
                text: String::new(),
                image_excerpts: Vec::new(),
                image_replay_from_seq: None,
            },
            created_at_ms: 1,
        }
    }

    /// Unpublished candidates after the pointer are trimmed on the next write.
    /// 指针之后的未发布候选在下一次写入时被裁剪。
    #[test]
    fn trims_unpublished_candidates_after_pointer() {
        let nodes = vec![node(1, 10, 5), node(2, 20, 15), node(3, 30, 25)];
        let trimmed = trim_unpublished_candidates(&nodes, Some(CompactionId::try_new(2).unwrap()));
        assert_eq!(trimmed.len(), 2);
        assert_eq!(trimmed[0].compaction_id.value(), 1);
        assert_eq!(trimmed[1].compaction_id.value(), 2);
        // No published pointer means no nodes at all.
        // 没有已发布指针意味着没有任何节点。
        assert!(trim_unpublished_candidates(&nodes, None).is_empty());
    }

    /// Node selection picks the last node at or before the target seq.
    /// 节点选择取目标 seq 及之前最后一个节点。
    #[test]
    fn selects_last_node_at_or_before_target() {
        let nodes = vec![node(1, 10, 5), node(2, 20, 15), node(3, 30, 25)];
        let selected = select_node_for_seq(&nodes, Seq::try_new(22).unwrap()).unwrap();
        assert_eq!(selected.compaction_id.value(), 2);
        let early = select_node_for_seq(&nodes, Seq::try_new(5).unwrap());
        assert!(early.is_none());
    }

    /// Node order validation rejects mixed order.
    /// 节点顺序校验拒绝乱序。
    #[test]
    fn order_validation_rejects_reversed_nodes() {
        assert!(assert_node_order(&[node(1, 10, 5), node(2, 20, 15)]));
        assert!(!assert_node_order(&[node(2, 20, 15), node(1, 10, 5)]));
        assert!(!assert_node_order(&[node(1, 10, 5), node(1, 20, 15)]));
        assert!(!assert_node_order(&[node(1, 10, 5), node(2, 10, 5)]));
    }
}
