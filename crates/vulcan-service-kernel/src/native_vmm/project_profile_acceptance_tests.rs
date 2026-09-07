//! Real queued extraction and manual profile policy ownership acceptance.
//! 真实排队提炼及手动画像策略归属验收。
use super::*;
use vulcan_service_api::VmmNativeProjectProfilePolicyEdit;

/// Reads the actual committed SQLite snapshot rather than a projected policy response.
/// 读取实际已提交 SQLite 快照，而非策略响应投影。
/// Returns durable records for the supplied isolated data plane.
/// 返回传入隔离数据面的持久记录。
async fn persisted(data_plane: &VmmDataPlaneHandle) -> vulcan_memorymesh_core::StorageSnapshot {
    serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("snapshot read")
            .expect("snapshot exists"),
    )
    .expect("snapshot schema")
}

/// Checks one exact content/owner pair and its finite lifetime measured from the authoritative effective date.
/// 检查准确内容/归属对及从权威生效日期计算的有限生命周期。
/// No display name, wall-clock estimate or record ordering participates in the assertion.
/// 断言不使用显示名称、墙上时钟估计或记录顺序。
fn assert_lifetime(
    snapshot: &vulcan_memorymesh_core::StorageSnapshot,
    content: &str,
    scope: &ScopeId,
    days: i64,
) {
    let nodes = snapshot
        .dataset
        .profile_nodes
        .iter()
        .filter(|node| node.content == content && node.scope_id == *scope)
        .collect::<Vec<_>>();
    assert_eq!(nodes.len(), 1, "exact profile content and owner");
    let node = nodes[0];
    assert_eq!(node.profile_level, ProfileLevel::Stable);
    assert_eq!(node.refresh_weight, 0);
    assert_eq!(
        node.expires_at_ns,
        Some(node.effective_date_ns.expect("effective date") + days * 86_400_000_000_000)
    );
}

/// Exercises stable owner overrides through production queue drain, rebinding and manual instructions.
/// 通过生产队列排空、重绑及手动指令验收稳定归属覆盖。
/// Only model responses are deterministic; actual native policy, Core, SQLite and vector storage execute unchanged.
/// 仅模型响应为确定性夹具；实际原生策略、Core、SQLite 及向量存储保持原实现。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // The inference-only fixture produces interleaved project and fixed-user profile candidates.
    // 仅替代推理的夹具产生交错的项目与固定用户画像候选。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::Nonempty,
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("capability");
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("runtime"),
    );
    let gate = Arc::new(RwLock::new(()));
    let client = NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate.clone(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    );
    let maintenance =
        crate::native_vmm::NativeVmmMaintenance::new(runtime.clone(), data_plane.clone(), gate);
    let mut settings = runtime
        .resource_catalog()
        .await
        .expect("project profile acceptance fixture value")
        .settings;
    settings.noise_semantic_enabled = false;
    runtime
        .update_resource_settings(settings)
        .await
        .expect("project profile acceptance fixture value");
    // Distinct finite rules make accidental global or sibling use observable in persisted expiry timestamps.
    // 不同有限规则使误用全局或同级规则可从持久过期时间中观察。
    let mut global = runtime
        .runtime_policy()
        .await
        .expect("project profile acceptance fixture value")
        .profiles;
    global.user.stable.base_ms = 17 * 86_400_000;
    global.user.stable.step_ms = 0;
    global.user.stable.max_ms = 17 * 86_400_000;
    global.project.stable.base_ms = 90 * 86_400_000;
    global.project.stable.step_ms = 0;
    global.project.stable.max_ms = 90 * 86_400_000;
    runtime
        .update_profile_policy(global.clone())
        .await
        .expect("global rules");
    let a = maintenance
        .create_and_bind_project("profile-owner-a", "Same name")
        .await
        .expect("project profile acceptance fixture value");
    let b = maintenance
        .create_and_bind_project("profile-owner-b", "Same name")
        .await
        .expect("project profile acceptance fixture value");
    for (project_id, days) in [(a.id, 31_u64), (b.id, 57_u64)] {
        let mut rules = global.project.clone();
        rules.stable.base_ms = days * 86_400_000;
        rules.stable.max_ms = days * 86_400_000;
        let saved = maintenance
            .update_project_profile_policy(VmmNativeProjectProfilePolicyEdit::Override {
                project_id,
                rules: rules.clone(),
            })
            .await
            .expect("save owner rules");
        assert_eq!(saved.effective, rules);
        assert_eq!(saved.project_id, project_id);
    }
    assert!(
        maintenance
            .project_profile_policy_summary(u64::MAX)
            .await
            .is_err()
    );
    assert!(
        maintenance
            .update_project_profile_policy(VmmNativeProjectProfilePolicyEdit::Inherit {
                project_id: u64::MAX
            })
            .await
            .is_err()
    );
    // Queue admission belongs to A despite a stale request ID, then host rebinding selects B for future input only.
    // 尽管请求 ID 过期，队列准入仍属于 A；随后宿主重绑仅为未来输入选择 B。
    client
        .post_action(PostActionRequest {
            host_project_id: "profile-owner-a".to_owned(),
            project_id: b.id,
            user_id: 1,
            session_id: "profile-policy-session".to_owned(),
            operation_id: "profile-policy-operation".to_owned(),
            user_content: "我喜欢西瓜和葡萄，这个项目确定使用 PostgreSQL，请记住。".to_owned(),
            assistant_content: "已了解水果偏好和数据库决定。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("intake a");
    maintenance
        .set_project_access("profile-owner-a", b.id, &[a.id])
        .await
        .expect("rebind with linked old owner");
    assert!(
        client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("drain accepted old owner")
    );
    let job = data_plane
        .load_pending_analysis_job("profile-policy-operation")
        .await
        .expect("project profile acceptance fixture value")
        .expect("project profile acceptance fixture value");
    assert_eq!(
        job.state,
        vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
        "{:?}",
        job.last_error
    );
    let first = persisted(&data_plane).await;
    assert_lifetime(
        &first,
        "项目使用 PostgreSQL 数据库。",
        &project_scope_id(a.id).expect("project profile acceptance fixture value"),
        31,
    );
    assert_lifetime(
        &first,
        "用户喜欢西瓜。",
        &subject_scope_id(1).expect("project profile acceptance fixture value"),
        17,
    );
    assert_lifetime(
        &first,
        "用户喜欢葡萄。",
        &subject_scope_id(1).expect("project profile acceptance fixture value"),
        17,
    );
    // A manual project instruction uses the new bound owner, while a personal instruction stays global.
    // 手动项目指令使用新的绑定归属，个人指令仍使用全局规则。
    for (target, content) in [
        (
            ProfileTarget::ProfileTargetProject,
            "Project B requires transactional writes.",
        ),
        (
            ProfileTarget::ProfileTargetUser,
            "The user prefers concise explanations.",
        ),
    ] {
        let result = client
            .apply_profile_instruction(ApplyProfileInstructionRequest {
                host_project_id: "profile-owner-a".to_owned(),
                project_id: a.id,
                user_id: 1,
                target,
                instruction: content.to_owned(),
            })
            .await
            .expect("instruction");
        assert_eq!(result.accepted_nodes.len(), 1);
    }
    let second = persisted(&data_plane).await;
    assert_lifetime(
        &second,
        "Project B requires transactional writes.",
        &project_scope_id(b.id).expect("project profile acceptance fixture value"),
        57,
    );
    assert_lifetime(
        &second,
        "The user prefers concise explanations.",
        &subject_scope_id(1).expect("project profile acceptance fixture value"),
        17,
    );
    maintenance
        .rename_project(b.id, "Renamed project")
        .await
        .expect("project profile acceptance fixture value");
    assert_eq!(
        maintenance
            .project_profile_policy_summary(b.id)
            .await
            .expect("project profile acceptance fixture value")
            .effective
            .stable
            .base_ms,
        57 * 86_400_000
    );
    maintenance
        .update_project_profile_policy(VmmNativeProjectProfilePolicyEdit::Inherit {
            project_id: b.id,
        })
        .await
        .expect("project profile acceptance fixture value");
    client
        .apply_profile_instruction(ApplyProfileInstructionRequest {
            host_project_id: "profile-owner-a".to_owned(),
            project_id: a.id,
            user_id: 1,
            target: ProfileTarget::ProfileTargetProject,
            instruction: "Project B keeps explicit migration records.".to_owned(),
        })
        .await
        .expect("inherited instruction");
    let final_snapshot = persisted(&data_plane).await;
    assert_lifetime(
        &final_snapshot,
        "Project B keeps explicit migration records.",
        &project_scope_id(b.id).expect("project profile acceptance fixture value"),
        90,
    );
    // Saving inheritance never retroactively rewrites prior project or personal nodes.
    // 保存继承绝不追溯重写已有项目或个人节点。
    for node in &second.dataset.profile_nodes {
        assert_eq!(
            final_snapshot
                .dataset
                .profile_nodes
                .iter()
                .find(|current| current.id == node.id),
            Some(node)
        );
    }
    assert_eq!(
        runtime
            .runtime_policy()
            .await
            .expect("project profile acceptance fixture value")
            .profiles,
        global
    );
    verify_refresh_freezes_admitted_owner_rules(
        &client,
        &maintenance,
        &data_plane,
        &port,
        b.id,
        &global.project,
    )
    .await;
}

/// Verifies actual refresh extensions, caps and command freezing while the Host AI response is paused.
/// 在 Host AI 响应暂停期间验证实际刷新延长、上限及命令冻结。
/// Uses the existing active project node and two complete policy generations without changing global users.
/// 使用已有活动项目节点及两个完整策略代际，不修改全局用户。
async fn verify_refresh_freezes_admitted_owner_rules(
    client: &NativeVmmClient,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    data_plane: &VmmDataPlaneHandle,
    port: &CapturingAnalysisPort,
    project_id: u64,
    defaults: &vulcan_service_api::VmmNativeProfileLifetimes,
) {
    // Old rules produce 57+2=59 days at weight one; the next generation caps weight two at 75 days.
    // 旧规则在权重一时产生 57+2=59 天；下一代规则在权重二时截断为 75 天。
    let mut old = defaults.clone();
    old.stable.base_ms = 57 * 86_400_000;
    old.stable.step_ms = 2 * 86_400_000;
    old.stable.max_ms = 60 * 86_400_000;
    let mut next = defaults.clone();
    next.stable.base_ms = 70 * 86_400_000;
    next.stable.step_ms = 3 * 86_400_000;
    next.stable.max_ms = 75 * 86_400_000;
    maintenance
        .update_project_profile_policy(VmmNativeProjectProfilePolicyEdit::Override {
            project_id,
            rules: old,
        })
        .await
        .expect("old refresh rules");
    // Channels expose the actual admitted inference boundary without wall-clock sleeps or unbounded tasks.
    // 通道暴露实际已准入推理边界，不使用墙上时钟休眠或无界任务。
    let (entered, observed) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    *port
        .profile_instruction_pause
        .lock()
        .expect("lock project profile acceptance fixture") = Some(ProfileInstructionPause {
        entered,
        resume: resumed,
    });
    let request = ApplyProfileInstructionRequest {
        host_project_id: "profile-owner-a".to_owned(),
        project_id,
        user_id: 1,
        target: ProfileTarget::ProfileTargetProject,
        instruction: "Refresh the project-policy fixture.".to_owned(),
    };
    let (result, ()) = tokio::join!(client.apply_profile_instruction(request.clone()), async {
        tokio::time::timeout(Duration::from_secs(10), observed)
            .await
            .expect("admitted instruction timeout")
            .expect("admitted instruction");
        maintenance
            .update_project_profile_policy(VmmNativeProjectProfilePolicyEdit::Override {
                project_id,
                rules: next,
            })
            .await
            .expect("new rules while admitted");
        resume.send(()).expect("resume exact instruction");
    });
    assert_eq!(result.expect("frozen refresh").accepted_nodes.len(), 1);
    assert_refreshed_lifetime(&persisted(data_plane).await, project_id, 1, 59);
    let result = client
        .apply_profile_instruction(request)
        .await
        .expect("next generation refresh");
    assert_eq!(result.accepted_nodes.len(), 1);
    let snapshot = persisted(data_plane).await;
    assert_refreshed_lifetime(&snapshot, project_id, 2, 75);
    assert_lifetime(
        &snapshot,
        "用户喜欢西瓜。",
        &subject_scope_id(1).expect("project profile acceptance fixture value"),
        17,
    );
    assert_lifetime(
        &snapshot,
        "The user prefers concise explanations.",
        &subject_scope_id(1).expect("project profile acceptance fixture value"),
        17,
    );
}

/// Checks the one active refreshed node, keeping retired versions distinct and auditable.
/// 检查唯一活动刷新节点，保持退役版本独立可审计。
/// Returns only after exact owner, refresh weight and expiry assertions pass.
/// 仅在准确归属、刷新权重及过期时间断言通过后返回。
fn assert_refreshed_lifetime(
    snapshot: &vulcan_memorymesh_core::StorageSnapshot,
    project_id: u64,
    weight: u32,
    days: i64,
) {
    let scope = project_scope_id(project_id).expect("project profile acceptance fixture value");
    let nodes = snapshot
        .dataset
        .profile_nodes
        .iter()
        .filter(|node| {
            node.scope_id == scope
                && node.content == "Project B requires transactional writes."
                && node.state == vulcan_memorymesh_core::ProfileNodeState::Active
        })
        .collect::<Vec<_>>();
    assert_eq!(nodes.len(), 1, "one active refreshed owner node");
    let node = nodes[0];
    assert_eq!(node.refresh_weight, weight);
    assert_eq!(
        node.expires_at_ns,
        Some(
            node.effective_date_ns
                .expect("project profile acceptance fixture value")
                + days * 86_400_000_000_000
        )
    );
}
