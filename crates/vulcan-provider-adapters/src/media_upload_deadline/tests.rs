//! Frozen budget identity and cancellation tests independent from provider progress presentation.
//! 独立于供应商进度呈现的冻结预算身份及取消测试。

use super::*;

/// Creates one explicit one-second contract for identity validation without selecting another channel or network.
/// 为身份校验创建明确一秒契约，不选择另一通道或网络。
fn contract() -> ConversationMediaUploadContract {
    ConversationMediaUploadContract {
        timeout_ms: 1000,
        upload_channel_id: "files".into(),
        contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
        reference_scope: crate::UploadReferenceScope::Session,
        cache_duration_ms: Some(60000),
        reference_encoding: crate::UploadReferenceEncoding::ProviderFileId,
        max_bytes: Some(1048576),
        max_count: Some(1),
    }
}

/// Rejects replacing the timeout/channel, and validates bounds without inventing a fallback duration.
/// 拒绝替换超时/通道，并在不虚构回退时长的情况下校验边界。
#[test]
fn upload_deadline_requires_original_channel_and_duration() {
    // One immutable original declaration supplies the only valid budget identity.
    // 一个不可变原声明提供唯一有效预算身份。
    let original = contract();
    let deadline = MediaUploadDeadline::for_contract(&original).expect("deadline");
    assert!(deadline.validate_contract(&original).is_ok());
    let mut changed = original.clone();
    changed.timeout_ms = 2000;
    assert_eq!(
        deadline.validate_contract(&changed),
        Err(MediaUploadFailure::InvalidContract)
    );
    changed = original.clone();
    changed.upload_channel_id = "larger-channel".into();
    assert_eq!(
        deadline.validate_contract(&changed),
        Err(MediaUploadFailure::InvalidContract)
    );
    for timeout in [0, 999, crate::provider_manifest::MAX_ASYNC_TIMEOUT_MS + 1] {
        changed = original.clone();
        changed.timeout_ms = timeout;
        assert!(matches!(
            MediaUploadDeadline::for_contract(&changed),
            Err(MediaUploadFailure::InvalidContract)
        ));
    }
}

/// Parent cancellation wins both before the first stage poll and during a non-yielding successful poll.
/// 父取消在首次阶段轮询前及不让出的成功轮询中均优先。
#[tokio::test]
async fn upload_deadline_never_polls_pre_cancelled_work() {
    // No wall-clock waiting is needed for deterministic cancellation ordering.
    // 确定性取消顺序不需要墙钟等待。
    let deadline = MediaUploadDeadline::for_contract(&contract()).expect("deadline");
    let parent = MediaUploadCancellation::new();
    parent.cancel();
    let mut polled = false;
    assert_eq!(
        deadline
            .wait(&parent, async {
                polled = true;
                7
            })
            .await,
        Err(MediaUploadFailure::Cancelled)
    );
    assert!(!polled);
    // A distinct live parent may cancel itself while producing an otherwise successful stage value.
    // 不同活动父令牌可在产出其他方面成功的阶段值时取消自身。
    let live = MediaUploadCancellation::new();
    assert_eq!(
        deadline
            .wait(&live, async {
                live.cancel();
                7
            })
            .await,
        Err(MediaUploadFailure::Cancelled)
    );
}

/// A large asynchronous source buffer stays heap-owned instead of expanding the enclosing deadline future's stack frame.
/// 大型异步来源缓冲区保持堆所有，而非扩张外围截止时间Future的栈帧。
#[tokio::test]
async fn upload_deadline_keeps_large_stage_frame_out_of_parent() {
    // The same production wait must remain compact even while its child retains a full source-read chunk.
    // 即使子阶段保留完整来源读取块，相同生产等待也必须保持紧凑。
    let deadline = MediaUploadDeadline::for_contract(&contract()).expect("deadline");
    let parent = MediaUploadCancellation::new();
    let waiting = deadline.wait(&parent, async {
        let chunk = [1_u8; 65536];
        tokio::task::yield_now().await;
        std::hint::black_box(chunk)[0]
    });
    assert!(
        std::mem::size_of_val(&waiting) < 1024,
        "deadline wrapper must not inline large source stages"
    );
    assert_eq!(waiting.await.expect("stage result"), 1);
}
