//! Automatic admission binds durable policy to existing target receipts and their original native operations.
//! 自动入场将持久策略绑定至既有目标回执及原原生操作。

use super::*;

/// Trusted host provenance identifies a lifecycle trigger and the exact policy that authorized its target.
/// 可信宿主来源标识生命周期触发及授权该目标的精确策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginAutomaticPrewarmOrigin {
    /// Host-issued lifecycle identity; transport retries must retain it instead of creating another trigger.
    /// 宿主签发生命周期身份；传输重试必须保留它，不能创建另一触发。
    pub trigger_id: String,
    /// Exact acknowledged policy revision; a later change does not authorize the original target.
    /// 精确已确认策略修订；后续变化不授权原目标。
    pub policy_revision: String,
}

impl VulcanPluginRuntimeManager {
    /// Prepare request under origin's exact policy; return the same retained receipt for an identical trigger.
    /// 按 origin 的精确策略准备 request；相同触发返回同一保留回执。
    /// The trusted service must validate lifecycle ownership; this operation does not start Lua.
    /// 可信服务必须校验生命周期归属；此操作不启动 Lua。
    pub fn prepare_automatic_tool_prewarm(
        &self,
        request: VulcanPluginPrewarmTargetRequest,
        origin: VulcanPluginAutomaticPrewarmOrigin,
    ) -> Result<VulcanPluginFormalCallStatus, VulcanPluginRuntimeError> {
        for identity in [&origin.trigger_id, &origin.policy_revision] {
            if identity.is_empty() || identity.contains('\0') {
                return Err(VulcanPluginRuntimeError::new(
                    "Automatic prewarm identities must be nonempty and contain no zero byte.",
                ));
            }
        }
        // Original manifest and workspace validation remains owned by the ordinary prewarm input builder.
        // 原清单及工作区校验仍由普通预热输入构建器负责。
        let owner = self.required_formal_owner()?;
        {
            // Re-observation precedes catalog resolution, so an original receipt survives later package changes.
            // 重新观测先于目录解析，使原回执在后续包变化后仍可读取。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| calls_error())?;
            let calls = owner.calls.lock().map_err(|_| calls_error())?;
            if let Some(original) = original_call(
                &calls,
                &CallAdmission::PrewarmTarget {
                    request: request.clone(),
                    automatic: Some(origin.clone()),
                },
            )? {
                return original.status();
            }
        }
        let (plugin, mut input) = self.reusable_prewarm_input(&owner, request, true)?;
        let CallAdmission::PrewarmTarget { automatic, .. } = &mut input.admission else {
            unreachable!("target input builder always returns target admission");
        };
        *automatic = Some(origin);
        self.register_owned_formal_call(&owner, plugin, input)?
            .status()
    }
}

/// Find an identical original trigger in calls; return its retained owner or reject a changed target.
/// 在 calls 中查找相同原触发；返回保留所有者，或拒绝被更改目标。
/// The caller holds the publication gate and call registry, so concurrent preparation cannot duplicate it.
/// 调用方持有发布门及调用表，因此并发准备不能复制该触发。
pub(in crate::runtime::formal::calls) fn original_call(
    calls: &BTreeMap<String, Arc<FormalCall>>,
    admission: &CallAdmission,
) -> Result<Option<Arc<FormalCall>>, VulcanPluginRuntimeError> {
    let CallAdmission::PrewarmTarget {
        request,
        automatic: Some(origin),
    } = admission
    else {
        return Ok(None);
    };
    for call in calls.values() {
        let state = call.lock()?;
        let Some(target) = &state.prewarm else {
            continue;
        };
        if target.automatic.as_ref() == Some(origin)
            && target.request.owner_session_id == request.owner_session_id
            && target.request.tool_id == request.tool_id
        {
            if target.request != *request {
                return Err(VulcanPluginRuntimeError::new(
                    "Automatic prewarm trigger already belongs to another target.",
                ));
            }
            return Ok(Some(Arc::clone(call)));
        }
    }
    Ok(None)
}

/// Validate automatic admission against the live policy while publication remains excluded; manual work is unchanged.
/// 发布保持排除时，对照活动策略校验自动入场；手动工作不变。
pub(in crate::runtime::formal::calls) fn validate_admission(
    owner: &ManagedFormalRuntime,
    admission: &CallAdmission,
    publication: &std::sync::MutexGuard<'_, ()>,
) -> Result<(), VulcanPluginRuntimeError> {
    if let CallAdmission::PrewarmTarget {
        request,
        automatic: Some(origin),
    } = admission
    {
        owner.validate_automatic_prewarm_policy(request, origin, publication)?;
    }
    Ok(())
}

/// Validate a retained call's automatic provenance before any additional initialization; return exact policy errors.
/// 任何额外初始化前校验保留调用的自动来源；返回精确策略错误。
pub(in crate::runtime::formal::calls) fn validate_call(
    owner: &ManagedFormalRuntime,
    call: &FormalCall,
    publication: &std::sync::MutexGuard<'_, ()>,
) -> Result<(), VulcanPluginRuntimeError> {
    let state = call.lock()?;
    if let Some(target) = &state.prewarm
        && let Some(origin) = &target.automatic
    {
        owner.validate_automatic_prewarm_policy(&target.request, origin, publication)?;
    }
    Ok(())
}
