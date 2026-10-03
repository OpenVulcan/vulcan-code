//! Targeted bounded handoff, privacy, correlation ordering and tail-settlement evidence.
//! 有界交接、隐私、关联顺序及尾部退出的定向证据。

use super::*;

/// Return one exact Core queue record with no allocated instance or invented measurement.
/// 返回一个精确 Core 队列记录，不含已分配实例或虚构测量。
fn queue_record() -> Value {
    json!({"luaskills_embedded_diagnostic":1,"runtime_id":"runtime-original","pool_id":"pool-original",
        "operation_id":"operation-original","instance_id":null,"phase":"queue","elapsed_ns":9,
        "lua_heap_bytes":null,"host_wait":null,"succeeded":null})
}

/// Return the original typed host binding, with absent controller and distinct fixed session IDs.
/// 返回原类型宿主绑定，控制器缺失且固定会话身份分别保存。
fn binding() -> VulcanPluginNativeIdentityEvent {
    VulcanPluginNativeIdentityEvent::OperationBound {
        runtime_id: "runtime-original".into(),
        pool_id: "pool-original".into(),
        operation_id: "operation-original".into(),
        plugin_id: "plugin-original".into(),
        plugin_generation: 7,
        package_generation: "generation-original".into(),
        execution_revision: "revision-original".into(),
        core_session_id: Some("core-session-original".into()),
        controller_session_id: None,
        host_invocation_id: None,
        host_session_id: Some("host-session-original".into()),
        trusted_request_id: None,
        prewarm: false,
        finalization_instance_id: None,
    }
}

/// Permit isolated consumer IO without installing a global tracing subscriber.
/// 允许隔离消费者 IO，无需安装全局 tracing 接收器。
fn enabled() -> bool {
    true
}

/// Deny output through the consumer's actual target-filter seam.
/// 通过消费者实际目标过滤接缝拒绝输出。
fn filtered() -> bool {
    false
}

/// Return a metadata-only ingress and receiver using the production capacity source.
/// 使用生产容量来源返回仅元数据交接器及接收端。
fn ingress() -> (Ingress, Receiver<Observation>) {
    // The fixed production queue capacity is shared with the deterministic stalled receiver fixture.
    // 固定生产队列容量与确定性停顿接收端夹具共用。
    let (sender, receiver) = mpsc::sync_channel(QUEUE_RECORDS);
    (
        Ingress {
            sender,
            losses: Arc::new(Losses::default()),
        },
        receiver,
    )
}

/// Read actual service file JSON records, preserving their persisted order.
/// 读取真实服务文件 JSON 记录，保留持久化顺序。
fn records(root: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(root.join("service.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Prove a stalled consumer cannot block callbacks and both capacity and byte losses are explicit.
/// 证明停顿消费者不能阻塞回调，并且容量及字节丢失均明确。
#[test]
fn stalled_consumer_handoff_is_bounded_and_nonblocking() {
    // Keep the receiver alive without reading, then issue more callbacks than its actual capacity.
    // 保持接收端存活但不读取，再发出超过其实际容量的回调。
    let (ingress, receiver) = ingress();
    // An exact bounded Core event for the fixture.
    // 夹具使用的精确有界 Core 事件。
    let message = RuntimeLogEvent {
        level: RuntimeLogLevel::Info,
        message: queue_record().to_string(),
    };
    // The monotonic start used to verify nonblocking handoff.
    // 用于验证非阻塞交接的单调起点。
    let started = Instant::now();
    for _ in 0..(QUEUE_RECORDS * 4) {
        ingress.core(&message);
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "callbacks waited on a stalled consumer"
    );
    assert_eq!(receiver.try_iter().count(), QUEUE_RECORDS);
    assert_eq!(
        ingress.losses.full.load(Ordering::Acquire),
        (QUEUE_RECORDS * 3) as u64
    );
    ingress.core(&RuntimeLogEvent {
        level: RuntimeLogLevel::Info,
        message: "x".repeat(RECORD_BYTES + 1),
    });
    assert_eq!(ingress.losses.oversize.load(Ordering::Acquire), 1);
    drop(receiver);
    ingress.core(&message);
    assert_eq!(ingress.losses.disconnected.load(Ordering::Acquire), 1);
    assert!(ingress.losses.incomplete.load(Ordering::Acquire));
}

/// Reject private/unknown fields, missing nullable keys, duplicate keys and fabricated retirement ownership.
/// 拒绝私有或未知字段、缺失可空键、重复键及虚构退役所有权。
#[test]
fn strict_schema_privacy_and_retirement_null() {
    // The exact current Core stage schema fixture.
    // 精确当前 Core 阶段 Schema 夹具。
    let original = queue_record();
    assert_eq!(
        parse_core(&original.to_string()).unwrap(),
        Some(original.clone())
    );
    for private in [
        "value",
        "effects",
        "workspace_root",
        "security_partition",
        "secret",
    ] {
        // An intentionally private-field-bearing schema violation.
        // 刻意包含私有字段的 Schema 违规记录。
        let mut invalid = original.clone();
        invalid
            .as_object_mut()
            .unwrap()
            .insert(private.into(), json!("PRIVATE_SENTINEL"));
        assert!(parse_core(&invalid.to_string()).is_err());
    }
    // A selected Core record missing a required nullable key.
    // 缺失必需可空键的选中 Core 记录。
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().remove("instance_id");
    assert!(parse_core(&missing.to_string()).is_err());
    // An original JSON string with a duplicate identity key.
    // 包含重复身份键的原 JSON 字符串。
    let duplicate = original
        .to_string()
        .replacen('{', "{\"runtime_id\":\"PRIVATE_SENTINEL\",", 1);
    assert!(parse_core(&duplicate).is_err());
    assert!(
        parse_core("ordinary Core message containing PRIVATE_SENTINEL")
            .unwrap()
            .is_none()
    );
    // Ordinary mentions and future versions are outside the explicitly selected Core schema.
    // 普通文本提及及未来版本不属于明确选中的 Core Schema。
    assert!(
        parse_core(
            "ordinary log mentions luaskills_embedded_diagnostic but has no selected JSON marker"
        )
        .unwrap()
        .is_none()
    );
    // A future marker outside the explicit selected scope.
    // 明确选中范围之外的未来标记。
    let mut unselected_version = original.clone();
    unselected_version["luaskills_embedded_diagnostic"] = json!(2);
    assert!(
        parse_core(&unselected_version.to_string())
            .unwrap()
            .is_none()
    );
    // The exact physical-retirement schema with absent operation.
    // 操作缺失的精确物理退役 Schema。
    let retired = json!({"luaskills_embedded_diagnostic":1,"runtime_id":"runtime-original","pool_id":"pool-original",
        "operation_id":null,"instance_id":"instance-original","phase":"retired","close_elapsed_ns":5,
        "destruction_elapsed_ns":3,"lua_heap_before_close_bytes":11});
    assert_eq!(
        parse_core(&retired.to_string()).unwrap(),
        Some(retired.clone())
    );
    // An intentionally fabricated last-business retirement owner.
    // 刻意虚构的最后业务退役所有者。
    let mut forged = retired;
    forged["operation_id"] = json!("last-business-operation");
    assert!(parse_core(&forged.to_string()).is_err());
}

/// Stage-before-binding order remains joinable after handles are gone, using exact original persisted keys.
/// 句柄消失后，先阶段后绑定仍可使用持久化的精确原键关联。
#[test]
fn actual_file_preserves_cross_order_identity_and_private_exclusion() {
    // The isolated fixture directory; user service logs are never touched.
    // 隔离夹具目录；绝不访问用户服务日志。
    let root = tempfile::tempdir().unwrap();
    // The shared real file writer, with one rotation owner.
    // 共享真实文件写入器，仅一个轮转所有者。
    let writer =
        crate::logging::LogWriter::native_test_writer(root.path().to_owned(), 64 * 1024).unwrap();
    let (ingress, receiver) = ingress();
    // Shared capture loss accounting without native owners.
    // 不含原生所有者的共享采集丢失账本。
    let losses = Arc::clone(&ingress.losses);
    ingress.core(&RuntimeLogEvent {
        level: RuntimeLogLevel::Info,
        message: queue_record().to_string(),
    });
    ingress.core(&RuntimeLogEvent {
        level: RuntimeLogLevel::Info,
        message: "ordinary PRIVATE_SENTINEL secret path".into(),
    });
    ingress.identity(binding());
    losses.native_closed.store(true, Ordering::Release);
    drop(ingress);
    assert!(consume(receiver, writer, losses, Weak::new(), enabled).is_ok());
    // Actually persisted JSON records in file order.
    // 按文件顺序实际持久化的 JSON 记录。
    let records = records(root.path());
    // The selected persisted original phase record location.
    // 选中的持久化原阶段记录位置。
    let stage = records
        .iter()
        .position(|record| record["event"] == "native_phase")
        .unwrap();
    // The matching original identity record location.
    // 匹配的原身份记录位置。
    let binding = records
        .iter()
        .position(|record| record["event"] == "native_identity")
        .unwrap();
    assert!(stage < binding);
    for key in ["runtime_id", "pool_id", "operation_id"] {
        assert_eq!(
            records[stage]["native"][key],
            records[binding]["identity"][key]
        );
    }
    assert_eq!(
        records[binding]["identity"]["controller_session_id"],
        Value::Null
    );
    assert_eq!(
        records[binding]["identity"]["core_session_id"],
        "core-session-original"
    );
    assert_eq!(
        records[binding]["identity"]["host_session_id"],
        "host-session-original"
    );
    assert!(
        !serde_json::to_string(&records)
            .unwrap()
            .contains("PRIVATE_SENTINEL")
    );
}

/// Filtered records and unconfirmed native shutdown both return explicit incomplete receipts.
/// 被过滤记录及未经确认的原生关闭均返回明确不完整回执。
#[test]
fn filter_and_unconfirmed_shutdown_cannot_claim_completion() {
    for (allow, closed) in [(false, true), (true, false)] {
        // The isolated fixture directory; user service logs are never touched.
        // 隔离夹具目录；绝不访问用户服务日志。
        let root = tempfile::tempdir().unwrap();
        // The shared real file writer, with one rotation owner.
        // 共享真实文件写入器，仅一个轮转所有者。
        let writer =
            crate::logging::LogWriter::native_test_writer(root.path().to_owned(), 64 * 1024)
                .unwrap();
        let (ingress, receiver) = ingress();
        // Shared capture loss accounting without native owners.
        // 不含原生所有者的共享采集丢失账本。
        let losses = Arc::clone(&ingress.losses);
        losses.native_closed.store(closed, Ordering::Release);
        ingress.identity(binding());
        drop(ingress);
        assert!(
            consume(
                receiver,
                writer,
                losses,
                Weak::new(),
                if allow { enabled } else { filtered }
            )
            .is_err()
        );
        if allow {
            // The actual persisted terminal capture checkpoint.
            // 实际持久化终态采集检查点。
            let summary = records(root.path())
                .into_iter()
                .find(|record| record["event"] == "native_capture_checkpoint")
                .unwrap();
            assert_eq!(summary["incomplete"], true);
            assert_eq!(summary["native_closed"], false);
        } else {
            assert!(records(root.path()).is_empty());
        }
    }
}

/// A real filesystem failure is returned from the shared writer and prevents a clean drain receipt.
/// 共享写入器返回真实文件系统失败，并禁止干净排空回执。
#[test]
fn actual_io_failure_is_not_a_tracing_success() {
    // Isolated temporary fixture ownership.
    // 隔离临时夹具所有权。
    let temp = tempfile::tempdir().unwrap();
    // The isolated fixture directory; user service logs are never touched.
    // 隔离夹具目录；绝不访问用户服务日志。
    let root = temp.path().join("log-root");
    // The shared real file writer, with one rotation owner.
    // 共享真实文件写入器，仅一个轮转所有者。
    let writer = crate::logging::LogWriter::native_test_writer(root.clone(), 64 * 1024).unwrap();
    writer.native_test_close();
    std::fs::remove_file(root.join("service.jsonl")).unwrap();
    std::fs::remove_dir(&root).unwrap();
    std::fs::write(&root, b"blocked directory").unwrap();
    let (ingress, receiver) = ingress();
    // Shared capture loss accounting without native owners.
    // 不含原生所有者的共享采集丢失账本。
    let losses = Arc::clone(&ingress.losses);
    losses.native_closed.store(true, Ordering::Release);
    ingress.identity(binding());
    drop(ingress);
    assert!(consume(receiver, writer, Arc::clone(&losses), Weak::new(), enabled).is_err());
    assert!(losses.incomplete.load(Ordering::Acquire));
}

/// Observer totals produce only checkpoint deltas, even when consumer and finish see the same value.
/// 即使消费者及结束边界观察同一值，观察器总数也仅产生检查点差值。
#[test]
fn observer_failure_deltas_do_not_double_count_or_emit_lifetime_totals() {
    // This ledger is shared by the consumer and the finish boundary.
    // 此账本由消费者及结束边界共享。
    let losses = Losses::default();
    losses.observe_failures(2);
    losses.observe_failures(2);
    assert_eq!(losses.checkpoint()["observer_missing"], 2);
    assert_eq!(losses.checkpoint()["observer_missing"], 0);
    losses.observe_failures(3);
    assert_eq!(losses.checkpoint()["observer_missing"], 1);
    assert!(losses.incomplete.load(Ordering::Acquire));
}

/// Read one actual archived JSON file without assuming source-line or record positions.
/// 读取一个真实归档 JSON 文件，不假设源码行号或记录位置。
fn archived_records(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// A terminal checkpoint's own size forces rotation; its actual file reports the new segment and zero old counts.
/// 终态检查点自身大小迫使轮转；其真实文件报告新分段，并清除旧计数。
#[test]
fn checkpoint_self_rotation_uses_actual_segment_and_resets_counts() {
    // Freeze the exact production checkpoint shape to derive a small deterministic test limit.
    // 冻结精确生产检查点形状，派生确定性小容量测试上限。
    let sizing_losses = Losses::default();
    sizing_losses.native_closed.store(true, Ordering::Release);
    // One actual phase will be accepted in the old segment before the terminal checkpoint.
    // 终态检查点之前，一个真实阶段会在旧分段被接受。
    let old_counts = WindowCounts {
        accepted: 1,
        ..WindowCounts::default()
    };
    // The margin is a test fixture amount, not another production segment or queue capacity.
    // 余量是测试夹具大小，不是另一个生产分段或队列容量。
    let limit = (NativeCheckpoint::new(0, 0, &old_counts, &sizing_losses, true, false)
        .line()
        .unwrap()
        .len()
        + 64) as u64;
    // The isolated real writer shares production rotation and record persistence.
    // 隔离真实写入器共享生产轮转及记录持久化实现。
    let root = tempfile::tempdir().unwrap();
    // Inspect the same writer after the actual consumer has finished, without opening a second log.
    // 真实消费者完成后检查同一写入器，不打开第二份日志。
    let writer =
        crate::logging::LogWriter::native_test_writer(root.path().to_owned(), limit).unwrap();
    // This clone is the identical shared writer, not a separate RollingLog.
    // 此克隆是完全相同的共享写入器，不是独立 RollingLog。
    let inspect = writer.clone();
    // The production queue carries the actual selected native phase.
    // 生产队列承载真实选中原生阶段。
    let (ingress, receiver) = ingress();
    // Only successful real native shutdown permits a clean final drain receipt.
    // 仅真实原生关闭成功允许干净最终排空回执。
    let losses = Arc::clone(&ingress.losses);
    losses.native_closed.store(true, Ordering::Release);
    ingress.core(&RuntimeLogEvent {
        level: RuntimeLogLevel::Info,
        message: queue_record().to_string(),
    });
    drop(ingress);
    assert!(consume(receiver, writer, losses, Weak::new(), enabled).is_ok());
    assert_eq!(
        inspect.native_segment().unwrap(),
        1,
        "fixture must rotate specifically while writing its terminal checkpoint"
    );
    // Find the actual current-file checkpoint by its stable event identity.
    // 通过稳定事件身份找到真实当前文件检查点。
    let checkpoint = records(root.path())
        .into_iter()
        .find(|record| record["event"] == "native_capture_checkpoint")
        .unwrap();
    assert_eq!(checkpoint["segment_id"], inspect.native_segment().unwrap());
    assert_eq!(
        checkpoint["counts"],
        serde_json::to_value(WindowCounts::default()).unwrap()
    );
    assert_eq!(checkpoint["counter_scope"], "observed_rotation_segment");
    assert_eq!(checkpoint["rotation_observed"], true);
    assert_eq!(checkpoint["terminal"], true);
    assert_eq!(
        checkpoint["complete_confirmation"],
        "owner_drain_and_file_flush_ack_required"
    );
    assert!(
        archived_records(&root.path().join("service.1.jsonl"))
            .iter()
            .any(|record| record["event"] == "native_phase")
    );
    assert!(
        std::fs::metadata(root.path().join("service.jsonl"))
            .unwrap()
            .len()
            <= limit
    );
}

/// A separate writer rotates after the checkpoint copied its old counts; the same log lock fixes final scope.
/// 另一个写入器在检查点复制旧计数后轮转；同一日志锁修正最终范围。
#[test]
fn checkpoint_after_other_writer_rotation_resets_only_segment_counts() {
    // Preserve a handoff loss across segment rotation; its scope is explicitly a checkpoint interval.
    // 跨分段轮转保留交接丢失；其范围明确是检查点区间。
    let losses = Losses::default();
    losses.native_closed.store(true, Ordering::Release);
    losses.full.store(3, Ordering::Release);
    losses.incomplete.store(true, Ordering::Release);
    // One actual old-segment native event establishes the old accepted count.
    // 一个真实旧分段原生事件建立旧接受计数。
    let old_counts = WindowCounts {
        accepted: 1,
        ..WindowCounts::default()
    };
    // Create the checkpoint before the other writer acquires the shared lock.
    // 在另一写入器获得共享锁之前创建检查点。
    let mut checkpoint = NativeCheckpoint::new(0, 0, &old_counts, &losses, true, false);
    // Derive a small limit that fits one checkpoint and a short new-segment tracing record.
    // 派生小容量上限，容纳一个检查点及一条短的新分段 tracing 记录。
    let limit = (checkpoint.line().unwrap().len() + 64) as u64;
    // Every operation below uses the same original writer and RollingLog.
    // 以下每项操作均使用同一个原写入器及 RollingLog。
    let root = tempfile::tempdir().unwrap();
    // The native and external writer handles are clones of one exact writer identity.
    // 原生及外部写入句柄是同一精确写入器身份的克隆。
    let writer =
        crate::logging::LogWriter::native_test_writer(root.path().to_owned(), limit).unwrap();
    // Persist the actual old-segment phase before filling its remaining capacity.
    // 填满剩余容量前持久化真实旧分段阶段。
    let phase = json!({"event":"native_phase", "native":queue_record()});
    assert_eq!(writer.write_native_record(&phase).unwrap(), 0);
    // Exact remaining bytes make the external record trigger one deterministic rotation.
    // 精确剩余字节使外部记录触发一次确定性轮转。
    let remaining = limit as usize - (serde_json::to_vec(&phase).unwrap().len() + 1);
    // Serialize valid fixed fixture JSON that fills, but does not exceed, the old segment.
    // 序列化合法固定夹具 JSON，填满但不超过旧分段。
    let padding = json!({"padding":"x".repeat(remaining - serde_json::to_vec(&json!({"padding":""})).unwrap().len() - 1)});
    assert_eq!(writer.write_native_record(&padding).unwrap(), 0);
    // The actual Write implementation models an independent tracing formatting call.
    // 真实 Write 实现模拟独立 tracing 格式化调用。
    let mut other_writer = writer.clone();
    std::io::Write::write_all(&mut other_writer, b"{\"event\":\"other_writer\"}\n").unwrap();
    assert_eq!(writer.native_segment().unwrap(), 1);
    assert_eq!(
        writer.write_native_checkpoint(&mut checkpoint, 0).unwrap(),
        1
    );
    // Read the actual current file after both writes to verify final placement and separate scopes.
    // 两次写入后读取真实当前文件，验证最终落点及分别定义的范围。
    let persisted = records(root.path())
        .into_iter()
        .find(|record| record["event"] == "native_capture_checkpoint")
        .unwrap();
    assert_eq!(persisted["segment_id"], writer.native_segment().unwrap());
    assert_eq!(
        persisted["counts"],
        serde_json::to_value(WindowCounts::default()).unwrap()
    );
    assert_eq!(persisted["counter_scope"], "observed_rotation_segment");
    assert_eq!(persisted["rotation_observed"], true);
    assert_eq!(
        persisted["handoff_scope"],
        "since_previous_consumer_checkpoint"
    );
    assert_eq!(persisted["handoff_losses"]["full"], 3);
    assert_eq!(persisted["incomplete"], true);
    assert!(
        archived_records(&root.path().join("service.1.jsonl"))
            .iter()
            .any(|record| record["event"] == "native_phase")
    );
    assert!(
        std::fs::metadata(root.path().join("service.jsonl"))
            .unwrap()
            .len()
            <= limit
    );
}

/// A checkpoint that cannot fit one segment returns a real error instead of a successful omission record.
/// 无法放入一个分段的检查点返回真实错误，而非成功的省略提示记录。
#[test]
fn oversized_checkpoint_is_rejected_without_false_persistence_ack() {
    // Exact checkpoint size defines the deliberately insufficient isolated segment capacity.
    // 精确检查点大小定义刻意不足的隔离分段容量。
    let losses = Losses::default();
    // This metadata retains the production fixed safe schema.
    // 此元数据保留生产固定安全 Schema。
    let mut checkpoint =
        NativeCheckpoint::new(0, 0, &WindowCounts::default(), &losses, true, false);
    // This isolated directory never accesses the user's service logs.
    // 此隔离目录绝不访问用户服务日志。
    let root = tempfile::tempdir().unwrap();
    // A one-byte deficit verifies exact serialization-based admission rather than estimated bounds.
    // 一个字节的不足验证基于精确序列化的准入，而非估计边界。
    let writer = crate::logging::LogWriter::native_test_writer(
        root.path().to_owned(),
        (checkpoint.line().unwrap().len() - 1) as u64,
    )
    .unwrap();
    assert!(writer.write_native_checkpoint(&mut checkpoint, 0).is_err());
    assert!(records(root.path()).is_empty());
    assert_eq!(writer.native_segment().unwrap(), 0);
}

/// Original callback Arc snapshots paused before entry keep shutdown pending until their tail is delivered.
/// 在入口前暂停的原回调 Arc 快照使关闭保持等待，直至其尾部交付。
#[test]
fn cloned_source_snapshot_before_entry_is_settled_before_drain() {
    // The isolated fixture directory; user service logs are never touched.
    // 隔离夹具目录；绝不访问用户服务日志。
    let root = tempfile::tempdir().unwrap();
    // The shared real file writer, with one rotation owner.
    // 共享真实文件写入器，仅一个轮转所有者。
    let writer =
        crate::logging::LogWriter::native_test_writer(root.path().to_owned(), 64 * 1024).unwrap();
    let (ingress, receiver) = ingress();
    // Owned handoff for the original Core callback.
    // 原 Core 回调拥有的交接器。
    let core_ingress = ingress.clone();
    // The exact original Core callback Arc.
    // 精确原 Core 回调 Arc。
    let core: RuntimeLogCallback = Arc::new(move |event| core_ingress.core(event));
    // Owned handoff for the identity callback.
    // 身份回调拥有的交接器。
    let identity_ingress = ingress.clone();
    // The exact original identity subscription.
    // 精确原身份订阅。
    let identity = Arc::new(
        install_native_identity_observer(Arc::new(move |event| identity_ingress.identity(event)))
            .unwrap(),
    );
    let observer = Arc::downgrade(&identity);
    // Shared capture loss accounting without native owners.
    // 不含原生所有者的共享采集丢失账本。
    let losses = Arc::clone(&ingress.losses);
    let (acknowledge, drained) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = acknowledge.try_send(consume(receiver, writer, losses, observer, enabled));
    });
    set_log_callback(Some(Arc::clone(&core)));
    // Exercise the actual public Core emitter with ordinary Info mixed between selected diagnostics.
    // 使用真实 Core 公开发送器，在选中诊断之间插入普通 Info。
    emit_runtime_log(RuntimeLogLevel::Info, queue_record().to_string());
    emit_runtime_log(RuntimeLogLevel::Info, "ordinary Core Info PRIVATE_SENTINEL");
    // Core clones its Arc before calling outside the source lock. Hold that exact ownership seam
    // without entering the callback; an entered-only counter would incorrectly report zero here.
    // Core 在源锁外调用前克隆 Arc。固定该精确所有权接缝且不进入回调；仅入口计数在此会错误报告零。
    let source_snapshot = Arc::clone(&core);
    // The service lifetime owner of both subscriptions.
    // 两个订阅的服务寿命所有者。
    let capture = Arc::new(NativeDiagnosticCapture {
        owner: Mutex::new(Some(CaptureOwner {
            core,
            identity,
            ingress,
            drained,
        })),
        result: Mutex::new(None),
    });
    // Shared fixture owner retained by the finish thread.
    // 结束线程保留的共享夹具所有者。
    let finishing = Arc::clone(&capture);
    let (finished, result) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = finished.try_send(finishing.finish_after_native_shutdown());
    });
    assert!(
        result.recv_timeout(Duration::from_millis(30)).is_err(),
        "finish ignored a source snapshot paused before callback entry"
    );
    source_snapshot(&RuntimeLogEvent {
        level: RuntimeLogLevel::Info,
        message: queue_record().to_string(),
    });
    drop(source_snapshot);
    assert!(result.recv_timeout(FINISH_TIMEOUT).unwrap().is_ok());
    assert!(capture.finish_after_native_shutdown().is_ok());
    assert!(
        records(root.path())
            .iter()
            .any(|record| record["event"] == "native_phase")
    );
}
