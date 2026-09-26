//! Provider-native tool request projection and unified output decoding.
//! 供应商原生工具请求投影与统一输出解码。

use crate::provider_task::{
    ProviderAsyncTaskPoll, ProviderAsyncTaskRecoveryContext, ProviderAsyncTaskStart,
};
use crate::{
    AuthenticationProjection, CredentialExecutionSecret, ProviderToolAdapterKind,
    ProviderToolExecutionKind, ProviderToolExecutionResult, ProviderToolExtractFailure,
    ProviderToolExtractOutput, ProviderToolExtractResult, ProviderToolGeneratedResource,
    ProviderToolNormalizedOutput, ProviderToolOutputKind, ProviderToolResourceDownloadPolicy,
    ProviderToolSearchOutput, ProviderToolSearchResult, ProviderToolTarget, ServiceEntry,
};
use base64::Engine;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

mod chat_sse;

/// Maximum provider tool metadata response accepted before typed decoding.
/// 强类型解码前允许接收的供应商工具元数据响应上限。
///
/// Applies to JSON bodies, SSE streams, and their accumulated text. Resources embedded inline in
/// such a response are therefore bounded by this limit before [`GENERIC_RESOURCE_BYTE_LIMIT`] applies.
/// 适用于 JSON 响应体、SSE 流及其累积文本。内嵌于此类响应中的资源因此在
/// [`GENERIC_RESOURCE_BYTE_LIMIT`] 生效之前已受本上限约束。
const PROVIDER_TOOL_RESPONSE_BYTE_LIMIT: usize = 8 * 1024 * 1024;

/// Maximum Unicode characters retained from one provider diagnostic field.
/// 单个供应商诊断字段允许保留的最大 Unicode 字符数。
const PROVIDER_TOOL_DIAGNOSTIC_FIELD_MAX_CHARACTERS: usize = 2_048;

/// Maximum Unicode characters retained in one complete provider diagnostic.
/// 一条完整供应商诊断允许保留的最大 Unicode 字符数。
const PROVIDER_TOOL_DIAGNOSTIC_MAX_CHARACTERS: usize = 8_192;

/// Maximum ordered resources one declarative output contract may materialize.
/// 单份声明式输出契约允许物化的最大有序资源数量。
const GENERIC_RESOURCE_COUNT_LIMIT: usize = 256;

/// Absolute item ceiling for normalized search, extract, and nested URL collections.
/// 归一化搜索、提取及嵌套 URL 集合的绝对条目上限。
const GENERIC_STRUCTURED_RESULT_COUNT_LIMIT: usize = 256;

/// Absolute decoded-byte ceiling accepted from one declarative resource extractor.
/// 单个声明式资源提取器允许的绝对解码字节上限。
///
/// This is the ceiling for manifest-declared `max_bytes`, binary follow-up downloads streamed outside
/// the metadata response, and request-side multipart files. It is intentionally larger than
/// [`PROVIDER_TOOL_RESPONSE_BYTE_LIMIT`]: only binary carriers can reach it, while inline resources
/// stay bounded by the smaller response limit.
/// 这是 Manifest 声明的 `max_bytes`、在元数据响应之外流式下载的二进制后续资源，以及请求侧
/// multipart 文件的上限。它刻意大于 [`PROVIDER_TOOL_RESPONSE_BYTE_LIMIT`]：只有二进制载体能触及它，
/// 内联资源仍受较小的响应上限约束。
const GENERIC_RESOURCE_BYTE_LIMIT: u64 = 100 * 1024 * 1024;

/// Formats one provider-tool transport failure without retaining request URL material.
/// 格式化一次供应商工具传输失败，且不保留请求 URL 材料。
///
/// # Parameters
/// 参数
/// `operation` is a controlled host label and `error` is the reqwest transport failure.
/// `operation` 是受控宿主标签，`error` 是 reqwest 传输失败。
///
/// # Returns
/// 返回值
/// A diagnostic containing transport details but no task identifier or signed query URL.
/// 返回包含传输详情但不含任务标识或签名查询 URL 的诊断。
fn format_provider_tool_transport_error(operation: &str, error: reqwest::Error) -> String {
    format!("{operation} request failed: {}", error.without_url())
}

/// Resolves the exact native endpoint for one frozen provider tool target.
/// 为一个冻结供应商工具目标解析精确原生端点。
///
/// # Parameters
/// 参数
/// `entry` owns the selected account while `target` may select a native origin distinct from
/// the conversation protocol root.
/// `entry` 持有所选账号，`target` 可以选择与会话协议根地址不同的原生源地址。
///
/// # Returns
/// 返回值
/// The absolute provider tool URL.
/// 返回绝对供应商工具 URL。
pub(crate) fn provider_tool_url(entry: &ServiceEntry, target: &ProviderToolTarget) -> String {
    let base_url = target
        .execution
        .endpoint_base_url
        .as_deref()
        .unwrap_or(entry.base_url.as_str());
    format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        target.execution.endpoint_path
    )
}

/// Reads one provider tool response through a shared bounded stream.
/// 通过共享有界流读取一次供应商工具响应。
///
/// # Parameters
/// 参数
/// `response` owns the unread response body and `operation` scopes diagnostics.
/// `response` 持有尚未读取的响应正文，`operation` 限定诊断信息。
///
/// # Returns
/// 返回值
/// UTF-8 response text no larger than the provider-tool metadata limit.
/// 返回不超过供应商工具元数据上限的 UTF-8 响应文本。
pub(crate) async fn read_bounded_provider_response(
    response: reqwest::Response,
    operation: &str,
) -> Result<String, String> {
    if response
        .content_length()
        .is_some_and(|length| length > PROVIDER_TOOL_RESPONSE_BYTE_LIMIT as u64)
    {
        return Err(format!(
            "{operation} response exceeds the 8 MiB metadata limit."
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            format!(
                "{operation} response could not be read: {}",
                error.without_url()
            )
        })?;
        if bytes.len().saturating_add(chunk.len()) > PROVIDER_TOOL_RESPONSE_BYTE_LIMIT {
            return Err(format!(
                "{operation} response exceeds the 8 MiB metadata limit."
            ));
        }
        bytes.extend_from_slice(chunk.as_ref());
    }
    String::from_utf8(bytes).map_err(|_| format!("{operation} response is not valid UTF-8 text."))
}

/// Parses one provider JSON document through the shared byte, depth, and node-count bounds.
/// 通过共享字节、深度与节点数量上限解析一份供应商 JSON 文档。
///
/// # Parameters
/// 参数
/// `text` is the provider-controlled UTF-8 document and `operation` scopes diagnostics.
/// `text` 是供应商控制的 UTF-8 文档，`operation` 限定诊断范围。
///
/// # Returns
/// 返回值
/// The bounded JSON tree or a controlled size, syntax, depth, or node-count failure.
/// 返回有界 JSON 树，或受控的大小、语法、深度或节点数量失败。
pub(crate) fn parse_bounded_provider_json(text: &str, operation: &str) -> Result<Value, String> {
    if text.len() > PROVIDER_TOOL_RESPONSE_BYTE_LIMIT {
        return Err(format!(
            "{operation} response exceeds the {PROVIDER_TOOL_RESPONSE_BYTE_LIMIT} byte limit."
        ));
    }
    let value: Value = serde_json::from_str(text)
        .map_err(|error| format!("{operation} JSON is invalid: {error}"))?;
    crate::provider_manifest::validate_runtime_json_bounds(&value, operation)?;
    Ok(value)
}

/// Validates and materializes defaults for one AI-visible provider-tool argument object.
/// 校验并物化一份 AI 可见供应商工具参数对象的默认值。
///
/// # Parameters
/// 参数
/// `target` owns the exact frozen input schema and `arguments` are the pre-upload AI values.
/// `target` 持有精确冻结的输入 Schema，`arguments` 是上传前的 AI 参数值。
///
/// # Returns
/// 返回值
/// Canonical validated arguments ready for declarative upload materialization.
/// 返回可进入声明式上传物化的规范已校验参数。
pub(crate) fn canonicalize_provider_tool_arguments(
    target: &ProviderToolTarget,
    mut arguments: Value,
) -> Result<Value, String> {
    // One frozen schema owns both validations; default materialization must not recompile it.
    // 同一冻结 Schema 持有两次校验；默认值物化不得重新编译 Schema。
    let validator =
        jsonschema::draft202012::new(&target.contract.input_schema).map_err(|error| {
            format!(
                "Provider tool `{}` input schema is invalid: {error}",
                target.contract.tool_id
            )
        })?;
    validate_provider_tool_arguments(&validator, &arguments, target.contract.tool_id.as_str())?;
    apply_json_schema_defaults(&target.contract.input_schema, &mut arguments, 0)?;
    validate_provider_tool_arguments(&validator, &arguments, target.contract.tool_id.as_str())?;
    validate_provider_tool_numeric_product_ranges(
        &target.contract.input_numeric_product_ranges,
        &arguments,
        target.contract.tool_id.as_str(),
    )?;
    Ok(arguments)
}

/// Executes one exact provider tool adapter.
/// 执行一个精确的供应商工具适配器。
///
/// # Parameters
/// 参数
/// `http`, `entry`, `target`, `arguments`, and `secret` are frozen for this invocation.
/// `http`、`entry`、`target`、`arguments` 与 `secret` 在当前调用期间保持冻结。
///
/// # Returns
/// 返回值
/// A unified typed result or an explicit provider error.
/// 返回统一强类型结果或明确的供应商错误。
pub(crate) async fn execute_provider_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    request_profiles: &[crate::provider_manifest::RequestProjectionProfile],
    response_profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    ensure_operation_profiles_supported(target, request_profiles, response_profiles)?;
    if target.execution.adapter != ProviderToolAdapterKind::GenericJson {
        return Err(format!(
            "Provider tool `{}` does not use the installed generic JSON adapter.",
            target.target_id
        ));
    }
    // The manifest-selected execution kind is authoritative. Every production target now uses
    // the same bounded declarative codec and no runtime branch is selected from a provider name.
    // Manifest 选择的执行族具有权威性。所有生产目标现在使用同一套有界声明式编解码器，
    // 运行时不再根据供应商名称选择分支。
    match target.execution.execution_kind {
        ProviderToolExecutionKind::HttpJsonSync
            if matches!(
                target.contract.output.kind,
                ProviderToolOutputKind::Text
                    | ProviderToolOutputKind::Resources
                    | ProviderToolOutputKind::Search
                    | ProviderToolOutputKind::Extract
            ) =>
        {
            let canonical_arguments = arguments;
            let projected_arguments = crate::execution::project_json_request_profiles(
                request_profiles,
                target.upstream_target_id.as_str(),
                canonical_arguments.clone(),
            )?;
            execute_generic_json_sync_request(
                http,
                entry,
                target,
                secret,
                &projected_arguments,
                &target.contract.tool_id,
            )
            .await
            .and_then(|body| {
                decode_and_project_generic_json_response(
                    &target.contract.output,
                    response_profiles,
                    body.as_str(),
                    target.contract.tool_id.as_str(),
                    Some(&canonical_arguments),
                )
            })
        }
        ProviderToolExecutionKind::HttpJsonSse
            if matches!(
                target.contract.output.kind,
                ProviderToolOutputKind::Text | ProviderToolOutputKind::Resources
            ) =>
        {
            let canonical_arguments = arguments;
            let projected_arguments = crate::execution::project_json_request_profiles(
                request_profiles,
                target.upstream_target_id.as_str(),
                canonical_arguments.clone(),
            )?;
            execute_generic_json_sse_request(
                http,
                entry,
                target,
                secret,
                &projected_arguments,
            )
            .await
            .and_then(|value| {
                let body = serde_json::to_string(&value).map_err(|error| {
                    format!("Provider structured SSE terminal event could not be serialized: {error}")
                })?;
                decode_and_project_generic_json_response(
                    &target.contract.output,
                    response_profiles,
                    body.as_str(),
                    target.contract.tool_id.as_str(),
                    Some(&canonical_arguments),
                )
            })
        }
        ProviderToolExecutionKind::HttpMultipartSse
            if matches!(
                target.contract.output.kind,
                ProviderToolOutputKind::Text | ProviderToolOutputKind::Resources
            ) =>
        {
            let canonical_arguments = arguments;
            let projected_arguments = crate::execution::project_json_request_profiles(
                request_profiles,
                target.upstream_target_id.as_str(),
                canonical_arguments.clone(),
            )?;
            execute_generic_multipart_sse_request(http, entry, target, secret, &projected_arguments)
                .await
                .and_then(|value| {
                    let body = serde_json::to_string(&value).map_err(|error| {
                        format!(
                            "Provider multipart SSE terminal event could not be serialized: {error}"
                        )
                    })?;
                    decode_and_project_generic_json_response(
                        &target.contract.output,
                        response_profiles,
                        body.as_str(),
                        target.contract.tool_id.as_str(),
                        Some(&canonical_arguments),
                    )
                })
        }
        ProviderToolExecutionKind::McpHttpCall
            if matches!(
                target.contract.output.kind,
                ProviderToolOutputKind::Text
                    | ProviderToolOutputKind::Resources
                    | ProviderToolOutputKind::Search
                    | ProviderToolOutputKind::Extract
            ) =>
        {
            let canonical_arguments = arguments;
            let call_arguments = crate::execution::project_json_request_profiles(
                request_profiles,
                target.upstream_target_id.as_str(),
                canonical_arguments.clone(),
            )?;
            execute_generic_mcp_http_call(http, entry, target, secret, &call_arguments)
                .await
                .and_then(|body| {
                    decode_and_project_generic_json_response(
                        &target.contract.output,
                        response_profiles,
                        body.as_str(),
                        target.contract.tool_id.as_str(),
                        Some(&canonical_arguments),
                    )
                })
        }
        ProviderToolExecutionKind::ConversationMediaAnalyze
            if target.contract.output.kind == ProviderToolOutputKind::Text =>
        {
            let canonical_arguments = arguments;
            let projected_arguments = crate::execution::project_json_request_profiles(
                request_profiles,
                target.upstream_target_id.as_str(),
                canonical_arguments.clone(),
            )?;
            let text = execute_generic_openai_chat_sse_request(
                http,
                entry,
                target,
                secret,
                &projected_arguments,
            )
            .await?;
            let response = serde_json::json!({"text": text});
            let projected = serde_json::to_string(&response).map_err(|error| {
                format!("Provider conversation media response could not be serialized: {error}")
            })?;
            decode_generic_json_sync_output_with_request(
                &target.contract.output,
                projected.as_str(),
                Some(&canonical_arguments),
            )
        }
        ProviderToolExecutionKind::HttpJsonAsyncPoll => Err(format!(
            "Provider tool `{}` must execute through the asynchronous task lifecycle.",
            target.target_id
        )),
        execution_kind => Err(format!(
            "Provider tool `{}` has unsupported generic execution kind `{execution_kind:?}` for output `{:?}`.",
            target.target_id, target.contract.output.kind
        )),
    }
}

/// Executes one generic OpenAI-compatible Chat SSE request and returns its visible text.
/// 执行一项通用 OpenAI 兼容 Chat SSE 请求并返回其可见文本。
///
/// # Parameters
/// 参数
/// The target freezes the endpoint and authentication while `body` is fully manifest-projected.
/// 目标冻结端点与鉴权，`body` 是由 Manifest 完整投影的请求体。
///
/// # Returns
/// 返回值
/// Bounded UTF-8 text assembled only from registered Chat delta content shapes.
/// 返回仅由已注册 Chat 增量内容结构拼接的有界 UTF-8 文本。
async fn execute_generic_openai_chat_sse_request(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    body: &Value,
) -> Result<String, String> {
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("text/event-stream"),
    );
    insert_declared_tool_headers(&mut headers, target)?;
    insert_authentication_header(&mut headers, target.execution.authentication, secret)?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|error| format_provider_tool_transport_error("Provider Chat SSE", error))?;
    let status = response.status();
    if !status.is_success() {
        let text = read_bounded_provider_response(response, "Provider Chat SSE").await?;
        return Err(format_http_error(
            "Provider Chat SSE",
            status.as_u16(),
            text.as_str(),
        ));
    }
    decode_generic_openai_chat_sse(response).await
}

/// Executes one generic JSON request and decodes its declared structured SSE terminal event.
/// 执行一个通用 JSON 请求并解码其声明的结构化 SSE 终止事件。
async fn execute_generic_json_sse_request(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    body: &Value,
) -> Result<Value, String> {
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("text/event-stream"),
    );
    insert_declared_tool_headers(&mut headers, target)?;
    insert_authentication_header(&mut headers, target.execution.authentication, secret)?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|error| format_provider_tool_transport_error("Provider structured SSE", error))?;
    decode_successful_structured_sse_response(response, target).await
}

/// Executes one closed multipart request and decodes its declared structured SSE terminal event.
/// 执行一项封闭 Multipart 请求并解码其声明的结构化 SSE 终止事件。
async fn execute_generic_multipart_sse_request(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    arguments: &Value,
) -> Result<Value, String> {
    let contract = target.execution.multipart_request.as_ref().ok_or_else(|| {
        format!(
            "Provider multipart SSE target `{}` omitted multipart_request.",
            target.target_id
        )
    })?;
    let mut form = reqwest::multipart::Form::new();
    for field in &contract.fields {
        let value = arguments.pointer(field.argument_path.as_str());
        if value.is_none() || value.is_some_and(Value::is_null) {
            if field.required {
                return Err(format!(
                    "Provider multipart SSE target `{}` requires argument `{}`.",
                    target.target_id, field.argument_path
                ));
            }
            continue;
        }
        let value = value.expect("checked multipart argument presence");
        if field.file {
            let data_url = value.as_str().ok_or_else(|| {
                format!(
                    "Provider multipart file argument `{}` must be a Data URL string.",
                    field.argument_path
                )
            })?;
            let (mime_type, bytes) = decode_bounded_multipart_data_url(data_url)?;
            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(format!("{}.bin", field.field_name))
                .mime_str(mime_type.as_str())
                .map_err(|error| {
                    format!(
                        "Provider multipart field `{}` has invalid MIME type: {error}",
                        field.field_name
                    )
                })?;
            form = form.part(field.field_name.clone(), part);
        } else {
            let text = match value {
                Value::String(text) => text.clone(),
                Value::Bool(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                _ => {
                    return Err(format!(
                        "Provider multipart scalar argument `{}` has an unsupported JSON type.",
                        field.argument_path
                    ));
                }
            };
            form = form.text(field.field_name.clone(), text);
        }
    }
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("text/event-stream"),
    );
    insert_declared_tool_headers(&mut headers, target)?;
    insert_authentication_header(&mut headers, target.execution.authentication, secret)?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .multipart(form)
        .send()
        .await
        .map_err(|error| format_provider_tool_transport_error("Provider multipart SSE", error))?;
    decode_successful_structured_sse_response(response, target).await
}

/// Decodes one successful response according to a frozen structured SSE declaration.
/// 根据冻结的结构化 SSE 声明解码一份成功响应。
async fn decode_successful_structured_sse_response(
    response: reqwest::Response,
    target: &ProviderToolTarget,
) -> Result<Value, String> {
    let status = response.status();
    if !status.is_success() {
        let text = read_bounded_provider_response(response, "Provider structured SSE").await?;
        return Err(format_http_error(
            "Provider structured SSE",
            status.as_u16(),
            text.as_str(),
        ));
    }
    decode_generic_structured_sse(response, target).await
}

/// Decodes one bounded declared SSE stream and returns exactly one terminal event object.
/// 解码一条有界声明式 SSE 流并返回唯一终止事件对象。
async fn decode_generic_structured_sse(
    response: reqwest::Response,
    target: &ProviderToolTarget,
) -> Result<Value, String> {
    let contract = target.execution.sse_events.as_ref().ok_or_else(|| {
        format!(
            "Provider structured SSE target `{}` omitted sse_events.",
            target.target_id
        )
    })?;
    let mut stream = response.bytes_stream();
    let mut pending = Vec::new();
    // scan_from skips the already scanned prefix of pending so each byte is searched for a boundary once.
    // scan_from 跳过 pending 中已扫描过的前缀，使每个字节只被边界搜索一次。
    let mut scan_from = 0usize;
    let mut terminal = None;
    let mut received_bytes = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            format!(
                "Provider structured SSE could not be read: {}",
                error.without_url()
            )
        })?;
        received_bytes = received_bytes.saturating_add(chunk.len());
        if received_bytes > PROVIDER_TOOL_RESPONSE_BYTE_LIMIT {
            return Err(format!(
                "Provider structured SSE exceeds the {PROVIDER_TOOL_RESPONSE_BYTE_LIMIT} byte limit."
            ));
        }
        pending.extend_from_slice(chunk.as_ref());
        // consumed is the end of the last complete frame; the buffer is compacted once per chunk.
        // consumed 是最后一个完整帧的结束位置；缓冲区每个分块只压缩一次。
        let mut consumed = 0usize;
        while let Some((frame_end, boundary_end)) =
            find_sse_frame_boundary(&pending, consumed.max(scan_from))
        {
            let frame_start = consumed;
            consumed = boundary_end;
            let data = sse_frame_data(&pending[frame_start..frame_end])?;
            let Some(data) = data else {
                continue;
            };
            if data == "[DONE]" {
                if !contract.allow_done_record || terminal.is_none() {
                    return Err(String::from(
                        "Provider structured SSE emitted an unexpected [DONE] record.",
                    ));
                }
                continue;
            }
            let value = parse_bounded_provider_json(data.as_str(), "Provider structured SSE")?;
            let event_type = value
                .pointer(contract.event_type_path.as_str())
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "Provider structured SSE event omitted string discriminator `{}`.",
                        contract.event_type_path
                    )
                })?;
            if contract.terminal_event_types.contains(event_type) {
                if terminal.replace(value).is_some() {
                    return Err(String::from(
                        "Provider structured SSE emitted multiple terminal events.",
                    ));
                }
            } else if !contract.nonterminal_event_types.contains(event_type) {
                return Err(format!(
                    "Provider structured SSE emitted undeclared event type `{event_type}`."
                ));
            }
        }
        pending.drain(..consumed);
        // A boundary may start in the last two unscanned bytes and complete in the next chunk.
        // 边界可能起始于最后两个字节，并在下一个分块中完成。
        scan_from = pending.len().saturating_sub(2);
    }
    if !pending.iter().all(u8::is_ascii_whitespace) {
        return Err(String::from(
            "Provider structured SSE ended with an incomplete frame.",
        ));
    }
    terminal.ok_or_else(|| String::from("Provider structured SSE returned no terminal event."))
}

/// Finds the earliest blank-line SSE frame boundary at or after `from`, accepting LF and CRLF line endings.
/// 查找 `from` 及之后最早的空行 SSE 帧边界，同时接受 LF 与 CRLF 行结束符。
///
/// # Returns
/// 返回值
/// Returns the frame end, including its final line terminator, and the end of the blank separator line.
/// 返回包含最后一个行结束符的帧结束位置，以及空白分隔行的结束位置。
fn find_sse_frame_boundary(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from;
    while let Some(offset) = bytes.get(index..)?.iter().position(|byte| *byte == b'\n') {
        let newline = index + offset;
        match bytes.get(newline + 1) {
            Some(b'\n') => return Some((newline + 1, newline + 2)),
            Some(b'\r') if bytes.get(newline + 2) == Some(&b'\n') => {
                return Some((newline + 1, newline + 3));
            }
            _ => index = newline + 1,
        }
    }
    None
}

/// Joins all data lines from one SSE frame into one JSON payload.
/// 把一个 SSE 帧的全部 data 行拼接为一份 JSON 载荷。
fn sse_frame_data(frame: &[u8]) -> Result<Option<String>, String> {
    let frame = std::str::from_utf8(frame)
        .map_err(|_| String::from("Provider structured SSE frame is not UTF-8."))?;
    let lines = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(lines.join("\n")))
}

/// Rejects encoded inputs whose padded decoded size exceeds the selected bound before allocation.
/// 在分配前拒绝填充后解码长度超过所选限制的编码输入。
fn validate_base64_decoded_size(encoded: &str, maximum: u64) -> Result<(), String> {
    let length = encoded.len();
    if length == 0 || length % 4 != 0 {
        return Err(String::from(
            "Provider multipart Data URL must contain non-empty padded base64.",
        ));
    }
    let padding = encoded
        .as_bytes()
        .iter()
        .rev()
        .take(3)
        .take_while(|byte| **byte == b'=')
        .count();
    if padding > 2 {
        return Err(String::from(
            "Provider multipart Data URL contains invalid base64 padding.",
        ));
    }
    let decoded = (length / 4)
        .checked_mul(3)
        .and_then(|size| size.checked_sub(padding));
    if decoded.is_none_or(|size| size as u64 > maximum) {
        return Err(format!(
            "Provider multipart file exceeds the {maximum} decoded byte limit."
        ));
    }
    Ok(())
}

/// Decodes one bounded base64 Data URL for a multipart file part.
/// 解码一个用于 Multipart 文件部分的有界 Base64 Data URL。
fn decode_bounded_multipart_data_url(data_url: &str) -> Result<(String, Vec<u8>), String> {
    let payload = data_url
        .strip_prefix("data:")
        .ok_or_else(|| String::from("Provider multipart file must use a Data URL."))?;
    let (metadata, encoded) = payload.split_once(',').ok_or_else(|| {
        String::from("Provider multipart Data URL omitted its payload separator.")
    })?;
    let mime_type = metadata
        .strip_suffix(";base64")
        .ok_or_else(|| String::from("Provider multipart Data URL must use base64 encoding."))?;
    if mime_type.is_empty() {
        return Err(String::from(
            "Provider multipart Data URL omitted its MIME type.",
        ));
    }
    validate_base64_decoded_size(encoded, GENERIC_RESOURCE_BYTE_LIMIT)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("Provider multipart Data URL is invalid base64: {error}"))?;
    if bytes.is_empty() || bytes.len() as u64 > GENERIC_RESOURCE_BYTE_LIMIT {
        return Err(format!(
            "Provider multipart file must contain 1 to {GENERIC_RESOURCE_BYTE_LIMIT} decoded bytes."
        ));
    }
    Ok((mime_type.to_string(), bytes))
}

/// Decodes one bounded OpenAI-compatible Chat SSE response.
/// 解码一份有界 OpenAI 兼容 Chat SSE 响应。
///
/// # Parameters
/// 参数
/// `response` is one successful streaming HTTP response.
/// `response` 是一份成功的流式 HTTP 响应。
///
/// # Returns
/// 返回值
/// Non-empty visible text after a unique `[DONE]` terminal record.
/// 返回唯一 `[DONE]` 终止记录之后确认的非空可见文本。
async fn decode_generic_openai_chat_sse(response: reqwest::Response) -> Result<String, String> {
    let mut stream = response.bytes_stream();
    let mut decoder = chat_sse::ChatSseDecoder::default();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            format!(
                "Provider Chat SSE could not be read: {}",
                error.without_url()
            )
        })?;
        decoder.push(&chunk)?;
    }
    decoder.finish()
}

/// Appends visible text from one closed OpenAI-compatible Chat delta event.
/// 从一项封闭 OpenAI 兼容 Chat 增量事件追加可见文本。
///
/// # Parameters
/// 参数
/// `output` is the bounded accumulator and `value` is one decoded SSE data object.
/// `output` 是有界累加器，`value` 是一个已解码 SSE data 对象。
///
/// # Returns
/// 返回值
/// Success when every encountered content value uses a registered text shape.
/// 当遇到的每个内容值均使用已注册文本结构时返回成功。
fn append_generic_openai_chat_delta(output: &mut String, value: &Value) -> Result<(), String> {
    let choices = value
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("Provider Chat SSE event omitted choices."))?;
    for choice in choices {
        let Some(content) = choice.get("delta").and_then(|delta| delta.get("content")) else {
            continue;
        };
        if let Some(text) = content.as_str() {
            output.push_str(text);
            continue;
        }
        if let Some(parts) = content.as_array() {
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(text) = part.get("text").and_then(Value::as_str)
                {
                    output.push_str(text);
                }
            }
            continue;
        }
        if !content.is_null() {
            return Err(String::from(
                "Provider Chat SSE content has an unsupported shape.",
            ));
        }
    }
    Ok(())
}

/// Rejects operation profiles for named codecs whose wire body is not the canonical arguments.
/// 拒绝把操作 Profile 用于 Wire 正文并非规范参数的具名编解码器。
///
/// # Parameters
/// 参数
/// `target` selects the installed codec while `request_profiles` and `response_profiles` are the
/// exact frozen operation chains.
/// `target` 选择已安装编解码器，`request_profiles` 与 `response_profiles` 是精确冻结的操作链。
///
/// # Returns
/// 返回值
/// Success for the generic JSON primitive or an explicit pre-dispatch incompatibility error.
/// 通用 JSON 原语返回成功，否则返回明确的分派前不兼容错误。
fn ensure_operation_profiles_supported(
    target: &ProviderToolTarget,
    request_profiles: &[crate::provider_manifest::RequestProjectionProfile],
    response_profiles: &[crate::provider_manifest::ResponseProjectionProfile],
) -> Result<(), String> {
    if target.execution.adapter == ProviderToolAdapterKind::GenericJson
        || (request_profiles.is_empty() && response_profiles.is_empty())
    {
        return Ok(());
    }
    Err(format!(
        "Provider tool `{}` references operation profiles but adapter `{:?}` does not expose the generic JSON wire contract.",
        target.target_id, target.execution.adapter
    ))
}

/// Decodes and projects one raw generic JSON response before output normalization.
/// 在输出归一化前解码并投影一个原始通用 JSON 响应。
///
/// # Parameters
/// 参数
/// `profiles` is the frozen response chain, `text` is the bounded UTF-8 response, and
/// `operation` scopes deterministic diagnostics.
/// `profiles` 是冻结响应链，`text` 是有界 UTF-8 响应，`operation` 限定确定性诊断范围。
///
/// # Returns
/// 返回值
/// The projected response object or an explicit decoding/projection failure.
/// 返回投影后的响应对象，或明确的解码与投影失败。
fn decode_and_project_generic_json_value(
    profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    text: &str,
    operation: &str,
) -> Result<Value, String> {
    let response = parse_bounded_provider_json(text, operation)?;
    crate::execution::project_json_response_profiles(profiles, response)
        .map_err(|error| format!("{operation} response profile failed: {error}"))
}

/// Projects and normalizes one generic JSON response through its output contract.
/// 通过输出契约投影并归一化一个通用 JSON 响应。
///
/// # Parameters
/// 参数
/// `contract`, `profiles`, and `text` are frozen response facts while `operation` scopes errors.
/// `contract`、`profiles` 与 `text` 是冻结响应事实，`operation` 限定错误范围。
///
/// # Returns
/// 返回值
/// One strongly typed provider-tool result or a deterministic contract failure.
/// 返回一个强类型供应商工具结果或确定性的契约失败。
fn decode_and_project_generic_json_response(
    contract: &crate::ProviderToolOutputContract,
    profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    text: &str,
    operation: &str,
    request_arguments: Option<&Value>,
) -> Result<ProviderToolExecutionResult, String> {
    let response = decode_and_project_generic_json_value(profiles, text, operation)?;
    let projected = serde_json::to_string(&response).map_err(|error| {
        format!("{operation} projected response could not be serialized: {error}")
    })?;
    decode_generic_json_sync_output_with_request(contract, projected.as_str(), request_arguments)
}

/// Applies literal JSON Schema defaults to one canonical provider-tool argument tree.
/// 把字面 JSON Schema 默认值应用到一份规范供应商工具参数树。
///
/// # Parameters
/// 参数
/// `schema` is the validated model-visible schema, `value` is the exact argument value, and
/// `depth` enforces the shared Manifest recursion ceiling.
/// `schema` 是已校验的模型可见 Schema，`value` 是精确参数值，`depth` 强制执行共享 Manifest 递归上限。
///
/// # Returns
/// 返回值
/// Success after deterministic default materialization or one bounded-depth error.
/// 确定性物化默认值后返回成功，或返回一条有界深度错误。
fn apply_json_schema_defaults(
    schema: &Value,
    value: &mut Value,
    depth: usize,
) -> Result<(), String> {
    if depth > crate::provider_manifest::MAX_JSON_DEPTH {
        return Err(format!(
            "Provider tool JSON Schema defaults exceed the {} level depth limit.",
            crate::provider_manifest::MAX_JSON_DEPTH
        ));
    }
    if let (Some(properties), Some(object)) = (
        schema.get("properties").and_then(Value::as_object),
        value.as_object_mut(),
    ) {
        for (property, property_schema) in properties {
            if !object.contains_key(property)
                && let Some(default) = property_schema.get("default")
            {
                object.insert(property.clone(), default.clone());
            }
            if let Some(property_value) = object.get_mut(property) {
                apply_json_schema_defaults(property_schema, property_value, depth + 1)?;
            }
        }
    }
    if let (Some(item_schema), Some(items)) = (schema.get("items"), value.as_array_mut()) {
        for item in items {
            apply_json_schema_defaults(item_schema, item, depth + 1)?;
        }
    }
    Ok(())
}

/// Validates one provider-tool argument object against its exact frozen JSON Schema.
/// 按照精确冻结的 JSON Schema 校验一份供应商工具参数对象。
///
/// # Parameters
/// 参数
/// `validator` is compiled from the frozen manifest schema; `arguments` are pre- or post-default values, and
/// `tool_id` scopes the controlled diagnostic.
/// `validator` 从冻结 Manifest Schema 编译，`arguments` 是默认值前后的精确值，`tool_id` 限定诊断。
///
/// # Returns
/// 返回值
/// Success only when the arguments satisfy the already-compiled frozen schema.
/// 仅当参数满足已编译冻结 Schema 的全部约束时返回成功。
fn validate_provider_tool_arguments(
    validator: &jsonschema::Validator,
    arguments: &Value,
    tool_id: &str,
) -> Result<(), String> {
    validator
        .validate(arguments)
        .map_err(|error| format!("Provider tool `{tool_id}` arguments are invalid: {error}"))
}

/// Enforces host-only product ranges after JSON Schema validation and default materialization.
/// 在 JSON Schema 校验与默认值物化之后执行仅宿主可见的乘积区间约束。
///
/// # Parameters
/// 参数
/// `ranges` is the frozen manifest declaration, `arguments` is the canonical input object, and
/// `tool_id` scopes deterministic diagnostics.
/// `ranges` 是冻结的 Manifest 声明，`arguments` 是规范输入对象，`tool_id` 限定确定性诊断。
///
/// # Returns
/// 返回值
/// Success when every present factor pair has a checked product inside its inclusive range.
/// 当每一对已提供因子的受检乘积都位于闭区间内时返回成功。
fn validate_provider_tool_numeric_product_ranges(
    ranges: &[crate::ProviderToolNumericProductRange],
    arguments: &Value,
    tool_id: &str,
) -> Result<(), String> {
    if ranges.len() > 8 {
        return Err(format!(
            "Provider tool `{tool_id}` declares more than 8 numeric product ranges."
        ));
    }
    for range in ranges {
        let left = arguments.pointer(range.factor_paths[0].as_str());
        let right = arguments.pointer(range.factor_paths[1].as_str());
        match (left, right) {
            (None, None) => continue,
            (None, Some(_)) | (Some(_), None) if range.require_all_or_none => {
                return Err(format!(
                    "Provider tool `{tool_id}` inputs `{}` and `{}` must be supplied together.",
                    range.factor_paths[0], range.factor_paths[1]
                ));
            }
            (None, Some(_)) | (Some(_), None) => continue,
            (Some(left), Some(right)) => {
                let left = left.as_u64().ok_or_else(|| {
                    format!(
                        "Provider tool `{tool_id}` input `{}` must be a non-negative integer.",
                        range.factor_paths[0]
                    )
                })?;
                let right = right.as_u64().ok_or_else(|| {
                    format!(
                        "Provider tool `{tool_id}` input `{}` must be a non-negative integer.",
                        range.factor_paths[1]
                    )
                })?;
                let product = left.checked_mul(right).ok_or_else(|| {
                    format!(
                        "Provider tool `{tool_id}` numeric product for `{}` and `{}` overflows.",
                        range.factor_paths[0], range.factor_paths[1]
                    )
                })?;
                if !(range.minimum..=range.maximum).contains(&product) {
                    return Err(format!(
                        "Provider tool `{tool_id}` numeric product for `{}` and `{}` must be between {} and {}.",
                        range.factor_paths[0], range.factor_paths[1], range.minimum, range.maximum
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Frozen dependencies shared by one provider asynchronous task lifecycle operation.
/// 一次供应商异步任务生命周期操作共享的冻结依赖。
pub(crate) struct ProviderAsyncExecutionContext<'a> {
    /// Shared HTTP client with the host security policy.
    /// 携带宿主安全策略的共享 HTTP 客户端。
    pub http: &'a reqwest::Client,
    /// Exact service entry selected for the task.
    /// 为任务选择的精确服务入口。
    pub entry: &'a ServiceEntry,
    /// Exact provider-tool target selected for the task.
    /// 为任务选择的精确供应商工具目标。
    pub target: &'a ProviderToolTarget,
    /// Frozen request projection profiles.
    /// 冻结的请求投影 Profile。
    pub request_profiles: &'a [crate::provider_manifest::RequestProjectionProfile],
    /// Frozen response projection profiles.
    /// 冻结的响应投影 Profile。
    pub response_profiles: &'a [crate::provider_manifest::ResponseProjectionProfile],
    /// Exact credential projection loaded for the frozen route.
    /// 为冻结路由加载的精确凭据投影。
    pub secret: &'a CredentialExecutionSecret,
}

/// Starts one exact provider-native asynchronous task.
/// 启动一个精确供应商原生异步任务。
///
/// # Parameters
/// 参数
/// Exact route, materialized arguments, and credential are frozen before dispatch.
/// 精确路由、已物化参数与凭据在分派前冻结。
///
/// # Returns
/// 返回值
/// A provider-confirmed task identity and non-secret recovery context.
/// 返回供应商确认的任务身份与非秘密恢复上下文。
pub(crate) async fn start_provider_async_task(
    context: ProviderAsyncExecutionContext<'_>,
    arguments: Value,
    async_execution: Option<&crate::provider_manifest::AsyncOperationDeclaration>,
) -> Result<ProviderAsyncTaskStart, String> {
    ensure_operation_profiles_supported(
        context.target,
        context.request_profiles,
        context.response_profiles,
    )?;
    if context.target.execution.adapter != ProviderToolAdapterKind::GenericJson {
        return Err(format!(
            "Provider tool `{}` does not use the installed generic JSON adapter.",
            context.target.target_id
        ));
    }
    start_generic_json_async_task(
        &context,
        arguments,
        require_generic_json_lifecycle(async_execution, context.target)?,
    )
    .await
}

/// Polls one exact provider-native asynchronous task once.
/// 单次轮询一个精确供应商原生异步任务。
///
/// # Parameters
/// 参数
/// Exact route, task identity, recovery context, and credential remain immutable.
/// 精确路由、任务身份、恢复上下文与凭据保持不可变。
///
/// # Returns
/// 返回值
/// A pending phase or a normalized terminal result.
/// 返回等待阶段或归一化终态结果。
pub(crate) async fn poll_provider_async_task(
    context: ProviderAsyncExecutionContext<'_>,
    provider_task_id: &str,
    recovery: &ProviderAsyncTaskRecoveryContext,
    async_execution: Option<&crate::provider_manifest::AsyncOperationDeclaration>,
) -> Result<ProviderAsyncTaskPoll, String> {
    ensure_operation_profiles_supported(context.target, &[], context.response_profiles)?;
    if context.target.execution.adapter != ProviderToolAdapterKind::GenericJson {
        return Err(format!(
            "Provider tool `{}` does not use the installed generic JSON adapter.",
            context.target.target_id
        ));
    }
    poll_generic_json_async_task(
        &context,
        provider_task_id,
        recovery,
        require_generic_json_lifecycle(async_execution, context.target)?,
    )
    .await
}

/// Requests cancellation of one exact provider-native asynchronous task.
/// 请求取消一个精确供应商原生异步任务。
///
/// # Parameters
/// 参数
/// Exact route, provider task identity, and credential remain immutable.
/// 精确路由、供应商任务身份与凭据保持不可变。
///
/// # Returns
/// 返回值
/// Success only after provider confirmation; unsupported adapters fail explicitly.
/// 仅在供应商确认后返回成功；不支持的适配器明确失败。
pub(crate) async fn cancel_provider_async_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    provider_task_id: &str,
    secret: &CredentialExecutionSecret,
    async_execution: Option<&crate::provider_manifest::AsyncOperationDeclaration>,
) -> Result<(), String> {
    if target.execution.adapter != ProviderToolAdapterKind::GenericJson {
        return Err(format!(
            "Provider tool `{}` does not use the installed generic JSON adapter.",
            target.target_id
        ));
    }
    cancel_generic_json_async_task(
        http,
        entry,
        target,
        provider_task_id,
        secret,
        require_generic_json_lifecycle(async_execution, target)?,
    )
    .await
}

/// Requires the closed generic JSON lifecycle attached to one asynchronous operation.
/// 要求一个异步操作附带封闭的通用 JSON 生命周期。
///
/// # Parameters
/// 参数
/// `async_execution` is the frozen operation payload and `target` scopes diagnostics.
/// `async_execution` 是冻结的操作载荷，`target` 限定诊断范围。
///
/// # Returns
/// 返回值
/// The exact generic lifecycle or a deterministic pre-dispatch error.
/// 返回精确通用生命周期或确定性的分派前错误。
fn require_generic_json_lifecycle<'a>(
    async_execution: Option<&'a crate::provider_manifest::AsyncOperationDeclaration>,
    target: &ProviderToolTarget,
) -> Result<&'a crate::provider_manifest::GenericJsonAsyncLifecycleDeclaration, String> {
    async_execution
        .and_then(|declaration| declaration.generic_json.as_ref())
        .ok_or_else(|| {
            format!(
                "Generic asynchronous provider tool `{}` has no generic JSON lifecycle declaration.",
                target.target_id
            )
        })
}

/// Resolves fixed and predicate-enabled headers for one asynchronous task creation request.
/// 为一次异步任务创建请求解析固定与断言启用的请求头。
///
/// # Parameters
/// 参数
/// `target` owns immutable header declarations and `arguments` contains post-upload canonical data.
/// `target` 持有不可变请求头声明，`arguments` 包含上传后的规范数据。
///
/// # Returns
/// 返回值
/// A non-overlapping header map ready for validated HTTP insertion.
/// 返回可供已校验 HTTP 插入使用的无重叠请求头映射。
fn provider_tool_start_headers(
    target: &ProviderToolTarget,
    arguments: &Value,
) -> Result<BTreeMap<String, String>, String> {
    let mut headers = target.execution.start_fixed_headers.clone();
    for rule in &target.execution.start_conditional_headers {
        let enabled = rule
            .when_any
            .iter()
            .map(|predicate| provider_tool_argument_predicate_matches(arguments, predicate))
            .collect::<Result<Vec<_>, String>>()?
            .into_iter()
            .any(|matched| matched);
        if enabled
            && headers
                .insert(rule.name.clone(), rule.value.clone())
                .is_some()
        {
            return Err(format!(
                "Provider tool conditional start header `{}` overlaps a fixed header.",
                rule.name
            ));
        }
    }
    Ok(headers)
}

/// Evaluates one bounded JSON Pointer pattern against canonical provider-tool arguments.
/// 针对规范供应商工具参数求值一个有界 JSON Pointer 模式。
///
/// # Parameters
/// 参数
/// `arguments` is the canonical JSON object and `predicate` declares one optional array wildcard
/// plus an optional exact scalar comparison.
/// `arguments` 是规范 JSON 对象，`predicate` 声明一个可选数组通配符与可选精确标量比较。
///
/// # Returns
/// 返回值
/// Whether at least one selected value satisfies the declaration.
/// 返回至少一个选中值是否满足声明。
fn provider_tool_argument_predicate_matches(
    arguments: &Value,
    predicate: &crate::ProviderToolArgumentPredicate,
) -> Result<bool, String> {
    let mut values = vec![arguments];
    for raw_segment in predicate.pointer_pattern.split('/').skip(1) {
        let mut next = Vec::new();
        if raw_segment == "*" {
            for value in values {
                if let Some(items) = value.as_array() {
                    next.extend(items.iter());
                }
            }
        } else {
            let segment = decode_json_pointer_segment(raw_segment)?;
            for value in values {
                match value {
                    Value::Object(object) => {
                        if let Some(selected) = object.get(segment.as_str()) {
                            next.push(selected);
                        }
                    }
                    Value::Array(items) => {
                        if let Ok(index) = segment.parse::<usize>()
                            && let Some(selected) = items.get(index)
                        {
                            next.push(selected);
                        }
                    }
                    _ => {}
                }
            }
        }
        values = next;
        if values.is_empty() {
            return Ok(false);
        }
    }
    Ok(values.into_iter().any(|value| {
        if let Some(expected) = predicate.equals.as_ref() {
            return value == expected;
        }
        match value {
            Value::Null => false,
            Value::String(value) => !value.is_empty(),
            Value::Array(values) => !values.is_empty(),
            Value::Object(values) => !values.is_empty(),
            Value::Bool(_) | Value::Number(_) => true,
        }
    }))
}

/// Decodes one RFC 6901 JSON Pointer segment and rejects malformed escapes.
/// 解码一个 RFC 6901 JSON Pointer 段并拒绝格式错误的转义。
///
/// # Parameters
/// 参数
/// `segment` is one raw slash-delimited pointer segment.
/// `segment` 是一个原始斜杠分隔 Pointer 段。
///
/// # Returns
/// 返回值
/// The decoded object key or array index text.
/// 返回已解码对象键或数组索引文本。
fn decode_json_pointer_segment(segment: &str) -> Result<String, String> {
    let mut decoded = String::with_capacity(segment.len());
    let mut characters = segment.chars();
    while let Some(character) = characters.next() {
        if character != '~' {
            decoded.push(character);
            continue;
        }
        match characters.next() {
            Some('0') => decoded.push('~'),
            Some('1') => decoded.push('/'),
            _ => {
                return Err(String::from(
                    "Provider tool conditional header pointer contains an invalid JSON Pointer escape.",
                ));
            }
        }
    }
    Ok(decoded)
}

/// Starts one adapter-free JSON task through its manifest-owned start and task-id contracts.
/// 通过 Manifest 持有的启动与任务标识契约启动一个无具名适配器 JSON 任务。
async fn start_generic_json_async_task(
    context: &ProviderAsyncExecutionContext<'_>,
    arguments: Value,
    lifecycle: &crate::provider_manifest::GenericJsonAsyncLifecycleDeclaration,
) -> Result<ProviderAsyncTaskStart, String> {
    let canonical_arguments = arguments;
    let arguments = crate::execution::project_json_request_profiles(
        context.request_profiles,
        context.target.upstream_target_id.as_str(),
        canonical_arguments.clone(),
    )?;
    let start_headers = provider_tool_start_headers(context.target, &canonical_arguments)?;
    let text = execute_generic_json_sync_request_with_headers(
        context.http,
        context.entry,
        context.target,
        context.secret,
        &arguments,
        context.target.contract.tool_id.as_str(),
        &start_headers,
    )
    .await?;
    let response = decode_and_project_generic_json_value(
        context.response_profiles,
        text.as_str(),
        "Provider task start",
    )?;
    if response.get("error").is_some() {
        return Err(format_structured_provider_error(
            "Provider task start",
            None,
            &response,
        ));
    }
    if let Some(requirements) = lifecycle.start_requirements.as_ref() {
        validate_generic_json_response_requirements(
            &response,
            requirements,
            "Provider task start",
        )?;
    }
    if let Some(status_pointer) = lifecycle.start_status_pointer.as_deref() {
        let status = response
            .pointer(status_pointer)
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "Provider task start response is missing string status at `{status_pointer}`."
                )
            })?;
        if !lifecycle
            .start_success_statuses
            .iter()
            .any(|accepted| accepted == status)
        {
            return Err(format!(
                "Provider task start returned unaccepted status `{status}`."
            ));
        }
    }
    let provider_task_id = response
        .pointer(lifecycle.task_id_pointer.as_str())
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.trim() == *value)
        .ok_or_else(|| {
            format!(
                "Provider task start response is missing non-empty task id at `{}`.",
                lifecycle.task_id_pointer
            )
        })?
        .to_string();
    Ok(ProviderAsyncTaskStart {
        provider_task_id,
        recovery: generic_json_async_recovery_context(&canonical_arguments, lifecycle)?,
        cancellation_supported: lifecycle.cancel_endpoint_path.is_some(),
    })
}

/// Builds provider-neutral ordered recovery fingerprints from one declared canonical argument.
/// 从一个已声明规范参数构建供应商中性的有序恢复指纹。
///
/// # Parameters
/// 参数
/// `arguments` contains post-upload canonical values and `lifecycle` selects the only authorized
/// identity source pointer.
/// `arguments` 包含上传后的规范值，`lifecycle` 选择唯一获授权的身份源 Pointer。
///
/// # Returns
/// 返回值
/// No recovery state, or at most 256 ordered SHA-256 fingerprints without retaining signed URLs.
/// 返回无恢复状态，或最多 256 个有序 SHA-256 指纹且不保留签名 URL。
fn generic_json_async_recovery_context(
    arguments: &Value,
    lifecycle: &crate::provider_manifest::GenericJsonAsyncLifecycleDeclaration,
) -> Result<ProviderAsyncTaskRecoveryContext, String> {
    let Some(pointer) = lifecycle.recovery_identity_source_pointer.as_deref() else {
        return Ok(ProviderAsyncTaskRecoveryContext::None);
    };
    let source = arguments.pointer(pointer).ok_or_else(|| {
        format!("Provider task recovery identity source `{pointer}` is missing after upload.")
    })?;
    let values = match source {
        Value::String(_) => std::slice::from_ref(source),
        Value::Array(values) if !values.is_empty() && values.len() <= 256 => values.as_slice(),
        _ => {
            return Err(format!(
                "Provider task recovery identity source `{pointer}` must be one string or an array of one through 256 strings."
            ));
        }
    };
    let fingerprints = values
        .iter()
        .map(|value| {
            let identity = value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "Provider task recovery identity source `{pointer}` contains a non-string or empty item."
                    )
                })?;
            Ok(hex::encode(Sha256::digest(identity.as_bytes())))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ProviderAsyncTaskRecoveryContext::OrderedSha256 { fingerprints })
}

/// Enforces one closed set of string requirements against a projected JSON response.
/// 对一份已投影 JSON 响应执行一组封闭字符串要求。
///
/// # Parameters
/// 参数
/// `response` is the bounded projected object, `requirements` contains validated pointers, and
/// `operation` scopes deterministic diagnostics.
/// `response` 是有界已投影对象，`requirements` 包含已校验 Pointer，`operation` 限定确定性诊断。
///
/// # Returns
/// 返回值
/// Success only when every declared non-empty and empty-or-missing predicate holds.
/// 仅当每项已声明非空与空或缺失断言均成立时返回成功。
fn validate_generic_json_response_requirements(
    response: &Value,
    requirements: &crate::provider_manifest::GenericJsonResponseRequirementsDeclaration,
    operation: &str,
) -> Result<(), String> {
    for pointer in &requirements.non_empty_string_pointers {
        if response
            .pointer(pointer)
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(format!(
                "{operation} response requires a non-empty string at `{pointer}`."
            ));
        }
    }
    for pointer in &requirements.empty_or_missing_string_pointers {
        if response
            .pointer(pointer)
            .is_some_and(|value| value.as_str().is_none_or(|value| !value.trim().is_empty()))
        {
            return Err(format!(
                "{operation} response requires an empty or missing string at `{pointer}`."
            ));
        }
    }
    Ok(())
}

/// Polls one adapter-free JSON task through exact manifest status sets and output contract.
/// 通过精确 Manifest 状态集合与输出契约轮询一个无具名适配器 JSON 任务。
async fn poll_generic_json_async_task(
    context: &ProviderAsyncExecutionContext<'_>,
    provider_task_id: &str,
    recovery: &ProviderAsyncTaskRecoveryContext,
    lifecycle: &crate::provider_manifest::GenericJsonAsyncLifecycleDeclaration,
) -> Result<ProviderAsyncTaskPoll, String> {
    let url = generic_json_async_url(
        context.entry,
        context.target,
        lifecycle.poll_endpoint_path.as_str(),
        lifecycle.task_id_query_parameter.as_deref(),
        lifecycle.task_id_in_path,
        None,
        provider_task_id,
    )?;
    let text = execute_generic_json_async_request(
        context.http,
        context.target,
        context.secret,
        lifecycle.poll_method,
        url,
        "Provider task poll",
    )
    .await?;
    let response = decode_and_project_generic_json_value(
        context.response_profiles,
        text.as_str(),
        "Provider task poll",
    )?;
    if response.get("error").is_some() {
        return Ok(ProviderAsyncTaskPoll::Failed(
            format_structured_provider_error("Provider task poll", None, &response),
        ));
    }
    if let Some(requirements) = lifecycle.poll_requirements.as_ref() {
        validate_generic_json_response_requirements(&response, requirements, "Provider task poll")?;
    }
    if let Some(pointer) = lifecycle.poll_task_id_pointer.as_deref()
        && response.pointer(pointer).and_then(Value::as_str) != Some(provider_task_id)
    {
        return Err(format!(
            "Provider task poll response does not correlate task id at `{pointer}`."
        ));
    }
    let status = response
        .pointer(lifecycle.status_pointer.as_str())
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "Provider task poll response is missing non-empty status at `{}`.",
                lifecycle.status_pointer
            )
        })?;
    if lifecycle
        .queued_statuses
        .iter()
        .any(|value| value == status)
    {
        return Ok(ProviderAsyncTaskPoll::Pending(
            crate::provider_task::ProviderAsyncTaskPhase::Queued,
        ));
    }
    if lifecycle
        .running_statuses
        .iter()
        .any(|value| value == status)
    {
        return Ok(ProviderAsyncTaskPoll::Pending(
            crate::provider_task::ProviderAsyncTaskPhase::Running,
        ));
    }
    if lifecycle
        .success_statuses
        .iter()
        .any(|value| value == status)
    {
        if let Some(requirements) = lifecycle.poll_success_requirements.as_ref()
            && let Err(error) = validate_generic_json_response_requirements(
                &response,
                requirements,
                "Provider task successful poll",
            )
        {
            return Ok(failed_provider_async_task_completed_result(error));
        }
        if let Some(follow_up) = lifecycle.success_batch_follow_up.as_ref() {
            let output = match execute_generic_json_async_batch_follow_up(
                context.http,
                context.entry,
                context.target,
                &response,
                recovery,
                follow_up,
            )
            .await
            {
                Ok(output) => output,
                Err(error) => {
                    return Ok(failed_provider_async_task_completed_result(error));
                }
            };
            return Ok(ProviderAsyncTaskPoll::Completed(
                ProviderToolExecutionResult {
                    upstream_response_id: Some(provider_task_id.to_string()),
                    output: ProviderToolNormalizedOutput::Text(output),
                },
            ));
        }
        let response = if let Some(follow_up) = lifecycle.success_follow_up.as_ref() {
            let identifier = match follow_up.identifier_source {
                crate::provider_manifest::GenericJsonAsyncFollowUpIdentifierSource::ResponsePointer => {
                    let pointer = follow_up.identifier_pointer.as_deref().ok_or_else(|| {
                        String::from(
                            "Provider task success follow-up response pointer is missing.",
                        )
                    })?;
                    match response
                        .pointer(pointer)
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                    {
                        Some(identifier) => identifier,
                        None => {
                            return Ok(failed_provider_async_task_completed_result(format!(
                                "Provider task successful response is missing non-empty follow-up identifier at `{pointer}`."
                            )));
                        }
                    }
                }
                crate::provider_manifest::GenericJsonAsyncFollowUpIdentifierSource::ProviderTaskId => {
                    provider_task_id
                }
            };
            let url = match generic_json_async_url(
                context.entry,
                context.target,
                follow_up.endpoint_path.as_str(),
                follow_up.identifier_query_parameter.as_deref(),
                follow_up.identifier_in_path,
                follow_up.identifier_path_suffix.as_deref(),
                identifier,
            ) {
                Ok(url) => url,
                Err(error) => {
                    return Ok(failed_provider_async_task_completed_result(error));
                }
            };
            if follow_up.response_body
                == crate::provider_manifest::GenericJsonAsyncFollowUpResponseBody::BinaryResource
            {
                let result = match execute_generic_json_async_binary_follow_up(
                    context.http,
                    context.target,
                    context.secret,
                    follow_up.method,
                    url,
                    provider_task_id,
                )
                .await
                {
                    Ok(result) => result,
                    Err(error) => {
                        return Ok(failed_provider_async_task_completed_result(error));
                    }
                };
                return Ok(ProviderAsyncTaskPoll::Completed(result));
            }
            let text = match execute_generic_json_async_request(
                context.http,
                context.target,
                context.secret,
                follow_up.method,
                url,
                "Provider task success follow-up",
            )
            .await
            {
                Ok(text) => text,
                Err(error) => {
                    return Ok(failed_provider_async_task_completed_result(error));
                }
            };
            let response = match decode_and_project_generic_json_value(
                context.response_profiles,
                text.as_str(),
                "Provider task success follow-up",
            ) {
                Ok(response) => response,
                Err(error) => {
                    return Ok(failed_provider_async_task_completed_result(error));
                }
            };
            if response.get("error").is_some() {
                return Ok(ProviderAsyncTaskPoll::Failed(
                    format_structured_provider_error(
                        "Provider task success follow-up",
                        None,
                        &response,
                    ),
                ));
            }
            response
        } else {
            response
        };
        return Ok(decode_completed_generic_json_async_result(
            &context.target.contract.output,
            &response,
            provider_task_id,
        ));
    }
    if lifecycle
        .failure_statuses
        .iter()
        .any(|value| value == status)
    {
        return Ok(ProviderAsyncTaskPoll::Failed(format!(
            "Provider task reached configured failure status `{status}`."
        )));
    }
    if lifecycle
        .cancelled_statuses
        .iter()
        .any(|value| value == status)
    {
        return Ok(ProviderAsyncTaskPoll::Cancelled);
    }
    Ok(ProviderAsyncTaskPoll::Failed(format!(
        "Provider task returned unknown status `{status}`."
    )))
}

/// Converts one provider-confirmed success document into a terminal normalized result.
/// 将一份供应商已确认成功的文档转换为终态规范化结果。
///
/// # Parameters
/// 参数
/// `contract` is the frozen output contract, `response` is the successful terminal document, and
/// `provider_task_id` is the correlated upstream task identity.
/// `contract` 是冻结输出契约，`response` 是成功终态文档，`provider_task_id` 是关联的上游任务身份。
///
/// # Returns
/// 返回值
/// A completed result, or an explicit terminal failure when the successful provider result cannot
/// satisfy the frozen host contract.
/// 返回完成结果；当供应商成功结果无法满足冻结宿主契约时，返回明确终态失败。
fn decode_completed_generic_json_async_result(
    contract: &crate::ProviderToolOutputContract,
    response: &Value,
    provider_task_id: &str,
) -> ProviderAsyncTaskPoll {
    // projected preserves the exact successful follow-up document for the shared output decoder.
    // projected 为共享输出解码器保留精确的成功跟随文档。
    let projected = match serde_json::to_string(response) {
        Ok(projected) => projected,
        Err(error) => {
            return failed_provider_async_task_completed_result(format!(
                "Provider task successful response could not be serialized: {error}"
            ));
        }
    };
    // result is terminal because the provider has already confirmed that generation completed.
    // result 属于终态，因为供应商已经确认生成完成。
    let mut result = match decode_generic_json_sync_output(contract, projected.as_str()) {
        Ok(result) => result,
        Err(error) => return failed_provider_async_task_completed_result(error),
    };
    if result.upstream_response_id.is_none() {
        result.upstream_response_id = Some(provider_task_id.to_string());
    }
    ProviderAsyncTaskPoll::Completed(result)
}

/// Builds one explicit host failure after the provider has already confirmed task success.
/// 在供应商已经确认任务成功后构建一项明确宿主失败。
///
/// # Parameters
/// 参数
/// `error` is the exact bounded result-contract or decoding diagnostic.
/// `error` 是精确且有界的结果契约或解码诊断。
///
/// # Returns
/// 返回值
/// A terminal task failure that cannot be mistaken for a transient provider observation error.
/// 返回不会被误认为瞬时供应商观察错误的任务终态失败。
fn failed_provider_async_task_completed_result(error: String) -> ProviderAsyncTaskPoll {
    ProviderAsyncTaskPoll::Failed(format!(
        "Provider task completed, but its result could not be accepted: {error}"
    ))
}

/// Appends one non-empty provider result with its separator only after the final size is admitted.
/// 先验证最终大小，再追加一个非空供应商结果及其分隔符。
fn append_bounded_provider_result(
    output: &mut String,
    text: &str,
    separator: &str,
) -> Result<(), String> {
    let separator = if output.is_empty() { "" } else { separator };
    let next_size = output
        .len()
        .checked_add(separator.len())
        .and_then(|size| size.checked_add(text.len()));
    if next_size.is_none_or(|size| size > PROVIDER_TOOL_RESPONSE_BYTE_LIMIT) {
        return Err(String::from(
            "Provider task batch follow-up text exceeds the 8 MiB output limit.",
        ));
    }
    output.push_str(separator);
    output.push_str(text);
    Ok(())
}

/// Fetches one bounded public JSON document per terminal item in submitted identity order.
/// 按已提交身份顺序为每个终态项目拉取一份有界公共 JSON 文档。
///
/// # Parameters
/// 参数
/// `http` owns transport policy; `entry` and `target` freeze the initiating origin; `terminal` is
/// the successful poll payload; `recovery` contains ordered identity fingerprints; `follow_up`
/// declares all item and text pointers.
/// `http` 持有传输策略；`entry` 与 `target` 冻结启动 Origin；`terminal` 是成功轮询载荷；
/// `recovery` 包含有序身份指纹；`follow_up` 声明全部项目与文本 Pointer。
///
/// # Returns
/// 返回值
/// Joined text no larger than the shared 8 MiB metadata ceiling.
/// 返回不超过共享 8 MiB 元数据上限的拼接文本。
async fn execute_generic_json_async_batch_follow_up(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    terminal: &Value,
    recovery: &ProviderAsyncTaskRecoveryContext,
    follow_up: &crate::provider_manifest::GenericJsonAsyncBatchFollowUpDeclaration,
) -> Result<String, String> {
    // exactOrigin is derived from the already-validated frozen target, never from response data.
    // exactOrigin 从已验证冻结目标派生，绝不来自响应数据。
    let exact_origin = reqwest::Url::parse(provider_tool_url(entry, target).as_str())
        .map_err(|_| String::from("Provider task batch follow-up origin is invalid."))?;
    let fingerprints = match recovery {
        ProviderAsyncTaskRecoveryContext::OrderedSha256 { fingerprints } => fingerprints,
        ProviderAsyncTaskRecoveryContext::None => {
            return Err(String::from(
                "Provider task batch follow-up recovery fingerprints are missing.",
            ));
        }
    };
    let items = terminal
        .pointer(follow_up.items_pointer.as_str())
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!(
                "Provider task batch follow-up is missing an item array at `{}`.",
                follow_up.items_pointer
            )
        })?;
    if items.len() != fingerprints.len()
        || items.is_empty()
        || items.len() > usize::from(follow_up.max_items)
    {
        return Err(String::from(
            "Provider task batch follow-up item count differs from the submitted identity count or configured limit.",
        ));
    }
    let mut items_by_fingerprint = BTreeMap::new();
    for item in items {
        let identity = item
            .pointer(follow_up.item_identity_pointer.as_str())
            .and_then(Value::as_str)
            .filter(|identity| !identity.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "Provider task batch item is missing an identity at `{}`.",
                    follow_up.item_identity_pointer
                )
            })?;
        let fingerprint = hex::encode(Sha256::digest(identity.as_bytes()));
        if items_by_fingerprint.insert(fingerprint, item).is_some() {
            return Err(String::from(
                "Provider task batch follow-up returned duplicate identities.",
            ));
        }
    }
    let mut output = String::new();
    for fingerprint in fingerprints {
        let item = items_by_fingerprint.remove(fingerprint).ok_or_else(|| {
            String::from("Provider task batch follow-up returned an unknown or missing identity.")
        })?;
        let status = item
            .pointer(follow_up.item_status_pointer.as_str())
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "Provider task batch item is missing a string status at `{}`.",
                    follow_up.item_status_pointer
                )
            })?;
        if !follow_up
            .item_success_statuses
            .iter()
            .any(|accepted| accepted == status)
        {
            return Err(format!(
                "Provider task batch item returned unaccepted status `{status}`."
            ));
        }
        let url_value = item
            .pointer(follow_up.item_url_pointer.as_str())
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "Provider task batch item is missing a follow-up URL at `{}`.",
                    follow_up.item_url_pointer
                )
            })?;
        // The frozen follow-up policy prevents provider response data from selecting a new origin.
        // 冻结的后续请求策略阻止供应商响应数据选择新的 Origin。
        let download_policy = ProviderToolResourceDownloadPolicy {
            encoding: follow_up.url_encoding,
            host_suffixes: follow_up.url_host_suffixes.clone(),
        };
        let url = download_policy.validate_url_with_exact_origin(
            url_value,
            "Provider task batch follow-up URL",
            &exact_origin,
        )?;
        let response = http.get(url).send().await.map_err(|error| {
            format_provider_tool_transport_error("Provider task batch follow-up", error)
        })?;
        let status_code = response.status();
        let body =
            read_bounded_provider_response(response, "Provider task batch follow-up").await?;
        if !status_code.is_success() {
            return Err(format_http_error(
                "Provider task batch follow-up",
                status_code.as_u16(),
                body.as_str(),
            ));
        }
        let payload = parse_bounded_provider_json(body.as_str(), "Provider task batch follow-up")?;
        let response_items = payload
            .pointer(follow_up.response_items_pointer.as_str())
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty() && items.len() <= 256)
            .ok_or_else(|| {
                format!(
                    "Provider task batch follow-up is missing one through 256 response items at `{}`.",
                    follow_up.response_items_pointer
                )
            })?;
        let texts = response_items
            .iter()
            .map(|response_item| {
                response_item
                    .pointer(follow_up.response_text_pointer.as_str())
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        format!(
                            "Provider task batch follow-up response item is missing text at `{}`.",
                            follow_up.response_text_pointer
                        )
                    })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut fetched = String::new();
        for text in texts {
            append_bounded_provider_result(
                &mut fetched,
                &text,
                follow_up.response_item_separator.as_str(),
            )?;
        }
        append_bounded_provider_result(&mut output, &fetched, follow_up.result_separator.as_str())?;
    }
    if !items_by_fingerprint.is_empty() {
        return Err(String::from(
            "Provider task batch follow-up returned identities outside the recovery order.",
        ));
    }
    if output.is_empty() {
        return Err(String::from(
            "Provider task batch follow-up produced no text.",
        ));
    }
    Ok(output)
}

/// Cancels one adapter-free JSON task only through an explicitly declared endpoint and method.
/// 仅通过显式声明的端点与方法取消一个无具名适配器 JSON 任务。
async fn cancel_generic_json_async_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    provider_task_id: &str,
    secret: &CredentialExecutionSecret,
    lifecycle: &crate::provider_manifest::GenericJsonAsyncLifecycleDeclaration,
) -> Result<(), String> {
    let endpoint_path = lifecycle.cancel_endpoint_path.as_deref().ok_or_else(|| {
        format!(
            "Generic asynchronous provider tool `{}` does not declare cancellation.",
            target.target_id
        )
    })?;
    let method = lifecycle.cancel_method.ok_or_else(|| {
        format!(
            "Generic asynchronous provider tool `{}` has no cancellation method.",
            target.target_id
        )
    })?;
    let url = generic_json_async_url(
        entry,
        target,
        endpoint_path,
        lifecycle.task_id_query_parameter.as_deref(),
        lifecycle.task_id_in_path,
        lifecycle.cancel_task_id_path_suffix.as_deref(),
        provider_task_id,
    )?;
    let text = execute_generic_json_async_request(
        http,
        target,
        secret,
        method,
        url,
        "Provider task cancellation",
    )
    .await?;
    if let Some(requirements) = lifecycle.cancel_requirements.as_ref() {
        let response = parse_bounded_provider_json(text.as_str(), "Provider task cancellation")?;
        if response.get("error").is_some() {
            return Err(format_structured_provider_error(
                "Provider task cancellation",
                None,
                &response,
            ));
        }
        validate_generic_json_response_requirements(
            &response,
            requirements,
            "Provider task cancellation",
        )?;
    }
    Ok(())
}

/// Builds one exact task endpoint URL and places the task id in its declared location.
/// 构建一个精确任务端点 URL，并把任务标识放入已声明位置。
fn generic_json_async_url(
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    endpoint_path: &str,
    task_id_query_parameter: Option<&str>,
    task_id_in_path: bool,
    task_id_path_suffix: Option<&str>,
    provider_task_id: &str,
) -> Result<reqwest::Url, String> {
    let base_url = target
        .execution
        .endpoint_base_url
        .as_deref()
        .unwrap_or(entry.base_url.as_str())
        .trim_end_matches('/');
    let mut url = reqwest::Url::parse(format!("{base_url}{endpoint_path}").as_str())
        .map_err(|_| String::from("Generic provider task endpoint URL is invalid."))?;
    if task_id_in_path {
        let mut segments = url.path_segments_mut().map_err(|_| {
            String::from("Generic provider task endpoint cannot accept path segments.")
        })?;
        segments.pop_if_empty().push(provider_task_id);
        if let Some(suffix) = task_id_path_suffix {
            segments.push(suffix);
        }
    } else if let Some(parameter) = task_id_query_parameter {
        if task_id_path_suffix.is_some() {
            return Err(String::from(
                "Generic provider task path suffix requires a path task identifier.",
            ));
        }
        url.query_pairs_mut()
            .append_pair(parameter, provider_task_id);
    } else {
        return Err(String::from(
            "Generic provider task identifier location is not declared.",
        ));
    }
    Ok(url)
}

/// Sends one bounded generic JSON task observation or cancellation request.
/// 发送一次有界通用 JSON 任务观测或取消请求。
async fn execute_generic_json_async_request(
    http: &reqwest::Client,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    method: crate::provider_manifest::GenericJsonAsyncRequestMethod,
    url: reqwest::Url,
    operation: &str,
) -> Result<String, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_declared_tool_headers(&mut headers, target)?;
    insert_authentication_header(&mut headers, target.execution.authentication, secret)?;
    let request = match method {
        crate::provider_manifest::GenericJsonAsyncRequestMethod::Get => http.get(url.clone()),
        crate::provider_manifest::GenericJsonAsyncRequestMethod::Post => http.post(url.clone()),
        crate::provider_manifest::GenericJsonAsyncRequestMethod::Delete => http.delete(url.clone()),
    };
    let response = request
        .headers(headers)
        .send()
        .await
        .map_err(|error| format_provider_tool_transport_error(operation, error))?;
    let status = response.status();
    let text = read_bounded_provider_response(response, operation).await?;
    if !status.is_success() {
        return Err(format_http_error(operation, status.as_u16(), text.as_str()));
    }
    Ok(text)
}

/// Downloads one bounded binary resource from a declared asynchronous success follow-up.
/// 从已声明的异步成功后续请求下载一项有界二进制资源。
///
/// # Parameters
/// 参数
/// `http`, `target`, and `secret` are the frozen execution authority; `method` and `url` are
/// compiled from the bundled manifest; `provider_task_id` scopes the stable output identity.
/// `http`、`target` 与 `secret` 是冻结的执行权限；`method` 与 `url` 由内置清单编译；
/// `provider_task_id` 限定稳定输出标识。
///
/// # Returns
/// 返回值
/// One normalized private resource or a controlled contract, transport, status, MIME, or size
/// failure.
/// 返回一项归一化私有资源，或受控的契约、传输、状态、MIME 或大小失败。
async fn execute_generic_json_async_binary_follow_up(
    http: &reqwest::Client,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    method: crate::provider_manifest::GenericJsonAsyncRequestMethod,
    url: reqwest::Url,
    provider_task_id: &str,
) -> Result<ProviderToolExecutionResult, String> {
    let [decoder] = target.contract.output.resource_decoders.as_slice() else {
        return Err(String::from(
            "Binary provider task follow-up requires exactly one resource decoder.",
        ));
    };
    if target.contract.output.kind != ProviderToolOutputKind::Resources
        || decoder.encoding != crate::ProviderToolResourceEncoding::BinaryBody
        || !decoder.download_host_suffixes.is_empty()
        || decoder.mime_type_request_path.is_some()
        || !decoder.mime_types_by_request_value.is_empty()
        || decoder.optional
    {
        return Err(String::from(
            "Binary provider task follow-up requires one mandatory closed binary-body resource decoder.",
        ));
    }
    let mime_type = decoder
        .mime_type
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            String::from("Binary provider task follow-up requires one exact resource MIME type.")
        })?;
    let max_bytes = decoder
        .max_bytes
        .filter(|value| (1..=GENERIC_RESOURCE_BYTE_LIMIT).contains(value))
        .ok_or_else(|| {
            String::from("Binary provider task follow-up has no valid resource byte limit.")
        })?;
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_str(mime_type)
            .map_err(|_| String::from("Binary provider task follow-up MIME type is invalid."))?,
    );
    insert_declared_tool_headers(&mut headers, target)?;
    insert_authentication_header(&mut headers, target.execution.authentication, secret)?;
    let request = match method {
        crate::provider_manifest::GenericJsonAsyncRequestMethod::Get => http.get(url.clone()),
        crate::provider_manifest::GenericJsonAsyncRequestMethod::Post => http.post(url.clone()),
        crate::provider_manifest::GenericJsonAsyncRequestMethod::Delete => http.delete(url.clone()),
    };
    let response = request.headers(headers).send().await.map_err(|error| {
        format_provider_tool_transport_error("Provider task binary follow-up", error)
    })?;
    let status = response.status();
    let response_mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .map(str::to_ascii_lowercase);
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes)
    {
        return Err(format!(
            "Provider task binary follow-up exceeds the declared {max_bytes} byte limit."
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            format_provider_tool_transport_error("Provider task binary follow-up", error)
        })?;
        if (bytes.len() as u64).saturating_add(chunk.len() as u64) > max_bytes {
            return Err(format!(
                "Provider task binary follow-up exceeds the declared {max_bytes} byte limit."
            ));
        }
        bytes.extend_from_slice(chunk.as_ref());
    }
    if !status.is_success() {
        let diagnostic = String::from_utf8_lossy(&bytes);
        return Err(format_http_error(
            "Provider task binary follow-up",
            status.as_u16(),
            diagnostic.as_ref(),
        ));
    }
    if response_mime.as_deref() != Some(mime_type) {
        return Err(format!(
            "Provider task binary follow-up returned MIME `{}` instead of declared `{mime_type}`.",
            response_mime.as_deref().unwrap_or("missing")
        ));
    }
    if bytes.is_empty() {
        return Err(String::from(
            "Provider task binary follow-up returned an empty resource body.",
        ));
    }
    let output_prefix = decoder.output_id_prefix.as_deref().unwrap_or("resource");
    Ok(ProviderToolExecutionResult {
        upstream_response_id: Some(provider_task_id.to_string()),
        output: ProviderToolNormalizedOutput::Resources(vec![ProviderToolGeneratedResource {
            output_id: format!("{output_prefix}-1"),
            kind: decoder.resource_kind.clone(),
            mime_type: mime_type.to_string(),
            bytes: Some(bytes),
            download_url: None,
            download_policy: None,
        }]),
    })
}

/// Stable host identity sent during every generic MCP initialization handshake.
/// 每次通用 MCP 初始化握手发送的稳定宿主身份。
const GENERIC_MCP_CLIENT_NAME: &str = "VulcanCode";
/// Stable host implementation revision sent during every generic MCP initialization handshake.
/// 每次通用 MCP 初始化握手发送的稳定宿主实现修订号。
const GENERIC_MCP_CLIENT_VERSION: &str = "1";

/// Executes one closed Streamable HTTP MCP initialization, notification, and tool-call sequence.
/// 执行一条封闭的 Streamable HTTP MCP 初始化、通知与工具调用序列。
///
/// # Parameters
/// 参数
/// `http`, `entry`, `target`, and `secret` are one frozen provider route; `call_arguments` is the
/// manifest-projected JSON object passed to the exact upstream tool name.
/// `http`、`entry`、`target` 与 `secret` 构成一条冻结供应商路由；`call_arguments` 是传给精确
/// 上游工具名称的 Manifest 投影 JSON 对象。
///
/// # Returns
/// 返回值
/// A provider-neutral JSON document containing the projected call arguments and bounded aggregated
/// items decoded from MCP text content blocks.
/// 返回一份供应商中立 JSON 文档，其中包含投影后的调用参数与从 MCP 文本内容块解码出的有界
/// 聚合条目。
async fn execute_generic_mcp_http_call(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    call_arguments: &Value,
) -> Result<String, String> {
    let contract = target.execution.mcp_http_call.as_ref().ok_or_else(|| {
        format!(
            "Provider tool `{}` omitted its MCP HTTP call contract.",
            target.target_id
        )
    })?;
    if !call_arguments.is_object() {
        return Err(format!(
            "Provider tool `{}` MCP call arguments must project to an object.",
            target.target_id
        ));
    }
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": contract.protocol_version,
            "capabilities": {},
            "clientInfo": {
                "name": GENERIC_MCP_CLIENT_NAME,
                "version": GENERIC_MCP_CLIENT_VERSION
            }
        }
    });
    let (initialize_body, initialize_content_type, mut session_id) =
        execute_generic_mcp_http_request(
            http,
            entry,
            target,
            secret,
            &initialize,
            None,
            "MCP initialize",
        )
        .await?;
    decode_generic_mcp_json_rpc_result(
        initialize_body.as_str(),
        initialize_content_type.as_str(),
        1,
    )?;

    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let (_, _, refreshed_session_id) = execute_generic_mcp_http_request(
        http,
        entry,
        target,
        secret,
        &initialized,
        session_id.as_deref(),
        "MCP initialized notification",
    )
    .await?;
    if refreshed_session_id.is_some() {
        session_id = refreshed_session_id;
    }

    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": target.upstream_target_id,
            "arguments": call_arguments
        }
    });
    let (call_body, call_content_type, _) = execute_generic_mcp_http_request(
        http,
        entry,
        target,
        secret,
        &call,
        session_id.as_deref(),
        "MCP tool call",
    )
    .await?;
    let result =
        decode_generic_mcp_json_rpc_result(call_body.as_str(), call_content_type.as_str(), 2)?;
    aggregate_generic_mcp_text_json(contract, call_arguments, &result)
}

/// Sends one authenticated MCP HTTP request while preserving the standard session header.
/// 发送一次已鉴权 MCP HTTP 请求并保留标准会话请求头。
///
/// # Parameters
/// 参数
/// The route arguments are frozen for one call; `body` is one exact JSON-RPC envelope,
/// `session_id` is the previously returned standard session token, and `operation` scopes errors.
/// 路由参数在单次调用内冻结；`body` 是一份精确 JSON-RPC 信封，`session_id` 是此前返回的标准
/// 会话令牌，`operation` 限定错误范围。
///
/// # Returns
/// 返回值
/// The bounded response body, normalized content type, and optional refreshed session id.
/// 返回有界响应正文、规范化内容类型与可选的刷新后会话标识。
async fn execute_generic_mcp_http_request(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    body: &Value,
    session_id: Option<&str>,
    operation: &str,
) -> Result<(String, String, Option<String>), String> {
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    insert_declared_tool_headers(&mut headers, target)?;
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    if let Some(session_id) = session_id {
        if session_id.len() > 1024 {
            return Err(String::from(
                "Provider MCP session identifier exceeds 1024 bytes.",
            ));
        }
        let value = HeaderValue::from_str(session_id)
            .map_err(|_| String::from("Provider MCP session identifier is invalid."))?;
        headers.insert(HeaderName::from_static("mcp-session-id"), value);
    }
    insert_authentication_header(&mut headers, target.execution.authentication, secret)?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|error| {
            format_provider_tool_transport_error(format!("Provider {operation}").as_str(), error)
        })?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json")
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let refreshed_session_id = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 1024)
        .map(str::to_string);
    let response_text = read_bounded_provider_response(response, operation).await?;
    if !status.is_success() {
        return Err(format_http_error(
            "Provider MCP",
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    Ok((response_text, content_type, refreshed_session_id))
}

/// Decodes one correlated JSON-RPC result from a JSON or bounded SSE response body.
/// 从 JSON 或有界 SSE 响应正文中解码一份已关联 JSON-RPC 结果。
///
/// # Parameters
/// 参数
/// `body` is the bounded response text, `content_type` selects JSON versus SSE framing, and
/// `expected_id` is the exact request correlation id.
/// `body` 是有界响应文本，`content_type` 选择 JSON 或 SSE 分帧，`expected_id` 是精确请求关联标识。
///
/// # Returns
/// 返回值
/// The correlated `result` value, or a controlled protocol/correlation/error diagnostic.
/// 返回已关联的 `result` 值，或一条受控协议、关联或错误诊断。
fn decode_generic_mcp_json_rpc_result(
    body: &str,
    content_type: &str,
    expected_id: u64,
) -> Result<Value, String> {
    let envelopes = if content_type == "text/event-stream"
        || body.lines().any(|line| line.starts_with("data:"))
    {
        decode_generic_mcp_sse_envelopes(body)?
    } else {
        vec![parse_bounded_provider_json(body, "Provider MCP response")?]
    };
    let mut correlated = envelopes
        .into_iter()
        .filter(|envelope| envelope.get("id").and_then(Value::as_u64) == Some(expected_id));
    let envelope = correlated
        .next()
        .ok_or_else(|| format!("Provider MCP response omitted correlation id {expected_id}."))?;
    if correlated.next().is_some() {
        return Err(format!(
            "Provider MCP response duplicated correlation id {expected_id}."
        ));
    }
    if envelope.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(String::from(
            "Provider MCP response uses an unsupported JSON-RPC version.",
        ));
    }
    if let Some(error) = envelope.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        return Err(format!("Provider MCP returned JSON-RPC error code {code}."));
    }
    envelope
        .get("result")
        .cloned()
        .ok_or_else(|| String::from("Provider MCP response omitted its result."))
}

/// Decodes at most 64 JSON-RPC envelopes from standard SSE `data` events.
/// 从标准 SSE `data` 事件中解码至多 64 份 JSON-RPC 信封。
///
/// # Parameters
/// 参数
/// `body` is the already byte-bounded UTF-8 response text.
/// `body` 是已经过字节上限约束的 UTF-8 响应文本。
///
/// # Returns
/// 返回值
/// Ordered JSON event payloads; malformed framing, invalid JSON, or excess events fail closed.
/// 返回有序 JSON 事件载荷；畸形分帧、非法 JSON 或事件超限都会封闭失败。
fn decode_generic_mcp_sse_envelopes(body: &str) -> Result<Vec<Value>, String> {
    let mut envelopes = Vec::new();
    let mut data_lines = Vec::new();
    for line in body.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if data_lines.is_empty() {
                continue;
            }
            if envelopes.len() >= 64 {
                return Err(String::from(
                    "Provider MCP SSE response exceeds the 64 event limit.",
                ));
            }
            let data = data_lines.join("\n");
            data_lines.clear();
            if data.trim() == "[DONE]" {
                continue;
            }
            envelopes.push(parse_bounded_provider_json(
                data.as_str(),
                "Provider MCP SSE data event",
            )?);
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.strip_prefix(' ').unwrap_or(data).to_string());
        } else if !line.starts_with(':')
            && !line.starts_with("event:")
            && !line.starts_with("id:")
            && !line.starts_with("retry:")
        {
            return Err(String::from(
                "Provider MCP SSE response contains an unregistered field.",
            ));
        }
    }
    if envelopes.is_empty() {
        return Err(String::from(
            "Provider MCP SSE response contains no JSON data event.",
        ));
    }
    Ok(envelopes)
}

/// Aggregates arrays embedded as JSON in standard MCP text content blocks.
/// 聚合标准 MCP 文本内容块中以 JSON 嵌入的数组。
///
/// # Parameters
/// 参数
/// `contract` owns all pointer and count bounds, `call_arguments` is the exact projected request,
/// and `result` is the correlated MCP tools/call result.
/// `contract` 持有全部 Pointer 与数量边界，`call_arguments` 是精确投影请求，`result` 是已关联
/// MCP tools/call 结果。
///
/// # Returns
/// 返回值
/// A serialized canonical object with `call_arguments` and ordered `items` fields.
/// 返回包含 `call_arguments` 与有序 `items` 字段的序列化规范对象。
fn aggregate_generic_mcp_text_json(
    contract: &crate::ProviderToolMcpHttpCallContract,
    call_arguments: &Value,
    result: &Value,
) -> Result<String, String> {
    if result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(String::from("Provider MCP tool call reported failure."));
    }
    let content = result
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("Provider MCP tool result omitted its content array."))?;
    if content.len() > usize::from(contract.max_content_blocks) {
        return Err(format!(
            "Provider MCP tool result exceeds the {} content block limit.",
            contract.max_content_blocks
        ));
    }
    let mut items = Vec::new();
    for block in content {
        if block.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let Some(text) = block
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
        else {
            continue;
        };
        let embedded = parse_bounded_provider_json(text, "Provider MCP text content")?;
        let selected = embedded
            .pointer(contract.content_json_array_pointer.as_str())
            .and_then(Value::as_array)
            .ok_or_else(|| {
                format!(
                    "Provider MCP text content is missing array `{}`.",
                    contract.content_json_array_pointer
                )
            })?;
        if items.len().saturating_add(selected.len()) > usize::from(contract.max_items) {
            return Err(format!(
                "Provider MCP embedded output exceeds the {} item limit.",
                contract.max_items
            ));
        }
        items.extend(selected.iter().cloned());
    }
    if items.is_empty() {
        return Err(String::from(
            "Provider MCP tool result returned no structured items.",
        ));
    }
    serde_json::to_string(&serde_json::json!({
        "call_arguments": call_arguments,
        "items": items
    }))
    .map_err(|error| format!("Provider MCP normalized response could not serialize: {error}"))
}

/// Executes one synchronous JSON tool request through the generic transport primitive.
/// 通过通用传输原语执行一次同步 JSON 工具请求。
///
/// # Parameters
/// 参数
/// All values are immutable snapshots resolved before dispatch; `operation` labels the call
/// for bounded error reporting.
/// 所有值均为分派前解析的不可变快照；`operation` 为有界错误报告标记调用。
///
/// # Returns
/// 返回值
/// The bounded upstream response text, or a controlled transport error.
/// 返回有界上游响应文本，或受控传输错误。
async fn execute_generic_json_sync_request<T: Serialize>(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    body: &T,
    operation: &str,
) -> Result<String, String> {
    execute_generic_json_sync_request_with_headers(
        http,
        entry,
        target,
        secret,
        body,
        operation,
        &std::collections::BTreeMap::new(),
    )
    .await
}

/// Executes one generic JSON request with additional phase-scoped fixed headers.
/// 使用额外阶段限定固定请求头执行一项通用 JSON 请求。
///
/// # Parameters
/// 参数
/// `additional_headers` contains only validated headers for the current lifecycle phase.
/// `additional_headers` 仅包含当前生命周期阶段已校验的请求头。
///
/// # Returns
/// 返回值
/// The bounded upstream response text, or a controlled transport error.
/// 返回有界上游响应文本，或受控传输错误。
async fn execute_generic_json_sync_request_with_headers<T: Serialize>(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    body: &T,
    operation: &str,
    additional_headers: &std::collections::BTreeMap<String, String>,
) -> Result<String, String> {
    let url = provider_tool_url(entry, target);
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    insert_declared_tool_headers(&mut headers, target)?;
    insert_fixed_tool_headers(&mut headers, additional_headers)?;
    insert_authentication_header(&mut headers, target.execution.authentication, secret)?;
    let response = http
        .post(url.as_str())
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|error| format_provider_tool_transport_error(operation, error))?;
    let status = response.status();
    let response_text = read_bounded_provider_response(response, operation).await?;
    if !status.is_success() {
        return Err(format_http_error(
            operation,
            status.as_u16(),
            response_text.as_str(),
        ));
    }
    Ok(response_text)
}

/// Decodes one generic JSON-sync response through the manifest-owned output contract.
/// 通过 Manifest 持有的输出契约解码一次通用 JSON 同步响应。
///
/// The contract's `source_path` names the provider response array and `normalized_result_path`
/// names the stable model-facing result field, so a new provider operation with the same closed
/// output kind becomes executable without a named adapter (plan §R9.6).
/// 契约的 `source_path` 命名供应商响应数组，`normalized_result_path` 命名面向模型的稳定
/// 结果字段，使具有相同封闭输出类别的新供应商操作无需命名适配器即可执行（方案 §R9.6）。
/// Decodes one generic response without request-owned normalization context.
/// 在没有请求自有归一化上下文的情况下解码一份通用响应。
fn decode_generic_json_sync_output(
    contract: &crate::ProviderToolOutputContract,
    body: &str,
) -> Result<ProviderToolExecutionResult, String> {
    decode_generic_json_sync_output_with_request(contract, body, None)
}

/// Decodes one generic response with optional canonical request arguments for output limits.
/// 使用可选规范请求参数解码一份通用响应，以应用输出数量限制。
fn decode_generic_json_sync_output_with_request(
    contract: &crate::ProviderToolOutputContract,
    body: &str,
    request_arguments: Option<&Value>,
) -> Result<ProviderToolExecutionResult, String> {
    let response = parse_bounded_provider_json(body, "Provider tool response")?;
    if response.get("error").is_some() {
        return Err(format_structured_provider_error(
            "Provider tool",
            None,
            &response,
        ));
    }
    if contract.kind == crate::ProviderToolOutputKind::Resources {
        if contract.resource_decoders.is_empty() {
            return Err(String::from(
                "Provider tool resource output has no declared resource decoder.",
            ));
        }
        let resources = decode_declared_generic_resources(contract, &response, request_arguments)?;
        return Ok(ProviderToolExecutionResult {
            upstream_response_id: generic_provider_response_id(&response),
            output: ProviderToolNormalizedOutput::Resources(resources),
        });
    }
    let source = response
        .pointer(contract.source_path.as_str())
        .ok_or_else(|| {
            format!(
                "Provider tool response is missing source path `{}`.",
                contract.source_path
            )
        })?;
    match contract.kind {
        crate::ProviderToolOutputKind::Text => {
            let text = source
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "Provider tool text source `{}` is not a non-empty string.",
                        contract.source_path
                    )
                })?;
            Ok(ProviderToolExecutionResult {
                upstream_response_id: generic_provider_response_id(&response),
                output: ProviderToolNormalizedOutput::Text(text.to_string()),
            })
        }
        crate::ProviderToolOutputKind::Resources => Err(String::from(
            "Provider tool resource output bypassed its declared resource decoders.",
        )),
        crate::ProviderToolOutputKind::Search => {
            let maximum_results = request_arguments
                .and_then(|arguments| arguments.pointer("/max_results"))
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok());
            let source_items = source.as_array().ok_or_else(|| {
                format!(
                    "Provider tool search source `{}` is not an array.",
                    contract.source_path
                )
            })?;
            if maximum_results.is_none()
                && source_items.len() > GENERIC_STRUCTURED_RESULT_COUNT_LIMIT
            {
                return Err(format!(
                    "Provider tool search response exceeds the {GENERIC_STRUCTURED_RESULT_COUNT_LIMIT}-item host limit."
                ));
            }
            let result_limit = maximum_results
                .unwrap_or(GENERIC_STRUCTURED_RESULT_COUNT_LIMIT)
                .min(GENERIC_STRUCTURED_RESULT_COUNT_LIMIT);
            let results = source_items
                .iter()
                .take(result_limit)
                .enumerate()
                .map(|(index, item)| {
                    let url = item
                        .get("url")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            format!("Provider tool search result {index} has no non-empty url.")
                        })?;
                    let parsed_url = validated_https_url(
                        url,
                        format!("Provider tool search result {index}").as_str(),
                    )?;
                    Ok(ProviderToolSearchResult {
                        rank: index + 1,
                        title: item
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        url: parsed_url.to_string(),
                        source_domain: parsed_url
                            .host_str()
                            .expect("validated HTTPS URL always has a host")
                            .to_ascii_lowercase(),
                        snippet: item
                            .get("content")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let query = response
                .get("query")
                .and_then(Value::as_str)
                .or_else(|| {
                    request_arguments
                        .and_then(|arguments| arguments.pointer("/query"))
                        .and_then(Value::as_str)
                })
                .unwrap_or_default()
                .to_string();
            Ok(ProviderToolExecutionResult {
                upstream_response_id: response
                    .get("request_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .filter(|value| !value.trim().is_empty()),
                output: ProviderToolNormalizedOutput::Search(ProviderToolSearchOutput {
                    query,
                    results,
                }),
            })
        }
        crate::ProviderToolOutputKind::Extract => {
            let source_items = source.as_array().ok_or_else(|| {
                format!(
                    "Provider tool extract source `{}` is not an array.",
                    contract.source_path
                )
            })?;
            if source_items.len() > GENERIC_STRUCTURED_RESULT_COUNT_LIMIT {
                return Err(format!(
                    "Provider tool extract response exceeds the {GENERIC_STRUCTURED_RESULT_COUNT_LIMIT}-item host limit."
                ));
            }
            let results = source_items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let url = item
                        .get("url")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            format!("Provider tool extract result {index} has no non-empty url.")
                        })?;
                    let parsed_url = validated_https_url(
                        url,
                        format!("Provider tool extract result {index}").as_str(),
                    )?;
                    let images = match item.get("images") {
                        None | Some(Value::Null) => Vec::new(),
                        Some(Value::Array(values)) => {
                            if values.len() > GENERIC_STRUCTURED_RESULT_COUNT_LIMIT {
                                return Err(format!(
                                    "Provider tool extract result {index} exceeds the {GENERIC_STRUCTURED_RESULT_COUNT_LIMIT}-image host limit."
                                ));
                            }
                            values
                                .iter()
                                .enumerate()
                                .map(|(image_index, value)| {
                                    let image = value
                                        .as_str()
                                        .filter(|value| !value.trim().is_empty())
                                        .ok_or_else(|| {
                                            format!(
                                                "Provider tool extract result {index} image {image_index} is not a non-empty URL."
                                            )
                                        })?;
                                    validated_https_url(
                                        image,
                                        format!(
                                            "Provider tool extract result {index} image {image_index}"
                                        )
                                        .as_str(),
                                    )
                                    .map(|url| url.to_string())
                                })
                                .collect::<Result<Vec<_>, String>>()?
                        }
                        Some(_) => {
                            return Err(format!(
                                "Provider tool extract result {index} images value is not an array."
                            ));
                        }
                    };
                    let favicon = match item.get("favicon") {
                        None | Some(Value::Null) => None,
                        Some(Value::String(value)) if value.trim().is_empty() => None,
                        Some(Value::String(value)) => Some(
                            validated_https_url(
                                value,
                                format!("Provider tool extract result {index} favicon").as_str(),
                            )?
                            .to_string(),
                        ),
                        Some(_) => {
                            return Err(format!(
                                "Provider tool extract result {index} favicon is not a string URL."
                            ));
                        }
                    };
                    Ok(ProviderToolExtractResult {
                        url: parsed_url.to_string(),
                        content: item
                            .get("raw_content")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        images,
                        favicon,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let failures = match response.get("failed_results") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(values)) => {
                    if values.len() > GENERIC_STRUCTURED_RESULT_COUNT_LIMIT {
                        return Err(format!(
                            "Provider tool extract failures exceed the {GENERIC_STRUCTURED_RESULT_COUNT_LIMIT}-item host limit."
                        ));
                    }
                    values
                        .iter()
                        .enumerate()
                        .map(|(index, item)| {
                            let url = item
                                .get("url")
                                .and_then(Value::as_str)
                                .filter(|value| !value.trim().is_empty())
                                .ok_or_else(|| {
                                    format!(
                                        "Provider tool extract failure {index} has no non-empty url."
                                    )
                                })?;
                            let parsed_url = validated_https_url(
                                url,
                                format!("Provider tool extract failure {index}").as_str(),
                            )?;
                            let error = item
                                    .get("error")
                                    .and_then(Value::as_str)
                                    .filter(|value| !value.trim().is_empty())
                                    .ok_or_else(|| {
                                        format!(
                                            "Provider tool extract failure {index} has no non-empty error."
                                        )
                                    })?;
                            Ok(ProviderToolExtractFailure {
                                url: parsed_url.to_string(),
                                error: error.to_string(),
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?
                }
                Some(_) => {
                    return Err(String::from(
                        "Provider tool extract failed_results value is not an array.",
                    ));
                }
            };
            Ok(ProviderToolExecutionResult {
                upstream_response_id: response
                    .get("request_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .filter(|value| !value.trim().is_empty()),
                output: ProviderToolNormalizedOutput::Extract(ProviderToolExtractOutput {
                    results,
                    failures,
                }),
            })
        }
        _ => Err(format!(
            "Provider tool output kind `{:?}` is not decodable by the generic JSON-sync primitive.",
            contract.kind
        )),
    }
}

/// Reads one stable upstream response identifier from the closed generic response envelope.
/// 从封闭通用响应信封读取一个稳定上游响应标识。
///
/// # Parameters
/// 参数
/// `response` is the complete validated JSON response object.
/// `response` 是完整且已校验的 JSON 响应对象。
///
/// # Returns
/// 返回值
/// The first exact non-empty `request_id` or `id` string.
/// 返回首个精确非空的 `request_id` 或 `id` 字符串。
fn generic_provider_response_id(response: &Value) -> Option<String> {
    response
        .get("request_id")
        .or_else(|| response.get("id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

/// Decodes every ordered resource extractor declared by one manifest output contract.
/// 解码一份 Manifest 输出契约声明的每个有序资源提取器。
///
/// # Parameters
/// 参数
/// `contract` owns the closed extractors, `response` is the projected provider JSON, and
/// `request_arguments` supplies only explicitly declared MIME-selection values.
/// `contract` 持有封闭提取器，`response` 是投影后的供应商 JSON，`request_arguments`
/// 仅提供显式声明的 MIME 选择值。
///
/// # Returns
/// 返回值
/// A non-empty bounded resource list or one controlled contract/provider-data error.
/// 返回非空有界资源列表，或一条受控契约/供应商数据错误。
fn decode_declared_generic_resources(
    contract: &crate::ProviderToolOutputContract,
    response: &Value,
    request_arguments: Option<&Value>,
) -> Result<Vec<ProviderToolGeneratedResource>, String> {
    // The accumulated list preserves declaration order followed by provider array order.
    // 累积列表依次保留声明顺序与供应商数组顺序。
    let mut resources = Vec::new();
    for (decoder_index, decoder) in contract.resource_decoders.iter().enumerate() {
        // The selected source remains borrowed from the bounded response body.
        // 选中的来源继续借用自有界响应正文。
        let source = response.pointer(decoder.source_path.as_str());
        let Some(source) = source else {
            if decoder.optional {
                continue;
            }
            return Err(format!(
                "Provider tool resource decoder {decoder_index} is missing source path `{}`.",
                decoder.source_path
            ));
        };
        if source.is_null()
            || source.as_str().is_some_and(|value| value.trim().is_empty())
            || source.as_array().is_some_and(Vec::is_empty)
        {
            if decoder.optional {
                continue;
            }
            return Err(format!(
                "Provider tool resource decoder {decoder_index} source `{}` is empty.",
                decoder.source_path
            ));
        }
        // A scalar is treated as one resource while an array preserves its exact order.
        // 标量按一个资源处理，数组则保留其精确顺序。
        let values = source
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_else(|| std::slice::from_ref(source));
        if resources.len().saturating_add(values.len()) > GENERIC_RESOURCE_COUNT_LIMIT {
            return Err(format!(
                "Provider tool resources exceed the {GENERIC_RESOURCE_COUNT_LIMIT} item limit."
            ));
        }
        for (source_index, value) in values.iter().enumerate() {
            // The global index is stable across multiple extractor declarations.
            // 全局索引在多个提取器声明之间保持稳定。
            let global_index = resources.len();
            resources.push(decode_declared_generic_resource(
                contract,
                decoder,
                value,
                request_arguments,
                decoder_index,
                source_index,
                global_index,
            )?);
        }
    }
    if resources.is_empty() {
        return Err(String::from(
            "Provider tool response did not contain any declared resources.",
        ));
    }
    Ok(resources)
}

/// Decodes one scalar through one exact declarative resource extractor.
/// 通过一个精确声明式资源提取器解码一个标量。
///
/// # Parameters
/// 参数
/// Indices scope deterministic diagnostics and output identifiers; all authority comes from the
/// frozen output contract and optional canonical request values.
/// 各索引用于限定确定性诊断与输出标识；全部权威来自冻结输出契约与可选规范请求值。
///
/// # Returns
/// 返回值
/// One validated inline-byte or public-URL resource.
/// 返回一项已校验的内联字节或公共 URL 资源。
fn decode_declared_generic_resource(
    contract: &crate::ProviderToolOutputContract,
    decoder: &crate::ProviderToolResourceDecoder,
    value: &Value,
    request_arguments: Option<&Value>,
    decoder_index: usize,
    source_index: usize,
    global_index: usize,
) -> Result<ProviderToolGeneratedResource, String> {
    // Declarative resource carriers are deliberately scalar strings only.
    // 声明式资源载体刻意只允许标量字符串。
    let carrier = value
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "Provider tool resource decoder {decoder_index} item {source_index} is not a non-empty string."
            )
        })?;
    // The optional decoded bytes are populated only for inline carrier families.
    // 可选解码字节仅由内联载体族填充。
    let mut bytes = None;
    // The optional URL is populated only for reviewed public-reference carrier families.
    // 可选 URL 仅由已审核公共引用载体族填充。
    let mut download_url = None;
    // URL carriers retain the exact frozen network authority for final host-side staging.
    // URL 载体保留精确的冻结网络权限，供最终宿主侧暂存复验。
    let mut download_policy = None;
    // A Data URL owns its embedded MIME while other carriers resolve MIME from the declaration.
    // Data URL 持有其内嵌 MIME，其他载体从声明解析 MIME。
    let embedded_mime = match decoder.encoding {
        crate::ProviderToolResourceEncoding::HttpsUrl
        | crate::ProviderToolResourceEncoding::PublicHttpUrl => {
            let policy = ProviderToolResourceDownloadPolicy {
                encoding: decoder.encoding,
                host_suffixes: decoder.download_host_suffixes.clone(),
            };
            policy.validate_url(
                carrier,
                format!("Provider tool resource {global_index}").as_str(),
            )?;
            download_url = Some(carrier.to_string());
            download_policy = Some(policy);
            None
        }
        crate::ProviderToolResourceEncoding::DataUrl => {
            // The decoded pair retains both the embedded MIME authority and exact bytes.
            // 解码结果同时保留内嵌 MIME 权威与精确字节。
            let (mime_type, decoded) = decode_generic_data_url(carrier, global_index)?;
            enforce_declared_resource_byte_limit(decoder, decoded.len(), global_index)?;
            bytes = Some(decoded);
            Some(mime_type)
        }
        crate::ProviderToolResourceEncoding::Base64 => {
            // Standard Base64 is the only registered raw-Base64 alphabet.
            // 标准 Base64 是唯一已注册的原始 Base64 字母表。
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(carrier)
                .map_err(|error| {
                    format!("Provider tool resource {global_index} base64 is invalid: {error}")
                })?;
            enforce_declared_resource_byte_limit(decoder, decoded.len(), global_index)?;
            bytes = Some(decoded);
            None
        }
        crate::ProviderToolResourceEncoding::Hex => {
            // Hexadecimal decoding is provider-neutral and rejects odd or non-hex input.
            // 十六进制解码与供应商无关，并拒绝奇数长度或非十六进制输入。
            let decoded = decode_generic_hexadecimal_bytes(carrier, global_index)?;
            enforce_declared_resource_byte_limit(decoder, decoded.len(), global_index)?;
            bytes = Some(decoded);
            None
        }
        crate::ProviderToolResourceEncoding::BinaryBody => {
            return Err(String::from(
                "Binary-body resources are valid only for a declared asynchronous success follow-up.",
            ));
        }
    };
    // MIME resolution is closed to the Data URL, one literal, or one exact request mapping.
    // MIME 解析封闭为 Data URL、一个字面量或一个精确请求映射。
    let mime_type = resolve_declared_resource_mime(
        decoder,
        request_arguments,
        embedded_mime.as_deref(),
        global_index,
    )?;
    validate_generic_resource_mime(contract, mime_type.as_str(), global_index)?;
    // The prefix is validated by manifest compilation before runtime activation.
    // 前缀在运行时激活前已由 Manifest 编译校验。
    let output_prefix = decoder.output_id_prefix.as_deref().unwrap_or("resource");
    Ok(ProviderToolGeneratedResource {
        output_id: format!("{output_prefix}-{}", source_index + 1),
        kind: decoder.resource_kind.clone(),
        mime_type,
        bytes,
        download_url,
        download_policy,
    })
}

/// Decodes one strict Base64 Data URL into its MIME type and bytes.
/// 把一个严格 Base64 Data URL 解码为 MIME 类型与字节。
fn decode_generic_data_url(value: &str, index: usize) -> Result<(String, Vec<u8>), String> {
    // The prefix must be exact so arbitrary URI schemes cannot enter the byte decoder.
    // 前缀必须精确，避免任意 URI Scheme 进入字节解码器。
    let payload = value
        .strip_prefix("data:")
        .ok_or_else(|| format!("Provider tool resource {index} is not a data URL."))?;
    // The first comma separates metadata from the bounded encoded payload.
    // 第一个逗号分隔元数据与有界编码载荷。
    let (metadata, encoded) = payload
        .split_once(',')
        .ok_or_else(|| format!("Provider tool resource {index} carries a malformed data URL."))?;
    // The first metadata item is the exact MIME and one later item must register Base64.
    // 第一项元数据是精确 MIME，后续至少一项必须注册 Base64。
    let mut metadata_parts = metadata.split(';');
    let mime_type = metadata_parts.next().unwrap_or_default();
    if mime_type.trim().is_empty() || !metadata_parts.any(|part| part == "base64") {
        return Err(format!(
            "Provider tool resource {index} data URL must declare a MIME type and base64 encoding."
        ));
    }
    // Standard Base64 produces the exact inline artifact bytes.
    // 标准 Base64 生成精确的内联产物字节。
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("Provider tool resource {index} base64 is invalid: {error}"))?;
    Ok((mime_type.to_string(), bytes))
}

/// Decodes one even-length hexadecimal resource string.
/// 解码一个偶数长度的十六进制资源字符串。
fn decode_generic_hexadecimal_bytes(value: &str, index: usize) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err(format!(
            "Provider tool resource {index} hexadecimal payload has odd length."
        ));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            // Each pair is ASCII by construction only when it is a valid hexadecimal byte.
            // 每个二元组仅在构成有效十六进制字节时才是合法 ASCII。
            let pair = std::str::from_utf8(pair).map_err(|_| {
                format!("Provider tool resource {index} hexadecimal payload is invalid.")
            })?;
            u8::from_str_radix(pair, 16).map_err(|_| {
                format!("Provider tool resource {index} hexadecimal payload is invalid.")
            })
        })
        .collect()
}

/// Enforces one manifest-declared byte ceiling within the engine-wide absolute limit.
/// 在引擎绝对上限内执行一个 Manifest 声明的字节上限。
fn enforce_declared_resource_byte_limit(
    decoder: &crate::ProviderToolResourceDecoder,
    decoded_length: usize,
    index: usize,
) -> Result<(), String> {
    // Raw byte carriers must always state a positive bounded ceiling.
    // 原始字节载体必须始终声明正数有界上限。
    let maximum = decoder.max_bytes.ok_or_else(|| {
        format!("Provider tool resource decoder for item {index} has no byte limit.")
    })?;
    if maximum == 0 || maximum > GENERIC_RESOURCE_BYTE_LIMIT {
        return Err(format!(
            "Provider tool resource decoder for item {index} has an invalid byte limit."
        ));
    }
    if decoded_length == 0 || decoded_length as u64 > maximum {
        return Err(format!(
            "Provider tool resource {index} must be non-empty and no larger than {maximum} bytes."
        ));
    }
    Ok(())
}

/// Resolves one exact resource MIME authority from a closed declaration.
/// 从一份封闭声明解析一个精确资源 MIME 权威。
fn resolve_declared_resource_mime(
    decoder: &crate::ProviderToolResourceDecoder,
    request_arguments: Option<&Value>,
    embedded_mime: Option<&str>,
    index: usize,
) -> Result<String, String> {
    if let Some(embedded_mime) = embedded_mime {
        if decoder.mime_type.is_some()
            || decoder.mime_type_request_path.is_some()
            || !decoder.mime_types_by_request_value.is_empty()
        {
            return Err(format!(
                "Provider tool resource decoder {index} has ambiguous MIME authority."
            ));
        }
        return Ok(embedded_mime.to_string());
    }
    if let Some(mime_type) = decoder.mime_type.as_deref() {
        if decoder.mime_type_request_path.is_some()
            || !decoder.mime_types_by_request_value.is_empty()
        {
            return Err(format!(
                "Provider tool resource decoder {index} has ambiguous MIME authority."
            ));
        }
        return Ok(mime_type.to_string());
    }
    // Request-selected MIME requires both one exact pointer and a non-empty literal table.
    // 请求选择 MIME 必须同时声明一个精确指针与一份非空字面量表。
    let request_path = decoder
        .mime_type_request_path
        .as_deref()
        .ok_or_else(|| format!("Provider tool resource decoder {index} has no MIME authority."))?;
    if decoder.mime_types_by_request_value.is_empty() {
        return Err(format!(
            "Provider tool resource decoder {index} has an empty request MIME mapping."
        ));
    }
    // Only exact scalar strings are accepted as request mapping keys.
    // 仅接受精确标量字符串作为请求映射键。
    let request_value = request_arguments
        .and_then(|arguments| arguments.pointer(request_path))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "Provider tool resource decoder {index} cannot resolve request MIME path `{request_path}`."
            )
        })?;
    decoder
        .mime_types_by_request_value
        .get(request_value)
        .cloned()
        .ok_or_else(|| {
            format!(
                "Provider tool resource decoder {index} has no MIME mapping for request value `{request_value}`."
            )
        })
}

/// Validates one resource MIME value against the exact manifest allowlist.
/// 对照精确 Manifest 允许列表校验一个资源 MIME 值。
///
/// # Parameters
/// 参数
/// `contract` owns allowed MIME values, `mime_type` is exact, and `index` scopes diagnostics.
/// `contract` 持有允许的 MIME 值，`mime_type` 是精确值，`index` 限定诊断范围。
///
/// # Returns
/// 返回值
/// Success only for a non-empty MIME explicitly allowed by the contract.
/// 仅对契约明确允许的非空 MIME 返回成功。
fn validate_generic_resource_mime(
    contract: &crate::ProviderToolOutputContract,
    mime_type: &str,
    index: usize,
) -> Result<(), String> {
    if mime_type.trim().is_empty()
        || (!contract.mime_types.is_empty()
            && !contract
                .mime_types
                .iter()
                .any(|allowed| allowed == mime_type))
    {
        return Err(format!(
            "Provider tool resource {index} MIME `{mime_type}` is not allowed by the output contract."
        ));
    }
    Ok(())
}

/// Validates an absolute HTTPS URL without accepting credentials or fragments as alternate sources.
/// 验证绝对 HTTPS 地址，且不把凭据或片段接受为其他来源。
///
/// # Parameters
/// 参数
/// `value` is the provider or AI supplied URL and `label` scopes diagnostics.
/// `value` 是供应商或 AI 提供的地址，`label` 用于限定诊断。
///
/// # Returns
/// 返回值
/// The parsed URL when it is an absolute credential-free HTTPS resource.
/// 当地址为不含凭据的绝对 HTTPS 资源时返回解析结果。
fn validated_https_url(value: &str, label: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| format!("{label} is not a valid URL."))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(format!(
            "{label} must be an absolute credential-free HTTPS URL."
        ));
    }
    Ok(url)
}

/// Inserts one exact direct-tool authentication header.
/// 插入一个精确的直连工具鉴权请求头。
fn insert_authentication_header(
    headers: &mut HeaderMap,
    projection: AuthenticationProjection,
    secret: &CredentialExecutionSecret,
) -> Result<(), String> {
    let (name, value) = match projection {
        AuthenticationProjection::BearerAuthorization
        | AuthenticationProjection::CodingDeviceBearer => (
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", secret.expose_value()),
        ),
        AuthenticationProjection::XApiKey => (
            HeaderName::from_static("x-api-key"),
            secret.expose_value().to_string(),
        ),
        _ => {
            return Err(String::from(
                "The selected provider tool authentication projection is not registered.",
            ));
        }
    };
    let value = HeaderValue::from_str(value.as_str())
        .map_err(|_| String::from("Provider tool authentication contains invalid header bytes."))?;
    headers.insert(name, value);
    if projection == AuthenticationProjection::CodingDeviceBearer {
        crate::execution::insert_coding_device_identity_headers(headers, secret)?;
    }
    Ok(())
}

/// Inserts one target's validated non-sensitive fixed headers without overriding engine authority.
/// 插入一个目标已校验的非敏感固定请求头，且不覆盖引擎权威字段。
///
/// # Parameters
/// 参数
/// `headers` is the outbound map and `target` owns the frozen fixed-header declaration.
/// `headers` 是出站映射，`target` 持有冻结的固定请求头声明。
///
/// # Returns
/// 返回值
/// Success after every declared name and value is parsed and inserted exactly once.
/// 在每个声明名称和值均完成解析并精确插入一次后返回成功。
fn insert_declared_tool_headers(
    headers: &mut HeaderMap,
    target: &ProviderToolTarget,
) -> Result<(), String> {
    insert_fixed_tool_headers(headers, &target.execution.fixed_headers)
}

/// Inserts one validated fixed-header map into an outbound request.
/// 把一份已校验固定请求头映射插入出站请求。
///
/// # Parameters
/// 参数
/// `headers` is the outbound map and `fixed_headers` is the immutable declaration.
/// `headers` 是出站映射，`fixed_headers` 是不可变声明。
///
/// # Returns
/// 返回值
/// Success after every declared header is parsed and inserted.
/// 在每个声明请求头均完成解析并插入后返回成功。
fn insert_fixed_tool_headers(
    headers: &mut HeaderMap,
    fixed_headers: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    for (name, value) in fixed_headers {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| format!("Provider tool fixed header name `{name}` is invalid."))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|_| format!("Provider tool fixed header `{name}` has an invalid value."))?;
        headers.insert(header_name, header_value);
    }
    Ok(())
}

/// Formats one provider tool HTTP error without exposing request or credential data.
/// 格式化一个供应商工具 HTTP 错误且不暴露请求或凭据数据。
pub(crate) fn format_http_error(label: &str, status: u16, body: &str) -> String {
    match parse_bounded_provider_json(body, label) {
        Ok(value) => format_structured_provider_error(label, Some(status), &value),
        Err(_) => format!(
            "{label} request failed: HTTP {status} | message=The provider returned an unreadable error response."
        ),
    }
}

/// Bounds one diagnostic at a Unicode character boundary.
/// 在 Unicode 字符边界上限制一段诊断文本。
///
/// # Parameters
/// 参数
/// `value` is provider-controlled text and `maximum_characters` is the exact retained ceiling.
/// `value` 是供应商控制的文本，`maximum_characters` 是精确保留上限。
///
/// # Returns
/// 返回值
/// The original text when already bounded, otherwise a character-safe ellipsized prefix.
/// 文本已在上限内时返回原文，否则返回按字符安全截断并带省略号的前缀。
fn bound_provider_diagnostic_text(value: &str, maximum_characters: usize) -> String {
    if value.chars().count() <= maximum_characters {
        return value.to_string();
    }
    // retained_characters reserves the three-character ellipsis inside the declared ceiling.
    // retained_characters 在声明上限内为三个字符的省略号预留空间。
    let retained_characters = maximum_characters.saturating_sub(3);
    // bounded never splits a UTF-8 code point because it operates on Unicode scalar values.
    // bounded 基于 Unicode 标量值操作，因此绝不会切开 UTF-8 码点。
    let mut bounded = value.chars().take(retained_characters).collect::<String>();
    bounded.push_str("...");
    bounded
}

/// Reports whether one provider-owned JSON key names credential or signing material.
/// 判断一个供应商 JSON 键是否命名凭据或签名材料。
///
/// # Parameters
/// 参数
/// `key` is the exact provider-owned object key.
/// `key` 是供应商持有的精确对象键。
///
/// # Returns
/// 返回值
/// `true` only for explicit credential/signing names or their conventional suffixes.
/// 仅对显式凭据、签名名称或其常规后缀返回 `true`。
fn provider_diagnostic_key_is_sensitive(key: &str) -> bool {
    // lowercase makes the closed comparison independent from provider key casing.
    // lowercase 使封闭比较不受供应商键名大小写影响。
    let lowercase = key.to_ascii_lowercase();
    matches!(
        lowercase.as_str(),
        "authorization"
            | "proxy-authorization"
            | "api_key"
            | "api-key"
            | "apikey"
            | "secret"
            | "client_secret"
            | "token"
            | "access_token"
            | "refresh_token"
            | "signature"
            | "policy"
            | "password"
            | "credential"
            | "oss_access_key_id"
    ) || [
        "_secret",
        "-secret",
        "_token",
        "-token",
        "_signature",
        "-signature",
        "_password",
        "-password",
        "_credential",
        "-credential",
        "_api_key",
        "-api-key",
    ]
    .iter()
    .any(|suffix| lowercase.ends_with(suffix))
}

/// Sanitizes one provider-controlled diagnostic scalar without retaining URLs or credentials.
/// 清理一个供应商控制的诊断标量，不保留 URL 或凭据。
///
/// # Parameters
/// 参数
/// `value` is one non-empty provider-controlled string.
/// `value` 是一段非空供应商控制字符串。
///
/// # Returns
/// 返回值
/// A bounded safe value or a stable redaction marker.
/// 返回有界安全值或稳定脱敏标记。
pub(crate) fn sanitize_provider_diagnostic_text(value: &str) -> String {
    // lowercase and trimmed expose credential shapes without changing retained safe text.
    // lowercase 与 trimmed 在不改变保留安全文本的前提下暴露凭据形态。
    let lowercase = value.to_ascii_lowercase();
    let trimmed = lowercase.trim();
    if lowercase.contains("://") {
        return String::from("<redacted-url>");
    }
    if [
        "authorization=",
        "authorization:",
        "api_key=",
        "api-key=",
        "apikey=",
        "secret=",
        "token=",
        "signature=",
        "password=",
        "\"api_key\"",
        "\"api-key\"",
        "\"apikey\"",
        "\"secret\"",
        "\"token\"",
        "\"signature\"",
        "\"password\"",
    ]
    .iter()
    .any(|marker| lowercase.contains(marker))
        || [
            "bearer ",
            "basic ",
            "sk-",
            "sk_",
            "xoxb-",
            "xoxp-",
            "ghp_",
            "github_pat_",
            "akia",
            "aiza",
        ]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
    {
        return String::from("<redacted>");
    }
    bound_provider_diagnostic_text(value, PROVIDER_TOOL_DIAGNOSTIC_FIELD_MAX_CHARACTERS)
}

/// Recursively redacts sensitive metadata keys and bounds every provider-controlled string.
/// 递归脱敏敏感 metadata 键，并限制每段供应商控制字符串。
///
/// # Parameters
/// 参数
/// `value` is a subtree from an already depth/node-bounded provider response.
/// `value` 来自已经过深度与节点上限校验的供应商响应子树。
///
/// # Returns
/// 返回值
/// A detached JSON value safe for bounded diagnostic serialization.
/// 返回可安全用于有界诊断序列化的独立 JSON 值。
fn redact_provider_diagnostic_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    // redacted_value removes exact secret-bearing fields before serialization.
                    // redacted_value 在序列化前移除精确承载秘密的字段。
                    let redacted_value = if provider_diagnostic_key_is_sensitive(key) {
                        Value::String(String::from("<redacted>"))
                    } else {
                        redact_provider_diagnostic_json(value)
                    };
                    (key.clone(), redacted_value)
                })
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.iter().map(redact_provider_diagnostic_json).collect())
        }
        Value::String(value) => Value::String(sanitize_provider_diagnostic_text(value)),
        _ => value.clone(),
    }
}

/// Converts one optional provider scalar into bounded secret-safe diagnostic text.
/// 把一个可选供应商标量转换为有界且秘密安全的诊断文本。
///
/// # Parameters
/// 参数
/// `value` is an optional scalar from a reviewed diagnostic field.
/// `value` 是来自已审核诊断字段的可选标量。
///
/// # Returns
/// 返回值
/// Sanitized string, number, or boolean text; empty and structured values return `None`.
/// 返回已清理的字符串、数字或布尔文本；空值与结构值返回 `None`。
fn safe_provider_diagnostic_scalar(value: Option<&Value>) -> Option<String> {
    value.and_then(|value| match value {
        Value::String(value) if !value.trim().is_empty() => {
            Some(sanitize_provider_diagnostic_text(value))
        }
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    })
}

/// Formats a structured provider error while preserving the bounded diagnostic contract.
/// 格式化结构化供应商错误，同时保留有界诊断契约。
///
/// # Parameters
/// 参数
/// `label` scopes the operation, `status` is optional for HTTP-200 error envelopes, and `body`
/// is the parsed provider response.
/// `label` 限定操作范围，`status` 对 HTTP 200 错误信封可选，`body` 是已解析供应商响应。
///
/// # Returns
/// 返回值
/// A deterministic diagnostic retaining status, code, type, param, message, request id, and
/// bounded metadata without request or credential material.
/// 返回一条确定性诊断，保留 status、code、type、param、message、request id 与有界 metadata，
/// 且不包含请求或凭据材料。
fn format_structured_provider_error(label: &str, status: Option<u16>, body: &Value) -> String {
    // error selects the standard error envelope while retaining compatible top-level responses.
    // error 选择标准错误信封，同时保留兼容的顶层响应。
    let error = body.get("error").unwrap_or(body);
    // fields accumulates only the reviewed structured diagnostic contract.
    // fields 只累积已审核的结构化诊断契约。
    let mut fields = Vec::new();
    if let Some(status) = status {
        fields.push(format!("status={status}"));
    }
    for (name, value) in [
        ("code", safe_provider_diagnostic_scalar(error.get("code"))),
        ("type", safe_provider_diagnostic_scalar(error.get("type"))),
        ("param", safe_provider_diagnostic_scalar(error.get("param"))),
    ] {
        if let Some(value) = value {
            fields.push(format!("{name}={value}"));
        }
    }
    // message follows the closed provider-compatible precedence without retaining raw bodies.
    // message 遵循封闭的供应商兼容优先级，且不保留原始正文。
    let message = safe_provider_diagnostic_scalar(error.get("message"))
        .or_else(|| safe_provider_diagnostic_scalar(body.pointer("/base_resp/status_msg")))
        .or_else(|| safe_provider_diagnostic_scalar(body.pointer("/detail/error")))
        .unwrap_or_else(|| String::from("The provider returned an unspecified error."));
    fields.push(format!("message={message}"));
    if let Some(request_id) = safe_provider_diagnostic_scalar(error.get("request_id"))
        .or_else(|| safe_provider_diagnostic_scalar(body.get("request_id")))
        .or_else(|| safe_provider_diagnostic_scalar(body.get("id")))
    {
        fields.push(format!("request_id={request_id}"));
    }
    if let Some(metadata) = error.get("metadata") {
        // safe_metadata removes credential/signing keys before rendering provider-controlled data.
        // safe_metadata 在渲染供应商控制数据前移除凭据与签名键。
        let safe_metadata = redact_provider_diagnostic_json(metadata);
        // rendered is character-bounded after JSON serialization to remain valid UTF-8.
        // rendered 在 JSON 序列化后按字符限制，确保始终是有效 UTF-8。
        let rendered = serde_json::to_string(&safe_metadata)
            .unwrap_or_else(|_| String::from("<unserializable metadata>"));
        fields.push(format!(
            "metadata={}",
            bound_provider_diagnostic_text(
                rendered.as_str(),
                PROVIDER_TOOL_DIAGNOSTIC_FIELD_MAX_CHARACTERS,
            )
        ));
    }
    // diagnostic applies the final whole-message ceiling after every field has been sanitized.
    // diagnostic 在每个字段完成清理后应用最终整条消息上限。
    let diagnostic = format!("{label} request failed: {}", fields.join(" | "));
    bound_provider_diagnostic_text(diagnostic.as_str(), PROVIDER_TOOL_DIAGNOSTIC_MAX_CHARACTERS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Separators count as UTF-8 bytes and failed admission leaves the previous result intact.
    /// 分隔符按 UTF-8 字节计入限制，准入失败保留此前结果不变。
    #[test]
    fn batch_result_budget_includes_all_separator_bytes() {
        let mut output = "x".repeat(PROVIDER_TOOL_RESPONSE_BYTE_LIMIT - 4);
        append_bounded_provider_result(&mut output, "z", "中").expect("exact byte bound");
        assert_eq!(output.len(), PROVIDER_TOOL_RESPONSE_BYTE_LIMIT);
        let before = output.clone();
        assert!(append_bounded_provider_result(&mut output, "x", "|").is_err());
        assert_eq!(output, before);
        let mut first = String::new();
        append_bounded_provider_result(&mut first, "a", "ignored")
            .expect("first item has no separator");
        assert_eq!(first, "a");
    }

    /// Length admission handles all padding widths without decoding oversized data.
    /// 长度准入覆盖所有填充宽度，且不解码超限数据。
    #[test]
    fn multipart_base64_size_is_checked_before_decoding() {
        for (encoded, bytes) in [("YQ==", 1), ("YWI=", 2), ("YWJj", 3)] {
            assert!(validate_base64_decoded_size(encoded, bytes).is_ok());
            assert!(validate_base64_decoded_size(encoded, bytes - 1).is_err());
        }
        assert!(validate_base64_decoded_size("", 1).is_err());
        assert!(validate_base64_decoded_size("a===", 4).is_err());
        assert!(validate_base64_decoded_size("YWJjYQ", 4).is_err());
        assert_eq!(
            decode_bounded_multipart_data_url("data:text/plain;base64,YQ==")
                .expect("valid carrier")
                .1,
            b"a"
        );
        assert!(decode_bounded_multipart_data_url("data:text/plain;base64,!!!!").is_err());
    }

    /// Reusing a validator must still reject both invalid inputs and invalid inserted defaults.
    /// 复用验证器仍必须拒绝非法输入以及物化后非法的默认值。
    #[test]
    fn reused_argument_validator_checks_before_and_after_defaults() {
        let mut target = bundled_manifest_tool_targets("tavily")
            .into_iter()
            .next()
            .expect("fixture target");
        target.contract.input_numeric_product_ranges.clear();
        target.contract.input_schema = serde_json::json!({
            "type": "object", "required": ["query"],
            "properties": { "query": {"type": "string"}, "limit": {"type": "integer", "default": 4} }
        });
        let value =
            canonicalize_provider_tool_arguments(&target, serde_json::json!({"query":"hello"}))
                .expect("valid default is materialized");
        assert_eq!(value.get("limit"), Some(&serde_json::json!(4)));
        assert!(
            canonicalize_provider_tool_arguments(&target, serde_json::json!({"query": false}))
                .is_err()
        );
        target.contract.input_schema["properties"]["limit"]["default"] =
            serde_json::json!("invalid");
        assert!(
            canonicalize_provider_tool_arguments(&target, serde_json::json!({"query":"hello"}))
                .is_err()
        );
    }

    /// Verifies direct Coding tools include the frozen Kimi device identity with bearer authorization.
    /// 验证 Coding 直连工具会随 Bearer 鉴权发送冻结的 Kimi 设备身份。
    #[test]
    fn coding_device_authentication_includes_identity_headers() {
        // secret reproduces the exact execution shape produced by Kimi Coding Plan OAuth.
        // secret 复现 Kimi Coding Plan OAuth 生成的精确执行结构。
        let secret = crate::CredentialSecretEnvelope::new(
            crate::CredentialSecretMaterial::KimiCodingPlanOAuth {
                tokens: crate::OAuthTokenSet {
                    access_token: String::from("test-access-token"),
                    refresh_token: String::from("test-refresh-token"),
                    issued_at_unix_ms: 1,
                    expires_at_unix_ms: 2,
                    scopes: Default::default(),
                    account_subject: None,
                    email: None,
                },
                device_id: String::from("12345678-1234-1234-1234-123456789abc"),
            },
        )
        .expect("Kimi OAuth credential should validate")
        .execution_secret()
        .expect("Kimi OAuth execution secret should resolve");
        let mut headers = HeaderMap::new();

        insert_authentication_header(
            &mut headers,
            AuthenticationProjection::CodingDeviceBearer,
            &secret,
        )
        .expect("Coding-device tool authentication should project");

        assert_eq!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-access-token")
        );
        assert_eq!(
            headers
                .get("x-msh-device-id")
                .and_then(|value| value.to_str().ok()),
            Some("12345678-1234-1234-1234-123456789abc")
        );
        assert_eq!(
            headers
                .get("x-msh-platform")
                .and_then(|value| value.to_str().ok()),
            Some("CLIProxyAPI")
        );
    }

    /// Verifies transport diagnostics never retain task identities or signed URL queries.
    /// 验证传输诊断绝不保留任务身份或签名 URL 查询串。
    #[tokio::test]
    async fn provider_tool_transport_diagnostic_removes_complete_request_url() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback listener should bind");
        let address = listener
            .local_addr()
            .expect("loopback listener address should resolve");
        let server = tokio::spawn(async move {
            let (stream, _) = listener
                .accept()
                .await
                .expect("loopback connection should arrive");
            drop(stream);
        });
        let sensitive_url =
            format!("http://{address}/tasks/provider-task-secret?signature=signed-query-secret");
        let error = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("loopback test client should build")
            .get(sensitive_url.as_str())
            .send()
            .await
            .expect_err("closed loopback response must fail");
        server.await.expect("loopback server should finish");

        let diagnostic = format_provider_tool_transport_error("Provider task poll", error);
        assert!(diagnostic.starts_with("Provider task poll request failed:"));
        assert!(!diagnostic.contains("provider-task-secret"));
        assert!(!diagnostic.contains("signed-query-secret"));
        assert!(!diagnostic.contains(address.to_string().as_str()));
    }

    /// Verifies provider JSON parsing rejects trees beyond shared depth and node ceilings.
    /// 验证供应商 JSON 解析会拒绝超过共享深度与节点数量上限的树。
    #[test]
    fn provider_json_parser_enforces_depth_and_node_bounds() {
        let mut deeply_nested = Value::Null;
        for _ in 0..=crate::provider_manifest::MAX_JSON_DEPTH {
            deeply_nested = Value::Array(vec![deeply_nested]);
        }
        let deep_text = serde_json::to_string(&deeply_nested).expect("deep fixture should encode");
        assert!(
            parse_bounded_provider_json(deep_text.as_str(), "Provider fixture")
                .expect_err("deep provider JSON must fail")
                .contains("depth limit")
        );

        let oversized_nodes =
            Value::Array(vec![Value::Null; crate::provider_manifest::MAX_JSON_NODES]);
        let oversized_text =
            serde_json::to_string(&oversized_nodes).expect("node fixture should encode");
        assert!(
            parse_bounded_provider_json(oversized_text.as_str(), "Provider fixture")
                .expect_err("oversized provider JSON node tree must fail")
                .contains("node limit")
        );
    }

    /// Returns manifest-owned provider-tool targets for one bundled provider fixture.
    /// 返回一个内置供应商夹具中由 Manifest 持有的供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact bundled provider identity.
    /// `provider_id` 是精确的内置供应商标识。
    ///
    /// # Returns
    /// 返回值
    /// The compiled target list, or an empty list when the manifest is absent.
    /// 返回已编译目标列表；清单不存在时返回空列表。
    fn bundled_manifest_tool_targets(provider_id: &str) -> Vec<ProviderToolTarget> {
        crate::provider_manifest::load_bundled_resolved_catalog()
            .manifest(provider_id)
            .map(|manifest| manifest.tool_targets.clone())
            .unwrap_or_default()
    }

    /// Verifies a provider-confirmed MiniMax success cannot keep polling after deterministic result rejection.
    /// 验证供应商已确认成功后，MiniMax 确定性结果拒绝不能继续轮询。
    #[test]
    fn minimax_completed_result_contract_failure_is_terminal() {
        // target retains the production MiniMax video output contract and its frozen URL authority.
        // target 保留生产 MiniMax 视频输出契约及其冻结 URL 权限。
        let target = bundled_manifest_tool_targets("minimax")
            .into_iter()
            .find(|target| target.target_id == "cn.MiniMax-Hailuo-2.3")
            .expect("bundled MiniMax Hailuo 2.3 target");
        // response represents a successful file-retrieve document with an unauthorized resource host.
        // response 表示包含未授权资源主机的成功文件查询文档。
        let response = serde_json::json!({
            "file": {
                "download_url": "https://untrusted.invalid/generated.mp4"
            }
        });

        match decode_completed_generic_json_async_result(
            &target.contract.output,
            &response,
            "video-task-completed",
        ) {
            ProviderAsyncTaskPoll::Failed(error) => {
                assert!(
                    error.starts_with(
                        "Provider task completed, but its result could not be accepted:"
                    )
                );
                assert!(error.contains("outside the declared scheme"));
            }
            other => panic!("completed invalid result must be terminal, got {other:?}"),
        }
    }

    /// Verifies APIKEY.FAN video completion resolves the authenticated binary content route.
    /// 验证 APIKEY.FAN 视频完成流程解析到带鉴权的二进制内容路由。
    #[test]
    fn apikeyfan_video_follow_up_builds_exact_content_url() {
        // catalog is the same bundled authority used by production startup.
        // catalog 是生产启动流程使用的同一份内置权威目录。
        let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
        // manifest owns both the public service origin and video target contract.
        // manifest 同时持有公共服务源地址与视频目标契约。
        let manifest = catalog
            .manifest("apikeyfan")
            .expect("bundled APIKEY.FAN manifest");
        // entry is the exact credential-bearing public API service.
        // entry 是携带凭据的精确公共 API 服务入口。
        let entry = manifest
            .descriptor
            .service_entries
            .iter()
            .find(|entry| entry.id == "grok-media")
            .expect("APIKEY.FAN public service entry");
        // target is the verified asynchronous Grok video generator.
        // target 是已验证的异步 Grok 视频生成目标。
        let target = manifest
            .tool_targets
            .iter()
            .find(|target| target.target_id == "grok-imagine-video.generate")
            .expect("APIKEY.FAN Grok video target");
        // url must match GET /v1/videos/{request_id}/content without query fallback.
        // url 必须精确匹配 GET /v1/videos/{request_id}/content，且不使用查询参数兜底。
        let url = generic_json_async_url(
            entry,
            target,
            "/videos",
            None,
            true,
            Some("content"),
            "request-123",
        )
        .expect("video content URL should resolve");
        assert_eq!(
            url.as_str(),
            "https://api.apikey.fan/v1/videos/request-123/content"
        );
    }

    /// Verifies a bundled generic target enforces its host-only numeric product range.
    /// 验证内置通用目标会执行其仅宿主可见的数值乘积区间。
    #[test]
    fn generic_qwen_image_target_enforces_total_pixel_range() {
        // catalog retains both generated targets and their immutable template provenance.
        // catalog 同时保留生成目标及其不可变模板来源。
        let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
        // manifest is the authoritative compiled Alibaba declaration.
        // manifest 是权威的已编译 Alibaba 声明。
        let manifest = catalog
            .manifest("alibaba")
            .expect("bundled Alibaba manifest");
        // instance identifies the generated target by exact template and model facts.
        // instance 通过精确模板与模型事实标识生成目标。
        let instance = manifest
            .tool_target_template_instances
            .values()
            .find(|instance| {
                instance.template_id == "alibaba.model-studio-cn.qwen-image-generate.v1"
                    && instance.model_id == "qwen-image-2.0-pro"
            })
            .expect("generic Qwen image template instance");
        // target is resolved through provenance instead of relying on an obsolete fixed target id.
        // target 通过来源解析，而不是依赖已废弃的固定目标标识。
        let target = manifest
            .tool_targets
            .iter()
            .find(|target| target.target_id == instance.target_id)
            .cloned()
            .expect("generic Qwen image target");
        assert_eq!(
            target.execution.adapter,
            ProviderToolAdapterKind::GenericJson
        );

        let valid = canonicalize_provider_tool_arguments(
            &target,
            serde_json::json!({"prompt": "A blue forge", "width": 512, "height": 512}),
        )
        .expect("minimum reviewed pixel product should pass");
        assert_eq!(valid["count"], 1);

        let error = canonicalize_provider_tool_arguments(
            &target,
            serde_json::json!({"prompt": "A blue forge", "width": 511, "height": 512}),
        )
        .expect_err("sub-minimum pixel product must fail before dispatch");
        assert!(
            error.contains("numeric product") && error.contains("262144"),
            "unexpected product-range error: {error}"
        );
    }

    /// Verifies the manifest-owned generic JSON-sync decoder preserves the reviewed search contract.
    /// 验证 Manifest 持有的通用 JSON 同步解码器保持已审核搜索契约。
    #[test]
    fn generic_json_sync_decodes_search_contract() {
        let contract = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Search,
            source_path: String::from("/results"),
            normalized_result_path: String::from("/search"),
            resource_kind: None,
            mime_types: Vec::new(),
            resource_decoders: Vec::new(),
        };
        let result = decode_generic_json_sync_output(
            &contract,
            r#"{"query":"Vulcan","results":[{"title":"First","url":"https://example.com/first","content":"one"}],"request_id":"request-1"}"#,
        )
        .expect("generic search decode must succeed");
        assert_eq!(result.upstream_response_id.as_deref(), Some("request-1"));
        let ProviderToolNormalizedOutput::Search(search) = result.output else {
            panic!("expected search output");
        };
        assert_eq!(search.query, "Vulcan");
        assert_eq!(search.results[0].rank, 1);
        assert_eq!(search.results[0].source_domain, "example.com");
        assert_eq!(search.results[0].snippet, "one");
    }

    /// Verifies the generic decoder normalizes an extract response including per-URL failures.
    /// 验证通用解码器归一化包含逐 URL 失败的提取响应。
    #[test]
    fn generic_json_sync_decodes_extract_contract() {
        let contract = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Extract,
            source_path: String::from("/results"),
            normalized_result_path: String::from("/extract"),
            resource_kind: None,
            mime_types: Vec::new(),
            resource_decoders: Vec::new(),
        };
        let result = decode_generic_json_sync_output(
            &contract,
            r##"{"results":[{"url":"https://example.com/page","raw_content":"# Page","images":["https://example.com/image.png"],"favicon":"https://example.com/favicon.ico"}],"failed_results":[{"url":"https://example.org/missing","error":"not found"}],"request_id":"request-2"}"##,
        )
        .expect("generic extract decode must succeed");
        let ProviderToolNormalizedOutput::Extract(extract) = result.output else {
            panic!("expected extract output");
        };
        assert_eq!(extract.results[0].content, "# Page");
        assert_eq!(
            extract.results[0].images[0],
            "https://example.com/image.png"
        );
        assert_eq!(extract.failures[0].error, "not found");
    }

    /// Verifies every nested extract URL and collection remains safe and bounded.
    /// 验证每个嵌套提取 URL 与集合均保持安全且有界。
    #[test]
    fn generic_json_sync_rejects_unsafe_or_unbounded_extract_urls() {
        let contract = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Extract,
            source_path: String::from("/results"),
            normalized_result_path: String::from("/extract"),
            resource_kind: None,
            mime_types: Vec::new(),
            resource_decoders: Vec::new(),
        };

        let image_error = decode_generic_json_sync_output(
            &contract,
            r#"{"results":[{"url":"https://example.com/page","images":["javascript:alert(1)"]}]}"#,
        )
        .expect_err("non-HTTPS image URL must fail");
        assert!(image_error.contains("absolute credential-free HTTPS URL"));

        let failure_error = decode_generic_json_sync_output(
            &contract,
            r#"{"results":[],"failed_results":[{"url":"file:///private.txt","error":"blocked"}]}"#,
        )
        .expect_err("non-HTTPS failure URL must fail before reaching an anchor");
        assert!(failure_error.contains("absolute credential-free HTTPS URL"));

        let images = vec![
            Value::String(String::from("https://example.com/image.png"));
            GENERIC_STRUCTURED_RESULT_COUNT_LIMIT + 1
        ];
        let oversized = serde_json::json!({
            "results": [{
                "url": "https://example.com/page",
                "images": images,
            }],
        });
        let oversized_text = serde_json::to_string(&oversized).expect("fixture should serialize");
        let limit_error = decode_generic_json_sync_output(&contract, oversized_text.as_str())
            .expect_err("unbounded nested image list must fail");
        assert!(limit_error.contains("256-image host limit"));
    }

    /// Verifies the generic decoder rejects a missing source path and a non-array source.
    /// 验证通用解码器拒绝缺失源路径与非数组源。
    #[test]
    fn generic_json_sync_decoder_rejects_invalid_contract_source() {
        let contract = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Search,
            source_path: String::from("/missing"),
            normalized_result_path: String::from("/search"),
            resource_kind: None,
            mime_types: Vec::new(),
            resource_decoders: Vec::new(),
        };
        let error =
            decode_generic_json_sync_output(&contract, r#"{"query":"Vulcan","results":[]}"#)
                .expect_err("missing source path must fail");
        assert!(error.contains("missing source path"), "error: {error}");

        let non_array = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Search,
            source_path: String::from("/results"),
            normalized_result_path: String::from("/search"),
            resource_kind: None,
            mime_types: Vec::new(),
            resource_decoders: Vec::new(),
        };
        let error = decode_generic_json_sync_output(
            &non_array,
            r#"{"query":"Vulcan","results":{"title":"not-an-array"}}"#,
        )
        .expect_err("non-array source must fail");
        assert!(error.contains("is not an array"), "error: {error}");
    }

    /// Verifies generic text and resource contracts produce strong normalized output shapes.
    /// 验证通用文本与资源契约会产生强类型归一化输出形态。
    #[test]
    fn generic_json_sync_decodes_text_and_resource_contracts() {
        let text_contract = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Text,
            source_path: String::from("/answer"),
            normalized_result_path: String::from("/text"),
            resource_kind: None,
            mime_types: Vec::new(),
            resource_decoders: Vec::new(),
        };
        let text = decode_generic_json_sync_output(
            &text_contract,
            r#"{"answer":"completed","id":"response-1"}"#,
        )
        .expect("generic text should decode");
        assert_eq!(text.upstream_response_id.as_deref(), Some("response-1"));
        assert!(matches!(
            text.output,
            ProviderToolNormalizedOutput::Text(ref value) if value == "completed"
        ));

        let resource_contract = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Resources,
            source_path: String::from("/unused"),
            normalized_result_path: String::from("/resources"),
            resource_kind: Some(String::from("image")),
            mime_types: vec![String::from("image/png")],
            resource_decoders: vec![
                crate::ProviderToolResourceDecoder {
                    source_path: String::from("/inline"),
                    encoding: crate::ProviderToolResourceEncoding::DataUrl,
                    download_host_suffixes: Vec::new(),
                    resource_kind: String::from("image"),
                    mime_type: None,
                    mime_type_request_path: None,
                    mime_types_by_request_value: std::collections::BTreeMap::new(),
                    optional: false,
                    max_bytes: Some(16),
                    output_id_prefix: Some(String::from("inline")),
                },
                crate::ProviderToolResourceDecoder {
                    source_path: String::from("/url"),
                    encoding: crate::ProviderToolResourceEncoding::HttpsUrl,
                    download_host_suffixes: vec![String::from("example.com")],
                    resource_kind: String::from("image"),
                    mime_type: Some(String::from("image/png")),
                    mime_type_request_path: None,
                    mime_types_by_request_value: std::collections::BTreeMap::new(),
                    optional: false,
                    max_bytes: None,
                    output_id_prefix: Some(String::from("url")),
                },
            ],
        };
        let resources = decode_generic_json_sync_output(
            &resource_contract,
            r#"{"inline":"data:image/png;base64,aGVsbG8=","url":"https://example.com/image.png"}"#,
        )
        .expect("generic resources should decode");
        let ProviderToolNormalizedOutput::Resources(resources) = resources.output else {
            panic!("expected resource output");
        };
        assert_eq!(resources[0].bytes.as_deref(), Some("hello".as_bytes()));
        assert_eq!(
            resources[1].download_url.as_deref(),
            Some("https://example.com/image.png")
        );
    }

    /// Verifies declarative resource extractors decode mixed carriers and request-selected MIME.
    /// 验证声明式资源提取器可解码混合载体与请求选择 MIME。
    #[test]
    fn generic_resource_decoders_handle_hex_and_optional_url() {
        // The audio MIME table is owned by the frozen canonical request value.
        // 音频 MIME 表由冻结的规范请求值持有。
        let audio_mime_mapping = std::collections::BTreeMap::from([
            (String::from("mp3"), String::from("audio/mpeg")),
            (String::from("wav"), String::from("audio/wav")),
        ]);
        // The output contract extracts one inline audio artifact and one optional subtitle URL.
        // 输出契约提取一项内联音频产物与一项可选字幕 URL。
        let contract = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Resources,
            source_path: String::from("/unused"),
            normalized_result_path: String::from("/resources"),
            resource_kind: None,
            mime_types: vec![
                String::from("audio/mpeg"),
                String::from("audio/wav"),
                String::from("application/json"),
            ],
            resource_decoders: vec![
                crate::ProviderToolResourceDecoder {
                    source_path: String::from("/data/audio"),
                    encoding: crate::ProviderToolResourceEncoding::Hex,
                    download_host_suffixes: Vec::new(),
                    resource_kind: String::from("audio"),
                    mime_type: None,
                    mime_type_request_path: Some(String::from("/output_format")),
                    mime_types_by_request_value: audio_mime_mapping,
                    optional: false,
                    max_bytes: Some(16),
                    output_id_prefix: Some(String::from("audio")),
                },
                crate::ProviderToolResourceDecoder {
                    source_path: String::from("/data/subtitle_file"),
                    encoding: crate::ProviderToolResourceEncoding::PublicHttpUrl,
                    download_host_suffixes: vec![String::from("example.com")],
                    resource_kind: String::from("file"),
                    mime_type: Some(String::from("application/json")),
                    mime_type_request_path: None,
                    mime_types_by_request_value: std::collections::BTreeMap::new(),
                    optional: true,
                    max_bytes: None,
                    output_id_prefix: Some(String::from("subtitle")),
                },
            ],
        };
        // The canonical arguments select MP3 independently of provider response fields.
        // 规范参数独立于供应商响应字段选择 MP3。
        let request = serde_json::json!({"output_format": "mp3"});
        // The projected response contains raw hex bytes plus one reviewed public URL.
        // 投影响应包含原始十六进制字节与一个已审核公共 URL。
        let result = decode_generic_json_sync_output_with_request(
            &contract,
            r#"{"data":{"audio":"68656c6c6f","subtitle_file":"http://example.com/subtitle.json"}}"#,
            Some(&request),
        )
        .expect("declarative mixed resources should decode");
        let ProviderToolNormalizedOutput::Resources(resources) = result.output else {
            panic!("expected resource output");
        };
        assert_eq!(resources[0].output_id, "audio-1");
        assert_eq!(resources[0].mime_type, "audio/mpeg");
        assert_eq!(resources[0].bytes.as_deref(), Some("hello".as_bytes()));
        assert_eq!(resources[1].output_id, "subtitle-1");
        assert_eq!(
            resources[1].download_url.as_deref(),
            Some("http://example.com/subtitle.json")
        );
        // The retained policy must match the exact decoder authority rather than response data.
        // 保留策略必须匹配精确解码器权限，而不是响应数据。
        let download_policy = resources[1]
            .download_policy
            .as_ref()
            .expect("URL resource must retain frozen download authority");
        assert_eq!(
            download_policy.encoding,
            crate::ProviderToolResourceEncoding::PublicHttpUrl
        );
        assert_eq!(download_policy.host_suffixes, [String::from("example.com")]);
        // An alternate valid HTTPS origin proves host authority is enforced independently of syntax.
        // 另一个有效 HTTPS Origin 证明宿主权限独立于语法得到执行。
        let error = download_policy
            .validate_url(
                "https://assets.example.org/subtitle.json",
                "Provider tool test resource",
            )
            .expect_err("undeclared host must remain outside the frozen authority");
        assert!(error.contains("declared scheme"));
        assert!(error.contains("host suffixes"));
        assert!(error.contains("assets.example.org"));

        // A non-default port is a distinct network authority that the host-only policy cannot grant.
        // 非默认端口属于宿主专用策略无法授权的另一项网络权限。
        let error = download_policy
            .validate_url(
                "https://example.com:8443/subtitle.json?signature=temporary",
                "Provider tool test resource",
            )
            .expect_err("undeclared port must remain outside the frozen authority");
        assert!(error.contains("default port"));

        // exactOrigin is a host-owned exception for a task initiated on one declared non-default port.
        // exactOrigin 是对一个已声明非默认端口启动任务的宿主持有例外。
        let exact_origin = reqwest::Url::parse("https://example.com:8443/v1/tasks/1")
            .expect("exact operation origin should parse");
        download_policy
            .validate_url_with_exact_origin(
                "https://example.com:8443/subtitle.json?signature=temporary",
                "Provider tool test resource",
                &exact_origin,
            )
            .expect("the exact frozen operation origin should retain its port");
        let wrong_port = download_policy
            .validate_url_with_exact_origin(
                "https://example.com:9443/subtitle.json?signature=temporary",
                "Provider tool test resource",
                &exact_origin,
            )
            .expect_err("a different non-default port must remain outside the frozen authority");
        assert!(wrong_port.contains("exact operation origin"));
    }

    /// Verifies declarative resource byte limits and MIME mappings fail closed.
    /// 验证声明式资源字节上限与 MIME 映射保持失败关闭。
    #[test]
    fn generic_resource_decoders_reject_overflow_and_unknown_mime_selection() {
        // The decoder permits only one decoded byte and one registered request format.
        // 解码器只允许一个解码字节与一个已注册请求格式。
        let contract = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Resources,
            source_path: String::from("/unused"),
            normalized_result_path: String::from("/resources"),
            resource_kind: None,
            mime_types: vec![String::from("audio/mpeg")],
            resource_decoders: vec![crate::ProviderToolResourceDecoder {
                source_path: String::from("/audio"),
                encoding: crate::ProviderToolResourceEncoding::Base64,
                download_host_suffixes: Vec::new(),
                resource_kind: String::from("audio"),
                mime_type: None,
                mime_type_request_path: Some(String::from("/format")),
                mime_types_by_request_value: std::collections::BTreeMap::from([(
                    String::from("mp3"),
                    String::from("audio/mpeg"),
                )]),
                optional: false,
                max_bytes: Some(1),
                output_id_prefix: Some(String::from("audio")),
            }],
        };
        // Two decoded bytes exceed the exact declared ceiling.
        // 两个解码字节超过精确声明上限。
        let overflow_error = decode_generic_json_sync_output_with_request(
            &contract,
            r#"{"audio":"aGk="}"#,
            Some(&serde_json::json!({"format": "mp3"})),
        )
        .expect_err("decoded bytes above max_bytes must fail");
        assert!(overflow_error.contains("no larger than 1 bytes"));

        // One decoded byte reaches MIME selection only after passing the byte guard.
        // 一个解码字节在通过字节守卫后才进入 MIME 选择。
        let mime_error = decode_generic_json_sync_output_with_request(
            &contract,
            r#"{"audio":"aA=="}"#,
            Some(&serde_json::json!({"format": "flac"})),
        )
        .expect_err("unregistered request MIME selection must fail");
        assert!(mime_error.contains("no MIME mapping"));
    }

    /// Verifies generic error envelopes retain every standard diagnostic field.
    /// 验证通用错误信封保留每个标准诊断字段。
    #[test]
    fn generic_json_sync_preserves_structured_error_fields() {
        let contract = crate::ProviderToolOutputContract {
            kind: ProviderToolOutputKind::Text,
            source_path: String::from("/answer"),
            normalized_result_path: String::from("/text"),
            resource_kind: None,
            mime_types: Vec::new(),
            resource_decoders: Vec::new(),
        };
        let error = decode_generic_json_sync_output(
            &contract,
            r#"{"request_id":"request-1","error":{"code":"bad_input","type":"invalid_request","param":"prompt","message":"Prompt is empty","metadata":{"provider_name":"fixture"}}}"#,
        )
        .expect_err("generic error envelope must fail");
        for expected in [
            "code=bad_input",
            "type=invalid_request",
            "param=prompt",
            "message=Prompt is empty",
            "request_id=request-1",
            "provider_name",
        ] {
            assert!(
                error.contains(expected),
                "missing `{expected}` in `{error}`"
            );
        }
    }

    /// Verifies structured diagnostics redact signing data and truncate Unicode without panicking.
    /// 验证结构化诊断会脱敏签名数据，并在不 panic 的情况下截断 Unicode。
    #[test]
    fn structured_error_diagnostics_are_secret_safe_and_character_bounded() {
        // long_provider_name crosses the per-field ceiling exclusively with multibyte characters.
        // long_provider_name 仅使用多字节字符跨越逐字段上限。
        let long_provider_name = "供".repeat(PROVIDER_TOOL_DIAGNOSTIC_FIELD_MAX_CHARACTERS + 64);
        // body contains credential-shaped scalars, a signed URL, and sensitive metadata keys.
        // body 包含凭据形态标量、签名 URL 与敏感 metadata 键。
        let body = serde_json::json!({
            "error": {
                "code": "sk-live-secret-code",
                "message": "https://upload.example.invalid/object?signature=temporary-signature",
                "metadata": {
                    "access_token": "temporary-access-token",
                    "provider_name": long_provider_name,
                    "signature": "temporary-signature"
                }
            },
            "request_id": "request-safe"
        });
        // body_text follows the same bounded JSON path used for live HTTP failures.
        // body_text 遵循实时 HTTP 失败使用的同一有界 JSON 路径。
        let body_text = serde_json::to_string(&body).expect("diagnostic fixture should serialize");

        // diagnostic must preserve safe structure while excluding every sensitive fixture value.
        // diagnostic 必须保留安全结构，同时排除每个敏感夹具值。
        let diagnostic = format_http_error("Provider fixture", 400, body_text.as_str());
        assert!(diagnostic.contains("status=400"));
        assert!(diagnostic.contains("<redacted>"));
        assert!(diagnostic.contains("<redacted-url>"));
        assert!(!diagnostic.contains("temporary-access-token"));
        assert!(!diagnostic.contains("temporary-signature"));
        assert!(
            diagnostic.chars().count() <= PROVIDER_TOOL_DIAGNOSTIC_MAX_CHARACTERS,
            "diagnostic exceeded the character ceiling"
        );
    }

    /// Verifies every bundled system target has left the legacy execution bridge.
    /// 验证每个内置系统目标均已离开旧执行桥。
    #[test]
    fn bundled_system_targets_use_closed_execution_families() {
        for provider_id in ["minimax", "alibaba", "tavily"] {
            for target in bundled_manifest_tool_targets(provider_id) {
                assert!(
                    matches!(
                        target.execution.execution_kind,
                        ProviderToolExecutionKind::HttpJsonSync
                            | ProviderToolExecutionKind::HttpJsonSse
                            | ProviderToolExecutionKind::HttpMultipartSse
                            | ProviderToolExecutionKind::HttpBinarySync
                            | ProviderToolExecutionKind::ConversationMediaAnalyze
                            | ProviderToolExecutionKind::HttpJsonAsyncPoll
                            | ProviderToolExecutionKind::McpHttpCall
                    ),
                    "target `{}` must use one closed execution family",
                    target.target_id
                );
            }
        }
    }

    /// Verifies every Tavily target declares the generic JSON-sync execution kind.
    /// 验证全部 Tavily 目标声明通用 JSON 同步执行族。
    #[test]
    fn tavily_targets_declare_http_json_sync_execution() {
        let targets = bundled_manifest_tool_targets("tavily");
        assert_eq!(targets.len(), 2);
        for target in targets {
            assert_eq!(
                target.execution.execution_kind,
                ProviderToolExecutionKind::HttpJsonSync,
                "target `{}` must use the generic primitive",
                target.target_id
            );
        }
    }

    /// Verifies mixed LF and CRLF frames split at the earliest blank line instead of merging.
    /// 验证 LF 与 CRLF 混用时在最早空行处分帧，而不是合并帧。
    #[test]
    fn sse_frame_boundary_prefers_earliest_blank_line_across_line_endings() {
        let bytes = b"data: a\n\ndata: b\r\n\r\n";
        let (frame_end, boundary_end) =
            find_sse_frame_boundary(bytes, 0).expect("first LF frame boundary");
        assert_eq!(&bytes[..frame_end], b"data: a\n");
        let (second_end, second_boundary) =
            find_sse_frame_boundary(bytes, boundary_end).expect("second CRLF frame boundary");
        assert_eq!(&bytes[boundary_end..second_end], b"data: b\r\n");
        assert_eq!(second_boundary, bytes.len());
    }

    /// Verifies a CRLF boundary split across chunks is found when scanning resumes near the old tail.
    /// 验证跨分块的 CRLF 边界在从旧尾部附近恢复扫描时仍能被找到。
    #[test]
    fn sse_frame_boundary_resumes_across_chunk_split() {
        let mut bytes = b"data: a\r\n\r".to_vec();
        assert!(find_sse_frame_boundary(&bytes, 0).is_none());
        let resume_from = bytes.len().saturating_sub(2);
        bytes.push(b'\n');
        let (frame_end, boundary_end) =
            find_sse_frame_boundary(&bytes, resume_from).expect("boundary completed by next chunk");
        assert_eq!(&bytes[..frame_end], b"data: a\r\n");
        assert_eq!(boundary_end, bytes.len());
    }
}
