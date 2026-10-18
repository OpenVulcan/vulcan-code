//! Ordered closed request and response profile mapping at the provider execution boundary.
//! 供应商执行边界上的有序封闭请求与响应 Profile 映射。

use super::json_template::render_complete_json_template;
use super::{ProviderExecutionFailure, ProviderExecutionRoute, pre_dispatch_failure};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::BTreeMap;
use vulcan_ai_contract::{AiExecutionRequest, ModelParameterValue, PromptCachePolicy};
use vulcan_llm_core::EncodedWireRequest;
use vulcan_llm_core::ProtocolConversionError;

/// Applies the ordered closed request-projection profile chain to one encoded wire request.
/// 把有序封闭请求投影 Profile 链应用到一个编码 Wire 请求。
///
/// Steps run in declaration order over the encoded body; the chain carries frozen profile
/// revisions for telemetry and every step is a registered primitive (plan §6.6/§R8.2).
/// 步骤按声明顺序作用于编码请求体；链携带冻结 Profile 修订用于遥测，每个步骤都是
/// 已注册原语（方案 §6.6/§R8.2）。
pub(super) fn apply_request_projection_profiles(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    for profile in &route.request_projection_profiles {
        for step in &profile.steps {
            apply_mapping_step(request, encoded, step)?;
        }
    }
    Ok(())
}

/// Executes one compiled request profile over a JSON body for the executable fixture harness.
/// 为可执行 fixture 验证器在 JSON 正文上执行一份已编译请求 Profile。
///
/// # Parameters
/// 参数
/// `profile` is the exact compiled payload; `model_id` supplies the canonical request identity;
/// `body` is the already protocol-encoded JSON body consumed by production mapping steps.
/// `profile` 是精确已编译载荷；`model_id` 提供规范请求身份；`body` 是生产映射步骤消费的
/// 已完成协议编码 JSON 正文。
///
/// # Returns
/// 返回值
/// The projected body, or the same controlled pre-dispatch failure used in production.
/// 返回投影后的正文，或与生产环境相同的受控分发前错误。
pub(crate) fn execute_request_profile_fixture(
    profile: &crate::provider_manifest::RequestProjectionProfile,
    model_id: &str,
    body: Value,
) -> Result<Value, String> {
    project_json_request_profiles(std::slice::from_ref(profile), model_id, body)
}

/// Applies an ordered request-profile chain to a JSON provider-operation body.
/// 将有序请求 Profile 链应用到 JSON 供应商操作正文。
///
/// # Parameters
/// 参数
/// `profiles` are frozen payloads from one manifest generation, `model_id` is the exact
/// provider-tool target identity used by bounded templates, and `body` is the canonical JSON
/// operation body after upload materialization.
/// `profiles` 是同一清单代际冻结的载荷，`model_id` 是受限模板使用的精确供应商工具目标身份，
/// `body` 是上传物化后的规范 JSON 操作正文。
///
/// # Returns
/// 返回值
/// The projected wire body, or the production-equivalent controlled pre-dispatch diagnostic.
/// 返回投影后的 Wire 正文，或与生产一致的受控分派前诊断。
pub(crate) fn project_json_request_profiles(
    profiles: &[crate::provider_manifest::RequestProjectionProfile],
    model_id: &str,
    body: Value,
) -> Result<Value, String> {
    let request = AiExecutionRequest {
        operation: vulcan_ai_contract::AiOperation::ConversationRespond,
        model_id: model_id.to_string(),
        cache_scope_id: None,
        continuation: None,
        messages: Vec::new(),
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: Default::default(),
        parallel_tool_calls: Default::default(),
        structured_output: Default::default(),
        stop_sequences: Vec::new(),
        prompt_cache: PromptCachePolicy::ProviderDefault,
        reasoning_effort: None,
        reasoning_output: Default::default(),
        model_parameters: BTreeMap::new(),
        resources: Vec::new(),
        output_token_limit: None,
    };
    let mut encoded = EncodedWireRequest {
        path: String::from("/fixture"),
        headers: BTreeMap::new(),
        body,
    };
    for profile in profiles {
        for step in &profile.steps {
            apply_mapping_step(&request, &mut encoded, step)
                .map_err(|failure| failure.error.message)?;
        }
    }
    Ok(encoded.body)
}

/// Applies an ordered response-profile chain to one JSON provider-operation response.
/// 将有序响应 Profile 链应用到一个 JSON 供应商操作响应。
///
/// # Parameters
/// 参数
/// `profiles` are frozen payloads from one manifest generation and `body` is the decoded raw
/// provider response before output-contract normalization.
/// `profiles` 是同一清单代际冻结的载荷，`body` 是输出契约归一化前已解码的原始供应商响应。
///
/// # Returns
/// 返回值
/// The projected response JSON, or a deterministic profile conversion diagnostic.
/// 返回投影后的响应 JSON，或确定性的 Profile 转换诊断。
pub(crate) fn project_json_response_profiles(
    profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    mut body: Value,
) -> Result<Value, String> {
    apply_response_projection_profiles(profiles, &mut body).map_err(|error| error.to_string())?;
    Ok(body)
}

/// Applies every frozen response/error profile to one decoded provider JSON value.
/// 把每一份冻结响应/错误 Profile 应用于一个已解码供应商 JSON 值。
pub(super) fn apply_response_projection_profiles(
    profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    value: &mut Value,
) -> Result<(), ProtocolConversionError> {
    if profiles.is_empty() {
        return Ok(());
    }
    if !value.is_object() {
        return Err(ProtocolConversionError::new(
            "Manifest response projection requires a JSON object value.",
        ));
    }
    for profile in profiles {
        for step in &profile.steps {
            apply_response_mapping_step(value, step)?;
        }
    }
    Ok(())
}

/// One stream or HTTP-error record after manifest response projection.
/// 经过清单响应投影后的一条流式或 HTTP 错误记录。
pub(crate) struct ProjectedResponseRecord<'a> {
    /// Record text handed to the protocol decoder; borrowed unchanged when no profile applies.
    /// 交给协议解码器的记录文本；无投影时原样借用。
    pub(crate) text: Cow<'a, str>,
    /// Projected JSON value, present only when projection already decoded the record.
    /// 投影后的 JSON 值；仅当投影已解码该记录时存在，供后续用量提取复用。
    pub(crate) value: Option<Value>,
}

/// Parses and projects one stream or HTTP-error JSON record, decoding it at most once.
/// 解析并投影一条流式或 HTTP 错误 JSON 记录，最多解码一次。
pub(crate) fn project_response_record<'a>(
    profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    text: &'a str,
) -> Result<ProjectedResponseRecord<'a>, ProtocolConversionError> {
    if profiles.is_empty() || text.trim() == "[DONE]" {
        return Ok(ProjectedResponseRecord {
            text: Cow::Borrowed(text),
            value: None,
        });
    }
    let mut value: Value = serde_json::from_str(text).map_err(|error| {
        ProtocolConversionError::new(format!(
            "Manifest response projection expected JSON but decoding failed: {error}"
        ))
    })?;
    apply_response_projection_profiles(profiles, &mut value)?;
    let projected = serde_json::to_string(&value).map_err(|error| {
        ProtocolConversionError::new(format!(
            "Manifest response projection failed to serialize JSON: {error}"
        ))
    })?;
    Ok(ProjectedResponseRecord {
        text: Cow::Owned(projected),
        value: Some(value),
    })
}

/// Parses, projects, and re-serializes one stream or HTTP-error JSON record.
/// 解析、投影并重新序列化一条流式或 HTTP 错误 JSON 记录。
pub(crate) fn project_response_record_text(
    profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    text: &str,
) -> Result<String, ProtocolConversionError> {
    project_response_record(profiles, text).map(|record| record.text.into_owned())
}

/// Applies one request-independent closed mapping step to a response JSON object.
/// 把一个不依赖请求上下文的封闭映射步骤应用于响应 JSON 对象。
fn apply_response_mapping_step(
    body: &mut Value,
    step: &crate::provider_manifest::MappingStep,
) -> Result<(), ProtocolConversionError> {
    match step {
        crate::provider_manifest::MappingStep::SetLiteral { path, value } => {
            write_response_json_pointer(body, path, value.clone())?;
        }
        crate::provider_manifest::MappingStep::SetDefaultIfMissing { path, value } => {
            if pointer_value(body, path).is_none() {
                write_response_json_pointer(body, path, value.clone())?;
            }
        }
        crate::provider_manifest::MappingStep::CopyJsonPointer { from, to } => {
            if let Some(value) = pointer_value(body, from).cloned() {
                write_response_json_pointer(body, to, value)?;
            }
        }
        crate::provider_manifest::MappingStep::RenameJsonPointer { from, to } => {
            if let Some(value) = pointer_value(body, from).cloned() {
                delete_json_pointer(body, from);
                write_response_json_pointer(body, to, value)?;
            }
        }
        crate::provider_manifest::MappingStep::DeleteJsonPointer { path } => {
            delete_json_pointer(body, path);
        }
        crate::provider_manifest::MappingStep::EnumMap {
            path,
            source,
            source_path,
            mapping,
            unmatched,
            default,
        } => {
            ensure_response_body_source(*source)?;
            let source_value = pointer_value(body, source_path).cloned();
            let mapped = source_value.as_ref().and_then(|value| {
                projection_scalar_key(value)
                    .as_deref()
                    .and_then(|key| mapping.get(key).cloned())
            });
            match (source_value, mapped, unmatched, default) {
                (None, _, _, Some(fallback)) => {
                    write_response_json_pointer(body, path, fallback.clone())?;
                }
                (None, _, _, None) => {}
                (_, Some(mapped), _, _) => write_response_json_pointer(body, path, mapped)?,
                (Some(_), None, crate::provider_manifest::EnumMapUnmatched::Remove, _) => {
                    delete_json_pointer(body, path);
                }
                (Some(_), None, crate::provider_manifest::EnumMapUnmatched::Error, _) => {
                    return Err(ProtocolConversionError::new(format!(
                        "Response enum map source `{source_path}` is not in the declared mapping table."
                    )));
                }
                (Some(_), None, crate::provider_manifest::EnumMapUnmatched::KeepLiteral, _) => {}
            }
        }
        crate::provider_manifest::MappingStep::WrapSingletonArray { path } => {
            if let Some(value) = pointer_value(body, path).cloned() {
                write_response_json_pointer(body, path, Value::Array(vec![value]))?;
            }
        }
        crate::provider_manifest::MappingStep::UnwrapSingletonArray { path } => {
            if let Some(Value::Array(items)) = pointer_value(body, path).cloned()
                && items.len() == 1
            {
                write_response_json_pointer(
                    body,
                    path,
                    items.into_iter().next().unwrap_or(Value::Null),
                )?;
            }
        }
        crate::provider_manifest::MappingStep::ConditionalSet {
            path,
            source,
            source_path,
            equals,
            then,
            otherwise,
        } => {
            ensure_response_body_source(*source)?;
            if let Some(value) = pointer_value(body, source_path) {
                let selected = if value.as_str().is_some_and(|text| text == equals) {
                    then
                } else {
                    otherwise
                };
                write_or_delete_response(body, path, selected)?;
            }
        }
        crate::provider_manifest::MappingStep::DeleteIfEquals {
            path,
            source,
            source_path,
            equals,
        } => {
            ensure_response_body_source(*source)?;
            if pointer_value(body, source_path)
                .and_then(Value::as_str)
                .is_some_and(|text| text == equals)
            {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::DeleteUnlessEquals {
            path,
            source,
            source_path,
            equals,
        } => {
            ensure_response_body_source(*source)?;
            let retains_target = pointer_value(body, source_path)
                .and_then(Value::as_str)
                .is_some_and(|text| text == equals);
            if !retains_target {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::RemoveIfEmpty { path } => {
            let empty = matches!(pointer_value(body, path), Some(Value::Array(items)) if items.is_empty())
                || matches!(pointer_value(body, path), Some(Value::Object(items)) if items.is_empty());
            if empty {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::RejectIfPresent {
            source,
            source_path,
            message,
        } => {
            ensure_response_body_source(*source)?;
            if pointer_value(body, source_path).is_some() {
                return Err(ProtocolConversionError::new(message.clone()));
            }
        }
        crate::provider_manifest::MappingStep::RejectIfAllPresent {
            conditions,
            message,
        } => {
            for condition in conditions {
                ensure_response_body_source(condition.source)?;
            }
            if conditions
                .iter()
                .all(|condition| pointer_value(body, &condition.source_path).is_some())
            {
                return Err(ProtocolConversionError::new(message.clone()));
            }
        }
        crate::provider_manifest::MappingStep::StringTemplate { .. }
        | crate::provider_manifest::MappingStep::FitThinkingBudget { .. }
        | crate::provider_manifest::MappingStep::CopyRequestField { .. } => {
            return Err(ProtocolConversionError::new(
                "Response projection contains a request-only mapping step.",
            ));
        }
        crate::provider_manifest::MappingStep::RebuildJsonTemplate { template } => {
            let source = body.clone();
            let rendered = render_complete_json_template(template, &source, "")
                .map_err(ProtocolConversionError::new)?;
            if !rendered.is_object() {
                return Err(ProtocolConversionError::new(
                    "Manifest response JSON template must render an object body.",
                ));
            }
            *body = rendered;
        }
    }
    Ok(())
}

/// Enforces that response mapping sources come only from the decoded response body.
/// 强制响应映射源只能来自已解码响应正文。
fn ensure_response_body_source(
    source: crate::provider_manifest::MappingStepSource,
) -> Result<(), ProtocolConversionError> {
    if source == crate::provider_manifest::MappingStepSource::BodyJsonPointer {
        Ok(())
    } else {
        Err(ProtocolConversionError::new(
            "Response projection may not read host request fields.",
        ))
    }
}

/// Safely writes one response JSON pointer without panicking on remote scalar ancestors.
/// 安全写入一个响应 JSON Pointer，远端标量祖先存在时不会触发 panic。
fn write_response_json_pointer(
    body: &mut Value,
    path: &str,
    value: Value,
) -> Result<(), ProtocolConversionError> {
    let segments = pointer_segments(path);
    if segments.is_empty() {
        return Err(ProtocolConversionError::new(
            "Response projection target JSON pointer is empty.",
        ));
    }
    let mut current = body;
    for segment in &segments[..segments.len() - 1] {
        let object = current.as_object_mut().ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "Response projection target `{path}` crosses a non-object ancestor."
            ))
        })?;
        current = object
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    let object = current.as_object_mut().ok_or_else(|| {
        ProtocolConversionError::new(format!(
            "Response projection target `{path}` has a non-object parent."
        ))
    })?;
    let last = segments.last().expect("validated non-empty response path");
    object.insert(last.to_string(), value);
    Ok(())
}

/// Writes a response value or deletes the target when the selected value is null.
/// 写入响应值；所选值为 null 时删除目标。
fn write_or_delete_response(
    body: &mut Value,
    path: &str,
    value: &Value,
) -> Result<(), ProtocolConversionError> {
    if value.is_null() {
        delete_json_pointer(body, path);
        Ok(())
    } else {
        write_response_json_pointer(body, path, value.clone())
    }
}

/// Applies one closed mapping step to the encoded wire request.
/// 把一个封闭映射步骤应用到编码 Wire 请求。
pub(super) fn apply_mapping_step(
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
    step: &crate::provider_manifest::MappingStep,
) -> Result<(), ProviderExecutionFailure> {
    if !encoded.body.is_object() {
        return Err(pre_dispatch_failure(
            "Manifest request projection requires a JSON object body.",
        ));
    }
    let body = &mut encoded.body;
    match step {
        crate::provider_manifest::MappingStep::SetLiteral { path, value } => {
            write_json_pointer(body, path, value.clone())?;
        }
        crate::provider_manifest::MappingStep::SetDefaultIfMissing { path, value } => {
            if pointer_value(body, path).is_none() {
                write_json_pointer(body, path, value.clone())?;
            }
        }
        crate::provider_manifest::MappingStep::CopyJsonPointer { from, to } => {
            if let Some(value) = pointer_value(body, from).cloned() {
                write_json_pointer(body, to, value)?;
            }
        }
        crate::provider_manifest::MappingStep::RenameJsonPointer { from, to } => {
            if let Some(value) = pointer_value(body, from).cloned() {
                delete_json_pointer(body, from);
                write_json_pointer(body, to, value)?;
            }
        }
        crate::provider_manifest::MappingStep::DeleteJsonPointer { path } => {
            delete_json_pointer(body, path);
        }
        crate::provider_manifest::MappingStep::FitThinkingBudget {
            path,
            limit_path,
            minimum,
        } => {
            fit_thinking_budget(body, path, limit_path, *minimum)?;
        }
        crate::provider_manifest::MappingStep::EnumMap {
            path,
            source,
            source_path,
            mapping,
            unmatched,
            default,
        } => {
            let value = projection_source_value(*source, source_path, request, body);
            let mapped = value.as_ref().and_then(|value| {
                projection_scalar_key(value)
                    .as_deref()
                    .and_then(|key| mapping.get(key).cloned())
            });
            match (value, mapped, unmatched, default) {
                (None, _, _, Some(fallback)) => {
                    write_json_pointer(body, path, fallback.clone())?;
                }
                (None, _, _, None) => {}
                (_, Some(mapped), _, _) => {
                    write_json_pointer(body, path, mapped)?;
                }
                (Some(_), None, _, _) => match unmatched {
                    crate::provider_manifest::EnumMapUnmatched::Remove => {
                        delete_json_pointer(body, path);
                    }
                    crate::provider_manifest::EnumMapUnmatched::Error => {
                        return Err(pre_dispatch_failure(format!(
                            "Enum map source `{source_path}` value is not in the declared mapping table."
                        )));
                    }
                    crate::provider_manifest::EnumMapUnmatched::KeepLiteral => {}
                },
            }
        }
        crate::provider_manifest::MappingStep::WrapSingletonArray { path } => {
            if let Some(value) = pointer_value(body, path).cloned() {
                write_json_pointer(body, path, Value::Array(vec![value]))?;
            }
        }
        crate::provider_manifest::MappingStep::UnwrapSingletonArray { path } => {
            if let Some(Value::Array(items)) = pointer_value(body, path).cloned()
                && items.len() == 1
            {
                write_json_pointer(body, path, items.into_iter().next().unwrap_or(Value::Null))?;
            }
        }
        crate::provider_manifest::MappingStep::StringTemplate { path, template } => {
            let rendered = template.replace("{model_id}", &request.model_id);
            write_json_pointer(body, path, Value::String(rendered))?;
        }
        crate::provider_manifest::MappingStep::CopyRequestField { path, field } => {
            if let Some(value) = request_field_value(field, request) {
                write_json_pointer(body, path, value)?;
            }
        }
        crate::provider_manifest::MappingStep::ConditionalSet {
            path,
            source,
            source_path,
            equals,
            then,
            otherwise,
        } => {
            let value = projection_source_value(*source, source_path, request, body);
            let matched = value
                .as_ref()
                .and_then(|value| value.as_str())
                .is_some_and(|text| text == equals);
            match (value, matched) {
                (None, _) => {}
                (_, true) => write_or_delete(body, path, then)?,
                (_, false) => write_or_delete(body, path, otherwise)?,
            }
        }
        crate::provider_manifest::MappingStep::DeleteIfEquals {
            path,
            source,
            source_path,
            equals,
        } => {
            let value = projection_source_value(*source, source_path, request, body);
            if value
                .as_ref()
                .and_then(|value| value.as_str())
                .is_some_and(|text| text == equals)
            {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::DeleteUnlessEquals {
            path,
            source,
            source_path,
            equals,
        } => {
            let retains_target = projection_source_value(*source, source_path, request, body)
                .as_ref()
                .and_then(Value::as_str)
                .is_some_and(|text| text == equals);
            if !retains_target {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::RemoveIfEmpty { path } => {
            let empty = matches!(
                pointer_value(body, path),
                Some(Value::Array(items)) if items.is_empty()
            ) || matches!(
                pointer_value(body, path),
                Some(Value::Object(items)) if items.is_empty()
            );
            if empty {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::RejectIfPresent {
            source,
            source_path,
            message,
        } => {
            if projection_source_value(*source, source_path, request, body).is_some() {
                return Err(pre_dispatch_failure(message.clone()));
            }
        }
        crate::provider_manifest::MappingStep::RejectIfAllPresent {
            conditions,
            message,
        } => {
            if conditions.iter().all(|condition| {
                projection_source_value(condition.source, &condition.source_path, request, body)
                    .is_some()
            }) {
                return Err(pre_dispatch_failure(message.clone()));
            }
        }
        crate::provider_manifest::MappingStep::RebuildJsonTemplate { template } => {
            let source = body.clone();
            let rendered = render_complete_json_template(template, &source, &request.model_id)
                .map_err(pre_dispatch_failure)?;
            if !rendered.is_object() {
                return Err(pre_dispatch_failure(
                    "Manifest request JSON template must render an object body.",
                ));
            }
            *body = rendered;
        }
    }
    Ok(())
}

/// Fits an optional thinking budget using bounded integer arithmetic before dispatch.
/// 在分派前使用有界整数运算调整可选思考预算。
///
/// `body` is the encoded request, `path` locates its budget, `limit_path` locates its optional
/// output limit, and `minimum` is the manifest-declared positive budget floor.
/// `body` 是已编码请求，`path` 定位预算，`limit_path` 定位可选输出上限，
/// `minimum` 是清单声明的正数预算下限。
/// Returns success for an absent budget or a valid fitted value; malformed or impossible limits fail.
/// 预算缺失或调整后的值有效时成功；格式错误或无法满足的上限显式失败。
fn fit_thinking_budget(
    body: &mut Value,
    path: &str,
    limit_path: &str,
    minimum: u64,
) -> Result<(), ProviderExecutionFailure> {
    // requested is optional only because disabled and adaptive thinking have no explicit token budget.
    // requested 仅因关闭与自适应思考没有显式令牌预算而允许缺失。
    let Some(requested) = pointer_value(body, path) else {
        return Ok(());
    };
    // budget is exact unsigned wire data, never a floating-point or string conversion.
    // budget 是精确无符号线路数据，不经过浮点数或字符串转换。
    let budget = requested
        .as_u64()
        .ok_or_else(|| pre_dispatch_failure("Thinking budget must be an unsigned integer."))?;
    if minimum == 0 || budget < minimum {
        return Err(pre_dispatch_failure(
            "Thinking budget is below its declared positive minimum.",
        ));
    }
    // An absent output limit preserves the explicit upstream budget, as in OpenCode v2.
    // 输出上限缺失时与 OpenCode v2 一样保留显式上游预算。
    let Some(limit) = pointer_value(body, limit_path) else {
        return Ok(());
    };
    // output_limit must leave room for both the minimum thinking budget and a final answer.
    // output_limit 必须为最低思考预算及最终答案同时留出空间。
    let output_limit = limit.as_u64().ok_or_else(|| {
        pre_dispatch_failure("Thinking output limit must be an unsigned integer.")
    })?;
    if output_limit <= minimum {
        return Err(pre_dispatch_failure(
            "Output limit must exceed the minimum thinking budget.",
        ));
    }
    // fitted mirrors the upstream half-output cap without multiplication or overflow.
    // fitted 对齐上游输出一半的限制，避免乘法与溢出。
    let fitted = budget.min((output_limit / 2).max(minimum));
    write_json_pointer(body, path, Value::from(fitted))
}

/// Safely writes one request JSON pointer while creating missing object ancestors.
/// 安全写入一个请求 JSON Pointer，并创建缺失的对象祖先。
///
/// # Parameters
/// 参数
/// `body` is the protocol-encoded request object, `path` is the validated manifest pointer, and
/// `value` is the exact projected value.
/// `body` 是协议编码后的请求对象，`path` 是已校验的 Manifest 指针，`value` 是精确投影值。
///
/// # Returns
/// 返回值
/// Success after insertion, or a controlled pre-dispatch failure when runtime data places a
/// scalar at an ancestor that the manifest expects to be an object.
/// 插入成功时返回成功；若运行时数据在 Manifest 期望为对象的祖先位置放置标量，则返回受控的
/// 分发前失败。
fn write_json_pointer(
    body: &mut Value,
    path: &str,
    value: Value,
) -> Result<(), ProviderExecutionFailure> {
    let segments = pointer_segments(path);
    if segments.is_empty() {
        return Err(pre_dispatch_failure(
            "Request projection target JSON pointer is empty.",
        ));
    }
    let mut current: &mut Value = body;
    for segment in &segments[..segments.len() - 1] {
        let object = current.as_object_mut().ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Request projection target `{path}` crosses a non-object ancestor."
            ))
        })?;
        let next = object
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        current = next;
    }
    let object = current.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure(format!(
            "Request projection target `{path}` has a non-object parent."
        ))
    })?;
    let Some(last) = segments.last() else {
        return Err(pre_dispatch_failure(
            "Request projection target JSON pointer is empty.",
        ));
    };
    object.insert(last.to_string(), value);
    Ok(())
}

/// Deletes one JSON pointer path, pruning now-empty ancestors.
/// 删除一个 JSON Pointer 路径并修剪变空的祖先。
fn delete_json_pointer(body: &mut Value, path: &str) {
    let segments = pointer_segments(path);
    if segments.is_empty() {
        return;
    }
    delete_pointer_recursive(body, &segments);
}

fn delete_pointer_recursive(value: &mut Value, segments: &[String]) {
    let Some((first, rest)) = segments.split_first() else {
        return;
    };
    if rest.is_empty() {
        match value {
            Value::Object(object) => {
                object.remove(first);
            }
            Value::Array(items) => {
                if let Some(index) = first.parse::<usize>().ok()
                    && index < items.len()
                {
                    items.remove(index);
                }
            }
            _ => {}
        }
        return;
    }
    match value {
        Value::Object(object) => {
            let Some(child) = object.get_mut(first) else {
                return;
            };
            delete_pointer_recursive(child, rest);
            if matches!(child, Value::Object(nested) if nested.is_empty()) {
                object.remove(first);
            }
        }
        Value::Array(items) => {
            let Some(index) = first.parse::<usize>().ok() else {
                return;
            };
            if index < items.len() {
                let child = &mut items[index];
                delete_pointer_recursive(child, rest);
                if matches!(child, Value::Object(nested) if nested.is_empty()) {
                    items.remove(index);
                }
            }
        }
        _ => {}
    }
}

/// Reads one JSON pointer value from a request body map.
/// 从请求体映射读取一个 JSON Pointer 值。
fn pointer_value<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    value.pointer(path)
}

/// Splits one JSON pointer into unescaped segments.
/// 把一个 JSON Pointer 拆分为未转义分段。
fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// Writes one request value or deletes the target when the value is null.
/// 写入一个请求值；值为 null 时删除目标。
///
/// # Parameters
/// 参数
/// `body`, `path`, and `value` carry the same runtime projection boundary as
/// `write_json_pointer`.
/// `body`、`path` 与 `value` 承载与 `write_json_pointer` 相同的运行时投影边界。
///
/// # Returns
/// 返回值
/// Success after deletion/insertion, or the controlled pointer-write failure.
/// 删除或插入后返回成功，或返回受控的指针写入失败。
fn write_or_delete(
    body: &mut Value,
    path: &str,
    value: &Value,
) -> Result<(), ProviderExecutionFailure> {
    if value.is_null() {
        delete_json_pointer(body, path);
        Ok(())
    } else {
        write_json_pointer(body, path, value.clone())
    }
}

/// Resolves one projection source value from a body pointer or a validated host field.
/// 从请求体指针或已校验宿主字段解析一个投影源值。
fn projection_source_value(
    source: crate::provider_manifest::MappingStepSource,
    source_path: &str,
    request: &AiExecutionRequest,
    body: &Value,
) -> Option<Value> {
    match source {
        crate::provider_manifest::MappingStepSource::BodyJsonPointer => {
            body.pointer(source_path).cloned()
        }
        crate::provider_manifest::MappingStepSource::RequestField => {
            if let Some(parameter_id) = source_path.strip_prefix("@model_parameters.") {
                request
                    .model_parameters
                    .get(parameter_id)
                    .and_then(model_parameter_to_value)
            } else {
                request_field_value(source_path, request)
            }
        }
    }
}

/// Converts one bounded JSON scalar into the canonical enum-map lookup key.
/// 把一个有界 JSON 标量转换为枚举映射使用的规范查找键。
///
/// # Parameters
/// 参数
/// `value` is the exact request-field or body value selected by the manifest source.
/// `value` 是 Manifest 来源选中的精确请求字段值或请求体值。
///
/// # Returns
/// 返回值
/// A stable textual key for strings, numbers, and booleans; arrays, objects, and null are rejected.
/// 为字符串、数字与布尔值返回稳定文本键；数组、对象与空值不参与映射。
fn projection_scalar_key(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// Resolves one bounded host request field value.
/// 解析一个有界宿主请求字段值。
fn request_field_value(field: &str, request: &AiExecutionRequest) -> Option<Value> {
    // The optional `@` marker distinguishes request fields from body pointers; both call
    // sites (CopyRequestField and projection_source_value) pass the raw path, so the marker
    // is stripped here once.
    // 可选 `@` 标记用于区分请求字段与请求体指针；两个调用点（CopyRequestField 与
    // projection_source_value）都传原始路径，因此在此统一剥离标记。
    let field = field.strip_prefix('@').unwrap_or(field);
    match field {
        "reasoning_effort" => request
            .reasoning_effort
            .map(|effort| Value::String(effort.as_str().to_string())),
        "model_id" => Some(Value::String(request.model_id.clone())),
        "output_token_limit" => request
            .output_token_limit
            .map(|limit| Value::Number(limit.into())),
        "cache_scope_id" => request
            .cache_scope_id
            .as_ref()
            .map(|scope| Value::String(scope.clone())),
        "prompt_cache" => (!matches!(request.prompt_cache, PromptCachePolicy::ProviderDefault))
            .then_some(Value::Bool(true)),
        _ => field
            .strip_prefix("model_parameters.")
            .and_then(|parameter_id| {
                request
                    .model_parameters
                    .get(parameter_id)
                    .and_then(model_parameter_to_value)
            }),
    }
}

/// Converts one strongly typed model parameter value into a wire JSON value.
/// 把一个强类型模型参数值转换为 Wire JSON 值。
fn model_parameter_to_value(value: &ModelParameterValue) -> Option<Value> {
    match value {
        ModelParameterValue::Boolean(value) => Some(Value::Bool(*value)),
        ModelParameterValue::Integer(value) => Some(Value::Number((*value).into())),
        ModelParameterValue::Decimal(value) => Some(Value::String(value.clone())),
        ModelParameterValue::String(value) => Some(Value::String(value.clone())),
        ModelParameterValue::Array(values) => Some(Value::Array(
            values.iter().filter_map(model_parameter_to_value).collect(),
        )),
        ModelParameterValue::Object(values) => Some(Value::Object(
            values
                .iter()
                .filter_map(|(key, value)| {
                    model_parameter_to_value(value).map(|value| (key.clone(), value))
                })
                .collect(),
        )),
    }
}

#[cfg(test)]
mod tests;
