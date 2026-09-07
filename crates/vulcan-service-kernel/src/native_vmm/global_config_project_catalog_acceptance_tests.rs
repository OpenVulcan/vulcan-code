//! Real enable-plan preflight against a changing persisted host-project catalog.
//! 面向变化中的持久宿主项目目录的真实启用计划预检。

use super::*;
use crate::vmm::config_save_test_scope::{GLOBAL_SAVE_PREFLIGHT_PAUSE, GlobalSavePause};

/// Verify adding an unbound host project invalidates an already reviewed empty enable plan.
/// 验证新增未绑定宿主项目会使已经审查的空启用计划失效。
/// Takes the live service, private configuration file, real data plane and existing native project ID.
/// 接收活动服务、私有配置文件、真实数据面及已有原生项目标识。
pub(super) async fn verify(
    state: &crate::SharedServiceBridgeState,
    path: &std::path::Path,
    data_plane: &VmmDataPlaneHandle,
    project_id: u64,
) {
    // Capture this catalog scenario's notifications separately from the enclosing save suites.
    // 独立于外围保存测试组捕获此目录场景的通知。
    let events = Arc::new(SaveEvents::default());
    // Every VMM operation uses the actual service emitter and unmodified service request.
    // 每项 VMM 操作都使用实际服务发送器及未经修改的服务请求。
    let app = ServiceEventEmitter::new(vec![events.clone()]);
    // All projects are initially bound by preceding real fixture operations.
    // 前面的真实夹具操作已绑定全部初始项目。
    let before = crate::provider_configuration_from_state(state).expect("catalog before");
    // Disable first so the subsequent enable must reconcile the complete current project set.
    // 先关闭，使后续启用必须对账完整当前项目集合。
    let mut disable = request(&before.vmm);
    disable.draft.enabled = false;
    save(state, &app, path, disable)
        .await
        .expect("disable before catalog review");
    // The enabled request contains no decisions because the reviewed catalog has no unbound projects.
    // 启用请求不包含决定，因为已审查目录中不存在未绑定项目。
    let enable = request(&before.vmm);
    assert!(enable.binding_plan.is_empty());
    // Pause only after real catalog validation, without holding the Provider transaction.
    // 仅在真实目录校验后暂停，不持有 Provider 事务。
    let pause = Arc::new(GlobalSavePause::default());
    // The pending writer is owned and aborted if any intermediate assertion fails.
    // 持有待完成写入器，并在任一中间断言失败时中止。
    let mut pending = tokio::task::JoinSet::new();
    pending.spawn(PROVIDER_CONFIGURATION_PATH.scope(
        path.to_owned(),
        GLOBAL_SAVE_PREFLIGHT_PAUSE.scope(
            pause.clone(),
            crate::service_save_vmm_memory_config(app.clone(), state.clone(), enable),
        ),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(30), pause.entered.notified())
        .await
        .expect("enable reaches reviewed boundary");
    // Use the attached APP controller rather than editing only an in-memory project list.
    // 使用已附加 APP 控制器，而非只修改内存项目列表。
    let controller = state
        .lock()
        .expect("catalog state")
        .controller_runtime
        .clone()
        .expect("actual APP controller");
    // A new stable host identity must receive its own explicit binding decision.
    // 新稳定宿主身份必须获得其自身明确绑定决定。
    let added_id = uuid::Uuid::new_v4().to_string();
    // The source path is another private fixture label beside the isolated configuration file.
    // 源码路径是位于隔离配置文件旁的另一个私有夹具位置。
    let added = vulcan_workbench_model::TaskProjectSummary::new(
        added_id.clone(),
        "Added during enable review",
        path.with_file_name("added-binding-project")
            .display()
            .to_string(),
        false,
        1,
        None,
        None,
        Vec::new(),
    );
    vulcan_workspace_store::workspace_store::persist_new_project(controller.clone(), &added)
        .await
        .expect("persist actual new host project");
    crate::refresh_workbench_snapshot(state.clone())
        .await
        .expect("publish actual changed index");
    assert!(data_plane.resolve_project_access(&added_id).await.is_err());
    // Freeze every save-owned observable only after the independent catalog refresh completes.
    // 仅在独立目录刷新完成后冻结全部由保存拥有的可观测状态。
    let reviewed_configuration =
        crate::provider_configuration_from_state(state).expect("still disabled");
    // Exact bytes must remain unchanged when the old plan is rejected.
    // 旧计划被拒绝时，准确字节必须保持不变。
    let reviewed_bytes = std::fs::read(path).expect("reviewed private bytes");
    // A catalog edit may advance revision; the subsequently rejected save may not.
    // 目录编辑可以推进版本；随后被拒绝的保存不能再推进。
    let reviewed_revision = state
        .lock()
        .expect("lock host catalog review fixture")
        .workbench
        .revision;
    // No success invalidation belongs to a rejected stale plan.
    // 被拒绝的陈旧计划不拥有成功失效通知。
    let reviewed_event_count = events
        .events
        .lock()
        .expect("lock host catalog review fixture")
        .len();
    pause.release.notify_one();
    // Inspect after the actual old save finishes, not while its writes are still paused.
    // 在实际旧保存完成后检查，而不是在其写入仍暂停时检查。
    let stale_result = pending
        .join_next()
        .await
        .expect("catalog save task")
        .expect("catalog save task must not panic");
    assert_eq!(
        crate::provider_configuration_from_state(state)
            .expect("host catalog review fixture value")
            .vmm
            .enabled,
        Some(false),
        "adding an unbound project must not silently enable memory with an obsolete empty plan"
    );
    assert_eq!(
        std::fs::read(path).expect("host catalog review fixture value"),
        reviewed_bytes
    );
    assert_eq!(
        state
            .lock()
            .expect("lock host catalog review fixture")
            .workbench
            .revision,
        reviewed_revision
    );
    assert_eq!(
        events
            .events
            .lock()
            .expect("lock host catalog review fixture")
            .len(),
        reviewed_event_count
    );
    assert!(
        stale_result
            .expect_err("changed project catalog must reject old plan")
            .contains("项目")
    );
    assert_configuration(path, state, &reviewed_configuration);
    // A freshly reviewed request now explicitly covers the one genuinely new unbound project.
    // 重新审查的请求现在明确覆盖唯一真正新增的未绑定项目。
    let mut reviewed = request(&reviewed_configuration.vmm);
    reviewed.draft.enabled = true;
    reviewed.binding_plan = vec![VmmProjectBindingPlanEntry {
        vulcan_project_id: added_id.clone(),
        action: "bind_existing".to_owned(),
        project_name: None,
        target_project_id: Some(project_id),
    }];
    save(state, &app, path, reviewed)
        .await
        .expect("fresh exact plan");
    assert_eq!(
        data_plane
            .resolve_project_access(&added_id)
            .await
            .expect("host catalog review fixture value")
            .owner
            .id,
        project_id
    );
    assert_configuration(path, state, &before);
    // Native binding ownership is the stable ID, so rename and relocation must not invalidate a reviewed plan.
    // 原生绑定归属于稳定 ID，因此重命名与路径迁移不得使已审查计划失效。
    let mut disable = request(&before.vmm);
    disable.draft.enabled = false;
    save(state, &app, path, disable)
        .await
        .expect("disable before native identity review");
    // Reuse the now fully bound project set, not a new or guessed binding decision.
    // 复用现已完整绑定的项目集合，不使用新增或猜测的绑定决定。
    let pause = Arc::new(GlobalSavePause::default());
    pending.spawn(PROVIDER_CONFIGURATION_PATH.scope(
        path.to_owned(),
        GLOBAL_SAVE_PREFLIGHT_PAUSE.scope(
            pause.clone(),
            crate::service_save_vmm_memory_config(app.clone(), state.clone(), request(&before.vmm)),
        ),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(30), pause.entered.notified())
        .await
        .expect("identity review reaches preflight");
    // Update the exact project inserted above through the real APP persistence path.
    // 通过真实 APP 持久化路径更新上面插入的准确项目。
    let mut relocated = added.clone();
    relocated.title = "Renamed after native review".to_owned();
    relocated.path = path
        .with_file_name("relocated-binding-project")
        .display()
        .to_string();
    vulcan_workspace_store::workspace_store::persist_new_project(controller, &relocated)
        .await
        .expect("persist actual rename and relocation");
    crate::refresh_workbench_snapshot(state.clone())
        .await
        .expect("publish actual identity-preserving edit");
    pause.release.notify_one();
    pending
        .join_next()
        .await
        .expect("identity save task")
        .expect("identity task must not panic")
        .expect("native ID-preserving edit remains admissible");
    assert_eq!(
        data_plane
            .resolve_project_access(&added_id)
            .await
            .expect("host catalog review fixture value")
            .owner
            .id,
        project_id
    );
    assert_eq!(
        crate::provider_configuration_from_state(state)
            .expect("host catalog review fixture value")
            .vmm
            .enabled,
        Some(true)
    );
    {
        // Verify the actual retained catalog kept the concurrent rename and source path.
        // 验证实际保留目录维持并发修改后的名称及源码路径。
        let guard = state.lock().expect("lock host catalog review fixture");
        // Resolve only the known inserted stable identity, never whichever row is selected.
        // 仅解析已知插入的稳定身份，绝不使用当前选中的任意行。
        let actual = guard
            .workspace_snapshot
            .as_ref()
            .expect("host catalog review fixture value")
            .task_projects
            .iter()
            .find(|project| project.id == added_id)
            .expect("retained added project");
        assert_eq!(actual.title, relocated.title);
        assert_eq!(actual.path, relocated.path);
    }
    assert_configuration(path, state, &before);
    // Both disable/enable pairs publish once each; the rejected stale save publishes nothing.
    // 两组关闭/启用各发布一次；被拒绝的陈旧保存不发布。
    let captured = events
        .events
        .lock()
        .expect("lock host catalog review fixture");
    assert_eq!(
        captured
            .iter()
            .filter(|event| matches!(event, ServiceEvent::Workbench(_)))
            .count(),
        4
    );
    assert_eq!(
        captured
            .iter()
            .filter(|event| matches!(event,
                ServiceEvent::WorkbenchDelta(envelope) if matches!(&envelope.delta,
                    WorkbenchDelta::ProjectSettingsChanged(changed)
                    if changed.project_id.is_none() && changed.domain == ProjectSettingsDomain::Vmm)
            ))
            .count(),
        4
    );
}
