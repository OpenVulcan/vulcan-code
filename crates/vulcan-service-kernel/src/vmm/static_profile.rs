//! VMM static profile helpers.
//! VMM static profile 辅助逻辑。

use super::*;

/// Builds the structured static VMM profile snapshot from active profile nodes.
/// 根据 active 画像节点构建结构化静态 VMM 画像快照。
///
/// # Parameters
/// 参数
/// `state` is the selected session runtime state containing profile-sync nodes.
/// `state` 是包含画像同步节点的当前会话运行态。
///
/// # Returns
/// 返回值
/// A grouped static profile summary when active profile nodes exist.
/// 存在 active 画像节点时返回分组后的静态画像摘要。
pub(crate) fn static_vmm_profile_snapshot(
    state: &SessionRuntimeStateEntry,
) -> Option<VmmStaticProfileSummary> {
    let sections = static_vmm_profile_sections(&state.vmm_profile_sync.nodes_by_id);
    if sections.is_empty() {
        return None;
    }
    Some(VmmStaticProfileSummary {
        updated_at: state.vmm_profile_sync.updated_at.clone(),
        sections,
    })
}

/// Groups active VMM profile nodes by target and profile date.
/// 按目标作用域和画像日期分组 active VMM 画像节点。
pub(crate) fn static_vmm_profile_sections(
    nodes_by_id: &BTreeMap<u64, vulcan_vmm::ProfileNodeEntry>,
) -> Vec<VmmStaticProfileSectionSummary> {
    static_vmm_profile_targets()
        .into_iter()
        .filter_map(|target| static_vmm_profile_section(nodes_by_id, target))
        .collect()
}

/// Builds one static VMM profile section for a target scope.
/// 为单个目标作用域构建一个静态 VMM 画像段落。
pub(crate) fn static_vmm_profile_section(
    nodes_by_id: &BTreeMap<u64, vulcan_vmm::ProfileNodeEntry>,
    target: vulcan_vmm::ProfileTarget,
) -> Option<VmmStaticProfileSectionSummary> {
    let mut grouped: BTreeMap<String, Vec<&vulcan_vmm::ProfileNodeEntry>> = BTreeMap::new();
    for node in nodes_by_id.values().filter(|node| node.target == target) {
        if node.content.trim().is_empty() {
            continue;
        }
        grouped
            .entry(static_vmm_profile_date(node))
            .or_default()
            .push(node);
    }
    if grouped.is_empty() {
        return None;
    }
    let dates = grouped
        .into_iter()
        .map(|(date, mut nodes)| {
            nodes.sort_by_key(|node| node.profile_node_id);
            VmmStaticProfileDateGroupSummary {
                date,
                nodes: nodes
                    .into_iter()
                    .map(static_vmm_profile_node_summary)
                    .collect(),
            }
        })
        .collect();
    Some(VmmStaticProfileSectionSummary {
        target: static_vmm_profile_target_key(target).to_string(),
        label: static_vmm_profile_target_label(target).to_string(),
        dates,
    })
}

/// Converts one active VMM node into a WebUI profile node summary.
/// 将单个 active VMM 节点转换为 WebUI 画像节点摘要。
pub(crate) fn static_vmm_profile_node_summary(
    node: &vulcan_vmm::ProfileNodeEntry,
) -> VmmStaticProfileNodeSummary {
    VmmStaticProfileNodeSummary {
        id: node.profile_node_id,
        priority: static_vmm_profile_marker("P", &node.priority),
        level: static_vmm_profile_marker("L", &node.level),
        refresh_weight: format!("W{}", node.refresh_weight),
        content: node.content.trim().to_string(),
        source_kind: static_vmm_profile_source_kind(node.source_kind).to_string(),
        source_id: node.source_id,
        bind_id: node.bind_id,
    }
}

/// Returns VMM profile targets in stable display order.
/// 按稳定展示顺序返回 VMM 画像目标。
pub(crate) fn static_vmm_profile_targets() -> [vulcan_vmm::ProfileTarget; 4] {
    [
        vulcan_vmm::ProfileTarget::ProfileTargetTeam,
        vulcan_vmm::ProfileTarget::ProfileTargetSpace,
        vulcan_vmm::ProfileTarget::ProfileTargetProject,
        vulcan_vmm::ProfileTarget::ProfileTargetUser,
    ]
}

/// Returns one compact target key for WebUI state.
/// 返回一个用于 WebUI 状态的紧凑目标键。
pub(crate) fn static_vmm_profile_target_key(target: vulcan_vmm::ProfileTarget) -> &'static str {
    match target {
        vulcan_vmm::ProfileTarget::ProfileTargetTeam => "team",
        vulcan_vmm::ProfileTarget::ProfileTargetSpace => "space",
        vulcan_vmm::ProfileTarget::ProfileTargetProject => "project",
        vulcan_vmm::ProfileTarget::ProfileTargetUser => "user",
        vulcan_vmm::ProfileTarget::ProfileTargetAll => "all",
        vulcan_vmm::ProfileTarget::ProfileTargetUnspecified => "unspecified",
    }
}

/// Returns the visible target label for one VMM profile scope.
/// 返回单个 VMM 画像作用域的可见标签。
pub(crate) fn static_vmm_profile_target_label(target: vulcan_vmm::ProfileTarget) -> &'static str {
    match target {
        vulcan_vmm::ProfileTarget::ProfileTargetTeam => "TEAM",
        vulcan_vmm::ProfileTarget::ProfileTargetSpace => "SPACE",
        vulcan_vmm::ProfileTarget::ProfileTargetProject => "PROJECT",
        vulcan_vmm::ProfileTarget::ProfileTargetUser => "USER",
        vulcan_vmm::ProfileTarget::ProfileTargetAll => "ALL",
        vulcan_vmm::ProfileTarget::ProfileTargetUnspecified => "UNSPECIFIED",
    }
}

/// Normalizes a VMM profile marker such as priority or lifetime.
/// 规范化 VMM 画像优先级或生命周期标记。
pub(crate) fn static_vmm_profile_marker(prefix: &str, value: &str) -> String {
    let value = value.trim().to_ascii_uppercase();
    if value.is_empty() {
        return format!("{prefix}?");
    }
    if value.starts_with(prefix) {
        value
    } else {
        format!("{prefix}{value}")
    }
}

/// Returns the profile date label for one VMM profile node.
/// 返回单个 VMM 画像节点的画像日期标签。
pub(crate) fn static_vmm_profile_date(node: &vulcan_vmm::ProfileNodeEntry) -> String {
    let date = node.profile_date.trim();
    if date.is_empty() {
        String::from("unknown-date")
    } else {
        date.to_string()
    }
}

/// Converts a VMM profile source kind into a stable API key.
/// 将 VMM 画像来源类型转换为稳定 API 键。
pub(crate) fn static_vmm_profile_source_kind(
    kind: vulcan_vmm::ProfileNodeSourceKind,
) -> &'static str {
    match kind {
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindTurnExtract => "turn_extract",
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindManualInstruction => {
            "manual_instruction"
        }
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindSystemSeed => "system_seed",
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindRetainedAfterUserDelete => {
            "retained_after_user_delete"
        }
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindUnspecified => "unspecified",
    }
}
