//! Transactional backend settings for the shared media delivery policy.
//! 共享媒体投递策略的事务化后台设置。

use serde::{Deserialize, Serialize};
use vulcan_provider_adapters::{
    MEDIA_INLINE_ENCODED_HARD_LIMIT_BYTES, MEDIA_INLINE_RAW_HARD_LIMIT_BYTES, MediaDeliveryPolicy,
    PersistedAiConfiguration,
};
use vulcan_service_api::ServiceEventEmitter;

use super::publication::mutate_public_ai_configuration_and_publish_in_transaction;
use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    public_ai_runtime_and_configuration,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Complete settings update; omitted, misspelled and partially supplied policy fields are rejected.
/// 完整设置更新；缺失、拼写错误或部分提供的策略字段均被拒绝。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicAiMediaDeliveryPolicySaveRequest {
    /// Effective settings revision read by the editor; stale whole-policy saves are rejected.
    /// 编辑器读取的生效设置修订；拒绝陈旧的完整策略保存。
    expected_revision: String,
    /// Full versioned policy with independent conversation and extension settings.
    /// 包含独立会话与扩展设置的完整版本化策略。
    policy: MediaDeliveryPolicy,
}

/// Explicit revision-bound reset command; no model-controlled purpose or threshold fields are accepted.
/// 显式绑定修订的重置命令，不接受模型控制的用途或阈值字段。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicAiMediaDeliveryPolicyResetRequest {
    /// Effective revision the user explicitly chose to reset.
    /// 用户显式选择重置的生效修订。
    expected_revision: String,
}

/// Browser projection of effective settings and backend-owned configuration ceilings.
/// 生效设置与后端持有配置上限的浏览器投影。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiMediaDeliverySettingsSnapshot {
    /// Effective policy, including memory-only defaults for configurations without explicit settings.
    /// 生效策略，包含未显式设置配置的仅内存默认值。
    policy: MediaDeliveryPolicy,
    /// Content fingerprint distinct from provider manifest and route revisions.
    /// 与供应商清单及路由修订相互独立的内容指纹。
    revision: String,
    /// Maximum configurable per-resource raw inline threshold, not an upload size cap.
    /// 可配置逐资源原始内联阈值的最大值，不是上传大小上限。
    max_inline_raw_bytes: u64,
    /// Maximum configurable aggregate Base64 budget, excluding protocol overhead.
    /// 可配置累计 Base64 预算的最大值，不含协议开销。
    max_total_encoded_bytes: u64,
}

impl PublicAiMediaDeliverySettingsSnapshot {
    /// Projects `configuration` without changing it; returns explicit invalid-policy errors.
    /// 投影 `configuration` 而不修改它；策略无效时返回明确错误。
    pub(super) fn from_configuration(
        configuration: &PersistedAiConfiguration,
    ) -> Result<Self, String> {
        // A copied effective policy cannot be changed by subsequent configuration publication.
        // 复制的生效策略不会受后续配置发布影响。
        let policy = configuration.effective_media_delivery_policy();
        Ok(Self {
            policy,
            revision: policy.fingerprint()?,
            max_inline_raw_bytes: MEDIA_INLINE_RAW_HARD_LIMIT_BYTES,
            max_total_encoded_bytes: MEDIA_INLINE_ENCODED_HARD_LIMIT_BYTES,
        })
    }
}

/// Saves all media settings through the existing configuration and Workbench transaction.
/// 通过现有配置与工作台事务保存全部媒体设置。
/// `app` publishes events, `state` owns configuration, and `request` contains validated settings.
/// `app` 发布事件，`state` 持有配置，`request` 携带待校验设置。
/// Returns the published browser snapshot; validation or publication errors preserve the prior state.
/// 返回已发布浏览器快照；校验或发布失败时保留原状态。
pub(crate) async fn service_save_public_ai_media_delivery_policy(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiMediaDeliveryPolicySaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    request.policy.validate()?;
    publish_media_delivery_policy(app, state, Some(request.policy), request.expected_revision).await
}

/// Restores defaults for an explicit settings action and returns the published backend snapshot.
/// 为显式设置动作恢复默认值，返回已发布后端快照。
/// `app` publishes events, `state` owns configuration, and `request` identifies the revision being reset.
/// `app` 发布事件，`state` 持有配置，`request` 标识被重置的修订。
pub(crate) async fn service_reset_public_ai_media_delivery_policy(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiMediaDeliveryPolicyResetRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    publish_media_delivery_policy(app, state, None, request.expected_revision).await
}

/// Publishes an explicitly saved `Some(policy)` or reset `None` using the same atomic owner.
/// 使用同一原子所有者发布显式保存的 `Some(policy)` 或重置的 `None`。
/// `app` and `state` retain their service ownership; returns the snapshot or an unchanged-state error.
/// `app` 与 `state` 保持服务所有权；返回快照，或保持状态不变的错误。
/// `expected_revision` must match the effective before-image inside the same configuration transaction.
/// `expected_revision` 必须在同一配置事务内匹配生效前镜像。
async fn publish_media_delivery_policy(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    policy: Option<MediaDeliveryPolicy>,
    expected_revision: String,
) -> Result<PublicAiSettingsSnapshot, String> {
    // Serialize the exact before-image, persistence and runtime publication as one transaction.
    // 将精确前镜像、持久化与运行时发布串行化为一个事务。
    let transaction = begin_public_ai_configuration_transaction().await;
    // Only this paired configuration owns the write; no global active-model re-selection occurs.
    // 仅此配对配置持有写入权，不重新选择全局活动模型。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // Compare inside the same transaction as mutation so another editor cannot be silently overwritten.
    // 在与修改相同的事务内比较，防止静默覆盖另一编辑器。
    require_media_delivery_revision(&current_configuration, &expected_revision)?;
    // Publication uses existing compensation paths and preserves all unrelated settings.
    // 发布使用既有补偿路径，保留全部无关设置。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存媒体投递策略失败",
        |configuration| match policy {
            Some(policy) => configuration.set_media_delivery_policy(policy),
            None => {
                configuration.reset_media_delivery_policy();
                Ok(())
            }
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Validates `expected_revision` against the effective policy in `configuration` without mutation.
/// 无修改地根据 `configuration` 中的生效策略校验 `expected_revision`。
/// Returns a stable conflict code before persistence or runtime publication when the editor is stale.
/// 编辑器陈旧时，在持久化或运行时发布之前返回稳定冲突代码。
fn require_media_delivery_revision(
    configuration: &PersistedAiConfiguration,
    expected_revision: &str,
) -> Result<(), String> {
    if configuration
        .effective_media_delivery_policy()
        .fingerprint()?
        != expected_revision
    {
        return Err(String::from(
            "MEDIA_DELIVERY_POLICY_CHANGED: media delivery settings changed; reload before saving or resetting.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies reading defaults is non-mutating and the snapshot advertises exact ceilings.
    /// 验证读取默认值不产生写入，且快照公开精确上限。
    #[test]
    fn media_delivery_settings_snapshot_keeps_defaults_in_memory() {
        // The source lacks explicit media settings and must remain unchanged.
        // 源配置没有显式媒体设置，必须保持不变。
        let configuration = PersistedAiConfiguration::default();
        // The browser receives effective settings and a stable revision from the same snapshot.
        // 浏览器从同一快照获取生效设置及稳定修订。
        let snapshot = PublicAiMediaDeliverySettingsSnapshot::from_configuration(&configuration)
            .expect("media policy fixture or operation must be valid");
        assert_eq!(snapshot.policy, MediaDeliveryPolicy::default());
        assert_eq!(
            snapshot.revision,
            snapshot
                .policy
                .fingerprint()
                .expect("media policy fixture or operation must be valid")
        );
        assert_eq!(snapshot.max_inline_raw_bytes, 50_331_648);
        assert_eq!(snapshot.max_total_encoded_bytes, 67_108_864);
        assert!(configuration.media_delivery_policy.is_none());
    }

    /// Rejects partial updates and purpose injection while preserving exact zero values.
    /// 拒绝部分更新和用途注入，同时保留精确零值。
    #[test]
    fn media_delivery_settings_request_is_exact() {
        assert!(
            serde_json::from_value::<PublicAiMediaDeliveryPolicyResetRequest>(
                json!({"expected_revision": "read-revision"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<PublicAiMediaDeliveryPolicyResetRequest>(
                json!({"expected_revision": "read-revision", "purpose": "independent_extension"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<PublicAiMediaDeliveryPolicySaveRequest>(
                json!({"expected_revision": "read-revision", "policy": {"version": 1}})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<PublicAiMediaDeliveryPolicySaveRequest>(json!({"expected_revision": "read-revision", "policy": MediaDeliveryPolicy::default(), "purpose": "independent_extension"})).is_err());
        assert_eq!(
            serde_json::from_value::<PublicAiMediaDeliveryPolicySaveRequest>(
                json!({"expected_revision": "read-revision", "policy": MediaDeliveryPolicy::default()})
            )
            .expect("media policy fixture or operation must be valid")
            .policy
            .conversation
            .image
            .with_upload_max_bytes,
            0
        );
    }

    /// Verifies a stale full editor or reset cannot pass the mutation transaction's revision gate.
    /// 验证陈旧的完整编辑器或重置不能通过修改事务的修订门。
    #[test]
    fn media_delivery_settings_rejects_stale_revision_before_mutation() {
        // A valid initial read observes the same memory-only defaults as the browser snapshot.
        // 有效初始读取观察到与浏览器快照相同的仅内存默认值。
        let mut configuration = PersistedAiConfiguration::default();
        let revision = configuration
            .effective_media_delivery_policy()
            .fingerprint()
            .expect("initial revision");
        require_media_delivery_revision(&configuration, &revision).expect("fresh revision");
        let mut changed = configuration.effective_media_delivery_policy();
        changed.independent_extension.file.without_upload_max_bytes = 17;
        configuration
            .set_media_delivery_policy(changed)
            .expect("concurrent edit");
        let before = serde_json::to_value(&configuration).expect("before image");
        assert!(
            require_media_delivery_revision(&configuration, &revision)
                .expect_err("stale revision")
                .starts_with("MEDIA_DELIVERY_POLICY_CHANGED:")
        );
        assert_eq!(
            serde_json::to_value(&configuration).expect("after image"),
            before
        );
        assert!(
            serde_json::from_value::<PublicAiMediaDeliveryPolicyResetRequest>(json!({})).is_err()
        );
        assert!(
            serde_json::from_value::<PublicAiMediaDeliveryPolicySaveRequest>(
                json!({"policy": changed})
            )
            .is_err()
        );
    }
}
