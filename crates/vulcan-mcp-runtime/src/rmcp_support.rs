//! Shared rmcp-side support helpers reused by multiple MCP client transports.
//! 被多个 MCP 客户端传输复用的共享 rmcp 侧支持辅助模块。

use std::time::Duration;

use rmcp::ClientHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult, JsonObject,
    Prompt, ReadResourceRequestParams, ReadResourceResult, Resource, ServerCapabilities,
    Tool as RmcpTool,
};
use rmcp::service::{NotificationContext, Peer, RoleClient};
use serde_json::{Map, Value};
use tokio::sync::broadcast;

use crate::error::McpRuntimeError;
use crate::transport::{McpToolSchema, McpTransportEvent};

/// Maximum decoded bytes retained for one MCP image content block.
/// 单个 MCP 图片内容块允许保留的最大解码字节数。
const MCP_IMAGE_MAX_BYTES: usize = 20 * 1024 * 1024;

/// Stable metadata key used by some MCP servers to declare which input fields should be treated as local file-path uploads.
/// 某些 MCP 服务器用于声明哪些输入字段应被视为本地文件路径上传的稳定元数据键。
const OPENAI_FILE_PARAMS_META_KEY: &str = "openai/fileParams";

/// Stable description suffix appended to model-visible file-path parameters so the model prefers passing absolute local paths.
/// 追加到模型可见文件路径参数上的稳定描述后缀，用于引导模型优先传入绝对本地路径。
const OPENAI_FILE_PARAM_GUIDANCE: &str = "This parameter expects an absolute local file path. If you want to upload a file, provide the absolute path to that file here.";

/// Shared rmcp notification adapter that forwards tool-list changes into the host runtime event stream.
/// 将工具列表变化转发到宿主运行时事件流的共享 rmcp 通知适配器。
#[derive(Clone)]
pub(crate) struct EventingClientHandler {
    /// Broadcast channel used to emit MCP transport lifecycle events.
    /// 用于发出 MCP 传输生命周期事件的广播通道。
    event_sender: broadcast::Sender<McpTransportEvent>,
}

impl EventingClientHandler {
    /// Create one eventing rmcp client handler from a shared broadcast sender.
    /// 根据共享广播发送端创建一个带事件能力的 rmcp 客户端处理器。
    pub(crate) fn new(event_sender: broadcast::Sender<McpTransportEvent>) -> Self {
        Self { event_sender }
    }
}

impl ClientHandler for EventingClientHandler {
    /// Forward `tools/list_changed` notifications into the host runtime event stream.
    /// 将 `tools/list_changed` 通知转发到宿主运行时事件流。
    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.event_sender.send(McpTransportEvent::ToolsChanged);
    }

    /// Forward `prompts/list_changed` notifications into the host runtime event stream.
    /// 将 `prompts/list_changed` 通知转发到宿主运行时事件流。
    async fn on_prompt_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.event_sender.send(McpTransportEvent::PromptsChanged);
    }

    /// Forward `resources/list_changed` notifications into the host runtime event stream.
    /// 将 `resources/list_changed` 通知转发到宿主运行时事件流。
    async fn on_resource_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.event_sender.send(McpTransportEvent::ResourcesChanged);
    }
}

/// Spawn one shared rmcp disconnect watcher that emits a typed disconnect reason once the peer closes.
/// 启动一个共享 rmcp 断连监视器，并在 peer 关闭后发出带类型的断连原因。
pub(crate) fn spawn_rmcp_disconnect_watch(
    endpoint: String,
    transport_label: &'static str,
    peer: Peer<RoleClient>,
    event_sender: broadcast::Sender<McpTransportEvent>,
) {
    tokio::spawn(async move {
        loop {
            if peer.is_transport_closed() {
                let _ = event_sender.send(McpTransportEvent::Disconnected {
                    reason: format!("MCP {transport_label} transport `{endpoint}` closed."),
                });
                break;
            }

            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
}

/// Build one rmcp tool-call request parameter object from the host-owned JSON argument contract.
/// 根据宿主自有 JSON 参数契约构建一个 rmcp 工具调用参数对象。
pub(crate) fn build_rmcp_call_tool_request_params(
    name: &str,
    args: Value,
) -> Result<CallToolRequestParams, McpRuntimeError> {
    let arguments = match args {
        Value::Object(map) => Some(map),
        Value::Null => None,
        other => {
            return Err(McpRuntimeError::invalid_argument(format!(
                "tools/call requires an object or null arguments, got {other}"
            )));
        }
    };

    let mut params = CallToolRequestParams::new(name.to_string());
    params.arguments = arguments;
    Ok(params)
}

/// Convert one rmcp `CallToolResult` into the host-owned JSON value contract.
/// 将单个 rmcp `CallToolResult` 转换为宿主自有 JSON 值契约。
pub(crate) fn rmcp_call_tool_result_to_value(
    result: CallToolResult,
) -> Result<Value, McpRuntimeError> {
    if result.is_error.unwrap_or(false) {
        let message = result
            .content
            .iter()
            .find_map(|content| content.as_text().map(|text| text.text.clone()))
            .unwrap_or_else(|| "tool returned error".to_string());
        return Err(McpRuntimeError::transport(message));
    }

    if let Some(value) = result.structured_content.as_ref()
        && !result
            .content
            .iter()
            .any(|content| content.as_image().is_some())
    {
        return Ok(value.clone());
    }

    if result
        .content
        .iter()
        .any(|content| content.as_image().is_some())
    {
        return rmcp_call_tool_result_with_media_to_value(result);
    }

    if let Some(text) = result
        .content
        .iter()
        .find_map(|content| content.as_text().map(|text| text.text.clone()))
    {
        if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
            return Ok(parsed);
        }
        return Ok(Value::String(text));
    }

    Ok(Value::Null)
}

/// Preserve text and image blocks in an explicit JSON envelope when an MCP result carries media.
/// MCP 结果包含媒体时，以明确的 JSON 信封保留文本与图片块。
///
/// The legacy scalar/structured return forms remain handled by the caller, so ordinary text tools
/// keep their existing shape while media-bearing results cannot silently lose content blocks.
/// 调用方继续处理旧的标量/结构化返回形态，因此普通文本工具保持原样，同时带媒体的结果不会静默丢失内容块。
fn rmcp_call_tool_result_with_media_to_value(
    result: CallToolResult,
) -> Result<Value, McpRuntimeError> {
    let mut content = Vec::with_capacity(result.content.len());
    for block in result.content {
        if let Some(text) = block.as_text() {
            content.push(serde_json::json!({
                "type": "text",
                "text": text.text,
            }));
            continue;
        }
        if let Some(image) = block.as_image() {
            validate_mcp_image_content(image.data.as_str(), image.mime_type.as_str())
                .map_err(McpRuntimeError::transport)?;
            content.push(serde_json::json!({
                "type": "image",
                "data": image.data,
                "mimeType": image.mime_type,
            }));
            continue;
        }
        return Err(McpRuntimeError::transport(
            "MCP media result contains an unsupported content block.",
        ));
    }

    let mut envelope = Map::new();
    envelope.insert(String::from("content"), Value::Array(content));
    if let Some(structured_content) = result.structured_content {
        envelope.insert(String::from("structured_content"), structured_content);
    }
    Ok(Value::Object(envelope))
}

/// Validate one MCP image MIME/data pair without including image data in diagnostics.
/// 校验单个 MCP 图片 MIME/数据对，诊断信息不包含图片数据。
fn validate_mcp_image_content(data: &str, mime_type: &str) -> Result<(), String> {
    if !matches!(mime_type, "image/png" | "image/jpeg" | "image/webp") {
        return Err(String::from(
            "MCP image content uses an unsupported MIME type.",
        ));
    }
    decode_standard_base64_bounded(data)?;
    Ok(())
}

/// Decode standard padded Base64 solely to validate syntax and enforce the decoded byte limit.
/// 仅为校验语法并限制解码字节数而解码标准带填充 Base64。
fn decode_standard_base64_bounded(data: &str) -> Result<Vec<u8>, String> {
    let bytes = data.as_bytes();
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return Err(String::from("MCP image content has invalid Base64 data."));
    }
    let padding = bytes.iter().rev().take_while(|byte| **byte == b'=').count();
    if padding > 2 || bytes[..bytes.len() - padding].contains(&b'=') {
        return Err(String::from("MCP image content has invalid Base64 data."));
    }
    let decoded_len = (bytes.len() / 4)
        .checked_mul(3)
        .and_then(|length| length.checked_sub(padding))
        .ok_or_else(|| String::from("MCP image content exceeds the 20 MiB limit."))?;
    if decoded_len == 0 {
        return Err(String::from("MCP image content has empty Base64 data."));
    }
    if decoded_len > MCP_IMAGE_MAX_BYTES {
        return Err(String::from("MCP image content exceeds the 20 MiB limit."));
    }

    let mut decoded = Vec::with_capacity(decoded_len);
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        let is_last = index + 1 == bytes.len() / 4;
        let first = decode_base64_value(chunk[0])?;
        let second = decode_base64_value(chunk[1])?;
        let third = if chunk[2] == b'=' {
            if !is_last || chunk[3] != b'=' || second & 0x0f != 0 {
                return Err(String::from("MCP image content has invalid Base64 data."));
            }
            None
        } else {
            Some(decode_base64_value(chunk[2])?)
        };
        let fourth = if chunk[3] == b'=' {
            if !is_last || third.is_some_and(|value| value & 0x03 != 0) {
                return Err(String::from("MCP image content has invalid Base64 data."));
            }
            None
        } else {
            Some(decode_base64_value(chunk[3])?)
        };
        decoded.push((first << 2) | (second >> 4));
        if let Some(third) = third {
            decoded.push((second << 4) | (third >> 2));
            if let Some(fourth) = fourth {
                decoded.push((third << 6) | fourth);
            }
        }
    }
    Ok(decoded)
}

/// Decode one standard Base64 alphabet byte.
/// 解码一个标准 Base64 字母表字节。
fn decode_base64_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(String::from("MCP image content has invalid Base64 data.")),
    }
}

/// Convert one rmcp tool schema into the host-owned MCP tool schema.
/// 将单个 rmcp 工具 schema 转换为宿主自有 MCP 工具 schema。
pub(crate) fn rmcp_tool_to_schema(tool: RmcpTool) -> McpToolSchema {
    let args_schema =
        build_model_visible_tool_input_schema(tool.input_schema.as_ref(), tool.meta.as_deref());
    McpToolSchema {
        name: tool.name.to_string(),
        description: tool
            .description
            .map(|value| value.to_string())
            .unwrap_or_default(),
        args_schema,
        result_schema: tool
            .output_schema
            .map(|schema| Value::Object((*schema).clone()))
            .unwrap_or(Value::Null),
    }
}

/// Builds the model-visible MCP input schema while preserving the raw transport schema semantics needed by invocation.
/// 构建模型可见的 MCP 输入 Schema，同时保留调用阶段所需的原始传输语义。
fn build_model_visible_tool_input_schema(
    input_schema: &Map<String, Value>,
    meta: Option<&Map<String, Value>>,
) -> Value {
    let file_params = declared_openai_file_input_param_names(meta);
    let mut visible_schema = Value::Object(input_schema.clone());
    if file_params.is_empty() {
        return visible_schema;
    }

    mask_input_schema_for_file_path_params(&mut visible_schema, &file_params);
    visible_schema
}

/// Reads the declared OpenAI-style file parameter names from one optional MCP tool metadata object.
/// 从可选的 MCP 工具元数据对象中读取声明的 OpenAI 风格文件参数名集合。
fn declared_openai_file_input_param_names(meta: Option<&Map<String, Value>>) -> Vec<String> {
    let Some(meta) = meta else {
        return Vec::new();
    };

    meta.get(OPENAI_FILE_PARAMS_META_KEY)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Masks the selected file-path parameters in one model-visible input schema so upstream models see a stable string-or-string-array contract.
/// 对模型可见输入 Schema 中被选中的文件路径参数执行遮罩，使上游模型看到稳定的字符串或字符串数组契约。
fn mask_input_schema_for_file_path_params(input_schema: &mut Value, file_params: &[String]) {
    let Some(properties) = input_schema
        .as_object_mut()
        .and_then(|schema| schema.get_mut("properties"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    for field_name in file_params {
        let Some(property_schema) = properties.get_mut(field_name) else {
            continue;
        };
        mask_input_property_schema(property_schema);
    }
}

/// Rewrites one selected input-property schema into a conservative model-visible file-path contract while preserving array-vs-scalar intent.
/// 将单个被选中的输入属性 Schema 改写为保守的模型可见文件路径契约，同时保留数组或标量语义。
fn mask_input_property_schema(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };

    let mut description = object
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_default();
    if description.is_empty() {
        description = String::from(OPENAI_FILE_PARAM_GUIDANCE);
    } else if !description.contains(OPENAI_FILE_PARAM_GUIDANCE) {
        description = format!("{description} {OPENAI_FILE_PARAM_GUIDANCE}");
    }

    let is_array = object.get("type").and_then(Value::as_str) == Some("array")
        || object.get("items").is_some();
    object.clear();
    object.insert(String::from("description"), Value::String(description));
    if is_array {
        object.insert(String::from("type"), Value::String(String::from("array")));
        object.insert(
            String::from("items"),
            serde_json::json!({ "type": "string" }),
        );
    } else {
        object.insert(String::from("type"), Value::String(String::from("string")));
    }
}

/// Returns whether one negotiated rmcp server capability snapshot advertises prompt support.
/// 返回单份已协商 rmcp 服务器能力快照是否声明了 prompt 支持。
pub(crate) fn rmcp_capabilities_support_prompts(capabilities: &ServerCapabilities) -> bool {
    capabilities.prompts.is_some()
}

/// Returns whether one negotiated rmcp server capability snapshot advertises resource support.
/// 返回单份已协商 rmcp 服务器能力快照是否声明了资源支持。
pub(crate) fn rmcp_capabilities_support_resources(capabilities: &ServerCapabilities) -> bool {
    capabilities.resources.is_some()
}

/// Returns whether one connected rmcp peer advertises prompt support after initialize.
/// 返回单个已连接 rmcp peer 在 initialize 后是否声明了 prompt 支持。
fn rmcp_peer_supports_prompts(peer: &Peer<RoleClient>) -> bool {
    peer.peer_info()
        .map(|info| rmcp_capabilities_support_prompts(&info.capabilities))
        .unwrap_or(false)
}

/// Returns whether one connected rmcp peer advertises resource support after initialize.
/// 返回单个已连接 rmcp peer 在 initialize 后是否声明了资源支持。
fn rmcp_peer_supports_resources(peer: &Peer<RoleClient>) -> bool {
    peer.peer_info()
        .map(|info| rmcp_capabilities_support_resources(&info.capabilities))
        .unwrap_or(false)
}

/// List tools from one rmcp client peer and convert them into the host-owned MCP schema contract.
/// 从单个 rmcp 客户端 peer 拉取工具列表，并将其转换为宿主自有 MCP schema 契约。
pub(crate) async fn list_rmcp_tools(
    peer: &Peer<RoleClient>,
    error_context: &str,
) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
    let tools = peer.list_all_tools().await.map_err(|error| {
        McpRuntimeError::transport(format!("tools/list({error_context}): {error}"))
    })?;
    Ok(tools.into_iter().map(rmcp_tool_to_schema).collect())
}

/// Convert one host-owned JSON prompt-argument payload into the rmcp prompt request contract.
/// 将一份宿主自有 JSON prompt 参数载荷转换为 rmcp prompt 请求契约。
fn build_rmcp_prompt_arguments(args: Value) -> Result<Option<JsonObject>, McpRuntimeError> {
    match args {
        Value::Null => Ok(None),
        Value::Object(map) => Ok(Some(map)),
        other => Err(McpRuntimeError::invalid_argument(format!(
            "prompts/get requires an object or null arguments, got {other}"
        ))),
    }
}

/// List prompts from one rmcp client peer.
/// 从单个 rmcp 客户端 peer 拉取 prompt 列表。
pub(crate) async fn list_rmcp_prompts(
    peer: &Peer<RoleClient>,
    error_context: &str,
) -> Result<Vec<Prompt>, McpRuntimeError> {
    if !rmcp_peer_supports_prompts(peer) {
        return Ok(Vec::new());
    }
    peer.list_all_prompts().await.map_err(|error| {
        McpRuntimeError::transport(format!("prompts/list({error_context}): {error}"))
    })
}

/// Resolve one rmcp prompt and return the typed prompt result.
/// 解析单个 rmcp prompt 并返回类型化 prompt 结果。
pub(crate) async fn get_rmcp_prompt(
    peer: &Peer<RoleClient>,
    error_context: &str,
    name: &str,
    args: Value,
) -> Result<GetPromptResult, McpRuntimeError> {
    if !rmcp_peer_supports_prompts(peer) {
        return Err(McpRuntimeError::transport(format!(
            "prompts/get({error_context}): server does not advertise prompts capability"
        )));
    }
    let mut params = GetPromptRequestParams::new(name.to_string());
    params.arguments = build_rmcp_prompt_arguments(args)?;
    peer.get_prompt(params).await.map_err(|error| {
        McpRuntimeError::transport(format!("prompts/get({error_context}): {error}"))
    })
}

/// List resources from one rmcp client peer.
/// 从单个 rmcp 客户端 peer 拉取资源列表。
pub(crate) async fn list_rmcp_resources(
    peer: &Peer<RoleClient>,
    error_context: &str,
) -> Result<Vec<Resource>, McpRuntimeError> {
    if !rmcp_peer_supports_resources(peer) {
        return Ok(Vec::new());
    }
    peer.list_all_resources().await.map_err(|error| {
        McpRuntimeError::transport(format!("resources/list({error_context}): {error}"))
    })
}

/// Read one rmcp resource by URI and return the typed resource result.
/// 通过 URI 读取单个 rmcp 资源并返回类型化资源结果。
pub(crate) async fn read_rmcp_resource(
    peer: &Peer<RoleClient>,
    error_context: &str,
    uri: &str,
) -> Result<ReadResourceResult, McpRuntimeError> {
    if !rmcp_peer_supports_resources(peer) {
        return Err(McpRuntimeError::transport(format!(
            "resources/read({error_context}): server does not advertise resources capability"
        )));
    }
    peer.read_resource(ReadResourceRequestParams::new(uri.to_string()))
        .await
        .map_err(|error| {
            McpRuntimeError::transport(format!("resources/read({error_context}): {error}"))
        })
}

/// Call one rmcp tool through the provided peer and convert the result into the host-owned JSON value contract.
/// 通过给定 peer 调用单个 rmcp 工具，并将结果转换为宿主自有 JSON 值契约。
pub(crate) async fn call_rmcp_tool(
    peer: &Peer<RoleClient>,
    error_context: &str,
    name: &str,
    args: Value,
) -> Result<Value, McpRuntimeError> {
    call_rmcp_tool_with_audience(
        peer,
        error_context,
        name,
        args,
        crate::McpToolResultAudience::Machine,
    )
    .await
}

/// Calls one MCP tool once and selects the consumer-specific result projection afterward.
/// 仅调用 MCP 工具一次，并在完成后选择消费者对应的结果投影。
/// Inputs retain the original request and peer; output is machine JSON or complete conversation content.
/// 输入保留原请求及 peer；输出为机器 JSON 或完整会话内容。
pub(crate) async fn call_rmcp_tool_with_audience(
    peer: &Peer<RoleClient>,
    error_context: &str,
    name: &str,
    args: Value,
    audience: crate::McpToolResultAudience,
) -> Result<Value, McpRuntimeError> {
    let params = build_rmcp_call_tool_request_params(name, args)?;
    let result = peer.call_tool(params).await.map_err(|error| {
        McpRuntimeError::transport(format!("tools/call({error_context}): {error}"))
    })?;
    project_rmcp_tool_result(result, audience)
}

mod conversation_result;

/// Selects the existing machine contract or the explicit conversation reading contract.
/// 选择既有机器契约或明确的会话阅读契约。
/// Accepts a completed response and audience; returns its single final projection.
/// 接受已完成响应与消费者；返回其唯一最终投影。
pub(crate) fn project_rmcp_tool_result(
    result: CallToolResult,
    audience: crate::McpToolResultAudience,
) -> Result<Value, McpRuntimeError> {
    match audience {
        crate::McpToolResultAudience::Machine => rmcp_call_tool_result_to_value(result),
        crate::McpToolResultAudience::Conversation => conversation_result::project(result),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MCP_IMAGE_MAX_BYTES, build_model_visible_tool_input_schema,
        build_rmcp_call_tool_request_params, declared_openai_file_input_param_names,
        rmcp_call_tool_result_to_value, rmcp_capabilities_support_prompts,
        rmcp_capabilities_support_resources,
    };
    use rmcp::model::{CallToolResult, ServerCapabilities};
    use serde_json::{Value, json};

    #[test]
    fn rmcp_call_tool_request_builder_rejects_non_object_arguments() {
        let error = build_rmcp_call_tool_request_params("demo", json!(["bad"]))
            .expect_err("non-object arguments must be rejected");

        assert_eq!(
            error.to_string(),
            String::from("tools/call requires an object or null arguments, got [\"bad\"]")
        );
    }

    #[test]
    fn rmcp_call_tool_result_prefers_structured_content() {
        let mut result = CallToolResult::default();
        result.structured_content = Some(json!({"ok": true}));

        assert_eq!(
            rmcp_call_tool_result_to_value(result).expect("structured content should decode"),
            json!({"ok": true})
        );
    }

    #[test]
    fn rmcp_call_tool_result_falls_back_to_plain_text() {
        let mut result = CallToolResult::default();
        result.content = vec![rmcp::model::ContentBlock::text("plain-text")];

        assert_eq!(
            rmcp_call_tool_result_to_value(result).expect("plain text should decode"),
            Value::String(String::from("plain-text"))
        );
    }

    #[test]
    fn rmcp_call_tool_result_preserves_ordered_text_and_image_blocks() {
        let mut result = CallToolResult::default();
        result.content = vec![
            rmcp::model::ContentBlock::text("before"),
            rmcp::model::ContentBlock::image("iVBORw0KGgo=", "image/png"),
            rmcp::model::ContentBlock::text("after"),
        ];

        assert_eq!(
            rmcp_call_tool_result_to_value(result).expect("media content should decode"),
            json!({
                "content": [
                    {"type": "text", "text": "before"},
                    {"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"},
                    {"type": "text", "text": "after"},
                ]
            })
        );
    }

    /// Verifies standard Base64 with two terminal padding characters remains valid.
    /// 验证带两个末尾填充字符的标准 Base64 仍然被视为有效数据。
    #[test]
    fn rmcp_call_tool_result_accepts_double_padded_image_data() {
        let mut result = CallToolResult::default();
        result.content = vec![rmcp::model::ContentBlock::image("TQ==", "image/png")];

        assert_eq!(
            rmcp_call_tool_result_to_value(result).expect("double-padded image should decode"),
            json!({
                "content": [{"type": "image", "data": "TQ==", "mimeType": "image/png"}],
            })
        );
    }

    #[test]
    fn rmcp_call_tool_result_preserves_structured_content_with_image_envelope() {
        let mut result = CallToolResult::default();
        result.structured_content = Some(json!({"ok": true}));
        result.content = vec![rmcp::model::ContentBlock::image(
            "iVBORw0KGgo=",
            "image/png",
        )];

        assert_eq!(
            rmcp_call_tool_result_to_value(result).expect("structured media should decode"),
            json!({
                "content": [{"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"}],
                "structured_content": {"ok": true},
            })
        );
    }

    #[test]
    fn rmcp_call_tool_result_rejects_invalid_image_without_echoing_data() {
        let secret = "not-valid-image-data";
        let mut result = CallToolResult::default();
        result.content = vec![rmcp::model::ContentBlock::image(secret, "image/png")];

        let error = rmcp_call_tool_result_to_value(result)
            .expect_err("invalid image Base64 must be rejected")
            .to_string();
        assert!(error.contains("invalid Base64"));
        assert!(!error.contains(secret));
    }

    #[test]
    fn rmcp_call_tool_result_rejects_unsupported_image_mime_without_echoing_data() {
        let secret = "iVBORw0KGgo=";
        let mut result = CallToolResult::default();
        result.content = vec![rmcp::model::ContentBlock::image(secret, "image/svg+xml")];

        let error = rmcp_call_tool_result_to_value(result)
            .expect_err("unsupported image MIME must be rejected")
            .to_string();
        assert!(error.contains("unsupported MIME"));
        assert!(!error.contains(secret));
    }

    #[test]
    fn rmcp_call_tool_result_rejects_images_over_the_decoded_limit() {
        let oversized = "A".repeat((MCP_IMAGE_MAX_BYTES / 3 + 1) * 4);
        let mut result = CallToolResult::default();
        result.content = vec![rmcp::model::ContentBlock::image(&oversized, "image/png")];

        let error = rmcp_call_tool_result_to_value(result)
            .expect_err("oversized image must be rejected")
            .to_string();
        assert!(error.contains("20 MiB"));
        assert!(!error.contains(&oversized));
    }

    #[test]
    fn rmcp_prompt_capability_detection_requires_explicit_support() {
        assert!(!rmcp_capabilities_support_prompts(
            &ServerCapabilities::default()
        ));
        assert!(rmcp_capabilities_support_prompts(
            &ServerCapabilities::builder().enable_prompts().build()
        ));
    }

    #[test]
    fn rmcp_resource_capability_detection_requires_explicit_support() {
        assert!(!rmcp_capabilities_support_resources(
            &ServerCapabilities::default()
        ));
        assert!(rmcp_capabilities_support_resources(
            &ServerCapabilities::builder().enable_resources().build()
        ));
    }

    /// Verifies OpenAI-style file parameter metadata is parsed into a stable ordered field-name list.
    /// 验证 OpenAI 风格文件参数元数据会被解析为稳定有序的字段名列表。
    #[test]
    fn declared_openai_file_input_param_names_reads_non_empty_entries() {
        let meta = json!({
            "openai/fileParams": ["document", "attachments", "", 7, null]
        });

        assert_eq!(
            declared_openai_file_input_param_names(meta.as_object()),
            vec![String::from("document"), String::from("attachments")]
        );
    }

    /// Verifies model-visible MCP input schemas rewrite declared file-path parameters into scalar or array string contracts with explicit guidance text.
    /// 验证模型可见 MCP 输入 Schema 会把声明的文件路径参数改写为带明确指引文本的标量或数组字符串契约。
    #[test]
    fn model_visible_input_schema_masks_declared_file_path_params() {
        let input_schema = json!({
            "type": "object",
            "properties": {
                "document": {
                    "type": "object",
                    "description": "Upload one source document."
                },
                "attachments": {
                    "type": "array",
                    "items": {
                        "type": "object"
                    }
                },
                "query": {
                    "type": "string"
                }
            }
        });
        let meta = json!({
            "openai/fileParams": ["document", "attachments"]
        });

        let visible_schema = build_model_visible_tool_input_schema(
            input_schema
                .as_object()
                .expect("input schema should stay object"),
            meta.as_object(),
        );

        assert_eq!(
            visible_schema["properties"]["document"]["type"],
            Value::String(String::from("string"))
        );
        assert_eq!(
            visible_schema["properties"]["attachments"]["type"],
            Value::String(String::from("array"))
        );
        assert_eq!(
            visible_schema["properties"]["attachments"]["items"]["type"],
            Value::String(String::from("string"))
        );
        assert_eq!(
            visible_schema["properties"]["query"]["type"],
            Value::String(String::from("string"))
        );
        let document_description = visible_schema["properties"]["document"]["description"]
            .as_str()
            .expect("document description should stay string");
        assert!(document_description.contains("Upload one source document."));
        assert!(document_description.contains("absolute local file path"));
    }
}
