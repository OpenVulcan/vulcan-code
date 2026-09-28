//! Host-admitted workspace scopes retained through actual operation completion.
//! 宿主入场工作区作用域，保留至真实操作完成。

use super::*;
use luaskills::runtime::embedded::{EmbeddedCall, OperationHandle, json_size};
use std::path::Path;

/// A canonical workspace belongs to one actual operation, independently of the waiting caller.
/// 一个规范工作区属于一个真实操作，独立于等待调用方。
struct WorkspaceRequest {
    /// Observation cannot execute callbacks or keep the core scheduler alive.
    /// 观察者不能执行回调，也不会保持核心调度器存活。
    operation: OperationHandle,
    /// Canonical host input; business arguments never supply this path.
    /// 规范宿主输入；业务参数绝不提供此路径。
    workspace: PathBuf,
    /// Exact serialized correlation and path bytes retained for admission accounting.
    /// 入场记账保留的精确序列化关联与路径字节数。
    bytes: usize,
}

/// Bounded request scopes use the core's existing operation and input budget sources.
/// 有界请求作用域使用核心既有操作与输入预算来源。
pub(super) struct WorkspaceRequests {
    /// Core operation capacity bounds retained host request identities.
    /// 核心操作容量限制保留宿主请求身份数量。
    max_requests: usize,
    /// Core queue byte capacity also bounds this separate retained host metadata store.
    /// 核心队列字节容量同时限制此独立保留宿主元数据存储。
    max_bytes: usize,
    /// Aggregate bytes remain reserved even when an observer cancels or disappears.
    /// 即使观察者取消或离开，累计字节仍保持预留。
    bytes: usize,
    /// Exact host correlations map to their admitted native operation and workspace.
    /// 精确宿主关联映射到其已入场原生操作与工作区。
    requests: BTreeMap<String, WorkspaceRequest>,
}

impl WorkspaceRequests {
    /// Copy validated limits from `config`; return an empty, runtime-local registry.
    /// 从 `config` 复制已校验上限；返回空的运行时内注册表。
    pub(super) fn new(config: &EmbeddedRuntimeConfig) -> Self {
        Self {
            max_requests: config.max_operations,
            max_bytes: config.max_queued_bytes,
            bytes: 0,
            requests: BTreeMap::new(),
        }
    }

    /// Remove only proven terminal operations; return their count, preserving entries on observation errors.
    /// 仅移除已证明终结的操作；返回数量，观测错误时保留条目。
    fn reap(&mut self) -> EmbeddedResult<usize> {
        // Collect proofs before mutating accounting; cancellation intent alone is insufficient.
        // 在修改记账之前收集证明；仅有取消意图并不足够。
        let mut terminal = Vec::new();
        for (request_id, request) in &self.requests {
            if request.operation.snapshot()?.phase.is_terminal() {
                terminal.push(request_id.clone());
            }
        }
        for request_id in &terminal {
            // Every key above still belongs to this exclusively locked registry.
            // 上方每个键仍属于此独占锁定注册表。
            let request = self.requests.remove(request_id).ok_or_else(|| {
                internal("Plugin workspace request disappeared during reclamation.")
            })?;
            self.bytes -= request.bytes;
        }
        Ok(terminal.len())
    }
}

impl VulcanPluginEmbeddedHost {
    /// Admit `call` with trusted `workspace` and an end-to-end admission/execution `timeout`.
    /// 使用可信 `workspace` 及覆盖入场与执行的 `timeout` 接纳 `call`。
    /// Returns the actual operation; the host must provide a unique live request correlation in its context.
    /// 返回真实操作；宿主必须在上下文中提供存活期间唯一的请求关联。
    pub fn submit_in_workspace(
        &self,
        call: EmbeddedCall,
        workspace: &Path,
        timeout: Duration,
    ) -> EmbeddedResult<OperationHandle> {
        // Start the original budget before inspecting or cloning request metadata.
        // 在检查或克隆请求元数据之前启动原始预算。
        let deadline = workspace_deadline(timeout)?;
        // Borrow identity before moving the exact unchanged call into native admission.
        // 在将精确未改动调用移入原生入场前借用身份。
        let request_id = request_identity(&call)?.to_owned();
        self.admit_workspace(&request_id, workspace, deadline, |remaining| {
            self.runtime.submit(call, remaining)
        })
    }

    /// Admit `call` to the exact `session_id`, using trusted `workspace` and total `timeout`.
    /// 使用可信 `workspace` 与总 `timeout`，将 `call` 接纳到精确 `session_id`。
    /// Returns the actual operation; a mismatched pool is rejected before any request scope is published.
    /// 返回真实操作；池不匹配时在发布任何请求作用域之前拒绝。
    pub fn submit_session_in_workspace(
        &self,
        session_id: &str,
        call: EmbeddedCall,
        workspace: &Path,
        timeout: Duration,
    ) -> EmbeddedResult<OperationHandle> {
        // Session lookup consumes the same deadline as path preparation and execution.
        // 会话查询与路径准备及执行消耗同一截止时间。
        let deadline = workspace_deadline(timeout)?;
        if self.runtime.session(session_id)?.pool_id != call.pool_id {
            return Err(error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin workspace call does not match its fixed session pool.",
            ));
        }
        // Require a host declaration instead of synthesizing or overwriting correlation.
        // 要求宿主声明，不合成或覆盖关联。
        let request_id = request_identity(&call)?.to_owned();
        self.admit_workspace(&request_id, workspace, deadline, |remaining| {
            self.runtime.submit_session(
                session_id,
                call.export,
                call.arguments,
                call.context,
                remaining,
            )
        })
    }

    /// Reclaim scopes whose actual operations are terminal; return the removed count.
    /// 回收实际操作已终结的作用域；返回移除数量。
    /// This does not forget core evidence, cancel work, or infer completion from an absent observer.
    /// 此操作不遗忘核心证据、不取消工作，也不根据观察者缺失推断完成。
    pub fn reap_workspace_requests(&self) -> EmbeddedResult<usize> {
        self.authority
            .workspaces
            .lock()
            .map_err(|_| internal("Plugin workspace authority lock is poisoned."))?
            .reap()
    }

    /// Bind `request_id` and canonical `workspace` to the result of private nonblocking `submit`.
    /// 将 `request_id` 与规范 `workspace` 绑定到私有非阻塞 `submit` 的结果。
    /// The supplied absolute `deadline` includes preparation; return the original handle without waiting for Lua.
    /// 提供的绝对 `deadline` 包含准备时间；返回原始句柄，不等待 Lua。
    fn admit_workspace(
        &self,
        request_id: &str,
        workspace: &Path,
        deadline: Instant,
        submit: impl FnOnce(Duration) -> EmbeddedResult<OperationHandle>,
    ) -> EmbeddedResult<OperationHandle> {
        // Only host-supplied existing directories can become request workspaces.
        // 只有宿主提供的现存目录能够成为请求工作区。
        let workspace = workspace
            .canonicalize()
            .map_err(|failure| error(EmbeddedErrorCode::InvalidArgument, failure.to_string()))?;
        if !workspace.is_dir() {
            return Err(error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin request workspace is not a directory.",
            ));
        }
        // Refuse lossy path conversion instead of granting a different directory spelling.
        // 拒绝有损路径转换，避免授予不同目录拼写。
        let workspace_text = workspace.to_str().ok_or_else(|| {
            error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin request workspace is not valid UTF-8.",
            )
        })?;
        // The lock spans only bounded bookkeeping and the core's nonblocking queue admission.
        // 此锁仅跨越有界记账和核心非阻塞队列入场。
        let mut scopes = self
            .authority
            .workspaces
            .lock()
            .map_err(|_| internal("Plugin workspace authority lock is poisoned."))?;
        scopes.reap()?;
        if scopes.requests.contains_key(request_id) {
            return Err(error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin request correlation is already live.",
            ));
        }
        // Retained path and correlation bytes share one explicit aggregate accounting boundary.
        // 保留路径与关联字节共享一个显式累计记账边界。
        let bytes = json_size(&(request_id, workspace_text), scopes.max_bytes)?;
        if scopes.requests.len() >= scopes.max_requests
            || bytes > scopes.max_bytes.saturating_sub(scopes.bytes)
        {
            return Err(error(
                EmbeddedErrorCode::CapacityExceeded,
                "Plugin workspace request capacity is exhausted.",
            ));
        }
        // Time spent resolving paths or waiting for admission never extends execution time.
        // 解析路径或等待入场消耗的时间绝不延长执行时间。
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|value| !value.is_zero())
            .ok_or_else(|| {
                error(
                    EmbeddedErrorCode::DeadlineExceeded,
                    "Plugin request deadline elapsed before admission.",
                )
            })?;
        // Core admission never runs Lua inline. A fast callback waits for this exact ID publication.
        // 核心入场绝不内联运行 Lua；快速回调等待此精确 ID 发布。
        let operation = submit(remaining)?;
        scopes.bytes += bytes;
        scopes.requests.insert(
            request_id.to_owned(),
            WorkspaceRequest {
                operation: operation.clone(),
                workspace,
                bytes,
            },
        );
        Ok(operation)
    }
}

impl HostAuthority {
    /// Resolve `invocation` to its registered request workspace or the explicitly uncorrelated `domain` workspace.
    /// 将 `invocation` 解析到已登记请求工作区，或明确无关联的 `domain` 工作区。
    /// Return a copied path; unknown or foreign request identities fail before any host service executes.
    /// 返回复制路径；未知或外来请求身份在任何宿主服务执行前失败。
    pub(super) fn workspace(
        &self,
        domain: &ExecutionAuthority,
        invocation: &CapabilityInvocation,
    ) -> EmbeddedResult<PathBuf> {
        // Absence is the declared initialization/independent-finalization and fixed-workspace mode.
        // 缺省表示已声明的初始化／独立关闭及固定工作区模式。
        let Some(request_id) = invocation.caller.request_id.as_deref() else {
            return Ok(domain.workspace.clone());
        };
        // Runtime and execution-domain identity are already checked by the native admission entry.
        // 原生入场入口已检查运行时与执行域身份。
        let scopes = self
            .workspaces
            .lock()
            .map_err(|_| internal("Plugin workspace authority lock is poisoned."))?;
        // Operation identity prevents a second call from borrowing even a known live correlation.
        // 操作身份防止第二次调用借用已知存活关联。
        let scope = scopes
            .requests
            .get(request_id)
            .filter(|scope| scope.operation.id() == invocation.caller.operation_id)
            .ok_or_else(|| {
                error(
                    EmbeddedErrorCode::PermissionDenied,
                    "Plugin request workspace authority is absent or belongs to another operation.",
                )
            })?;
        Ok(scope.workspace.clone())
    }
}

/// Borrow the explicitly provided request correlation from `call`; return an error when absent.
/// 从 `call` 借用明确提供的请求关联；缺省时返回错误。
fn request_identity(call: &EmbeddedCall) -> EmbeddedResult<&str> {
    call.context
        .request_context
        .as_ref()
        .and_then(|context| context.request_id.as_deref())
        .ok_or_else(|| {
            error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin workspace admission requires an explicit host request correlation.",
            )
        })
}

/// Convert positive `timeout` into one absolute deadline; return an error for zero or clock overflow.
/// 将正值 `timeout` 转换为唯一绝对截止时间；零值或时钟溢出时返回错误。
fn workspace_deadline(timeout: Duration) -> EmbeddedResult<Instant> {
    if timeout.is_zero() {
        return Err(error(
            EmbeddedErrorCode::InvalidArgument,
            "Plugin request timeout must be positive.",
        ));
    }
    Instant::now().checked_add(timeout).ok_or_else(|| {
        error(
            EmbeddedErrorCode::InvalidArgument,
            "Plugin request timeout is out of range.",
        )
    })
}
