//! Durable Continue identity regression at the physical request preparation boundary.
//! 物理请求准备边界上的可靠 Continue 身份回归。

use super::*;

/// Verifies a changed followup requires a fresh API while exact Continue replay remains idempotent.
/// 验证变化后的后续请求需要新 API，而精确 Continue 重放仍保持幂等。
/// Takes no arguments; returns no value and uses only an isolated on-disk session, without a provider.
/// 无参数、无返回值；仅使用隔离的磁盘会话，不调用供应商。
#[tokio::test]
async fn continued_followup_preserves_immutable_request_guard() {
    // directory keeps the real test store alive; handle is its unique durable mutation owner.
    // directory 保持真实测试存储存活；handle 是其唯一可靠修改所有者。
    let (_directory, handle, request) = preparation_fixture();
    // initial starts a genuine physical API so the subsequent Continue is admitted by the domain.
    // initial 启动真实物理 API，使后续 Continue 能通过领域准入。
    let initial = handle
        .prepare_physical_api_durable(
            ApiPurposeKind::MainGeneration,
            request.clone(),
            LockedBaselineProjection::empty(),
            None,
            3,
            |snapshot, _preparation| async move { Ok(Some(prepared_fixture(snapshot))) },
        )
        .await
        .expect("prepare initial API")
        .expect("initial frontier is current");
    // failure permits the explicit user Continue after the first request fails.
    // failure 允许用户在首个请求失败后显式继续。
    let mut failure = retryable_network_completion(4);
    failure.request = request.clone();
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| document.complete_api_group(initial.value.core_start.api_call_id, failure),
        )
        .expect("complete failed initial API");
    // action is the sole admitted Continue identity and must remain bound to only one API.
    // action 是唯一获准的 Continue 身份，必须始终只绑定一个 API。
    let action =
        ClientActionId::try_new("continue-followup-action").expect("valid Continue action");
    handle
        .continue_turn(ContinuationRequest::new(action.clone(), "", Vec::new()), 5)
        .expect("admit Continue");
    // continued records the exact original request and projection before any host restart.
    // continued 在任何宿主重启前记录精确原请求与投影。
    let continued = handle
        .prepare_physical_api_durable(
            ApiPurposeKind::Continue,
            request.clone(),
            LockedBaselineProjection::empty(),
            Some(action.clone()),
            6,
            |snapshot, _preparation| async move { Ok(Some(prepared_fixture(snapshot))) },
        )
        .await
        .expect("prepare continued API")
        .expect("Continue frontier is current");
    handle
        .mark_api_transport_observed_durable(continued.value.core_start.api_call_id, 7)
        .await
        .expect("record Continue transport");
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    continued.value.core_start.api_call_id,
                    ApiCompletion {
                        request: request.clone(),
                        usage: UsageSnapshot::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings::default(),
                        provider_source: None,
                        completed_at_ms: 8,
                    },
                )
            },
        )
        .expect("complete continued API");
    // starts is captured by event type, so assertions never depend on evolving event positions.
    // starts 按事件类型捕获，使断言不依赖会演进的事件位置。
    let starts = started_count(&handle);
    // replay proves that the fix does not remove legitimate idempotency for identical requests.
    // replay 证明修复没有取消相同请求的合法幂等行为。
    let replay = handle
        .prepare_physical_api_durable(
            ApiPurposeKind::Continue,
            request.clone(),
            LockedBaselineProjection::empty(),
            Some(action.clone()),
            9,
            |snapshot, _preparation| async move { Ok(Some(prepared_fixture(snapshot))) },
        )
        .await
        .expect("exact replay remains idempotent")
        .expect("replay frontier is current");
    assert!(!replay.changed);
    assert_eq!(
        replay.value.core_start.api_call_id,
        continued.value.core_start.api_call_id
    );
    assert_eq!(started_count(&handle), starts);

    // drift reproduces the reported error if a rebuilt request incorrectly retains the consumed action.
    // drift 复现重建请求错误保留已消费动作时报告的错误。
    let drift = handle
        .prepare_physical_api_durable(
            ApiPurposeKind::Continue,
            request.clone(),
            LockedBaselineProjection::empty(),
            Some(action),
            10,
            |snapshot, _preparation| async move {
                // prepared models the changed provider projection after a tool or compaction boundary.
                // prepared 模拟工具或压缩边界之后变化的供应商投影。
                let mut prepared = prepared_fixture(snapshot);
                prepared.projection.fingerprint_sha256 = "b".repeat(64);
                Ok(Some(prepared))
            },
        )
        .await;
    assert!(matches!(drift, Err(SessionRuntimeError::Domain(error))
    if matches!(*error, DomainError::ImmutableFieldMutation {
        entity: "api_started", field: "idempotent_request_drift"
    })));
    assert_eq!(started_count(&handle), starts);

    // followup uses the same changed projection with the consumed action removed by the host handoff.
    // followup 使用相同变化后的投影，但由宿主交接清除已消费动作。
    let followup = handle
        .prepare_physical_api_durable(
            ApiPurposeKind::MainGeneration,
            request,
            LockedBaselineProjection::empty(),
            None,
            11,
            |snapshot, _preparation| async move {
                // prepared retains the changed projection rather than replaying stale request content.
                // prepared 保留变化后的投影，而不重放旧请求内容。
                let mut prepared = prepared_fixture(snapshot);
                prepared.projection.fingerprint_sha256 = "b".repeat(64);
                Ok(Some(prepared))
            },
        )
        .await
        .expect("changed followup must create a fresh API")
        .expect("followup frontier is current");
    assert!(followup.changed);
    assert_ne!(
        followup.value.core_start.api_call_id,
        continued.value.core_start.api_call_id
    );
    assert_eq!(started_count(&handle), starts + 1);
    // loaded verifies the new identity is physically persisted, not merely accepted in memory.
    // loaded 验证新身份已经物理持久化，而非仅在内存中被接受。
    let loaded = load_session_v1_full(&handle.storage_locator().expect("fixture locator"))
        .expect("reload durable followup");
    assert_eq!(
        loaded
            .document
            .active_api()
            .expect("persisted followup")
            .api_call_id,
        followup.value.core_start.api_call_id
    );
}
