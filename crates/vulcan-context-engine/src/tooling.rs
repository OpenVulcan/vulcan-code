//! OCE-inspired tool-surface mapping for the VCE host boundary.
//! 面向 VCE 宿主边界的 OCE 工具面映射。
//!
//! This module describes tool registration facts without owning MCP, execution, storage, or
//! provider infrastructure.
//! 本模块只描述工具注册事实，不持有 MCP、执行、存储或模型服务基础设施。

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Canonical Vulcan-facing identifier for the single AI-visible semantic retrieval entry.
/// 面向 Vulcan 的唯一 AI 可见语义检索入口规范标识。
pub const CODEBASE_RETRIEVAL_TOOL_ID: &str = "codebase_retrieval";

/// Original OCE-client public tool name retained as descriptor metadata.
/// 作为描述元数据保留的 OCE-client 原始公开工具名称。
pub const CODEBASE_RETRIEVAL_TOOL_NAME: &str = "codebase-retrieval";

/// English model-facing description adapted to the VCE host-owned lifecycle boundary.
/// 按 VCE 宿主持有生命周期边界调整的英文模型侧工具描述。
pub const CODEBASE_RETRIEVAL_TOOL_DESCRIPTION: &str = "Search the host-maintained current workspace checkpoint for code matching a natural-language information request. The result returns relevant cross-language source context; workspace refresh and index lifecycle remain host-managed.";

/// Identify the execution ownership of one mapped VCE operation.
/// 标识单个 VCE 映射操作的执行所有权。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VceToolScope {
    /// A workspace-scoped operation that may be exposed to the AI.
    /// 可向 AI 暴露的工作区范围操作。
    Workspace,
    /// A host lifecycle operation that must remain outside the AI tool surface.
    /// 必须保持在 AI 工具面之外的宿主生命周期操作。
    HostOnly,
}

/// Stable semantic operation names extracted from the OCE server and client boundaries.
/// 从 OCE 服务端和客户端边界提取的稳定语义操作名称。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VceToolOperation {
    /// Retrieve semantic code context for one authorized workspace.
    /// 为一个已授权工作区检索代码语义上下文。
    CodebaseRetrieval,
    /// Classify which content-addressed blobs are unknown or not indexed.
    /// 区分内容寻址 Blob 是未知还是尚未索引。
    FindMissing,
    /// Upload path and content records to the host-owned indexing boundary.
    /// 将路径和内容记录上传到宿主持有的索引边界。
    BatchUpload,
    /// Read indexing and checkpoint readiness for a set of blobs.
    /// 查询一组 Blob 的索引和 checkpoint 就绪状态。
    BlobStatus,
    /// Advance the workspace checkpoint with added and deleted Blob identities.
    /// 使用新增和删除 Blob 身份推进工作区 checkpoint。
    CheckpointBlobs,
    /// Reconcile the complete workspace and submit the resulting sync plan.
    /// 协调完整工作区并提交生成的同步计划。
    Sync,
    /// Reconcile only explicitly changed paths and submit the resulting sync plan.
    /// 只协调明确变化的路径并提交生成的同步计划。
    SyncPaths,
    /// Read the local workspace/index lifecycle state without retrieval.
    /// 读取本地工作区和索引生命周期状态，不执行检索。
    Status,
    /// Put unsaved editor content into the explicit workspace overlay.
    /// 将未保存编辑器内容写入显式工作区覆盖层。
    Observe,
    /// Mark one workspace-relative path as removed.
    /// 将一个工作区相对路径标记为已删除。
    Remove,
    /// Start or maintain the host-owned filesystem watch lifecycle.
    /// 启动或维护宿主持有的文件监听生命周期。
    Watch,
}

impl VceToolOperation {
    /// Return the stable snake-case operation identifier used by the host adapter.
    /// 返回宿主适配器使用的稳定蛇形操作标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CodebaseRetrieval => CODEBASE_RETRIEVAL_TOOL_ID,
            Self::FindMissing => "find_missing",
            Self::BatchUpload => "batch_upload",
            Self::BlobStatus => "blob_status",
            Self::CheckpointBlobs => "checkpoint_blobs",
            Self::Sync => "sync",
            Self::SyncPaths => "sync_paths",
            Self::Status => "status",
            Self::Observe => "observe",
            Self::Remove => "remove",
            Self::Watch => "watch",
        }
    }

    /// Return whether this operation is the single AI-visible retrieval entry.
    /// 返回当前操作是否是唯一的 AI 可见检索入口。
    pub const fn is_ai_visible(self) -> bool {
        matches!(self, Self::CodebaseRetrieval)
    }

    /// Return the scope ownership required before the host registers this operation.
    /// 返回宿主注册当前操作前必须识别的范围所有权。
    pub const fn scope(self) -> VceToolScope {
        if self.is_ai_visible() {
            VceToolScope::Workspace
        } else {
            VceToolScope::HostOnly
        }
    }
}

/// One descriptor aligned with the fields consumed by VulcanCode native candidates.
/// 一个与 VulcanCode 原生候选工具消费字段对齐的描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceToolDescriptor {
    /// Stable host-facing tool identifier.
    /// 稳定的宿主侧工具标识。
    pub id: String,
    /// Human-readable tool name retained for provider projection.
    /// 为 Provider 投影保留的人类可读工具名称。
    pub name: String,
    /// English model-facing description.
    /// 英文模型侧工具描述。
    pub description: String,
    /// Optional English Markdown guidance for prompt assembly.
    /// 供提示词组装使用的可选英文 Markdown 指引。
    pub help_text: Option<String>,
    /// Whether the descriptor may be included in the AI-visible tool set.
    /// 当前描述是否可以进入 AI 可见工具集合。
    pub ai_visible: bool,
    /// Whether the operation is safe to invoke while Vulcan Plan mode is active.
    /// 当前操作是否允许在 Vulcan Plan 模式激活时调用。
    pub available_in_plan: bool,
}

/// One mapped operation definition that the upper host can project into its tool manager.
/// 上层宿主可以投影到工具管理器中的单个映射操作定义。
#[derive(Debug, Clone, PartialEq)]
pub struct VceToolDefinition {
    /// Descriptor metadata aligned with a Vulcan native candidate.
    /// 与 Vulcan 原生候选工具对齐的描述元数据。
    pub descriptor: VceToolDescriptor,
    /// JSON Schema intended for the model-visible or host-internal registration surface.
    /// 面向模型可见或宿主内部注册面的 JSON Schema。
    pub parameters_schema: Value,
    /// Domain operation selected after tool-name resolution.
    /// 工具名称解析后选择的领域操作。
    pub operation: VceToolOperation,
    /// Ownership boundary that the upper host must enforce.
    /// 上层宿主必须执行的所有权边界。
    pub scope: VceToolScope,
}

/// Typed arguments accepted by the OCE-compatible AI retrieval entry.
/// OCE 兼容 AI 检索入口接受的类型化参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodebaseRetrievalRequest {
    /// Natural-language description of the code context required by the model.
    /// 模型需要的代码上下文自然语言描述。
    pub information_request: String,
}

/// Raw arguments for one host-owned lifecycle operation.
/// 单个宿主持有生命周期操作的原始参数。
#[derive(Debug, Clone, PartialEq)]
pub struct HostOperationRequest {
    /// Stable VCE operation selected by the catalog.
    /// 由目录选择的稳定 VCE 操作。
    pub operation: VceToolOperation,
    /// Unmodified JSON arguments retained for the storage/indexing owner.
    /// 为存储或索引所有者保留的未修改 JSON 参数。
    pub arguments: Value,
}

/// Parsed VCE tool call separated into AI retrieval and host lifecycle variants.
/// 将 VCE 工具调用区分为 AI 检索和宿主生命周期变体。
#[derive(Debug, Clone, PartialEq)]
pub enum VceToolCall {
    /// One validated model-facing code retrieval request.
    /// 一条经过校验的模型侧代码检索请求。
    CodebaseRetrieval(CodebaseRetrievalRequest),
    /// One host-only operation preserving its exact JSON payload.
    /// 保留精确 JSON 载荷的一条宿主专用操作。
    HostOperation(HostOperationRequest),
}

/// Errors raised while resolving a mapped tool or validating its boundary arguments.
/// 解析映射工具或校验边界参数时产生的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolMappingError {
    /// The requested tool identifier is not present in the catalog.
    /// 请求的工具标识不在工具目录中。
    UnknownTool {
        /// Tool identifier absent from the static VCE catalog.
        /// 静态 VCE 目录中不存在的工具标识。
        tool_id: String,
    },
    /// The requested tool is host-only and cannot be invoked through the AI surface.
    /// 请求的工具仅供宿主使用，不能通过 AI 工具面调用。
    ToolNotAiVisible {
        /// Host-only tool identifier rejected at the AI boundary.
        /// 在 AI 边界被拒绝的宿主专用工具标识。
        tool_id: String,
    },
    /// The invocation payload must be a JSON object.
    /// 工具调用载荷必须是 JSON 对象。
    ArgumentsMustBeObject {
        /// Tool identifier whose arguments used an invalid JSON shape.
        /// 参数使用无效 JSON 形态的工具标识。
        tool_id: String,
    },
    /// A required argument is absent.
    /// 缺少必需参数。
    MissingArgument {
        /// Tool identifier requiring the absent argument.
        /// 要求缺失参数的工具标识。
        tool_id: String,
        /// Required argument that was absent.
        /// 实际缺失的必需参数。
        argument: String,
    },
    /// An argument has a JSON type different from the registered schema.
    /// 参数的 JSON 类型与注册 Schema 不同。
    InvalidArgumentType {
        /// Tool identifier owning the invalid argument.
        /// 拥有无效参数的工具标识。
        tool_id: String,
        /// Invalid argument name.
        /// 无效参数名称。
        argument: String,
        /// Expected JSON type rendered in the error.
        /// 错误中展示的期望 JSON 类型。
        expected: String,
    },
    /// A required string argument contains no non-whitespace characters.
    /// 必需字符串参数不包含任何非空白字符。
    EmptyArgument {
        /// Tool identifier receiving the empty argument.
        /// 接收到空参数的工具标识。
        tool_id: String,
        /// Required string argument containing no usable text.
        /// 不包含可用文本的必需字符串参数。
        argument: String,
    },
    /// The model attempted to provide a routing field owned by the Vulcan host.
    /// 模型尝试提供由 Vulcan 宿主持有的路由字段。
    HostManagedArgument {
        /// Tool identifier receiving an unauthorized routing field.
        /// 接收到未授权路由字段的工具标识。
        tool_id: String,
        /// Routing argument reserved for the Vulcan host.
        /// 为 Vulcan 宿主保留的路由参数。
        argument: String,
    },
    /// An argument is not part of the mapped public schema.
    /// 参数不属于映射后的公开 Schema。
    UnexpectedArgument {
        /// Tool identifier receiving an undeclared argument.
        /// 接收到未声明参数的工具标识。
        tool_id: String,
        /// Argument absent from the registered public Schema.
        /// 注册公开 Schema 中不存在的参数。
        argument: String,
    },
}

impl Display for ToolMappingError {
    /// Render one explicit English tool-boundary error for the AI/runtime surface.
    /// 为 AI 和运行时表面渲染一条明确的英文工具边界错误。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTool { tool_id } => {
                write!(formatter, "VCE tool `{tool_id}` was not found.")
            }
            Self::ToolNotAiVisible { tool_id } => {
                write!(
                    formatter,
                    "VCE tool `{tool_id}` is host-only and is not AI-visible."
                )
            }
            Self::ArgumentsMustBeObject { tool_id } => {
                write!(
                    formatter,
                    "VCE tool `{tool_id}` arguments must be a JSON object."
                )
            }
            Self::MissingArgument { tool_id, argument } => {
                write!(
                    formatter,
                    "VCE tool `{tool_id}` requires argument `{argument}`."
                )
            }
            Self::InvalidArgumentType {
                tool_id,
                argument,
                expected,
            } => write!(
                formatter,
                "VCE tool `{tool_id}` argument `{argument}` must be {expected}."
            ),
            Self::EmptyArgument { tool_id, argument } => write!(
                formatter,
                "VCE tool `{tool_id}` argument `{argument}` must not be empty."
            ),
            Self::HostManagedArgument { tool_id, argument } => write!(
                formatter,
                "VCE tool `{tool_id}` argument `{argument}` is host-managed and must not be provided by the model."
            ),
            Self::UnexpectedArgument { tool_id, argument } => write!(
                formatter,
                "VCE tool `{tool_id}` does not accept argument `{argument}`."
            ),
        }
    }
}

impl Error for ToolMappingError {}

/// Catalog containing the one public retrieval entry and host-only lifecycle definitions.
/// 包含唯一公开检索入口和宿主专用生命周期定义的工具目录。
#[derive(Debug, Clone)]
pub struct VceToolCatalog {
    /// Definitions keyed by their single canonical Vulcan-facing identifier.
    /// 以唯一规范 Vulcan 侧标识为键保存的工具定义。
    definitions: BTreeMap<String, VceToolDefinition>,
}

impl Default for VceToolCatalog {
    /// Build the default OCE-inspired VCE tool surface.
    /// 构建默认的 OCE 思路 VCE 工具面。
    fn default() -> Self {
        Self::new()
    }
}

impl VceToolCatalog {
    /// Build a catalog without registering a transport, provider, or storage implementation.
    /// 构建不注册传输、模型服务或存储实现的工具目录。
    pub fn new() -> Self {
        let definitions = mapped_tool_definitions()
            .into_iter()
            .map(|definition| (definition.descriptor.id.clone(), definition))
            .collect();
        Self { definitions }
    }

    /// Return one canonical definition for a Vulcan or OCE-compatible public identifier.
    /// 根据 Vulcan 或 OCE 兼容公开标识返回一条规范工具定义。
    pub fn definition(&self, tool_id: &str) -> Option<&VceToolDefinition> {
        let canonical_id = canonical_public_tool_id(tool_id)?;
        self.definitions.get(canonical_id)
    }

    /// Return all mapped definitions in stable identifier order.
    /// 按稳定标识顺序返回全部映射定义。
    pub fn definitions(&self) -> impl Iterator<Item = &VceToolDefinition> {
        self.definitions.values()
    }

    /// Return only definitions safe for the AI-visible Vulcan tool surface.
    /// 只返回可以进入 AI 可见 Vulcan 工具面的定义。
    pub fn ai_visible_definitions(&self) -> Vec<&VceToolDefinition> {
        self.definitions
            .values()
            .filter(|definition| definition.descriptor.ai_visible)
            .collect()
    }

    /// Return only host-owned lifecycle definitions for upper-layer registration.
    /// 只返回供上层注册的宿主自有生命周期定义。
    pub fn host_only_definitions(&self) -> Vec<&VceToolDefinition> {
        self.definitions
            .values()
            .filter(|definition| definition.scope == VceToolScope::HostOnly)
            .collect()
    }

    /// Parse one call for either the AI surface or an upper host lifecycle adapter.
    /// 为 AI 表面或上层宿主生命周期适配器解析一条调用。
    pub fn parse_call(
        &self,
        tool_id: &str,
        arguments: Value,
    ) -> Result<VceToolCall, ToolMappingError> {
        let definition = self
            .definition(tool_id)
            .ok_or_else(|| ToolMappingError::UnknownTool {
                tool_id: tool_id.to_string(),
            })?;
        match definition.operation {
            VceToolOperation::CodebaseRetrieval => {
                parse_codebase_retrieval_arguments(arguments).map(VceToolCall::CodebaseRetrieval)
            }
            operation => Ok(VceToolCall::HostOperation(HostOperationRequest {
                operation,
                arguments: require_object_arguments(definition.descriptor.id.as_str(), arguments)?,
            })),
        }
    }

    /// Parse one model-originated call while rejecting every host-only operation.
    /// 解析一条模型来源调用并拒绝所有宿主专用操作。
    pub fn parse_ai_call(
        &self,
        tool_id: &str,
        arguments: Value,
    ) -> Result<VceToolCall, ToolMappingError> {
        let definition = self
            .definition(tool_id)
            .ok_or_else(|| ToolMappingError::UnknownTool {
                tool_id: tool_id.to_string(),
            })?;
        if !definition.descriptor.ai_visible {
            return Err(ToolMappingError::ToolNotAiVisible {
                tool_id: definition.descriptor.id.clone(),
            });
        }
        self.parse_call(tool_id, arguments)
    }
}

/// Resolve the canonical Vulcan identifier for the OCE public name without registering an alias.
/// 将 OCE 公开名称解析为规范 Vulcan 标识，但不注册额外别名。
pub fn canonical_public_tool_id(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        CODEBASE_RETRIEVAL_TOOL_ID | CODEBASE_RETRIEVAL_TOOL_NAME => {
            Some(CODEBASE_RETRIEVAL_TOOL_ID)
        }
        "find_missing" => Some("find_missing"),
        "batch_upload" => Some("batch_upload"),
        "blob_status" => Some("blob_status"),
        "checkpoint_blobs" => Some("checkpoint_blobs"),
        "sync" => Some("sync"),
        "sync_paths" => Some("sync_paths"),
        "status" => Some("status"),
        "observe" => Some("observe"),
        "remove" => Some("remove"),
        "watch" => Some("watch"),
        _ => None,
    }
}

/// Parse the public OCE retrieval payload while keeping workspace routing host-owned.
/// 解析公开 OCE 检索载荷，同时保持工作区路由由宿主持有。
pub fn parse_codebase_retrieval_arguments(
    arguments: Value,
) -> Result<CodebaseRetrievalRequest, ToolMappingError> {
    let tool_id = CODEBASE_RETRIEVAL_TOOL_ID;
    let object = match arguments {
        Value::Object(object) => object,
        _ => {
            return Err(ToolMappingError::ArgumentsMustBeObject {
                tool_id: tool_id.to_string(),
            });
        }
    };
    for argument in object.keys() {
        if argument == "workspace_folder" {
            return Err(ToolMappingError::HostManagedArgument {
                tool_id: tool_id.to_string(),
                argument: argument.clone(),
            });
        }
        if argument != "information_request" {
            return Err(ToolMappingError::UnexpectedArgument {
                tool_id: tool_id.to_string(),
                argument: argument.clone(),
            });
        }
    }
    let information_request = object
        .get("information_request")
        .ok_or_else(|| ToolMappingError::MissingArgument {
            tool_id: tool_id.to_string(),
            argument: String::from("information_request"),
        })?
        .as_str()
        .ok_or_else(|| ToolMappingError::InvalidArgumentType {
            tool_id: tool_id.to_string(),
            argument: String::from("information_request"),
            expected: String::from("a string"),
        })?;
    if information_request.trim().is_empty() {
        return Err(ToolMappingError::EmptyArgument {
            tool_id: tool_id.to_string(),
            argument: String::from("information_request"),
        });
    }
    Ok(CodebaseRetrievalRequest {
        information_request: information_request.to_string(),
    })
}

/// Require a host operation payload to remain an object while preserving every field.
/// 要求宿主操作载荷保持对象形态，同时保留全部字段。
fn require_object_arguments(tool_id: &str, arguments: Value) -> Result<Value, ToolMappingError> {
    if matches!(arguments, Value::Object(_)) {
        Ok(arguments)
    } else {
        Err(ToolMappingError::ArgumentsMustBeObject {
            tool_id: tool_id.to_string(),
        })
    }
}

/// Build every mapped operation with stable descriptors and JSON Schemas.
/// 使用稳定描述和 JSON Schema 构建全部映射操作。
fn mapped_tool_definitions() -> Vec<VceToolDefinition> {
    vec![
        definition(
            VceToolOperation::CodebaseRetrieval,
            CODEBASE_RETRIEVAL_TOOL_NAME,
            CODEBASE_RETRIEVAL_TOOL_DESCRIPTION,
            codebase_retrieval_schema(),
            true,
            true,
            VceToolScope::Workspace,
        ),
        definition(
            VceToolOperation::FindMissing,
            "find_missing",
            "Classify content-addressed blobs as unknown or not indexed for the current workspace.",
            find_missing_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
        definition(
            VceToolOperation::BatchUpload,
            "batch_upload",
            "Submit changed path and content records to the host-owned indexing boundary.",
            batch_upload_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
        definition(
            VceToolOperation::BlobStatus,
            "blob_status",
            "Read Blob indexing and checkpoint readiness for a host-owned synchronization attempt.",
            blobs_envelope_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
        definition(
            VceToolOperation::CheckpointBlobs,
            "checkpoint_blobs",
            "Advance the host-owned workspace checkpoint using added and deleted Blob identities.",
            blobs_envelope_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
        definition(
            VceToolOperation::Sync,
            "sync",
            "Reconcile the complete workspace and submit its durable synchronization plan.",
            empty_object_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
        definition(
            VceToolOperation::SyncPaths,
            "sync_paths",
            "Reconcile explicitly changed workspace paths and submit their durable synchronization plan.",
            sync_paths_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
        definition(
            VceToolOperation::Status,
            "status",
            "Read local workspace and index lifecycle state without triggering retrieval.",
            empty_object_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
        definition(
            VceToolOperation::Observe,
            "observe",
            "Put unsaved editor content into the explicit workspace overlay.",
            observe_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
        definition(
            VceToolOperation::Remove,
            "remove",
            "Mark one workspace-relative path as removed from the current snapshot.",
            remove_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
        definition(
            VceToolOperation::Watch,
            "watch",
            "Start or maintain the host-owned filesystem watch lifecycle.",
            watch_schema(),
            false,
            false,
            VceToolScope::HostOnly,
        ),
    ]
}

/// Construct one mapped definition while keeping descriptor and operation metadata together.
/// 构建单个映射定义，同时保持描述和操作元数据的一致性。
fn definition(
    operation: VceToolOperation,
    name: &str,
    description: &str,
    parameters_schema: Value,
    ai_visible: bool,
    available_in_plan: bool,
    scope: VceToolScope,
) -> VceToolDefinition {
    VceToolDefinition {
        descriptor: VceToolDescriptor {
            id: operation.as_str().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            help_text: None,
            ai_visible,
            available_in_plan,
        },
        parameters_schema,
        operation,
        scope,
    }
}

/// Build the model-visible schema for the OCE semantic retrieval entry.
/// 构建 OCE 语义检索入口的模型可见 Schema。
fn codebase_retrieval_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "information_request": {
                "type": "string",
                "description": "A description of the code information you need.",
                "minLength": 1
            }
        },
        "required": ["information_request"],
        "additionalProperties": false
    })
}

/// Build the exact OCE server-side missing-Blob request shape for host use.
/// 构建供宿主使用的 OCE 服务端缺失 Blob 请求形状。
fn find_missing_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "mem_object_names": {
                "type": "array",
                "items": {"type": "string"},
                "default": []
            }
        },
        "additionalProperties": false
    })
}

/// Build the exact OCE batch-upload request shape for host use.
/// 构建供宿主使用的 OCE 批量上传请求形状。
fn batch_upload_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "blobs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "content": {"type": "string"},
                        "path": {"type": "string", "minLength": 1}
                    },
                    "required": ["content", "path"],
                    "additionalProperties": false
                },
                "default": []
            },
            "checkpoint_id": {"type": "string", "default": ""}
        },
        "additionalProperties": false
    })
}

/// Build the shared OCE Blob envelope used by status and checkpoint operations.
/// 构建状态和 checkpoint 操作共用的 OCE Blob 信封。
fn blobs_envelope_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "blobs": {
                "type": "object",
                "properties": {
                    "checkpoint_id": {"type": "string", "default": ""},
                    "added_blobs": {
                        "type": "array",
                        "items": {"type": "string"},
                        "default": []
                    },
                    "deleted_blobs": {
                        "type": "array",
                        "items": {"type": "string"},
                        "default": []
                    }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}

/// Build the empty object schema used by lifecycle operations without arguments.
/// 构建无参数生命周期操作使用的空对象 Schema。
fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

/// Build the changed-path payload used by client-side incremental synchronization.
/// 构建客户端增量同步使用的变化路径载荷。
fn sync_paths_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "changed_paths": {
                "type": "array",
                "items": {"type": "string"},
                "default": []
            }
        },
        "additionalProperties": false
    })
}

/// Build the unsaved editor overlay payload used by the client context.
/// 构建客户端上下文使用的未保存编辑器覆盖载荷。
fn observe_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "minLength": 1},
            "content": {"type": "string"}
        },
        "required": ["path", "content"],
        "additionalProperties": false
    })
}

/// Build the workspace-relative deletion payload used by the client context.
/// 构建客户端上下文使用的工作区相对路径删除载荷。
fn remove_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "minLength": 1}
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

/// Build the host-owned filesystem watcher lifecycle payload.
/// 构建宿主持有文件监听生命周期使用的载荷。
fn watch_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "debounce_ms": {"type": "integer", "minimum": 0, "default": 300}
        },
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify exactly one semantic retrieval definition is AI-visible.
    /// 验证恰好只有一个语义检索定义对 AI 可见。
    #[test]
    fn only_codebase_retrieval_is_ai_visible() {
        let catalog = VceToolCatalog::new();
        let visible = catalog.ai_visible_definitions();

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].descriptor.id, CODEBASE_RETRIEVAL_TOOL_ID);
        assert_eq!(visible[0].descriptor.name, CODEBASE_RETRIEVAL_TOOL_NAME);
        assert!(visible[0].descriptor.available_in_plan);
        assert_eq!(visible[0].scope, VceToolScope::Workspace);
    }

    /// Verify lifecycle operations remain host-only and are not accidentally exposed as aliases.
    /// 验证生命周期操作保持宿主专用，且不会意外暴露为 AI 别名。
    #[test]
    fn lifecycle_operations_are_host_only() {
        let catalog = VceToolCatalog::new();

        assert_eq!(catalog.host_only_definitions().len(), 10);
        for definition in catalog.host_only_definitions() {
            assert!(!definition.descriptor.ai_visible);
            assert!(!definition.descriptor.available_in_plan);
            assert_eq!(definition.scope, VceToolScope::HostOnly);
        }
        assert_eq!(catalog.definitions().count(), 11);
    }

    /// Verify both OCE and Vulcan public spellings resolve to one canonical definition.
    /// 验证 OCE 和 Vulcan 两种公开拼写解析到同一个规范定义。
    #[test]
    fn public_name_does_not_create_duplicate_surface() {
        let catalog = VceToolCatalog::new();

        assert_eq!(
            canonical_public_tool_id(CODEBASE_RETRIEVAL_TOOL_NAME),
            Some(CODEBASE_RETRIEVAL_TOOL_ID)
        );
        assert!(std::ptr::eq(
            catalog.definition(CODEBASE_RETRIEVAL_TOOL_NAME).unwrap(),
            catalog.definition(CODEBASE_RETRIEVAL_TOOL_ID).unwrap()
        ));
    }

    /// Verify the model-visible schema keeps workspace routing outside model arguments.
    /// 验证模型可见 Schema 将工作区路由保留在模型参数之外。
    #[test]
    fn retrieval_schema_requires_only_information_request() {
        let catalog = VceToolCatalog::new();
        let schema = &catalog
            .definition(CODEBASE_RETRIEVAL_TOOL_ID)
            .unwrap()
            .parameters_schema;

        assert_eq!(schema["required"], json!(["information_request"]));
        assert!(schema["properties"].get("workspace_folder").is_none());
    }

    /// Verify a valid retrieval call preserves the exact natural-language request.
    /// 验证有效检索调用保留精确的自然语言请求。
    #[test]
    fn parses_retrieval_request() {
        let catalog = VceToolCatalog::new();
        let call = catalog
            .parse_ai_call(
                CODEBASE_RETRIEVAL_TOOL_NAME,
                json!({"information_request": "Find the embedding registration path."}),
            )
            .unwrap();

        assert_eq!(
            call,
            VceToolCall::CodebaseRetrieval(CodebaseRetrievalRequest {
                information_request: String::from("Find the embedding registration path."),
            })
        );
    }

    /// Verify empty and non-object model payloads fail explicitly.
    /// 验证空载荷和非对象模型载荷都会显式失败。
    #[test]
    fn rejects_invalid_retrieval_arguments() {
        let catalog = VceToolCatalog::new();

        assert!(matches!(
            catalog.parse_ai_call(
                CODEBASE_RETRIEVAL_TOOL_ID,
                json!({"information_request": "  "})
            ),
            Err(ToolMappingError::EmptyArgument { .. })
        ));
        assert!(matches!(
            catalog.parse_ai_call(CODEBASE_RETRIEVAL_TOOL_ID, Value::Null),
            Err(ToolMappingError::ArgumentsMustBeObject { .. })
        ));
    }

    /// Verify model-provided workspace routing is rejected instead of silently trusted.
    /// 验证模型提供的工作区路由会被拒绝，而不会被静默信任。
    #[test]
    fn rejects_model_workspace_routing() {
        let catalog = VceToolCatalog::new();

        assert!(matches!(
            catalog.parse_ai_call(
                CODEBASE_RETRIEVAL_TOOL_ID,
                json!({
                    "information_request": "Find the entry point.",
                    "workspace_folder": "D:/other-project"
                })
            ),
            Err(ToolMappingError::HostManagedArgument { .. })
        ));
    }

    /// Verify host lifecycle payloads stay raw and are not conflated with AI retrieval.
    /// 验证宿主生命周期载荷保持原样，且不会与 AI 检索混淆。
    #[test]
    fn preserves_host_operation_payload() {
        let catalog = VceToolCatalog::new();
        let arguments = json!({
            "changed_paths": ["src/lib.rs", "src/tooling.rs"]
        });
        let call = catalog.parse_call("sync_paths", arguments.clone()).unwrap();

        assert_eq!(
            call,
            VceToolCall::HostOperation(HostOperationRequest {
                operation: VceToolOperation::SyncPaths,
                arguments,
            })
        );
    }

    /// Verify a host-only operation cannot enter the model-originated call path.
    /// 验证宿主专用操作不能进入模型来源调用路径。
    #[test]
    fn rejects_host_operation_from_ai_surface() {
        let catalog = VceToolCatalog::new();
        let error = catalog
            .parse_ai_call("batch_upload", json!({"blobs": []}))
            .unwrap_err();

        assert_eq!(
            error,
            ToolMappingError::ToolNotAiVisible {
                tool_id: String::from("batch_upload")
            }
        );
    }

    /// Verify unknown identifiers are explicit and never treated as a fallback operation.
    /// 验证未知标识会显式报错，绝不会被当作回退操作。
    #[test]
    fn rejects_unknown_tool() {
        let catalog = VceToolCatalog::new();
        let error = catalog
            .parse_call("codebase_retrieval_internal", json!({}))
            .unwrap_err();

        assert_eq!(
            error,
            ToolMappingError::UnknownTool {
                tool_id: String::from("codebase_retrieval_internal")
            }
        );
    }
}
