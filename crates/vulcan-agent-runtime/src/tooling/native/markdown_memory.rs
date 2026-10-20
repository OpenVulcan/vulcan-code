//! Markdown memory tools: outline, search and atomic writes over the user-directory memory files.
//! Markdown 记忆工具：对用户目录记忆文件的大纲、搜索与原子写入。

use super::*;
use vulcan_markdown_memory::{
    MAX_NODE_LEVEL, MAX_OPERATIONS_PER_WRITE, MAX_SEARCH_LIMIT, MAX_SEARCH_PATTERNS,
    MarkdownMemorySettings, MarkdownMemoryStore, MemoryMode, NodeSource, OutlineRequest,
    ProjectContext, SearchRequest, WriteOperation, render_outline, render_search_page,
    search_nodes,
};

/// Stable identifier of the memory heading-tree tool.
/// 记忆标题树工具的稳定标识。
pub const MEMORY_OUTLINE_TOOL_ID: &str = "memory_outline";
/// Stable identifier of the memory search and read tool.
/// 记忆搜索与读取工具的稳定标识。
pub const MEMORY_SEARCH_TOOL_ID: &str = "memory_search";
/// Stable identifier of the memory write tool.
/// 记忆写入工具的稳定标识。
pub const MEMORY_WRITE_TOOL_ID: &str = "memory_write";

/// Returns whether a native tool belongs to the Markdown memory family.
/// 返回原生工具是否属于 Markdown 记忆工具族。
pub(crate) fn native_tool_id_is_markdown_memory_tool(tool_id: &str) -> bool {
    matches!(
        tool_id,
        MEMORY_OUTLINE_TOOL_ID | MEMORY_SEARCH_TOOL_ID | MEMORY_WRITE_TOOL_ID
    )
}

/// Markdown memory reachable from one execution: the store, the request project and settings.
/// 单次执行可访问的 Markdown 记忆：存储、请求项目与设置。
#[derive(Debug, Clone)]
pub(crate) struct MarkdownMemoryAccess {
    /// Store under the user Vulcan root.
    /// 用户 Vulcan 根目录下的存储。
    pub store: MarkdownMemoryStore,
    /// Host project of the request.
    /// 请求所属的宿主项目。
    pub context: ProjectContext,
    /// Mode and budget settings read for this execution.
    /// 本次执行读取到的模式与预算设置。
    pub settings: MarkdownMemorySettings,
}

/// Reads the global memory mode from the user directory; a missing file means the default `vmm`.
/// 从用户目录读取全局记忆模式；文件缺失表示默认的 `vmm`。
pub fn current_memory_mode() -> Result<MemoryMode, String> {
    let Some(user_root) = crate::runtime_assets::resolve_user_vulcan_root() else {
        return Ok(MemoryMode::default());
    };
    MarkdownMemoryStore::new(&user_root)
        .settings()
        .map(|settings| settings.mode)
        .map_err(|error| error.to_string())
}

/// Projects the saved global VMM layer for runtime use: outside `vmm` mode VMM is disabled in
/// memory only, and the saved configuration is never rewritten. An unreadable mode keeps the
/// saved layer, because the mode file is the only source that can turn VMM off.
/// 投影已保存的全局 VMM 层供运行时使用：非 `vmm` 模式下仅在内存中关闭 VMM，绝不改写已保存配置。
/// 模式无法读取时保留已保存层，因为只有模式文件能关闭 VMM。
pub fn memory_mode_vmm_layer(saved: &VmmConfigLayer) -> VmmConfigLayer {
    project_vmm_layer_for_mode(saved, current_memory_mode().unwrap_or_default())
}

/// Projects a global VMM layer for an explicit memory mode.
/// 按明确的记忆模式投影全局 VMM 层。
pub fn project_vmm_layer_for_mode(saved: &VmmConfigLayer, mode: MemoryMode) -> VmmConfigLayer {
    let mut layer = saved.clone();
    if mode != MemoryMode::Vmm {
        layer.enabled = Some(false);
    }
    layer
}

/// Resolves Markdown memory for a project; `None` when the mode is not `markdown`, the user root
/// is unknown, or the user turned memory off for this project (which also hides user and space).
/// 解析某项目的 Markdown 记忆；模式不是 `markdown`、用户根目录未知或用户关闭了本项目记忆
/// （此时用户层与空间层同样不生效）时返回 `None`。
pub(crate) fn resolve_markdown_memory_access(
    host_project_id: Option<&str>,
    workspace_root: Option<&Path>,
) -> Result<Option<MarkdownMemoryAccess>, String> {
    let Some(user_root) = crate::runtime_assets::resolve_user_vulcan_root() else {
        return Ok(None);
    };
    resolve_markdown_memory_access_in(&user_root, host_project_id, workspace_root)
}

/// Resolves Markdown memory below an explicit user Vulcan root; see [`resolve_markdown_memory_access`].
/// 在明确的用户 Vulcan 根目录下解析 Markdown 记忆；见 [`resolve_markdown_memory_access`]。
pub(crate) fn resolve_markdown_memory_access_in(
    user_root: &Path,
    host_project_id: Option<&str>,
    workspace_root: Option<&Path>,
) -> Result<Option<MarkdownMemoryAccess>, String> {
    let store = MarkdownMemoryStore::new(user_root);
    let settings = store.settings().map_err(|error| error.to_string())?;
    if settings.mode != MemoryMode::Markdown {
        return Ok(None);
    }
    let context = ProjectContext {
        host_project_id: host_project_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
        root_path: workspace_root.map(|root| root.display().to_string()),
        memory_project_id: None,
    };
    if let Some(host) = context.host_project_id.as_deref() {
        let registry = store.registry().map_err(|error| error.to_string())?;
        if registry
            .project_for_host(host)
            .is_some_and(|project| !project.memory_enabled)
        {
            return Ok(None);
        }
    }
    Ok(Some(MarkdownMemoryAccess {
        store,
        context,
        settings,
    }))
}

/// Arguments of `memory_write`.
/// `memory_write` 的参数。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MemoryWriteToolArgs {
    /// Operations applied in order, all or nothing.
    /// 按顺序应用的操作，全部成功或全部不生效。
    operations: Vec<WriteOperation>,
}

/// JSON Schema of the scope selector.
/// 层选择器的 JSON Schema。
fn scope_schema() -> Value {
    json!({"type": "string", "enum": ["user", "space", "project"]})
}

/// Builds the heading-tree tool; read-only and available to subagents and Plan mode.
/// 构建标题树工具；只读，子代理与计划模式可用。
pub(crate) fn build_memory_outline_tool() -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        NativeToolDescriptor {
            id: MEMORY_OUTLINE_TOOL_ID.into(),
            name: MEMORY_OUTLINE_TOOL_ID.into(),
            description: "List the heading tree of long-term memory: the user profile, space memory (rules for every project) and this project's memory, with node ids, revisions, update dates and body sizes. Use it to see what is remembered and where a new fact belongs before memory_write.".into(),
            help_text: None,
            ai_visible: true,
            available_in_plan: true,
        },
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "scopes": {"type": "array", "uniqueItems": true, "items": scope_schema(), "description": "Scopes to list; defaults to all."},
                "section": {"type": "string", "description": "Section key or H1 title to list."},
                "pathPrefix": {"type": "string", "description": "Title path prefix, segments separated by `/`."},
                "depth": {"type": "integer", "minimum": 1, "maximum": MAX_NODE_LEVEL, "description": "Deepest heading level listed; defaults to all."}
            }
        }),
        |request| Box::pin(execute_memory_outline(request)),
    )
}

/// Builds the search and read tool; read-only and available to subagents and Plan mode.
/// 构建搜索与读取工具；只读，子代理与计划模式可用。
pub(crate) fn build_memory_search_tool() -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        NativeToolDescriptor {
            id: MEMORY_SEARCH_TOOL_ID.into(),
            name: MEMORY_SEARCH_TOOL_ID.into(),
            description: "Search long-term memory nodes with several patterns combined with OR (regex unless fixedStrings; case-insensitive by default), optionally limited to scopes, a section or a title path prefix; or read nodes by id. Each result is a whole node: scope, title path, id, revision, update time and body, with matching lines prefixed by `>> `. Catalog lines injected at session start are titles only: read the node before relying on it or changing it.".into(),
            help_text: None,
            ai_visible: true,
            available_in_plan: true,
        },
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "patterns": {"type": "array", "maxItems": MAX_SEARCH_PATTERNS, "items": {"type": "string", "minLength": 1}, "description": "Patterns combined with OR; include synonyms and other languages. Mutually exclusive with ids."},
                "fixedStrings": {"type": "boolean", "description": "Match patterns as literal text."},
                "ignoreCase": {"type": "boolean", "description": "Case-insensitive matching; defaults to true."},
                "scopes": {"type": "array", "uniqueItems": true, "items": scope_schema(), "description": "Scopes to search; defaults to all."},
                "section": {"type": "string", "description": "Section key or H1 title filter."},
                "pathPrefix": {"type": "string", "description": "Title path prefix filter, segments separated by `/`."},
                "ids": {"type": "array", "items": {"type": "string"}, "description": "Node ids to read whole. Mutually exclusive with patterns."},
                "limit": {"type": "integer", "minimum": 1, "maximum": MAX_SEARCH_LIMIT, "description": "Nodes per page."},
                "cursor": {"type": "string", "description": "Cursor from a previous result: the next result page, or the next body page of a single id."}
            }
        }),
        |request| Box::pin(execute_memory_search(request)),
    )
}

/// Builds the write tool; hidden in Plan mode and forbidden for subagents.
/// 构建写入工具；计划模式隐藏，子代理禁用。
pub(crate) fn build_memory_write_tool() -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        NativeToolDescriptor {
            id: MEMORY_WRITE_TOOL_ID.into(),
            name: MEMORY_WRITE_TOOL_ID.into(),
            description: format!(
                "Change long-term memory with 1 to {MAX_OPERATIONS_PER_WRITE} operations applied all-or-nothing: create, replace, append, rename, move (across scopes and sections too) and delete. Every action except create needs the node's current expectedRevision from memory_search or memory_outline. Scopes: `user` describes who the user is; `space` holds rules and environment that hold for every project; `project` holds facts, decisions and constraints of this project only. The `base` section of space and project is injected in full every session, so keep it to a few critical, stable rules and put the rest in other sections. Write when the user asks you to remember something, corrects your approach, or a decision is made that the repository does not show. Make titles state the fact itself; bodies cannot contain H1-H4 headings."
            ),
            help_text: None,
            ai_visible: true,
            available_in_plan: false,
        },
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "operations": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_OPERATIONS_PER_WRITE,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "action": {"type": "string", "enum": ["create", "replace", "append", "rename", "move", "delete"]},
                            "id": {"type": "string", "description": "Node id; every action except create."},
                            "expectedRevision": {"type": "string", "description": "Revision last read; every action except create."},
                            "scope": {"type": "string", "enum": ["user", "space", "project"], "description": "Destination scope; required for create, optional for move."},
                            "parentId": {"type": "string", "description": "create/move: become the last child of this node."},
                            "section": {"type": "string", "description": "create/move: become a child of this section (key or H1 title)."},
                            "newSection": {"type": "boolean", "description": "create/move: become a new H1 section. Give exactly one of parentId, section, newSection."},
                            "title": {"type": "string", "description": "create/rename: heading text."},
                            "body": {"type": "string", "description": "create/replace: Markdown body."},
                            "text": {"type": "string", "description": "append: Markdown appended after a blank line."},
                            "withChildren": {"type": "boolean", "description": "delete: also delete child nodes."}
                        },
                        "required": ["action"]
                    }
                }
            },
            "required": ["operations"]
        }),
        |request| Box::pin(execute_memory_write(request)),
    )
}

/// Resolves the memory of the request's project or fails when memory is not active.
/// 解析请求项目的记忆；记忆未生效时失败。
fn require_access(
    request: &NativeToolExecutionRequest,
) -> Result<MarkdownMemoryAccess, NativeToolError> {
    resolve_markdown_memory_access(
        request.host_project_id.as_deref(),
        request.workspace_root.as_deref(),
    )
    .map_err(NativeToolError::new)?
    .ok_or_else(|| {
        NativeToolError::new(
            "Markdown memory is not active for this conversation; the memory mode changed or memory is off for this project.",
        )
    })
}

/// Parses tool arguments with a uniform error.
/// 以统一错误解析工具参数。
fn parse_arguments<T: serde::de::DeserializeOwned>(
    request: &NativeToolExecutionRequest,
) -> Result<T, NativeToolError> {
    serde_json::from_value(request.arguments.clone())
        .map_err(|error| NativeToolError::new(format!("Invalid arguments: {error}")))
}

/// Runs blocking file work off the async executor and returns model-facing text.
/// 在异步执行器之外运行阻塞文件操作并返回面向模型的文本。
async fn run_blocking(
    work: impl FnOnce() -> Result<String, NativeToolError> + Send + 'static,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let text = tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| NativeToolError::new(format!("Memory task failed: {error}")))??;
    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(text),
    ))
}

/// Lists the heading tree of the visible scopes.
/// 列出可见层的标题树。
async fn execute_memory_outline(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args = parse_arguments::<OutlineRequest>(&request)?;
    let access = require_access(&request)?;
    run_blocking(move || {
        let documents = access
            .store
            .load_scopes(&access.context)
            .map_err(|error| NativeToolError::new(error.to_string()))?;
        render_outline(&documents, &args).map_err(|error| NativeToolError::new(error.to_string()))
    })
    .await
}

/// Searches or reads nodes of the visible scopes.
/// 搜索或读取可见层的节点。
async fn execute_memory_search(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args = parse_arguments::<SearchRequest>(&request)?;
    let access = require_access(&request)?;
    run_blocking(move || {
        let documents = access
            .store
            .load_scopes(&access.context)
            .map_err(|error| NativeToolError::new(error.to_string()))?;
        let page = search_nodes(&documents, &args)
            .map_err(|error| NativeToolError::new(error.to_string()))?;
        Ok(render_search_page(&page))
    })
    .await
}

/// Applies write operations atomically and returns the receipt.
/// 原子应用写入操作并返回回执。
async fn execute_memory_write(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args = parse_arguments::<MemoryWriteToolArgs>(&request)?;
    let access = require_access(&request)?;
    let session = request.session_id.map(|id| id.to_string());
    run_blocking(move || {
        access
            .store
            .write(
                &access.context,
                session.as_deref(),
                NodeSource::Ai,
                &access.settings.budget,
                args.operations,
            )
            .map(|receipt| receipt.render())
            .map_err(|error| NativeToolError::new(error.to_string()))
    })
    .await
}
