//! Validates detail windows through actual durable intake and the native client facade.
//! 通过实际持久入库及原生客户端门面验证详情窗口。

use super::*;

#[path = "turn_details_recycle_acceptance_tests.rs"]
mod recycle_windows;

/// Registers ordinary completed turns and checks authorized, ordered, read-only detail projection.
/// 登记普通已完成回合，检查已授权、顺序准确且只读的详情投影。
/// Takes isolated production facades; returns after checking pending turns and archived sessions.
/// 接收隔离生产门面；检查待提炼回合及归档会话后返回。
pub(super) async fn verify(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    data_plane: &VmmDataPlaneHandle,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
) {
    // Independent owners prevent earlier migration fixtures from determining access implicitly.
    // 独立归属防止此前迁移夹具隐式决定访问范围。
    let project = maintenance
        .create_and_bind_project("window-host", "窗口项目")
        .await
        .expect("window project");
    let foreign = maintenance
        .create_and_bind_project("window-foreign-host", "另一个窗口项目")
        .await
        .expect("foreign window project");
    for index in 0..12 {
        intake(client, "window-host", project.id, "window-session", index).await;
        if index == 5 {
            // Interleaved IDs must never become neighbors across sessions or projects.
            // 交错 ID 不能成为跨会话或跨项目的相邻回合。
            intake(client, "window-host", project.id, "other-session", index).await;
            intake(
                client,
                "window-foreign-host",
                foreign.id,
                "window-session",
                index,
            )
            .await;
        }
    }
    // Read exact persisted identities; intake intentionally does not expose or predict numeric IDs.
    // 读取准确持久身份；入库刻意不暴露或预测数字 ID。
    let before = runtime.storage_snapshot().expect("window snapshot");
    let scope = super::super::project_scope_id(project.id).expect("window scope");
    let foreign_scope = super::super::project_scope_id(foreign.id).expect("foreign scope");
    let mut ids = before
        .dataset
        .turns
        .iter()
        .filter(|turn| {
            turn.scope_id == scope && turn.session_source_id.as_str() == "window-session"
        })
        .map(|turn| parse_core_numeric_id(&turn.id, "vmm-turn-").expect("numeric turn"))
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(ids.len(), 12);
    let other_id = before
        .dataset
        .turns
        .iter()
        .find(|turn| turn.scope_id == scope && turn.session_source_id.as_str() == "other-session")
        .map(|turn| parse_core_numeric_id(&turn.id, "vmm-turn-").expect("other numeric turn"))
        .expect("other session turn");
    let foreign_id = before
        .dataset
        .turns
        .iter()
        .find(|turn| turn.scope_id == foreign_scope)
        .map(|turn| parse_core_numeric_id(&turn.id, "vmm-turn-").expect("foreign numeric turn"))
        .expect("foreign turn");
    // Management and an admitted runtime session must produce identical authorized windows.
    // 管理查看与已准入运行时会话必须产生相同的已授权窗口。
    let request = GetTurnDetailsRequest {
        user_id: 1,
        project_id: project.id,
        host_project_id: "window-host".to_owned(),
        turn_ids: vec![ids[11], other_id, ids[0], ids[6], ids[6], foreign_id],
        ..Default::default()
    };
    let details = client
        .get_turn_details(request.clone())
        .await
        .expect("management details");
    assert_eq!(
        details
            .turns
            .iter()
            .map(|turn| turn.turn_id)
            .collect::<Vec<_>>(),
        vec![ids[11], other_id, ids[0], ids[6], ids[6]],
    );
    assert_eq!(details.turns[0].previous_turn_ids, ids[8..11]);
    assert!(details.turns[0].next_turn_ids.is_empty());
    assert!(details.turns[1].previous_turn_ids.is_empty());
    assert!(details.turns[1].next_turn_ids.is_empty());
    assert!(details.turns[2].previous_turn_ids.is_empty());
    assert_eq!(details.turns[2].next_turn_ids, ids[1..4]);
    assert_eq!(details.turns[3].previous_turn_ids, ids[3..6]);
    assert_eq!(details.turns[3].next_turn_ids, ids[7..10]);
    assert_eq!(details.turns[3], details.turns[4]);
    assert_eq!(
        details.turns[3].user_question,
        "本项目第 6 项决定使用 PostgreSQL 保存业务数据。"
    );
    assert_eq!(
        details.turns[3].assistant_answer,
        "已记录第 6 项数据库决定。"
    );
    let runtime_details = client
        .get_turn_details(GetTurnDetailsRequest {
            runtime_session_id: Some("window-session".to_owned()),
            ..request.clone()
        })
        .await
        .expect("runtime details");
    assert_eq!(runtime_details.turns, details.turns);
    assert_eq!(runtime.storage_snapshot().expect("after reads"), before);
    // Archiving classifies a session, but must not erase detail windows or trigger analysis.
    // 归档仅分类会话，不能清除详情窗口或触发提炼。
    let preview = maintenance
        .preview_session_archive(vulcan_service_api::VmmNativeSessionArchiveSelection {
            vmm_project_id: project.id,
            session_source_ids: vec!["window-session".to_owned()],
            archived: true,
        })
        .await
        .expect("window archive preview");
    maintenance
        .execute_session_archive("window-archive", &preview.preview_id)
        .await
        .expect("window archive");
    let archived = client
        .get_turn_details(request.clone())
        .await
        .expect("archived details");
    assert_eq!(archived.turns, details.turns);
    // Expanding read access exposes the other anchor, not its neighbors in the same-named source session.
    // 扩展读取权限只暴露另一锚点，不把同名来源会话的回合拼入其窗口。
    maintenance
        .replace_query_links("window-host", &[foreign.id])
        .await
        .expect("link foreign project");
    let linked = client
        .get_turn_details(request.clone())
        .await
        .expect("linked details");
    assert_eq!(linked.turns.len(), 6);
    assert_eq!(linked.turns[..5], details.turns);
    assert_eq!(linked.turns[5].turn_id, foreign_id);
    assert!(linked.turns[5].previous_turn_ids.is_empty());
    assert!(linked.turns[5].next_turn_ids.is_empty());
    maintenance
        .replace_query_links("window-host", &[])
        .await
        .expect("remove read link");
    recycle_windows::verify(client, runtime, data_plane, maintenance, &request, &ids).await;
    assert!(
        maintenance
            .preview_migration(project.id, foreign.id)
            .await
            .is_err()
    );
    // Drain the actual per-session queue before moving content; never bypass the pending-work guard.
    // 移动内容前排空实际逐会话队列，绝不绕过待处理作业门禁。
    for _ in 0..12 {
        assert!(
            client
                .drain_due_pending_analysis_jobs()
                .await
                .expect("window analysis")
        );
    }
    // Successful draining must mean extracted records, not failed jobs merely reaching a terminal state.
    // 成功排空必须意味着记录已提炼，而非失败作业仅到达终态。
    for (project_id, count) in [(project.id, 13), (foreign.id, 1)] {
        let records = maintenance
            .memory_records(project_id, None)
            .await
            .expect("completed window records");
        assert_eq!(records.turns.len(), count);
        assert!(
            records
                .turns
                .iter()
                .all(|turn| turn.extracted == Some(true))
        );
    }
    let migration = maintenance
        .preview_migration(project.id, foreign.id)
        .await
        .expect("window migration preview");
    assert_eq!(migration.source.turns, 13);
    assert_eq!(migration.session_renames.len(), 1);
    maintenance
        .execute_migration(
            "window-migration",
            project.id,
            foreign.id,
            migration.storage_version,
            &migration.preview_fingerprint,
            true,
        )
        .await
        .expect("window migration");
    // Confirm the actual renamed owner instead of inferring it from an unchanged neighbor list.
    // 确认真正改名后的归属，而不是从不变的邻居列表推断。
    let moved_snapshot = runtime.storage_snapshot().expect("renamed snapshot");
    assert_eq!(
        moved_snapshot
            .dataset
            .turns
            .iter()
            .filter(|turn| {
                turn.scope_id == foreign_scope
                    && turn.session_source_id.as_str() == "window-session-1"
            })
            .count(),
        12,
    );
    assert!(
        client
            .get_turn_details(request.clone())
            .await
            .expect("empty source details")
            .turns
            .is_empty()
    );
    maintenance
        .set_project_access("window-host", foreign.id, &[])
        .await
        .expect("explicit migrated binding");
    let moved_request = GetTurnDetailsRequest {
        project_id: foreign.id,
        ..request
    };
    let moved = client
        .get_turn_details(moved_request.clone())
        .await
        .expect("moved windows");
    assert_eq!(moved.turns.len(), 6);
    for (actual, original) in moved.turns.iter().zip(&linked.turns) {
        // Analysis may populate detail text; original dialogue and neighbor identities must remain unchanged.
        // 提炼可能填充详情文本，但原始问答及相邻身份必须保持不变。
        assert_eq!(actual.turn_id, original.turn_id);
        assert_eq!(actual.previous_turn_ids, original.previous_turn_ids);
        assert_eq!(actual.next_turn_ids, original.next_turn_ids);
        assert_eq!(actual.user_question, original.user_question);
        assert_eq!(actual.assistant_answer, original.assistant_answer);
    }
    // A confirmed project clear removes all anchors, including the destination's original turn.
    // 确认项目清空后删除全部锚点，包括目标项目原有回合。
    let clear = maintenance
        .preview_clear(foreign.id)
        .await
        .expect("window clear preview");
    maintenance
        .execute_clear("window-clear", foreign.id, clear.storage_version)
        .await
        .expect("window clear");
    assert!(
        client
            .get_turn_details(moved_request)
            .await
            .expect("cleared details")
            .turns
            .is_empty()
    );
}

/// Persists one ordinary PostAction with an exact fixture-owned operation identity.
/// 使用夹具拥有的准确操作身份持久化一个普通 PostAction。
/// Takes owner/session/index; returns only after durable acceptance, without draining analysis.
/// 接收归属、会话及序号；持久接受后返回，不排空提炼作业。
async fn intake(client: &NativeVmmClient, host: &str, project: u64, session: &str, index: usize) {
    // Distinct operations prevent idempotent replay from masking missing neighboring turns.
    // 不同操作防止幂等重放掩盖相邻回合缺失。
    let response = client
        .post_action(PostActionRequest {
            operation_id: format!("{host}-{session}-{index}"),
            session_id: session.to_owned(),
            user_id: 1,
            project_id: project,
            host_project_id: host.to_owned(),
            user_content: format!("本项目第 {index} 项决定使用 PostgreSQL 保存业务数据。"),
            assistant_content: format!("已记录第 {index} 项数据库决定。"),
            ..Default::default()
        })
        .await
        .expect("window intake");
    assert!(response.accepted);
}
