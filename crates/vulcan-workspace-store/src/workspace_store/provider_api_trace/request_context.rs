//! Bounded view of one actual Provider dispatch for the local conversation inspector.
//! 为本机会话检查面板提供一次实际供应商分派的有界视图。

use serde::Serialize;
use serde_json::Value;
use std::fmt;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use vulcan_session_core::v1::context::LockedSourceKind;
use vulcan_session_core::v1::event::ApiPurposeKind;

use super::{REDACTED_PROVIDER_SECRET, is_sensitive_json_key};
use std::collections::BTreeMap;

mod tool_nodes;
use tool_nodes::{ToolNodePart, project_tool_parts};

// Bound the snapshot independently of the model window, media bytes and archive trace settings.
// 独立于模型窗口、媒体字节及追踪设置限制快照大小。
const CONTENT_BUDGET: usize = 512 * 1024;
const NODE_CONTENT_BUDGET: usize = 24 * 1024;
const RESERVED_NODE_PREVIEW: usize = 1024;
const NODE_LIMIT: usize = 2048;
const LABEL_LIMIT: usize = 96;
/// Prefix fields acting as the system prompt across supported provider protocols.
/// 各支持的供应商协议用来承载系统提示词的前缀字段。
const SYSTEM_FIELDS: &[&str] = &[
    "instructions",
    "system",
    "systemInstruction",
    "system_instruction",
];

/// Semantic origin supplied by the canonical prefix renderer.
/// 由规范前缀渲染器提供的语义来源。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(untagged)]
pub enum RequestContextPromptKind {
    /// Instruction resolved on each request.
    /// 每次请求解析的固定指令。
    Fixed(RequestContextFixedPrompt),
    /// Instruction from the authoritative locked baseline.
    /// 来自权威锁定基线的指令。
    Locked(LockedSourceKind),
}

/// Product-level fixed instructions and their display categories.
/// 产品级固定指令及其展示分类。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestContextFixedPrompt {
    /// Base system and agent instructions.
    /// 基础系统与代理指令。
    SystemPrompt,
    /// Fixed developer instructions.
    /// 固定开发者指令。
    DeveloperPrompt,
}

/// One initial prompt fragment, transiently used to identify which origins reached the encoder output.
/// 一段初始提示词，临时用于识别进入编码输出的来源。
#[derive(Clone)]
pub struct RequestContextPromptSource {
    /// Stable source classification and label.
    /// 稳定的来源分类和标签。
    origin: PromptOrigin,
    /// Exact rendered body used for correlation, never serialized as source metadata.
    /// 用于关联的精确渲染正文，不作为来源元数据序列化。
    text: String,
}

impl fmt::Debug for RequestContextPromptSource {
    /// Formats structural metadata without exposing the body or source path.
    /// 格式化结构元数据，不暴露正文或来源路径。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestContextPromptSource")
            .field("kind", &self.origin.kind)
            .finish_non_exhaustive()
    }
}

impl RequestContextPromptSource {
    /// Associates a rendered fragment with its producer-declared kind and optional name.
    /// 将渲染片段与生产者声明的类型及可选名称关联。
    pub fn new(kind: RequestContextPromptKind, source: Option<String>, text: String) -> Self {
        // Producer wrappers have an exact closing tag; plain prompt text carries no wrapper metadata.
        // 生产者信封有精确闭合标签，普通提示词正文不携带信封元数据。
        let tag = outer_prompt_tag(&text).map(str::to_owned);
        Self {
            origin: PromptOrigin { kind, source, tag },
            text,
        }
    }
}

/// Safe source label published alongside the whole initial prompt preview.
/// 与完整初始提示词预览一同发布的安全来源标签。
#[derive(Debug, Clone, Serialize)]
struct PromptOrigin {
    /// Exact classification used by the inspector.
    /// 检查面板使用的精确分类。
    kind: RequestContextPromptKind,
    /// Optional producer label such as a plugin or project file.
    /// 插件或项目文件等可选生产者标签。
    source: Option<String>,
    /// Producer's exact outer tag, when the entire fragment is framed.
    /// 片段完整封装时的精确生产者外层标签。
    tag: Option<String>,
}

/// Returns the exact opening tag name when it encloses the entire prompt fragment.
/// 当标签涵盖整个提示词片段时返回精确的起始标签名称。
fn outer_prompt_tag(text: &str) -> Option<&str> {
    // The canonical tag names are simple identifiers followed by optional XML attributes.
    // 规范标签名称是简单标识符，后面可跟 XML 属性。
    let (opening, rest) = text.trim().strip_prefix('<')?.split_once('>')?;
    let tag = opening.split_ascii_whitespace().next()?;
    if !tag.as_bytes().first()?.is_ascii_alphabetic()
        || !tag
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, b'_' | b'-'))
    {
        return None;
    }
    (rest.strip_suffix('>')?.strip_suffix(tag)?.ends_with("</")).then_some(tag)
}

/// Stable dispatch facts supplied by the session owner and its resolved transport route.
/// 由会话所有者及其已解析传输路由提供的稳定分派事实。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRequestContextDispatch {
    /// Unique execution identifier for this physical attempt.
    /// 本次物理尝试的唯一执行标识。
    pub execution_id: String,
    /// V1 API identifier, serialized as a decimal string for lossless WebUI display.
    /// V1 API 标识，以十进制字符串序列化，供 WebUI 无损展示。
    pub api_call_id: String,
    /// Purpose recorded by the V1 preparation owner.
    /// V1 准备所有者记录的调用用途。
    pub purpose: ApiPurposeKind,
    /// Exact model identifier selected for this request.
    /// 为本次请求选定的精确模型标识。
    pub model: String,
    /// Exact protocol profile used to encode the request.
    /// 编码本次请求使用的精确协议 Profile。
    pub protocol: String,
    /// Local dispatch time in Unix milliseconds.
    /// 以 Unix 毫秒记录的本机分派时间。
    pub dispatched_at_ms: i64,
}

/// One node from a context-bearing field in the final encoded request.
/// 最终编码请求中承载上下文的一个字段节点。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderRequestContextNode {
    /// Actual top-level field containing the encoded node.
    /// 包含编码节点的实际顶层字段。
    field: &'static str,
    /// Zero-based wire array index when the field is an array of nodes.
    /// 字段为节点数组时，其基于零的传输数组下标。
    index: Option<usize>,
    /// Role present on the wire, when the protocol supplies one.
    /// 协议提供时，记录实际传输角色。
    role: Option<String>,
    /// Provider-specific node type, when encoded separately from the role.
    /// 与角色独立编码时，记录供应商节点类型。
    kind: Option<String>,
    /// Bounded, credential-filtered text of the node.
    /// 有界且已过滤凭据的节点文本。
    content: String,
    /// Decoded, bounded text built before raw JSON truncation.
    /// 在原始 JSON 截断前生成的已解码有界文本。
    text: Option<String>,
    /// Displayable reasoning that prefixes the assistant content or tool invocation.
    /// 助手正文或工具调用之前的可显示思考。
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<String>,
    /// Readable call and result blocks in source order.
    /// 依据来源顺序排列的可读调用及返回块。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_parts: Vec<ToolNodePart>,
    /// Whether this node's preview reached its display budget.
    /// 此节点的预览是否达到显示预算。
    truncated: bool,
}

/// Latest exact-dispatch inspector snapshot, separate from any persistent trace history.
/// 最近一次精确分派的检查快照，独立于持久化追踪历史。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRequestContext {
    /// Facts that identify this dispatch and its encoding.
    /// 用于标识本次分派及其编码的事实。
    pub dispatch: ProviderRequestContextDispatch,
    /// Number of encoded nodes including any omitted at the row limit.
    /// 编码节点数量，包含因行数上限而省略的节点。
    node_count: usize,
    /// Encoded context in protocol order, within the inspector budget.
    /// 检查面板预算内按协议顺序排列的编码上下文。
    nodes: Vec<ProviderRequestContextNode>,
    /// Ordered initial prompt origins proven present in the encoded request.
    /// 经确认已出现在编码请求中的有序初始提示词来源。
    prompt_origins: Vec<PromptOrigin>,
}

/// Bounded rows shared by physical dispatch and logical context previews.
/// 在物理派发与逻辑上下文预览之间共享的有界行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestContextRows {
    /// Number of nodes before applying the display limit.
    /// 应用显示上限前的节点数量。
    node_count: usize,
    /// Nodes retained for the inspector.
    /// 为检查面板保留的节点。
    nodes: Vec<ProviderRequestContextNode>,
    /// Ordered initial prompt origins for the displayed assembly.
    /// 当前展示装配的有序初始提示词来源。
    prompt_origins: Vec<PromptOrigin>,
}

/// Bounded visible assistant text from the session event matching the inspected request.
/// 与受检请求匹配的会话事件中有界的可见 assistant 正文。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestContextAssistantResponse {
    /// Plain text presented by the session.
    /// 会话展示的纯文本。
    content: String,
    /// Displayable reasoning emitted by the same physical response.
    /// 同一次实发响应产生的可显示思考。
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<String>,
    /// Whether the original text exceeded the node display budget.
    /// 原文是否超过节点显示预算。
    truncated: bool,
}

impl RequestContextAssistantResponse {
    /// Bounds the response body and its ordered reasoning prefix under the inspector node limit.
    /// 在检查器节点上限内限制响应正文及其有序思考前缀。
    pub fn from_text(text: &str, reasoning_prefix: &[&str]) -> Self {
        // A bounded copy keeps the RPC cost independent of long model responses.
        // 有界复制使 RPC 开销不依赖模型长回复的总长度。
        let end =
            text.floor_char_boundary(readable_body_limit(NODE_CONTENT_BUDGET, reasoning_prefix));
        // Give reasoning the unused part of the same node budget after preserving the answer.
        // 在保留回答后，将同一节点预算的剩余部分分配给思考。
        let reasoning = render_readable_context(reasoning_prefix, NODE_CONTENT_BUDGET - end, "");
        Self {
            content: text[..end].to_owned(),
            truncated: end < text.len()
                || reasoning_prefix
                    .iter()
                    .map(|text| text.len())
                    .sum::<usize>()
                    > reasoning.as_ref().map_or(0, String::len),
            reasoning,
        }
    }
}

/// Formats the simulated prompt, tools and conversation with the inspector's normal limits.
/// 使用检查面板的常规限制格式化模拟提示词、工具和会话节点。
pub fn render_simulated_request_context(
    body: &Value,
    prefix: &[RequestContextPromptSource],
) -> RequestContextRows {
    project_context_rows(
        body,
        ["baseline", "tools", "conversation"].into_iter(),
        "kind",
        "",
        prefix,
    )
}

/// Session-owned in-memory slot for the latest physical request.
/// 会话所有的最近一次物理请求内存槽。
#[derive(Default)]
pub(super) struct ProviderRequestContextSlot {
    /// Orders callbacks before they project their potentially different-sized bodies.
    /// 在各回调投影大小可能不同的请求正文前确定顺序。
    next_ticket: AtomicU64,
    /// The last published snapshot and the dispatch order that owns it.
    /// 最近发布的快照及其所属分派顺序。
    latest: Mutex<Option<(u64, Arc<ProviderRequestContext>)>>,
}

impl fmt::Debug for ProviderRequestContextSlot {
    /// Keeps prompt previews out of diagnostics for the containing session owner.
    /// 避免会话所有者的通用诊断带出提示词预览。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderRequestContextSlot")
            .finish_non_exhaustive()
    }
}

impl ProviderRequestContextSlot {
    /// Publishes a bounded view of `body` with the exact `credential` masked.
    /// 发布 `body` 的有界视图，并遮蔽精确 `credential`。
    pub(super) fn observe(
        &self,
        dispatch: ProviderRequestContextDispatch,
        body: &Value,
        credential: &str,
        prefix: &[RequestContextPromptSource],
    ) {
        // Claim before projection so a slow earlier attempt cannot replace a newer request.
        // 投影前领取序号，防止较慢的旧尝试覆盖新请求。
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        let snapshot = Arc::new(project_request_context(dispatch, body, credential, prefix));
        if let Ok(mut latest) = self.latest.lock()
            && latest
                .as_ref()
                .is_none_or(|(published, _)| *published < ticket)
        {
            *latest = Some((ticket, snapshot));
        }
    }

    /// Returns the current immutable snapshot without loading any session history.
    /// 返回当前不可变快照，无需加载会话历史。
    pub(super) fn latest(&self) -> Option<Arc<ProviderRequestContext>> {
        let latest = self.latest.lock().ok()?;
        latest.as_ref().map(|(_, snapshot)| Arc::clone(snapshot))
    }
}

/// Projects context-bearing fields in wire order and bounds the combined row content.
/// 按传输顺序投影上下文字段，并限制行内容总量。
fn project_request_context(
    dispatch: ProviderRequestContextDispatch,
    body: &Value,
    credential: &str,
    prefix: &[RequestContextPromptSource],
) -> ProviderRequestContext {
    // These keys come from the Responses, Chat, Messages, Gemini and decision request encoders.
    // 这些键来自 Responses、Chat、Messages、Gemini 及决策请求编码器。
    const PREFIX_FIELDS: &[&str] = &[
        "previous_response_id",
        "previous_interaction_id",
        "cachedContent",
        "instructions",
        "system",
        "systemInstruction",
        "system_instruction",
    ];
    const NODE_FIELDS: &[&str] = &["messages", "contents", "input", "questions", "tools"];
    let rows = project_context_rows(
        body,
        PREFIX_FIELDS.iter().chain(NODE_FIELDS).copied(),
        "type",
        credential,
        prefix,
    );
    ProviderRequestContext {
        dispatch,
        node_count: rows.node_count,
        nodes: rows.nodes,
        prompt_origins: rows.prompt_origins,
    }
}

/// Returns a bounded display projection for ordered fields and their explicit item discriminator.
/// 为有序字段及其明确的条目判别键返回有界展示投影。
fn project_context_rows(
    body: &Value,
    context_fields: impl Iterator<Item = &'static str> + Clone,
    kind_key: &str,
    credential: &str,
    prefix: &[RequestContextPromptSource],
) -> RequestContextRows {
    // Reserve space for later rows so an early tool log cannot hide every recent input.
    // 为后续行留出空间，防止前面的工具日志占满预算而遮住近期输入。
    let node_count = context_fields
        .clone()
        .filter_map(|field| body.get(field).filter(|value| !value.is_null()))
        .map(|value| value.as_array().map_or(1, Vec::len))
        .sum::<usize>();
    let visible_count = node_count.min(NODE_LIMIT);
    let reserved_per_row = (CONTENT_BUDGET / visible_count.max(1)).min(RESERVED_NODE_PREVIEW);
    let mut nodes = Vec::with_capacity(visible_count);
    let mut remaining = CONTENT_BUDGET;
    // Each source is reported at most once, and only after matching the actual system body.
    // 每个来源最多报告一次，且必须匹配实际系统正文。
    let mut matched_origins = vec![false; prefix.len()];
    // A result on this wire request can use its exact earlier call ID to recover the name.
    // 此传输请求中的工具返回可用其精确的先前调用标识恢复工具名。
    let mut tool_names = BTreeMap::new();
    let mut add = |field, index, value: &Value, preceding_reasoning: &[&str]| {
        if nodes.len() == visible_count {
            return false;
        }
        let reserved = (visible_count - nodes.len() - 1) * reserved_per_row;
        let limit = remaining.saturating_sub(reserved).min(NODE_CONTENT_BUDGET);
        let mut preview = NodePreview {
            text: String::new(),
            limit,
            truncated: false,
            credential,
        };
        preview.render(value, 0);
        remaining -= preview.text.len();
        // The provider shapes keep textual message parts beside the structured raw node.
        // 供应商结构在原始节点中保留独立的文本消息片段。
        let tool_parts = project_tool_parts(value, &mut tool_names, limit, credential);
        let text_parts = context_node_text_parts(value);
        // Wire reasoning has its own presentation, independently of JSON property order.
        // 传输记录中的思考独立展示，与 JSON 属性顺序无关。
        let reasoning_parts = context_node_reasoning_parts(value);
        // Separate Responses reasoning items belong only to the next model output item.
        // 独立的 Responses 思考项目仅属于后续模型输出项目。
        let reasoning_parts = if reasoning_parts.is_empty() {
            preceding_reasoning
        } else {
            &reasoning_parts
        };
        if SYSTEM_FIELDS.contains(&field)
            || matches!(
                value.get("role").and_then(Value::as_str),
                Some("system" | "developer")
            )
        {
            for (matched, source) in matched_origins.iter_mut().zip(prefix) {
                *matched |= !source.text.trim().is_empty()
                    && text_parts
                        .iter()
                        .any(|text| text.contains(source.text.trim()));
            }
        }
        // Raw and readable projections are independently bounded so incomplete JSON still has a text preview.
        // 原始与可读投影各自有界，保证不完整 JSON 也有正文预览。
        let text = render_readable_context(
            &text_parts,
            readable_body_limit(limit, reasoning_parts),
            credential,
        );
        // Bound the combined readable preview even when raw content was heavily truncated.
        // 即使原始内容严重截断，也对可读预览的合计大小设限。
        let reasoning = render_readable_context(
            reasoning_parts,
            limit.saturating_sub(text.as_ref().map_or(0, String::len)),
            credential,
        );
        // Reasoning can be truncated independently of the raw item when it came from a prior node.
        // 思考来自上一节点时，可能独立于当前原始项目被截断。
        let truncated = preview.truncated
            || reasoning_parts.iter().map(|text| text.len()).sum::<usize>()
                > reasoning.as_ref().map_or(0, String::len);
        nodes.push(ProviderRequestContextNode {
            field,
            index,
            role: value
                .get("role")
                .and_then(Value::as_str)
                .map(|role| redact_prefix(role, LABEL_LIMIT, credential)),
            kind: value
                .get(kind_key)
                .and_then(Value::as_str)
                .map(|kind| redact_prefix(kind, LABEL_LIMIT, credential)),
            content: preview.text,
            text,
            reasoning,
            tool_parts,
            truncated,
        });
        true
    };
    'fields: for field in context_fields {
        if let Some(value) = body.get(field).filter(|value| !value.is_null()) {
            if let Some(items) = value.as_array() {
                // Carry only immediately adjacent OpenAI Responses reasoning, within this input array.
                // 只在此输入数组中携带紧邻的 OpenAI Responses 思考。
                let mut pending_reasoning = Vec::new();
                // A later reasoning block starts a new prefix after the preceding call batch.
                // 前一批调用之后的思考块开始一段新前缀。
                let mut previous_was_call = false;
                for (index, item) in items.iter().enumerate() {
                    // Only Responses input has independent reasoning and function-call items.
                    // 只有 Responses 输入包含独立的思考和函数调用项目。
                    let item_type = item.get("type").and_then(Value::as_str);
                    if field == "input" && item_type == Some("reasoning") {
                        if previous_was_call {
                            pending_reasoning.clear();
                        }
                        previous_was_call = false;
                        pending_reasoning.extend(context_node_reasoning_parts(item));
                        if !add(field, Some(index), item, &[]) {
                            break 'fields;
                        }
                        continue;
                    }
                    // Keep the same reasoning prefix across the contiguous tool-call batch.
                    // 在连续工具调用批次中保留同一段思考前缀。
                    let function_call = field == "input" && is_responses_tool_call(item_type);
                    // A user message or tool result ends the association.
                    // 用户消息或工具结果会结束关联。
                    let receives_reasoning = function_call
                        || (field == "input"
                            && item.get("role").and_then(Value::as_str) == Some("assistant"));
                    if !add(
                        field,
                        Some(index),
                        item,
                        if receives_reasoning {
                            &pending_reasoning
                        } else {
                            &[]
                        },
                    ) {
                        break 'fields;
                    }
                    if !function_call {
                        pending_reasoning.clear();
                    }
                    previous_was_call = function_call;
                }
            } else if !add(field, None, value, &[]) {
                break;
            }
        }
    }
    // Redact source labels under the same credential rule as the node itself.
    // 来源标签沿用节点的凭据脱敏规则。
    let prompt_origins = prefix
        .iter()
        .zip(matched_origins)
        .filter(|(_, matched)| *matched)
        .map(|(source, _)| PromptOrigin {
            kind: source.origin.kind,
            source: source
                .origin
                .source
                .as_deref()
                .map(|label| redact_prefix(label, RESERVED_NODE_PREVIEW, credential)),
            tag: source
                .origin
                .tag
                .as_deref()
                .map(|tag| redact_prefix(tag, LABEL_LIMIT, credential)),
        })
        .collect();
    RequestContextRows {
        node_count,
        nodes,
        prompt_origins,
    }
}

/// Reads text parts from provider message, content block and simulated message shapes.
/// 从供应商消息、内容片段及模拟消息结构中读取文本片段。
fn context_node_text_parts(value: &Value) -> Vec<&str> {
    if value.get("type").and_then(Value::as_str) == Some("reasoning")
        || value.get("role").and_then(Value::as_str) == Some("tool")
    {
        return Vec::new();
    }
    if let Some(text) = value
        .as_str()
        .or_else(|| value.get("text").and_then(Value::as_str))
    {
        return vec![text];
    }
    if let Some(content) = value.get("content") {
        if let Some(text) = content.as_str() {
            return vec![text];
        }
        return content
            .as_array()
            .into_iter()
            .flatten()
            .filter(|part| part.get("type").and_then(Value::as_str) != Some("thinking"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect();
    }
    // Gemini systemInstruction and message contents expose their text as parts.
    // Gemini 的 systemInstruction 与消息正文通过 parts 提供文本。
    value
        .get("parts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("thought").and_then(Value::as_bool) != Some(true))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect()
}

/// Extracts readable reasoning from known projected and provider-encoded node shapes.
/// 从已知投影及供应商编码结构提取可读思考。
/// Empty, encrypted and signature-only blocks produce no visible text.
/// 空正文、加密片段和仅签名片段不产生可见文字。
fn context_node_reasoning_parts(value: &Value) -> Vec<&str> {
    match (
        value.get("role").and_then(Value::as_str),
        value.get("kind").and_then(Value::as_str),
        value.get("type").and_then(Value::as_str),
    ) {
        (_, Some("assistant_text"), _) => value
            .get("reasoning")
            .and_then(Value::as_str)
            .into_iter()
            .collect(),
        (_, _, Some("reasoning")) => ["summary", "content"]
            .into_iter()
            .filter_map(|field| value.get(field).and_then(Value::as_array))
            .flatten()
            .filter(|part| {
                matches!(
                    part.get("type").and_then(Value::as_str),
                    Some("summary_text" | "reasoning_text")
                )
            })
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .filter(|text| !text.trim().is_empty())
            .collect(),
        (Some("assistant"), _, _) => value
            .get("reasoning_content")
            .and_then(Value::as_str)
            .into_iter()
            .chain(
                value
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|part| part.get("type").and_then(Value::as_str) == Some("thinking"))
                    .filter_map(|part| part.get("thinking").and_then(Value::as_str)),
            )
            .filter(|text| !text.trim().is_empty())
            .collect(),
        (Some("model"), _, _) => value
            .get("parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|part| part.get("thought").and_then(Value::as_bool) == Some(true))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .filter(|text| !text.trim().is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Identifies the two tool invocation types emitted in Responses input.
/// 识别 Responses 输入中的两种工具调用类型。
fn is_responses_tool_call(item_type: Option<&str>) -> bool {
    matches!(item_type, Some("function_call" | "custom_tool_call"))
}

/// Reserves up to half of the node budget for the supplied thinking prefix.
/// 为给定思考前缀最多预留一半的节点预算。
fn readable_body_limit(limit: usize, reasoning: &[&str]) -> usize {
    limit
        - (limit / 2).min(
            reasoning.iter().map(|text| text.len()).sum::<usize>()
                + reasoning.len().saturating_sub(1) * 2,
        )
}

/// Renders readable text within the node budget using the same credential and binary filtering.
/// 在节点预算内生成可读文本，沿用相同的凭据与二进制过滤。
fn render_readable_context(parts: &[&str], limit: usize, credential: &str) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    // Use the same bounded writer to keep multi-part text within its per-row limit.
    // 用相同的有界写入器限制多片段正文的单行预算。
    let mut preview = NodePreview {
        text: String::new(),
        limit,
        credential,
        truncated: false,
    };
    for (index, text) in parts.iter().enumerate() {
        if preview.text.len() >= limit {
            break;
        }
        if index > 0 {
            preview.push("\n\n");
        }
        if text.starts_with("data:") {
            preview.push("{INLINE_MEDIA}");
        } else {
            let safe = redact_prefix(text, limit.saturating_sub(preview.text.len()), credential);
            preview.push(&safe);
        }
    }
    Some(preview.text)
}

/// Writes a bounded JSON-like preview without cloning or encoding the original node in full.
/// 写入有界的类 JSON 预览，无需完整克隆或编码原节点。
struct NodePreview<'a> {
    // The output buffer and its remaining display budget are owned by one row.
    // 输出缓冲区及显示预算由单行持有。
    text: String,
    limit: usize,
    // Indicates an omitted suffix or deeply nested subtree.
    // 标识已省略的后缀或深层子树。
    truncated: bool,
    // The exact credential is borrowed for masking and is never stored in the snapshot.
    // 仅借用精确凭据用于遮蔽，从不写入快照。
    credential: &'a str,
}

impl NodePreview<'_> {
    /// Appends at most the remaining number of UTF-8 bytes.
    /// 追加至多剩余的 UTF-8 字节数。
    fn push(&mut self, text: &str) {
        let end = text.floor_char_boundary(self.limit.saturating_sub(self.text.len()));
        self.text.push_str(&text[..end]);
        self.truncated |= end < text.len();
    }

    /// Writes one value and omits sensitive structured fields and inline binary data.
    /// 写入单个值，省略敏感结构字段及内联二进制数据。
    fn render(&mut self, value: &Value, depth: usize) {
        if depth == 32 || self.text.len() == self.limit {
            self.truncated = true;
            return;
        }
        match value {
            Value::String(text) if text.starts_with("data:") => self.push("\"{INLINE_MEDIA}\""),
            Value::String(text) => {
                // Inspect enough source to cover a credential spanning the preview cutoff.
                // 检查足够多的来源，覆盖跨越预览截止位置的凭据。
                let remaining = self.limit - self.text.len();
                let safe = redact_prefix(text, remaining, self.credential);
                self.truncated |= text.len() > remaining;
                if depth == 0 {
                    self.push(&safe);
                } else if let Ok(quoted) = serde_json::to_string(&safe) {
                    self.push(&quoted);
                }
            }
            Value::Array(items) => {
                self.push("[");
                for (index, item) in items.iter().enumerate() {
                    if self.text.len() == self.limit {
                        self.truncated = true;
                        break;
                    }
                    self.push(if index == 0 { "\n" } else { ",\n" });
                    self.push(&"  ".repeat(depth + 1));
                    self.render(item, depth + 1);
                }
                if !items.is_empty() {
                    self.push("\n");
                    self.push(&"  ".repeat(depth));
                }
                self.push("]");
            }
            Value::Object(fields) => {
                self.push("{");
                for (index, (key, child)) in fields.iter().enumerate() {
                    if self.text.len() == self.limit {
                        self.truncated = true;
                        break;
                    }
                    self.push(if index == 0 { "\n" } else { ",\n" });
                    self.push(&"  ".repeat(depth + 1));
                    // The key uses the remaining node budget, just like its value.
                    // 字段名与字段值一样使用节点的剩余预算。
                    let remaining = self.limit - self.text.len();
                    let safe_key = redact_prefix(key, remaining, self.credential);
                    self.truncated |= key.len() > remaining;
                    if let Ok(quoted) = serde_json::to_string(&safe_key) {
                        self.push(&quoted);
                    }
                    self.push(": ");
                    if is_sensitive_json_key(key) {
                        self.push("\"{APIKEY}\"");
                    } else if key == "encrypted_content"
                        || key == "file_data"
                        || (key == "data"
                            && (fields.contains_key("mime_type")
                                || fields.contains_key("mimeType")
                                || fields.get("type").and_then(Value::as_str) == Some("base64")))
                    {
                        self.push("\"{INLINE_DATA}\"");
                    } else {
                        self.render(child, depth + 1);
                    }
                }
                if !fields.is_empty() {
                    self.push("\n");
                    self.push(&"  ".repeat(depth));
                }
                self.push("}");
            }
            scalar => self.push(&scalar.to_string()),
        }
    }
}

/// Masks a credential before returning a UTF-8-safe display prefix.
/// 遮蔽凭据后返回 UTF-8 安全的显示前缀。
fn redact_prefix(text: &str, limit: usize, credential: &str) -> String {
    let end = text.floor_char_boundary(limit.saturating_add(credential.len()));
    let prefix = &text[..end];
    if credential.is_empty() {
        return prefix.to_owned();
    }
    let redacted = prefix.replace(credential, REDACTED_PROVIDER_SECRET);
    redacted[..redacted.floor_char_boundary(limit)].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Shows thinking for projected and wire tool calls without borrowing across unrelated messages.
    /// 展示投影及传输工具调用的思考，不跨无关消息借用。
    #[test]
    fn request_context_keeps_reasoning_with_its_assistant_or_tool_call() {
        // Chat carries inline reasoning; Responses carries independently ordered reasoning items.
        // Chat 内联思考；Responses 独立排序思考项目。
        let wire = json!({
            "messages": [
                {"role":"assistant","content":"answer","reasoning_content":"consider masked-key"},
                {"role":"assistant","content":null,"reasoning_content":"call reasoning","tool_calls":[{"id":"call-a","function":{"name":"lookup","arguments":"{}"}}]},
                {"role":"assistant","content":[{"type":"thinking","thinking":"anthropic reasoning"},{"type":"tool_use","id":"call-b"}]},
            ],
            "contents": [
                {"role":"model","parts":[{"thought":true,"text":"gemini reasoning"},{"text":"visible"}]},
            ],
            "input": [
                {"type":"reasoning","summary":[{"type":"summary_text","text":"shared tool reasoning"}]},
                {"type":"function_call","call_id":"call-c"},
                {"type":"custom_tool_call","call_id":"call-d"},
                {"type":"reasoning","content":[{"type":"reasoning_text","text":"next tool reasoning"}]},
                {"type":"function_call","call_id":"call-e"},
                {"type":"function_call_output","call_id":"call-c","output":"ok"},
                {"type":"message","role":"assistant","content":"unrelated"},
                {"type":"reasoning","encrypted_content":"opaque"},
                {"type":"message","role":"assistant","content":"no readable reasoning"},
            ],
        });
        // Use the same physical projection and assert the reason, call kind and visible body separately.
        // 使用同一物理投影，分别断言思考、调用类型和可见正文。
        let actual = project_request_context(dispatch("reasoning"), &wire, "masked-key", &[]);
        assert_eq!(actual.nodes[0].text.as_deref(), Some("answer"));
        assert_eq!(
            actual.nodes[0].reasoning.as_deref(),
            Some(format!("consider {REDACTED_PROVIDER_SECRET}").as_str())
        );
        assert!(actual.nodes[1].tool_parts.iter().any(|part| !part.result));
        assert_eq!(actual.nodes[1].reasoning.as_deref(), Some("call reasoning"));
        assert_eq!(
            actual.nodes[2].reasoning.as_deref(),
            Some("anthropic reasoning")
        );
        assert_eq!(actual.nodes[3].text.as_deref(), Some("visible"));
        assert_eq!(
            actual.nodes[3].reasoning.as_deref(),
            Some("gemini reasoning")
        );
        for node in actual
            .nodes
            .iter()
            .filter(|node| node.field == "input" && node.tool_parts.iter().any(|part| !part.result))
        {
            // A fresh reasoning block after calls starts its own prefix.
            // 调用之后出现的新思考块开始自己的前缀。
            let expected = if node.content.contains("\"call-e\"") {
                "next tool reasoning"
            } else {
                "shared tool reasoning"
            };
            assert_eq!(node.reasoning.as_deref(), Some(expected));
        }
        assert!(
            actual
                .nodes
                .iter()
                .filter(|node| node.field == "input" && node.role.as_deref() == Some("assistant"))
                .all(|node| node.reasoning.is_none())
        );

        // A simulated assistant item carries the tool payload and reasoning as sibling fields.
        // 模拟助手项目在同级字段中携带工具载荷和思考。
        let simulated = render_simulated_request_context(
            &json!({"conversation":[{"kind":"assistant_text","text":"","reasoning":"projected reasoning","tool_calls":[{"tool_name":"lookup"}]}]}),
            &[],
        );
        assert!(
            simulated.nodes[0]
                .tool_parts
                .iter()
                .any(|part| !part.result)
        );
        assert_eq!(
            simulated.nodes[0].reasoning.as_deref(),
            Some("projected reasoning")
        );
    }

    /// Verifies that tool returns use the exact call ID and decode model output separately.
    /// 验证工具返回使用精确调用 ID，并独立解码模型输出。
    #[test]
    fn request_context_names_calls_and_out_of_order_results() {
        // Return tool B before tool A to distinguish identity matching from positional matching.
        // 先返回工具 B，再返回工具 A，以区分按标识匹配与按位置匹配。
        let body = json!({"messages":[
            {"role":"assistant","content":null,"tool_calls":[
                {"id":"call-a","function":{"name":"skill","arguments":"{\"name\":\"fixture\"}"}},
                {"id":"call-b","function":{"name":"file_read","arguments":"{\"path\":\"a.md\"}"}}
            ]},
            {"role":"tool","tool_call_id":"call-b","content":"## Read result"},
            {"role":"tool","tool_call_id":"call-a","content":json!({
                "host_result":null,"is_error":false,"model_output":"## Skill result",
                "tool_call_id":"call-a","tool_name":"skill"
            }).to_string()}
        ]});
        // The display contains tool blocks, while the original wire envelope remains available.
        // 展示包含工具块，原始传输信封仍可查看。
        let snapshot =
            serde_json::to_value(project_request_context(dispatch("tools"), &body, "", &[]))
                .unwrap();
        // Check the exact order and distinct labels emitted by the inspector DTO.
        // 检查检查面板数据按精确顺序提供独立标签。
        let parts = snapshot["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|node| node["toolParts"].as_array().into_iter().flatten())
            .collect::<Vec<_>>();
        assert_eq!(
            parts
                .iter()
                .map(|part| {
                    (
                        part["name"].as_str().unwrap(),
                        part["result"].as_bool().unwrap(),
                    )
                })
                .collect::<Vec<_>>(),
            [
                ("skill", false),
                ("file_read", false),
                ("file_read", true),
                ("skill", true)
            ]
        );
        // Verify argument objects by value so pretty-printing can evolve independently.
        // 按值校验参数对象，使美化格式可独立演进。
        let arguments = parts
            .iter()
            .filter(|part| part["result"] == false)
            .map(|part| serde_json::from_str::<Value>(part["content"].as_str().unwrap()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            arguments,
            [json!({"name":"fixture"}), json!({"path":"a.md"})]
        );
        assert_eq!(
            parts
                .iter()
                .filter(|part| part["result"] == true)
                .map(|part| { part["content"].as_str().unwrap() })
                .collect::<Vec<_>>(),
            ["## Read result", "## Skill result"]
        );
    }

    /// Builds the minimal facts for one synthetic dispatch.
    /// 构建一次模拟分派的最小事实。
    fn dispatch(execution_id: &str) -> ProviderRequestContextDispatch {
        ProviderRequestContextDispatch {
            execution_id: execution_id.into(),
            api_call_id: "1".into(),
            purpose: ApiPurposeKind::MainGeneration,
            model: "fixture".into(),
            protocol: "responses".into(),
            dispatched_at_ms: 1,
        }
    }

    /// Shows actual incremental input in wire order and keeps only the newest bounded snapshot.
    /// 按传输顺序展示实际增量输入，并只保留最新的有界快照。
    #[test]
    fn latest_context_uses_wire_nodes_and_masks_secrets() {
        let slot = ProviderRequestContextSlot::default();
        // The non-ASCII fixture verifies UTF-8 preservation independently of UI wording.
        // 非 ASCII 夹具验证 UTF-8 内容保留，与界面文案无关。
        let user_text = "中文";
        assert!(slot.latest().is_none());
        slot.observe(
            dispatch("previous"),
            &json!({"messages":[{"role":"user","content":"old"}]}),
            "",
            &[],
        );
        slot.observe(dispatch("current"), &json!({
            "previous_response_id":"cursor", "instructions":"instructions", "input":[
                    {"role":"user","content":format!("{user_text} credential-value")},
                {"type":"function_call_output","output":"x".repeat(NODE_CONTENT_BUDGET),"api_key":"masked-by-key"},
            ]
        }), "credential-value", &[]);
        let latest = slot.latest().unwrap();
        assert_eq!(latest.dispatch.execution_id, "current");
        assert_eq!(
            latest
                .nodes
                .iter()
                .map(|node| node.field)
                .collect::<Vec<_>>(),
            ["previous_response_id", "instructions", "input", "input"]
        );
        assert!(latest.nodes[2].content.contains(user_text));
        assert!(!latest.nodes[2].content.contains("credential-value"));
        assert!(!latest.nodes[3].content.contains("masked-by-key"));
        assert!(latest.nodes[3].truncated);
    }

    /// Verifies origins follow encoded system text and readable content survives raw JSON clipping.
    /// 验证来源依照已编码系统正文，且原始 JSON 截短时仍可预览正文。
    #[test]
    fn request_context_projects_readable_text_and_verified_origins() {
        // Construct one framed memory origin and one origin omitted by the encoder.
        // 构造一个已封装的记忆来源和一个被编码器省略的来源。
        let profile = "<vmm_profile>fixture</vmm_profile>";
        let prefix = [
            RequestContextPromptSource::new(
                RequestContextPromptKind::Locked(LockedSourceKind::MemoryProfile),
                Some("VMM".into()),
                profile.into(),
            ),
            RequestContextPromptSource::new(
                RequestContextPromptKind::Locked(LockedSourceKind::AgentPersona),
                None,
                "omitted".into(),
            ),
        ];
        let body = json!({ "messages": [{
            "role": "system", "content": format!("{profile}\n\n## fixture\n{}", "\n".repeat(NODE_CONTENT_BUDGET))
        }, {
            "role": "user", "content": "omitted"
        }] });
        let projected = project_request_context(dispatch("fixture"), &body, "", &prefix);
        assert_eq!(projected.prompt_origins.len(), 1);
        assert_eq!(
            projected.prompt_origins[0].tag.as_deref(),
            Some("vmm_profile")
        );
        let system = projected
            .nodes
            .iter()
            .find(|node| node.role.as_deref() == Some("system"))
            .unwrap();
        assert!(system.truncated);
        assert!(system.text.as_deref().unwrap().contains("\n## fixture\n"));
        assert!(system.content.contains("\\n## fixture\\n"));
    }
}
