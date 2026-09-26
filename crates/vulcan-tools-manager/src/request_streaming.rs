use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

/// Host-approved staging policy for one tool request parser.
/// 单个工具请求解析器使用的宿主批准暂存策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolRequestStagingPolicy {
    /// Never materialize streamed arguments outside the in-memory request assembler.
    /// 绝不在内存请求组装器之外落地流式参数。
    #[default]
    Forbidden,
    /// Allow a host-owned parser to stage decoded content without touching the final target.
    /// 允许宿主自有解析器暂存已解码内容，但不得触碰最终目标。
    HostTemporaryFile,
}

/// Stable staging state exposed by a safe parser preview.
/// 安全解析器预览暴露的稳定暂存状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolRequestStagingStatus {
    /// The tool has no staging operation.
    /// 当前工具没有暂存操作。
    #[default]
    NotApplicable,
    /// Decoded content is being written to a host-owned temporary resource.
    /// 已解码内容正在写入宿主管理的临时资源。
    Active,
    /// Staging failed and the request safely degraded to preview-only mode.
    /// 暂存失败，请求已安全降级为仅预览模式。
    Degraded,
    /// The staged resource passed final consistency validation.
    /// 暂存资源已通过最终一致性校验。
    Validated,
    /// The staged resource was removed after abort or validation mismatch.
    /// 暂存资源已在中止或校验不一致后删除。
    Discarded,
}

/// Throttling policy applied to generic token and parser-preview projections.
/// 应用于通用 Token 与解析器预览投影的节流策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolRequestProgressPolicy {
    /// Minimum interval between regular snapshots, even when byte growth is large.
    /// 常规快照的最小间隔，即使字节增长很大也不能绕过。
    pub minimum_interval_ms: u64,
    /// Minimum received-byte growth before another regular projection is eligible.
    /// 常规投影再次具备资格前所需的最小接收字节增长量。
    pub minimum_byte_growth: usize,
    /// Maximum interval in milliseconds between visible progress snapshots.
    /// 可见进度快照之间允许的最大毫秒间隔。
    pub maximum_interval_ms: u64,
    /// Minimum byte growth used after the request crosses the large-input threshold.
    /// 请求跨过大输入阈值后使用的最小字节增长量。
    pub large_input_minimum_byte_growth: usize,
    /// Input size at which large-request throttling becomes active.
    /// 启用大请求节流的输入大小。
    pub large_input_threshold_bytes: usize,
}

impl Default for ToolRequestProgressPolicy {
    /// Builds the host-wide bounded progress policy.
    /// 构建宿主统一的有界进度策略。
    ///
    /// # Returns
    /// 返回值
    /// The default policy used when a tool does not override progress cadence.
    /// 返回工具未覆盖进度节奏时使用的默认策略。
    fn default() -> Self {
        Self {
            minimum_interval_ms: 50,
            minimum_byte_growth: 256,
            maximum_interval_ms: 100,
            large_input_minimum_byte_growth: 16 * 1024,
            large_input_threshold_bytes: 1024 * 1024,
        }
    }
}

/// One bounded semantic line emitted by a request parser for structured preview renderers.
/// 请求解析器为结构化预览渲染器发出的一条有界语义行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRequestPreviewLine {
    /// Zero-based operation index in the eventual request.
    /// 最终请求中的零基操作序号。
    pub operation_index: usize,
    /// Zero-based affected-resource index.
    /// 受影响资源列表中的零基资源序号。
    pub resource_index: usize,
    /// Semantic side, normally `old` or `new`.
    /// 语义侧，通常为 `old` 或 `new`。
    pub side: String,
    /// Zero-based line index inside the selected array.
    /// 所选数组中的零基逻辑行序号。
    pub line_index: usize,
    /// One bounded logical-line content value.
    /// 一条有界逻辑行的内容值。
    pub content: String,
}

/// Declarative sensitivity boundary for one host-owned request preview.
/// 单个宿主自有请求预览的声明式敏感性边界。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolRequestSensitivityPolicy {
    /// Exact preview field identifiers approved for user-visible projection.
    /// 获准进行用户可见投影的精确预览字段标识。
    pub allowed_preview_fields: Vec<String>,
    /// Exact JSON paths that must never be copied into a preview.
    /// 绝不能复制进预览的精确 JSON 路径。
    pub redacted_json_paths: Vec<String>,
    /// Maximum UTF-8 bytes retained in the bounded body preview.
    /// 有界正文预览允许保留的最大 UTF-8 字节数。
    pub maximum_body_preview_bytes: usize,
    /// Maximum number of structured preview lines retained by the coordinator.
    /// 协调器允许保留的结构化预览行数上限。
    pub maximum_structured_preview_lines: usize,
    /// Maximum UTF-8 bytes retained across structured preview line content.
    /// 结构化预览逻辑行内容允许保留的 UTF-8 字节总上限。
    pub maximum_structured_preview_bytes: usize,
}

/// Request-scoped context supplied when a parser session is created.
/// 创建解析器会话时提供的请求级上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRequestParserContext {
    /// Stable physical request-stream identifier.
    /// 稳定的物理请求流标识。
    pub request_stream_id: String,
    /// Exact public tool name selected from the frozen execution snapshot.
    /// 从冻结执行快照中选择的精确公开工具名称。
    pub tool_name: String,
}

/// One segment in the canonical path of an incremental JSON value.
/// 增量 JSON 值规范路径中的一个片段。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncrementalJsonPathSegment {
    /// Object member selected by its fully decoded key.
    /// 通过完整解码键选择的对象成员。
    Key(String),
    /// Array item selected by its zero-based position.
    /// 通过从零开始的位置选择的数组元素。
    Index(usize),
}

/// Scalar value completed by the incremental JSON lexical layer.
/// 由增量 JSON 词法层完成的标量值。
#[derive(Debug, Clone, PartialEq)]
pub enum IncrementalJsonScalar {
    /// Finite or arbitrary-precision JSON number retained as exact source text.
    /// 以精确源文本保留的有限或任意精度 JSON 数字。
    Number(String),
    /// JSON boolean value.
    /// JSON 布尔值。
    Boolean(bool),
    /// JSON null value.
    /// JSON 空值。
    Null,
}

/// One stable semantic event emitted from an incomplete JSON argument stream.
/// 从不完整 JSON 参数流发出的单条稳定语义事件。
#[derive(Debug, Clone, PartialEq)]
pub enum IncrementalJsonEvent {
    /// Opens an object value at the supplied canonical path.
    /// 在给定规范路径打开对象值。
    ObjectStarted {
        /// Canonical path of the object value.
        /// 对象值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
    },
    /// Closes an object value at the supplied canonical path.
    /// 在给定规范路径关闭对象值。
    ObjectEnded {
        /// Canonical path of the object value.
        /// 对象值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
    },
    /// Opens an array value at the supplied canonical path.
    /// 在给定规范路径打开数组值。
    ArrayStarted {
        /// Canonical path of the array value.
        /// 数组值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
    },
    /// Closes an array value at the supplied canonical path.
    /// 在给定规范路径关闭数组值。
    ArrayEnded {
        /// Canonical path of the array value.
        /// 数组值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
    },
    /// Completes one decoded object key.
    /// 完成一个已解码对象键。
    ObjectKey {
        /// Canonical path of the object that owns the key.
        /// 拥有该键的对象规范路径。
        object_path: Vec<IncrementalJsonPathSegment>,
        /// Fully decoded member key.
        /// 完整解码后的成员键。
        key: String,
    },
    /// Opens one JSON string value.
    /// 打开一个 JSON 字符串值。
    StringStarted {
        /// Canonical path of the string value.
        /// 字符串值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
    },
    /// Emits a decoded non-empty fragment of one JSON string value.
    /// 发出一个 JSON 字符串值的非空已解码片段。
    StringContent {
        /// Canonical path of the string value.
        /// 字符串值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
        /// Decoded content fragment with escapes and surrogate pairs resolved.
        /// 已解析转义和代理对的解码内容碎片。
        content: String,
    },
    /// Closes one JSON string value.
    /// 关闭一个 JSON 字符串值。
    StringEnded {
        /// Canonical path of the string value.
        /// 字符串值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
    },
    /// Completes one non-string scalar value.
    /// 完成一个非字符串标量值。
    ScalarCompleted {
        /// Canonical path of the scalar value.
        /// 标量值的规范路径。
        path: Vec<IncrementalJsonPathSegment>,
        /// Exact completed scalar value.
        /// 精确完成的标量值。
        value: IncrementalJsonScalar,
    },
}

/// Sanitized parser snapshot eligible for runtime-event and WebUI projection.
/// 可用于运行时事件及 WebUI 投影的净化解析器快照。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolRequestPreview {
    /// Stable host-owned preview renderer key.
    /// 稳定的宿主自有预览渲染键。
    pub kind: String,
    /// Monotonic parser snapshot revision.
    /// 单调递增的解析器快照修订号。
    pub revision: u64,
    /// Small declared title fields safe for direct display.
    /// 可安全直接显示的小型声明式标题字段。
    pub title_fields: BTreeMap<String, Value>,
    /// Numeric or status progress fields safe for compact display.
    /// 可安全紧凑显示的数值或状态进度字段。
    pub progress_fields: BTreeMap<String, Value>,
    /// Optional bounded and already-redacted plain-text body preview.
    /// 可选的有界且已脱敏纯文本正文预览。
    pub body_preview: Option<String>,
    /// Bounded semantic lines for renderers that must not parse display text.
    /// 供不应解析展示文本的渲染器使用的有界语义行。
    pub structured_lines: Vec<ToolRequestPreviewLine>,
    /// Declared resources affected by the eventual request.
    /// 最终请求会影响的声明式资源。
    pub affected_resources: Vec<String>,
    /// Stable redaction markers without secret values.
    /// 不含敏感值的稳定脱敏标记。
    pub redaction_flags: Vec<String>,
    /// Current host-owned staging state.
    /// 当前宿主自有暂存状态。
    pub staging_status: ToolRequestStagingStatus,
}

/// Per-request parser instance that consumes semantic JSON events without executing the tool.
/// 按请求创建的解析器实例，消费语义 JSON 事件但不执行工具。
pub trait ToolRequestParserSession: Send {
    /// Consumes one canonical incremental JSON event.
    /// 消费一条规范增量 JSON 事件。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the next ordered semantic event for this exact request.
    /// `event` 是该精确请求的下一条有序语义事件。
    ///
    /// # Returns
    /// 返回值
    /// Success or an English host diagnostic that triggers preview-only degradation.
    /// 返回成功，或触发仅预览降级的英文宿主诊断。
    fn push_json_event(&mut self, event: &IncrementalJsonEvent) -> Result<(), String>;

    /// Returns the latest sanitized snapshot when semantic preview state changed.
    /// 当语义预览状态发生变化时返回最近的净化快照。
    ///
    /// # Returns
    /// 返回值
    /// The latest safe snapshot, or `None` before any declared field becomes available.
    /// 返回最近的安全快照；在声明字段可用前返回 `None`。
    fn snapshot(&self) -> Option<ToolRequestPreview>;

    /// Validates parser and staging state against the authoritative complete arguments.
    /// 根据权威完整参数校验解析器与暂存状态。
    ///
    /// # Parameters
    /// 参数
    /// `arguments` is the fully parsed JSON value accepted by the existing invocation path.
    /// `arguments` 是现有调用链接受的完整 JSON 值。
    ///
    /// # Returns
    /// 返回值
    /// The final safe snapshot, or an English consistency diagnostic.
    /// 返回最终安全快照，或英文一致性诊断。
    fn finish(&mut self, arguments: &Value) -> Result<Option<ToolRequestPreview>, String>;

    /// Releases parser-owned staging resources after cancellation, retry, or invalid input.
    /// 在取消、重试或无效输入后释放解析器持有的暂存资源。
    fn abort(&mut self);
}

/// Host-owned factory that creates a fresh parser session for each logical request.
/// 宿主自有工厂，为每个逻辑请求创建全新的解析器会话。
pub trait ToolRequestParserFactory: Send + Sync {
    /// Creates one isolated parser session.
    /// 创建一个隔离的解析器会话。
    ///
    /// # Parameters
    /// 参数
    /// `context` carries stable request identity but no raw arguments.
    /// `context` 携带稳定请求身份但不包含原始参数。
    ///
    /// # Returns
    /// 返回值
    /// A new parser instance owned exclusively by the logical request.
    /// 返回由该逻辑请求独占的新解析器实例。
    fn create(&self, context: ToolRequestParserContext) -> Box<dyn ToolRequestParserSession>;
}

/// Execution-only request-streaming capability frozen with one resolved tool definition.
/// 随单个已解析工具定义冻结的仅执行期请求流能力。
#[derive(Clone)]
pub struct ToolRequestStreamingCapability {
    /// Optional host-approved parser factory; absence means token-and-byte progress only.
    /// 可选的宿主批准解析器工厂；缺省时仅显示 Token 与字节进度。
    pub parser_factory: Option<Arc<dyn ToolRequestParserFactory>>,
    /// Optional stable preview renderer key.
    /// 可选的稳定预览渲染键。
    pub preview_kind: Option<String>,
    /// Host-approved staging boundary.
    /// 宿主批准的暂存边界。
    pub staging_policy: ToolRequestStagingPolicy,
    /// Declared preview and redaction policy.
    /// 声明式预览与脱敏策略。
    pub sensitivity_policy: ToolRequestSensitivityPolicy,
    /// Per-tool projection cadence.
    /// 按工具设置的投影节奏。
    pub progress_policy: ToolRequestProgressPolicy,
}

impl Debug for ToolRequestStreamingCapability {
    /// Renders capability metadata without exposing the opaque parser factory.
    /// 在不暴露不透明解析器工厂的前提下渲染能力元数据。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the compact debug representation.
    /// `formatter` 接收紧凑调试表示。
    ///
    /// # Returns
    /// 返回值
    /// Standard formatting success or failure.
    /// 返回标准格式化成功或失败。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolRequestStreamingCapability")
            .field("has_parser_factory", &self.parser_factory.is_some())
            .field("preview_kind", &self.preview_kind)
            .field("staging_policy", &self.staging_policy)
            .field("sensitivity_policy", &self.sensitivity_policy)
            .field("progress_policy", &self.progress_policy)
            .finish()
    }
}
