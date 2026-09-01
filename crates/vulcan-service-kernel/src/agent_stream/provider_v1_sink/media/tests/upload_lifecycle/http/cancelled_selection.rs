//! Original upload choice survives cancellation after a real request but before any receipt exists.
//! 真实请求已发送但尚无回执时取消，原上传选择仍保留。

use super::*;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission,
};
use vulcan_session_core::v1::{
    ClientActionId, ClientInputSubmission, LockedBaselineProjection, TurnOutcome,
    event::{ApiPurposeKind, ApiRequestSnapshot, EventBody},
};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    create_session_v1_storage_fixture, load_session_v1_full,
};

/// Cancels a real upload before its response and verifies prompt-owned selection, real cancellation progress and no B/A automatic upload.
/// 在真实上传响应前取消，并验证提示词所有选择、真实取消进度及B/A均不自动上传。
#[tokio::test]
async fn v1_https_cancel_before_receipt_preserves_upload_choice()
-> Result<(), Box<dyn std::error::Error>> {
    // The fixture owns all files and the V1 session, not a real user project.
    // 夹具持有全部文件及V1会话，不使用真实用户项目。
    let directory = tempfile::tempdir()?;
    // Stable owner identity is shared by the request and private receipt scope.
    // 稳定所有者身份由请求及私有回执作用域共享。
    let session_id = SessionId::new_v4();
    // Real storage layout supplies the authoritative project partition.
    // 真实存储布局提供权威项目分区。
    let locator = create_session_v1_storage_fixture(directory.path(), &session_id.to_string(), 1)?;
    // Use the locator's project path, never a guessed sibling directory.
    // 使用定位器项目路径，绝不猜测同级目录。
    let project = locator.project_directory();
    // The existing catalog owner validates source identity and any private receipt writes.
    // 既有目录所有者校验来源身份及任何私有回执写入。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Canonical source bytes are actually transmitted by the test's file-create request.
    // 规范来源字节由测试的文件创建请求实际传输。
    let entry = register(&project, &registry, b"abc")?;
    // Capture and eventual Stop share this unique production session owner.
    // 捕获及最终Stop共享本唯一生产会话所有者。
    let handle = SessionHandle::new(
        session_id,
        locator.clone(),
        load_session_v1_full(&locator)?.document,
    );
    handle.submit_root_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("https-cancel-selection")?,
            "inspect pending image",
            vec![entry.resource_id],
        ),
        2,
    )?;
    // The server signals that the real body arrived before cancellation is attempted.
    // 服务器在尝试取消之前通知真实正文已到达。
    let (received_tx, received_rx) = tokio::sync::oneshot::channel();
    // Dropping this sender abandons the response after the client future has been cancelled.
    // 客户端Future取消后，丢弃本发送端以放弃响应。
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    // TLS and URL restrictions remain identical to all other real upload fixtures.
    // TLS及URL限制保持与其他真实上传夹具相同。
    let (http, server) = https_upload_fixture(
        200,
        r#"{"id":"never-returned-file"}"#,
        Some(UploadReplyGate {
            request_received: received_tx,
            release_reply: reply_rx,
        }),
    )
    .await?;
    // Record only the actual runtime progress phases, including the cancellation emitted by Future drop.
    // 仅记录实际运行时进度阶段，包括Future丢弃发出的取消。
    let phases = Arc::new(std::sync::Mutex::new(Vec::new()));
    // The observer's lifetime spans the cancelled physical preparation.
    // 观察器生命周期跨越被取消的物理准备。
    let observed = Arc::clone(&phases);
    // A is frozen before the real upload and uses the same exact session identity.
    // A在真实上传前冻结，并使用相同精确会话身份。
    let (mut first, first_secrets) = context_with_http(
        false,
        true,
        Some(Arc::new(move |event| {
            observed.lock().expect("phases").push(event.phase)
        })),
        Some(http),
    )
    .await?;
    first.session_scope_id = session_id.to_string();
    // The selected request route is explicit rather than inferred from a cached provider reference.
    // 所选请求路线显式给定，而非从缓存供应商引用推断。
    let route = original_route(&first);
    // Bind the API candidate to its exact model, connection and protocol.
    // 将API候选绑定到其精确模型、连接及协议。
    let request = ApiRequestSnapshot {
        provider: route.provider_id.clone(),
        profile: Some(route.protocol_profile_id.clone()),
        model: route.model_id.clone(),
        connection_id: Some(route.connection_id.clone()),
        thinking_level: None,
        temperature: None,
    };
    // The parent task will be aborted exactly as the production stream owner is aborted by Stop.
    // 父任务将与生产流所有者被Stop中止一样被中止。
    let preparing_handle = Arc::clone(&handle);
    // Keep a separate frozen context for post-cancellation restoration checks.
    // 保留独立冻结上下文，以在取消后检查恢复。
    let preparing_context = first.clone();
    // All preparation and later assertions address the same catalog instance.
    // 全部准备及后续断言针对同一目录实例。
    let preparing_registry = Arc::clone(&registry);
    // Real source verification, pre-upload selection persistence and HTTP transfer occur inside this future.
    // 真实来源校验、上传前选择持久化及HTTP传输在本Future中发生。
    let preparing = tokio::spawn(async move {
        preparing_handle
            .prepare_physical_api_durable::<(), _>(
                ApiPurposeKind::MainGeneration,
                request,
                LockedBaselineProjection::empty(),
                None,
                4,
                move |snapshot, preparation| async move {
                    prepare_v1_user_media_durable(
                        &snapshot,
                        Some(&preparing_context),
                        preparing_registry,
                        preparation,
                    )
                    .await?;
                    Ok(None)
                },
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), received_rx).await??;
    // Before cancellation and before any reply, the original choice must already be physically durable.
    // 取消前及任何回复前，原选择必须已物理可靠落盘。
    let loaded = load_session_v1_full(&locator)?;
    // The root prompt owns the selected upload, not an API-start record or file-global latest cache.
    // 根提示词持有所选上传，而非API起始记录或文件全局最近缓存。
    let EventBody::UserInput(input) = &loaded.document.events()[0].event else {
        panic!("root")
    };
    // Retain the exact selected proof for comparisons after Stop and model switches.
    // 保留精确选择证明，以在Stop及模型切换后比较。
    let selected = input
        .parameters
        .as_ref()
        .expect("parameters")
        .original_resource(0)
        .expect("selected upload")
        .clone();
    assert_eq!(selected.original_route, route);
    assert!(matches!(
        selected.delivery,
        UserPromptResourceDeliveryKind::UploadPending { .. }
    ));
    assert!(
        load_project_resource_catalog(&project)?.entries[0]
            .provider_bindings
            .is_empty()
    );
    preparing.abort();
    assert!(
        preparing
            .await
            .expect_err("upload parent cancelled")
            .is_cancelled()
    );
    drop(reply_tx);
    assert_eq!(server.await?["content_base64"], "YWJj");
    // Use the same settlement gate as Stop before closing the original turn.
    // 在闭合原回合之前，使用与Stop相同的结算门。
    let settlement = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handle.acquire_request_preparation_settlement(),
    )
    .await?;
    // Use actual host time after the completed durable selection, preserving the real timestamp error if conversion fails.
    // 在可靠选择之后使用实际宿主时间，转换失败时保留真实时间戳错误。
    let stop_at_ms = i64::try_from(crate::session_runtime_kernel::current_unix_millis_u64())?;
    handle.mutate_durable(
        SessionMutationAdmission::ExistingTurn,
        SessionFlushReason::TurnCompleted,
        |document| document.close_turn(TurnOutcome::Stopped, 0, stop_at_ms),
    )?;
    drop(settlement);
    assert_eq!(
        phases.lock().expect("phases").last(),
        Some(&RuntimeMediaUploadPhase::Cancelled)
    );
    assert!(
        !phases
            .lock()
            .expect("phases")
            .contains(&RuntimeMediaUploadPhase::Completed)
    );
    // A later unrelated receipt for the same file cannot be guessed as this cancelled occurrence's missing result.
    // 同一文件的较晚无关回执不能被猜作本取消出现所缺结果。
    seed_receipt(
        &project,
        &registry,
        &entry,
        &first,
        "unrelated-later-file",
        false,
    )?;
    // Replay obtains real serialized parameters from the original session, not a fabricated selected-state fixture.
    // 重放从原会话取得真实序列化参数，而非伪造选择状态夹具。
    let replay = handle.capture_context_compaction_planning_snapshot()?;
    // Only the original attempted upload may have consumed A's credentials.
    // 只有原尝试上传可能消耗A的凭据。
    let first_reads = first_secrets.reads.load(Ordering::SeqCst);
    for has_upload in [false, true] {
        // B is tested both with and without its own upload capability.
        // 分别测试B具有及不具有自身上传能力。
        let (mut second, second_secrets) = context(true, has_upload).await?;
        second.session_scope_id = session_id.to_string();
        // Any new authenticated upload would change this counter.
        // 任何新的认证上传都会改变本计数。
        let reads = second_secrets.reads.load(Ordering::SeqCst);
        // The real current projection also supplies canonical metadata to the actual composer planner.
        // 真实当前投影也向实际输入区规划器提供规范元数据。
        let view = prepare_v1_user_media(&replay, Some(&second), Arc::clone(&registry)).await?;
        assert!(metadata(&view, 0).contains("another model"));
        assert_eq!(
            crate::media_upload::test_composer_project_decision(
                &second.execution,
                &view.prompts[&0].attachments[0],
                &entry,
                &vulcan_session_core::v1::UserPromptResourcePreparation::Prepared(selected.clone()),
                &second.session_scope_id
            )?,
            (
                vulcan_service_api::ComposerAttachmentDeliveryPlan::MetadataOnly,
                "original_upload_belongs_to_another_model"
            )
        );
        assert_eq!(second_secrets.reads.load(Ordering::SeqCst), reads);
    }
    // Returning to A still needs the exact missing receipt, not the unrelated one seeded above.
    // 返回A仍需要精确缺失回执，而非上面加入的无关回执。
    let original_view = prepare_v1_user_media(&replay, Some(&first), registry).await?;
    assert!(
        metadata(&original_view, 0)
            .contains(MediaDeliveryRejection::OriginalReferenceUnavailable.explanation())
    );
    assert_eq!(
        crate::media_upload::test_composer_project_decision(
            &first.execution,
            &original_view.prompts[&0].attachments[0],
            &entry,
            &vulcan_session_core::v1::UserPromptResourcePreparation::Prepared(selected.clone()),
            &first.session_scope_id
        )?,
        (
            vulcan_service_api::ComposerAttachmentDeliveryPlan::MetadataOnly,
            "original_upload_reference_unavailable"
        )
    );
    assert_eq!(first_secrets.reads.load(Ordering::SeqCst), first_reads);
    // No stale API or guessed completion was introduced by cancellation or either model view.
    // 取消及两个模型视图均未引入陈旧API或猜测完成。
    let restored = load_session_v1_full(&locator)?;
    assert!(
        !restored
            .document
            .events()
            .iter()
            .any(|event| matches!(event.event, EventBody::ApiStarted(_)))
    );
    // Re-read the same original event after every projection to detect accidental history mutation.
    // 在全部投影之后重读相同原事件，以检测意外历史修改。
    let EventBody::UserInput(input) = &restored.document.events()[0].event else {
        panic!("root")
    };
    assert_eq!(
        input
            .parameters
            .as_ref()
            .expect("parameters")
            .original_resource(0),
        Some(&selected)
    );
    Ok(())
}
