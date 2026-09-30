//! Original predecessor callbacks reuse formal call ownership and retain their exact request through native retirement.
//! 原前驱回调复用正式调用所有权，并在原生退役期间保留精确请求。

use super::*;
use crate::{VulcanPluginActivationCandidate, VulcanPluginPredecessorDrain};

impl VulcanPluginRuntimeManager {
    /// Invoke only drain's frozen old disable callback under candidate's actual original request ownership.
    /// 仅在 candidate 实际原请求所有权下调用 drain 固定旧停用回调。
    /// Return after observed invocation and cleanup; no callback means no effect receipt or execution environment.
    /// 观测调用及清理后返回；无回调表示没有副作用回执或执行环境。
    pub async fn disable_activation_predecessor(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
        drain: &VulcanPluginPredecessorDrain,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // The normal lifecycle budget includes preparation, initialization and callback execution.
        // 普通生命周期预算包含准备、初始化及回调执行。
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(
                crate::runtime::VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS,
            ))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Predecessor callback deadline is out of range.")
            })?;
        self.invoke_formal_predecessor_disable(candidate, drain, deadline)
            .await
    }

    /// Execute one original disable before deadline; preserve normal supervision and a durable one-attempt receipt.
    /// 在 deadline 前执行单个原停用；保留普通监督及持久单次尝试回执。
    /// Candidate and drain must denote the same prepared original; errors never make this callback replayable.
    /// candidate 与 drain 必须指向相同准备原请求；错误绝不使此回调可重放。
    pub(super) async fn invoke_formal_predecessor_disable(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
        drain: &VulcanPluginPredecessorDrain,
        deadline: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // The completed original drain, not current target metadata, fixes the old package and native scope.
        // 已完成原排空固定旧包及原生作用域，而非当前目标元数据。
        let authority = ActivationExecution::predecessor_disable(&self.state, candidate, drain)?;
        // Read only the original package's declaration; the new package may have entirely different exports.
        // 仅读取原包声明；新包可能具有完全不同的导出。
        let manifest = authority.plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original predecessor manifest is unavailable.")
        })?;
        // Missing old Lua or disable declaration is an explicit no-effect branch.
        // 缺少旧 Lua 或停用声明是显式无副作用分支。
        let Some(export) = manifest
            .lua_runtime
            .as_ref()
            .and_then(|runtime| runtime.lifecycle.disable.clone())
        else {
            return Ok(());
        };
        // Exact scope remains fixed through module initialization and native cleanup.
        // 精确作用域在模块初始化及原生清理期间保持固定。
        let scope = authority.operation.scope();
        self.validate_plugin_requirements_in_scope(&authority.plugin, scope)?;
        // One existing formal parent owns both the old callback and later candidate work.
        // 单个既有正式父级同时拥有旧回调及后续候选工作。
        let owner = self.required_formal_owner()?;
        // Frozen old configuration and credential pins are carried by the actual descriptor.
        // 固定旧配置及凭据固定句柄由实际描述符携带。
        let plugin = authority.plugin.clone();
        // The same durable UUID enters the plugin's event envelope for external deduplication.
        // 相同持久 UUID 进入插件事件信封，供外部去重。
        let request_id = authority.request_id;
        // Existing lifecycle execution consumes one explicit export and never adds tool or hook finalizers.
        // 既有生命周期执行消费单个显式导出，绝不添加工具或 Hook 关闭函数。
        let input = Preparation {
            workspace: CallWorkspace::LifecycleEnvironment,
            arguments: json!({"event":"disable","plugin_id":plugin.id,"request_id":request_id.to_string()}),
            finalizer: None,
            scope,
            security_partition: format!("predecessor:{}:{request_id}:disable", plugin.id),
            deadline,
            admission: CallAdmission::ActivationLifecycle { authority, export },
        };
        // Persist the old callback attempt before environment writes or any Lua initialization.
        // 环境写入或任何 Lua 初始化之前持久保存旧回调尝试。
        let effect = candidate
            .begin_effect_attempt(crate::VulcanPluginActivationEffect::DisablePredecessor)
            .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?;
        // Native work keeps the original request lock independently from this finite observation.
        // 原生工作独立于此有限观测保留原请求锁。
        let result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),
            self.invoke_owned_formal_call(owner, plugin, input)).await
            .map_err(|_| VulcanPluginRuntimeError::new(format!("Original predecessor callback {request_id} timed out; inspect its actual calls before recovery.")))
            .and_then(|result| result.map(|_| ()));
        effect.finish(result.is_ok()).map_err(|failure| VulcanPluginRuntimeError::new(format!(
            "Original predecessor callback {request_id} receipt is uncertain: {failure}; inspect the original request."
        )))?;
        result
    }

    /// Query actual old-version callbacks for request_id without admitting work or selecting another request.
    /// 查询 request_id 的实际旧版本回调，不接纳工作或选择其他请求。
    /// Return the existing formal status records; candidate calls are intentionally outside this query.
    /// 返回既有正式状态记录；此查询范围明确不包含候选调用。
    pub fn activation_predecessor_calls(
        &self,
        request_id: Uuid,
    ) -> Result<Vec<VulcanPluginFormalCallStatus>, VulcanPluginRuntimeError> {
        // A query cannot allocate a replacement parent.
        // 查询不能分配替代父级。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(Vec::new());
        };
        owner
            .calls
            .lock()
            .map_err(|_| calls_error())?
            .values()
            .filter(|call| {
                call.activation.as_ref().is_some_and(|authority| {
                    authority.request_id == request_id && !authority.operation.is_candidate()
                })
            })
            .map(|call| call.status())
            .collect()
    }

    /// Continue retirement of request_id's original old callbacks; return existing cleanup counts and failures.
    /// 继续退役 request_id 的原旧回调；返回既有清理计数及失败。
    /// Cancellation of the observer neither releases native request ownership nor replays the callback.
    /// 观察者取消既不释放原生请求所有权，也不重放回调。
    pub async fn drain_activation_predecessor_calls(
        &self,
        request_id: Uuid,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        tokio::runtime::Handle::try_current().map_err(|failure| {
            VulcanPluginRuntimeError::new(format!(
                "Predecessor callback drain requires a Tokio runtime: {failure}"
            ))
        })?;
        // Retain only the existing manager for the owned drain supervisor.
        // 仅为自有排空监督器保留既有管理器。
        let manager = self.clone();
        tokio::spawn(async move {
            // No parent means no retained callback; inspection never creates one.
            // 没有父级表示没有保留回调；检查绝不创建父级。
            let Some(owner) = manager.formal_runtime_owner()? else {
                return Ok(VulcanPluginDrainReport::default());
            };
            // Select both the exact request and old-version operation class.
            // 同时选择精确请求及旧版本操作类别。
            let calls = owner
                .calls
                .lock()
                .map_err(|_| calls_error())?
                .values()
                .filter(|call| {
                    call.activation.as_ref().is_some_and(|authority| {
                        authority.request_id == request_id && !authority.operation.is_candidate()
                    })
                })
                .cloned()
                .collect();
            manager.drain_selected_formal_calls(&owner, calls).await
        })
        .await
        .map_err(|failure| {
            VulcanPluginRuntimeError::new(format!(
                "Predecessor callback drain supervisor failed: {failure}"
            ))
        })?
    }
}
