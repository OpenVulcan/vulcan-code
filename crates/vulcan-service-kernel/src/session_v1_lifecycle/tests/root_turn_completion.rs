//! First-session publication must permit real logical completion and a second user prompt.
//! 首次会话发布必须支持真实逻辑结束及第二条用户提示。

use super::*;
use crate::session_runtime_registry_v1::SuccessfulTurnSettlement;
use vulcan_session_core::v1::{
    UsageSnapshot,
    collaboration::CollaborationRunControl,
    document::ApiCompletion,
    event::{ApiEndReason, ApiTimings, ContinuationEligibility},
};

/// Creates through the production lifecycle, completes two API/turn pairs, and verifies durable reload.
/// 经生产生命周期创建会话，完成两次 API 及回合结算，并验证可靠重载。
/// Uses isolated storage without a provider; a deferred first completion reproduces the reported admission failure.
/// 使用隔离存储、不请求供应商；首次完成被延后即可复现报告中的准入失败。
#[tokio::test]
async fn new_root_completes_and_accepts_second_prompt() {
    // Fixture and registry use the same creation/publication path as a new WebUI conversation.
    // 夹具及注册表使用新建网页会话相同的创建与发布路径。
    let fixture = LifecycleStorageFixture::start().await;
    let registry = SessionRuntimeRegistry::default();
    let session_id = SessionId::new_v4();
    let handle = create_fixture_session(&fixture, &registry, session_id).await;
    // Explicit times exclude timing races; each iteration owns a distinct physical API and logical turn.
    // 显式时间排除时序竞争；每次循环拥有不同物理 API 和逻辑回合。
    for now_ms in [1_787_788_800_010, 1_787_788_800_020] {
        let request = ApiRequestSnapshot::new("provider", "model");
        let preparation = handle
            .read(|document| lifecycle_test_api_input(document, request.clone(), now_ms))
            .expect("prepare current logical turn");
        let api_call_id = handle
            .prepare_and_start_api_durable(preparation)
            .expect("publish physical API")
            .value
            .api_call_id;
        handle
            .update_assistant_text(
                api_call_id,
                "completed reply",
                AssistantSemanticStatus::Completed,
                now_ms + 1,
            )
            .expect("publish completed assistant");
        handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                |document| {
                    document.complete_api_group(
                        api_call_id,
                        ApiCompletion {
                            request,
                            usage: UsageSnapshot::default(),
                            status: ApiCallStatus::Completed,
                            end_reason: Some(ApiEndReason::CompletedSuccessfully),
                            error: None,
                            continuation_eligibility: ContinuationEligibility::Forbidden,
                            timing: ApiTimings {
                                completed_at_ms: Some(now_ms + 2),
                                ..ApiTimings::default()
                            },
                            provider_source: None,
                            completed_at_ms: now_ms + 2,
                        },
                    )
                },
            )
            .expect("publish successful API terminal");
        let receipt = handle
            .complete_successful_turn(now_ms + 2)
            .await
            .expect("settle successful turn");
        assert!(
            matches!(receipt.value, SuccessfulTurnSettlement::Completed(_)),
            "A newly created ordinary session must close after its final reply, not defer while naturally idle"
        );
        assert_eq!(
            handle
                .read(|document| document.collaboration_run_control())
                .unwrap(),
            CollaborationRunControl::NaturallyIdle
        );
        // Reload the actual persisted document before admitting another prompt, with no repair or manual state override.
        // 接纳下一提示前重载真实持久化文档，不进行修复或手工覆盖状态。
        let persisted = load_session_v1_tail(&handle.storage_locator().unwrap())
            .expect("reload completed session");
        assert!(
            persisted
                .document
                .active_turn()
                .is_none_or(|turn| turn.closed)
        );
        handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new(format!("next-{now_ms}")).unwrap(),
                    "next prompt",
                    vec![],
                ),
                now_ms + 3,
            )
            .expect("accept prompt after successful turn");
        assert_eq!(
            handle
                .read(|document| document.collaboration_run_control())
                .unwrap(),
            CollaborationRunControl::Running
        );
    }
    fixture.shutdown().await;
}
