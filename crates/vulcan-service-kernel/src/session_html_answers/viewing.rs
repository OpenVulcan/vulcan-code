//! Exact stored-source authority for the Core-owned HTML viewer transport.
//! Core 自有 HTML 查看传输层的精确持久源码权威。

use crate::{SharedServiceBridgeState, session_runtime_registry_v1::SessionRuntimeError};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};
use vulcan_service_api::{
    HtmlAnswerRevokeRequest, HtmlAnswerSourceIdentity, HtmlAnswerViewError,
    HtmlAnswerViewLinkRequest, HtmlAnswerViewLinkResponse,
};
use vulcan_service_host::html_answers::{
    HtmlAnswerContent, HtmlAnswerRuntime, HtmlAnswerRuntimeConfig,
};
use vulcan_session_core::v1::{
    EventBody, Seq,
    html_answers::{HtmlAnswerAvailability, project_html_answers},
};
use vulcan_workspace_store::workspace_store::{
    locate_embedded_task_session_v1, locate_session_v1_storage,
};

/// Construct the transport from discovered local authority or a complete explicit proxy configuration.
/// 根据已发现本地权威或完整显式代理配置构造传输层。
/// `state` is weakly captured; `local_origin` and `static_root` come from the owning Core listener.
/// 对 `state` 捕获弱引用；`local_origin` 和 `static_root` 来自所属 Core 监听器。
pub(crate) fn create_html_answer_runtime(
    state: &SharedServiceBridgeState,
    local_origin: &str,
    static_root: Option<PathBuf>,
) -> Result<Arc<HtmlAnswerRuntime>, String> {
    let viewer = optional_environment("VULCAN_HTML_VIEWER_ORIGIN")?;
    let renderer = optional_environment("VULCAN_HTML_RENDERER_ORIGIN")?;
    let port = optional_environment("VULCAN_HTML_RENDERER_PORT")?;
    let (viewer_origin, renderer_origin, renderer_port) = match (viewer, renderer, port) {
        (None, None, None) => (local_origin.to_owned(), None, 0),
        (Some(viewer), Some(renderer), Some(port)) => {
            let port = port
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| String::from("HTML renderer proxy port must be a non-zero u16."))?;
            (viewer, Some(renderer), port)
        }
        _ => {
            return Err(String::from(
                "HTML proxy configuration requires viewer origin, renderer origin and loopback renderer port together.",
            ));
        }
    };
    let weak = Arc::downgrade(state);
    HtmlAnswerRuntime::new(
        HtmlAnswerRuntimeConfig {
            viewer_origin,
            renderer_origin,
            renderer_port,
            static_root,
        },
        Arc::new(move |source| {
            let weak = weak.clone();
            Box::pin(async move {
                let state = weak.upgrade().ok_or(HtmlAnswerViewError::Closed)?;
                resolve_html_answer_source(state, source).await
            })
        }),
    )
    .map_err(str::to_owned)
}

/// Read one explicit optional environment field, rejecting non-Unicode values rather than using defaults.
/// 读取一个显式可选环境字段；非 Unicode 值被拒绝，不使用默认值掩盖。
fn optional_environment(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} must contain Unicode text.")),
    }
}

/// Issue one link through the actual HTTP bridge after shared-catalog locale validation.
/// 共享目录语言校验后，通过实际 HTTP 桥接签发一个链接。
/// Returns no general Core credential; failures use stable, non-sensitive categories.
/// 不返回通用 Core 凭据；失败使用稳定且不含敏感信息的分类。
pub(crate) async fn service_create_html_answer_view_link(
    state: SharedServiceBridgeState,
    request: HtmlAnswerViewLinkRequest,
) -> Result<HtmlAnswerViewLinkResponse, String> {
    if vulcan_i18n::normalize(&request.locale).is_none() {
        return Err(String::from("invalid_request"));
    }
    viewer_runtime(&state)?
        .issue(request)
        .await
        .map_err(view_error_code)
}

/// Revoke a caller-owned grant without loading or mutating the conversation.
/// 撤销调用方拥有的授权，不加载或修改会话。
pub(crate) fn service_revoke_html_answer_view_link(
    state: SharedServiceBridgeState,
    request: HtmlAnswerRevokeRequest,
) -> Result<(), String> {
    viewer_runtime(&state)?
        .revoke(request.client_id, request.grant_id)
        .map_err(view_error_code)
}

/// Borrow the current bridge's runtime, returning a stable unavailable category before startup.
/// 借用当前桥接运行时；启动前返回稳定的不可用分类。
fn viewer_runtime(state: &SharedServiceBridgeState) -> Result<Arc<HtmlAnswerRuntime>, String> {
    state
        .lock()
        .map_err(|_| String::from("unavailable"))?
        .html_answer_runtime
        .as_ref()
        .and_then(std::sync::Weak::upgrade)
        .ok_or_else(|| String::from("unavailable"))
}

/// Convert the shared wire error enum to its exact machine-readable code without extra diagnostics.
/// 将共享线协议错误枚举转换为精确机器可读码，不附加其他诊断。
fn view_error_code(error: HtmlAnswerViewError) -> String {
    // This fieldless enum serializes only as a string by its declared serde representation.
    // 该无字段枚举按已声明的 serde 表示只序列化为字符串。
    serde_json::to_value(error)
        .expect("fieldless view error serializes")
        .as_str()
        .expect("view error uses snake_case string representation")
        .to_owned()
}

/// Re-read one exact stored event under globally ordered lifecycle guards and reconstruct only its page.
/// 在全局有序生命周期守卫内重新读取一个精确持久事件，并只重建其中一个页面。
/// `source` must match session, optional embedded parent, generation, semantic revision and full hash.
/// `source` 必须匹配会话、可选嵌入父会话、代次、语义修订及完整摘要。
pub(crate) async fn resolve_html_answer_source(
    state: SharedServiceBridgeState,
    source: HtmlAnswerSourceIdentity,
) -> Result<HtmlAnswerContent, HtmlAnswerViewError> {
    if source.parent_session_id == Some(source.session_id)
        || source.event_revision == 0
        || source.window_generation == 0
        || !vulcan_session_core::v1::html_answers::html_answer_identity_is_valid(
            &source.artifact_id,
        )
    {
        return Err(HtmlAnswerViewError::InvalidRequest);
    }
    let seq = Seq::try_new(source.event_seq).map_err(|_| HtmlAnswerViewError::InvalidRequest)?;
    let (controller, registry) = {
        let guard = state.lock().map_err(|_| HtmlAnswerViewError::Unavailable)?;
        (
            guard
                .controller_runtime
                .clone()
                .ok_or(HtmlAnswerViewError::Unavailable)?,
            guard.session_runtime_registry.clone(),
        )
    };
    // Matching deletion/edit lock order prevents either operation from crossing this authorization read.
    // 与删除和编辑保持相同锁顺序，防止这些操作穿过当前授权读取。
    let identities = [Some(source.session_id), source.parent_session_id]
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>();
    let mut lifecycles = Vec::with_capacity(identities.len());
    for id in identities {
        lifecycles.push(
            registry
                .acquire_lifecycle_transition(id)
                .await
                .map_err(|_| HtmlAnswerViewError::Unavailable)?,
        );
    }
    let indexed_owner = source.parent_session_id.unwrap_or(source.session_id);
    let root = locate_session_v1_storage(&controller, &indexed_owner.to_string())
        .await
        .map_err(|_| HtmlAnswerViewError::Unavailable)?;
    // Only the explicitly declared embedded route may select a child; no candidate-path fallback exists.
    // 只有显式声明的嵌入路由可选择子会话；不存在候选路径回退。
    let locator = if source.parent_session_id.is_some() {
        locate_embedded_task_session_v1(&root, &source.session_id.to_string())
            .map_err(|_| HtmlAnswerViewError::Unavailable)?
    } else {
        root
    };
    tokio::task::spawn_blocking(move || {
        let _lifecycles = lifecycles;
        let range = locator.load_event_range(seq, seq).map_err(|_| HtmlAnswerViewError::Unavailable)?;
        if range.state.segment_generation != source.window_generation { return Err(HtmlAnswerViewError::StaleSource); }
        let event = range.events.iter().find(|event| event.seq == seq).ok_or(HtmlAnswerViewError::StaleSource)?;
        if event.event_revision != source.event_revision { return Err(HtmlAnswerViewError::StaleSource); }
        match registry.get(source.session_id) {
            Ok(handle) => {
                let matches = handle.read(|document| {
                    document.segment_generation() == source.window_generation
                        && document.events().iter().find(|candidate| candidate.seq == seq)
                            .is_none_or(|candidate| candidate.event_revision == source.event_revision)
                }).map_err(|_| HtmlAnswerViewError::Unavailable)?;
                if !matches { return Err(HtmlAnswerViewError::StaleSource); }
            }
            Err(SessionRuntimeError::SessionNotRegistered(_)) => {}
            Err(_) => return Err(HtmlAnswerViewError::Unavailable),
        }
        let descriptor = project_html_answers(&source.session_id.to_string(), event, true).into_iter()
            .find(|descriptor| matches!(&descriptor.availability, HtmlAnswerAvailability::Ready { artifact_id, .. } if artifact_id == &source.artifact_id))
            .ok_or(HtmlAnswerViewError::StaleSource)?;
        let HtmlAnswerAvailability::Ready { source: offsets, .. } = descriptor.availability else { return Err(HtmlAnswerViewError::StaleSource); };
        let EventBody::AssistantText(assistant) = &event.event else { return Err(HtmlAnswerViewError::StaleSource); };
        let text = assistant.text.get(offsets.byte_start..offsets.byte_end).ok_or(HtmlAnswerViewError::StaleSource)?;
        Ok(HtmlAnswerContent { title: descriptor.title.ok_or(HtmlAnswerViewError::StaleSource)?, source: text.to_owned() })
    }).await.map_err(|_| HtmlAnswerViewError::Unavailable)?
}
