//! Durable V1 request-configuration mutation helpers.
//! 可靠 V1 请求配置修改辅助逻辑。

use vulcan_provider_config::config::ReasoningEffortLevel;
use vulcan_session_core::v1::config::{
    ActiveRequestConfig, KnowledgeLibrarySelectionConfig, PendingRequestConfig,
    ProviderToolSelectionConfig, VmmSessionPolicyConfig,
};
use vulcan_session_core::v1::document::SessionSelectionBundleInput;
use vulcan_workbench_model::SessionRuntimeStateEntry;

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
};

/// Immutable fallback facts captured from the currently admitted legacy execution projection.
/// 从当前已准入旧执行投影捕获的不可变回退事实。
#[derive(Debug, Clone, Default)]
pub(crate) struct SessionRequestFallback {
    /// Exact Provider id used when V1 has not yet initialized its active request.
    /// V1 尚未初始化活动请求时使用的精确 Provider 标识。
    pub(crate) provider: Option<String>,
    /// Exact non-secret connection id used before the first V1 request boundary.
    /// 首个 V1 请求边界前使用的精确非秘密连接标识。
    pub(crate) connection_id: Option<String>,
    /// Exact model id used when the V1 root is still fresh.
    /// V1 根仍为 fresh 时使用的精确模型标识。
    pub(crate) model: Option<String>,
    /// Exact reasoning preference selected for the session.
    /// 当前会话选择的精确思考偏好。
    pub(crate) thinking_level: Option<ReasoningEffortLevel>,
    /// Exact optional temperature selected for the session.
    /// 当前会话选择的精确可选温度。
    pub(crate) temperature: Option<f32>,
}

impl SessionRequestFallback {
    /// Captures the exact next-request route and controls from one admitted runtime projection.
    /// 从一份已准入运行时投影捕获精确的下一请求路由与控制项。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the exact session-owned process projection already selected by the caller.
    /// `runtime_state` 是调用方已经选定、由精确会话持有的进程内投影。
    ///
    /// # Returns
    /// 返回值
    /// A complete fallback that prefers the pending route over the active route without inventing values.
    /// 返回一份完整回退事实；其严格优先使用待生效路由，其次使用活动路由，不会发明任何值。
    pub(crate) fn from_runtime_state(runtime_state: &SessionRuntimeStateEntry) -> Self {
        let pending_route = runtime_state.pending_model_selection.as_ref();
        Self {
            provider: pending_route
                .map(|selection| selection.provider_id.clone())
                .or_else(|| runtime_state.active_provider_id.clone()),
            connection_id: pending_route
                .map(|selection| selection.connection_id.clone())
                .or_else(|| runtime_state.active_connection_id.clone()),
            model: pending_route
                .map(|selection| selection.model_id.clone())
                .or_else(|| runtime_state.active_model_id.clone()),
            thinking_level: runtime_state.active_reasoning_effort,
            temperature: runtime_state.active_temperature,
        }
    }
}

/// Durably records a complete next-request model route while preserving reasoning controls.
/// 可靠记录完整的下一请求模型路由，同时保留思考控制项。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document; route ids are the validated target; `fallback` is used only before V1 initialization.
/// `handle` 持有 V1 文档；路由标识是已校验目标；`fallback` 仅在 V1 初始化前使用。
///
/// # Returns
/// 返回值
/// `true` only when metadata was durably changed.
/// 仅当 metadata 已可靠修改时返回 `true`。
pub(crate) fn set_pending_model_request_v1(
    handle: &SessionHandle,
    provider: String,
    connection_id: String,
    model: String,
    fallback: SessionRequestFallback,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let active = &document.metadata.config.active_request;
                let pending = document.metadata.config.pending_request.as_ref();
                let request = PendingRequestConfig {
                    provider: Some(provider.clone()),
                    connection_id: Some(connection_id.clone()),
                    model: Some(model.clone()),
                    thinking_level: pending
                        .and_then(|value| value.thinking_level.clone())
                        .or_else(|| active.thinking_level.clone())
                        .or_else(|| fallback.thinking_level.map(reasoning_effort_level_key)),
                    temperature: pending
                        .and_then(|value| value.temperature)
                        .or(active.temperature)
                        .or_else(|| fallback.temperature.map(f64::from)),
                    set_at_ms: now_ms,
                };
                set_pending_request_if_effectively_changed(document, request, now_ms)
            },
        )
        .map_err(|error| format!("保存 V1 下一请求模型配置失败：{error}"))?;
    Ok(receipt.changed)
}

/// Durably records a complete next-request reasoning selection while preserving its route.
/// 可靠记录完整的下一请求思考选择，同时保留其路由。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document; `thinking_level` is the selected stable key; `fallback` is used only for a fresh root.
/// `handle` 持有 V1 文档；`thinking_level` 是已选择的稳定键；`fallback` 仅用于 fresh 根。
///
/// # Returns
/// 返回值
/// `true` only when metadata was durably changed.
/// 仅当 metadata 已可靠修改时返回 `true`。
pub(crate) fn set_pending_reasoning_request_v1(
    handle: &SessionHandle,
    thinking_level: ReasoningEffortLevel,
    fallback: SessionRequestFallback,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let active = &document.metadata.config.active_request;
                let pending = document.metadata.config.pending_request.as_ref();
                let request = PendingRequestConfig {
                    provider: pending
                        .and_then(|value| value.provider.clone())
                        .or_else(|| active.provider.clone())
                        .or_else(|| fallback.provider.clone()),
                    connection_id: pending
                        .and_then(|value| value.connection_id.clone())
                        .or_else(|| active.connection_id.clone())
                        .or_else(|| fallback.connection_id.clone()),
                    model: pending
                        .and_then(|value| value.model.clone())
                        .or_else(|| active.model.clone())
                        .or_else(|| fallback.model.clone()),
                    thinking_level: Some(reasoning_effort_level_key(thinking_level)),
                    temperature: pending
                        .and_then(|value| value.temperature)
                        .or(active.temperature)
                        .or_else(|| fallback.temperature.map(f64::from)),
                    set_at_ms: now_ms,
                };
                set_pending_request_if_effectively_changed(document, request, now_ms)
            },
        )
        .map_err(|error| format!("保存 V1 下一请求思考配置失败：{error}"))?;
    Ok(receipt.changed)
}

/// Durably records one complete session-owned Provider tool selection.
/// 可靠记录一份由当前会话持有的完整 Provider 工具选择。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document, `selection` is already canonical and complete, and `now_ms` is
/// the semantic mutation time.
/// `handle` 持有 V1 文档，`selection` 已经规范且完整，`now_ms` 是语义修改时间。
///
/// # Returns
/// 返回值
/// `true` only when the exact durable selection changed.
/// 仅当精确持久化选择真实变化时返回 `true`。
pub(crate) fn set_provider_tool_selection_v1(
    handle: &SessionHandle,
    selection: ProviderToolSelectionConfig,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                if document.set_provider_tool_selection(selection, now_ms)? {
                    Ok(SessionMutationChange::Changed(()))
                } else {
                    Ok(SessionMutationChange::Unchanged(()))
                }
            },
        )
        .map_err(|error| format!("保存 V1 Provider 工具选择失败：{error}"))?;
    Ok(receipt.changed)
}

/// Durably commits one complete session-selection bundle as a single metadata revision.
/// 把一份完整会话选择配置作为单个 metadata 修订可靠提交。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document, `selection` contains all first-turn selections, and `now_ms`
/// is the semantic mutation time.
/// `handle` 持有 V1 文档，`selection` 包含全部首轮选择，`now_ms` 是语义修改时间。
///
/// # Returns
/// 返回值
/// `true` only when at least one durable selection changed.
/// 仅当至少一项持久化选择真实变化时返回 `true`。
pub(crate) fn set_session_selection_bundle_v1(
    handle: &SessionHandle,
    selection: SessionSelectionBundleInput,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                if document.set_session_selection_bundle(selection, now_ms)? {
                    Ok(SessionMutationChange::Changed(()))
                } else {
                    Ok(SessionMutationChange::Unchanged(()))
                }
            },
        )
        .map_err(|error| format!("保存 V1 会话选择配置失败：{error}"))?;
    Ok(receipt.changed)
}

/// Durably records one complete session-owned knowledge-library selection.
/// 可靠记录一份由当前会话持有的完整知识库选择。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document, `selection` is authorized and canonical, and `now_ms` is the
/// semantic mutation time.
/// `handle` 持有 V1 文档，`selection` 已授权且规范，`now_ms` 是语义修改时间。
///
/// # Returns
/// 返回值
/// `true` only when the exact durable selection changed.
/// 仅在精确持久化选择真实变化时返回 `true`。
pub(crate) fn set_knowledge_library_selection_v1(
    handle: &SessionHandle,
    selection: KnowledgeLibrarySelectionConfig,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                if document.set_knowledge_library_selection(selection, now_ms)? {
                    Ok(SessionMutationChange::Changed(()))
                } else {
                    Ok(SessionMutationChange::Unchanged(()))
                }
            },
        )
        .map_err(|error| format!("保存 V1 知识库选择失败：{error}"))?;
    Ok(receipt.changed)
}

/// Identifies one independently mutable session VMM policy field.
/// 标识一项可独立修改的会话 VMM 策略字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmmSessionPolicyField {
    /// Precheck-memory injection override.
    /// 预检记忆注入覆盖。
    Precheck,
    /// Completed-turn memory writeback override.
    /// 已完成回合记忆回写覆盖。
    MemoryWriteback,
}

/// Durably updates one VMM tri-state value while preserving its sibling in the same owner lock.
/// 在同一 owner 锁内可靠更新一项 VMM 三态值并保留另一项。
///
/// # Parameters
/// 参数
/// `handle` owns the V1 document, `field` selects the exact policy, `value` is inherit/enabled/
/// disabled, and `now_ms` is the semantic mutation time.
/// `handle` 持有 V1 文档，`field` 选择精确策略，`value` 表示继承/开启/关闭，`now_ms` 是语义修改时间。
///
/// # Returns
/// 返回值
/// `true` only when the exact durable policy pair changed.
/// 仅在精确持久化策略对真实变化时返回 `true`。
pub(crate) fn set_vmm_session_policy_value_v1(
    handle: &SessionHandle,
    field: VmmSessionPolicyField,
    value: Option<bool>,
    now_ms: i64,
) -> Result<bool, String> {
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let mut config: VmmSessionPolicyConfig =
                    document.metadata.config.selection.vmm.clone();
                match field {
                    VmmSessionPolicyField::Precheck => config.precheck_enabled = value,
                    VmmSessionPolicyField::MemoryWriteback => {
                        config.memory_writeback_enabled = value;
                    }
                }
                if document.set_vmm_session_policy(config, now_ms)? {
                    Ok(SessionMutationChange::Changed(()))
                } else {
                    Ok(SessionMutationChange::Unchanged(()))
                }
            },
        )
        .map_err(|error| format!("保存 V1 VMM 会话策略失败：{error}"))?;
    Ok(receipt.changed)
}

/// Applies one complete pending request only when it differs from both pending and active facts.
/// 仅当完整待生效请求不同于现有待生效及活动事实时应用修改。
fn set_pending_request_if_effectively_changed(
    document: &mut vulcan_session_core::v1::document::SessionDocument,
    request: PendingRequestConfig,
    now_ms: i64,
) -> Result<SessionMutationChange<()>, vulcan_session_core::v1::errors::DomainError> {
    if document.metadata.config.pending_request.is_none()
        && active_request_matches_pending(&document.metadata.config.active_request, &request)
    {
        return Ok(SessionMutationChange::Unchanged(()));
    }
    if document.set_pending_request(request, now_ms)? {
        Ok(SessionMutationChange::Changed(()))
    } else {
        Ok(SessionMutationChange::Unchanged(()))
    }
}

/// Compares the stable effective fields while deliberately ignoring the pending timestamp.
/// 比较稳定生效字段，并有意忽略待生效时间戳。
fn active_request_matches_pending(
    active: &ActiveRequestConfig,
    pending: &PendingRequestConfig,
) -> bool {
    active.provider == pending.provider
        && active.connection_id == pending.connection_id
        && active.model == pending.model
        && active.thinking_level == pending.thinking_level
        && active.temperature == pending.temperature
}

/// Converts one provider-config reasoning enum into its stable kebab-case protocol key.
/// 将 Provider 配置思考枚举转换为稳定的 kebab-case 协议键。
pub(crate) fn reasoning_effort_level_key(level: ReasoningEffortLevel) -> String {
    String::from(match level {
        ReasoningEffortLevel::Off => "off",
        ReasoningEffortLevel::None => "none",
        ReasoningEffortLevel::Adaptive => "adaptive",
        ReasoningEffortLevel::Minimal => "minimal",
        ReasoningEffortLevel::Low => "low",
        ReasoningEffortLevel::Medium => "medium",
        ReasoningEffortLevel::High => "high",
        ReasoningEffortLevel::XHigh => "x-high",
        ReasoningEffortLevel::Max => "max",
        ReasoningEffortLevel::Ultra => "ultra",
    })
}

#[cfg(test)]
mod tests {
    //! Durable V1 pending-request service tests.
    //! 可靠 V1 待生效请求服务测试。

    use super::*;
    use crate::session_runtime_registry_v1::SessionPersistenceBackend;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use vulcan_session_core::v1::document::{FlushCandidate, SessionDocument};
    use vulcan_workbench_model::SessionId;
    use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

    /// Counts accepted metadata flushes for deterministic idempotency assertions.
    /// 统计已接纳 metadata flush，以进行确定性幂等断言。
    struct RequestConfigTestBackend {
        /// Number of durable candidates accepted by this backend.
        /// 当前后端接纳的可靠候选数量。
        flush_count: AtomicU64,
    }

    impl RequestConfigTestBackend {
        /// Creates one empty request-configuration persistence recorder.
        /// 创建一个空的请求配置持久化记录器。
        fn new() -> Self {
            Self {
                flush_count: AtomicU64::new(0),
            }
        }
    }

    impl SessionPersistenceBackend for RequestConfigTestBackend {
        /// Accepts one full V1 candidate and publishes its exact metadata.
        /// 接纳一份完整 V1 候选并发布其精确 metadata。
        fn flush(
            &self,
            candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            self.flush_count.fetch_add(1, Ordering::SeqCst);
            Ok(SessionV1FlushResult {
                written_file_count: candidate.dirty.len(),
                published_metadata: Some(candidate.metadata.clone()),
            })
        }
    }

    /// Builds one handle with optional initialized active request facts.
    /// 构建一份带可选已初始化活动请求事实的句柄。
    fn request_config_handle(
        session_id: SessionId,
        active_request: ActiveRequestConfig,
    ) -> (Arc<SessionHandle>, Arc<RequestConfigTestBackend>) {
        let mut document =
            SessionDocument::new(session_id.to_string(), "project-a", "Request config", 1);
        document.metadata.config.active_request = active_request;
        let backend = Arc::new(RequestConfigTestBackend::new());
        (
            SessionHandle::with_backend(session_id, document, backend.clone()),
            backend,
        )
    }

    /// Verifies model selection preserves reasoning controls and repeated selection is a true no-op.
    /// 验证模型选择保留思考控制，并且重复选择是真正的无操作。
    #[test]
    fn pending_model_request_preserves_reasoning_and_is_idempotent() {
        let session_id = SessionId::from_u128(0x6201);
        let (handle, backend) = request_config_handle(
            session_id,
            ActiveRequestConfig {
                provider: Some(String::from("provider-a")),
                connection_id: Some(String::from("connection-a")),
                model: Some(String::from("model-a")),
                thinking_level: Some(String::from("high")),
                temperature: Some(0.25),
            },
        );
        let fallback = SessionRequestFallback::default();

        assert!(
            set_pending_model_request_v1(
                handle.as_ref(),
                String::from("provider-b"),
                String::from("connection-b"),
                String::from("model-b"),
                fallback.clone(),
                2,
            )
            .expect("new pending model route should persist")
        );
        assert!(
            !set_pending_model_request_v1(
                handle.as_ref(),
                String::from("provider-b"),
                String::from("connection-b"),
                String::from("model-b"),
                fallback,
                3,
            )
            .expect("same pending model route should succeed idempotently")
        );
        let (active, pending) = handle
            .read(|document| {
                (
                    document.metadata.config.active_request.clone(),
                    document.metadata.config.pending_request.clone(),
                )
            })
            .expect("request metadata should read");
        assert_eq!(active.model.as_deref(), Some("model-a"));
        let pending = pending.expect("pending route should exist");
        assert_eq!(pending.model.as_deref(), Some("model-b"));
        assert_eq!(pending.thinking_level.as_deref(), Some("high"));
        assert_eq!(pending.temperature, Some(0.25));
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    }

    /// Verifies reasoning selection preserves the pending route and does not mutate the active request.
    /// 验证思考选择保留待生效路由，并且不修改活动请求。
    #[test]
    fn pending_reasoning_request_preserves_pending_route_until_next_boundary() {
        let session_id = SessionId::from_u128(0x6202);
        let (handle, backend) = request_config_handle(
            session_id,
            ActiveRequestConfig {
                provider: Some(String::from("provider-a")),
                connection_id: Some(String::from("connection-a")),
                model: Some(String::from("model-a")),
                thinking_level: Some(String::from("medium")),
                temperature: None,
            },
        );
        set_pending_model_request_v1(
            handle.as_ref(),
            String::from("provider-b"),
            String::from("connection-b"),
            String::from("model-b"),
            SessionRequestFallback::default(),
            2,
        )
        .expect("pending route should persist");

        assert!(
            set_pending_reasoning_request_v1(
                handle.as_ref(),
                ReasoningEffortLevel::XHigh,
                SessionRequestFallback::default(),
                3,
            )
            .expect("pending reasoning should persist")
        );
        let (active, pending) = handle
            .read(|document| {
                (
                    document.metadata.config.active_request.clone(),
                    document.metadata.config.pending_request.clone(),
                )
            })
            .expect("request metadata should read");
        assert_eq!(active.model.as_deref(), Some("model-a"));
        assert_eq!(active.thinking_level.as_deref(), Some("medium"));
        let pending = pending.expect("combined pending request should exist");
        assert_eq!(pending.provider.as_deref(), Some("provider-b"));
        assert_eq!(pending.connection_id.as_deref(), Some("connection-b"));
        assert_eq!(pending.model.as_deref(), Some("model-b"));
        assert_eq!(pending.thinking_level.as_deref(), Some("x-high"));
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 2);
    }

    /// Verifies selecting the already-active complete request does not create pending metadata.
    /// 验证选择已经生效的完整请求不会创建待生效 metadata。
    #[test]
    fn active_request_selection_is_a_durable_noop() {
        let session_id = SessionId::from_u128(0x6203);
        let (handle, backend) = request_config_handle(
            session_id,
            ActiveRequestConfig {
                provider: Some(String::from("provider-a")),
                connection_id: Some(String::from("connection-a")),
                model: Some(String::from("model-a")),
                thinking_level: Some(String::from("high")),
                temperature: Some(0.5),
            },
        );

        assert!(
            !set_pending_model_request_v1(
                handle.as_ref(),
                String::from("provider-a"),
                String::from("connection-a"),
                String::from("model-a"),
                SessionRequestFallback {
                    thinking_level: Some(ReasoningEffortLevel::High),
                    temperature: Some(0.5),
                    ..SessionRequestFallback::default()
                },
                2,
            )
            .expect("active route selection should succeed idempotently")
        );
        assert!(
            handle
                .read(|document| document.metadata.config.pending_request.is_none())
                .expect("pending request state should read")
        );
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 0);
    }

    /// Verifies Provider-tool selection is durably stored once and exact replay does not flush.
    /// 验证 Provider 工具选择只可靠写入一次，精确重放不会再次 flush。
    #[test]
    fn provider_tool_selection_persists_once_and_replays_idempotently() {
        let session_id = SessionId::from_u128(0x6204);
        let (handle, backend) = request_config_handle(session_id, ActiveRequestConfig::default());
        let selection = ProviderToolSelectionConfig {
            initialized: true,
            overridden: true,
            enabled_capabilities: vec![String::from("web_search")],
            selections: vec![
                vulcan_session_core::v1::config::ProviderToolCapabilitySelection {
                    capability: String::from("web_search"),
                    target: vulcan_session_core::v1::config::ProviderToolTargetConfig {
                        connection_id: String::from("connection-a"),
                        target_id: String::from("target-a"),
                    },
                },
            ],
            media_understanding_modes: Vec::new(),
            extension_target_sets: Vec::new(),
        };

        assert!(
            set_provider_tool_selection_v1(handle.as_ref(), selection.clone(), 2)
                .expect("new Provider-tool selection should persist")
        );
        assert!(
            !set_provider_tool_selection_v1(handle.as_ref(), selection.clone(), 3)
                .expect("same Provider-tool selection should replay idempotently")
        );
        assert_eq!(
            handle
                .read(|document| document.metadata.config.selection.provider_tools.clone())
                .expect("Provider-tool metadata should read"),
            selection
        );
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    }

    /// The complete first-turn selection bundle is one durable flush and exact replay is a no-op.
    /// 完整首轮选择配置只产生一次可靠 flush，精确重放保持无操作。
    #[test]
    fn session_selection_bundle_persists_as_one_revision_and_one_flush() {
        let session_id = SessionId::from_u128(0x6206);
        let (handle, backend) = request_config_handle(session_id, ActiveRequestConfig::default());
        let input = SessionSelectionBundleInput {
            output_style: Some(String::from("concise")),
            provider_tools: ProviderToolSelectionConfig {
                initialized: true,
                overridden: false,
                enabled_capabilities: vec![String::from("web_search")],
                selections: Vec::new(),
                media_understanding_modes: Vec::new(),
                extension_target_sets: Vec::new(),
            },
            knowledge_libraries: KnowledgeLibrarySelectionConfig {
                mode: vulcan_session_core::v1::config::KnowledgeLibrarySelectionMode::Explicit,
                library_ids: vec![String::from("library-a")],
                retrieval_mode: Some(
                    vulcan_session_core::v1::config::KnowledgeRetrievalMode::DisplayBm25,
                ),
            },
            vmm: VmmSessionPolicyConfig {
                precheck_enabled: Some(true),
                memory_writeback_enabled: Some(false),
            },
        };

        assert!(
            set_session_selection_bundle_v1(handle.as_ref(), input.clone(), 2)
                .expect("new selection bundle should persist")
        );
        assert!(
            !set_session_selection_bundle_v1(handle.as_ref(), input.clone(), 3)
                .expect("same selection bundle should replay idempotently")
        );
        assert_eq!(
            handle
                .read(|document| document.metadata.config.selection.clone())
                .expect("selection metadata should read")
                .knowledge_libraries,
            input.knowledge_libraries
        );
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 1);
    }

    /// Knowledge and VMM selections persist exactly once and preserve independent VMM fields.
    /// 知识库与 VMM 选择只持久化一次，并保留相互独立的 VMM 字段。
    #[test]
    fn knowledge_and_vmm_selections_persist_once_and_replay_idempotently() {
        let session_id = SessionId::from_u128(0x6205);
        let (handle, backend) = request_config_handle(session_id, ActiveRequestConfig::default());
        let knowledge = KnowledgeLibrarySelectionConfig {
            mode: vulcan_session_core::v1::config::KnowledgeLibrarySelectionMode::Explicit,
            library_ids: vec![String::from("library-a"), String::from("library-b")],
            retrieval_mode: Some(
                vulcan_session_core::v1::config::KnowledgeRetrievalMode::DisplayHybrid,
            ),
        };

        assert!(
            set_knowledge_library_selection_v1(handle.as_ref(), knowledge.clone(), 2)
                .expect("new knowledge selection should persist")
        );
        assert!(
            !set_knowledge_library_selection_v1(handle.as_ref(), knowledge.clone(), 3)
                .expect("same knowledge selection should replay idempotently")
        );
        assert!(
            set_vmm_session_policy_value_v1(
                handle.as_ref(),
                VmmSessionPolicyField::Precheck,
                Some(false),
                4,
            )
            .expect("precheck override should persist")
        );
        assert!(
            set_vmm_session_policy_value_v1(
                handle.as_ref(),
                VmmSessionPolicyField::MemoryWriteback,
                Some(true),
                5,
            )
            .expect("writeback override should persist")
        );
        assert!(
            !set_vmm_session_policy_value_v1(
                handle.as_ref(),
                VmmSessionPolicyField::Precheck,
                Some(false),
                6,
            )
            .expect("same precheck override should replay idempotently")
        );
        let (persisted_knowledge, persisted_vmm) = handle
            .read(|document| {
                (
                    document
                        .metadata
                        .config
                        .selection
                        .knowledge_libraries
                        .clone(),
                    document.metadata.config.selection.vmm.clone(),
                )
            })
            .expect("selection metadata should read");
        assert_eq!(persisted_knowledge, knowledge);
        assert_eq!(persisted_vmm.precheck_enabled, Some(false));
        assert_eq!(persisted_vmm.memory_writeback_enabled, Some(true));
        assert_eq!(backend.flush_count.load(Ordering::SeqCst), 3);
    }
}
