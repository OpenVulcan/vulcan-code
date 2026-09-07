//! Resolves native turn detail windows from one authoritative active snapshot.
//! 从单份权威活跃快照解析原生回合详情窗口。

use std::collections::{BTreeMap, BTreeSet};
use vulcan_memorymesh_core::{ScopeAccess, ScopeId, SessionSourceId, SubjectId, Turn, TurnId};
use vulcan_vmm::VmmClientError;

use super::client::parse_core_numeric_id;

/// Matches the external VMM detail window radius on each side of the selected turn.
/// 对齐外置 VMM 在选中回合两侧的详情窗口半径。
const TURN_DETAIL_RADIUS: usize = 3;

/// Resolves requested turns in caller order and attaches numeric-ID-ordered adjacent turns.
/// 按调用方顺序解析请求回合，并附加按数字 ID 排序的相邻回合。
/// `active_turns` must come from one snapshot, excluding recycle batches; access is checked here.
/// `active_turns` 必须来自单份不含回收批次的快照；本函数校验访问边界。
/// Returns no entry for missing or unauthorized anchors, matching the existing native endpoint.
/// 缺失或未授权的锚点不返回条目，与现有原生入口一致。
pub(super) fn resolve_turn_details(
    active_turns: &[Turn],
    subject_id: &SubjectId,
    access: &ScopeAccess,
    requested_ids: &[TurnId],
) -> Result<Vec<Turn>, VmmClientError> {
    access
        .validate()
        .map_err(|message| VmmClientError::new("invalid_scope_access", message))?;
    // requested identifies anchors without changing the caller's eventual output order.
    // requested 标识锚点，但不改变最终输出的调用方顺序。
    let requested = requested_ids.iter().collect::<BTreeSet<_>>();
    // sessions restricts the numeric-ID validation and window scan to authorized anchor sessions.
    // sessions 将数字 ID 校验及窗口扫描限定在已授权锚点所在会话。
    let sessions = active_turns
        .iter()
        .filter(|turn| {
            &turn.subject_id == subject_id
                && access.visible_scopes.contains(&turn.scope_id)
                && requested.contains(&turn.id)
        })
        .map(|turn| (turn.scope_id.clone(), turn.session_source_id.clone()))
        .collect::<BTreeSet<_>>();
    // grouped keeps same-name sessions in different projects separate and scans the snapshot once.
    // grouped 隔离不同项目的同名会话，并只扫描一次快照。
    let mut grouped = BTreeMap::<(ScopeId, SessionSourceId), Vec<(u64, &Turn)>>::new();
    for turn in active_turns {
        // key carries the exact project and session owner; subject isolation is checked separately.
        // key 携带准确项目及会话归属；主体隔离单独检查。
        let key = (turn.scope_id.clone(), turn.session_source_id.clone());
        if &turn.subject_id == subject_id && sessions.contains(&key) {
            grouped
                .entry(key)
                .or_default()
                .push((parse_core_numeric_id(&turn.id, "vmm-turn-")?, turn));
        }
    }
    // resolved stores each anchor once so duplicate batch requests can retain existing semantics.
    // resolved 仅计算每个锚点一次，使重复批量请求仍保持既有语义。
    let mut resolved = BTreeMap::new();
    for turns in grouped.values_mut() {
        turns.sort_by_key(|(id, _)| *id);
        for (index, (_, turn)) in turns.iter().enumerate() {
            if !requested.contains(&turn.id) {
                continue;
            }
            // detail is projected from the same snapshot as its neighbors, avoiding torn windows.
            // detail 与相邻回合投影自同一快照，避免窗口撕裂。
            let mut detail = (*turn).clone();
            detail.previous_turn_ids = turns[index.saturating_sub(TURN_DETAIL_RADIUS)..index]
                .iter()
                .map(|(_, adjacent)| adjacent.id.clone())
                .collect();
            detail.next_turn_ids = turns
                .iter()
                .skip(index + 1)
                .take(TURN_DETAIL_RADIUS)
                .map(|(_, adjacent)| adjacent.id.clone())
                .collect();
            resolved.insert(detail.id.clone(), detail);
        }
    }
    Ok(requested_ids
        .iter()
        .filter_map(|id| resolved.get(id).cloned())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_memorymesh_core::TurnStatus;

    /// Creates a minimal turn with intentionally identical timestamps and stale stored neighbors.
    /// 创建时间戳有意相同且带陈旧相邻指针的最小回合。
    fn turn(id: u64, subject: &str, project: &str, session: &str) -> Turn {
        Turn {
            id: TurnId::new(format!("vmm-turn-{id}")).expect("turn id"),
            subject_id: SubjectId::new(subject).expect("subject"),
            scope_id: ScopeId::new(project).expect("project"),
            session_source_id: SessionSourceId::new(session).expect("session"),
            timeline: Vec::new(),
            previous_turn_ids: vec![TurnId::new("vmm-turn-999").expect("stale id")],
            next_turn_ids: vec![TurnId::new("vmm-turn-998").expect("stale id")],
            created_at_ns: 1,
            status: TurnStatus::Pending,
            user_input_kind: None,
            analysis_details: None,
        }
    }

    /// Converts expected numeric IDs into the canonical native identity shape.
    /// 将预期数字 ID 转换为规范原生身份形状。
    fn ids(values: &[u64]) -> Vec<TurnId> {
        values
            .iter()
            .map(|id| TurnId::new(format!("vmm-turn-{id}")).expect("turn id"))
            .collect()
    }

    /// Queries fixture turns with project one as owner and explicit linked project two visibility.
    /// 查询以项目一为归属并显式连带可见项目二的样本回合。
    fn query(turns: &[Turn], requested: &[u64]) -> Result<Vec<Turn>, VmmClientError> {
        resolve_turn_details(
            turns,
            &SubjectId::new("user-1").expect("subject"),
            &ScopeAccess {
                write_scope: ScopeId::new("project-1").expect("owner"),
                visible_scopes: vec![
                    ScopeId::new("project-1").expect("owner"),
                    ScopeId::new("project-2").expect("linked"),
                ],
            },
            &ids(requested),
        )
    }

    /// Verifies first/middle/last windows, numeric rather than lexical ordering, and batch ordering.
    /// 验证首中尾窗口、数字而非字典序排序，以及批量请求顺序。
    #[test]
    fn details_window_matches_external_numeric_radius() {
        // turns deliberately scrambles seven records containing numeric IDs two and ten.
        // turns 有意打乱含数字 ID 二和十的七条记录。
        let turns = [10, 2, 13, 4, 11, 3, 12]
            .into_iter()
            .map(|id| turn(id, "user-1", "project-1", "session"))
            .collect::<Vec<_>>();
        // result includes a duplicate anchor and a missing ID to verify the public batch shape.
        // result 包含重复锚点及缺失 ID，以验证公开批量形状。
        let result = query(&turns, &[13, 10, 2, 10, 900]).expect("details");
        assert_eq!(
            result
                .iter()
                .map(|turn| turn.id.clone())
                .collect::<Vec<_>>(),
            ids(&[13, 10, 2, 10])
        );
        assert_eq!(result[0].previous_turn_ids, ids(&[10, 11, 12]));
        assert!(result[0].next_turn_ids.is_empty());
        assert_eq!(result[1].previous_turn_ids, ids(&[2, 3, 4]));
        assert_eq!(result[1].next_turn_ids, ids(&[11, 12, 13]));
        assert!(result[2].previous_turn_ids.is_empty());
        assert_eq!(result[2].next_turn_ids, ids(&[3, 4, 10]));
        assert_eq!(result[1], result[3]);
    }

    /// Verifies project, subject and session isolation even with linked-project read permission.
    /// 验证即使拥有连带项目读取权限，项目、主体及会话仍隔离。
    #[test]
    fn details_window_never_crosses_owner_boundaries() {
        // turns includes each forbidden boundary and two separately authorized anchor groups.
        // turns 包含每种禁止跨越的边界，以及两组分别已授权的锚点。
        let turns = vec![
            turn(1, "user-1", "project-1", "session"),
            turn(2, "user-2", "project-1", "session"),
            turn(3, "user-1", "project-2", "session"),
            turn(4, "user-1", "project-1", "other-session"),
            turn(5, "user-1", "project-3", "session"),
            turn(6, "user-1", "project-1", "session"),
            turn(7, "user-1", "project-2", "session"),
        ];
        // result excludes unauthorized requested anchors while retaining independent linked windows.
        // result 排除未授权请求锚点，同时保留独立的连带项目窗口。
        let result = query(&turns, &[1, 2, 3, 5]).expect("details");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].next_turn_ids, ids(&[6]));
        assert_eq!(result[1].next_turn_ids, ids(&[7]));
        assert!(result.iter().all(|turn| turn.previous_turn_ids.is_empty()));
    }

    /// Verifies removing an active turn closes the window gap and moving a session preserves IDs.
    /// 验证移出活跃回合后窗口补齐缺口，迁移会话后仍保留 ID。
    #[test]
    fn details_window_uses_current_membership_after_removal_and_move() {
        // turns omits ID four, representing a turn already moved out of the active dataset.
        // turns 不含 ID 四，表示该回合已移出活跃数据集。
        let mut turns = [1, 2, 3, 5, 6, 7, 8]
            .into_iter()
            .map(|id| turn(id, "user-1", "project-1", "session"))
            .collect::<Vec<_>>();
        assert_eq!(
            query(&turns, &[5]).expect("before move")[0].previous_turn_ids,
            ids(&[1, 2, 3])
        );
        for turn in &mut turns {
            turn.scope_id = ScopeId::new("project-2").expect("destination");
            turn.session_source_id = SessionSourceId::new("session-1").expect("renamed session");
        }
        // moved verifies the current owner, not cached original ownership, determines adjacency.
        // moved 验证相邻关系由当前归属决定，而非缓存的原始归属。
        let moved = query(&turns, &[5]).expect("after move");
        assert_eq!(moved[0].previous_turn_ids, ids(&[1, 2, 3]));
        assert_eq!(moved[0].next_turn_ids, ids(&[6, 7, 8]));
    }

    /// Rejects malformed native identities only when they belong to an authorized anchor session.
    /// 仅当损坏原生身份属于已授权锚点会话时才拒绝查询。
    #[test]
    fn details_window_does_not_parse_unrelated_private_ids() {
        // malformed represents a record outside the requested session, not an alternate ID format.
        // malformed 表示请求会话之外的记录，而非另一种允许的 ID 格式。
        let mut malformed = turn(2, "user-2", "project-1", "session");
        malformed.id = TurnId::new("invalid-native-id").expect("opaque core id");
        assert_eq!(
            query(
                &[turn(1, "user-1", "project-1", "session"), malformed.clone()],
                &[1]
            )
            .expect("isolated query")
            .len(),
            1
        );
        malformed.subject_id = SubjectId::new("user-1").expect("subject");
        assert!(
            query(
                &[turn(1, "user-1", "project-1", "session"), malformed],
                &[1]
            )
            .is_err()
        );
    }
}
