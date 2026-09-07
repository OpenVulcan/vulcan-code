//! Verifies dynamic detail windows through actual source recycling and restoration.
//! 通过实际来源回收及恢复验证动态详情窗口。

use super::*;

/// Recycles three middle turns and restores their exact identities without changing pending work.
/// 回收三个中间回合并恢复其准确身份，不改变待处理作业。
/// `client`, `runtime`, `data_plane` and `maintenance` share the real isolated fixture owners.
/// `client`、`runtime`、`data_plane` 与 `maintenance` 共享真实隔离夹具的归属方。
/// `request` retains duplicate and cross-session anchors; `ids` contains the twelve ordered source turns.
/// `request` 保留重复及跨会话锚点；`ids` 包含按序排列的十二个来源回合。
/// Returns after checking read-only queries, compacted neighbor windows and complete durable restoration.
/// 检查只读查询、重排后的邻接窗口及完整持久恢复后返回。
pub(super) async fn verify(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    data_plane: &VmmDataPlaneHandle,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    request: &GetTurnDetailsRequest,
    ids: &[u64],
) {
    assert_eq!(ids.len(), 12);
    // The baseline retains every non-selected record and all unrelated projects in the same Core.
    // 基线保留同一 Core 中全部未选中记录及所有无关项目。
    let before = runtime.storage_snapshot().expect("before window recycle");
    // Queue rows must survive recycling so restoration can resume the original accepted work.
    // 队列行必须跨回收保留，使恢复后能够继续原始已接受作业。
    let pending_before = data_plane
        .list_active_pending_analysis_jobs()
        .await
        .expect("window pending baseline");
    // Three separated middle turns exercise both missing anchors and backfilled neighbor windows.
    // 三个分隔的中间回合同时覆盖锚点缺失与邻接窗口补足。
    let selected_ids = [ids[3], ids[5], ids[7]];
    // Reuse the public client's exact canonical identity conversion for the management selection.
    // 管理选择复用公开客户端的准确规范身份转换。
    let selected_sources = selected_ids
        .iter()
        .map(|id| {
            crate::native_vmm::client::turn_id(*id)
                .expect("selected turn")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pending_before
            .iter()
            .filter(|job| selected_sources
                .iter()
                .any(|id| job.turn_id.as_ref() == Some(id)))
            .count(),
        3,
    );
    // Keep the original batch order and inject recycled anchors at both ends of the request.
    // 保留原批次顺序，并在请求两端加入将回收的锚点。
    let mut mixed_request = request.clone();
    mixed_request.turn_ids.insert(0, ids[5]);
    mixed_request.turn_ids.extend([ids[7], ids[3]]);
    // Capture actual original dialogue and detail fields, not a synthetic partial response.
    // 捕获实际原始问答与详情字段，而非合成的局部响应。
    let original = client
        .get_turn_details(request.clone())
        .await
        .expect("original window details");
    assert_eq!(original.turns.len(), 5);
    // The extended response later proves that all three recycled anchors return with their original content.
    // 扩展响应随后证明三个已回收锚点均以原始内容恢复。
    let original_mixed = client
        .get_turn_details(mixed_request.clone())
        .await
        .expect("original mixed details");
    assert_eq!(original_mixed.turns.len(), 8);
    // Formal preview owns the immutable selection and version; the test never edits storage directly.
    // 正式预览持有不可变选择及版本；测试绝不直接修改存储。
    let preview = maintenance
        .preview_manual_removal(vulcan_service_api::VmmNativeRemovalQuery {
            vmm_project_id: request.project_id,
            selection: vulcan_service_api::VmmNativeRemovalTarget::Turns {
                ids: selected_sources.clone(),
            },
        })
        .await
        .expect("window recycle preview");
    assert_eq!(preview.impact.turn_ids, selected_sources);
    assert_eq!(preview.impact.pending_turn_ids, selected_sources);
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("read-only window preview"),
        before
    );
    // Execute the same confirmation that was inspected, without a second mutable target list.
    // 执行已检查的同一确认，不传入第二份可变目标列表。
    let recycled = maintenance
        .execute_manual_removal("window-recycle-middle-turns", &preview.preview_id)
        .await
        .expect("recycle middle turns");
    assert_eq!(recycled.impact, preview.impact);
    assert_eq!(recycled.storage_version, before.storage_version + 1);
    // These expected neighbors are explicit fixture identities, independent of the production window algorithm.
    // 预期邻居为明确夹具身份，不依赖生产窗口算法。
    let mut expected = original.turns.clone();
    expected[2].next_turn_ids = vec![ids[1], ids[2], ids[4]];
    expected[3].previous_turn_ids = vec![ids[1], ids[2], ids[4]];
    expected[3].next_turn_ids = vec![ids[8], ids[9], ids[10]];
    expected[4] = expected[3].clone();
    // Management reads omit all selected anchors while preserving duplicate requests and unrelated session isolation.
    // 管理读取省略全部所选锚点，同时保留重复请求及无关会话隔离。
    let remaining = client
        .get_turn_details(mixed_request.clone())
        .await
        .expect("recycled window details");
    assert_eq!(remaining.turns, expected);
    // An archived but still admitted runtime session must see the same active-only neighbor membership.
    // 已归档但仍允许准入的运行时会话必须看到相同的仅活动邻接集合。
    let runtime_remaining = client
        .get_turn_details(GetTurnDetailsRequest {
            runtime_session_id: Some("window-session".to_owned()),
            ..mixed_request.clone()
        })
        .await
        .expect("runtime recycled window details");
    assert_eq!(runtime_remaining.turns, expected);
    assert_eq!(
        data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("recycled window queue"),
        pending_before,
    );
    maintenance
        .restore_recycle_batch(
            "window-restore-middle-turns",
            &recycled.batch_id,
            recycled.storage_version,
        )
        .await
        .expect("restore middle turns");
    // Only the two atomic maintenance commits may change the complete authoritative snapshot.
    // 完整权威快照只允许发生两次原子维护提交的版本变化。
    let mut restored_expected = before;
    restored_expected.storage_version += 2;
    assert_eq!(
        runtime.storage_snapshot().expect("restored window Core"),
        restored_expected
    );
    // Verify real persisted content independently from the process-local authoritative owner.
    // 独立于进程内权威归属方验证真实持久内容。
    let mut durable: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("durable window snapshot")
            .expect("persisted window document"),
    )
    .expect("decode durable window snapshot");
    // Restore appends durable rows; Core exports them by its TurnId BTreeMap order, not insertion position.
    // 恢复追加持久行；Core 按 TurnId 有序映射顺序导出，而非按插入位置导出。
    // Normalize only this physical array order; retain every identity, duplicate and complete field for comparison.
    // 仅归一化此物理数组顺序；保留全部身份、重复项及完整字段参与比较。
    durable
        .dataset
        .turns
        .sort_by(|left, right| left.id.cmp(&right.id));
    assert_eq!(durable, restored_expected.without_embeddings());
    assert_eq!(
        data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("restored window queue"),
        pending_before,
    );
    assert_eq!(
        client
            .get_turn_details(mixed_request)
            .await
            .expect("restored mixed windows")
            .turns,
        original_mixed.turns,
    );
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("read-only restored windows"),
        restored_expected
    );
}
