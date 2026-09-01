//! Actual HTTPS completion after draft promotion and the first B-model request.
//! 草稿提升及首次B模型请求之后的实际HTTPS完成。

use super::*;
use crate::session_runtime_registry_v1::{
    SessionHandle, SessionPreparedPhysicalRequest, test_provider_request_budget_evidence,
};
use vulcan_agent_runtime::{
    attachment_asset_index::{
        AttachmentDraftMediaDelivery, AttachmentDraftMediaOrigin, acquire_staged_attachment_ref,
        complete_attachment_draft_upload, promote_staged_attachment_to_message_ref,
        record_attachment_draft_media, register_attachment_draft_upload_handoff,
    },
    attachment_assets::{attachment_descriptor_from_mime_type, stage_attachment_bytes},
};
use vulcan_session_core::v1::{
    ClientActionId, ClientInputSubmission, LockedBaselineProjection, ProviderProjectionProof,
    UserPromptResourcePreparation,
    event::{ApiPurposeKind, ApiRequestSnapshot, EventBody},
};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    create_session_v1_storage_fixture, load_session_v1_full,
};

/// Holds a real upload reply across durable prompt admission and B's first API, then restores only A's original reference without rewriting history.
/// 跨可靠提示词准入及B首次API暂缓真实上传回复，随后只恢复A原引用而不改写历史。
#[tokio::test]
async fn draft_https_late_completion_restores_frozen_original_prompt()
-> Result<(), Box<dyn std::error::Error>> {
    // Real isolated source and session directories remain alive through completion and disk reload.
    // 真实隔离来源及会话目录在完成及磁盘重载期间保持存活。
    let workspace = tempfile::tempdir()?;
    let directory = tempfile::tempdir()?;
    let session_id = SessionId::new_v4();
    let locator = create_session_v1_storage_fixture(directory.path(), &session_id.to_string(), 1)?;
    let project = locator.project_directory();
    let handle = SessionHandle::new(
        session_id,
        locator.clone(),
        load_session_v1_full(&locator)?.document,
    );
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let scope = "composer-draft:v1:new:late-upload";
    let descriptor = attachment_descriptor_from_mime_type("image/png").ok_or("image descriptor")?;
    let attachment = stage_attachment_bytes(
        &workspace.path().to_string_lossy(),
        &descriptor,
        "source.png",
        None,
        b"abc",
    )?;
    acquire_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        scope,
        std::slice::from_ref(&attachment),
    )?;
    // The real server exposes a body-complete boundary, not a sleep-based race.
    // 真实服务器暴露正文完成边界，而非基于睡眠的竞争。
    let (received_tx, received_rx) = tokio::sync::oneshot::channel();
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let (http, server) = https_upload_fixture(
        200,
        r#"{"id":"file-draft-original-A"}"#,
        Some(UploadReplyGate {
            request_received: received_tx,
            release_reply: reply_rx,
        }),
    )
    .await?;
    let (mut first, first_secrets) = context_with_http(false, true, None, Some(http)).await?;
    first.session_scope_id = session_id.to_string();
    let contract = first
        .execution
        .client
        .conversation_media_upload_contract(
            &first.execution.route,
            ResourceKind::Image,
            "image/png",
        )?
        .ok_or("upload contract")?;
    let original = record_attachment_draft_media(
        &attachment,
        scope,
        AttachmentDraftMediaOrigin::new(
            &attachment,
            original_route(&first),
            AttachmentDraftMediaDelivery::Upload {
                upload_channel_id: contract.upload_channel_id.clone(),
                upload_contract_fingerprint: contract.contract_fingerprint.clone(),
                receipt: None,
            },
        )?,
    )?;
    // Existing source registration fixes the exact catalog entry; it is not an upload completion or a prompt-history rewrite.
    // 既有来源注册固定精确目录条目；它不是上传完成或提示词历史改写。
    let ids = registry
        .resolve_or_register_managed_attachments(&project, std::slice::from_ref(&attachment))?;
    let catalog = load_project_resource_catalog(&project)?;
    let source = resolve_project_resource_source(&project, &catalog, ids[0])?.ok_or("source")?;
    let upload_context = first.clone();
    let upload_attachment = attachment.clone();
    let upload_original = original.clone();
    let upload_registry = Arc::clone(&registry);
    // Physical upload remains parent-owned; only known successful completion gets an owned blocking publication worker.
    // 物理上传仍由父任务持有；只有已知成功完成才取得自有阻塞发布工作线程。
    let upload = tokio::spawn(async move {
        // The test uses the same single declared deadline as production source/cache/upload preparation.
        // 测试使用与生产来源/缓存/上传准备相同的单一声明截止时间。
        let deadline = vulcan_provider_adapters::MediaUploadDeadline::for_contract(&contract)
            .map_err(|error| error.to_string())?;
        let binding = Box::pin(prepare_upload(
            &source,
            &upload_context,
            &contract,
            &MediaUploadCancellation::default(),
            &deadline,
        ))
        .await
        .map_err(|error| error.to_string())?;
        tokio::task::spawn_blocking(move || {
            complete_attachment_draft_upload(
                &upload_attachment,
                scope,
                &upload_original,
                binding,
                &upload_registry,
            )
        })
        .await
        .map_err(|error| error.to_string())?
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), received_rx).await??;
    // Register the exact destination while the original lease is still live, then durably admit the original prompt.
    // 原租约仍活动时登记精确目标，随后可靠准入原提示词。
    register_attachment_draft_upload_handoff(&attachment, &original, &project, ids[0], &registry)?;
    let parameters = UserPromptParameters::for_resource_preparations(
        &ids,
        vec![UserPromptResourcePreparation::Prepared(
            original.prompt_delivery(ids[0])?,
        )],
    )?
    .ok_or("prompt parameters")?;
    handle.submit_root_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("draft-late")?,
            "inspect the original image",
            ids.clone(),
        )
        .with_parameters(parameters)?,
        2,
    )?;
    promote_staged_attachment_to_message_ref(
        &workspace.path().to_string_lossy(),
        &session_id.to_string(),
        "draft-late",
        std::slice::from_ref(&attachment),
        &[scope.into()],
    )?;
    // B supports its own uploads but may send only the address for A's unfinished original intent.
    // B支持自己的上传，但对A未完成原意图只能发送地址。
    let (mut second, second_secrets) = context(true, true).await?;
    second.session_scope_id = session_id.to_string();
    let second_route = original_route(&second);
    let request = ApiRequestSnapshot {
        provider: second_route.provider_id.clone(),
        profile: Some(second_route.protocol_profile_id.clone()),
        model: second_route.model_id.clone(),
        connection_id: Some(second_route.connection_id.clone()),
        thinking_level: None,
        temperature: None,
    };
    let second_context = second.clone();
    let second_registry = Arc::clone(&registry);
    handle
        .prepare_physical_api_durable(
            ApiPurposeKind::MainGeneration,
            request,
            LockedBaselineProjection::empty(),
            None,
            4,
            move |snapshot, preparation| async move {
                let media = prepare_v1_user_media_durable(
                    &snapshot,
                    Some(&second_context),
                    second_registry,
                    Arc::clone(&preparation),
                )
                .await?;
                assert!(metadata(&media, 0).contains("another model"));
                tokio::task::spawn_blocking(move || {
                    let refreshed = preparation
                        .recapture()?
                        .expect("only own preparation writes");
                    Ok(Some(SessionPreparedPhysicalRequest {
                        transport: (),
                        user_prompt_media: media.bindings,
                        projection: ProviderProjectionProof {
                            segment_generation: refreshed.segment_generation,
                            document_revision: refreshed.document_revision,
                            retained_from_seq: refreshed.retained_from_seq,
                            through_seq: refreshed.through_seq,
                            fingerprint_sha256: "a".repeat(64),
                        },
                        request_budget_evidence: test_provider_request_budget_evidence(),
                    }))
                })
                .await
                .expect("recapture worker")
            },
        )
        .await?
        .ok_or("first B request permit")?;
    // Save the entire original durable event sequence before releasing the provider response.
    // 放行供应商回复之前保存完整原可靠事件序列。
    let before = load_session_v1_full(&locator)?.document.events().to_vec();
    assert!(
        before
            .iter()
            .any(|event| matches!(event.event, EventBody::ApiStarted(_)))
    );
    let EventBody::UserInput(input) = &before[0].event else {
        panic!("original prompt")
    };
    let Some(UserPromptParameters {
        media_delivery: UserPromptMediaDelivery::Frozen { origin },
        ..
    }) = &input.parameters
    else {
        panic!("frozen prompt")
    };
    assert_eq!(origin.route, second_route);
    assert_eq!(origin.resources[0], original.prompt_delivery(ids[0])?);
    reply_tx.send(()).map_err(|_| "reply owner")?;
    upload.await??;
    server.await?;
    assert_eq!(load_session_v1_full(&locator)?.document.events(), before);
    // Real persisted prompt proof and the real returned reference drive both request and composer projection.
    // 真实持久提示词证明及真实返回引用驱动请求和输入区投影。
    let replay = handle.capture_context_compaction_planning_snapshot()?;
    let reads = first_secrets.reads.load(Ordering::SeqCst);
    let second_reads = second_secrets.reads.load(Ordering::SeqCst);
    assert_eq!(
        native(
            &prepare_v1_user_media(&replay, Some(&first), Arc::clone(&registry)).await?,
            0
        )
        .id,
        "file-draft-original-A"
    );
    assert!(
        metadata(
            &prepare_v1_user_media(&replay, Some(&second), Arc::clone(&registry)).await?,
            0
        )
        .contains("another model")
    );
    let entry = load_project_resource_catalog(&project)?.entries.remove(0);
    let restored = crate::media_upload::test_composer_project_decision(
        &first.execution,
        &attachment,
        &entry,
        &UserPromptResourcePreparation::Prepared(original.prompt_delivery(ids[0])?),
        &session_id.to_string(),
    )?;
    assert_eq!(
        restored.0,
        vulcan_service_api::ComposerAttachmentDeliveryPlan::UploadReference
    );
    assert_eq!(restored.1, "original_upload_reference_restored");
    // A newer unrelated receipt is not another occurrence's authority, even if its source and route match.
    // 即使来源及路线相同，较新无关回执也不是另一出现的权威。
    seed_receipt(
        &project,
        &registry,
        &entry,
        &first,
        "file-unrelated-newer",
        false,
    )?;
    assert_eq!(
        native(
            &prepare_v1_user_media(&replay, Some(&first), registry).await?,
            0
        )
        .id,
        "file-draft-original-A"
    );
    assert_eq!(first_secrets.reads.load(Ordering::SeqCst), reads);
    assert_eq!(second_secrets.reads.load(Ordering::SeqCst), second_reads);
    assert_eq!(load_session_v1_full(&locator)?.document.events(), before);
    Ok(())
}
