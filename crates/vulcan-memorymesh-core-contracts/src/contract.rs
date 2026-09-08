//! Closed structured-output contracts and LLM output validation.
//! 封闭结构化输出契约与 LLM 输出校验。

use crate::{
    Admission, ContextEvidenceRelation, CoreError, ErrorCode, ErrorStage, LlmOutput, LlmPurpose,
    MemoryCandidate, ProfileCandidate,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Versions the strict provider projection rules included in every output-contract digest.
/// Version 2 preserves `SchemaAst::Optional` as object-key omission instead of `required`.
/// 为纳入每个输出契约摘要的严格提供方投影规则标记版本。
pub const PROVIDER_SCHEMA_PROJECTION_VERSION: u32 = 2;

/// Identifies one closed structured-output contract owned by Core.
/// 标识一个由 Core 拥有的封闭结构化输出契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum StructuredOutputContractId {
    /// Contracts `PreCheck` intent extraction output.
    /// 约定 `PreCheck` 意图提取输出。
    PreCheckIntent = 1,
    /// Contracts `PreCheck` candidate review output.
    /// 约定 `PreCheck` 候选审核输出。
    PreCheckReview = 2,
    /// Contracts `PostAction` turn analysis output.
    /// 约定 `PostAction` 轮次分析输出。
    PostActionAnalysis = 3,
    /// Contracts `PostAction` candidate review output.
    /// 约定 `PostAction` 候选审核输出。
    PostActionReview = 6,
    /// Contracts profile instruction review output.
    /// 约定画像指令审核输出。
    ProfileInstruction = 4,
}

impl StructuredOutputContractId {
    /// Maps one LLM purpose to its only authoritative output contract.
    /// 将一个 LLM 目的映射到其唯一权威输出契约。
    #[must_use]
    pub const fn for_purpose(purpose: LlmPurpose) -> Self {
        match purpose {
            LlmPurpose::PreCheckIntent => Self::PreCheckIntent,
            LlmPurpose::PreCheckReview => Self::PreCheckReview,
            LlmPurpose::PostActionAnalysis => Self::PostActionAnalysis,
            LlmPurpose::PostActionReview => Self::PostActionReview,
            LlmPurpose::ProfileInstruction => Self::ProfileInstruction,
        }
    }

    /// Returns this contract's authoritative version used by effects, schema digests and recovery.
    /// 返回此契约的权威版本，用于效果、Schema摘要和恢复校验。
    #[must_use]
    pub const fn version(self) -> u32 {
        match self {
            Self::PostActionReview => 3,
            _ => 2,
        }
    }

    /// Returns the stable contract digest text used by checkpoints and fingerprints.
    /// 返回供检查点与指纹使用的稳定契约摘要文本。
    #[must_use]
    pub fn digest(self) -> String {
        let contract = structured_output_contract(self);
        let mut writer = crate::CanonicalWriter::new();
        writer.string("vmm-structured-output-contract-v1");
        writer.u32(self as u32);
        writer.u32(contract.version);
        writer.u32(PROVIDER_SCHEMA_PROJECTION_VERSION);
        write_schema_digest_material(&mut writer, &contract.root);
        writer.list(&contract.constraints, write_constraint_digest_material);
        let digest = Sha256::digest(&writer.buffer);
        let mut encoded = String::with_capacity(71);
        encoded.push_str("sha256:");
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(encoded, "{byte:02x}");
        }
        encoded
    }
}

/// Describes one typed schema node inside the closed contract AST.
/// 描述封闭契约 AST 中的一个强类型 Schema 节点。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SchemaAst {
    /// Describes one closed object with ordered typed fields.
    /// 描述一个具有有序强类型字段的封闭对象。
    Object {
        /// Carries fields in stable provider order.
        /// 以稳定的 Provider 顺序携带字段。
        fields: Vec<SchemaField>,
        /// States whether undeclared object fields are accepted.
        /// 表示是否接受未声明的对象字段。
        additional_properties: bool,
    },
    /// Describes a bounded UTF-8 string.
    /// 描述一个有界 UTF-8 字符串。
    String {
        /// Carries the optional minimum character count.
        /// 携带可选最小字符数。
        min_length: Option<u32>,
        /// Carries the optional maximum character count.
        /// 携带可选最大字符数。
        max_length: Option<u32>,
    },
    /// Describes a bounded signed integer.
    /// 描述一个有界有符号整数。
    Integer {
        /// Carries the optional inclusive minimum.
        /// 携带可选闭区间最小值。
        minimum: Option<i64>,
        /// Carries the optional inclusive maximum.
        /// 携带可选闭区间最大值。
        maximum: Option<i64>,
    },
    /// Describes a bounded floating-point number.
    /// 描述一个有界浮点数。
    Float {
        /// Carries the optional inclusive minimum.
        /// 携带可选闭区间最小值。
        minimum: Option<f64>,
        /// Carries the optional inclusive maximum.
        /// 携带可选闭区间最大值。
        maximum: Option<f64>,
    },
    /// Describes one boolean value.
    /// 描述一个布尔值。
    Boolean,
    /// Describes one closed set of string or integer values.
    /// 描述一组封闭的字符串或整数值。
    ClosedEnum {
        /// Carries every allowed value in stable order.
        /// 以稳定顺序携带每个允许值。
        values: Vec<SchemaEnumValue>,
    },
    /// Describes one bounded homogeneous array.
    /// 描述一个有界同构数组。
    Array {
        /// Describes every array element.
        /// 描述每个数组元素。
        items: Box<Self>,
        /// Carries the optional minimum item count.
        /// 携带可选最小元素数。
        min_items: Option<u32>,
        /// Carries the optional maximum item count.
        /// 携带可选最大元素数。
        max_items: Option<u32>,
        /// States whether equal array elements are rejected.
        /// 表示是否拒绝相等的数组元素。
        unique_items: bool,
    },
    /// Marks one field as omittable while retaining the wrapped value type.
    /// 将一个字段标记为可省略，同时保留被包装的值类型。
    Optional {
        /// Carries the wrapped value schema.
        /// 携带被包装的值 Schema。
        value: Box<Self>,
    },
    /// Allows explicit JSON null without changing whether the object key may be omitted.
    /// 允许显式JSON空值，不改变对象键是否可省略。
    Nullable {
        /// Carries the non-null value schema and all its original constraints.
        /// 携带非空值Schema及其全部原有约束。
        value: Box<Self>,
    },
}

/// Describes one ordered named field inside an object schema.
/// 描述对象 Schema 中一个有序命名字段。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SchemaField {
    /// Carries the exact Provider JSON field name.
    /// 携带精确的 Provider JSON 字段名。
    pub name: String,
    /// Carries the field value schema; `Optional` means omission is allowed.
    /// 携带字段值 Schema；`Optional` 表示允许省略。
    pub value: SchemaAst,
}

/// Describes one value in a closed schema enumeration.
/// 描述封闭 Schema 枚举中的一个值。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaEnumValue {
    /// Carries one allowed string.
    /// 携带一个允许的字符串。
    String(String),
    /// Carries one allowed integer.
    /// 携带一个允许的整数。
    Integer(i64),
}

/// Describes one typed cross-field rule that cannot be expressed by a field node alone.
/// 描述一个无法仅由字段节点表达的强类型跨字段规则。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossFieldConstraint {
    /// Couples one boolean flag with whether one array is empty.
    /// 将一个布尔标志与一个数组是否为空建立约束。
    BooleanArrayCoupling {
        /// Names the boolean field path.
        /// 命名布尔字段路径。
        flag_path: String,
        /// Names the array field path.
        /// 命名数组字段路径。
        array_path: String,
    },
    /// Requires an output field to echo one exact typed input field.
    /// 要求一个输出字段回显一个精确的强类型输入字段。
    EchoInput {
        /// Names the output field path.
        /// 命名输出字段路径。
        output_path: String,
        /// Names the typed input field path.
        /// 命名强类型输入字段路径。
        input_path: String,
    },
    /// Requires accepted and rejected indexes to form a disjoint in-range partition.
    /// 要求接纳与拒绝索引构成互斥且不越界的划分。
    CandidatePartition {
        /// Names the accepted index path.
        /// 命名接纳索引路径。
        accepted_path: String,
        /// Names the rejected index path.
        /// 命名拒绝索引路径。
        rejected_path: String,
        /// Names the typed input collection that bounds the indexes.
        /// 命名约束索引范围的强类型输入集合。
        input_collection_path: String,
    },
    /// Requires every output identity to reference a visible typed input identity.
    /// 要求每个输出身份引用一个可见的强类型输入身份。
    ReferenceSubset {
        /// Names the output identity path.
        /// 命名输出身份路径。
        output_path: String,
        /// Names the typed input identity collection.
        /// 命名强类型输入身份集合。
        input_collection_path: String,
    },
}

/// Describes one complete closed output contract.
/// 描述一个完整的封闭输出契约。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructuredOutputContract {
    /// Identifies the contract.
    /// 标识契约。
    pub id: StructuredOutputContractId,
    /// Carries the frozen contract version.
    /// 携带冻结的契约版本。
    pub version: u32,
    /// Carries the root schema for the typed output.
    /// 携带强类型输出的根 Schema。
    pub root: SchemaAst,
    /// Carries the bounded cross-field constraints enforced on decoded outputs.
    /// 携带在解码输出上强制执行的有界跨字段约束。
    pub constraints: Vec<CrossFieldConstraint>,
}

/// Returns the authoritative contract for one purpose.
/// 返回一个目的的权威契约。
#[must_use]
pub fn structured_output_contract(id: StructuredOutputContractId) -> StructuredOutputContract {
    match id {
        StructuredOutputContractId::PreCheckIntent => precheck_intent_contract(),
        StructuredOutputContractId::PreCheckReview => precheck_review_contract(),
        StructuredOutputContractId::PostActionAnalysis => postaction_analysis_contract(),
        StructuredOutputContractId::PostActionReview => postaction_review_contract(),
        StructuredOutputContractId::ProfileInstruction => profile_instruction_contract(),
    }
}

/// Validates one decoded output against its purpose contract.
/// 针对其目的契约校验一个已解码输出。
///
/// # Errors
/// Returns a stable error when the output violates the closed contract.
/// 当输出违反封闭契约时返回稳定错误。
#[allow(clippy::too_many_lines)]
pub fn validate_llm_output(purpose: LlmPurpose, output: &LlmOutput) -> Result<(), CoreError> {
    if StructuredOutputContractId::for_purpose(purpose) != output_contract_of(output) {
        return Err(contract_violation(format!(
            "LLM output does not match the contract for purpose {purpose:?}"
        )));
    }
    match (purpose, output) {
        (
            LlmPurpose::PreCheckIntent,
            LlmOutput::PreCheckIntent {
                reason,
                need_memory,
                queries,
            },
        ) => {
            if !text_in_range(reason, 1, 512) {
                return Err(contract_violation(
                    "PreCheckIntent reason violates the contract bound".to_owned(),
                ));
            }
            let unique_queries = queries.iter().collect::<std::collections::BTreeSet<_>>();
            if queries.len() > 64
                || unique_queries.len() != queries.len()
                || queries.iter().any(|query| !text_in_range(query, 1, 2_048))
                || (*need_memory && queries.is_empty())
                || (!*need_memory && !queries.is_empty())
            {
                return Err(contract_violation(
                    "PreCheckIntent need-memory and query constraints were violated".to_owned(),
                ));
            }
            Ok(())
        }
        (
            LlmPurpose::PreCheckReview,
            LlmOutput::PreCheckReview {
                accepted_memory_ids,
            },
        ) => {
            let mut seen = std::collections::BTreeSet::new();
            for memory_id in accepted_memory_ids {
                if !seen.insert(memory_id.to_string()) {
                    return Err(contract_violation(
                        "PreCheckReview accepted identities must be unique".to_owned(),
                    ));
                }
            }
            if accepted_memory_ids.len() > 256 {
                return Err(contract_violation(
                    "PreCheckReview accepted identities exceed the contract bound".to_owned(),
                ));
            }
            Ok(())
        }
        (
            LlmPurpose::PostActionAnalysis,
            LlmOutput::PostActionAnalysis {
                details,
                memories,
                profiles,
                context_evidence,
                ..
            },
        ) => {
            if !text_in_range(details, 0, 8_192)
                || memories.len() > 64
                || profiles.len() > 64
                || context_evidence.len() > 4_096
            {
                return Err(contract_violation(
                    "PostActionAnalysis candidate counts exceed the contract bounds".to_owned(),
                ));
            }
            validate_memory_candidates(memories)?;
            validate_profile_candidates(profiles)?;
            let mut evidence_per_candidate = vec![0_usize; memories.len()];
            for evidence in context_evidence {
                let index = usize::try_from(evidence.candidate_index).map_err(|error| {
                    contract_violation(format!(
                        "PostActionAnalysis context evidence index overflow: {error}"
                    ))
                })?;
                let Some(count) = evidence_per_candidate.get_mut(index) else {
                    return Err(contract_violation(
                        "PostActionAnalysis context evidence index is out of range".to_owned(),
                    ));
                };
                *count += 1;
                if *count > 64
                    || !text_in_range(&evidence.context_key, 1, 256)
                    || !text_in_range(&evidence.context_value, 1, 1_024)
                {
                    return Err(contract_violation(
                        "PostActionAnalysis context evidence label is empty".to_owned(),
                    ));
                }
                if !matches!(
                    evidence.relation,
                    ContextEvidenceRelation::Support | ContextEvidenceRelation::Rebuttal
                ) {
                    return Err(contract_violation(
                        "PostActionAnalysis context evidence relation is unknown".to_owned(),
                    ));
                }
            }
            Ok(())
        }
        (
            LlmPurpose::PostActionReview,
            LlmOutput::PostActionReview {
                accepted_candidates,
                dropped_candidates,
                accepted_profile_candidates,
                invalid_profile_candidate_indexes,
                retired_profile_nodes,
                reason,
            },
        ) => {
            if accepted_candidates.len() > 64
                || dropped_candidates.len() > 64
                || accepted_profile_candidates.len() > 64
                || invalid_profile_candidate_indexes.len() > 64
                || retired_profile_nodes.len() > 128
                || reason
                    .as_ref()
                    .is_some_and(|value| !text_in_range(value, 1, 6_200))
            {
                return Err(contract_violation(
                    "PostActionReview result exceeds the contract bounds".to_owned(),
                ));
            }
            let mut seen = std::collections::BTreeSet::new();
            for accepted in accepted_candidates {
                let supersede_unique = accepted
                    .supersede_memory_ids
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>();
                if !seen.insert(accepted.candidate_index)
                    || accepted.candidate_index > 63
                    || accepted.supersede_memory_ids.len() > 16
                    || supersede_unique.len() != accepted.supersede_memory_ids.len()
                    || accepted
                        .supersede_memory_ids
                        .iter()
                        .any(|identity| crate::MemoryId::new(identity).is_err())
                {
                    return Err(contract_violation(
                        "PostActionReview accepted indexes and supersede identities violate the closed contract".to_owned(),
                    ));
                }
            }
            for dropped in dropped_candidates {
                if dropped.candidate_index > 63 || !seen.insert(dropped.candidate_index) {
                    return Err(contract_violation(
                        "PostActionReview indexes must not overlap accept and drop".to_owned(),
                    ));
                }
                if dropped
                    .dedupe_memory_id
                    .as_ref()
                    .is_some_and(|identity| crate::MemoryId::new(identity).is_err())
                {
                    return Err(contract_violation(
                        "PostActionReview supplied dedupe identity is invalid".to_owned(),
                    ));
                }
                if dropped
                    .reason
                    .as_ref()
                    .is_some_and(|value| !text_in_range(value, 0, 2_048))
                {
                    return Err(contract_violation(
                        "PostActionReview dropped candidate reason exceeds the closed contract"
                            .to_owned(),
                    ));
                }
            }
            let mut profile_seen = std::collections::BTreeSet::new();
            for accepted in accepted_profile_candidates {
                if !profile_seen.insert(accepted.candidate_index)
                    || accepted.candidate_index > 63
                    || !text_in_range(&accepted.normalized_content, 1, 2_048)
                    || !matches!(accepted.priority, 20 | 60 | 100)
                    || accepted
                        .level_reason
                        .as_ref()
                        .is_some_and(|value| !text_in_range(value, 1, 1_024))
                    || accepted.supersede_nodes.len() > 16
                {
                    return Err(contract_violation(
                        "PostActionReview accepted profile candidates violate the closed contract"
                            .to_owned(),
                    ));
                }
            }
            for index in invalid_profile_candidate_indexes {
                if *index > 63 || !profile_seen.insert(*index) {
                    return Err(contract_violation(
                        "PostActionReview profile indexes must not overlap accept and invalid"
                            .to_owned(),
                    ));
                }
            }
            let mut retired_seen = std::collections::BTreeSet::new();
            for accepted in accepted_profile_candidates {
                for retired in &accepted.supersede_nodes {
                    if !text_in_range(&retired.reason, 0, 2_048)
                        || !retired_seen.insert(retired.node_id.clone())
                    {
                        return Err(contract_violation(
                            "PostActionReview profile retirements must be unique across all decisions"
                                .to_owned(),
                        ));
                    }
                }
            }
            for retired in retired_profile_nodes {
                if !text_in_range(&retired.reason, 0, 2_048)
                    || !retired_seen.insert(retired.node_id.clone())
                {
                    return Err(contract_violation(
                        "PostActionReview retired profile identities must be unique".to_owned(),
                    ));
                }
            }
            Ok(())
        }
        (
            LlmPurpose::ProfileInstruction,
            LlmOutput::ProfileInstruction {
                accepted_nodes,
                retired_nodes,
                review_reason,
            },
        ) => {
            if accepted_nodes.len() > 64
                || retired_nodes.len() > 64
                || !text_in_range(review_reason, 0, 2_048)
            {
                return Err(contract_violation(
                    "ProfileInstruction candidate counts exceed the contract bounds".to_owned(),
                ));
            }
            let mut retired_seen = std::collections::BTreeSet::new();
            for retired in retired_nodes {
                if !text_in_range(&retired.reason, 0, 1_024)
                    || !retired_seen.insert(retired.node_id.clone())
                {
                    return Err(contract_violation(
                        "ProfileInstruction retired identities must be unique".to_owned(),
                    ));
                }
            }
            for accepted in accepted_nodes {
                if !text_in_range(&accepted.content, 1, 2_048)
                    || !matches!(accepted.priority, 20 | 60 | 100)
                    || accepted
                        .level_reason
                        .as_ref()
                        .is_some_and(|value| !text_in_range(value, 1, 1_024))
                    || accepted.supersede_nodes.len() > 16
                {
                    return Err(contract_violation(
                        "ProfileInstruction accepted nodes violate the closed contract".to_owned(),
                    ));
                }
                for retired in &accepted.supersede_nodes {
                    if !text_in_range(&retired.reason, 0, 1_024)
                        || !retired_seen.insert(retired.node_id.clone())
                    {
                        return Err(contract_violation(
                            "ProfileInstruction retirements must be unique across all decisions"
                                .to_owned(),
                        ));
                    }
                }
            }
            Ok(())
        }
        _ => Err(contract_violation(format!(
            "LLM output variant does not match purpose {purpose:?}"
        ))),
    }
}

/// Maps an output variant back to its owning contract.
/// 将输出变体映射回其归属契约。
const fn output_contract_of(output: &LlmOutput) -> StructuredOutputContractId {
    match output {
        LlmOutput::PreCheckIntent { .. } => StructuredOutputContractId::PreCheckIntent,
        LlmOutput::PreCheckReview { .. } => StructuredOutputContractId::PreCheckReview,
        LlmOutput::PostActionAnalysis { .. } => StructuredOutputContractId::PostActionAnalysis,
        LlmOutput::PostActionReview { .. } => StructuredOutputContractId::PostActionReview,
        LlmOutput::ProfileInstruction { .. } => StructuredOutputContractId::ProfileInstruction,
    }
}

fn validate_memory_candidates(memories: &[MemoryCandidate]) -> Result<(), CoreError> {
    for candidate in memories {
        if !text_in_range(&candidate.summary, 1, 2_048)
            || !text_in_range(&candidate.content, 1, 8_192)
            || !valid_evidence_source(&candidate.evidence_source)
            || !matches!(candidate.admission, Admission::Accept | Admission::Drop)
            || !valid_admission_reason(candidate.admission, candidate.admission_reason.as_deref())
        {
            return Err(contract_violation(
                "memory candidate violates the closed admission contract".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_profile_candidates(profiles: &[ProfileCandidate]) -> Result<(), CoreError> {
    for candidate in profiles {
        if !text_in_range(&candidate.content, 1, 2_048)
            || !valid_evidence_source(&candidate.evidence_source)
            || !matches!(candidate.admission, Admission::Accept | Admission::Drop)
            || !valid_admission_reason(candidate.admission, candidate.admission_reason.as_deref())
        {
            return Err(contract_violation(
                "profile candidate violates the closed contract".to_owned(),
            ));
        }
    }
    Ok(())
}

/// Reports whether one evidence-source label belongs to the authoritative VMM set.
/// 判断一个证据来源标签是否属于 VMM 权威集合。
fn valid_evidence_source(source: &str) -> bool {
    matches!(
        source.trim(),
        "user_asserted"
            | "user_confirmed"
            | "assistant_recalled_memory"
            | "assistant_recalled_profile"
            | "assistant_general_knowledge"
            | "assistant_external_research"
            | "assistant_tool_discovered"
            | "mixed"
    )
}

/// Reports whether an internal admission decision preserves the VMM keep/drop reason coupling.
/// 判断内部准入决定是否保持 VMM keep/drop 与原因的配对约束。
fn valid_admission_reason(admission: Admission, reason: Option<&str>) -> bool {
    match admission {
        Admission::Accept => reason.is_none_or(|value| value.trim().is_empty()),
        Admission::Drop => matches!(
            reason.map(str::trim),
            Some(
                "qa_answer_only"
                    | "derived_from_existing_memory"
                    | "derived_from_profile_echo"
                    | "general_knowledge_answer"
                    | "non_durable"
            )
        ),
    }
}

/// Reports whether trimmed Unicode text stays within one inclusive character bound.
/// 判断裁剪后的 Unicode 文本是否处于一个闭区间字符长度范围内。
fn text_in_range(value: &str, minimum: usize, maximum: usize) -> bool {
    let length = value.trim().chars().count();
    let maximum_utf8_bytes = maximum.saturating_mul(4);
    (minimum..=maximum).contains(&length)
        && value.len() <= maximum_utf8_bytes
        && !value.contains('\0')
}

fn contract_violation(diagnostic: String) -> CoreError {
    CoreError::new(
        ErrorCode::StructuredOutputContractViolation,
        ErrorStage::Effect,
        diagnostic,
    )
}

/// Builds the frozen `PreCheck` intent contract.
/// 构建冻结的 `PreCheck` 意图契约。
fn precheck_intent_contract() -> StructuredOutputContract {
    StructuredOutputContract {
        id: StructuredOutputContractId::PreCheckIntent,
        version: StructuredOutputContractId::PreCheckIntent.version(),
        root: object(vec![
            field("reason", string(Some(1), Some(512))),
            field("need_memory", SchemaAst::Boolean),
            field("queries", array(string(Some(1), Some(2_048)), 0, 64, true)),
        ]),
        constraints: vec![CrossFieldConstraint::BooleanArrayCoupling {
            flag_path: "need_memory".to_owned(),
            array_path: "queries".to_owned(),
        }],
    }
}

/// Builds the frozen `PreCheck` candidate-review contract.
/// 构建冻结的 `PreCheck` 候选审核契约。
fn precheck_review_contract() -> StructuredOutputContract {
    StructuredOutputContract {
        id: StructuredOutputContractId::PreCheckReview,
        version: StructuredOutputContractId::PreCheckReview.version(),
        root: object(vec![field(
            "selected_candidate_numbers",
            array(integer(Some(1), Some(256)), 0, 256, true),
        )]),
        constraints: vec![CrossFieldConstraint::ReferenceSubset {
            output_path: "selected_candidate_numbers[]".to_owned(),
            input_collection_path: "input.candidates[]".to_owned(),
        }],
    }
}

/// Builds the frozen `PostAction` analysis contract.
/// 构建冻结的 `PostAction` 分析契约。
fn postaction_analysis_contract() -> StructuredOutputContract {
    let evidence_source = closed_strings(&[
        "user_asserted",
        "user_confirmed",
        "assistant_recalled_memory",
        "assistant_recalled_profile",
        "assistant_general_knowledge",
        "assistant_external_research",
        "assistant_tool_discovered",
        "mixed",
    ]);
    let context_edge = object(vec![
        field("context_key", string(Some(1), Some(256))),
        field("context_value", string(Some(1), Some(1_024))),
        field("relation", closed_strings(&["support", "rebuttal"])),
    ]);
    let memory_node = object(vec![
        field("category", closed_integers(&[0, 1, 2, 3, 4, 5, 6, 7])),
        field("abstract", string(Some(1), Some(2_048))),
        field("details", optional(string(Some(0), Some(8_192)))),
        field("evidence_source", evidence_source.clone()),
        field("admission", closed_strings(&["keep", "drop"])),
        field("admission_reason", string(Some(0), Some(1_024))),
        field("context_edges", optional(array(context_edge, 0, 64, false))),
    ]);
    let profile_node = object(vec![
        field("profile_type", closed_integers(&[0, 1])),
        field("content", string(Some(1), Some(2_048))),
        field("evidence_source", evidence_source),
        field("admission", closed_strings(&["keep", "drop"])),
        field("admission_reason", string(Some(0), Some(1_024))),
    ]);
    StructuredOutputContract {
        id: StructuredOutputContractId::PostActionAnalysis,
        version: StructuredOutputContractId::PostActionAnalysis.version(),
        root: object(vec![
            field(
                "user_input_kind",
                closed_strings(&["question", "statement", "mixed"]),
            ),
            field("turn_id", string(Some(1), Some(512))),
            field("details", optional(string(Some(0), Some(8_192)))),
            field("memory_nodes", optional(array(memory_node, 0, 64, false))),
            field("profile_nodes", optional(array(profile_node, 0, 64, false))),
        ]),
        constraints: vec![CrossFieldConstraint::EchoInput {
            output_path: "turn_id".to_owned(),
            input_path: "input.target_turn.id".to_owned(),
        }],
    }
}

/// Builds the frozen `PostAction` candidate-review contract.
/// 构建冻结的 `PostAction` 候选审核契约。
fn postaction_review_contract() -> StructuredOutputContract {
    let memory_accepted = object(vec![
        field("candidate_index", integer(Some(0), Some(63))),
        field(
            "supersede_memory_ids",
            optional(array(string(Some(1), Some(512)), 0, 16, true)),
        ),
    ]);
    let memory_dropped = object(vec![
        field("candidate_index", integer(Some(0), Some(63))),
        field("dedupe_memory_id", optional(string(Some(1), Some(512)))),
    ]);
    let memory_section = object(vec![
        field(
            "accepted_candidates",
            optional(array(memory_accepted, 0, 64, false)),
        ),
        field(
            "dropped_candidates",
            optional(array(memory_dropped, 0, 64, false)),
        ),
        field("reason", optional(string(Some(0), Some(2_048)))),
    ]);
    let profile_accepted = object(vec![
        field("candidate_index", integer(Some(0), Some(63))),
        field("normalized_content", string(Some(1), Some(2_048))),
        field("priority", closed_strings(&["P0", "P1", "P2"])),
        field("level", closed_strings(&["L0", "L1", "L2", "L3"])),
        field("level_reason", optional(string(Some(0), Some(1_024)))),
        field(
            "supersede_node_ids",
            optional(array(string(Some(1), Some(512)), 0, 16, true)),
        ),
    ]);
    let profile_section = object(vec![
        field(
            "accepted_candidates",
            optional(array(profile_accepted, 0, 64, false)),
        ),
        field(
            "invalid_candidate_indexes",
            optional(array(integer(Some(0), Some(63)), 0, 64, true)),
        ),
        field(
            "retire_only_node_ids",
            optional(array(string(Some(1), Some(512)), 0, 64, true)),
        ),
        field("reason", optional(string(Some(0), Some(2_048)))),
    ]);
    StructuredOutputContract {
        id: StructuredOutputContractId::PostActionReview,
        version: StructuredOutputContractId::PostActionReview.version(),
        root: object(vec![
            field("memory", optional(memory_section)),
            field("user", optional(profile_section.clone())),
            field("project", optional(profile_section)),
        ]),
        constraints: vec![
            CrossFieldConstraint::CandidatePartition {
                accepted_path: "memory.accepted_candidates[].candidate_index".to_owned(),
                rejected_path: "memory.dropped_candidates[].candidate_index".to_owned(),
                input_collection_path: "input.candidates[]".to_owned(),
            },
            CrossFieldConstraint::ReferenceSubset {
                output_path: "*.supersede_*_ids[]".to_owned(),
                input_collection_path: "input.visible_active_entities[]".to_owned(),
            },
        ],
    }
}

/// Builds the frozen profile-instruction contract.
/// 构建冻结的画像指令契约。
fn profile_instruction_contract() -> StructuredOutputContract {
    let retired = object(vec![
        field("node_id", string(Some(1), Some(512))),
        field("reason", optional(string(Some(0), Some(1_024)))),
    ]);
    let accepted = object(vec![
        field("normalized_content", string(Some(1), Some(2_048))),
        field("priority", closed_strings(&["P0", "P1", "P2"])),
        field("level", closed_strings(&["L0", "L1", "L2", "L3"])),
        field("level_reason", optional(string(Some(0), Some(1_024)))),
        field(
            "supersede_nodes",
            optional(array(retired.clone(), 0, 16, false)),
        ),
    ]);
    StructuredOutputContract {
        id: StructuredOutputContractId::ProfileInstruction,
        version: StructuredOutputContractId::ProfileInstruction.version(),
        root: object(vec![
            field("accepted_nodes", optional(array(accepted, 0, 64, false))),
            field("retired_nodes", optional(array(retired, 0, 64, false))),
            field("reason", optional(string(Some(0), Some(2_048)))),
        ]),
        constraints: vec![CrossFieldConstraint::ReferenceSubset {
            output_path: "accepted_nodes[].supersede_nodes[].node_id|retired_nodes[].node_id"
                .to_owned(),
            input_collection_path: "input.current_nodes[].id".to_owned(),
        }],
    }
}

/// Builds one closed object schema.
/// 构建一个封闭对象 Schema。
const fn object(fields: Vec<SchemaField>) -> SchemaAst {
    SchemaAst::Object {
        fields,
        additional_properties: false,
    }
}

/// Builds one named schema field.
/// 构建一个命名 Schema 字段。
fn field(name: &str, value: SchemaAst) -> SchemaField {
    SchemaField {
        name: name.to_owned(),
        value,
    }
}

/// Builds one bounded string schema.
/// 构建一个有界字符串 Schema。
const fn string(min_length: Option<u32>, max_length: Option<u32>) -> SchemaAst {
    SchemaAst::String {
        min_length,
        max_length,
    }
}

/// Builds one bounded integer schema.
/// 构建一个有界整数 Schema。
const fn integer(minimum: Option<i64>, maximum: Option<i64>) -> SchemaAst {
    SchemaAst::Integer { minimum, maximum }
}

/// Builds one closed string enumeration schema.
/// 构建一个封闭字符串枚举 Schema。
fn closed_strings(values: &[&str]) -> SchemaAst {
    SchemaAst::ClosedEnum {
        values: values
            .iter()
            .map(|value| SchemaEnumValue::String((*value).to_owned()))
            .collect(),
    }
}

/// Builds one closed integer enumeration schema.
/// 构建一个封闭整数枚举 Schema。
fn closed_integers(values: &[i64]) -> SchemaAst {
    SchemaAst::ClosedEnum {
        values: values
            .iter()
            .copied()
            .map(SchemaEnumValue::Integer)
            .collect(),
    }
}

/// Builds one bounded array schema.
/// 构建一个有界数组 Schema。
fn array(items: SchemaAst, min_items: u32, max_items: u32, unique_items: bool) -> SchemaAst {
    SchemaAst::Array {
        items: Box::new(items),
        min_items: Some(min_items),
        max_items: Some(max_items),
        unique_items,
    }
}

/// Marks a source-omissible field as nullable too, preserving its existing default under strict provider output.
/// 将来源可省略字段同时标为可空，使严格提供方输出保持原有缺省语义。
fn optional(value: SchemaAst) -> SchemaAst {
    SchemaAst::Optional {
        value: Box::new(SchemaAst::Nullable {
            value: Box::new(value),
        }),
    }
}

/// Writes one schema node into canonical contract-digest material.
/// 将一个 Schema 节点写入规范契约摘要材料。
fn write_schema_digest_material(writer: &mut crate::CanonicalWriter, schema: &SchemaAst) {
    match schema {
        SchemaAst::Object {
            fields,
            additional_properties,
        } => {
            writer.u32(1);
            writer.bool(*additional_properties);
            writer.list(fields, |inner, field| {
                inner.string(&field.name);
                write_schema_digest_material(inner, &field.value);
            });
        }
        SchemaAst::String {
            min_length,
            max_length,
        } => {
            writer.u32(2);
            writer.optional(*min_length, crate::CanonicalWriter::u32);
            writer.optional(*max_length, crate::CanonicalWriter::u32);
        }
        SchemaAst::Integer { minimum, maximum } => {
            writer.u32(3);
            writer.optional(*minimum, crate::CanonicalWriter::i64);
            writer.optional(*maximum, crate::CanonicalWriter::i64);
        }
        SchemaAst::Float { minimum, maximum } => {
            writer.u32(4);
            writer.optional(*minimum, |inner, value| inner.u64(value.to_bits()));
            writer.optional(*maximum, |inner, value| inner.u64(value.to_bits()));
        }
        SchemaAst::Boolean => writer.u32(5),
        SchemaAst::ClosedEnum { values } => {
            writer.u32(6);
            writer.list(values, |inner, value| match value {
                SchemaEnumValue::String(value) => {
                    inner.u32(1);
                    inner.string(value);
                }
                SchemaEnumValue::Integer(value) => {
                    inner.u32(2);
                    inner.i64(*value);
                }
            });
        }
        SchemaAst::Array {
            items,
            min_items,
            max_items,
            unique_items,
        } => {
            writer.u32(7);
            write_schema_digest_material(writer, items);
            writer.optional(*min_items, crate::CanonicalWriter::u32);
            writer.optional(*max_items, crate::CanonicalWriter::u32);
            writer.bool(*unique_items);
        }
        SchemaAst::Optional { value } => {
            writer.u32(8);
            write_schema_digest_material(writer, value);
        }
        SchemaAst::Nullable { value } => {
            writer.u32(9);
            write_schema_digest_material(writer, value);
        }
    }
}

/// Writes one typed cross-field rule into canonical contract-digest material.
/// 将一个强类型跨字段规则写入规范契约摘要材料。
fn write_constraint_digest_material(
    writer: &mut crate::CanonicalWriter,
    constraint: &CrossFieldConstraint,
) {
    match constraint {
        CrossFieldConstraint::BooleanArrayCoupling {
            flag_path,
            array_path,
        } => {
            writer.u32(1);
            writer.string(flag_path);
            writer.string(array_path);
        }
        CrossFieldConstraint::EchoInput {
            output_path,
            input_path,
        } => {
            writer.u32(2);
            writer.string(output_path);
            writer.string(input_path);
        }
        CrossFieldConstraint::CandidatePartition {
            accepted_path,
            rejected_path,
            input_collection_path,
        } => {
            writer.u32(3);
            writer.string(accepted_path);
            writer.string(rejected_path);
            writer.string(input_collection_path);
        }
        CrossFieldConstraint::ReferenceSubset {
            output_path,
            input_collection_path,
        } => {
            writer.u32(4);
            writer.string(output_path);
            writer.string(input_collection_path);
        }
    }
}
