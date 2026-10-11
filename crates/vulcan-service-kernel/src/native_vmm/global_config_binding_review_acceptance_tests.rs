//! Real native binding changes between enable-plan review and global persistence.
//! 启用计划审查与全局持久化之间发生的真实原生绑定变化。

use super::*;
use crate::vmm::config_save_test_scope::{GLOBAL_SAVE_PREFLIGHT_PAUSE, GlobalSavePause};

/// Executes one actual native management action in its own task to bound the acceptance stack.
/// 在独立任务中执行一个实际原生管理动作，以限制验收调用栈。
/// Takes the live service, emitter, declared action and exact payload; returns after real service success.
/// 接收活动服务、发送器、已声明动作及准确载荷，在真实服务成功后返回。
async fn binding_action(
    state: &crate::SharedServiceBridgeState,
    app: &ServiceEventEmitter,
    action: &str,
    payload: serde_json::Value,
) {
    tokio::spawn(crate::service_run_vmm_memory_action(
        app.clone(),
        state.clone(),
        vulcan_service_api::VmmMemoryActionRequest {
            action: action.to_owned(),
            command_id: uuid::Uuid::new_v4().to_string(),
            scope: "global".to_owned(),
            project_id: None,
            conversation_id: None,
            payload,
        },
    ))
    .await
    .expect("management task must not panic")
    .expect("actual management binding action");
}

/// Reject a reviewed enable plan after a real management action unbinds another host.
/// 真实管理动作解绑另一个宿主后，拒绝已审查的旧启用计划。
/// Takes the actual service, private configuration path, real data plane and exact existing host/owner IDs.
/// 接收实际服务、私有配置路径、真实数据面以及准确已有宿主与所有者标识。
/// Returns after verifying rejection has no save side effects and a fresh explicit binding succeeds.
/// 验证拒绝没有保存副作用且新明确绑定成功后返回。
pub(super) async fn verify(
    state: &crate::SharedServiceBridgeState,
    path: &std::path::Path,
    data_plane: &VmmDataPlaneHandle,
    host_id: &str,
    project_id: u64,
) {
    // Keep this scenario's real save and management notifications independent from the enclosing suites.
    // 使此场景的真实保存及管理通知独立于外围测试组。
    let events = Arc::new(SaveEvents::default());
    // Both request kinds use the same actual service emitter.
    // 两种请求使用相同的实际服务发送器。
    let app = ServiceEventEmitter::new(vec![events.clone()]);
    // All current host projects have actual bindings when this scenario starts.
    // 此场景开始时，全部当前宿主项目已有实际绑定。
    let before = crate::provider_configuration_from_state(state).expect("binding review before");
    // An explicit plan requires complete unbound-project coverage even when resuming the same backend.
    // 即使恢复同一后端，显式计划也要求完整覆盖未绑定项目。
    let mut disable = request(&before.vmm);
    disable.draft.enabled = false;
    save(state, &app, path, disable)
        .await
        .expect("disable before binding review");
    binding_action(
        state,
        &app,
        "native_project_unbind",
        serde_json::json!({ "vulcanProjectId": host_id }),
    )
    .await;
    // The second host is another fixture project bound to the same owner, resolved from the live catalog.
    // 第二个宿主取自活动目录中另一个绑定到同一所有者的夹具项目。
    let host_ids = state
        .lock()
        .expect("lock native binding review fixture")
        .workspace_snapshot
        .as_ref()
        .expect("native binding review fixture value")
        .task_projects
        .iter()
        .map(|project| project.id.clone())
        .filter(|id| id != host_id)
        .collect::<Vec<_>>();
    let mut other_host_id = None;
    for id in host_ids {
        // Only a host already owned by the fixture owner can be restored by the same plan target.
        // 只有已归属夹具所有者的宿主才能由同一计划目标恢复。
        if data_plane
            .resolve_project_access(&id)
            .await
            .is_ok_and(|access| access.owner.id == project_id)
        {
            other_host_id = Some(id);
            break;
        }
    }
    let other_host_id = other_host_id.expect("fixture has another bound host project");
    // The reviewed plan covers exactly the one unbound host and genuinely passes native preflight.
    // 已审查计划恰好覆盖唯一未绑定宿主，并确实通过原生预检。
    let mut reviewed = request(&before.vmm);
    reviewed.binding_plan = vec![VmmProjectBindingPlanEntry {
        vulcan_project_id: host_id.to_owned(),
        action: "bind_existing".to_owned(),
        project_name: None,
        target_project_id: Some(project_id),
    }];
    let pause = Arc::new(GlobalSavePause::default());
    // Own the paused request so assertion failures cannot leave a detached writer behind.
    // 持有暂停请求，避免断言失败后遗留脱离管理的写入器。
    let mut pending = tokio::task::JoinSet::new();
    pending.spawn(PROVIDER_CONFIGURATION_PATH.scope(
        path.to_owned(),
        GLOBAL_SAVE_PREFLIGHT_PAUSE.scope(
            pause.clone(),
            crate::service_save_vmm_memory_config(app.clone(), state.clone(), reviewed),
        ),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(30), pause.entered.notified())
        .await
        .expect("reviewed plan passed actual preflight");
    binding_action(
        state,
        &app,
        "native_project_unbind",
        serde_json::json!({ "vulcanProjectId": other_host_id }),
    )
    .await;
    assert!(
        data_plane
            .resolve_project_access(&other_host_id)
            .await
            .is_err()
    );
    // Capture only after the independent management operation has completed its legitimate projections.
    // 仅在独立管理操作完成其合法投影后捕获状态。
    let disabled = crate::provider_configuration_from_state(state).expect("disabled after unbind");
    // A rejected stale save must preserve the complete physical configuration bytes.
    // 被拒绝的陈旧保存必须保持完整物理配置字节。
    let bytes = std::fs::read(path).expect("private disabled bytes");
    // Management may publish its own revision, but the rejected save must not advance it.
    // 管理操作可以发布自身版本，但被拒绝的保存不得推进它。
    let revision = state
        .lock()
        .expect("lock native binding review fixture")
        .workbench
        .revision;
    // Likewise preserve the event boundary after management has finished.
    // 同样保持管理完成后的事件边界。
    let event_count = events
        .events
        .lock()
        .expect("lock native binding review fixture")
        .len();
    pause.release.notify_one();
    // Inspect the final actual request result rather than an intermediate paused state.
    // 检查实际请求最终结果，而非中间暂停状态。
    let stale = pending
        .join_next()
        .await
        .expect("binding save handle")
        .expect("binding save task must not panic");
    assert_eq!(
        crate::provider_configuration_from_state(state)
            .expect("native binding review fixture value")
            .vmm
            .enabled,
        Some(false),
        "a reviewed plan must not enable VMM after another host becomes unbound"
    );
    assert!(
        stale
            .expect_err("unreviewed unbound host must reject activation")
            .contains("binding")
    );
    assert_eq!(
        std::fs::read(path).expect("native binding review fixture value"),
        bytes
    );
    assert_eq!(
        state
            .lock()
            .expect("lock native binding review fixture")
            .workbench
            .revision,
        revision
    );
    assert_eq!(
        events
            .events
            .lock()
            .expect("lock native binding review fixture")
            .len(),
        event_count
    );
    assert_configuration(path, state, &disabled);
    // The new user decision covers both unbound hosts by their exact stable identities.
    // 新用户决定按准确稳定身份覆盖两个未绑定宿主。
    let mut enable = request(&disabled.vmm);
    enable.draft.enabled = true;
    enable.binding_plan = [host_id, other_host_id.as_str()]
        .into_iter()
        .map(|id| VmmProjectBindingPlanEntry {
            vulcan_project_id: id.to_owned(),
            action: "bind_existing".to_owned(),
            project_name: None,
            target_project_id: Some(project_id),
        })
        .collect();
    save(state, &app, path, enable)
        .await
        .expect("fresh explicit binding review");
    for id in [host_id, other_host_id.as_str()] {
        assert_eq!(
            data_plane
                .resolve_project_access(id)
                .await
                .expect("native binding review fixture value")
                .owner
                .id,
            project_id
        );
    }
    assert_configuration(path, state, &before);
    verify_concurrent_binding_is_preserved(state, &app, path, data_plane, host_id, project_id)
        .await;
    // Two successful disable/enable pairs publish exactly once each; rejected saves emit no success event.
    // 两组成功关闭/启用各发布恰好一次；被拒绝的保存不发送成功事件。
    let captured = events
        .events
        .lock()
        .expect("lock native binding review fixture");
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

/// Accepts a covered plan without overwriting a newer binding chosen by the real management service.
/// 接受已覆盖的计划，同时不覆盖真实管理服务选择的较新绑定。
/// Takes the service, emitter, private path, actual data plane and exact original host/owner IDs.
/// 接收服务、发送器、私有路径、实际数据面以及准确原始宿主与所有者标识。
/// Returns after checking the newer binding and restoring the fixture's original owner explicitly.
/// 检查较新绑定并明确恢复夹具原始所有者后返回。
async fn verify_concurrent_binding_is_preserved(
    state: &crate::SharedServiceBridgeState,
    app: &ServiceEventEmitter,
    path: &std::path::Path,
    data_plane: &VmmDataPlaneHandle,
    host_id: &str,
    project_id: u64,
) {
    // Choose an actual different owner from this private fixture's native catalog, never a guessed numeric ID.
    // 从此私有夹具的原生目录选择实际不同所有者，绝不猜测数字标识。
    let alternative = data_plane
        .list_projects()
        .await
        .expect("actual native catalog")
        .into_iter()
        .find(|project| project.id != project_id)
        .expect("fixture has another existing owner");
    // Preserve the full Provider baseline while testing only native binding admission.
    // 测试原生绑定准入时保留完整 Provider 基线。
    let before =
        crate::provider_configuration_from_state(state).expect("before concurrent binding");
    // This enable request must explicitly cover the host that the management service first unbinds.
    // 此启用请求必须明确覆盖管理服务首先解绑的宿主。
    let mut disable = request(&before.vmm);
    disable.draft.enabled = false;
    save(state, app, path, disable)
        .await
        .expect("disable before concurrent binding");
    binding_action(
        state,
        app,
        "native_project_unbind",
        serde_json::json!({ "vulcanProjectId": host_id }),
    )
    .await;
    // The reviewed decision names the original owner, making an unwanted overwrite observable.
    // 已审查决定指定原始所有者，使不应发生的覆盖可被观察。
    let mut enable = request(&before.vmm);
    enable.binding_plan = vec![VmmProjectBindingPlanEntry {
        vulcan_project_id: host_id.to_owned(),
        action: "bind_existing".to_owned(),
        project_name: None,
        target_project_id: Some(project_id),
    }];
    // Pause after real coverage validation, while another explicit binding remains allowed.
    // 在真实覆盖校验后暂停，此时仍允许另一项明确绑定。
    let pause = Arc::new(GlobalSavePause::default());
    // Keep ownership of the pending writer throughout the concurrent management operation.
    // 在并发管理操作期间始终持有待完成写入器。
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
        .expect("explicit plan reaches preflight");
    binding_action(
        state,
        app,
        "native_project_bind_existing",
        serde_json::json!({ "vulcanProjectId": host_id, "vmmProjectId": alternative.id }),
    )
    .await;
    assert_eq!(
        data_plane
            .resolve_project_access(host_id)
            .await
            .expect("native binding review fixture value")
            .owner
            .id,
        alternative.id
    );
    pause.release.notify_one();
    pending
        .join_next()
        .await
        .expect("covered save task")
        .expect("covered save must not panic")
        .expect("a concurrently bound host is still covered");
    assert_eq!(
        data_plane
            .resolve_project_access(host_id)
            .await
            .expect("native binding review fixture value")
            .owner
            .id,
        alternative.id
    );
    assert_eq!(
        crate::provider_configuration_from_state(state)
            .expect("native binding review fixture value")
            .vmm
            .enabled,
        Some(true)
    );
    assert_configuration(path, state, &before);
    binding_action(
        state,
        app,
        "native_project_bind_existing",
        serde_json::json!({ "vulcanProjectId": host_id, "vmmProjectId": project_id }),
    )
    .await;
    assert_eq!(
        data_plane
            .resolve_project_access(host_id)
            .await
            .expect("native binding review fixture value")
            .owner
            .id,
        project_id
    );
}
