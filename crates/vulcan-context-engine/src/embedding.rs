//! Provider-neutral Embedding planning and vector aggregation.
//! 不绑定供应商的向量请求规划和向量聚合逻辑。

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A logical Embedding input owned by the VCE indexing or query pipeline.
/// 由 VCE 索引或查询管线拥有的逻辑向量输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingInput {
    /// Stable identity retained across batching and result alignment.
    /// 在批处理和结果对齐过程中保持不变的稳定身份。
    pub input_id: String,
    /// Text sent to the Embedding provider after VCE preprocessing.
    /// VCE 预处理后提交给向量服务的文本。
    pub text: String,
}

impl EmbeddingInput {
    /// Create a validated logical Embedding input.
    /// 创建经过校验的逻辑向量输入。
    pub fn new(
        input_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<Self, EmbeddingError> {
        let input_id = input_id.into();
        let text = text.into();
        if input_id.trim().is_empty() {
            return Err(EmbeddingError::EmptyInputId);
        }
        if text.trim().is_empty() {
            return Err(EmbeddingError::EmptyText { input_id });
        }
        Ok(Self { input_id, text })
    }
}

/// A character-preserving segment of one logical Embedding input.
/// 一个保留字符边界的逻辑向量输入片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSegment {
    /// Stable identity of the logical input that owns this segment.
    /// 拥有该片段的逻辑输入稳定身份。
    pub input_id: String,
    /// Zero-based segment ordinal within the logical input.
    /// 逻辑输入内从零开始的片段序号。
    pub ordinal: usize,
    /// Inclusive character offset of the segment start.
    /// 片段起始位置的包含式字符偏移。
    pub start_char: usize,
    /// Exclusive character offset of the segment end.
    /// 片段结束位置的排他式字符偏移。
    pub end_char: usize,
    /// Exact segment text sent to the provider.
    /// 提交给向量服务的精确片段文本。
    pub text: String,
}

impl TextSegment {
    /// Return the stable provider input identity for this segment.
    /// 返回该片段在向量请求中的稳定输入身份。
    pub fn request_input_id(&self) -> String {
        format!("{}::segment:{}", self.input_id, self.ordinal)
    }

    /// Return the number of Unicode scalar values in this segment.
    /// 返回该片段包含的 Unicode 标量值数量。
    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }
}

/// One provider-sized batch of Embedding segments.
/// 一个符合供应商限制的向量片段批次。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingBatch {
    /// Ordered segments whose result order must be preserved.
    /// 必须保持返回顺序的有序片段集合。
    pub segments: Vec<TextSegment>,
    /// Sum of Unicode scalar values in this batch.
    /// 该批次包含的 Unicode 标量值总数。
    pub total_chars: usize,
}

impl EmbeddingBatch {
    /// Return the stable provider input IDs in request order.
    /// 按请求顺序返回供应商输入稳定身份。
    pub fn request_input_ids(&self) -> Vec<String> {
        self.segments
            .iter()
            .map(TextSegment::request_input_id)
            .collect()
    }

    /// Validate public batch metadata before projecting it into a provider request.
    /// 在将公开批次元数据投影为供应商请求前校验它。
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if self.segments.is_empty() {
            return Err(EmbeddingError::InvalidPlan {
                reason: "Embedding batch contains no segments".to_owned(),
            });
        }
        let mut request_ids = BTreeSet::new();
        let mut total_chars = 0usize;
        for segment in &self.segments {
            let char_len = segment.char_len();
            total_chars =
                total_chars
                    .checked_add(char_len)
                    .ok_or_else(|| EmbeddingError::InvalidPlan {
                        reason: "Embedding batch character count overflowed".to_owned(),
                    })?;
            if segment.input_id.trim().is_empty()
                || segment.text.is_empty()
                || segment.start_char >= segment.end_char
                || segment.end_char - segment.start_char != char_len
                || !request_ids.insert(segment.request_input_id())
            {
                return Err(EmbeddingError::InvalidPlan {
                    reason: format!(
                        "Embedding segment `{}` has inconsistent identity, text, or range",
                        segment.request_input_id()
                    ),
                });
            }
        }
        if total_chars != self.total_chars {
            return Err(EmbeddingError::InvalidPlan {
                reason: format!(
                    "Embedding batch character count mismatch: expected {}, received {total_chars}",
                    self.total_chars
                ),
            });
        }
        Ok(())
    }
}

/// A deterministic plan for splitting, batching, and later aggregating Embeddings.
/// 用于切分、批处理和后续聚合向量的确定性计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingPlan {
    /// Original logical inputs in caller order.
    /// 按调用方顺序保存的原始逻辑输入。
    pub inputs: Vec<EmbeddingInput>,
    /// Provider-sized batches in stable order.
    /// 按稳定顺序排列的供应商批次。
    pub batches: Vec<EmbeddingBatch>,
}

impl EmbeddingPlan {
    /// Build a plan with explicit model item and character budgets.
    /// 使用明确的模型条数和字符预算创建向量计划。
    pub fn new(
        inputs: Vec<EmbeddingInput>,
        max_inputs: usize,
        max_chars: usize,
        max_segment_chars: usize,
        overlap_chars: usize,
    ) -> Result<Self, EmbeddingError> {
        if inputs.is_empty() {
            return Err(EmbeddingError::EmptyInputs);
        }
        if max_inputs == 0 || max_chars == 0 || max_segment_chars == 0 {
            return Err(EmbeddingError::InvalidBudget);
        }
        let effective_segment_chars = max_segment_chars.min(max_chars);
        let effective_overlap = overlap_chars.min(effective_segment_chars.saturating_sub(1));

        let mut seen_ids = BTreeSet::new();
        let mut segments = Vec::new();
        for input in &inputs {
            if input.input_id.trim().is_empty() {
                return Err(EmbeddingError::EmptyInputId);
            }
            if input.text.trim().is_empty() {
                return Err(EmbeddingError::EmptyText {
                    input_id: input.input_id.clone(),
                });
            }
            if !seen_ids.insert(input.input_id.clone()) {
                return Err(EmbeddingError::DuplicateInputId {
                    input_id: input.input_id.clone(),
                });
            }
            segments.extend(split_text(
                input,
                effective_segment_chars,
                effective_overlap,
            )?);
        }

        let mut batches = Vec::new();
        let mut current = Vec::new();
        let mut current_chars = 0usize;
        for segment in segments {
            let segment_chars = segment.char_len();
            let exceeds_items = current.len() >= max_inputs;
            let next_chars = current_chars.checked_add(segment_chars);
            let exceeds_chars =
                !current.is_empty() && next_chars.is_none_or(|total_chars| total_chars > max_chars);
            if exceeds_items || exceeds_chars {
                batches.push(EmbeddingBatch {
                    segments: current,
                    total_chars: current_chars,
                });
                current = Vec::new();
                current_chars = 0;
            }
            current_chars = current_chars
                .checked_add(segment_chars)
                .ok_or(EmbeddingError::InvalidBudget)?;
            current.push(segment);
        }
        if !current.is_empty() {
            batches.push(EmbeddingBatch {
                segments: current,
                total_chars: current_chars,
            });
        }

        let plan = Self { inputs, batches };
        plan.validate()?;
        Ok(plan)
    }

    /// Validate that public plan fields still describe the original inputs exactly once.
    /// 校验公开计划字段仍然精确且仅一次地描述原始输入。
    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if self.inputs.is_empty() || self.batches.is_empty() {
            return Err(EmbeddingError::InvalidPlan {
                reason: "Embedding plan requires inputs and batches".to_owned(),
            });
        }

        let mut input_ids = BTreeSet::new();
        let mut input_positions = BTreeMap::new();
        let mut input_characters = Vec::with_capacity(self.inputs.len());
        for (position, input) in self.inputs.iter().enumerate() {
            if input.input_id.trim().is_empty()
                || input.text.trim().is_empty()
                || !input_ids.insert(input.input_id.clone())
            {
                return Err(EmbeddingError::InvalidPlan {
                    reason: format!(
                        "Embedding input `{}` is invalid or duplicated",
                        input.input_id
                    ),
                });
            }
            input_positions.insert(input.input_id.as_str(), position);
            input_characters.push(input.text.chars().collect::<Vec<_>>());
        }

        let mut next_ordinals = vec![0usize; self.inputs.len()];
        let mut covered_ends = vec![0usize; self.inputs.len()];
        let mut last_input_position = None;
        for batch in &self.batches {
            batch.validate()?;
            for segment in &batch.segments {
                let Some(&position) = input_positions.get(segment.input_id.as_str()) else {
                    return Err(EmbeddingError::InvalidPlan {
                        reason: format!(
                            "Embedding segment `{}` references an unknown input",
                            segment.request_input_id()
                        ),
                    });
                };
                if last_input_position.is_some_and(|previous| position < previous)
                    || segment.ordinal != next_ordinals[position]
                    || (segment.ordinal == 0 && segment.start_char != 0)
                    || (segment.ordinal > 0
                        && (segment.start_char > covered_ends[position]
                            || segment.end_char <= covered_ends[position]))
                {
                    return Err(EmbeddingError::InvalidPlan {
                        reason: format!(
                            "Embedding segment `{}` is out of deterministic input order",
                            segment.request_input_id()
                        ),
                    });
                }
                let characters = &input_characters[position];
                let source_text = characters
                    .get(segment.start_char..segment.end_char)
                    .map(|slice| slice.iter().collect::<String>());
                if source_text.as_deref() != Some(segment.text.as_str()) {
                    return Err(EmbeddingError::InvalidPlan {
                        reason: format!(
                            "Embedding segment `{}` does not match its original input",
                            segment.request_input_id()
                        ),
                    });
                }
                next_ordinals[position] =
                    next_ordinals[position].checked_add(1).ok_or_else(|| {
                        EmbeddingError::InvalidPlan {
                            reason: format!(
                                "Embedding segment ordinal overflowed for `{}`",
                                segment.input_id
                            ),
                        }
                    })?;
                covered_ends[position] = segment.end_char;
                last_input_position = Some(position);
            }
        }

        if covered_ends
            .iter()
            .zip(input_characters.iter())
            .any(|(covered, characters)| *covered != characters.len())
        {
            return Err(EmbeddingError::InvalidPlan {
                reason: "Embedding plan does not cover every original input".to_owned(),
            });
        }
        Ok(())
    }

    /// Aggregate validated segment vectors into one normalized vector per logical input.
    /// 将经过校验的片段向量聚合为每个逻辑输入的一个归一化向量。
    pub fn aggregate(
        &self,
        vectors: Vec<SegmentVector>,
    ) -> Result<Vec<EmbeddingVector>, EmbeddingError> {
        self.validate()?;
        let expected_segments = self
            .batches
            .iter()
            .flat_map(|batch| batch.segments.iter())
            .map(TextSegment::request_input_id)
            .collect::<BTreeSet<_>>();
        let mut received = BTreeMap::new();
        for vector in vectors {
            let input_id = vector.input_id.clone();
            if !expected_segments.contains(&input_id) {
                return Err(EmbeddingError::UnexpectedResultId { input_id });
            }
            if received.insert(input_id.clone(), vector).is_some() {
                return Err(EmbeddingError::DuplicateResultId { input_id });
            }
        }
        if received.len() != expected_segments.len() {
            return Err(EmbeddingError::ResultCountMismatch {
                expected: expected_segments.len(),
                actual: received.len(),
            });
        }

        let mut output = Vec::with_capacity(self.inputs.len());
        for input in &self.inputs {
            let segments = self
                .batches
                .iter()
                .flat_map(|batch| batch.segments.iter())
                .filter(|segment| segment.input_id == input.input_id)
                .collect::<Vec<_>>();
            let mut segment_vectors = Vec::with_capacity(segments.len());
            for segment in segments {
                let Some(vector) = received.get(&segment.request_input_id()) else {
                    return Err(EmbeddingError::ResultCountMismatch {
                        expected: expected_segments.len(),
                        actual: received.len(),
                    });
                };
                segment_vectors.push((segment, vector.values.as_slice()));
            }
            output.push(aggregate_weighted_vectors(
                &input.input_id,
                segment_vectors,
            )?);
        }
        Ok(output)
    }
}

/// A vector returned for one provider Embedding input.
/// 一个供应商向量输入对应的返回向量。
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentVector {
    /// Stable segment request identity.
    /// 稳定的片段请求身份。
    pub input_id: String,
    /// Vector values in provider order.
    /// 按供应商顺序返回的向量值。
    pub values: Vec<f32>,
}

/// A normalized vector representing one logical VCE input.
/// 表示一个 VCE 逻辑输入的归一化向量。
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingVector {
    /// Stable logical input identity.
    /// 稳定的逻辑输入身份。
    pub input_id: String,
    /// L2-normalized vector values.
    /// 经过 L2 归一化的向量值。
    pub values: Vec<f32>,
}

/// Identify the logical and physical properties of an Embedding space.
/// 标识向量空间的逻辑和物理属性。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingSpace {
    /// Consumer-owned logical space identity.
    /// 由消费者拥有的逻辑空间身份。
    pub vector_space_id: String,
    /// Provider/model fingerprint captured by the host.
    /// 宿主捕获的供应商和模型指纹。
    pub physical_fingerprint: Option<String>,
    /// Expected vector dimension.
    /// 期望的向量维度。
    pub dimension: usize,
    /// Preprocessing version affecting vector compatibility.
    /// 影响向量兼容性的预处理版本。
    pub preprocessing_version: String,
    /// Chunker version affecting document vector compatibility.
    /// 影响文档向量兼容性的切块器版本。
    pub chunker_version: String,
}

impl EmbeddingSpace {
    /// Return whether all required compatibility fields and any supplied fingerprint are usable.
    /// 返回所有必需兼容字段及已提供指纹是否均可使用。
    pub(crate) fn is_well_formed(&self) -> bool {
        !self.vector_space_id.trim().is_empty()
            && self.dimension > 0
            && !self.preprocessing_version.trim().is_empty()
            && !self.chunker_version.trim().is_empty()
            && self
                .physical_fingerprint
                .as_ref()
                .is_none_or(|fingerprint| !fingerprint.trim().is_empty())
    }
}

/// Errors raised by Embedding planning and vector validation.
/// 向量计划和向量校验产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingError {
    /// No logical inputs were supplied.
    /// 没有提供逻辑输入。
    EmptyInputs,
    /// A logical input ID is empty.
    /// 逻辑输入身份为空。
    EmptyInputId,
    /// A logical input contains no non-whitespace text.
    /// 逻辑输入不包含非空白文本。
    EmptyText {
        /// Stable identity of the input whose text is empty.
        /// 文本为空的输入稳定身份。
        input_id: String,
    },
    /// A logical input ID occurs more than once.
    /// 逻辑输入身份重复。
    DuplicateInputId {
        /// Stable logical identity that occurred more than once.
        /// 出现多次的稳定逻辑身份。
        input_id: String,
    },
    /// A provider result ID occurs more than once.
    /// 供应商结果身份重复。
    DuplicateResultId {
        /// Provider result identity that occurred more than once.
        /// 出现多次的供应商结果身份。
        input_id: String,
    },
    /// A provider returned an ID that was not requested.
    /// 供应商返回了请求中不存在的身份。
    UnexpectedResultId {
        /// Provider result identity absent from the request.
        /// 请求中不存在的供应商结果身份。
        input_id: String,
    },
    /// The number of returned vectors differs from the number of requested segments.
    /// 返回向量数量与请求片段数量不一致。
    ResultCountMismatch {
        /// Number of vectors required by the request.
        /// 请求要求的向量数量。
        expected: usize,
        /// Number of vectors returned by the provider.
        /// 供应商实际返回的向量数量。
        actual: usize,
    },
    /// A provider vector has a different dimension from its peers.
    /// 供应商向量与其他向量的维度不一致。
    DimensionMismatch {
        /// Vector dimension established by the compatible result set.
        /// 兼容结果集确定的向量维度。
        expected: usize,
        /// Dimension of the incompatible provider vector.
        /// 不兼容供应商向量的实际维度。
        actual: usize,
    },
    /// A batch or segment budget is zero.
    /// 批次或片段预算为零。
    InvalidBudget,
    /// Public plan or batch fields no longer form a deterministic request.
    /// 公开计划或批次字段不再构成确定性请求。
    InvalidPlan {
        /// Explicit structural inconsistency in the public plan.
        /// 公开计划中的明确结构不一致。
        reason: String,
    },
    /// A weighted vector cannot be normalized because its norm is zero.
    /// 加权向量范数为零，无法归一化。
    ZeroNorm,
}

impl Display for EmbeddingError {
    /// Formats an Embedding error for deterministic diagnostics.
    /// 将向量错误格式化为确定性的诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInputs => formatter.write_str("Embedding inputs must not be empty"),
            Self::EmptyInputId => formatter.write_str("Embedding input_id must not be empty"),
            Self::EmptyText { input_id } => {
                write!(formatter, "Embedding input text is empty for {input_id}")
            }
            Self::DuplicateInputId { input_id } => {
                write!(formatter, "Embedding input_id is duplicated: {input_id}")
            }
            Self::DuplicateResultId { input_id } => {
                write!(
                    formatter,
                    "Embedding result input_id is duplicated: {input_id}"
                )
            }
            Self::UnexpectedResultId { input_id } => {
                write!(
                    formatter,
                    "Embedding result input_id was not requested: {input_id}"
                )
            }
            Self::ResultCountMismatch { expected, actual } => write!(
                formatter,
                "Embedding result count mismatch: expected {expected}, received {actual}"
            ),
            Self::DimensionMismatch { expected, actual } => write!(
                formatter,
                "Embedding vector dimension mismatch: expected {expected}, received {actual}"
            ),
            Self::InvalidBudget => {
                formatter.write_str("Embedding budgets must be greater than zero")
            }
            Self::InvalidPlan { reason } => {
                write!(formatter, "Embedding plan is invalid: {reason}")
            }
            Self::ZeroNorm => formatter.write_str("Embedding vector has zero L2 norm"),
        }
    }
}

impl Error for EmbeddingError {}

/// Split one logical input at whitespace or newline boundaries while preserving Unicode text.
/// 在空白或换行边界切分逻辑输入，同时保留 Unicode 文本。
pub fn split_text(
    input: &EmbeddingInput,
    max_chars: usize,
    overlap_chars: usize,
) -> Result<Vec<TextSegment>, EmbeddingError> {
    if max_chars == 0 {
        return Err(EmbeddingError::InvalidBudget);
    }
    let characters = input.text.chars().collect::<Vec<_>>();
    if characters.is_empty() || input.text.trim().is_empty() {
        return Err(EmbeddingError::EmptyText {
            input_id: input.input_id.clone(),
        });
    }

    let mut segments = Vec::new();
    let mut start = 0usize;
    let bounded_overlap = overlap_chars.min(max_chars.saturating_sub(1));
    while start < characters.len() {
        let hard_end = start.saturating_add(max_chars).min(characters.len());
        let end = if hard_end == characters.len() {
            hard_end
        } else {
            preferred_break(&characters, start, hard_end).unwrap_or(hard_end)
        };
        let end = end.max(start + 1);
        let text = characters[start..end].iter().collect::<String>();
        segments.push(TextSegment {
            input_id: input.input_id.clone(),
            ordinal: segments.len(),
            start_char: start,
            end_char: end,
            text,
        });
        if end == characters.len() {
            break;
        }
        let next_start = end.saturating_sub(bounded_overlap).max(start + 1);
        start = next_start;
    }
    Ok(segments)
}

/// Choose a readable break near the end of a model-sized character window.
/// 在模型窗口末尾附近选择可读的切分边界。
fn preferred_break(characters: &[char], start: usize, hard_end: usize) -> Option<usize> {
    let minimum = start + ((hard_end - start) / 2).max(1);
    (minimum..hard_end)
        .rev()
        .find(|index| characters[*index - 1].is_whitespace())
        .map(|index| index.max(start + 1))
}

/// Aggregate segment vectors with character-length weights and L2-normalize the result.
/// 按字符长度加权聚合片段向量并执行 L2 归一化。
fn aggregate_weighted_vectors(
    input_id: &str,
    segment_vectors: Vec<(&TextSegment, &[f32])>,
) -> Result<EmbeddingVector, EmbeddingError> {
    let first = segment_vectors
        .first()
        .ok_or(EmbeddingError::ResultCountMismatch {
            expected: 1,
            actual: 0,
        })?;
    let dimension = first.1.len();
    if dimension == 0 {
        return Err(EmbeddingError::DimensionMismatch {
            expected: 1,
            actual: 0,
        });
    }

    // Accumulate in f64 so finite provider values cannot overflow before normalization.
    // 使用 f64 累加，避免有限的供应商向量值在归一化前提前溢出。
    let mut aggregate = vec![0.0f64; dimension];
    for (segment, vector) in segment_vectors {
        if vector.len() != dimension {
            return Err(EmbeddingError::DimensionMismatch {
                expected: dimension,
                actual: vector.len(),
            });
        }
        let weight = segment.char_len().max(1) as f64;
        for (target, value) in aggregate.iter_mut().zip(vector.iter()) {
            *target += f64::from(*value) * weight;
        }
    }
    let norm = aggregate
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if norm == 0.0 || !norm.is_finite() {
        return Err(EmbeddingError::ZeroNorm);
    }
    let aggregate = aggregate
        .into_iter()
        .map(|value| (value / norm) as f32)
        .collect();
    Ok(EmbeddingVector {
        input_id: input_id.to_owned(),
        values: aggregate,
    })
}

/// Normalize a vector in place using its Euclidean norm.
/// 使用欧几里得范数原地归一化向量。
pub fn normalize_l2(vector: &mut [f32]) -> Result<(), EmbeddingError> {
    // Compute the norm in f64 because squaring a finite f32 can overflow f32.
    // 使用 f64 计算范数，因为有限 f32 的平方仍可能溢出 f32。
    let norm = vector
        .iter()
        .map(|value| {
            let value = f64::from(*value);
            value * value
        })
        .sum::<f64>()
        .sqrt();
    if norm == 0.0 || !norm.is_finite() {
        return Err(EmbeddingError::ZeroNorm);
    }
    for value in vector {
        *value = (f64::from(*value) / norm) as f32;
    }
    Ok(())
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Verify readable splitting, stable segment identities, and overlap.
    /// 验证可读切分、稳定片段身份和重叠行为。
    #[test]
    fn splits_at_whitespace_with_bounded_overlap() {
        let input = EmbeddingInput::new("chunk-1", "alpha beta gamma delta").unwrap();
        let segments = split_text(&input, 11, 2).unwrap();
        assert!(segments.len() >= 2);
        assert_eq!(segments[0].ordinal, 0);
        assert_eq!(segments[0].request_input_id(), "chunk-1::segment:0");
        assert!(segments[1].start_char < segments[0].end_char);
    }

    /// Verify an optional physical fingerprint is either absent or meaningful.
    /// 验证可选物理指纹只能缺省或包含有效内容。
    #[test]
    fn embedding_space_rejects_blank_optional_fingerprint() {
        let space = EmbeddingSpace {
            vector_space_id: "code-space".to_owned(),
            physical_fingerprint: Some("   ".to_owned()),
            dimension: 2,
            preprocessing_version: "pre-v1".to_owned(),
            chunker_version: "chunk-v1".to_owned(),
        };

        assert!(!space.is_well_formed());
    }

    /// Verify batch limits count provider segments rather than hidden logical inputs.
    /// 验证批次限制统计供应商片段，而不是隐藏的逻辑输入。
    #[test]
    fn plans_batches_by_item_and_character_budget() {
        let inputs = vec![
            EmbeddingInput::new("a", "one two").unwrap(),
            EmbeddingInput::new("b", "three four").unwrap(),
        ];
        let plan = EmbeddingPlan::new(inputs, 1, 100, 100, 0).unwrap();
        assert_eq!(plan.batches.len(), 2);
        assert_eq!(plan.batches[0].segments.len(), 1);
    }

    /// Verify a provider batch never admits one segment larger than its character budget.
    /// 验证供应商批次绝不会接纳超过自身字符预算的单个片段。
    #[test]
    fn segment_size_is_bounded_by_batch_character_budget() {
        let plan = EmbeddingPlan::new(
            vec![EmbeddingInput::new("a", "abcdefghij").unwrap()],
            10,
            4,
            10,
            0,
        )
        .unwrap();

        assert!(plan.batches.iter().all(|batch| batch.total_chars <= 4));
        assert!(
            plan.batches
                .iter()
                .flat_map(|batch| &batch.segments)
                .all(|segment| segment.char_len() <= 4)
        );
    }

    /// Verify weighted vectors are aligned and normalized in logical input order.
    /// 验证加权向量按逻辑输入顺序对齐并归一化。
    #[test]
    fn aggregates_and_normalizes_segment_vectors() {
        let inputs = vec![EmbeddingInput::new("a", "short").unwrap()];
        let plan = EmbeddingPlan::new(inputs, 10, 100, 100, 0).unwrap();
        let segment_id = plan.batches[0].segments[0].request_input_id();
        let output = plan
            .aggregate(vec![SegmentVector {
                input_id: segment_id,
                values: vec![3.0, 4.0],
            }])
            .unwrap();
        assert_eq!(output[0].input_id, "a");
        assert!((output[0].values[0] - 0.6).abs() < 0.0001);
        assert!((output[0].values[1] - 0.8).abs() < 0.0001);
    }

    /// Verify finite f32 components remain normalizable even when their squares exceed f32.
    /// 验证有限 f32 分量即使平方超出 f32 范围也仍可正常归一化。
    #[test]
    fn normalizes_large_finite_components_without_intermediate_overflow() {
        let mut vector = vec![1.0e20_f32, 1.0e20_f32];

        normalize_l2(&mut vector).unwrap();

        let expected = 1.0_f32 / 2.0_f32.sqrt();
        assert!((vector[0] - expected).abs() < 0.0001);
        assert!((vector[1] - expected).abs() < 0.0001);
    }

    /// Verify missing and unexpected provider result identities fail explicitly.
    /// 验证缺少或多出的供应商结果身份会显式失败。
    #[test]
    fn validates_result_identity_coverage() {
        let plan = EmbeddingPlan::new(
            vec![EmbeddingInput::new("a", "text").unwrap()],
            10,
            100,
            100,
            0,
        )
        .unwrap();
        let error = plan
            .aggregate(vec![SegmentVector {
                input_id: "not-requested".to_owned(),
                values: vec![1.0],
            }])
            .unwrap_err();
        assert!(matches!(error, EmbeddingError::UnexpectedResultId { .. }));
    }

    /// Verify direct construction cannot bypass empty-input validation in the plan.
    /// 验证直接构造输入也不能绕过计划层的空内容校验。
    #[test]
    fn plan_revalidates_public_input_fields() {
        let error = EmbeddingPlan::new(
            vec![EmbeddingInput {
                input_id: String::from("empty"),
                text: String::from("   "),
            }],
            1,
            100,
            100,
            0,
        )
        .unwrap_err();
        assert!(matches!(error, EmbeddingError::EmptyText { .. }));
    }

    /// Verify public plan and batch fields cannot detach provider segments from original inputs.
    /// 验证公开计划与批次字段不能将供应商片段从原始输入中脱离。
    #[test]
    fn aggregate_rejects_forged_public_plan_fields() {
        let mut plan = EmbeddingPlan::new(
            vec![EmbeddingInput::new("source", "alpha beta").unwrap()],
            4,
            100,
            100,
            0,
        )
        .unwrap();
        let segment_id = plan.batches[0].segments[0].request_input_id();
        plan.batches[0].segments[0].text = "forged text".to_owned();

        let error = plan
            .aggregate(vec![SegmentVector {
                input_id: segment_id,
                values: vec![1.0, 0.0],
            }])
            .unwrap_err();
        assert!(matches!(error, EmbeddingError::InvalidPlan { .. }));

        let mut plan = EmbeddingPlan::new(
            vec![EmbeddingInput::new("source", "alpha beta").unwrap()],
            4,
            100,
            100,
            0,
        )
        .unwrap();
        plan.batches[0].total_chars = 0;
        assert!(matches!(
            plan.validate(),
            Err(EmbeddingError::InvalidPlan { .. })
        ));
    }
}
