//! Source-based manual-removal inspection; never reuses age-based retention selectors.
//! 基于来源的人工移除检查，绝不复用基于年龄的保留清洗选择器。
use super::*;
use vulcan_service_api::{
    VmmNativeRemovalEvidenceIdentity, VmmNativeRemovalImpact, VmmNativeRemovalQuery,
    VmmNativeRemovalTarget,
};

impl NativeVmmMaintenance {
    /// Inspects an exact selection against one snapshot and the owning project's hot-window policy.
    /// 基于单份快照及归属项目热窗口策略检查准确选择。
    /// Returns related identities only; neither issues a confirmation token nor changes records.
    /// 仅返回关联身份，不签发确认令牌或修改记录。
    pub(crate) async fn inspect_removal(
        &self,
        query: VmmNativeRemovalQuery,
    ) -> Result<VmmNativeRemovalImpact, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.read().await;
        self.ensure_project(query.vmm_project_id).await?;
        let rules = self
            .runtime
            .project_retention_summary(query.vmm_project_id)
            .await?;
        build_impact(
            &self.runtime.storage_snapshot()?,
            query,
            rules.effective.turn_hot_window_size,
        )
    }
}

/// Resolves exact source relationships without applying expiry, level or protection filters.
/// 解析准确来源关系，不应用有效期、级别或保护过滤。
/// Takes immutable state, declared selection and hot-window size; rejects missing or paused sessions.
/// 接收不可变状态、声明选择及热窗口大小；拒绝缺失或已暂停的会话。
pub(super) fn build_impact(
    snapshot: &StorageSnapshot,
    mut query: VmmNativeRemovalQuery,
    hot_window_size: u32,
) -> Result<VmmNativeRemovalImpact, String> {
    let scope = project_scope(query.vmm_project_id)?;
    let subject = native_vmm_subject_id()?;
    let user_scope = native_vmm_subject_scope()?;
    let (ids, whole_sessions) = match &mut query.selection {
        VmmNativeRemovalTarget::Sessions { ids } => (ids, true),
        VmmNativeRemovalTarget::Turns { ids } => (ids, false),
    };
    if ids.is_empty() || ids.len() > 100 {
        return Err("请选择 1 至 100 个记忆会话或回合。".to_owned());
    }
    for id in ids.iter() {
        if id.is_empty() || id.trim() != id {
            return Err("所选身份不能为空或包含首尾空白。".to_owned());
        }
        if whole_sessions {
            SessionSourceId::new(id.clone()).map_err(|error| error.to_string())?;
        } else {
            super::super::client::parse_core_numeric_id(id, "vmm-turn-")
                .map_err(|error| error.to_string())?;
        }
    }
    ids.sort();
    ids.dedup();
    let selected_ids = ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
    // Session identity is composite; a matching title or a foreign project session is never sufficient.
    // 会话身份为复合身份，同名标题或其他项目会话绝不足以匹配。
    let mut sessions = BTreeSet::new();
    if whole_sessions {
        sessions.extend(selected_ids.iter().copied());
    }
    let mut selected_turns = Vec::new();
    let mut session_turns: BTreeMap<&str, Vec<(u64, &vulcan_memorymesh_core::Turn)>> =
        BTreeMap::new();
    for turn in &snapshot.dataset.turns {
        if turn.scope_id != scope || turn.subject_id != subject {
            continue;
        }
        if !whole_sessions && selected_ids.contains(turn.id.as_str()) {
            sessions.insert(turn.session_source_id.as_str());
        }
        if (whole_sessions && selected_ids.contains(turn.session_source_id.as_str()))
            || (!whole_sessions && selected_ids.contains(turn.id.as_str()))
        {
            selected_turns.push(turn);
        }
    }
    if !whole_sessions && selected_turns.len() != selected_ids.len() {
        return Err("一个或多个所选回合不在当前项目的活动存储中，请刷新。".to_owned());
    }
    for id in &sessions {
        let session = snapshot
            .memory_sessions
            .iter()
            .find(|session| {
                session.subject_id == subject
                    && session.scope_id == scope
                    && session.session_source_id.as_str() == *id
            })
            .ok_or_else(|| format!("项目中不存在记忆会话 `{id}`，请刷新。"))?;
        if !session.status.allows_runtime_memory() {
            return Err(format!("记忆会话 `{id}` 已回收或遗忘，不能再次选择回收。"));
        }
    }
    // Numeric ordering matches external newer.id > selected.id, including IDs 2 and 10.
    // 数值排序对应外置 newer.id > selected.id 规则，包含 ID 2 与 10。
    for turn in &snapshot.dataset.turns {
        if turn.scope_id == scope
            && turn.subject_id == subject
            && sessions.contains(turn.session_source_id.as_str())
        {
            let numeric = super::super::client::parse_core_numeric_id(&turn.id, "vmm-turn-")
                .map_err(|error| error.to_string())?;
            session_turns
                .entry(turn.session_source_id.as_str())
                .or_default()
                .push((numeric, turn));
        }
    }
    let mut hot_ids = BTreeSet::new();
    for turns in session_turns.values_mut() {
        turns.sort_by_key(|(numeric, _)| *numeric);
        for (_, turn) in turns.iter().rev().take(hot_window_size as usize) {
            hot_ids.insert(turn.id.as_str());
        }
    }
    let selected_turn_ids = selected_turns
        .iter()
        .map(|turn| turn.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut memories = snapshot
        .dataset
        .memories
        .iter()
        .filter(|memory| {
            memory.scope_id == scope
                && memory.is_visible_to_subject(&subject)
                && (memory
                    .source_turn_id
                    .as_ref()
                    .is_some_and(|id| selected_turn_ids.contains(id.as_str()))
                    || (whole_sessions
                        && memory
                            .origin_session_source_id
                            .as_ref()
                            .is_some_and(|id| sessions.contains(id.as_str()))))
        })
        .collect::<Vec<_>>();
    memories.sort_by(|left, right| left.id.cmp(&right.id));
    let memory_ids = memories
        .iter()
        .map(|memory| memory.id.to_string())
        .collect::<Vec<_>>();
    let memory_set = memory_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut context_evidence = snapshot
        .dataset
        .context_evidence
        .iter()
        .filter(|evidence| memory_set.contains(evidence.memory_id.as_str()))
        .map(|evidence| VmmNativeRemovalEvidenceIdentity {
            memory_id: evidence.memory_id.to_string(),
            context_key: evidence.context_key.clone(),
            context_value: evidence.context_value.clone(),
        })
        .collect::<Vec<_>>();
    context_evidence.sort_by(|left, right| {
        (&left.memory_id, &left.context_key, &left.context_value).cmp(&(
            &right.memory_id,
            &right.context_key,
            &right.context_value,
        ))
    });
    let mut project_profile_node_ids = Vec::new();
    let mut user_profile_node_ids = Vec::new();
    for node in &snapshot.dataset.profile_nodes {
        if !node
            .source_turn_ids
            .iter()
            .any(|id| selected_turn_ids.contains(id.as_str()))
        {
            continue;
        }
        // Project profiles are scope-owned; only personal profiles require the fixed subject.
        // 项目画像按作用域归属，只有个人画像要求固定主体。
        if node.scope_id == scope {
            project_profile_node_ids.push(node.id.to_string());
        } else if node.scope_id == user_scope && node.subject_id.as_ref() == Some(&subject) {
            user_profile_node_ids.push(node.id.to_string());
        }
    }
    project_profile_node_ids.sort();
    user_profile_node_ids.sort();
    // The already validated numeric map orders selected turns without a second parser or fallback.
    // 已校验数值映射对所选回合排序，不使用第二套解析器或兜底。
    let mut ordered = session_turns
        .values()
        .flatten()
        .filter(|(_, turn)| selected_turn_ids.contains(turn.id.as_str()))
        .copied()
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(numeric, _)| *numeric);
    let impact = VmmNativeRemovalImpact {
        storage_version: snapshot.storage_version,
        hot_window_size,
        session_source_ids: sessions.into_iter().map(str::to_owned).collect(),
        turn_ids: ordered
            .iter()
            .map(|(_, turn)| turn.id.to_string())
            .collect(),
        pending_turn_ids: ordered
            .iter()
            .filter(|(_, turn)| turn.status == TurnStatus::Pending)
            .map(|(_, turn)| turn.id.to_string())
            .collect(),
        hot_window_turn_ids: ordered
            .iter()
            .filter(|(_, turn)| hot_ids.contains(turn.id.as_str()))
            .map(|(_, turn)| turn.id.to_string())
            .collect(),
        vector_memory_ids: memories
            .iter()
            .filter(|memory| memory.vector_space_id.is_some())
            .map(|memory| memory.id.to_string())
            .collect(),
        memory_ids,
        context_evidence,
        project_profile_node_ids,
        user_profile_node_ids,
        query,
    };
    Ok(impact)
}
