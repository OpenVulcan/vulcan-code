//! Real global-save preflight races with credential rotation and backend disablement.
//! 凭据轮换及后端关闭期间真实全局保存预检的竞争验收。

use super::*;
use crate::vmm::config_save_test_scope::{GLOBAL_SAVE_PREFLIGHT_PAUSE, GlobalSavePause};

/// Verify stale preflight cannot overwrite newer credentials or bypass enable-time binding decisions.
/// 验证陈旧预检不能覆盖较新凭据或绕过启用时绑定决定。
/// Takes the live service, private path and actual native project/session; uses unmodified service responses.
/// 接收活动服务、私有路径及实际原生项目和会话；使用未经修改的服务响应。
pub(super) async fn verify(
    state: &crate::SharedServiceBridgeState,
    path: &std::path::Path,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    host_id: &str,
    project_id: u64,
    session_id: uuid::Uuid,
) {
    // Capture this suite's real service events independently of the surrounding save scenarios.
    // 独立于外围保存场景捕获本组真实服务事件。
    let events = Arc::new(SaveEvents::default());
    // This emitter is passed unchanged through every real save in the preflight suite.
    // 此发送器不经改变地传递给预检组中的每次真实保存。
    let app = ServiceEventEmitter::new(vec![events.clone()]);
    // Credential rotation and disablement each invalidate a different preflight assumption.
    // 凭据轮换及关闭分别使不同的预检依据失效。
    for disable_after_preflight in [false, true] {
        // The actual live configuration is the old request's preflight authority.
        // 实际活动配置是旧请求的预检权威依据。
        let before = crate::provider_configuration_from_state(state).expect("preflight before");
        assert_eq!(before.vmm.enabled, Some(true));
        // This request changes only tuning; both secret fields retain their canonical redacted placeholders.
        // 此请求只调参；两个秘密字段均保留规范脱敏占位符。
        let mut stale = request(&before.vmm);
        stale.draft.profile_refresh_turns = "11".to_owned();
        assert_eq!(
            stale.draft.grpc_api_key,
            vulcan_service_api::WEBUI_REDACTED_SECRET_PLACEHOLDER
        );
        assert_eq!(
            stale.draft.management_bearer_token,
            vulcan_service_api::WEBUI_REDACTED_SECRET_PLACEHOLDER
        );
        // Only the stale request pauses, after all real preflight work but before durable mutation.
        // 只暂停旧请求，位置在全部真实预检工作之后、持久修改之前。
        let pause = Arc::new(GlobalSavePause::default());
        // Own the paused task so assertion failure cannot leave a detached configuration writer.
        // 持有暂停任务，防止断言失败后留下脱离的配置写入器。
        let mut pending = tokio::task::JoinSet::new();
        pending.spawn(PROVIDER_CONFIGURATION_PATH.scope(
            path.to_owned(),
            GLOBAL_SAVE_PREFLIGHT_PAUSE.scope(
                pause.clone(),
                crate::service_save_vmm_memory_config(app.clone(), state.clone(), stale),
            ),
        ));
        tokio::time::timeout(std::time::Duration::from_secs(30), pause.entered.notified())
            .await
            .expect("stale save reaches preflight boundary");
        // The peer changes current credentials or disables native memory through the same actual save service.
        // 另一请求通过同一真实保存服务修改当前凭据或关闭原生记忆。
        let mut newer = request(&before.vmm);
        if disable_after_preflight {
            newer.draft.enabled = false;
        } else {
            newer.draft.grpc_api_key = "rotated-isolated-grpc-secret".to_owned();
            newer.draft.management_bearer_token = "rotated-isolated-management-secret".to_owned();
        }
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            save(state, &app, path, newer),
        )
        .await
        .expect("peer save must not be blocked by preflight")
        .expect("peer save");
        if disable_after_preflight {
            maintenance
                .unbind_project(host_id)
                .await
                .expect("explicit unbind while disabled");
        }
        // Freeze the exact newer document, bytes and publication counters before resuming the stale request.
        // 恢复旧请求前冻结准确的新文档、字节及发布计数。
        let newer_configuration =
            crate::provider_configuration_from_state(state).expect("newer authority");
        // The real file bytes detect any unintended write, even if runtime state is later repaired.
        // 真实文件字节可检测任何非预期写入，即使运行时随后被修复。
        let newer_bytes = std::fs::read(path).expect("newer file");
        // Rejected preflight must not advance the workbench or emit success invalidations.
        // 被拒绝的预检不得推进工作台或发送成功失效通知。
        let newer_revision = state
            .lock()
            .expect("lock VMM preflight fixture")
            .workbench
            .revision;
        // Keep the exact synchronous emission count before the old request is allowed to return.
        // 在允许旧请求返回前保留准确同步发送数量。
        let newer_event_count = events
            .events
            .lock()
            .expect("lock VMM preflight fixture")
            .len();
        pause.release.notify_one();
        // Retain the real result so data preservation is checked independently from the error contract.
        // 保留真实结果，使数据保留检查独立于错误契约。
        let stale_result = pending
            .join_next()
            .await
            .expect("stale save task")
            .expect("stale save task must not panic");
        // Read authority after the old request actually finishes, not while it is still paused.
        // 在旧请求真正完成后读取权威状态，而不是在其仍暂停时读取。
        let after = crate::provider_configuration_from_state(state).expect("after stale save");
        assert_eq!(
            after.vmm.grpc_api_key, newer_configuration.vmm.grpc_api_key,
            "a redacted old draft must not restore a rotated credential"
        );
        assert_eq!(
            after.vmm.management_bearer_token,
            newer_configuration.vmm.management_bearer_token
        );
        assert_eq!(after.vmm, newer_configuration.vmm);
        assert_eq!(
            std::fs::read(path).expect("VMM preflight fixture value"),
            newer_bytes
        );
        assert_eq!(
            state
                .lock()
                .expect("lock VMM preflight fixture")
                .workbench
                .revision,
            newer_revision
        );
        assert_eq!(
            events
                .events
                .lock()
                .expect("lock VMM preflight fixture")
                .len(),
            newer_event_count
        );
        assert!(
            stale_result
                .expect_err("stale preflight must be rejected")
                .contains("reload")
        );
        assert_configuration(path, state, &before);
        if disable_after_preflight {
            assert!(
                maintenance
                    .prepare_recall_publication(Some(host_id), &session_id.to_string())
                    .await
                    .is_err()
            );
            // Restoring native operation still requires the user's exact missing-project binding decision.
            // 恢复原生运行仍要求用户对准确缺失项目作出绑定决定。
            let mut restored = request(&after.vmm);
            restored.draft.enabled = true;
            restored.binding_plan = vec![VmmProjectBindingPlanEntry {
                vulcan_project_id: host_id.to_owned(),
                action: "bind_existing".to_owned(),
                project_name: None,
                target_project_id: Some(project_id),
            }];
            save(state, &app, path, restored)
                .await
                .expect("reviewed enable with exact binding");
        }
    }
    verify_unrelated_provider_edit(state, path, &app).await;
    // Two peer saves, explicit re-enable and a still-valid tuning request each publish once.
    // 两个较新保存、明确重新启用及仍然有效的调参请求各发布一次。
    let captured = events.events.lock().expect("lock VMM preflight fixture");
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

/// Verify non-VMM configuration edits remain independent and survive a concurrent valid VMM save.
/// 验证非 VMM 配置编辑保持独立，并在并发有效 VMM 保存后仍然保留。
/// Takes the actual state, private file and emitter; uses the real shared Provider transaction for the peer edit.
/// 接收实际状态、私有文件及发送器；另一项编辑使用真实共享 Provider 事务。
async fn verify_unrelated_provider_edit(
    state: &crate::SharedServiceBridgeState,
    path: &std::path::Path,
    app: &ServiceEventEmitter,
) {
    // Freeze the exact preflight document before changing only another Provider-owned field.
    // 仅改变另一个 Provider 所属字段前，冻结准确预检文档。
    let before = crate::provider_configuration_from_state(state).expect("unrelated before");
    // VMM tuning must remain admissible because its own authoritative layer will not change.
    // VMM 调参必须仍可准入，因为其自身权威配置层不会变化。
    let mut tuning = request(&before.vmm);
    tuning.draft.profile_refresh_turns = "19".to_owned();
    // Pause at the same real preflight boundary used by conflicting VMM edits.
    // 在冲突 VMM 编辑使用的同一真实预检边界暂停。
    let pause = Arc::new(GlobalSavePause::default());
    // Retain request ownership and abort on failure rather than leaving an unobserved writer.
    // 保留请求所有权并在失败时中止，而非留下无人观察的写入器。
    let mut pending = tokio::task::JoinSet::new();
    pending.spawn(PROVIDER_CONFIGURATION_PATH.scope(
        path.to_owned(),
        GLOBAL_SAVE_PREFLIGHT_PAUSE.scope(
            pause.clone(),
            crate::service_save_vmm_memory_config(app.clone(), state.clone(), tuning),
        ),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(30), pause.entered.notified())
        .await
        .expect("valid tuning preflight");
    // This existing field belongs to Provider request diagnostics, not to the VMM configuration layer.
    // 此既有字段属于 Provider 请求诊断，不属于 VMM 配置层。
    let next_trace_enabled = !before.provider_request_trace_enabled;
    PROVIDER_CONFIGURATION_PATH
        .scope(path.to_owned(), async {
            crate::mutate_provider_configuration(
                state,
                "保存测试请求诊断设置失败",
                |configuration| {
                    configuration.provider_request_trace_enabled = next_trace_enabled;
                    Ok(())
                },
                |_, _| (),
            )
            .await
            .expect("independent Provider transaction");
        })
        .await;
    pause.release.notify_one();
    // The VMM request must complete normally instead of treating every Provider edit as a conflict.
    // VMM 请求必须正常完成，不能把每项 Provider 编辑都当作冲突。
    let response = pending
        .join_next()
        .await
        .expect("valid tuning handle")
        .expect("valid tuning task")
        .expect("unrelated edit must not invalidate VMM preflight");
    assert_eq!(response.vmm_memory.global_draft.profile_refresh_turns, "19");
    // Compare the real live authority and persisted document after both transactions have completed.
    // 两个事务完成后，比较真实活动权威及持久文档。
    let after = crate::provider_configuration_from_state(state).expect("after independent save");
    assert_eq!(after.provider_request_trace_enabled, next_trace_enabled);
    assert_eq!(after.vmm.profile_refresh_turns, Some(19));
    // All unrelated fields remain equal except the one explicitly changed by the independent transaction.
    // 除独立事务明确修改的一个字段外，全部无关字段保持相等。
    let mut expected = before;
    expected.provider_request_trace_enabled = next_trace_enabled;
    assert_configuration(path, state, &expected);
}
