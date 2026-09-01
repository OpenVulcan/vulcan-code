//! Preparation cancellation before queued planning and source materialization.
//! 排队规划及来源物化之前的准备取消。

use super::*;

/// A dropped preparation guard cancels its queued real planner before catalog reads or origin creation.
/// 被丢弃的准备守卫在目录读取或原身份创建前取消排队真实规划器。
#[tokio::test]
async fn dropped_preparation_cancels_queued_metadata_planning()
-> Result<(), Box<dyn std::error::Error>> {
    // The fixture owns an empty temporary project; no catalog or entity is created.
    // 夹具持有空临时项目；不创建目录或实体。
    let project = tempfile::tempdir()?;
    // Use the real frozen execution fixture without any upload or provider request.
    // 使用真实冻结执行夹具，不进行任何上传或供应商请求。
    let (context, secrets) = context(false, false).await?;
    // Preserve credential reads performed by execution selection, not by cancelled media planning.
    // 保留执行选择产生的凭据读取数，取消媒体规划不应增加它。
    let reads = secrets.reads.load(Ordering::SeqCst);
    // A syntactically valid resource occurrence would require project lookup if cancellation were ignored.
    // 若忽略取消，语法有效资源出现将要求项目查询。
    let id = ResourceId::try_new(1)?;
    // Freeze the exact prompt occurrence order through the same snapshot helper as the V1 media tests.
    // 通过V1媒体测试共用快照辅助方法冻结精确提示词出现顺序。
    let input = snapshot(
        project.path(),
        id,
        vec![UserPromptParameters::for_new_resources(&[id])],
    )?;
    // The preparation owns cancellation from before its blocking planner is queued.
    // 准备在阻塞规划器排队之前就持有取消。
    let guard = PreparationCancellation(MediaUploadCancellation::new());
    // Clone the exact token into the queued worker, as the production entrypoint does.
    // 如同生产入口，把精确令牌克隆至排队工作线程。
    let worker_cancellation = guard.0.clone();
    // A deterministic release gate models a planner waiting in the blocking executor without sleeps.
    // 确定性释放闸门模拟规划器等待阻塞执行器，不使用休眠。
    let (release, queued) = std::sync::mpsc::channel();
    // Execute the actual planner after cancellation, not a test-only replacement planner.
    // 取消后执行实际规划器，而非测试专用替代规划器。
    let worker = tokio::task::spawn_blocking(move || {
        queued.recv().expect("release queued planner");
        plan_prompts(
            &input,
            &context,
            PromptMediaPreparationMode::Dispatch,
            &worker_cancellation,
        )
    });
    drop(guard);
    release.send(())?;
    // No plan can acquire a pending prompt's first-delivery identity after its owning preparation was dropped.
    // 所属准备被丢弃后，任何规划都不能取得待准备提示词的首次投递身份。
    let error = worker.await?.err().expect("cancelled planner");
    assert!(error.to_string().contains("V1 media preparation cancelled"));
    assert_eq!(secrets.reads.load(Ordering::SeqCst), reads);
    assert_eq!(std::fs::read_dir(project.path())?.count(), 0);
    Ok(())
}

/// A pre-cancelled materializer rejects even metadata-only work before generating attachment or origin output.
/// 预先取消物化器在生成附件或原身份输出前拒绝工作，包括仅元数据工作。
#[tokio::test]
async fn cancelled_materialization_produces_no_prompt_delivery()
-> Result<(), Box<dyn std::error::Error>> {
    // The project and registry are isolated, with no source entity to read or reference to publish.
    // 项目及注册表隔离，没有来源实体可读或引用可发布。
    let project = tempfile::tempdir()?;
    // This is the same frozen execution used by production metadata-only planning.
    // 此处使用生产仅元数据规划所用的相同冻结执行。
    let (context, _) = context(false, false).await?;
    // Cancellation is explicit and belongs to this exact resource preparation.
    // 取消明确且属于此次精确资源准备。
    let cancellation = MediaUploadCancellation::new();
    cancellation.cancel();
    // An unknown source would normally remain an address attachment, not become a new upload.
    // 未知来源通常应保留为地址附件，而非变成新上传。
    let planned = PlannedResource {
        admission_origin: None,
        original: None,
        id: ResourceId::try_new(1)?,
        entry: None,
        source: None,
        kind: ResourceKind::File,
        upload: None,
        decision: MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::SourceUnavailable,
        },
        original_receipt: None,
    };
    // Even harmless metadata cannot be returned as a successful new prompt delivery after cancellation.
    // 取消后，即使无害元数据也不能作为成功新提示词投递返回。
    let error = materialize_resource(
        planned,
        &context,
        &Arc::new(ProjectResourceCatalogRegistry::default()),
        project.path(),
        &cancellation,
        None,
    )
    .await
    .err()
    .expect("cancelled materializer");
    assert!(error.to_string().contains("V1 media preparation cancelled"));
    assert_eq!(std::fs::read_dir(project.path())?.count(), 0);
    Ok(())
}
