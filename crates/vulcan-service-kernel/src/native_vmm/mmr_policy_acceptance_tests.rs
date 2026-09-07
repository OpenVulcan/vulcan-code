//! Actual MMR policy consumption on the shared normally written five-document search fixture.
//! 在共享正常写入五文档搜索夹具上验证实际多样性策略消费。

use super::*;

/// Verifies diversity after runtime hydration from relational and physical vector storage.
/// 验证从关系与物理向量存储重建运行时后的多样性行为。
/// Takes the rebuilt service/client, project, original saved group and stable IDs; restores that group afterward.
/// 接收重建服务／客户端、项目、原保存组及稳定 ID；随后恢复该保存组。
pub(super) async fn verify_reloaded(
    service: &crate::state::SharedServiceBridgeState,
    client: &NativeVmmClient,
    project_id: u64,
    original: &VmmNativeRetrievalPolicySummary,
    vector: &[u64],
) {
    // These searches use restored physical vectors, not vectors retained by the original writer runtime.
    // 这些搜索使用恢复的物理向量，而不是原写入运行时保留的向量。
    let mut enabled = original.clone();
    enabled.mmr_enabled = true;
    enabled.mmr_lambda = 0.0;
    save_global(service, &enabled).await;
    check(
        client,
        project_id,
        "hybrid-host-b",
        &[vector[0], vector[4], vector[3], vector[2], vector[1]],
        "vector_search_mmr",
    )
    .await;
    check(
        client,
        project_id,
        "hybrid-host-b",
        &[vector[0], vector[3]],
        "vector_search_mmr",
    )
    .await;
    save_global(service, original).await;
    check(client, project_id, "hybrid-host-b", vector, "vector_search").await;
}

/// Verifies enablement, endpoint/fractional weights, candidate-pool expansion and caller snapshot isolation.
/// 验证启用、端点／小数权重、候选池扩大及调用方快照隔离。
/// Takes the real service/client, explicit query pause owner, project, mutable saved policy and observed vector IDs.
/// 接收真实服务／客户端、明确查询暂停持有方、项目、可变保存策略及已观测向量 ID。
/// Returns after all public searches; the parent additionally verifies unchanged storage and disk reload.
/// 所有公开搜索完成后返回；父级额外验证存储不变及磁盘重载。
pub(super) async fn verify(
    service: &crate::state::SharedServiceBridgeState,
    client: &NativeVmmClient,
    port: &CapturingAnalysisPort,
    project_id: u64,
    global: &mut VmmNativeRetrievalPolicySummary,
    vector: &[u64],
) {
    assert_eq!(vector.len(), 5);
    assert!(!global.hybrid_enabled);
    assert_eq!(global.rerank_top_n, 0);
    assert!(!global.weibull_enabled);
    // Every document has a unique orthogonal residual; after the strongest first hit, redundancy follows cosine.
    // 每条文档有独有正交残差；首个最强命中选定后，冗余度随余弦变化。
    let diverse = [vector[0], vector[4], vector[3], vector[2], vector[1]];
    global.mmr_enabled = false;
    global.mmr_lambda = 0.0;
    save_global(service, global).await;
    check(client, project_id, "hybrid-host-b", vector, "vector_search").await;
    check(
        client,
        project_id,
        "hybrid-host-b",
        &vector[..2],
        "vector_search",
    )
    .await;

    global.mmr_enabled = true;
    for weight in [1.0, 0.75] {
        global.mmr_lambda = weight;
        save_global(service, global).await;
        check(
            client,
            project_id,
            "hybrid-host-b",
            vector,
            "vector_search_mmr",
        )
        .await;
    }
    for weight in [0.0, 0.25] {
        global.mmr_lambda = weight;
        save_global(service, global).await;
        check(
            client,
            project_id,
            "hybrid-host-b",
            &diverse,
            "vector_search_mmr",
        )
        .await;
    }
    global.mmr_lambda = 0.0;
    save_global(service, global).await;
    // Final limit two admits four vectors, excluding the fifth vector even though it would be most diverse.
    // 最终数量二接纳四条向量，即使第五条最多样也不能进入候选池。
    check(
        client,
        project_id,
        "hybrid-host-b",
        &[vector[0], vector[3]],
        "vector_search_mmr",
    )
    .await;
    // Final limit three admits all five; the previously excluded fifth document becomes the second result.
    // 最终数量三接纳全部五条；此前排除的第五文档成为第二结果。
    check(
        client,
        project_id,
        "hybrid-host-b",
        &diverse[..3],
        "vector_search_mmr",
    )
    .await;

    // Host A freezes only its explicit group while host B continues reading the current global group.
    // 宿主 A 仅固定其显式组，宿主 B 继续读取当前全局组。
    let mut override_a = global.clone();
    override_a.mmr_lambda = 1.0;
    save_a(service, Some(override_a.clone()), &override_a).await;
    global.mmr_lambda = 0.25;
    save_global(service, global).await;
    check(
        client,
        project_id,
        "hybrid-host-a",
        vector,
        "vector_search_mmr",
    )
    .await;
    check(
        client,
        project_id,
        "hybrid-host-b",
        &diverse,
        "vector_search_mmr",
    )
    .await;
    save_a(service, None, global).await;
    check(
        client,
        project_id,
        "hybrid-host-a",
        &diverse,
        "vector_search_mmr",
    )
    .await;

    // Explicit caller disablement overrides a globally active diversity stage, including public provenance.
    // 显式调用方关闭覆盖全局启用的多样性阶段，包括公开来源标记。
    override_a.mmr_enabled = false;
    save_a(service, Some(override_a.clone()), &override_a).await;
    check(client, project_id, "hybrid-host-a", vector, "vector_search").await;
    check(
        client,
        project_id,
        "hybrid-host-b",
        &diverse,
        "vector_search_mmr",
    )
    .await;
    save_a(service, None, global).await;
    check(
        client,
        project_id,
        "hybrid-host-a",
        &diverse,
        "vector_search_mmr",
    )
    .await;

    // Pause before physical channel reads so changing both controls cannot affect any later stage of this call.
    // 在物理通道读取前暂停，使两个控制项的修改都不能影响本次调用的任何后续阶段。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // Only the formal-save branch resumes this admitted query.
    // 仅正式保存分支恢复此已准入查询。
    let (resume, resumed) = tokio::sync::oneshot::channel();
    // The parent deliberately uses this closed fixture mode for the entire shared dataset.
    // 父级为整个共享数据集明确使用此封闭夹具模式。
    let AnalysisFixture::HybridPolicy(slot) = &port.fixture else {
        panic!("MMR query fixture required");
    };
    *slot.lock().expect("arm MMR query") = Some(precheck_policy::IntentPause {
        entered,
        resume: resumed,
    });
    // Save disabled relevance-only settings while the active command still owns its diversity-enabled snapshot.
    // 在活动命令仍持有多样性启用快照时，保存关闭且仅相关性的设置。
    let mut next = global.clone();
    next.mmr_enabled = false;
    next.mmr_lambda = 1.0;
    tokio::join!(
        check(
            client,
            project_id,
            "hybrid-host-b",
            &diverse,
            "vector_search_mmr"
        ),
        async {
            tokio::time::timeout(Duration::from_secs(10), observed)
                .await
                .expect("bounded MMR admission")
                .expect("MMR query reached");
            save_global(service, &next).await;
            resume.send(()).expect("resume MMR query");
        }
    );
    *global = next;
    check(client, project_id, "hybrid-host-b", vector, "vector_search").await;
}
