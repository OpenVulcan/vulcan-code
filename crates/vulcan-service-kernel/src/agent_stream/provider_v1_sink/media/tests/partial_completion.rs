//! Projection of completed prompt occurrences before the first API is frozen.
//! 首次API冻结前已完成提示词出现的投影。

use super::*;

/// Reuses prompt-owned partial upload proof as B-address and A-original with no new credentials or upload.
/// 将提示词所有部分上传证明复用为B地址及A原引用，不读取新凭据或重新上传。
#[tokio::test]
async fn v1_partial_completion_restores_original_upload_before_first_api()
-> Result<(), Box<dyn std::error::Error>> {
    // All catalog mutations are isolated from real projects and provider credentials.
    // 全部目录修改隔离于真实项目及供应商凭据。
    let project = tempfile::tempdir()?;
    // Private provider references stay in the existing project catalog.
    // 私有供应商引用留在既有项目目录中。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Real source registration retains immutable content identity.
    // 真实来源注册保留不可变内容身份。
    let entry = register(project.path(), &registry, b"abc")?;
    // A already completed an upload for this exact prompt occurrence.
    // A已为本精确提示词出现完成上传。
    let (first, first_secrets) = context(false, true).await?;
    // B has upload capability too; that must not grant it this older upload intent.
    // B同样具有上传能力；这不能向其授予更早上传意图。
    let (second, second_secrets) = context(true, true).await?;
    // The fixture stores one exact receipt rather than consulting a latest-file heuristic.
    // 夹具存储一个精确回执，而非查询最近文件启发式。
    let original = seed_receipt(
        project.path(),
        &registry,
        &entry,
        &first,
        "file-partial-A",
        false,
    )?;
    // No admitted draft proof exists: only the real runtime completion state owns the origin.
    // 不存在准入草稿证明：仅真实运行时完成状态持有原身份。
    let parameters = UserPromptParameters {
        version: 1,
        prepared_resources: vec![],
        unproven_resource_positions: vec![],
        media_delivery: UserPromptMediaDelivery::Preparing {
            resources: vec![Some(original.resources[0].clone())],
        },
    };
    // First-request eligibility remains open even though this occurrence already finished.
    // 即使本次出现已完成，首次请求资格仍保持开放。
    let inputs = snapshot(project.path(), entry.resource_id, vec![Some(parameters)])?;
    // Count credentials after model selection so any accidental re-upload is observable.
    // 在模型选择后统计凭据，使任何意外重新上传可观察。
    let first_reads = first_secrets.reads.load(Ordering::SeqCst);
    // B must not consume its upload credentials for A's already completed occurrence.
    // B不能为A已完成出现消耗自身上传凭据。
    let second_reads = second_secrets.reads.load(Ordering::SeqCst);
    // B exposes metadata only and keeps the complete earlier original result in its eventual binding.
    // B仅暴露元数据，并在最终绑定中保留完整更早原结果。
    let switched = prepare_v1_user_media(&inputs, Some(&second), registry.clone()).await?;
    assert!(metadata(&switched, 0).contains("another model"));
    assert_eq!(switched.bindings[0].origin.resources, original.resources);
    assert_eq!(switched.bindings[0].origin.route, original_route(&second));
    // Returning before any API starts must restore the exact original receipt, not create a new reference.
    // 任何API开始前回切必须恢复精确原回执，不创建新引用。
    let restored = prepare_v1_user_media(&inputs, Some(&first), registry).await?;
    assert_eq!(native(&restored, 0).id, "file-partial-A");
    assert_eq!(restored.bindings[0].origin.resources, original.resources);
    assert_eq!(first_secrets.reads.load(Ordering::SeqCst), first_reads);
    assert_eq!(second_secrets.reads.load(Ordering::SeqCst), second_reads);
    Ok(())
}

/// Restores a partial inline choice on another supported model even when that model normally prefers uploading.
/// 即使另一受支持模型通常优先上传，也在其上恢复部分完成内联选择。
#[tokio::test]
async fn v1_partial_completion_inline_remains_inline_on_supported_model()
-> Result<(), Box<dyn std::error::Error>> {
    // Both model views share canonical bytes but not mutable request-local ownership.
    // 两个模型视图共享规范字节，但不共享可变请求局部归属。
    let project = tempfile::tempdir()?;
    // Use the real isolated catalog owner for both model views.
    // 为两个模型视图使用真实隔离目录所有者。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Register canonical bytes through the production resource entrypoint.
    // 通过生产资源入口注册规范字节。
    let entry = register(project.path(), &registry, b"abc")?;
    // A's completed inline result remains the original choice throughout both projections.
    // A已完成内联结果在两次投影中始终为原选择。
    let (first, _) = context(false, false).await?;
    // B normally uploads small images because its upload-inline threshold is zero.
    // B通常上传小图片，因为其有上传内联阈值为零。
    let (second, second_secrets) = context(true, true).await?;
    // Partial state is host-owned and contains no media bytes.
    // 部分状态为宿主所有，不含媒体字节。
    let original = UserPromptResourceDelivery {
        resource_id: entry.resource_id,
        original_route: original_route(&first),
        delivery: UserPromptResourceDeliveryKind::Inline,
    };
    // Runtime completion is independent of immutable admission proof.
    // 运行时完成独立于不可变准入证明。
    let parameters = UserPromptParameters {
        version: 1,
        prepared_resources: vec![],
        unproven_resource_positions: vec![],
        media_delivery: UserPromptMediaDelivery::Preparing {
            resources: vec![Some(original.clone())],
        },
    };
    // The prompt remains eligible for its first physical request.
    // 提示词仍有资格执行首次物理请求。
    let inputs = snapshot(project.path(), entry.resource_id, vec![Some(parameters)])?;
    // Any fresh upload would read the credential store again.
    // 任何新上传都会再次读取凭据存储。
    let reads = second_secrets.reads.load(Ordering::SeqCst);
    // Run the actual final admission and materialization path for model B.
    // 为模型B运行真实最终准入及物化路径。
    let prepared = prepare_v1_user_media(&inputs, Some(&second), registry).await?;
    assert_eq!(
        native(&prepared, 0).source_kind,
        ResourceSourceKind::InlineData
    );
    assert_eq!(prepared.bindings[0].origin.resources, vec![original]);
    assert_eq!(second_secrets.reads.load(Ordering::SeqCst), reads);
    Ok(())
}
