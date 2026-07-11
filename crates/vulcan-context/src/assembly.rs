//! Layered context assembly primitives reused by request builders, previews, and future baseline tracking.
//! 供请求构建、预览与后续基线追踪复用的分层上下文装配基础能力。

use serde::{Deserialize, Serialize};
use vulcan_core::types::{
    ConversationMessage, ConversationPayload, ProviderRequestSnapshot, ProviderRequestToolSchema,
};

/// Logical layer kind inside one assembled context stack.
/// 单个已装配上下文栈中的逻辑层类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextLayerKind {
    /// Fixed system-level instructions.
    /// 固定系统级说明。
    System,
    /// Project-specific rules or local repository instructions.
    /// 项目级规则或本地仓库说明。
    ProjectRules,
    /// Runtime-state details such as permissions, tools, or active mode.
    /// 运行时状态细节，例如权限、工具或活动模式。
    Runtime,
    /// Recent canonical conversation history.
    /// 最近的规范会话历史。
    History,
    /// Retrieved external or historical context.
    /// 检索得到的外部或历史上下文。
    Retrieval,
    /// Compaction summary or retained tail after history rewrite.
    /// 压缩摘要或历史重写后保留的尾部内容。
    Compaction,
    /// Tool-related context such as tool schemas or tool availability notes.
    /// 工具相关上下文，例如工具 schema 或工具可用性说明。
    Tools,
    /// Direct user-authored instruction text.
    /// 用户直接编写的指令文本。
    User,
}

impl ContextLayerKind {
    /// Returns one short display label for previews and diagnostics.
    /// 返回供预览与诊断使用的一条简短显示标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::System => "系统",
            Self::ProjectRules => "项目规则",
            Self::Runtime => "运行时",
            Self::History => "历史",
            Self::Retrieval => "检索",
            Self::Compaction => "压缩",
            Self::Tools => "工具",
            Self::User => "用户",
        }
    }
}

/// One named context layer stored as plain text for later rendering and request assembly.
/// 一条以纯文本保存、供后续渲染与请求装配使用的已命名上下文层。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextLayer {
    /// Logical kind used to group and reorder the layer.
    /// 用于分组与重排该层的逻辑类型。
    pub kind: ContextLayerKind,
    /// Human-readable label used by previews and diagnostics.
    /// 供预览与诊断使用的人类可读标签。
    pub label: String,
    /// Plain-text payload carried by the layer.
    /// 该层携带的纯文本载荷。
    pub content: String,
}

impl ContextLayer {
    /// Builds one new context layer from kind, label, and content.
    /// 根据类型、标签与内容构建一条新的上下文层。
    pub fn new(
        kind: ContextLayerKind,
        label: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            label: label.into(),
            content: content.into(),
        }
    }

    /// Returns whether this layer carries any visible text after trimming.
    /// 返回该层在去除空白后是否仍然包含可见文本。
    pub fn has_content(&self) -> bool {
        !self.content.trim().is_empty()
    }
}

/// One assembled context stack that can be rendered, diffed, or summarized before a provider request.
/// 一份可在 Provider 请求前渲染、对比或汇总的已装配上下文栈。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAssembly {
    /// Ordered context layers that will be combined into one request.
    /// 将被组合进单次请求的有序上下文层集合。
    pub layers: Vec<ContextLayer>,
}

impl ContextAssembly {
    /// Builds one empty context assembly.
    /// 构建一份空的上下文装配结果。
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Appends one layer to the end of the assembly.
    /// 向装配结果末尾追加一条上下文层。
    pub fn push(&mut self, layer: ContextLayer) {
        self.layers.push(layer);
    }

    /// Extends the assembly with multiple layers in order.
    /// 按顺序用多条上下文层扩展装配结果。
    pub fn extend(&mut self, layers: impl IntoIterator<Item = ContextLayer>) {
        self.layers.extend(layers);
    }

    /// Returns whether the assembly currently contains no visible layers.
    /// 返回该装配结果当前是否不包含任何可见层。
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Returns one compact human-readable summary line for diagnostics and previews.
    /// 返回一条供诊断与预览使用的紧凑人类可读摘要行。
    pub fn summary_note(&self) -> String {
        if self.layers.is_empty() {
            return String::from("上下文为空");
        }

        let labels = self
            .layers
            .iter()
            .map(|layer| layer.label.trim().to_string())
            .collect::<Vec<_>>()
            .join(" · ");
        format!("{} 层 · {}", self.layers.len(), labels)
    }

    /// Builds one layered assembly from a provider request snapshot.
    /// 根据 Provider 请求快照构建一份分层上下文装配结果。
    pub fn from_provider_request_snapshot(snapshot: &ProviderRequestSnapshot) -> Self {
        let mut assembly = Self::new();

        if !snapshot.system_prompt.trim().is_empty() {
            assembly.extend(render_system_prompt_layers(&snapshot.system_prompt));
        }

        assembly.extend(snapshot.history.iter().enumerate().map(|(index, message)| {
            ContextLayer::new(
                ContextLayerKind::History,
                format!("History #{} · {}", index + 1, render_message_role(message)),
                render_message_layer_content(message),
            )
        }));

        if !snapshot.prompt.trim().is_empty() {
            assembly.push(ContextLayer::new(
                ContextLayerKind::User,
                "Effective Outgoing Prompt",
                snapshot.prompt.clone(),
            ));
        }

        assembly.extend(snapshot.tool_schemas.iter().map(render_tool_schema_layer));

        assembly
    }
}

/// Splits one provider system prompt into stable diagnostic layers without changing the outgoing request payload.
/// 将单份 Provider 系统提示词拆成稳定诊断层，同时不改变实际发出的请求载荷。
fn render_system_prompt_layers(system_prompt: &str) -> Vec<ContextLayer> {
    let mut runtime_lines = Vec::new();
    let mut non_runtime_lines = Vec::new();

    for line in system_prompt.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Current workspace:")
            || trimmed.starts_with("Current execution mode:")
        {
            runtime_lines.push(line.to_string());
        } else {
            non_runtime_lines.push(line.to_string());
        }
    }

    let non_runtime_text = non_runtime_lines.join("\n");
    let (system_text, project_rules_text) =
        split_project_rules_from_system_prompt(&non_runtime_text);
    let mut layers = Vec::new();

    if !system_text.trim().is_empty() {
        layers.push(ContextLayer::new(
            ContextLayerKind::System,
            "System Preamble",
            system_text,
        ));
    }
    if !project_rules_text.trim().is_empty() {
        layers.push(ContextLayer::new(
            ContextLayerKind::ProjectRules,
            "Project Instructions",
            project_rules_text,
        ));
    }
    if !runtime_lines.is_empty() {
        layers.push(ContextLayer::new(
            ContextLayerKind::Runtime,
            "Runtime Context",
            runtime_lines.join("\n"),
        ));
    }

    if layers.is_empty() {
        layers.push(ContextLayer::new(
            ContextLayerKind::System,
            "System Preamble",
            system_prompt.to_string(),
        ));
    }

    layers
}

/// Separates repository/project instruction blocks from the base system prompt when known markers are present.
/// 当存在已知标记时，将仓库或项目指令块从基础系统提示词中分离出来。
fn split_project_rules_from_system_prompt(system_prompt: &str) -> (String, String) {
    const PROJECT_RULE_MARKERS: [&str; 3] = [
        "# AGENTS.md instructions",
        "<INSTRUCTIONS>",
        "--- project-doc ---",
    ];

    let marker_index = PROJECT_RULE_MARKERS
        .iter()
        .filter_map(|marker| system_prompt.find(marker))
        .min();
    let Some(marker_index) = marker_index else {
        return (system_prompt.trim().to_string(), String::new());
    };

    let (system_text, project_rules_text) = system_prompt.split_at(marker_index);
    (
        system_text.trim().to_string(),
        project_rules_text.trim().to_string(),
    )
}

/// Builds one layered context assembly from a provider request snapshot.
/// 根据 Provider 请求快照构建一份分层上下文装配结果。
pub fn build_provider_request_context_assembly(
    snapshot: &ProviderRequestSnapshot,
) -> ContextAssembly {
    ContextAssembly::from_provider_request_snapshot(snapshot)
}

/// One stable baseline snapshot used to tell whether a later assembly still matches the stable non-volatile shape.
/// 用于判断后续装配结果是否仍匹配稳定且非易变形态的基线快照。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBaseline {
    /// Stable fingerprint that identifies one previously accepted context shape.
    /// 标识某个先前已确认上下文形态的稳定指纹。
    pub fingerprint: String,
    /// Number of layers that were part of the baseline snapshot.
    /// 该基线快照中包含的层数量。
    pub layer_count: usize,
}

/// Structured comparison between one stored context baseline and one freshly assembled context stack.
/// 一份已存上下文基线与一份新装配上下文栈之间的结构化对比结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBaselineComparison {
    /// Whether both the layer count and fingerprint still match.
    /// 层数与指纹是否都仍然匹配。
    pub matches: bool,
    /// Whether the layer count is unchanged.
    /// 层数量是否保持不变。
    pub layer_count_matches: bool,
    /// Whether the full assembly fingerprint is unchanged.
    /// 完整装配指纹是否保持不变。
    pub fingerprint_matches: bool,
    /// Number of layers stored in the baseline.
    /// 基线中记录的层数量。
    pub baseline_layer_count: usize,
    /// Number of layers in the freshly assembled context.
    /// 新装配上下文中的层数量。
    pub current_layer_count: usize,
    /// Fingerprint stored in the baseline.
    /// 基线中记录的指纹。
    pub baseline_fingerprint: String,
    /// Fingerprint calculated from the freshly assembled context.
    /// 从新装配上下文计算出的指纹。
    pub current_fingerprint: String,
}

impl ContextBaselineComparison {
    /// Returns one compact human-readable diagnostics sentence.
    /// 返回一条紧凑的人类可读诊断语句。
    pub fn summary_note(&self) -> String {
        if self.matches {
            return format!("基线匹配 · {} 层", self.current_layer_count);
        }

        let layer_label = if self.layer_count_matches {
            format!("层数未变 {}", self.current_layer_count)
        } else {
            format!(
                "层数变化 {} -> {}",
                self.baseline_layer_count, self.current_layer_count
            )
        };
        let fingerprint_label = if self.fingerprint_matches {
            "指纹未变"
        } else {
            "指纹变化"
        };
        format!("基线失配 · {layer_label} · {fingerprint_label}")
    }
}

impl ContextBaseline {
    /// Builds one baseline snapshot from the current stable assembly shape.
    /// 根据当前稳定装配形态构建一份基线快照。
    pub fn from_assembly(assembly: &ContextAssembly) -> Self {
        Self {
            fingerprint: build_context_baseline_fingerprint(assembly),
            layer_count: context_baseline_layer_count(assembly),
        }
    }

    /// Returns whether the provided assembly still matches this stable baseline exactly.
    /// 返回给定装配结果是否仍与当前稳定基线完全匹配。
    pub fn matches(&self, assembly: &ContextAssembly) -> bool {
        self.layer_count == context_baseline_layer_count(assembly)
            && self.fingerprint == build_context_baseline_fingerprint(assembly)
    }

    /// Compares this baseline with one freshly assembled context stack and returns structured diagnostics.
    /// 将当前基线与一份新装配上下文栈对比，并返回结构化诊断结果。
    pub fn compare(&self, assembly: &ContextAssembly) -> ContextBaselineComparison {
        let current_fingerprint = build_context_baseline_fingerprint(assembly);
        let layer_count_matches = self.layer_count == context_baseline_layer_count(assembly);
        let fingerprint_matches = self.fingerprint == current_fingerprint;
        ContextBaselineComparison {
            matches: layer_count_matches && fingerprint_matches,
            layer_count_matches,
            fingerprint_matches,
            baseline_layer_count: self.layer_count,
            current_layer_count: context_baseline_layer_count(assembly),
            baseline_fingerprint: self.fingerprint.clone(),
            current_fingerprint,
        }
    }
}

/// Builds one stable fingerprint for the full layered context assembly.
/// 为完整分层上下文装配结果构建一份稳定指纹。
pub fn build_context_fingerprint(assembly: &ContextAssembly) -> String {
    build_context_fingerprint_from_layers(assembly.layers.iter())
}

/// Builds one stable fingerprint for the non-volatile baseline layer subset.
/// 为非易变的基线层子集构建一份稳定指纹。
pub fn build_context_baseline_fingerprint(assembly: &ContextAssembly) -> String {
    build_context_fingerprint_from_layers(context_baseline_layers(assembly))
}

/// Returns the number of baseline layers that participate in stable comparison.
/// 返回参与稳定对比的基线层数量。
fn context_baseline_layer_count(assembly: &ContextAssembly) -> usize {
    context_baseline_layers(assembly).count()
}

/// Returns the context layers that remain part of the stable baseline comparison.
/// 返回仍参与稳定基线对比的上下文层。
fn context_baseline_layers(assembly: &ContextAssembly) -> impl Iterator<Item = &ContextLayer> {
    assembly.layers.iter().filter(|layer| {
        !matches!(
            layer.kind,
            ContextLayerKind::History | ContextLayerKind::User
        )
    })
}

/// Builds one stable fingerprint from any ordered layer iterator.
/// 从任意有序层迭代器构建一份稳定指纹。
fn build_context_fingerprint_from_layers<'a, I>(layers: I) -> String
where
    I: IntoIterator<Item = &'a ContextLayer>,
{
    layers
        .into_iter()
        .map(|layer| {
            format!(
                "{:?}|{}|{}",
                layer.kind,
                layer.label.trim(),
                layer.content.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Renders one provider request tool schema as a dedicated context layer.
/// 将单个 Provider 请求工具 schema 渲染为独立上下文层。
fn render_tool_schema_layer(definition: &ProviderRequestToolSchema) -> ContextLayer {
    ContextLayer::new(
        ContextLayerKind::Tools,
        format!("Tool Schema · {}", definition.id),
        render_tool_schema_content(definition),
    )
}

/// Renders one conversation message into a plain-text context layer payload.
/// 将单条会话消息渲染为纯文本上下文层载荷。
fn render_message_layer_content(message: &ConversationMessage) -> String {
    match &message.payload {
        ConversationPayload::UserText { text, attachments } => {
            let mut sections = vec![text.trim().to_string()];
            if !attachments.is_empty() {
                sections.push(String::from("附件："));
                sections.extend(attachments.iter().map(|attachment| {
                    let byte_label = attachment
                        .byte_len
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| String::from("未知"));
                    format!(
                        "- kind={:?} · file={} · mime={} · bytes={}",
                        attachment.kind, attachment.file_name, attachment.mime_type, byte_label
                    )
                }));
            }
            sections
                .into_iter()
                .filter(|section| !section.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }
        ConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            tool_calls,
            ..
        } => {
            let mut sections = Vec::new();
            if !text.trim().is_empty() {
                sections.push(format!("文本：\n{}", text.trim()));
            }
            if !reasoning.trim().is_empty() {
                sections.push(format!("推理：\n{}", reasoning.trim()));
            }
            if let Some(provider_message_id) = provider_message_id.as_deref() {
                sections.push(format!("provider_message_id={provider_message_id}"));
            }
            if !tool_calls.is_empty() {
                sections.push(String::from("工具调用："));
                sections.extend(tool_calls.iter().map(|tool_call| {
                    let arguments = serde_json::to_string_pretty(&tool_call.arguments)
                        .unwrap_or_else(|_| tool_call.arguments.to_string());
                    format!(
                        "- {} · id={} \n{}",
                        tool_call.tool_name, tool_call.id, arguments
                    )
                }));
            }
            sections.join("\n")
        }
        ConversationPayload::ToolResult { tool_call_id, text } => {
            let mut sections = vec![format!("message_id={}", message.id)];
            if let Some(tool_call_id) = tool_call_id.as_deref() {
                sections.push(format!("tool_call_id={tool_call_id}"));
            }
            if !text.trim().is_empty() {
                sections.push(text.trim().to_string());
            }
            sections.join("\n")
        }
        ConversationPayload::AgentMessage { .. } => message
            .payload
            .agent_message_model_text()
            .unwrap_or_default(),
    }
}

/// Renders one conversation message role as a short human-readable label.
/// 将单条会话消息角色渲染为简短的人类可读标签。
fn render_message_role(message: &ConversationMessage) -> &'static str {
    match &message.payload {
        ConversationPayload::UserText { .. } => "user",
        ConversationPayload::Assistant { .. } => "assistant",
        ConversationPayload::ToolResult { .. } => "tool_result",
        ConversationPayload::AgentMessage { .. } => "agent_message",
    }
}

/// Renders one provider request tool schema into a plain-text context layer payload.
/// 将单个 Provider 请求工具 schema 渲染为纯文本上下文层载荷。
fn render_tool_schema_content(definition: &ProviderRequestToolSchema) -> String {
    let parameters = serde_json::to_string_pretty(&definition.parameters_schema)
        .unwrap_or_else(|_| definition.parameters_schema.to_string());
    format!(
        "name={}\nsource={}\ndescription={}\n{}",
        definition.name, definition.source_label, definition.description, parameters
    )
}

#[cfg(test)]
mod tests {
    //! Layered context assembly regression tests covering provider-request conversion and baseline stability.
    //! 覆盖 Provider 请求转换与基线稳定性的分层上下文装配回归测试。

    use super::{
        ContextAssembly, ContextBaseline, ContextLayerKind, build_provider_request_context_assembly,
    };
    use serde_json::json;
    use vulcan_core::types::{
        ConversationMessage, ConversationPayload, ProviderContinuation, ProviderPromptLayers,
        ProviderRequestSnapshot, ProviderRequestToolSchema,
    };

    /// Builds one provider request snapshot that exercises all high-value context layers.
    /// 构建一份覆盖高价值上下文层的 Provider 请求快照。
    fn sample_snapshot() -> ProviderRequestSnapshot {
        ProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::from("system"),
            prompt_layers: ProviderPromptLayers::default(),
            prompt: String::from("prompt"),
            history: vec![ConversationMessage {
                id: String::from("m1"),
                payload: ConversationPayload::UserText {
                    text: String::from("hello"),
                    attachments: Vec::new(),
                },
            }],
            temperature: None,
            max_turns: 4,
            tool_schemas: vec![ProviderRequestToolSchema {
                id: String::from("tool-1"),
                name: String::from("tool-1"),
                description: String::from("tool"),
                source_label: String::from("native"),
                parameters_schema: json!({"type":"object","properties":{}}),
            }],
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        }
    }

    /// Converts one provider request snapshot into visible context layers with a stable baseline.
    /// 将一份 Provider 请求快照转换为可见上下文层，并保持稳定基线。
    #[test]
    fn provider_request_snapshot_builds_layered_context() {
        let assembly = build_provider_request_context_assembly(&sample_snapshot());

        assert_eq!(assembly.layers.len(), 4);
        assert_eq!(assembly.layers[0].kind, ContextLayerKind::System);
        assert_eq!(assembly.layers[1].kind, ContextLayerKind::History);
        assert_eq!(assembly.layers[2].kind, ContextLayerKind::User);
        assert_eq!(assembly.layers[3].kind, ContextLayerKind::Tools);
        assert!(!assembly.summary_note().is_empty());
    }

    /// Keeps the baseline stable for identical layered context assemblies.
    /// 对相同的分层上下文装配结果保持基线稳定。
    #[test]
    fn layered_context_baseline_matches_identical_assembly() {
        let assembly = ContextAssembly::from_provider_request_snapshot(&sample_snapshot());
        let baseline = ContextBaseline::from_assembly(&assembly);

        assert!(baseline.matches(&assembly));
        assert!(baseline.compare(&assembly).matches);
        assert_eq!(baseline.layer_count, 2);
        assert!(!baseline.fingerprint.is_empty());
    }

    /// Keeps the stable baseline unchanged when only history or current user text changes.
    /// 仅当历史层或当前用户文本变化时，稳定基线保持不变。
    #[test]
    fn layered_context_baseline_ignores_history_and_user_text_changes() {
        let assembly = ContextAssembly::from_provider_request_snapshot(&sample_snapshot());
        let baseline = ContextBaseline::from_assembly(&assembly);
        let mut changed_assembly = assembly.clone();
        changed_assembly.layers[1].content = String::from("history changed");
        changed_assembly.layers[2].content = String::from("user prompt changed");

        let comparison = baseline.compare(&changed_assembly);

        assert!(baseline.matches(&changed_assembly));
        assert!(comparison.matches);
        assert_eq!(comparison.baseline_layer_count, 2);
        assert_eq!(comparison.current_layer_count, 2);
        assert_eq!(
            comparison.baseline_fingerprint,
            comparison.current_fingerprint
        );
    }

    /// Reports a structured mismatch when the assembled context changes after the baseline is captured.
    /// 当基线捕获后装配上下文发生变化时，报告结构化失配结果。
    #[test]
    fn layered_context_baseline_reports_changed_assembly() {
        let assembly = ContextAssembly::from_provider_request_snapshot(&sample_snapshot());
        let baseline = ContextBaseline::from_assembly(&assembly);
        let mut changed_assembly = assembly.clone();
        changed_assembly.push(super::ContextLayer::new(
            ContextLayerKind::Runtime,
            "Runtime Context",
            "Current execution mode: Build",
        ));

        let comparison = baseline.compare(&changed_assembly);

        assert!(!comparison.matches);
        assert!(!comparison.layer_count_matches);
        assert!(!comparison.fingerprint_matches);
        assert!(comparison.summary_note().contains("基线失配"));
    }

    /// Splits embedded project and runtime instructions out of the provider system prompt for diagnostics.
    /// 为诊断展示从 Provider 系统提示词中拆出内嵌项目指令与运行时指令。
    #[test]
    fn provider_request_snapshot_splits_system_project_and_runtime_layers() {
        let mut snapshot = sample_snapshot();
        snapshot.system_prompt = String::from(
            "base instructions\n\n# AGENTS.md instructions for D:\\repo\n<INSTRUCTIONS>\nproject rules\n</INSTRUCTIONS>\nCurrent workspace: D:\\repo\nCurrent execution mode: Build",
        );

        let assembly = build_provider_request_context_assembly(&snapshot);

        assert_eq!(assembly.layers[0].kind, ContextLayerKind::System);
        assert_eq!(assembly.layers[1].kind, ContextLayerKind::ProjectRules);
        assert_eq!(assembly.layers[2].kind, ContextLayerKind::Runtime);
        assert!(assembly.layers[0].content.contains("base instructions"));
        assert!(assembly.layers[1].content.contains("project rules"));
        assert!(assembly.layers[2].content.contains("Current workspace"));
    }
}
