//! Typed local compaction summaries.
//! 类型化本地压缩摘要。

use crate::v1::{DomainError, Seq};
use serde::{Deserialize, Serialize};

/// Maximum UTF-8 bytes in one stored visual description.
/// 单条持久视觉描述的 UTF-8 字节上限。
pub const MAX_COMPACTED_IMAGE_DESCRIPTION_BYTES: usize = 2048;

/// A host-verified image reference and its bounded visual extraction.
/// 宿主已校验的图片引用与有界视觉摘取结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactedImageExcerpt {
    /// Host-owned address exposed by the original conversation image or native reader.
    /// 原会话图片或原生读取器暴露的宿主地址。
    pub locator: String,
    /// Exact chronological occurrences in this session branch.
    /// 此会话分支中精确、有序的资源出现序号。
    pub source_seqs: Vec<Seq>,
    /// Catalog content identity of the original image.
    /// 原图在资源目录中的内容身份。
    pub sha256: String,
    /// Visual findings, or absence when the original could not be read.
    /// 视觉摘取内容，原图无法读取时为空。
    pub description: Option<String>,
}

/// Persisted context payload produced by a completed local summary.
/// 由已完成本地摘要产生的持久上下文载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompactionContent {
    /// Exact completed local model summary, also present in its source event.
    /// 精确完成的本地模型摘要，同时存在于其来源事件。
    Summary {
        /// Unmodified semantic text used by the next context generation.
        /// 下一上下文代次使用的未修改语义文本。
        text: String,
        /// Image records committed with the same generation as the exact model summary.
        /// 和精确模型摘要一同提交到同一代次的图片记录。
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        image_excerpts: Vec<CompactedImageExcerpt>,
        /// The first still-unsent prompt when this image compaction was planned.
        /// 规划此次图片压缩时首条尚未发送的提示词。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image_replay_from_seq: Option<Seq>,
    },
}

impl CompactionContent {
    /// Validates bounded image records and the pending-image boundary of this local summary.
    /// 校验本地摘要的有界图片记录和待发送图片边界。
    pub(crate) fn validate_images(
        &self,
        retained_from: Seq,
        occurred_at: Seq,
    ) -> Result<(), DomainError> {
        let Self::Summary {
            image_excerpts,
            image_replay_from_seq,
            ..
        } = self;
        let mut seen = std::collections::BTreeSet::new();
        if image_replay_from_seq
            .is_some_and(|replay| replay < retained_from || replay >= occurred_at)
            || image_excerpts.iter().any(|image| {
                image.locator.trim().is_empty()
                    || image.sha256.len() != 64
                    || !image.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || image.source_seqs.is_empty()
                    || image.source_seqs.windows(2).any(|pair| pair[0] >= pair[1])
                    || image.source_seqs.iter().any(|seq| {
                        *seq >= occurred_at
                            || image_replay_from_seq.is_some_and(|replay| *seq >= replay)
                    })
                    || image.description.as_ref().is_some_and(|text| {
                        text.trim().is_empty() || text.len() > MAX_COMPACTED_IMAGE_DESCRIPTION_BYTES
                    })
                    || !seen.insert((&image.locator, &image.sha256))
            })
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "compaction",
                field: "image_excerpts",
            });
        }
        Ok(())
    }
}
