//! Unpublished initialization uses the original formal parent, call registry and capacity admission.
//! 未发布初始化使用原正式父级、调用注册表及容量入场。

use super::*;
use crate::VulcanPluginActivationCandidate;

impl VulcanPluginRuntimeManager {
    /// Initialize candidate's declared Lua module under the existing probe deadline without public publication.
    /// 在既有探测期限内初始化 candidate 声明的 Lua 模块，不进行公开发布。
    /// Success requires tracked initialization and cleanup; timeout may return earlier while actual ownership remains.
    /// 成功要求受跟踪初始化及清理完成；超时可能提前返回，而实际所有权继续保留。
    /// Package top-level effects are possible and are never replayed here.
    /// 插件包顶层副作用可能发生，此处绝不重放。
    /// This does not invoke install callbacks or validate external connections; unfinished work keeps commit and abort fenced.
    /// 此接口不调用安装回调或验证外部连接；未完成工作持续阻止提交及中止。
    pub async fn probe_activation_candidate(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // One original deadline includes authority checks, dependency preparation and actual initialization.
        // 单个原始截止时间包含权威检查、依赖准备及实际初始化。
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(VULCAN_PLUGIN_PROBE_TIMEOUT_MS))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Candidate probe deadline is out of range.")
            })?;
        self.invoke_formal_candidate_probe(candidate, deadline)
            .await
    }

    /// Run candidate under the original absolute deadline; return the existing tracked call's result.
    /// 在原绝对 deadline 下运行 candidate；返回既有受跟踪调用的结果。
    pub(super) async fn invoke_formal_candidate_probe(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
        deadline: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Only an actual inspected transaction can create this private execution authority.
        // 只有实际已检查事务可以创建此私有执行权威。
        let authority = CandidateExecution::new(&self.state, candidate)?;
        // Validated content-only packages have no module initialization to run.
        // 已校验纯内容插件包没有需要运行的模块初始化。
        let manifest = authority.plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Activation candidate has no valid manifest.")
        })?;
        if !manifest.has_lua_runtime() {
            return Ok(());
        }
        self.validate_plugin_requirements_in_scope(
            &authority.plugin,
            VulcanPluginBridgeLeaseScope::EnablingProbe,
        )?;
        // This is the already attached parent; candidate validation cannot create another scheduler.
        // 此对象为已经附加的父级；候选校验不能创建另一个调度器。
        let owner = self.required_formal_owner()?;
        // The frozen descriptor is retained independently of any public catalog observation.
        // 固定描述符独立于任何公开目录观测保留。
        let plugin = authority.plugin.clone();
        // Retain the original request identity for timeout diagnostics after admission consumes its authority.
        // 在入场消费权威后仍为超时诊断保留原请求身份。
        let request_id = authority.request_id;
        // Candidate work receives the existing preparation, byte accounting and cancellation supervision.
        // 候选工作使用既有准备、字节计量及取消监督。
        let input = Preparation {
            workspace: CallWorkspace::LifecycleEnvironment,
            arguments: json!({}),
            finalizer: None,
            scope: VulcanPluginBridgeLeaseScope::EnablingProbe,
            security_partition: format!("candidate:{}:{}", plugin.id, authority.request_id),
            deadline,
            admission: CallAdmission::CandidateProbe { authority },
        };
        tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            self.invoke_owned_formal_call(owner, plugin, input),
        )
        .await
        .map_err(|_| VulcanPluginRuntimeError::new(format!(
            "Activation candidate probe {request_id} timed out; inspect its original calls for actual retirement."
        )))?
        .map(|_| ())
    }

    /// Query retained candidate calls for request_id in this manager's original parent without starting or replaying work.
    /// 在此管理器原父级中查询 request_id 的保留候选调用，不启动或重放工作。
    /// Empty results mean no retained calls now, not proof that initialization never ran or the journal completed.
    /// 空结果表示当前没有保留调用，不证明初始化从未运行或日志已完成。
    pub fn activation_candidate_calls(
        &self,
        request_id: Uuid,
    ) -> Result<Vec<VulcanPluginFormalCallStatus>, VulcanPluginRuntimeError> {
        // An unattached parent has no retained work and must not be created by inspection.
        // 未附加父级没有保留工作，检查不得创建它。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(Vec::new());
        };
        owner
            .calls
            .lock()
            .map_err(|_| calls_error())?
            .values()
            .filter(|call| {
                call.candidate
                    .as_ref()
                    .is_some_and(|authority| authority.request_id == request_id)
            })
            .map(|call| call.status())
            .collect()
    }

    /// Cancel and drain currently admitted candidate probes for request_id, leaving ordinary plugin calls untouched.
    /// 取消并排空 request_id 当前已接纳候选探测，保持普通插件调用不变。
    /// Returns exact original-call cleanup evidence; unfinished work continues to fence storage mutation.
    /// 返回精确原调用清理证据；未完成工作继续阻止存储变更。
    pub async fn drain_activation_candidate_probes(
        &self,
        request_id: Uuid,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        tokio::runtime::Handle::try_current().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Candidate drain requires a Tokio runtime: {error}"
            ))
        })?;
        // The drain supervisor survives an interrupted external waiter.
        // 排空监督者在外部等待者中断后继续存活。
        let manager = self.clone();
        tokio::spawn(async move {
            // Query original ownership only; absence never creates a replacement parent.
            // 仅查询原所有权；缺失绝不创建替代父级。
            let Some(owner) = manager.formal_runtime_owner()? else {
                return Ok(VulcanPluginDrainReport::default());
            };
            // Exact request filtering prevents cancelling old-version work in the same plugin namespace.
            // 精确请求过滤防止取消相同插件命名空间中的旧版本工作。
            let calls = owner
                .calls
                .lock()
                .map_err(|_| calls_error())?
                .values()
                .filter(|call| {
                    call.candidate
                        .as_ref()
                        .is_some_and(|authority| authority.request_id == request_id)
                })
                .cloned()
                .collect();
            manager.drain_selected_formal_calls(&owner, calls).await
        })
        .await
        .map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Candidate drain supervisor failed: {error}"))
        })?
    }
}
