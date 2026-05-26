/// Final destination used by one prompt segment.
/// 单个提示词片段的最终目标层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PromptTarget {
    /// System instruction stream.
    /// 系统指令流。
    System,
    /// Developer instruction stream.
    /// 开发者指令流。
    Developer,
    /// User-visible contextual stream.
    /// 用户侧上下文流。
    User,
    /// Tool-help metadata stream.
    /// 工具帮助元数据流。
    ToolMetadata,
    /// Diagnostics-only stream.
    /// 仅诊断流。
    DiagnosticsOnly,
}

/// Logical node kind used by diagnostics and ordering.
/// 诊断与排序使用的逻辑节点类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PromptNodeKind {
    /// System node.
    /// 系统节点。
    System,
    /// Developer node.
    /// 开发者节点。
    Developer,
    /// User node.
    /// 用户节点。
    User,
    /// Tool-help node.
    /// 工具帮助节点。
    ToolMetadata,
}

/// Prompt assembly mode used to select one strategy.
/// 选择策略的提示词装配模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PromptAssemblyMode {
    /// Primary/root agent composition.
    /// 主代理组合模式。
    Primary,
    /// Internal context-compaction composition.
    /// 内部上下文压缩组合模式。
    ContextCompaction,
    /// Subagent clone composition.
    /// 子代理分身组合模式。
    SubagentClone,
    /// Subagent functional composition.
    /// 子代理功能组合模式。
    SubagentFunctional,
}

/// Personality selection requested by the caller.
/// 调用方请求的人格选择。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersonalitySelection {
    /// Use the catalog default.
    /// 使用目录默认人格。
    Default,
    /// Use one named personality.
    /// 使用具名人格。
    Named(String),
    /// Use one inline personality payload.
    /// 使用内联人格内容。
    Inline {
        /// Stable inline personality identifier.
        /// 稳定的内联人格标识。
        id: String,
        /// Inline personality text body.
        /// 内联人格正文文本。
        text: String,
    },
    /// Disable personality injection.
    /// 禁用人格注入。
    Disabled,
}

/// Model capabilities used by prompt assembly.
/// 提示词装配使用的模型能力描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDescriptor {
    /// Provider/model family slug.
    /// Provider 或模型家族标识。
    pub family: String,
    /// Exact model identifier.
    /// 精确模型标识。
    pub model_id: String,
    /// Whether the model can absorb personality into system instructions.
    /// 模型是否可将人格直接吸收到系统提示词。
    pub supports_native_personality: bool,
}

/// Visible tool descriptor used by tool-help composition.
/// 工具帮助组合使用的可见工具描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDescriptor {
    /// Stable tool identifier.
    /// 稳定工具标识。
    pub id: String,
    /// Human-facing display title.
    /// 面向人的显示标题。
    pub title: String,
    /// Short capability description.
    /// 简短能力描述。
    pub description: String,
    /// Optional explicit help text override.
    /// 可选显式帮助文本覆盖。
    pub help_text: Option<String>,
    /// Optional schema fingerprint supplied by the caller.
    /// 调用方提供的可选 schema 指纹。
    pub schema_fingerprint: Option<String>,
}

/// Final rendered prompt segment.
/// 最终渲染后的提示词片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSegment {
    /// Stable node identifier.
    /// 稳定节点标识。
    pub node_id: String,
    /// Stable cache key.
    /// 稳定缓存键。
    pub cache_key: String,
    /// Logical node kind.
    /// 逻辑节点类型。
    pub kind: PromptNodeKind,
    /// Final target layer.
    /// 最终目标层。
    pub target: PromptTarget,
    /// Human-readable title.
    /// 可读标题。
    pub title: String,
    /// Final body text.
    /// 最终正文文本。
    pub body: String,
    /// Human-readable source label for diagnostics.
    /// 供诊断使用的人类可读来源标签。
    pub source_label: String,
    /// Whether the segment came from cache.
    /// 片段是否来自缓存。
    pub from_cache: bool,
}

impl PromptSegment {
    /// Build one fresh segment instance.
    /// 构建一个新的片段实例。
    pub fn new(
        node_id: impl Into<String>,
        cache_key: impl Into<String>,
        kind: PromptNodeKind,
        target: PromptTarget,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            cache_key: cache_key.into(),
            kind,
            target,
            title: title.into(),
            body: body.into(),
            source_label: String::from("unspecified"),
            from_cache: false,
        }
    }

    /// Attach one human-readable source label to this segment.
    /// 为该片段附加一个人类可读来源标签。
    pub fn with_source_label(mut self, source_label: impl Into<String>) -> Self {
        self.source_label = source_label.into();
        self
    }

    /// Clone the segment and mark it as cache-backed.
    /// 克隆片段并将其标记为来自缓存。
    pub fn cached_copy(&self) -> Self {
        let mut copy = self.clone();
        copy.from_cache = true;
        copy
    }
}
