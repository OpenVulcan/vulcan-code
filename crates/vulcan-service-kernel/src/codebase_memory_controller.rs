//! Long-lived host controller for the managed multi-project Codebase Memory MCP sidecar.
//! 托管多项目 Codebase Memory MCP Sidecar 的长生命周期宿主控制器。

use super::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::time::Duration;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use vulcan_agent_runtime::mcp::{
    HOST_MANAGED_CODEBASE_MEMORY_PARTITION_KEY, MANAGED_CODEBASE_MEMORY_CONTRACT,
    MANAGED_CODEBASE_MEMORY_PRODUCT, MANAGED_CODEBASE_MEMORY_READ_ONLY_TOOLS,
    MANAGED_CODEBASE_MEMORY_SERVER_ID, ManagedCodebaseMemoryAuthorization,
    ManagedCodebaseMemoryExecutionContext, ManagedCodebaseMemoryProjectRoute,
    McpRuntimeServerStatus, McpRuntimeToolAllowlist, McpServerConfig, McpServerSource,
    McpServerTransportConfig, replace_managed_codebase_memory_mcp_runtime_partition,
};
use vulcan_service_api::CodebaseMemoryProjectSettingsSummary;

/// Hidden control tools required from the Vulcan-specific sidecar contract.
/// Vulcan 专用 Sidecar 契约要求的隐藏控制工具。
const REQUIRED_CONTROL_TOOLS: [&str; 3] = [
    "vulcan_sync_projects",
    "vulcan_project_status",
    "vulcan_reindex_project",
];
/// Relative directory reserved for the managed Codebase Memory distribution.
/// 为托管 Codebase Memory 分发保留的相对目录。
const MANAGED_DISTRIBUTION_DIRECTORY: &str = "codebase-memory";
/// Manifest file that pins the packaged binary identity and digest.
/// 固定打包二进制身份与摘要的清单文件。
const MANAGED_DISTRIBUTION_MANIFEST: &str = "manifest.json";
/// Polling interval used only to detect managed transport reconnect transitions.
/// 仅用于检测托管传输重连转换的轮询间隔。
const MANAGED_RECOVERY_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Host-owned deadline for private control acknowledgements that must not perform indexing inline.
/// 私有控制确认的宿主持有截止时间；这些调用不得内联执行索引。
const MANAGED_CONTROL_TOOL_TIMEOUT: Duration = Duration::from_secs(30);

/// Validated packaged sidecar selection used for launch and initialize identity checks.
/// 用于启动和 initialize 身份校验的已验证打包 Sidecar 选择。
#[derive(Debug, Clone)]
struct ManagedCodebaseMemoryDistribution {
    /// Strict MCP server configuration for the packaged executable.
    /// 打包可执行文件对应的严格 MCP 服务器配置。
    server: McpServerConfig,
    /// Exact binary version expected from initialize.
    /// initialize 必须返回的精确二进制版本。
    version: String,
}

/// Immutable packaged sidecar manifest selected for the current platform.
/// 为当前平台选择的不可变打包 Sidecar 清单。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCodebaseMemoryManifest {
    /// Exact private protocol contract implemented by the binary.
    /// 二进制实现的精确私有协议契约。
    protocol: String,
    /// Exact binary version packaged with Vulcan Code.
    /// 随 Vulcan Code 打包的精确二进制版本。
    version: String,
    /// Platform key selected by the current build.
    /// 当前构建选择的平台键。
    platform: String,
    /// File name relative to the managed distribution directory.
    /// 相对于托管分发目录的文件名。
    executable: String,
    /// Lowercase hexadecimal SHA-256 digest of the executable.
    /// 可执行文件的小写十六进制 SHA-256 摘要。
    sha256: String,
}

/// Mutable per-project diagnostics retained independently from enablement intent.
/// 与启用意图独立保留的可变项目级诊断。
#[derive(Debug, Clone, Default)]
struct ManagedProjectDiagnostic {
    /// Current project indexing state reported by the sidecar.
    /// Sidecar 报告的当前项目索引状态。
    index_status: Option<String>,
    /// Last successful index timestamp reported by the sidecar.
    /// Sidecar 报告的最近成功索引时间。
    last_successful_index_at: Option<String>,
    /// Current index revision reported by the sidecar.
    /// Sidecar 报告的当前索引修订号。
    index_revision: Option<String>,
    /// Latest non-sensitive project error.
    /// 最近一次不含敏感信息的项目错误。
    last_error: Option<String>,
}

/// Strict response returned by the private full-project synchronization tool.
/// 私有全量项目同步工具返回的严格响应。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedProjectSyncResponse {
    /// Echoed private contract version.
    /// 回显的私有契约版本。
    contract: String,
    /// Transaction state after the authoritative full synchronization commits.
    /// 权威全量同步提交后的事务状态。
    status: String,
    /// Echoed synchronization generation.
    /// 回显的同步代次。
    generation: u64,
    /// Number of active projects retained by the committed registry.
    /// 已提交注册表保留的活动项目数量。
    project_count: u64,
    /// Number of projects newly added by this synchronization.
    /// 本次同步新增的项目数量。
    added: u64,
    /// Number of existing projects retained by this synchronization.
    /// 本次同步保留的既有项目数量。
    retained: u64,
    /// Number of projects removed by this synchronization.
    /// 本次同步移除的项目数量。
    removed: u64,
    /// Number of active projects currently reporting failed lifecycle state.
    /// 当前报告失败生命周期状态的活动项目数量。
    failed: u64,
    /// Per-project acceptance outcomes.
    /// 每项目接纳结果。
    projects: Vec<ManagedProjectSyncOutcome>,
}

/// Strict per-project outcome returned by full synchronization.
/// 全量同步返回的严格每项目结果。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedProjectSyncOutcome {
    /// Stable project identity from the request.
    /// 请求中的稳定项目身份。
    project_id: String,
    /// Whether the sidecar accepted this project watcher configuration.
    /// Sidecar 是否接纳该项目 Watcher 配置。
    accepted: bool,
    /// Registry reconciliation result for this project.
    /// 该项目的注册表协调结果。
    change: String,
    /// Current project lifecycle state.
    /// 当前项目生命周期状态。
    lifecycle: String,
    /// Current project index state.
    /// 当前项目索引状态。
    index_status: String,
    /// Whether the project watcher is currently registered.
    /// 当前是否已注册项目 Watcher。
    watcher_registered: bool,
    /// Monotonic successful index revision, or zero before the first success.
    /// 单调递增的成功索引修订号，首次成功前为零。
    index_revision: u64,
    /// Unix epoch milliseconds of the latest successful index, or zero when absent.
    /// 最近成功索引的 Unix 毫秒时间戳，不存在时为零。
    last_success_at_ms: u64,
    /// Stable non-sensitive project error code when available.
    /// 可用时稳定且不含敏感信息的项目错误码。
    error_code: Option<String>,
    /// Non-sensitive project-specific error.
    /// 不含敏感信息的项目级错误。
    error: Option<String>,
}

/// Strict response returned by the private project-status tool.
/// 私有项目状态工具返回的严格响应。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedProjectStatusResponse {
    /// Echoed private contract version.
    /// 回显的私有契约版本。
    contract: String,
    /// Stable project identity from the request.
    /// 请求中的稳定项目身份。
    project_id: String,
    /// Current project lifecycle state.
    /// 当前项目生命周期状态。
    lifecycle: String,
    /// Current project index state.
    /// 当前项目索引状态。
    index_status: String,
    /// Whether the project watcher is currently registered.
    /// 当前是否已注册项目 Watcher。
    watcher_registered: bool,
    /// Monotonic successful index revision, or zero before the first success.
    /// 单调递增的成功索引修订号，首次成功前为零。
    index_revision: u64,
    /// Unix epoch milliseconds of the latest successful index, or zero when absent.
    /// 最近成功索引的 Unix 毫秒时间戳，不存在时为零。
    last_success_at_ms: u64,
    /// Current indexed node count.
    /// 当前已索引节点数量。
    #[serde(rename = "nodes")]
    _nodes: u64,
    /// Current indexed edge count.
    /// 当前已索引边数量。
    #[serde(rename = "edges")]
    _edges: u64,
    /// Stable non-sensitive project error code when available.
    /// 可用时稳定且不含敏感信息的项目错误码。
    error_code: Option<String>,
    /// Non-sensitive project-specific error.
    /// 不含敏感信息的项目级错误。
    error: Option<String>,
}

/// Strict acknowledgement returned by the private project-reindex tool.
/// 私有项目重建索引工具返回的严格确认。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedProjectReindexResponse {
    /// Echoed private contract version.
    /// 回显的私有契约版本。
    contract: String,
    /// Stable project identity from the request.
    /// 请求中的稳定项目身份。
    project_id: String,
    /// Whether the sidecar accepted the non-destructive reindex request.
    /// Sidecar 是否接纳非破坏性的重建索引请求。
    accepted: bool,
    /// Scheduling state after accepting the request.
    /// 接纳请求后的调度状态。
    status: String,
    /// Whether the request joined an already running reindex operation.
    /// 请求是否合并进已经运行的重建索引操作。
    coalesced: bool,
}

/// Converts an optional managed Unix-millisecond timestamp into the API's RFC 3339 text.
/// 将可选的托管 Unix 毫秒时间戳转换为 API 使用的 RFC 3339 文本。
///
/// # Parameters
/// 参数
/// `value` is zero when no successful index exists, otherwise Unix epoch milliseconds.
/// `value` 在尚无成功索引时为零，否则表示 Unix 纪元毫秒数。
///
/// # Returns
/// 返回值
/// Returns no timestamp for zero, a formatted timestamp for valid values, or a protocol error.
/// 零值返回无时间戳，有效值返回格式化时间戳，无效值返回协议错误。
fn managed_success_timestamp_text(value: u64) -> Result<Option<String>, String> {
    if value == 0 {
        return Ok(None);
    }
    let timestamp = OffsetDateTime::from_unix_timestamp_nanos(i128::from(value) * 1_000_000)
        .map_err(|error| {
            format!("Codebase Memory returned an invalid successful-index timestamp: {error}")
        })?;
    timestamp.format(&Rfc3339).map(Some).map_err(|error| {
        format!("Codebase Memory successful-index timestamp could not be formatted: {error}")
    })
}

/// Converts the managed numeric index revision into the API's optional text representation.
/// 将托管数字索引修订号转换为 API 使用的可选文本表示。
///
/// # Parameters
/// 参数
/// `value` is zero before the first successful index.
/// `value` 在首次成功索引前为零。
///
/// # Returns
/// 返回值
/// Returns no revision for zero or the decimal revision text otherwise.
/// 零值返回无修订号，否则返回十进制修订号文本。
fn managed_index_revision_text(value: u64) -> Option<String> {
    (value != 0).then(|| value.to_string())
}

/// Selects the controller-facing diagnostic from the contract-defined error fields.
/// 从契约规定的错误字段中选择控制器侧诊断。
///
/// # Parameters
/// 参数
/// `error_code` is the stable machine-readable code and `error` is the user-facing detail.
/// `error_code` 是稳定的机器可读错误码，`error` 是面向用户的详细信息。
///
/// # Returns
/// 返回值
/// Returns the detailed error when present, otherwise the stable code.
/// 详细错误存在时返回详细错误，否则返回稳定错误码。
fn managed_project_error(error_code: Option<String>, error: Option<String>) -> Option<String> {
    match error {
        Some(error) => Some(error),
        None => error_code,
    }
}

/// Validates the exact lifecycle-to-index-state mapping exposed by the managed sidecar.
/// 校验托管 Sidecar 暴露的精确生命周期到索引状态映射。
///
/// # Parameters
/// 参数
/// `lifecycle`, `index_status`, and `watcher_registered` are one atomic project snapshot.
/// `lifecycle`、`index_status` 与 `watcher_registered` 构成一份原子项目快照。
///
/// # Returns
/// 返回值
/// Returns success for a contract-valid diagnostic tuple or an explicit protocol error.
/// 契约有效的诊断元组返回成功，否则返回明确协议错误。
fn validate_managed_project_diagnostics(
    lifecycle: &str,
    index_status: &str,
    watcher_registered: bool,
) -> Result<(), String> {
    let expected_status = match lifecycle {
        "pending" => "pending",
        "indexing" => "running",
        "ready" => "idle",
        "offline" => "paused",
        "failed" => "failed",
        _ => {
            return Err(format!(
                "Codebase Memory returned unknown project lifecycle `{lifecycle}`."
            ));
        }
    };
    if index_status != expected_status {
        return Err(format!(
            "Codebase Memory returned index status `{index_status}` for lifecycle `{lifecycle}`; expected `{expected_status}`."
        ));
    }
    if lifecycle == "offline" && watcher_registered {
        return Err(String::from(
            "Codebase Memory returned an offline project with a registered watcher.",
        ));
    }
    Ok(())
}

/// Mutable controller state protected behind one short-lived mutex.
/// 由一个短生命周期互斥锁保护的可变控制器状态。
#[derive(Debug, Default)]
struct ManagedControllerState {
    /// Monotonic full-project synchronization generation.
    /// 全量项目同步使用的单调代次。
    generation: u64,
    /// Current sidecar lifecycle label.
    /// 当前 Sidecar 生命周期标签。
    service_status: String,
    /// Exact enabled projects from the latest authoritative workspace snapshot.
    /// 最近权威工作区快照中的精确启用项目集合。
    enabled_projects: BTreeMap<String, ManagedCodebaseMemoryProjectRoute>,
    /// Latest diagnostics keyed by stable project identifier.
    /// 按稳定项目标识索引的最近诊断。
    project_diagnostics: BTreeMap<String, ManagedProjectDiagnostic>,
    /// Whether the latest observed managed transport is connected and synchronized.
    /// 最近观察到的托管传输是否已连接且已完成同步。
    runtime_synchronized: bool,
    /// Whether the service already owns the single reconnect observer.
    /// 服务是否已经持有唯一重连观察器。
    recovery_monitor_started: bool,
}

/// Long-lived host controller coordinating authorization, lifecycle, and project synchronization.
/// 协调授权、生命周期与项目同步的长生命周期宿主控制器。
#[derive(Debug, Clone, Default)]
pub(crate) struct CodebaseMemoryController {
    /// Live authorization table consumed by tool registration and invocation.
    /// 工具注册与调用使用的实时授权表。
    authorization: ManagedCodebaseMemoryAuthorization,
    /// Controller-owned mutable lifecycle state.
    /// 控制器持有的可变生命周期状态。
    state: Arc<Mutex<ManagedControllerState>>,
    /// Serializes full-project synchronizations across settings changes and reconnect recovery.
    /// 在设置变更与重连恢复之间串行化全量项目同步。
    synchronization_gate: Arc<tokio::sync::Mutex<()>>,
}

impl CodebaseMemoryController {
    /// Rebuilds enabled projects and session authorization from one authoritative workspace snapshot.
    /// 根据一份权威工作区快照重建启用项目与会话授权。
    ///
    /// # Parameters
    /// 参数
    /// `workspace` is the exact persisted workspace projection after the latest mutation.
    /// `workspace` 是最近一次变更后的精确持久化工作区投影。
    ///
    /// # Returns
    /// 返回值
    /// Returns the enabled project list used by the next full synchronization.
    /// 返回下一次全量同步使用的启用项目列表。
    pub(crate) fn refresh_workspace(
        &self,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Vec<ManagedCodebaseMemoryProjectRoute>, String> {
        let enabled_projects = workspace
            .task_projects
            .iter()
            .filter(|project| project.codebase_memory_enabled)
            .map(|project| -> Result<_, String> {
                let pwd = normalized_managed_project_path(&project.path)?;
                Ok((
                    project.id.clone(),
                    ManagedCodebaseMemoryProjectRoute {
                        project_id: project.id.clone(),
                        pwd,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let session_projects = workspace
            .task_projects
            .iter()
            .filter_map(|project| {
                enabled_projects
                    .get(&project.id)
                    .map(|route| (project, route))
            })
            .flat_map(|(project, route)| {
                project
                    .session_ids
                    .iter()
                    .copied()
                    .map(|session_id| (session_id, route.project_id.clone()))
            })
            .collect::<Vec<_>>();
        self.authorization
            .replace_workspace(enabled_projects.values().cloned(), session_projects);
        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        state.enabled_projects = enabled_projects;
        let enabled_project_ids = state
            .enabled_projects
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        state
            .project_diagnostics
            .retain(|project_id, _| enabled_project_ids.contains(project_id));
        for project_id in enabled_project_ids {
            state.project_diagnostics.entry(project_id).or_default();
        }
        Ok(state.enabled_projects.values().cloned().collect())
    }

    /// Builds one request-scoped execution context only for a currently authorized session.
    /// 仅为当前已授权会话构建请求级执行上下文。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the real execution session identifier.
    /// `session_id` 是真实执行会话标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns `None` when the session has no enabled project route.
    /// 会话没有启用项目路由时返回 `None`。
    pub(crate) fn execution_context(
        &self,
        session_id: SessionId,
    ) -> Option<ManagedCodebaseMemoryExecutionContext> {
        self.authorization.resolve(session_id).ok().map(|route| {
            ManagedCodebaseMemoryExecutionContext {
                session_id,
                project_id: route.project_id,
                authorization: self.authorization.clone(),
            }
        })
    }

    /// Returns the current project-owned setting and retained runtime diagnostics.
    /// 返回当前项目自有设置与保留的运行时诊断。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the exact active project identity.
    /// `project_id` 是精确活动项目身份。
    /// `enabled` is the authoritative persisted project flag.
    /// `enabled` 是权威持久化项目标记。
    ///
    /// # Returns
    /// 返回值
    /// Returns one WebUI-facing settings summary.
    /// 返回一份面向 WebUI 的设置摘要。
    pub(crate) fn settings_summary(
        &self,
        project_id: &str,
        enabled: bool,
    ) -> CodebaseMemoryProjectSettingsSummary {
        let state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        let diagnostic = state.project_diagnostics.get(project_id);
        CodebaseMemoryProjectSettingsSummary {
            project_id: project_id.to_string(),
            enabled,
            service_status: if state.service_status.is_empty() {
                String::from("stopped")
            } else {
                state.service_status.clone()
            },
            index_status: diagnostic.and_then(|value| value.index_status.clone()),
            last_successful_index_at: diagnostic
                .and_then(|value| value.last_successful_index_at.clone()),
            index_revision: diagnostic.and_then(|value| value.index_revision.clone()),
            last_error: diagnostic.and_then(|value| value.last_error.clone()),
        }
    }

    /// Advances and returns the next full-project synchronization generation.
    /// 推进并返回下一次全量项目同步代次。
    ///
    /// # Returns
    /// 返回值
    /// Returns the strictly increasing generation value.
    /// 返回严格递增的代次值。
    fn next_generation(&self) -> u64 {
        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        state.generation = state.generation.saturating_add(1);
        state.generation
    }

    /// Updates the sidecar lifecycle label and optionally records one project-neutral error.
    /// 更新 Sidecar 生命周期标签，并可选记录一个项目无关错误。
    ///
    /// # Parameters
    /// 参数
    /// `status` is the stable lifecycle label and `error` is a non-sensitive diagnostic.
    /// `status` 是稳定生命周期标签，`error` 是不含敏感信息的诊断。
    ///
    /// # Returns
    /// 返回值
    /// Returns whether the projected lifecycle or diagnostic state changed.
    /// 返回投影的生命周期或诊断状态是否发生变化。
    pub(crate) fn set_service_status(&self, status: &str, error: Option<&str>) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        let mut changed = state.service_status != status;
        state.service_status = status.to_string();
        if let Some(error) = error {
            for diagnostic in state.project_diagnostics.values_mut() {
                changed |= diagnostic.last_error.as_deref() != Some(error);
                diagnostic.last_error = Some(error.to_string());
            }
        }
        changed
    }

    /// Marks the current project synchronization as incomplete and records one recoverable error.
    /// 将当前项目同步标记为未完成，并记录一个可恢复错误。
    ///
    /// # Parameters
    /// 参数
    /// `error` is a non-sensitive diagnostic safe for project settings.
    /// `error` 是可安全用于项目设置的不含敏感信息诊断。
    ///
    /// # Returns
    /// 返回值
    /// Returns whether the projected recovery state changed.
    /// 返回投影的恢复状态是否发生变化。
    pub(crate) fn mark_runtime_unsynchronized(&self, error: &str) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        let mut changed = state.runtime_synchronized || state.service_status != "degraded";
        state.runtime_synchronized = false;
        state.service_status = String::from("degraded");
        for diagnostic in state.project_diagnostics.values_mut() {
            changed |= diagnostic.last_error.as_deref() != Some(error);
            diagnostic.last_error = Some(error.to_string());
        }
        changed
    }

    /// Returns the current authoritative enabled-project routes.
    /// 返回当前权威启用项目路由。
    ///
    /// # Returns
    /// 返回值
    /// Returns a stable project-id ordered snapshot.
    /// 返回按稳定项目标识排序的快照。
    fn enabled_projects(&self) -> Vec<ManagedCodebaseMemoryProjectRoute> {
        self.state
            .lock()
            .expect("Codebase Memory controller state poisoned")
            .enabled_projects
            .values()
            .cloned()
            .collect()
    }

    /// Applies one complete set of sidecar synchronization outcomes after validating project identity.
    /// 在校验项目身份后应用一组完整的 Sidecar 同步结果。
    ///
    /// # Parameters
    /// 参数
    /// `expected_projects` is the exact project set sent in the request.
    /// `expected_projects` 是请求中发送的精确项目集合。
    /// `outcomes` contains exactly one response for each expected project.
    /// `outcomes` 必须为每个预期项目包含且仅包含一个响应。
    ///
    /// # Returns
    /// 返回值
    /// Returns an optional aggregate rejection warning after every valid outcome is retained.
    /// 在保留全部有效结果后返回可选的聚合拒绝警告。
    fn apply_sync_outcomes(
        &self,
        expected_projects: &[ManagedCodebaseMemoryProjectRoute],
        outcomes: Vec<ManagedProjectSyncOutcome>,
    ) -> Result<Option<String>, String> {
        let expected_ids = expected_projects
            .iter()
            .map(|project| project.project_id.clone())
            .collect::<BTreeSet<_>>();
        let mut received_ids = BTreeSet::new();
        for outcome in &outcomes {
            if !expected_ids.contains(&outcome.project_id)
                || !received_ids.insert(outcome.project_id.clone())
            {
                return Err(format!(
                    "Codebase Memory synchronization returned an unexpected or duplicate project `{}`.",
                    outcome.project_id
                ));
            }
        }
        if received_ids != expected_ids {
            let missing = expected_ids
                .difference(&received_ids)
                .cloned()
                .collect::<Vec<_>>();
            return Err(format!(
                "Codebase Memory synchronization omitted projects: {}.",
                missing.join(", ")
            ));
        }

        let mut normalized_outcomes = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            if !matches!(outcome.change.as_str(), "added" | "retained") {
                return Err(format!(
                    "Codebase Memory synchronization returned unknown change `{}` for project `{}`.",
                    outcome.change, outcome.project_id
                ));
            }
            validate_managed_project_diagnostics(
                &outcome.lifecycle,
                &outcome.index_status,
                outcome.watcher_registered,
            )?;
            if outcome.accepted != (outcome.lifecycle != "failed") {
                return Err(format!(
                    "Codebase Memory synchronization returned inconsistent acceptance and lifecycle for project `{}`.",
                    outcome.project_id
                ));
            }
            normalized_outcomes.push((
                outcome.project_id,
                outcome.accepted,
                outcome.index_status,
                managed_success_timestamp_text(outcome.last_success_at_ms)?,
                managed_index_revision_text(outcome.index_revision),
                managed_project_error(outcome.error_code, outcome.error),
            ));
        }

        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        let mut rejected = Vec::new();
        for (project_id, accepted, index_status, last_successful_index_at, index_revision, error) in
            normalized_outcomes
        {
            let diagnostic = state
                .project_diagnostics
                .entry(project_id.clone())
                .or_default();
            diagnostic.index_status = Some(index_status);
            diagnostic.last_successful_index_at = last_successful_index_at;
            diagnostic.index_revision = index_revision;
            diagnostic.last_error = error.clone();
            if !accepted {
                rejected.push(match error {
                    Some(error) => format!("{project_id}: {error}"),
                    None => project_id,
                });
            }
        }
        if rejected.is_empty() {
            Ok(None)
        } else {
            Ok(Some(format!(
                "Codebase Memory rejected projects: {}.",
                rejected.join("; ")
            )))
        }
    }

    /// Applies one strict project-status response to retained diagnostics.
    /// 将一份严格项目状态响应应用到保留诊断。
    fn apply_project_status(&self, status: ManagedProjectStatusResponse) -> Result<(), String> {
        validate_managed_project_diagnostics(
            &status.lifecycle,
            &status.index_status,
            status.watcher_registered,
        )?;
        let last_successful_index_at = managed_success_timestamp_text(status.last_success_at_ms)?;
        let index_revision = managed_index_revision_text(status.index_revision);
        let error = managed_project_error(status.error_code, status.error);
        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        let diagnostic = state
            .project_diagnostics
            .entry(status.project_id)
            .or_default();
        diagnostic.index_status = Some(status.index_status);
        diagnostic.last_successful_index_at = last_successful_index_at;
        diagnostic.index_revision = index_revision;
        diagnostic.last_error = error;
        Ok(())
    }

    /// Records one non-sensitive error for an exact currently enabled project.
    /// 为一个精确的当前启用项目记录不含敏感信息的错误。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the enabled project and `error` is safe for user-facing diagnostics.
    /// `project_id` 选择启用项目，`error` 可安全用于面向用户的诊断。
    fn set_project_error(&self, project_id: &str, error: &str) {
        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        if state.enabled_projects.contains_key(project_id) {
            state
                .project_diagnostics
                .entry(project_id.to_string())
                .or_default()
                .last_error = Some(error.to_string());
        }
    }

    /// Records one managed transport observation and decides whether full synchronization must replay.
    /// 记录一次托管传输观察结果，并判断是否必须重放全量同步。
    ///
    /// # Parameters
    /// 参数
    /// `connected` states whether the exact managed server is currently connected.
    /// `connected` 表示精确托管服务器当前是否已连接。
    ///
    /// # Returns
    /// 返回值
    /// Returns `(replay_required, status_changed)` for the current observation.
    /// 返回当前观察对应的“需要重放、状态已变化”二元组。
    fn observe_runtime_connection(&self, connected: bool) -> (bool, bool) {
        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        if !connected {
            let status_changed = state.runtime_synchronized
                || (!state.enabled_projects.is_empty() && state.service_status != "offline");
            state.runtime_synchronized = false;
            if !state.enabled_projects.is_empty() {
                state.service_status = String::from("offline");
            }
            return (false, status_changed);
        }
        (
            !state.runtime_synchronized && !state.enabled_projects.is_empty(),
            false,
        )
    }

    /// Marks the current connected transport as synchronized.
    /// 将当前已连接传输标记为已同步。
    fn mark_runtime_synchronized(&self) {
        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        state.runtime_synchronized = true;
        state.service_status = String::from("ready");
    }

    /// Claims ownership of the single reconnect observer for this controller lifetime.
    /// 为当前控制器生命周期认领唯一重连观察器。
    ///
    /// # Returns
    /// 返回值
    /// Returns true only to the first caller.
    /// 仅对首个调用方返回 true。
    fn claim_recovery_monitor(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("Codebase Memory controller state poisoned");
        if state.recovery_monitor_started {
            false
        } else {
            state.recovery_monitor_started = true;
            true
        }
    }
}

/// Validates and normalizes one persisted project root before it enters the private MCP contract.
/// 在持久化项目根目录进入私有 MCP 契约前完成校验与规范化。
///
/// # Parameters
/// 参数
/// `path` is the exact path owned by the project entity.
/// `path` 是项目实体持有的精确路径。
///
/// # Returns
/// 返回值
/// Returns normalized native absolute path text, or an explicit configuration error.
/// 返回规范化的原生绝对路径文本，或明确的配置错误。
fn normalized_managed_project_path(path: &str) -> Result<String, String> {
    let project_path = Path::new(path);
    if path.trim().is_empty() || !project_path.is_absolute() {
        return Err(format!(
            "Codebase Memory project path must be absolute: `{path}`."
        ));
    }
    if project_path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) {
        return Err(format!(
            "Codebase Memory project path must not contain relative components: `{path}`."
        ));
    }
    Ok(vulcan_agent_runtime::runtime_assets::native_host_path_text_from_path(project_path))
}

/// Loads project-owned Codebase Memory settings from the exact active project.
/// 从精确活动项目加载项目自有的 Codebase Memory 设置。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative workspace and controller state.
/// `state` 持有权威工作区与控制器状态。
/// `project_id` is the exact stable project identity.
/// `project_id` 是精确稳定项目身份。
///
/// # Returns
/// 返回值
/// Returns the persisted setting with current managed-runtime diagnostics.
/// 返回持久化设置与当前托管运行时诊断。
pub(crate) fn service_get_project_codebase_memory_settings(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<CodebaseMemoryProjectSettingsSummary, String> {
    let stable_project_id = project_id.trim();
    if stable_project_id.is_empty() {
        return Err(String::from("项目标识不能为空。"));
    }
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let project = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| {
            workspace
                .task_projects
                .iter()
                .find(|project| project.id == stable_project_id)
        })
        .ok_or_else(|| format!("项目 `{stable_project_id}` 不存在。"))?;
    Ok(guard
        .codebase_memory_controller
        .settings_summary(&project.id, project.codebase_memory_enabled))
}

/// Persists one project enablement change and immediately reconciles the managed runtime.
/// 持久化单个项目启用变更并立即协调托管运行时。
///
/// # Parameters
/// 参数
/// `app` emits the independent project-settings invalidation event.
/// `app` 发送独立的项目设置失效事件。
/// `state` owns workspace persistence and runtime generations.
/// `state` 持有工作区持久化与运行时代次。
/// `project_id` is the exact active project identity.
/// `project_id` 是精确活动项目身份。
/// `enabled` is the requested project-owned setting.
/// `enabled` 是请求的项目自有设置。
///
/// # Returns
/// 返回值
/// Returns the persisted setting even when sidecar startup is degraded.
/// 即使 Sidecar 启动降级，也返回已持久化设置。
pub(crate) async fn service_set_project_codebase_memory_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    project_id: String,
    enabled: bool,
) -> Result<CodebaseMemoryProjectSettingsSummary, String> {
    let stable_project_id = project_id.trim().to_string();
    if stable_project_id.is_empty() {
        return Err(String::from("项目标识不能为空。"));
    }
    // persistGuard serializes this project mutation with project removal and every session lifecycle writer.
    // persistGuard 将本次项目变更与项目移除及所有会话生命周期写入串行化。
    let persist_lock = webui_workspace_persist_lock();
    let persist_guard = persist_lock.lock().await;
    let controller_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let project_exists = guard.workspace_snapshot.as_ref().is_some_and(|workspace| {
            workspace
                .task_projects
                .iter()
                .any(|project| project.id == stable_project_id)
        });
        if !project_exists {
            return Err(format!("项目 `{stable_project_id}` 不存在。"));
        }
        guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("控制器运行时尚未就绪。"))?
    };
    vulcan_workspace_store::workspace_store::persist_project_codebase_memory_enabled(
        controller_runtime,
        &stable_project_id,
        enabled,
    )
    .await?;
    refresh_workbench_snapshot(state.clone()).await?;
    // The durable project mutation is complete; runtime reconciliation has its own synchronization gate.
    // 持久化项目变更已经完成；运行时协调使用独立同步闸门。
    drop(persist_guard);
    let reconcile_error = reconcile_codebase_memory_runtime(&state).await.err();
    if let Some(error) = reconcile_error.as_deref() {
        let controller = state
            .lock()
            .map_err(|lock_error| format!("failed to lock service state: {lock_error}"))?
            .codebase_memory_controller
            .clone();
        controller.mark_runtime_unsynchronized(error);
    }
    commit_and_emit_project_settings_changed_delta(
        &app,
        &state,
        Some(stable_project_id.clone()),
        vulcan_service_api::ProjectSettingsDomain::CodebaseMemory,
    )?;
    service_get_project_codebase_memory_settings(state, stable_project_id)
}

/// Requests a non-destructive reindex for one exact enabled project.
/// 为一个精确启用项目请求非破坏性重建索引。
///
/// # Parameters
/// 参数
/// `app` emits the project-settings invalidation event after acceptance.
/// `app` 在请求被接受后发送项目设置失效事件。
/// `state` owns the current managed runtime.
/// `state` 持有当前托管运行时。
/// `project_id` is the exact enabled project identity.
/// `project_id` 是精确启用项目身份。
///
/// # Returns
/// 返回值
/// Returns current settings after the sidecar accepts the request.
/// Sidecar 接受请求后返回当前设置。
pub(crate) async fn service_reindex_project_codebase_memory(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<CodebaseMemoryProjectSettingsSummary, String> {
    let stable_project_id = project_id.trim().to_string();
    // persistGuard prevents project removal from overtaking an accepted reindex request.
    // persistGuard 防止项目移除越过已经接纳的重建索引请求。
    let persist_lock = webui_workspace_persist_lock();
    let persist_guard = persist_lock.lock().await;
    if let Err(error) = reindex_codebase_memory_project(&state, &stable_project_id).await {
        if let Ok(guard) = state.lock() {
            guard
                .codebase_memory_controller
                .set_project_error(&stable_project_id, &error);
        }
        if let Err(delta_error) = commit_and_emit_project_settings_changed_delta(
            &app,
            &state,
            Some(stable_project_id.clone()),
            vulcan_service_api::ProjectSettingsDomain::CodebaseMemory,
        ) {
            eprintln!(
                "[webui-service] managed Codebase Memory reindex error delta failed: {delta_error}"
            );
        }
        drop(persist_guard);
        return Err(error);
    }
    commit_and_emit_project_settings_changed_delta(
        &app,
        &state,
        Some(stable_project_id.clone()),
        vulcan_service_api::ProjectSettingsDomain::CodebaseMemory,
    )?;
    let settings = service_get_project_codebase_memory_settings(state, stable_project_id);
    drop(persist_guard);
    settings
}

/// Reconciles the managed sidecar lifecycle and synchronizes the complete enabled-project set.
/// 协调托管 Sidecar 生命周期并同步完整启用项目集合。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative workspace, controller, and published MCP generation.
/// `state` 持有权威工作区、控制器与已发布 MCP 代次。
///
/// # Returns
/// 返回值
/// Returns success after the next runtime generation and project set are published.
/// 下一运行时代次与项目集合发布后返回成功。
pub(crate) async fn reconcile_codebase_memory_runtime(
    state: &SharedServiceBridgeState,
) -> Result<(), String> {
    let (controller, projects, current_runtime) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("工作区尚未就绪，无法协调 Codebase Memory。"))?;
        let projects = guard
            .codebase_memory_controller
            .refresh_workspace(workspace)?;
        let current_runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("Agent 运行时尚未就绪，无法协调 Codebase Memory。"))?
            .extension_generation
            .mcp_runtime();
        (
            guard.codebase_memory_controller.clone(),
            projects,
            current_runtime,
        )
    };

    if projects.is_empty() {
        let next_runtime = current_runtime.without_managed_codebase_memory_partition();
        publish_managed_runtime_generation(state, next_runtime)?;
        current_runtime.shutdown_managed_codebase_memory_partition();
        controller.observe_runtime_connection(false);
        controller.set_service_status("stopped", None);
        return Ok(());
    }

    controller.set_service_status("starting", None);
    let distribution = managed_server_distribution()?;
    let next_runtime = if managed_server_is_connected(&current_runtime) {
        current_runtime
    } else {
        replace_managed_codebase_memory_mcp_runtime_partition(
            &current_runtime,
            vec![distribution.server.clone()],
        )
        .await?
    };
    validate_managed_initialize_identity(&next_runtime, &distribution.version)?;
    validate_managed_tool_contract(&next_runtime)?;
    let synchronization_warning = synchronize_enabled_projects(&controller, &next_runtime).await?;
    publish_managed_runtime_generation(state, next_runtime)?;
    controller.mark_runtime_synchronized();
    if let Some(warning) = synchronization_warning.as_deref() {
        controller.set_service_status("degraded", Some(warning));
    }
    Ok(())
}

/// Starts the lightweight reconnect observer for the managed Codebase Memory partition.
/// 启动托管 Codebase Memory 分区的轻量重连观察器。
///
/// # Parameters
/// 参数
/// `state` is downgraded so the observer never extends service lifetime.
/// `state` 会被降级为弱引用，因此观察器绝不会延长服务生命周期。
/// `app` emits independent project-settings revisions after status transitions.
/// `app` 在状态转换后发送独立的项目设置修订。
pub(crate) fn start_codebase_memory_recovery_monitor(
    state: SharedServiceBridgeState,
    app: ServiceEventEmitter,
) {
    let should_start = state
        .lock()
        .map(|guard| guard.codebase_memory_controller.claim_recovery_monitor())
        .unwrap_or(false);
    if !should_start {
        return;
    }
    let weak_state = Arc::downgrade(&state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(MANAGED_RECOVERY_POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Some(state) = weak_state.upgrade() else {
                break;
            };
            match recover_codebase_memory_after_reconnect(&state).await {
                Ok(true) => {
                    emit_codebase_memory_status_changed(&app, &state);
                }
                Ok(false) => {}
                Err(error) => {
                    let controller = match state.lock() {
                        Ok(guard) => guard.codebase_memory_controller.clone(),
                        Err(lock_error) => {
                            eprintln!(
                                "[webui-service] managed Codebase Memory recovery state lock failed: {lock_error}"
                            );
                            continue;
                        }
                    };
                    if controller.mark_runtime_unsynchronized(&error) {
                        emit_codebase_memory_status_changed(&app, &state);
                    }
                    eprintln!(
                        "[webui-service] managed Codebase Memory reconnect recovery degraded: {error}"
                    );
                }
            }
        }
    });
}

/// Emits one independent Codebase Memory settings revision after a runtime status transition.
/// 在运行时状态转换后发送一个独立的 Codebase Memory 设置修订。
fn emit_codebase_memory_status_changed(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) {
    if let Err(error) = commit_and_emit_project_settings_changed_delta(
        app,
        state,
        None,
        vulcan_service_api::ProjectSettingsDomain::CodebaseMemory,
    ) {
        eprintln!("[webui-service] managed Codebase Memory status delta failed: {error}");
    }
}

/// Replays the authoritative project set exactly once after each connected transition.
/// 在每次连接转换后精确重放一次权威项目集合。
///
/// # Returns
/// 返回值
/// Returns whether the WebUI-facing status changed.
/// 返回面向 WebUI 的状态是否发生变化。
async fn recover_codebase_memory_after_reconnect(
    state: &SharedServiceBridgeState,
) -> Result<bool, String> {
    let (controller, runtime) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(agent_runtime) = guard.agent_runtime.as_ref() else {
            return Ok(false);
        };
        (
            guard.codebase_memory_controller.clone(),
            agent_runtime.extension_generation.mcp_runtime(),
        )
    };
    let Some(runtime_status) = managed_server_status(&runtime) else {
        return Ok(false);
    };
    let connected = runtime_status == McpRuntimeServerStatus::Connected;
    let (replay_required, status_changed) = controller.observe_runtime_connection(connected);
    if !replay_required {
        return Ok(status_changed);
    }

    let distribution = managed_server_distribution()?;
    validate_managed_initialize_identity(&runtime, &distribution.version)?;
    validate_managed_tool_contract(&runtime)?;
    let synchronization_warning = synchronize_enabled_projects(&controller, &runtime).await?;
    controller.mark_runtime_synchronized();
    if let Some(warning) = synchronization_warning.as_deref() {
        controller.set_service_status("degraded", Some(warning));
    }
    Ok(true)
}

/// Invokes the hidden project reindex control tool for one enabled project.
/// 为一个已启用项目调用隐藏的项目重建索引控制工具。
///
/// # Parameters
/// 参数
/// `state` owns the current managed runtime and `project_id` selects one exact enabled project.
/// `state` 持有当前托管运行时，`project_id` 选择一个精确启用项目。
///
/// # Returns
/// 返回值
/// Returns success after the sidecar accepts the reindex request.
/// Sidecar 接受重建请求后返回成功。
pub(crate) async fn reindex_codebase_memory_project(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<(), String> {
    let (route, runtime, controller) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("工作区尚未就绪。"))?;
        let project = workspace
            .task_projects
            .iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| format!("项目 `{project_id}` 不存在。"))?;
        if !project.codebase_memory_enabled {
            return Err(format!("项目 `{project_id}` 尚未启用 Codebase Memory。"));
        }
        let runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("Agent 运行时尚未就绪。"))?
            .extension_generation
            .mcp_runtime();
        (
            ManagedCodebaseMemoryProjectRoute {
                project_id: project.id.clone(),
                pwd: normalized_managed_project_path(&project.path)?,
            },
            runtime,
            guard.codebase_memory_controller.clone(),
        )
    };
    let value = invoke_control_tool(
        &runtime,
        "vulcan_reindex_project",
        json!({
            "contract": MANAGED_CODEBASE_MEMORY_CONTRACT,
            "project_id": route.project_id,
            "pwd": route.pwd,
        }),
    )
    .await?;
    let response: ManagedProjectReindexResponse =
        serde_json::from_value(value).map_err(|error| {
            format!("Codebase Memory reindex returned an invalid response: {error}")
        })?;
    if response.contract != MANAGED_CODEBASE_MEMORY_CONTRACT
        || response.project_id != route.project_id
    {
        return Err(String::from(
            "Codebase Memory reindex returned a mismatched contract or project identity.",
        ));
    }
    if !response.accepted {
        let error = format!(
            "Codebase Memory rejected reindex for project `{}`.",
            route.project_id
        );
        controller.set_project_error(&route.project_id, &error);
        return Err(error);
    }
    let expected_coalesced = match response.status.as_str() {
        "started" => false,
        "running" => true,
        _ => {
            return Err(format!(
                "Codebase Memory reindex returned unknown scheduling status `{}`.",
                response.status
            ));
        }
    };
    if response.coalesced != expected_coalesced {
        return Err(format!(
            "Codebase Memory reindex returned inconsistent scheduling status `{}` and coalesced flag.",
            response.status
        ));
    }
    refresh_managed_project_status(&controller, &runtime, &route).await
}

/// Publishes one MCP access generation while preserving the current plugin catalog.
/// 发布一份 MCP 访问代次，同时保留当前插件目录。
fn publish_managed_runtime_generation(
    state: &SharedServiceBridgeState,
    next_runtime: McpRuntimeAccess,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let agent_runtime = guard
        .agent_runtime
        .as_mut()
        .ok_or_else(|| String::from("Agent 运行时尚未就绪。"))?;
    agent_runtime.extension_generation = Arc::new(ExtensionGeneration::new(
        agent_runtime
            .extension_generation
            .revision()
            .saturating_add(1),
        agent_runtime.extension_generation.plugin_catalog(),
        next_runtime,
    ));
    Ok(())
}

/// Returns whether the current access generation contains the connected managed server.
/// 返回当前访问代次是否包含已连接的托管服务器。
fn managed_server_is_connected(runtime: &McpRuntimeAccess) -> bool {
    managed_server_status(runtime) == Some(McpRuntimeServerStatus::Connected)
}

/// Returns the exact managed server status without accepting same-id servers from other partitions.
/// 返回精确托管服务器状态，且不接纳其他分区中的同名服务器。
fn managed_server_status(runtime: &McpRuntimeAccess) -> Option<McpRuntimeServerStatus> {
    runtime
        .server_states()
        .into_iter()
        .find(|server| {
            server.id == MANAGED_CODEBASE_MEMORY_SERVER_ID
                && server.source == "managed-codebase-memory"
        })
        .map(|server| server.status)
}

/// Loads and validates the packaged sidecar manifest before selecting one strict stdio server.
/// 在选择严格 stdio 服务器前加载并校验打包 Sidecar 清单。
fn managed_server_distribution() -> Result<ManagedCodebaseMemoryDistribution, String> {
    let distribution_root =
        vulcan_agent_runtime::runtime_assets::resolve_managed_runtime_distribution_root()
            .ok_or_else(|| {
                String::from("Codebase Memory managed runtime distribution root is unavailable.")
            })?
            .join(MANAGED_DISTRIBUTION_DIRECTORY);
    let manifest_path = distribution_root.join(MANAGED_DISTRIBUTION_MANIFEST);
    let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
        format!(
            "Codebase Memory managed runtime manifest is unavailable at `{}`: {error}",
            manifest_path.display()
        )
    })?;
    let manifest: ManagedCodebaseMemoryManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|error| {
            format!(
                "Codebase Memory managed runtime manifest `{}` is invalid: {error}",
                manifest_path.display()
            )
        })?;
    validate_manifest(&manifest)?;
    let executable_path = distribution_root.join(&manifest.executable);
    let executable_bytes = fs::read(&executable_path).map_err(|error| {
        format!(
            "Codebase Memory managed runtime executable is unavailable at `{}`: {error}",
            executable_path.display()
        )
    })?;
    let digest = format!("{:x}", Sha256::digest(executable_bytes));
    if digest != manifest.sha256 {
        return Err(format!(
            "Codebase Memory managed runtime checksum mismatch for `{}`.",
            executable_path.display()
        ));
    }
    // cacheRoot is user-writable application data and never mutates packaged output assets.
    // cacheRoot 使用用户可写的应用数据目录，绝不修改打包输出资产。
    let cache_root =
        vulcan_workspace_store::data_plane::application_data_root().join("codebase-memory");
    fs::create_dir_all(&cache_root).map_err(|error| {
        format!(
            "Codebase Memory managed cache root `{}` could not be created: {error}",
            cache_root.display()
        )
    })?;
    // environment passes only the dedicated managed cache root into the isolated child process.
    // environment 仅向隔离子进程传递专用托管缓存根目录。
    let mut environment = BTreeMap::new();
    environment.insert(
        String::from("VULCAN_CBM_CACHE_DIR"),
        vulcan_agent_runtime::runtime_assets::native_host_path_text_from_path(&cache_root),
    );
    Ok(ManagedCodebaseMemoryDistribution {
        version: manifest.version.clone(),
        server: McpServerConfig {
            source: McpServerSource::HostManagedCodebaseMemory,
            id: String::from(MANAGED_CODEBASE_MEMORY_SERVER_ID),
            name: format!("Vulcan Codebase Memory {}", manifest.version),
            enabled: true,
            transport: McpServerTransportConfig::Stdio {
                command: vulcan_agent_runtime::runtime_assets::native_host_path_text_from_path(
                    &executable_path,
                ),
                args: vec![String::from("--vulcan-managed")],
                cwd: Some(
                    vulcan_agent_runtime::runtime_assets::native_host_path_text_from_path(
                        &distribution_root,
                    ),
                ),
                env: environment,
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        },
    })
}

/// Validates the immutable manifest against the current platform and private contract.
/// 根据当前平台与私有契约校验不可变清单。
fn validate_manifest(manifest: &ManagedCodebaseMemoryManifest) -> Result<(), String> {
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    // expected_executable pins the only dedicated product name accepted for this platform.
    // expected_executable 固定当前平台唯一允许的专版产品名称。
    let expected_executable = if cfg!(windows) {
        "vulcan-codebase-memory-mcp.exe"
    } else {
        "vulcan-codebase-memory-mcp"
    };
    if manifest.protocol != MANAGED_CODEBASE_MEMORY_CONTRACT {
        return Err(format!(
            "Codebase Memory managed runtime protocol `{}` does not match `{MANAGED_CODEBASE_MEMORY_CONTRACT}`.",
            manifest.protocol
        ));
    }
    if manifest.platform != platform {
        return Err(format!(
            "Codebase Memory managed runtime platform `{}` does not match `{platform}`.",
            manifest.platform
        ));
    }
    if manifest.executable != expected_executable {
        return Err(format!(
            "Codebase Memory managed runtime executable `{}` does not match the dedicated product `{expected_executable}`.",
            manifest.executable
        ));
    }
    if manifest.version.trim().is_empty()
        || manifest.executable.trim().is_empty()
        || manifest.sha256.len() != 64
        || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || manifest
            .sha256
            .bytes()
            .any(|byte| byte.is_ascii_uppercase())
    {
        return Err(String::from(
            "Codebase Memory managed runtime manifest contains an invalid version, executable, or SHA-256 digest.",
        ));
    }
    if Path::new(&manifest.executable).is_absolute()
        || Path::new(&manifest.executable)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(String::from(
            "Codebase Memory managed runtime executable must stay inside its distribution directory.",
        ));
    }
    Ok(())
}

/// Validates the initialize identity of the exact managed server before publication.
/// 在发布前校验精确托管服务器的 initialize 身份。
///
/// # Parameters
/// 参数
/// `runtime` contains the newly connected managed partition.
/// `runtime` 包含新连接的托管分区。
/// `expected_version` is pinned by the validated packaged manifest.
/// `expected_version` 由已校验的打包清单固定。
///
/// # Returns
/// 返回值
/// Returns success only for the expected product and binary version with a negotiated MCP version.
/// 仅当产品、二进制版本符合预期且已协商 MCP 版本时返回成功。
fn validate_managed_initialize_identity(
    runtime: &McpRuntimeAccess,
    expected_version: &str,
) -> Result<(), String> {
    let server = runtime
        .server_states()
        .into_iter()
        .find(|server| {
            server.id == MANAGED_CODEBASE_MEMORY_SERVER_ID
                && server.source == "managed-codebase-memory"
        })
        .ok_or_else(|| {
            String::from("Codebase Memory managed runtime initialize identity is unavailable.")
        })?;
    if server.status != McpRuntimeServerStatus::Connected {
        return Err(format!(
            "Codebase Memory managed runtime is not connected after initialize: {}.",
            server.status.label()
        ));
    }
    let protocol_version = server.protocol_version.as_deref().unwrap_or_default();
    if protocol_version.trim().is_empty() {
        return Err(String::from(
            "Codebase Memory managed runtime did not report an MCP protocol version during initialize.",
        ));
    }
    if server.server_product.as_deref() != Some(MANAGED_CODEBASE_MEMORY_PRODUCT) {
        return Err(format!(
            "Codebase Memory managed runtime product mismatch: expected `{MANAGED_CODEBASE_MEMORY_PRODUCT}`, got `{}`.",
            server.server_product.as_deref().unwrap_or("<missing>")
        ));
    }
    if server.server_version.as_deref() != Some(expected_version) {
        return Err(format!(
            "Codebase Memory managed runtime version mismatch: expected `{expected_version}`, got `{}`.",
            server.server_version.as_deref().unwrap_or("<missing>")
        ));
    }
    Ok(())
}

/// Verifies all private control and public read-only tools before publishing the managed runtime.
/// 在发布托管运行时前校验全部私有控制工具与公开只读工具。
fn validate_managed_tool_contract(runtime: &McpRuntimeAccess) -> Result<(), String> {
    let available = runtime
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .filter(|tool| {
            tool.partition_key() == HOST_MANAGED_CODEBASE_MEMORY_PARTITION_KEY
                && tool.server_id() == Some(MANAGED_CODEBASE_MEMORY_SERVER_ID)
        })
        .filter_map(|tool| tool.origin_tool_name().map(str::to_string))
        .collect::<BTreeSet<_>>();
    let missing = REQUIRED_CONTROL_TOOLS
        .iter()
        .chain(MANAGED_CODEBASE_MEMORY_READ_ONLY_TOOLS.iter())
        .filter(|tool_name| !available.contains(**tool_name))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Codebase Memory managed runtime is missing required tools: {}.",
            missing.join(", ")
        ))
    }
}

/// Refreshes retained diagnostics for one exact enabled project.
/// 刷新一个精确启用项目的保留诊断。
async fn refresh_managed_project_status(
    controller: &CodebaseMemoryController,
    runtime: &McpRuntimeAccess,
    route: &ManagedCodebaseMemoryProjectRoute,
) -> Result<(), String> {
    let value = invoke_control_tool(
        runtime,
        "vulcan_project_status",
        json!({
            "contract": MANAGED_CODEBASE_MEMORY_CONTRACT,
            "project_id": route.project_id,
            "pwd": route.pwd,
        }),
    )
    .await?;
    let response: ManagedProjectStatusResponse =
        serde_json::from_value(value).map_err(|error| {
            format!("Codebase Memory project status returned an invalid response: {error}")
        })?;
    if response.contract != MANAGED_CODEBASE_MEMORY_CONTRACT
        || response.project_id != route.project_id
    {
        return Err(String::from(
            "Codebase Memory project status returned a mismatched contract or project identity.",
        ));
    }
    controller.apply_project_status(response)
}

/// Sends one authoritative full-project synchronization to the hidden control tool.
/// 向隐藏控制工具发送一次权威全量项目同步。
async fn synchronize_enabled_projects(
    controller: &CodebaseMemoryController,
    runtime: &McpRuntimeAccess,
) -> Result<Option<String>, String> {
    // synchronizationGuard preserves request ordering so a reconnect replay cannot overtake a settings mutation.
    // synchronizationGuard 保持请求顺序，避免重连重放越过设置变更。
    let _synchronization_guard = controller.synchronization_gate.lock().await;
    let projects = controller.enabled_projects();
    let generation = controller.next_generation();
    let value = invoke_control_tool(
        runtime,
        "vulcan_sync_projects",
        json!({
            "contract": MANAGED_CODEBASE_MEMORY_CONTRACT,
            "generation": generation,
            "projects": projects.iter().map(|project| json!({
                "project_id": project.project_id,
                "pwd": project.pwd,
            })).collect::<Vec<_>>(),
        }),
    )
    .await?;
    let response: ManagedProjectSyncResponse = serde_json::from_value(value).map_err(|error| {
        format!("Codebase Memory synchronization returned an invalid response: {error}")
    })?;
    if response.contract != MANAGED_CODEBASE_MEMORY_CONTRACT {
        return Err(format!(
            "Codebase Memory synchronization contract mismatch: expected `{MANAGED_CODEBASE_MEMORY_CONTRACT}`, got `{}`.",
            response.contract
        ));
    }
    if response.generation != generation {
        return Err(format!(
            "Codebase Memory synchronization generation mismatch: expected `{generation}`, got `{}`.",
            response.generation
        ));
    }
    if response.status != "committed" {
        return Err(format!(
            "Codebase Memory synchronization returned transaction status `{}` instead of `committed`.",
            response.status
        ));
    }
    // expectedCount is the exact project cardinality sent in this synchronization transaction.
    // expectedCount 是本次同步事务发送的精确项目基数。
    let expected_count = u64::try_from(projects.len()).map_err(|error| {
        format!("Codebase Memory project count could not be represented by the contract: {error}")
    })?;
    // reconciledCount proves that each active project was classified exactly once.
    // reconciledCount 证明每个活动项目都被且仅被归类一次。
    let reconciled_count = response
        .added
        .checked_add(response.retained)
        .ok_or_else(|| String::from("Codebase Memory synchronization counters overflowed."))?;
    // rejectedCount is derived from the exact per-project acceptance outcomes.
    // rejectedCount 根据精确的每项目接纳结果计算。
    let rejected_count = u64::try_from(
        response
            .projects
            .iter()
            .filter(|project| !project.accepted)
            .count(),
    )
    .map_err(|error| {
        format!("Codebase Memory rejected-project count could not be represented: {error}")
    })?;
    if response.project_count != expected_count
        || reconciled_count != expected_count
        || response.projects.len() != projects.len()
        || response.failed != rejected_count
    {
        return Err(format!(
            "Codebase Memory synchronization counters are inconsistent: expected={expected_count}, project_count={}, added={}, retained={}, removed={}, failed={}, outcomes={}.",
            response.project_count,
            response.added,
            response.retained,
            response.removed,
            response.failed,
            response.projects.len()
        ));
    }
    controller.apply_sync_outcomes(&projects, response.projects)
}

/// Invokes one exact hidden control tool on the managed server.
/// 在托管服务器上调用一个精确隐藏控制工具。
async fn invoke_control_tool(
    runtime: &McpRuntimeAccess,
    tool_name: &str,
    args: Value,
) -> Result<Value, String> {
    let binding = runtime
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|tool| {
            tool.partition_key() == HOST_MANAGED_CODEBASE_MEMORY_PARTITION_KEY
                && tool.server_id() == Some(MANAGED_CODEBASE_MEMORY_SERVER_ID)
                && tool.origin_tool_name() == Some(tool_name)
        })
        .ok_or_else(|| {
            format!("Codebase Memory managed control tool `{tool_name}` is unavailable.")
        })?;
    tokio::time::timeout(MANAGED_CONTROL_TOOL_TIMEOUT, binding.invoke(args))
        .await
        .map_err(|_| {
            format!(
                "Codebase Memory managed control tool `{tool_name}` timed out after {} seconds.",
                MANAGED_CONTROL_TOOL_TIMEOUT.as_secs()
            )
        })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_workbench_model::TaskProjectSummary;

    /// Verifies that only enabled project sessions enter the live authorization table.
    /// 验证只有启用项目的会话会进入实时授权表。
    #[test]
    fn refresh_workspace_authorizes_only_enabled_project_sessions() {
        let enabled_session = Uuid::new_v4();
        let disabled_session = Uuid::new_v4();
        let enabled_path = std::env::temp_dir()
            .join("vulcan-codebase-memory-enabled")
            .to_string_lossy()
            .into_owned();
        let disabled_path = std::env::temp_dir()
            .join("vulcan-codebase-memory-disabled")
            .to_string_lossy()
            .into_owned();
        let workspace = WorkspaceSnapshot {
            task_projects: vec![
                TaskProjectSummary::new(
                    "enabled",
                    "Enabled",
                    enabled_path,
                    false,
                    0,
                    None,
                    None,
                    vec![enabled_session],
                )
                .with_codebase_memory_enabled(true),
                TaskProjectSummary::new(
                    "disabled",
                    "Disabled",
                    disabled_path,
                    false,
                    1,
                    None,
                    None,
                    vec![disabled_session],
                ),
            ],
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: enabled_session,
            hydrated_session_ids: BTreeSet::new(),
        };
        let controller = CodebaseMemoryController::default();

        let projects = controller
            .refresh_workspace(&workspace)
            .expect("enabled project paths should be valid");

        assert_eq!(projects.len(), 1);
        assert!(controller.execution_context(enabled_session).is_some());
        assert!(controller.execution_context(disabled_session).is_none());
    }

    /// Verifies that a packaged executable cannot escape its managed distribution directory.
    /// 验证打包可执行文件不能逃逸托管分发目录。
    #[test]
    fn manifest_rejects_parent_directory_executable() {
        let manifest = ManagedCodebaseMemoryManifest {
            protocol: String::from(MANAGED_CODEBASE_MEMORY_CONTRACT),
            version: String::from("1.0.0"),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            executable: String::from("../sidecar"),
            sha256: "0".repeat(64),
        };

        assert!(validate_manifest(&manifest).is_err());
    }

    /// Verifies that a public or renamed MCP executable cannot enter the dedicated distribution.
    /// 验证公共版或重命名 MCP 可执行文件不能进入专版分发。
    #[test]
    fn manifest_rejects_non_dedicated_executable_name() {
        let manifest = ManagedCodebaseMemoryManifest {
            protocol: String::from(MANAGED_CODEBASE_MEMORY_CONTRACT),
            version: String::from("1.0.0"),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            executable: if cfg!(windows) {
                String::from("codebase-memory-mcp.exe")
            } else {
                String::from("codebase-memory-mcp")
            },
            sha256: "0".repeat(64),
        };

        assert!(validate_manifest(&manifest).is_err());
    }

    /// Verifies reconnect recovery requests one replay per disconnected-to-connected cycle.
    /// 验证重连恢复在每个断开到连接周期内只请求一次重放。
    #[test]
    fn reconnect_observation_requires_one_replay_per_connection_cycle() {
        let controller = CodebaseMemoryController::default();
        let session_id = Uuid::new_v4();
        let project_path = std::env::temp_dir()
            .join("vulcan-codebase-memory-reconnect")
            .to_string_lossy()
            .into_owned();
        let workspace = WorkspaceSnapshot {
            task_projects: vec![
                TaskProjectSummary::new(
                    "enabled",
                    "Enabled",
                    project_path,
                    false,
                    0,
                    None,
                    None,
                    vec![session_id],
                )
                .with_codebase_memory_enabled(true),
            ],
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        controller
            .refresh_workspace(&workspace)
            .expect("enabled project path should be valid");

        assert_eq!(controller.observe_runtime_connection(true), (true, false));
        controller.mark_runtime_synchronized();
        assert_eq!(controller.observe_runtime_connection(true), (false, false));
        controller.mark_runtime_unsynchronized("temporary sync failure");
        assert_eq!(controller.observe_runtime_connection(true), (true, false));
        controller.mark_runtime_synchronized();
        assert_eq!(controller.observe_runtime_connection(false), (false, true));
        assert_eq!(controller.observe_runtime_connection(false), (false, false));
        assert_eq!(controller.observe_runtime_connection(true), (true, false));
    }

    /// Verifies partial synchronization outcomes retain every project diagnostic before reporting degradation.
    /// 验证部分同步结果会先保留每个项目诊断，再报告降级。
    #[test]
    fn synchronization_outcomes_record_partial_failures_without_dropping_projects() {
        let controller = CodebaseMemoryController::default();
        let projects = vec![
            ManagedCodebaseMemoryProjectRoute {
                project_id: String::from("project-a"),
                pwd: String::from(r"D:\a"),
            },
            ManagedCodebaseMemoryProjectRoute {
                project_id: String::from("project-b"),
                pwd: String::from(r"D:\b"),
            },
        ];
        {
            let mut state = controller
                .state
                .lock()
                .expect("controller state should lock");
            state.enabled_projects = projects
                .iter()
                .cloned()
                .map(|route| (route.project_id.clone(), route))
                .collect();
        }

        let warning = controller
            .apply_sync_outcomes(
                &projects,
                vec![
                    ManagedProjectSyncOutcome {
                        project_id: String::from("project-a"),
                        accepted: true,
                        change: String::from("added"),
                        lifecycle: String::from("ready"),
                        index_status: String::from("idle"),
                        watcher_registered: true,
                        index_revision: 7,
                        last_success_at_ms: 1_753_401_600_000,
                        error_code: None,
                        error: None,
                    },
                    ManagedProjectSyncOutcome {
                        project_id: String::from("project-b"),
                        accepted: false,
                        change: String::from("retained"),
                        lifecycle: String::from("failed"),
                        index_status: String::from("failed"),
                        watcher_registered: false,
                        index_revision: 0,
                        last_success_at_ms: 0,
                        error_code: Some(String::from("index_unavailable")),
                        error: Some(String::from("index unavailable")),
                    },
                ],
            )
            .expect("complete outcomes should satisfy the protocol")
            .expect("one rejected project should produce a degradation warning");

        assert!(warning.contains("project-b"));
        assert_eq!(
            controller
                .settings_summary("project-a", true)
                .index_revision
                .as_deref(),
            Some("7")
        );
        assert_eq!(
            controller
                .settings_summary("project-b", true)
                .last_error
                .as_deref(),
            Some("index unavailable")
        );
    }

    /// Verifies strict host response models accept the exact C sidecar wire payloads.
    /// 验证严格宿主响应模型接纳 C Sidecar 的精确线上载荷。
    #[test]
    fn managed_response_contract_deserializes_exact_sidecar_payloads() {
        // syncPayload mirrors the committed full-reconciliation response produced by the C sidecar.
        // syncPayload 对应 C Sidecar 生成的已提交全量协调响应。
        let sync_payload = json!({
            "contract": MANAGED_CODEBASE_MEMORY_CONTRACT,
            "status": "committed",
            "generation": 4,
            "project_count": 1,
            "added": 1,
            "retained": 0,
            "removed": 2,
            "failed": 0,
            "projects": [{
                "project_id": "project-a",
                "accepted": true,
                "change": "added",
                "lifecycle": "ready",
                "index_status": "idle",
                "watcher_registered": true,
                "index_revision": 9,
                "last_success_at_ms": 1_753_401_600_000_u64
            }]
        });
        let sync: ManagedProjectSyncResponse =
            serde_json::from_value(sync_payload).expect("exact sync payload should deserialize");
        assert_eq!(sync.status, "committed");
        assert_eq!(sync.removed, 2);
        assert_eq!(sync.projects[0].index_revision, 9);

        // statusPayload mirrors the exact per-project diagnostic response.
        // statusPayload 对应精确的每项目诊断响应。
        let status_payload = json!({
            "contract": MANAGED_CODEBASE_MEMORY_CONTRACT,
            "project_id": "project-a",
            "lifecycle": "indexing",
            "index_status": "running",
            "watcher_registered": true,
            "index_revision": 9,
            "last_success_at_ms": 1_753_401_600_000_u64,
            "nodes": 42,
            "edges": 17
        });
        let status: ManagedProjectStatusResponse = serde_json::from_value(status_payload)
            .expect("exact status payload should deserialize");
        assert_eq!(status._nodes, 42);
        assert_eq!(status._edges, 17);

        // reindexPayload mirrors both a newly started and a coalesced scheduling acknowledgement.
        // reindexPayload 对应新启动或合并调度的重建索引确认。
        let reindex_payload = json!({
            "contract": MANAGED_CODEBASE_MEMORY_CONTRACT,
            "project_id": "project-a",
            "accepted": true,
            "status": "running",
            "coalesced": true
        });
        let reindex: ManagedProjectReindexResponse = serde_json::from_value(reindex_payload)
            .expect("exact reindex payload should deserialize");
        assert!(reindex.accepted);
        assert!(reindex.coalesced);
    }

    /// Verifies strict host response models reject undeclared sidecar fields.
    /// 验证严格宿主响应模型拒绝未声明的 Sidecar 字段。
    #[test]
    fn managed_response_contract_rejects_unknown_fields() {
        let payload = json!({
            "contract": MANAGED_CODEBASE_MEMORY_CONTRACT,
            "project_id": "project-a",
            "accepted": true,
            "status": "started",
            "coalesced": false,
            "unexpected": true
        });

        assert!(serde_json::from_value::<ManagedProjectReindexResponse>(payload).is_err());
    }

    /// Verifies only normalized absolute project paths enter the private contract.
    /// 验证只有规范绝对项目路径才能进入私有契约。
    #[test]
    fn managed_project_path_rejects_relative_components() {
        let absolute_path = std::env::temp_dir().join("vulcan-codebase-memory-root");
        let relative_component_path = absolute_path.join("..").join("other");
        assert!(normalized_managed_project_path("relative/project").is_err());
        assert!(
            normalized_managed_project_path(&relative_component_path.to_string_lossy()).is_err()
        );
        assert_eq!(
            normalized_managed_project_path(&absolute_path.to_string_lossy())
                .expect("absolute path should normalize"),
            vulcan_agent_runtime::runtime_assets::native_host_path_text_from_path(&absolute_path)
        );
    }
}
