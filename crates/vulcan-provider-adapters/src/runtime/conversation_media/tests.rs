//! Deterministic lifecycle checks for the same wrapper used by real V1 uploads.
//! 真实V1上传使用的同一包装器的确定性生命周期检查。

use super::*;
use std::sync::Mutex;

/// Captures non-secret progress events in one isolated recorder for lifecycle assertions.
/// 在隔离记录器中捕获非秘密进度事件，用于生命周期断言。
type RecordedProgress = Arc<Mutex<Vec<RuntimeMediaUploadProgressEvent>>>;

/// Returns an isolated production emitter context and its ordered event recorder.
/// 返回隔离生产发射上下文及其有序事件记录器。
fn progress_context() -> (RuntimeMediaUploadProgressContext, RecordedProgress) {
    // The observer records actual phase emissions without using a service or provider.
    // 观察器记录实际阶段发射，不使用服务或供应商。
    let events: RecordedProgress = Arc::default();
    // Retain the same recorder in the installed host callback.
    // 在已安装宿主回调中保留相同记录器。
    let observed = Arc::clone(&events);
    (
        RuntimeMediaUploadProgressContext {
            observer: Arc::new(move |event| observed.lock().unwrap().push(event)),
            base: RuntimeMediaUploadProgressEvent {
                transfer_id: "isolated-transfer".into(),
                owner_session_id: "session-owner".into(),
                provider_id: "provider".into(),
                service_entry_id: "service".into(),
                connection_id: "account".into(),
                model_id: "model-a".into(),
                protocol_profile_id: "frozen-profile".into(),
                manifest_revision: 7,
                manifest_generation: 9,
                route_revision: "frozen-revision".into(),
                operation_id: crate::CONVERSATION_MEDIA_INPUT_OPERATION_ID.into(),
                upload_channel_id: "files".into(),
                file_name: "image.png".into(),
                media_kind: "image".into(),
                phase: RuntimeMediaUploadPhase::Preparing,
                bytes_sent: 0,
                total_bytes: Some(3),
                error: None,
            },
            last_bytes_sent: Arc::new(AtomicU64::new(0)),
            dispatch_state: Arc::default(),
        },
        events,
    )
}

/// Reads the exact ordered lifecycle phases from `events`, returning a compact assertion view.
/// 从`events`读取精确有序生命周期阶段，返回紧凑断言视图。
fn phases(events: &RecordedProgress) -> Vec<RuntimeMediaUploadPhase> {
    events
        .lock()
        .unwrap()
        .iter()
        .map(|event| event.phase)
        .collect()
}

/// A successful physical operation emits one terminal, seals late callbacks and leaves its parent active.
/// 成功物理操作发出一次终态，封闭迟到回调，并使父操作保持活动。
#[tokio::test]
async fn conversation_upload_success_seals_late_progress() -> Result<(), String> {
    // Source scope and telemetry are independent of a real network provider.
    // 来源作用域及遥测独立于真实网络供应商。
    let (context, events) = progress_context();
    // A retained byte callback simulates an in-flight producer arriving after completion.
    // 保留的字节回调模拟完成后才到达的进行中生产方。
    let late = context.byte_callback();
    // Parent request cancellation must not be consumed by an individual upload.
    // 单次上传不得消费父请求取消状态。
    let parent = MediaUploadCancellation::new();
    // Only the upload future emits byte counts, not the wrapper itself.
    // 只有上传Future发出字节计数，包装器本身不发出。
    let value = run_observed_upload(
        Some(context),
        &parent,
        "files",
        1000,
        |progress, _| async move {
            progress.expect("installed observer")(2, Some(3));
            Ok(42)
        },
    )
    .await?;
    assert_eq!(value, 42);
    late(3, Some(3));
    assert_eq!(
        phases(&events),
        vec![
            RuntimeMediaUploadPhase::Preparing,
            RuntimeMediaUploadPhase::Uploading,
            RuntimeMediaUploadPhase::Completed
        ]
    );
    assert!(!parent.is_cancelled());
    assert_eq!(events.lock().unwrap().last().unwrap().bytes_sent, 3);
    Ok(())
}

/// Credential-stage and post-byte failures emit Failed once with the last real byte count, never Cancelled.
/// 凭据阶段及传输字节后失败以最近真实字节数发出一次Failed，绝非Cancelled。
#[tokio::test]
async fn conversation_upload_failure_preserves_last_real_progress() {
    for transmitted in [0, 2] {
        // Each failure has an independent observable transfer identity.
        // 每次失败具有独立可观察传输身份。
        let (context, events) = progress_context();
        // Retain an explicit parent for cancellation-isolation assertions.
        // 保留显式父令牌，用于取消隔离断言。
        let parent = MediaUploadCancellation::new();
        // The injected operation fails before or after the declared byte callback.
        // 注入操作在声明字节回调之前或之后失败。
        let result: Result<(), MediaUploadFailure> = run_observed_upload(
            Some(context),
            &parent,
            "files",
            1000,
            |progress, _| async move {
                if transmitted > 0 {
                    progress.unwrap()(transmitted, Some(3));
                }
                Err(MediaUploadFailure::CredentialUnavailable)
            },
        )
        .await;
        assert!(result.is_err());
        assert!(!parent.is_cancelled());
        // The terminal row must retain the physically observed bytes, including zero before credentials.
        // 终态行必须保留物理观测字节，包括凭据前的零字节。
        let recorded = events.lock().unwrap();
        assert_eq!(
            recorded.last().unwrap().phase,
            RuntimeMediaUploadPhase::Failed
        );
        assert_eq!(recorded.last().unwrap().bytes_sent, transmitted);
        assert_eq!(
            recorded
                .iter()
                .filter(|event| matches!(
                    event.phase,
                    RuntimeMediaUploadPhase::Failed
                        | RuntimeMediaUploadPhase::Completed
                        | RuntimeMediaUploadPhase::Cancelled
                ))
                .count(),
            1
        );
    }
}

/// One total timeout spans all operation stages and cancels only transfer-local background work.
/// 单一总超时覆盖全部操作阶段，且仅取消传输局部后台工作。
#[tokio::test]
async fn conversation_upload_timeout_is_failed_not_cancelled() {
    // The production interface minimum is one second, not a special test-only timeout.
    // 生产接口最短超时是一秒，不是测试专用超时。
    let (context, events) = progress_context();
    // Parent request remains usable for later attachments after an individual timeout.
    // 单个附件超时后，父请求仍可处理后续附件。
    let parent = MediaUploadCancellation::new();
    // Capture the actual transfer token to prove timeout stops background work.
    // 捕获实际传输令牌，证明超时停止后台工作。
    let captured = Arc::new(Mutex::new(None));
    // The operation retains only its own copy of the capture recorder.
    // 操作仅保留自身的捕获记录器副本。
    let recorded_token = Arc::clone(&captured);
    // A never-completing credential/upload phase must be bounded by the outer total deadline.
    // 永不完成的凭据/上传阶段必须受外层总截止时间约束。
    let result: Result<(), MediaUploadFailure> = run_observed_upload(
        Some(context),
        &parent,
        "files",
        1000,
        |progress, token| async move {
            *recorded_token.lock().unwrap() = Some(token);
            progress.unwrap()(1, Some(3));
            std::future::pending().await
        },
    )
    .await;
    assert_eq!(
        result.unwrap_err(),
        MediaUploadFailure::Timeout {
            channel_id: "files".into(),
            timeout_ms: 1000
        }
    );
    assert_eq!(
        phases(&events),
        vec![
            RuntimeMediaUploadPhase::Preparing,
            RuntimeMediaUploadPhase::Uploading,
            RuntimeMediaUploadPhase::Failed
        ]
    );
    assert!(!parent.is_cancelled());
    assert!(captured.lock().unwrap().as_ref().unwrap().is_cancelled());
}

/// Carries source/cache time into the observed credential/transfer stage instead of resetting the original declared budget.
/// 将来源/缓存耗时带入可观测凭据/传输阶段，而非重置原声明预算。
#[tokio::test]
async fn conversation_upload_inherits_preparation_deadline() {
    // The real production deadline and observer have one shared original start.
    // 真实生产截止时间及观察器具有一个共享原起点。
    let deadline = MediaUploadDeadline::for_channel("files", 1000).expect("declared deadline");
    let (context, events) = progress_context();
    let parent = MediaUploadCancellation::new();
    deadline
        .wait(&parent, tokio::time::sleep(Duration::from_millis(600)))
        .await
        .expect("source/cache stage");
    // Another separately short phase would succeed only if upload incorrectly reset the full one-second budget.
    // 另一单独较短阶段只有在上传错误重置完整一秒预算时才会成功。
    let result = run_observed_upload_until(Some(context), &parent, &deadline, |_, _| async {
        tokio::time::sleep(Duration::from_millis(600)).await;
        Ok(())
    })
    .await;
    assert_eq!(
        result,
        Err(MediaUploadFailure::Timeout {
            channel_id: "files".into(),
            timeout_ms: 1000
        })
    );
    assert_eq!(
        phases(&events),
        vec![
            RuntimeMediaUploadPhase::Preparing,
            RuntimeMediaUploadPhase::Failed
        ]
    );
    assert!(!parent.is_cancelled());
}

/// An already expired source/cache phase cannot poll credentials, emit fake upload progress or reset its deadline on a clone.
/// 已到期来源/缓存阶段不能轮询凭据、发出虚假上传进度，或在克隆时重置截止时间。
#[tokio::test]
async fn conversation_upload_expired_preparation_never_starts_transfer() {
    // Expire the original budget through its actual production wait, not a modified private timestamp.
    // 经由实际生产等待耗尽原预算，不修改私有时间戳。
    let deadline = MediaUploadDeadline::for_channel("files", 1000).expect("deadline");
    let parent = MediaUploadCancellation::new();
    assert!(matches!(
        deadline.wait(&parent, std::future::pending::<()>()).await,
        Err(MediaUploadFailure::Timeout { .. })
    ));
    let cloned = deadline.clone();
    let (context, events) = progress_context();
    let mut polled = false;
    let result = run_observed_upload_until(Some(context), &parent, &cloned, |_, _| {
        polled = true;
        async { Ok(()) }
    })
    .await;
    assert!(matches!(result, Err(MediaUploadFailure::Timeout { .. })));
    assert!(!polled);
    assert!(phases(&events).is_empty());
    assert!(!parent.is_cancelled());
    parent.cancel();
    assert_eq!(cloned.check(&parent), Err(MediaUploadFailure::Cancelled));
}

/// Credential and transfer stages share one deadline, including a non-yielding final successful poll.
/// 凭据和传输阶段共享同一截止时间，也包括不让出的最终成功轮询。
#[tokio::test]
async fn conversation_upload_deadline_is_not_reset_by_stages_or_final_poll() {
    for cooperative in [true, false] {
        // Each stage test uses the production one-second minimum and independent observer state.
        // 每个阶段测试使用生产一秒最小值及独立观察器状态。
        let (context, events) = progress_context();
        // Expiry is a transfer failure, not a cancellation of the whole request.
        // 到期是传输失败，不是取消整个请求。
        let parent = MediaUploadCancellation::new();
        // Two individually short stages still exceed the same total budget when combined.
        // 两个分别较短的阶段合计后仍超过同一总预算。
        let result = run_observed_upload(
            Some(context),
            &parent,
            "files",
            1000,
            |progress, _| async move {
                if cooperative {
                    tokio::time::sleep(Duration::from_millis(600)).await;
                }
                progress.unwrap()(1, Some(3));
                if cooperative {
                    tokio::time::sleep(Duration::from_millis(600)).await;
                } else {
                    // Simulate a synchronous final observer that returns success after the total deadline.
                    // 模拟在总截止时间后才返回成功的同步最终观察器。
                    std::thread::sleep(Duration::from_millis(1100));
                }
                Ok(())
            },
        )
        .await;
        assert_eq!(
            result.unwrap_err(),
            MediaUploadFailure::Timeout {
                channel_id: "files".into(),
                timeout_ms: 1000
            }
        );
        assert_eq!(
            phases(&events),
            vec![
                RuntimeMediaUploadPhase::Preparing,
                RuntimeMediaUploadPhase::Uploading,
                RuntimeMediaUploadPhase::Failed
            ]
        );
        assert!(!parent.is_cancelled());
    }
}

/// Dropping an upload future emits Cancelled and stops background work with or without UI observation.
/// 丢弃上传Future会发出Cancelled，并无论是否有UI观察器都停止后台工作。
#[tokio::test]
async fn conversation_upload_drop_closes_progress_and_background_work() {
    for observed in [false, true] {
        // Each future owns a separate progress lifecycle.
        // 每个Future持有独立进度生命周期。
        let (context, events) = progress_context();
        // The token is reported only after the operation actually starts.
        // 仅在操作实际启动之后报告令牌。
        let (sender, receiver) = tokio::sync::oneshot::channel();
        // Spawn only an isolated never-completing upload wrapper, not a service.
        // 仅生成隔离的永不完成上传包装器，不启动服务。
        let task = tokio::spawn(async move {
            run_observed_upload(
                observed.then_some(context),
                &MediaUploadCancellation::new(),
                "files",
                1000,
                |progress, token| async move {
                    if let Some(progress) = progress {
                        progress(2, Some(3));
                    }
                    sender
                        .send(token)
                        .unwrap_or_else(|_| panic!("receiver retained"));
                    std::future::pending::<Result<(), MediaUploadFailure>>().await
                },
            )
            .await
        });
        // Await a real in-flight marker before aborting its owning future.
        // 在中止所属Future之前等待真实进行中标记。
        let token = receiver.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(token.is_cancelled());
        if observed {
            assert_eq!(
                phases(&events),
                vec![
                    RuntimeMediaUploadPhase::Preparing,
                    RuntimeMediaUploadPhase::Uploading,
                    RuntimeMediaUploadPhase::Cancelled
                ]
            );
            assert_eq!(events.lock().unwrap().last().unwrap().bytes_sent, 2);
        } else {
            assert!(events.lock().unwrap().is_empty());
        }
    }
}

/// Parent cancellation during preparation wins over success and prevents any later byte callback reopening the row.
/// 准备期间父取消优先于成功，并防止后续字节回调重新打开进度行。
#[tokio::test]
async fn conversation_upload_parent_cancellation_closes_once() {
    // Retain both scope cancellation and exact observer events.
    // 同时保留作用域取消和精确观察事件。
    let parent = MediaUploadCancellation::new();
    // The operation requests its own parent cancellation after one actual byte update.
    // 操作在一次真实字节更新后请求取消其父作用域。
    let cancel = parent.clone();
    // The late callback shares the exact same emission seal.
    // 迟到回调共享精确相同的发射封闭状态。
    let (context, events) = progress_context();
    // A callback arriving after the terminal must be ignored, not turn the row back into Uploading.
    // 终态之后到达的回调必须被忽略，不能使行重新变为Uploading。
    let late = context.byte_callback();
    // Yield after cancellation so the select observes it before completing the result.
    // 取消后让出执行，使select在完成结果之前观察到取消。
    let result: Result<(), MediaUploadFailure> = run_observed_upload(
        Some(context),
        &parent,
        "files",
        1000,
        |progress, _| async move {
            progress.unwrap()(1, Some(3));
            cancel.cancel();
            std::future::pending().await
        },
    )
    .await;
    assert_eq!(result.unwrap_err(), MediaUploadFailure::Cancelled);
    late(3, Some(3));
    assert_eq!(
        phases(&events),
        vec![
            RuntimeMediaUploadPhase::Preparing,
            RuntimeMediaUploadPhase::Uploading,
            RuntimeMediaUploadPhase::Cancelled
        ]
    );
}

/// Parent cancellation during the same poll as success must still produce Cancelled instead of Completed.
/// 与成功处于同次轮询的父取消仍必须产生Cancelled，而非Completed。
#[tokio::test]
async fn conversation_upload_same_poll_cancellation_wins_over_success() {
    // The same cancellation token is observed by the wrapper and the operation.
    // 包装器与操作观察相同取消令牌。
    let parent = MediaUploadCancellation::new();
    // A cloned token can be cancelled from inside the operation's successful poll.
    // 克隆令牌可在操作成功轮询内部被取消。
    let cancel = parent.clone();
    // Record the exact terminal boundary of the production wrapper.
    // 记录生产包装器的精确终态边界。
    let (context, events) = progress_context();
    assert!(
        run_observed_upload(Some(context), &parent, "files", 1000, |_, _| async move {
            cancel.cancel();
            Ok(())
        })
        .await
        .unwrap_err()
            == MediaUploadFailure::Cancelled
    );
    assert_eq!(
        phases(&events),
        vec![
            RuntimeMediaUploadPhase::Preparing,
            RuntimeMediaUploadPhase::Cancelled
        ]
    );
}

/// Invalid timeouts and pre-cancelled requests cannot emit Preparing or poll the operation.
/// 无效超时及预先取消的请求不能发出Preparing或轮询操作。
#[tokio::test]
async fn conversation_upload_invalid_or_cancelled_preflight_has_no_progress() {
    for timeout in [0, 999, u64::MAX, 1000] {
        // Only the valid timeout case is pre-cancelled explicitly.
        // 仅合法超时用例被显式预先取消。
        let parent = MediaUploadCancellation::new();
        if timeout == 1000 {
            parent.cancel();
        }
        // No external service observes these isolated preflight errors.
        // 没有外部服务观察这些隔离预检错误。
        let (context, events) = progress_context();
        // Polling the operation would violate the preflight-before-side-effect contract.
        // 轮询操作将违反先预检后副作用契约。
        let result: Result<(), MediaUploadFailure> =
            run_observed_upload(Some(context), &parent, "files", timeout, |_, _| async {
                panic!("operation must not be polled")
            })
            .await;
        assert!(result.is_err());
        assert!(events.lock().unwrap().is_empty());
    }
}

/// Reentrant host callbacks queue a terminal safely, preserving order without holding the emitter lock.
/// 可重入宿主回调安全排队终态，在不持有发射锁时保留顺序。
#[test]
fn conversation_upload_reentrant_observer_preserves_terminal_order() {
    // The callback obtains a clone only after construction, avoiding any global observer state.
    // 回调仅在构造后取得克隆，避免任何全局观察器状态。
    let active = Arc::new(Mutex::new(None::<RuntimeMediaUploadProgressContext>));
    // Record events under an independent observer mutex.
    // 在独立观察器互斥锁下记录事件。
    let events: RecordedProgress = Arc::default();
    // Obtain the same baseline metadata used by asynchronous lifecycle tests.
    // 取得异步生命周期测试使用的相同基准元数据。
    let (mut context, _) = progress_context();
    // Each callback retains independent clones rather than borrowing construction state.
    // 每个回调保留独立克隆，不借用构造状态。
    let emitting = Arc::clone(&active);
    // The recorder remains accessible for final assertions.
    // 记录器仍可用于最终断言。
    let observed = Arc::clone(&events);
    context.observer = Arc::new(move |event| {
        // Only the first callback reenters the production emitter.
        // 仅第一个回调重入生产发射器。
        let preparing = event.phase == RuntimeMediaUploadPhase::Preparing;
        observed.lock().unwrap().push(event);
        if preparing {
            let emitter = emitting.lock().unwrap().as_ref().unwrap().clone();
            emitter.emit(RuntimeMediaUploadPhase::Uploading, 1, Some(3), None);
            emitter.emit(RuntimeMediaUploadPhase::Uploading, 2, Some(3), None);
            emitter.emit(RuntimeMediaUploadPhase::Completed, 3, Some(3), None);
            emitter.emit(RuntimeMediaUploadPhase::Uploading, 3, Some(3), None);
        }
    });
    *active.lock().unwrap() = Some(context.clone());
    context.emit(RuntimeMediaUploadPhase::Preparing, 0, Some(3), None);
    assert_eq!(
        phases(&events),
        vec![
            RuntimeMediaUploadPhase::Preparing,
            RuntimeMediaUploadPhase::Uploading,
            RuntimeMediaUploadPhase::Completed
        ]
    );
    assert_eq!(events.lock().unwrap()[1].bytes_sent, 2);
    // Break the test-only callback cycle after assertions.
    // 在断言之后打破仅测试回调循环。
    active.lock().unwrap().take();
}
