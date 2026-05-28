//! Structured document nodes used by agent-oriented transcript renderers.
//! 面向代理转录渲染器使用的结构化文档节点定义。

use pulldown_cmark::{Event, Options, Parser, TagEnd};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;

/// Attribute map stored on one extension directive block.
/// 存储在单个扩展指令块上的属性映射。
pub type AgentViewAttributes = BTreeMap<String, String>;

/// Presentation kinds used by tool blocks so different host-managed tool results can evolve independent transcript renderers.
/// 工具区块使用的展示种类，使不同宿主管理工具结果可以演进为彼此独立的转录渲染器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum AgentViewToolPresentationKind {
    /// Generic tool card used by MCP, unknown skills, and any tool result without a dedicated presentation family.
    /// 供 MCP、未知 skill 以及任何没有专属展示家族的工具结果使用的通用工具卡片。
    Generic,
    /// Dedicated one-shot command-run card used by host-managed shell-command results.
    /// 供宿主管理 shell-command 结果使用的专属一次性命令执行卡片。
    CommandRun,
    /// Dedicated command-session card used by host-managed shell-session results.
    /// 供宿主管理 shell-session 结果使用的专属命令会话卡片。
    CommandSession,
    /// Dedicated task-session card used by host-managed delegate/task tool flows.
    /// 供宿主管理的委派或任务工具流使用的专属 task-session 卡片。
    TaskSession,
    /// Dedicated generated-image tool card.
    /// 专属图片生成工具卡片。
    GeneratedImage,
    /// Dedicated generated-video tool card.
    /// 专属视频生成工具卡片。
    GeneratedVideo,
    /// Dedicated generated-audio tool card.
    /// 专属音频生成工具卡片。
    GeneratedAudio,
    /// Dedicated generated-document tool card.
    /// 专属文档生成工具卡片。
    GeneratedDocument,
}

impl AgentViewToolPresentationKind {
    /// Shared attribute key used to persist the presentation kind on tool blocks.
    /// 用于在工具区块上持久化展示种类的共享属性键。
    pub const ATTRIBUTE_KEY: &'static str = "tool-presentation-kind";

    /// Returns the stable lowercase attribute value used for persistence and cross-layer routing.
    /// 返回用于持久化与跨层路由的稳定小写属性值。
    pub fn as_attribute_value(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::CommandRun => "command-run",
            Self::CommandSession => "command-session",
            Self::TaskSession => "task-session",
            Self::GeneratedImage => "generated-image",
            Self::GeneratedVideo => "generated-video",
            Self::GeneratedAudio => "generated-audio",
            Self::GeneratedDocument => "generated-document",
        }
    }

    /// Parses one persisted attribute value into the corresponding presentation kind.
    /// 将一条已持久化属性值解析为对应展示种类。
    pub fn from_attribute_value(value: &str) -> Option<Self> {
        match value.trim() {
            "generic" => Some(Self::Generic),
            "command-run" => Some(Self::CommandRun),
            "command-session" => Some(Self::CommandSession),
            "task-session" => Some(Self::TaskSession),
            "generated-image" => Some(Self::GeneratedImage),
            "generated-video" => Some(Self::GeneratedVideo),
            "generated-audio" => Some(Self::GeneratedAudio),
            "generated-document" => Some(Self::GeneratedDocument),
            _ => None,
        }
    }

    /// Writes the current presentation kind into the provided attribute map.
    /// 将当前展示种类写入给定属性映射。
    pub fn write_to_attributes(self, attributes: &mut AgentViewAttributes) {
        attributes.insert(
            Self::ATTRIBUTE_KEY.to_string(),
            self.as_attribute_value().to_string(),
        );
    }

    /// Reads the persisted or legacy presentation kind from one tool-block attribute map when it is available.
    /// 当存在时，从单个工具区块属性映射中读取持久化或旧版兼容的展示种类。
    pub fn from_attributes(attributes: &AgentViewAttributes) -> Option<Self> {
        attributes
            .get(Self::ATTRIBUTE_KEY)
            .and_then(|value| Self::from_attribute_value(value))
            .or_else(|| Self::from_legacy_attributes(attributes))
    }

    /// Resolves the effective presentation kind from one tool-block attribute map, defaulting to the generic tool card.
    /// 从单个工具区块属性映射解析有效展示种类，并在缺失时回退到通用工具卡片。
    pub fn resolve_from_attributes(attributes: &AgentViewAttributes) -> Self {
        Self::from_attributes(attributes).unwrap_or(Self::Generic)
    }

    /// Reads the older string-based tool visual markers so pre-refactor transcripts still map into the new presentation families.
    /// 读取旧版基于字符串的工具视觉标记，使重构前的转录仍可映射到新的展示家族。
    fn from_legacy_attributes(attributes: &AgentViewAttributes) -> Option<Self> {
        match attributes.get("tool-visual-kind").map(String::as_str) {
            Some("command-run") => Some(Self::CommandRun),
            Some("command-session") => Some(Self::CommandSession),
            Some("task-session") => Some(Self::TaskSession),
            Some("generated-media") => match attributes
                .get("generated-media-kind")
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref()
            {
                Some("image") => Some(Self::GeneratedImage),
                Some("video") => Some(Self::GeneratedVideo),
                Some("audio") => Some(Self::GeneratedAudio),
                Some("pdf") | Some("document") => Some(Self::GeneratedDocument),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Top-level structured transcript document consumed by future agent-view renderers.
/// 供后续 agent-view 渲染器消费的顶层结构化转录文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentViewDocument {
    /// Ordered block sequence kept exactly as the parser or runtime producer emitted it.
    /// 按解析器或运行时产出顺序原样保留的区块序列。
    pub blocks: Vec<AgentViewBlock>,
}

impl AgentViewDocument {
    /// Creates one empty transcript document with no blocks.
    /// 创建一份不包含任何区块的空转录文档。
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one standard Markdown block to the tail of the current document.
    /// 向当前文档尾部追加一个标准 Markdown 区块。
    pub fn push_markdown(&mut self, raw_source: impl Into<String>) {
        self.blocks
            .push(AgentViewBlock::Markdown(AgentViewMarkdownBlock::new(
                raw_source,
            )));
    }

    /// Appends one extension directive block to the tail of the current document.
    /// 向当前文档尾部追加一个扩展指令区块。
    pub fn push_directive(&mut self, block: AgentViewDirectiveBlock) {
        self.blocks.push(AgentViewBlock::Directive(block));
    }

    /// Builds the copy-oriented plain-text projection of the whole document.
    /// 构建整份文档面向复制场景的纯文本投影。
    pub fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .map(AgentViewBlock::plain_text)
            .filter(|segment| !segment.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Builds the whole-document selectable projection used by the first unified transcript surface.
    /// 构建第一版统一转录表面使用的整份文档可选投影。
    pub fn selectable_text(&self) -> String {
        self.blocks
            .iter()
            .map(AgentViewBlock::selectable_text)
            .filter(|segment| !segment.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// One top-level document block rendered inside the unified transcript surface.
/// 渲染在统一转录表面中的单个顶层文档区块。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentViewBlock {
    /// Standard Markdown fragment that should be rendered using the baseline Markdown rules.
    /// 应按基础 Markdown 规则渲染的标准 Markdown 片段。
    Markdown(AgentViewMarkdownBlock),
    /// Structured extension directive block carrying higher-level agent semantics.
    /// 携带更高层代理语义的结构化扩展指令区块。
    Directive(AgentViewDirectiveBlock),
}

impl AgentViewBlock {
    /// Returns the exact raw source slice that produced the current block.
    /// 返回生成当前区块的原始源码切片。
    pub fn raw_source(&self) -> &str {
        match self {
            Self::Markdown(block) => &block.raw_source,
            Self::Directive(block) => &block.raw_source,
        }
    }

    /// Builds the copy-oriented plain-text projection of the current block.
    /// 构建当前区块面向复制场景的纯文本投影。
    pub fn plain_text(&self) -> String {
        match self {
            Self::Markdown(block) => block.plain_text(),
            Self::Directive(block) => block.plain_text(),
        }
    }

    /// Builds the selectable projection of the current block while preserving block identity cues.
    /// 构建当前区块的可选投影，并保留区块身份提示。
    pub fn selectable_text(&self) -> String {
        match self {
            Self::Markdown(block) => block.selectable_text(),
            Self::Directive(block) => block.selectable_text(),
        }
    }

    /// Returns the shared attribute map carried by the current block.
    /// 返回当前区块携带的共享属性映射。
    pub fn attributes(&self) -> &AgentViewAttributes {
        match self {
            Self::Markdown(block) => &block.attributes,
            Self::Directive(block) => &block.attributes,
        }
    }

    /// Returns the mutable shared attribute map carried by the current block.
    /// 返回当前区块携带的可变共享属性映射。
    pub fn attributes_mut(&mut self) -> &mut AgentViewAttributes {
        match self {
            Self::Markdown(block) => &mut block.attributes,
            Self::Directive(block) => &mut block.attributes,
        }
    }
}

/// Standard Markdown block preserved before future renderer-specific layout expansion.
/// 在后续渲染器特定布局展开之前保留的标准 Markdown 区块。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentViewMarkdownBlock {
    /// Parsed key-value attributes attached by higher-level runtime adapters.
    /// 由上层运行时适配器附着的已解析键值属性集合。
    #[serde(default)]
    pub attributes: AgentViewAttributes,
    /// Exact Markdown source stored for lossless copy, persistence, and reparsing.
    /// 为无损复制、持久化与重新解析保留的精确 Markdown 源文本。
    pub raw_source: String,
}

impl AgentViewMarkdownBlock {
    /// Creates one Markdown block from the provided raw source.
    /// 根据提供的原始源码创建一个 Markdown 区块。
    pub fn new(raw_source: impl Into<String>) -> Self {
        Self::new_with_attributes(raw_source, AgentViewAttributes::new())
    }

    /// Creates one Markdown block from the provided raw source and higher-level runtime attributes.
    /// 根据提供的原始源码与上层运行时属性创建一个 Markdown 区块。
    pub fn new_with_attributes(
        raw_source: impl Into<String>,
        attributes: AgentViewAttributes,
    ) -> Self {
        Self {
            attributes,
            raw_source: raw_source.into(),
        }
    }

    /// Builds the copy-oriented plain-text projection of the Markdown source.
    /// 构建该 Markdown 源文本面向复制场景的纯文本投影。
    pub fn plain_text(&self) -> String {
        markdown_source_to_plain_text(&self.raw_source)
    }

    /// Builds the selectable projection of the Markdown block while preserving the original Markdown syntax.
    /// 构建 Markdown 区块的可选投影，并保留原始 Markdown 语法。
    pub fn selectable_text(&self) -> String {
        self.raw_source.trim_end().to_string()
    }
}

/// Structured extension directive block used for agent-specific output semantics.
/// 用于代理专属输出语义的结构化扩展指令区块。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentViewDirectiveBlock {
    /// Semantic directive kind resolved from the opening fence header.
    /// 从起始围栏头部解析得到的语义指令类型。
    pub kind: AgentViewDirectiveKind,
    /// Parsed key-value attributes supplied on the opening fence line.
    /// 起始围栏行上提供的键值属性集合。
    pub attributes: AgentViewAttributes,
    /// Exact raw source slice preserved for copy-original and persistence flows.
    /// 为复制原文与持久化流程保留的精确源码切片。
    pub raw_source: String,
    /// Inner directive body stored without the outer opening and closing fence lines.
    /// 不包含起始与结束围栏行的内部指令正文。
    pub body: String,
    /// Whether the renderer should treat this block as collapsed by default.
    /// 渲染器是否应默认将该区块视为折叠状态。
    pub collapsed_by_default: bool,
}

impl AgentViewDirectiveBlock {
    /// Creates one directive block from already parsed semantic parts.
    /// 根据已经解析完成的语义部件创建一个指令区块。
    pub fn new(
        kind: AgentViewDirectiveKind,
        attributes: AgentViewAttributes,
        body: impl Into<String>,
        raw_source: impl Into<String>,
        collapsed_by_default: bool,
    ) -> Self {
        Self {
            kind,
            attributes,
            raw_source: raw_source.into(),
            body: body.into(),
            collapsed_by_default,
        }
    }

    /// Creates one runtime-owned thinking block with optional extra attributes.
    /// 创建一个由运行时直接持有的思考区块，并允许附带额外属性。
    pub fn thinking(body: impl Into<String>, collapsed_by_default: bool) -> Self {
        let body = body.into();
        let mut attributes = AgentViewAttributes::new();
        if collapsed_by_default {
            attributes.insert(String::from("collapsed"), String::from("true"));
        }
        let raw_source =
            render_directive_source(&AgentViewDirectiveKind::Thinking, &attributes, &body);

        Self::new(
            AgentViewDirectiveKind::Thinking,
            attributes,
            body,
            raw_source,
            collapsed_by_default,
        )
    }

    /// Creates one runtime-owned tool-call block with optional extra attributes.
    /// 创建一个由运行时直接持有的工具调用区块，并允许附带额外属性。
    pub fn tool_call(
        body: impl Into<String>,
        collapsed_by_default: bool,
        attributes: AgentViewAttributes,
    ) -> Self {
        let body = body.into();
        let raw_source =
            render_directive_source(&AgentViewDirectiveKind::ToolCall, &attributes, &body);

        Self::new(
            AgentViewDirectiveKind::ToolCall,
            attributes,
            body,
            raw_source,
            collapsed_by_default,
        )
    }

    /// Creates one runtime-owned tool-result block with optional extra attributes.
    /// 创建一个由运行时直接持有的工具结果区块，并允许附带额外属性。
    pub fn tool_result(
        body: impl Into<String>,
        collapsed_by_default: bool,
        attributes: AgentViewAttributes,
    ) -> Self {
        let body = body.into();
        let raw_source =
            render_directive_source(&AgentViewDirectiveKind::ToolResult, &attributes, &body);

        Self::new(
            AgentViewDirectiveKind::ToolResult,
            attributes,
            body,
            raw_source,
            collapsed_by_default,
        )
    }

    /// Creates one runtime-owned diff block with optional extra attributes.
    /// 创建一个由运行时直接持有的差异区块，并允许附带额外属性。
    pub fn diff(
        body: impl Into<String>,
        collapsed_by_default: bool,
        attributes: AgentViewAttributes,
    ) -> Self {
        let body = body.into();
        let raw_source = render_directive_source(&AgentViewDirectiveKind::Diff, &attributes, &body);

        Self::new(
            AgentViewDirectiveKind::Diff,
            attributes,
            body,
            raw_source,
            collapsed_by_default,
        )
    }

    /// Builds the copy-oriented plain-text projection of the directive body.
    /// 构建该指令正文面向复制场景的纯文本投影。
    pub fn plain_text(&self) -> String {
        markdown_source_to_plain_text(&self.body)
    }

    /// Builds the selectable projection of the directive block with a readable title and its original body.
    /// 构建指令区块的可选投影，包含可读标题与原始正文。
    pub fn selectable_text(&self) -> String {
        let title_line = self.selectable_title();
        let trimmed_body = self.body.trim_end();

        if trimmed_body.is_empty() {
            title_line
        } else {
            format!("{title_line}\n{trimmed_body}")
        }
    }

    /// Returns the readable selectable title used by the unified transcript surface.
    /// 返回统一转录表面使用的可读可选标题。
    pub fn selectable_title(&self) -> String {
        let base_label = match &self.kind {
            AgentViewDirectiveKind::UserPrompt => "用户指令",
            AgentViewDirectiveKind::Thinking => "思考过程",
            AgentViewDirectiveKind::ToolCall => "工具调用",
            AgentViewDirectiveKind::ToolResult => "工具结果",
            AgentViewDirectiveKind::Diff => "文件差异",
            AgentViewDirectiveKind::Notice => "系统提示",
            AgentViewDirectiveKind::Custom(label) if label == "context-summary" => "上下文总结",
            AgentViewDirectiveKind::Custom(label) => label.as_str(),
        };

        let title_suffix = self
            .attributes
            .get("title")
            .map(|title| title.trim())
            .filter(|title| !title.is_empty());

        match title_suffix {
            Some(title) => format!("[{base_label}] {title}"),
            None => format!("[{base_label}]"),
        }
    }
}

/// Semantic directive kinds supported by the first-generation agent-view pipeline.
/// 第一代 agent-view 管线支持的语义指令类型集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentViewDirectiveKind {
    /// User-authored prompt block shown with dedicated transcript chrome.
    /// 以专属转录外框展示的用户提示词区块。
    UserPrompt,
    /// Model thinking block that may float first and dock later.
    /// 可先浮动后停靠的模型思考区块。
    Thinking,
    /// Tool-call request block emitted by the runtime.
    /// 由运行时发出的工具调用请求区块。
    ToolCall,
    /// Tool-result block emitted after one tool finishes.
    /// 某个工具完成后发出的工具结果区块。
    ToolResult,
    /// File-diff or patch-summary block.
    /// 文件差异或补丁摘要区块。
    Diff,
    /// Informational notice block used for warnings and system hints.
    /// 用于告警与系统提示的信息区块。
    Notice,
    /// Custom extension directive kind preserved losslessly for forward compatibility.
    /// 为前向兼容而无损保留的自定义扩展指令类型。
    Custom(String),
}

impl AgentViewDirectiveKind {
    /// Returns the canonical directive label used in extension fences.
    /// 返回扩展围栏中使用的规范指令标签。
    pub fn as_label(&self) -> Cow<'_, str> {
        match self {
            Self::UserPrompt => Cow::Borrowed("user-prompt"),
            Self::Thinking => Cow::Borrowed("thinking"),
            Self::ToolCall => Cow::Borrowed("tool-call"),
            Self::ToolResult => Cow::Borrowed("tool-result"),
            Self::Diff => Cow::Borrowed("diff"),
            Self::Notice => Cow::Borrowed("notice"),
            Self::Custom(value) => Cow::Borrowed(value.as_str()),
        }
    }
}

/// Converts one Markdown source string into a copy-oriented plain-text projection.
/// 将一段 Markdown 源文本转换为面向复制场景的纯文本投影。
pub fn markdown_source_to_plain_text(source: &str) -> String {
    let parser = Parser::new_ext(source, Options::all());
    let mut output = String::new();

    for event in parser {
        match event {
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
                output.push_str(&text);
            }
            Event::SoftBreak | Event::HardBreak => output.push('\n'),
            Event::Rule => {
                if !output.ends_with('\n') && !output.is_empty() {
                    output.push('\n');
                }
                output.push_str("---");
                output.push('\n');
            }
            Event::TaskListMarker(checked) => {
                output.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::CodeBlock
                | TagEnd::BlockQuote(_)
                | TagEnd::HtmlBlock
                | TagEnd::Item
                | TagEnd::TableRow
                | TagEnd::FootnoteDefinition
                | TagEnd::DefinitionListTitle
                | TagEnd::DefinitionListDefinition,
            ) => {
                if !output.ends_with('\n') && !output.is_empty() {
                    output.push('\n');
                }
            }
            _ => {}
        }
    }

    output.trim_end().to_string()
}

/// Renders one directive block back into its fence-based Markdown-compatible source form.
/// 将一个指令区块重新渲染为基于围栏的 Markdown 兼容源码形式。
pub fn render_directive_source(
    kind: &AgentViewDirectiveKind,
    attributes: &AgentViewAttributes,
    body: &str,
) -> String {
    let mut source = format!(":::{}", kind.as_label());

    for (key, value) in attributes {
        source.push(' ');
        source.push_str(key);
        source.push('=');
        if value.contains(' ') {
            source.push('"');
            source.push_str(value);
            source.push('"');
        } else {
            source.push_str(value);
        }
    }

    source.push('\n');
    source.push_str(body);
    if !body.is_empty() && !body.ends_with('\n') {
        source.push('\n');
    }
    source.push_str(":::");
    source
}

#[cfg(test)]
mod tests {
    //! Targeted document-projection tests for unified transcript copy and selection flows.
    //! 面向统一转录复制与选择流程的定向文档投影测试。

    use super::{
        AgentViewAttributes, AgentViewDirectiveBlock, AgentViewDirectiveKind, AgentViewDocument,
        AgentViewToolPresentationKind,
    };

    /// Verifies that the selectable projection preserves Markdown syntax while still exposing directive titles.
    /// 验证可选投影会保留 Markdown 语法，同时仍然暴露指令标题。
    #[test]
    fn builds_selectable_projection_with_markdown_and_directive_titles() {
        let mut document = AgentViewDocument::new();
        document.push_markdown("## 结果\n\n支持 **Markdown**。\n");
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(String::from("title"), String::from("cargo check"));
        document.push_directive(AgentViewDirectiveBlock::new(
            AgentViewDirectiveKind::ToolCall,
            attributes,
            "cargo check -p vulcan-code-gui\n",
            ":::tool-call title=\"cargo check\"\ncargo check -p vulcan-code-gui\n:::",
            true,
        ));

        let selectable_text = document.selectable_text();

        assert!(selectable_text.contains("## 结果"));
        assert!(selectable_text.contains("**Markdown**"));
        assert!(selectable_text.contains("[工具调用] cargo check"));
        assert!(selectable_text.contains("cargo check -p vulcan-code-gui"));
    }

    /// Verifies that one persisted typed tool presentation attribute round-trips into the matching enum variant.
    /// 验证一条已持久化的强类型工具展示属性可以往返解析为匹配的枚举变体。
    #[test]
    fn parses_persisted_tool_presentation_kind_attribute() {
        let mut attributes = AgentViewAttributes::new();
        AgentViewToolPresentationKind::GeneratedVideo.write_to_attributes(&mut attributes);

        assert_eq!(
            AgentViewToolPresentationKind::from_attributes(&attributes),
            Some(AgentViewToolPresentationKind::GeneratedVideo)
        );
    }

    /// Verifies that the new command-session presentation kind can round-trip through persisted attributes.
    /// 验证新的 command-session 展示种类可以通过持久化属性完成往返解析。
    #[test]
    fn parses_command_session_tool_presentation_kind_attribute() {
        let mut attributes = AgentViewAttributes::new();
        AgentViewToolPresentationKind::CommandSession.write_to_attributes(&mut attributes);

        assert_eq!(
            AgentViewToolPresentationKind::from_attributes(&attributes),
            Some(AgentViewToolPresentationKind::CommandSession)
        );
    }

    /// Verifies that legacy string-based tool visual markers still map into the new generated-document presentation family.
    /// 验证旧版基于字符串的工具视觉标记仍可映射到新的文档生成展示家族。
    #[test]
    fn maps_legacy_tool_visual_markers_to_typed_presentation_kind() {
        let mut attributes = AgentViewAttributes::new();
        attributes.insert(
            String::from("tool-visual-kind"),
            String::from("generated-media"),
        );
        attributes.insert(String::from("generated-media-kind"), String::from("pdf"));

        assert_eq!(
            AgentViewToolPresentationKind::from_attributes(&attributes),
            Some(AgentViewToolPresentationKind::GeneratedDocument)
        );
    }
}
