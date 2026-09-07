//! Real full-text authorization and session-boundary acceptance across both production read modes.
//! 两种生产读取模式下真实全文授权与会话边界验收。

use super::*;
use vulcan_memorymesh_core::{
    CandidateObservationQuery, ChatCompactBoundary, MemoryCandidateScope, SessionRecallBoundary,
    SessionRecallMode, SessionSourceId, Turn, TurnId, TurnStatus,
};

/// Runs both real read variants and compares the exact admitted identity set.
/// 运行两种真实读取变体并比较准确接纳身份集合。
/// `boundary` supplies candidate/session restrictions; `expected` lists stable suffixes, or rejects invalid grants.
/// `boundary` 提供候选/会话约束；`expected` 列出稳定标识尾号，或要求拒绝非法授权。
async fn assert_boundary(
    runtime: &NativeVmmCoreRuntime,
    boundary: CandidateQueryBoundary,
    expected: Option<&[usize]>,
) {
    // Both queries share authority and time, so only the transport variant differs.
    // 两个查询共享授权和时间，只有传输变体不同。
    let access = ScopeAccess {
        write_scope: ScopeId::new("selected").expect("write scope"),
        visible_scopes: vec![
            ScopeId::new("selected").expect("own scope"),
            ScopeId::new("linked").expect("linked scope"),
        ],
    };
    let observation = CandidateObservationQuery {
        subject_id: SubjectId::new("subject").expect("subject"),
        scope_access: access.clone(),
        query: "西瓜".to_owned(),
        query_embedding: vec![1.0, 0.0],
        vector_space_id: VectorSpaceId::new("test-generation").expect("generation"),
        boundary: boundary.clone(),
        limit: 16,
        vector_contract: vulcan_memorymesh_core::RetrievalPolicy::v1()
            .vector_contract
            .contract,
        lexical_contract: Some(lexical::score_contract().contract),
        now_ns: 10,
    };
    for mut query in [
        StorageQuery::LexicalCandidates {
            subject_id: observation.subject_id.clone(),
            scope_access: access.clone(),
            queries: vec!["西瓜".to_owned(), "西瓜".to_owned()],
            vector_space_id: observation.vector_space_id.clone(),
            boundary: boundary.clone(),
            limit: 16,
            expected_contract: lexical::score_contract().contract,
            now_ns: 10,
        },
        StorageQuery::CandidateObservation(observation),
    ] {
        let result = runtime.host.execute_storage_query(&query).await;
        let Some(expected) = expected else {
            assert!(result.is_err(), "unauthorized candidate scope must fail");
            // Empty work must still validate the same grant instead of bypassing authority checks.
            // 空工作仍须校验相同授权，不能绕过权限检查。
            match &mut query {
                StorageQuery::LexicalCandidates { queries, limit, .. } => {
                    queries.clear();
                    *limit = 0;
                }
                StorageQuery::CandidateObservation(observation) => {
                    observation.query.clear();
                    observation.limit = 0;
                }
                _ => unreachable!("the fixture constructs only lexical-bearing queries"),
            }
            assert!(runtime.host.execute_storage_query(&query).await.is_err());
            continue;
        };
        let channel = match result.expect("production boundary query") {
            HostEffectResult::StorageQuery(StorageQueryResult::LexicalCandidates(channel)) => {
                channel
            }
            HostEffectResult::StorageQuery(StorageQueryResult::CandidateObservation(result)) => {
                assert!(result.vector_channel.candidates.is_empty());
                assert_eq!(result.memories.len(), expected.len());
                CandidateChannelResult {
                    candidates: result
                        .lexical_channel
                        .expect("enabled lexical channel")
                        .candidates,
                    memories: result.memories,
                }
            }
            _ => panic!("lexical-bearing result expected"),
        };
        // Sorting ignores valid physical tie ordering, but preserves duplicate and unauthorized hit detection.
        // 排序忽略合法物理同分顺序，但保留重复及越权命中检测。
        let mut actual = channel
            .candidates
            .iter()
            .map(|hit| hit.memory_id.to_string())
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(
            actual,
            expected
                .iter()
                .map(|index| format!("vmm-memory-{index:04}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(channel.memories.len(), expected.len());
    }
}

/// Verifies stable-ID reads after actual publication and physical rebuild without broadening ownership.
/// 验证实际发布及物理重建后的稳定身份读取，且不扩大归属权限。
/// `runtime` owns the isolated controller; `before` is restored after all boundary assertions.
/// `runtime` 持有隔离控制器；全部边界断言后恢复 `before`。
pub(super) async fn verify_query_boundaries(
    runtime: &NativeVmmCoreRuntime,
    before: &StorageSnapshot,
) {
    let mut fixture = before.clone();
    fixture.dataset.memories = (0..9)
        .map(|index| indexed_memory(index, "selected"))
        .collect();
    let current_session = SessionSourceId::new("boundary-session").expect("current session");
    fixture.dataset.memories[0].origin_session_source_id = Some(current_session.clone());
    for index in [1, 2] {
        fixture.dataset.memories[index].scope_id = ScopeId::new("linked").expect("linked");
        fixture.dataset.memories[index].source_subject_id =
            Some(SubjectId::new("other-subject").expect("other subject"));
    }
    fixture.dataset.memories[1].access_class = MemoryAccessClass::ScopeShared;
    fixture.dataset.memories[1].origin_session_source_id =
        Some(SessionSourceId::new("other-session").expect("other session"));
    fixture.dataset.memories[3].scope_id = ScopeId::new("not-granted").expect("hidden scope");
    fixture.dataset.memories[3].access_class = MemoryAccessClass::ScopeShared;
    fixture.dataset.memories[4].expires_at_ns = Some(10);
    fixture.dataset.memories[5].vector_space_id =
        Some(VectorSpaceId::new("other-generation").expect("other generation"));
    fixture.dataset.memories[6].state = MemoryState::Deleted;
    for index in [7, 8] {
        let turn_id = TurnId::new(format!("boundary-turn-{index}")).expect("turn");
        fixture.dataset.memories[index].source_turn_id = Some(turn_id.clone());
        fixture.dataset.memories[index].origin_session_source_id = Some(current_session.clone());
        fixture.dataset.turns.push(Turn {
            id: turn_id,
            subject_id: SubjectId::new("subject").expect("subject"),
            scope_id: ScopeId::new("selected").expect("scope"),
            session_source_id: current_session.clone(),
            timeline: Vec::new(),
            previous_turn_ids: Vec::new(),
            next_turn_ids: Vec::new(),
            created_at_ns: index as i64,
            status: TurnStatus::ProcessedUnknown,
            user_input_kind: None,
            analysis_details: None,
        });
    }
    runtime
        .host
        .persist_prepared_storage_transition("fts-boundary-seed", "test", before, &fixture, &())
        .await
        .expect("publish boundary fixture");
    let mut boundary = CandidateQueryBoundary {
        candidate_scope: None,
        session_recall_boundary: None,
    };
    assert_boundary(runtime, boundary.clone(), Some(&[0, 1, 7, 8])).await;
    boundary.candidate_scope = Some(MemoryCandidateScope {
        scope_ids: vec![ScopeId::new("linked").expect("linked")],
        origin_session_source_id: None,
    });
    assert_boundary(runtime, boundary.clone(), Some(&[1])).await;
    boundary.candidate_scope = Some(MemoryCandidateScope {
        scope_ids: vec![
            ScopeId::new("selected").expect("own"),
            ScopeId::new("linked").expect("linked"),
        ],
        origin_session_source_id: Some(current_session.clone()),
    });
    assert_boundary(runtime, boundary.clone(), Some(&[0, 7, 8])).await;
    boundary.candidate_scope = Some(MemoryCandidateScope {
        scope_ids: vec![ScopeId::new("not-granted").expect("hidden")],
        origin_session_source_id: None,
    });
    assert_boundary(runtime, boundary.clone(), None).await;
    boundary.candidate_scope = None;
    boundary.session_recall_boundary = Some(SessionRecallBoundary {
        session_source_id: current_session.clone(),
        mode: SessionRecallMode::CompactAware,
    });
    assert_boundary(runtime, boundary.clone(), Some(&[0, 1])).await;
    let mut compacted = fixture.clone();
    compacted
        .dataset
        .chat_compact_boundaries
        .push(ChatCompactBoundary {
            subject_id: SubjectId::new("subject").expect("subject"),
            scope_id: ScopeId::new("selected").expect("scope"),
            session_source_id: current_session.clone(),
            compacted_turn_id: TurnId::new("boundary-turn-7").expect("boundary turn"),
            compacted_at_ns: 9,
        });
    runtime
        .host
        .persist_prepared_storage_transition(
            "fts-boundary-compact",
            "test",
            &fixture,
            &compacted,
            &(),
        )
        .await
        .expect("publish compact boundary");
    assert_boundary(runtime, boundary.clone(), Some(&[0, 1, 7])).await;
    // Physical reconstruction may reorder rows but must not erase compact or authorization semantics.
    // 物理重建可以调整行顺序，但不得抹掉压缩或授权语义。
    assert_eq!(
        runtime
            .host
            .data_plane
            .rebuild_lexical_index()
            .await
            .expect("rebuild"),
        8
    );
    assert_boundary(runtime, boundary.clone(), Some(&[0, 1, 7])).await;
    boundary.session_recall_boundary = Some(SessionRecallBoundary {
        session_source_id: current_session,
        mode: SessionRecallMode::Full,
    });
    assert_boundary(runtime, boundary, Some(&[0, 1, 7, 8])).await;
    runtime
        .host
        .persist_prepared_storage_transition(
            "fts-boundary-restore",
            "test",
            &compacted,
            before,
            &(),
        )
        .await
        .expect("restore fixture baseline");
}
