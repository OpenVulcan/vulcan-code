//! Local service bridge used by the Tauri WebUI.
//! Tauri WebUI 使用的本地服务桥接层。

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File},
    hash::{Hash, Hasher},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, OnceLock},
};

use base64::Engine;
use base64::engine::general_purpose::{
    STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD as BASE64_URL_SAFE_NO_PAD,
};
use futures_util::StreamExt;
use http_range::HttpRange;
use serde_json::{Value, json};
use tauri::http::{
    Method, Response, StatusCode,
    header::{
        ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, CACHE_CONTROL,
        CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE,
    },
};
use tauri::{Emitter, Manager};
use time::OffsetDateTime;
use vldb_controller_client::ControllerSqliteValue;
use vulcan_agent_runtime::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport, ExecutionStreamEvent,
    RuntimePromptIntervention,
    agent_profile::{AgentProfileCatalog, load_agent_profile_catalog},
    agent_runtime::{
        context_injection_runtime::{
            context_injection_panel_cards_for_kind,
            remove_legacy_vmm_profile_context_injection_state,
        },
        conversation_hooks::{
            ConversationCompletionRequest, ConversationHookRuntime, ConversationPrecheckOutcome,
            VmmWritebackHookEffect,
        },
        events::{RuntimeEvent, RuntimeEventKind},
        question_runtime::QuestionRuntime,
        task_runtime::{DetachedTaskRuntimeEvent, TaskRuntime},
        token_budget_runtime::TokenBudgetRuntime,
        vmm_hooks::{VmmHookConfigSnapshot, VmmHookRuntimeHandle, register_builtin_vmm_hooks},
    },
    attachment_assets::{
        attachment_descriptor_from_mime_type, attachment_descriptor_from_path,
        stage_attachment_bytes, stage_attachment_file,
    },
    conversation_protocol::conversation_state_to_session_messages,
    luaskills_runtime::LuaSkillsRuntimeHandle,
    mcp::{McpRuntimeAccess, bootstrap_mcp_runtime_access},
    provider_capabilities::{ResolvedModelCapabilities, resolve_effective_model_capabilities},
    provider_config::{
        self, PersistedAiToolConfiguration, ProviderConfigurationFile, ProviderRuntimeKind,
        ReasoningEffortLevel,
    },
    provider_input_normalizer::normalize_provider_user_input_for_model,
    session_event_runtime::{
        SessionStreamingRecordIds, append_runtime_event_projection_to_state,
        append_stream_delta_record_to_state, apply_checkpoint_conversation_state,
        apply_terminal_execution_report_to_state, collapse_streaming_thinking_in_state,
    },
    stream_runtime_run_with_intervention,
    tooling::CommandSessionRuntime,
    workbench_ai_capability_router::{
        WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS, effective_ai_tool_configuration_for_prompt_overrides,
        effective_workbench_disabled_ai_capability_kinds,
    },
    workbench_prompt_turn::{
        SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, SharedPromptTurnContinueInput,
        SharedPromptTurnStart, begin_shared_prompt_turn,
        continue_shared_prompt_turn_after_precheck, shared_prompt_precheck_plan,
    },
};
use vulcan_context::{ContextBudgetSeverity, ContextBudgetSnapshot, SessionConversationRecord};
use vulcan_context_injection::{InjectionKind, InjectionPanelCard};
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_service_api::{
    AiCapabilitySummary, AiCapabilityTargetSummary, CommandSessionSummary,
    ComposerAttachmentCapabilitySummary, ComposerAttachmentSummary, ContextInjectionCardSummary,
    ContextInjectionItemSummary, ContextInjectionSnapshot, ContextUsageSnapshot,
    ConversationMessage, ConversationSummary, FeatureAreaSummary, LogEntrySummary, MemorySummary,
    MessageRole, PendingPromptSummary, PluginSummary, ProjectSummary, ProviderModelSummary,
    ProviderSummary, ReasoningSelectionOptionSummary, ReasoningSelectionSummary,
    RuntimeModuleSnapshot, ServiceEvent, SettingSectionSummary, StartupSnapshot,
    StartupStepSnapshot, StartupStepStatus, SubagentNavigationSnapshot, SubagentTaskSummary,
    ToolCallSummary, WorkbenchSnapshot,
};
use vulcan_workbench_model::{
    PENDING_PROMPT_QUEUE_LIMIT, PendingPromptBusyLaneContext, PendingPromptDispatchContext,
    PendingPromptInternalQueueEnqueueError, PendingPromptModelReference,
    PendingPromptPanelDropTarget, PendingPromptPanelEntry, PendingPromptPanelEntryState,
    PendingPromptQueueRouteError, PendingPromptQueueRouteOutcome, PendingPromptRuntimeOptions,
    PendingSessionPromptEntry, PendingSessionPromptSource, SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
    SelectedSubagentTaskPanel, SessionId, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
    SessionSummary, SessionTitleSource, SubagentTaskPanelState, SubagentTaskRunState,
    TaskProjectSummary, apply_pending_prompt_panel_drop_target,
    build_delegate_completion_followup_preview, build_delegate_completion_followup_prompt,
    build_stopped_task_session_metadata, clear_internal_pending_prompt_queue_and_restore_panel,
    enqueue_pending_prompt_internal_queue_entry, next_pending_prompt_dispatch_source,
    parse_subagent_task_panel_metadata, parse_subagent_task_prompt_metadata,
    pending_prompt_dispatch_block_reason_for_source, pending_prompt_lane_is_busy,
    remove_pending_prompt_panel_entry, route_pending_prompt_panel_entry_into_internal_queue,
    subagent_navigation_task_label, take_guided_pending_prompt_entries,
    take_next_pending_prompt_entry, task_session_message_match_score,
};
use vulcan_workspace_store::{
    data_plane::{APP_SPACE_ID, ControllerRuntimeHandle},
    workspace_store::{SessionHydrationSnapshot, WorkspaceSnapshot},
};

use crate::luaskills_bridge::{TauriLuaSkillsCatalog, TauriLuaSkillsHandle};
use crate::mcp_bridge::{TauriMcpRuntimeCatalog, TauriMcpRuntimeHandle};
use crate::token_budget_bridge::TauriTokenBudgetRuntime;

/// Shared runtime handles used to execute real conversation turns.
/// 用于执行真实会话轮次的共享运行时句柄集合。
#[derive(Clone)]
struct AgentRuntimeBridge {
    /// Provider configuration captured during service startup.
    /// 服务启动期间捕获的 Provider 配置。
    provider_configuration: ProviderConfigurationFile,
    /// Standalone AI capability configuration captured during service startup.
    /// 服务启动期间捕获的独立 AI 能力配置。
    ai_tool_configuration: PersistedAiToolConfiguration,
    /// Agent profile catalog reused by runtime preparation.
    /// 运行时准备阶段复用的 Agent 画像目录。
    agent_profile_catalog: AgentProfileCatalog,
    /// LuaSkills runtime handle reused by provider-visible tool execution.
    /// Provider 可见工具执行复用的 LuaSkills 运行时句柄。
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// MCP runtime access reused by provider-visible MCP tools.
    /// Provider 可见 MCP 工具复用的 MCP 运行时访问面。
    mcp_runtime: Option<McpRuntimeAccess>,
    /// Shared task runtime used by nested task tools.
    /// 嵌套任务工具使用的共享任务运行时。
    task_runtime: Arc<TaskRuntime>,
    /// Shared ask-question runtime used by the ask tool.
    /// ask 工具使用的共享提问运行时。
    question_runtime: Arc<QuestionRuntime>,
    /// Shared command-session runtime used by shell session tools.
    /// shell 会话工具使用的共享命令会话运行时。
    command_session_runtime: Arc<CommandSessionRuntime>,
    /// Shared token-budget runtime used by context assembly.
    /// 上下文装配使用的共享 Token 预算运行时。
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    /// Shared conversation hook runtime used to mirror iced turn assembly semantics.
    /// 用于复刻 iced 回合组装语义的共享会话 hook 运行时。
    conversation_hook_runtime: Arc<Mutex<ConversationHookRuntime>>,
    /// Shared VMM hook runtime snapshot used by built-in memory hooks.
    /// 内置记忆 hook 使用的共享 VMM hook 运行时快照。
    vmm_hook_runtime: VmmHookRuntimeHandle,
}

/// Number of recent turns loaded for the selected conversation.
/// 为当前选中会话加载的最近轮次数量。
const SELECTED_SESSION_TURN_LOAD_LIMIT: usize = 24;

/// Maximum media bytes returned for one range response.
/// 单次范围响应返回的最大媒体字节数。
const MEDIA_ASSET_RANGE_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Route prefix used by the generated media custom protocol.
/// 生成媒体自定义协议使用的路由前缀。
const MEDIA_ASSET_ROUTE_PREFIX: &str = "/asset/";

/// Shared service state exposed to Tauri commands.
/// 暴露给 Tauri 命令的共享服务状态。
struct ServiceBridgeState {
    /// Latest startup snapshot.
    /// 最近一次启动快照。
    startup: StartupSnapshot,
    /// Latest workbench snapshot.
    /// 最近一次工作台快照。
    workbench: WorkbenchSnapshot,
    /// Latest workspace snapshot retained for streaming turn updates.
    /// 为流式会话轮次更新保留的最近工作区快照。
    workspace_snapshot: Option<WorkspaceSnapshot>,
    /// Long-lived shared agent runtime bridge.
    /// 长生命周期共享 Agent 运行时桥接。
    agent_runtime: Option<AgentRuntimeBridge>,
    /// Tokio runtime that owns bootstrap-spawned background tasks such as shared MCP access.
    /// 持有引导期后台任务的 Tokio 运行时，例如共享 MCP 访问任务。
    service_runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Long-lived controller runtime handle.
    /// 长生命周期控制器运行时句柄。
    controller_runtime: Option<ControllerRuntimeHandle>,
    /// Long-lived LuaSkills runtime handle.
    /// 长生命周期 LuaSkills 运行时句柄。
    luaskills_runtime: Option<TauriLuaSkillsHandle>,
    /// Long-lived token budget runtime handle.
    /// 长生命周期 Token 预算运行时句柄。
    token_budget_runtime: Option<Arc<TauriTokenBudgetRuntime>>,
    /// Long-lived MCP runtime handle.
    /// 长生命周期 MCP 运行时句柄。
    mcp_runtime: Option<TauriMcpRuntimeHandle>,
    /// Latest context-budget snapshots keyed by session id.
    /// 按会话标识索引的最近上下文预算快照。
    live_context_budgets: BTreeMap<SessionId, ContextBudgetSnapshot>,
    /// Cached subagent task panels keyed by parent session and child task id.
    /// 按父会话与子任务标识索引的子代理任务面板缓存。
    session_subagent_tasks: BTreeMap<SessionId, BTreeMap<SessionId, SubagentTaskPanelState>>,
    /// Parent sessions whose current coordination round should keep subagent navigation visible.
    /// 当前协调轮次中应保持子代理导航可见的父会话集合。
    session_subagent_navigation_rounds: BTreeSet<SessionId>,
    /// Currently opened subagent surface selection.
    /// 当前打开的子代理界面选择。
    active_subagent_panel: Option<SelectedSubagentTaskPanel>,
    /// Detached child-task listeners already started by the WebUI bridge.
    /// WebUI 桥接层已经启动的分离子任务监听器集合。
    detached_subagent_task_listeners: BTreeSet<SessionId>,
    /// Active WebUI stream task handles keyed by session id.
    /// 按会话标识索引的 WebUI 活动流任务句柄。
    active_stream_tasks: BTreeMap<SessionId, tauri::async_runtime::JoinHandle<()>>,
    /// Latest background persistence version keyed by session id.
    /// 按会话标识索引的最新后台持久化版本。
    session_persist_versions: BTreeMap<SessionId, u64>,
}

/// Thread-safe service state handle.
/// 线程安全的服务状态句柄。
type SharedServiceBridgeState = Arc<Mutex<ServiceBridgeState>>;

/// Shared process-wide gate for WebUI workspace persistence.
/// WebUI 工作区持久化使用的进程级共享闸门。
static WEBUI_WORKSPACE_PERSIST_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

/// Returns the shared async persistence gate used by every WebUI workspace write.
/// 返回所有 WebUI 工作区写入共用的异步持久化闸门。
///
/// # Returns
/// 返回值
/// A shared mutex that serializes SQLite-backed workspace writes.
/// 用于串行化 SQLite 工作区写入的共享互斥锁。
fn webui_workspace_persist_lock() -> Arc<tokio::sync::Mutex<()>> {
    WEBUI_WORKSPACE_PERSIST_LOCK
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// Runs the Tauri application and registers local service commands.
/// 运行 Tauri 应用并注册本地服务命令。
pub fn run() {
    let state = Arc::new(Mutex::new(ServiceBridgeState {
        startup: startup_snapshot_with_phase("pending", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workspace_snapshot: None,
        agent_runtime: None,
        service_runtime: None,
        controller_runtime: None,
        luaskills_runtime: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        live_context_budgets: BTreeMap::new(),
        session_subagent_tasks: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeSet::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeSet::new(),
        active_stream_tasks: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
    }));

    tauri::Builder::default()
        .register_uri_scheme_protocol("vulcan-media", |_context, request| {
            vulcan_media_protocol_response(request)
        })
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            service_health,
            service_startup_snapshot,
            service_workbench_snapshot,
            service_session_messages,
            service_pick_composer_attachments,
            service_stage_composer_attachment_paths,
            service_stage_composer_attachment_bytes,
            service_submit_conversation_prompt,
            service_stop_conversation_run,
            service_open_subagent_panel,
            service_close_subagent_panel,
            service_stop_subagent_task,
            service_insert_pending_prompt,
            service_edit_pending_prompt,
            service_remove_pending_prompt,
            service_reorder_pending_prompts,
            service_select_workbench_model,
            service_select_workbench_reasoning_effort,
            service_open_project_directory,
            service_open_project_directory_path,
            service_add_project_from_folder,
            service_toggle_project_pinned,
            service_reorder_projects,
            service_rename_project,
            service_set_project_icon,
            service_reset_project_icon,
            service_set_expanded_task_projects,
            service_archive_project_conversations,
            service_rename_conversation,
            service_archive_conversation,
            service_mark_conversation_read,
            service_select_conversation,
            service_open_external_url
        ])
        .setup(|app| {
            emit_initial_service_events(app.handle());
            spawn_service_bootstrap(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Vulcan Code Tauri shell");
}

/// Returns a simple service health marker for the WebUI.
/// 为 WebUI 返回简单的服务健康标记。
#[tauri::command]
fn service_health() -> String {
    "ready".to_string()
}

/// Opens a trusted external URL with the system default browser.
/// 使用系统默认浏览器打开可信外部链接。
#[tauri::command]
fn service_open_external_url(url: String) -> Result<String, String> {
    if url != "https://github.com/OpenVulcan/vulcan-code" {
        return Err(String::from("不允许打开未授权的外部链接。"));
    }
    open_url_in_system_browser(&url)?;
    Ok(String::from("已请求使用系统默认浏览器打开 GitHub。"))
}

/// Serves generated and local media files through a local custom protocol with HTTP range support.
/// 通过带 HTTP Range 支持的本地自定义协议提供生成媒体文件和本地媒体文件。
fn vulcan_media_protocol_response(request: tauri::http::Request<Vec<u8>>) -> Response<Vec<u8>> {
    match build_vulcan_media_protocol_response(request) {
        Ok(response) => response,
        Err(error) => vulcan_media_error_response(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

/// Builds one media protocol response for images, audio, video, and PDFs.
/// 为图片、音频、视频和 PDF 构建单个媒体协议响应。
fn build_vulcan_media_protocol_response(
    request: tauri::http::Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, String> {
    let media_path = media_path_from_protocol_request(&request)?;
    if !is_allowed_media_asset_path(&media_path) {
        return Ok(vulcan_media_error_response(
            StatusCode::FORBIDDEN,
            "媒体路径不是允许的媒体资源类型。",
        ));
    }

    let mut file = match File::open(&media_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(vulcan_media_error_response(
                StatusCode::NOT_FOUND,
                "媒体文件不存在。",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(vulcan_media_error_response(
                StatusCode::FORBIDDEN,
                "没有权限读取媒体文件。",
            ));
        }
        Err(error) => return Err(format!("无法打开媒体文件：{error}")),
    };
    let file_len = file
        .metadata()
        .map_err(|error| format!("无法读取媒体文件元数据：{error}"))?
        .len();
    let mime_type = infer_media_asset_mime_type(&media_path);
    let response = Response::builder()
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(ACCESS_CONTROL_EXPOSE_HEADERS, "content-range")
        .header(ACCEPT_RANGES, "bytes")
        .header(CACHE_CONTROL, "no-store")
        .header(CONTENT_TYPE, mime_type);

    if request.method() == Method::HEAD {
        return response
            .header(CONTENT_LENGTH, file_len)
            .body(Vec::new())
            .map_err(|error| format!("无法构建媒体 HEAD 响应：{error}"));
    }

    if let Some(range_header) = request
        .headers()
        .get("range")
        .and_then(|value| value.to_str().ok())
    {
        let range = single_media_http_range(range_header, file_len)?;
        let range_start = range.start;
        let range_end = range.start + range.length - 1;
        let mut body = Vec::with_capacity(range.length as usize);
        file.seek(SeekFrom::Start(range_start))
            .map_err(|error| format!("无法定位媒体范围：{error}"))?;
        file.take(range.length)
            .read_to_end(&mut body)
            .map_err(|error| format!("无法读取媒体范围：{error}"))?;
        return response
            .status(StatusCode::PARTIAL_CONTENT)
            .header(
                CONTENT_RANGE,
                format!("bytes {range_start}-{range_end}/{file_len}"),
            )
            .header(CONTENT_LENGTH, body.len())
            .body(body)
            .map_err(|error| format!("无法构建媒体范围响应：{error}"));
    }

    let mut body = Vec::with_capacity(file_len.min(MEDIA_ASSET_RANGE_MAX_BYTES) as usize);
    file.read_to_end(&mut body)
        .map_err(|error| format!("无法读取媒体文件：{error}"))?;
    response
        .header(CONTENT_LENGTH, file_len)
        .body(body)
        .map_err(|error| format!("无法构建媒体响应：{error}"))
}

/// Extracts the local media path encoded inside a custom protocol request.
/// 提取自定义协议请求中编码的本地媒体路径。
fn media_path_from_protocol_request(
    request: &tauri::http::Request<Vec<u8>>,
) -> Result<PathBuf, String> {
    let request_path = request.uri().path();
    let encoded_path = request_path
        .strip_prefix(MEDIA_ASSET_ROUTE_PREFIX)
        .ok_or_else(|| String::from("媒体路由格式无效。"))?;
    let path_bytes = BASE64_URL_SAFE_NO_PAD
        .decode(encoded_path)
        .map_err(|error| format!("媒体路径解码失败：{error}"))?;
    let path_text = String::from_utf8(path_bytes)
        .map_err(|error| format!("媒体路径不是有效 UTF-8：{error}"))?;
    Ok(PathBuf::from(path_text))
}

/// Returns whether a media path belongs to the generated attachment storage tree.
/// 返回媒体路径是否属于生成附件存储树。
fn is_allowed_generated_media_path(path: &Path) -> bool {
    let normalized_components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(|text| text.to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    normalized_components
        .windows(3)
        .any(|window| window == [".vulcan", "code", "attachments"])
}

/// Returns whether a media path can be served by the local media protocol.
/// 返回媒体路径是否允许通过本地媒体协议提供。
fn is_allowed_media_asset_path(path: &Path) -> bool {
    if is_allowed_generated_media_path(path) {
        return true;
    }
    has_supported_media_asset_extension(path)
}

/// Returns whether a local path extension is supported by the media protocol.
/// 返回本地路径扩展名是否受媒体协议支持。
fn has_supported_media_asset_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png")
            | Some("jpg")
            | Some("jpeg")
            | Some("webp")
            | Some("gif")
            | Some("bmp")
            | Some("svg")
            | Some("mp4")
            | Some("webm")
            | Some("mov")
            | Some("mkv")
            | Some("mp3")
            | Some("wav")
            | Some("m4a")
            | Some("aac")
            | Some("flac")
            | Some("ogg")
            | Some("pdf")
    )
}

/// Parses one bounded HTTP range for media playback.
/// 解析一个有上限的媒体播放 HTTP 范围。
fn single_media_http_range(range_header: &str, file_len: u64) -> Result<HttpRange, String> {
    let mut ranges = HttpRange::parse(range_header, file_len)
        .map_err(|error| format!("媒体 Range 请求无效：{error:?}"))?;
    let Some(mut range) = ranges.drain(..).next() else {
        return Err(String::from("媒体 Range 请求为空。"));
    };
    if range.start >= file_len || range.length == 0 {
        return Err(String::from("媒体 Range 请求越界。"));
    }
    range.length = range.length.min(MEDIA_ASSET_RANGE_MAX_BYTES);
    Ok(range)
}

/// Builds one plain text error response for the generated media protocol.
/// 为生成媒体协议构建一个纯文本错误响应。
fn vulcan_media_error_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// Infers a conservative MIME type from a generated media path extension.
/// 根据生成媒体路径扩展名推断保守 MIME 类型。
fn infer_media_asset_mime_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => String::from("image/png"),
        Some("jpg") | Some("jpeg") => String::from("image/jpeg"),
        Some("webp") => String::from("image/webp"),
        Some("gif") => String::from("image/gif"),
        Some("bmp") => String::from("image/bmp"),
        Some("svg") => String::from("image/svg+xml"),
        Some("mp4") => String::from("video/mp4"),
        Some("webm") => String::from("video/webm"),
        Some("mov") => String::from("video/quicktime"),
        Some("mkv") => String::from("video/x-matroska"),
        Some("mp3") => String::from("audio/mpeg"),
        Some("wav") => String::from("audio/wav"),
        Some("m4a") => String::from("audio/mp4"),
        Some("aac") => String::from("audio/aac"),
        Some("flac") => String::from("audio/flac"),
        Some("ogg") => String::from("audio/ogg"),
        Some("pdf") => String::from("application/pdf"),
        _ => String::from("application/octet-stream"),
    }
}

/// Returns the current startup snapshot.
/// 返回当前启动快照。
#[tauri::command]
fn service_startup_snapshot(
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<StartupSnapshot, String> {
    state
        .lock()
        .map(|guard| guard.startup.clone())
        .map_err(|error| format!("failed to lock service startup state: {error}"))
}

/// Returns the current workbench snapshot.
/// 返回当前工作台快照。
#[tauri::command]
fn service_workbench_snapshot(
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    state
        .lock()
        .map(|guard| guard.workbench.clone())
        .map_err(|error| format!("failed to lock service workbench state: {error}"))
}

/// Loads the recent transcript window for one conversation.
/// 加载单个会话的最近转录窗口。
#[tauri::command]
async fn service_session_messages(
    state: tauri::State<'_, SharedServiceBridgeState>,
    session_id: String,
) -> Result<Vec<ConversationMessage>, String> {
    let parsed_session_id = session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{session_id}': {error}"))?;
    let controller_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(messages) =
            in_memory_session_messages_for_service(&guard.workspace_snapshot, parsed_session_id)
        {
            return Ok(conversation_messages_from_session(
                &session_id,
                messages.as_slice(),
            ));
        }
        guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?
    };
    let hydration_snapshot =
        match vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
            controller_runtime,
            parsed_session_id,
            SELECTED_SESSION_TURN_LOAD_LIMIT,
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(error) if is_missing_session_manifest_error(&error) => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };

    Ok(conversation_messages_from_session(
        &session_id,
        hydration_snapshot.messages.as_slice(),
    ))
}

/// Opens the native attachment picker and stages selected files for composer use.
/// 打开原生附件选择器，并将选中文件暂存给输入区使用。
#[tauri::command]
async fn service_pick_composer_attachments(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    kind: Option<String>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    let (workspace_path, kind_filter) = composer_attachment_workspace_and_kind(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        kind.as_deref(),
    )?;
    let mut dialog = rfd::FileDialog::new();
    if let Some(kind_filter) = kind_filter.as_ref() {
        dialog = dialog.add_filter(
            composer_attachment_kind_label(kind_filter),
            composer_attachment_kind_extensions(kind_filter),
        );
    } else {
        dialog = dialog.add_filter(
            "附件",
            &[
                "png", "jpg", "jpeg", "webp", "gif", "bmp", "mp3", "wav", "m4a", "aac", "flac",
                "ogg", "mp4", "mov", "mkv", "webm", "avi", "pdf",
            ],
        );
    }
    let Some(paths) = dialog.pick_files() else {
        return Ok(Vec::new());
    };
    stage_composer_attachment_paths_for_workspace(&workspace_path, paths)
}

/// Stages existing local paths as composer attachments.
/// 将已有本地路径暂存为输入区附件。
#[tauri::command]
async fn service_stage_composer_attachment_paths(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    paths: Vec<String>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    stage_composer_attachment_paths_for_workspace(&workspace_path, paths)
}

/// Stages one browser-provided in-memory file as a composer attachment.
/// 将浏览器提供的一份内存文件暂存为输入区附件。
#[tauri::command]
async fn service_stage_composer_attachment_bytes(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    file_name: String,
    mime_type: String,
    data_base64: String,
) -> Result<ComposerAttachmentSummary, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let descriptor = attachment_descriptor_from_mime_type(&mime_type)
        .or_else(|| attachment_descriptor_from_path_fallback(&file_name))
        .ok_or_else(|| format!("文件 `{}` 不是受支持的附件格式。", file_name.trim()))?;
    let bytes = BASE64_STANDARD
        .decode(data_base64.trim())
        .map_err(|error| format!("解码附件 `{}` 失败：{}。", file_name.trim(), error))?;
    let attachment = stage_attachment_bytes(
        &workspace_path,
        &descriptor,
        file_name.trim(),
        None,
        bytes.as_slice(),
    )?;
    Ok(composer_attachment_summary_from_core(&attachment))
}

/// Resolves workspace path and optional attachment-kind filter for composer staging.
/// 解析输入区附件暂存所需的工作区路径与可选附件类型过滤。
fn composer_attachment_workspace_and_kind(
    state: &tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
    kind: Option<&str>,
) -> Result<(String, Option<ConversationAttachmentKind>), String> {
    let workspace_path = composer_attachment_workspace_path(state, project_id, conversation_id)?;
    let kind = kind.map(parse_composer_attachment_kind).transpose()?;
    Ok((workspace_path, kind))
}

/// Resolves the project workspace path used to stage one composer attachment.
/// 解析暂存输入区附件时使用的项目工作区路径。
fn composer_attachment_workspace_path(
    state: &tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
        return Ok(workspace_root().display().to_string());
    };
    let owner_project_id = conversation_id
        .and_then(parse_session_id)
        .and_then(|session_id| {
            workspace_snapshot
                .sessions
                .iter()
                .find(|session| session.id == session_id)
                .and_then(|session| session.project_id.as_deref())
        })
        .or(project_id)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(owner_project_id) = owner_project_id else {
        return Ok(workspace_root().display().to_string());
    };
    Ok(workspace_snapshot
        .task_projects
        .iter()
        .find(|project| project.id == owner_project_id)
        .map(|project| project.path.clone())
        .unwrap_or_else(|| workspace_root().display().to_string()))
}

/// Stages local paths for a resolved workspace path and converts them to service DTOs.
/// 为已解析的工作区路径暂存本地路径，并转换为 service DTO。
fn stage_composer_attachment_paths_for_workspace(
    workspace_path: &str,
    paths: Vec<PathBuf>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    paths
        .iter()
        .map(|path| stage_attachment_file(workspace_path, path))
        .map(|result| result.map(|attachment| composer_attachment_summary_from_core(&attachment)))
        .collect()
}

/// Converts a core conversation attachment into the WebUI service DTO.
/// 将核心会话附件转换为 WebUI service DTO。
fn composer_attachment_summary_from_core(
    attachment: &ConversationAttachment,
) -> ComposerAttachmentSummary {
    ComposerAttachmentSummary {
        kind: attachment.kind.modality_name().to_string(),
        file_name: attachment.file_name.clone(),
        local_path: attachment.local_path.clone(),
        source_path: attachment.source_path.clone(),
        asset_id: attachment.asset_id.clone(),
        byte_len: attachment.byte_len,
        preview_local_path: attachment.preview_local_path.clone(),
        mime_type: attachment.mime_type.clone(),
    }
}

/// Converts a WebUI service attachment DTO back into the core conversation attachment.
/// 将 WebUI service 附件 DTO 转回核心会话附件。
fn composer_attachment_summary_to_core(
    summary: ComposerAttachmentSummary,
) -> Result<ConversationAttachment, String> {
    Ok(ConversationAttachment {
        kind: parse_composer_attachment_kind(&summary.kind)?,
        file_name: summary.file_name,
        local_path: summary.local_path,
        source_path: summary.source_path,
        asset_id: summary.asset_id,
        byte_len: summary.byte_len,
        preview_local_path: summary.preview_local_path,
        mime_type: summary.mime_type,
    })
}

/// Parses one stable composer attachment kind key.
/// 解析单个稳定输入区附件类型键。
fn parse_composer_attachment_kind(value: &str) -> Result<ConversationAttachmentKind, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "image" => Ok(ConversationAttachmentKind::Image),
        "audio" => Ok(ConversationAttachmentKind::Audio),
        "video" => Ok(ConversationAttachmentKind::Video),
        "pdf" | "document" => Ok(ConversationAttachmentKind::Pdf),
        other => Err(format!("不支持的附件类型：{other}")),
    }
}

/// Resolves an attachment descriptor from a browser file name when the MIME type is missing.
/// 当浏览器未提供 MIME 类型时，从文件名解析附件描述。
fn attachment_descriptor_from_path_fallback(
    file_name: &str,
) -> Option<vulcan_agent_runtime::attachment_assets::AttachmentDescriptor> {
    attachment_descriptor_from_path(Path::new(file_name))
}

/// Returns the localized label used by the native file picker filter.
/// 返回原生文件选择器过滤器使用的本地化名称。
fn composer_attachment_kind_label(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "图片",
        ConversationAttachmentKind::Audio => "音频",
        ConversationAttachmentKind::Video => "视频",
        ConversationAttachmentKind::Pdf => "PDF",
    }
}

/// Returns file extensions accepted by the native picker for one attachment kind.
/// 返回单种附件类型在原生选择器中接受的扩展名。
fn composer_attachment_kind_extensions(
    kind: &ConversationAttachmentKind,
) -> &'static [&'static str] {
    match kind {
        ConversationAttachmentKind::Image => &["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        ConversationAttachmentKind::Audio => &["mp3", "wav", "m4a", "aac", "flac", "ogg"],
        ConversationAttachmentKind::Video => &["mp4", "mov", "mkv", "webm", "avi"],
        ConversationAttachmentKind::Pdf => &["pdf"],
    }
}

/// Returns the browser file picker accept list for one attachment kind.
/// 返回单种附件类型对应的浏览器文件选择 accept 列表。
fn composer_attachment_kind_accept(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "image/png,image/jpeg,image/webp,image/gif,image/bmp",
        ConversationAttachmentKind::Audio => {
            "audio/mpeg,audio/wav,audio/mp4,audio/aac,audio/flac,audio/ogg"
        }
        ConversationAttachmentKind::Video => {
            "video/mp4,video/quicktime,video/x-matroska,video/webm,video/x-msvideo"
        }
        ConversationAttachmentKind::Pdf => "application/pdf",
    }
}

/// Returns the current in-memory transcript projection for one service session.
/// 返回单个服务会话当前的内存转录投影。
fn in_memory_session_messages_for_service(
    workspace_snapshot: &Option<WorkspaceSnapshot>,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    in_memory_session_messages_from_workspace(workspace_snapshot.as_ref()?, session_id)
}

/// Returns the current in-memory transcript projection for one session from a concrete workspace snapshot.
/// 从具体工作区快照返回单个会话当前的内存转录投影。
fn in_memory_session_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    if let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) {
        let messages = conversation_state_to_session_messages(&runtime_state.conversation_state);
        if !messages.is_empty() {
            return Some(messages);
        }
    }
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .cloned()
}

/// Submits one user prompt through the shared agent runtime and returns the immediate workbench snapshot.
/// 通过共享 Agent 运行时提交一条用户提示词，并返回即时工作台快照。
#[tauri::command]
async fn service_submit_conversation_prompt(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    prompt: String,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
    disabled_ai_capability_kinds: Option<Vec<String>>,
    disabled_ai_capability_target_selectors: Option<Vec<String>>,
    ai_capability_target_order_overrides: Option<BTreeMap<String, Vec<String>>>,
) -> Result<WorkbenchSnapshot, String> {
    let prompt = prompt.trim().to_string();
    let attachments = attachments
        .unwrap_or_default()
        .into_iter()
        .map(composer_attachment_summary_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    if prompt.is_empty() && attachments.is_empty() {
        return Err(String::from("请输入任务指令。"));
    }

    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let previous_workspace_snapshot = current_workspace_snapshot_from_state(&state)?;
    let submission_session_id = conversation_id.as_deref().and_then(parse_session_id);
    let mut workspace_snapshot = if let Some(session_id) = submission_session_id
        && let Some(workspace_snapshot) = previous_workspace_snapshot.clone()
        && retained_submission_snapshot_has_session_context(&workspace_snapshot, session_id)
    {
        workspace_snapshot
    } else {
        load_workspace_for_submission(controller_runtime.clone(), conversation_id.as_deref())
            .await?
    };
    preserve_active_session_run_state(
        previous_workspace_snapshot.as_ref(),
        &mut workspace_snapshot,
    );
    if let Some(session_id) = conversation_id.as_deref().and_then(parse_session_id) {
        let (has_active_stream_task, has_running_subagents) = {
            let mut guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            (
                guard.active_stream_tasks.contains_key(&session_id),
                session_has_running_subagent_tasks_from_state(
                    &mut guard,
                    &workspace_snapshot,
                    session_id,
                ),
            )
        };
        if session_has_blocking_system_prompt_work(
            &workspace_snapshot,
            &agent_runtime.provider_configuration,
            session_id,
            has_active_stream_task,
            has_running_subagents,
        ) {
            let workspace_path = workspace_path_for_session(&workspace_snapshot, session_id);
            let entry = build_webui_pending_prompt_panel_entry(
                prompt,
                attachments,
                current_pending_prompt_model_reference(&agent_runtime.provider_configuration),
                pending_prompt_runtime_options_from_submission(
                    &agent_runtime,
                    reasoning_effort.as_deref(),
                    disabled_ai_capability_kinds.as_deref(),
                )?,
            );
            enqueue_webui_pending_prompt_panel_entry(
                &mut workspace_snapshot,
                &agent_runtime,
                session_id,
                &workspace_path,
                entry,
            )?;
            let workbench = store_and_emit_workbench_snapshot(
                &app,
                &state,
                &controller_runtime,
                &luaskills_catalog,
                &token_budget_runtime,
                &mcp_catalog,
                &agent_runtime.provider_configuration,
                &agent_runtime.ai_tool_configuration,
                workspace_snapshot,
                &logs,
            )?;
            let snapshot_for_persist =
                current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
                    String::from("service is still starting; workspace snapshot is not ready")
                })?;
            spawn_workspace_session_persist_task(
                state.inner().clone(),
                controller_runtime.clone(),
                snapshot_for_persist,
                session_id,
                Some(String::from("queued_submit")),
            );
            return Ok(workbench);
        }
    }
    let (session_id, session_title, workspace_path) = materialize_prompt_session(
        &mut workspace_snapshot,
        project_id.as_deref(),
        conversation_id.as_deref(),
        if prompt.is_empty() {
            "附件会话"
        } else {
            &prompt
        },
    )?;
    if conversation_id.is_some() {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.session_subagent_navigation_rounds.remove(&session_id);
        if guard
            .active_subagent_panel
            .is_some_and(|selection| selection.parent_session_id == session_id)
        {
            guard.active_subagent_panel = None;
        }
    }
    let disabled_ai_capability_kinds = effective_workbench_disabled_ai_capability_kinds(
        &agent_runtime.ai_tool_configuration,
        disabled_ai_capability_kinds.as_deref(),
    )?;
    let disabled_ai_capability_target_selectors =
        parse_ai_capability_target_selectors(disabled_ai_capability_target_selectors);
    let ai_capability_target_order_overrides =
        parse_ai_capability_target_order_overrides(ai_capability_target_order_overrides)?;
    snapshot_webui_context_before_user_prompt(
        &workspace_snapshot,
        session_id,
        PendingSessionPromptSource::UserFollowup,
    )?;
    let precheck_start = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            SharedPromptTurnBeginInput {
                session_id,
                session_title: &session_title,
                workspace_path: &workspace_path,
                prompt_entry_id: format!(
                    "webui-prompt-{session_id}-{}",
                    current_unix_millis_text()
                ),
                prompt: &prompt,
                attachments,
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::UserFollowup,
                execution_mode: execution_mode.unwrap_or_else(|| String::from("build")),
                reasoning_effort: reasoning_effort.as_deref().and_then(parse_reasoning_effort),
                disabled_ai_capability_kinds,
                disabled_ai_capability_target_selectors,
                ai_capability_target_order_overrides,
            },
        )
    }?;
    apply_webui_prompt_begin_outer_steps(&mut workspace_snapshot, session_id, &session_title);
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    let precheck_plan = webui_shared_prompt_precheck_plan(
        &agent_runtime,
        &workspace_snapshot,
        session_id,
        &workspace_path,
    )?;
    let mut workbench = store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot.clone(),
        &logs,
    )?;
    spawn_workspace_session_persist_task(
        state.inner().clone(),
        controller_runtime.clone(),
        workspace_snapshot,
        session_id,
        Some(String::from("submit_begin")),
    );
    let service_state = state.inner().clone();
    if start_webui_precheck_or_continue(
        &app,
        &service_state,
        &controller_runtime,
        &agent_runtime,
        precheck_start,
        precheck_plan,
    )? {
        workbench = current_workbench_snapshot_from_state(&state)?;
    }
    Ok(workbench)
}

/// Stops the selected WebUI conversation run and clears guided internal queue work.
/// 停止当前选中的 WebUI 会话运行，并清理已引导的内部队列任务。
#[tauri::command]
async fn service_stop_conversation_run(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let active_task = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .active_stream_tasks
        .remove(&selected_session_id);
    if let Some(active_task) = active_task {
        active_task.abort();
    }
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let running_subagent_task_states = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        rebuild_webui_subagent_task_cache_for_session(
            &mut guard,
            &workspace_snapshot,
            selected_session_id,
        );
        running_webui_subagent_task_states_from_state(&guard, selected_session_id)
    };
    for task_state in &running_subagent_task_states {
        let _ = agent_runtime.task_runtime.stop_task(task_state.task_id);
        mark_webui_subagent_task_stopped_in_workspace(
            &mut workspace_snapshot,
            selected_session_id,
            task_state.task_id,
            Some(task_state),
        );
    }
    if !running_subagent_task_states.is_empty() {
        let running_subagent_task_ids = running_subagent_task_states
            .iter()
            .map(|state| state.task_id)
            .collect::<Vec<_>>();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        mark_webui_subagent_tasks_stopped_in_state(
            &mut guard,
            selected_session_id,
            &running_subagent_task_ids,
        );
    }
    stop_webui_session_work(&mut workspace_snapshot, selected_session_id)?;
    if !workspace_snapshot
        .session_runtime_state
        .contains_key(&selected_session_id)
    {
        return Err(String::from("停止后未找到会话运行时状态。"));
    }
    persist_webui_session_runtime_state_update(
        state.inner(),
        controller_runtime.clone(),
        selected_session_id,
        &workspace_snapshot,
    )
    .await?;
    store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )
}

/// Opens one subagent transcript surface under the selected parent conversation.
/// 打开当前父会话下的单个子代理转录界面。
#[tauri::command]
async fn service_open_subagent_panel(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    parent_conversation_id: String,
    task_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid subagent task id '{task_id}': {error}"))?;
    let workspace_snapshot = current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        rebuild_webui_subagent_task_cache_for_session(
            &mut guard,
            &workspace_snapshot,
            parent_session_id,
        );
        if !guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .is_some_and(|tasks| tasks.contains_key(&task_session_id))
        {
            return Err(String::from("没有找到可打开的子代理任务。"));
        }
        guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
            parent_session_id,
            task_id: task_session_id,
        });
    }
    store_current_workbench_snapshot(
        &app,
        state.inner(),
        &controller_runtime,
        &agent_runtime,
        workspace_snapshot,
    )
}

/// Closes the active subagent transcript surface and returns to the parent conversation.
/// 关闭活动子代理转录界面并返回父会话。
#[tauri::command]
async fn service_close_subagent_panel(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let workspace_snapshot = current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.active_subagent_panel = None;
    }
    store_current_workbench_snapshot(
        &app,
        state.inner(),
        &controller_runtime,
        &agent_runtime,
        workspace_snapshot,
    )
}

/// Stops one running subagent task through the shared task runtime.
/// 通过共享任务运行时停止单个运行中的子代理任务。
#[tauri::command]
async fn service_stop_subagent_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    parent_conversation_id: String,
    task_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid subagent task id '{task_id}': {error}"))?;
    let state_snapshot = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .and_then(|tasks| tasks.get(&task_session_id))
            .cloned()
    };
    let did_stop = agent_runtime.task_runtime.stop_task(task_session_id)?;
    let workspace_snapshot = if did_stop {
        with_workspace_mutation(&app, state.inner(), |workspace| {
            mark_webui_subagent_task_stopped_in_workspace(
                workspace,
                parent_session_id,
                task_session_id,
                state_snapshot.as_ref(),
            );
        })?
    } else {
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?
    };
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if did_stop {
            if let Some(state) = guard
                .session_subagent_tasks
                .get_mut(&parent_session_id)
                .and_then(|tasks| tasks.get_mut(&task_session_id))
            {
                state.run_state = SubagentTaskRunState::Stopped;
                if state.result_text.trim().is_empty() {
                    state.result_text = SUBAGENT_TASK_STOPPED_SUMMARY_TEXT.to_string();
                }
            }
        }
        guard
            .detached_subagent_task_listeners
            .remove(&task_session_id);
    }
    store_current_workbench_snapshot(
        &app,
        state.inner(),
        &controller_runtime,
        &agent_runtime,
        workspace_snapshot,
    )
}

/// Inserts one visible pending prompt into the shared internal prompt queue.
/// 将单条可见待发送提示词插入共享内部提示词队列。
#[tauri::command]
async fn service_insert_pending_prompt(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    entry_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let workspace_path = workspace_path_for_session(&workspace_snapshot, selected_session_id);
    insert_webui_pending_prompt_into_internal_queue(
        &mut workspace_snapshot,
        &agent_runtime,
        selected_session_id,
        &workspace_path,
        &entry_id,
    )?;
    mark_webui_session_waiting_for_guided_prompt_dispatch(
        &mut workspace_snapshot,
        selected_session_id,
    );
    let mut workbench = store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    let snapshot_for_persist = current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    spawn_workspace_session_persist_task(
        state.inner().clone(),
        controller_runtime.clone(),
        snapshot_for_persist,
        selected_session_id,
        Some(String::from("guided_insert")),
    );
    let started_next_prompt = match maybe_prepare_next_webui_pending_prompt(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        selected_session_id,
        true,
    )
    .await
    {
        Ok(started) => started,
        Err(error) => return Err(error),
    };
    if started_next_prompt {
        workbench = current_workbench_snapshot_from_state(&state)?;
    }
    Ok(workbench)
}

/// Restores one visible pending prompt into the WebUI composer and removes it from the queue.
/// 将单条可见待发送提示词恢复到 WebUI 输入区，并从队列中移除。
#[tauri::command]
async fn service_edit_pending_prompt(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    entry_id: String,
) -> Result<PendingPromptSummary, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let summary = {
        let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get_mut(&selected_session_id)
        else {
            return Err(String::from("当前会话没有可编辑的待发送队列。"));
        };
        let Some(entry) = remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, &entry_id)
        else {
            return Err(String::from("没有找到待编辑的待发送提示词。"));
        };
        runtime_state.updated_at = current_unix_millis_text();
        pending_prompt_summary_from_panel_entry(&entry)
    };
    store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    let snapshot_for_persist = current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    spawn_workspace_session_persist_task(
        state.inner().clone(),
        controller_runtime.clone(),
        snapshot_for_persist,
        selected_session_id,
        Some(String::from("pending_edit")),
    );
    Ok(summary)
}

/// Removes one visible pending prompt from the selected conversation queue.
/// 从当前选中会话队列中移除单条可见待发送提示词。
#[tauri::command]
async fn service_remove_pending_prompt(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    entry_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let removed = if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&selected_session_id)
    {
        if remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, &entry_id).is_some() {
            runtime_state.updated_at = current_unix_millis_text();
            true
        } else {
            false
        }
    } else {
        false
    };
    let workbench = store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    if removed {
        let snapshot_for_persist =
            current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
        spawn_workspace_session_persist_task(
            state.inner().clone(),
            controller_runtime.clone(),
            snapshot_for_persist,
            selected_session_id,
            Some(String::from("pending_remove")),
        );
    }
    Ok(workbench)
}

/// Reorders the visible pending prompt queue for the selected conversation.
/// 重排当前选中会话的可见待发送提示词队列。
#[tauri::command]
async fn service_reorder_pending_prompts(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: Option<String>,
    dragged_entry_id: String,
    target_entry_id: Option<String>,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .map(Ok)
        .unwrap_or_else(|| selected_session_id_from_state(&state))?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    workspace_snapshot.selected_session = selected_session_id;
    let drop_target = target_entry_id
        .filter(|entry_id| !entry_id.trim().is_empty())
        .map(PendingPromptPanelDropTarget::Before)
        .unwrap_or(PendingPromptPanelDropTarget::End);
    let did_reorder = if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&selected_session_id)
    {
        if apply_pending_prompt_panel_drop_target(
            &mut runtime_state.ui_state,
            &dragged_entry_id,
            &drop_target,
        ) {
            runtime_state.updated_at = current_unix_millis_text();
            true
        } else {
            false
        }
    } else {
        false
    };
    let workbench = store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    if did_reorder {
        let snapshot_for_persist =
            current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
        spawn_workspace_session_persist_task(
            state.inner().clone(),
            controller_runtime.clone(),
            snapshot_for_persist,
            selected_session_id,
            Some(String::from("pending_reorder")),
        );
    }
    Ok(workbench)
}

/// Selects and persists the active workbench provider/model pair.
/// 选择并持久化当前工作台使用的供应商与模型组合。
#[tauri::command]
async fn service_select_workbench_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    provider_id: String,
    model_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let provider_id = provider_id.trim().to_string();
    let model_id = model_id.trim().to_string();
    if provider_id.is_empty() || model_id.is_empty() {
        return Err(String::from("模型选择缺少 Provider 或模型标识。"));
    }

    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let has_model = provider_config::effective_model_choices(
            &provider_id,
            &agent_runtime.provider_configuration,
        )
        .into_iter()
        .any(|choice| choice.id == model_id);
        if !has_model {
            return Err(format!(
                "模型 `{model_id}` 不在 `{provider_id}` 的已启用模型列表中。"
            ));
        }

        let mut next_configuration = agent_runtime.provider_configuration.clone();
        next_configuration.model = Some(format!("{provider_id}/{model_id}"));
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存模型选择失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        guard.workbench.providers = providers(&next_configuration);
        guard.workbench.reasoning_selection = reasoning_selection(&next_configuration);
        guard.workbench.clone()
    };

    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
}

/// Persists the active workbench reasoning effort through the shared provider configuration.
/// 通过共享 Provider 配置持久化当前工作台思考档位。
#[tauri::command]
async fn service_select_workbench_reasoning_effort(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    effort: String,
) -> Result<WorkbenchSnapshot, String> {
    let selected_effort =
        parse_reasoning_effort(effort.trim()).ok_or_else(|| format!("未知思考档位：{effort}"))?;
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        next_configuration.workbench_reasoning_effort = Some(selected_effort);
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存思考档位失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        guard.workbench.reasoning_selection = reasoning_selection(&next_configuration);
        guard.workbench.clone()
    };

    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
}

/// Opens one project folder in the host system file manager.
/// 在宿主系统文件管理器中打开单个项目文件夹。
#[tauri::command]
fn service_open_project_directory(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
) -> Result<String, String> {
    let project = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .cloned()
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    open_path_in_system_file_manager(&project.path)?;
    Ok(format!("已请求打开项目目录：{}。", project.name))
}

/// Opens one arbitrary project directory path in the host system file manager.
/// 在宿主系统文件管理器中打开任意项目目录路径。
#[tauri::command]
fn service_open_project_directory_path(path: String) -> Result<String, String> {
    open_path_in_system_file_manager(&path)?;
    Ok(format!("已请求打开目录：{path}。"))
}

/// Opens the host folder picker and adds or restores the selected project.
/// 打开宿主文件夹选择器，并新增或恢复选中的项目。
#[tauri::command]
async fn service_add_project_from_folder(
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<Option<WorkbenchSnapshot>, String> {
    let initial_directory = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .projects
        .first()
        .map(|project| project.path.clone())
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root()));
    let Some(selected_path) = pick_task_project_folder(&initial_directory)? else {
        return Ok(None);
    };
    let normalized_path = normalize_task_project_path(&selected_path);
    let normalized_path_text = normalized_workspace_path_text_from_path(&normalized_path);
    let controller_runtime = controller_runtime_from_state(&state)?;
    let workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;

    if workspace_snapshot
        .task_projects
        .iter()
        .any(|project| task_project_matches_path(project, &normalized_path_text))
    {
        return refresh_workbench_snapshot(state).await.map(Some);
    }

    if let Some(project) = workspace_snapshot
        .archived_task_projects
        .iter()
        .find(|project| task_project_matches_path(project, &normalized_path_text))
        .cloned()
    {
        let restored_sessions = project
            .session_ids
            .iter()
            .filter_map(|session_id| {
                workspace_snapshot
                    .archived_sessions
                    .iter()
                    .find(|session| session.id == *session_id)
                    .cloned()
            })
            .collect::<Vec<_>>();
        vulcan_workspace_store::workspace_store::restore_project_with_sessions(
            controller_runtime,
            &workspace_snapshot.task_projects,
            &project,
            &restored_sessions,
        )
        .await?;
        return refresh_workbench_snapshot(state).await.map(Some);
    }

    let title = normalized_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalized_path_text.clone());
    let order = workspace_snapshot
        .task_projects
        .iter()
        .filter(|project| !project.pinned)
        .map(|project| project.order)
        .max()
        .map(|order| order + 1)
        .unwrap_or(0);
    let project_id = build_task_project_id(&workspace_snapshot, &title, &normalized_path_text);
    let project = TaskProjectSummary::new(
        project_id,
        title,
        normalized_path_text,
        false,
        order,
        None,
        None,
        Vec::new(),
    );
    vulcan_workspace_store::workspace_store::persist_new_project(controller_runtime, &project)
        .await?;
    refresh_workbench_snapshot(state).await.map(Some)
}

/// Toggles one project pinned state and returns the refreshed workbench snapshot.
/// 切换单个项目置顶状态并返回刷新后的工作台快照。
#[tauri::command]
async fn service_toggle_project_pinned(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id),
            ]],
        )
        .await
        .map_err(|error| format!("切换项目置顶状态失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Reorders visible projects inside one pinned or regular section.
/// 在同一个置顶或普通分区内重排可见项目。
#[tauri::command]
async fn service_reorder_projects(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    if project_ids.is_empty() {
        return refresh_workbench_snapshot(state).await;
    }

    let snapshot = service_workbench_snapshot(state.clone())?;
    let first_project = snapshot
        .projects
        .iter()
        .find(|project| project.id == project_ids[0])
        .ok_or_else(|| String::from("重排项目失败：第一个项目不存在。"))?;
    let section_pinned = first_project.pinned;

    for project_id in &project_ids {
        let Some(project) = snapshot
            .projects
            .iter()
            .find(|candidate| candidate.id == *project_id)
        else {
            return Err(format!("重排项目失败：项目不存在：{project_id}"));
        };
        if project.pinned != section_pinned {
            return Err(String::from("重排项目失败：不能跨置顶与普通分区拖拽。"));
        }
    }

    let controller_runtime = controller_runtime_from_state(&state)?;
    let updated_at = current_unix_millis_text();
    let rows = project_ids
        .into_iter()
        .enumerate()
        .map(|(index, project_id)| {
            vec![
                ControllerSqliteValue::Int64(index as i64),
                ControllerSqliteValue::String(updated_at.clone()),
                ControllerSqliteValue::String(project_id),
            ]
        })
        .collect();
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            rows,
        )
        .await
        .map_err(|error| format!("重排项目失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Applies a project display name and returns the refreshed workbench snapshot.
/// 应用项目显示名称并返回刷新后的工作台快照。
#[tauri::command]
async fn service_rename_project(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    vulcan_workspace_store::workspace_store::rename_project(controller_runtime, &project_id, &name)
        .await?;
    refresh_workbench_snapshot(state).await
}

/// Applies a project icon override and returns the refreshed workbench snapshot.
/// 应用项目图标覆盖并返回刷新后的工作台快照。
#[tauri::command]
async fn service_set_project_icon(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
    icon_pack: String,
    icon_key: String,
) -> Result<WorkbenchSnapshot, String> {
    let normalized_icon_pack = normalize_non_empty_name(&icon_pack)?;
    let normalized_icon_key = normalize_non_empty_name(&icon_key)?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET icon_pack = ?1, icon_key = ?2, updated_at = ?3 WHERE id = ?4 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(normalized_icon_pack),
                ControllerSqliteValue::String(normalized_icon_key),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id),
            ]],
        )
        .await
        .map_err(|error| format!("设置项目图标失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Clears a project icon override and returns the refreshed workbench snapshot.
/// 清除项目图标覆盖并返回刷新后的工作台快照。
#[tauri::command]
async fn service_reset_project_icon(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET icon_pack = NULL, icon_key = NULL, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id),
            ]],
        )
        .await
        .map_err(|error| format!("还原项目图标失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Persists the expanded project identifiers and returns the refreshed workbench snapshot.
/// 持久化展开项目标识集合并返回刷新后的工作台快照。
#[tauri::command]
async fn service_set_expanded_task_projects(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    let workspace_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    let expanded_project_ids = project_ids.into_iter().collect::<BTreeSet<_>>();
    let collapsed_project_ids = workspace_snapshot
        .task_projects
        .iter()
        .filter(|project| !expanded_project_ids.contains(&project.id))
        .map(|project| project.id.clone())
        .collect::<Vec<_>>();
    vulcan_workspace_store::workspace_store::persist_collapsed_task_project_ids(
        controller_runtime,
        &collapsed_project_ids,
    )
    .await?;
    refresh_workbench_snapshot(state).await
}

/// Archives all visible conversations under one project and returns the refreshed workbench snapshot.
/// 归档单个项目下的全部可见对话并返回刷新后的工作台快照。
#[tauri::command]
async fn service_archive_project_conversations(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    let workspace_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    let Some(project) = workspace_snapshot
        .task_projects
        .iter()
        .find(|project| project.id == project_id)
    else {
        return Err(format!("未找到要归档的项目：{project_id}"));
    };

    vulcan_workspace_store::workspace_store::archive_project_with_sessions(
        controller_runtime,
        &project_id,
        &project.session_ids,
    )
    .await?;
    refresh_workbench_snapshot(state).await
}

/// Applies a conversation display name and returns the refreshed workbench snapshot.
/// 应用会话显示名称并返回刷新后的工作台快照。
#[tauri::command]
async fn service_rename_conversation(
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let normalized_name = normalize_non_empty_name(&name)?;
    let parsed_session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE sessions SET title = ?1, title_source = 'manual', updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(normalized_name),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(parsed_session_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("重命名会话失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Archives one visible conversation and returns the refreshed workbench snapshot.
/// 归档单个可见会话并返回刷新后的工作台快照。
#[tauri::command]
async fn service_archive_conversation(
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    vulcan_workspace_store::workspace_store::archive_single_session(controller_runtime, session_id)
        .await?;
    refresh_workbench_snapshot(state).await
}

/// Marks one conversation as read and returns the refreshed workbench snapshot.
/// 将单个会话标记为已读并返回刷新后的工作台快照。
#[tauri::command]
async fn service_mark_conversation_read(
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    let mut workspace_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    let marked_read = if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        if runtime_state.ui_state.unread {
            runtime_state.ui_state.unread = false;
            runtime_state.updated_at = current_unix_millis_text();
            true
        } else {
            false
        }
    } else {
        false
    };
    if !marked_read {
        return refresh_workbench_snapshot(state).await;
    }
    persist_webui_session_runtime_state_update(
        state.inner(),
        controller_runtime,
        session_id,
        &workspace_snapshot,
    )
    .await?;
    refresh_workbench_snapshot(state).await
}

/// Selects one conversation in the retained WebUI workspace state.
/// 在 WebUI 保留的工作区状态中选中一个会话。
#[tauri::command]
async fn service_select_conversation(
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        provider_configuration,
        ai_tool_configuration,
        live_context_budgets,
        mut workspace_snapshot,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = guard
            .luaskills_runtime
            .as_ref()
            .and_then(|handle| crate::luaskills_bridge::load_luaskills_catalog(handle).ok())
            .unwrap_or_else(empty_luaskills_catalog);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = guard
            .mcp_runtime
            .as_ref()
            .map(TauriMcpRuntimeHandle::catalog)
            .unwrap_or_else(empty_mcp_catalog);
        let logs = guard.startup.recent_logs.clone();
        let workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let live_context_budgets = guard.live_context_budgets.clone();
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            provider_configuration,
            ai_tool_configuration,
            live_context_budgets,
            workspace_snapshot,
        )
    };
    let exists = workspace_snapshot
        .sessions
        .iter()
        .any(|session| session.id == session_id);
    if !exists {
        return refresh_workbench_snapshot(state).await;
    }
    workspace_snapshot.selected_session = session_id;
    let (surface_messages, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
        )
    };
    let workbench = workbench_from_runtime(
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        &logs,
        &live_context_budgets,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.workbench = workbench.clone();
    }
    Ok(workbench)
}

/// Opens one filesystem directory with the host file manager.
/// 使用宿主文件管理器打开单个文件系统目录。
fn open_path_in_system_file_manager(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Opens one URL with the host default browser.
/// 使用宿主默认浏览器打开单个 URL。
fn open_url_in_system_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Normalizes and validates a user-provided display name.
/// 规范化并校验用户提供的显示名称。
fn normalize_non_empty_name(name: &str) -> Result<String, String> {
    let normalized_name = name.trim();
    if normalized_name.is_empty() {
        return Err(String::from("名称不能为空。"));
    }
    Ok(normalized_name.to_string())
}

/// Reads the controller runtime handle from shared service state.
/// 从共享服务状态中读取控制器运行时句柄。
fn controller_runtime_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<ControllerRuntimeHandle, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .controller_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; controller runtime is not ready"))
}

/// Reads the selected session identifier from the retained workspace state.
/// 从保留的工作区状态中读取当前选中会话标识。
fn selected_session_id_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<SessionId, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref()
        && workspace_snapshot.selected_session != SessionId::nil()
    {
        return Ok(workspace_snapshot.selected_session);
    }
    guard
        .workbench
        .selected_conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .ok_or_else(|| String::from("当前没有选中的会话。"))
}

/// Clones the latest in-memory workspace snapshot retained by the service bridge.
/// 克隆服务桥接层保留的最近内存工作区快照。
fn current_workspace_snapshot_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<Option<WorkspaceSnapshot>, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workspace_snapshot.clone())
}

/// Clones the latest in-memory workspace snapshot from a shared service state handle.
/// 从共享服务状态句柄克隆最近的内存工作区快照。
fn current_workspace_snapshot_from_shared_state(
    state: &SharedServiceBridgeState,
) -> Result<Option<WorkspaceSnapshot>, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workspace_snapshot.clone())
}

/// Clones the latest in-memory workbench snapshot retained by the service bridge.
/// 克隆服务桥接层保留的最近内存工作台快照。
///
/// # Parameters
/// 参数
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
///
/// # Returns
/// 返回值
/// The latest workbench snapshot already emitted by an in-memory mutation.
/// 返回内存变更已经发出的最近工作台快照。
fn current_workbench_snapshot_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workbench.clone())
}

/// Reads all long-lived runtime handles required by a WebUI prompt submission.
/// 读取 WebUI 提示词提交所需的全部长生命周期运行时句柄。
fn runtime_parts_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<
    (
        ControllerRuntimeHandle,
        AgentRuntimeBridge,
        TauriLuaSkillsCatalog,
        Arc<TauriTokenBudgetRuntime>,
        TauriMcpRuntimeCatalog,
        Vec<String>,
    ),
    String,
> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; controller runtime is not ready")
    })?;
    let agent_runtime = guard
        .agent_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
    let luaskills_catalog = guard
        .luaskills_runtime
        .as_ref()
        .and_then(|handle| crate::luaskills_bridge::load_luaskills_catalog(handle).ok())
        .unwrap_or_else(empty_luaskills_catalog);
    let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; token budget runtime is not ready")
    })?;
    let mcp_catalog = guard
        .mcp_runtime
        .as_ref()
        .map(TauriMcpRuntimeHandle::catalog)
        .unwrap_or_else(empty_mcp_catalog);
    Ok((
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        guard.startup.recent_logs.clone(),
    ))
}

/// Loads the workspace snapshot and hydrates the selected submitted session when it already exists.
/// 加载工作区快照，并在提交目标会话已存在时水合该会话。
async fn load_workspace_for_submission(
    controller_runtime: ControllerRuntimeHandle,
    conversation_id: Option<&str>,
) -> Result<WorkspaceSnapshot, String> {
    let mut workspace_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    if let Some(session_id) = conversation_id.and_then(parse_session_id) {
        match vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
            controller_runtime,
            session_id,
            SELECTED_SESSION_TURN_LOAD_LIMIT,
        )
        .await
        {
            Ok(hydration) => {
                workspace_snapshot
                    .session_messages
                    .insert(session_id, hydration.messages);
                if let Some(runtime_state) = hydration.runtime_state {
                    workspace_snapshot
                        .session_runtime_state
                        .insert(session_id, runtime_state);
                }
                workspace_snapshot.hydrated_session_ids.insert(session_id);
            }
            Err(error) if is_missing_session_manifest_error(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(workspace_snapshot)
}

/// Returns whether a retained WebUI snapshot has enough session context for prompt submission.
/// 判断保留的 WebUI 快照是否已有足够的会话上下文可直接提交提示词。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the in-memory snapshot retained by the service bridge.
/// `workspace_snapshot` 是服务桥保留的内存工作区快照。
/// `session_id` selects the submitted conversation.
/// `session_id` 选择本次提交目标会话。
///
/// # Returns
/// 返回值
/// `true` when the session was hydrated or already owns concrete conversation context.
/// 当会话已水合或已拥有具体会话上下文时返回 `true`。
fn retained_submission_snapshot_has_session_context(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    if workspace_snapshot
        .hydrated_session_ids
        .contains(&session_id)
    {
        return true;
    }
    if workspace_snapshot
        .session_messages
        .contains_key(&session_id)
    {
        return true;
    }
    workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(runtime_state_has_concrete_conversation_context)
}

/// Returns whether a runtime state carries concrete conversation content rather than a shell row.
/// 判断运行态是否携带具体会话内容，而不是仅有轻量 shell 行。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state to inspect.
/// `runtime_state` 是待检查的会话运行态。
///
/// # Returns
/// 返回值
/// `true` when canonical messages or visible records are already loaded.
/// 当规范消息或可见记录已经加载时返回 `true`。
fn runtime_state_has_concrete_conversation_context(
    runtime_state: &SessionRuntimeStateEntry,
) -> bool {
    !runtime_state.conversation_state.messages.is_empty()
        || !runtime_state.conversation_state.records.is_empty()
}

/// Returns whether a submitted prompt must enter the visible queue instead of starting immediately.
/// 判断提交的提示词是否必须进入可见队列而不是立即启动。
fn session_has_blocking_system_prompt_work(
    workspace_snapshot: &WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
    has_active_stream_task: bool,
    has_running_subagents: bool,
) -> bool {
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return has_active_stream_task || has_running_subagents;
    };
    let dispatch_context = webui_pending_prompt_dispatch_context(
        workspace_snapshot,
        provider_configuration,
        session_id,
        has_active_stream_task,
        has_running_subagents,
    );
    let has_precheck_progress = matches!(
        dispatch_context.run_phase,
        Some(SessionRunPhase::MemoryRecall | SessionRunPhase::Preparing)
    );
    pending_prompt_lane_is_busy(
        &runtime_state.ui_state,
        PendingPromptBusyLaneContext {
            dispatch: dispatch_context,
            has_precheck_progress,
        },
    )
}

/// Builds the shared pending-prompt dispatch context from WebUI-owned runtime facts.
/// 从 WebUI 持有的运行态事实构建共享待发送提示词分发上下文。
fn webui_pending_prompt_dispatch_context(
    workspace_snapshot: &WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
    has_active_stream_task: bool,
    has_running_subagents: bool,
) -> PendingPromptDispatchContext {
    PendingPromptDispatchContext {
        uses_anthropic_runtime: session_uses_anthropic_runtime(
            workspace_snapshot,
            provider_configuration,
            session_id,
        ),
        has_running_subagents,
        has_active_run: has_active_stream_task,
        run_phase: workspace_snapshot
            .session_run_phases
            .get(&session_id)
            .copied(),
        has_active_compaction: false,
    }
}

/// Returns whether the shared task runtime still reports a child subagent as running.
/// 判断共享任务运行时是否仍报告某个子代理正在运行。
fn session_has_running_subagent_tasks_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    if session_cached_subagent_tasks_have_live_run(guard, session_id) {
        return true;
    }
    rebuild_webui_subagent_task_cache_for_session(guard, workspace_snapshot, session_id);
    session_cached_subagent_tasks_have_live_run(guard, session_id)
}

/// Returns whether the WebUI route cache still has a live child task for one parent session.
/// 返回 WebUI 路由缓存中是否仍有属于某个父会话的实时子任务。
fn session_cached_subagent_tasks_have_live_run(
    guard: &ServiceBridgeState,
    session_id: SessionId,
) -> bool {
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let live_listener_task_ids = &guard.detached_subagent_task_listeners;
    guard
        .session_subagent_tasks
        .get(&session_id)
        .is_some_and(|tasks| {
            tasks.values().any(|state| {
                state.run_state.is_running()
                    && webui_subagent_task_has_live_run(
                        task_runtime,
                        live_listener_task_ids,
                        state.task_id,
                    )
            })
        })
}

/// Returns child task states that are still marked as running in the WebUI-routed subagent cache.
/// 返回 WebUI 路由子代理缓存中仍标记为运行中的子任务状态。
fn running_webui_subagent_task_states_from_state(
    guard: &ServiceBridgeState,
    session_id: SessionId,
) -> Vec<SubagentTaskPanelState> {
    guard
        .session_subagent_tasks
        .get(&session_id)
        .map(|tasks| {
            tasks
                .values()
                .filter(|state| state.run_state.is_running())
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Marks the provided running WebUI subagent tasks as stopped and clears their live listener routes.
/// 将给定的 WebUI 运行中子代理标记为已停止，并清理它们的实时监听路由。
fn mark_webui_subagent_tasks_stopped_in_state(
    guard: &mut ServiceBridgeState,
    session_id: SessionId,
    task_ids: &[SessionId],
) {
    if let Some(tasks) = guard.session_subagent_tasks.get_mut(&session_id) {
        for task_id in task_ids {
            if let Some(state) = tasks.get_mut(task_id) {
                state.run_state = SubagentTaskRunState::Stopped;
                if state.result_text.trim().is_empty() {
                    state.result_text = SUBAGENT_TASK_STOPPED_SUMMARY_TEXT.to_string();
                }
            }
        }
    }
    for task_id in task_ids {
        guard.detached_subagent_task_listeners.remove(task_id);
    }
    guard.session_subagent_navigation_rounds.remove(&session_id);
    if guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id == session_id)
    {
        guard.active_subagent_panel = None;
    }
}

/// Stops WebUI-visible session work and restores queue state to the iced stopped semantics.
/// 停止 WebUI 可见会话任务，并按 iced 停止语义恢复队列状态。
fn stop_webui_session_work(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    let now = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    clear_internal_pending_prompt_queue_and_restore_panel(&mut runtime_state.ui_state);
    runtime_state.active_precheck_turn_id = None;
    runtime_state.updated_at = now.clone();
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("已按用户请求停止当前会话运行。");
        session.last_updated_label = String::from("刚刚");
    }
    workspace_snapshot.session_messages.insert(
        session_id,
        conversation_state_to_session_messages(&runtime_state.conversation_state),
    );
    workspace_snapshot.hydrated_session_ids.insert(session_id);
    Ok(())
}

/// Marks one idle WebUI session as waiting for an already guided internal prompt.
/// 将一个空闲 WebUI 会话标记为正在等待已引导的内部提示词。
fn mark_webui_session_waiting_for_guided_prompt_dispatch(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) {
    if workspace_snapshot
        .sessions
        .iter()
        .any(|session| session.id == session_id && session.status == SessionState::Running)
    {
        return;
    }
    let now = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.updated_at = now.clone();
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("已有消息接入主会话，正在等待当前系统任务处理完成后继续。");
        session.last_updated_label = String::from("刚刚");
    }
    workspace_snapshot.session_messages.insert(
        session_id,
        conversation_state_to_session_messages(&runtime_state.conversation_state),
    );
    workspace_snapshot.hydrated_session_ids.insert(session_id);
}

/// Builds one WebUI-originated visible pending prompt entry.
/// 构建一条来自 WebUI 的可见待发送提示词条目。
fn build_webui_pending_prompt_panel_entry(
    prompt: String,
    attachments: Vec<ConversationAttachment>,
    model_reference: Option<PendingPromptModelReference>,
    runtime_options: PendingPromptRuntimeOptions,
) -> PendingPromptPanelEntry {
    let created_at = current_unix_millis_text();
    PendingPromptPanelEntry {
        id: format!("webui-pending-panel-{}", SessionId::new_v4()),
        preview: pending_prompt_preview_text(&prompt, &attachments),
        prompt,
        attachments,
        model_reference,
        runtime_options,
        state: PendingPromptPanelEntryState::Ready,
        created_at,
    }
}

/// Appends one visible pending prompt entry to the selected session queue and mirrors the queued user event.
/// 将一条可见待发送提示词追加到选中会话队列，并镜像已排队用户事件。
fn enqueue_webui_pending_prompt_panel_entry(
    workspace_snapshot: &mut WorkspaceSnapshot,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    workspace_path: &str,
    entry: PendingPromptPanelEntry,
) -> Result<(), String> {
    let now = current_unix_millis_text();
    let project_id = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.clone());
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    if runtime_state.ui_state.pending_prompt_panel_queue.len() >= PENDING_PROMPT_QUEUE_LIMIT {
        return Err(String::from(
            "待发送消息队列已满，最多保留 10 条，请等待前面的消息完成后再继续追加。",
        ));
    }
    let entry_id = entry.id.clone();
    let prompt = entry.prompt.clone();
    runtime_state
        .ui_state
        .pending_prompt_panel_queue
        .push(entry);
    runtime_state.updated_at = now;
    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
        hook_runtime.record_queued_user_prompt(
            session_id,
            &entry_id,
            project_id,
            workspace_path.to_string(),
            PendingSessionPromptSource::UserFollowup,
            &prompt,
        );
    }
    Ok(())
}

/// Inserts one visible WebUI queue row into the internal session prompt queue.
/// 将一条 WebUI 可见队列行插入内部会话提示词队列。
fn insert_webui_pending_prompt_into_internal_queue(
    workspace_snapshot: &mut WorkspaceSnapshot,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    workspace_path: &str,
    entry_id: &str,
) -> Result<(), String> {
    let project_id = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.clone());
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| String::from("当前会话没有可引导的待发送队列。"))?;
    let created_at = current_unix_millis_text();
    let internal_entry_id = format!("webui-pending-internal-{}", SessionId::new_v4());
    let route_outcome = route_pending_prompt_panel_entry_into_internal_queue(
        &mut runtime_state.ui_state,
        entry_id,
        internal_entry_id,
        created_at,
    )
    .map_err(|error| match error {
        PendingPromptQueueRouteError::MissingPanelEntry => {
            String::from("没有找到待引导的待发送提示词。")
        }
        PendingPromptQueueRouteError::InternalQueueFull => {
            String::from("待发送消息队列已满，最多保留 10 条，请等待前面的消息完成后再继续追加。")
        }
    })?;
    runtime_state.updated_at = current_unix_millis_text();
    let PendingPromptQueueRouteOutcome::Inserted(internal_entry) = route_outcome else {
        return Ok(());
    };
    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
        hook_runtime.record_queued_user_prompt(
            session_id,
            entry_id,
            project_id,
            workspace_path.to_string(),
            PendingSessionPromptSource::UserFollowup,
            &internal_entry.prompt,
        );
    }
    Ok(())
}

/// Returns whether one session should use Anthropic-specific queue dispatch ordering.
/// 返回单个会话是否应使用 Anthropic 专属队列分发顺序。
fn session_uses_anthropic_runtime(
    workspace_snapshot: &WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
) -> bool {
    if let Some(active_provider_id) = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_provider_id.as_deref())
    {
        return provider_config::supported_provider_by_id(active_provider_id)
            .is_some_and(|definition| definition.runtime == ProviderRuntimeKind::Anthropic);
    }
    provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
        .and_then(|(provider_id, _)| provider_config::supported_provider_by_id(provider_id))
        .is_some_and(|definition| definition.runtime == ProviderRuntimeKind::Anthropic)
}

/// Returns the workspace path associated with one session.
/// 返回单个会话关联的工作区路径。
fn workspace_path_for_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> String {
    workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.as_deref())
        .and_then(|project_id| {
            workspace_snapshot
                .task_projects
                .iter()
                .find(|project| project.id == project_id)
        })
        .map(|project| project.path.clone())
        .unwrap_or_else(|| workspace_root().display().to_string())
}

/// Builds one compact single-line preview used by the pending prompt queue strip.
/// 构建待发送提示词队列条使用的单行紧凑预览。
fn pending_prompt_preview_text(prompt: &str, attachments: &[ConversationAttachment]) -> String {
    let trimmed_prompt = prompt.trim();
    let attachment_prefix = if attachments.is_empty() {
        String::new()
    } else {
        format!("{} 个附件 · ", attachments.len())
    };
    let text = if trimmed_prompt.is_empty() {
        String::from("附件消息")
    } else {
        trimmed_prompt
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let preview = format!("{attachment_prefix}{text}");
    if preview.chars().count() <= 48 {
        preview
    } else {
        format!("{}...", preview.chars().take(48).collect::<String>())
    }
}

/// Captures the active provider/model identity for one queued WebUI prompt.
/// 为单条 WebUI 排队提示词捕获当前激活的供应商与模型身份。
fn current_pending_prompt_model_reference(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<PendingPromptModelReference> {
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    Some(PendingPromptModelReference {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        provider_label: provider_config::provider_summary(provider_id),
        model_name: provider_config::model_summary(provider_id, model_id, provider_configuration),
        openrouter_endpoint_route: (provider_id == "openrouter")
            .then(|| {
                provider_config::openrouter_endpoint_route_for_model(
                    provider_configuration,
                    model_id,
                )
            })
            .flatten(),
    })
}

/// Captures runtime controls for one queued WebUI prompt from the shared service router input.
/// 从共享服务路由输入捕获单条 WebUI 排队提示词的运行控制参数。
fn pending_prompt_runtime_options_from_submission(
    agent_runtime: &AgentRuntimeBridge,
    reasoning_effort: Option<&str>,
    disabled_ai_capability_kinds: Option<&[String]>,
) -> Result<PendingPromptRuntimeOptions, String> {
    Ok(PendingPromptRuntimeOptions {
        reasoning_effort: reasoning_effort
            .and_then(parse_reasoning_effort)
            .or_else(|| active_reasoning_selection_effort(&agent_runtime.provider_configuration))
            .unwrap_or(ReasoningEffortLevel::Medium),
        temperature: None,
        web_search_override: Default::default(),
        web_search_provider_id: None,
        disabled_ai_capability_kinds: effective_workbench_disabled_ai_capability_kinds(
            &agent_runtime.ai_tool_configuration,
            disabled_ai_capability_kinds,
        )?,
    })
}

/// Clones provider configuration and overrides only the queued prompt's locked model identity.
/// 克隆 Provider 配置，并仅覆盖排队提示词锁定的模型身份。
fn provider_configuration_for_model_reference(
    provider_configuration: &ProviderConfigurationFile,
    model_reference: Option<&PendingPromptModelReference>,
) -> ProviderConfigurationFile {
    let mut next_configuration = provider_configuration.clone();
    if let Some(model_reference) = model_reference {
        next_configuration.model = Some(format!(
            "{}/{}",
            model_reference.provider_id, model_reference.model_id
        ));
        if model_reference.provider_id == "openrouter" {
            provider_config::set_openrouter_endpoint_route_for_model(
                &mut next_configuration,
                &model_reference.model_id,
                model_reference.openrouter_endpoint_route.clone(),
            );
        }
    }
    next_configuration
}

/// Clones the runtime bridge with the queued prompt's locked provider configuration.
/// 使用排队提示词锁定的 Provider 配置克隆运行时桥接。
fn agent_runtime_for_pending_prompt(
    agent_runtime: &AgentRuntimeBridge,
    prompt_entry: &PendingSessionPromptEntry,
) -> AgentRuntimeBridge {
    let mut next_runtime = agent_runtime.clone();
    next_runtime.provider_configuration = provider_configuration_for_model_reference(
        &agent_runtime.provider_configuration,
        prompt_entry.model_reference.as_ref(),
    );
    next_runtime
}

/// Returns the owning project id for one session when it has a persisted project scope.
/// 当会话拥有持久化项目作用域时返回所属项目标识。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the session summaries.
/// `workspace_snapshot` 包含会话摘要。
/// `session_id` selects the session.
/// `session_id` 选择目标会话。
///
/// # Returns
/// 返回值
/// A non-empty project id when the session belongs to one project.
/// 当会话属于某个项目时返回非空项目标识。
fn owning_project_id_for_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<String> {
    workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.clone())
        .filter(|project_id| !project_id.trim().is_empty())
}

/// Captures the iced-equivalent context snapshot before a WebUI user prompt mutates context.
/// 在 WebUI 用户提示词改变上下文前捕获与 iced 等价的上下文快照。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the current runtime state.
/// `workspace_snapshot` 包含当前运行态。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `source` identifies whether this prompt is visible user input or delegated completion.
/// `source` 标识该提示词是可见用户输入还是委派完成回流。
///
/// # Returns
/// 返回值
/// `Ok(())` after the snapshot either succeeds or is not needed.
/// 快照成功或无需快照后返回 `Ok(())`。
fn snapshot_webui_context_before_user_prompt(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    source: PendingSessionPromptSource,
) -> Result<(), String> {
    if source == PendingSessionPromptSource::DelegateCompletion {
        return Ok(());
    }
    let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) else {
        return Ok(());
    };
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return Ok(());
    };
    vulcan_workspace_store::workspace_store::snapshot_session_context_before_user_prompt(
        &project_id,
        session_id,
        runtime_state,
    )
}

/// Applies iced-equivalent outer steps after one WebUI prompt begin reducer has run.
/// 在 WebUI 提示词 begin reducer 执行后应用与 iced 等价的外层步骤。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the mutable WebUI workspace snapshot.
/// `workspace_snapshot` 是可变 WebUI 工作区快照。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `session_title` labels diagnostics.
/// `session_title` 标记诊断信息。
fn apply_webui_prompt_begin_outer_steps(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    session_title: &str,
) {
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        && remove_legacy_vmm_profile_context_injection_state(
            &mut runtime_state.context_injection_state,
        )
    {
        runtime_state.updated_at = current_unix_millis_text();
    }
    persist_webui_context_current_document(workspace_snapshot, session_id, session_title);
}

/// Persists the iced-equivalent current context document for a WebUI session when possible.
/// 在可行时为 WebUI 会话持久化与 iced 等价的当前上下文文档。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the current runtime state.
/// `workspace_snapshot` 包含当前运行态。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `session_title` labels diagnostics.
/// `session_title` 标记诊断信息。
fn persist_webui_context_current_document(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    session_title: &str,
) {
    let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) else {
        return;
    };
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return;
    };
    if let Err(error) =
        vulcan_workspace_store::workspace_store::persist_session_context_current_document(
            &project_id,
            session_id,
            runtime_state,
        )
    {
        eprintln!("[tauri-service] current context persist failed for `{session_title}`: {error}");
    }
}

/// Resolves or creates the session that should receive the submitted prompt.
/// 解析或创建应该接收本次提示词的会话。
fn materialize_prompt_session(
    workspace_snapshot: &mut WorkspaceSnapshot,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
    prompt: &str,
) -> Result<(SessionId, String, String), String> {
    if let Some(session_id) = conversation_id.and_then(parse_session_id)
        && let Some(session) = workspace_snapshot
            .sessions
            .iter_mut()
            .find(|candidate| candidate.id == session_id)
    {
        session.status = SessionState::Running;
        session.last_updated_label = String::from("刚刚");
        workspace_snapshot.selected_session = session_id;
        let workspace_path = session
            .project_id
            .as_deref()
            .and_then(|owner_id| {
                workspace_snapshot
                    .task_projects
                    .iter()
                    .find(|project| project.id == owner_id)
            })
            .map(|project| project.path.clone())
            .unwrap_or_default();
        return Ok((session_id, session.title.clone(), workspace_path));
    }

    let project_id = project_id.ok_or_else(|| String::from("新建会话需要提供项目标识。"))?;
    let project = workspace_snapshot
        .task_projects
        .iter_mut()
        .find(|candidate| candidate.id == project_id)
        .ok_or_else(|| format!("未找到项目：{project_id}"))?;
    let session_id = SessionId::new_v4();
    let session_title = prompt_to_session_title(prompt);
    let session = SessionSummary::new(
        session_id,
        Some(project.id.clone()),
        session_title.clone(),
        "正在执行 WebUI 提交的任务",
        SessionState::Running,
        0,
        0,
        "刚刚",
    )
    .with_title_source(SessionTitleSource::Generated);

    project
        .session_ids
        .retain(|existing| *existing != session_id);
    project.session_ids.insert(0, session_id);
    let workspace_path = project.path.clone();
    workspace_snapshot.sessions.insert(0, session);
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::MemoryRecall);
    workspace_snapshot.selected_session = session_id;
    Ok((session_id, session_title, workspace_path))
}

/// Parses prompt-level AI capability target order overrides from the WebUI payload.
/// 从 WebUI 载荷中解析提示词级 AI 能力目标顺序覆盖。
///
/// # Parameters
/// 参数
/// `raw_overrides` contains capability-keyed target selector arrays.
/// `raw_overrides` 包含按能力键分组的目标选择器数组。
///
/// # Returns
/// 返回值
/// A semantic capability keyed map accepted by the shared runtime router.
/// 共享运行时路由器可接受的语义能力映射。
fn parse_ai_capability_target_order_overrides(
    raw_overrides: Option<BTreeMap<String, Vec<String>>>,
) -> Result<BTreeMap<vulcan_provider_config::config::AiToolKind, Vec<String>>, String> {
    let mut overrides = BTreeMap::new();
    for (raw_kind, selectors) in raw_overrides.unwrap_or_default() {
        let Some(kind) =
            vulcan_agent_runtime::workbench_ai_capability_router::parse_workbench_ai_tool_kind(
                &raw_kind,
            )
        else {
            return Err(format!("未知的扩展能力排序类型：{raw_kind}"));
        };
        let cleaned_selectors = selectors
            .into_iter()
            .map(|selector| selector.trim().to_string())
            .filter(|selector| !selector.is_empty())
            .collect::<Vec<_>>();
        if !cleaned_selectors.is_empty() {
            overrides.insert(kind, cleaned_selectors);
        }
    }
    Ok(overrides)
}

/// Parses prompt-level AI capability target selectors from the WebUI payload.
/// 从 WebUI 载荷中解析提示词级 AI 能力目标选择器。
///
/// # Parameters
/// 参数
/// `raw_selectors` contains optional target selector strings.
/// `raw_selectors` 包含可选的目标选择器字符串。
///
/// # Returns
/// 返回值
/// A trimmed selector set accepted by the shared prompt-turn core.
/// 共享提示词回合核心可接受的已清洗选择器集合。
fn parse_ai_capability_target_selectors(raw_selectors: Option<Vec<String>>) -> BTreeSet<String> {
    raw_selectors
        .unwrap_or_default()
        .into_iter()
        .map(|selector| selector.trim().to_string())
        .filter(|selector| !selector.is_empty())
        .collect()
}

/// Loads project-local Vulcan Code configuration for one WebUI execution workspace.
/// 为一个 WebUI 执行工作区加载项目级 Vulcan Code 配置。
///
/// # Parameters
/// 参数
/// `workspace_path` is the absolute project workspace path.
/// `workspace_path` 是项目工作区绝对路径。
///
/// # Returns
/// 返回值
/// The project-local configuration, or the default configuration when loading fails.
/// 项目级配置；加载失败时返回默认配置。
fn load_project_configuration(
    workspace_path: &str,
) -> vulcan_provider_config::config::ProjectVulcanCodeConfigurationFile {
    vulcan_provider_config::config::load_project_configuration_file(&PathBuf::from(workspace_path))
        .unwrap_or_default()
}

/// Builds the shared precheck plan for a WebUI session using the iced VMM layering rules.
/// 使用 iced 的 VMM 分层规则为一个 WebUI 会话构建共享 precheck 计划。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns provider configuration and hook runtime state.
/// `agent_runtime` 持有 Provider 配置与 hook 运行态。
/// `workspace_snapshot` contains session-scoped VMM overrides.
/// `workspace_snapshot` 包含会话级 VMM 覆盖配置。
/// `session_id` selects the session being prepared.
/// `session_id` 选择正在准备的会话。
/// `workspace_path` identifies the project configuration file to load.
/// `workspace_path` 标识需要加载的项目配置文件。
///
/// # Returns
/// 返回值
/// A shared precheck plan matching the iced shell.
/// 返回与 iced 外壳一致的共享 precheck 计划。
fn webui_shared_prompt_precheck_plan(
    agent_runtime: &AgentRuntimeBridge,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    workspace_path: &str,
) -> Result<SharedPromptPrecheckPlan, String> {
    let project_configuration = load_project_configuration(workspace_path);
    let session_layer = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .map(|runtime_state| &runtime_state.vmm);
    let effective_vmm_config = vulcan_vmm::VmmEffectiveConfig::resolve(
        &agent_runtime.provider_configuration.vmm,
        Some(&project_configuration.vmm),
        session_layer,
    );
    let precheck_hook_snapshots = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?
        .precheck_hook_snapshots();
    Ok(shared_prompt_precheck_plan(
        &effective_vmm_config,
        &precheck_hook_snapshots,
    ))
}

/// Builds the shared VMM hook snapshot from the current Tauri workspace state.
/// 根据当前 Tauri 工作区状态构建共享 VMM hook 快照。
fn build_vmm_hook_config_snapshot(
    provider_configuration: &ProviderConfigurationFile,
    workspace_snapshot: &WorkspaceSnapshot,
) -> VmmHookConfigSnapshot {
    let mut projects_by_workspace = std::collections::BTreeMap::new();
    for project in workspace_snapshot
        .task_projects
        .iter()
        .chain(workspace_snapshot.archived_task_projects.iter())
    {
        let project_configuration = load_project_configuration(&project.path);
        if !project_configuration.vmm.is_empty() {
            projects_by_workspace.insert(project.path.clone(), project_configuration.vmm);
        }
    }

    let sessions = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .filter_map(|(session_id, runtime_state)| {
            (!runtime_state.vmm.is_empty())
                .then(|| (session_id.to_string(), runtime_state.vmm.clone()))
        })
        .collect();
    let outboxes = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .filter_map(|(session_id, runtime_state)| {
            (!runtime_state.vmm_writeback_outbox.is_empty()).then(|| {
                (
                    session_id.to_string(),
                    runtime_state.vmm_writeback_outbox.clone(),
                )
            })
        })
        .collect();
    let profile_sync = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .filter_map(|(session_id, runtime_state)| {
            (!runtime_state.vmm_profile_sync.is_empty()).then(|| {
                (
                    session_id.to_string(),
                    runtime_state.vmm_profile_sync.clone(),
                )
            })
        })
        .collect();
    let last_compaction_ids = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .map(|(session_id, runtime_state)| {
            (
                session_id.to_string(),
                runtime_state.last_compaction_id.clone(),
            )
        })
        .collect();

    VmmHookConfigSnapshot {
        global: provider_configuration.vmm.clone(),
        projects_by_workspace,
        sessions,
        outboxes,
        profile_sync,
        last_compaction_ids,
    }
}

/// Persists only the changed session after a WebUI runtime mutation.
/// 在 WebUI 运行时变更后仅持久化被修改的单个会话。
async fn persist_workspace_session_update(
    state: Option<&SharedServiceBridgeState>,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if let Some(state) = state {
        invalidate_webui_session_persist_version(state, session_id)?;
    }
    let persist_lock = webui_workspace_persist_lock();
    let _persist_guard = persist_lock.lock().await;
    persist_workspace_session_update_unlocked(controller_runtime, workspace_snapshot, session_id)
        .await
}

/// Persists one session snapshot after the caller has acquired the shared WebUI write gate.
/// 在调用方已取得共享 WebUI 写入闸门后持久化单个会话快照。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the workspace snapshot that contains the changed session.
/// `workspace_snapshot` 是包含变更会话的工作区快照。
/// `session_id` selects the session to persist.
/// `session_id` 选择要持久化的会话。
///
/// # Returns
/// 返回值
/// `Ok(())` when the session has been saved.
/// 会话保存完成时返回 `Ok(())`。
async fn persist_workspace_session_update_unlocked(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    let (session, archived) = workspace_snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session_id)
        .map(|session| (session, false))
        .or_else(|| {
            workspace_snapshot
                .archived_sessions
                .iter()
                .find(|candidate| candidate.id == session_id)
                .map(|session| (session, true))
        })
        .ok_or_else(|| format!("单会话保存失败：未找到会话：{session_id}"))?;
    let session_messages = if archived {
        &workspace_snapshot.archived_session_messages
    } else {
        &workspace_snapshot.session_messages
    };
    let session_compactions = if archived {
        &workspace_snapshot.archived_session_compactions
    } else {
        &workspace_snapshot.session_compactions
    };
    let session_runtime_state = if archived {
        &workspace_snapshot.archived_session_runtime_state
    } else {
        &workspace_snapshot.session_runtime_state
    };
    let runtime_state = session_runtime_state.get(&session_id);
    let rebuilt_messages;
    let messages = if let Some(runtime_state) = runtime_state {
        rebuilt_messages =
            conversation_state_to_session_messages(&runtime_state.conversation_state);
        rebuilt_messages.as_slice()
    } else if let Some(messages) = session_messages.get(&session_id) {
        messages.as_slice()
    } else {
        &[]
    };
    let compactions = session_compactions.get(&session_id).map(Vec::as_slice);
    let task_projects = workspace_snapshot
        .task_projects
        .iter()
        .cloned()
        .chain(workspace_snapshot.archived_task_projects.iter().cloned())
        .collect::<Vec<_>>();

    vulcan_workspace_store::workspace_store::persist_single_session_update(
        controller_runtime,
        &task_projects,
        session,
        archived,
        messages,
        compactions,
        runtime_state,
    )
    .await
    .map(|_| ())
}

/// Persists one WebUI session snapshot in the background without blocking visible UI updates.
/// 在后台持久化一个 WebUI 会话快照，不阻塞可见 UI 更新。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the already-rendered snapshot to save.
/// `workspace_snapshot` 是已经渲染过、待保存的快照。
/// `session_id` selects the session that changed.
/// `session_id` 选择发生变更的会话。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
fn spawn_workspace_session_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    reason: Option<String>,
) {
    let persist_version = match next_webui_session_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[tauri-service] background persist version failed: {error}");
            return;
        }
    };
    tauri::async_runtime::spawn(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_persist_version_is_current(&state, session_id, persist_version) {
            return;
        }
        if let Err(error) = persist_workspace_session_update_unlocked(
            controller_runtime,
            &workspace_snapshot,
            session_id,
        )
        .await
        {
            let reason = reason.unwrap_or_else(|| String::from("session_update"));
            eprintln!("[tauri-service] background persist failed ({reason}): {error}");
        }
    });
}

/// Persists only one WebUI session runtime-state row in the background.
/// 在后台仅持久化一个 WebUI 会话运行态行。
///
/// # Parameters
/// 参数
/// `state` owns the retained service state and per-session write versions.
/// `state` 持有保留服务状态与按会话划分的写入版本。
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the already-rendered snapshot containing the runtime state.
/// `workspace_snapshot` 是已经渲染过且包含运行态的快照。
/// `session_id` selects the session runtime-state row to persist.
/// `session_id` 选择需要持久化的会话运行态行。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
fn spawn_workspace_session_runtime_state_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    reason: Option<String>,
) {
    let persist_version = match next_webui_session_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[tauri-service] runtime-state persist version failed: {error}");
            return;
        }
    };
    tauri::async_runtime::spawn(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_persist_version_is_current(&state, session_id, persist_version) {
            return;
        }
        let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .or_else(|| {
                workspace_snapshot
                    .archived_session_runtime_state
                    .get(&session_id)
                    .cloned()
            })
        else {
            return;
        };
        if let Err(error) =
            vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
                controller_runtime,
                session_id,
                &runtime_state,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("runtime_state");
            eprintln!(
                "[tauri-service] background runtime-state persist failed ({reason_label}): {error}"
            );
        }
    });
}

/// Allocates the next background persistence version for one WebUI session.
/// 为一个 WebUI 会话分配下一次后台持久化版本。
///
/// # Parameters
/// 参数
/// `state` owns the per-session persistence version table.
/// `state` 持有按会话归属的持久化版本表。
/// `session_id` selects the session being persisted.
/// `session_id` 选择正在持久化的会话。
///
/// # Returns
/// 返回值
/// The version assigned to the new background persistence task.
/// 返回分配给新后台持久化任务的版本。
fn next_webui_session_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<u64, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let next_version = guard
        .session_persist_versions
        .get(&session_id)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    guard
        .session_persist_versions
        .insert(session_id, next_version);
    Ok(next_version)
}

/// Returns whether a background persistence task is still the latest one for its session.
/// 判断某个后台持久化任务是否仍是其会话的最新任务。
///
/// # Parameters
/// 参数
/// `state` owns the per-session persistence version table.
/// `state` 持有按会话归属的持久化版本表。
/// `session_id` selects the session being persisted.
/// `session_id` 选择正在持久化的会话。
/// `persist_version` is the task-local persistence version.
/// `persist_version` 是任务本地持久化版本。
///
/// # Returns
/// 返回值
/// `true` when the task may write to disk.
/// 当该任务可以写入磁盘时返回 `true`。
fn webui_session_persist_version_is_current(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    persist_version: u64,
) -> bool {
    state
        .lock()
        .ok()
        .and_then(|guard| guard.session_persist_versions.get(&session_id).copied())
        == Some(persist_version)
}

/// Invalidates older background persistence snapshots for one WebUI session.
/// 使某个 WebUI 会话上较旧的后台持久化快照失效。
///
/// # Parameters
/// 参数
/// `state` owns the per-session persistence version table.
/// `state` 持有按会话归属的持久化版本表。
/// `session_id` selects the session whose pending background writes should be invalidated.
/// `session_id` 选择需要使待处理后台写入失效的会话。
fn invalidate_webui_session_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    let _ = next_webui_session_persist_version(state, session_id)?;
    Ok(())
}

/// Persists one WebUI session runtime-state mutation through the full session snapshot path.
/// 通过完整会话快照路径持久化一个 WebUI 会话运行态变更。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `session_id` selects the runtime-state row to update.
/// `session_id` 选择要更新的运行态记录。
/// `workspace_snapshot` carries the current complete session snapshot.
/// `workspace_snapshot` 携带当前完整会话快照。
///
/// # Returns
/// 返回值
/// `Ok(())` after the store write finishes.
/// 存储写入完成后返回 `Ok(())`。
async fn persist_webui_session_runtime_state_update(
    state: &SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Result<(), String> {
    persist_workspace_session_update(
        Some(state),
        controller_runtime,
        workspace_snapshot,
        session_id,
    )
    .await
}

/// Builds an empty LuaSkills catalog for degraded snapshot rendering.
/// 为降级快照渲染构建空 LuaSkills 目录。
fn empty_luaskills_catalog() -> TauriLuaSkillsCatalog {
    TauriLuaSkillsCatalog {
        runtime_root: PathBuf::new(),
        skills: Vec::new(),
        entry_count: 0,
    }
}

/// Builds an empty MCP catalog for degraded snapshot rendering.
/// 为降级快照渲染构建空 MCP 目录。
fn empty_mcp_catalog() -> TauriMcpRuntimeCatalog {
    TauriMcpRuntimeCatalog {
        configured_server_count: 0,
        enabled_server_count: 0,
        connected_server_count: 0,
        tool_count: 0,
        prompt_count: 0,
        resource_count: 0,
        summary_rows: Vec::new(),
    }
}

/// Parses a user-facing optional session identifier and ignores local draft identifiers.
/// 解析用户界面传入的可选会话标识，并忽略本地草稿标识。
fn parse_session_id(value: &str) -> Option<SessionId> {
    value.parse::<SessionId>().ok()
}

/// Converts the composer reasoning string into the shared provider-config enum.
/// 将输入区思考档位字符串转换为共享 provider-config 枚举。
fn parse_reasoning_effort(value: &str) -> Option<ReasoningEffortLevel> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Some(ReasoningEffortLevel::Off),
        "low" => Some(ReasoningEffortLevel::Low),
        "medium" => Some(ReasoningEffortLevel::Medium),
        "high" => Some(ReasoningEffortLevel::High),
        "x-high" | "max" => Some(ReasoningEffortLevel::XHigh),
        _ => None,
    }
}

/// Converts one shared reasoning-effort enum into the WebUI wire value.
/// 将单个共享思考档位枚举转换为 WebUI 传输值。
fn reasoning_effort_wire_value(effort: ReasoningEffortLevel) -> &'static str {
    match effort {
        ReasoningEffortLevel::Off => "off",
        ReasoningEffortLevel::Low => "low",
        ReasoningEffortLevel::Medium => "medium",
        ReasoningEffortLevel::High => "high",
        ReasoningEffortLevel::XHigh => "x-high",
    }
}

/// Resolves the effective reasoning effort for the active model using the shared selector view.
/// 使用共享选择器视图解析当前激活模型的生效思考档位。
fn active_reasoning_selection_effort(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ReasoningEffortLevel> {
    let preferred_effort = provider_configuration
        .workbench_reasoning_effort
        .unwrap_or(ReasoningEffortLevel::Medium);
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let active_model =
        provider_config::effective_model_choices(provider_id, provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    Some(
        definition
            .reasoning_selection_view(active_model.as_ref(), preferred_effort)
            .selected_effort,
    )
}

/// Builds the WebUI reasoning selector snapshot from the shared provider configuration.
/// 从共享 Provider 配置构建 WebUI 思考选择器快照。
fn reasoning_selection(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ReasoningSelectionSummary> {
    let preferred_effort = provider_configuration
        .workbench_reasoning_effort
        .unwrap_or(ReasoningEffortLevel::Medium);
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let active_model =
        provider_config::effective_model_choices(provider_id, provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    let view = definition.reasoning_selection_view(active_model.as_ref(), preferred_effort);
    Some(ReasoningSelectionSummary {
        options: view
            .options
            .into_iter()
            .map(|option| ReasoningSelectionOptionSummary {
                value: reasoning_effort_wire_value(option.effort).to_string(),
                label: option.label,
            })
            .collect(),
        selected_effort: reasoning_effort_wire_value(view.selected_effort).to_string(),
        note: view.note,
    })
}

/// Creates a readable generated session title from the submitted prompt.
/// 根据提交提示词创建可读的自动会话标题。
fn prompt_to_session_title(prompt: &str) -> String {
    let title = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("新会话");
    title.chars().take(48).collect()
}

/// Stores and emits one rebuilt workbench snapshot after a workspace mutation.
/// 在工作区发生变更后存储并发出一份重建后的工作台快照。
fn store_and_emit_workbench_snapshot(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, SharedServiceBridgeState>,
    controller_runtime: &ControllerRuntimeHandle,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    workspace_snapshot: WorkspaceSnapshot,
    logs: &[String],
) -> Result<WorkbenchSnapshot, String> {
    let (live_context_budgets, surface_messages, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            guard.live_context_budgets.clone(),
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
        )
    };
    let workbench = workbench_from_runtime(
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        logs,
        &live_context_budgets,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.workbench = workbench.clone();
    }
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
}

/// Continues one WebUI prompt turn after the shared conversation precheck completes.
/// 在共享会话预检查完成后继续一个 WebUI 提示词回合。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained WebUI workspace snapshot.
/// `state` 持有 WebUI 保留的工作区快照。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` provides provider and hook runtime state.
/// `agent_runtime` 提供 Provider 与 hook 运行态。
/// `start` is the first-stage prompt turn prepared before precheck.
/// `start` 是 precheck 前准备好的第一阶段提示词回合。
/// `precheck_outcome` is the completed hook result.
/// `precheck_outcome` 是已完成的 hook 结果。
///
/// # Returns
/// 返回值
/// The execution request when the turn should continue, otherwise `None`.
/// 需要继续执行时返回执行请求，否则返回 `None`。
fn continue_webui_prompt_turn_after_precheck_from_state(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    precheck_outcome: ConversationPrecheckOutcome,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    let session_id = start.session_id;
    let mut workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    if workspace_snapshot.session_run_phases.get(&session_id) == Some(&SessionRunPhase::Stopped) {
        spawn_workspace_session_persist_task(
            state.clone(),
            controller_runtime.clone(),
            workspace_snapshot,
            session_id,
            Some(String::from("precheck_stopped")),
        );
        return Ok(None);
    }
    let ai_tool_configuration = effective_ai_tool_configuration_for_prompt_overrides(
        &agent_runtime.ai_tool_configuration,
        &start.disabled_ai_capability_kinds,
        &start.disabled_ai_capability_target_selectors,
        &start.ai_capability_target_order_overrides,
    );
    let project_configuration = load_project_configuration(&start.workspace_path);
    let runtime_reasoning_effort = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_reasoning_effort);
    let session_title_for_context = start.session_title.clone();
    let continuation = continue_shared_prompt_turn_after_precheck(
        &mut workspace_snapshot,
        SharedPromptTurnContinueInput {
            start,
            precheck_outcome,
            ai_tool_configuration,
            project_configuration,
            runtime_reasoning_effort,
            provider_reasoning_effort: active_reasoning_selection_effort(
                &agent_runtime.provider_configuration,
            ),
            patch_visible_user_prompt_record: true,
        },
    )?;
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    if let Some(detail) = continuation.precheck_failure_detail.as_deref() {
        eprintln!("[tauri-service] precheck hook failed but request will continue: {detail}");
        if let Err(error) = push_webui_service_diagnostic(
            state,
            format!("warning · Precheck hook 执行失败：{detail}"),
        ) {
            eprintln!("[tauri-service] record precheck warning failed: {error}");
        }
    }
    let request = continuation.request;
    persist_webui_context_current_document(
        &workspace_snapshot,
        session_id,
        &session_title_for_context,
    );
    let now = current_unix_millis_text();
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.updated_at = now.clone();
    }
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Running;
        session.focus = String::from("正在请求");
        session.last_updated_label = String::from("刚刚");
    }
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        agent_runtime,
        workspace_snapshot.clone(),
    )?;
    spawn_workspace_session_persist_task(
        state.clone(),
        controller_runtime.clone(),
        workspace_snapshot,
        session_id,
        Some(String::from("precheck_continue")),
    );
    Ok(Some(request))
}

/// Spawns the shared precheck continuation chain for one WebUI prompt turn.
/// 为一个 WebUI 提示词回合启动共享 precheck continuation 链路。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and is forwarded to the stream task.
/// `app` 发出工作台快照，并继续传递给流任务。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` owns provider and hook runtimes.
/// `agent_runtime` 持有 Provider 与 hook 运行时。
/// `start` is the first-stage prompt turn.
/// `start` 是第一阶段提示词回合。
fn spawn_webui_precheck_continue_task(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    start: SharedPromptTurnStart,
) {
    tauri::async_runtime::spawn(async move {
        let session_id = start.session_id;
        let precheck_outcome = start.precheck_request.clone().run().await;
        match continue_webui_prompt_turn_after_precheck_from_state(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            start,
            precheck_outcome,
        ) {
            Ok(Some(request)) => {
                spawn_agent_stream_task(app, state, controller_runtime, agent_runtime, request)
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("[tauri-service] continue WebUI prompt after precheck failed: {error}");
                if let Err(cleanup_error) = mark_webui_precheck_continuation_failed(
                    &app,
                    &state,
                    &controller_runtime,
                    &agent_runtime,
                    session_id,
                    &error,
                ) {
                    eprintln!(
                        "[tauri-service] cleanup failed WebUI precheck continuation failed: {cleanup_error}"
                    );
                }
            }
        }
    });
}

/// Starts precheck dispatch or immediately continues with a skipped precheck using shared semantics.
/// 使用共享语义启动 precheck 调度，或在需要跳过时立即继续。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and runtime stream events.
/// `app` 发出工作台快照与运行时流事件。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` owns provider and hook runtimes.
/// `agent_runtime` 持有 Provider 与 hook 运行时。
/// `start` is the first-stage prompt turn.
/// `start` 是第一阶段提示词回合。
/// `precheck_plan` decides whether handlers should run.
/// `precheck_plan` 决定是否需要运行处理器。
///
/// # Returns
/// 返回值
/// Whether any continuation or precheck task was started.
/// 是否已经启动 continuation 或 precheck 任务。
fn start_webui_precheck_or_continue(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    precheck_plan: SharedPromptPrecheckPlan,
) -> Result<bool, String> {
    if precheck_plan.should_skip_precheck || !precheck_plan.has_precheck_hooks {
        let precheck_outcome = start.precheck_request.clone().skip();
        if let Some(request) = continue_webui_prompt_turn_after_precheck_from_state(
            app,
            state,
            controller_runtime,
            agent_runtime,
            start,
            precheck_outcome,
        )? {
            spawn_agent_stream_task(
                app.clone(),
                state.clone(),
                controller_runtime.clone(),
                agent_runtime.clone(),
                request,
            );
            return Ok(true);
        }
        return Ok(false);
    }
    spawn_webui_precheck_continue_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        agent_runtime.clone(),
        start,
    );
    Ok(true)
}

/// Marks a WebUI prompt turn as stopped when precheck continuation cannot be applied.
/// 当 WebUI 提示词回合无法应用 precheck continuation 时，将其标记为已停止。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained workspace snapshot.
/// `state` 持有保留的工作区快照。
/// `controller_runtime` persists the cleaned session state.
/// `controller_runtime` 持久化清理后的会话状态。
/// `agent_runtime` provides projection configuration for the emitted workbench.
/// `agent_runtime` 提供发出工作台快照所需的投影配置。
/// `session_id` selects the failed prompt turn.
/// `session_id` 选择失败的提示词回合。
/// `error` describes the continuation failure.
/// `error` 描述 continuation 失败原因。
///
/// # Returns
/// 返回值
/// `Ok(())` after the failed state is projected and persisted.
/// 失败状态完成投影与持久化后返回 `Ok(())`。
fn mark_webui_precheck_continuation_failed(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    error: &str,
) -> Result<(), String> {
    let mut workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    let now = current_unix_millis_text();
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.active_precheck_turn_id = None;
        append_runtime_event_projection_to_state(
            runtime_state,
            session_id,
            &now,
            &RuntimeEvent::new(
                RuntimeEventKind::Failed,
                "Precheck 结果已忽略",
                error.to_string(),
            ),
        );
        runtime_state.updated_at = now.clone();
    }
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("Precheck 结果无法应用，已停止本轮请求。");
        session.last_updated_label = String::from("刚刚");
    }
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        agent_runtime,
        workspace_snapshot.clone(),
    )?;
    spawn_workspace_session_persist_task(
        state.clone(),
        controller_runtime.clone(),
        workspace_snapshot,
        session_id,
        Some(String::from("precheck_continue_failed")),
    );
    Ok(())
}

/// Spawns one background task that forwards agent runtime stream events into the WebUI state.
/// 启动一个后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
fn spawn_agent_stream_task(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
) {
    let session_id = request.session_id;
    let task_state = state.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let mut record_ids = SessionStreamingRecordIds::default();
        let mut reached_finished_event = false;
        let prompt_intervention = build_webui_prompt_intervention(
            app.clone(),
            task_state.clone(),
            agent_runtime.provider_configuration.clone(),
            agent_runtime.ai_tool_configuration.clone(),
        );
        let mut stream = Box::pin(stream_runtime_run_with_intervention(
            agent_runtime.provider_configuration.clone(),
            agent_runtime.agent_profile_catalog.clone(),
            agent_runtime.luaskills_runtime.clone(),
            agent_runtime.mcp_runtime.clone(),
            Arc::clone(&agent_runtime.task_runtime),
            Arc::clone(&agent_runtime.question_runtime),
            Arc::clone(&agent_runtime.command_session_runtime),
            Arc::clone(&agent_runtime.token_budget_runtime),
            request,
            Some(prompt_intervention),
        ));

        while let Some(event) = stream.next().await {
            match event {
                ExecutionStreamEvent::Prepared(preparation) => {
                    if let Err(error) = apply_execution_preparation(&app, &task_state, &preparation)
                    {
                        eprintln!("[tauri-service] apply preparation failed: {error}");
                    }
                }
                ExecutionStreamEvent::RequestStarted { session_id } => {
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[tauri-service] apply request-started failed: {error}");
                    }
                }
                ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Thinking,
                    ) {
                        eprintln!("[tauri-service] apply thinking phase failed: {error}");
                    }
                    if let Err(error) = append_stream_delta_record(
                        &app,
                        &task_state,
                        session_id,
                        &mut record_ids.thinking_record_id,
                        SessionMessageRecordKind::ThinkingOutput,
                        "思考",
                        &delta,
                        true,
                    ) {
                        eprintln!("[tauri-service] append thinking delta failed: {error}");
                    }
                }
                ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                    if let Err(error) =
                        collapse_streaming_thinking(&app, &task_state, session_id, &record_ids)
                    {
                        eprintln!("[tauri-service] complete thinking failed: {error}");
                    }
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[tauri-service] restore request phase failed: {error}");
                    }
                }
                ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[tauri-service] apply output phase failed: {error}");
                    }
                    if let Err(error) = append_stream_delta_record(
                        &app,
                        &task_state,
                        session_id,
                        &mut record_ids.output_record_id,
                        SessionMessageRecordKind::AssistantOutput,
                        "输出",
                        &delta,
                        false,
                    ) {
                        eprintln!("[tauri-service] append output delta failed: {error}");
                    }
                }
                ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                    if is_provider_tool_boundary_runtime_event(&event) {
                        record_ids.thinking_record_id = None;
                        record_ids.output_record_id = None;
                    }
                    if matches!(event.kind, RuntimeEventKind::ToolStarted) {
                        if let Err(error) = apply_session_phase(
                            &app,
                            &task_state,
                            session_id,
                            SessionRunPhase::ToolRunning,
                        ) {
                            eprintln!("[tauri-service] apply tool phase failed: {error}");
                        }
                    }
                    if matches!(event.kind, RuntimeEventKind::ToolFinished) {
                        if let Err(error) = apply_session_phase(
                            &app,
                            &task_state,
                            session_id,
                            SessionRunPhase::Requesting,
                        ) {
                            eprintln!(
                                "[tauri-service] restore request phase after tool failed: {error}"
                            );
                        }
                    }
                    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
                        hook_runtime.record_tool_runtime_event(session_id, &event);
                    }
                    if let Err(error) = maybe_start_webui_detached_subagent_task_listener(
                        app.clone(),
                        task_state.clone(),
                        controller_runtime.clone(),
                        agent_runtime.clone(),
                        session_id,
                        &event,
                    ) {
                        eprintln!(
                            "[tauri-service] start detached subagent listener failed: {error}"
                        );
                    }
                    let subagent_event_consumed = match ingest_webui_subagent_runtime_event(
                        &app,
                        &task_state,
                        session_id,
                        &event,
                    ) {
                        Ok(consumed) => consumed,
                        Err(error) => {
                            eprintln!("[tauri-service] ingest subagent event failed: {error}");
                            false
                        }
                    };
                    if !subagent_event_consumed
                        && let Err(error) =
                            append_runtime_event_projection(&app, &task_state, session_id, &event)
                    {
                        eprintln!("[tauri-service] append runtime event failed: {error}");
                    }
                    match maybe_prepare_next_webui_pending_prompt(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        session_id,
                        false,
                    )
                    .await
                    {
                        Ok(_) => {}
                        Err(error) => {
                            eprintln!(
                                "[tauri-service] prepare pending prompt after runtime event failed: {error}"
                            );
                        }
                    }
                }
                ExecutionStreamEvent::ConversationCheckpoint {
                    session_id,
                    conversation_state,
                    last_successful_provider_usage,
                    provider_request_count,
                } => {
                    if let Err(error) = apply_conversation_checkpoint(
                        &app,
                        &task_state,
                        &controller_runtime,
                        session_id,
                        conversation_state,
                        last_successful_provider_usage,
                        provider_request_count,
                    ) {
                        eprintln!("[tauri-service] apply checkpoint failed: {error}");
                    }
                }
                ExecutionStreamEvent::Finished(report) => {
                    reached_finished_event = true;
                    let finished_session_id = report.session_id;
                    if let Ok(mut guard) = task_state.lock() {
                        guard.active_stream_tasks.remove(&finished_session_id);
                    }
                    let final_report_apply = match apply_execution_report_to_memory(
                        &app,
                        &task_state,
                        &agent_runtime,
                        report,
                    ) {
                        Ok(apply) => Some(apply),
                        Err(error) => {
                            eprintln!("[tauri-service] apply final report failed: {error}");
                            None
                        }
                    };
                    let started_next_prompt = match maybe_prepare_next_webui_pending_prompt(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        finished_session_id,
                        false,
                    )
                    .await
                    {
                        Ok(started) => started,
                        Err(error) => {
                            eprintln!(
                                "[tauri-service] prepare next pending prompt failed: {error}"
                            );
                            false
                        }
                    };
                    if let Some(completion_request) = final_report_apply
                        .as_ref()
                        .and_then(|apply| apply.completion_request.clone())
                    {
                        spawn_conversation_completion_hook_task(
                            app.clone(),
                            task_state.clone(),
                            controller_runtime.clone(),
                            agent_runtime.clone(),
                            completion_request,
                        );
                    }
                    if !started_next_prompt
                        && let Some(apply) = final_report_apply
                        && let Err(error) = persist_workspace_session_update(
                            Some(&task_state),
                            controller_runtime.clone(),
                            &apply.workspace_snapshot,
                            finished_session_id,
                        )
                        .await
                    {
                        eprintln!("[tauri-service] persist final report failed: {error}");
                    }
                    break;
                }
            }
        }
        if !reached_finished_event && let Ok(mut guard) = task_state.lock() {
            guard.active_stream_tasks.remove(&session_id);
        }
    });
    if let Ok(mut guard) = state.lock()
        && let Some(previous_task) = guard.active_stream_tasks.insert(session_id, handle)
    {
        previous_task.abort();
    }
}

/// Starts one detached child-task listener using the same task runtime stream as iced.
/// 使用与 iced 相同的任务运行时流启动单个分离子任务监听器。
fn maybe_start_webui_detached_subagent_task_listener(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    if event.kind != RuntimeEventKind::DelegateStarted {
        return Ok(());
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(());
    };
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(());
    };
    let task_id = panel_state.task_id;
    let should_start = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.detached_subagent_task_listeners.insert(task_id)
    };
    if !should_start {
        return Ok(());
    }
    tauri::async_runtime::spawn(async move {
        let mut record_ids = SessionStreamingRecordIds::default();
        let mut stream = Box::pin(
            agent_runtime
                .task_runtime
                .subscribe_detached_task_events(task_id),
        );
        while let Some(event) = stream.next().await {
            if let Err(error) = handle_webui_detached_subagent_task_event(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                task_id,
                &mut record_ids,
                event,
            )
            .await
            {
                eprintln!("[tauri-service] detached subagent event failed: {error}");
            }
        }
    });
    Ok(())
}

/// Applies one detached subagent runtime event to the shared WebUI transcript projection.
/// 将单条分离子代理运行时事件应用到共享 WebUI 转录投影。
async fn handle_webui_detached_subagent_task_event(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    task_id: SessionId,
    record_ids: &mut SessionStreamingRecordIds,
    event: DetachedTaskRuntimeEvent,
) -> Result<(), String> {
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(event) => {
            handle_webui_detached_child_execution_event(
                app,
                state,
                controller_runtime,
                agent_runtime,
                record_ids,
                event,
            )
            .await
        }
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id,
            event,
        } => {
            let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
            let consumed =
                ingest_webui_subagent_runtime_event(app, state, parent_session_id, &event)?;
            if !consumed {
                append_runtime_event_projection(app, state, parent_session_id, &event)?;
            }
            if is_finished {
                {
                    let mut guard = state
                        .lock()
                        .map_err(|error| format!("failed to lock service state: {error}"))?;
                    guard.detached_subagent_task_listeners.remove(&task_id);
                }
                if let Some(workspace_snapshot) =
                    current_workspace_snapshot_from_shared_state(state)?
                {
                    spawn_workspace_session_persist_task(
                        state.clone(),
                        controller_runtime.clone(),
                        workspace_snapshot,
                        parent_session_id,
                        Some(String::from("detached_subagent_parent_event")),
                    );
                }
            }
            maybe_prepare_next_webui_pending_prompt(
                app,
                state,
                controller_runtime,
                agent_runtime,
                parent_session_id,
                false,
            )
            .await?;
            Ok(())
        }
    }
}

/// Applies one child execution stream event using the same projection functions as root WebUI turns.
/// 使用与根 WebUI 回合相同的投影函数应用单条子执行流事件。
async fn handle_webui_detached_child_execution_event(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    record_ids: &mut SessionStreamingRecordIds,
    event: ExecutionStreamEvent,
) -> Result<(), String> {
    match event {
        ExecutionStreamEvent::Prepared(_) => Ok(()),
        ExecutionStreamEvent::RequestStarted { session_id } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Thinking)?;
            append_stream_delta_record(
                app,
                state,
                session_id,
                &mut record_ids.thinking_record_id,
                SessionMessageRecordKind::ThinkingOutput,
                "思考",
                &delta,
                true,
            )
        }
        ExecutionStreamEvent::ThinkingCompleted { session_id } => {
            collapse_streaming_thinking(app, state, session_id, record_ids)?;
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::OutputDelta { session_id, delta } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)?;
            append_stream_delta_record(
                app,
                state,
                session_id,
                &mut record_ids.output_record_id,
                SessionMessageRecordKind::AssistantOutput,
                "输出",
                &delta,
                false,
            )
        }
        ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
            if is_provider_tool_boundary_runtime_event(&event) {
                record_ids.thinking_record_id = None;
                record_ids.output_record_id = None;
            }
            append_runtime_event_projection(app, state, session_id, &event)
        }
        ExecutionStreamEvent::ConversationCheckpoint {
            session_id,
            conversation_state,
            last_successful_provider_usage,
            provider_request_count,
        } => apply_conversation_checkpoint(
            app,
            state,
            controller_runtime,
            session_id,
            conversation_state,
            last_successful_provider_usage,
            provider_request_count,
        ),
        ExecutionStreamEvent::Finished(report) => {
            apply_execution_report_to_memory(app, state, agent_runtime, report).map(|_| ())
        }
    }
}

/// Builds the WebUI runtime intervention that injects guided prompts before the next provider request.
/// 构建 WebUI 运行时干预器，用于在下一次 Provider 请求前注入引导提示词。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit refreshed workbench snapshots.
/// `app` 是用于发出刷新后工作台快照的 Tauri 应用句柄。
/// `state` is the shared service bridge state that owns the active workspace snapshot.
/// `state` 是拥有当前工作区快照的共享服务桥接状态。
/// `provider_configuration` is the provider configuration used by the shared pending router.
/// `provider_configuration` 是共享待发送路由器使用的 Provider 配置。
///
/// # Returns
/// 返回值
/// A callback consumed by the host-controlled runtime stream.
/// 返回供宿主控制运行时流消费的回调。
fn build_webui_prompt_intervention(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    provider_configuration: ProviderConfigurationFile,
    ai_tool_configuration: PersistedAiToolConfiguration,
) -> RuntimePromptIntervention {
    Arc::new(move |session_id, controller: &mut VulcanCoreController| {
        match take_webui_internal_guidance_prompts(
            &app,
            &state,
            &provider_configuration,
            session_id,
        ) {
            Ok(entries) => {
                entries
                    .iter()
                    .filter_map(|entry| {
                        let (prompt, attachments) = normalize_webui_interjected_prompt_for_provider(
                            &provider_configuration,
                            &ai_tool_configuration,
                            entry,
                        );
                        controller.append_interjected_user_turn_with_attachments(
                            prompt.as_str(),
                            attachments,
                        )
                    })
                    .count()
                    > 0
            }
            Err(error) => {
                eprintln!("[tauri-service] apply guided prompt intervention failed: {error}");
                false
            }
        }
    })
}

/// Normalizes one dynamically interjected WebUI prompt through the same provider/model capability gate used by request bootstrap.
/// 通过与请求 bootstrap 相同的 Provider/模型能力门禁归一化一条 WebUI 动态引导提示词。
///
/// # Parameters
/// 参数
/// `provider_configuration` is the active provider configuration captured by the running stream.
/// `provider_configuration` 是当前运行流捕获的活跃 Provider 配置。
/// `ai_tool_configuration` is the active AI tool routing configuration captured by the running stream.
/// `ai_tool_configuration` 是当前运行流捕获的活跃 AI 工具路由配置。
/// `entry` is the internal queued prompt selected for runtime guidance.
/// `entry` 是被选中用于运行时引导的内部排队提示词。
///
/// # Returns
/// 返回值
/// Provider-safe prompt text and attachments after unsupported modalities have been downgraded.
/// 返回不支持模态被降级后的 Provider 安全提示词文本与附件集合。
fn normalize_webui_interjected_prompt_for_provider(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    entry: &PendingSessionPromptEntry,
) -> (String, Vec<ConversationAttachment>) {
    let scoped_provider_configuration = provider_configuration_for_model_reference(
        provider_configuration,
        entry.model_reference.as_ref(),
    );
    let Some((provider_id, model_id)) = scoped_provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
    else {
        return (entry.prompt.clone(), entry.attachments.clone());
    };
    let Some(definition) = provider_config::supported_provider_by_id(provider_id) else {
        return (entry.prompt.clone(), entry.attachments.clone());
    };
    let effective_ai_tool_configuration = effective_ai_tool_configuration_for_prompt_overrides(
        ai_tool_configuration,
        &entry.runtime_options.disabled_ai_capability_kinds,
        &BTreeSet::new(),
        &BTreeMap::new(),
    );
    let active_model =
        provider_config::effective_model_choices(provider_id, &scoped_provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    let capabilities = resolve_effective_model_capabilities(
        definition,
        active_model.as_ref(),
        &effective_ai_tool_configuration,
    );
    let api_family = definition
        .effective_runtime_for_model(model_id)
        .api_family();
    let normalized = normalize_provider_user_input_for_model(
        api_family,
        Some(&capabilities),
        &entry.prompt,
        &entry.attachments,
    );
    (normalized.text, normalized.attachments)
}

/// Takes already-guided WebUI prompts from the shared internal queue.
/// 从共享内部队列取出已经引导的 WebUI 提示词集合。
///
/// # Parameters
/// 参数
/// `app` is used by the workspace mutation helper to emit the updated queue view.
/// `app` 由工作区变更辅助函数用于发出更新后的队列视图。
/// `state` contains the current workspace snapshot and live runtime handles.
/// `state` 包含当前工作区快照与实时运行时句柄。
/// `provider_configuration` lets the shared router preserve iced Anthropic dispatch preference.
/// `provider_configuration` 用于让共享路由器保留 iced 的 Anthropic 分发优先级。
/// `session_id` identifies the session whose stream reached a safe tool boundary.
/// `session_id` 标识流已经到达安全工具边界的会话。
///
/// # Returns
/// 返回值
/// Guided prompt entries waiting in the internal queue, preserving queue order.
/// 按队列顺序返回内部队列中等待的引导条目集合。
fn take_webui_internal_guidance_prompts(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
) -> Result<Vec<PendingSessionPromptEntry>, String> {
    let mut guided_entries = Vec::new();
    with_workspace_mutation(app, state, |workspace| {
        let prefer_delegate_completion =
            session_uses_anthropic_runtime(workspace, provider_configuration, session_id);
        let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) else {
            return;
        };
        guided_entries = take_guided_pending_prompt_entries(
            &mut runtime_state.ui_state,
            prefer_delegate_completion,
        );
        if guided_entries.is_empty() {
            return;
        }
        runtime_state.updated_at = current_unix_millis_text();
    })?;
    Ok(guided_entries)
}

/// Prepares the next visible queued prompt after one WebUI-owned session run reaches a clean idle point.
/// 在 WebUI 拥有的单次会话运行到达干净空闲点后准备下一条可见排队提示词。
///
/// `allow_from_stopped_phase` lets manual insert match iced while automatic continuation still respects stop.
/// `allow_from_stopped_phase` 让手动插入对齐 iced，同时自动续跑仍尊重停止状态。
async fn maybe_prepare_next_webui_pending_prompt(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    allow_from_stopped_phase: bool,
) -> Result<bool, String> {
    let (mut workspace_snapshot, has_active_stream_task, has_running_subagents) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard
            .workspace_snapshot
            .clone()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let has_running_subagents = session_has_running_subagent_tasks_from_state(
            &mut guard,
            &workspace_snapshot,
            session_id,
        );
        (
            workspace_snapshot,
            guard.active_stream_tasks.contains_key(&session_id),
            has_running_subagents,
        )
    };
    let prefer_delegate_completion = session_uses_anthropic_runtime(
        &workspace_snapshot,
        &agent_runtime.provider_configuration,
        session_id,
    );
    let Some(next_source) = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| {
            next_pending_prompt_dispatch_source(&runtime_state.ui_state, prefer_delegate_completion)
        })
    else {
        return Ok(false);
    };
    if pending_prompt_dispatch_block_reason_for_source(
        webui_pending_prompt_dispatch_context(
            &workspace_snapshot,
            &agent_runtime.provider_configuration,
            session_id,
            has_active_stream_task,
            has_running_subagents,
        ),
        allow_from_stopped_phase,
        Some(next_source),
    )
    .is_some()
    {
        return Ok(false);
    }
    let Some(session_title) = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .map(|session| session.title.clone())
    else {
        return Ok(false);
    };
    let Some(entry) = take_next_webui_pending_prompt(
        &mut workspace_snapshot,
        session_id,
        prefer_delegate_completion,
    ) else {
        return Ok(false);
    };
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.session_subagent_navigation_rounds.remove(&session_id);
        if guard
            .active_subagent_panel
            .is_some_and(|selection| selection.parent_session_id == session_id)
        {
            guard.active_subagent_panel = None;
        }
    }
    let workspace_path = workspace_path_for_session(&workspace_snapshot, session_id);
    let runtime_options = entry.runtime_options.clone();
    let entry_agent_runtime = agent_runtime_for_pending_prompt(agent_runtime, &entry);
    let panel_entry_id = entry.panel_entry_id.clone();
    let disabled_ai_capability_kinds = runtime_options
        .disabled_ai_capability_kinds
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    snapshot_webui_context_before_user_prompt(&workspace_snapshot, session_id, entry.source)?;
    let precheck_start = {
        let mut hook_runtime = entry_agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            SharedPromptTurnBeginInput {
                session_id,
                session_title: &session_title,
                workspace_path: &workspace_path,
                prompt_entry_id: entry.id.clone(),
                prompt: &entry.prompt,
                attachments: entry.attachments,
                append_canonical_user_message: true,
                append_visible_user_prompt_record: entry.source
                    != PendingSessionPromptSource::DelegateCompletion,
                source: entry.source,
                execution_mode: String::from("build"),
                reasoning_effort: Some(runtime_options.reasoning_effort),
                disabled_ai_capability_kinds,
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_order_overrides: BTreeMap::new(),
            },
        )
    }?;
    apply_webui_prompt_begin_outer_steps(&mut workspace_snapshot, session_id, &session_title);
    entry_agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &entry_agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    let precheck_plan = webui_shared_prompt_precheck_plan(
        &entry_agent_runtime,
        &workspace_snapshot,
        session_id,
        &workspace_path,
    )?;
    if let Some(panel_entry_id) = panel_entry_id.as_deref()
        && let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
    {
        let _ = remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, panel_entry_id);
        runtime_state.updated_at = current_unix_millis_text();
    }
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        &entry_agent_runtime,
        workspace_snapshot.clone(),
    )?;
    spawn_workspace_session_runtime_state_persist_task(
        state.clone(),
        controller_runtime.clone(),
        workspace_snapshot,
        session_id,
        Some(String::from("queued_begin")),
    );
    start_webui_precheck_or_continue(
        app,
        state,
        controller_runtime,
        &entry_agent_runtime,
        precheck_start,
        precheck_plan,
    )?;
    Ok(true)
}

/// Removes the next dispatchable queued prompt from the internal or visible WebUI queues.
/// 从 WebUI 内部或可见队列中移除下一条可分发排队提示词。
fn take_next_webui_pending_prompt(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    prefer_delegate_completion: bool,
) -> Option<PendingSessionPromptEntry> {
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)?;
    let created_at = current_unix_millis_text();
    let internal_entry_id = format!("webui-pending-internal-{}", SessionId::new_v4());
    let entry = take_next_pending_prompt_entry(
        &mut runtime_state.ui_state,
        prefer_delegate_completion,
        internal_entry_id,
        created_at,
    )?;
    runtime_state.updated_at = current_unix_millis_text();
    Some(entry)
}

/// Rebuilds and emits the retained workbench snapshot after a service-side queue mutation.
/// 在 service 侧队列变更后重建并发出保留的工作台快照。
fn store_current_workbench_snapshot(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    workspace_snapshot: WorkspaceSnapshot,
) -> Result<WorkbenchSnapshot, String> {
    let (luaskills_catalog, token_budget_runtime, mcp_catalog, logs) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let luaskills_catalog = guard
            .luaskills_runtime
            .as_ref()
            .and_then(|handle| crate::luaskills_bridge::load_luaskills_catalog(handle).ok())
            .unwrap_or_else(empty_luaskills_catalog);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = guard
            .mcp_runtime
            .as_ref()
            .map(TauriMcpRuntimeHandle::catalog)
            .unwrap_or_else(empty_mcp_catalog);
        (
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            guard.startup.recent_logs.clone(),
        )
    };
    let (live_context_budgets, surface_messages, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            guard.live_context_budgets.clone(),
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
        )
    };
    let workbench = workbench_from_runtime(
        controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        &logs,
        &live_context_budgets,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.workbench = workbench.clone();
    }
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
}

/// Applies execution preparation identity fields to the target session runtime state.
/// 将执行准备得到的身份字段应用到目标会话运行时状态。
fn apply_execution_preparation(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    preparation: &ExecutionPreparation,
) -> Result<(), String> {
    if let Some(context_budget) = preparation.context_budget.clone() {
        state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .live_context_budgets
            .insert(preparation.session_id, context_budget);
    }
    with_workspace_mutation(app, state, |workspace| {
        let runtime_state = workspace
            .session_runtime_state
            .entry(preparation.session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            });
        runtime_state.active_agent_id = Some(preparation.agent_id.clone());
        runtime_state.active_provider_id = Some(preparation.model_blueprint.provider_id.clone());
        runtime_state.active_model_id = Some(preparation.model_blueprint.model_id.clone());
        runtime_state.active_openrouter_endpoint_route = preparation
            .model_blueprint
            .openrouter_endpoint_route
            .clone();
        runtime_state.active_reasoning_effort =
            Some(preparation.model_blueprint.reasoning.requested_effort);
        runtime_state.active_temperature = preparation.temperature;
        runtime_state.live_context_budget_fingerprint = preparation
            .context_budget
            .as_ref()
            .map(|budget| budget.fingerprint.clone());
        runtime_state.live_context_assembly_fingerprint = preparation
            .context_baseline
            .as_ref()
            .map(|baseline| baseline.fingerprint.clone());
        runtime_state.updated_at = current_unix_millis_text();
        workspace
            .session_run_phases
            .insert(preparation.session_id, SessionRunPhase::Preparing);
    })
    .map(|_| ())
}

/// Updates one session phase in the retained workspace snapshot.
/// 更新保留工作区快照中的单个会话阶段。
fn apply_session_phase(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    phase: SessionRunPhase,
) -> Result<(), String> {
    with_workspace_mutation(app, state, |workspace| {
        workspace.session_run_phases.insert(session_id, phase);
    })
    .map(|_| ())
}

/// Appends or extends one streaming transcript record.
/// 追加或扩展单条流式转录记录。
fn append_stream_delta_record(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_id_slot: &mut Option<String>,
    record_kind: SessionMessageRecordKind,
    title: &str,
    delta: &str,
    collapsed_by_default: bool,
) -> Result<(), String> {
    if delta.is_empty() {
        return Ok(());
    }
    with_workspace_mutation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        append_stream_delta_record_to_state(
            runtime_state,
            session_id,
            &now,
            record_id_slot,
            record_kind,
            title,
            delta,
            collapsed_by_default,
        );
        workspace.session_messages.insert(
            session_id,
            conversation_state_to_session_messages(&runtime_state.conversation_state),
        );
        workspace.session_run_phases.insert(
            session_id,
            match record_kind {
                SessionMessageRecordKind::ThinkingOutput => SessionRunPhase::Thinking,
                _ => SessionRunPhase::Requesting,
            },
        );
        workspace.hydrated_session_ids.insert(session_id);
    })
    .map(|_| ())
}

/// Keeps the thinking row collapsed after its streaming phase completes.
/// 在思考流阶段完成后保持思考行折叠。
fn collapse_streaming_thinking(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_ids: &SessionStreamingRecordIds,
) -> Result<(), String> {
    with_workspace_mutation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        collapse_streaming_thinking_in_state(runtime_state, &now, record_ids);
    })
    .map(|_| ())
}

/// Projects runtime events that have user-facing transcript cards into the retained session state.
/// 将具备用户可见转录卡片的运行时事件投影到保留的会话状态中。
fn append_runtime_event_projection(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    if !matches!(
        event.kind,
        RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
    ) {
        return Ok(());
    }
    with_workspace_mutation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        if append_runtime_event_projection_to_state(runtime_state, session_id, &now, event) {
            workspace.session_messages.insert(
                session_id,
                conversation_state_to_session_messages(&runtime_state.conversation_state),
            );
            workspace.hydrated_session_ids.insert(session_id);
        }
    })
    .map(|_| ())
}

/// Consumes one delegate runtime event into the WebUI subagent cache and child transcript state.
/// 将一条委派运行时事件吸收到 WebUI 子代理缓存与子会话转录状态中。
fn ingest_webui_subagent_runtime_event(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    if !matches!(
        event.kind,
        RuntimeEventKind::DelegateStarted | RuntimeEventKind::DelegateFinished
    ) {
        return Ok(false);
    }
    let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(true);
    };
    let prompt = parse_subagent_task_prompt_metadata(metadata);
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(true);
    };
    let normalized_state = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let task_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| &runtime.task_runtime);
        let normalized = if is_finished {
            normalize_webui_subagent_task_state(&panel_state, task_runtime, false)
        } else {
            panel_state.clone()
        };
        guard
            .session_subagent_navigation_rounds
            .insert(parent_session_id);
        if is_finished {
            guard
                .detached_subagent_task_listeners
                .remove(&normalized.task_id);
        }
        guard
            .session_subagent_tasks
            .entry(parent_session_id)
            .or_default()
            .insert(normalized.task_id, normalized.clone());
        normalized
    };

    let workspace_snapshot = with_workspace_mutation(app, state, |workspace| {
        if let Some(prompt) = prompt.as_deref() {
            seed_webui_subagent_task_prompt_if_needed(workspace, normalized_state.task_id, prompt);
        }
        if let Some(conversation_state) = normalized_state.conversation_state.clone() {
            replace_webui_subagent_task_conversation_state(
                workspace,
                normalized_state.task_id,
                conversation_state,
            );
        }
        if is_finished {
            enqueue_webui_delegate_completion_followup_if_needed(
                workspace,
                parent_session_id,
                &normalized_state,
            );
        }
    })?;
    if is_finished {
        refresh_webui_subagent_navigation_round_visibility(
            state,
            &workspace_snapshot,
            parent_session_id,
        )?;
    }
    Ok(true)
}

/// Seeds one child task transcript with its initial prompt when no child transcript exists yet.
/// 当子任务尚无转录时，使用初始提示词播种子任务转录。
fn seed_webui_subagent_task_prompt_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
    prompt: &str,
) {
    if prompt.trim().is_empty()
        || workspace_snapshot
            .session_messages
            .get(&task_id)
            .is_some_and(|messages| !messages.is_empty())
    {
        return;
    }
    let now = current_unix_millis_text();
    workspace_snapshot.session_messages.insert(
        task_id,
        vec![
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", prompt)
                .with_runtime_metadata(format!("session-{task_id}-subagent-prompt-{now}"), now),
        ],
    );
    workspace_snapshot.hydrated_session_ids.insert(task_id);
}

/// Appends the canonical stopped notice to a child task transcript when it is missing.
/// 当子任务转录缺少停止提示时追加规范停止提示。
fn append_webui_subagent_stop_notice_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
) {
    let messages = workspace_snapshot
        .session_messages
        .entry(task_id)
        .or_default();
    if messages.iter().any(|message| {
        message.title == "执行已停止" && message.body.trim() == SUBAGENT_TASK_STOPPED_SUMMARY_TEXT
    }) {
        return;
    }
    let now = current_unix_millis_text();
    messages.push(
        SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "执行已停止",
            SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
        )
        .with_runtime_metadata(format!("session-{task_id}-subagent-stopped-{now}"), now),
    );
    workspace_snapshot.hydrated_session_ids.insert(task_id);
}

/// Marks one WebUI subagent task as stopped in the child transcript and parent task card.
/// 在子转录与父级 task 卡片中将单个 WebUI 子代理任务标记为已停止。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot to mutate.
/// `workspace_snapshot` 是需要变更的 WebUI 保留工作区快照。
/// `parent_session_id` owns the visible task card.
/// `parent_session_id` 拥有可见 task 卡片。
/// `task_id` is the child task session identifier.
/// `task_id` 是子任务会话标识。
/// `state_snapshot` is the latest cached subagent state when available.
/// `state_snapshot` 是可用时最近缓存的子代理状态。
fn mark_webui_subagent_task_stopped_in_workspace(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_id: SessionId,
    state_snapshot: Option<&SubagentTaskPanelState>,
) {
    append_webui_subagent_stop_notice_if_needed(workspace_snapshot, task_id);
    let payload = json!({
        "task_id": task_id.to_string(),
        "task_session_id": task_id.to_string(),
    });
    let Some(message_id) = find_webui_task_session_tool_message_id_for_payload(
        workspace_snapshot,
        parent_session_id,
        &payload,
    ) else {
        return;
    };
    let current_metadata = workspace_snapshot
        .session_messages
        .get(&parent_session_id)
        .and_then(|messages| messages.iter().find(|message| message.id == message_id))
        .and_then(|message| message.metadata.as_ref())
        .or_else(|| {
            workspace_snapshot
                .session_runtime_state
                .get(&parent_session_id)
                .and_then(|runtime_state| {
                    runtime_state
                        .conversation_state
                        .records
                        .iter()
                        .find(|record| record.id == message_id)
                        .and_then(|record| record.metadata.as_ref())
                })
        });
    let updated_at = current_unix_millis_text();
    let updated_metadata = build_stopped_task_session_metadata(
        current_metadata,
        state_snapshot,
        task_id,
        SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
        &updated_at,
    );
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&parent_session_id)
    {
        for record in &mut runtime_state.conversation_state.records {
            if record.id == message_id {
                record.metadata = Some(updated_metadata.clone());
                runtime_state.updated_at = updated_at.clone();
                break;
            }
        }
    }
    if let Some(messages) = workspace_snapshot
        .session_messages
        .get_mut(&parent_session_id)
    {
        for message in messages {
            if message.id == message_id {
                message.metadata = Some(updated_metadata.clone());
                break;
            }
        }
    }
}

/// Finds the best parent task-card message id for a WebUI task-session payload.
/// 为 WebUI task-session 载荷查找最匹配的父级 task 卡片消息标识。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides projected messages and canonical runtime records.
/// `workspace_snapshot` 提供投影消息与规范运行时记录。
/// `parent_session_id` selects the parent session to scan.
/// `parent_session_id` 选择需要扫描的父会话。
/// `payload` is the task-session payload to match.
/// `payload` 是需要匹配的 task-session 载荷。
///
/// # Returns
/// 返回值
/// The best matching message id when one can be found.
/// 找到匹配项时返回最佳消息标识。
fn find_webui_task_session_tool_message_id_for_payload(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    payload: &Value,
) -> Option<String> {
    let mut best_match: Option<(usize, String)> = None;
    if let Some(messages) = workspace_snapshot.session_messages.get(&parent_session_id)
        && collect_best_webui_task_session_message_match(
            messages.iter().rev(),
            payload,
            &mut best_match,
        )
    {
        return best_match.map(|(_, message_id)| message_id);
    }
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get(&parent_session_id)
        && collect_best_webui_task_session_record_match(
            runtime_state.conversation_state.records.iter().rev(),
            payload,
            &mut best_match,
        )
    {
        return best_match.map(|(_, message_id)| message_id);
    }
    best_match.map(|(_, message_id)| message_id)
}

/// Updates the current best task-card match from one iterator of session messages.
/// 从一组会话消息迭代器中更新当前最佳 task 卡片匹配项。
///
/// # Parameters
/// 参数
/// `messages` yields candidate parent session messages.
/// `messages` 产出候选父会话消息。
/// `payload` is the task-session payload to match.
/// `payload` 是需要匹配的 task-session 载荷。
/// `best_match` stores the current best score and message id.
/// `best_match` 保存当前最佳分数与消息标识。
///
/// # Returns
/// 返回值
/// `true` when an exact internal task id match has been found.
/// 找到精确内部任务标识匹配时返回 `true`。
fn collect_best_webui_task_session_message_match<'a>(
    messages: impl Iterator<Item = &'a SessionMessageEntry>,
    payload: &Value,
    best_match: &mut Option<(usize, String)>,
) -> bool {
    for message in messages {
        if message.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        let Some(metadata) = message.metadata.as_ref() else {
            continue;
        };
        if metadata.get("tool_name").and_then(Value::as_str) != Some("task")
            && metadata.get("kind").and_then(Value::as_str) != Some("task_session")
        {
            continue;
        }
        let score = task_session_message_match_score(metadata, payload);
        if score == 0 {
            continue;
        }
        if best_match
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            *best_match = Some((score, message.id.clone()));
        }
        if score == usize::MAX {
            return true;
        }
    }
    false
}

/// Updates the current best task-card match from one iterator of canonical session records.
/// 从一组规范会话记录迭代器中更新当前最佳 task 卡片匹配项。
///
/// # Parameters
/// 参数
/// `records` yields candidate parent session records.
/// `records` 产出候选父会话记录。
/// `payload` is the task-session payload to match.
/// `payload` 是需要匹配的 task-session 载荷。
/// `best_match` stores the current best score and message id.
/// `best_match` 保存当前最佳分数与消息标识。
///
/// # Returns
/// 返回值
/// `true` when an exact internal task id match has been found.
/// 找到精确内部任务标识匹配时返回 `true`。
fn collect_best_webui_task_session_record_match<'a>(
    records: impl Iterator<Item = &'a SessionConversationRecord>,
    payload: &Value,
    best_match: &mut Option<(usize, String)>,
) -> bool {
    for record in records {
        if record.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        let Some(metadata) = record.metadata.as_ref() else {
            continue;
        };
        if metadata.get("tool_name").and_then(Value::as_str) != Some("task")
            && metadata.get("kind").and_then(Value::as_str) != Some("task_session")
        {
            continue;
        }
        let score = task_session_message_match_score(metadata, payload);
        if score == 0 {
            continue;
        }
        if best_match
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            *best_match = Some((score, record.id.clone()));
        }
        if score == usize::MAX {
            return true;
        }
    }
    false
}

/// Replaces one child task transcript with the canonical conversation returned by the subagent.
/// 使用子代理返回的规范会话替换单个子任务转录。
fn replace_webui_subagent_task_conversation_state(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
    conversation_state: vulcan_agent_runtime::conversation_protocol::SessionConversationState,
) {
    let now = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(task_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let preserved_records = runtime_state.conversation_state.records.clone();
    runtime_state.conversation_state = conversation_state;
    if !preserved_records.is_empty() {
        runtime_state.conversation_state.records = preserved_records;
    }
    runtime_state.updated_at = now;
    workspace_snapshot.session_messages.insert(
        task_id,
        conversation_state_to_session_messages(&runtime_state.conversation_state),
    );
    workspace_snapshot.hydrated_session_ids.insert(task_id);
}

/// Enqueues the shared delegated-completion follow-up into the parent session internal queue.
/// 将共享委派完成跟进提示词放入父会话内部队列。
fn enqueue_webui_delegate_completion_followup_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    state: &SubagentTaskPanelState,
) {
    if workspace_snapshot
        .session_run_phases
        .get(&parent_session_id)
        .is_some_and(|phase| *phase == SessionRunPhase::Stopped)
    {
        return;
    }
    let now = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(parent_session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let enqueue_result = enqueue_pending_prompt_internal_queue_entry(
        &mut runtime_state.ui_state,
        PendingSessionPromptEntry {
            id: format!("webui-delegate-completion-{}", SessionId::new_v4()),
            prompt: build_delegate_completion_followup_prompt(state),
            preview: build_delegate_completion_followup_preview(state),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            source: PendingSessionPromptSource::DelegateCompletion,
            task_id: Some(state.task_id.to_string()),
            panel_entry_id: None,
            created_at: now.clone(),
        },
        false,
    );
    match enqueue_result {
        Ok(_) => {
            runtime_state.updated_at = now;
        }
        Err(PendingPromptInternalQueueEnqueueError::QueueFull) => {}
    }
}

/// Hides the selected parent subagent navigation round once iced-equivalent blocking work has settled.
/// 当与 iced 等价的阻塞工作全部收敛后，隐藏该轮子代理导航。
fn refresh_webui_subagent_navigation_round_visibility(
    state: &SharedServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let has_running = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .is_some_and(|tasks| {
            let live_listener_task_ids = &guard.detached_subagent_task_listeners;
            tasks
                .values()
                .map(|state| {
                    normalize_webui_subagent_task_state(
                        state,
                        task_runtime,
                        webui_subagent_task_has_live_run(
                            task_runtime,
                            live_listener_task_ids,
                            state.task_id,
                        ),
                    )
                })
                .any(|state| state.run_state.is_running())
        });
    let has_active_stream_task = guard.active_stream_tasks.contains_key(&parent_session_id);
    let has_active_phase = workspace_snapshot
        .session_run_phases
        .get(&parent_session_id)
        .copied()
        .is_some_and(is_active_session_run_phase);
    let has_pending_delegate_completion = workspace_snapshot
        .session_runtime_state
        .get(&parent_session_id)
        .is_some_and(|runtime_state| {
            runtime_state
                .ui_state
                .pending_prompt_queue
                .iter()
                .any(|entry| entry.source == PendingSessionPromptSource::DelegateCompletion)
        });
    if !(has_active_stream_task
        || has_active_phase
        || has_running
        || has_pending_delegate_completion)
    {
        guard
            .session_subagent_navigation_rounds
            .remove(&parent_session_id);
    }
    Ok(())
}

/// Applies one safe canonical conversation checkpoint and persists it off the stream path.
/// 应用一份安全规范会话检查点，并在流式关键路径外持久化。
fn apply_conversation_checkpoint(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    conversation_state: vulcan_agent_runtime::conversation_protocol::SessionConversationState,
    last_successful_provider_usage: Option<vulcan_core::types::ProviderUsageSnapshot>,
    provider_request_count: usize,
) -> Result<(), String> {
    let workspace_snapshot = with_workspace_mutation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        apply_checkpoint_conversation_state(runtime_state, conversation_state, &now);
        if let Some(usage) = last_successful_provider_usage {
            runtime_state.last_provider_usage = Some(usage.clone());
            runtime_state.last_successful_provider_usage = Some(usage);
        }
        if provider_request_count > 0 {
            runtime_state.last_safe_provider_request_count = Some(provider_request_count);
        }
        workspace.session_messages.insert(
            session_id,
            conversation_state_to_session_messages(&runtime_state.conversation_state),
        );
        workspace.hydrated_session_ids.insert(session_id);
    })?;
    spawn_workspace_session_runtime_state_persist_task(
        state.clone(),
        controller_runtime.clone(),
        workspace_snapshot,
        session_id,
        Some(String::from("conversation_checkpoint")),
    );
    Ok(())
}

/// Result produced after applying one terminal report to retained memory.
/// 将一份终态报告应用到保留内存后产生的结果。
struct TerminalReportMemoryApply {
    /// Updated workspace snapshot after applying the terminal report.
    /// 应用终态报告后的更新工作区快照。
    workspace_snapshot: WorkspaceSnapshot,
    /// Completed-turn hook request that should run asynchronously.
    /// 需要异步执行的完成回合 hook 请求。
    completion_request: Option<ConversationCompletionRequest>,
}

/// Applies the terminal execution report to retained memory and emits the updated workbench view.
/// 将终态执行报告应用到保留内存中，并发出更新后的工作台视图。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit the refreshed workbench snapshot.
/// `app` 是用于发出刷新后工作台快照的 Tauri 应用句柄。
/// `state` owns the retained workspace snapshot mutated by the stream task.
/// `state` 持有由流任务修改的保留工作区快照。
/// `agent_runtime` provides conversation hooks and VMM synchronization state.
/// `agent_runtime` 提供会话 hook 与 VMM 同步状态。
/// `report` is the terminal execution report produced by the agent runtime.
/// `report` 是 Agent 运行时产出的终态执行报告。
///
/// # Returns
/// 返回值
/// The updated workspace snapshot after the terminal report has been applied in memory.
/// 返回终态报告已经应用到内存后的更新工作区快照。
fn apply_execution_report_to_memory(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    report: ExecutionRunReport,
) -> Result<TerminalReportMemoryApply, String> {
    let session_id = report.session_id;
    let mut completion_request = None;
    let workspace_snapshot = with_workspace_mutation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        let apply_result =
            apply_terminal_execution_report_to_state(runtime_state, session_id, &now, &report);
        let project_id = workspace
            .sessions
            .iter()
            .find(|candidate| candidate.id == session_id)
            .and_then(|session| session.project_id.clone());
        let conversation_state = runtime_state.conversation_state.clone();
        if apply_result.is_success {
            let hook_turn_id =
                if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
                    let (hook_turn_id, request) = hook_runtime.prepare_turn_completion_dispatch(
                        session_id,
                        project_id.clone(),
                        report.assistant_output.as_deref().unwrap_or(
                            "The model completed the run but did not return visible text output.",
                        ),
                        report.provider_request_count,
                        &conversation_state,
                    );
                    completion_request = Some(request);
                    hook_turn_id
                } else {
                    String::new()
                };
            runtime_state
                .context_injection_state
                .consume_turn(&hook_turn_id, &now);
            runtime_state.vmm_profile_sync.record_completed_turn();
            let profile_sync = runtime_state.vmm_profile_sync.clone();
            agent_runtime
                .vmm_hook_runtime
                .store_profile_sync_state(session_id, profile_sync);
        } else if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
            hook_runtime.terminate_turn(
                session_id,
                project_id,
                "execution_failed",
                report.provider_request_count,
            );
        }
        workspace.session_messages.insert(
            session_id,
            conversation_state_to_session_messages(&runtime_state.conversation_state),
        );
        if let Some(session) = workspace
            .sessions
            .iter_mut()
            .find(|candidate| candidate.id == session_id)
        {
            session.status = if apply_result.is_success {
                SessionState::Completed
            } else {
                SessionState::Blocked
            };
            session.last_updated_label = String::from("刚刚");
        }
        if workspace.selected_session != session_id {
            runtime_state.ui_state.unread = true;
        }
        workspace.session_run_phases.insert(
            session_id,
            if apply_result.is_success {
                SessionRunPhase::Ready
            } else {
                SessionRunPhase::Failed
            },
        );
        workspace.hydrated_session_ids.insert(session_id);
        agent_runtime
            .vmm_hook_runtime
            .replace_snapshot(build_vmm_hook_config_snapshot(
                &agent_runtime.provider_configuration,
                workspace,
            ));
    })?;
    Ok(TerminalReportMemoryApply {
        workspace_snapshot,
        completion_request,
    })
}

/// Spawns the completed-turn hook without blocking queued prompt dispatch.
/// 启动完成回合 hook，避免阻塞排队提示词分发。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit any later VMM outbox updates.
/// `app` 是用于发出后续 VMM outbox 更新的 Tauri 应用句柄。
/// `state` owns the retained workspace snapshot that receives hook side effects.
/// `state` 持有接收 hook 副作用的保留工作区快照。
/// `controller_runtime` persists VMM outbox changes after the hook finishes.
/// `controller_runtime` 在 hook 完成后持久化 VMM outbox 变更。
/// `agent_runtime` provides VMM hook snapshot refresh state.
/// `agent_runtime` 提供 VMM hook 快照刷新状态。
/// `completion_request` is the prepared completed-turn hook dispatch request.
/// `completion_request` 是已准备的完成回合 hook 调度请求。
fn spawn_conversation_completion_hook_task(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    completion_request: ConversationCompletionRequest,
) {
    tauri::async_runtime::spawn(async move {
        let outcome = completion_request.run().await;
        let session_id = outcome.session_id;
        if let Err(error) = outcome.report.as_ref() {
            eprintln!(
                "[tauri-service] completed-turn hook failed for {}: {}",
                outcome.turn_id, error
            );
        }
        if outcome.vmm_writeback_effects.is_empty() {
            return;
        }
        let workspace_snapshot = match with_workspace_mutation(&app, &state, |workspace| {
            let now = current_unix_millis_text();
            if let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) {
                apply_vmm_writeback_effects_to_runtime_state(
                    runtime_state,
                    outcome.vmm_writeback_effects,
                    &now,
                );
                runtime_state.updated_at = now;
            }
            agent_runtime
                .vmm_hook_runtime
                .replace_snapshot(build_vmm_hook_config_snapshot(
                    &agent_runtime.provider_configuration,
                    workspace,
                ));
        }) {
            Ok(workspace_snapshot) => workspace_snapshot,
            Err(error) => {
                eprintln!("[tauri-service] apply completed-turn hook effects failed: {error}");
                return;
            }
        };
        if let Err(error) = persist_workspace_session_update(
            Some(&state),
            controller_runtime,
            &workspace_snapshot,
            session_id,
        )
        .await
        {
            eprintln!("[tauri-service] persist completed-turn hook effects failed: {error}");
        }
    });
}

/// Applies VMM writeback hook effects to one session runtime state.
/// 将 VMM 写回 hook 效果应用到单个会话运行态。
fn apply_vmm_writeback_effects_to_runtime_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    effects: Vec<VmmWritebackHookEffect>,
    now: &str,
) {
    for effect in effects {
        match effect {
            VmmWritebackHookEffect::Flushed { entry_ids } => {
                let ids = entry_ids.into_iter().collect::<BTreeSet<_>>();
                runtime_state
                    .vmm_writeback_outbox
                    .retain(|entry| !ids.contains(&entry.id));
            }
            VmmWritebackHookEffect::Enqueue { request, error } => {
                let id = format!(
                    "vmm-outbox-{}-{}",
                    request.session_id,
                    stable_vmm_outbox_request_hash(&request)
                );
                if let Some(existing) = runtime_state
                    .vmm_writeback_outbox
                    .iter_mut()
                    .find(|entry| entry.id == id)
                {
                    existing.request = request;
                    existing.attempt_count = existing.attempt_count.saturating_add(1);
                    existing.last_error = Some(error);
                    existing.updated_at = now.to_string();
                } else {
                    runtime_state.vmm_writeback_outbox.push(
                        vulcan_workbench_model::VmmWritebackOutboxEntry {
                            id,
                            request,
                            attempt_count: 1,
                            last_error: Some(error),
                            created_at: now.to_string(),
                            updated_at: now.to_string(),
                        },
                    );
                }
            }
            VmmWritebackHookEffect::Failed { entry_id, error } => {
                if let Some(existing) = runtime_state
                    .vmm_writeback_outbox
                    .iter_mut()
                    .find(|entry| entry.id == entry_id)
                {
                    existing.attempt_count = existing.attempt_count.saturating_add(1);
                    existing.last_error = Some(error);
                    existing.updated_at = now.to_string();
                }
            }
        }
    }
}

/// Builds one stable hash for a VMM writeback request.
/// 为一条 VMM 写回请求构建稳定哈希。
fn stable_vmm_outbox_request_hash(request: &vulcan_vmm::PostActionRequest) -> u64 {
    let mut hasher = StableVmmOutboxHasher::default();
    hasher.write_str(&request.session_id);
    hasher.write_u64(request.user_id);
    hasher.write_u64(request.project_id);
    hasher.write_str(&request.user_content);
    hasher.write_str(&request.assistant_content);
    for item in &request.timeline {
        hasher.write_str(item.kind.as_wire_type());
        hasher.write_str(&item.content);
    }
    hasher.finish()
}

/// Tiny deterministic FNV-1a helper used for persisted VMM outbox ids.
/// 用于持久化 VMM outbox 标识的小型确定性 FNV-1a 辅助器。
struct StableVmmOutboxHasher {
    /// Current FNV-1a accumulator.
    /// 当前 FNV-1a 累加值。
    value: u64,
}

impl Default for StableVmmOutboxHasher {
    /// Creates one FNV-1a hasher with the canonical 64-bit offset basis.
    /// 使用标准 64 位 offset basis 创建一个 FNV-1a 哈希器。
    fn default() -> Self {
        Self {
            value: 0xcbf29ce484222325,
        }
    }
}

impl StableVmmOutboxHasher {
    /// Writes a length-prefixed string into the hash stream.
    /// 将带长度前缀的字符串写入哈希流。
    fn write_str(&mut self, value: &str) {
        self.write_u64(value.len() as u64);
        self.write_bytes(value.as_bytes());
    }

    /// Writes one integer into the hash stream in little-endian form.
    /// 以小端形式将一个整数写入哈希流。
    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Writes raw bytes into the FNV-1a accumulator.
    /// 将原始字节写入 FNV-1a 累加器。
    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.value ^= u64::from(*byte);
            self.value = self.value.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Returns the final deterministic hash value.
    /// 返回最终的确定性哈希值。
    fn finish(self) -> u64 {
        self.value
    }
}

/// Mutates the retained workspace snapshot and emits a rebuilt workbench snapshot.
/// 修改保留的工作区快照并发出重建后的工作台快照。
fn with_workspace_mutation<F>(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    mutate: F,
) -> Result<WorkspaceSnapshot, String>
where
    F: FnOnce(&mut WorkspaceSnapshot),
{
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        ai_tool_configuration,
        workspace_snapshot,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = guard
            .luaskills_runtime
            .as_ref()
            .and_then(|handle| crate::luaskills_bridge::load_luaskills_catalog(handle).ok())
            .unwrap_or_else(empty_luaskills_catalog);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = guard
            .mcp_runtime
            .as_ref()
            .map(TauriMcpRuntimeHandle::catalog)
            .unwrap_or_else(empty_mcp_catalog);
        let logs = guard.startup.recent_logs.clone();
        let mut workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let live_context_budgets = guard.live_context_budgets.clone();
        mutate(&mut workspace_snapshot);
        let surface_messages =
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot);
        let subagent_navigation =
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot);
        let workbench = workbench_from_runtime(
            &controller_runtime,
            &luaskills_catalog,
            &token_budget_runtime,
            &mcp_catalog,
            &provider_configuration,
            &ai_tool_configuration,
            &workspace_snapshot,
            surface_messages.as_slice(),
            subagent_navigation,
            &logs,
            &live_context_budgets,
        );
        guard.workspace_snapshot = Some(workspace_snapshot.clone());
        guard.workbench = workbench.clone();
        let _ = app.emit(
            "vulcan://service-event",
            ServiceEvent::Workbench(Box::new(workbench)),
        );
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            ai_tool_configuration,
            workspace_snapshot,
        )
    };
    let _ = (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        ai_tool_configuration,
    );
    Ok(workspace_snapshot)
}

/// Returns the selected session messages from runtime state before falling back to persisted rows.
/// 优先从运行时状态返回当前选中会话消息，缺失时回退到持久化行。
fn selected_session_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<Vec<SessionMessageEntry>> {
    in_memory_session_messages_from_workspace(
        workspace_snapshot,
        workspace_snapshot.selected_session,
    )
}

/// Normalizes one subagent task state against the shared task runtime.
/// 根据共享任务运行时归一化单条子代理任务状态。
fn normalize_webui_subagent_task_state(
    state: &SubagentTaskPanelState,
    task_runtime: Option<&Arc<TaskRuntime>>,
    has_live_listener: bool,
) -> SubagentTaskPanelState {
    if !state.run_state.is_running()
        || has_live_listener
        || task_runtime
            .is_some_and(|runtime| runtime.is_task_running(state.task_id).unwrap_or(false))
    {
        return state.clone();
    }
    let mut normalized = state.clone();
    normalized.run_state = if normalized.result_text.trim() == SUBAGENT_TASK_STOPPED_SUMMARY_TEXT {
        SubagentTaskRunState::Stopped
    } else {
        SubagentTaskRunState::Completed
    };
    normalized
}

/// Returns whether one child task still has a live WebUI route or runtime abort handle.
/// 返回单个子任务是否仍拥有 WebUI 实时路由或运行时中止句柄。
fn webui_subagent_task_has_live_run(
    task_runtime: Option<&Arc<TaskRuntime>>,
    live_listener_task_ids: &BTreeSet<SessionId>,
    task_id: SessionId,
) -> bool {
    live_listener_task_ids.contains(&task_id)
        || task_runtime.is_some_and(|runtime| runtime.is_task_running(task_id).unwrap_or(false))
}

/// Rebuilds the selected parent session subagent cache from shared task-session metadata.
/// 从共享 task-session 元数据重建当前父会话的子代理缓存。
fn rebuild_webui_subagent_task_cache_for_session(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) {
    if parent_session_id == SessionId::nil() {
        return;
    }
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let previous_states = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .cloned()
        .unwrap_or_default();
    let live_listener_task_ids = &guard.detached_subagent_task_listeners;
    let mut persisted_states = workspace_snapshot
        .session_runtime_state
        .get(&parent_session_id)
        .into_iter()
        .flat_map(|runtime_state| runtime_state.conversation_state.records.iter())
        .filter_map(|record| {
            record.metadata.as_ref().and_then(|metadata| {
                parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
            })
        })
        .collect::<Vec<_>>();
    if let Some(projected_messages) = workspace_snapshot.session_messages.get(&parent_session_id) {
        persisted_states.extend(projected_messages.iter().filter_map(|message| {
            message.metadata.as_ref().and_then(|metadata| {
                parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
            })
        }));
    }
    let entry = guard
        .session_subagent_tasks
        .entry(parent_session_id)
        .or_default();
    entry.retain(|_, state| {
        state.run_state.is_running()
            && webui_subagent_task_has_live_run(task_runtime, live_listener_task_ids, state.task_id)
    });
    for state in persisted_states {
        let normalized = if state.run_state.is_running() {
            previous_states
                .get(&state.task_id)
                .map(|existing| {
                    normalize_webui_subagent_task_state(
                        existing,
                        task_runtime,
                        webui_subagent_task_has_live_run(
                            task_runtime,
                            live_listener_task_ids,
                            existing.task_id,
                        ),
                    )
                })
                .filter(|existing| !existing.run_state.is_running())
                .unwrap_or_else(|| {
                    normalize_webui_subagent_task_state(
                        &state,
                        task_runtime,
                        webui_subagent_task_has_live_run(
                            task_runtime,
                            live_listener_task_ids,
                            state.task_id,
                        ),
                    )
                })
        } else {
            state
        };
        entry.insert(normalized.task_id, normalized);
    }
    if entry.is_empty() {
        guard.session_subagent_tasks.remove(&parent_session_id);
    }
    if guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id == parent_session_id)
        && !guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .is_some_and(|tasks| tasks.contains_key(&guard.active_subagent_panel.unwrap().task_id))
    {
        guard.active_subagent_panel = None;
    }
}

/// Returns the currently active transcript surface session id for the WebUI.
/// 返回 WebUI 当前活动转录界面的会话标识。
fn active_surface_session_id_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> SessionId {
    let parent_session_id = workspace_snapshot.selected_session;
    rebuild_webui_subagent_task_cache_for_session(guard, workspace_snapshot, parent_session_id);
    if let Some(selection) = guard.active_subagent_panel
        && selection.parent_session_id == parent_session_id
        && let Some(state) = guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .and_then(|tasks| tasks.get(&selection.task_id))
    {
        let task_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| &runtime.task_runtime);
        let normalized = normalize_webui_subagent_task_state(
            state,
            task_runtime,
            webui_subagent_task_has_live_run(
                task_runtime,
                &guard.detached_subagent_task_listeners,
                state.task_id,
            ),
        );
        if normalized.run_state.is_running() {
            return selection.task_id;
        }
    }
    if guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id == parent_session_id)
    {
        guard.active_subagent_panel = None;
    }
    parent_session_id
}

/// Returns transcript messages for the currently active WebUI surface.
/// 返回 WebUI 当前活动界面的转录消息。
fn active_surface_messages_from_workspace_and_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Vec<SessionMessageEntry> {
    let active_session_id = active_surface_session_id_from_state(guard, workspace_snapshot);
    in_memory_session_messages_from_workspace(workspace_snapshot, active_session_id)
        .unwrap_or_default()
}

/// Builds the subagent navigation snapshot for the selected parent conversation.
/// 为当前选中父会话构建子代理导航快照。
fn subagent_navigation_snapshot_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    let parent_session_id = workspace_snapshot.selected_session;
    if parent_session_id == SessionId::nil() {
        return None;
    }
    rebuild_webui_subagent_task_cache_for_session(guard, workspace_snapshot, parent_session_id);
    let requested_active_task_id = guard.active_subagent_panel.and_then(|selection| {
        (selection.parent_session_id == parent_session_id).then_some(selection.task_id)
    });
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let running_states = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .map(|entries| {
            let live_listener_task_ids = &guard.detached_subagent_task_listeners;
            entries
                .values()
                .map(|state| {
                    normalize_webui_subagent_task_state(
                        state,
                        task_runtime,
                        webui_subagent_task_has_live_run(
                            task_runtime,
                            live_listener_task_ids,
                            state.task_id,
                        ),
                    )
                })
                .filter(|state| state.run_state.is_running())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if running_states.is_empty() {
        return None;
    }
    let active_task_id = requested_active_task_id
        .filter(|task_id| running_states.iter().any(|state| state.task_id == *task_id));
    let tasks = running_states
        .iter()
        .map(|state| subagent_task_summary_from_state(state, active_task_id))
        .collect::<Vec<_>>();
    let active_conversation_id = active_task_id.unwrap_or(parent_session_id).to_string();
    Some(SubagentNavigationSnapshot {
        parent_conversation_id: parent_session_id.to_string(),
        active_conversation_id,
        active_task_id: active_task_id.map(|task_id| task_id.to_string()),
        tasks,
    })
}

/// Converts one shared subagent panel state into a service API summary.
/// 将一条共享子代理面板状态转换为 service API 摘要。
fn subagent_task_summary_from_state(
    state: &SubagentTaskPanelState,
    active_task_id: Option<SessionId>,
) -> SubagentTaskSummary {
    SubagentTaskSummary {
        task_id: state.task_id.to_string(),
        public_task_id: state.public_task_id.clone(),
        label: subagent_navigation_task_label(state),
        description: state.description.clone(),
        run_state: subagent_run_state_key(state.run_state).to_string(),
        is_running: state.run_state.is_running(),
        is_active: active_task_id == Some(state.task_id),
    }
}

/// Returns the service API run-state key for one shared subagent state.
/// 返回单个共享子代理状态对应的 service API 运行状态键。
fn subagent_run_state_key(run_state: SubagentTaskRunState) -> &'static str {
    match run_state {
        SubagentTaskRunState::Running => "running",
        SubagentTaskRunState::Completed => "completed",
        SubagentTaskRunState::Stopped => "stopped",
        SubagentTaskRunState::Failed => "failed",
    }
}

/// Refreshes the WebUI workbench after a focused workspace metadata update.
/// 在一次聚焦的工作区元数据更新后刷新 WebUI 工作台。
async fn refresh_workbench_snapshot(
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        previous_snapshot,
        provider_configuration,
        ai_tool_configuration,
        live_context_budgets,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = guard
            .luaskills_runtime
            .as_ref()
            .and_then(|handle| crate::luaskills_bridge::load_luaskills_catalog(handle).ok())
            .unwrap_or_else(|| TauriLuaSkillsCatalog {
                runtime_root: PathBuf::new(),
                skills: Vec::new(),
                entry_count: 0,
            });
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = guard
            .mcp_runtime
            .as_ref()
            .map(TauriMcpRuntimeHandle::catalog)
            .unwrap_or_else(|| TauriMcpRuntimeCatalog {
                configured_server_count: 0,
                enabled_server_count: 0,
                connected_server_count: 0,
                tool_count: 0,
                prompt_count: 0,
                resource_count: 0,
                summary_rows: Vec::new(),
            });
        let logs = guard.startup.recent_logs.clone();
        let previous_snapshot = guard.workspace_snapshot.clone();
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let live_context_budgets = guard.live_context_budgets.clone();
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            previous_snapshot,
            provider_configuration,
            ai_tool_configuration,
            live_context_budgets,
        )
    };

    let mut refreshed_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    preserve_active_session_run_state(previous_snapshot.as_ref(), &mut refreshed_snapshot);
    let selected_messages =
        if let Some(messages) = selected_session_messages_from_workspace(&refreshed_snapshot) {
            messages
        } else {
            let hydration_snapshot = match load_selected_session_hydration(
                controller_runtime.clone(),
                refreshed_snapshot.selected_session,
            )
            .await
            {
                Ok(snapshot) => snapshot,
                Err(error) if is_missing_session_manifest_error(&error) => None,
                Err(error) => return Err(error),
            };
            hydration_snapshot
                .map(|snapshot| snapshot.messages)
                .unwrap_or_default()
        };
    if refreshed_snapshot.selected_session != SessionId::nil() && !selected_messages.is_empty() {
        refreshed_snapshot
            .session_messages
            .insert(refreshed_snapshot.selected_session, selected_messages);
    }
    let (surface_messages, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &refreshed_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &refreshed_snapshot),
        )
    };
    let workbench = workbench_from_runtime(
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &refreshed_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        &logs,
        &live_context_budgets,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(refreshed_snapshot);
    }
    Ok(workbench)
}

/// Preserves in-memory running session markers across database-backed snapshot refreshes.
/// 在基于数据库重载快照时保留内存中的会话运行中标记。
fn preserve_active_session_run_state(
    previous_snapshot: Option<&WorkspaceSnapshot>,
    refreshed_snapshot: &mut WorkspaceSnapshot,
) {
    let Some(previous_snapshot) = previous_snapshot else {
        return;
    };
    if previous_snapshot.selected_session != SessionId::nil()
        && refreshed_snapshot
            .sessions
            .iter()
            .any(|session| session.id == previous_snapshot.selected_session)
    {
        refreshed_snapshot.selected_session = previous_snapshot.selected_session;
    }
    for previous_session in previous_snapshot.sessions.iter().filter(|session| {
        session.status == SessionState::Running
            || previous_snapshot
                .session_run_phases
                .get(&session.id)
                .copied()
                .is_some_and(is_active_session_run_phase)
            || previous_snapshot
                .session_runtime_state
                .get(&session.id)
                .is_some_and(|runtime_state| {
                    !runtime_state.ui_state.pending_prompt_queue.is_empty()
                        || !runtime_state.ui_state.pending_prompt_panel_queue.is_empty()
                })
    }) {
        let previous_phase = previous_snapshot
            .session_run_phases
            .get(&previous_session.id)
            .copied();
        let had_active_run = previous_session.status == SessionState::Running
            || previous_phase.is_some_and(is_active_session_run_phase);
        if !refreshed_snapshot
            .sessions
            .iter()
            .any(|session| session.id == previous_session.id)
        {
            preserve_missing_visible_session_summary(
                previous_snapshot,
                refreshed_snapshot,
                previous_session,
            );
        }
        if let Some(refreshed_session) = refreshed_snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == previous_session.id)
        {
            refreshed_session.status = if had_active_run {
                SessionState::Running
            } else {
                previous_session.status
            };
            refreshed_session.last_updated_label = previous_session.last_updated_label.clone();
        }
        if let Some(previous_phase) = previous_phase {
            let preserved_phase = if had_active_run && !is_active_session_run_phase(previous_phase)
            {
                SessionRunPhase::Preparing
            } else {
                previous_phase
            };
            refreshed_snapshot
                .session_run_phases
                .insert(previous_session.id, preserved_phase);
        }
        if let Some(mut runtime_state) = previous_snapshot
            .session_runtime_state
            .get(&previous_session.id)
            .cloned()
        {
            if let Some(refreshed_runtime_state) = refreshed_snapshot
                .session_runtime_state
                .get(&previous_session.id)
                && !runtime_state_has_concrete_conversation_context(&runtime_state)
                && runtime_state_has_concrete_conversation_context(refreshed_runtime_state)
            {
                runtime_state.conversation_state =
                    refreshed_runtime_state.conversation_state.clone();
            }
            refreshed_snapshot
                .session_runtime_state
                .insert(previous_session.id, runtime_state);
        }
        if let Some(messages) = previous_snapshot
            .session_messages
            .get(&previous_session.id)
            .cloned()
        {
            refreshed_snapshot
                .session_messages
                .insert(previous_session.id, messages);
        }
    }
}

/// Preserves one in-memory visible session that has not reached the database snapshot yet.
/// 保留一个尚未进入数据库快照的内存可见会话。
///
/// # Parameters
/// 参数
/// `previous_snapshot` is the retained in-memory snapshot before database refresh.
/// `previous_snapshot` 是数据库刷新前保留的内存快照。
/// `refreshed_snapshot` is the database-backed snapshot being repaired.
/// `refreshed_snapshot` 是正在修复的数据库来源快照。
/// `previous_session` is the active or queued session to preserve.
/// `previous_session` 是需要保留的运行中或排队会话。
fn preserve_missing_visible_session_summary(
    previous_snapshot: &WorkspaceSnapshot,
    refreshed_snapshot: &mut WorkspaceSnapshot,
    previous_session: &SessionSummary,
) {
    let previous_index = previous_snapshot
        .sessions
        .iter()
        .position(|session| session.id == previous_session.id)
        .unwrap_or_default();
    let insert_index = previous_index.min(refreshed_snapshot.sessions.len());
    refreshed_snapshot
        .sessions
        .insert(insert_index, previous_session.clone());
    if previous_snapshot.selected_session == previous_session.id {
        refreshed_snapshot.selected_session = previous_session.id;
    }
    preserve_visible_project_session_membership(
        previous_snapshot,
        refreshed_snapshot,
        previous_session,
    );
}

/// Preserves the project sidebar membership for one in-memory session.
/// 保留一个内存会话在项目侧栏中的归属关系。
///
/// # Parameters
/// 参数
/// `previous_snapshot` is the retained snapshot that still contains the sidebar relation.
/// `previous_snapshot` 是仍包含侧栏关系的保留快照。
/// `refreshed_snapshot` is the database-backed snapshot being repaired.
/// `refreshed_snapshot` 是正在修复的数据库来源快照。
/// `previous_session` selects the session whose project membership must be retained.
/// `previous_session` 选择需要保留项目归属的会话。
fn preserve_visible_project_session_membership(
    previous_snapshot: &WorkspaceSnapshot,
    refreshed_snapshot: &mut WorkspaceSnapshot,
    previous_session: &SessionSummary,
) {
    let Some(project_id) = previous_session.project_id.as_deref() else {
        return;
    };
    let Some(previous_project) = previous_snapshot
        .task_projects
        .iter()
        .find(|project| project.id == project_id)
    else {
        return;
    };
    let previous_index = previous_project
        .session_ids
        .iter()
        .position(|session_id| *session_id == previous_session.id)
        .unwrap_or_default();
    if let Some(refreshed_project) = refreshed_snapshot
        .task_projects
        .iter_mut()
        .find(|project| project.id == project_id)
    {
        refreshed_project
            .session_ids
            .retain(|session_id| *session_id != previous_session.id);
        let insert_index = previous_index.min(refreshed_project.session_ids.len());
        refreshed_project
            .session_ids
            .insert(insert_index, previous_session.id);
    }
}

/// Returns whether one session phase still represents active work.
/// 判断单个会话阶段是否仍代表活跃执行。
fn is_active_session_run_phase(phase: SessionRunPhase) -> bool {
    matches!(
        phase,
        SessionRunPhase::Preparing
            | SessionRunPhase::MemoryRecall
            | SessionRunPhase::Requesting
            | SessionRunPhase::Thinking
            | SessionRunPhase::ToolRunning
    )
}

/// Returns whether a runtime event closes the current streamed thinking segment.
/// 判断运行时事件是否会关闭当前流式思考片段。
fn is_provider_tool_boundary_runtime_event(event: &RuntimeEvent) -> bool {
    matches!(
        event.kind,
        RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
    ) && (event.title.starts_with("工具调用 · ") || event.title.starts_with("工具返回 · "))
}

/// Loads the selected session transcript window when a selected session exists.
/// 在存在选中会话时加载其转录窗口。
async fn load_selected_session_hydration(
    controller_runtime: ControllerRuntimeHandle,
    selected_session_id: SessionId,
) -> Result<Option<SessionHydrationSnapshot>, String> {
    if selected_session_id == SessionId::nil() {
        return Ok(None);
    }
    vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
        controller_runtime,
        selected_session_id,
        SELECTED_SESSION_TURN_LOAD_LIMIT,
    )
    .await
    .map(Some)
}

/// Returns whether a hydration failure is caused by an empty draft row without an event-store manifest.
/// 判断水合失败是否源自空草稿行缺少事件存储 manifest。
fn is_missing_session_manifest_error(error: &str) -> bool {
    error.contains("manifest.json")
        && (error.contains("os error 3")
            || error.contains("os error 2")
            || error.contains("系统找不到指定的路径")
            || error.contains("The system cannot find the path specified")
            || error.contains("No such file or directory"))
}

/// Returns the current Unix timestamp as millisecond text for SQLite metadata rows.
/// 返回当前 Unix 毫秒时间戳文本，用于 SQLite 元数据行。
fn current_unix_millis_text() -> String {
    OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .checked_div(1_000_000)
        .unwrap_or_default()
        .to_string()
}

/// Emits initial service events after the WebView is created.
/// 在 WebView 创建后发出初始服务事件。
fn emit_initial_service_events(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(guard) = state.lock()
    {
        emit_service_snapshots(app, &guard.startup, &guard.workbench);
    }
}

/// Starts the real service bootstrap on a background thread.
/// 在后台线程启动真实服务引导流程。
fn spawn_service_bootstrap(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name(String::from("vulcan-tauri-service-bootstrap"))
        .spawn(move || {
            if let Err(error) = run_service_bootstrap(&app) {
                update_service_failure(&app, error);
            }
        })
        .expect("failed to spawn Vulcan service bootstrap thread");
}

/// Loads the same standalone AI capability configuration used by the iced workbench.
/// 加载与 iced 工作台一致的独立 AI 能力配置。
fn load_service_ai_tool_configuration(
    provider_configuration: &ProviderConfigurationFile,
    logs: &mut Vec<String>,
) -> PersistedAiToolConfiguration {
    match vulcan_agent_runtime::ai_tool_configuration::load_ai_tool_configuration_file(
        provider_configuration,
    ) {
        Ok(loaded) => {
            if loaded.normalized {
                if let Err(error) =
                    vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
                        &loaded.configuration,
                    )
                {
                    logs.push(format!(
                        "[tauri-service] ai capability configuration normalization save failed: {error}"
                    ));
                }
            }
            match loaded.source {
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::DedicatedFile => logs.push(String::from(
                    "[tauri-service] ai capability configuration loaded from ~/.vulcan/vulcan-ai-capabilities.json",
                )),
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::LegacyProviderFile => {
                    match vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
                        &loaded.configuration,
                    ) {
                        Ok(()) => logs.push(String::from(
                            "[tauri-service] legacy embedded ai capability configuration migrated to ~/.vulcan/vulcan-ai-capabilities.json",
                        )),
                        Err(error) => logs.push(format!(
                            "[tauri-service] legacy ai capability configuration migration failed: {error}"
                        )),
                    }
                }
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::Default => logs.push(String::from(
                    "[tauri-service] ai capability configuration uses built-in defaults",
                )),
            }
            loaded.configuration
        }
        Err(error) => {
            logs.push(format!(
                "[tauri-service] ai capability configuration load failed, falling back: {error}"
            ));
            if provider_configuration.ai_tools.is_default() {
                PersistedAiToolConfiguration::default()
            } else {
                provider_configuration.ai_tools.clone()
            }
        }
    }
}

/// Runs the real startup sequence shared with the legacy GUI.
/// 运行与旧版 GUI 共享的真实启动顺序。
fn run_service_bootstrap(app: &tauri::AppHandle) -> Result<(), String> {
    let mut logs = vec![format!(
        "[tauri-service] workspace root: {}",
        workspace_root().display()
    )];
    let controller_configuration =
        vulcan_provider_config::config::load_controller_configuration_file()?;
    let provider_configuration = vulcan_provider_config::config::load_configuration_file()?;
    let ai_tool_configuration =
        load_service_ai_tool_configuration(&provider_configuration, &mut logs);

    update_startup(app, "bootstrapping-vldb", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Running,
                    "正在初始化 VLDB/controller 数据平面",
                ),
                ("luaskills", StartupStepStatus::Pending, "等待 VLDB 就绪"),
                (
                    "token-budget",
                    StartupStepStatus::Pending,
                    "等待 LuaSkills 就绪",
                ),
                ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });

    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("创建 Tauri service tokio runtime 失败：{error}"))?,
    );

    let controller_report = runtime.block_on(
        vulcan_workspace_store::data_plane::bootstrap_application_data_plane(
            controller_configuration.clone(),
        ),
    )?;
    logs.push(format!(
        "[tauri-service] vldb ready: {}",
        controller_report.handle.sqlite_path.display()
    ));
    let controller_runtime = controller_report.handle.clone();

    update_startup(app, "bootstrapping-luaskills", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Running,
                    "正在初始化 LuaSkills 引擎与技能根",
                ),
                (
                    "token-budget",
                    StartupStepStatus::Pending,
                    "等待 LuaSkills 就绪",
                ),
                ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });

    let luaskills_runtime = crate::luaskills_bridge::bootstrap_luaskills(
        controller_configuration.clone(),
        |message| update_luaskills_bootstrap_progress(app, &logs, message),
    )?;
    let luaskills_catalog = crate::luaskills_bridge::load_luaskills_catalog(&luaskills_runtime)?;
    logs.push(format!(
        "[tauri-service] luaskills ready: root={} skills={} entries={}",
        luaskills_catalog.runtime_root.display(),
        luaskills_catalog.skills.len(),
        luaskills_catalog.entry_count
    ));

    update_startup(app, "bootstrapping-token-budget", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Ready,
                    "LuaSkills 引擎与技能目录已就绪",
                ),
                (
                    "token-budget",
                    StartupStepStatus::Running,
                    "正在初始化 Token 预算运行时",
                ),
                ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });

    let token_budget_runtime = crate::token_budget_bridge::TauriTokenBudgetRuntime::initialize();
    logs.push(format!(
        "[tauri-service] token budget: {}",
        token_budget_runtime.status_row()
    ));

    update_startup(app, "bootstrapping-mcp", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Ready,
                    "LuaSkills 引擎与技能目录已就绪",
                ),
                (
                    "token-budget",
                    token_budget_status(&token_budget_runtime),
                    token_budget_runtime.status_row().as_str(),
                ),
                ("mcp", StartupStepStatus::Running, "正在初始化 MCP Runtime"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });

    let mcp_runtime = runtime
        .block_on(crate::mcp_bridge::bootstrap_mcp_runtime(
            provider_configuration.clone(),
        ))
        .map_err(|error| format!("MCP Runtime 初始化失败：{error}"))?;
    let mcp_catalog = mcp_runtime.catalog();
    logs.push(format!(
        "[tauri-service] mcp ready: servers={}/{} connected={} tools={} prompts={} resources={}",
        mcp_catalog.enabled_server_count,
        mcp_catalog.configured_server_count,
        mcp_catalog.connected_server_count,
        mcp_catalog.tool_count,
        mcp_catalog.prompt_count,
        mcp_catalog.resource_count
    ));
    let agent_profile_catalog = load_agent_profile_catalog()?;
    let agent_luaskills_runtime = LuaSkillsRuntimeHandle {
        engine: Arc::clone(&luaskills_runtime.engine),
        runtime_root: luaskills_runtime.runtime_root.clone(),
        system_lua_lib_dir: luaskills_runtime.runtime_root.join("system_lua_lib"),
        skill_roots: luaskills_runtime.skill_roots.clone(),
        entry_count: luaskills_catalog.entry_count,
        operation_progress_events: Arc::new(Mutex::new(VecDeque::new())),
    };
    let agent_mcp_runtime = runtime
        .block_on(bootstrap_mcp_runtime_access(provider_configuration.clone()))
        .map_err(|error| format!("Agent MCP Runtime 初始化失败：{error}"))?;
    let agent_token_budget_runtime = Arc::new(
        TokenBudgetRuntime::initialize_from_resource_directory_with_backend(
            token_budget_runtime.resource_directory().to_path_buf(),
            token_budget_runtime.backend(),
        ),
    );
    let vmm_hook_runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot::default());
    let mut conversation_hook_runtime = ConversationHookRuntime::new();
    register_builtin_vmm_hooks(
        conversation_hook_runtime.dispatcher_mut(),
        vmm_hook_runtime.clone(),
    )
    .map_err(|error| format!("VMM hook 注册失败：{error}"))?;
    let agent_runtime = AgentRuntimeBridge {
        provider_configuration: provider_configuration.clone(),
        ai_tool_configuration: ai_tool_configuration.clone(),
        agent_profile_catalog,
        luaskills_runtime: Some(agent_luaskills_runtime),
        mcp_runtime: Some(agent_mcp_runtime),
        task_runtime: Arc::new(TaskRuntime::new()),
        question_runtime: Arc::new(QuestionRuntime::new()),
        command_session_runtime: Arc::new(CommandSessionRuntime::new()),
        token_budget_runtime: agent_token_budget_runtime,
        conversation_hook_runtime: Arc::new(Mutex::new(conversation_hook_runtime)),
        vmm_hook_runtime,
    };
    logs.push(String::from(
        "[tauri-service] agent runtime bridge ready: provider/LuaSkills/MCP/token-budget handles are mounted",
    ));

    update_startup(app, "loading-workspace", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Ready,
                    "LuaSkills 引擎与技能目录已就绪",
                ),
                (
                    "token-budget",
                    token_budget_status(&token_budget_runtime),
                    token_budget_runtime.status_row().as_str(),
                ),
                ("mcp", StartupStepStatus::Ready, "MCP Runtime 已初始化"),
                (
                    "workspace",
                    StartupStepStatus::Running,
                    "正在载入项目、会话与当前会话正文",
                ),
            ],
        )
    });

    let workspace_snapshot = runtime.block_on(
        vulcan_workspace_store::workspace_store::load_or_seed_workspace(controller_runtime.clone()),
    )?;
    let vmm_hook_snapshot =
        build_vmm_hook_config_snapshot(&provider_configuration, &workspace_snapshot);
    agent_runtime
        .vmm_hook_runtime
        .seed_profile_cache(vmm_hook_snapshot.profile_sync.clone());
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(vmm_hook_snapshot);
    let selected_session_id = workspace_snapshot.selected_session;
    let hydration_snapshot = runtime.block_on(load_selected_session_hydration(
        controller_runtime.clone(),
        selected_session_id,
    ))?;
    logs.push(format!(
        "[tauri-service] workspace loaded: projects={} sessions={} selected={}",
        workspace_snapshot.task_projects.len(),
        workspace_snapshot.sessions.len(),
        selected_session_id
    ));

    let startup = startup_snapshot_with_phase(
        "ready",
        startup_steps_with_status(
            OffsetDateTime::now_utc(),
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Ready,
                    "LuaSkills 引擎与技能目录已就绪",
                ),
                (
                    "token-budget",
                    token_budget_status(&token_budget_runtime),
                    token_budget_runtime.status_row().as_str(),
                ),
                ("mcp", StartupStepStatus::Ready, "MCP Runtime 已初始化"),
                (
                    "workspace",
                    StartupStepStatus::Ready,
                    "项目、会话与当前会话正文已载入",
                ),
            ],
        ),
        &logs,
    );
    let workbench = workbench_from_runtime(
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        hydration_snapshot
            .as_ref()
            .map(|snapshot| snapshot.messages.as_slice())
            .unwrap_or(&[]),
        None,
        &logs,
        &BTreeMap::new(),
    );

    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(mut guard) = state.lock()
    {
        guard.startup = startup.clone();
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.agent_runtime = Some(agent_runtime);
        guard.service_runtime = Some(runtime);
        guard.controller_runtime = Some(controller_runtime);
        guard.luaskills_runtime = Some(luaskills_runtime);
        guard.token_budget_runtime = Some(token_budget_runtime);
        guard.mcp_runtime = Some(mcp_runtime);
    }
    emit_service_snapshots(app, &startup, &workbench);
    Ok(())
}

/// Updates startup state while a bootstrap step is running.
/// 在引导步骤运行中更新启动状态。
fn update_startup<F>(app: &tauri::AppHandle, phase: &str, logs: &[String], build_steps: F)
where
    F: FnOnce(OffsetDateTime) -> Vec<StartupStepSnapshot>,
{
    let startup = startup_snapshot_with_phase(phase, build_steps(OffsetDateTime::now_utc()), logs);
    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(mut guard) = state.lock()
    {
        guard.startup = startup.clone();
    }
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Startup(Box::new(startup)),
    );
}

/// Emits a LuaSkills-specific startup progress snapshot.
/// 发出 LuaSkills 专用启动进度快照。
fn update_luaskills_bootstrap_progress(app: &tauri::AppHandle, logs: &[String], message: &str) {
    update_startup(app, "bootstrapping-luaskills", logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                ("luaskills", StartupStepStatus::Running, message),
                (
                    "token-budget",
                    StartupStepStatus::Pending,
                    "等待 LuaSkills 就绪",
                ),
                ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });
}

/// Updates the shared service state after a startup failure.
/// 在启动失败后更新共享服务状态。
fn update_service_failure(app: &tauri::AppHandle, error: String) {
    let logs = vec![
        format!("[tauri-service] startup failed: {error}"),
        format!(
            "[tauri-service] workspace root: {}",
            workspace_root().display()
        ),
    ];
    let startup = startup_snapshot_with_phase(
        "failed",
        startup_steps_with_status(
            OffsetDateTime::now_utc(),
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                ("vldb", StartupStepStatus::Error, error.as_str()),
                (
                    "luaskills",
                    StartupStepStatus::Pending,
                    "上游启动失败，未继续",
                ),
                (
                    "token-budget",
                    StartupStepStatus::Pending,
                    "上游启动失败，未继续",
                ),
                ("mcp", StartupStepStatus::Pending, "上游启动失败，未继续"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "上游启动失败，未继续",
                ),
            ],
        ),
        &logs,
    );
    let workbench = fallback_workbench_snapshot(&logs);
    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(mut guard) = state.lock()
    {
        guard.startup = startup.clone();
        guard.workbench = workbench.clone();
    }
    emit_service_snapshots(app, &startup, &workbench);
}

/// Emits startup and workbench snapshots to subscribed WebUI clients.
/// 向已订阅的 WebUI 客户端发送启动与工作台快照。
fn emit_service_snapshots(
    app: &tauri::AppHandle,
    startup: &StartupSnapshot,
    workbench: &WorkbenchSnapshot,
) {
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Startup(Box::new(startup.clone())),
    );
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
}

/// Builds a startup snapshot for one phase.
/// 为单个阶段构建启动快照。
fn startup_snapshot_with_phase(
    phase: &str,
    steps: Vec<StartupStepSnapshot>,
    logs: &[String],
) -> StartupSnapshot {
    StartupSnapshot {
        ready: phase == "ready",
        phase: phase.to_string(),
        steps: if steps.is_empty() {
            startup_steps_with_status(
                OffsetDateTime::now_utc(),
                &[
                    (
                        "service-process",
                        StartupStepStatus::Pending,
                        "等待 Tauri 本地服务桥接启动",
                    ),
                    ("vldb", StartupStepStatus::Pending, "等待初始化"),
                    ("luaskills", StartupStepStatus::Pending, "等待初始化"),
                    ("token-budget", StartupStepStatus::Pending, "等待初始化"),
                    ("mcp", StartupStepStatus::Pending, "等待初始化"),
                    ("workspace", StartupStepStatus::Pending, "等待初始化"),
                ],
            )
        } else {
            steps
        },
        recent_logs: logs.to_vec(),
    }
}

/// Builds ordered startup steps from compact tuples.
/// 从紧凑元组构建有序启动步骤。
fn startup_steps_with_status(
    updated_at: OffsetDateTime,
    compact_steps: &[(&str, StartupStepStatus, &str)],
) -> Vec<StartupStepSnapshot> {
    compact_steps
        .iter()
        .map(|(id, status, message)| StartupStepSnapshot {
            id: (*id).to_string(),
            label: startup_step_label(id),
            status: status.clone(),
            message: Some((*message).to_string()),
            updated_at: Some(updated_at),
        })
        .collect()
}

/// Resolves the user-facing label for one startup step id.
/// 解析单个启动步骤标识对应的用户可见名称。
fn startup_step_label(id: &str) -> String {
    match id {
        "service-process" => "Core Service",
        "vldb" => "VLDB",
        "luaskills" => "LuaSkills",
        "token-budget" => "Token Budget",
        "mcp" => "MCP Runtime",
        "workspace" => "Workspace",
        _ => id,
    }
    .to_string()
}

/// Converts runtime state into a WebUI workbench snapshot.
/// 将运行时状态转换为 WebUI 工作台快照。
fn workbench_from_runtime(
    controller: &ControllerRuntimeHandle,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    surface_messages: &[SessionMessageEntry],
    subagent_navigation: Option<SubagentNavigationSnapshot>,
    logs: &[String],
    live_context_budgets: &BTreeMap<SessionId, ContextBudgetSnapshot>,
) -> WorkbenchSnapshot {
    let selected_conversation_id = if workspace.selected_session == SessionId::nil() {
        None
    } else {
        Some(workspace.selected_session.to_string())
    };
    let active_conversation_id = subagent_navigation
        .as_ref()
        .map(|navigation| navigation.active_conversation_id.as_str())
        .or(selected_conversation_id.as_deref());
    let messages = active_conversation_id
        .map(|conversation_id| {
            conversation_messages_from_session(conversation_id, surface_messages)
        })
        .unwrap_or_default();
    let selected_runtime_state = workspace
        .session_runtime_state
        .get(&workspace.selected_session);

    WorkbenchSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        modules: runtime_modules(
            controller,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
        ),
        projects: workspace
            .task_projects
            .iter()
            .map(|project| ProjectSummary {
                id: project.id.clone(),
                name: project.title.clone(),
                path: project.path.clone(),
                pinned: project.pinned,
                icon_pack: project.icon_pack.clone(),
                icon_key: project.icon_key.clone(),
            })
            .collect(),
        expanded_project_ids: workspace.expanded_task_projects.iter().cloned().collect(),
        conversations: workspace
            .sessions
            .iter()
            .filter(|session| session_event_manifest_exists(session))
            .map(|session| {
                let run_phase = workspace
                    .session_run_phases
                    .get(&session.id)
                    .copied()
                    .unwrap_or(SessionRunPhase::Idle);
                let is_running = session.status == SessionState::Running
                    || is_active_session_run_phase(run_phase);
                ConversationSummary {
                    id: session.id.to_string(),
                    title: session.title.clone(),
                    status: if is_running {
                        SessionState::Running.label()
                    } else {
                        session.status.label()
                    }
                    .to_string(),
                    run_phase: run_phase.to_string(),
                    last_updated_label: session.last_updated_label.clone(),
                    last_updated_at: session.updated_at.clone(),
                    unread: workspace
                        .session_runtime_state
                        .get(&session.id)
                        .is_some_and(|runtime_state| runtime_state.ui_state.unread),
                    project_id: session.project_id.clone(),
                }
            })
            .collect(),
        selected_conversation_id,
        messages,
        subagent_navigation,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: plugins_from_luaskills(luaskills_catalog),
        settings: settings(),
        providers: providers(provider_configuration),
        reasoning_selection: reasoning_selection(provider_configuration),
        ai_capabilities: ai_capabilities(ai_tool_configuration),
        composer_attachment_capabilities: composer_attachment_capabilities(
            provider_configuration,
            ai_tool_configuration,
        ),
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: selected_runtime_state
            .map(|runtime_state| {
                runtime_state
                    .ui_state
                    .pending_prompt_panel_queue
                    .iter()
                    .map(pending_prompt_summary_from_panel_entry)
                    .collect()
            })
            .unwrap_or_default(),
        memories: memories(workspace),
        command_sessions: command_sessions(controller),
        logs: logs
            .iter()
            .enumerate()
            .map(|(index, message)| {
                log_entry(&format!("startup-{index}"), "info", "service", message)
            })
            .chain(
                mcp_catalog
                    .summary_rows
                    .iter()
                    .enumerate()
                    .map(|(index, message)| {
                        log_entry(&format!("mcp-{index}"), "info", "mcp", message)
                    }),
            )
            .collect(),
        context_injection: context_injection_snapshot_from_runtime_state(selected_runtime_state),
        context_usage: context_usage_snapshot_from_runtime_state(
            workspace.selected_session,
            selected_runtime_state,
            live_context_budgets,
        ),
    }
}

/// Converts one persisted visible pending-prompt entry into a WebUI summary.
/// 将单条持久化可见待发送提示词转换为 WebUI 摘要。
fn pending_prompt_summary_from_panel_entry(
    entry: &PendingPromptPanelEntry,
) -> PendingPromptSummary {
    PendingPromptSummary {
        id: entry.id.clone(),
        prompt: entry.prompt.clone(),
        preview: entry.preview.clone(),
        attachments: entry
            .attachments
            .iter()
            .map(composer_attachment_summary_from_core)
            .collect(),
        state: match entry.state {
            PendingPromptPanelEntryState::Ready => String::from("ready"),
            PendingPromptPanelEntryState::Inserted => String::from("inserted"),
        },
        created_at: entry.created_at.clone(),
    }
}

/// Builds the selected-session context injection side-panel snapshot.
/// 构建当前选中会话的上下文注入侧面板快照。
fn context_injection_snapshot_from_runtime_state(
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Option<ContextInjectionSnapshot> {
    let runtime_state = runtime_state?;
    let dynamic_cards = context_injection_cards_to_summary(context_injection_panel_cards_for_kind(
        &runtime_state.context_injection_state,
        Some(InjectionKind::Transient),
    ));
    let mut static_cards = Vec::new();
    if let Some(profile_card) = static_vmm_profile_panel_card(
        runtime_state
            .vmm_profile_sync
            .base_profile_content
            .as_deref(),
        runtime_state.vmm_profile_sync.updated_at.as_deref(),
    ) {
        static_cards.push(profile_card);
    }
    static_cards.extend(context_injection_cards_to_summary(
        context_injection_panel_cards_for_kind(
            &runtime_state.context_injection_state,
            Some(InjectionKind::Persistent),
        ),
    ));

    Some(ContextInjectionSnapshot {
        dynamic_cards,
        static_cards,
    })
}

/// Converts shared injection panel cards into service API DTOs.
/// 将共享注入面板卡片转换为 service API DTO。
fn context_injection_cards_to_summary(
    cards: Vec<InjectionPanelCard>,
) -> Vec<ContextInjectionCardSummary> {
    cards
        .into_iter()
        .map(|card| ContextInjectionCardSummary {
            tag: card.tag,
            description: card.description,
            items: card
                .items
                .into_iter()
                .map(|item| ContextInjectionItemSummary {
                    content: item.content,
                    detail: item.detail,
                    turn_label: item.turn_label,
                    ttl_label: item.ttl_label,
                    source_display: item.source_display,
                })
                .collect(),
        })
        .collect()
}

/// Builds the static VMM profile card aligned with the iced side panel.
/// 构建与 iced 侧面板一致的静态 VMM 画像卡片。
fn static_vmm_profile_panel_card(
    content: Option<&str>,
    updated_at: Option<&str>,
) -> Option<ContextInjectionCardSummary> {
    let content = content
        .map(str::trim)
        .filter(|content| !content.is_empty())?;
    let mut detail_lines = Vec::new();
    if let Some(updated_at) = updated_at.map(str::trim).filter(|value| !value.is_empty()) {
        detail_lines.push(format!("更新时间：{updated_at}"));
    }
    detail_lines.push(String::from("来源：VMM profile system hook"));
    detail_lines.push(String::from("内容："));
    detail_lines.push(content.to_string());

    Some(ContextInjectionCardSummary {
        tag: String::from("vmm_profile"),
        description: String::from(
            "VMM 画像基点。该静态上下文由系统提示词 hook 持续复用，面板仅用于查看。",
        ),
        items: vec![ContextInjectionItemSummary {
            content: compact_panel_preview(content),
            detail: detail_lines.join("\n"),
            turn_label: None,
            ttl_label: String::from("静态"),
            source_display: Some(String::from("VMM profile")),
        }],
    })
}

/// Builds the same compact multi-line preview used by the iced panel.
/// 构建与 iced 面板一致的多行紧凑预览。
fn compact_panel_preview(content: &str) -> String {
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(7)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.len() > 6 {
        lines.truncate(6);
        lines.push(String::from("..."));
    }
    lines.join("\n")
}

/// Builds the selected-session context usage orb snapshot.
/// 构建当前选中会话的上下文占用圆盘快照。
fn context_usage_snapshot_from_runtime_state(
    session_id: SessionId,
    runtime_state: Option<&SessionRuntimeStateEntry>,
    live_context_budgets: &BTreeMap<SessionId, ContextBudgetSnapshot>,
) -> Option<ContextUsageSnapshot> {
    let runtime_state = runtime_state?;
    let context_budget = live_context_budgets.get(&session_id);
    let mut tooltip_rows = Vec::new();
    if let Some(context_budget) = context_budget {
        tooltip_rows.push(format!(
            "输入占用：{:.0}% · {}",
            context_budget.usage_percent(),
            context_budget_severity_label(context_budget.severity),
        ));
        tooltip_rows.push(format!(
            "输入窗口（API实测/本地估算）：{}",
            context_budget.usage_label()
        ));
        tooltip_rows.extend(context_budget_breakdown_rows(context_budget));
        tooltip_rows.push(format!(
            "预留：输出 {} · 推理 {} · 压缩 {}",
            context_budget.reserved_output_tokens,
            context_budget.reserved_reasoning_tokens,
            context_budget.reserved_compaction_tokens
        ));
        if !context_budget.baseline_matches_active_runtime {
            let reason = context_budget
                .baseline_mismatch_reason
                .as_deref()
                .unwrap_or("当前运行时上下文与压缩锚点不一致");
            tooltip_rows.push(format!("恢复：基线失配（{reason}），已回退完整上下文。"));
        }
        if let Some(usage) = context_budget.observed_usage.as_ref() {
            tooltip_rows.push(format!("最近 API：{}", provider_usage_label(usage)));
        }
    } else {
        tooltip_rows.push(String::from("请求拆分：等待预算引擎计算"));
    }

    tooltip_rows.push(if runtime_state.last_compaction_id.is_some() {
        String::from("压缩状态：已使用摘要前文")
    } else {
        String::from("压缩状态：尚未触发")
    });

    Some(ContextUsageSnapshot {
        percent: context_budget
            .map(|budget| budget.usage_percent().round().clamp(0.0, 100.0) as u8)
            .unwrap_or(0),
        severity: context_budget
            .map(context_budget_severity_key)
            .unwrap_or_else(|| String::from("unknown")),
        baseline_matches_active_runtime: context_budget
            .map(|budget| budget.baseline_matches_active_runtime)
            .unwrap_or(true),
        tooltip_title: String::from("上下文占用"),
        tooltip_rows,
    })
}

/// Builds the context-budget token breakdown rows used by the tooltip.
/// 构建提示浮层使用的上下文预算 Token 拆分行。
fn context_budget_breakdown_rows(context_budget: &ContextBudgetSnapshot) -> Vec<String> {
    let raw_parts = [
        ("系统", context_budget.estimated_system_tokens),
        ("历史", context_budget.estimated_history_tokens),
        ("当前", context_budget.estimated_prompt_tokens),
        ("工具", context_budget.estimated_tool_tokens),
    ];
    let raw_total: usize = raw_parts.iter().map(|(_, value)| *value).sum();
    let prediction_row = format!(
        "预测拆分（预算器）：{}",
        raw_parts
            .iter()
            .map(|(label, value)| format!("{label} {value}"))
            .collect::<Vec<_>>()
            .join(" / ")
    );

    let Some(observed_input) =
        vulcan_context::observed_input_tokens_from_usage(context_budget.observed_usage.as_ref())
    else {
        return vec![prediction_row];
    };
    if raw_total == 0 {
        return vec![prediction_row];
    }
    let calibrated = raw_parts
        .iter()
        .map(|(label, value)| {
            let scaled = ((*value as f64 / raw_total as f64) * observed_input as f64).round();
            format!("{label} {}", scaled as usize)
        })
        .collect::<Vec<_>>()
        .join(" / ");
    vec![
        String::from("说明：API实测为准；拆分已按实测输入比例校准。"),
        prediction_row,
        format!("校准拆分（实测）：{calibrated}"),
    ]
}

/// Returns the stable context-budget severity key consumed by WebUI CSS.
/// 返回 WebUI CSS 消费的稳定上下文预算严重级别键。
fn context_budget_severity_key(context_budget: &ContextBudgetSnapshot) -> String {
    if !context_budget.baseline_matches_active_runtime {
        return String::from("baseline-mismatch");
    }
    match context_budget.severity {
        ContextBudgetSeverity::Healthy => "healthy",
        ContextBudgetSeverity::Warning => "warning",
        ContextBudgetSeverity::CompactSoon => "compact-soon",
        ContextBudgetSeverity::Overflow => "overflow",
    }
    .to_string()
}

/// Returns the user-facing context-budget severity label.
/// 返回面向用户的上下文预算严重级别标签。
fn context_budget_severity_label(severity: ContextBudgetSeverity) -> &'static str {
    match severity {
        ContextBudgetSeverity::Healthy => "正常",
        ContextBudgetSeverity::Warning => "预警",
        ContextBudgetSeverity::CompactSoon => "建议压缩",
        ContextBudgetSeverity::Overflow => "预算溢出",
    }
}

/// Formats one provider usage row for the context usage tooltip.
/// 为上下文占用提示浮层格式化单条 Provider 用量行。
fn provider_usage_label(usage: &vulcan_core::types::ProviderUsageSnapshot) -> String {
    let mut parts = Vec::new();
    if let Some(input) = usage.input_tokens {
        parts.push(format!("输入 {input}"));
    }
    if let Some(output) = usage.output_tokens {
        parts.push(format!("输出 {output}"));
    }
    if let Some(reasoning) = usage.reasoning_tokens {
        parts.push(format!("推理 {reasoning}"));
    }
    if let Some(total) = usage.total_tokens {
        parts.push(format!("总计 {total}"));
    }
    if let Some(cached) = usage.cached_input_tokens {
        parts.push(format!("缓存命中 {cached}"));
    }
    if let Some(created) = usage.cache_creation_input_tokens {
        parts.push(format!("缓存写入 {created}"));
    }
    if parts.is_empty() {
        String::from("无可用 Token 数据")
    } else {
        parts.join(" / ")
    }
}

/// Converts session transcript entries into WebUI messages.
/// 将会话转录项转换为 WebUI 消息。
fn conversation_messages_from_session(
    conversation_id: &str,
    messages: &[SessionMessageEntry],
) -> Vec<ConversationMessage> {
    messages
        .iter()
        .filter(|message| message.is_visible_to_user())
        .filter(|message| !message.body.trim().is_empty())
        .map(|message| ConversationMessage {
            id: if message.id.is_empty() {
                format!("{}-{}", conversation_id, message.created_at)
            } else {
                message.id.clone()
            },
            conversation_id: conversation_id.to_string(),
            role: match message.kind {
                SessionMessageKind::User => MessageRole::User,
                SessionMessageKind::Assistant => MessageRole::Assistant,
                SessionMessageKind::Error => MessageRole::System,
            },
            title: non_empty_string(&message.title),
            record_kind: Some(message.record_kind.as_persisted_key().to_string()),
            body: message.body.clone(),
            metadata: message.metadata.clone(),
            attachments: composer_message_attachments_from_metadata(message.metadata.as_ref()),
        })
        .collect()
}

/// Extracts composer attachments from durable transcript metadata.
/// 从持久转录元数据中提取输入区附件。
fn composer_message_attachments_from_metadata(
    metadata: Option<&serde_json::Value>,
) -> Vec<ComposerAttachmentSummary> {
    let Some(metadata) = metadata else {
        return Vec::new();
    };
    let Some(attachment_value) = metadata
        .get("attachments")
        .or_else(|| metadata.get("transcript_attachments"))
    else {
        return Vec::new();
    };
    serde_json::from_value::<Vec<ConversationAttachment>>(attachment_value.clone())
        .unwrap_or_default()
        .iter()
        .map(composer_attachment_summary_from_core)
        .collect()
}

/// Returns whether a persisted session has a readable event-store manifest on disk.
/// 返回持久化会话在磁盘上是否存在可读取的事件存储 manifest。
fn session_event_manifest_exists(session: &vulcan_workbench_model::SessionSummary) -> bool {
    let Some(project_id) = session.project_id.as_deref() else {
        return false;
    };
    user_vulcan_code_sessions_dir()
        .join(sanitize_session_store_component(project_id))
        .join(session.id.to_string())
        .join("manifest.json")
        .is_file()
}

/// Sanitizes one session-store path component with the same rules as the workspace event store.
/// 使用与工作区事件存储一致的规则清洗一个会话存储路径片段。
fn sanitize_session_store_component(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::from("_");
    }
    trimmed
        .chars()
        .map(|character| match character {
            ':' | '\\' | '/' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => character,
        })
        .collect()
}

/// Returns a trimmed string only when it contains visible content.
/// 仅当字符串包含可见内容时返回裁剪后的字符串。
fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Builds a fallback workbench snapshot used before real startup completes or after failure.
/// 构建真实启动完成前或失败后的回退工作台快照。
fn fallback_workbench_snapshot(logs: &[String]) -> WorkbenchSnapshot {
    let workspace_root = workspace_root();
    WorkbenchSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        modules: vec![
            runtime_module(
                "core",
                "Core Service",
                StartupStepStatus::Running,
                "服务正在启动",
            ),
            runtime_module(
                "vldb",
                "VLDB",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "luaskills",
                "LuaSkills",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "token-budget",
                "Token Budget",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "mcp",
                "MCP Runtime",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "workspace",
                "Workspace",
                StartupStepStatus::Pending,
                "等待真实工作区快照",
            ),
        ],
        projects: vec![ProjectSummary {
            id: String::from("current-workspace"),
            name: String::from("vulcan-code"),
            path: workspace_root.display().to_string(),
            pinned: false,
            icon_pack: None,
            icon_key: None,
        }],
        expanded_project_ids: vec![String::from("current-workspace")],
        conversations: Vec::new(),
        selected_conversation_id: None,
        messages: Vec::new(),
        subagent_navigation: None,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: Vec::new(),
        settings: settings(),
        providers: providers(&ProviderConfigurationFile::default()),
        reasoning_selection: reasoning_selection(&ProviderConfigurationFile::default()),
        ai_capabilities: ai_capabilities(&ProviderConfigurationFile::default().ai_tools),
        composer_attachment_capabilities: composer_attachment_capabilities(
            &ProviderConfigurationFile::default(),
            &ProviderConfigurationFile::default().ai_tools,
        ),
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: Vec::new(),
        memories: Vec::new(),
        command_sessions: Vec::new(),
        logs: logs
            .iter()
            .enumerate()
            .map(|(index, message)| {
                log_entry(&format!("fallback-{index}"), "info", "service", message)
            })
            .collect(),
        context_injection: None,
        context_usage: None,
    }
}

/// Creates feature areas matching the legacy GUI navigation surface.
/// 创建与旧版 GUI 导航面相匹配的功能区域。
fn feature_areas() -> Vec<FeatureAreaSummary> {
    vec![
        feature_area("chat", "控制台", "控制台、消息流与模型输入区", "真实快照"),
        feature_area("projects", "项目", "工作区项目列表与绑定状态", "真实快照"),
        feature_area(
            "providers",
            "模型",
            "Provider、模型、能力配置",
            "待接入配置详情",
        ),
        feature_area("tools", "工具", "工具调用、命令会话与输出", "待接入运行态"),
        feature_area(
            "plugins",
            "插件",
            "LuaSkills 与高级插件控制面板",
            "真实目录",
        ),
        feature_area(
            "memory",
            "VMM 记忆",
            "用户、项目、画像与记忆节点",
            "会话绑定快照",
        ),
        feature_area(
            "settings",
            "设置",
            "运行时、界面、权限与远程访问设置",
            "基础区域",
        ),
        feature_area("logs", "诊断", "启动日志、错误与运行时事件", "真实启动日志"),
    ]
}

/// Creates one feature area summary.
/// 创建单个功能区域摘要。
fn feature_area(id: &str, label: &str, description: &str, status: &str) -> FeatureAreaSummary {
    FeatureAreaSummary {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        status: status.to_string(),
    }
}

/// Converts LuaSkills catalog rows into plugin summaries.
/// 将 LuaSkills 目录行转换为插件摘要。
fn plugins_from_luaskills(catalog: &TauriLuaSkillsCatalog) -> Vec<PluginSummary> {
    catalog
        .skills
        .iter()
        .map(|skill| PluginSummary {
            id: skill.skill_id.clone(),
            name: skill.display_name.clone(),
            kind: format!("LuaSkill / {}", skill.root_name),
            status: if skill.enabled {
                "已启用"
            } else {
                "已停用"
            }
            .to_string(),
            capability: format!("{} 个入口：{}", skill.entry_count, skill.capability),
        })
        .collect()
}

/// Creates first-version settings sections.
/// 创建第一版设置分组。
fn settings() -> Vec<SettingSectionSummary> {
    vec![
        setting_section(
            "runtime",
            "运行时",
            "Core Service、LuaSkills、VLDB、MCP 的启动与状态设置",
            &["启动模式", "运行时目录", "MCP 工具策略", "VLDB 控制器"],
        ),
        setting_section(
            "providers",
            "模型与 Provider",
            "模型供应商、默认模型、工具能力与推理配置",
            &["默认 Provider", "默认模型", "工具调用", "推理强度"],
        ),
        setting_section(
            "plugins",
            "插件",
            "LuaSkills、Rust lib 插件、WebUI 控制面板与权限",
            &["插件目录", "启用状态", "权限声明", "WebUI 面板"],
        ),
        setting_section(
            "remote",
            "远程访问",
            "浏览器、手机、远程客户端访问 Core Service 的设置",
            &["监听地址", "访问令牌", "远程开关", "审计日志"],
        ),
    ]
}

/// Creates one settings section summary.
/// 创建单个设置分组摘要。
fn setting_section(
    id: &str,
    label: &str,
    description: &str,
    items: &[&str],
) -> SettingSectionSummary {
    SettingSectionSummary {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        items: items.iter().map(|item| item.to_string()).collect(),
    }
}

/// Creates provider summaries from the local catalog surface.
/// 从本地目录面创建 Provider 摘要。
fn providers(provider_configuration: &ProviderConfigurationFile) -> Vec<ProviderSummary> {
    let active_model_reference = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
        .map(|(provider_id, model_id)| (provider_id.to_string(), model_id.to_string()));
    let mut configured_providers = provider_config::supported_provider_choices()
        .into_iter()
        .filter(|provider_choice| {
            provider_configuration
                .provider
                .contains_key(&provider_choice.id)
        })
        .map(|provider_choice| {
            let models = provider_config::effective_model_choices(
                &provider_choice.id,
                provider_configuration,
            )
            .into_iter()
            .map(|model_choice| {
                let active =
                    active_model_reference
                        .as_ref()
                        .is_some_and(|(provider_id, model_id)| {
                            provider_id == &provider_choice.id && model_id == &model_choice.id
                        });
                ProviderModelSummary {
                    id: model_choice.id,
                    name: model_choice.name,
                    active,
                }
            })
            .collect::<Vec<_>>();
            let active = models.iter().any(|model| model.active);
            ProviderSummary {
                id: provider_choice.id,
                name: provider_choice.name,
                status: String::from("已配置"),
                enabled_models: format!("{} 个已启用模型", models.len()),
                active,
                models,
            }
        })
        .collect::<Vec<_>>();
    configured_providers.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    if !configured_providers.is_empty() {
        return configured_providers;
    }

    let provider_root = vulcan_runtime_assets::resolve_provider_catalog_directory();
    vec![ProviderSummary {
        id: String::from("provider-catalog"),
        name: String::from("Provider Catalog"),
        status: if provider_root.is_dir() {
            "已载入"
        } else {
            "未找到"
        }
        .to_string(),
        enabled_models: provider_root.display().to_string(),
        active: false,
        models: Vec::new(),
    }]
}

/// Creates composer AI capability summaries from the shared provider configuration.
/// 从共享 Provider 配置创建输入区 AI 能力摘要。
///
/// # Parameters
/// 参数
/// `configuration` is the persisted AI tool configuration.
/// `configuration` 是持久化的 AI 工具配置。
///
/// # Returns
/// 返回值
/// Ordered AI capability summaries matching the iced workbench quick-toggle surface.
/// 与 iced 工作台快捷开关界面顺序一致的 AI 能力摘要。
fn ai_capabilities(
    configuration: &vulcan_provider_config::config::PersistedAiToolConfiguration,
) -> Vec<AiCapabilitySummary> {
    WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS
        .into_iter()
        .map(|kind| {
            let targets = configuration
                .resolved_configured_targets_for_kind(kind)
                .into_iter()
                .map(|target| AiCapabilityTargetSummary {
                    target_selector: target.target_selector,
                    vendor_id: target.vendor_id,
                    vendor_name: target.vendor_name,
                    target_alias: target.target_alias,
                    target_name: target.target_name,
                    model_id: target.model_id,
                    enabled: target.enabled,
                })
                .collect::<Vec<_>>();
            let available = configuration.has_usable_targets_for_kind(kind);
            AiCapabilitySummary {
                kind: kind.key().to_string(),
                label: kind.label().to_string(),
                available,
                enabled: available && !configuration.disabled_capability_kinds.contains(&kind),
                targets,
            }
        })
        .collect()
}

/// Creates composer attachment gates using the same capability resolver as runtime execution.
/// 使用与运行时执行相同的能力解析器创建输入区附件门控。
fn composer_attachment_capabilities(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Vec<ComposerAttachmentCapabilitySummary> {
    let capabilities =
        active_workbench_model_capabilities(provider_configuration, ai_tool_configuration);
    [
        ConversationAttachmentKind::Image,
        ConversationAttachmentKind::Video,
        ConversationAttachmentKind::Audio,
        ConversationAttachmentKind::Pdf,
    ]
    .into_iter()
    .map(|kind| {
        let native = capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.supports_attachment_kind(kind.clone()));
        let enabled = capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.allows_staged_attachment_kind(kind.clone()));
        ComposerAttachmentCapabilitySummary {
            kind: kind.modality_name().to_string(),
            label: composer_attachment_kind_label(&kind).to_string(),
            enabled,
            native,
            bridged: enabled && !native,
            accept: composer_attachment_kind_accept(&kind).to_string(),
        }
    })
    .collect()
}

/// Resolves the active workbench model capabilities from persisted provider settings.
/// 从持久化 Provider 设置解析当前工作台模型能力。
fn active_workbench_model_capabilities(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ResolvedModelCapabilities> {
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let active_model =
        provider_config::effective_model_choices(provider_id, provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    Some(resolve_effective_model_capabilities(
        definition,
        active_model.as_ref(),
        ai_tool_configuration,
    ))
}

/// Creates VMM memory summaries from restored workspace state.
/// 从已恢复工作区状态创建 VMM 记忆摘要。
fn memories(
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> Vec<MemorySummary> {
    vec![
        MemorySummary {
            id: String::from("projects"),
            title: String::from("项目绑定"),
            category: String::from("Project"),
            state: format!("已恢复 {} 个项目", workspace.task_projects.len()),
        },
        MemorySummary {
            id: String::from("sessions"),
            title: String::from("会话绑定"),
            category: String::from("Conversation"),
            state: format!(
                "已恢复 {} 个会话运行态",
                workspace.session_runtime_state.len()
            ),
        },
    ]
}

/// Creates basic tool call summaries for service bridge commands.
/// 为 service bridge 命令创建基础工具调用摘要。
fn tool_calls() -> Vec<ToolCallSummary> {
    vec![
        ToolCallSummary {
            id: String::from("service-startup"),
            name: String::from("service_startup_snapshot"),
            status: String::from("ready"),
            detail: String::from("WebUI 可读取真实启动阶段快照。"),
        },
        ToolCallSummary {
            id: String::from("service-workbench"),
            name: String::from("service_workbench_snapshot"),
            status: String::from("ready"),
            detail: String::from("WebUI 可读取真实项目、会话、插件与当前会话正文。"),
        },
    ]
}

/// Creates command-session summaries for currently retained service backends.
/// 为当前保留的服务后端创建命令会话摘要。
fn command_sessions(controller: &ControllerRuntimeHandle) -> Vec<CommandSessionSummary> {
    vec![CommandSessionSummary {
        id: String::from("vldb-controller"),
        command: controller.executable.clone(),
        status: String::from("running"),
        preview: format!(
            "endpoint={} sqlite={}",
            controller.endpoint,
            controller.sqlite_path.display()
        ),
    }]
}

/// Creates runtime module cards shown by the dashboard.
/// 创建仪表盘展示的运行时模块卡片。
fn runtime_modules(
    controller: &ControllerRuntimeHandle,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
) -> Vec<RuntimeModuleSnapshot> {
    vec![
        runtime_module(
            "core",
            "Core Service",
            StartupStepStatus::Ready,
            "Tauri 本地服务运行中",
        ),
        runtime_module(
            "vldb",
            "VLDB",
            StartupStepStatus::Ready,
            &format!(
                "{} · {}",
                controller.endpoint,
                controller.sqlite_path.display()
            ),
        ),
        runtime_module(
            "luaskills",
            "LuaSkills",
            StartupStepStatus::Ready,
            &format!(
                "{} · 技能 {} · 入口 {}",
                luaskills_catalog.runtime_root.display(),
                luaskills_catalog.skills.len(),
                luaskills_catalog.entry_count
            ),
        ),
        runtime_module(
            "token-budget",
            "Token Budget",
            token_budget_status(token_budget_runtime),
            token_budget_runtime.status_row().as_str(),
        ),
        runtime_module(
            "mcp",
            "MCP Runtime",
            if mcp_catalog.enabled_server_count == 0 || mcp_catalog.connected_server_count > 0 {
                StartupStepStatus::Ready
            } else {
                StartupStepStatus::Warning
            },
            &format!(
                "server {}/{} · connected {} · tools {} · prompts {} · resources {}",
                mcp_catalog.enabled_server_count,
                mcp_catalog.configured_server_count,
                mcp_catalog.connected_server_count,
                mcp_catalog.tool_count,
                mcp_catalog.prompt_count,
                mcp_catalog.resource_count
            ),
        ),
        runtime_module(
            "workspace",
            "Workspace",
            StartupStepStatus::Ready,
            "项目与会话快照已恢复",
        ),
    ]
}

/// Resolves the startup status for token budget runtime diagnostics.
/// 解析 Token 预算运行时诊断对应的启动状态。
fn token_budget_status(token_budget_runtime: &TauriTokenBudgetRuntime) -> StartupStepStatus {
    if token_budget_runtime.is_available() {
        StartupStepStatus::Ready
    } else {
        StartupStepStatus::Warning
    }
}

/// Creates one runtime module snapshot.
/// 创建单个运行时模块快照。
fn runtime_module(
    id: &str,
    label: &str,
    status: StartupStepStatus,
    message: &str,
) -> RuntimeModuleSnapshot {
    RuntimeModuleSnapshot {
        id: id.to_string(),
        label: label.to_string(),
        status,
        message: message.to_string(),
    }
}

/// Appends one WebUI-visible service diagnostic message to the retained startup log buffer.
/// 向保留的启动日志缓冲区追加一条 WebUI 可见的服务诊断消息。
///
/// # Parameters
/// 参数
/// `state` owns the retained service snapshot.
/// `state` 持有保留的服务快照。
/// `message` is the user-visible diagnostic text.
/// `message` 是用户可见的诊断文本。
///
/// # Returns
/// 返回值
/// `Ok(())` after the diagnostic has been retained.
/// 诊断信息完成保留后返回 `Ok(())`。
fn push_webui_service_diagnostic(
    state: &SharedServiceBridgeState,
    message: String,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard.startup.recent_logs.push(message);
    if guard.startup.recent_logs.len() > 200 {
        let overflow = guard.startup.recent_logs.len().saturating_sub(200);
        guard.startup.recent_logs.drain(0..overflow);
    }
    Ok(())
}

/// Creates one diagnostic log entry.
/// 创建单条诊断日志。
fn log_entry(id: &str, level: &str, source: &str, message: &str) -> LogEntrySummary {
    LogEntrySummary {
        id: id.to_string(),
        level: level.to_string(),
        source: source.to_string(),
        message: message.to_string(),
    }
}

/// Returns the repository root used by the local service bridge.
/// 返回本地服务桥接层使用的仓库根目录。
fn workspace_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Opens one native folder-picker dialog and returns the selected path when confirmed.
/// 打开一个原生文件夹选择器，并在确认后返回选中的路径。
fn pick_task_project_folder(initial_directory: &str) -> Result<Option<PathBuf>, String> {
    let mut dialog = rfd::FileDialog::new().set_title("选择要添加的项目文件夹");
    let initial_path = Path::new(initial_directory);
    if initial_path.exists() {
        dialog = dialog.set_directory(initial_path);
    }
    Ok(dialog.pick_folder())
}

/// Normalizes one project path by resolving it when the host filesystem allows it.
/// 在宿主文件系统允许时解析并规范化单个项目路径。
fn normalize_task_project_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Converts one filesystem path into a stable native display and persistence string.
/// 将单个文件系统路径转换成稳定的原生展示与持久化字符串。
fn normalized_workspace_path_text_from_path(path: &Path) -> String {
    normalize_workspace_path_text(&path.to_string_lossy())
}

/// Normalizes one raw workspace path string and strips Windows verbatim prefixes.
/// 规范化单个原始工作区路径字符串，并移除 Windows verbatim 前缀。
fn normalize_workspace_path_text(path: &str) -> String {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return String::new();
    }
    let path_without_verbatim_prefix = strip_windows_verbatim_prefix(trimmed_path);
    if looks_like_windows_path(trimmed_path, &path_without_verbatim_prefix) {
        path_without_verbatim_prefix.replace('/', "\\")
    } else {
        path_without_verbatim_prefix
    }
}

/// Returns whether one path should be displayed with Windows separators.
/// 返回单个路径是否应使用 Windows 分隔符显示。
fn looks_like_windows_path(original_path: &str, normalized_path: &str) -> bool {
    let bytes = normalized_path.as_bytes();
    bytes.len() >= 3
        && bytes[1] == b':'
        && bytes[0].is_ascii_alphabetic()
        && matches!(bytes[2], b'/' | b'\\')
        || normalized_path.starts_with(r"\\")
        || original_path.starts_with(r"\\?\UNC\")
        || original_path.starts_with("//?/UNC/")
        || (cfg!(windows) && normalized_path.starts_with("//"))
}

/// Removes Windows verbatim path prefixes while preserving normal addressing semantics.
/// 移除 Windows verbatim 路径前缀，同时保留普通寻址语义。
fn strip_windows_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix("//?/UNC/") {
        return format!("//{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    if let Some(rest) = path.strip_prefix("//?/") {
        return rest.to_string();
    }
    path.to_string()
}

/// Checks whether one project points to the normalized selected path.
/// 检查单个项目是否指向已规范化的选中路径。
fn task_project_matches_path(project: &TaskProjectSummary, normalized_path_text: &str) -> bool {
    normalized_workspace_path_text_from_path(&normalize_task_project_path(Path::new(&project.path)))
        == normalized_path_text
}

/// Builds a stable project id from the title and normalized path while avoiding collisions.
/// 根据标题与规范化路径构建稳定项目标识，并避免冲突。
fn build_task_project_id(
    workspace_snapshot: &WorkspaceSnapshot,
    title: &str,
    normalized_path_text: &str,
) -> String {
    let base_slug = slugify_identifier(title);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized_path_text.hash(&mut hasher);
    let path_suffix = format!("{:x}", hasher.finish());
    let path_suffix = &path_suffix[..path_suffix.len().min(8)];
    let candidate = if base_slug.is_empty() {
        format!("project-{path_suffix}")
    } else {
        format!("{base_slug}-{path_suffix}")
    };

    if !workspace_snapshot
        .task_projects
        .iter()
        .chain(workspace_snapshot.archived_task_projects.iter())
        .any(|project| project.id == candidate)
    {
        return candidate;
    }

    let mut counter = 2usize;
    loop {
        let next_candidate = format!("{candidate}-{counter}");
        if !workspace_snapshot
            .task_projects
            .iter()
            .chain(workspace_snapshot.archived_task_projects.iter())
            .any(|project| project.id == next_candidate)
        {
            return next_candidate;
        }
        counter += 1;
    }
}

/// Converts one label into a compact ASCII identifier component.
/// 将单个标签转换为紧凑的 ASCII 标识片段。
fn slugify_identifier(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

/// Returns the user-level Vulcan Code session event-store directory.
/// 返回用户级 Vulcan Code 会话事件存储目录。
fn user_vulcan_code_sessions_dir() -> PathBuf {
    user_home_dir()
        .join(".vulcan")
        .join("code")
        .join("sessions")
}

/// Returns the current user's home directory using platform environment variables.
/// 使用平台环境变量返回当前用户主目录。
fn user_home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Returns the first existing path from a workspace-relative candidate list.
/// 从工作区相对候选列表中返回第一个存在的路径。
#[allow(dead_code)]
fn first_existing_path(workspace_root: &Path, candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(|candidate| workspace_root.join(candidate))
        .find(|path| path.exists())
}
