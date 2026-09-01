//! Source integrity, framing and cancellation regressions for streaming file-create bodies.
//! 流式文件创建正文的来源完整性、封装及取消回归。

use super::*;
use std::sync::{Arc, Mutex};

/// Writes isolated `bytes` and returns the directory guard and exact host-proven local source.
/// 写入隔离`bytes`，返回目录守卫及精确宿主已证明本地来源。
async fn local_source(bytes: &[u8]) -> Result<(tempfile::TempDir, UploadResource), std::io::Error> {
    // Test files never touch project assets or user attachment leases.
    // 测试文件绝不触碰项目资产或用户附件租约。
    let directory = tempfile::tempdir()?;
    // A real local source exercises the bounded filesystem reader rather than an inline substitute.
    // 真实本地来源覆盖有界文件系统读取器，而非内联替代品。
    let path = directory.path().join("source.bin");
    tokio::fs::write(&path, bytes).await?;
    Ok((
        directory,
        UploadResource {
            source_identity: Some(crate::PreparedMediaSourceIdentity {
                byte_len: bytes.len() as u64,
                content_sha256: hex::encode(Sha256::digest(bytes)),
            }),
            file_name: Some("source.bin".into()),
            local_path: Some(path),
            inline_bytes: None,
            mime_type: "application/octet-stream".into(),
            byte_len: bytes.len() as u64,
            remote_url: None,
            item_count: 1,
        },
    ))
}

/// Drains `prepared`, returning the exact wire bytes or source failure while asserting completion agrees.
/// 消费`prepared`，返回精确线上字节或来源失败，并断言完成结果一致。
async fn collect_body(prepared: PreparedFileCreateBody) -> Result<Vec<u8>, MediaUploadFailure> {
    // The test collects small fixtures only; production passes the same stream directly to reqwest.
    // 测试仅收集小型夹具；生产将相同流直接交给reqwest。
    let PreparedFileCreateBody {
        mut body,
        content_length,
        completion,
    } = prepared;
    // Collected bytes permit exact JSON and Base64 assertions without network timing dependencies.
    // 收集字节允许精确JSON及Base64断言，不依赖网络时序。
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await {
        match chunk {
            Ok(chunk) => {
                assert!(chunk.len() <= RAW_CHUNK_BYTES / 3 * 4);
                bytes.extend_from_slice(&chunk);
            }
            Err(failure) => {
                assert_eq!(
                    completion.await.expect("typed source completion"),
                    Err(failure.clone())
                );
                assert!(body.next().await.is_none());
                assert!(
                    serde_json::from_slice::<Value>(&bytes).is_err(),
                    "failed source cannot close its JSON frame"
                );
                return Err(failure);
            }
        }
    }
    assert_eq!(bytes.len() as u64, content_length);
    assert_eq!(completion.await.expect("typed source completion"), Ok(()));
    Ok(bytes)
}

/// Original JSON values, exact lengths and raw progress survive every padding/chunk boundary for local and inline sources.
/// 本地及内联来源在全部填充/分块边界保持原JSON值、精确长度及原始进度。
#[tokio::test]
async fn file_create_stream_preserves_json_length_padding_and_progress()
-> Result<(), Box<dyn std::error::Error>> {
    for length in [
        0,
        1,
        2,
        3,
        4,
        RAW_CHUNK_BYTES - 1,
        RAW_CHUNK_BYTES,
        RAW_CHUNK_BYTES + 1,
        RAW_CHUNK_BYTES * 2 + 2,
    ] {
        // Nonuniform bytes expose chunk ordering and padding corruption.
        // 非均匀字节暴露分块顺序及填充损坏。
        let bytes: Vec<u8> = (0..length).map(|index| (index % 251) as u8).collect();
        // Keep each local entity alive through both declared source variants.
        // 在两个声明来源分支期间保持每个本地实体存活。
        let (_directory, mut resource) = local_source(&bytes).await?;
        for inline in [false, true] {
            resource.inline_bytes = inline.then(|| bytes.clone());
            // Fixed reserved fields must still be overwritten, with escaped Unicode metadata preserved.
            // 固定保留字段仍须被覆盖，同时保留转义Unicode元数据。
            let fields = BTreeMap::from([
                ("mime_type".into(), "wrong/type".into()),
                ("content_base64".into(), "must-not-survive".into()),
                ("purpose\"\\".into(), "文件\n\"\\".into()),
            ]);
            // Capture raw source progress without publishing global UI events.
            // 捕获原始来源进度，不发布全局UI事件。
            let events = Arc::new(Mutex::new(Vec::new()));
            // Only the stream owns the callback clone.
            // 仅由流持有回调克隆。
            let recorded = Arc::clone(&events);
            // Preparing the body does not read source chunks or report sent bytes.
            // 准备正文不会读取来源块或报告已发送字节。
            let prepared = prepare_file_create_body(
                &resource,
                &fields,
                Some(Arc::new(move |sent, total| {
                    recorded.lock().expect("events").push((sent, total))
                })),
                MediaUploadCancellation::new(),
            )
            .await?;
            assert!(events.lock().expect("events").is_empty());
            // The collected wire contract is semantically identical to the former whole-file JSON builder.
            // 收集到的线上契约与原整文件JSON构造器在语义上完全一致。
            let actual: Value = serde_json::from_slice(&collect_body(prepared).await?)?;
            assert_eq!(
                actual,
                serde_json::json!({
                    "mime_type": "application/octet-stream",
                    "content_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                    "purpose\"\\": "文件\n\"\\",
                })
            );
            // Expected progress is source length per bounded chunk, never encoded byte length.
            // 预期进度为每个有界块后的来源长度，绝不是编码字节长度。
            let expected: Vec<_> = (1..=length.div_ceil(RAW_CHUNK_BYTES))
                .map(|index| {
                    (
                        (index * RAW_CHUNK_BYTES).min(length) as u64,
                        Some(length as u64),
                    )
                })
                .collect();
            assert_eq!(*events.lock().expect("events"), expected);
        }
    }
    Ok(())
}

/// A same-length mutation after opening is rejected by the digest of the actual streamed bytes.
/// 打开后的同长度修改由实际流式字节的摘要拒绝。
#[tokio::test]
async fn file_create_stream_rejects_same_length_mutation() -> Result<(), Box<dyn std::error::Error>>
{
    // A full chunk plus a tail requires incremental hashing across multiple reads.
    // 一个完整块加尾部要求跨多次读取增量哈希。
    let bytes = vec![7_u8; RAW_CHUNK_BYTES + 2];
    // The proof predates the deliberate mutation.
    // 证明早于故意修改。
    let (_directory, source) = local_source(&bytes).await?;
    // No raw source read occurs while creating the stream.
    // 创建流时不发生原始来源读取。
    let prepared = prepare_file_create_body(
        &source,
        &BTreeMap::new(),
        None,
        MediaUploadCancellation::new(),
    )
    .await?;
    tokio::fs::write(
        source.local_path.as_ref().expect("local path"),
        vec![9_u8; bytes.len()],
    )
    .await?;
    assert_eq!(
        collect_body(prepared).await,
        Err(MediaUploadFailure::SourceChanged)
    );
    Ok(())
}

/// Growth and truncation after admission cannot escape the raw-byte limit or close a complete JSON request.
/// 准入后的增长及截短不能逃逸原始字节限制或闭合完整JSON请求。
#[tokio::test]
async fn file_create_stream_rejects_growth_and_truncation() -> Result<(), Box<dyn std::error::Error>>
{
    for length in [0, 2, 5] {
        // Three bytes are the exact original admitted length in every case.
        // 三字节是每种情况的精确原准入长度。
        let (_directory, source) = local_source(b"abc").await?;
        // Open first, then change the very same source entity before polling content.
        // 先打开，再于轮询内容前修改同一来源实体。
        let prepared = prepare_file_create_body(
            &source,
            &BTreeMap::new(),
            None,
            MediaUploadCancellation::new(),
        )
        .await?;
        tokio::fs::write(
            source.local_path.as_ref().expect("local path"),
            vec![b'a'; length],
        )
        .await?;
        assert_eq!(
            collect_body(prepared).await,
            Err(MediaUploadFailure::SourceChanged)
        );
    }
    Ok(())
}

/// Cancellation from an actual raw-byte callback terminates the body and its typed completion together.
/// 实际原始字节回调触发取消时，正文及强类型完成一起终结。
#[tokio::test]
async fn file_create_stream_cancellation_stops_before_next_chunk()
-> Result<(), Box<dyn std::error::Error>> {
    // A multi-chunk source proves cancellation does not read or publish the remaining chunks.
    // 多块来源证明取消不会读取或发布剩余块。
    let (_directory, source) = local_source(&vec![5_u8; RAW_CHUNK_BYTES * 3]).await?;
    // The same transfer token reaches both body reads and progress-driven cancellation.
    // 同一传输令牌同时到达正文读取及进度触发取消。
    let cancellation = MediaUploadCancellation::new();
    // Own the callback token without borrowing the caller.
    // 持有回调令牌，不借用调用方。
    let cancel_from_progress = cancellation.clone();
    // Record how many raw bytes were touched before cancellation.
    // 记录取消前实际接触的原始字节数。
    let events = Arc::new(Mutex::new(Vec::new()));
    // Share only this isolated test's observations.
    // 仅共享此隔离测试的观察结果。
    let recorded = Arc::clone(&events);
    // Cancellation happens inside the first callback, before the chunk is yielded to HTTP.
    // 取消发生在首次回调内部，早于向HTTP产出该块。
    let prepared = prepare_file_create_body(
        &source,
        &BTreeMap::new(),
        Some(Arc::new(move |sent, _| {
            recorded.lock().expect("events").push(sent);
            cancel_from_progress.cancel();
        })),
        cancellation.clone(),
    )
    .await?;
    assert_eq!(
        collect_body(prepared).await,
        Err(MediaUploadFailure::Cancelled)
    );
    assert_eq!(
        *events.lock().expect("events"),
        vec![RAW_CHUNK_BYTES as u64]
    );
    assert!(matches!(
        prepare_file_create_body(&source, &BTreeMap::new(), None, cancellation).await,
        Err(MediaUploadFailure::Cancelled)
    ));
    Ok(())
}

/// Dropping an unconsumed body closes its completion without starting a background source-reading task.
/// 丢弃未消费正文会关闭完成通道，不启动后台来源读取任务。
#[tokio::test]
async fn file_create_stream_drop_never_reports_source_success()
-> Result<(), Box<dyn std::error::Error>> {
    // A real source is kept alive independently of its short-lived body.
    // 真实来源的存活独立于短命正文。
    let (_directory, source) = local_source(b"abc").await?;
    // Only metadata is prepared before dropping the producer.
    // 丢弃生产者前仅准备元数据。
    let prepared = prepare_file_create_body(
        &source,
        &BTreeMap::new(),
        None,
        MediaUploadCancellation::new(),
    )
    .await?;
    drop(prepared.body);
    assert!(prepared.completion.await.is_err());
    Ok(())
}

/// Unrepresentable framing and inconsistent source lengths fail before source-sized allocation or reading.
/// 不可表示封装及不一致来源长度在来源规模分配或读取前失败。
#[tokio::test]
async fn file_create_stream_preflight_rejects_impossible_lengths()
-> Result<(), Box<dyn std::error::Error>> {
    // Use a known small resource, then replace metadata only to test preflight ordering.
    // 使用已知小型资源，再仅替换元数据以验证预检顺序。
    let (_directory, mut source) = local_source(b"abc").await?;
    source.byte_len = u64::MAX;
    source.local_path = None;
    assert!(matches!(
        prepare_file_create_body(
            &source,
            &BTreeMap::new(),
            None,
            MediaUploadCancellation::new()
        )
        .await,
        Err(MediaUploadFailure::InvalidContract)
    ));
    source.byte_len = 4;
    source.inline_bytes = Some(vec![1, 2, 3]);
    assert!(matches!(
        prepare_file_create_body(
            &source,
            &BTreeMap::new(),
            None,
            MediaUploadCancellation::new()
        )
        .await,
        Err(MediaUploadFailure::SourceChanged)
    ));
    source.byte_len = 3;
    source
        .source_identity
        .as_mut()
        .expect("host proof")
        .byte_len = 9;
    assert!(matches!(
        prepare_file_create_body(
            &source,
            &BTreeMap::new(),
            None,
            MediaUploadCancellation::new()
        )
        .await,
        Err(MediaUploadFailure::SourceChanged)
    ));
    Ok(())
}
