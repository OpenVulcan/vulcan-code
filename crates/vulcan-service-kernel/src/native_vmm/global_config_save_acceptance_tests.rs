//! Real global VMM saves with isolated configuration persistence and a nonempty native data plane.
//! 使用隔离配置持久化及非空原生数据面的真实全局 VMM 保存。

use super::*;
use crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH;
use crate::tests::test_support::{
    TestHostRuntimeRoot, test_agent_runtime_bridge, test_service_state_with_parent_session,
};
use futures_util::FutureExt;
use vulcan_agent_runtime::agent_runtime::vmm_hooks::VmmHookConfigSnapshot;
use vulcan_provider_config::config::ProviderConfigurationFile;
use vulcan_service_api::{
    ProjectSettingsDomain, ServiceEvent, ServiceEventEmitter, ServiceEventSink,
    VmmConfigSaveRequest, VmmProjectBindingPlanEntry, WorkbenchDelta, WorkbenchSnapshot,
};

#[path = "global_config_preflight_acceptance_tests.rs"]
mod preflight_acceptance;

#[path = "global_config_project_catalog_acceptance_tests.rs"]
mod project_catalog_acceptance;

#[path = "global_config_binding_review_acceptance_tests.rs"]
mod binding_review_acceptance;

/// Captures the ordered notifications emitted by actual configuration service calls.
/// 捕获真实配置服务调用发出的有序通知。
#[derive(Default)]
struct SaveEvents {
    /// Retained owned service events; no synthetic success event is inserted.
    /// 保留的服务事件所有权；不插入合成成功事件。
    events: StdMutex<Vec<ServiceEvent>>,
}

impl ServiceEventSink for SaveEvents {
    /// Accept one emitted event and return after synchronous capture.
    /// 接收一个已发出的事件，在同步捕获后返回。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events.lock().expect("save events").push(event);
    }
}

/// Build a request from the same canonical draft projection consumed by the settings UI.
/// 从设置界面消费的同一规范草稿投影构建请求。
/// Takes the authoritative layer; returns a global request without implicit binding decisions.
/// 接收权威配置层；返回不含隐式绑定决定的全局请求。
fn request(layer: &vulcan_vmm::VmmConfigLayer) -> VmmConfigSaveRequest {
    VmmConfigSaveRequest {
        scope: "global".to_owned(),
        project_id: None,
        draft: crate::vmm::vmm_config_draft_summary(
            layer,
            &vulcan_vmm::VmmEffectiveConfig::resolve(layer, None, None),
            false,
            true,
        ),
        binding_plan: Vec::new(),
    }
}

/// Run the actual service future inside its own task-local private destination.
/// 在服务协程自身的任务局部私有目标作用域内运行真实服务。
/// Takes state, emitter, exact file and request; returns the unmodified service result.
/// 接收状态、发送器、精确文件及请求；返回未经修改的服务结果。
async fn save(
    state: &crate::SharedServiceBridgeState,
    app: &ServiceEventEmitter,
    path: &std::path::Path,
    request: VmmConfigSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    // Scope the spawned future itself: Tokio task locals are deliberately not inherited by spawn.
    // 对派生协程自身设置作用域：Tokio 任务局部值明确不会被 spawn 继承。
    tokio::spawn(PROVIDER_CONFIGURATION_PATH.scope(
        path.to_owned(),
        crate::service_save_vmm_memory_config(app.clone(), state.clone(), request),
    ))
    .await
    .expect("configuration service task must not panic")
}

/// Verify disk and live configuration equality while preserving every unrelated document field.
/// 验证磁盘及活动配置相等，同时保留文档中全部无关字段。
/// Takes the private destination, service state and original non-VMM configuration; returns no value.
/// 接收私有目标、服务状态及原始非 VMM 配置；不返回值。
fn assert_configuration(
    path: &std::path::Path,
    state: &crate::SharedServiceBridgeState,
    original: &ProviderConfigurationFile,
) {
    // Decode the real bytes produced by the production serializer and atomic writer.
    // 解码生产序列化器及原子写入器产生的真实字节。
    let durable: ProviderConfigurationFile =
        serde_json::from_slice(&std::fs::read(path).expect("private configuration bytes"))
            .expect("persisted provider document");
    // Read the actual retained authority, not the detached setup bridge.
    // 读取实际保留的权威状态，而非已脱离的初始化桥接对象。
    let live = crate::provider_configuration_from_state(state).expect("live configuration");
    assert_eq!(
        serde_json::to_value(&durable).expect("global VMM save fixture value"),
        serde_json::to_value(&live).expect("global VMM save fixture value")
    );
    // Only the VMM layer is owned by this save family.
    // 此保存请求族仅拥有 VMM 配置层。
    let mut expected = original.clone();
    expected.vmm = live.vmm;
    assert_eq!(
        serde_json::to_value(durable).expect("global VMM save fixture value"),
        serde_json::to_value(expected).expect("global VMM save fixture value")
    );
}

/// Verify that an older save finishing late cannot override the newest committed native mode.
/// 验证较早的保存延迟完成时不能覆盖最新提交的原生模式。
/// Takes live state, emitter, private path and native authority; each mode order uses two complete requests.
/// 接收活动状态、发送器、私有路径及原生权威；每种模式顺序均使用两次完整请求。
async fn verify_late_save_completion(
    state: &crate::SharedServiceBridgeState,
    app: &ServiceEventEmitter,
    path: &std::path::Path,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    host_id: &str,
    session_id: uuid::Uuid,
) {
    use crate::vmm::config_save_test_scope::{GLOBAL_SAVE_COMMIT_PAUSE, GlobalSavePause};

    // The shared Hook handle must agree with both disk and worker admission after either interleaving.
    // 共享 Hook 句柄必须在两种交错后均与磁盘及工作器准入一致。
    let hook = state
        .lock()
        .expect("concurrent state")
        .agent_runtime
        .as_ref()
        .expect("concurrent agent")
        .vmm_hook_runtime
        .clone();
    // Exercise both last-enable-wins and last-disable-wins without changing the committed-mode rule.
    // 验证最后启用生效及最后关闭生效两种情况，不改变已提交模式权威规则。
    for latest_enabled in [true, false] {
        // The original live document supplies both requests without inventing defaults or binding plans.
        // 原始活动文档提供两个请求，不编造默认值或绑定计划。
        let before = crate::provider_configuration_from_state(state).expect("concurrent before");
        // Only the first request pauses; the second remains an ordinary independent service task.
        // 只有第一个请求暂停；第二个仍是普通独立服务任务。
        let pause = Arc::new(GlobalSavePause::default());
        // A JoinSet aborts the paused request if a later assertion fails, preventing a detached writer.
        // 后续断言失败时 JoinSet 会中止暂停请求，防止产生脱离的写入器。
        let mut saves = tokio::task::JoinSet::new();
        // Commit the opposite mode first, then hold its continuation until the newer save completes.
        // 先提交相反模式，再保留其续接，直到较新的保存完成。
        let mut earlier_request = request(&before.vmm);
        earlier_request.draft.enabled = !latest_enabled;
        saves.spawn(PROVIDER_CONFIGURATION_PATH.scope(
            path.to_owned(),
            GLOBAL_SAVE_COMMIT_PAUSE.scope(
                pause.clone(),
                crate::service_save_vmm_memory_config(app.clone(), state.clone(), earlier_request),
            ),
        ));
        tokio::time::timeout(std::time::Duration::from_secs(30), pause.entered.notified())
            .await
            .expect("first save must reach committed boundary");
        assert_eq!(
            crate::provider_configuration_from_state(state)
                .expect("global VMM save fixture value")
                .vmm
                .enabled
                .expect("global VMM save fixture value"),
            !latest_enabled
        );
        // Complete the newer real save while the older request remains paused after its transaction.
        // 较早请求仍暂停在事务后时，完成较新的真实保存。
        let mut latest_request = request(&before.vmm);
        latest_request.draft.enabled = latest_enabled;
        // The complete response must project the latest committed mode rather than the earlier request.
        // 完整响应必须投影最新提交的模式，而不是较早请求。
        let latest_response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            save(state, app, path, latest_request),
        )
        .await
        .expect("new save must not retain the older transaction lock")
        .expect("newer save");
        assert_eq!(
            latest_response.vmm_memory.global_draft.enabled,
            latest_enabled
        );
        assert_eq!(
            maintenance
                .prepare_recall_publication(Some(host_id), &session_id.to_string())
                .await
                .is_ok(),
            latest_enabled,
            "newer save publishes worker mode immediately"
        );
        pause.release.notify_one();
        // Late completion still refreshes from the current authority; it must not restore its own stale mode.
        // 延迟完成仍从当前权威刷新；不得恢复其自身过时模式。
        let late_response = saves
            .join_next()
            .await
            .expect("older save handle")
            .expect("older save must not panic")
            .expect("older save response");
        assert_configuration(path, state, &before);
        assert_eq!(
            late_response.vmm_memory.global_draft.enabled,
            latest_enabled
        );
        assert_eq!(
            crate::provider_configuration_from_state(state)
                .expect("global VMM save fixture value")
                .vmm
                .enabled
                .expect("global VMM save fixture value"),
            latest_enabled
        );
        assert_eq!(
            hook.global_memory_mode()
                .expect("global VMM save fixture value"),
            latest_enabled.then_some(vulcan_vmm::VmmBackendMode::Native)
        );
        assert_eq!(
            maintenance
                .prepare_recall_publication(Some(host_id), &session_id.to_string())
                .await
                .is_ok(),
            latest_enabled,
            "late save must not override newer committed native mode"
        );
    }
}

/// Exercise persistence failure, ordinary tuning, disable, rejected enable and explicit re-enable.
/// 验证持久化失败、普通调参、关闭、启用拒绝及明确重新启用。
/// Uses the supplied real Core/data plane and stored hit; every controller and file is private.
/// 使用传入的真实 Core、数据面及已存储命中；全部控制器及文件均为私有。
pub(super) async fn verify(
    runtime: &NativeVmmCoreRuntime,
    data_plane: &VmmDataPlaneHandle,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    project_id: u64,
    memory: &vulcan_vmm::MemorySearchHit,
) {
    // The attached APP controller supplies the real index used by the full workbench refresh.
    // 附加的 APP 控制器提供完整工作台刷新实际使用的索引。
    let session_id = uuid::Uuid::new_v4();
    // This guard owns all V1/configuration files until the attached controller has shut down.
    // 此守卫持有全部 V1 及配置文件，直至附加控制器关闭。
    let root = TestHostRuntimeRoot::new(session_id);
    // Retain the actual client so cleanup runs even when an acceptance assertion panics.
    // 保留真实客户端，使验收断言恐慌时仍执行清理。
    let controller_runtime = root.attached_host_runtime_handle().await;
    // Catch only to release the managed process; assertion failures are rethrown below.
    // 仅为释放托管进程而捕获；断言失败将在下方重新抛出。
    let outcome = std::panic::AssertUnwindSafe(async {
        vulcan_workspace_store::data_plane::prepare_application_schema_for_test(
            &controller_runtime,
        )
        .await
        .expect("production application schema on private binding");
        // Register the V1 locator's exact owner through the normal project persistence API.
        // 通过普通项目持久化接口注册 V1 定位器的准确归属。
        let indexed_project = vulcan_workbench_model::TaskProjectSummary::new(
            root.v1_storage_locator().project_id(),
            "Global save fixture",
            root.data_root()
                .join("private-project")
                .display()
                .to_string(),
            false,
            0,
            None,
            None,
            Vec::new(),
        );
        vulcan_workspace_store::workspace_store::persist_new_project(
            controller_runtime.clone(),
            &indexed_project,
        )
        .await
        .expect("persist exact indexed owner");
        // Load authoritative indexed projects and sessions instead of inventing UI ownership.
        // 加载权威索引项目及会话，而非编造界面归属。
        let workspace = vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller_runtime.clone(),
        )
        .await
        .expect("real indexed workspace");
        // Resolve the exact host identity from the indexed session metadata.
        // 从索引会话元数据解析精确宿主身份。
        let host_id = workspace
            .sessions
            .iter()
            .find(|item| item.id == session_id)
            .expect("indexed session")
            .project_id
            .clone()
            .expect("indexed project");
        assert!(
            workspace
                .task_projects
                .iter()
                .any(|item| item.id == host_id)
        );
        for project in &workspace.task_projects {
            maintenance
                .bind_existing_project(&project.id, project_id)
                .await
                .expect("initial exact binding");
        }
        maintenance.set_native_backend_enabled(true);
        maintenance
            .validate_host_ai()
            .await
            .expect("isolated AI ready");
        // Install the same native configuration in the authoritative bridge and Hook runtime.
        // 在权威桥接及 Hook 运行时安装相同原生配置。
        let mut agent = test_agent_runtime_bridge();
        agent.provider_configuration.vmm.enabled = Some(true);
        agent.provider_configuration.vmm.backend_mode = Some(vulcan_vmm::VmmBackendMode::Native);
        agent.provider_configuration.vmm.grpc_api_key = Some("isolated-grpc-secret".to_owned());
        // Native tests retain valid external credential fields without activating any external transport.
        // 原生测试保留有效的外置凭据字段，但不启用任何外置传输。
        agent.provider_configuration.vmm.management_base_url =
            Some("https://vmm.example.test".to_owned());
        agent.provider_configuration.vmm.management_bearer_token =
            Some("isolated-management-secret".to_owned());
        agent
            .vmm_hook_runtime
            .replace_snapshot(VmmHookConfigSnapshot {
                global: agent.provider_configuration.vmm.clone(),
                ..Default::default()
            })
            .expect("initial Hook");
        // Keep the live shared Hook handle to observe publication across cloned bridges.
        // 保留活动共享 Hook 句柄，观察跨桥接克隆的发布。
        let hook = agent.vmm_hook_runtime.clone();
        // Preserve the entire original document, including unrelated model and tool configuration.
        // 保留完整原始文档，包含无关模型及工具配置。
        let original = agent.provider_configuration.clone();
        // Use the actual indexed snapshot for binding validation and final workbench projection.
        // 将真实索引快照用于绑定校验及最终工作台投影。
        let mut service = test_service_state_with_parent_session(session_id);
        service.agent_runtime = Some(agent);
        service.controller_runtime = Some(controller_runtime);
        service.native_vmm_maintenance = Some(maintenance.clone());
        service.workspace_snapshot = Some(workspace);
        // The existing controller holds a real durable recall sourced from the nonempty Core fixture.
        // 既有控制器持有源自非空 Core 夹具的真实持久召回。
        let controller = service
            .vmm_controller_registry
            .get_or_open(root.v1_storage_locator(), session_id, "1")
            .expect("resident controller");
        // A publication barrier derives the authoritative binding identity without changing Core data.
        // 发布屏障派生权威绑定身份，不改变 Core 数据。
        let publication = maintenance
            .prepare_recall_publication(Some(&host_id), &session_id.to_string())
            .await
            .expect("initial publication");
        // Retain only the source identity; release the gate before invoking any save operation.
        // 仅保留来源身份；调用任何保存操作前释放门控。
        let source = publication.profile_read_source().expect("exact source");
        drop(publication);
        controller
            .execute_command("global-save-recall", "1", |cached| {
                cached.select_read_source(&session_id.to_string(), Some(&source), "1")?;
                cached.apply_recall_batch(
                    &session_id.to_string(),
                    "global-save-anchor",
                    &vulcan_vmm::VmmRecallBatch {
                        ttl_turns: 3,
                        injection_tag: "VMM_MEMORY".to_owned(),
                        injection_description: "Stored memory".to_owned(),
                        queries: Vec::new(),
                        items: vec![vulcan_vmm::ContextItem {
                            memory_id: memory.memory_id,
                            text: memory.abstract_text.clone(),
                            ..Default::default()
                        }],
                    },
                    "1",
                )?;
                Ok(())
            })
            .expect("real recall cache");
        // Freeze the complete cache and Core snapshot to detect unintended resets or deletions.
        // 冻结完整缓存及 Core 快照，检测非预期重置或删除。
        let cache_before = controller.snapshot().expect("cache before");
        assert!(!cache_before.active_leases.is_empty());
        // Store both relational and Core projections before configuration-only actions.
        // 在纯配置动作前保留关系及 Core 投影。
        let core_before = runtime.storage_snapshot().expect("Core before");
        // Persisted Core bytes independently detect accidental storage mutations.
        // 持久 Core 字节独立检测意外存储修改。
        let stored_before = data_plane
            .load_core_snapshot_json()
            .await
            .expect("stored before");
        // Use a fresh private configuration path, never the application's user configuration.
        // 使用全新私有配置路径，绝不使用应用的用户配置。
        let path = root
            .data_root()
            .join("configuration")
            .join("vulcan-code.json");
        assert_ne!(
            path,
            vulcan_provider_config::config::configuration_file_path()
        );
        vulcan_provider_config::config::save_configuration_file_at_path(&original, &path)
            .expect("persist private before-image");
        // Own the live service state and actual event sink throughout the whole save sequence.
        // 在完整保存序列期间持有活动服务状态及真实事件接收器。
        let state = Arc::new(StdMutex::new(service));
        // Capture actual workbench and settings invalidation events.
        // 捕获真实工作台及设置失效事件。
        let events = Arc::new(SaveEvents::default());
        // The real service emitter publishes every observed event.
        // 真实服务发送器发布全部观测事件。
        let app = ServiceEventEmitter::new(vec![events.clone()]);
        // A directory at the exact destination forces a real atomic-write failure before live commit.
        // 精确目标位置的目录在活动提交前触发真实原子写入失败。
        let blocked_path = root.data_root().join("blocked-configuration");
        std::fs::create_dir(&blocked_path).expect("private blocked destination");
        // A valid disable request must not mutate runtime when durable persistence fails.
        // 有效关闭请求在持久保存失败时不得修改运行时。
        let mut disabled = request(&original.vmm);
        disabled.draft.enabled = false;
        assert!(
            save(&state, &app, &blocked_path, disabled.clone())
                .await
                .is_err()
        );
        assert_configuration(&path, &state, &original);
        assert_eq!(
            controller
                .snapshot()
                .expect("global VMM save fixture value"),
            cache_before
        );
        assert_eq!(
            hook.global_memory_mode()
                .expect("global VMM save fixture value"),
            Some(vulcan_vmm::VmmBackendMode::Native)
        );
        assert!(
            events
                .events
                .lock()
                .expect("lock global VMM save fixture")
                .is_empty()
        );
        // Same-backend tuning must retain recall and preserve redacted secrets on disk.
        // 同后端调参必须保留召回，并在磁盘上保留已脱敏显示的秘密。
        let mut tuning = request(&original.vmm);
        tuning.draft.profile_refresh_turns = "7".to_owned();
        save(&state, &app, &path, tuning)
            .await
            .expect("ordinary real save");
        assert_configuration(&path, &state, &original);
        assert_eq!(
            controller
                .snapshot()
                .expect("global VMM save fixture value"),
            cache_before
        );
        assert_eq!(
            hook.resolve_publication_config(session_id, "")
                .expect("global VMM save fixture value")
                .profile_refresh_turns,
            7
        );
        assert_eq!(
            crate::provider_configuration_from_state(&state)
                .expect("global VMM save fixture value")
                .vmm
                .grpc_api_key,
            original.vmm.grpc_api_key
        );
        // Disable clears runtime cache but never stored memories or profiles.
        // 关闭清空运行时缓存，但绝不删除已存储记忆或画像。
        save(&state, &app, &path, disabled)
            .await
            .expect("real disable");
        assert_configuration(&path, &state, &original);
        assert!(
            controller
                .snapshot()
                .expect("global VMM save fixture value")
                .active_leases
                .is_empty()
        );
        assert!(
            controller
                .snapshot()
                .expect("global VMM save fixture value")
                .read_source
                .is_none()
        );
        assert_eq!(
            hook.global_memory_mode()
                .expect("global VMM save fixture value"),
            None
        );
        assert!(
            maintenance
                .prepare_recall_publication(Some(&host_id), &session_id.to_string())
                .await
                .is_err()
        );
        // Remove one binding explicitly so resume and explicit reconciliation observe an unbound project.
        // 明确移除一个绑定，使恢复与显式对账都面对一个未绑定项目。
        maintenance
            .unbind_project(&host_id)
            .await
            .expect("explicit unbind before enable");
        // Preserve all rejected-request observables, not just the response error.
        // 保留全部拒绝请求的可观测状态，而不仅是响应错误。
        let disabled_bytes = std::fs::read(&path).expect("global VMM save fixture value");
        // Workbench generation must not advance when the binding plan is rejected before persistence.
        // 绑定计划在持久化前被拒绝时，工作台代次不得推进。
        let rejected_revision = state
            .lock()
            .expect("lock global VMM save fixture")
            .workbench
            .revision;
        // No event may be emitted for a rejected enable request.
        // 被拒绝的启用请求不得发送事件。
        let rejected_event_count = events
            .events
            .lock()
            .expect("lock global VMM save fixture")
            .len();
        // An explicit plan that does not exactly cover the unbound projects is rejected before persistence.
        // 未准确覆盖未绑定项目的显式计划会在持久化前被拒绝。
        let mut enabled = request(&original.vmm);
        let mut mismatched = enabled.clone();
        mismatched.binding_plan = vec![VmmProjectBindingPlanEntry {
            vulcan_project_id: String::from("project-not-in-workspace"),
            action: "bind_existing".to_owned(),
            project_name: None,
            target_project_id: Some(project_id),
        }];
        assert!(
            save(&state, &app, &path, mismatched)
                .await
                .expect_err("mismatched plan")
                .contains("must cover exactly")
        );
        assert_eq!(
            std::fs::read(&path).expect("global VMM save fixture value"),
            disabled_bytes
        );
        assert_eq!(
            state
                .lock()
                .expect("lock global VMM save fixture")
                .workbench
                .revision,
            rejected_revision
        );
        assert_eq!(
            events
                .events
                .lock()
                .expect("lock global VMM save fixture")
                .len(),
            rejected_event_count
        );
        assert_eq!(
            hook.global_memory_mode()
                .expect("global VMM save fixture value"),
            None
        );
        // Resuming the same configured backend needs no plan and leaves the unbound project unbound.
        // 恢复同一已配置后端无需计划，且保持未绑定项目不被绑定。
        save(&state, &app, &path, enabled.clone())
            .await
            .expect("resume without plan");
        assert_configuration(&path, &state, &original);
        assert_eq!(
            hook.global_memory_mode()
                .expect("global VMM save fixture value"),
            Some(vulcan_vmm::VmmBackendMode::Native)
        );
        assert!(
            data_plane.resolve_project_access(&host_id).await.is_err(),
            "resume must not rebind an explicitly unbound project"
        );
        // An explicit plan on the same backend reconciles the unbound project.
        // 同一后端上的显式计划会对账未绑定项目。
        enabled.binding_plan = vec![VmmProjectBindingPlanEntry {
            vulcan_project_id: host_id.clone(),
            action: "bind_existing".to_owned(),
            project_name: None,
            target_project_id: Some(project_id),
        }];
        save(&state, &app, &path, enabled)
            .await
            .expect("explicit binding plan");
        assert_configuration(&path, &state, &original);
        assert_eq!(
            hook.global_memory_mode()
                .expect("global VMM save fixture value"),
            Some(vulcan_vmm::VmmBackendMode::Native)
        );
        assert_eq!(
            data_plane
                .resolve_project_access(&host_id)
                .await
                .expect("global VMM save fixture value")
                .owner
                .id,
            project_id
        );
        assert!(
            controller
                .snapshot()
                .expect("global VMM save fixture value")
                .active_leases
                .is_empty(),
            "resume must not resurrect old recall"
        );
        drop(
            maintenance
                .prepare_recall_publication(Some(&host_id), &session_id.to_string())
                .await
                .expect("enabled publication"),
        );
        preflight_acceptance::verify(&state, &path, maintenance, &host_id, project_id, session_id)
            .await;
        project_catalog_acceptance::verify(&state, &path, data_plane, project_id).await;
        binding_review_acceptance::verify(&state, &path, data_plane, &host_id, project_id).await;
        verify_late_save_completion(&state, &app, &path, maintenance, &host_id, session_id).await;
        assert_eq!(
            runtime
                .storage_snapshot()
                .expect("global VMM save fixture value"),
            core_before
        );
        assert_eq!(
            data_plane
                .load_core_snapshot_json()
                .await
                .expect("global VMM save fixture value"),
            stored_before
        );
        // Each successful save publishes its full workbench and exactly one global VMM invalidation.
        // 每次成功保存发布完整工作台及恰好一条全局 VMM 失效通知。
        let captured = events.events.lock().expect("lock global VMM save fixture");
        assert_eq!(
            captured
                .iter()
                .filter(|event| matches!(event, ServiceEvent::Workbench(_)))
                .count(),
            8
        );
        assert_eq!(captured.iter().filter(|event| matches!(event,
            ServiceEvent::WorkbenchDelta(envelope) if matches!(&envelope.delta,
                WorkbenchDelta::ProjectSettingsChanged(changed)
                if changed.project_id.is_none() && changed.domain == ProjectSettingsDomain::Vmm)
        )).count(), 8);
    })
    .catch_unwind()
    .await;
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
