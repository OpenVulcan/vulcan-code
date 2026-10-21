//! Hook-payload projection and VMM context-injection conversion.
//! Hook payload 投影与 VMM 上下文注入转换。

use crate::config::{VmmBackendMode, VmmEffectiveConfig};
use crate::protocol::{
    ContextItem, PostActionRequest, PostActionTimelineItem, PostActionTimelineType,
    PreCheckRequest, PreCheckResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fmt::{self, Display};
use vulcan_context_injection::{
    InjectionCompactionPolicy, InjectionInsert, InjectionInsertItem, InjectionItemRenderer,
    InjectionKind, InjectionScope, InjectionStateItem,
};

/// Maximum user content characters sent to VMM.
/// 发送给 VMM 的用户内容最大字符数。
const USER_CONTENT_LIMIT: usize = 4_000;

/// Maximum assistant content characters sent to VMM.
/// 发送给 VMM 的助手内容最大字符数。
const ASSISTANT_CONTENT_LIMIT: usize = 6_000;

/// Maximum middle timeline content characters sent to VMM.
/// 发送给 VMM 的中间时间线内容最大字符数。
const TIMELINE_CONTENT_LIMIT: usize = 1_600;

/// Renderer id used for VMM precheck memory injection items.
/// VMM precheck 记忆注入项使用的渲染器标识。
pub const VMM_PRECHECK_MEMORY_RENDERER_ID: &str = "vmm.precheck.memory";

/// Metadata key for VMM precheck memory text.
/// VMM precheck 记忆文本的元数据键。
const VMM_METADATA_TEXT: &str = "text";

/// Metadata key for VMM relevance score.
/// VMM 相关性分数的元数据键。
const VMM_METADATA_SCORE: &str = "score";

/// Metadata key for source turn id.
/// 来源 turn 标识的元数据键。
const VMM_METADATA_TURN_ID: &str = "turn_id";

/// Metadata key for source dialogue availability.
/// 来源对话可回查性的元数据键。
const VMM_METADATA_HAS_DIALOGUE: &str = "has_dialogue";

/// Metadata key for VMM creation datetime.
/// VMM 创建时间的元数据键。
const VMM_METADATA_CREATED_DATETIME: &str = "created_datetime";

/// Metadata key for durable VMM memory id.
/// 长期 VMM 记忆标识的元数据键。
const VMM_METADATA_MEMORY_ID: &str = "memory_id";

/// Error returned while projecting hook payload into VMM requests.
/// 将 hook payload 投影为 VMM 请求时返回的错误。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmAdapterError {
    /// Stable error kind.
    /// 稳定错误类型。
    pub kind: String,
    /// Human-readable error message.
    /// 人类可读错误信息。
    pub message: String,
}

impl VmmAdapterError {
    /// Builds one adapter error.
    /// 构建一个适配器错误。
    fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

impl Display for VmmAdapterError {
    /// Formats the adapter error for hook diagnostics.
    /// 为 hook 诊断格式化适配器错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for VmmAdapterError {}

/// Hook timeline item normalized for VMM projection decisions.
/// 为 VMM 投影决策归一化后的 hook 时间线项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmHookTimelineItem {
    /// Generic hook role such as user, assistant, or tool.
    /// 通用 hook 角色，例如 user、assistant 或 tool。
    pub role: String,
    /// Generic hook event kind such as user_followup or ask_answer.
    /// 通用 hook 事件类型，例如 user_followup 或 ask_answer。
    pub kind: String,
    /// Compact event content.
    /// 精简后的事件内容。
    pub content: String,
    /// Optional source id from hook payload.
    /// hook payload 中的可选来源标识。
    pub source_id: Option<String>,
}

/// Completed turn projection used before building PostAction.
/// 构建 PostAction 前使用的已完成回合投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmCompletedTurnProjection {
    /// Host session identifier.
    /// 宿主会话标识。
    pub session_id: String,
    /// First user prompt.
    /// 首个用户提示词。
    pub user_content: String,
    /// Final assistant answer.
    /// 最终助手回答。
    pub assistant_content: String,
    /// VMM middle timeline items.
    /// VMM 中间时间线项。
    pub timeline: Vec<PostActionTimelineItem>,
}

/// Builds a VMM PreCheck request from a precheck hook payload.
/// 从 precheck hook payload 构建 VMM PreCheck 请求。
pub fn precheck_request_from_payload(
    payload: &Value,
    config: &VmmEffectiveConfig,
) -> Result<PreCheckRequest, VmmAdapterError> {
    let session_id = required_string(payload, "session_id")?;
    let user_content = required_string(payload, "user_content")?;
    Ok(PreCheckRequest {
        operation_id: required_string(payload, "turn_id")?,
        session_id,
        user_id: required_config_id(config.user_id, "user_id")?,
        project_id: required_config_id(config.project_id, "project_id")?,
        host_project_id: optional_string(payload, "project_id").unwrap_or_default(),
        user_content: compact_text(&user_content, USER_CONTENT_LIMIT),
        recall_mode: config.precheck_recall_mode(),
    })
}

/// Builds a VMM PostAction request from a completed-turn hook payload.
/// 从 completed-turn hook payload 构建 VMM PostAction 请求。
pub fn post_action_request_from_completed_payload(
    payload: &Value,
    config: &VmmEffectiveConfig,
) -> Result<PostActionRequest, VmmAdapterError> {
    let projection = completed_turn_projection_from_payload(payload, config.backend_mode)?;
    Ok(PostActionRequest {
        expected_source: None,
        operation_id: required_string(payload, "turn_id")?,
        session_id: projection.session_id,
        user_id: required_config_id(config.user_id, "user_id")?,
        project_id: required_config_id(config.project_id, "project_id")?,
        host_project_id: optional_string(payload, "project_id").unwrap_or_default(),
        user_content: projection.user_content,
        assistant_content: projection.assistant_content,
        timeline: projection.timeline,
    })
}

/// Builds a VMM ChatCompact request from a compaction hook payload.
/// 从压缩 hook payload 构建 VMM ChatCompact 请求。
pub fn chat_compact_request_from_payload(
    payload: &Value,
    config: &VmmEffectiveConfig,
) -> Result<crate::protocol::ChatCompactRequest, VmmAdapterError> {
    Ok(crate::protocol::ChatCompactRequest {
        operation_id: String::new(),
        session_id: required_string(payload, "session_id")?,
        user_id: required_config_id(config.user_id, "user_id")?,
        project_id: required_config_id(config.project_id, "project_id")?,
        host_project_id: optional_string(payload, "project_id").unwrap_or_default(),
    })
}

/// Converts a VMM PreCheck response into a transient context injection.
/// 将 VMM PreCheck 响应转换成临时上下文注入。
pub fn injection_from_precheck_response(
    response: &PreCheckResponse,
    config: &VmmEffectiveConfig,
) -> Option<InjectionInsert> {
    if !response.session_memory_status.allows_runtime_memory() || !response.should_inject {
        return None;
    }
    let items = response
        .context_items
        .iter()
        .filter_map(vmm_context_item_to_injection_item)
        .map(|item| InjectionInsertItem {
            ttl_turns: Some(config.implicit_memory_turns),
            compaction_policy: Some(InjectionCompactionPolicy::Keep),
            source_display: Some(String::from("VMM")),
            ..item
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return None;
    }
    Some(InjectionInsert {
        kind: InjectionKind::Transient,
        tag: config.injection_tag.clone(),
        description: config.injection_description.clone(),
        items,
        ttl_turns: Some(config.implicit_memory_turns),
        compaction_policy: InjectionCompactionPolicy::Keep,
        scope: InjectionScope::Session,
        priority: -50,
        source_display: Some(String::from("VMM")),
    })
}

/// Builds a completed-turn projection from generic hook JSON.
/// 从通用 hook JSON 构建已完成回合投影。
fn completed_turn_projection_from_payload(
    payload: &Value,
    backend_mode: VmmBackendMode,
) -> Result<VmmCompletedTurnProjection, VmmAdapterError> {
    let session_id = required_string(payload, "session_id")?;
    // Native persistence owns cleaning and token clipping; do not discard line structure before it.
    // 原生持久化负责清洗及 Token 裁剪；在此之前不能丢弃行结构。
    let user_content =
        postaction_content(payload, "user_content", backend_mode, USER_CONTENT_LIMIT).ok_or_else(
            || {
                VmmAdapterError::new(
                    "missing_user_content",
                    "completed-turn payload does not contain user_content",
                )
            },
        )?;
    // Keep the final assistant boundary independent from middle ask interactions.
    // 最终助手回答边界与中间 ask 交互保持独立。
    let assistant_content = postaction_content(
        payload,
        "assistant_content",
        backend_mode,
        ASSISTANT_CONTENT_LIMIT,
    )
    .ok_or_else(|| {
        VmmAdapterError::new(
            "missing_assistant_content",
            "completed-turn payload does not contain assistant_content",
        )
    })?;
    let timeline = parse_hook_timeline(payload.get("timeline"), backend_mode)
        .into_iter()
        .filter_map(project_hook_timeline_item)
        .collect::<Vec<_>>();
    if user_content.is_empty() {
        return Err(VmmAdapterError::new(
            "missing_user_content",
            "completed-turn payload does not contain user_content",
        ));
    }
    if assistant_content.is_empty() {
        return Err(VmmAdapterError::new(
            "missing_assistant_content",
            "completed-turn payload does not contain assistant_content",
        ));
    }
    Ok(VmmCompletedTurnProjection {
        session_id,
        user_content,
        assistant_content,
        timeline: dedupe_adjacent_timeline(timeline),
    })
}

/// Parses hook timeline values into normalized items.
/// 将 hook timeline 值解析为归一化条目。
fn parse_hook_timeline(
    value: Option<&Value>,
    backend_mode: VmmBackendMode,
) -> Vec<VmmHookTimelineItem> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| parse_hook_timeline_item(item, backend_mode))
                .collect()
        })
        .unwrap_or_default()
}

/// Parses one hook timeline item.
/// 解析一条 hook 时间线项。
fn parse_hook_timeline_item(
    value: &Value,
    backend_mode: VmmBackendMode,
) -> Option<VmmHookTimelineItem> {
    let role = optional_string(value, "role")?;
    let kind = optional_string(value, "kind")?;
    let content = postaction_content(value, "content", backend_mode, TIMELINE_CONTENT_LIMIT)?;
    if content.is_empty() {
        return None;
    }
    Some(VmmHookTimelineItem {
        role,
        kind,
        content,
        source_id: optional_string(value, "source_id"),
    })
}

/// Reads one nonblank PostAction field; native preserves raw layout while external retains its transport cap.
/// 读取一个非空 PostAction 字段；原生保留原始排版，外置保留既有传输上限。
/// `key` names the exact payload field, `limit` applies only to ExternalGrpc; absent/blank values return None.
/// `key` 为准确载荷字段名，`limit` 仅作用于 ExternalGrpc；缺失或空白值返回 None。
fn postaction_content(
    payload: &Value,
    key: &str,
    backend_mode: VmmBackendMode,
    limit: usize,
) -> Option<String> {
    // Mode comes from the saved effective configuration, never from payload shape or connection fields.
    // 模式来自已保存的有效配置，绝不按载荷形状或连接字段猜测。
    match backend_mode {
        VmmBackendMode::Native => payload
            .get(key)
            .and_then(Value::as_str)
            .filter(|content| !content.trim().is_empty())
            .map(str::to_owned),
        VmmBackendMode::ExternalGrpc => {
            optional_string(payload, key).map(|content| compact_text(&content, limit))
        }
    }
}

/// Projects one generic hook timeline item into the VMM middle timeline contract.
/// 将一条通用 hook 时间线项投影到 VMM 中间时间线契约。
fn project_hook_timeline_item(item: VmmHookTimelineItem) -> Option<PostActionTimelineItem> {
    let kind = match (item.role.as_str(), item.kind.as_str()) {
        ("user", "user_followup" | "user_interrupt") => PostActionTimelineType::User,
        ("assistant", "ask") => PostActionTimelineType::Assistant,
        ("user", "ask_answer" | "ask_rejected") => PostActionTimelineType::User,
        _ => return None,
    };
    Some(PostActionTimelineItem {
        kind,
        content: item.content,
    })
}

/// Removes only adjacent duplicate VMM timeline items.
/// 只移除相邻重复的 VMM 时间线项。
fn dedupe_adjacent_timeline(timeline: Vec<PostActionTimelineItem>) -> Vec<PostActionTimelineItem> {
    let mut deduped = Vec::new();
    let mut seen_adjacent = BTreeSet::new();
    for item in timeline {
        let key = format!("{}:{}", item.kind.as_wire_type(), item.content);
        if seen_adjacent.contains(&key) {
            continue;
        }
        seen_adjacent.clear();
        seen_adjacent.insert(key);
        deduped.push(item);
    }
    deduped
}

/// Builds the renderer used for VMM precheck memory items.
/// 构建 VMM precheck 记忆内容项使用的渲染器。
pub fn vmm_precheck_memory_renderer() -> InjectionItemRenderer {
    InjectionItemRenderer {
        render_model: render_vmm_precheck_memory_model,
        render_panel: render_vmm_precheck_memory_panel,
        render_detail: render_vmm_precheck_memory_detail,
    }
}

/// Converts one PreCheck context item into a structured injection item.
/// 将一条 PreCheck 上下文项转换为结构化注入内容项。
fn vmm_context_item_to_injection_item(item: &ContextItem) -> Option<InjectionInsertItem> {
    let text = collapse_context_item_text(&item.text);
    if text.is_empty() {
        return None;
    }
    let metadata = json!({
        VMM_METADATA_TEXT: item.text,
        VMM_METADATA_SCORE: item.score,
        VMM_METADATA_TURN_ID: item.turn_id,
        VMM_METADATA_HAS_DIALOGUE: item.has_dialogue,
        VMM_METADATA_CREATED_DATETIME: item.created_datetime,
        VMM_METADATA_MEMORY_ID: item.memory_id,
    });
    let item_identity = if item.memory_id > 0 {
        format!("memory:{}", item.memory_id)
    } else if item.has_dialogue && item.turn_id > 0 {
        format!("turn:{}", item.turn_id)
    } else {
        String::from("noturn")
    };
    let dedupe_key = format!("{VMM_PRECHECK_MEMORY_RENDERER_ID}:{item_identity}:{text}");
    Some(InjectionInsertItem {
        content: text,
        renderer_id: Some(String::from(VMM_PRECHECK_MEMORY_RENDERER_ID)),
        metadata: Some(metadata),
        dedupe_key: Some(dedupe_key),
        ..InjectionInsertItem::new(String::new())
    })
}

/// Renders one VMM precheck memory item for provider-visible model context.
/// 为模型可见上下文渲染一条 VMM precheck 记忆项。
fn render_vmm_precheck_memory_model(item: &InjectionStateItem) -> String {
    render_vmm_context_item_model(&vmm_context_item_from_injection_item(item))
}

/// Renders one compact VMM precheck memory panel row.
/// 渲染一条紧凑的 VMM precheck 记忆面板行。
fn render_vmm_precheck_memory_panel(item: &InjectionStateItem) -> String {
    render_vmm_context_item_panel(&vmm_context_item_from_injection_item(item))
}

/// Renders one detailed VMM precheck memory tooltip body.
/// 渲染一段详细的 VMM precheck 记忆悬停正文。
fn render_vmm_precheck_memory_detail(item: &InjectionStateItem) -> String {
    render_vmm_context_item_detail(&vmm_context_item_from_injection_item(item))
}

/// Renders one structured VMM context item into immutable provider-visible text.
/// 将一条结构化 VMM 上下文项渲染为不可变的 Provider 可见文本。
///
/// # Parameters
/// 参数
/// `item` is the recalled VMM context item whose semantic payload must be frozen.
/// `item` 是需要冻结语义载荷的 VMM 召回上下文项。
///
/// # Returns
/// 返回值
/// A single-line provider block preserving the established VMM metadata format.
/// 返回保留既有 VMM 元数据格式的单行 Provider 文本块。
pub fn render_vmm_context_item_model(item: &ContextItem) -> String {
    let text = collapse_context_item_text(&item.text);
    if text.is_empty() {
        return String::new();
    }
    let turn_label = format_vmm_context_item_turn_label(item);
    let source_score = format_context_source_score(item.score);
    let created_time = format_context_created_time(&item.created_datetime)
        .map(|time| format!("|TIME:{time}"))
        .unwrap_or_default();
    let memory_label = if item.memory_id > 0 {
        format!("|MEMORY_ID:{}", item.memory_id)
    } else {
        String::new()
    };
    format!("[{turn_label}{memory_label}|SOURCE:{source_score}{created_time}]{text}")
}

/// Renders one structured VMM context item for the compact memory panel.
/// 为紧凑记忆面板渲染一条结构化 VMM 上下文项。
///
/// # Parameters
/// 参数
/// `item` is the recalled VMM context item shown in the panel.
/// `item` 是需要在面板中展示的 VMM 召回上下文项。
///
/// # Returns
/// 返回值
/// Normalized memory text suitable for one compact panel row.
/// 返回适合单条紧凑面板行的规范化记忆文本。
pub fn render_vmm_context_item_panel(item: &ContextItem) -> String {
    collapse_context_item_text(&item.text)
}

/// Renders one structured VMM context item for the detailed memory tooltip.
/// 为记忆详情悬停框渲染一条结构化 VMM 上下文项。
///
/// # Parameters
/// 参数
/// `item` is the recalled VMM context item whose metadata is displayed.
/// `item` 是需要展示元数据的 VMM 召回上下文项。
///
/// # Returns
/// 返回值
/// A multiline English detail body matching the existing memory-panel contract.
/// 返回符合既有记忆面板契约的多行英文详情正文。
pub fn render_vmm_context_item_detail(item: &ContextItem) -> String {
    let mut lines = Vec::new();
    let text = render_vmm_context_item_panel(item);
    if !text.is_empty() {
        lines.push(format!("Content: {text}"));
    }
    if let Some(created_datetime) = format_context_created_time(&item.created_datetime) {
        lines.push(format!("Time: {created_datetime}"));
    }
    if item.memory_id > 0 {
        lines.push(format!("Memory ID: {}", item.memory_id));
    }
    lines.push(format!(
        "Source: {}",
        format_vmm_context_item_turn_label(item)
    ));
    lines.push(format!(
        "Relevance: {}",
        format_context_source_score(item.score)
    ));
    lines.push(format!(
        "Traceable: {}",
        if item.has_dialogue { "yes" } else { "no" }
    ));
    lines.join("\n")
}

/// Builds a structured VMM context item from one renderer-owned generic injection item.
/// 从一条归属于渲染器的通用注入项构建结构化 VMM 上下文项。
///
/// # Parameters
/// 参数
/// `item` is an item whose renderer id already established VMM ownership.
/// `item` 是已由渲染器标识确认归属于 VMM 的注入项。
///
/// # Returns
/// 返回值
/// The exact structured fields understood by the VMM renderer.
/// 返回 VMM 渲染器所理解的精确结构化字段。
pub(crate) fn vmm_context_item_from_injection_item(item: &InjectionStateItem) -> ContextItem {
    ContextItem {
        text: vmm_metadata_text(item),
        score: vmm_metadata_score(item),
        turn_id: vmm_metadata_turn_id(item),
        has_dialogue: vmm_metadata_has_dialogue(item),
        created_datetime: vmm_metadata_created_datetime(item).unwrap_or_default(),
        memory_id: vmm_metadata_memory_id(item),
    }
}

/// Builds the dialogue lookup marker from one structured VMM item.
/// 从一条结构化 VMM 项构建对话回查标记。
///
/// # Parameters
/// 参数
/// `item` is the VMM context item whose source marker is needed.
/// `item` 是需要生成来源标记的 VMM 上下文项。
///
/// # Returns
/// 返回值
/// `TURN_ID:<id>` for expandable dialogue rows, otherwise `NOTURN`.
/// 对可展开对话行返回 `TURN_ID:<id>`，否则返回 `NOTURN`。
fn format_vmm_context_item_turn_label(item: &ContextItem) -> String {
    if item.has_dialogue && item.turn_id > 0 {
        format!("TURN_ID:{}", item.turn_id)
    } else {
        String::from("NOTURN")
    }
}

/// Formats one VMM relevance score for provider-visible context.
/// 为模型可见上下文格式化一个 VMM 相关性分数。
fn format_context_source_score(score: f64) -> String {
    if score.is_finite() {
        format!("{score:.3}")
    } else {
        String::from("0.000")
    }
}

/// Normalizes one optional VMM creation datetime for inline metadata.
/// 为行内元数据规范化一个可选的 VMM 创建时间。
fn format_context_created_time(value: &str) -> Option<String> {
    let normalized = collapse_context_item_text(value);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// Reads structured VMM memory text from one injection item.
/// 从注入内容项读取结构化 VMM 记忆文本。
fn vmm_metadata_text(item: &InjectionStateItem) -> String {
    item.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(VMM_METADATA_TEXT))
        .and_then(Value::as_str)
        .map(collapse_context_item_text)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| item.content.clone())
}

/// Reads structured VMM relevance score from one injection item.
/// 从注入内容项读取结构化 VMM 相关性分数。
fn vmm_metadata_score(item: &InjectionStateItem) -> f64 {
    item.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(VMM_METADATA_SCORE))
        .and_then(Value::as_f64)
        .unwrap_or_default()
}

/// Reads structured VMM source turn id from one injection item.
/// 从注入内容项读取结构化 VMM 来源 turn 标识。
fn vmm_metadata_turn_id(item: &InjectionStateItem) -> u64 {
    item.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(VMM_METADATA_TURN_ID))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

/// Reads structured VMM memory id from one injection item.
/// 从注入内容项读取结构化 VMM 记忆标识。
fn vmm_metadata_memory_id(item: &InjectionStateItem) -> u64 {
    item.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(VMM_METADATA_MEMORY_ID))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

/// Reads structured VMM dialogue availability from one injection item.
/// 从注入内容项读取结构化 VMM 对话可回查性。
fn vmm_metadata_has_dialogue(item: &InjectionStateItem) -> bool {
    item.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(VMM_METADATA_HAS_DIALOGUE))
        .and_then(Value::as_bool)
        .unwrap_or_default()
}

/// Reads structured VMM creation datetime from one injection item.
/// 从注入内容项读取结构化 VMM 创建时间。
fn vmm_metadata_created_datetime(item: &InjectionStateItem) -> Option<String> {
    item.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(VMM_METADATA_CREATED_DATETIME))
        .and_then(Value::as_str)
        .and_then(format_context_created_time)
}

/// Collapses a recalled context field into one compact inline string.
/// 将召回上下文字段压缩为一条紧凑的行内字符串。
fn collapse_context_item_text(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a required string field from JSON.
/// 从 JSON 中读取必填字符串字段。
fn required_string(payload: &Value, key: &str) -> Result<String, VmmAdapterError> {
    optional_string(payload, key).ok_or_else(|| {
        VmmAdapterError::new(
            format!("missing_{key}"),
            format!("payload does not contain a non-empty {key} string"),
        )
    })
}

/// Reads an optional string field from JSON.
/// 从 JSON 中读取可选字符串字段。
fn optional_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty())
}

/// Requires one resolved numeric config id.
/// 要求一个已解析的数字配置标识。
fn required_config_id(value: Option<u64>, key: &str) -> Result<u64, VmmAdapterError> {
    value.filter(|id| *id > 0).ok_or_else(|| {
        VmmAdapterError::new(
            format!("missing_{key}"),
            format!("effective VMM config does not contain a valid {key}"),
        )
    })
}

/// Compacts text to one character limit after normalization.
/// 归一化后按字符上限压缩文本。
fn compact_text(value: &str, limit: usize) -> String {
    let normalized = normalize_text(value);
    if normalized.chars().count() <= limit {
        return normalized;
    }
    let mut compacted = normalized
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    compacted.push('…');
    compacted
}

/// Normalizes text whitespace while preserving paragraph boundaries.
/// 规范化文本空白，同时保留段落边界。
fn normalize_text(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    //! Tests for VMM hook adapter projection.
    //! VMM hook 适配投影测试。

    use super::*;
    use crate::config::VmmConfigLayer;
    use serde_json::json;

    /// Builds one ready effective config for adapter tests.
    /// 为适配器测试构建一个可用最终配置。
    fn ready_config() -> VmmEffectiveConfig {
        VmmEffectiveConfig::resolve(
            &VmmConfigLayer {
                grpc_target: Some(String::from("127.0.0.1:50051")),
                user_id: Some(1),
                project_id: Some(2),
                implicit_memory_turns: Some(5),
                ..VmmConfigLayer::default()
            },
            None,
            None,
        )
    }

    #[test]
    fn precheck_payload_builds_request_with_compact_recall_mode() {
        let mut config = ready_config();
        config.session_compact_recall = true;
        let payload = json!({
            "turn_id": "turn-a",
            "session_id": "session-a",
            "user_content": " hello "
        });

        let request = precheck_request_from_payload(&payload, &config).expect("request");

        assert_eq!(request.session_id, "session-a");
        assert_eq!(request.user_id, 1);
        assert_eq!(request.project_id, 2);
        assert_eq!(
            request.recall_mode,
            crate::protocol::PreCheckRecallMode::PreCheckRecallModeSessionCompact
        );
    }

    #[test]
    fn completed_payload_projects_only_vmm_middle_timeline() {
        let payload = json!({
            "turn_id": "turn-a",
            "session_id": "session-a",
            "user_content": "first user",
            "assistant_content": "final answer",
            "timeline": [
                { "role": "tool", "kind": "tool_started", "content": "shell" },
                { "role": "user", "kind": "user_interrupt", "content": "extra user" },
                { "role": "assistant", "kind": "ask", "content": "need choice?" },
                { "role": "user", "kind": "ask_answer", "content": "choice A" },
                { "role": "tool", "kind": "tool_finished", "content": "done" }
            ]
        });

        let request =
            post_action_request_from_completed_payload(&payload, &ready_config()).expect("request");

        assert_eq!(request.timeline.len(), 3);
        assert_eq!(request.timeline[0].kind, PostActionTimelineType::User);
        assert_eq!(request.timeline[0].content, "extra user");
        assert_eq!(request.timeline[1].kind, PostActionTimelineType::Assistant);
        assert_eq!(request.timeline[1].content, "need choice?");
        assert_eq!(request.timeline[2].kind, PostActionTimelineType::User);
        assert_eq!(request.timeline[2].content, "choice A");
    }

    /// Verifies native PostAction preserves layout and tails while external retains its established caps.
    /// 验证原生 PostAction 保留排版及尾部，外置则保留既有上限。
    #[test]
    fn completed_payload_content_preparation_is_backend_specific() {
        // Long machine-like content must reach native cleaning before any character truncation.
        // 长机器文本必须先到达原生清洗器，不能提前按字符截断。
        let content = format!("  header\n\n    indented\n{}\nTAIL", "x".repeat(6500));
        // A tool entry proves native preservation does not broaden the allowed timeline roles.
        // 工具条目用于证明原生保留原文并未扩大允许的时间线角色。
        let payload = json!({
            "turn_id": "turn-layout", "session_id": "session-layout",
            "user_content": content, "assistant_content": content,
            "timeline": [
                {"role": "assistant", "kind": "ask", "content": content},
                {"role": "tool", "kind": "tool_finished", "content": "must not leak"}
            ]
        });
        // Mutate only the saved backend mode between the two projections.
        // 两次投影之间仅修改已保存的后端模式。
        let mut config = ready_config();
        config.backend_mode = VmmBackendMode::Native;
        // Native projection is byte-preserving, leaving all sanitation to its own intake.
        // 原生投影逐字节保留，将全部清洗交给自身准入。
        let native =
            post_action_request_from_completed_payload(&payload, &config).expect("native request");
        assert_eq!(native.user_content, content);
        assert_eq!(native.assistant_content, content);
        assert_eq!(native.timeline.len(), 1);
        assert_eq!(native.timeline[0].content, content);
        config.backend_mode = VmmBackendMode::ExternalGrpc;
        // External projection must not accidentally inherit native uncapped transport semantics.
        // 外置投影不得意外继承原生无截断的传输语义。
        let external = post_action_request_from_completed_payload(&payload, &config)
            .expect("external request");
        assert_eq!(
            external.user_content,
            compact_text(&content, USER_CONTENT_LIMIT)
        );
        assert_eq!(
            external.assistant_content,
            compact_text(&content, ASSISTANT_CONTENT_LIMIT)
        );
        assert_eq!(
            external.timeline[0].content,
            compact_text(&content, TIMELINE_CONTENT_LIMIT)
        );
        assert!(external.user_content.ends_with('…'));
    }

    #[test]
    fn precheck_response_converts_context_items_to_transient_injection() {
        let response = PreCheckResponse {
            should_inject: true,
            context_items: vec![
                ContextItem {
                    text: String::from("memory one\nsecond line"),
                    score: 0.9986,
                    turn_id: 11,
                    has_dialogue: true,
                    created_datetime: String::from("2026-05-02 14:20:00"),
                    ..ContextItem::default()
                },
                ContextItem {
                    text: String::from("detached memory"),
                    score: 0.25,
                    turn_id: 22,
                    has_dialogue: false,
                    ..ContextItem::default()
                },
                ContextItem {
                    text: String::from(""),
                    ..ContextItem::default()
                },
            ],
            read_source: crate::VmmMemoryReadSource::new(
                crate::VmmMemorySource::native(1).expect("owner"),
                vec![1],
            )
            .expect("source"),
            degraded: false,
            trace_id: String::new(),
            session_memory_status: crate::SessionMemoryStatus::Active,
            recall_outcome: crate::PreCheckRecallOutcome::Matched,
            recall_queries: Vec::new(),
        };

        let insert = injection_from_precheck_response(&response, &ready_config()).expect("insert");

        assert_eq!(insert.tag, "vmm_inject");
        assert_eq!(insert.ttl_turns, Some(5));
        assert_eq!(insert.items.len(), 2);
        assert_eq!(insert.items[0].content, "memory one second line");
        assert_eq!(
            insert.items[0].renderer_id.as_deref(),
            Some(VMM_PRECHECK_MEMORY_RENDERER_ID)
        );
        assert_eq!(
            insert.items[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(VMM_METADATA_TEXT))
                .and_then(Value::as_str),
            Some("memory one\nsecond line")
        );
        assert_eq!(
            insert.items[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(VMM_METADATA_TURN_ID))
                .and_then(Value::as_u64),
            Some(11)
        );
        assert_eq!(
            insert.items[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(VMM_METADATA_MEMORY_ID))
                .and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(insert.items[1].content, "detached memory");
        assert_eq!(insert.items[0].source_display.as_deref(), Some("VMM"));
    }

    #[test]
    fn vmm_renderer_builds_model_and_panel_text_from_metadata() {
        let mut state = vulcan_context_injection::ContextInjectionState::default();
        let response = PreCheckResponse {
            should_inject: true,
            context_items: vec![ContextItem {
                text: String::from("memory one\nsecond line"),
                score: 0.9986,
                turn_id: 11,
                has_dialogue: true,
                created_datetime: String::from("2026-05-02 14:20:00"),
                memory_id: 137,
            }],
            read_source: crate::VmmMemoryReadSource::new(
                crate::VmmMemorySource::native(1).expect("owner"),
                vec![1],
            )
            .expect("source"),
            degraded: false,
            trace_id: String::new(),
            session_memory_status: crate::SessionMemoryStatus::Active,
            recall_outcome: crate::PreCheckRecallOutcome::Matched,
            recall_queries: Vec::new(),
        };
        state.apply_insert_for_turn(
            injection_from_precheck_response(&response, &ready_config()).expect("insert"),
            "1",
            Some("turn-a"),
        );
        let registry = vulcan_context_injection::InjectionRendererRegistry::default()
            .with_renderer(
                VMM_PRECHECK_MEMORY_RENDERER_ID,
                vmm_precheck_memory_renderer(),
            );
        let options = vulcan_context_injection::InjectionRenderOptions {
            registry: &registry,
            current_turn_marker: Some("turn-a"),
        };

        let view = state.resolve_view_with_options(&options);
        let cards = state.panel_cards_with_registry(&registry);

        assert!(view.transient_prefix.contains("<CURRENT_TURN>"));
        assert!(view.transient_prefix.contains(
            "[TURN_ID:11|MEMORY_ID:137|SOURCE:0.999|TIME:2026-05-02 14:20:00]memory one second line"
        ));
        assert_eq!(cards[0].items[0].content, "memory one second line");
        assert!(
            cards[0].items[0]
                .detail
                .contains("Time: 2026-05-02 14:20:00")
        );
        assert!(cards[0].items[0].detail.contains("Memory ID: 137"));
    }
}
