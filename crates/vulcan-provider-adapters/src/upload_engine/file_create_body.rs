//! Bounded JSON/Base64 framing for the declared provider file-create primitive.
//! 为声明的供应商文件创建原语提供有界JSON/Base64正文封装。

use super::*;
use tokio::io::AsyncRead;
use tokio::sync::oneshot;

/// Raw chunk size divisible by three, so only the final Base64 chunk can contain padding.
/// 可被三整除的原始块大小，因此只有最后一个Base64块可能包含填充。
const RAW_CHUNK_BYTES: usize = 48 * 1024;

/// One owned streaming body and its exact framing length and source-validation outcome.
/// 一份拥有所有权的流式正文及其精确封装长度和来源校验结果。
pub(super) struct PreparedFileCreateBody {
    /// HTTP body containing the unchanged JSON fields and incrementally encoded original bytes.
    /// 包含不变JSON字段及增量编码原始字节的HTTP正文。
    pub body: futures_util::stream::BoxStream<'static, Result<Vec<u8>, MediaUploadFailure>>,
    /// Exact full JSON byte length, including Base64 padding, without allocating that length.
    /// 包含Base64填充的完整精确JSON字节长度，不按该长度分配内存。
    pub content_length: u64,
    /// Source result published before the closing JSON frame, independent of HTTP error wrapping.
    /// 在闭合JSON封装前发布的来源结果，独立于HTTP错误包装。
    pub completion: oneshot::Receiver<Result<(), MediaUploadFailure>>,
}

/// Opens `resource` and frames `fixed_fields` without reading the complete source into memory.
/// 打开`resource`并封装`fixed_fields`，不将完整来源读入内存。
/// `progress` counts raw bytes yielded; `cancellation` owns every read. Returns an exact-length body or a typed preflight error.
/// `progress`统计已产出原始字节；`cancellation`约束每次读取。返回精确长度正文或强类型预检错误。
pub(super) async fn prepare_file_create_body(
    resource: &UploadResource,
    fixed_fields: &BTreeMap<String, String>,
    progress: Option<MediaUploadProgressCallback>,
    cancellation: MediaUploadCancellation,
) -> Result<PreparedFileCreateBody, MediaUploadFailure> {
    if cancellation.is_cancelled() {
        return Err(MediaUploadFailure::Cancelled);
    }
    // Both reserved fields override manifest fixed fields exactly as in the original JSON builder.
    // 两个保留字段均覆盖清单固定字段，与原JSON构造器完全一致。
    let mut fields: Map<String, Value> = fixed_fields
        .iter()
        .filter(|(name, _)| name.as_str() != "content_base64")
        .map(|(name, value)| (name.clone(), Value::String(value.clone())))
        .collect();
    fields.insert(
        "mime_type".into(),
        Value::String(resource.mime_type.clone()),
    );
    // Serialize only small declared metadata; the payload never becomes a serde_json::Value.
    // 仅序列化小型声明元数据；负载绝不成为serde_json::Value。
    let mut prefix =
        serde_json::to_vec(&fields).map_err(|_| MediaUploadFailure::InvalidContract)?;
    if prefix.pop() != Some(b'}') {
        return Err(MediaUploadFailure::InvalidContract);
    }
    prefix.extend_from_slice(b",\"content_base64\":\"");
    // Checked arithmetic rejects an unrepresentable HTTP length before opening or allocating source bytes.
    // 检查算术在打开或分配来源字节前拒绝无法表示的HTTP长度。
    let content_length = crate::media_base64_encoded_len(resource.byte_len)
        .and_then(|encoded| encoded.checked_add(prefix.len() as u64))
        .and_then(|framed| framed.checked_add(2))
        .ok_or(MediaUploadFailure::InvalidContract)?;
    // The original host digest is optional only for ordinary workspace sources without an earlier catalog proof.
    // 原宿主摘要仅对没有更早目录证明的普通工作区来源可选。
    let expected_identity = resource.source_identity.clone();
    if expected_identity
        .as_ref()
        .is_some_and(|identity| identity.byte_len != resource.byte_len)
    {
        return Err(MediaUploadFailure::SourceChanged);
    }
    // Existing source variants are explicit: inline bytes take precedence, otherwise the exact local path is required.
    // 既有来源分支明确：内联字节优先，否则要求精确本地路径。
    let mut source: Box<dyn AsyncRead + Unpin + Send> = if let Some(bytes) = &resource.inline_bytes
    {
        if bytes.len() as u64 != resource.byte_len {
            return Err(MediaUploadFailure::SourceChanged);
        }
        Box::new(std::io::Cursor::new(bytes.clone()))
    } else {
        // Freeze one handle before metadata and transfer; never re-resolve the AI-visible locator.
        // 在元数据及传输前冻结一个句柄；绝不重新解析AI可见定位符。
        let path = resource
            .local_path
            .as_ref()
            .ok_or(MediaUploadFailure::SourceUnavailable)?;
        // Opening is still covered by the caller's common lifecycle deadline and cancellation.
        // 打开仍受调用方共同生命周期截止时间和取消约束。
        let file = tokio::select! {
            biased;
            _ = cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
            result = tokio::fs::File::open(path) => result.map_err(|_| MediaUploadFailure::SourceUnavailable)?,
        };
        // Handle metadata rejects source growth or replacement before body construction.
        // 句柄元数据在正文构造前拒绝来源增长或替换。
        let metadata = tokio::select! {
            biased;
            _ = cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
            result = file.metadata() => result.map_err(|_| MediaUploadFailure::SourceUnavailable)?,
        };
        if !metadata.is_file() || metadata.len() != resource.byte_len {
            return Err(MediaUploadFailure::SourceChanged);
        }
        Box::new(file)
    };
    // Keep the admitted raw-byte bound separate from HTTP's encoded framing length.
    // 将准入原始字节限制与HTTP编码封装长度分开。
    let total_bytes = resource.byte_len;
    // The typed completion channel survives reqwest's internal body-error wrappers without parsing messages.
    // 强类型完成通道跨越reqwest内部正文错误包装，无需解析消息。
    let (completion_sender, completion) = oneshot::channel();
    // Only one raw chunk and one encoded chunk are live regardless of the local file size.
    // 无论本地文件大小如何，始终只同时保留一个原始块和一个编码块。
    let encoded: futures_util::stream::BoxStream<'static, Result<Vec<u8>, MediaUploadFailure>> = async_stream::try_stream! {
        if cancellation.is_cancelled() {
            Err(MediaUploadFailure::Cancelled)?;
        }
        yield prefix;
        // Read exact multiples of three; arbitrary short filesystem reads must not introduce intermediate padding.
        // 精确读取三的倍数；任意文件系统短读不能引入中间填充。
        let mut buffer = vec![0_u8; RAW_CHUNK_BYTES];
        // Digest the exact bytes exposed to this body, not a second path-based read after transfer.
        // 对本正文暴露的精确字节计算摘要，不使用传输后第二次基于路径的读取。
        let mut digest = Sha256::new();
        // Monotonic raw-byte progress is never inflated by JSON or Base64 overhead.
        // 单调原始字节进度绝不因JSON或Base64开销而膨胀。
        let mut sent = 0_u64;
        while sent < total_bytes {
            // This conversion is bounded by the small constant, not by the potentially large source size.
            // 此转换受小型常量而非潜在巨大来源大小约束。
            let length = (total_bytes - sent).min(RAW_CHUNK_BYTES as u64) as usize;
            tokio::select! {
                biased;
                _ = cancellation.wait() => Err(MediaUploadFailure::Cancelled),
                result = source.read_exact(&mut buffer[..length]) => {
                    result.map(|_| ()).map_err(|error| if error.kind() == std::io::ErrorKind::UnexpectedEof {
                        MediaUploadFailure::SourceChanged
                    } else {
                        MediaUploadFailure::SourceUnavailable
                    })
                },
            }?;
            digest.update(&buffer[..length]);
            sent += length as u64;
            if let Some(callback) = &progress {
                callback(sent, Some(total_bytes));
            }
            if cancellation.is_cancelled() {
                Err(MediaUploadFailure::Cancelled)?;
            }
            yield base64::engine::general_purpose::STANDARD.encode(&buffer[..length]).into_bytes();
        }
        // One extra byte detects growth but is never encoded or counted as transmitted media.
        // 一个额外字节检测增长，但绝不编码或计为已传输媒体。
        let mut extra = [0_u8; 1];
        // Validation must finish before the closing frame can make the JSON request complete.
        // 校验必须在闭合封装使JSON请求完整之前完成。
        let extra_bytes = tokio::select! {
            biased;
            _ = cancellation.wait() => Err(MediaUploadFailure::Cancelled),
            result = source.read(&mut extra) => result.map_err(|_| MediaUploadFailure::SourceUnavailable),
        }?;
        if extra_bytes != 0 || expected_identity.is_some_and(|identity| hex::encode(digest.finalize()) != identity.content_sha256) {
            Err(MediaUploadFailure::SourceChanged)?;
        }
    }.boxed();
    // Separate source completion from network status; an early 200 is insufficient to prove a complete source.
    // 将来源完成与网络状态分开；提前200不足以证明来源完整。
    let body = stream! {
        tokio::pin!(encoded);
        while let Some(chunk) = encoded.next().await {
            match chunk {
                Ok(bytes) => yield Ok(bytes),
                Err(failure) => {
                    let _ = completion_sender.send(Err(failure.clone()));
                    yield Err(failure);
                    return;
                }
            }
        }
        let _ = completion_sender.send(Ok(()));
        yield Ok(b"\"}".to_vec());
    };
    Ok(PreparedFileCreateBody {
        body: body.boxed(),
        content_length,
        completion,
    })
}

#[cfg(test)]
mod tests;
