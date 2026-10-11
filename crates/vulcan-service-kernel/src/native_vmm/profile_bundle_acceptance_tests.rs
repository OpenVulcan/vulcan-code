//! Real profile instruction, binding and mutually exclusive bundle-mode acceptance.
//! 真实画像指令、绑定及互斥画像包模式验收。

use super::*;
use vulcan_vmm::{ApplyProfileInstructionRequest, GetProfileBundleRequest, ProfileBundleMode};

/// Checks the full mode/explanation matrix against exact persisted contents and ordered project scopes.
/// 根据准确持久化内容及有序项目作用域检查完整模式与说明矩阵。
/// Inputs identify the caller, a deliberately unrelated numeric envelope ID and expected visible facts.
/// 输入标识调用方、刻意无关的封套数字 ID 以及预期可见事实。
async fn assert_bundle_matrix(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    port: &CapturingAnalysisPort,
    host_project_id: &str,
    envelope_project_id: u64,
    projects: &[(u64, &str)],
    user_content: Option<&str>,
) {
    // Reads may render at the current clock, but cannot mutate domain state or call an AI provider.
    // 读取可以按当前时钟渲染，但不得修改领域状态或调用 AI 供应商。
    let before = runtime.storage_snapshot().expect("before bundle reads");
    let llm_count = port.requests.lock().expect("LLM requests").len();
    let embedding_count = port
        .embedding_requests
        .lock()
        .expect("embedding requests")
        .len();
    for mode in [
        ProfileBundleMode::ProfileBundleModeUnspecified,
        ProfileBundleMode::ProfileBundleModeFull,
        ProfileBundleMode::ProfileBundleModeSplit,
    ] {
        for explanation in [None, Some(false), Some(true)] {
            let response = client
                .get_profile_bundle(GetProfileBundleRequest {
                    host_project_id: host_project_id.to_owned(),
                    project_id: envelope_project_id,
                    user_id: 1,
                    mode,
                    include_explanation: explanation,
                })
                .await
                .expect("actual profile bundle query");
            let split = mode == ProfileBundleMode::ProfileBundleModeSplit;
            assert_eq!(
                response.mode,
                if split {
                    mode
                } else {
                    ProfileBundleMode::ProfileBundleModeFull
                }
            );
            assert_eq!(
                response.include_explanation,
                !split && explanation.unwrap_or(false)
            );
            // Follow the actual kernel action projection used by WebUI, not a test-only DTO.
            // 沿用 WebUI 实际使用的内核动作投影，不构造测试专用 DTO。
            let action = crate::vmm::vmm_memory_action_result(
                "load_profile_bundle",
                serde_json::json!({}),
                vulcan_service_runtime::vmm_management::VmmManagementOutcome::ProfileBundle(
                    response.clone(),
                ),
            )
            .expect("actual WebUI action projection");
            let wire = serde_json::to_value(action.data).expect("serialized action union");
            assert_eq!(wire["kind"], "profile_bundle");
            assert_eq!(
                wire["value"]["mode"],
                if split {
                    "PROFILE_BUNDLE_MODE_SPLIT"
                } else {
                    "PROFILE_BUNDLE_MODE_FULL"
                }
            );
            assert_eq!(wire["value"]["combined_text"], response.combined_text);
            assert_eq!(wire["value"]["user_profile"], response.user_profile);
            assert_eq!(wire["value"]["project_profile"], response.project_profile);
            assert!(!response.trace_id.is_empty());
            assert!(response.team_profile.is_empty());
            assert!(response.space_profile.is_empty());
            assert!(response.explanation_text.is_empty());
            assert!(response.environment_priority_text.is_empty());
            // Exactly one representation is consumable; helper fields cannot duplicate combined content.
            // 只有一种表示可以消费；辅助字段不能重复组合内容。
            let project_text = if split {
                assert!(response.combined_text.is_empty());
                response.project_profile.as_str()
            } else {
                assert!(response.project_profile.is_empty());
                assert!(response.user_profile.is_empty());
                response.combined_text.as_str()
            };
            let mut last_header = None;
            for (index, (project_id, content)) in projects.iter().enumerate() {
                let label = if index == 0 {
                    "Project".to_owned()
                } else {
                    format!("Linked Project {index}")
                };
                let header = format!(
                    "[{label} ({})]\n",
                    project_scope_id(*project_id).expect("project scope")
                );
                let position = project_text
                    .find(&header)
                    .expect("exact scoped project header");
                if let Some(previous) = last_header {
                    assert!(previous < position, "caller link order changed");
                }
                last_header = Some(position);
                assert_eq!(
                    project_text.matches(*content).count(),
                    1,
                    "project content duplicated"
                );
                assert!(!response.user_profile.contains(*content));
            }
            // This distinct unlinked project fact must never appear, including with a forged envelope ID.
            // 这条独立的未连带项目事实绝不能出现，包括封套中指定其 ID 的情况。
            assert!(!project_text.contains("绝密项目只采用 Oracle"));
            if let Some(content) = user_content {
                let user_text = if split {
                    &response.user_profile
                } else {
                    &response.combined_text
                };
                assert_eq!(
                    user_text.matches(content).count(),
                    1,
                    "fixed-user content missing or duplicated"
                );
                if split {
                    assert!(!project_text.contains(content));
                }
            } else {
                assert!(response.user_profile.is_empty());
            }
            if projects.is_empty() && split {
                assert!(response.project_profile.is_empty());
            }
            if projects.is_empty() && user_content.is_none() {
                assert!(response.combined_text.is_empty());
            }
            assert_eq!(
                project_text.contains("P/L/W legend"),
                !split
                    && explanation == Some(true)
                    && (!projects.is_empty() || user_content.is_some())
            );
        }
    }
    assert_eq!(
        runtime.storage_snapshot().expect("after bundle reads"),
        before
    );
    assert_eq!(port.requests.lock().expect("LLM requests").len(), llm_count);
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding requests")
            .len(),
        embedding_count
    );
}

/// Creates one profile via the production natural-language instruction flow and verifies accepted content.
/// 通过生产自然语言指令流程创建一条画像，并验证接纳的内容。
/// The deliberately stale envelope ID cannot redirect a bound host project's write scope.
/// 刻意过期的封套 ID 不能重定向已绑定宿主项目的写入作用域。
async fn instruct(
    client: &NativeVmmClient,
    host: &str,
    project_id: u64,
    target: ProfileTarget,
    content: &str,
) {
    let response = client
        .apply_profile_instruction(ApplyProfileInstructionRequest {
            host_project_id: host.to_owned(),
            project_id,
            user_id: 1,
            target,
            instruction: content.to_owned(),
        })
        .await
        .expect("real profile instruction");
    assert_eq!(response.accepted_nodes.len(), 1);
    assert_eq!(response.accepted_nodes[0].content, content);
    assert!(response.retired_nodes.is_empty());
}

/// Exercises empty, project-only, user-only, combined and linked views with real SQLite and Core reads.
/// 使用真实 SQLite 与 Core 读取验收空、仅项目、仅用户、组合及连带视图。
/// Only model output is controlled; registration, instruction validation, storage and bindings are real.
/// 仅模型输出受控；注册、指令校验、存储及绑定均走实际实现。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
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
        .expect("profile capability");
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("profile runtime"),
    );
    let gate = Arc::new(RwLock::new(()));
    let client = Arc::new(NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate.clone(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    ));
    let maintenance =
        crate::native_vmm::NativeVmmMaintenance::new(runtime.clone(), data_plane.clone(), gate);
    // Identical display names make name-based association detectable; tags and IDs remain distinct.
    // 相同显示名称使按名称关联的错误可被发现；标签和 ID 始终不同。
    let owner = maintenance
        .create_and_bind_project("bundle-owner", "相同项目名称")
        .await
        .expect("owner");
    let linked = maintenance
        .create_and_bind_project("bundle-linked", "相同项目名称")
        .await
        .expect("linked");
    let second = maintenance
        .create_and_bind_project("bundle-second", "相同项目名称")
        .await
        .expect("second linked");
    let foreign = maintenance
        .create_and_bind_project("bundle-foreign", "相同项目名称")
        .await
        .expect("foreign");
    let empty = maintenance
        .create_and_bind_project("bundle-empty", "空项目")
        .await
        .expect("empty");
    assert_bundle_matrix(
        &client,
        &runtime,
        &port,
        "bundle-owner",
        foreign.id,
        &[],
        None,
    )
    .await;
    for (host, project_id, content) in [
        (
            "bundle-owner",
            owner.id,
            "主项目采用 PostgreSQL 并要求事务写入。",
        ),
        (
            "bundle-linked",
            linked.id,
            "连带项目采用 SQLite 保留离线数据。",
        ),
        (
            "bundle-second",
            second.id,
            "第二连带项目采用 Redis 保存缓存。",
        ),
        (
            "bundle-foreign",
            foreign.id,
            "绝密项目只采用 Oracle，禁止跨项目读取。",
        ),
    ] {
        instruct(
            &client,
            host,
            project_id,
            ProfileTarget::ProfileTargetProject,
            content,
        )
        .await;
    }
    assert_bundle_matrix(
        &client,
        &runtime,
        &port,
        "bundle-owner",
        foreign.id,
        &[(owner.id, "主项目采用 PostgreSQL 并要求事务写入。")],
        None,
    )
    .await;
    instruct(
        &client,
        "bundle-empty",
        empty.id,
        ProfileTarget::ProfileTargetUser,
        "固定用户喜欢西瓜和葡萄。",
    )
    .await;
    assert_bundle_matrix(
        &client,
        &runtime,
        &port,
        "bundle-empty",
        foreign.id,
        &[],
        Some("固定用户喜欢西瓜和葡萄。"),
    )
    .await;
    maintenance
        .set_project_access("bundle-owner", owner.id, &[second.id, linked.id])
        .await
        .expect("ordered query links");
    let projects = [
        (owner.id, "主项目采用 PostgreSQL 并要求事务写入。"),
        (second.id, "第二连带项目采用 Redis 保存缓存。"),
        (linked.id, "连带项目采用 SQLite 保留离线数据。"),
    ];
    assert_bundle_matrix(
        &client,
        &runtime,
        &port,
        "bundle-owner",
        foreign.id,
        &projects,
        Some("固定用户喜欢西瓜和葡萄。"),
    )
    .await;
    maintenance
        .rename_project(linked.id, "连带项目改名后")
        .await
        .expect("rename display only");
    // Explicit management selection intentionally has no host binding and reads only the chosen project.
    // 显式管理选择有意不携带宿主绑定，只读取选定项目。
    assert_bundle_matrix(
        &client,
        &runtime,
        &port,
        "",
        owner.id,
        &[(owner.id, "主项目采用 PostgreSQL 并要求事务写入。")],
        Some("固定用户喜欢西瓜和葡萄。"),
    )
    .await;
    assert_bundle_matrix(
        &client,
        &runtime,
        &port,
        "bundle-owner",
        foreign.id,
        &projects,
        Some("固定用户喜欢西瓜和葡萄。"),
    )
    .await;
    // A query link grants visibility only. The instruction can see and modify only the actual owner.
    // 查询连带仅授予可见性。指令只能查看及修改实际归属项目。
    let before = runtime
        .storage_snapshot()
        .expect("before owner instruction");
    instruct(
        &client,
        "bundle-owner",
        linked.id,
        ProfileTarget::ProfileTargetProject,
        "主项目新增只属于自己的审计约束。",
    )
    .await;
    // Bound the capture guard and its borrowed input before any later asynchronous maintenance.
    // 在后续异步维护之前限定捕获守卫及其借用输入的作用域。
    {
        let captures = port.requests.lock().expect("instruction captures");
        let input = analysis_input(captures.last().expect("last actual instruction"));
        assert_eq!(
            input["bind_id"],
            project_scope_id(owner.id).expect("owner scope").as_str()
        );
        assert_eq!(input["target"], "PROJECT");
        assert!(!input.to_string().contains("离线数据"));
        assert!(!input.to_string().contains("Redis"));
    }
    let after = runtime.storage_snapshot().expect("after owner instruction");
    for node in &before.dataset.profile_nodes {
        assert_eq!(
            after
                .dataset
                .profile_nodes
                .iter()
                .find(|candidate| candidate.id == node.id),
            Some(node)
        );
    }
    assert_eq!(
        after.dataset.profile_nodes.len(),
        before.dataset.profile_nodes.len() + 1
    );
    assert_eq!(
        after
            .dataset
            .profile_nodes
            .iter()
            .find(|node| node.content == "主项目新增只属于自己的审计约束。")
            .expect("new owner node")
            .scope_id,
        project_scope_id(owner.id).expect("owner scope")
    );
    let persisted: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("SQLite read")
            .expect("persisted profile snapshot"),
    )
    .expect("typed snapshot");
    assert_eq!(persisted, after.without_embeddings());
    maintenance
        .replace_query_links("bundle-owner", &[])
        .await
        .expect("unlink");
    let response = client
        .get_profile_bundle(GetProfileBundleRequest {
            host_project_id: "bundle-owner".to_owned(),
            project_id: foreign.id,
            user_id: 1,
            mode: ProfileBundleMode::ProfileBundleModeSplit,
            include_explanation: Some(true),
        })
        .await
        .expect("unlinked bundle");
    assert!(!response.project_profile.contains("离线数据"));
    assert!(!response.project_profile.contains("Redis"));
    assert!(response.user_profile.contains("固定用户喜欢西瓜和葡萄。"));
    profile_cache_acceptance::verify(&client, &runtime, &maintenance, &data_plane, owner.id).await;
}

#[path = "profile_cache_acceptance_tests.rs"]
mod profile_cache_acceptance;
