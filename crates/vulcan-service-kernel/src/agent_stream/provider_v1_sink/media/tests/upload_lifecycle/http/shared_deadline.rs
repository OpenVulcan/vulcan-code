//! Actual HTTPS timeout inherits preparation time and never emits a false Cancelled terminal.
//! 实际HTTPS超时继承准备耗时，绝不发出虚假Cancelled终态。

use super::*;

/// A real source-read stage consumes part of the frozen budget before HTTPS; late response would wrongly succeed only after a reset.
/// 真实来源读取阶段在HTTPS之前消耗部分冻结预算；迟到响应只有在错误重置后才会成功。
#[tokio::test]
async fn v1_https_upload_inherits_source_preparation_deadline()
-> Result<(), Box<dyn std::error::Error>> {
    // Isolate the physical catalog, cancellation owner and progress record from every user resource.
    // 将物理目录、取消所有者及进度记录与全部用户资源隔离。
    let project = tempfile::tempdir()?;
    let registry = ProjectResourceCatalogRegistry::default();
    let entry = register(project.path(), &registry, b"abc")?;
    let catalog = load_project_resource_catalog(project.path())?;
    let source = resolve_project_resource_source(project.path(), &catalog, entry.resource_id)?
        .ok_or("source")?;
    let parent = MediaUploadCancellation::new();
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed_events = Arc::clone(&events);
    let (received_tx, received_rx) = tokio::sync::oneshot::channel();
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let (http, server) = https_upload_fixture(
        200,
        r#"{"id":"file-too-late"}"#,
        Some(UploadReplyGate {
            request_received: received_tx,
            release_reply: reply_rx,
        }),
    )
    .await?;
    let (context, secrets) = context_with_http(
        false,
        true,
        Some(Arc::new(move |event| {
            observed_events.lock().expect("events").push(event)
        })),
        Some(http),
    )
    .await?;
    let contract = context
        .execution
        .client
        .conversation_media_upload_contract(
            &context.execution.route,
            ResourceKind::Image,
            "image/png",
        )?
        .ok_or("declared upload")?;
    assert_eq!(contract.timeout_ms, 1000);
    let deadline = vulcan_provider_adapters::MediaUploadDeadline::for_contract(&contract)?;
    let started = tokio::time::Instant::now();
    // Run the real admitted source reader within the same budget, with explicit elapsed preparation time.
    // 在同一预算内运行真实准入来源读取器，并明确消耗准备时间。
    deadline
        .wait(&parent, async {
            context
                .execution
                .client
                .read_admitted_media_source_identity(&source.path, source.entry.size, Some(&parent))
                .await?;
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            Ok::<(), String>(())
        })
        .await??;
    let upload_context = context.clone();
    let upload_parent = parent.clone();
    let upload = tokio::spawn(async move {
        Box::pin(prepare_upload(
            &source,
            &upload_context,
            &contract,
            &upload_parent,
            &deadline,
        ))
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), received_rx).await??;
    let mut upload = upload;
    // Correct expiry wins first. A reset would still be waiting and would accept the deliberately late valid response.
    // 正确到期先获胜；重置则仍在等待并会接纳故意迟到的有效响应。
    let result = tokio::select! {
        result = &mut upload => { drop(reply_tx); result? },
        _ = tokio::time::sleep_until(started + std::time::Duration::from_millis(1100)) => {
            reply_tx.send(()).map_err(|_| "reply owner")?;
            upload.await?
        }
    };
    server.await?;
    assert!(matches!(
        result,
        Err(MediaUploadFailure::Timeout {
            timeout_ms: 1000,
            ..
        })
    ));
    assert!(!parent.is_cancelled());
    let phases = events
        .lock()
        .expect("events")
        .iter()
        .map(|event| event.phase)
        .collect::<Vec<_>>();
    assert_eq!(phases.first(), Some(&RuntimeMediaUploadPhase::Preparing));
    assert_eq!(phases.last(), Some(&RuntimeMediaUploadPhase::Failed));
    assert!(!phases.contains(&RuntimeMediaUploadPhase::Cancelled));
    assert!(!phases.contains(&RuntimeMediaUploadPhase::Completed));
    assert_eq!(
        load_project_resource_catalog(project.path())?.entries[0]
            .provider_bindings
            .len(),
        0
    );
    assert!(secrets.reads.load(Ordering::SeqCst) > 0);
    Ok(())
}

/// Verified upload completion remains publishable when its terminal observer consumes the remaining time before private receipt persistence.
/// 已验证上传完成在终态观察器于私有回执持久化前耗尽剩余时间时，仍可发布。
#[tokio::test]
async fn v1_https_known_completion_survives_post_success_deadline_expiry()
-> Result<(), Box<dyn std::error::Error>> {
    // Real isolated bytes and a successful HTTPS reply establish known completion, not a guessed cached reference.
    // 真实隔离字节及成功HTTPS回复建立已知完成，而非猜测缓存引用。
    let project = tempfile::tempdir()?;
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    let entry = register(project.path(), &registry, b"abc")?;
    let (http, server) =
        https_upload_fixture(200, r#"{"id":"file-known-before-expiry"}"#, None).await?;
    let completed = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&completed);
    let (context, _) = context_with_http(
        false,
        true,
        Some(Arc::new(move |event| {
            if event.phase == RuntimeMediaUploadPhase::Completed {
                assert!(!observed.swap(true, Ordering::SeqCst));
                // This is deliberately after adapter validation and before the caller's durable publication worker.
                // 此处特意位于适配器校验之后、调用方可靠发布工作线程之前。
                std::thread::sleep(std::time::Duration::from_millis(1100));
            }
        })),
        Some(http),
    )
    .await?;
    let contract = context
        .execution
        .client
        .conversation_media_upload_contract(
            &context.execution.route,
            ResourceKind::Image,
            "image/png",
        )?
        .ok_or("upload contract")?;
    assert_eq!(contract.timeout_ms, 1000);
    let pending = snapshot(
        project.path(),
        entry.resource_id,
        vec![UserPromptParameters::for_new_resources(
            &[entry.resource_id],
        )],
    )?;
    let prepared = prepare_v1_user_media(&pending, Some(&context), Arc::clone(&registry)).await?;
    server.await?;
    assert!(completed.load(Ordering::SeqCst));
    assert_eq!(native(&prepared, 0).id, "file-known-before-expiry");
    let catalog = load_project_resource_catalog(project.path())?;
    assert_eq!(catalog.entries[0].provider_bindings.len(), 1);
    assert!(
        catalog.entries[0].provider_bindings[0].matches_prompt_upload(
            &prepared.bindings[0].origin.resources[0].delivery,
            &entry.sha256,
            entry.size
        )
    );
    Ok(())
}
