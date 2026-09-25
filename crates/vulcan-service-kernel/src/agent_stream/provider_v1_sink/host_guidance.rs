//! Versioned host guidance recognition and append-only request upgrades.
//! 版本化宿主指引识别与请求追加式升级。

use super::*;
use vulcan_ai_contract::{HOST_MESSAGE_GUIDANCE_VERSION, host_message_guidance_version};
use vulcan_session_core::v1::dynamic_context::DynamicContextInput;
use vulcan_session_core::v1::event::SystemContextScope;

/// Stable producer identity shared by all guidance revisions and their compaction carry-forward.
/// 所有指引修订及压缩保留共用的稳定生产者标识。
pub(super) const HOST_GUIDANCE_UPDATE_KEY: &str = "dynamic.host_message_guidance";

/// Validates host identity and returns the registered text revision in `baseline`, if present.
/// 校验宿主身份，返回 `baseline` 中存在的已登记文本修订。
/// Unknown text, forged coordinates and duplicate sources fail without changing history.
/// 未知正文、伪造坐标和重复来源直接失败，不修改历史。
pub(super) fn baseline_version(
    baseline: &LockedBaselineProjection,
) -> Result<Option<usize>, DomainError> {
    let mut sources = baseline
        .locked_sources()
        .iter()
        .filter(|source| source.node_id == "host-message-guidance");
    let Some(source) = sources.next() else {
        return Ok(None);
    };
    if sources.next().is_some()
        || source.kind != LockedSourceKind::BaselineHooks
        || source.target != LockedPromptTarget::System
        || source.source_label.as_deref() != Some("host_message_guidance")
        || source.source_id.as_deref() != Some("v1:host_message_guidance")
    {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "host_message_guidance",
            field: "source_identity",
        });
    }
    host_message_guidance_version(&source.content)
        .map(Some)
        .ok_or(DomainError::ImmutableFieldMutation {
            entity: "host_message_guidance",
            field: "unregistered_revision",
        })
}

/// Builds the upgrade producer from the actual initialized runtime, or the admitted fresh candidate.
/// 从实际已初始化运行时或获准的新候选构造升级生产者。
/// A current baseline removes obsolete overlays; older recognized baselines retain their exact bytes.
/// 当前基线撤下已过时覆盖层；已识别旧基线保留精确字节。
pub(super) fn upgrade_input(
    document: &vulcan_session_core::v1::document::SessionDocument,
    candidate: &LockedBaselineProjection,
) -> Result<DynamicContextInput, DomainError> {
    let initialized =
        LockedBaselineProjection::from_initialized_runtime_lock(&document.runtime_lock)?;
    let version = baseline_version(initialized.as_ref().unwrap_or(candidate))?;
    if initialized.is_some() && version.is_none() {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "host_message_guidance",
            field: "missing_initialized_source",
        });
    }
    let body = version.filter(|version| *version < HOST_MESSAGE_GUIDANCE_VERSION).map(|_| format!(
        "Host message guidance revision {HOST_MESSAGE_GUIDANCE_VERSION} supersedes earlier host-message guidance from this point onward. This update does not start a task or require acknowledgement.\n\n{HOST_MESSAGE_GUIDANCE}"
    ));
    Ok(DynamicContextInput::new(
        HOST_GUIDANCE_UPDATE_KEY,
        body,
        SystemContextScope::Session,
        None,
    ))
}
