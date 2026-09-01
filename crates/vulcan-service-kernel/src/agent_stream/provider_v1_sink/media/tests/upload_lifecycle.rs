//! Real V1 preparation must consume the installed runtime media observer without fake cache progress.
//! 真实V1准备必须消费已安装运行时媒体观察器，不伪造缓存进度。

use super::*;
use vulcan_provider_adapters::{RuntimeMediaUploadPhase, RuntimeMediaUploadProgressEvent};

mod http;

/// A real V1 pending upload reports credential failure through the existing observer and retains its attachment.
/// 真实V1待处理上传通过既有观察器报告凭据失败，并保留附件。
#[tokio::test]
async fn v1_real_upload_reports_terminal_progress_and_preserves_prompt()
-> Result<(), Box<dyn std::error::Error>> {
    // Resource registration uses an isolated production project catalog.
    // 资源注册使用隔离生产项目目录。
    let project = tempfile::tempdir()?;
    // The same catalog owner receives all possible receipt writes.
    // 相同目录所有者接收全部可能的回执写入。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // This real source is small enough for the exact declared upload interface.
    // 此真实来源足够小，符合精确声明上传接口。
    let entry = register(project.path(), &registry, b"abc")?;
    // Capture actual runtime observer events, not a hand-built service delta.
    // 捕获实际运行时观察器事件，而非手工构建服务增量。
    let events = Arc::new(std::sync::Mutex::new(
        Vec::<RuntimeMediaUploadProgressEvent>::new(),
    ));
    // The observer belongs to the runtime before the model execution is frozen.
    // 观察器在模型执行冻结之前归运行时所有。
    let observed = Arc::clone(&events);
    // No real network request can begin after the controlled credential failure.
    // 受控凭据失败之后不可能开始真实网络请求。
    let (context, secrets) = context_with_observer(
        false,
        true,
        Some(Arc::new(move |event| observed.lock().unwrap().push(event))),
    )
    .await?;
    secrets.reject_reads.store(true, Ordering::SeqCst);
    // Only a genuinely new prompt owns permission to attempt this upload.
    // 只有真正新提示词拥有尝试本次上传的权限。
    let input = snapshot(
        project.path(),
        entry.resource_id,
        vec![UserPromptParameters::for_new_resources(
            &[entry.resource_id],
        )],
    )?;
    // Execute the real V1 policy, source verification, credential boundary and final attachment projection.
    // 执行真实V1策略、来源验证、凭据边界及最终附件投影。
    let prepared = prepare_v1_user_media(&input, Some(&context), Arc::clone(&registry)).await?;
    assert!(
        metadata(&prepared, 0)
            .contains(MediaDeliveryRejection::UploadPreparationFailed.explanation())
    );
    assert_eq!(prepared.bindings.len(), 1);
    assert!(matches!(
        prepared.bindings[0].origin.resources[0].delivery,
        UserPromptResourceDeliveryKind::UploadPending { .. }
    ));
    assert_eq!(prepared.prompts[&0].attachments.len(), 1);
    assert!(
        load_project_resource_catalog(project.path())?.entries[0]
            .provider_bindings
            .is_empty()
    );
    // Failure has one stable transfer identity and zero fabricated uploaded bytes.
    // 失败具有一个稳定传输身份，且没有伪造已上传字节。
    let recorded = events.lock().unwrap();
    assert_eq!(
        recorded.iter().map(|event| event.phase).collect::<Vec<_>>(),
        vec![
            RuntimeMediaUploadPhase::Preparing,
            RuntimeMediaUploadPhase::Failed
        ]
    );
    assert_eq!(recorded[0].transfer_id, recorded[1].transfer_id);
    for event in recorded.iter() {
        assert_eq!(event.owner_session_id, context.session_scope_id);
        assert_eq!(event.model_id, context.execution.model.id);
        assert_eq!(
            event.operation_id,
            vulcan_provider_adapters::CONVERSATION_MEDIA_INPUT_OPERATION_ID
        );
        assert_eq!(event.upload_channel_id, "offline-files");
        assert_eq!(event.bytes_sent, 0);
        assert_eq!(event.total_bytes, Some(3));
    }
    assert!(recorded[1].error.as_deref().unwrap().contains("credential"));
    assert!(
        !recorded[1]
            .error
            .as_deref()
            .unwrap()
            .contains("isolated credential preparation failure")
    );
    Ok(())
}

/// Fresh-intent cache reuse and historical restoration both bypass credentials and upload progress.
/// 新意图缓存复用及历史恢复都绕过凭据读取与上传进度。
#[tokio::test]
async fn v1_upload_cache_reuse_does_not_emit_fake_progress()
-> Result<(), Box<dyn std::error::Error>> {
    // Use the real append-only catalog with a previously verified immutable receipt.
    // 使用真实仅追加目录及以前已验证的不可变回执。
    let project = tempfile::tempdir()?;
    // The same registry is retained across cache and historical replay.
    // 缓存及历史重放期间保留相同注册表。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Equal bytes do not collapse the two independent prompt intentions.
    // 相同字节不折叠两个独立提示词意图。
    let entry = register(project.path(), &registry, b"abc")?;
    // Any emitted upload event would be a regression for this cache-only path.
    // 本纯缓存路径发出的任何上传事件都构成回归。
    let events = Arc::new(std::sync::Mutex::new(
        Vec::<RuntimeMediaUploadProgressEvent>::new(),
    ));
    // Freeze the observer at the same point as the selected model execution.
    // 在与所选模型执行相同的边界冻结观察器。
    let observed = Arc::clone(&events);
    // A real declared upload route is present, but no network operation is needed.
    // 存在真实声明上传路线，但不需要网络操作。
    let (context, secrets) = context_with_observer(
        false,
        true,
        Some(Arc::new(move |event| observed.lock().unwrap().push(event))),
    )
    .await?;
    // Seed the exact original receipt rather than a loose file-ID cache value.
    // 写入精确原回执，而非宽松文件ID缓存值。
    let origin = seed_receipt(
        project.path(),
        &registry,
        &entry,
        &context,
        "file-existing",
        false,
    )?;
    // Any late credential access fails, proving cache reuse stays outside physical upload setup.
    // 任何较晚凭据访问均失败，证明缓存复用保持在物理上传设置之外。
    let reads = secrets.reads.load(Ordering::SeqCst);
    secrets.reject_reads.store(true, Ordering::SeqCst);
    // Preserve the new occurrence and the historical occurrence in chronological prompt order.
    // 按提示词时序保留新出现及历史出现。
    let input = snapshot(
        project.path(),
        entry.resource_id,
        vec![
            UserPromptParameters::for_new_resources(&[entry.resource_id]),
            frozen(origin),
        ],
    )?;
    // Both rows must materialize the exact same valid receipt without a new transfer row.
    // 两行都必须物化精确相同有效回执，不产生新传输行。
    let prepared = prepare_v1_user_media(&input, Some(&context), Arc::clone(&registry)).await?;
    assert_eq!(native(&prepared, 0).id, "file-existing");
    assert_eq!(native(&prepared, 1).id, "file-existing");
    assert!(events.lock().unwrap().is_empty());
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    assert_eq!(
        load_project_resource_catalog(project.path())?.entries[0]
            .provider_bindings
            .len(),
        1
    );
    Ok(())
}
