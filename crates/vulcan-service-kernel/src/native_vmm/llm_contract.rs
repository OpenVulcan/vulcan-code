//! Native VMM JSON Schema projection and strict Host AI response decoding.
//! 原生 VMM Core LLM Effect 的宿主 AI JSON Schema 投影与严格响应解码。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_memorymesh_core::{
    AcceptedCandidate, AcceptedProfileCandidate, AcceptedProfileNode, Admission,
    ContextEvidenceCandidate, ContextEvidenceRelation, CoreError, DroppedCandidate, ErrorCode,
    ErrorStage, LlmEffect, LlmInput, LlmOutput, LlmPurpose, MemoryCandidate, MemoryCategory,
    MemoryReviewCandidate, ProfileCandidate, ProfileLevel, ProfileNodeId, RetiredProfileNode,
    SchemaAst, SchemaEnumValue, SectionRole, StructuredOutputContractId, SupersedeNodeRef, TurnId,
    TurnInputKind, structured_output_contract, validate_llm_output,
};

/// Limits one raw Host response before any JSON allocation or traversal.
/// 在任何 JSON 分配或遍历前限制一份原始 Host 响应。
const MAX_HOST_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// Decodes null only on explicitly annotated source-default fields, preserving the same typed default as omission.
/// 仅在显式标记的来源缺省字段上解码null，保留与省略相同的强类型默认值；错误类型仍返回解码错误。
fn deserialize_optional_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

/// Projects a contract into strict host Schema; all keys are required and source-optional values are nullable.
/// 将契约投影为严格提供方Schema；全部键必填，来源可选值允许空值，业务约束仍由Core强制校验。
///
/// # Errors
/// Returns a stable contract error if the AST contains a mixed or empty closed enumeration.
/// 当 AST 包含混合类型或空的封闭枚举时返回稳定契约错误。
pub(super) fn host_json_schema(id: StructuredOutputContractId) -> Result<Vec<u8>, CoreError> {
    let contract = structured_output_contract(id);
    let schema = project_schema(&contract.root)?;
    serde_json::to_vec(&schema)
        .map_err(|error| host_contract_error(format!("failed to encode Host JSON Schema: {error}")))
}

/// Strictly decodes one raw Host JSON response for the exact typed LLM effect.
/// 为精确的强类型 LLM Effect 严格解码一份 Host 原始 JSON 响应。
///
/// # Errors
/// Returns `StructuredOutputContractViolation` for identity drift, malformed JSON, unknown
/// fields, invalid enums, out-of-range references, or cross-field contract violations.
/// 当身份漂移、JSON 损坏、未知字段、无效枚举、越界引用或跨字段契约违规时返回
/// `StructuredOutputContractViolation`。
pub(super) fn parse_host_response(
    effect: &LlmEffect,
    bytes: &[u8],
) -> Result<LlmOutput, CoreError> {
    validate_effect_contract_identity(effect)?;
    if bytes.len() > MAX_HOST_RESPONSE_BYTES {
        return Err(host_contract_error(
            "Host response exceeds the 4 MiB boundary".to_owned(),
        ));
    }
    let output = match effect.purpose {
        LlmPurpose::PreCheckIntent => parse_precheck_intent(bytes)?,
        LlmPurpose::PreCheckReview => parse_precheck_review(effect, bytes)?,
        LlmPurpose::PostActionAnalysis => parse_postaction_analysis(bytes)?,
        LlmPurpose::PostActionReview => parse_postaction_review(effect, bytes)?,
        LlmPurpose::ProfileInstruction => parse_profile_instruction(bytes)?,
    };
    validate_llm_output(effect.purpose, &output)?;
    validate_output_against_input(effect, &output)?;
    Ok(output)
}

/// Carries a serializable closed JSON Schema node without untyped runtime maps.
/// 携带一个不使用非类型化运行时 Map 的可序列化封闭 JSON Schema 节点。
#[derive(Clone, Debug, Default, Serialize)]
struct HostJsonSchema {
    /// Carries the root Schema dialect URI.
    /// 携带根 Schema 方言 URI。
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    schema_uri: Option<&'static str>,
    /// Carries the JSON value type.
    /// 携带 JSON 值类型。
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    type_name: Option<&'static str>,
    /// Carries ordered object properties.
    /// 携带有序对象属性。
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    properties: BTreeMap<String, Self>,
    /// Carries required object property names.
    /// 携带对象必填属性名。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    required: Vec<String>,
    /// Closes or opens undeclared object properties.
    /// 关闭或开放未声明对象属性。
    #[serde(
        rename = "additionalProperties",
        skip_serializing_if = "Option::is_none"
    )]
    additional_properties: Option<bool>,
    /// Describes homogeneous array elements.
    /// 描述同构数组元素。
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<Box<Self>>,
    /// Carries the minimum array length.
    /// 携带数组最小长度。
    #[serde(rename = "minItems", skip_serializing_if = "Option::is_none")]
    min_items: Option<u32>,
    /// Carries the maximum array length.
    /// 携带数组最大长度。
    #[serde(rename = "maxItems", skip_serializing_if = "Option::is_none")]
    max_items: Option<u32>,
    /// Enforces array element uniqueness.
    /// 强制数组元素唯一。
    #[serde(rename = "uniqueItems", skip_serializing_if = "Option::is_none")]
    unique_items: Option<bool>,
    /// Carries the minimum string length.
    /// 携带字符串最小长度。
    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    min_length: Option<u32>,
    /// Carries the maximum string length.
    /// 携带字符串最大长度。
    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    max_length: Option<u32>,
    /// Carries an inclusive numeric minimum.
    /// 携带闭区间数值最小值。
    #[serde(skip_serializing_if = "Option::is_none")]
    minimum: Option<HostNumber>,
    /// Carries an inclusive numeric maximum.
    /// 携带闭区间数值最大值。
    #[serde(skip_serializing_if = "Option::is_none")]
    maximum: Option<HostNumber>,
    /// Carries a homogeneous closed enumeration.
    /// 携带同构封闭枚举。
    #[serde(rename = "enum", skip_serializing_if = "Vec::is_empty")]
    enum_values: Vec<HostEnumValue>,
    /// Carries explicit alternative value schemas, keeping nullability separate from key omission.
    /// 携带显式备选值Schema，将可空性与键省略区分。
    #[serde(rename = "anyOf", skip_serializing_if = "Vec::is_empty")]
    any_of: Vec<Self>,
}

/// Carries one strongly classified Host enum value.
/// 携带一个强分类的 Host 枚举值。
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
enum HostEnumValue {
    /// Carries one string enum value.
    /// 携带一个字符串枚举值。
    String(String),
    /// Carries one integer enum value.
    /// 携带一个整数枚举值。
    Integer(i64),
}

/// Carries one exact integer or floating-point JSON Schema boundary.
/// 携带一个精确的整数或浮点 JSON Schema 边界。
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(untagged)]
enum HostNumber {
    /// Carries one exact signed integer boundary.
    /// 携带一个精确的有符号整数边界。
    Integer(i64),
    /// Carries one floating-point boundary.
    /// 携带一个浮点边界。
    Float(f64),
}

/// Projects one typed AST node to its serializable Host representation.
/// 将一个强类型 AST 节点投影到可序列化的 Host 表示。
fn project_schema(schema: &SchemaAst) -> Result<HostJsonSchema, CoreError> {
    match schema {
        SchemaAst::Object {
            fields,
            additional_properties,
        } => {
            let mut projected = HostJsonSchema {
                type_name: Some("object"),
                additional_properties: Some(*additional_properties),
                ..HostJsonSchema::default()
            };
            for field in fields {
                let value = project_schema(&field.value)?;
                projected.required.push(field.name.clone());
                if projected
                    .properties
                    .insert(field.name.clone(), value)
                    .is_some()
                {
                    return Err(host_contract_error(format!(
                        "duplicate Schema property {}",
                        field.name
                    )));
                }
            }
            Ok(projected)
        }
        // String bounds and uniqueness remain authoritative local checks rather than unsupported host hints.
        // 字符串边界与唯一性保留为本地权威校验，不依赖提供方未承诺的提示关键字。
        SchemaAst::String { .. } => Ok(HostJsonSchema {
            type_name: Some("string"),
            ..HostJsonSchema::default()
        }),
        SchemaAst::Integer { minimum, maximum } => Ok(HostJsonSchema {
            type_name: Some("integer"),
            minimum: minimum.map(HostNumber::Integer),
            maximum: maximum.map(HostNumber::Integer),
            ..HostJsonSchema::default()
        }),
        SchemaAst::Float { minimum, maximum } => Ok(HostJsonSchema {
            type_name: Some("number"),
            minimum: minimum.map(HostNumber::Float),
            maximum: maximum.map(HostNumber::Float),
            ..HostJsonSchema::default()
        }),
        SchemaAst::Boolean => Ok(HostJsonSchema {
            type_name: Some("boolean"),
            ..HostJsonSchema::default()
        }),
        SchemaAst::ClosedEnum { values } => project_closed_enum(values),
        SchemaAst::Array {
            items,
            min_items,
            max_items,
            unique_items: _,
        } => Ok(HostJsonSchema {
            type_name: Some("array"),
            items: Some(Box::new(project_schema(items)?)),
            min_items: *min_items,
            max_items: *max_items,
            ..HostJsonSchema::default()
        }),
        SchemaAst::Optional { value } => project_schema(value),
        SchemaAst::Nullable { value } => Ok(HostJsonSchema {
            any_of: vec![
                project_schema(value)?,
                HostJsonSchema {
                    type_name: Some("null"),
                    ..HostJsonSchema::default()
                },
            ],
            ..HostJsonSchema::default()
        }),
    }
}

/// Projects one homogeneous closed enumeration.
/// 投影一个同构封闭枚举。
fn project_closed_enum(values: &[SchemaEnumValue]) -> Result<HostJsonSchema, CoreError> {
    let Some(first) = values.first() else {
        return Err(host_contract_error(
            "closed enumeration must not be empty".to_owned(),
        ));
    };
    let string_kind = matches!(first, SchemaEnumValue::String(_));
    let mut projected = Vec::with_capacity(values.len());
    for value in values {
        match value {
            SchemaEnumValue::String(value) if string_kind => {
                projected.push(HostEnumValue::String(value.clone()));
            }
            SchemaEnumValue::Integer(value) if !string_kind => {
                projected.push(HostEnumValue::Integer(*value));
            }
            _ => {
                return Err(host_contract_error(
                    "closed enumeration must contain one value type".to_owned(),
                ));
            }
        }
    }
    Ok(HostJsonSchema {
        type_name: Some(if string_kind { "string" } else { "integer" }),
        enum_values: projected,
        ..HostJsonSchema::default()
    })
}

/// Validates the purpose, contract, version, and digest bound to one effect.
/// 校验一个 Effect 绑定的用途、契约、版本与摘要。
fn validate_effect_contract_identity(effect: &LlmEffect) -> Result<(), CoreError> {
    let expected = StructuredOutputContractId::for_purpose(effect.purpose);
    if effect.output_contract != expected
        || effect.output_contract_version != expected.version()
        || effect.output_contract_digest != expected.digest()
    {
        return Err(host_contract_error(
            "LLM effect structured-output identity does not match the authoritative contract"
                .to_owned(),
        ));
    }
    Ok(())
}

/// Validates output references and partitions against the exact typed effect input.
/// 针对精确的强类型 Effect 输入校验输出引用与划分。
fn validate_output_against_input(effect: &LlmEffect, output: &LlmOutput) -> Result<(), CoreError> {
    match (&effect.input, output) {
        (
            LlmInput::PostActionAnalysis { target_turn, .. },
            LlmOutput::PostActionAnalysis { turn_id, .. },
        ) => {
            if target_turn.id != *turn_id {
                return Err(host_contract_error(
                    "PostActionAnalysis turn_id does not echo the target turn".to_owned(),
                ));
            }
        }
        (
            LlmInput::PostActionReview {
                context: _,
                memory_candidates,
                profile_candidates,
                current_profile_nodes,
            },
            LlmOutput::PostActionReview {
                accepted_candidates,
                dropped_candidates,
                accepted_profile_candidates,
                invalid_profile_candidate_indexes,
                retired_profile_nodes,
                ..
            },
        ) => {
            validate_memory_review_partition(
                memory_candidates,
                accepted_candidates,
                dropped_candidates,
            )?;
            validate_profile_review_partition(
                profile_candidates,
                current_profile_nodes,
                accepted_profile_candidates,
                invalid_profile_candidate_indexes,
                retired_profile_nodes,
            )?;
        }
        (
            LlmInput::ProfileInstruction { current_nodes, .. },
            LlmOutput::ProfileInstruction {
                accepted_nodes,
                retired_nodes,
                ..
            },
        ) => {
            validate_profile_references(
                current_nodes,
                accepted_nodes
                    .iter()
                    .flat_map(|value| value.supersede_nodes.iter().map(|node| &node.node_id))
                    .chain(retired_nodes.iter().map(|node| &node.node_id)),
            )?;
        }
        (
            LlmInput::PreCheckIntent {
                max_search_queries, ..
            },
            LlmOutput::PreCheckIntent { queries, .. },
        ) => {
            if queries.len() > *max_search_queries as usize {
                return Err(host_contract_error(
                    "PreCheckIntent exceeds the effect query budget".to_owned(),
                ));
            }
        }
        (LlmInput::PreCheckReview { .. }, LlmOutput::PreCheckReview { .. }) => {}
        _ => {
            return Err(host_contract_error(
                "LLM output cannot be validated against the effect input variant".to_owned(),
            ));
        }
    }
    Ok(())
}

/// Validates one complete memory-review partition and every candidate-local memory reference.
/// 校验一个完整记忆审核分区及每个候选局部记忆引用。
fn validate_memory_review_partition(
    candidates: &[MemoryReviewCandidate],
    accepted: &[AcceptedCandidate],
    dropped: &[DroppedCandidate],
) -> Result<(), CoreError> {
    let mut partition = BTreeSet::new();
    for index in accepted
        .iter()
        .map(|value| value.candidate_index)
        .chain(dropped.iter().map(|value| value.candidate_index))
    {
        if usize::try_from(index).map_or(true, |value| value >= candidates.len())
            || !partition.insert(index)
        {
            return Err(host_contract_error(
                "PostActionReview memory partition is duplicated or out of range".to_owned(),
            ));
        }
    }
    if partition.len() != candidates.len() {
        return Err(host_contract_error(
            "PostActionReview must classify every memory candidate exactly once".to_owned(),
        ));
    }
    for decision in accepted {
        let visible = candidate_local_memory_ids(candidates, decision.candidate_index)?;
        if decision
            .supersede_memory_ids
            .iter()
            .any(|identity| !visible.contains(identity.as_str()))
        {
            return Err(host_contract_error(
                "PostActionReview supersede reference belongs to another candidate".to_owned(),
            ));
        }
    }
    for decision in dropped {
        let visible = candidate_local_memory_ids(candidates, decision.candidate_index)?;
        if decision
            .dedupe_memory_id
            .as_ref()
            .is_some_and(|identity| !visible.contains(identity.as_str()))
        {
            return Err(host_contract_error(
                "PostActionReview dedupe reference belongs to another candidate".to_owned(),
            ));
        }
    }
    Ok(())
}

/// Returns the visible memory identities belonging to one exact review candidate.
/// 返回属于一个精确审核候选的可见记忆身份。
fn candidate_local_memory_ids(
    candidates: &[MemoryReviewCandidate],
    candidate_index: u32,
) -> Result<BTreeSet<&str>, CoreError> {
    let index = usize::try_from(candidate_index).map_err(|error| {
        host_contract_error(format!(
            "PostActionReview candidate index overflow: {error}"
        ))
    })?;
    let candidate = candidates.get(index).ok_or_else(|| {
        host_contract_error("PostActionReview candidate index is out of range".to_owned())
    })?;
    Ok(candidate
        .similar_memories
        .iter()
        .map(|hit| hit.memory.id.as_str())
        .collect())
}

/// Validates one complete profile-review partition and every visible profile reference.
/// 校验一个完整画像审核分区及每个可见画像引用。
fn validate_profile_review_partition(
    candidates: &[ProfileCandidate],
    current_nodes: &[vulcan_memorymesh_core::ProfileNode],
    accepted: &[AcceptedProfileCandidate],
    invalid_indexes: &[u32],
    retired: &[RetiredProfileNode],
) -> Result<(), CoreError> {
    let mut partition = BTreeSet::new();
    for index in accepted
        .iter()
        .map(|value| value.candidate_index)
        .chain(invalid_indexes.iter().copied())
    {
        if usize::try_from(index).map_or(true, |value| value >= candidates.len())
            || !partition.insert(index)
        {
            return Err(host_contract_error(
                "PostActionReview profile partition is duplicated or out of range".to_owned(),
            ));
        }
    }
    if partition.len() != candidates.len() {
        return Err(host_contract_error(
            "PostActionReview must classify every profile candidate exactly once".to_owned(),
        ));
    }
    validate_profile_references(
        current_nodes,
        accepted
            .iter()
            .flat_map(|value| value.supersede_nodes.iter().map(|node| &node.node_id))
            .chain(retired.iter().map(|node| &node.node_id)),
    )
}

/// Validates every referenced profile identity against the visible active input set.
/// 针对可见活跃输入集合校验每个被引用画像身份。
fn validate_profile_references<'a>(
    current_nodes: &[vulcan_memorymesh_core::ProfileNode],
    references: impl Iterator<Item = &'a ProfileNodeId>,
) -> Result<(), CoreError> {
    let visible = current_nodes
        .iter()
        .map(|node| &node.id)
        .collect::<std::collections::BTreeSet<_>>();
    if references.into_iter().any(|value| !visible.contains(value)) {
        return Err(host_contract_error(
            "profile review references a node outside the visible active set".to_owned(),
        ));
    }
    Ok(())
}

/// Strictly decodes one JSON document into a closed response DTO.
/// 将一份 JSON 文档严格解码为封闭响应 DTO。
fn decode_json<T>(bytes: &[u8], label: &str) -> Result<T, CoreError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_slice(bytes)
        .map_err(|error| host_contract_error(format!("invalid {label} Host response: {error}")))
}

/// Decodes one `PreCheck` intent response.
/// 解码一份 `PreCheck` 意图响应。
fn parse_precheck_intent(bytes: &[u8]) -> Result<LlmOutput, CoreError> {
    let raw: PreCheckIntentResponse = decode_json(bytes, "PreCheckIntent")?;
    Ok(LlmOutput::PreCheckIntent {
        reason: raw.reason,
        need_memory: raw.need_memory,
        queries: raw.queries,
    })
}

/// Decodes one `PreCheck` review and binds one-based candidate numbers to typed memory IDs.
/// 解码一份 `PreCheck` 审核，并把从一开始的候选编号绑定为强类型记忆 ID。
fn parse_precheck_review(effect: &LlmEffect, bytes: &[u8]) -> Result<LlmOutput, CoreError> {
    let raw: PreCheckReviewResponse = decode_json(bytes, "PreCheckReview")?;
    let LlmInput::PreCheckReview { candidates, .. } = &effect.input else {
        return Err(host_contract_error(
            "PreCheckReview effect carries a mismatched input variant".to_owned(),
        ));
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut accepted = Vec::with_capacity(raw.selected_candidate_numbers.len());
    for number in raw.selected_candidate_numbers {
        if number == 0 || number > 256 || !seen.insert(number) {
            return Err(host_contract_error(
                "PreCheckReview candidate numbers must be unique and one-based".to_owned(),
            ));
        }
        let index = usize::try_from(number - 1).map_err(|error| {
            host_contract_error(format!("invalid PreCheckReview candidate number: {error}"))
        })?;
        let hit = candidates.get(index).ok_or_else(|| {
            host_contract_error("PreCheckReview candidate number is out of range".to_owned())
        })?;
        accepted.push(hit.memory_id.clone());
    }
    Ok(LlmOutput::PreCheckReview {
        accepted_memory_ids: accepted,
    })
}

/// Decodes one `PostAction` L1 analysis response.
/// 解码一份 `PostAction` L1 分析响应。
fn parse_postaction_analysis(bytes: &[u8]) -> Result<LlmOutput, CoreError> {
    let raw: PostActionAnalysisResponse = decode_json(bytes, "PostActionAnalysis")?;
    let user_input_kind = match raw.user_input_kind.as_str() {
        "question" => TurnInputKind::Question,
        "statement" => TurnInputKind::Statement,
        "mixed" => TurnInputKind::Mixed,
        _ => {
            return Err(host_contract_error(
                "PostActionAnalysis user_input_kind is unknown".to_owned(),
            ));
        }
    };
    let turn_id = typed_turn_id(raw.turn_id)?;
    let mut memories = Vec::with_capacity(raw.memory_nodes.len());
    let mut context_evidence = Vec::new();
    for (index, memory) in raw.memory_nodes.into_iter().enumerate() {
        validate_raw_text(&memory.details, 8_192)?;
        let category = MemoryCategory::try_from_u32(memory.category).ok_or_else(|| {
            host_contract_error("PostActionAnalysis memory category is unknown".to_owned())
        })?;
        let admission = parse_admission(&memory.admission)?;
        let admission_reason = parse_admission_reason(admission, memory.admission_reason)?;
        let candidate_index = u32::try_from(index)
            .map_err(|error| host_contract_error(format!("too many memory candidates: {error}")))?;
        for edge in memory.context_edges {
            context_evidence.push(ContextEvidenceCandidate {
                candidate_index,
                context_key: edge.context_key,
                context_value: edge.context_value,
                relation: parse_relation(&edge.relation)?,
            });
        }
        let summary = memory.r#abstract;
        let content = if memory.details.trim().is_empty() {
            summary.clone()
        } else {
            memory.details
        };
        memories.push(MemoryCandidate {
            category,
            summary,
            content,
            evidence_source: memory.evidence_source,
            admission,
            admission_reason,
        });
    }
    let mut profiles = Vec::with_capacity(raw.profile_nodes.len());
    for profile in raw.profile_nodes {
        let section_role = match profile.profile_type {
            0 => SectionRole::Subject,
            1 => SectionRole::Environment,
            _ => {
                return Err(host_contract_error(
                    "PostActionAnalysis profile_type is unknown".to_owned(),
                ));
            }
        };
        let admission = parse_admission(&profile.admission)?;
        let admission_reason = parse_admission_reason(admission, profile.admission_reason)?;
        profiles.push(ProfileCandidate {
            section_role,
            content: profile.content,
            evidence_source: profile.evidence_source,
            admission,
            admission_reason,
        });
    }
    Ok(LlmOutput::PostActionAnalysis {
        user_input_kind,
        turn_id,
        details: raw.details,
        memories,
        profiles,
        context_evidence,
    })
}

/// Decodes one nested `PostAction` L2 review and restores Core-global profile indexes.
/// 解码一份嵌套 `PostAction` L2 审核，并恢复 Core 全局画像索引。
fn parse_postaction_review(effect: &LlmEffect, bytes: &[u8]) -> Result<LlmOutput, CoreError> {
    let raw: PostActionReviewResponse = decode_json(bytes, "PostActionReview")?;
    let LlmInput::PostActionReview {
        profile_candidates, ..
    } = &effect.input
    else {
        return Err(host_contract_error(
            "PostActionReview effect carries a mismatched input variant".to_owned(),
        ));
    };
    let subject_indexes = profile_indexes(profile_candidates, SectionRole::Subject)?;
    let environment_indexes = profile_indexes(profile_candidates, SectionRole::Environment)?;
    let mut accepted_candidates = Vec::new();
    let mut dropped_candidates = Vec::new();
    let mut reasons = Vec::new();
    if let Some(memory) = raw.memory {
        validate_raw_text(&memory.reason, 2_048)?;
        push_reason(&mut reasons, "memory", &memory.reason);
        for accepted in memory.accepted_candidates {
            accepted_candidates.push(AcceptedCandidate {
                candidate_index: checked_index(accepted.candidate_index)?,
                supersede_memory_ids: accepted.supersede_memory_ids,
            });
        }
        for dropped in memory.dropped_candidates {
            dropped_candidates.push(DroppedCandidate {
                candidate_index: checked_index(dropped.candidate_index)?,
                dedupe_memory_id: dropped.dedupe_memory_id,
                reason: nonempty_optional(&memory.reason),
            });
        }
    }
    let mut accepted_profile_candidates = Vec::new();
    let mut invalid_profile_candidate_indexes = Vec::new();
    let mut retired_profile_nodes = Vec::new();
    if let Some(user) = raw.user {
        push_reason(&mut reasons, "user", &user.reason);
        append_profile_review(
            user,
            &subject_indexes,
            &mut accepted_profile_candidates,
            &mut invalid_profile_candidate_indexes,
            &mut retired_profile_nodes,
        )?;
    }
    if let Some(project) = raw.project {
        push_reason(&mut reasons, "project", &project.reason);
        append_profile_review(
            project,
            &environment_indexes,
            &mut accepted_profile_candidates,
            &mut invalid_profile_candidate_indexes,
            &mut retired_profile_nodes,
        )?;
    }
    Ok(LlmOutput::PostActionReview {
        accepted_candidates,
        dropped_candidates,
        accepted_profile_candidates,
        invalid_profile_candidate_indexes,
        retired_profile_nodes,
        reason: (!reasons.is_empty()).then(|| reasons.join("; ")),
    })
}

/// Decodes one manual profile instruction review.
/// 解码一份手工画像指令审核。
fn parse_profile_instruction(bytes: &[u8]) -> Result<LlmOutput, CoreError> {
    let raw: ProfileInstructionResponse = decode_json(bytes, "ProfileInstruction")?;
    let mut accepted_nodes = Vec::with_capacity(raw.accepted_nodes.len());
    for accepted in raw.accepted_nodes {
        validate_raw_text(&accepted.level_reason, 1_024)?;
        accepted_nodes.push(AcceptedProfileNode {
            content: accepted.normalized_content,
            priority: parse_priority(&accepted.priority)?,
            profile_level: parse_level(&accepted.level)?,
            level_reason: nonempty_optional(&accepted.level_reason),
            supersede_nodes: accepted
                .supersede_nodes
                .into_iter()
                .map(parse_supersede_node)
                .collect::<Result<Vec<_>, _>>()?,
        });
    }
    let retired_nodes = raw
        .retired_nodes
        .into_iter()
        .map(parse_retired_node)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LlmOutput::ProfileInstruction {
        accepted_nodes,
        retired_nodes,
        review_reason: raw.reason,
    })
}

/// Appends one role-local profile review after mapping indexes to the Core-global list.
/// 把一个角色局部画像审核的索引映射为 Core 全局列表后追加。
fn append_profile_review(
    section: ProfileReviewSection,
    global_indexes: &[u32],
    accepted: &mut Vec<AcceptedProfileCandidate>,
    invalid: &mut Vec<u32>,
    retired: &mut Vec<RetiredProfileNode>,
) -> Result<(), CoreError> {
    validate_raw_text(&section.reason, 2_048)?;
    if section.retire_only_node_ids.len() > 64 {
        return Err(host_contract_error(
            "profile section retirement list exceeds its bound".to_owned(),
        ));
    }
    let retirement_reason = section.reason.trim().to_owned();
    for candidate in section.accepted_candidates {
        validate_raw_text(&candidate.level_reason, 1_024)?;
        let global_index = role_local_index(global_indexes, candidate.candidate_index)?;
        let supersede_nodes = candidate
            .supersede_node_ids
            .into_iter()
            .map(|node_id| {
                Ok(SupersedeNodeRef {
                    node_id: typed_profile_node_id(node_id)?,
                    reason: retirement_reason.clone(),
                })
            })
            .collect::<Result<Vec<_>, CoreError>>()?;
        accepted.push(AcceptedProfileCandidate {
            candidate_index: global_index,
            normalized_content: candidate.normalized_content,
            priority: parse_priority(&candidate.priority)?,
            profile_level: parse_level(&candidate.level)?,
            level_reason: nonempty_optional(&candidate.level_reason),
            supersede_nodes,
        });
    }
    for index in section.invalid_candidate_indexes {
        invalid.push(role_local_index(global_indexes, index)?);
    }
    for node_id in section.retire_only_node_ids {
        retired.push(RetiredProfileNode {
            node_id: typed_profile_node_id(node_id)?,
            reason: retirement_reason.clone(),
        });
    }
    Ok(())
}

/// Builds the stable global indexes owned by one profile role.
/// 构建一个画像角色拥有的稳定全局索引。
fn profile_indexes(
    candidates: &[ProfileCandidate],
    role: SectionRole,
) -> Result<Vec<u32>, CoreError> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.section_role == role)
        .map(|(index, _)| {
            u32::try_from(index).map_err(|error| {
                host_contract_error(format!("profile candidate index overflow: {error}"))
            })
        })
        .collect()
}

/// Maps one role-local candidate index to its Core-global index.
/// 将一个角色局部候选索引映射为 Core 全局索引。
fn role_local_index(global_indexes: &[u32], local: u32) -> Result<u32, CoreError> {
    let index = usize::try_from(local).map_err(|error| {
        host_contract_error(format!("invalid profile candidate index: {error}"))
    })?;
    global_indexes
        .get(index)
        .copied()
        .ok_or_else(|| host_contract_error("profile candidate index is out of range".to_owned()))
}

/// Converts one nonnegative raw candidate index to the public index width.
/// 将一个非负原始候选索引转换为公共索引宽度。
fn checked_index(index: u32) -> Result<u32, CoreError> {
    if index > 63 {
        Err(host_contract_error(
            "memory candidate index is out of range".to_owned(),
        ))
    } else {
        Ok(index)
    }
}

/// Parses one closed admission label.
/// 解析一个封闭准入标签。
fn parse_admission(value: &str) -> Result<Admission, CoreError> {
    match value {
        "keep" => Ok(Admission::Accept),
        "drop" => Ok(Admission::Drop),
        _ => Err(host_contract_error(
            "candidate admission is unknown".to_owned(),
        )),
    }
}

/// Validates the required VMM admission reason and maps an empty keep reason to `None`.
/// 校验 VMM 必填准入原因，并把 keep 对应的空原因映射为 `None`。
fn parse_admission_reason(
    admission: Admission,
    value: Option<String>,
) -> Result<Option<String>, CoreError> {
    let value = value
        .ok_or_else(|| host_contract_error("candidate admission_reason is required".to_owned()))?;
    validate_raw_text(&value, 1_024)?;
    let normalized = value.trim();
    match admission {
        Admission::Accept if normalized.is_empty() => Ok(None),
        Admission::Drop
            if matches!(
                normalized,
                "qa_answer_only"
                    | "derived_from_existing_memory"
                    | "derived_from_profile_echo"
                    | "general_knowledge_answer"
                    | "non_durable"
            ) =>
        {
            Ok(Some(normalized.to_owned()))
        }
        _ => Err(host_contract_error(
            "candidate admission_reason does not match admission".to_owned(),
        )),
    }
}

/// Parses one closed context-evidence relation label.
/// 解析一个封闭情境证据关系标签。
fn parse_relation(value: &str) -> Result<ContextEvidenceRelation, CoreError> {
    match value {
        "support" => Ok(ContextEvidenceRelation::Support),
        "rebuttal" => Ok(ContextEvidenceRelation::Rebuttal),
        _ => Err(host_contract_error(
            "context evidence relation is unknown".to_owned(),
        )),
    }
}

/// Maps one VMM priority label to the Core numeric priority scale.
/// 将一个 VMM 优先级标签映射到 Core 数值优先级刻度。
fn parse_priority(value: &str) -> Result<u32, CoreError> {
    match value {
        "P0" => Ok(100),
        "P1" => Ok(60),
        "P2" => Ok(20),
        _ => Err(host_contract_error(
            "profile priority is unknown".to_owned(),
        )),
    }
}

/// Maps one VMM lifecycle label to the Core profile level.
/// 将一个 VMM 生命周期标签映射到 Core 画像等级。
fn parse_level(value: &str) -> Result<ProfileLevel, CoreError> {
    match value {
        "L0" => Ok(ProfileLevel::Transient),
        "L1" => Ok(ProfileLevel::Situational),
        "L2" => Ok(ProfileLevel::Stable),
        "L3" => Ok(ProfileLevel::Persistent),
        _ => Err(host_contract_error("profile level is unknown".to_owned())),
    }
}

/// Converts one raw supersede DTO into a typed reference.
/// 将一个原始替代 DTO 转换为强类型引用。
fn parse_supersede_node(value: ProfileNodeReason) -> Result<SupersedeNodeRef, CoreError> {
    validate_raw_text(&value.reason, 1_024)?;
    Ok(SupersedeNodeRef {
        node_id: typed_profile_node_id(value.node_id)?,
        reason: value.reason.trim().to_owned(),
    })
}

/// Converts one raw retirement DTO into a typed retirement.
/// 将一个原始退役 DTO 转换为强类型退役。
fn parse_retired_node(value: ProfileNodeReason) -> Result<RetiredProfileNode, CoreError> {
    validate_raw_text(&value.reason, 1_024)?;
    Ok(RetiredProfileNode {
        node_id: typed_profile_node_id(value.node_id)?,
        reason: value.reason.trim().to_owned(),
    })
}

/// Validates and converts one opaque turn identity.
/// 校验并转换一个不透明轮次身份。
fn typed_turn_id(value: String) -> Result<TurnId, CoreError> {
    TurnId::new(value).map_err(|error| host_contract_error(error.to_string()))
}

/// Validates and converts one opaque profile-node identity.
/// 校验并转换一个不透明画像节点身份。
fn typed_profile_node_id(value: String) -> Result<ProfileNodeId, CoreError> {
    ProfileNodeId::new(value).map_err(|error| host_contract_error(error.to_string()))
}

/// Checks optional raw text before trimming, fallback, or section assembly can erase its bounds.
/// 在裁剪、回退或审核块合并抹去原始边界前，检查可选文本的字符与字节上界。
fn validate_raw_text(value: &str, maximum: usize) -> Result<(), CoreError> {
    if value.contains('\0') || value.len() > maximum * 4 || value.trim().chars().count() > maximum {
        return Err(host_contract_error(
            "raw Host text violates its field bound".to_owned(),
        ));
    }
    Ok(())
}

/// Returns one trimmed optional string only when it is nonempty.
/// 仅当字符串去空白后非空时返回可选字符串。
fn nonempty_optional(value: &str) -> Option<String> {
    let normalized = value.trim();
    (!normalized.is_empty()).then(|| normalized.to_owned())
}

/// Appends one labeled nonempty audit reason.
/// 追加一个带标签的非空审计原因。
fn push_reason(target: &mut Vec<String>, label: &str, reason: &str) {
    if let Some(reason) = nonempty_optional(reason) {
        target.push(format!("{label}: {reason}"));
    }
}

/// Builds the stable Core error for every Host boundary contract failure.
/// 为每个 Host 边界契约失败构建稳定 Core 错误。
fn host_contract_error(diagnostic: String) -> CoreError {
    CoreError::new(
        ErrorCode::StructuredOutputContractViolation,
        ErrorStage::Effect,
        diagnostic,
    )
}

/// Carries the exact `PreCheck` L1 Host response.
/// 携带精确的 `PreCheck` L1 Host 响应。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreCheckIntentResponse {
    /// Carries the concise decision reason.
    /// 携带简短决策原因。
    reason: String,
    /// States whether recall is needed.
    /// 表示是否需要召回。
    need_memory: bool,
    /// Carries generated recall queries.
    /// 携带生成的召回查询。
    queries: Vec<String>,
}

/// Carries the exact `PreCheck` L2 Host response.
/// 携带精确的 `PreCheck` L2 Host 响应。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreCheckReviewResponse {
    /// Carries one-based selected candidate numbers.
    /// 携带从一开始的已选候选编号。
    selected_candidate_numbers: Vec<u32>,
}

/// Carries the exact `PostAction` L1 Host response.
/// 携带精确的 `PostAction` L1 Host 响应。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostActionAnalysisResponse {
    /// Carries the dominant input kind label.
    /// 携带主导输入类型标签。
    user_input_kind: String,
    /// Echoes the target turn identity.
    /// 回显目标轮次身份。
    turn_id: String,
    /// Carries compact turn details.
    /// 携带紧凑轮次详情。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    details: String,
    /// Carries extracted memory nodes.
    /// 携带提取出的记忆节点。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    memory_nodes: Vec<PostActionMemoryNode>,
    /// Carries extracted profile nodes.
    /// 携带提取出的画像节点。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    profile_nodes: Vec<PostActionProfileNode>,
}

/// Carries one exact `PostAction` L1 memory candidate.
/// 携带一条精确的 `PostAction` L1 记忆候选。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostActionMemoryNode {
    /// Carries the authoritative VMM category discriminant.
    /// 携带权威 VMM 分类判别值。
    category: u32,
    /// Carries the compact abstract.
    /// 携带紧凑摘要。
    r#abstract: String,
    /// Carries complete details.
    /// 携带完整详情。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    details: String,
    /// Carries the evidence source label.
    /// 携带证据来源标签。
    evidence_source: String,
    /// Carries the admission label.
    /// 携带准入标签。
    admission: String,
    /// Carries the optional admission reason.
    /// 携带可选准入原因。
    admission_reason: Option<String>,
    /// Carries embedded context evidence observations.
    /// 携带内嵌情境证据观测。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    context_edges: Vec<PostActionContextEdge>,
}

/// Carries one exact context edge inside a `PostAction` L1 candidate.
/// 携带 `PostAction` L1 候选中的一条精确情境边。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostActionContextEdge {
    /// Carries the context key.
    /// 携带情境键。
    context_key: String,
    /// Carries the context value.
    /// 携带情境值。
    context_value: String,
    /// Carries the relation label.
    /// 携带关系标签。
    relation: String,
}

/// Carries one exact `PostAction` L1 profile candidate.
/// 携带一条精确的 `PostAction` L1 画像候选。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostActionProfileNode {
    /// Carries the VMM user/project profile type.
    /// 携带 VMM 用户/项目画像类型。
    profile_type: u32,
    /// Carries atomic profile content.
    /// 携带原子画像内容。
    content: String,
    /// Carries the evidence source label.
    /// 携带证据来源标签。
    evidence_source: String,
    /// Carries the admission label.
    /// 携带准入标签。
    admission: String,
    /// Carries the optional admission reason.
    /// 携带可选准入原因。
    admission_reason: Option<String>,
}

/// Carries the exact nested `PostAction` L2 Host response.
/// 携带精确的嵌套 `PostAction` L2 Host 响应。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostActionReviewResponse {
    /// Carries the optional memory review block.
    /// 携带可选记忆审核块。
    memory: Option<MemoryReviewSection>,
    /// Carries the optional user-profile review block.
    /// 携带可选用户画像审核块。
    user: Option<ProfileReviewSection>,
    /// Carries the optional project-profile review block.
    /// 携带可选项目画像审核块。
    project: Option<ProfileReviewSection>,
}

/// Carries one memory block in a `PostAction` L2 response.
/// 携带 `PostAction` L2 响应中的一个记忆块。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryReviewSection {
    /// Carries accepted memory candidates.
    /// 携带已接纳记忆候选。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    accepted_candidates: Vec<MemoryAccepted>,
    /// Carries dropped memory candidates.
    /// 携带被丢弃记忆候选。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    dropped_candidates: Vec<MemoryDropped>,
    /// Carries the block reason.
    /// 携带该块原因。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    reason: String,
}

/// Carries one accepted memory candidate in a `PostAction` L2 response.
/// 携带 `PostAction` L2 响应中的一条已接纳记忆候选。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryAccepted {
    /// Carries the zero-based candidate index.
    /// 携带从零开始的候选索引。
    candidate_index: u32,
    /// Carries exact superseded memory identities.
    /// 携带精确被替代记忆身份。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    supersede_memory_ids: Vec<String>,
}

/// Carries one dropped memory candidate in a `PostAction` L2 response.
/// 携带 `PostAction` L2 响应中的一条被丢弃记忆候选。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryDropped {
    /// Carries the zero-based candidate index.
    /// 携带从零开始的候选索引。
    candidate_index: u32,
    /// Carries an optional existing target; explicit invalid identities are still rejected after decoding.
    /// 携带可选既有目标；显式非法身份仍在解码后被拒绝。
    dedupe_memory_id: Option<String>,
}

/// Carries one user or project profile block in a `PostAction` L2 response.
/// 携带 `PostAction` L2 响应中的一个用户或项目画像块。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileReviewSection {
    /// Carries accepted profile candidates.
    /// 携带已接纳画像候选。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    accepted_candidates: Vec<ProfileAccepted>,
    /// Carries rejected role-local indexes.
    /// 携带被拒绝的角色局部索引。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    invalid_candidate_indexes: Vec<u32>,
    /// Carries active nodes retired without replacement.
    /// 携带无替代项而被退役的活跃节点。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    retire_only_node_ids: Vec<String>,
    /// Carries the block reason.
    /// 携带该块原因。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    reason: String,
}

/// Carries one accepted profile candidate in a `PostAction` L2 response.
/// 携带 `PostAction` L2 响应中的一条已接纳画像候选。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileAccepted {
    /// Carries the role-local candidate index.
    /// 携带角色局部候选索引。
    candidate_index: u32,
    /// Carries normalized atomic content.
    /// 携带规范化原子内容。
    normalized_content: String,
    /// Carries the VMM priority label.
    /// 携带 VMM 优先级标签。
    priority: String,
    /// Carries the VMM lifecycle label.
    /// 携带 VMM 生命周期标签。
    level: String,
    /// Carries the optional lifecycle reason.
    /// 携带可选生命周期原因。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    level_reason: String,
    /// Carries exact superseded profile-node identities.
    /// 携带精确被替代画像节点身份。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    supersede_node_ids: Vec<String>,
}

/// Carries the exact manual profile instruction Host response.
/// 携带精确的手工画像指令 Host 响应。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileInstructionResponse {
    /// Carries accepted new profile nodes.
    /// 携带已接纳新画像节点。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    accepted_nodes: Vec<ProfileInstructionAccepted>,
    /// Carries directly retired active nodes.
    /// 携带被直接退役的活跃节点。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    retired_nodes: Vec<ProfileNodeReason>,
    /// Carries the overall review reason.
    /// 携带总体审核原因。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    reason: String,
}

/// Carries one accepted node in a manual profile response.
/// 携带手工画像响应中的一条已接纳节点。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileInstructionAccepted {
    /// Carries normalized atomic content.
    /// 携带规范化原子内容。
    normalized_content: String,
    /// Carries the VMM priority label.
    /// 携带 VMM 优先级标签。
    priority: String,
    /// Carries the VMM lifecycle label.
    /// 携带 VMM 生命周期标签。
    level: String,
    /// Carries the optional lifecycle reason.
    /// 携带可选生命周期原因。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    level_reason: String,
    /// Carries nodes superseded by the new node.
    /// 携带被新节点替代的节点。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    supersede_nodes: Vec<ProfileNodeReason>,
}

/// Carries one profile-node identity with an explicit reason.
/// 携带一个具有显式原因的画像节点身份。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileNodeReason {
    /// Carries the exact node identity.
    /// 携带精确节点身份。
    node_id: String,
    /// Carries the explicit reason.
    /// 携带显式原因。
    #[serde(default, deserialize_with = "deserialize_optional_default")]
    reason: String,
}
